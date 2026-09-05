//! Collection module — library scanner, file watcher, and DB integration.

use crate::{
    covermanager::CoverManager,
    db::Database,
    models::{
        self, FileType, MusicDirectory, PruneResult, QueuePopulationMode, ScanPhase, ScanProgress,
        Song, SongSource,
    },
};
use anyhow::{Context, Result};
use lofty::{
    config::ParsingMode,
    file::TaggedFileExt,
    prelude::*,
    probe::Probe,
    tag::{items::Timestamp, Accessor, ItemKey, Tag},
};
use rayon::prelude::*;
use rusqlite::params;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::UNIX_EPOCH,
};
use tauri::{AppHandle, Emitter, Manager};
use walkdir::WalkDir;

/// Chunk size for batching per-file DB writes into transactions during a
/// library scan, instead of one implicit-autocommit write per file. Also
/// bounds how many decoded `Song`s are held in memory at once.
const SCAN_WRITE_BATCH_SIZE: usize = 300;

/// Thread count for the scan's parallel tag-reading pool. Leaves headroom
/// below the machine's full core count so a scan doesn't compete with the
/// audio playback thread or background analysis for CPU.
fn scan_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .saturating_sub(2)
        .max(1)
}

mod query;
mod reconcile;
mod watcher;
pub(crate) use reconcile::resolve_case_insensitive_path;
pub use watcher::{start_watcher, SelfWriteTracker, WatcherPauseGuard};

#[derive(Debug)]
pub struct CollectionScanner {
    db: Arc<Database>,
}

