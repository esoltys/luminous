//! Playlist files in library folders, kept in two-way sync with a playlist
//! (#2280). A third `impl PlaylistManager` block beside `playlist.rs` and
//! `import_export.rs`.
//!
//! A playlist file (`.m3u`, `.m3u8`, `.pls`, `.xspf`) found in a library
//! folder becomes a Custom playlist linked to it by a `playlist_files` row.
//! The row records, from the last time the two agreed, a hash of the file's
//! bytes and a hash of what Luminous would write for the playlist. Each side
//! has changed exactly when its hash no longer matches:
//!
//! - **File → playlist.** [`PlaylistManager::sync_playlist_files`] re-reads a
//!   file whose bytes changed and replaces its playlist's items in place, so
//!   the playlist keeps its id, pins, sort and play position.
//! - **Playlist → file.** [`PlaylistFileWriter`] renders each file-backed
//!   playlist again and rewrites the file when the rendering changed, in the
//!   style it was read in. Nothing counts edits: whatever changed the
//!   playlist (an edit, undo, a sort, a duplicate merge, the organizer
//!   moving a song, a tag edit) shows in the rendering. Edits only nudge
//!   the writer to look soon; a missed nudge delays a write, never loses it.
//! - **Both.** A file that changed while its playlist's rendering changed
//!   too is a conflict. Neither side is written until the user picks one
//!   with [`PlaylistManager::resolve_file_conflict`].
//!
//! Renaming a playlist renames its file; deleting it moves the file to the
//! trash; a file that disappears leaves its playlist as a plain Custom one.

use super::import_export::{
    entry_key, export_tracks, playlist_name_for, replace_items, resolve_entries, ResolvedEntry,
};
use super::PlaylistManager;
use crate::library_paths;
use anyhow::{anyhow, Result};
use luminous_metadata::playlist::{self, FileStyle, ParsedTrack, PlaylistFormat};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::Duration;

/// How long the writer waits after the last edit before writing files, so a
/// burst of edits (a drag-reorder, a multi-select remove) is one write.
const WRITE_DEBOUNCE: Duration = Duration::from_millis(750);

/// The playlist file extensions a scan or the watcher treats as playlists.
pub(crate) fn is_playlist_file(path: &Path) -> bool {
    PlaylistFormat::from_path(path).is_some()
}

/// What a sync pass changed, for the caller to announce to the UI.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FileSyncReport {
    /// Playlists created, re-read, unlinked, or put in conflict.
    pub changed: Vec<i64>,
    /// Playlists that entered a conflict in this pass.
    pub new_conflicts: Vec<FileConflict>,
}

impl FileSyncReport {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.new_conflicts.is_empty()
    }

    /// Folds `other` into this report.
    pub fn merge(&mut self, other: FileSyncReport) {
        for id in other.changed {
            self.add_changed(id);
        }
        self.new_conflicts.extend(other.new_conflicts);
    }

    /// Notes what syncing one file did to its playlist.
    fn record(&mut self, inbound: Inbound) {
        match inbound {
            Inbound::Synced {
                playlist_id,
                changed: true,
            }
            | Inbound::Detached(playlist_id) => self.add_changed(playlist_id),
            Inbound::Conflict(conflict) => {
                self.add_changed(conflict.playlist_id);
                self.new_conflicts.push(conflict);
            }
            Inbound::Synced { changed: false, .. } | Inbound::Skipped => {}
        }
    }

    fn add_changed(&mut self, playlist_id: i64) {
        if !self.changed.contains(&playlist_id) {
            self.changed.push(playlist_id);
        }
    }
}

/// A playlist whose file changed while it had edits not yet written.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileConflict {
    pub playlist_id: i64,
    pub name: String,
}

/// Which side of a conflict the user keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeepSide {
    /// The playlist as it is in Luminous; it overwrites the file.
    Luminous,
    /// The file as it is on disk; it replaces the playlist's tracks.
    File,
}

/// Announces a sync pass: `playlists-changed` for the playlists it touched,
/// and `playlist-file-conflict` for each new conflict (the UI asks the user
/// which side to keep).
pub fn announce(app: &tauri::AppHandle, report: &FileSyncReport) {
    use tauri::Emitter;
    if !report.changed.is_empty() {
        let _ = app.emit("playlists-changed", report.changed.clone());
    }
    for conflict in &report.new_conflicts {
        let _ = app.emit("playlist-file-conflict", conflict);
    }
}

/// One `playlist_files` row.
struct LinkedFile {
    playlist_id: i64,
    path: PathBuf,
    /// The file's bytes when the two last agreed.
    file_hash: String,
    /// What Luminous would have written for the playlist then.
    render_hash: String,
    style: FileStyle,
    conflict: bool,
}

const LINKED_COLS: &str =
    "playlist_id, path, file_hash, render_hash, style, conflict FROM playlist_files";

impl LinkedFile {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let style: String = row.get(4)?;
        Ok(Self {
            playlist_id: row.get(0)?,
            path: PathBuf::from(row.get::<_, String>(1)?),
            file_hash: row.get(2)?,
            render_hash: row.get(3)?,
            style: serde_json::from_str(&style).unwrap_or_default(),
            conflict: row.get(5)?,
        })
    }

    fn by_path(conn: &Connection, path: &Path) -> Result<Option<Self>> {
        Ok(conn
            .query_row(
                &format!("SELECT {LINKED_COLS} WHERE path_key = ?1"),
                params![path_key(path)],
                Self::from_row,
            )
            .optional()?)
    }

    fn by_playlist(conn: &Connection, playlist_id: i64) -> Result<Option<Self>> {
        Ok(conn
            .query_row(
                &format!("SELECT {LINKED_COLS} WHERE playlist_id = ?1"),
                params![playlist_id],
                Self::from_row,
            )
            .optional()?)
    }

    /// The hash of what Luminous would write for this playlist now.
    fn render_hash_now(&self, conn: &Connection) -> Result<String> {
        Ok(content_hash(&render(
            conn,
            self.playlist_id,
            &self.path,
            &self.style,
        )?))
    }
}

