//! Playlist import (`.m3u`/`.pls`/etc.) and export. Split out of `playlist.rs`
//! (#577 item 18) — a second `impl PlaylistManager` block alongside the one
//! in `playlist.rs` that owns CRUD/auto-playlist-sync/dynamic-playlist logic.
//! The resolve/replace helpers here are shared with `file_sync`, which keeps
//! playlist files in library folders in sync with their playlists (#2280).

use super::snapshot::{strip_snapshot, TrackSnapshot};
use super::PlaylistManager;
use crate::models::{Playlist, PlaylistItem};
use anyhow::{anyhow, Result};
use luminous_metadata::playlist::{ExportTrack, ParsedTrack};
use rusqlite::params;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use uuid::Uuid;

/// Normalize a path to the same form used for `songs.path` in the DB, so an
/// imported playlist's track paths (from `.m3u`/`.pls`, often relative or
/// with `./`/`../` segments) can be matched against already-scanned library
/// songs by exact string comparison. Prefers `canonicalize` (resolves
/// symlinks too, and strips Windows' `\\?\` verbatim prefix so the result
/// matches how `collection.rs` stores paths); if the path doesn't exist on
/// disk yet, or lives on a mapped network drive, falls back to lexically
/// collapsing `.`/`..` components without touching the filesystem.
pub(super) fn clean_path<P: AsRef<std::path::Path>>(path: P) -> std::path::PathBuf {
    let p = path.as_ref();
    if let Ok(canonical) = std::fs::canonicalize(p) {
        let s = canonical.to_string_lossy();
        #[cfg(windows)]
        let cleaned_s = match s.strip_prefix(r"\\?\UNC\") {
            // A mapped network drive (`Z:\...`) canonicalizes to its UNC
            // target, which never matches the drive-letter form the scanner
            // stored — fall through to the lexical cleanup instead. A path
            // that was already UNC gets its `\\` prefix back.
            Some(unc) if p.to_string_lossy().starts_with(r"\\") => Some(format!(r"\\{unc}")),
            Some(_) => None,
            None => Some(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()),
        };
        #[cfg(not(windows))]
        let cleaned_s = Some(s.to_string());

        if let Some(cleaned_s) = cleaned_s {
            return std::path::PathBuf::from(cleaned_s);
        }
    }

    use std::path::Component;
    let mut components = Vec::new();
    for component in p.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                components.pop();
            }
            c => components.push(c),
        }
    }
    components.iter().collect()
}

/// The library song a playlist track refers to, or `None`. Tries each of
/// `paths` exactly and then separator/case-insensitively, then falls back to
/// title + artist (case-insensitive), requiring the length to be within 2 s
/// of `duration_sec` when that is known. `beginning_nanosec` narrows a path
/// match to the CUE track starting there; `None` takes the file's first track
/// (a playlist file line names a whole file). Shared by playlist import and
/// "Relink unavailable", so both resolve a track the same way.
fn match_library_song(
    conn: &rusqlite::Connection,
    paths: &[&str],
    beginning_nanosec: Option<i64>,
    title: Option<&str>,
    artist: Option<&str>,
    duration_sec: Option<i64>,
) -> Option<i64> {
    for path in paths.iter().filter(|p| !p.is_empty()) {
        let by_path = conn
            .query_row(
                "SELECT id FROM songs
                 WHERE (path = ?1 OR LOWER(REPLACE(path, '/', '\\')) = LOWER(?2))
                   AND (?3 IS NULL OR beginning_nanosec = ?3)
                 ORDER BY beginning_nanosec LIMIT 1",
                params![path, path.replace('/', "\\"), beginning_nanosec],
                |row| row.get(0),
            )
            .ok();
        if by_path.is_some() {
            return by_path;
        }
    }
    let (title, artist) = (title?, artist?);
    match duration_sec {
        Some(dur) => conn.query_row(
            "SELECT id FROM songs WHERE LOWER(title) = LOWER(?1) AND LOWER(artist) = LOWER(?2) AND ABS((length_nanosec / 1000000000) - ?3) <= 2 LIMIT 1",
            params![title, artist, dur],
            |row| row.get(0),
        ),
        None => conn.query_row(
            "SELECT id FROM songs WHERE LOWER(title) = LOWER(?1) AND LOWER(artist) = LOWER(?2) LIMIT 1",
            params![title, artist],
            |row| row.get(0),
        ),
    }
    .ok()
}