impl CollectionScanner {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Register `path` as a watched music directory. Silently a no-op (not
    /// an error) if it's already registered. Doesn't itself scan for
    /// songs — the caller (`commands::collection::add_directory`) separately
    /// (re)starts the file watcher, and the frontend triggers the initial
    /// scan.
    pub fn add_directory(&self, path: &str) -> Result<MusicDirectory> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "INSERT OR IGNORE INTO directories (path, subdirs) VALUES (?1, 1)",
            params![path],
        )?;
        let id = conn.last_insert_rowid();
        Ok(MusicDirectory {
            id,
            path: path.to_string(),
            subdirs: true,
            is_available: std::path::Path::new(path).exists(),
            nickname: None,
            icon: None,
            color: None,
        })
    }

    /// Remove a directory from the watched list (songs remain but are marked unavailable).
    pub fn remove_directory(&self, path: &str) -> Result<()> {
        let conn = self.db.pool.get()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM directories WHERE path = ?1", params![path])?;

        // Mark all songs under this directory as unavailable
        let mut to_mark = Vec::new();
        {
            let mut stmt = tx
                .prepare("SELECT id, path FROM songs WHERE source IN (1, 2) AND unavailable = 0")?;
            let rows = stmt.query_map([], |row| {
                let id: i64 = row.get(0)?;
                let p: String = row.get(1)?;
                Ok((id, p))
            })?;

            let target_dir = std::path::Path::new(path);
            for (id, p) in rows.flatten() {
                if std::path::Path::new(&p).starts_with(target_dir) {
                    to_mark.push(id);
                }
            }
        }

        if !to_mark.is_empty() {
            let mut upd_stmt = tx.prepare("UPDATE songs SET unavailable = 1 WHERE id = ?1")?;
            for id in to_mark {
                upd_stmt.execute(params![id])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// List all watched directories.
    pub fn get_directories(&self) -> Result<Vec<MusicDirectory>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare("SELECT id, path, subdirs, nickname, icon, color FROM directories ORDER BY path")?;
        let dirs = stmt
            .query_map([], |row| {
                let path: String = row.get(1)?;
                let is_available = std::path::Path::new(&path).exists();
                Ok(MusicDirectory {
                    id: row.get(0)?,
                    path,
                    subdirs: row.get(2)?,
                    is_available,
                    nickname: row.get(3)?,
                    icon: row.get(4)?,
                    color: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(dirs)
    }

    /// Update metadata (nickname, icon, color) for a watched directory.
    pub fn update_directory_metadata(
        &self,
        id: i64,
        nickname: Option<String>,
        icon: Option<String>,
        color: Option<String>,
    ) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE directories SET nickname = ?1, icon = ?2, color = ?3 WHERE id = ?4",
            params![nickname, icon, color, id],
        )?;
        Ok(())
    }
    /// Returns ids of songs (from `path`) whose file no longer exists on disk, excluding
    /// any song that lives under a watched directory root which is currently unreachable.
    ///
    /// A disconnected network share or a sleeping external drive makes every file under it
    /// look identically "gone" to a plain `Path::exists()` check, so without this guard a
    /// scan that happens to run while a watched drive is unmounted would read as "the user
    /// deleted their whole library" and hard-delete every song under it. Treating "can't
    /// currently verify" as distinct from "confirmed missing" keeps that from happening.
    fn find_missing_song_ids(
        &self,
        conn: &rusqlite::Connection,
        only_available: bool,
    ) -> Result<Vec<i64>> {
        let all_roots: Vec<PathBuf> = self
            .get_directories()?
            .into_iter()
            .map(|d| PathBuf::from(d.path))
            .collect();

        let unreachable_roots = reconcile::unreachable_watched_roots(conn);

        for root in &unreachable_roots {
            log::warn!(
                "Watched directory '{}' is unreachable (disconnected drive or network share?); \
                 songs under it will not be treated as missing this scan",
                root.display()
            );
        }

        let query = if only_available {
            "SELECT id, path, source FROM songs WHERE path IS NOT NULL AND unavailable = 0"
        } else {
            "SELECT id, path, source FROM songs WHERE path IS NOT NULL"
        };
        let mut stmt = conn.prepare(query)?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            let source: i32 = row.get(2)?;
            Ok((id, path, source))
        })?;

        let mut missing = Vec::new();
        for (id, path, source) in rows.flatten() {
            let p = Path::new(&path);

            // If the file is local (source 1 or 2) and not in any watched directory, it is orphaned.
            if source == 1 || source == 2 {
                let is_watched = all_roots.iter().any(|root| p.starts_with(root));
                if !is_watched {
                    missing.push(id);
                    continue;
                }
            }

            if p.exists() {
                continue;
            }
            if unreachable_roots.iter().any(|root| p.starts_with(root)) {
                continue;
            }
            missing.push(id);
        }
        Ok(missing)
    }

    /// Soft-deletes songs whose file no longer exists on disk by flagging them `unavailable`,
    /// without ever removing rows. This is the automatic counterpart run after every scan;
    /// only the explicit "Clean Up Missing Songs" action (`prune_missing_songs`) hard-deletes.
    /// Songs under a watched directory root that can't currently be verified (see
    /// `find_missing_song_ids`) are left untouched rather than flagged.
    pub fn mark_missing_unavailable(&self) -> Result<usize> {
        let conn = self.db.pool.get()?;
        let to_mark = self.find_missing_song_ids(&conn, true)?;

        let marked = to_mark.len();
        if !to_mark.is_empty() {
            let tx = conn.unchecked_transaction()?;
            {
                let mut upd_stmt = tx.prepare("UPDATE songs SET unavailable = 1 WHERE id = ?1")?;
                for id in &to_mark {
                    upd_stmt.execute(params![id])?;
                }
            }
            tx.commit()?;
            log::info!("Marked {marked} song(s) unavailable (file missing on disk)");
        }
        Ok(marked)
    }

    /// Merges `songs` rows that point to the same physical file (matched by
    /// case-insensitive path) into one, keeping the row with the highest id
    /// (the most recently upserted, reflecting current tags) and re-pointing
    /// playlist membership and play history from the others before deleting
    /// them. Rolls the discarded rows' rating/playcount/skipcount/lastplayed
    /// into the survivor rather than just discarding them.
    ///
    /// These duplicates can only arise from a case-only rename on a
    /// case-insensitive filesystem (Windows/macOS) slipping past
    /// `reconcile_moved_songs` before that was fixed to compare against the
    /// exact-cased paths a scan actually finds on disk — libraries scanned
    /// before that fix can still carry the extra rows, hence this cleanup.
    pub fn merge_duplicate_songs(&self) -> Result<usize> {
        let conn = self.db.pool.get()?;

        let mut stmt = conn.prepare("SELECT id, path FROM songs WHERE path IS NOT NULL")?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        let mut groups: std::collections::HashMap<String, Vec<i64>> =
            std::collections::HashMap::new();
        for (id, path) in rows {
            groups.entry(path.to_lowercase()).or_default().push(id);
        }

        let mut merged = 0usize;
        let tx = conn.unchecked_transaction()?;
        for mut ids in groups.into_values() {
            if ids.len() < 2 {
                continue;
            }
            ids.sort_unstable();
            let survivor = ids.pop().expect("len >= 2 checked above");
            for dup_id in ids {
                tx.execute(
                    "UPDATE playlist_items SET song_id = ?1 WHERE song_id = ?2",
                    params![survivor, dup_id],
                )?;
                tx.execute(
                    "UPDATE play_history SET song_id = ?1 WHERE song_id = ?2",
                    params![survivor, dup_id],
                )?;
                tx.execute(
                    "UPDATE songs SET
                        rating = MAX(rating, (SELECT rating FROM songs WHERE id = ?2)),
                        playcount = playcount + (SELECT playcount FROM songs WHERE id = ?2),
                        skipcount = skipcount + (SELECT skipcount FROM songs WHERE id = ?2),
                        lastplayed = MAX(IFNULL(lastplayed, 0), IFNULL((SELECT lastplayed FROM songs WHERE id = ?2), 0))
                     WHERE id = ?1",
                    params![survivor, dup_id],
                )?;
                tx.execute("DELETE FROM songs WHERE id = ?1", params![dup_id])?;
                merged += 1;
            }
        }
        tx.commit()?;

        if merged > 0 {
            log::info!("Merged {merged} duplicate song row(s) pointing to the same file");
        }
        Ok(merged)
    }

    /// Hard-deletes songs from the database when their file no longer exists on disk or are marked unavailable.
    /// Also sweeps every watched directory for empty folders and removes them.
    ///
    /// This is only ever invoked explicitly (the "Clean Up Missing Songs" action) — automatic
    /// scans call `mark_missing_unavailable` instead, which never deletes.
    pub fn prune_missing_songs(&self) -> Result<PruneResult> {
        let merged_duplicates = self.merge_duplicate_songs()?;

        let conn = self.db.pool.get()?;

        let mut to_delete = self.find_missing_song_ids(&conn, false)?;

        let mut stmt_unavail = conn.prepare("SELECT id FROM songs WHERE unavailable = 1")?;
        let unavail_rows = stmt_unavail.query_map([], |row| row.get::<_, i64>(0))?;
        for id in unavail_rows.flatten() {
            if !to_delete.contains(&id) {
                to_delete.push(id);
            }
        }

        let deleted_count = to_delete.len();
        if !to_delete.is_empty() {
            let tx = conn.unchecked_transaction()?;
            {
                let mut del_stmt = tx.prepare("DELETE FROM songs WHERE id = ?1")?;
                for id in &to_delete {
                    del_stmt.execute(params![id])?;
                }
                tx.execute_batch("DELETE FROM playlist_items WHERE song_id IS NULL;")?;
            }
            tx.commit()?;
            log::info!(
                "Hard-deleted {} song(s) from database (file missing or unavailable)",
                deleted_count
            );
        }

        let mut removed_folders = 0;
        for dir in self.get_directories()? {
            removed_folders += crate::organizer::remove_empty_dirs_under_root(Path::new(&dir.path));
        }

        Ok(PruneResult {
            deleted_songs: deleted_count,
            removed_folders,
            merged_duplicates,
        })
    }

    /// Scan all watched directories, emitting progress events to the frontend.
    /// If `force` is true, skips mtime checks and re-reads metadata for all files.
    /// `silent` marks this as a watcher-triggered catch-up scan rather than an
    /// explicit user action — see `ScanProgress::silent` (#233). Thin
    /// Tauri-facing wrapper around `scan_all_core` — see that method for the
    /// actual scan logic.
    pub async fn scan_all(&self, app: AppHandle, force: bool, silent: bool) -> Result<()> {
        let _watcher_pause_guard = app
            .try_state::<crate::AppState>()
            .map(|state| WatcherPauseGuard::new(Arc::clone(&state.watcher_paused)));

        let app_data_dir = app.path().app_data_dir().expect("no app data dir");
        let app_for_progress = app.clone();
        self.scan_all_core(app_data_dir, force, silent, true, move |progress| {
            let _ = app_for_progress.emit("scan-progress", progress);
        })
        .await
    }

    /// Core of `scan_all`, decoupled from Tauri's `AppHandle` (progress
    /// emission is a plain callback, and the covers-cache directory is
    /// passed in) so it's directly callable from tests without mocking a
    /// Tauri app — see `tests/library_scan_bdd.rs` and
    /// `tests/cover_art_bdd.rs`. `resolve_remote_art` gates Phase 3's iTunes
    /// network fallback; local embedded-art/folder-art resolution always
    /// runs regardless since neither touches the network. Tests pass
    /// `false` to keep scans deterministic and offline.
    pub async fn scan_all_core(
        &self,
        app_data_dir: PathBuf,
        force: bool,
        silent: bool,
        resolve_remote_art: bool,
        mut on_progress: impl FnMut(ScanProgress),
    ) -> Result<()> {
        let dirs = self.get_directories()?;
        if dirs.is_empty() {
            // Still tell the frontend we're done — otherwise the isScanning
            // flag it optimistically set before calling this command is
            // never cleared, permanently disabling rescan/cleanup controls.
            on_progress(ScanProgress {
                phase: ScanPhase::Done,
                scanned: 0,
                total: 0,
                current_path: None,
                silent,
            });
            return Ok(());
        }

        // Phase 1: discover all files
        on_progress(ScanProgress {
            phase: ScanPhase::Discovering,
            scanned: 0,
            total: 0,
            current_path: None,
            silent,
        });

        let mut all_paths: Vec<PathBuf> = Vec::new();
        for dir in &dirs {
            let walker = WalkDir::new(&dir.path)
                .follow_links(true)
                .into_iter()
                .filter_entry(|e| e.file_name() != "Duplicates");
            for entry in walker.filter_map(|e| e.ok()) {
                let path = entry.path().to_path_buf();
                if path.is_file() && is_audio_file(&path) {
                    all_paths.push(path);
                }
            }
        }

        let total = all_paths.len() as u64;
        log::info!("Scan found {total} audio files (force={force})");

        // Phase 2: read tags
        on_progress(ScanProgress {
            phase: ScanPhase::ReadingTags,
            scanned: 0,
            total,
            current_path: None,
            silent,
        });

        let mut scanned = 0u64;
        let cover_manager = CoverManager::new(Arc::clone(&self.db), app_data_dir);

        {
            let conn = self.db.pool.get()?;

            // Repoint DB rows for files that moved to a different watched folder
            // (e.g. a library split/reorganization) before doing anything else,
            // so the per-file loop below and mark_missing_unavailable() both see
            // the corrected path instead of treating the move as delete+new-insert.
            match reconcile::reconcile_moved_songs(&conn, &all_paths) {
                Ok(0) => {}
                Ok(n) => log::info!("Reconciled {n} moved song(s) to their new path"),
                Err(e) => log::warn!("Failed to reconcile moved songs during scan: {e}"),
            }

            // Load every known (path -> mtime) pair in one query instead of
            // issuing a SELECT per file — for large libraries this turns
            // O(files) round-trips into a single read.
            let mut known_mtimes: HashMap<String, i64> = HashMap::new();
            if !force {
                let mut stmt =
                    conn.prepare("SELECT path, mtime FROM songs WHERE unavailable = 0")?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?;
                for row in rows.filter_map(|r| r.ok()) {
                    known_mtimes.insert(row.0, row.1);
                }
            }

            // Partition into unchanged (skip tag re-read) vs. needs-update.
            let mut needs_update: Vec<&PathBuf> = Vec::new();
            for path in &all_paths {
                if !force {
                    let path_str = path.to_string_lossy().to_string();
                    let mtime = get_mtime(path).unwrap_or(0);
                    if known_mtimes.get(&path_str) == Some(&mtime) {
                        scanned += 1;
                        continue;
                    }
                }
                needs_update.push(path);
            }

            // Read tags + resolve local art in parallel across a bounded thread
            // pool — this is disk-I/O/CPU-bound per file and independent of the
            // DB, so it's the part worth parallelizing. We deliberately cap the
            // pool below the machine's full core count (rather than using
            // Rayon's global default of one thread per core) so a library scan
            // doesn't starve the audio playback thread or background loudness/
            // waveform analysis of CPU. DB writes below stay on the single
            // connection, serialized in batched transactions, which also caps
            // how many decoded `Song`s are held in memory at once.
            let scan_pool = rayon::ThreadPoolBuilder::new()
                .num_threads(scan_thread_count())
                .build()
                .context("failed to build scan thread pool")?;

            for chunk in needs_update.chunks(SCAN_WRITE_BATCH_SIZE) {
                let prepared: Vec<(PathBuf, Result<Song>)> = scan_pool.install(|| {
                    chunk
                        .par_iter()
                        .map(|path| ((*path).clone(), read_and_prepare_song(&cover_manager, path)))
                        .collect()
                });

                let tx = conn.unchecked_transaction()?;
                for (path, result) in prepared {
                    match result {
                        Ok(song) => {
                            if let Err(e) = upsert_song(&tx, &song) {
                                log::warn!("Failed to save tags for {}: {e}", path.display());
                            }
                        }
                        Err(e) => log::warn!("Failed to read tags for {}: {e}", path.display()),
                    }

                    scanned += 1;

                    // Emit progress every 50 files to avoid flooding
                    if scanned.is_multiple_of(50) || scanned == total {
                        on_progress(ScanProgress {
                            phase: ScanPhase::ReadingTags,
                            scanned,
                            total,
                            current_path: Some(path.to_string_lossy().to_string()),
                            silent,
                        });
                    }
                }
                tx.commit()?;
            }
        }

        // Mark songs from these directories that no longer exist as unavailable.
        // This is a soft-delete only: automatic scans never hard-delete, so a watched
        // directory that's merely unreachable (asleep drive, disconnected network share)
        // at scan time can't cause data loss. Hard-deleting is reserved for the explicit
        // "Clean Up Missing Songs" action (`prune_missing_songs`).
        if let Err(e) = self.mark_missing_unavailable() {
            log::error!("Failed to mark missing songs during scan: {e}");
        }

        // Phase 3: Resolve missing album artwork (local & remote) and backfill visualizers
        log::info!("Starting artwork resolution for missing albums...");
        let mut albums_to_resolve = Vec::new();
        if let Ok(conn) = self.db.pool.get() {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT
                    id,
                    path,
                    COALESCE(NULLIF(album_artist, ''), artist) AS effective_artist,
                    album,
                    art_embedded
                 FROM songs
                 WHERE source IN (1, 2)
                   AND album IS NOT NULL
                   AND (art_unset = 1 OR (art_automatic IS NULL AND art_manual IS NULL))
                 GROUP BY effective_artist, album",
            ) {
                if let Ok(mut rows) = stmt.query([]) {
                    while let Ok(Some(row)) = rows.next() {
                        if let (
                            Ok(id),
                            Ok(path_str),
                            Ok(effective_artist),
                            Ok(album),
                            Ok(art_embedded),
                        ) = (
                            row.get::<_, i64>(0),
                            row.get::<_, String>(1),
                            row.get::<_, String>(2),
                            row.get::<_, String>(3),
                            row.get::<_, bool>(4),
                        ) {
                            albums_to_resolve.push((
                                id,
                                path_str,
                                effective_artist,
                                album,
                                art_embedded,
                            ));
                        }
                    }
                }
            }
        }

        let total_updating_items = albums_to_resolve.len() as u64;

        if total_updating_items == 0 {
            on_progress(ScanProgress {
                phase: ScanPhase::Updating,
                scanned: total,
                total,
                current_path: None,
                silent,
            });
        }

        let mut remote_fetch_count = 0;
        for (idx, (song_id, path_str, effective_artist, album, art_embedded)) in
            albums_to_resolve.into_iter().enumerate()
        {
            let updating_scanned = idx as u64 + 1;
            let display_desc = if !effective_artist.trim().is_empty() && !album.trim().is_empty() {
                format!("Cover art: {effective_artist} - {album}")
            } else if !album.trim().is_empty() {
                format!("Cover art: {album}")
            } else {
                let file_name = Path::new(&path_str)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| path_str.clone());
                format!("Cover art: {file_name}")
            };

            on_progress(ScanProgress {
                phase: ScanPhase::Updating,
                scanned: updating_scanned,
                total: total_updating_items,
                current_path: Some(display_desc),
                silent,
            });

            let path = Path::new(&path_str);
            let mut resolved = false;

            // 1. Try embedded art
            if art_embedded {
                if let Ok(Some(cached_filename)) =
                    cover_manager.extract_embedded_art(path, &effective_artist, &album)
                {
                    if let Ok(conn) = self.db.pool.get() {
                        let _ = conn.execute(
                            "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE COALESCE(NULLIF(album_artist, ''), artist) = ?2 AND album = ?3",
                            params![cached_filename, effective_artist, album],
                        );
                    }
                    resolved = true;
                }
            }

            // 2. Try folder art
            if !resolved {
                if let Some(folder_art_path) = cover_manager.scan_folder_art(path) {
                    let folder_art_str = folder_art_path.to_string_lossy().to_string();
                    if let Ok(conn) = self.db.pool.get() {
                        let _ = conn.execute(
                            "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE COALESCE(NULLIF(album_artist, ''), artist) = ?2 AND album = ?3",
                            params![folder_art_str, effective_artist, album],
                        );
                    }
                    resolved = true;
                }
            }

            // 3. Try remote fetch (limit to 50 to avoid long scans / rate limits)
            if !resolved && resolve_remote_art && remote_fetch_count < 50 {
                remote_fetch_count += 1;
                std::thread::sleep(std::time::Duration::from_millis(150));

                if let Ok(Some(filename)) = cover_manager.fetch_remote_cover(song_id).await {
                    if let Ok(conn) = self.db.pool.get() {
                        let _ = conn.execute(
                            "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE COALESCE(NULLIF(album_artist, ''), artist) = ?2 AND album = ?3",
                            params![filename, effective_artist, album],
                        );
                    }
                }
            }
        }

        // Done
        on_progress(ScanProgress {
            phase: ScanPhase::Done,
            scanned: total,
            total,
            current_path: None,
            silent,
        });

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Audio file detection
// ---------------------------------------------------------------------------

pub(crate) const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "opus", "m4a", "aac", "alac", "wav", "aiff", "aif", "wv", "mpc", "ape",
    "tta", "dsf", "dff", "asf", "wma", "m4b",
];