/// What Luminous writes to `path` for `playlist_id`: its name and tracks in
/// the file's format and `style`.
fn render(conn: &Connection, playlist_id: i64, path: &Path, style: &FileStyle) -> Result<Vec<u8>> {
    let format = PlaylistFormat::from_path(path)
        .ok_or_else(|| anyhow!("Not a playlist file: {}", path.display()))?;
    let name: String = conn.query_row(
        "SELECT name FROM playlists WHERE id = ?1",
        params![playlist_id],
        |r| r.get(0),
    )?;
    let items = PlaylistManager::get_playlist_tracks_from_conn(conn, playlist_id)?;
    Ok(playlist::playlist_file_bytes(
        &name,
        &export_tracks(&items),
        format,
        path,
        style,
    ))
}

fn path_key(path: &Path) -> String {
    library_paths::key(&path.to_string_lossy())
}

fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Records that `playlist_id` and the file at `path` agree, the file holding
/// `bytes` in `style`: the link is created or repointed, the playlist's
/// rendering as it is now becomes the agreed one, and any conflict is over.
fn record_agreement(
    conn: &Connection,
    playlist_id: i64,
    path: &Path,
    bytes: &[u8],
    style: &FileStyle,
) -> Result<()> {
    let rendered = render(conn, playlist_id, path, style)?;
    conn.execute(
        "INSERT INTO playlist_files (playlist_id, path, path_key, file_hash, render_hash, style)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(playlist_id) DO UPDATE SET
             path = excluded.path, path_key = excluded.path_key,
             file_hash = excluded.file_hash, render_hash = excluded.render_hash,
             style = excluded.style, conflict = 0",
        params![
            playlist_id,
            path.to_string_lossy(),
            path_key(path),
            content_hash(bytes),
            content_hash(&rendered),
            serde_json::to_string(style)?,
        ],
    )?;
    Ok(())
}

fn unlink(conn: &Connection, playlist_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM playlist_files WHERE playlist_id = ?1",
        params![playlist_id],
    )?;
    Ok(())
}

fn set_conflict(conn: &Connection, playlist_id: i64) -> Result<FileConflict> {
    conn.execute(
        "UPDATE playlist_files SET conflict = 1 WHERE playlist_id = ?1",
        params![playlist_id],
    )?;
    let name = conn.query_row(
        "SELECT name FROM playlists WHERE id = ?1",
        params![playlist_id],
        |r| r.get(0),
    )?;
    Ok(FileConflict { playlist_id, name })
}