/// One entry of a playlist file, resolved against the library.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ResolvedEntry {
    /// The library song (or radio station row) the entry is, if any.
    pub song_id: Option<i64>,
    /// Where the entry points, relative locations resolved against the
    /// playlist file's folder: what the playlist item keeps as its `url`.
    pub url: String,
    /// For an entry the library doesn't have, what the file said about it
    /// (a `TrackSnapshot`), so it still shows and can be relinked later.
    pub snapshot: Option<String>,
}

/// Resolves the entries of the playlist file at `playlist_path`: a stream URL
/// becomes a radio station row, and a file is matched to a library song by
/// path, then by title + artist (+ duration). Nothing is dropped — an entry
/// that matches nothing comes back with `song_id: None` and a snapshot.
pub(super) fn resolve_entries(
    conn: &rusqlite::Connection,
    tracks: &[ParsedTrack],
    playlist_path: &Path,
) -> Result<Vec<ResolvedEntry>> {
    use luminous_metadata::playlist;

    let base_dir = playlist_path.parent();
    let mut entries = Vec::with_capacity(tracks.len());
    for track in tracks {
        if playlist::is_http_url(&track.path_or_url) {
            let song = crate::radio_directory::upsert_stream_station(
                conn,
                &track.path_or_url,
                track.title.as_deref(),
                None,
            )?;
            entries.push(ResolvedEntry {
                song_id: Some(song.id),
                url: song.path.unwrap_or_else(|| track.path_or_url.clone()),
                snapshot: None,
            });
            continue;
        }

        // A playlist written on Windows separates folders with `\`, which
        // only Windows reads as a separator; elsewhere the whole location
        // would be one file name. The parsed text keeps its `\` so the
        // file's style (`FileStyle::backslash`) survives a rewrite.
        let location = if cfg!(windows) {
            track.path_or_url.clone()
        } else {
            track.path_or_url.replace('\\', "/")
        };
        let mut resolved_path = std::path::PathBuf::from(&location);
        if resolved_path.is_relative() {
            if let Some(base) = base_dir {
                resolved_path = base.join(&resolved_path);
            }
        }
        let cleaned_path = clean_path(&resolved_path);
        let path_str = cleaned_path.to_string_lossy().to_string();
        let paths = [path_str.as_str(), track.path_or_url.as_str()];

        // Path first: it settles almost every entry without opening the file.
        if let Some(song_id) = match_library_song(conn, &paths, None, None, None, None) {
            entries.push(ResolvedEntry {
                song_id: Some(song_id),
                url: path_str,
                snapshot: None,
            });
            continue;
        }

        let mut title = track.title.clone();
        let mut artist = track.artist.clone();
        let mut album = track.album.clone();
        if (title.is_none() || artist.is_none()) && cleaned_path.is_file() {
            if let Ok(meta) = luminous_metadata::audio::read_tags(&cleaned_path) {
                title = title.or(meta.title);
                artist = artist.or(meta.artist);
                album = album.or(meta.album);
            }
        }
        if title.is_none() && cleaned_path.is_file() {
            title = cleaned_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string());
        }

        let song_id = match_library_song(
            conn,
            &paths,
            None,
            title.as_deref(),
            artist.as_deref(),
            track.duration_sec,
        );
        let snapshot = song_id.is_none().then(|| {
            TrackSnapshot {
                title,
                artist,
                album,
                path: Some(path_str.clone()),
                duration_sec: track.duration_sec,
                beginning_nanosec: 0,
            }
            .to_json()
        });
        entries.push(ResolvedEntry {
            song_id,
            url: path_str,
            snapshot,
        });
    }
    Ok(entries)
}