pub(crate) fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub(crate) fn get_mtime(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

fn detect_filetype(path: &Path) -> FileType {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp3") => FileType::Mp3,
        Some("flac") => FileType::Flac,
        Some("ogg") => FileType::OggVorbis,
        Some("opus") => FileType::OggOpus,
        Some("m4a") | Some("aac") => FileType::Aac,
        Some("alac") => FileType::Alac,
        Some("wav") => FileType::Wav,
        Some("aiff") | Some("aif") => FileType::Aiff,
        Some("wv") => FileType::WavPack,
        Some("mpc") => FileType::Mpc,
        Some("ape") => FileType::Ape,
        Some("tta") => FileType::TrueAudio,
        Some("dsf") => FileType::Dsf,
        Some("dff") => FileType::Dsdiff,
        Some("asf") | Some("wma") => FileType::Asf,
        _ => FileType::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Tag reading via lofty
// ---------------------------------------------------------------------------

/// Reads all values under `key` from `tag`, joined into the `; `-delimited
/// storage format shared by `songs.genre`/`artist`/`album_artist`/`composer`
/// (see `models::join_multi_value`). Uses `get_strings` rather than a
/// singular `Accessor` helper (e.g. `Accessor::artist()`), since lofty
/// already splits ID3v2.4's null-byte-separated frames (and native
/// Vorbis/APEv2/MP4 multi-value items) into separate items on read — the
/// `Accessor` helpers would instead collapse them into one `" / "`-joined
/// display string. Each resulting value is further run through
/// `models::parse_multi_value` in case a tool joined multiple values into
/// one string with `;` (e.g. Mp3tag/Winamp's convention) without a real
/// multi-value mechanism.
///
/// Deliberately does *not* also split on `/`, despite that being a
/// real legacy join convention too (TPE1's own "Lead performer(s)/
/// Soloist(s)" convention, Picard's default `id3v23_join_with = "/"` for
/// ID3v2.3) — auto-splitting on `/` silently corrupts any value that
/// legitimately contains one, most visibly band names like "AC/DC" (see
/// `models::parse_multi_value`'s doc comment). A file relying on that
/// convention reads back as one combined value instead of being auto-split;
/// the user can split it into separate chips themselves in the tag editor.
///
/// Genre gets one extra layer of legacy handling on top of this that
/// Artist/Album Artist/Composer don't need: lofty's own ID3v2 read path
/// special-cases `TCON` to translate ID3v1 numeric genre IDs (and the
/// `(4)Eurodisco`-style parenthesized convention) to their textual
/// equivalents before this function ever sees them — see lofty's
/// `GenresIter`. Artist/Composer have no such legacy numeric convention, so
/// `get_strings` returns their raw text with no extra decoding required.
fn read_multi_value(tag: &Tag, key: ItemKey) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    let values: Vec<String> = tag
        .get_strings(key)
        .flat_map(models::parse_multi_value)
        .filter(|v| seen.insert(v.to_lowercase()))
        .collect();

    if values.is_empty() {
        None
    } else {
        Some(models::join_multi_value(&values))
    }
}

pub(crate) fn read_tags(path: &Path) -> Result<Song> {
    let path_str = path.to_string_lossy().to_string();
    let mtime = get_mtime(path);
    let filesize = std::fs::metadata(path).ok().map(|m| m.len() as i64);
    let filetype = detect_filetype(path);

    let tagged_file = Probe::open(path)
        .context("lofty: cannot open file")?
        .read()
        .context("lofty: cannot read file")?;

    let properties = tagged_file.properties();

    let duration_ns = (properties.duration().as_secs_f64() * 1_000_000_000.0) as i64;
    let bitrate = properties.audio_bitrate().map(|b| b as i32);
    let samplerate = properties.sample_rate().map(|r| r as i32);
    let channels = properties.channels().map(|c| c as i32);
    let bitdepth = properties.bit_depth().map(|b| b as i32);

    // Lofty's public API reports only the average bitrate for VBR MP3s, with no way to
    // tell VBR and CBR apart. Sniff the Xing/Info/VBRI header ourselves so the UI can
    // label an average as an average instead of implying a constant rate.
    let is_vbr = if filetype == FileType::Mp3 {
        detect_mp3_vbr(path)
    } else {
        None
    };

    let mut song = Song {
        source: SongSource::LocalFile,
        filetype,
        path: Some(path_str),
        length_nanosec: Some(duration_ns),
        bitrate,
        samplerate,
        channels,
        bitdepth,
        filesize,
        mtime,
        is_vbr,
        ..Default::default()
    };

    // Prefer the primary tag (ID3v2, VorbisComment, etc.), but fall back to secondary tags
    // (ID3v1, APE, etc.) if the primary tag is missing or lacks specific fields.
    let mut candidate_tags: Vec<&Tag> = Vec::new();
    if let Some(primary) = tagged_file.primary_tag() {
        candidate_tags.push(primary);
    }
    for t in tagged_file.tags() {
        if !candidate_tags
            .iter()
            .any(|existing| std::ptr::eq(*existing, t))
        {
            candidate_tags.push(t);
        }
    }

    for tag in candidate_tags.iter().copied() {
        if song.title.is_none() {
            song.title = tag.title().map(|t| t.to_string());
        }
        if song.artist.is_none() {
            song.artist = read_multi_value(tag, ItemKey::TrackArtist);
        }
        if song.album.is_none() {
            song.album = tag.album().map(|a| a.to_string());
        }
        if song.genre.is_none() {
            song.genre = read_multi_value(tag, ItemKey::Genre);
        }
        if song.comment.is_none() {
            song.comment = tag.comment().map(|c| c.to_string());
        }
        // `date()` checks ItemKey::RecordingDate (ID3v2 TDRC, MP4, Vorbis DATE)
        // before falling back to ItemKey::Year (ID3v1 only) — using Year alone
        // silently drops the year for every ID3v2-tagged file (#428).
        if song.year.is_none() {
            song.year = tag.date().map(|d| d.year as i32).or_else(|| {
                tag.get_string(ItemKey::Year)
                    .and_then(|s| s.trim().parse::<i32>().ok())
            });
        }
        // Original release date (ID3v2 TDOR, MP4/Vorbis equivalents) — used by
        // decade auto-playlists as a fallback when `year` (the pressing's own
        // date, e.g. a reissue) isn't set. Parsed the same relaxed way `date()`
        // parses ItemKey::RecordingDate, since lofty has no Accessor helper for it.
        if song.originalyear.is_none() {
            song.originalyear = tag
                .get_string(ItemKey::OriginalReleaseDate)
                .and_then(|s| {
                    Timestamp::parse(&mut s.as_bytes(), ParsingMode::Relaxed)
                        .ok()
                        .flatten()
                })
                .map(|d| d.year as i32);
        }
        if song.track.is_none() {
            song.track = tag.track().map(|t| t as i32);
        }
        if song.disc.is_none() {
            song.disc = tag.disk().map(|d| d as i32);
        }

        // Album artist (various tag formats store this differently)
        if song.album_artist.is_none() {
            song.album_artist = read_multi_value(tag, ItemKey::AlbumArtist);
        }

        // TCMP/cpil/COMPILATION "part of a compilation" flag — stored as text
        // "1"/"0" across every format lofty supports (ID3v2, MP4, Vorbis/APE).
        if !song.compilation {
            song.compilation = tag
                .get_string(ItemKey::FlagCompilation)
                .map(|s| s.trim() == "1")
                .unwrap_or(false);
        }

        if song.composer.is_none() {
            song.composer = read_multi_value(tag, ItemKey::Composer);
        }

        if song.lyrics.is_none() {
            song.lyrics = tag.get_string(ItemKey::Lyrics).map(|s| s.to_string());
        }

        if song.grouping.is_none() {
            song.grouping = tag.get_string(ItemKey::ContentGroup).map(|s| s.to_string());
        }
        if song.initial_key.is_none() {
            song.initial_key = tag.get_string(ItemKey::InitialKey).map(|s| s.to_string());
        }
        // ID3v2 (TBPM) and MP4 (tmpo) store BPM as an integer field; Vorbis/APE
        // store it as freeform text — check both generic keys to cover either.
        // Some taggers write "0" as an "unknown tempo" sentinel rather than
        // omitting the tag — treat that the same as absent, since 0 is never a
        // real tempo.
        if song.bpm.is_none() {
            song.bpm = tag
                .get_string(ItemKey::IntegerBpm)
                .or_else(|| tag.get_string(ItemKey::Bpm))
                .and_then(|s| s.trim().parse::<f32>().ok())
                .filter(|&b| b > 0.0);
        }

        // ReplayGain 2.0 tags (#77) — fallback gain until R128 analysis runs.
        if song.replaygain_track_gain.is_none() {
            song.replaygain_track_gain = tag
                .get_string(ItemKey::ReplayGainTrackGain)
                .and_then(parse_replaygain_db);
        }
        if song.replaygain_album_gain.is_none() {
            song.replaygain_album_gain = tag
                .get_string(ItemKey::ReplayGainAlbumGain)
                .and_then(parse_replaygain_db);
        }

        // Custom sort order tags (#151)
        if song.titlesort.is_none() {
            song.titlesort = tag
                .get_string(ItemKey::TrackTitleSortOrder)
                .map(|s| s.to_string());
        }
        if song.artistsort.is_none() {
            song.artistsort = tag
                .get_string(ItemKey::TrackArtistSortOrder)
                .map(|s| s.to_string());
        }
        if song.albumsort.is_none() {
            song.albumsort = tag
                .get_string(ItemKey::AlbumTitleSortOrder)
                .map(|s| s.to_string());
        }
        if song.album_artist_sort.is_none() {
            song.album_artist_sort = tag
                .get_string(ItemKey::AlbumArtistSortOrder)
                .map(|s| s.to_string());
        }
        if song.composersort.is_none() {
            song.composersort = tag
                .get_string(ItemKey::ComposerSortOrder)
                .map(|s| s.to_string());
        }

        // MusicBrainz identifiers (#752). Only the keys lofty maps to a typed
        // ItemKey are read here — `musicbrainz_disc_id`,
        // `musicbrainz_original_artist_id`, and `musicbrainz_original_album_id`
        // have no such mapping in lofty 0.25 (Picard writes them as raw TXXX
        // frames) and are left unpopulated rather than hand-parsing per-format
        // frames for them.
        //
        // Note for future tag-*writing* support: lofty 0.25's ID3v2 write
        // path silently drops `MusicBrainzReleaseId`/`ReleaseGroupId`/
        // `TrackId` when converting a generic `Tag` back to frames (they're
        // missing from its TXXX write allowlist), even though it reads them
        // back from an existing frame just fine — see the round-trip test
        // below for details. Read-only use here is unaffected.
        if song.musicbrainz_artist_id.is_none() {
            song.musicbrainz_artist_id = tag
                .get_string(ItemKey::MusicBrainzArtistId)
                .map(|s| s.to_string());
        }
        if song.musicbrainz_album_artist_id.is_none() {
            song.musicbrainz_album_artist_id = tag
                .get_string(ItemKey::MusicBrainzReleaseArtistId)
                .map(|s| s.to_string());
        }
        if song.musicbrainz_album_id.is_none() {
            song.musicbrainz_album_id = tag
                .get_string(ItemKey::MusicBrainzReleaseId)
                .map(|s| s.to_string());
        }
        if song.musicbrainz_release_group_id.is_none() {
            song.musicbrainz_release_group_id = tag
                .get_string(ItemKey::MusicBrainzReleaseGroupId)
                .map(|s| s.to_string());
        }
        if song.musicbrainz_recording_id.is_none() {
            song.musicbrainz_recording_id = tag
                .get_string(ItemKey::MusicBrainzRecordingId)
                .map(|s| s.to_string());
        }
        if song.musicbrainz_track_id.is_none() {
            song.musicbrainz_track_id = tag
                .get_string(ItemKey::MusicBrainzTrackId)
                .map(|s| s.to_string());
        }
        if song.musicbrainz_work_id.is_none() {
            song.musicbrainz_work_id = tag
                .get_string(ItemKey::MusicBrainzWorkId)
                .map(|s| s.to_string());
        }

        // AcoustID fingerprint match (written by Luminous's own AcoustID lookup
        // flow in tageditor.rs, or by another tagger) — same dead-columns bug
        // as the MusicBrainz IDs above (#752).
        if song.acoustid_id.is_none() {
            song.acoustid_id = tag.get_string(ItemKey::AcoustId).map(|s| s.to_string());
        }
        if song.acoustid_fingerprint.is_none() {
            song.acoustid_fingerprint = tag
                .get_string(ItemKey::AcoustIdFingerprint)
                .map(|s| s.to_string());
        }

        // Release metadata Picard writes alongside the MusicBrainz IDs, but
        // not IDs themselves (#752).
        if song.musicbrainz_release_type.is_none() {
            song.musicbrainz_release_type = tag
                .get_string(ItemKey::MusicBrainzReleaseType)
                .map(|s| s.to_string());
        }
        if song.musicbrainz_release_country.is_none() {
            song.musicbrainz_release_country = tag
                .get_string(ItemKey::ReleaseCountry)
                .map(|s| s.to_string());
        }
        if song.barcode.is_none() {
            song.barcode = tag.get_string(ItemKey::Barcode).map(|s| s.to_string());
        }
        if song.catalog_number.is_none() {
            song.catalog_number = tag
                .get_string(ItemKey::CatalogNumber)
                .map(|s| s.to_string());
        }
    }

    song.art_embedded = candidate_tags.iter().any(|t| !t.pictures().is_empty());

    Ok(song)
}

/// Reads tags for `path`, resolves embedded/folder cover art via `cover_manager`,
/// and upserts the resulting row. `read_tags()` alone never populates
/// `art_automatic` (only the `art_embedded` flag) — that cache lookup/write is
/// a separate step, so any caller that skips it and upserts read_tags()'s
/// output directly ends up nulling out an already-cached art_automatic via
/// upsert_song()'s unconditional overwrite. Shared by the initial scan and
/// the realtime file watcher so both keep it populated correctly.
/// Read tags + resolve local art for a single file, without touching the DB.
/// Split out from [`read_and_upsert_song`] so the (disk-I/O-heavy, CPU-light)
/// tag-reading work can be run in parallel across a thread pool while DB
/// writes stay serialized on one connection (see `scan_all` phase 2).
pub(crate) fn read_and_prepare_song(cover_manager: &CoverManager, path: &Path) -> Result<Song> {
    let mut song = read_tags(path)?;

    if song.art_embedded {
        let artist = song
            .album_artist
            .as_deref()
            .unwrap_or(song.artist.as_deref().unwrap_or(""));
        // A loose single (no album tag) has no release name to key the cache
        // on — falling back to "" would hash every such single by the same
        // artist to the identical cache filename, so the last one scanned
        // silently overwrites the rest's cached art (#106). The title is the
        // closest thing a single has to its own "release name".
        let album = song
            .album
            .as_deref()
            .filter(|a| !a.trim().is_empty())
            .or(song.title.as_deref())
            .unwrap_or("");
        if let Ok(Some(cached_filename)) = cover_manager.extract_embedded_art(path, artist, album) {
            song.art_automatic = Some(cached_filename);
            song.art_unset = false;
        }
    }

    if song.art_automatic.is_none() {
        if let Some(folder_art_path) = cover_manager.scan_folder_art(path) {
            song.art_automatic = Some(folder_art_path.to_string_lossy().to_string());
            song.art_unset = false;
        }
    }

    Ok(song)
}

pub(crate) fn read_and_upsert_song(
    conn: &rusqlite::Connection,
    cover_manager: &CoverManager,
    path: &Path,
) -> Result<()> {
    let song = read_and_prepare_song(cover_manager, path)?;
    upsert_song(conn, &song)
}

/// Detect whether an MP3 file is VBR-encoded by looking for a Xing/Info/VBRI header
/// in the first audio frame. "Xing" and "VBRI" mark true VBR; "Info" is the Xing
/// encoder's own marker for CBR (see http://gabriel.mp3-tech.org/mp3infotag.html).
/// A plain CBR file has no such header at all, so `Some(false)` covers both cases.
/// Returns `None` only if the file couldn't be read.
fn detect_mp3_vbr(path: &Path) -> Option<bool> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; 8 * 1024];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);

    // Skip a leading ID3v2 tag so its (rare) text content can't be mistaken for a
    // VBR marker — the real header lives in the first MPEG frame, right after it.
    let mut offset = 0;
    if buf.len() >= 10 && &buf[0..3] == b"ID3" {
        let size = ((buf[6] as u32 & 0x7f) << 21)
            | ((buf[7] as u32 & 0x7f) << 14)
            | ((buf[8] as u32 & 0x7f) << 7)
            | (buf[9] as u32 & 0x7f);
        offset = (10 + size as usize).min(buf.len());
    }

    let audio = &buf[offset..];
    let has_marker = |needle: &[u8]| audio.windows(needle.len()).any(|w| w == needle);

    if has_marker(b"Xing") || has_marker(b"VBRI") {
        Some(true)
    } else {
        Some(false)
    }
}