/// Whether `path` is inside one of the library's folders.
fn in_library_folder(conn: &Connection, path: &Path) -> Result<bool> {
    let path = path.to_string_lossy();
    let mut stmt = conn.prepare("SELECT path FROM directories")?;
    let folders = stmt.query_map([], |r| r.get::<_, String>(0))?;
    for folder in folders {
        if library_paths::is_under(&path, &folder?) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A playlist that only lists files from its own folder: the per-album
/// `.m3u` a CD ripper writes next to the tracks. Importing every one of
/// those would bury the user's own playlists, so a scan skips them. Decided
/// from the entries as written, without touching the disk, since a large
/// library has one in every album folder.
fn lists_only_its_own_folder(path: &Path, tracks: &[ParsedTrack]) -> bool {
    let Some(dir) = path.parent() else {
        return false;
    };
    !tracks.is_empty()
        && tracks.iter().all(|t| {
            let location = t
                .path_or_url
                .trim_start_matches("./")
                .trim_start_matches(".\\");
            if playlist::is_http_url(location) {
                return false;
            }
            if !location.contains(['/', '\\']) {
                return true;
            }
            Path::new(location)
                .parent()
                .is_some_and(|parent| library_paths::same(parent, dir))
        })
}

/// A file name for a playlist called `name`: characters no file system
/// accepts become `_`, and trailing dots and spaces (which Windows drops)
/// are trimmed.
fn file_stem_for(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "Playlist".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The playlist files a scan of the library folders `roots` should sync:
/// those it `found`, then every linked file under those roots it didn't
/// find, so playlists whose files were deleted while Luminous was closed
/// are unlinked (after the found ones, so a moved file keeps its playlist).
/// Pass only roots the scan actually walked: an unplugged drive's files
/// aren't gone. Reads only the database, so the caller can take the
/// playlists lock per file rather than for the whole list.
pub fn scanned_playlist_files(
    conn: &Connection,
    found: &[PathBuf],
    roots: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let found_keys: HashSet<String> = found.iter().map(|p| path_key(p)).collect();
    let root_keys: Vec<String> = roots.iter().map(|r| path_key(r)).collect();
    let mut paths = found.to_vec();
    let mut stmt = conn.prepare("SELECT path, path_key FROM playlist_files")?;
    let linked = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in linked {
        let (path, key) = row?;
        let under_root = root_keys
            .iter()
            .any(|root| library_paths::is_under_key(&key, root));
        if under_root && !found_keys.contains(&key) {
            paths.push(PathBuf::from(path));
        }
    }
    Ok(paths)
}

/// What reading one playlist file did.
enum Inbound {
    /// The playlist now matches the file (it may already have).
    Synced {
        playlist_id: i64,
        changed: bool,
    },
    Conflict(FileConflict),
    /// The file is gone; its playlist stays as a plain one.
    Detached(i64),
    /// Not a playlist Luminous keeps (unknown format, an album-folder
    /// playlist, or a missing file nothing was linked to).
    Skipped,
}

impl PlaylistManager {
    /// Reads the playlist files at `paths` into their playlists: a new file
    /// becomes a file-backed Custom playlist, a changed one updates its
    /// playlist in place, and a path that no longer exists unlinks its
    /// playlist (which stays, as a plain Custom playlist). Files still there
    /// are read before missing ones are unlinked, so a file moved within
    /// the batch keeps its playlist. A file that fails to read is logged and
    /// skipped; the rest of the batch still syncs.
    pub fn sync_playlist_files(&mut self, paths: &[PathBuf]) -> Result<FileSyncReport> {
        let (present, missing): (Vec<&PathBuf>, Vec<&PathBuf>) =
            paths.iter().partition(|p| p.is_file());
        let mut report = FileSyncReport::default();
        for path in present.into_iter().chain(missing) {
            match self.sync_file(path, false) {
                Ok(inbound) => report.record(inbound),
                Err(e) => log::warn!("Couldn't sync playlist file {}: {e}", path.display()),
            }
        }
        Ok(report)
    }

    /// The playlist for the file at `path` the user chose to import, when
    /// that file is in a library folder: it becomes (or already is) a
    /// file-backed playlist. `None` for a file outside the library, which
    /// imports as a plain copy.
    pub(super) fn import_library_playlist_file(&mut self, path: &Path) -> Result<Option<i64>> {
        if !in_library_folder(&*self.db.pool.get()?, path)? {
            return Ok(None);
        }
        Ok(match self.sync_file(path, true)? {
            Inbound::Synced { playlist_id, .. } => Some(playlist_id),
            Inbound::Conflict(c) => Some(c.playlist_id),
            Inbound::Detached(_) | Inbound::Skipped => None,
        })
    }

    /// Brings the playlist linked to `path` (or a new one) in line with the
    /// file. `chosen` is a file the user picked themself, so the album-folder
    /// rule doesn't apply to it.
    fn sync_file(&mut self, path: &Path, chosen: bool) -> Result<Inbound> {
        let conn = self.db.pool.get()?;
        // The file before its row: the writer records a new hash before it
        // writes, so bytes Luminous just wrote always find their hash here.
        let read = std::fs::read(path);
        let linked = LinkedFile::by_path(&conn, path)?;
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(match linked {
                    Some(l) => {
                        unlink(&conn, l.playlist_id)?;
                        Inbound::Detached(l.playlist_id)
                    }
                    None => Inbound::Skipped,
                });
            }
            Err(e) => return Err(e.into()),
        };
        let Some(format) = PlaylistFormat::from_path(path) else {
            return Ok(Inbound::Skipped);
        };
        let hash = content_hash(&bytes);

        if let Some(l) = &linked {
            if l.conflict || l.file_hash == hash {
                return Ok(Inbound::Synced {
                    playlist_id: l.playlist_id,
                    changed: false,
                });
            }
            if l.render_hash_now(&conn)? != l.render_hash {
                return Ok(Inbound::Conflict(set_conflict(&conn, l.playlist_id)?));
            }
        }

        let parsed = playlist::parse_playlist_bytes(&bytes, format)?;
        // Before resolving anything: a ripper's per-album playlist is
        // common, and resolving every one on every scan would be wasted.
        if linked.is_none() && !chosen && lists_only_its_own_folder(path, &parsed.tracks) {
            return Ok(Inbound::Skipped);
        }
        let entries = resolve_entries(&conn, &parsed.tracks, path)?;
        let file_name = playlist_name_for(path, parsed.title.clone());
        // The name the playlist ends up with. A file only names its playlist
        // when it first links, when it was moved or renamed outside
        // Luminous, or through an XSPF title; a plain content change keeps
        // the playlist's own name, which may differ from the file's (a
        // rename whose file name was taken, or had to be sanitized).
        let (playlist_id, name) = match linked {
            Some(l) => (l.playlist_id, parsed.title.map(|_| file_name)),
            None => match moved_playlist(&conn, &hash)? {
                Some(id) => (id, Some(file_name)),
                None => match unlinked_twin(&conn, &file_name, &entries)? {
                    Some(id) => (id, None),
                    None => (self.create_playlist(&file_name)?.id, None),
                },
            },
        };
        // One transaction, so the writer never sees the new items without
        // the agreement that says they came from the file.
        let tx = conn.unchecked_transaction()?;
        replace_items(&tx, playlist_id, &entries)?;
        if let Some(name) = name {
            tx.execute(
                "UPDATE playlists SET name = ?1 WHERE id = ?2",
                params![name, playlist_id],
            )?;
        }
        tx.execute(
            "UPDATE playlists SET updated = ?1 WHERE id = ?2",
            params![crate::clock::unix_now(), playlist_id],
        )?;
        record_agreement(&tx, playlist_id, path, &bytes, &parsed.style)?;
        tx.commit()?;
        Ok(Inbound::Synced {
            playlist_id,
            changed: true,
        })
    }

    /// Writes every file-backed playlist whose rendering changed since it
    /// last agreed with its file, in the style the file was read in. A file
    /// changed on disk meanwhile is not overwritten: its playlist goes into
    /// conflict instead. A file that's gone unlinks its playlist. Run by
    /// [`PlaylistFileWriter`]; a failed write is logged and retried on the
    /// next pass.
    pub fn write_edited_playlist_files(&self) -> Result<FileSyncReport> {
        let conn = self.db.pool.get()?;
        let linked: Vec<i64> = conn
            .prepare("SELECT playlist_id FROM playlist_files WHERE conflict = 0")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut report = FileSyncReport::default();
        for playlist_id in linked {
            match self.write_file(&conn, playlist_id) {
                Ok(inbound) => report.record(inbound),
                Err(e) => log::warn!("Couldn't write the file of playlist {playlist_id}: {e}"),
            }
        }
        Ok(report)
    }

    /// Writes one playlist to its file if its rendering changed. The
    /// playlist itself is unchanged unless the file was gone (unlinked) or
    /// changed too (conflict).
    fn write_file(&self, conn: &Connection, playlist_id: i64) -> Result<Inbound> {
        let unchanged = Inbound::Synced {
            playlist_id,
            changed: false,
        };
        // The row and the rendering from one snapshot, so a file read in
        // between can't pair a new rendering with the old agreement.
        let snapshot = conn.unchecked_transaction()?;
        let Some(linked) = LinkedFile::by_playlist(&snapshot, playlist_id)? else {
            return Ok(Inbound::Skipped);
        };
        let bytes = render(&snapshot, playlist_id, &linked.path, &linked.style)?;
        snapshot.commit()?;
        let hash = content_hash(&bytes);
        if linked.conflict || hash == linked.render_hash {
            return Ok(unchanged);
        }
        let on_disk = match std::fs::read(&linked.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                unlink(conn, playlist_id)?;
                return Ok(Inbound::Detached(playlist_id));
            }
            Err(e) => return Err(e.into()),
        };
        if content_hash(&on_disk) != linked.file_hash {
            return Ok(Inbound::Conflict(set_conflict(conn, playlist_id)?));
        }
        // Record the new agreement before writing, so the watcher's notice
        // for this write finds the file already agreed and does nothing.
        // That, not the watcher's self-write list, keeps Luminous from
        // reading its own writes back: the list would also hide an edit made
        // elsewhere moments later. Only if the row is still the one this
        // pass read: a file read in meanwhile wins, and the next pass looks
        // again.
        let swap = "UPDATE playlist_files SET file_hash = ?1, render_hash = ?2
                    WHERE playlist_id = ?3 AND file_hash = ?4 AND render_hash = ?5
                      AND conflict = 0";
        let claimed = conn.execute(
            swap,
            params![
                hash,
                hash,
                playlist_id,
                linked.file_hash,
                linked.render_hash
            ],
        )?;
        if claimed == 0 {
            return Ok(unchanged);
        }
        if hash != linked.file_hash {
            if let Err(e) = std::fs::write(&linked.path, &bytes) {
                conn.execute(
                    swap,
                    params![
                        linked.file_hash,
                        linked.render_hash,
                        playlist_id,
                        hash,
                        hash
                    ],
                )?;
                return Err(e.into());
            }
        }
        Ok(unchanged)
    }

    /// Ends a conflict by keeping one side: [`KeepSide::Luminous`] writes the
    /// playlist over the file, [`KeepSide::File`] reads the file over the
    /// playlist. A playlist that isn't in conflict (or isn't file-backed) is
    /// left alone, so a second click is harmless.
    pub fn resolve_file_conflict(
        &mut self,
        playlist_id: i64,
        keep: KeepSide,
    ) -> Result<FileSyncReport> {
        let conn = self.db.pool.get()?;
        let Some(linked) = LinkedFile::by_playlist(&conn, playlist_id)? else {
            return Ok(FileSyncReport::default());
        };
        if !linked.conflict {
            return Ok(FileSyncReport::default());
        }
        let mut report = FileSyncReport::default();
        report.add_changed(playlist_id);
        match keep {
            KeepSide::Luminous => {
                // Agree on the file as it is now and on no rendering at all,
                // so the writer finds the playlist changed and writes it.
                let on_disk = std::fs::read(&linked.path).unwrap_or_default();
                conn.execute(
                    "UPDATE playlist_files
                     SET conflict = 0, file_hash = ?1, render_hash = ''
                     WHERE playlist_id = ?2",
                    params![content_hash(&on_disk), playlist_id],
                )?;
                self.nudge_writer();
            }
            KeepSide::File => {
                // Agree on the playlist as it is now and on no file at all,
                // so the file reads in over it.
                conn.execute(
                    "UPDATE playlist_files
                     SET conflict = 0, file_hash = '', render_hash = ?1
                     WHERE playlist_id = ?2",
                    params![linked.render_hash_now(&conn)?, playlist_id],
                )?;
                drop(conn);
                self.sync_file(&linked.path, true)?;
            }
        }
        Ok(report)
    }

    /// Renames `playlist_id`'s file to match its new name, keeping the
    /// extension and folder. If a file of that name already exists the file
    /// keeps its old name; the playlist is renamed either way. An `.xspf`
    /// also stores the name inside, so the writer is nudged to rewrite it.
    pub(super) fn rename_linked_file(
        &self,
        conn: &Connection,
        playlist_id: i64,
        name: &str,
    ) -> Result<()> {
        let Some(linked) = LinkedFile::by_playlist(conn, playlist_id)? else {
            return Ok(());
        };
        let ext = linked
            .path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let target = linked
            .path
            .with_file_name(format!("{}.{ext}", file_stem_for(name)));
        if !library_paths::same(&target, &linked.path) && !target.exists() {
            std::fs::rename(&linked.path, &target)?;
            conn.execute(
                "UPDATE playlist_files SET path = ?1, path_key = ?2 WHERE playlist_id = ?3",
                params![target.to_string_lossy(), path_key(&target), playlist_id],
            )?;
        }
        self.nudge_writer();
        Ok(())
    }

    /// Moves `playlist_id`'s file to the system trash before the playlist
    /// is deleted, so the next scan doesn't bring it back. A file already
    /// gone is fine; one that can't be trashed fails the delete, leaving
    /// both in place.
    pub(super) fn trash_linked_file(&self, conn: &Connection, playlist_id: i64) -> Result<()> {
        let Some(linked) = LinkedFile::by_playlist(conn, playlist_id)? else {
            return Ok(());
        };
        if linked.path.exists() {
            trash::delete(&linked.path).map_err(|e| {
                anyhow!("Couldn't move {} to the trash: {e}", linked.path.display())
            })?;
        }
        Ok(())
    }

    /// Links a playlist just exported to `path` with the file, when `path`
    /// is in a library folder and the playlist is one a user edits by hand
    /// (not the Queue, a smart or an auto playlist) and isn't linked yet.
    /// Without this, the next scan would import the export as a twin.
    pub(super) fn link_exported_file(
        &self,
        conn: &Connection,
        playlist_id: i64,
        path: &Path,
        bytes: &[u8],
        style: &FileStyle,
    ) -> Result<()> {
        if !in_library_folder(conn, path)? {
            return Ok(());
        }
        let (name, dynamic, system): (String, bool, bool) = conn.query_row(
            "SELECT name, dynamic_enabled, COALESCE(is_system, 0) FROM playlists WHERE id = ?1",
            params![playlist_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if dynamic || system || crate::models::Playlist::is_queue_row(&name, dynamic) {
            return Ok(());
        }
        if LinkedFile::by_playlist(conn, playlist_id)?.is_some()
            || LinkedFile::by_path(conn, path)?.is_some()
        {
            return Ok(());
        }
        record_agreement(conn, playlist_id, path, bytes, style)
    }

    /// Asks the writer to look at file-backed playlists soon. Call it after
    /// any change to a playlist; it is a channel send, and a playlist
    /// without a file costs nothing more.
    pub(super) fn nudge_writer(&self) {
        if let Some(writer) = &self.file_writer {
            writer.nudge();
        }
    }
}

/// The linked playlist whose file vanished from its recorded path and whose
/// last agreed content is exactly `hash`: the same file, moved or renamed
/// outside Luminous. Taking it over keeps the playlist instead of leaving a
/// detached copy beside a new one.
fn moved_playlist(conn: &Connection, hash: &str) -> Result<Option<i64>> {
    let mut stmt =
        conn.prepare("SELECT playlist_id, path FROM playlist_files WHERE file_hash = ?1")?;
    let candidates = stmt.query_map(params![hash], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    for candidate in candidates {
        let (id, path) = candidate?;
        if !Path::new(&path).exists() {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// A plain playlist (not linked, not the Queue, not smart or auto) named
/// `name` that holds exactly `entries` — one imported by hand from this file
/// before it was synced. Linking it avoids a duplicate.
fn unlinked_twin(conn: &Connection, name: &str, entries: &[ResolvedEntry]) -> Result<Option<i64>> {
    if crate::models::Playlist::is_queue_row(name, false) {
        return Ok(None);
    }
    let mut stmt = conn.prepare(
        "SELECT p.id FROM playlists p
         WHERE p.name = ?1 AND p.dynamic_enabled = 0 AND COALESCE(p.is_system, 0) = 0
           AND NOT EXISTS (SELECT 1 FROM playlist_files pf WHERE pf.playlist_id = p.id)",
    )?;
    let ids: Vec<i64> = stmt
        .query_map(params![name], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let wanted: Vec<_> = entries
        .iter()
        .map(|e| entry_key(e.song_id, &e.url))
        .collect();
    for id in ids {
        let have: Vec<_> = conn
            .prepare(
                "SELECT song_id, url FROM playlist_items WHERE playlist_id = ?1 ORDER BY position",
            )?
            .query_map(params![id], |r| {
                Ok(entry_key(
                    r.get(0)?,
                    r.get::<_, Option<String>>(1)?
                        .as_deref()
                        .unwrap_or_default(),
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        if have == wanted {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// Writes edited playlists to their files from a background thread, a
/// moment after the last edit (see [`WRITE_DEBOUNCE`]). Cloned into the
/// playlist manager, whose edits nudge it.
#[derive(Clone, Debug)]
pub struct PlaylistFileWriter {
    nudge: mpsc::Sender<()>,
}

impl PlaylistFileWriter {
    /// Starts the writer thread. It makes one pass straight away, so edits
    /// a previous session didn't get to write (it quit inside the debounce)
    /// still reach their files. `on_report` hears about each pass that
    /// unlinked a playlist or found a conflict.
    pub fn spawn(
        db: Arc<crate::db::Database>,
        on_report: impl Fn(FileSyncReport) + Send + 'static,
    ) -> Result<Self> {
        let (nudge, nudges) = mpsc::channel::<()>();
        let writer = PlaylistManager::new(db)?;
        std::thread::Builder::new()
            .name("playlist-file-writer".into())
            .spawn(move || loop {
                // Wait out the burst; a closed channel still gets its last pass.
                while nudges.recv_timeout(WRITE_DEBOUNCE).is_ok() {}
                match writer.write_edited_playlist_files() {
                    Ok(report) if !report.is_empty() => on_report(report),
                    Ok(_) => {}
                    Err(e) => log::warn!("Couldn't write edited playlist files: {e}"),
                }
                if nudges.recv().is_err() {
                    break;
                }
            })?;
        Ok(Self { nudge })
    }

    /// Asks for a write pass shortly. Cheap; call it whenever something may
    /// have changed a file-backed playlist (an edit, or `library-changed`,
    /// since the organizer and duplicate merges change songs under them).
    pub fn nudge(&self) {
        let _ = self.nudge.send(());
    }
}

/// Spawns [`PlaylistFileWriter`], wires the `library-changed` nudge listener,
/// and attaches the writer to `playlists` (#2280).
pub fn start(
    app: &tauri::AppHandle,
    db: Arc<crate::db::Database>,
    playlists: &Arc<tokio::sync::Mutex<PlaylistManager>>,
) {
    use tauri::Listener;

    let handle = app.clone();
    match PlaylistFileWriter::spawn(db, move |report| announce(&handle, &report)) {
        Ok(writer) => {
            // The organizer and duplicate merges change songs under file-backed
            // playlists without an edit of their own; their library change wakes
            // the writer.
            let nudger = writer.clone();
            app.listen("library-changed", move |_| nudger.nudge());
            playlists.blocking_lock().attach_file_sync(writer);
        }
        Err(e) => log::error!("Failed to start the playlist file writer: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::import_export::clean_path;
    use super::*;
    use crate::db::Database;

    /// A library folder `Music` with two songs, and a manager over it.
    struct Library {
        _dir: tempfile::TempDir,
        root: PathBuf,
        db: Arc<Database>,
        pm: PlaylistManager,
        songs: [PathBuf; 2],
    }

    impl Library {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let db = Arc::new(Database::new(dir.path().join("data")).unwrap());
            let root = clean_path(dir.path()).join("Music");
            let album = root.join("Band").join("Album");
            std::fs::create_dir_all(&album).unwrap();
            let songs = [album.join("01.mp3"), album.join("02.mp3")];
            let conn = db.pool.get().unwrap();
            conn.execute(
                "INSERT INTO directories (path) VALUES (?1)",
                params![root.to_string_lossy()],
            )
            .unwrap();
            for (i, song) in songs.iter().enumerate() {
                std::fs::write(song, b"audio").unwrap();
                conn.execute(
                    "INSERT INTO songs (id, title, artist, path) VALUES (?1, ?2, 'Band', ?3)",
                    params![
                        i as i64 + 1,
                        format!("Song {}", i + 1),
                        song.to_string_lossy()
                    ],
                )
                .unwrap();
            }
            drop(conn);
            let pm = PlaylistManager::new(Arc::clone(&db)).unwrap();
            Self {
                _dir: dir,
                root,
                db,
                pm,
                songs,
            }
        }

        /// Writes `text` to `Music/Playlists/<name>` and returns its path.
        fn playlist_file(&self, name: &str, text: &str) -> PathBuf {
            let path = self.root.join("Playlists").join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        }

        fn song_ids(&self, playlist_id: i64) -> Vec<Option<i64>> {
            self.pm
                .get_playlist_tracks(playlist_id)
                .unwrap()
                .iter()
                .map(|i| i.song.as_ref().map(|s| s.id))
                .collect()
        }

        fn playlist(&self, id: i64) -> crate::models::Playlist {
            self.pm
                .get_playlists()
                .unwrap()
                .into_iter()
                .find(|p| p.id == id)
                .unwrap()
        }

        /// What a scan of `roots` that found `found` syncs, as `scan_all` runs it.
        fn scanned(&mut self, found: &[PathBuf], roots: &[PathBuf]) -> FileSyncReport {
            let conn = self.db.pool.get().unwrap();
            let paths = scanned_playlist_files(&conn, found, roots).unwrap();
            drop(conn);
            let mut report = FileSyncReport::default();
            for path in paths {
                report.merge(self.pm.sync_playlist_files(&[path]).unwrap());
            }
            report
        }

        fn only_synced(&mut self, path: &Path) -> i64 {
            let report = self.pm.sync_playlist_files(&[path.to_path_buf()]).unwrap();
            assert_eq!(report.changed.len(), 1, "{report:?}");
            report.changed[0]
        }
    }

    const MIX: &str = "#EXTM3U\r\n..\\Band\\Album\\01.mp3\r\n..\\Band\\Album\\02.mp3\r\n";

    #[test]
    fn a_library_playlist_file_becomes_a_file_backed_playlist_once() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);

        let playlist = lib.playlist(id);
        assert_eq!(playlist.name, "Mix");
        assert_eq!(
            playlist.source_path.as_deref(),
            Some(&*path.to_string_lossy())
        );
        assert!(!playlist.source_conflict);
        assert_eq!(lib.song_ids(id), vec![Some(1), Some(2)]);

        let root = lib.root.clone();
        let again = lib.scanned(std::slice::from_ref(&path), &[root]);
        assert!(again.is_empty(), "an unchanged file is left alone");
        assert_eq!(lib.pm.get_playlists().unwrap().len(), 1);
    }

    #[test]
    fn an_outside_edit_updates_the_playlist_in_place_keeping_item_ids() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);
        let uuid_of_second = lib.pm.get_playlist_tracks(id).unwrap()[1].uuid.clone();

        std::fs::write(&path, "..\\Band\\Album\\02.mp3\r\n").unwrap();
        assert_eq!(lib.only_synced(&path), id);

        let items = lib.pm.get_playlist_tracks(id).unwrap();
        assert_eq!(lib.song_ids(id), vec![Some(2)]);
        assert_eq!(items[0].uuid, uuid_of_second);
    }

    #[test]
    fn a_scan_skips_an_album_folders_own_playlist_but_an_explicit_import_keeps_it() {
        let mut lib = Library::new();
        let album = lib.songs[0].parent().unwrap().to_path_buf();
        let path = album.join("Album.m3u");
        std::fs::write(&path, "01.mp3\n02.mp3\n").unwrap();

        let report = lib
            .pm
            .sync_playlist_files(std::slice::from_ref(&path))
            .unwrap();
        assert!(report.is_empty());
        assert!(lib.pm.get_playlists().unwrap().is_empty());

        let imported = lib.pm.import_playlist(&path).unwrap();
        assert!(imported.source_path.is_some());
    }

    #[test]
    fn an_edit_in_luminous_is_written_in_the_files_own_style() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);
        let first = lib.pm.get_playlist_tracks(id).unwrap()[0].uuid.clone();

        lib.pm.remove_from_playlist(id, &[first]).unwrap();
        assert!(lib.pm.write_edited_playlist_files().unwrap().is_empty());

        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "#EXTM3U\r\n#EXTINF:-1,Band - Song 2\r\n..\\Band\\Album\\02.mp3\r\n"
        );
        let reread = lib
            .pm
            .sync_playlist_files(std::slice::from_ref(&path))
            .unwrap();
        assert!(reread.is_empty(), "the write doesn't read back as a change");
        assert_eq!(lib.song_ids(id), vec![Some(2)]);

        // An edit elsewhere straight after Luminous's write still lands.
        std::fs::write(
            &path,
            "..\\Band\\Album\\01.mp3\r\n..\\Band\\Album\\02.mp3\r\n",
        )
        .unwrap();
        lib.pm
            .sync_playlist_files(std::slice::from_ref(&path))
            .unwrap();
        assert_eq!(lib.song_ids(id), vec![Some(1), Some(2)]);
    }

    #[test]
    fn both_sides_changing_is_a_conflict_until_the_user_keeps_one() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);

        lib.pm.add_songs_to_playlist(id, &[1]).unwrap();
        std::fs::write(&path, "..\\Band\\Album\\02.mp3\r\n").unwrap();

        let report = lib
            .pm
            .sync_playlist_files(std::slice::from_ref(&path))
            .unwrap();
        assert_eq!(
            report.new_conflicts,
            vec![FileConflict {
                playlist_id: id,
                name: "Mix".into()
            }]
        );
        assert!(lib.playlist(id).source_conflict);
        lib.pm.write_edited_playlist_files().unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "..\\Band\\Album\\02.mp3\r\n",
            "a conflict writes nothing"
        );
        assert_eq!(lib.song_ids(id), vec![Some(1), Some(2), Some(1)]);

        lib.pm.resolve_file_conflict(id, KeepSide::File).unwrap();
        assert_eq!(lib.song_ids(id), vec![Some(2)]);
        assert!(!lib.playlist(id).source_conflict);
    }

    #[test]
    fn keeping_luminous_in_a_conflict_overwrites_the_file() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);
        lib.pm.add_songs_to_playlist(id, &[1]).unwrap();
        std::fs::write(&path, "..\\Band\\Album\\02.mp3\r\n").unwrap();
        lib.pm
            .sync_playlist_files(std::slice::from_ref(&path))
            .unwrap();

        lib.pm
            .resolve_file_conflict(id, KeepSide::Luminous)
            .unwrap();
        lib.pm.write_edited_playlist_files().unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("01.mp3").count(), 2, "{text}");
        assert!(lib.pm.sync_playlist_files(&[path]).unwrap().is_empty());
        assert_eq!(lib.song_ids(id), vec![Some(1), Some(2), Some(1)]);
    }

    #[test]
    fn a_deleted_file_leaves_a_plain_playlist_but_an_unwalked_root_does_not() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);

        std::fs::remove_file(&path).unwrap();
        let elsewhere = PathBuf::from("/not/scanned");
        let report = lib.scanned(&[], &[elsewhere]);
        assert!(
            report.is_empty(),
            "a root the scan didn't walk unlinks nothing"
        );
        assert!(lib.playlist(id).source_path.is_some());

        let root = lib.root.clone();
        let report = lib.scanned(&[], &[root]);
        assert_eq!(report.changed, vec![id]);
        assert_eq!(lib.playlist(id).source_path, None);
        assert_eq!(lib.song_ids(id), vec![Some(1), Some(2)]);
    }

    #[test]
    fn a_file_moved_outside_luminous_keeps_its_playlist() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);
        let moved = lib.root.join("Playlists").join("Road Trip.m3u");
        std::fs::rename(&path, &moved).unwrap();

        let report = lib.pm.sync_playlist_files(&[path, moved.clone()]).unwrap();
        assert_eq!(report.changed, vec![id]);
        let playlist = lib.playlist(id);
        assert_eq!(playlist.name, "Road Trip");
        assert_eq!(
            playlist.source_path.as_deref(),
            Some(&*moved.to_string_lossy())
        );
        assert_eq!(lib.pm.get_playlists().unwrap().len(), 1);
    }

    #[test]
    fn renaming_the_playlist_renames_its_file() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);

        lib.pm.rename_playlist(id, "Night: Drive?").unwrap();

        let renamed = lib.root.join("Playlists").join("Night_ Drive_.m3u");
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_to_string(&renamed).unwrap(),
            MIX,
            "content untouched"
        );
        assert_eq!(
            lib.playlist(id).source_path.as_deref(),
            Some(&*renamed.to_string_lossy())
        );
        assert!(lib.pm.sync_playlist_files(&[renamed]).unwrap().is_empty());
    }

    #[test]
    fn an_export_into_the_library_links_and_one_elsewhere_does_not() {
        let mut lib = Library::new();
        let playlist = lib.pm.create_playlist("Mine").unwrap();
        lib.pm.add_songs_to_playlist(playlist.id, &[2]).unwrap();

        let outside = lib.root.parent().unwrap().join("Mine.m3u8");
        lib.pm.export_playlist(playlist.id, &outside, true).unwrap();
        assert_eq!(lib.playlist(playlist.id).source_path, None);

        let inside = lib.root.join("Mine.m3u8");
        lib.pm.export_playlist(playlist.id, &inside, true).unwrap();
        assert!(lib.playlist(playlist.id).source_path.is_some());
        assert!(lib.pm.sync_playlist_files(&[inside]).unwrap().is_empty());
        assert_eq!(lib.pm.get_playlists().unwrap().len(), 1);
    }

    #[test]
    fn an_earlier_manual_import_of_the_file_is_linked_not_duplicated() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let manual = lib.pm.create_playlist("Mix").unwrap();
        lib.pm.add_songs_to_playlist(manual.id, &[1, 2]).unwrap();
        // A by-hand import stores each entry's resolved path as its url.
        let conn = lib.db.pool.get().unwrap();
        for (pos, song) in lib.songs.iter().enumerate() {
            conn.execute(
                "UPDATE playlist_items SET url = ?1 WHERE playlist_id = ?2 AND position = ?3",
                params![song.to_string_lossy(), manual.id, pos as i32],
            )
            .unwrap();
        }
        drop(conn);

        assert_eq!(lib.only_synced(&path), manual.id);
        assert_eq!(lib.pm.get_playlists().unwrap().len(), 1);
    }

    #[test]
    fn a_station_list_in_the_library_is_one_file_playlist_however_it_is_imported() {
        let mut lib = Library::new();
        let url = "https://stream.example.com/groove";
        let path = lib.playlist_file(
            "Stations.pls",
            &format!("[playlist]\nNumberOfEntries=1\nFile1={url}\nTitle1=Groove\n"),
        );
        let id = lib.only_synced(&path);
        let station = lib.pm.get_playlist_tracks(id).unwrap()[0]
            .song
            .clone()
            .unwrap();
        assert_eq!(station.stream_url.as_deref(), Some(url));

        // The Radio view's "Import as a station list?" calls import_playlist.
        let imported = lib.pm.import_playlist(&path).unwrap();
        assert_eq!(imported.id, id);
        assert_eq!(imported.kind, crate::models::PlaylistKind::StationList);
        assert!(imported.station_list_spec.is_none());
        assert_eq!(lib.pm.get_playlists().unwrap().len(), 1);

        lib.pm.add_songs_to_playlist(id, &[station.id]).unwrap();
        lib.pm.write_edited_playlist_files().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches(url).count(), 2, "{text}");
        assert!(!text.contains("radio://"));
    }

    #[test]
    fn an_outside_edit_keeps_a_name_the_file_couldnt_take() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);
        lib.playlist_file("Taken.m3u", "");

        lib.pm.rename_playlist(id, "Taken").unwrap();
        assert!(
            path.exists(),
            "the file keeps its name when the new one is taken"
        );
        std::fs::write(&path, "..\\Band\\Album\\02.mp3\r\n").unwrap();
        lib.only_synced(&path);

        assert_eq!(lib.playlist(id).name, "Taken");
    }

    #[test]
    fn undo_sort_and_a_song_moving_all_reach_the_file() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);
        let lines = |path: &Path| -> Vec<String> {
            std::fs::read_to_string(path)
                .unwrap()
                .lines()
                .filter(|l| !l.starts_with('#'))
                .map(str::to_string)
                .collect()
        };

        let first = lib.pm.get_playlist_tracks(id).unwrap()[0].uuid.clone();
        lib.pm.remove_from_playlist(id, &[first]).unwrap();
        lib.pm.write_edited_playlist_files().unwrap();
        assert_eq!(lines(&path).len(), 1);
        lib.pm.undo().unwrap();
        lib.pm.write_edited_playlist_files().unwrap();
        assert_eq!(lines(&path).len(), 2, "undo is an edit too");

        lib.pm.set_playlist_sort(id, Some("title"), false).unwrap();
        lib.pm.write_edited_playlist_files().unwrap();
        assert!(lines(&path)[0].ends_with("02.mp3"), "{:?}", lines(&path));

        // The organizer moving a song changes no playlist row, only the song.
        let moved = lib.root.join("Band").join("Moved.mp3");
        lib.db
            .pool
            .get()
            .unwrap()
            .execute(
                "UPDATE songs SET path = ?1 WHERE id = 1",
                params![moved.to_string_lossy()],
            )
            .unwrap();
        lib.pm.write_edited_playlist_files().unwrap();
        assert!(lines(&path).iter().any(|l| l.ends_with("Moved.mp3")));
    }

    #[test]
    fn a_file_named_after_the_queue_imports_under_its_file_name() {
        let mut lib = Library::new();
        lib.pm.queue().unwrap();
        let path = lib.playlist_file("Queue.m3u", MIX);
        let id = lib.only_synced(&path);
        assert_eq!(lib.playlist(id).name, "Queue.m3u");
    }

    #[test]
    fn a_vanished_file_unlinks_on_write_and_deleting_needs_no_file() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);
        lib.pm.add_songs_to_playlist(id, &[1]).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(
            lib.pm.write_edited_playlist_files().unwrap().changed,
            vec![id]
        );
        assert_eq!(lib.playlist(id).source_path, None);
        lib.pm.delete_playlist(id).unwrap();
        assert!(lib.pm.get_playlists().unwrap().is_empty());
    }

    #[test]
    fn reading_a_file_in_never_rewrites_it_on_its_own() {
        // Luminous would write this file with #EXTINF lines; that is no
        // reason to rewrite a file nobody changed.
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        lib.only_synced(&path);

        assert!(lib.pm.write_edited_playlist_files().unwrap().is_empty());

        assert_eq!(std::fs::read_to_string(&path).unwrap(), MIX);
    }

    #[test]
    fn a_change_made_outside_the_playlist_editor_reaches_the_file() {
        // A duplicate merge rewrites playlist rows without going through
        // the editor and without nudging; the next pass still finds it.
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        lib.only_synced(&path);

        let conn = lib.db.pool.get().unwrap();
        crate::collection::reconcile::merge_song_rows(&conn, 2, 1).unwrap();
        drop(conn);
        assert!(lib.pm.write_edited_playlist_files().unwrap().is_empty());

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("01.mp3"), "{text}");
        assert!(text.contains("02.mp3"), "{text}");
    }

    #[test]
    fn a_tag_edit_reaches_the_titles_in_the_file() {
        let mut lib = Library::new();
        let path = lib.playlist_file("Mix.m3u", MIX);
        let id = lib.only_synced(&path);
        lib.pm.add_songs_to_playlist(id, &[1]).unwrap();
        lib.pm.write_edited_playlist_files().unwrap();

        let conn = lib.db.pool.get().unwrap();
        conn.execute("UPDATE songs SET title = 'Renamed' WHERE id = 2", [])
            .unwrap();
        drop(conn);
        lib.pm.write_edited_playlist_files().unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("#EXTINF:-1,Band - Renamed\r\n"), "{text}");
    }
}