/// Replaces `playlist_id`'s items with `entries`, in order. An item already
/// in the playlist for the same song (or, for one not in the library, the
/// same location) keeps its uuid and its other item keys, so re-reading a
/// changed playlist file doesn't disturb what Luminous remembers about the
/// tracks that stayed. Doesn't touch the playlist row itself. Run it inside
/// a transaction: it deletes before it inserts.
pub(super) fn replace_items(
    tx: &rusqlite::Connection,
    playlist_id: i64,
    entries: &[ResolvedEntry],
) -> Result<()> {
    // Each key's existing items, in order, as (uuid, additional_metadata).
    type Kept = VecDeque<(String, Option<String>)>;
    let mut kept: HashMap<(Option<i64>, String), Kept> = HashMap::new();
    {
        let mut stmt = tx.prepare(
            "SELECT song_id, url, uuid, additional_metadata FROM playlist_items
             WHERE playlist_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map(params![playlist_id], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        for row in rows {
            let (song_id, url, uuid, meta) = row?;
            let key = entry_key(song_id, url.as_deref().unwrap_or_default());
            kept.entry(key).or_default().push_back((uuid, meta));
        }
    }
    tx.execute(
        "DELETE FROM playlist_items WHERE playlist_id = ?1",
        params![playlist_id],
    )?;
    {
        let mut insert = tx.prepare(
            "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type, url, additional_metadata)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
        )?;
        for (pos, entry) in entries.iter().enumerate() {
            let previous = kept
                .get_mut(&entry_key(entry.song_id, &entry.url))
                .and_then(VecDeque::pop_front);
            let (uuid, meta) = match previous {
                // A library song's snapshot keys are stale once it is matched.
                Some((uuid, meta)) if entry.song_id.is_some() => {
                    (uuid, meta.as_deref().and_then(strip_snapshot))
                }
                Some((uuid, _)) => (uuid, entry.snapshot.clone()),
                None => (Uuid::new_v4().to_string(), entry.snapshot.clone()),
            };
            insert.execute(params![
                playlist_id,
                entry.song_id,
                pos as i32,
                uuid,
                entry.url,
                meta
            ])?;
        }
    }
    Ok(())
}

/// What identifies an item across a re-read: its song, or for one not in
/// the library, its location.
pub(super) fn entry_key(song_id: Option<i64>, url: &str) -> (Option<i64>, String) {
    match song_id {
        Some(id) => (Some(id), String::new()),
        None => (None, crate::library_paths::key(url)),
    }
}

/// The playlist's items as tracks to write to a playlist file. A radio
/// station is written as its stream URL, not its internal `radio://` id; an
/// item not in the library keeps the location it was imported with.
pub(super) fn export_tracks(items: &[PlaylistItem]) -> Vec<ExportTrack<'_>> {
    items
        .iter()
        .filter_map(|item| {
            let Some(song) = &item.song else {
                return item.url.as_ref().map(|url| ExportTrack {
                    path: Path::new(url),
                    title: None,
                    artist: None,
                    album: None,
                    duration_sec: None,
                });
            };
            let is_radio_uri = song
                .path
                .as_deref()
                .is_some_and(|p| p.starts_with(crate::media_source::RADIO_URI_SCHEME));
            let location = if is_radio_uri || song.source.is_live_stream() {
                song.stream_url
                    .as_deref()
                    .or(song.path.as_deref())
                    .or(item.url.as_deref())?
            } else {
                song.path.as_deref().or(item.url.as_deref())?
            };
            Some(ExportTrack {
                path: Path::new(location),
                title: song.title.as_deref(),
                artist: song.artist.as_deref(),
                album: song.album.as_deref(),
                duration_sec: song.length_nanosec.map(|ns| ns / 1_000_000_000),
            })
        })
        .collect()
}

/// The name a playlist read from the file at `path` gets: the file's own
/// title (XSPF), else its file name without extension. A name reserved for
/// the built-in Queue keeps its extension, so a `Queue.m3u` still imports.
pub(super) fn playlist_name_for(path: &Path, title: Option<String>) -> String {
    let stem = || {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Imported Playlist".to_string())
    };
    let name = title.filter(|t| !t.trim().is_empty()).unwrap_or_else(stem);
    if super::is_reserved_playlist_name(&name) {
        path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Imported Playlist".to_string())
    } else {
        name
    }
}

impl PlaylistManager {
    /// Parse an `.m3u`/`.pls`/etc. playlist file (format detected from the
    /// extension) and create a new playlist from it, resolving each entry
    /// with [`resolve_entries`]. A file inside a library folder becomes a
    /// file-backed playlist kept in sync with it (see `file_sync`); importing
    /// one that already is returns its playlist instead of a duplicate.
    pub fn import_playlist<P: AsRef<std::path::Path>>(&mut self, file_path: P) -> Result<Playlist> {
        use luminous_metadata::playlist::{self, PlaylistFormat};

        let path = file_path.as_ref();
        if let Some(id) = self.import_library_playlist_file(path)? {
            return self
                .get_playlists()?
                .into_iter()
                .find(|p| p.id == id)
                .ok_or_else(|| anyhow!("Imported playlist {id} not found"));
        }

        let format = PlaylistFormat::from_path(path)
            .ok_or_else(|| anyhow!("Unsupported playlist format: {}", path.display()))?;
        let bytes = std::fs::read(path)?;
        let parsed = playlist::parse_playlist_bytes(&bytes, format)?;
        let playlist = self.create_playlist(&playlist_name_for(path, parsed.title))?;
        let conn = self.db.pool.get()?;
        let entries = resolve_entries(&conn, &parsed.tracks, path)?;
        let tx = conn.unchecked_transaction()?;
        replace_items(&tx, playlist.id, &entries)?;
        tx.commit()?;
        self.get_playlists()?
            .into_iter()
            .find(|p| p.id == playlist.id)
            .ok_or_else(|| anyhow!("Imported playlist {} not found", playlist.id))
    }

    /// Relinks `playlist_id`'s missing tracks (items whose library song is
    /// gone, or that import couldn't match) to library songs found by
    /// `match_library_song` from their stored path and tags. Each relinked
    /// item keeps its uuid and position and loses its snapshot keys. Returns
    /// how many were relinked. Items whose path reappears in the library are
    /// relinked automatically on insert; this covers moved or retagged files.
    pub fn relink_missing_items(&self, playlist_id: i64) -> Result<usize> {
        let conn = self.db.pool.get()?;
        let missing: Vec<(i64, Option<String>, Option<String>)> = conn
            .prepare(
                "SELECT id, url, additional_metadata FROM playlist_items
                 WHERE playlist_id = ?1 AND song_id IS NULL AND type = 0",
            )?
            .query_map(params![playlist_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<rusqlite::Result<_>>()?;

        let mut relinked = 0;
        for (item_id, url, meta) in missing {
            let snapshot = meta
                .as_deref()
                .and_then(TrackSnapshot::parse)
                .unwrap_or_default();
            let paths: Vec<&str> = [url.as_deref(), snapshot.path.as_deref()]
                .into_iter()
                .flatten()
                .collect();
            let Some(song_id) = match_library_song(
                &conn,
                &paths,
                Some(snapshot.beginning_nanosec),
                snapshot.title.as_deref(),
                snapshot.artist.as_deref(),
                snapshot.duration_sec,
            ) else {
                continue;
            };
            conn.execute(
                "UPDATE playlist_items SET song_id = ?1, additional_metadata = ?2 WHERE id = ?3",
                params![song_id, meta.as_deref().and_then(strip_snapshot), item_id],
            )?;
            relinked += 1;
        }
        if relinked > 0 {
            self.touch_updated(&conn, playlist_id)?;
        }
        Ok(relinked)
    }

    /// Write the playlist's tracks to `export_path` in the format inferred
    /// from its extension (fails if unsupported). `relative` controls
    /// whether track paths are written relative to `export_path`'s
    /// directory or as absolute paths — matters for moving the exported
    /// file to another machine/directory alongside its music. A file written
    /// into a library folder is linked to the playlist (see `file_sync`), so
    /// the next scan keeps the two in sync instead of importing a twin.
    pub fn export_playlist<P: AsRef<std::path::Path>>(
        &self,
        playlist_id: i64,
        export_path: P,
        relative: bool,
    ) -> Result<()> {
        use luminous_metadata::playlist::{self, FileStyle, PlaylistFormat};

        let path = export_path.as_ref();
        let format = PlaylistFormat::from_path(path).ok_or_else(|| {
            anyhow!(
                "Unsupported playlist format for export path: {}",
                path.display()
            )
        })?;

        let conn = self.db.pool.get()?;
        let playlist_name: String = conn.query_row(
            "SELECT name FROM playlists WHERE id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )?;
        let items = Self::get_playlist_tracks_from_conn(&conn, playlist_id)?;
        let style = FileStyle {
            relative,
            ..FileStyle::default()
        };
        let bytes = playlist::playlist_file_bytes(
            &playlist_name,
            &export_tracks(&items),
            format,
            path,
            &style,
        );

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        super::write_atomic(path, &bytes)?;
        self.link_exported_file(&conn, playlist_id, path, &bytes, &style)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup_test_db() -> (tempfile::TempDir, Database) {
        let temp_dir = tempfile::Builder::new()
            .prefix("luminous_playlist_import_export_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        (temp_dir, db)
    }

    #[test]
    fn test_import_relative_pls_resolution() {
        let (temp_dir, db) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        let music_dir = temp_dir.path().join("Music");
        std::fs::create_dir_all(&music_dir).unwrap();
        let song_file = music_dir.join("song1.mp3");
        std::fs::write(&song_file, b"dummy audio").unwrap();

        let song_path_str = clean_path(&song_file).to_string_lossy().to_string();

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, path) VALUES ('Song One', 'Artist One', ?1)",
                params![song_path_str],
            )
            .unwrap();
        }

        let downloads_dir = temp_dir.path().join("Downloads");
        std::fs::create_dir_all(&downloads_dir).unwrap();
        let pls_file = downloads_dir.join("playlist.pls");

        let pls_content = "[playlist]\nNumberOfEntries=1\nFile1=../Music/song1.mp3\n".to_string();
        std::fs::write(&pls_file, pls_content).unwrap();

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let imported = manager.import_playlist(&pls_file).unwrap();

        let tracks = manager.get_playlist_tracks(imported.id).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(
            tracks[0].song.as_ref().unwrap().title.as_deref(),
            Some("Song One")
        );
    }

    #[test]
    fn test_match_library_song_prefers_path_then_tags_within_two_seconds() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();
        conn.execute_batch(
            "INSERT INTO songs (id, title, artist, path, length_nanosec) VALUES
                 (1, 'Song', 'Band', 'C:/Music/a.flac', 200000000000),
                 (2, 'Other', 'Band', 'C:/Music/b.flac', 100000000000);",
        )
        .unwrap();
        let find = |path: &str, title, artist, dur| {
            match_library_song(&conn, &[path], None, title, artist, dur)
        };
        // Separator/case-insensitive path wins over contradicting tags.
        assert_eq!(
            find(r"c:\music\B.FLAC", Some("Song"), Some("Band"), None),
            Some(2)
        );
        assert_eq!(
            find("/gone.flac", Some("song"), Some("BAND"), Some(201)),
            Some(1)
        );
        assert_eq!(
            find("/gone.flac", Some("Song"), Some("Band"), Some(210)),
            None
        );
        assert_eq!(find("/gone.flac", Some("Song"), None, None), None);
    }

    #[test]
    fn test_match_library_song_narrows_a_path_match_to_the_cue_track_offset() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();
        conn.execute_batch(
            "INSERT INTO songs (id, title, artist, path, beginning_nanosec) VALUES
                 (1, 'One', 'Band', '/m/disc.flac', 0),
                 (2, 'Two', 'Band', '/m/disc.flac', 300000000000);",
        )
        .unwrap();
        let find = |offset, title| {
            match_library_song(
                &conn,
                &["/m/disc.flac"],
                offset,
                Some(title),
                Some("Band"),
                None,
            )
        };
        assert_eq!(find(Some(300_000_000_000), "Two"), Some(2));
        assert_eq!(
            find(None, "Two"),
            Some(1),
            "a whole-file reference takes the first track"
        );
        // Offsets changed (re-split CUE): fall through to the tags, not the first track.
        assert_eq!(find(Some(290_000_000_000), "Two"), Some(2));
    }

    #[test]
    fn test_relink_missing_items_matches_moved_song_by_tags_and_keeps_other_keys() {
        let (_dir, db) = setup_test_db();
        let db = std::sync::Arc::new(db);
        let manager = PlaylistManager::new(db.clone()).unwrap();
        let conn = db.pool.get().unwrap();
        conn.execute_batch(
            "INSERT INTO playlists (id, name, dynamic_enabled) VALUES (50, 'Mine', 0);
             INSERT INTO songs (id, title, artist, path, length_nanosec) VALUES
                 (1, 'Song', 'Band', '/old/a.flac', 200000000000);
             INSERT INTO playlist_items (playlist_id, song_id, position, uuid) VALUES
                 (50, 1, 0, 'moved'),
                 (50, NULL, 1, 'unknown');
             DELETE FROM songs WHERE id = 1;
             INSERT INTO songs (id, title, artist, path, length_nanosec) VALUES
                 (2, 'Song', 'Band', '/new/a.flac', 200000000000);",
        )
        .unwrap();
        conn.execute(
            "UPDATE playlist_items SET additional_metadata = json_set(additional_metadata, '$.autoContinue', json('true'))
             WHERE uuid = 'moved'",
            [],
        )
        .unwrap();

        assert_eq!(manager.relink_missing_items(50).unwrap(), 1);

        let (song_id, meta, pos): (Option<i64>, Option<String>, i32) = conn
            .query_row(
                "SELECT song_id, additional_metadata, position FROM playlist_items WHERE uuid = 'moved'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(song_id, Some(2));
        assert_eq!(meta.as_deref(), Some("{\"autoContinue\":true}"));
        assert_eq!(pos, 0);
        assert_eq!(
            manager.relink_missing_items(50).unwrap(),
            0,
            "nothing left that can match"
        );
    }

    #[test]
    fn test_import_pls_with_http_stream_creates_station_item_and_exports_stream_url() {
        let (temp_dir, db) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        let pls_file = temp_dir.path().join("stations.pls");
        let stream_url = "https://stream.example.com/live%20feed/channel?q=1";
        std::fs::write(
            &pls_file,
            format!(
                "[playlist]\nNumberOfEntries=1\nFile1={stream_url}\nTitle1=Indie Radio\nLength1=-1\n"
            ),
        )
        .unwrap();

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let imported = manager.import_playlist(&pls_file).unwrap();
        assert_eq!(imported.kind, crate::models::PlaylistKind::StationList);
        assert!(imported.station_list_spec.is_none());
        assert_eq!(imported.track_count, 1);
        let tracks = manager.get_playlist_tracks(imported.id).unwrap();
        assert_eq!(tracks.len(), 1);
        let song = tracks[0].song.as_ref().expect("station should be a Song");
        assert_eq!(song.source, crate::models::SongSource::Stream);
        assert!(song.path.as_deref().unwrap().starts_with("radio://custom-"));
        assert_eq!(song.stream_url.as_deref(), Some(stream_url));
        assert_eq!(song.title.as_deref(), Some("Indie Radio"));

        let export_m3u = temp_dir.path().join("exported.m3u");
        manager
            .export_playlist(imported.id, &export_m3u, true)
            .unwrap();
        let m3u_text = std::fs::read_to_string(&export_m3u).unwrap();
        assert!(m3u_text.contains(stream_url));
        assert!(!m3u_text.contains("radio://"));

        // Overwrite via export_playlist atomically and leave no .tmp file.
        manager
            .export_playlist(imported.id, &export_m3u, true)
            .unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }
}