/// Parse a ReplayGain gain tag value (e.g. "-6.2 dB", "-6.2") into a f64.
fn parse_replaygain_db(s: &str) -> Option<f64> {
    s.trim()
        .trim_end_matches(|c: char| c.is_alphabetic() || c.is_whitespace())
        .trim()
        .parse::<f64>()
        .ok()
}

// ---------------------------------------------------------------------------
// Database upsert
// ---------------------------------------------------------------------------

pub(crate) fn upsert_song(conn: &rusqlite::Connection, song: &Song) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO songs ({}) VALUES ({})
                  ON CONFLICT(path) DO UPDATE SET
                    title=excluded.title, titlesort=excluded.titlesort,
                    artist=excluded.artist, artistsort=excluded.artistsort,
                    album=excluded.album, albumsort=excluded.albumsort,
                    album_artist=excluded.album_artist, album_artist_sort=excluded.album_artist_sort,
                    composer=excluded.composer, composersort=excluded.composersort,
                    lyrics=excluded.lyrics, comment=excluded.comment,
                    track=excluded.track, disc=excluded.disc,
                    year=excluded.year, originalyear=excluded.originalyear,
                    genre=excluded.genre, genresort=excluded.genresort,
                    compilation=excluded.compilation,
                    grouping=excluded.grouping, bpm=excluded.bpm, initial_key=excluded.initial_key,
                    length_nanosec=excluded.length_nanosec,
                    bitrate=excluded.bitrate, samplerate=excluded.samplerate,
                    channels=excluded.channels, bitdepth=excluded.bitdepth,
                    filesize=excluded.filesize, mtime=excluded.mtime,
                    art_embedded=excluded.art_embedded,
                    art_automatic=excluded.art_automatic,
                    art_unset=excluded.art_unset,
                    filetype=excluded.filetype, source=excluded.source,
                    replaygain_track_gain=excluded.replaygain_track_gain,
                    replaygain_album_gain=excluded.replaygain_album_gain,
                    is_vbr=excluded.is_vbr,
                    musicbrainz_artist_id=excluded.musicbrainz_artist_id,
                    musicbrainz_album_artist_id=excluded.musicbrainz_album_artist_id,
                    musicbrainz_album_id=excluded.musicbrainz_album_id,
                    musicbrainz_release_group_id=excluded.musicbrainz_release_group_id,
                    musicbrainz_recording_id=excluded.musicbrainz_recording_id,
                    musicbrainz_track_id=excluded.musicbrainz_track_id,
                    musicbrainz_work_id=excluded.musicbrainz_work_id,
                    acoustid_id=excluded.acoustid_id,
                    acoustid_fingerprint=excluded.acoustid_fingerprint,
                    musicbrainz_release_type=excluded.musicbrainz_release_type,
                    musicbrainz_release_country=excluded.musicbrainz_release_country,
                    barcode=excluded.barcode,
                    catalog_number=excluded.catalog_number,
                    unavailable=0",
            SONG_INSERT_COLS, SONG_INSERT_PLACEHOLDERS
        ),
        params![
            song.source as i32,
            song.filetype as i32,
            song.path,
            song.title,
            song.titlesort,
            song.artist,
            song.artistsort,
            song.album,
            song.albumsort,
            song.album_artist,
            song.album_artist_sort,
            song.composer,
            song.composersort,
            song.lyrics,
            song.comment,
            song.track,
            song.disc,
            song.year,
            song.originalyear,
            song.genre,
            song.genresort,
            song.compilation,
            song.grouping,
            song.bpm,
            song.initial_key,
            song.length_nanosec,
            song.bitrate,
            song.samplerate,
            song.channels,
            song.bitdepth,
            song.filesize,
            song.mtime,
            song.art_embedded,
            song.art_automatic,
            song.art_unset,
            song.replaygain_track_gain,
            song.replaygain_album_gain,
            song.is_vbr,
            song.musicbrainz_artist_id,
            song.musicbrainz_album_artist_id,
            song.musicbrainz_album_id,
            song.musicbrainz_release_group_id,
            song.musicbrainz_recording_id,
            song.musicbrainz_track_id,
            song.musicbrainz_work_id,
            song.acoustid_id,
            song.acoustid_fingerprint,
            song.musicbrainz_release_type,
            song.musicbrainz_release_country,
            song.barcode,
            song.catalog_number,
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Queue population mode (#120) — weighted-random selection fragments
// ---------------------------------------------------------------------------

/// Extra `WHERE`-clause fragment and `ORDER BY` expression implementing a
/// `QueuePopulationMode`'s bias. Spliced onto a base scope filter (genre,
/// decade, or a Smart Playlist's own filter query) by `TagManager`'s
/// curated-tag matchers, `get_songs_by_decade`, and `search_songs_by_mode`.
/// Binds no parameters of its own, so it's safe to splice into either
/// positional (`?N`) SQL.
///
/// Ordering uses `RANDOM()` (uniform shuffle) for unweighted modes, and an
/// `RANDOM() * weight` key for `Familiar`/`Discover` — a lightweight, easy-
/// to-audit approximation of weighted random sampling: good enough to bias
/// selection without returning the same deterministic block every time.
pub(crate) fn mode_query_fragments(mode: QueuePopulationMode) -> (&'static str, &'static str) {
    match mode {
        QueuePopulationMode::All => ("", "RANDOM()"),
        QueuePopulationMode::Favourites => (" AND rating >= 4", "RANDOM()"),
        QueuePopulationMode::DeepCuts => (" AND (playcount = 0 OR lastplayed IS NULL)", "RANDOM()"),
        QueuePopulationMode::Familiar => (
            " AND playcount > 0",
            "((ABS(RANDOM()) % 1000000) / 1000000.0) * (playcount + 1) DESC",
        ),
        QueuePopulationMode::Discover => (
            " AND playcount > 0",
            "((ABS(RANDOM()) % 1000000) / 1000000.0) * (1.0 / playcount) DESC",
        ),
    }
}

// ---------------------------------------------------------------------------
// SQL column helpers
// ---------------------------------------------------------------------------

pub(crate) const SONG_SELECT_COLS: &str = "
    id, source, filetype, path, url, stream_url,
    title, titlesort, artist, artistsort,
    album, albumsort, album_artist, album_artist_sort,
    composer, composersort, performer, performersort,
    grouping, comment, lyrics,
    track, disc, year, originalyear, genre, genresort, compilation,
    bpm, initial_key,
    length_nanosec, beginning_nanosec, end_nanosec,
    bitrate, samplerate, bitdepth, channels, filesize, mtime,
    rating, playcount, skipcount, lastplayed, lastseen,
    art_embedded, art_automatic, art_manual, art_unset,
    cue_path,
    ebur128_integrated_loudness_lufs, ebur128_loudness_range_lu,
    unavailable,
    replaygain_track_gain, replaygain_album_gain,
    is_vbr, is_instrumental, not_included, added,
    musicbrainz_artist_id, musicbrainz_album_artist_id, musicbrainz_album_id,
    musicbrainz_release_group_id, musicbrainz_recording_id, musicbrainz_track_id,
    musicbrainz_work_id, acoustid_id, acoustid_fingerprint,
    musicbrainz_release_type, musicbrainz_release_country, barcode, catalog_number
";

/// Same columns as `SONG_SELECT_COLS`, in the same order, qualified with the
/// `s` alias for use in joined queries. Keep in sync with `SONG_SELECT_COLS`
/// and `row_to_song_at`'s field order — `cargo test` in `collection.rs`
/// exercises every column through `row_to_song`, so a mismatch fails loudly
/// there instead of silently defaulting a field at a call site.
pub(crate) const SONG_SELECT_COLS_QUALIFIED: &str =
    "s.id, s.source, s.filetype, s.path, s.url, s.stream_url,
    s.title, s.titlesort, s.artist, s.artistsort,
    s.album, s.albumsort, s.album_artist, s.album_artist_sort,
    s.composer, s.composersort, s.performer, s.performersort,
    s.grouping, s.comment, s.lyrics,
    s.track, s.disc, s.year, s.originalyear, s.genre, s.genresort, s.compilation,
    s.bpm, s.initial_key,
    s.length_nanosec, s.beginning_nanosec, s.end_nanosec,
    s.bitrate, s.samplerate, s.bitdepth, s.channels, s.filesize, s.mtime,
    s.rating, s.playcount, s.skipcount, s.lastplayed, s.lastseen,
    s.art_embedded, s.art_automatic, s.art_manual, s.art_unset,
    s.cue_path,
    s.ebur128_integrated_loudness_lufs, s.ebur128_loudness_range_lu,
    s.unavailable, s.replaygain_track_gain, s.replaygain_album_gain,
    s.is_vbr, s.is_instrumental, s.not_included, s.added,
    s.musicbrainz_artist_id, s.musicbrainz_album_artist_id, s.musicbrainz_album_id,
    s.musicbrainz_release_group_id, s.musicbrainz_recording_id, s.musicbrainz_track_id,
    s.musicbrainz_work_id, s.acoustid_id, s.acoustid_fingerprint,
    s.musicbrainz_release_type, s.musicbrainz_release_country, s.barcode, s.catalog_number";

pub(crate) const SONG_SELECT_COL_COUNT: usize = 71;

const SONG_INSERT_COLS: &str = "
    source, filetype, path, title, titlesort, artist, artistsort, album, albumsort, album_artist, album_artist_sort,
    composer, composersort, lyrics, comment, track, disc, year, originalyear, genre, genresort, compilation,
    grouping, bpm, initial_key,
    length_nanosec, bitrate, samplerate, channels, bitdepth,
    filesize, mtime, art_embedded, art_automatic, art_unset,
    replaygain_track_gain, replaygain_album_gain, is_vbr,
    musicbrainz_artist_id, musicbrainz_album_artist_id, musicbrainz_album_id,
    musicbrainz_release_group_id, musicbrainz_recording_id, musicbrainz_track_id,
    musicbrainz_work_id, acoustid_id, acoustid_fingerprint,
    musicbrainz_release_type, musicbrainz_release_country, barcode, catalog_number
";

const SONG_INSERT_PLACEHOLDERS: &str =
    "?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36,?37,?38,?39,?40,?41,?42,?43,?44,?45,?46,?47,?48,?49,?50,?51";

pub(crate) fn row_to_song(row: &rusqlite::Row) -> rusqlite::Result<Song> {
    row_to_song_at(row, 0)
}

/// Maps the `SONG_SELECT_COLS`/`SONG_SELECT_COLS_QUALIFIED` block of a row to
/// a `Song`, starting at `offset` — for queries that select other columns
/// (e.g. a join table's own fields) before the song columns. This is the
/// single place that knows the column order those two constants promise;
/// callers should never hand-roll their own `row.get(n)` mapping, since that
/// duplication is exactly what lets a new/reordered song column silently
/// default at one call site while every other stays correct (see the queue
/// "Date Added" regression this replaced).
pub(crate) fn row_to_song_at(row: &rusqlite::Row, offset: usize) -> rusqlite::Result<Song> {
    let col = |i: usize| offset + i;
    Ok(Song {
        id: row.get::<_, Option<i64>>(col(0))?.unwrap_or(0),
        source: row.get::<_, i64>(col(1)).map(SongSource::from)?,
        filetype: row.get::<_, i64>(col(2)).map(FileType::from)?,
        path: row.get(col(3))?,
        url: row.get(col(4))?,
        stream_url: row.get(col(5))?,
        title: row.get(col(6))?,
        titlesort: row.get(col(7))?,
        artist: row.get(col(8))?,
        artistsort: row.get(col(9))?,
        album: row.get(col(10))?,
        albumsort: row.get(col(11))?,
        album_artist: row.get(col(12))?,
        album_artist_sort: row.get(col(13))?,
        composer: row.get(col(14))?,
        composersort: row.get(col(15))?,
        performer: row.get(col(16))?,
        performersort: row.get(col(17))?,
        grouping: row.get(col(18))?,
        comment: row.get(col(19))?,
        lyrics: row.get(col(20))?,
        track: row.get(col(21))?,
        disc: row.get(col(22))?,
        year: row.get(col(23))?,
        originalyear: row.get(col(24))?,
        genre: row.get(col(25))?,
        genresort: row.get(col(26))?,
        compilation: row.get(col(27))?,
        bpm: row.get(col(28))?,
        initial_key: row.get(col(29))?,
        length_nanosec: row.get(col(30))?,
        beginning_nanosec: row.get::<_, Option<i64>>(col(31))?.unwrap_or(0),
        end_nanosec: row.get::<_, Option<i64>>(col(32))?.unwrap_or(0),
        bitrate: row.get(col(33))?,
        samplerate: row.get(col(34))?,
        bitdepth: row.get(col(35))?,
        channels: row.get(col(36))?,
        filesize: row.get(col(37))?,
        mtime: row.get(col(38))?,
        rating: row.get::<_, Option<f32>>(col(39))?.unwrap_or(-1.0),
        playcount: row.get::<_, Option<i32>>(col(40))?.unwrap_or(0),
        skipcount: row.get::<_, Option<i32>>(col(41))?.unwrap_or(0),
        lastplayed: row.get(col(42))?,
        lastseen: row.get(col(43))?,
        art_embedded: row.get(col(44))?,
        art_automatic: row.get(col(45))?,
        art_manual: row.get(col(46))?,
        art_unset: row.get(col(47))?,
        cue_path: row.get(col(48))?,
        ebur128_integrated_loudness_lufs: row.get(col(49))?,
        ebur128_loudness_range_lu: row.get(col(50))?,
        unavailable: row.get::<_, Option<bool>>(col(51))?.unwrap_or(false),
        replaygain_track_gain: row.get(col(52))?,
        replaygain_album_gain: row.get(col(53))?,
        is_vbr: row.get(col(54))?,
        is_instrumental: row.get::<_, Option<bool>>(col(55))?.unwrap_or(false),
        not_included: row.get::<_, Option<bool>>(col(56))?.unwrap_or(false),
        added: row.get(col(57))?,
        musicbrainz_artist_id: row.get(col(58))?,
        musicbrainz_album_artist_id: row.get(col(59))?,
        musicbrainz_album_id: row.get(col(60))?,
        musicbrainz_release_group_id: row.get(col(61))?,
        musicbrainz_recording_id: row.get(col(62))?,
        musicbrainz_track_id: row.get(col(63))?,
        musicbrainz_work_id: row.get(col(64))?,
        acoustid_id: row.get(col(65))?,
        acoustid_fingerprint: row.get(col(66))?,
        musicbrainz_release_type: row.get(col(67))?,
        musicbrainz_release_country: row.get(col(68))?,
        barcode: row.get(col(69))?,
        catalog_number: row.get(col(70))?,
        ..Default::default()
    })
}

/// Parse a decade label like `"1990s"`/`"1990"` into its inclusive year
/// bounds `(1990, 1999)` for a decade auto-playlist's year filter. The
/// input year is floored to its decade regardless of which year within the
/// decade was passed (e.g. `"1995"` also yields `(1990, 1999)`).
pub fn parse_decade_range(decade: &str) -> Option<(i32, i32)> {
    let clean = decade.trim().trim_end_matches(['s', 'S']);
    let year: i32 = clean.parse().ok()?;
    let start = (year / 10) * 10;
    let end = start + 9;
    Some((start, end))
}

/// Parses a `bpmrange:` dynamic-spec suffix (e.g. `"60-90"` or the
/// open-ended `"150-"`) into a `(min, max)` pair for
/// [`CollectionScanner::get_songs_by_bpm_range`].
pub fn parse_bpm_range_spec(spec: &str) -> Option<(f64, Option<f64>)> {
    let (min_str, max_str) = spec.split_once('-')?;
    let min: f64 = min_str.trim().parse().ok()?;
    let max = if max_str.trim().is_empty() {
        None
    } else {
        Some(max_str.trim().parse().ok()?)
    };
    Some((min, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FileType, Song, SongSource};
    use std::sync::Arc;

    #[test]
    fn test_read_multi_value_uses_multivalue_items_directly() {
        // Simulates a file lofty already split into multiple ItemKey items
        // on read (e.g. ID3v2.4's null-separated TCON/TPE1/TPE2/TCOM, or
        // Vorbis Comments' native repeated fields) — no further splitting
        // should occur.
        let mut tag = Tag::new(lofty::tag::TagType::VorbisComments);
        tag.push(lofty::tag::TagItem::new(
            ItemKey::Genre,
            lofty::tag::ItemValue::Text("Rock".to_string()),
        ));
        tag.push(lofty::tag::TagItem::new(
            ItemKey::Genre,
            lofty::tag::ItemValue::Text("Jazz Fusion".to_string()),
        ));

        assert_eq!(
            read_multi_value(&tag, ItemKey::Genre).as_deref(),
            Some("Rock; Jazz Fusion")
        );
    }

    #[test]
    fn test_read_multi_value_splits_legacy_semicolon_joined_string() {
        // Simulates a file tagged by Mp3tag/Winamp (`;`-joined) — lofty
        // hands back a single item containing the joined string, which
        // read_multi_value must still split into a proper multi-value list.
        let mut tag = Tag::new(lofty::tag::TagType::Id3v2);
        tag.push(lofty::tag::TagItem::new(
            ItemKey::Genre,
            lofty::tag::ItemValue::Text("Rock; Blues".to_string()),
        ));
        tag.push(lofty::tag::TagItem::new(
            ItemKey::TrackArtist,
            lofty::tag::ItemValue::Text("Artist A; Artist B".to_string()),
        ));

        assert_eq!(
            read_multi_value(&tag, ItemKey::Genre).as_deref(),
            Some("Rock; Blues")
        );
        assert_eq!(
            read_multi_value(&tag, ItemKey::TrackArtist).as_deref(),
            Some("Artist A; Artist B")
        );
    }

    #[test]
    fn test_read_multi_value_does_not_split_slash_joined_legacy_string() {
        // Regression test: TPE1's own long-standing "Lead performer(s)/
        // Soloist(s)" convention (and Picard's default `id3v23_join_with =
        // "/"` for ID3v2.3) is a real legacy multi-value join, but auto-
        // splitting on `/` collides with slash-containing values that are
        // one legitimate name, not a join — most visibly a real band name
        // like "AC/DC", which must read back as a single artist rather than
        // being torn into "AC" and "DC".
        let mut tag = Tag::new(lofty::tag::TagType::Id3v2);
        tag.push(lofty::tag::TagItem::new(
            ItemKey::TrackArtist,
            lofty::tag::ItemValue::Text("AC/DC".to_string()),
        ));
        tag.push(lofty::tag::TagItem::new(
            ItemKey::Genre,
            lofty::tag::ItemValue::Text("Hip-Hop/Rap".to_string()),
        ));

        assert_eq!(
            read_multi_value(&tag, ItemKey::TrackArtist).as_deref(),
            Some("AC/DC")
        );
        assert_eq!(
            read_multi_value(&tag, ItemKey::Genre).as_deref(),
            Some("Hip-Hop/Rap")
        );
    }

    #[test]
    fn test_read_multi_value_none_when_no_item() {
        let tag = Tag::new(lofty::tag::TagType::Id3v2);
        assert_eq!(read_multi_value(&tag, ItemKey::Genre), None);
        assert_eq!(read_multi_value(&tag, ItemKey::AlbumArtist), None);
        assert_eq!(read_multi_value(&tag, ItemKey::Composer), None);
    }

    /// Writes a minimal valid WAV file, so tests don't need a binary audio
    /// fixture checked into the repo. WAV's default primary tag is ID3v2
    /// (see lofty's `FileType::Wav => TagType::Id3v2` mapping), so this
    /// exercises a genuine TCON round-trip.
    fn write_test_wav(path: &Path) {
        let sample_rate = 8_000u32;
        let channels = 1u16;
        let bits_per_sample = 16u16;
        let data = vec![0u8; (sample_rate / 10) as usize * 2]; // 100ms of silence

        let byte_rate = sample_rate * channels as u32 * (bits_per_sample / 8) as u32;
        let block_align = channels * (bits_per_sample / 8);

        let mut wav = Vec::with_capacity(44 + data.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);

        std::fs::write(path, wav).expect("failed to write test wav fixture");
    }

    #[test]
    fn test_read_tags_end_to_end_multi_genre_round_trip() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        crate::tageditor::write_tags(
            &path,
            &crate::tageditor::TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                genre: "Rock; Jazz Fusion; Live",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let song = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(song.genre.as_deref(), Some("Rock; Jazz Fusion; Live"));
    }

    #[test]
    fn test_read_tags_end_to_end_multi_artist_album_artist_composer_round_trip() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        crate::tageditor::write_tags(
            &path,
            &crate::tageditor::TagWriteRequest {
                title: "Title",
                artist: "Artist A; Artist B",
                album: "Album",
                album_artist: "Album Artist A; Album Artist B",
                composer: "Composer A; Composer B",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let song = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(song.artist.as_deref(), Some("Artist A; Artist B"));
        assert_eq!(
            song.album_artist.as_deref(),
            Some("Album Artist A; Album Artist B")
        );
        assert_eq!(song.composer.as_deref(), Some("Composer A; Composer B"));
    }

    /// Covers `MusicBrainzArtistId`/`MusicBrainzReleaseArtistId` — the two
    /// MusicBrainz keys confirmed (both here and via `exiftool` against a
    /// real Picard-tagged file, Def Leppard's *Hysteria*, 1991) to round-trip
    /// cleanly through ID3v2. The other five `musicbrainz_*` `read_tags()`
    /// reads (`MusicBrainzReleaseId`/`ReleaseGroupId`/`RecordingId`/
    /// `TrackId`/`WorkId`) aren't exercised by a round-trip test here: lofty
    /// 0.25's ID3v2 *write* path doesn't reliably re-serialize a `Tag` that
    /// was only mutated in memory via `insert_text` for those keys (observed
    /// empirically — e.g. `MusicBrainzWorkId` and `MusicBrainzRecordingId`
    /// silently vanish on save despite being set), even though its read path
    /// parses an already-on-disk frame for any of them the same way it does
    /// for `MusicBrainzArtistId` (see `item::init_key_map`'s unconditional
    /// ID3v2 TXXX/UFID table). Since Luminous only *reads* these fields (no
    /// tag-editor support for writing them), that write-side gap doesn't
    /// affect the scanner — it's just not practically testable via lofty's
    /// public write API without hand-building raw ID3v2 frames (#752).
    #[test]
    fn test_read_tags_musicbrainz_ids_round_trip() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);
        crate::tageditor::write_tags(
            &path,
            &crate::tageditor::TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let mut tagged_file = Probe::open(&path)
            .expect("probe")
            .read()
            .expect("read tagged file");
        let tag = tagged_file.primary_tag_mut().expect("primary tag");
        tag.insert_text(ItemKey::MusicBrainzArtistId, "artist-uuid".to_string());
        tag.insert_text(
            ItemKey::MusicBrainzReleaseArtistId,
            "album-artist-uuid".to_string(),
        );
        tagged_file
            .save_to_path(&path, lofty::config::WriteOptions::default())
            .expect("save tagged file");

        let song = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(song.musicbrainz_artist_id.as_deref(), Some("artist-uuid"));
        assert_eq!(
            song.musicbrainz_album_artist_id.as_deref(),
            Some("album-artist-uuid")
        );
    }

    /// Unlike the three MusicBrainz release/release-group/track keys above,
    /// `AcoustId`/`AcoustIdFingerprint` *are* on lofty's ID3v2 TXXX write
    /// allowlist, so this exercises the full `write_tags()` -> `read_tags()`
    /// round trip directly rather than hand-poking the tag (#752).
    #[test]
    fn test_write_tags_then_read_tags_acoustid_round_trip() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        crate::tageditor::write_tags(
            &path,
            &crate::tageditor::TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                acoustid_id: Some("acoustid-uuid"),
                acoustid_fingerprint: Some("AQADtEmI"),
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let song = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(song.acoustid_id.as_deref(), Some("acoustid-uuid"));
        assert_eq!(song.acoustid_fingerprint.as_deref(), Some("AQADtEmI"));
    }

    /// `write_tags()` has no support for these (Luminous's tag editor doesn't
    /// expose them for editing), so — like the MusicBrainz IDs test above —
    /// this hand-inserts the tag items directly to simulate a file already
    /// tagged by Picard, and only exercises `read_tags()` (#752).
    #[test]
    fn test_read_tags_release_metadata_round_trip() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);
        crate::tageditor::write_tags(
            &path,
            &crate::tageditor::TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let mut tagged_file = Probe::open(&path)
            .expect("probe")
            .read()
            .expect("read tagged file");
        let tag = tagged_file.primary_tag_mut().expect("primary tag");
        tag.insert_text(ItemKey::MusicBrainzReleaseType, "album".to_string());
        tag.insert_text(ItemKey::ReleaseCountry, "JP".to_string());
        tag.insert_text(ItemKey::Barcode, "4988011329586".to_string());
        tag.insert_text(ItemKey::CatalogNumber, "PHCR-1144".to_string());
        tagged_file
            .save_to_path(&path, lofty::config::WriteOptions::default())
            .expect("save tagged file");

        let song = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(song.musicbrainz_release_type.as_deref(), Some("album"));
        assert_eq!(song.musicbrainz_release_country.as_deref(), Some("JP"));
        assert_eq!(song.barcode.as_deref(), Some("4988011329586"));
        assert_eq!(song.catalog_number.as_deref(), Some("PHCR-1144"));
    }

    #[test]
    fn test_read_tags_id3v2_year_round_trip_via_recording_date() {
        // Regression test for #433: the lofty 0.21->0.25 bump switched year
        // writes to ItemKey::Year directly, which has no ID3v2 mapping and
        // silently produced no frame at all for ID3v2-tagged files (nearly
        // every MP3/WAV). write_tags now goes through set_date(), which
        // writes ItemKey::RecordingDate (ID3v2 TDRC) — verify the full
        // write -> read round trip actually preserves the year instead of
        // silently dropping it, since a subsequent library rescan reading
        // back None would wipe the year in the database too.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        crate::tageditor::write_tags(
            &path,
            &crate::tageditor::TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                year: Some(1995),
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let song = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(
            song.year,
            Some(1995),
            "year written via write_tags must survive an ID3v2 write/read round trip"
        );
    }

    #[test]
    fn test_read_tags_id3v2_year_cleared_removes_recording_date() {
        // Companion to the round-trip test above: write_tags's None branch
        // must clear both ItemKey::RecordingDate (via remove_date()) and the
        // legacy ItemKey::Year key, or a previously-set year would survive
        // an edit that's supposed to clear it.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        crate::tageditor::write_tags(
            &path,
            &crate::tageditor::TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                year: Some(1995),
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");
        crate::tageditor::write_tags(
            &path,
            &crate::tageditor::TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                ..Default::default()
            },
        )
        .expect("second write_tags (clearing year) should succeed");

        let song = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(
            song.year, None,
            "clearing the year must remove it, not leave the old value"
        );
    }

    #[test]
    fn test_read_tags_originalyear_from_original_release_date() {
        // Regression test for #433's second fix: songs.originalyear is read
        // by decade auto-playlists as a fallback (COALESCE(year,
        // originalyear)), but read_tags() never assigned it before this fix
        // — it was permanently NULL for every scanned song regardless of
        // what the file actually had tagged. ID3v2's TDOR frame surfaces
        // through lofty as ItemKey::OriginalReleaseDate; write it directly
        // (write_tags has no originalyear param) and confirm read_tags
        // actually populates song.originalyear from it.
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        {
            let mut tagged_file = Probe::open(&path)
                .expect("probe open")
                .read()
                .expect("probe read");
            let tag_type = tagged_file.primary_tag_type();
            if tagged_file.primary_tag().is_none() {
                tagged_file.insert_tag(Tag::new(tag_type));
            }
            let tag = tagged_file.primary_tag_mut().expect("primary tag");
            tag.insert_text(ItemKey::OriginalReleaseDate, "1969".to_string());
            tagged_file
                .save_to_path(&path, lofty::config::WriteOptions::default())
                .expect("save");
        }

        let song = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(
            song.originalyear,
            Some(1969),
            "originalyear must be populated from ItemKey::OriginalReleaseDate"
        );
    }

    #[test]
    fn test_detect_mp3_vbr_finds_xing_header() {
        let path = write_temp_file("vbr_xing", &[b"\xFF\xFB\x90\x00", b"Xing", &[0u8; 100]]);
        assert_eq!(detect_mp3_vbr(&path), Some(true));
    }

    #[test]
    fn test_detect_mp3_vbr_finds_vbri_header() {
        let path = write_temp_file("vbr_vbri", &[b"\xFF\xFB\x90\x00", b"VBRI", &[0u8; 100]]);
        assert_eq!(detect_mp3_vbr(&path), Some(true));
    }

    #[test]
    fn test_detect_mp3_vbr_no_header_is_cbr() {
        let path = write_temp_file("cbr_plain", &[b"\xFF\xFB\x90\x00", &[0u8; 200]]);
        assert_eq!(detect_mp3_vbr(&path), Some(false));
    }

    #[test]
    fn test_detect_mp3_vbr_info_marker_is_cbr() {
        let path = write_temp_file("cbr_info", &[b"\xFF\xFB\x90\x00", b"Info", &[0u8; 100]]);
        assert_eq!(detect_mp3_vbr(&path), Some(false));
    }

    #[test]
    fn test_detect_mp3_vbr_skips_id3v2_tag_content() {
        // A synchsafe ID3v2 header claiming a 4-byte tag whose body is "Xing" —
        // detection must skip past it and not mistake tag text for the real header.
        let id3_header: &[u8] = &[b'I', b'D', b'3', 3, 0, 0, 0, 0, 0, 4];
        let path = write_temp_file(
            "cbr_id3_xing_text",
            &[id3_header, b"Xing", b"\xFF\xFB\x90\x00", &[0u8; 100]],
        );
        assert_eq!(detect_mp3_vbr(&path), Some(false));
    }

    fn write_temp_file(name: &str, chunks: &[&[u8]]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "luminous_vbr_test_{name}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut data = Vec::new();
        for chunk in chunks {
            data.extend_from_slice(chunk);
        }
        std::fs::write(&path, data).unwrap();
        path
    }

    #[test]
    fn test_read_tags_id3v1() {
        let path = std::env::temp_dir().join(format!(
            "luminous_id3v1_test_{}.mp3",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // Create 10 valid MPEG-1 Layer 3 frames (128kbps, 44.1kHz, stereo = 417 bytes each)
        let mut mp3_bytes = Vec::new();
        let frame_header = [0xFF, 0xFB, 0x90, 0x00];
        for _ in 0..10 {
            mp3_bytes.extend_from_slice(&frame_header);
            mp3_bytes.resize(mp3_bytes.len() + 413, 0);
        }

        // 128-byte ID3v1.1 tag
        let mut id3v1 = [0u8; 128];
        id3v1[0..3].copy_from_slice(b"TAG");
        // Title (30 bytes)
        let title = b"Bohemian Rhapsody";
        id3v1[3..3 + title.len()].copy_from_slice(title);
        // Artist (30 bytes)
        let artist = b"Queen";
        id3v1[33..33 + artist.len()].copy_from_slice(artist);
        // Album (30 bytes)
        let album = b"Greatest Hits";
        id3v1[63..63 + album.len()].copy_from_slice(album);
        // Year (4 bytes)
        id3v1[93..97].copy_from_slice(b"1981");
        // Comment (28 bytes)
        let comment = b"Rock Classic";
        id3v1[97..97 + comment.len()].copy_from_slice(comment);
        // Zero byte separator for ID3v1.1 track number
        id3v1[125] = 0;
        // Track number
        id3v1[126] = 1;
        // Genre index (17 = Rock)
        id3v1[127] = 17;

        mp3_bytes.extend_from_slice(&id3v1);
        std::fs::write(&path, &mp3_bytes).expect("failed to write test mp3 fixture");

        let song = read_tags(&path).expect("read_tags should succeed for ID3v1 MP3");
        assert_eq!(song.title.as_deref(), Some("Bohemian Rhapsody"));
        assert_eq!(song.artist.as_deref(), Some("Queen"));
        assert_eq!(song.album.as_deref(), Some("Greatest Hits"));
        assert_eq!(song.year, Some(1981));
        assert_eq!(song.comment.as_deref(), Some("Rock Classic"));
        assert_eq!(song.track, Some(1));
        assert_eq!(song.genre.as_deref(), Some("Rock"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_tags_fallback_to_secondary_tags() {
        let path = std::env::temp_dir().join(format!(
            "luminous_id3v1_fallback_test_{}.mp3",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // Create 10 valid MPEG-1 Layer 3 frames
        let mut mp3_bytes = Vec::new();
        let frame_header = [0xFF, 0xFB, 0x90, 0x00];
        for _ in 0..10 {
            mp3_bytes.extend_from_slice(&frame_header);
            mp3_bytes.resize(mp3_bytes.len() + 413, 0);
        }

        // 128-byte ID3v1.1 tag with year and album
        let mut id3v1 = [0u8; 128];
        id3v1[0..3].copy_from_slice(b"TAG");
        let album = b"Night at the Opera";
        id3v1[63..63 + album.len()].copy_from_slice(album);
        id3v1[93..97].copy_from_slice(b"1975");
        mp3_bytes.extend_from_slice(&id3v1);

        std::fs::write(&path, &mp3_bytes).expect("failed to write test mp3 fixture");

        // Write an ID3v2 tag that has Title and Artist but no Album and no Year
        let mut id3v2_tag = Tag::new(lofty::tag::TagType::Id3v2);
        id3v2_tag.set_title("Love of My Life".to_string());
        id3v2_tag.set_artist("Queen".to_string());
        id3v2_tag
            .save_to_path(&path, lofty::config::WriteOptions::default())
            .expect("should save ID3v2 tag");

        let song = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(song.title.as_deref(), Some("Love of My Life"));
        assert_eq!(song.artist.as_deref(), Some("Queen"));
        // Album and Year fall back to ID3v1
        assert_eq!(song.album.as_deref(), Some("Night at the Opera"));
        assert_eq!(song.year, Some(1975));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_upsert_song_round_trips_compilation_flag() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_compilation_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let song = Song {
            path: Some("path/comp.mp3".to_string()),
            title: Some("Comp Track".to_string()),
            artist: Some("Artist A".to_string()),
            album: Some("Now That's What I Call Tests".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Mp3,
            unavailable: false,
            compilation: true,
            ..Default::default()
        };
        upsert_song(&conn, &song).unwrap();

        let stored: bool = conn
            .query_row(
                "SELECT compilation FROM songs WHERE path = ?1",
                params!["path/comp.mp3"],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored, "compilation flag should persist through upsert");

        // Re-upserting the same path with compilation=false should clear it
        // (SONG_INSERT_COLS previously omitted this column entirely, so it
        // could only ever stay 0 — this guards against that regression).
        let song_updated = Song {
            compilation: false,
            ..song
        };
        upsert_song(&conn, &song_updated).unwrap();
        let stored: bool = conn
            .query_row(
                "SELECT compilation FROM songs WHERE path = ?1",
                params!["path/comp.mp3"],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!stored, "compilation flag should update on re-scan");

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_parse_decade_range() {
        assert_eq!(parse_decade_range("1980s"), Some((1980, 1989)));
        assert_eq!(parse_decade_range("1990S"), Some((1990, 1999)));
        assert_eq!(parse_decade_range("2000"), Some((2000, 2009)));
        assert_eq!(parse_decade_range("2020s"), Some((2020, 2029)));
        assert_eq!(parse_decade_range("invalid"), None);
    }

    #[test]
    fn test_get_songs_by_tag_population_modes() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_population_mode_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let genre = "Test Genre";
        let base = Song {
            genre: Some(genre.to_string()),
            source: SongSource::LocalFile,
            ..Default::default()
        };

        // upsert_song() deliberately never touches rating/playcount/lastplayed
        // (those are owned by the stats.rs write path, preserved across
        // rescans) — so seed songs via upsert_song(), then set stats directly.
        let seed =
            |path: &str, title: &str, rating: f32, playcount: i32, lastplayed: Option<i64>| {
                upsert_song(
                    &conn,
                    &Song {
                        title: Some(title.to_string()),
                        path: Some(path.to_string()),
                        ..base.clone()
                    },
                )
                .unwrap();
                conn.execute(
                    "UPDATE songs SET rating = ?1, playcount = ?2, lastplayed = ?3 WHERE path = ?4",
                    params![rating, playcount, lastplayed, path],
                )
                .unwrap();
            };

        // Rated 5 stars, never played — should surface under Favourites, and
        // under Deep Cuts (playcount = 0).
        seed("/music/fav.mp3", "Fav", 5.0, 0, None);

        // Heavily played, unrated — should surface under Discover (playcount > 0)
        // but not Favourites or Deep Cuts.
        seed(
            "/music/familiar.mp3",
            "Familiar",
            0.0,
            50,
            Some(1_700_000_000),
        );

        // Lightly played, unrated — should surface under Discover, not
        // Favourites or Deep Cuts.
        seed(
            "/music/discover.mp3",
            "Discover",
            0.0,
            2,
            Some(1_700_000_000),
        );

        // Never played, no lastplayed timestamp — should surface under Deep
        // Cuts, not Favourites or Discover.
        seed("/music/deepcut.mp3", "DeepCut", 0.0, 0, None);

        // Played once, low rating — excluded from all three biased modes.
        seed("/music/neutral.mp3", "Neutral", 2.0, 1, Some(1_700_000_000));

        let tag_manager = crate::tags::TagManager::new(db);

        fn titles(mut songs: Vec<Song>) -> Vec<String> {
            let mut out: Vec<String> = songs.drain(..).filter_map(|s| s.title).collect();
            out.sort();
            out
        }

        let all = tag_manager
            .get_songs_by_tag(genre, 10, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(
            titles(all),
            vec!["DeepCut", "Discover", "Familiar", "Fav", "Neutral"]
        );

        let familiar = tag_manager
            .get_songs_by_tag(genre, 10, QueuePopulationMode::Familiar)
            .unwrap();
        assert_eq!(
            titles(familiar),
            vec!["Discover", "Familiar", "Neutral"],
            "Familiar excludes never-played songs (playcount > 0), then biases order by \
             playcount among the rest"
        );

        let favourites = tag_manager
            .get_songs_by_tag(genre, 10, QueuePopulationMode::Favourites)
            .unwrap();
        assert_eq!(titles(favourites), vec!["Fav"]);

        let discover = tag_manager
            .get_songs_by_tag(genre, 10, QueuePopulationMode::Discover)
            .unwrap();
        assert_eq!(titles(discover), vec!["Discover", "Familiar", "Neutral"]);

        let deep_cuts = tag_manager
            .get_songs_by_tag(genre, 10, QueuePopulationMode::DeepCuts)
            .unwrap();
        assert_eq!(titles(deep_cuts), vec!["DeepCut", "Fav"]);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_prune_missing_songs_hard_deletes() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_hard_delete_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO directories (path) VALUES (?1)",
            params![temp_dir.to_string_lossy()],
        )
        .unwrap();

        let real_file = temp_dir.join("real.mp3");
        std::fs::write(&real_file, b"audio").unwrap();

        let song_real = Song {
            path: Some(real_file.to_string_lossy().to_string()),
            title: Some("Real Track".to_string()),
            source: SongSource::LocalFile,
            ..Default::default()
        };
        upsert_song(&conn, &song_real).unwrap();

        let song_missing = Song {
            path: Some(temp_dir.join("missing.mp3").to_string_lossy().to_string()),
            title: Some("Missing Track".to_string()),
            source: SongSource::LocalFile,
            ..Default::default()
        };
        upsert_song(&conn, &song_missing).unwrap();

        let scanner = CollectionScanner::new(db.clone());
        let pruned = scanner.prune_missing_songs().unwrap();
        assert_eq!(pruned.deleted_songs, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM songs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let remaining_path: String = conn
            .query_row("SELECT path FROM songs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining_path, real_file.to_string_lossy().to_string());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_prune_missing_songs_skips_unreachable_watched_root() {
        // Simulates a watched directory that lives on a drive that's disconnected or
        // unmounted at scan time: the root itself can't be read, so every song under it
        // must NOT be treated as user-deleted, even though a plain `Path::exists()` check
        // on each song's path would say "gone" just the same as if the folder were removed.
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_unreachable_root_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        // A watched directory root that never exists on disk (stand-in for an
        // unmounted network share or a sleeping external drive).
        let unreachable_root = temp_dir.join("unreachable_share");
        let scanner = CollectionScanner::new(db.clone());
        scanner
            .add_directory(&unreachable_root.to_string_lossy())
            .unwrap();

        let song_under_unreachable_root = Song {
            path: Some(
                unreachable_root
                    .join("track.mp3")
                    .to_string_lossy()
                    .to_string(),
            ),
            title: Some("On The Unreachable Share".to_string()),
            source: SongSource::LocalFile,
            ..Default::default()
        };
        upsert_song(&conn, &song_under_unreachable_root).unwrap();

        // A song whose path is missing but isn't under any watched root at all —
        // reachability doesn't apply to it, so it should still be treated as missing.
        let song_orphaned = Song {
            path: Some(
                temp_dir
                    .join("not_under_any_watched_dir.mp3")
                    .to_string_lossy()
                    .to_string(),
            ),
            title: Some("Orphaned Track".to_string()),
            source: SongSource::LocalFile,
            ..Default::default()
        };
        upsert_song(&conn, &song_orphaned).unwrap();

        // Automatic path: mark_missing_unavailable must leave the song under the
        // unreachable root alone, while still flagging the orphaned one.
        let marked = scanner.mark_missing_unavailable().unwrap();
        assert_eq!(
            marked, 1,
            "only the orphaned song should be flagged missing"
        );

        let unavailable_flags: Vec<(String, bool)> = {
            let mut stmt = conn.prepare("SELECT path, unavailable FROM songs").unwrap();
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .flatten()
                .collect()
        };
        for (path, unavailable) in &unavailable_flags {
            if path.contains("unreachable_share") {
                assert!(
                    !unavailable,
                    "song under unreachable watched root must not be flagged missing"
                );
            } else {
                assert!(*unavailable, "orphaned missing song should be flagged");
            }
        }

        // Explicit path: prune_missing_songs must not hard-delete the song under the
        // unreachable root either, even though its file path fails Path::exists().
        let pruned = scanner.prune_missing_songs().unwrap();
        assert_eq!(
            pruned.deleted_songs, 1,
            "only the already-flagged orphaned song should be hard-deleted"
        );

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM songs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 1,
            "song under the unreachable watched root must survive the prune"
        );

        let remaining_path: String = conn
            .query_row("SELECT path FROM songs", [], |r| r.get(0))
            .unwrap();
        assert!(remaining_path.contains("unreachable_share"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_merge_duplicate_songs_by_case_insensitive_path() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_merge_dup_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let song_a = Song {
            path: Some("C:/Music/HERO/track.mp3".to_string()),
            title: Some("Sugar".to_string()),
            source: SongSource::LocalFile,
            ..Default::default()
        };
        upsert_song(&conn, &song_a).unwrap();
        let id_a: i64 = conn
            .query_row("SELECT id FROM songs", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "UPDATE songs SET playcount = 3, rating = 0.8 WHERE id = ?1",
            params![id_a],
        )
        .unwrap();

        // Same physical file, re-scanned after the folder was renamed to "Hero" —
        // differs only by case from song_a's path.
        let song_b = Song {
            path: Some("C:/Music/Hero/track.mp3".to_string()),
            title: Some("Sugar".to_string()),
            source: SongSource::LocalFile,
            ..Default::default()
        };
        upsert_song(&conn, &song_b).unwrap();
        let id_b: i64 = conn
            .query_row("SELECT id FROM songs WHERE id != ?1", params![id_a], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "UPDATE songs SET playcount = 2 WHERE id = ?1",
            params![id_b],
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());
        let merged = scanner.merge_duplicate_songs().unwrap();
        assert_eq!(merged, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM songs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 1,
            "duplicate rows for the same file must collapse into one"
        );

        let (remaining_id, playcount, rating): (i64, i32, f64) = conn
            .query_row("SELECT id, playcount, rating FROM songs", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(
            remaining_id, id_b,
            "the most recently upserted row (higher id) survives"
        );
        assert_eq!(
            playcount, 5,
            "playcounts from both rows must be summed, not discarded"
        );
        assert_eq!(
            rating, 0.8,
            "a real rating from the discarded row must not be lost to an unset -1"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_read_and_prepare_song_falls_back_to_folder_art() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_prep_song_art_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);

        let audio_path = temp_dir.join("track.wav");
        let folder_art_path = temp_dir.join("folder.jpg");

        write_test_wav(&audio_path);
        std::fs::write(&folder_art_path, b"JPEG image content").unwrap();

        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let cover_manager = CoverManager::new(db, temp_dir.clone());

        let song = read_and_prepare_song(&cover_manager, &audio_path).unwrap();

        assert!(song.art_automatic.is_some());
        assert_eq!(
            song.art_automatic.unwrap(),
            folder_art_path.to_string_lossy().to_string()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
