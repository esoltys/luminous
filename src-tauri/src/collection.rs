//! Collection module — library scanner, file watcher, and DB integration.

use crate::{
    covermanager::CoverManager,
    db::Database,
    models::{
        self, AlbumItem, ArtistProfile, ArtistSocialLink, BatchPhase, BatchProgress, FileType,
        HomeItem, LibraryStats, MusicDirectory, Playlist, PruneResult, QueuePopulationMode,
        ScanPhase, ScanProgress, Song, SongSource,
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
use notify::Watcher;
use rayon::prelude::*;
use rusqlite::{params, ToSql};
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc,
    },
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

/// How long to keep the watcher paused *after* a guard's write is done,
/// before actually re-arming it. The OS delivers filesystem-change
/// notifications asynchronously — on a network share or a busy disk, a
/// "modified" event for a write that already returned can still land on the
/// watcher's channel a few hundred ms later. Unpausing the instant the guard
/// drops raced that delivery and let self-inflicted writes leak through as a
/// spurious watcher batch anyway (#233).
const WATCHER_UNPAUSE_GRACE: std::time::Duration = std::time::Duration::from_millis(1500);

/// Pauses the realtime file watcher for as long as it (plus its grace period)
/// is alive. Any code that writes to a file Luminous itself already knows
/// about — `scan_all`'s tag reads and cover-art writes, or an explicit
/// tag-editor save — should hold one for the duration of the write. Without
/// it, the (already-running) watcher misreads that self-inflicted activity
/// as an external change and re-reports it through its own batch-processing
/// events, producing a spurious "songs added"/"songs updated" toast for an
/// action that already has (or doesn't need) its own feedback (#233).
///
/// Backed by a depth counter rather than a bool so overlapping guards (e.g. a
/// scan and a tag-editor save close together) don't let one's release
/// re-arm the watcher while the other is still writing.
pub struct WatcherPauseGuard {
    flag: Arc<AtomicU32>,
}

impl WatcherPauseGuard {
    pub fn new(flag: Arc<AtomicU32>) -> Self {
        flag.fetch_add(1, Ordering::Relaxed);
        Self { flag }
    }
}

impl Drop for WatcherPauseGuard {
    fn drop(&mut self) {
        let flag = Arc::clone(&self.flag);
        std::thread::spawn(move || {
            std::thread::sleep(WATCHER_UNPAUSE_GRACE);
            flag.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

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
        let mut stmt = conn.prepare("SELECT id, path, subdirs FROM directories ORDER BY path")?;
        let dirs = stmt
            .query_map([], |row| {
                let path: String = row.get(1)?;
                let is_available = std::path::Path::new(&path).exists();
                Ok(MusicDirectory {
                    id: row.get(0)?,
                    path,
                    subdirs: row.get(2)?,
                    is_available,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(dirs)
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

        let unreachable_roots = unreachable_watched_roots(conn);

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
    /// explicit user action — see `ScanProgress::silent` (#233).
    pub async fn scan_all(&self, app: AppHandle, force: bool, silent: bool) -> Result<()> {
        let _watcher_pause_guard = app
            .try_state::<crate::AppState>()
            .map(|state| WatcherPauseGuard::new(Arc::clone(&state.watcher_paused)));

        let dirs = self.get_directories()?;
        if dirs.is_empty() {
            // Still tell the frontend we're done — otherwise the isScanning
            // flag it optimistically set before calling this command is
            // never cleared, permanently disabling rescan/cleanup controls.
            let _ = app.emit(
                "scan-progress",
                ScanProgress {
                    phase: ScanPhase::Done,
                    scanned: 0,
                    total: 0,
                    current_path: None,
                    silent,
                },
            );
            return Ok(());
        }

        // Phase 1: discover all files
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                phase: ScanPhase::Discovering,
                scanned: 0,
                total: 0,
                current_path: None,
                silent,
            },
        );

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
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                phase: ScanPhase::ReadingTags,
                scanned: 0,
                total,
                current_path: None,
                silent,
            },
        );

        let mut scanned = 0u64;
        let app_data_dir = app.path().app_data_dir().expect("no app data dir");
        let cover_manager = CoverManager::new(Arc::clone(&self.db), app_data_dir);

        {
            let conn = self.db.pool.get()?;

            // Repoint DB rows for files that moved to a different watched folder
            // (e.g. a library split/reorganization) before doing anything else,
            // so the per-file loop below and mark_missing_unavailable() both see
            // the corrected path instead of treating the move as delete+new-insert.
            match reconcile_moved_songs(&conn, &all_paths) {
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
                        let _ = app.emit(
                            "scan-progress",
                            ScanProgress {
                                phase: ScanPhase::ReadingTags,
                                scanned,
                                total,
                                current_path: Some(path.to_string_lossy().to_string()),
                                silent,
                            },
                        );
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
            let _ = app.emit(
                "scan-progress",
                ScanProgress {
                    phase: ScanPhase::Updating,
                    scanned: total,
                    total,
                    current_path: None,
                    silent,
                },
            );
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

            let _ = app.emit(
                "scan-progress",
                ScanProgress {
                    phase: ScanPhase::Updating,
                    scanned: updating_scanned,
                    total: total_updating_items,
                    current_path: Some(display_desc),
                    silent,
                },
            );

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
            if !resolved && remote_fetch_count < 50 {
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
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                phase: ScanPhase::Done,
                scanned: total,
                total,
                current_path: None,
                silent,
            },
        );

        Ok(())
    }

    /// Full-text + field search across the library.
    pub fn search_songs(&self, query: &str, limit: i64) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let query_trimmed = query.trim();
        if query_trimmed.is_empty() {
            let sql = format!(
                "SELECT {} FROM songs WHERE unavailable = 0 ORDER BY album_artist, album, disc, track LIMIT ?1",
                SONG_SELECT_COLS
            );
            let mut stmt = conn.prepare(&sql)?;
            let songs = stmt
                .query_map(params![limit], row_to_song)?
                .filter_map(|r| r.ok())
                .collect();
            return Ok(songs);
        }

        let parsed = crate::filter_parser::parse_query(query_trimmed);

        let mut where_clauses = vec!["unavailable = 0".to_string()];
        let mut query_params: Vec<Box<dyn ToSql>> = Vec::new();

        // If bare terms exist, match against FTS5 or LIKE
        if !parsed.bare_terms.is_empty() {
            let bare_str = parsed.bare_terms.join(" ");
            let fts_query = format!("{bare_str}*");
            let like_query = format!("%{bare_str}%");

            query_params.push(Box::new(fts_query));
            let fts_param_idx = query_params.len();
            query_params.push(Box::new(like_query));
            let like_param_idx = query_params.len();

            let bare_sql = format!(
                "(id IN (SELECT rowid FROM songs_fts WHERE songs_fts MATCH ?{fts_param_idx}) OR (title LIKE ?{like_param_idx} OR artist LIKE ?{like_param_idx} OR album LIKE ?{like_param_idx}))"
            );
            where_clauses.push(bare_sql);
        }

        // Add qualified field filter clauses
        for filter in parsed.field_filters {
            let param_idx = query_params.len() + 1;
            let clause = filter.to_sql_clause(param_idx);
            query_params.push(Box::new(filter.value));
            where_clauses.push(clause);
        }

        query_params.push(Box::new(limit));
        let limit_param_idx = query_params.len();

        let where_str = where_clauses.join(" AND ");
        let sql = format!(
            "SELECT {} FROM songs WHERE {} ORDER BY album_artist, album, disc, track LIMIT ?{}",
            SONG_SELECT_COLS, where_str, limit_param_idx
        );

        let params_refs: Vec<&dyn ToSql> = query_params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params_refs.as_slice(), row_to_song)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(songs)
    }

    /// Same rule-query parsing as `search_songs`, but replaces the default
    /// alphabetical ordering with `mode`'s weighted-random bias (see #120),
    /// for populating a Smart Playlist that has a `population_mode` set.
    /// `query` is expected to be a non-empty filter-rule string (as produced
    /// from a playlist's `dynamic_spec`), not a blank/browse-all query.
    pub fn search_songs_by_mode(
        &self,
        query: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);

        let parsed = crate::filter_parser::parse_query(query.trim());

        let mut where_clauses = vec!["unavailable = 0".to_string()];
        if !extra_where.is_empty() {
            where_clauses.push(extra_where.trim_start_matches(" AND ").to_string());
        }
        let mut query_params: Vec<Box<dyn ToSql>> = Vec::new();

        if !parsed.bare_terms.is_empty() {
            let bare_str = parsed.bare_terms.join(" ");
            let fts_query = format!("{bare_str}*");
            let like_query = format!("%{bare_str}%");

            query_params.push(Box::new(fts_query));
            let fts_param_idx = query_params.len();
            query_params.push(Box::new(like_query));
            let like_param_idx = query_params.len();

            let bare_sql = format!(
                "(id IN (SELECT rowid FROM songs_fts WHERE songs_fts MATCH ?{fts_param_idx}) OR (title LIKE ?{like_param_idx} OR artist LIKE ?{like_param_idx} OR album LIKE ?{like_param_idx}))"
            );
            where_clauses.push(bare_sql);
        }

        for filter in parsed.field_filters {
            let param_idx = query_params.len() + 1;
            let clause = filter.to_sql_clause(param_idx);
            query_params.push(Box::new(filter.value));
            where_clauses.push(clause);
        }

        query_params.push(Box::new(limit));
        let limit_param_idx = query_params.len();

        let where_str = where_clauses.join(" AND ");
        let sql = format!(
            "SELECT {} FROM songs WHERE {} ORDER BY {} LIMIT ?{}",
            SONG_SELECT_COLS, where_str, order_by, limit_param_idx
        );

        let params_refs: Vec<&dyn ToSql> = query_params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params_refs.as_slice(), row_to_song)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(songs)
    }

    /// Paginated library listing — every available local/collection song
    /// (not streaming sources), ordered by album artist/album/disc/track.
    pub fn get_songs(&self, limit: i64, offset: i64) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE source IN (1, 2) AND unavailable = 0
             ORDER BY album_artist, album, disc, track
             LIMIT ?1 OFFSET ?2",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![limit, offset], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    pub fn get_songs_by_album(&self, album: &str) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE album = ?1
               AND source IN (1, 2)
               AND unavailable = 0
             ORDER BY disc, track",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![album], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Matches `artist` against either the per-track `artist` column or the
    /// `album_artist` column (via `multi_value_contains_pattern`), whichever
    /// contains it as one of its individual `; `-delimited values — not just
    /// the "effective" (album_artist-preferred) column this used to check
    /// with a single exact-equality comparison. Checking both columns
    /// separately, rather than collapsing to one via `COALESCE` first,
    /// matters for two real cases a single-column check misses:
    /// - A collab/featured credit on an otherwise normal (non-compilation)
    ///   album, e.g. artist = "Evergrey; Mikael Stanne", album_artist =
    ///   "Evergrey" — clicking "Mikael Stanne" only finds this track by
    ///   checking the raw `artist` column, since `album_artist` alone would
    ///   win under a `COALESCE` and never mention him.
    /// - A Various Artists compilation track, e.g. artist = "Artist X",
    ///   album_artist = "Various Artists" — clicking "Artist X" only finds
    ///   it by checking `artist` directly (this case was previously only
    ///   reachable via the separate album-level `get_compilations_by_artist`
    ///   query, which still exists for the album-card view of the same data).
    ///
    /// The library-wide Artists browse grouping (`get_artists`) is
    /// intentionally *not* fanned out the same way — a collab track still
    /// contributes to one combined "Evergrey; Mikael Stanne" card there
    /// rather than to both artists' cards/counts. Full fan-out of that
    /// grouping is tracked as separate follow-up work, same scope decision
    /// #143 made for genre.
    pub fn get_songs_by_artist(&self, artist: &str) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE ({} OR {})
               AND source IN (1, 2)
               AND unavailable = 0
             ORDER BY album, disc, track",
            SONG_SELECT_COLS,
            multi_value_contains_sql("COALESCE(artist, '')", "?1"),
            multi_value_contains_sql("COALESCE(album_artist, '')", "?1")
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![multi_value_contains_pattern(artist)], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Compilation albums featuring `artist` on at least one track — the
    /// mirror image of `get_songs_by_artist`'s effective-artist match: this
    /// matches on the *raw per-track* `artist` column (via
    /// `multi_value_contains_pattern`, so a per-track collab credit still
    /// matches), since a Various Artists compilation's effective
    /// (album-level) artist is never the individual track artist, so it
    /// would otherwise never surface on that artist's own detail page
    /// (#343). An album counts as a compilation if any track has
    /// `compilation = 1`, its tracks disagree on `album_artist`, or
    /// `album_artist` is literally "Various Artists". Returns the same
    /// aggregated shape as `get_albums()`, but with `artist` always
    /// "Various Artists" since these are compilations by definition.
    pub fn get_compilations_by_artist(&self, artist: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT
                album,
                MIN(year) AS year,
                COUNT(*) AS track_count,
                MAX(COALESCE(disc, 1)) AS disc_count,
                MAX(CAST(art_embedded AS INTEGER)) AS art_embedded,
                MAX(art_automatic) AS art_automatic,
                MAX(art_manual) AS art_manual,
                (
                    SELECT genre
                    FROM songs g
                    WHERE g.album = songs.album AND g.source IN (1, 2) AND g.unavailable = 0
                      AND g.genre IS NOT NULL AND g.genre != ''
                    GROUP BY genre
                    ORDER BY COUNT(*) DESC, genre ASC
                    LIMIT 1
                ) AS genre,
                COALESCE(
                    (SELECT rating FROM album_ratings ar WHERE ar.album_key = songs.album),
                    -1
                ) AS rating,
                MAX(added) AS added,
                COALESCE(SUM(length_nanosec), 0) AS total_duration_nanosec
             FROM songs
             WHERE source IN (1, 2) AND unavailable = 0 AND album IS NOT NULL
               AND album IN (
                 SELECT album FROM songs s2
                 WHERE s2.source IN (1, 2) AND s2.unavailable = 0 AND {}
               )
               AND album IN (
                 SELECT album FROM songs s3
                 WHERE s3.source IN (1, 2) AND s3.unavailable = 0
                 GROUP BY album
                 -- Mirrors get_albums()'s various-artists fallback: a compilation
                 -- either has TCMP set, is explicitly credited to Various
                 -- Artists, or fails to agree on a single effective album
                 -- artist (the same condition that makes get_albums() emit
                 -- NULL and the UI fall back to displaying Various Artists).
                 HAVING MAX(s3.compilation) = 1
                    OR MAX(CASE WHEN s3.album_artist = 'Various Artists' THEN 1 ELSE 0 END) = 1
                    OR NOT (
                         COUNT(DISTINCT NULLIF(s3.album_artist, '')) = 1
                         OR (
                           COUNT(DISTINCT NULLIF(s3.album_artist, '')) = 0
                           AND COUNT(DISTINCT NULLIF(s3.artist, '')) = 1
                         )
                       )
               )
             GROUP BY album
             ORDER BY album",
            multi_value_contains_sql("s2.artist", "?1")
        );
        let mut stmt = conn.prepare(&sql)?;
        let albums: Vec<serde_json::Value> = stmt
            .query_map(params![multi_value_contains_pattern(artist)], |row| {
                Ok(serde_json::json!({
                    "artist": "Various Artists",
                    "album": row.get::<_, Option<String>>(0)?,
                    "year": row.get::<_, Option<i32>>(1)?,
                    "track_count": row.get::<_, i32>(2)?,
                    "disc_count": row.get::<_, i32>(3)?,
                    "art_embedded": row.get::<_, bool>(4)?,
                    "art_automatic": row.get::<_, Option<String>>(5)?,
                    "art_manual": row.get::<_, Option<String>>(6)?,
                    "genre": row.get::<_, Option<String>>(7)?,
                    "rating": row.get::<_, f32>(8)?,
                    "added": row.get::<_, Option<i64>>(9)?,
                    "total_duration_nanosec": row.get::<_, i64>(10)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(albums)
    }

    /// Songs favourited via the 5-star/heart rating, for the "Favourites" auto-playlist.
    pub fn get_favourite_songs(&self) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE rating = 5
               AND source IN (1, 2)
               AND unavailable = 0
             ORDER BY album_artist, album, disc, track",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map([], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Most recently added songs, for the "Recently Added" auto-playlist.
    pub fn get_recently_added_songs(&self, limit: i64) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE source IN (1, 2)
               AND unavailable = 0
               AND added IS NOT NULL
             ORDER BY added DESC
             LIMIT ?1",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Songs in a genre, selected per `mode`'s bias (see #120), for per-genre
    /// auto-playlists.
    ///
    /// `genre` is matched with exact equality against the full (possibly
    /// multi-value, `; `-delimited) `songs.genre` column — a song tagged
    /// only `"Rock"` and one tagged `"Rock; Blues"` are treated as
    /// different genres here, unlike the substring-matching `genre:` Smart
    /// Playlist filter in `filter_parser.rs`. Deliberately left unsplit for
    /// #143: fanning multi-value genres out into their own auto-playlists
    /// is native-tags territory, tracked by #224.
    pub fn get_songs_by_genre(
        &self,
        genre: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
        let sql = format!(
            "SELECT {} FROM songs
             WHERE genre = ?1
               AND source IN (1, 2)
               AND unavailable = 0
               {extra_where}
             ORDER BY {order_by}
             LIMIT ?2",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![genre, limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Distinct non-empty genres present in the library, used to build one
    /// auto-playlist per genre.
    pub fn get_library_genres(&self) -> Result<Vec<String>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT genre FROM songs
             WHERE source IN (1, 2)
               AND unavailable = 0
               AND genre IS NOT NULL
               AND genre != ''
             ORDER BY genre",
        )?;
        let genres = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(genres)
    }

    /// Distinct decades present in the library (e.g. "1980s", "1990s"), used to build one
    /// auto-playlist per decade.
    pub fn get_library_decades(&self) -> Result<Vec<String>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT (COALESCE(year, originalyear) / 10 * 10) AS decade_start
             FROM songs
             WHERE source IN (1, 2)
               AND unavailable = 0
               AND COALESCE(year, originalyear) IS NOT NULL
               AND COALESCE(year, originalyear) >= 1000
               AND COALESCE(year, originalyear) <= 9999
             ORDER BY decade_start ASC",
        )?;
        let decades = stmt
            .query_map([], |row| {
                let start: i32 = row.get(0)?;
                Ok(format!("{}s", start))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(decades)
    }

    /// Songs in a decade (e.g. "1980s"), selected per `mode`'s bias (see
    /// #120), for per-decade auto-playlists.
    pub fn get_songs_by_decade(
        &self,
        decade: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let (start, end) = match parse_decade_range(decade) {
            Some(range) => range,
            None => return Ok(Vec::new()),
        };
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
        let sql = format!(
            "SELECT {} FROM songs
             WHERE COALESCE(year, originalyear) >= ?1
               AND COALESCE(year, originalyear) <= ?2
               AND source IN (1, 2)
               AND unavailable = 0
               {extra_where}
             ORDER BY {order_by}
             LIMIT ?3",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![start, end, limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Songs whose BPM falls within `[min, max]` (an unbounded `max` means "or
    /// higher"), selected per `mode`'s bias (see #120), for the fixed-bucket
    /// BPM auto-playlists (Down-Tempo, Mid-Tempo, Uptempo, High Energy, Extreme).
    pub fn get_songs_by_bpm_range(
        &self,
        min: f64,
        max: Option<f64>,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
        let upper_bound = match max {
            Some(_) => "AND bpm <= ?2",
            None => "",
        };
        let sql = format!(
            "SELECT {} FROM songs
             WHERE bpm >= ?1
               {upper_bound}
               AND source IN (1, 2)
               AND unavailable = 0
               {extra_where}
             ORDER BY {order_by}
             LIMIT ?3",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![min, max.unwrap_or(0.0), limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// One aggregated entry per distinct `album` value across the whole
    /// library (untyped JSON, not a `Song`/`AlbumItem` — see the query below
    /// for the exact field set), each summarizing every track that shares
    /// that album name regardless of which artist tagged it.
    pub fn get_albums(&self) -> Result<Vec<serde_json::Value>> {
        let conn = self.db.pool.get()?;
        // Group only by album name so that tracks with different per-track artists
        // but the same album title are consolidated into a single entry.
        // album_artist is taken as the shared value when all tracks agree on it;
        // if they differ (true various-artist albums), it comes back as NULL.
        let mut stmt = conn.prepare(
            "SELECT
                CASE
                    WHEN COUNT(DISTINCT NULLIF(album_artist, '')) = 1 THEN MAX(NULLIF(album_artist, ''))
                    WHEN COUNT(DISTINCT NULLIF(album_artist, '')) = 0 AND COUNT(DISTINCT NULLIF(artist, '')) = 1 THEN MAX(NULLIF(artist, ''))
                    ELSE NULL
                END AS album_artist,
                album,
                MIN(year) AS year,
                COUNT(*) AS track_count,
                MAX(COALESCE(disc, 1)) AS disc_count,
                MAX(CAST(art_embedded AS INTEGER)) AS art_embedded,
                MAX(art_automatic) AS art_automatic,
                MAX(art_manual) AS art_manual,
                (
                    SELECT genre
                    FROM songs g
                    WHERE g.album = songs.album AND g.source IN (1, 2) AND g.unavailable = 0
                      AND g.genre IS NOT NULL AND g.genre != ''
                    GROUP BY genre
                    ORDER BY COUNT(*) DESC, genre ASC
                    LIMIT 1
                ) AS genre,
                COALESCE(
                    (SELECT rating FROM album_ratings ar WHERE ar.album_key = songs.album),
                    -1
                ) AS rating,
                MAX(added) AS added,
                COALESCE(SUM(length_nanosec), 0) AS total_duration_nanosec
             FROM songs
             WHERE source IN (1, 2) AND album IS NOT NULL AND unavailable = 0
             GROUP BY album
             ORDER BY album_artist, album",
        )?;
        let albums: Vec<serde_json::Value> = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "artist": row.get::<_, Option<String>>(0)?,
                    "album": row.get::<_, Option<String>>(1)?,
                    "year": row.get::<_, Option<i32>>(2)?,
                    "track_count": row.get::<_, i32>(3)?,
                    "disc_count": row.get::<_, i32>(4)?,
                    "art_embedded": row.get::<_, bool>(5)?,
                    "art_automatic": row.get::<_, Option<String>>(6)?,
                    "art_manual": row.get::<_, Option<String>>(7)?,
                    "genre": row.get::<_, Option<String>>(8)?,
                    "rating": row.get::<_, f32>(9)?,
                    "added": row.get::<_, Option<i64>>(10)?,
                    "total_duration_nanosec": row.get::<_, i64>(11)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(albums)
    }

    /// One aggregated entry per effective artist (album_artist, falling back
    /// to artist) across the library — untyped JSON, see the query below for
    /// the exact field set. Multi-value album_artist/artist columns group as
    /// one combined pseudo-artist rather than fanning out per individual
    /// artist — see `get_songs_by_artist` for why that's out of scope here.
    pub fn get_artists(&self) -> Result<Vec<serde_json::Value>> {
        let conn = self.db.pool.get()?;
        // Artists are grouped case-insensitively (COLLATE NOCASE) so that tag-casing
        // drift across files/albums for the same real-world artist (e.g. "The War On
        // Drugs" vs "The War on Drugs") doesn't show up as two separate cards — see
        // issue #295. `MIN(...)` picks one deterministic casing per group to display.
        let mut stmt = conn.prepare(
            "WITH album_counts AS (
                SELECT album, COUNT(*) AS track_count
                FROM songs
                WHERE source IN (1, 2) AND album IS NOT NULL AND unavailable = 0
                GROUP BY album
             ),
             base AS (
                SELECT s.id, s.album, COALESCE(NULLIF(s.album_artist, ''), s.artist, '') AS effective_artist
                FROM songs s
                WHERE s.source IN (1, 2) AND s.unavailable = 0
             ),
             grouped AS (
                SELECT MIN(effective_artist) AS effective_artist, COUNT(*) AS song_count
                FROM base
                GROUP BY effective_artist COLLATE NOCASE
             )
             SELECT
                g.effective_artist,
                (
                    SELECT COUNT(DISTINCT CASE WHEN ac.track_count > 7 THEN b.album END)
                    FROM base b
                    LEFT JOIN album_counts ac ON b.album = ac.album
                    WHERE b.effective_artist = g.effective_artist COLLATE NOCASE
                ) AS album_count,
                g.song_count,
                (
                    SELECT genre
                    FROM songs sg
                    WHERE COALESCE(NULLIF(sg.album_artist, ''), sg.artist, '') = g.effective_artist COLLATE NOCASE
                      AND sg.source IN (1, 2) AND sg.unavailable = 0 AND sg.genre IS NOT NULL AND sg.genre != ''
                    GROUP BY sg.genre
                    ORDER BY COUNT(*) DESC, sg.genre ASC
                    LIMIT 1
                ) AS genre
             FROM grouped g
             ORDER BY g.effective_artist COLLATE NOCASE",
        )?;
        let artists: Vec<serde_json::Value> = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "name": row.get::<_, Option<String>>(0)?,
                    "album_count": row.get::<_, i32>(1)?,
                    "song_count": row.get::<_, i32>(2)?,
                    "genre": row.get::<_, Option<String>>(3)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(artists)
    }

    /// Artists ranked by total play count across their songs (ties broken
    /// alphabetically). Artists with zero plays are excluded entirely —
    /// this powers the Home "Top Artists" carousel, not a full artist
    /// directory (see `get_artists` for that).
    pub fn get_top_artists(&self, limit: i64) -> Result<Vec<serde_json::Value>> {
        let conn = self.db.pool.get()?;
        // See get_artists() for why grouping is case-insensitive (issue #295).
        let mut stmt = conn.prepare(
            "WITH album_counts AS (
                SELECT album, COUNT(*) AS track_count
                FROM songs
                WHERE source IN (1, 2) AND album IS NOT NULL AND unavailable = 0
                GROUP BY album
             ),
             base AS (
                SELECT s.id, s.album, s.playcount,
                       COALESCE(NULLIF(s.album_artist, ''), s.artist, '') AS effective_artist
                FROM songs s
                WHERE s.source IN (1, 2) AND s.unavailable = 0
             ),
             grouped AS (
                SELECT
                    MIN(effective_artist) AS effective_artist,
                    COUNT(*) AS song_count,
                    SUM(COALESCE(playcount, 0)) AS total_playcount
                FROM base
                GROUP BY effective_artist COLLATE NOCASE
                HAVING SUM(COALESCE(playcount, 0)) > 0
             )
             SELECT
                g.effective_artist,
                (
                    SELECT COUNT(DISTINCT CASE WHEN ac.track_count > 7 THEN b.album END)
                    FROM base b
                    LEFT JOIN album_counts ac ON b.album = ac.album
                    WHERE b.effective_artist = g.effective_artist COLLATE NOCASE
                ) AS album_count,
                g.song_count,
                g.total_playcount,
                (
                    SELECT genre
                    FROM songs sg
                    WHERE COALESCE(NULLIF(sg.album_artist, ''), sg.artist, '') = g.effective_artist COLLATE NOCASE
                      AND sg.source IN (1, 2) AND sg.unavailable = 0 AND sg.genre IS NOT NULL AND sg.genre != ''
                    GROUP BY sg.genre
                    ORDER BY COUNT(*) DESC, sg.genre ASC
                    LIMIT 1
                ) AS genre
             FROM grouped g
             ORDER BY g.total_playcount DESC, g.effective_artist COLLATE NOCASE
             LIMIT ?1",
        )?;
        let artists: Vec<serde_json::Value> = stmt
            .query_map(params![limit], |row| {
                Ok(serde_json::json!({
                    "name": row.get::<_, Option<String>>(0)?,
                    "album_count": row.get::<_, i32>(1)?,
                    "song_count": row.get::<_, i32>(2)?,
                    "genre": row.get::<_, Option<String>>(4)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(artists)
    }

    /// Retrieve the customizable profile (website, tags, social links, bio)
    /// for an artist (#473). Returns empty/default profile if none saved yet.
    pub fn get_artist_profile(&self, artist: &str) -> Result<ArtistProfile> {
        let conn = self.db.pool.get()?;
        get_artist_profile_conn(&conn, artist)
    }

    /// Save or update an artist's customizable profile (#473).
    pub fn set_artist_profile(&self, profile: &ArtistProfile) -> Result<ArtistProfile> {
        let conn = self.db.pool.get()?;
        set_artist_profile_conn(&conn, profile)
    }

    /// Retrieve all custom artist profiles in the library (#473).
    pub fn get_all_artist_profiles(&self) -> Result<Vec<ArtistProfile>> {
        let conn = self.db.pool.get()?;
        get_all_artist_profiles_conn(&conn)
    }

    pub fn get_library_stats(&self) -> Result<LibraryStats> {
        let conn = self.db.pool.get()?;
        let stats = conn.query_row(
            "SELECT
                COUNT(*) as total_songs,
                COUNT(DISTINCT COALESCE(NULLIF(album_artist,''), artist)) as total_artists,
                COUNT(DISTINCT album) as total_albums,
                COALESCE(SUM(length_nanosec), 0) as total_duration,
                COALESCE(SUM(filesize), 0) as total_filesize
             FROM songs WHERE source IN (1, 2) AND unavailable = 0",
            [],
            |row| {
                Ok(LibraryStats {
                    total_songs: row.get(0)?,
                    total_artists: row.get(1)?,
                    total_albums: row.get(2)?,
                    total_duration_nanosec: row.get(3)?,
                    total_filesize_bytes: row.get(4)?,
                })
            },
        )?;
        Ok(stats)
    }

    /// Flat list of the `limit` most recently played distinct songs, one row
    /// per song regardless of how many times or contexts it was played in.
    /// Unlike `get_recently_played`, this doesn't group by playback context
    /// into Album/Playlist/Song cards — used where a plain song list is
    /// wanted (e.g. building a "Recently Played" auto-playlist) rather than
    /// Home-screen cards.
    pub fn get_recently_played_songs(&self, limit: i64) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {SONG_SELECT_COLS}
             FROM songs s
             JOIN (
                 SELECT song_id, MAX(played_at) as last_played_at
                 FROM play_history
                 GROUP BY song_id
             ) ph ON s.id = ph.song_id
             WHERE s.source IN (1, 2) AND s.unavailable = 0
             ORDER BY ph.last_played_at DESC
             LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    pub fn clear_play_history(&self) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute("DELETE FROM play_history", [])?;
        Ok(())
    }

    /// Recently played, grouped by what the user actually played from —
    /// an Album card if they were browsing an album, a Playlist card if
    /// they played from a playlist, or a Song card for a standalone pick.
    /// See `play_history` (migration 10) and `PlayContext`.
    pub fn get_recently_played(&self, limit: i64) -> Result<Vec<HomeItem>> {
        let conn = self.db.pool.get()?;
        // Every track of an album collapses into a single Album card, so
        // `limit` raw song rows can produce far fewer than `limit` HomeItems.
        // 20x is a heuristic overfetch so grouping still has enough rows to
        // reach `limit` items for a normal-sized library; it isn't a
        // guarantee for pathological cases (e.g. one giant album).
        let query_limit = limit * 20;
        let home_item_select_cols = home_item_select_cols();
        let sql = format!(
            "SELECT {home_item_select_cols}, ph.context_type, ph.playlist_id
             FROM play_history ph
             JOIN songs s ON s.id = ph.song_id
             WHERE s.source IN (1, 2) AND s.unavailable = 0
               AND NOT (
                   ph.context_type = 'playlist'
                   AND ph.playlist_id IN (
                       SELECT id FROM playlists WHERE dynamic_enabled = 0 AND LOWER(name) = 'queue'
                   )
               )
             ORDER BY ph.played_at DESC
             LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows: Vec<(Song, i64, i64, String, Option<i64>)> = stmt
            .query_map(params![query_limit], |row| {
                let song = row_to_song(row)?;
                let album_track_count: i64 = row.get(56)?;
                let album_disc_count: i64 = row.get(57)?;
                let context_type: String = row.get(58)?;
                let playlist_id: Option<i64> = row.get(59)?;
                Ok((
                    song,
                    album_track_count,
                    album_disc_count,
                    context_type,
                    playlist_id,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let playlist_ids: Vec<i64> = {
            use std::collections::HashSet;
            rows.iter()
                .filter(|(_, _, _, context_type, playlist_id)| {
                    context_type == "playlist" && playlist_id.is_some()
                })
                .filter_map(|(_, _, _, _, playlist_id)| *playlist_id)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect()
        };
        let playlists_by_id = get_playlists_by_ids(&conn, &playlist_ids)?;

        Ok(group_by_play_context(
            rows,
            limit as usize,
            &playlists_by_id,
        ))
    }

    /// Most frequently played, grouped the same way as `get_recently_played`
    /// (Album/Playlist/Song by recorded context) but ordered by total play
    /// count per context instead of recency.
    pub fn get_most_frequently_played(&self, limit: i64) -> Result<Vec<HomeItem>> {
        let conn = self.db.pool.get()?;
        let sql = "
            SELECT
                ph.context_type,
                ph.playlist_id,
                COUNT(*) as play_count,
                MAX(ph.played_at) as last_played,
                MAX(ph.song_id) as representative_song_id
            FROM play_history ph
            JOIN songs s ON s.id = ph.song_id
            WHERE s.source IN (1, 2) AND s.unavailable = 0
              AND NOT (
                  ph.context_type = 'playlist'
                  AND ph.playlist_id IN (
                      SELECT id FROM playlists WHERE dynamic_enabled = 0 AND LOWER(name) = 'queue'
                  )
              )
            GROUP BY
                ph.context_type,
                CASE ph.context_type WHEN 'playlist' THEN ph.playlist_id END,
                CASE ph.context_type WHEN 'album' THEN s.album END,
                CASE ph.context_type WHEN 'album' THEN COALESCE(s.album_artist, s.artist) END,
                CASE ph.context_type WHEN 'song' THEN ph.song_id END
            ORDER BY play_count DESC, last_played DESC
            LIMIT ?1
        ";
        let mut stmt = conn.prepare(sql)?;
        struct AggRow {
            context_type: String,
            playlist_id: Option<i64>,
            representative_song_id: i64,
        }
        let agg_rows: Vec<AggRow> = stmt
            .query_map(params![limit], |row| {
                Ok(AggRow {
                    context_type: row.get(0)?,
                    playlist_id: row.get(1)?,
                    representative_song_id: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        let playlist_ids: Vec<i64> = {
            use std::collections::HashSet;
            agg_rows
                .iter()
                .filter(|r| r.context_type == "playlist")
                .filter_map(|r| r.playlist_id)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect()
        };
        let playlists_by_id = get_playlists_by_ids(&conn, &playlist_ids)?;

        let song_ids: Vec<i64> = agg_rows.iter().map(|r| r.representative_song_id).collect();
        let songs_by_id = get_songs_by_ids(&conn, &song_ids)?;

        let mut items: Vec<HomeItem> = agg_rows
            .into_iter()
            .filter_map(|row| {
                let (song, album_track_count, album_disc_count) =
                    songs_by_id.get(&row.representative_song_id)?.clone();
                Some(home_item_for_context(
                    &row.context_type,
                    row.playlist_id,
                    song,
                    album_track_count,
                    album_disc_count,
                    &playlists_by_id,
                ))
            })
            .collect();
        attach_album_ratings(&conn, &mut items)?;
        Ok(items)
    }

    /// Recently added songs grouped into Album cards where an album's other
    /// tracks were also added together, or standalone Song cards otherwise —
    /// same grouping mechanism as `get_recently_played`, see its comments.
    pub fn get_recently_added(&self, limit: i64) -> Result<Vec<HomeItem>> {
        let conn = self.db.pool.get()?;
        // See get_recently_played's identical overfetch-then-group comment.
        let query_limit = limit * 20;
        let home_item_select_cols = home_item_select_cols();
        let sql = format!(
            "SELECT {home_item_select_cols}
             FROM songs s
             WHERE s.source IN (1, 2) AND s.unavailable = 0 AND s.added IS NOT NULL
             ORDER BY s.added DESC
             LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs_with_counts: Vec<(Song, i64, i64)> = stmt
            .query_map(params![query_limit], |row| {
                let song = row_to_song(row)?;
                let count: i64 = row.get(56)?;
                let disc_count: i64 = row.get(57)?;
                Ok((song, count, disc_count))
            })?
            .filter_map(|r| r.ok())
            .collect();
        let mut items = group_songs_into_home_items(songs_with_counts, limit as usize);
        attach_album_ratings(&conn, &mut items)?;
        Ok(items)
    }
}

fn group_songs_into_home_items(
    songs_with_counts: Vec<(Song, i64, i64)>,
    limit: usize,
) -> Vec<HomeItem> {
    use std::collections::HashSet;
    let mut items = Vec::new();
    let mut seen_albums = HashSet::new();

    for (song, album_track_count, album_disc_count) in songs_with_counts {
        if items.len() >= limit {
            break;
        }

        if let Some(ref album_name) = song.album {
            if !album_name.trim().is_empty() && album_track_count > 1 {
                let artist_name = song
                    .album_artist
                    .clone()
                    .or_else(|| song.artist.clone())
                    .unwrap_or_default();
                let album_key = album_name.trim().to_lowercase();

                if !seen_albums.contains(&album_key) {
                    seen_albums.insert(album_key);
                    items.push(HomeItem::Album {
                        album: AlbumItem {
                            artist: Some(artist_name),
                            album: Some(album_name.clone()),
                            year: song.year,
                            track_count: album_track_count as i32,
                            disc_count: album_disc_count as i32,
                            art_embedded: song.art_embedded,
                            art_automatic: song.art_automatic.clone(),
                            art_manual: song.art_manual.clone(),
                            genre: song.genre.clone(),
                            sample_song_id: Some(song.id),
                            rating: crate::stats::RATING_UNRATED,
                            total_duration_nanosec: 0,
                        },
                    });
                }
                continue;
            }
        }

        items.push(HomeItem::Song {
            song: Box::new(song),
        });
    }

    items
}

/// Fill in the real rating for every `HomeItem::Album` in `items`, looked up
/// from `album_ratings`. `group_songs_into_home_items`/`home_item_for_context`
/// build `AlbumItem`s without a DB connection in scope, so they default to
/// unrated — this backfills the actual value once a connection is available.
fn attach_album_ratings(conn: &rusqlite::Connection, items: &mut [HomeItem]) -> Result<()> {
    for item in items.iter_mut() {
        if let HomeItem::Album { album } = item {
            if let Some(ref name) = album.album {
                album.rating = crate::stats::get_album_rating(conn, name)?;
            }
        }
    }
    Ok(())
}

/// Retrieve customizable profile for an artist from SQLite (#473).
pub fn get_artist_profile_conn(
    conn: &rusqlite::Connection,
    artist: &str,
) -> Result<ArtistProfile> {
    let mut stmt = conn.prepare(
        "SELECT artist_key, website, tags, social_links, bio FROM artist_profiles WHERE artist_key = ?1 COLLATE NOCASE",
    )?;
    let result = stmt.query_row(params![artist], |row| {
        let artist_key: String = row.get(0)?;
        let website: Option<String> = row.get(1)?;
        let tags_json: String = row.get(2)?;
        let social_links_json: String = row.get(3)?;
        let bio: Option<String> = row.get(4)?;

        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let social_links: Vec<ArtistSocialLink> =
            serde_json::from_str(&social_links_json).unwrap_or_default();

        Ok(ArtistProfile {
            artist_key,
            website,
            tags,
            social_links,
            bio,
        })
    });

    match result {
        Ok(profile) => Ok(profile),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ArtistProfile {
            artist_key: artist.to_string(),
            website: None,
            tags: Vec::new(),
            social_links: Vec::new(),
            bio: None,
        }),
        Err(e) => Err(e.into()),
    }
}

/// Upsert an artist profile into SQLite (#473).
pub fn set_artist_profile_conn(
    conn: &rusqlite::Connection,
    profile: &ArtistProfile,
) -> Result<ArtistProfile> {
    let tags_json = serde_json::to_string(&profile.tags)?;
    let social_links_json = serde_json::to_string(&profile.social_links)?;

    conn.execute(
        "INSERT INTO artist_profiles (artist_key, website, tags, social_links, bio)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(artist_key) DO UPDATE SET
            website = excluded.website,
            tags = excluded.tags,
            social_links = excluded.social_links,
            bio = excluded.bio",
        params![
            profile.artist_key,
            profile.website,
            tags_json,
            social_links_json,
            profile.bio
        ],
    )?;

    Ok(profile.clone())
}

/// Retrieve all saved artist profiles in SQLite (#473).
pub fn get_all_artist_profiles_conn(conn: &rusqlite::Connection) -> Result<Vec<ArtistProfile>> {
    let mut stmt = conn.prepare(
        "SELECT artist_key, website, tags, social_links, bio FROM artist_profiles ORDER BY artist_key COLLATE NOCASE",
    )?;
    let profiles = stmt
        .query_map([], |row| {
            let artist_key: String = row.get(0)?;
            let website: Option<String> = row.get(1)?;
            let tags_json: String = row.get(2)?;
            let social_links_json: String = row.get(3)?;
            let bio: Option<String> = row.get(4)?;

            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let social_links: Vec<ArtistSocialLink> =
                serde_json::from_str(&social_links_json).unwrap_or_default();

            Ok(ArtistProfile {
                artist_key,
                website,
                tags,
                social_links,
                bio,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(profiles)
}

/// Dedup key for context-aware Recently Played — one entry per album,
/// playlist, or standalone song, keeping only the most recent occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PlayContextKey {
    Playlist(i64),
    Album(String),
    Song(i64),
}

fn group_by_play_context(
    rows: Vec<(Song, i64, i64, String, Option<i64>)>,
    limit: usize,
    playlists_by_id: &std::collections::HashMap<i64, Playlist>,
) -> Vec<HomeItem> {
    use std::collections::HashSet;
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for (song, album_track_count, album_disc_count, context_type, playlist_id) in rows {
        if items.len() >= limit {
            break;
        }

        let key = match context_type.as_str() {
            "playlist" if playlist_id.is_some() => PlayContextKey::Playlist(playlist_id.unwrap()),
            "album" if song.album.as_deref().is_some_and(|a| !a.trim().is_empty()) => {
                PlayContextKey::Album(song.album.clone().unwrap().trim().to_lowercase())
            }
            _ => PlayContextKey::Song(song.id),
        };

        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);

        items.push(home_item_for_context(
            &context_type,
            playlist_id,
            song,
            album_track_count,
            album_disc_count,
            playlists_by_id,
        ));
    }

    items
}

/// Build the HomeItem a play-history context maps to: a Playlist card when
/// resolvable, an Album card when the representative song carries an album
/// tag, otherwise a standalone Song card.
fn home_item_for_context(
    context_type: &str,
    playlist_id: Option<i64>,
    song: Song,
    album_track_count: i64,
    album_disc_count: i64,
    playlists_by_id: &std::collections::HashMap<i64, Playlist>,
) -> HomeItem {
    match context_type {
        "playlist" => match playlist_id.and_then(|id| playlists_by_id.get(&id)) {
            Some(playlist) => HomeItem::Playlist {
                playlist: playlist.clone(),
            },
            None => HomeItem::Song {
                song: Box::new(song),
            },
        },
        "album" if song.album.as_deref().is_some_and(|a| !a.trim().is_empty()) => {
            let artist_name = song
                .album_artist
                .clone()
                .or_else(|| song.artist.clone())
                .unwrap_or_default();
            HomeItem::Album {
                album: AlbumItem {
                    artist: Some(artist_name),
                    album: song.album.clone(),
                    year: song.year,
                    track_count: album_track_count as i32,
                    disc_count: album_disc_count as i32,
                    art_embedded: song.art_embedded,
                    art_automatic: song.art_automatic.clone(),
                    art_manual: song.art_manual.clone(),
                    genre: song.genre.clone(),
                    sample_song_id: Some(song.id),
                    rating: crate::stats::RATING_UNRATED,
                    total_duration_nanosec: 0,
                },
            }
        }
        _ => HomeItem::Song {
            song: Box::new(song),
        },
    }
}

fn get_playlists_by_ids(
    conn: &rusqlite::Connection,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, Playlist>> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT p.id, p.name, p.dynamic_enabled, p.dynamic_spec, p.population_mode,
                p.last_played_row, p.created, p.updated,
                (SELECT COUNT(*) FROM playlist_items pi WHERE pi.playlist_id = p.id) as track_count
         FROM playlists p WHERE p.id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let map = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            let playlist = Playlist::from_row(row)?;
            Ok((playlist.id, playlist))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(map)
}

fn get_songs_by_ids(
    conn: &rusqlite::Connection,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, (Song, i64, i64)>> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let home_item_select_cols = home_item_select_cols();
    let sql = format!("SELECT {home_item_select_cols} FROM songs s WHERE s.id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let map = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            let song = row_to_song(row)?;
            let album_track_count: i64 = row.get(56)?;
            let album_disc_count: i64 = row.get(57)?;
            Ok((song.id, (song, album_track_count, album_disc_count)))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(map)
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
        if !candidate_tags.iter().any(|existing| std::ptr::eq(*existing, t)) {
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
            song.grouping = tag
                .get_string(ItemKey::ContentGroup)
                .map(|s| s.to_string());
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
                    title=excluded.title, artist=excluded.artist,
                    album=excluded.album, album_artist=excluded.album_artist,
                    track=excluded.track, disc=excluded.disc,
                    year=excluded.year, originalyear=excluded.originalyear, genre=excluded.genre,
                    compilation=excluded.compilation,
                    composer=excluded.composer, lyrics=excluded.lyrics,
                    grouping=excluded.grouping, bpm=excluded.bpm, initial_key=excluded.initial_key,
                    comment=excluded.comment, length_nanosec=excluded.length_nanosec,
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
                    unavailable=0",
            SONG_INSERT_COLS, SONG_INSERT_PLACEHOLDERS
        ),
        params![
            song.source as i32,
            song.filetype as i32,
            song.path,
            song.title,
            song.artist,
            song.album,
            song.album_artist,
            song.composer,
            song.lyrics,
            song.comment,
            song.track,
            song.disc,
            song.year,
            song.originalyear,
            song.genre,
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
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-value "contains this value" matching (artist click-through)
// ---------------------------------------------------------------------------

/// SQL `WHERE`-clause fragment testing whether `column_expr` (assumed to be
/// a `; `-delimited multi-value column like `artist`/`album_artist`, or a
/// `COALESCE`/`NULLIF` expression over one) contains `param` — bound via
/// `multi_value_contains_pattern` — as one of its individual values, not
/// merely as a substring. Both sides are wrapped in `;` boundary markers so
/// a value can't be matched by being a substring of an unrelated, longer
/// value in the same position (e.g. clicking artist "Stan" must not also
/// match a song credited only to "Stan Getz").
fn multi_value_contains_sql(column_expr: &str, param: &str) -> String {
    format!("(';' || REPLACE({column_expr}, '; ', ';') || ';') LIKE {param} ESCAPE '\\'")
}

/// Builds the bound LIKE pattern paired with `multi_value_contains_sql`,
/// escaping `value` so any literal `%`, `_`, or `\` in an artist/composer
/// name can't be misread as a LIKE wildcard.
fn multi_value_contains_pattern(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%;{escaped};%")
}

// ---------------------------------------------------------------------------
// Queue population mode (#120) — weighted-random selection fragments
// ---------------------------------------------------------------------------

/// Extra `WHERE`-clause fragment and `ORDER BY` expression implementing a
/// `QueuePopulationMode`'s bias. Spliced onto a base scope filter (genre,
/// decade, or a Smart Playlist's own filter query) by `get_songs_by_genre`,
/// `get_songs_by_decade`, and `search_songs_by_mode`. Binds no parameters of
/// its own, so it's safe to splice into either positional (`?N`) SQL.
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
            "",
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
    track, disc, year, originalyear, genre, compilation,
    bpm, initial_key,
    length_nanosec, beginning_nanosec, end_nanosec,
    bitrate, samplerate, bitdepth, channels, filesize, mtime,
    rating, playcount, skipcount, lastplayed, lastseen,
    art_embedded, art_automatic, art_manual, art_unset,
    cue_path,
    ebur128_integrated_loudness_lufs, ebur128_loudness_range_lu,
    unavailable,
    replaygain_track_gain, replaygain_album_gain,
    is_vbr, is_instrumental, added
";

/// Same columns as `SONG_SELECT_COLS`, in the same order, qualified with the
/// `s` alias for use in joined queries. Keep in sync with `SONG_SELECT_COLS`
/// and `row_to_song_at`'s field order — `cargo test` in `collection.rs`
/// exercises every column through `row_to_song`, so a mismatch fails loudly
/// there instead of silently defaulting a field at a call site.
pub(crate) const SONG_SELECT_COLS_QUALIFIED: &str = "s.id, s.source, s.filetype, s.path, s.url, s.stream_url,
    s.title, s.titlesort, s.artist, s.artistsort,
    s.album, s.albumsort, s.album_artist, s.album_artist_sort,
    s.composer, s.composersort, s.performer, s.performersort,
    s.grouping, s.comment, s.lyrics,
    s.track, s.disc, s.year, s.originalyear, s.genre, s.compilation,
    s.bpm, s.initial_key,
    s.length_nanosec, s.beginning_nanosec, s.end_nanosec,
    s.bitrate, s.samplerate, s.bitdepth, s.channels, s.filesize, s.mtime,
    s.rating, s.playcount, s.skipcount, s.lastplayed, s.lastseen,
    s.art_embedded, s.art_automatic, s.art_manual, s.art_unset,
    s.cue_path,
    s.ebur128_integrated_loudness_lufs, s.ebur128_loudness_range_lu,
    s.unavailable, s.replaygain_track_gain, s.replaygain_album_gain,
    s.is_vbr, s.is_instrumental, s.added";

/// `SONG_SELECT_COLS_QUALIFIED` plus correlated `album_track_count` and
/// `album_disc_count` subqueries. Shared by the home-screen queries
/// (`get_recently_played`, `get_most_frequently_played`, `get_recently_added`),
/// which all join on `songs s`.
fn home_item_select_cols() -> String {
    format!(
        "{SONG_SELECT_COLS_QUALIFIED},
    (SELECT COUNT(*) FROM songs s2
     WHERE s2.source IN (1, 2) AND s2.unavailable = 0 AND s2.album = s.album
    ) AS album_track_count,
    (SELECT COALESCE(MAX(COALESCE(s2.disc, 1)), 1) FROM songs s2
     WHERE s2.source IN (1, 2) AND s2.unavailable = 0 AND s2.album = s.album
    ) AS album_disc_count"
    )
}

const SONG_INSERT_COLS: &str = "
    source, filetype, path, title, artist, album, album_artist,
    composer, lyrics, comment, track, disc, year, originalyear, genre, compilation,
    grouping, bpm, initial_key,
    length_nanosec, bitrate, samplerate, channels, bitdepth,
    filesize, mtime, art_embedded, art_automatic, art_unset,
    replaygain_track_gain, replaygain_album_gain, is_vbr
";

const SONG_INSERT_PLACEHOLDERS: &str =
    "?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32";

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
        compilation: row.get(col(26))?,
        bpm: row.get(col(27))?,
        initial_key: row.get(col(28))?,
        length_nanosec: row.get(col(29))?,
        beginning_nanosec: row.get::<_, Option<i64>>(col(30))?.unwrap_or(0),
        end_nanosec: row.get::<_, Option<i64>>(col(31))?.unwrap_or(0),
        bitrate: row.get(col(32))?,
        samplerate: row.get(col(33))?,
        bitdepth: row.get(col(34))?,
        channels: row.get(col(35))?,
        filesize: row.get(col(36))?,
        mtime: row.get(col(37))?,
        rating: row.get::<_, Option<f32>>(col(38))?.unwrap_or(-1.0),
        playcount: row.get::<_, Option<i32>>(col(39))?.unwrap_or(0),
        skipcount: row.get::<_, Option<i32>>(col(40))?.unwrap_or(0),
        lastplayed: row.get(col(41))?,
        lastseen: row.get(col(42))?,
        art_embedded: row.get(col(43))?,
        art_automatic: row.get(col(44))?,
        art_manual: row.get(col(45))?,
        art_unset: row.get(col(46))?,
        cue_path: row.get(col(47))?,
        ebur128_integrated_loudness_lufs: row.get(col(48))?,
        ebur128_loudness_range_lu: row.get(col(49))?,
        unavailable: row.get::<_, Option<bool>>(col(50))?.unwrap_or(false),
        replaygain_track_gain: row.get(col(51))?,
        replaygain_album_gain: row.get(col(52))?,
        is_vbr: row.get(col(53))?,
        is_instrumental: row.get::<_, Option<bool>>(col(54))?.unwrap_or(false),
        added: row.get(col(55))?,
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// File Watcher & Deletion Sync
// ---------------------------------------------------------------------------

/// Resolves `path` to its real on-disk path when it differs only in case from
/// what's stored — e.g. a tag-driven rename shifted an album folder's casing.
/// `Path::exists()` treats a stale-cased path as a hit on a case-insensitive-
/// but-case-preserving filesystem (Windows, macOS), which is why this drift
/// only ever surfaces as a real playback failure on Linux/ext4. Returns
/// `None` if some component genuinely doesn't exist on disk under any case.
pub(crate) fn resolve_case_insensitive_path(path: &Path) -> Option<PathBuf> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => {
                // Don't shortcut on `current.join(name).exists()` — on
                // case-insensitive filesystems (Windows, default macOS) that
                // returns true even when `name`'s casing doesn't match what's
                // really on disk, which would defeat the whole point of this
                // function. Always resolve the real on-disk casing via
                // `read_dir`, preferring an exact match if one exists.
                if !current.is_dir() {
                    return None;
                }
                let target = name.to_string_lossy().to_lowercase();
                let mut case_insensitive_match = None;
                let real_name = std::fs::read_dir(&current)
                    .ok()?
                    .filter_map(|e| e.ok())
                    .find_map(|e| {
                        let entry_name = e.file_name();
                        if entry_name == name {
                            return Some(entry_name);
                        }
                        if case_insensitive_match.is_none()
                            && entry_name.to_string_lossy().to_lowercase() == target
                        {
                            case_insensitive_match = Some(entry_name);
                        }
                        None
                    })
                    .or(case_insensitive_match)?;
                current.push(real_name);
            }
            _ => current.push(component),
        }
    }
    Some(current)
}

/// Matches DB rows whose file has vanished from its recorded path ("orphans")
/// against freshly discovered files that have no DB row yet ("candidates"),
/// using filename + file size as the identity heuristic, and repoints the
/// orphan's `path`/`mtime` in place instead of leaving it for delete+reinsert.
///
/// This is what lets a folder move/split (including across two different
/// watched roots) survive a rescan without losing the song's id — and with
/// it, its play count, rating, and playlist membership. A key only
/// reconciles when exactly one orphan and exactly one candidate share it;
/// ambiguous matches (e.g. duplicate files) are left alone and fall back to
/// the normal insert/prune behavior.
fn reconcile_moved_songs(conn: &rusqlite::Connection, all_paths: &[PathBuf]) -> Result<usize> {
    let mut existing_paths_stmt = conn.prepare("SELECT path FROM songs WHERE path IS NOT NULL")?;
    let existing_paths: std::collections::HashSet<String> = existing_paths_stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();
    drop(existing_paths_stmt);

    // Exact-cased paths this scan actually found on disk. Used below instead of
    // `Path::exists()` — on a case-insensitive-but-case-preserving filesystem
    // (Windows, macOS), `exists()` on a stale-cased stored path still resolves
    // to the real file, so a case-only rename (e.g. an album folder renamed to
    // match a retagged album name) would never be recognized as needing a
    // repath, and the next scan would insert a second row for the same file
    // instead of updating the first.
    let disk_paths: std::collections::HashSet<String> = all_paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    let mut candidates: std::collections::HashMap<(String, i64), Vec<&PathBuf>> =
        std::collections::HashMap::new();
    for path in all_paths {
        if existing_paths.contains(&path.to_string_lossy().to_string()) {
            continue;
        }
        let (Some(filename), Ok(metadata)) = (
            path.file_name().map(|f| f.to_string_lossy().to_lowercase()),
            std::fs::metadata(path),
        ) else {
            continue;
        };
        candidates
            .entry((filename, metadata.len() as i64))
            .or_default()
            .push(path);
    }

    if candidates.is_empty() {
        return Ok(0);
    }

    let mut stmt = conn.prepare("SELECT id, path, filesize FROM songs WHERE path IS NOT NULL")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;

    let mut orphans: std::collections::HashMap<(String, i64), Vec<i64>> =
        std::collections::HashMap::new();
    for (id, path, filesize) in rows.flatten() {
        let Some(filesize) = filesize else { continue };
        let p = Path::new(&path);
        if disk_paths.contains(&path) {
            continue;
        }
        let Some(filename) = p.file_name().map(|f| f.to_string_lossy().to_lowercase()) else {
            continue;
        };
        orphans.entry((filename, filesize)).or_default().push(id);
    }

    let mut reconciled = 0usize;
    let mut update_stmt = conn.prepare("UPDATE songs SET path = ?1, mtime = ?2 WHERE id = ?3")?;
    for (key, orphan_ids) in orphans {
        if orphan_ids.len() != 1 {
            continue; // ambiguous — multiple missing songs share this filename+size
        }
        let Some(candidate_paths) = candidates.get(&key) else {
            continue;
        };
        if candidate_paths.len() != 1 {
            continue; // ambiguous — multiple new files share this filename+size
        }

        let new_path = candidate_paths[0];
        let new_path_str = new_path.to_string_lossy().to_string();
        let mtime = get_mtime(new_path).unwrap_or(0);
        update_stmt.execute(params![new_path_str, mtime, orphan_ids[0]])?;
        reconciled += 1;
    }

    Ok(reconciled)
}

/// Matches paths that disappeared ("removed") against paths that newly
/// appeared ("added") within the same watcher debounce window, using
/// filename + file size as the identity heuristic — the same heuristic
/// `reconcile_moved_songs` uses for a full rescan. A matched pair is
/// repointed via `UPDATE songs SET path/mtime` in place instead of being
/// deleted and reinserted, which is what preserves rating/playcount/
/// playlist membership when a file is moved between (or within) watched
/// folders while the app is running. Returns the paths that remain
/// unmatched, for the caller to fall back to normal delete/insert handling.
fn reconcile_watcher_batch(
    conn: &rusqlite::Connection,
    removed_paths: &[PathBuf],
    added_paths: &[PathBuf],
) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    if removed_paths.is_empty() || added_paths.is_empty() {
        return Ok((removed_paths.to_vec(), added_paths.to_vec()));
    }

    let mut candidates: std::collections::HashMap<(String, i64), Vec<&PathBuf>> =
        std::collections::HashMap::new();
    for path in added_paths {
        let (Some(filename), Ok(metadata)) = (
            path.file_name().map(|f| f.to_string_lossy().to_lowercase()),
            std::fs::metadata(path),
        ) else {
            continue;
        };
        candidates
            .entry((filename, metadata.len() as i64))
            .or_default()
            .push(path);
    }

    let mut orphans: std::collections::HashMap<(String, i64), Vec<(i64, &PathBuf)>> =
        std::collections::HashMap::new();
    for path in removed_paths {
        let path_str = path.to_string_lossy().to_string();
        let Some(filename) = path.file_name().map(|f| f.to_string_lossy().to_lowercase()) else {
            continue;
        };
        let row: Option<(i64, i64)> = conn
            .query_row(
                "SELECT id, filesize FROM songs WHERE path = ?1 AND filesize IS NOT NULL",
                params![path_str],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        if let Some((id, filesize)) = row {
            orphans
                .entry((filename, filesize))
                .or_default()
                .push((id, path));
        }
    }

    let mut matched_removed = std::collections::HashSet::new();
    let mut matched_added = std::collections::HashSet::new();
    let mut update_stmt = conn.prepare("UPDATE songs SET path = ?1, mtime = ?2 WHERE id = ?3")?;

    for (key, orphan_list) in &orphans {
        if orphan_list.len() != 1 {
            continue; // ambiguous — multiple missing songs share this filename+size
        }
        let Some(candidate_list) = candidates.get(key) else {
            continue;
        };
        if candidate_list.len() != 1 {
            continue; // ambiguous — multiple new files share this filename+size
        }

        let (song_id, old_path) = orphan_list[0];
        let new_path = candidate_list[0];
        let new_path_str = new_path.to_string_lossy().to_string();
        let mtime = get_mtime(new_path).unwrap_or(0);
        update_stmt.execute(params![new_path_str, mtime, song_id])?;
        matched_removed.insert(old_path);
        matched_added.insert(new_path);
    }

    let still_removed = removed_paths
        .iter()
        .filter(|p| !matched_removed.contains(p))
        .cloned()
        .collect();
    let still_added = added_paths
        .iter()
        .filter(|p| !matched_added.contains(p))
        .cloned()
        .collect();
    Ok((still_removed, still_added))
}

/// Helper to hard-delete a path and its subpaths from the SQLite database.
pub fn delete_path_and_subpaths(db: &Database, path_str: &str) -> Result<usize> {
    let conn = db.pool.get()?;
    let path_fw = path_str.replace('\\', "/");
    let path_bw = path_str.replace('/', "\\");

    let deleted = conn.execute(
        "DELETE FROM songs
         WHERE (
            path = ?1 OR path = ?2
            OR path LIKE ?1 || '/%'
            OR path LIKE ?1 || '\\%'
            OR path LIKE ?2 || '/%'
            OR path LIKE ?2 || '\\%'
         )",
        params![path_fw, path_bw],
    )?;
    if deleted > 0 {
        let _ = conn.execute("DELETE FROM playlist_items WHERE song_id IS NULL", []);
    }
    Ok(deleted)
}

/// Soft-delete counterpart to `delete_path_and_subpaths`, used when `path_str`
/// falls under a watched root that's currently unreachable (see
/// `unreachable_watched_roots`) — flags matching songs `unavailable` instead
/// of removing them, so a disconnected drive observed by the realtime watcher
/// can't be mistaken for a bulk deletion the way a plain hard-delete would.
fn mark_path_and_subpaths_unavailable(db: &Database, path_str: &str) -> Result<usize> {
    let conn = db.pool.get()?;
    let path_fw = path_str.replace('\\', "/");
    let path_bw = path_str.replace('/', "\\");

    let marked = conn.execute(
        "UPDATE songs SET unavailable = 1
         WHERE unavailable = 0 AND (
            path = ?1 OR path = ?2
            OR path LIKE ?1 || '/%'
            OR path LIKE ?1 || '\\%'
            OR path LIKE ?2 || '/%'
            OR path LIKE ?2 || '\\%'
         )",
        params![path_fw, path_bw],
    )?;
    Ok(marked)
}

/// Watched directory roots that can't currently be read (disconnected drive,
/// sleeping network share, etc). Shared by the scanner's missing-file check
/// (`CollectionScanner::find_missing_song_ids`) and the realtime watcher below
/// so a temporary disconnect is never treated as a bulk deletion by either
/// path.
fn unreachable_watched_roots(conn: &rusqlite::Connection) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT path FROM directories") {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for path in rows.flatten() {
                let p = PathBuf::from(&path);
                if std::fs::read_dir(&p).is_err() {
                    roots.push(p);
                }
            }
        }
    }
    roots
}

/// Message delivered from the notify callback to the watcher thread. A plain
/// `notify::Event` channel would silently lose whatever changes happened
/// during an `Err` (e.g. the OS's change-journal buffer overflowing during a
/// large bulk move) — `Overflow` makes that loss explicit and recoverable.
enum WatcherMsg {
    Event(notify::Event),
    Overflow,
}

/// Monotonic id distinguishing one debounced watcher batch from the next, so
/// the frontend can tell which `batch-processing-*` events belong together
/// (see #233).
static NEXT_BATCH_ID: AtomicU64 = AtomicU64::new(0);

/// Whether a raw `notify` event represents an actual filesystem mutation
/// worth waking the batch processor for. inotify's watch mask (see notify's
/// `inotify.rs`) includes OPEN/CLOSE so renames can be reported reliably,
/// but that also means every non-mutating read — a song opened for playback
/// or waveform decoding, a directory merely listed — surfaces as
/// `EventKind::Access`. Forwarding those made every play action look like an
/// external library edit and fire a spurious "song(s) updated" toast on
/// every single play.
fn is_mutating_watcher_event(kind: &notify::EventKind) -> bool {
    !matches!(kind, notify::EventKind::Access(_))
}

/// Start background directory watching using notify.
pub fn start_watcher(app: AppHandle, state: &crate::AppState) {
    let db = Arc::clone(&state.db);
    let app_clone = app.clone();

    let (tx, rx) = std::sync::mpsc::channel::<WatcherMsg>();

    let watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        match res {
            Ok(event) => {
                if is_mutating_watcher_event(&event.kind) {
                    let _ = tx.send(WatcherMsg::Event(event));
                }
            }
            Err(e) => {
                // The OS-level change buffer (e.g. Windows' ReadDirectoryChangesW)
                // has a fixed size and can overflow during a large bulk move
                // (splitting a library across folders). When that happens notify
                // reports an error instead of the individual events, so we can't
                // know what changed — fall back to a full rescan instead of
                // silently going stale.
                log::warn!("File watcher error (possible missed events): {e}");
                let _ = tx.send(WatcherMsg::Overflow);
            }
        }
    });

    let mut watcher = match watcher {
        Ok(w) => w,
        Err(e) => {
            log::error!("Failed to create file watcher: {e}");
            return;
        }
    };

    // Watch all monitored directories
    if let Ok(conn) = db.pool.get() {
        let mut stmt = match conn.prepare("SELECT path FROM directories") {
            Ok(s) => s,
            Err(_) => return,
        };
        let dirs = stmt.query_map([], |row| row.get::<_, String>(0));
        if let Ok(dirs) = dirs {
            for dir_path in dirs.flatten() {
                let p = PathBuf::from(dir_path);
                if p.exists() {
                    let _ = watcher.watch(&p, notify::RecursiveMode::Recursive);
                }
            }
        }
    }

    // Save the watcher inside AppState to keep it alive
    {
        let mut w_guard = state.watcher.lock();
        *w_guard = Some(watcher);
    }

    // Spawn the background thread to handle watcher events
    let db_for_thread = Arc::clone(&db);
    let watcher_paused = Arc::clone(&state.watcher_paused);
    std::thread::Builder::new()
        .name("luminous-watcher".to_string())
        .spawn(move || {
            let cover_manager = app_clone
                .path()
                .app_data_dir()
                .ok()
                .map(|dir| CoverManager::new(Arc::clone(&db_for_thread), dir));

            while let Ok(msg) = rx.recv() {
                if watcher_paused.load(std::sync::atomic::Ordering::Relaxed) > 0 {
                    continue;
                }

                // Check if real-time folder watching is disabled in app settings
                let realtime_enabled = if let Ok(conn) = db_for_thread.pool.get() {
                    conn.query_row(
                        "SELECT value FROM app_settings WHERE key = 'watch_folders_realtime'",
                        [],
                        |row| row.get::<_, String>(0),
                    )
                    .map(|v| v != "false")
                    .unwrap_or(true)
                } else {
                    true
                };

                if !realtime_enabled {
                    continue;
                }

                // A file move surfaces as a separate "removed" event for the old
                // path and "added" event for the new one — sometimes in the same
                // notify::Event, sometimes as two events in quick succession.
                // Collect everything that arrives within a short debounce window
                // so both halves of a move can be reconciled together below,
                // instead of being handled independently as a delete + a fresh
                // insert (which would reset rating/playcount/added-date on the
                // "new" row).
                //
                // The window slides: it resets on every new event instead of
                // expiring 400ms after the *first* one. A real folder copy or
                // extraction can spread its filesystem events over several
                // seconds (disk I/O, antivirus scanning, etc), and a fixed
                // deadline would chop that single logical operation into many
                // separate batches — each emitting its own "batch-processing-*"
                // events and its own "songs added" toast instead of one (#233).
                // `max_batch_duration` bounds the worst case so a folder that's
                // never quiet (e.g. continuously written to) still flushes
                // periodically instead of buffering forever.
                let mut batch = vec![msg];
                let debounce = std::time::Duration::from_millis(400);
                let max_batch_duration = std::time::Duration::from_secs(20);
                let batch_started_at = std::time::Instant::now();
                loop {
                    let elapsed = batch_started_at.elapsed();
                    if elapsed >= max_batch_duration {
                        break;
                    }
                    match rx.recv_timeout(debounce.min(max_batch_duration - elapsed)) {
                        Ok(next) => batch.push(next),
                        Err(_) => break,
                    }
                }

                if batch.iter().any(|m| matches!(m, WatcherMsg::Overflow)) {
                    // We don't know what changed — a full rescan (which also
                    // reconciles moved files) is the only reliable recovery.
                    log::warn!("Recovering from a watcher error with a full library rescan");
                    let scanner = CollectionScanner::new(Arc::clone(&db_for_thread));
                    let app_handle_scan = app_clone.clone();
                    tauri::async_runtime::block_on(async move {
                        let _ = scanner.scan_all(app_handle_scan.clone(), false, true).await;
                        let _ = app_handle_scan.emit("library-changed", ());
                    });
                    continue;
                }

                let mut removed_paths = std::collections::HashSet::new();
                let mut added_paths = std::collections::HashSet::new();
                let mut dir_paths = std::collections::HashSet::new();
                for msg in batch {
                    let WatcherMsg::Event(event) = msg else {
                        continue;
                    };
                    for path in event.paths {
                        if !path.exists() {
                            removed_paths.insert(path);
                        } else if path.is_file() && is_audio_file(&path) {
                            added_paths.insert(path);
                        } else if path.is_dir() {
                            dir_paths.insert(path);
                        }
                    }
                }
                let removed_paths: Vec<PathBuf> = removed_paths.into_iter().collect();
                let added_paths: Vec<PathBuf> = added_paths.into_iter().collect();

                if let Ok(conn) = db_for_thread.pool.get() {
                    let (still_removed, still_added) =
                        match reconcile_watcher_batch(&conn, &removed_paths, &added_paths) {
                            Ok(r) => r,
                            Err(e) => {
                                log::error!("Failed to reconcile watcher batch: {e}");
                                (removed_paths.clone(), added_paths.clone())
                            }
                        };

                    let reconciled_count = removed_paths.len() - still_removed.len();
                    if reconciled_count > 0 {
                        log::info!(
                            "Watcher reconciled {} moved song(s) in place (rating/playcount preserved)",
                            reconciled_count
                        );
                        let _ = app_clone.emit("library-changed", ());
                    }

                    // Emit a single batch-lifecycle notification covering every file this
                    // debounce window collected, instead of one signal per file — a
                    // multi-file drag-and-drop or folder import would otherwise "avalanche"
                    // the frontend toast queue (#233). `still_removed` can include
                    // non-audio companion paths (cover art, playlists, Thumbs.db) and bare
                    // directory paths swept up by a recursive move/delete — only count
                    // actual audio files toward the "songs" total so those don't inflate
                    // it (`still_added` is already audio-only, per `added_paths`' filter).
                    let removed_song_count = still_removed.iter().filter(|p| is_audio_file(p)).count();
                    let total_count = removed_song_count + still_added.len();
                    let batch_id = NEXT_BATCH_ID.fetch_add(1, Ordering::Relaxed);
                    let mut processed_count = 0usize;
                    if total_count > 0 {
                        let _ = app_clone.emit(
                            "batch-processing-started",
                            BatchProgress {
                                batch_id,
                                current_count: 0,
                                total_count,
                                phase: BatchPhase::Removing,
                            },
                        );
                    }

                    // A disconnected drive makes `notify` report every path under it as
                    // "removed" the same as an actual deletion would. Songs under a root
                    // that's currently unreachable get soft-flagged instead of
                    // hard-deleted here, mirroring the scan-time protection in
                    // `find_missing_song_ids` — otherwise unplugging a watched USB drive
                    // mid-session would permanently wipe every song on it from the library.
                    let unreachable_roots = unreachable_watched_roots(&conn);

                    for path in &still_removed {
                        let path_str = path.to_string_lossy().to_string();
                        let is_song = is_audio_file(path);

                        if unreachable_roots.iter().any(|root| path.starts_with(root)) {
                            log::warn!(
                                "Watcher saw '{}' disappear, but its watched root is currently \
                                 unreachable (disconnected drive or network share?) — marking \
                                 unavailable instead of deleting",
                                path_str
                            );
                            match mark_path_and_subpaths_unavailable(&db_for_thread, &path_str) {
                                Ok(marked) => {
                                    if marked > 0 {
                                        log::info!(
                                            "Marked {} song(s) unavailable under unreachable root",
                                            marked
                                        );
                                        let _ = app_clone.emit("library-changed", ());
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to mark path unavailable in db: {e}");
                                }
                            }
                            if is_song {
                                processed_count += 1;
                                let _ = app_clone.emit(
                                    "batch-processing-progress",
                                    BatchProgress {
                                        batch_id,
                                        current_count: processed_count,
                                        total_count,
                                        phase: BatchPhase::Removing,
                                    },
                                );
                            }
                            continue;
                        }

                        log::info!("Watcher detected deletion: {}", path_str);
                        match delete_path_and_subpaths(&db_for_thread, &path_str) {
                            Ok(deleted) => {
                                if deleted > 0 {
                                    log::info!("Pruned {} deleted song(s) from db", deleted);
                                    let _ = app_clone.emit("library-changed", ());
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to delete path from db: {e}");
                            }
                        }
                        if !is_song {
                            continue;
                        }
                        processed_count += 1;
                        let _ = app_clone.emit(
                            "batch-processing-progress",
                            BatchProgress {
                                batch_id,
                                current_count: processed_count,
                                total_count,
                                phase: BatchPhase::Removing,
                            },
                        );
                    }

                    for path in &still_added {
                        log::info!(
                            "Watcher detected file addition/change: {}",
                            path.to_string_lossy()
                        );
                        let result = match &cover_manager {
                            Some(cm) => read_and_upsert_song(&conn, cm, path),
                            None => read_tags(path).and_then(|song| upsert_song(&conn, &song)),
                        };
                        if result.is_ok() {
                            let _ = app_clone.emit("library-changed", ());
                        }
                        processed_count += 1;
                        let _ = app_clone.emit(
                            "batch-processing-progress",
                            BatchProgress {
                                batch_id,
                                current_count: processed_count,
                                total_count,
                                phase: BatchPhase::Adding,
                            },
                        );
                    }

                    if total_count > 0 {
                        let _ = app_clone.emit(
                            "batch-processing-completed",
                            BatchProgress {
                                batch_id,
                                current_count: total_count,
                                total_count,
                                phase: BatchPhase::Done,
                            },
                        );
                    }
                }

                // `scan_all` already walks every watched directory in one pass, so a
                // batch with several new/changed subdirectories (e.g. dropping a folder
                // of albums, each its own subdir) must trigger it only once — looping
                // per-directory here re-scanned the whole library once per subfolder,
                // each emitting its own `scan-progress` "done" phase and thus its own
                // "X tracks added" toast (#233). It's marked `silent` because the
                // still_added/still_removed handling above (or the per-file watcher
                // events feeding it) already produced its own batch-processing toast
                // for this same directory change — this rescan is just a safety-net
                // catch-up for files the granular per-file watcher may have missed,
                // not a separate user-facing event.
                if !dir_paths.is_empty() {
                    log::info!(
                        "Watcher detected {} director{} added/changed",
                        dir_paths.len(),
                        if dir_paths.len() == 1 { "y" } else { "ies" }
                    );
                    let scanner = CollectionScanner::new(Arc::clone(&db_for_thread));
                    let app_handle_scan = app_clone.clone();
                    tauri::async_runtime::block_on(async move {
                        let _ = scanner.scan_all(app_handle_scan.clone(), false, true).await;
                        let _ = app_handle_scan.emit("library-changed", ());
                    });
                }
            }
        })
        .expect("failed to spawn watcher thread");
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
    fn test_multi_value_contains_pattern_escapes_like_wildcards() {
        // An artist/composer name containing a literal '%' or '_' must not
        // be misread as a LIKE wildcard when embedded in the pattern.
        assert_eq!(multi_value_contains_pattern("50%"), "%;50\\%;%");
        assert_eq!(
            multi_value_contains_pattern("Under_score"),
            "%;Under\\_score;%"
        );
        assert_eq!(
            multi_value_contains_pattern(r"back\slash"),
            r"%;back\\slash;%"
        );
        // The common case: no wildcard characters, just wrapped in ';'.
        assert_eq!(multi_value_contains_pattern("Evergrey"), "%;Evergrey;%");
    }

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
            "Title",
            "Artist",
            "Album",
            "",
            "",
            "Rock; Jazz Fusion; Live",
            None,
            None,
            None,
            "",
            None,
            "",
            false,
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
            "Title",
            "Artist A; Artist B",
            "Album",
            "Album Artist A; Album Artist B",
            "Composer A; Composer B",
            "",
            None,
            None,
            None,
            "",
            None,
            "",
            false,
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
            "Title",
            "Artist",
            "Album",
            "",
            "",
            "",
            None,
            None,
            Some(1995),
            "",
            None,
            "",
            false,
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
            "Title",
            "Artist",
            "Album",
            "",
            "",
            "",
            None,
            None,
            Some(1995),
            "",
            None,
            "",
            false,
        )
        .expect("write_tags should succeed");
        crate::tageditor::write_tags(
            &path, "Title", "Artist", "Album", "", "", "", None, None, None, "", None, "", false,
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
    fn test_watcher_ignores_non_mutating_access_events() {
        use notify::event::{AccessKind, AccessMode};
        use notify::EventKind;

        assert!(!is_mutating_watcher_event(&EventKind::Access(
            AccessKind::Open(AccessMode::Read)
        )));
        assert!(!is_mutating_watcher_event(&EventKind::Access(
            AccessKind::Close(AccessMode::Read)
        )));
        assert!(!is_mutating_watcher_event(&EventKind::Access(
            AccessKind::Any
        )));
    }

    #[test]
    fn test_watcher_forwards_real_change_events() {
        use notify::event::{CreateKind, ModifyKind, RemoveKind};
        use notify::EventKind;

        assert!(is_mutating_watcher_event(&EventKind::Create(
            CreateKind::File
        )));
        assert!(is_mutating_watcher_event(&EventKind::Modify(
            ModifyKind::Any
        )));
        assert!(is_mutating_watcher_event(&EventKind::Remove(
            RemoveKind::File
        )));
        assert!(is_mutating_watcher_event(&EventKind::Any));
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
    fn test_get_albums_artist_resolution() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_coll_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str,
                           title: &str,
                           artist: Option<&str>,
                           album: Option<&str>,
                           album_artist: Option<&str>| {
            let song = Song {
                path: Some(path.to_string()),
                title: Some(title.to_string()),
                artist: artist.map(|s| s.to_string()),
                album: album.map(|s| s.to_string()),
                album_artist: album_artist.map(|s| s.to_string()),
                source: SongSource::LocalFile,
                filetype: FileType::Mp3,
                unavailable: false,
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        };

        // Scenario 1: Album where all tracks have the same artist, and album_artist is None
        insert_song(
            "path/1.mp3",
            "Track 1",
            Some("Artist A"),
            Some("Album One"),
            None,
        );
        insert_song(
            "path/2.mp3",
            "Track 2",
            Some("Artist A"),
            Some("Album One"),
            None,
        );

        // Scenario 2: Album with different artists, and album_artist is None (Various Artists fallback)
        insert_song(
            "path/3.mp3",
            "Track 3",
            Some("Artist B"),
            Some("Album Two"),
            None,
        );
        insert_song(
            "path/4.mp3",
            "Track 4",
            Some("Artist C"),
            Some("Album Two"),
            None,
        );

        // Scenario 3: Album where all tracks have same album_artist but different track artists
        insert_song(
            "path/5.mp3",
            "Track 5",
            Some("Artist B"),
            Some("Album Three"),
            Some("Artist A"),
        );
        insert_song(
            "path/6.mp3",
            "Track 6",
            Some("Artist C"),
            Some("Album Three"),
            Some("Artist A"),
        );

        // Scenario 4: Album where tracks have different album_artists
        insert_song(
            "path/7.mp3",
            "Track 7",
            Some("Artist X"),
            Some("Album Four"),
            Some("Artist Y"),
        );
        insert_song(
            "path/8.mp3",
            "Track 8",
            Some("Artist Z"),
            Some("Album Four"),
            Some("Artist W"),
        );

        let albums = scanner.get_albums().unwrap();

        let find_album = |name: &str| -> &serde_json::Value {
            albums
                .iter()
                .find(|a| a["album"].as_str() == Some(name))
                .unwrap()
        };

        // Assert Album One -> album_artist is "Artist A"
        let album_one = find_album("Album One");
        assert_eq!(album_one["artist"].as_str(), Some("Artist A"));
        assert_eq!(album_one["track_count"].as_i64(), Some(2));
        assert!(album_one["added"].is_number());

        // Assert Album Two -> album_artist is None (will fall back to Various Artists in UI)
        let album_two = find_album("Album Two");
        assert_eq!(album_two["artist"].as_str(), None);
        assert_eq!(album_two["track_count"].as_i64(), Some(2));

        // Assert Album Three -> album_artist is "Artist A"
        let album_three = find_album("Album Three");
        assert_eq!(album_three["artist"].as_str(), Some("Artist A"));
        assert_eq!(album_three["track_count"].as_i64(), Some(2));

        // Assert Album Four -> album_artist is None (Various Artists fallback)
        let album_four = find_album("Album Four");
        assert_eq!(album_four["artist"].as_str(), None);
        assert_eq!(album_four["track_count"].as_i64(), Some(2));

        let _ = std::fs::remove_dir_all(temp_dir);
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
    fn test_get_compilations_by_artist() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_compilations_by_artist_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str,
                           artist: &str,
                           album: &str,
                           album_artist: Option<&str>,
                           compilation: bool| {
            let song = Song {
                path: Some(path.to_string()),
                title: Some(path.to_string()),
                artist: Some(artist.to_string()),
                album: Some(album.to_string()),
                album_artist: album_artist.map(|s| s.to_string()),
                source: SongSource::LocalFile,
                filetype: FileType::Mp3,
                unavailable: false,
                compilation,
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        };

        // A properly tagged compilation: TCMP=1, shared "Various Artists" album_artist.
        insert_song(
            "path/comp1.mp3",
            "Artist A",
            "Compilation One",
            Some("Various Artists"),
            true,
        );
        insert_song(
            "path/comp2.mp3",
            "Artist B",
            "Compilation One",
            Some("Various Artists"),
            true,
        );

        // A compilation identifiable only by disagreeing album_artist (no TCMP).
        insert_song("path/comp3.mp3", "Artist A", "Compilation Two", None, false);
        insert_song("path/comp4.mp3", "Artist C", "Compilation Two", None, false);

        // Artist A's own solo album should NOT show up as a compilation.
        insert_song(
            "path/solo1.mp3",
            "Artist A",
            "Solo Album",
            Some("Artist A"),
            false,
        );
        insert_song(
            "path/solo2.mp3",
            "Artist A",
            "Solo Album",
            Some("Artist A"),
            false,
        );

        let comps = scanner.get_compilations_by_artist("Artist A").unwrap();
        let names: Vec<&str> = comps.iter().map(|a| a["album"].as_str().unwrap()).collect();
        assert!(names.contains(&"Compilation One"));
        assert!(names.contains(&"Compilation Two"));
        assert!(!names.contains(&"Solo Album"));
        assert_eq!(comps.len(), 2);

        for comp in &comps {
            assert_eq!(comp["artist"].as_str(), Some("Various Artists"));
        }

        // Artist B only appears on Compilation One.
        let comps_b = scanner.get_compilations_by_artist("Artist B").unwrap();
        assert_eq!(comps_b.len(), 1);
        assert_eq!(comps_b[0]["album"].as_str(), Some("Compilation One"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_delete_path_and_subpaths_windows_separators() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_prune_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let insert_test_song = |p: &str| {
            let song = Song {
                path: Some(p.to_string()),
                title: Some("Test Track".to_string()),
                source: SongSource::LocalFile,
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        };

        insert_test_song(r"C:\Music\ArtistName\AlbumOne\song1.mp3");
        insert_test_song(r"C:\Music\ArtistName\AlbumOne\song2.mp3");
        insert_test_song(r"C:\Music\OtherArtist\song3.mp3");

        let pruned = delete_path_and_subpaths(&db, r"C:\Music\ArtistName").unwrap();
        assert_eq!(pruned, 2);

        let scanner = CollectionScanner::new(db.clone());
        let songs = scanner.get_songs(100, 0).unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(
            songs[0].path.as_deref(),
            Some(r"C:\Music\OtherArtist\song3.mp3")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_artists_album_count_filtering() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_artist_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        // Artist A: Single with 1 track (track_count <= 7) -> should count as 0 albums
        let song_a = Song {
            artist: Some("Artist Single".to_string()),
            album: Some("Single Album".to_string()),
            title: Some("Single Track".to_string()),
            source: SongSource::LocalFile,
            path: Some(r"C:\Music\Artist Single\single.mp3".to_string()),
            ..Default::default()
        };
        upsert_song(&conn, &song_a).unwrap();

        // Artist B: Full Album with 8 tracks (track_count > 7) -> should count as 1 album
        for i in 1..=8 {
            let song_b = Song {
                artist: Some("Artist Full".to_string()),
                album: Some("Full Album".to_string()),
                title: Some(format!("Track {}", i)),
                source: SongSource::LocalFile,
                path: Some(format!(r"C:\Music\Artist Full\track{}.mp3", i)),
                ..Default::default()
            };
            upsert_song(&conn, &song_b).unwrap();
        }

        let scanner = CollectionScanner::new(db.clone());
        let artists = scanner.get_artists().unwrap();

        let single_artist = artists
            .iter()
            .find(|a| a["name"].as_str() == Some("Artist Single"))
            .unwrap();
        assert_eq!(single_artist["album_count"].as_i64(), Some(0));
        assert_eq!(single_artist["song_count"].as_i64(), Some(1));

        let full_artist = artists
            .iter()
            .find(|a| a["name"].as_str() == Some("Artist Full"))
            .unwrap();
        assert_eq!(full_artist["album_count"].as_i64(), Some(1));
        assert_eq!(full_artist["song_count"].as_i64(), Some(8));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression test for #295: songs whose artist tag only differs in case
    /// (e.g. from albums organized/tagged at different times) must be merged
    /// into a single artist entry rather than shown as two separate artists.
    #[test]
    fn test_get_artists_merges_case_only_variants() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_artist_case_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                artist: Some("The War on Drugs".to_string()),
                album: Some("Lost in the Dream".to_string()),
                title: Some("Under the Pressure".to_string()),
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\The War on Drugs\track1.mp3".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        upsert_song(
            &conn,
            &Song {
                artist: Some("The War On Drugs".to_string()),
                album: Some("A Deeper Understanding".to_string()),
                title: Some("Holding On".to_string()),
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\The War On Drugs\track2.mp3".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());
        let artists = scanner.get_artists().unwrap();

        let matches: Vec<&serde_json::Value> = artists
            .iter()
            .filter(|a| {
                a["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case("the war on drugs"))
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "expected a single merged artist entry, got {:?}",
            artists
        );
        assert_eq!(matches[0]["song_count"].as_i64(), Some(2));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression test for #362: a song with no artist/album_artist tags at
    /// all (both NULL) must still be reachable via the same "effective
    /// artist" value that get_artists() groups it under — previously
    /// get_artists() grouped NULL artists together as SQL NULL while
    /// get_songs_by_artist() only matched against `''`, so NULL never
    /// equaled `''` and the click-through returned zero songs.
    #[test]
    fn test_untagged_song_reachable_via_get_artists_grouping() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_untagged_artist_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                artist: None,
                album_artist: None,
                album: None,
                title: None,
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\untagged.ogg".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());
        let artists = scanner.get_artists().unwrap();

        let untagged = artists
            .iter()
            .find(|a| a["name"].as_str() == Some(""))
            .expect("untagged song should surface as an empty-string artist entry");
        assert_eq!(untagged["song_count"].as_i64(), Some(1));

        let songs = scanner.get_songs_by_artist("").unwrap();
        assert_eq!(
            songs.len(),
            1,
            "get_songs_by_artist(\"\") must return the song grouped under the empty-string artist"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression test for the artist click-through gap found while testing
    /// #150: `get_songs_by_artist` used to match with exact equality against
    /// a single "effective" column (album_artist, falling back to artist),
    /// so a collab track credited to artist = "Evergrey; Mikael Stanne" with
    /// album_artist = "Evergrey" was unreachable by clicking "Mikael
    /// Stanne" — the effective column resolved to just "Evergrey" and never
    /// mentioned him at all, regardless of how the artist column was split.
    /// Checking the raw `artist` column too (not only the COALESCE'd
    /// effective one) is what actually fixes this.
    #[test]
    fn test_get_songs_by_artist_finds_individual_values_in_a_multi_artist_credit() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_multi_artist_click_through_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                artist: Some("Evergrey; Mikael Stanne".to_string()),
                album_artist: Some("Evergrey".to_string()),
                album: Some("Architects Of A New Weave".to_string()),
                title: Some("A Burning Flame".to_string()),
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\a_burning_flame.flac".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        // A solo Evergrey track, to confirm the match isn't overly broad.
        upsert_song(
            &conn,
            &Song {
                artist: Some("Evergrey".to_string()),
                album_artist: None,
                album: Some("Escape Of The Phoenix".to_string()),
                title: Some("Where August Mourns".to_string()),
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\where_august_mourns.flac".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());

        let evergrey_songs = scanner.get_songs_by_artist("Evergrey").unwrap();
        assert_eq!(
            evergrey_songs.len(),
            2,
            "clicking Evergrey must surface both the solo track and the collab track"
        );

        let stanne_songs = scanner.get_songs_by_artist("Mikael Stanne").unwrap();
        assert_eq!(
            stanne_songs.len(),
            1,
            "clicking Mikael Stanne must surface the collab track"
        );
        assert_eq!(stanne_songs[0].title.as_deref(), Some("A Burning Flame"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// A Various Artists compilation track (album_artist = "Various
    /// Artists", per-track artist = the actual performer) must be reachable
    /// by clicking the individual track artist, not just via the separate
    /// `get_compilations_by_artist` album-card query.
    #[test]
    fn test_get_songs_by_artist_finds_various_artists_compilation_track() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_compilation_click_through_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                artist: Some("Artist X".to_string()),
                album_artist: Some("Various Artists".to_string()),
                album: Some("Now That's What I Call Tests".to_string()),
                title: Some("Track One".to_string()),
                compilation: true,
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\track_one.flac".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());
        let songs = scanner.get_songs_by_artist("Artist X").unwrap();
        assert_eq!(
            songs.len(),
            1,
            "clicking a compilation track's own artist must surface it directly"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// A value that's a substring of a different, unrelated artist's full
    /// name must not false-positive match — clicking "Stan" (if that were a
    /// real credited artist) must not also surface a song credited only to
    /// "Stan Getz".
    #[test]
    fn test_get_songs_by_artist_does_not_match_substrings_of_unrelated_names() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_artist_substring_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                artist: Some("Stan Getz".to_string()),
                album_artist: None,
                album: Some("Getz/Gilberto".to_string()),
                title: Some("The Girl From Ipanema".to_string()),
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\girl_from_ipanema.flac".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());
        let songs = scanner.get_songs_by_artist("Stan").unwrap();
        assert_eq!(
            songs.len(),
            0,
            "\"Stan\" must not match \"Stan Getz\" as a substring"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_top_artists_ranks_by_playcount_and_excludes_zero_plays() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_top_artists_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        // upsert_song() deliberately never touches playcount (it's owned by
        // the stats.rs write path, preserved across rescans) — so seed songs
        // via upsert_song(), then set playcount directly.
        let seed = |path: &str, artist: &str, title: &str, playcount: i32| {
            upsert_song(
                &conn,
                &Song {
                    artist: Some(artist.to_string()),
                    title: Some(title.to_string()),
                    source: SongSource::LocalFile,
                    path: Some(path.to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
            conn.execute(
                "UPDATE songs SET playcount = ?1 WHERE path = ?2",
                params![playcount, path],
            )
            .unwrap();
        };

        // Artist Low: one song, played twice.
        seed(r"C:\Music\Artist Low\low.mp3", "Artist Low", "Low Track", 2);

        // Artist High: two songs, playcounts sum to 10.
        seed(
            r"C:\Music\Artist High\a.mp3",
            "Artist High",
            "High Track A",
            6,
        );
        seed(
            r"C:\Music\Artist High\b.mp3",
            "Artist High",
            "High Track B",
            4,
        );

        // Artist Unplayed: never played, must be excluded entirely.
        seed(
            r"C:\Music\Artist Unplayed\track.mp3",
            "Artist Unplayed",
            "Unplayed Track",
            0,
        );

        let scanner = CollectionScanner::new(db.clone());
        let top_artists = scanner.get_top_artists(10).unwrap();

        let names: Vec<&str> = top_artists
            .iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["Artist High", "Artist Low"]);

        let high = &top_artists[0];
        assert_eq!(high["song_count"].as_i64(), Some(2));

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
    fn test_get_library_decades_and_songs() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_decade_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let s1 = Song {
            title: Some("80s Song".to_string()),
            year: Some(1984),
            source: SongSource::LocalFile,
            path: Some("/music/80s.mp3".to_string()),
            ..Default::default()
        };
        let s2 = Song {
            title: Some("90s Song".to_string()),
            originalyear: Some(1995),
            source: SongSource::LocalFile,
            path: Some("/music/90s.mp3".to_string()),
            ..Default::default()
        };
        upsert_song(&conn, &s1).unwrap();
        upsert_song(&conn, &s2).unwrap();

        let scanner = CollectionScanner::new(db);
        let decades = scanner.get_library_decades().unwrap();
        assert_eq!(decades, vec!["1980s".to_string(), "1990s".to_string()]);

        let songs_80s = scanner
            .get_songs_by_decade("1980s", 10, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(songs_80s.len(), 1);
        assert_eq!(songs_80s[0].title.as_deref(), Some("80s Song"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Verifies each `QueuePopulationMode`'s WHERE-clause bias (see #120)
    /// selects the correct subset of songs. Ordering is randomized by
    /// design, so this only asserts set membership, not order.
    #[test]
    fn test_get_songs_by_genre_population_modes() {
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

        let scanner = CollectionScanner::new(db);

        fn titles(mut songs: Vec<Song>) -> Vec<String> {
            let mut out: Vec<String> = songs.drain(..).filter_map(|s| s.title).collect();
            out.sort();
            out
        }

        let all = scanner
            .get_songs_by_genre(genre, 10, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(
            titles(all),
            vec!["DeepCut", "Discover", "Familiar", "Fav", "Neutral"]
        );

        let familiar = scanner
            .get_songs_by_genre(genre, 10, QueuePopulationMode::Familiar)
            .unwrap();
        assert_eq!(
            titles(familiar),
            vec!["DeepCut", "Discover", "Familiar", "Fav", "Neutral"],
            "Familiar biases order via weighting, not filtering"
        );

        let favourites = scanner
            .get_songs_by_genre(genre, 10, QueuePopulationMode::Favourites)
            .unwrap();
        assert_eq!(titles(favourites), vec!["Fav"]);

        let discover = scanner
            .get_songs_by_genre(genre, 10, QueuePopulationMode::Discover)
            .unwrap();
        assert_eq!(titles(discover), vec!["Discover", "Familiar", "Neutral"]);

        let deep_cuts = scanner
            .get_songs_by_genre(genre, 10, QueuePopulationMode::DeepCuts)
            .unwrap();
        assert_eq!(titles(deep_cuts), vec!["DeepCut", "Fav"]);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_recently_played_groups_by_play_context() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_recent_played_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str, title: &str, album: Option<&str>| {
            let song = Song {
                path: Some(path.to_string()),
                title: Some(title.to_string()),
                artist: Some("Artist".to_string()),
                album: album.map(|s| s.to_string()),
                album_artist: album.map(|_| "Artist".to_string()),
                source: SongSource::LocalFile,
                filetype: FileType::Mp3,
                unavailable: false,
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
            conn.query_row("SELECT id FROM songs WHERE path = ?1", params![path], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        };

        let standalone_id = insert_song("path/standalone.mp3", "Standalone", None);
        let album_track_1 = insert_song("path/album_a1.mp3", "Album Track 1", Some("Album A"));
        let album_track_2 = insert_song("path/album_a2.mp3", "Album Track 2", Some("Album A"));
        let playlist_track = insert_song("path/playlist_track.mp3", "Playlist Track", None);

        conn.execute(
            "INSERT INTO playlists (name) VALUES ('My Playlist')",
            params![],
        )
        .unwrap();
        let playlist_id = conn.last_insert_rowid();

        // played_at ascending: standalone (oldest) -> two album plays -> playlist play (newest)
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('song', ?1, NULL, 100)",
            params![standalone_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('album', ?1, NULL, 200)",
            params![album_track_1],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('album', ?1, NULL, 201)",
            params![album_track_2],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('playlist', ?1, ?2, 300)",
            params![playlist_track, playlist_id],
        )
        .unwrap();

        let recent = scanner.get_recently_played(10).unwrap();
        assert_eq!(recent.len(), 3, "album plays should collapse into one card");
        match &recent[0] {
            HomeItem::Playlist { playlist } => assert_eq!(playlist.id, playlist_id),
            other => panic!("expected Playlist as most recent, got {other:?}"),
        }
        match &recent[1] {
            HomeItem::Album { album } => assert_eq!(album.album.as_deref(), Some("Album A")),
            other => panic!("expected Album next, got {other:?}"),
        }
        match &recent[2] {
            HomeItem::Song { song } => assert_eq!(song.id, standalone_id),
            other => panic!("expected standalone Song last, got {other:?}"),
        }

        // Most frequently played: play the standalone song two more times so it
        // outranks the (2-play) album and (1-play) playlist contexts.
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('song', ?1, NULL, 400)",
            params![standalone_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('song', ?1, NULL, 500)",
            params![standalone_id],
        )
        .unwrap();

        let frequent = scanner.get_most_frequently_played(10).unwrap();
        assert_eq!(frequent.len(), 3);
        match &frequent[0] {
            HomeItem::Song { song } => assert_eq!(song.id, standalone_id),
            other => panic!("expected most-played standalone Song first, got {other:?}"),
        }

        // Test exclusion of internal 'Queue' playlist from recently played & most frequently played
        conn.execute(
            "INSERT INTO playlists (name, dynamic_enabled) VALUES ('Queue', 0)",
            params![],
        )
        .unwrap();
        let queue_playlist_id = conn.last_insert_rowid();

        // Play from Queue playlist with high played_at, and many times
        for i in 0..10 {
            conn.execute(
                "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('playlist', ?1, ?2, ?3)",
                params![playlist_track, queue_playlist_id, 1000 + i],
            )
            .unwrap();
        }

        // Verify that 'Queue' does not show up in recently played (still length 3)
        let recent_after_queue = scanner.get_recently_played(10).unwrap();
        assert_eq!(recent_after_queue.len(), 3);
        for item in &recent_after_queue {
            if let HomeItem::Playlist { playlist } = item {
                assert_ne!(playlist.id, queue_playlist_id);
            }
        }

        // Verify that 'Queue' does not show up in most frequently played (still length 3)
        let frequent_after_queue = scanner.get_most_frequently_played(10).unwrap();
        assert_eq!(frequent_after_queue.len(), 3);
        for item in &frequent_after_queue {
            if let HomeItem::Playlist { playlist } = item {
                assert_ne!(playlist.id, queue_playlist_id);
            }
        }

        // Verify that plays with explicit album context are attributed to their album
        for i in 0..15 {
            conn.execute(
                "INSERT INTO play_history (context_type, song_id, played_at) VALUES ('album', ?1, ?2)",
                params![album_track_1, 2000 + i],
            )
            .unwrap();
        }
        let frequent_after_album = scanner.get_most_frequently_played(10).unwrap();
        assert!(frequent_after_album.iter().any(|item| matches!(item, HomeItem::Album { album, .. } if album.album.as_deref() == Some("Album A"))));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_recently_added_collapses_album_with_varying_track_artists() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_rec_added_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str, title: &str, artist: &str, album: &str| -> i64 {
            let song = Song {
                source: SongSource::LocalFile,
                path: Some(path.to_string()),
                title: Some(title.to_string()),
                artist: Some(artist.to_string()),
                album: Some(album.to_string()),
                album_artist: None,
                added: Some(1000),
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
            conn.query_row("SELECT id FROM songs WHERE path = ?1", params![path], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        };

        // 7 tracks with Artist A, 3 tracks with Artist B (total 10 tracks)
        for i in 1..=7 {
            insert_song(
                &format!("path/track_{i}.mp3"),
                &format!("Track {i}"),
                "Artist A",
                "Mixed Album",
            );
        }
        for i in 8..=10 {
            insert_song(
                &format!("path/track_{i}.mp3"),
                &format!("Track {i}"),
                "Artist B",
                "Mixed Album",
            );
        }

        let items = scanner.get_recently_added(10).unwrap();
        assert_eq!(
            items.len(),
            1,
            "all 10 tracks should collapse into a single album card"
        );
        match &items[0] {
            HomeItem::Album { album } => {
                assert_eq!(album.album.as_deref(), Some("Mixed Album"));
                assert_eq!(
                    album.track_count, 10,
                    "should count all 10 tracks, not split by artist"
                );
            }
            other => panic!("expected Album item, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_recently_added_surfaces_existing_album_rating() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_rec_added_rating_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        // Needs 2+ tracks — a single-track "album" renders as a Song item, not
        // an Album item (see group_songs_into_home_items's album_track_count > 1 check).
        for i in 1..=2 {
            let song = Song {
                source: SongSource::LocalFile,
                path: Some(format!("path/rated_{i}.mp3")),
                title: Some(format!("Rated Track {i}")),
                artist: Some("Artist A".to_string()),
                album: Some("Rated Album".to_string()),
                album_artist: None,
                added: Some(1000),
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        }

        crate::stats::set_album_rating(&conn, "Rated Album", 4.5).unwrap();

        let items = scanner.get_recently_added(10).unwrap();
        match &items[0] {
            HomeItem::Album { album } => {
                assert_eq!(album.album.as_deref(), Some("Rated Album"));
                assert_eq!(
                    album.rating, 4.5,
                    "should surface the rating set via set_album_rating, not default to unrated"
                );
            }
            other => panic!("expected Album item, got {other:?}"),
        }

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
    fn test_reconcile_moved_songs_repaths_unique_match() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_reconcile_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let old_dir = temp_dir.join("old");
        let new_dir = temp_dir.join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        // The file now lives only at `new_path` — `old_path` is what the DB
        // still has recorded, simulating a folder moved to a different
        // watched root between scans.
        let old_path = old_dir.join("track.mp3");
        let new_path = new_dir.join("track.mp3");
        let content = b"pretend audio bytes";
        std::fs::write(&new_path, content).unwrap();

        let song = Song {
            path: Some(old_path.to_string_lossy().to_string()),
            title: Some("Moved Track".to_string()),
            source: SongSource::LocalFile,
            filesize: Some(content.len() as i64),
            ..Default::default()
        };
        upsert_song(&conn, &song).unwrap();
        let original_id: i64 = conn
            .query_row("SELECT id FROM songs", [], |r| r.get(0))
            .unwrap();
        // upsert_song() never touches playcount (it's only mutated via
        // stats::record_play) — set it directly to prove reconciliation's
        // UPDATE preserves stats tied to the id instead of resetting them.
        conn.execute(
            "UPDATE songs SET playcount = 7 WHERE id = ?1",
            params![original_id],
        )
        .unwrap();

        let reconciled = reconcile_moved_songs(&conn, std::slice::from_ref(&new_path)).unwrap();
        assert_eq!(reconciled, 1);

        let (id, path, playcount): (i64, String, i32) = conn
            .query_row("SELECT id, path, playcount FROM songs", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(id, original_id, "repath must preserve the song's id");
        assert_eq!(path, new_path.to_string_lossy().to_string());
        assert_eq!(
            playcount, 7,
            "repath must not reset stats tied to the song id"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    #[cfg(any(windows, target_os = "macos"))]
    fn test_reconcile_moved_songs_detects_case_only_rename() {
        // Regression test for a duplicate-song bug: renaming an album folder's
        // case only (e.g. "HERO" -> "Hero") on a case-insensitive-but-case-preserving
        // filesystem used to leave the stale-cased DB row undetected as an orphan,
        // because `Path::exists()` on the stale-cased path still resolved to the
        // real (renamed) file. The next scan would then insert a second row for
        // the same physical file instead of repathing the first.
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_reconcile_case_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let album_dir = temp_dir.join("Hero");
        std::fs::create_dir_all(&album_dir).unwrap();

        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let real_path = album_dir.join("track.mp3");
        let content = b"pretend audio bytes";
        std::fs::write(&real_path, content).unwrap();

        // The stale, differently-cased path the DB still has recorded.
        let stale_path = temp_dir.join("HERO").join("track.mp3");
        assert!(
            stale_path.exists(),
            "sanity check: this test requires a case-insensitive filesystem"
        );

        let song = Song {
            path: Some(stale_path.to_string_lossy().to_string()),
            title: Some("Sugar".to_string()),
            source: SongSource::LocalFile,
            filesize: Some(content.len() as i64),
            ..Default::default()
        };
        upsert_song(&conn, &song).unwrap();
        let original_id: i64 = conn
            .query_row("SELECT id FROM songs", [], |r| r.get(0))
            .unwrap();

        let reconciled = reconcile_moved_songs(&conn, std::slice::from_ref(&real_path)).unwrap();
        assert_eq!(
            reconciled, 1,
            "a case-only rename must be recognized as a repath, not left stale"
        );

        let (id, path): (i64, String) = conn
            .query_row("SELECT id, path FROM songs", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(id, original_id, "repath must preserve the song's id");
        assert_eq!(path, real_path.to_string_lossy().to_string());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_resolve_case_insensitive_path_finds_real_casing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_resolve_case_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let album_dir = temp_dir.join("HERO");
        std::fs::create_dir_all(&album_dir).unwrap();
        let real_path = album_dir.join("Track.mp3");
        std::fs::write(&real_path, b"pretend audio bytes").unwrap();

        // Same path but with the stale casing a stored DB row might have.
        let stale_path = temp_dir.join("Hero").join("track.mp3");

        let resolved = resolve_case_insensitive_path(&stale_path)
            .expect("a case-only mismatch should still resolve to the real file");
        assert_eq!(resolved, real_path);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_resolve_case_insensitive_path_none_when_truly_missing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_resolve_case_missing_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let missing_path = temp_dir.join("Nonexistent").join("track.mp3");
        assert!(resolve_case_insensitive_path(&missing_path).is_none());

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
    fn test_reconcile_moved_songs_skips_ambiguous_matches() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_reconcile_ambiguous_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let old_dir = temp_dir.join("old");
        let new_dir_a = temp_dir.join("new_a");
        let new_dir_b = temp_dir.join("new_b");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir_a).unwrap();
        std::fs::create_dir_all(&new_dir_b).unwrap();

        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        // Two missing rows and two newly discovered files all share the same
        // filename + size — there's no way to know which orphan maps to
        // which candidate, so reconciliation must leave both alone.
        let content = b"duplicate track bytes";
        let new_path_a = new_dir_a.join("track.mp3");
        let new_path_b = new_dir_b.join("track.mp3");
        std::fs::write(&new_path_a, content).unwrap();
        std::fs::write(&new_path_b, content).unwrap();

        for i in 0..2 {
            let song = Song {
                path: Some(
                    old_dir
                        .join(format!("orphan{i}/track.mp3"))
                        .to_string_lossy()
                        .to_string(),
                ),
                title: Some(format!("Orphan {i}")),
                source: SongSource::LocalFile,
                filesize: Some(content.len() as i64),
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        }

        let reconciled =
            reconcile_moved_songs(&conn, &[new_path_a.clone(), new_path_b.clone()]).unwrap();
        assert_eq!(
            reconciled, 0,
            "ambiguous filename+size matches must not be guessed at"
        );

        let mut stmt = conn
            .prepare("SELECT path FROM songs ORDER BY path")
            .unwrap();
        let paths: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(
            paths
                .iter()
                .all(|p| p.starts_with(&old_dir.to_string_lossy().to_string())),
            "orphan rows must be untouched, got {paths:?}"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_artist_profile_crud() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_artist_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        let conn = db.pool.get().unwrap();

        // Initially unconfigured artist returns default profile
        let initial = get_artist_profile_conn(&conn, "Shania Twain").unwrap();
        assert_eq!(initial.artist_key, "Shania Twain");
        assert_eq!(initial.website, None);
        assert!(initial.tags.is_empty());
        assert!(initial.social_links.is_empty());
        assert_eq!(initial.bio, None);

        // Save profile
        let profile = ArtistProfile {
            artist_key: "Shania Twain".to_string(),
            website: Some("https://www.shaniatwain.com".to_string()),
            tags: vec![
                "pop".to_string(),
                "country".to_string(),
                "canadian".to_string(),
            ],
            social_links: vec![
                ArtistSocialLink {
                    platform: "instagram".to_string(),
                    handle_or_url: "@shaniatwain".to_string(),
                },
                ArtistSocialLink {
                    platform: "youtube".to_string(),
                    handle_or_url: "https://youtube.com/@ShaniaTwain".to_string(),
                },
            ],
            bio: Some("Canadian singer-songwriter".to_string()),
        };

        set_artist_profile_conn(&conn, &profile).unwrap();

        // Retrieve saved profile (case-insensitive key match)
        let loaded = get_artist_profile_conn(&conn, "shania twain").unwrap();
        assert_eq!(loaded.artist_key, "Shania Twain");
        assert_eq!(loaded.website, Some("https://www.shaniatwain.com".to_string()));
        assert_eq!(loaded.tags, vec!["pop", "country", "canadian"]);
        assert_eq!(loaded.social_links.len(), 2);
        assert_eq!(loaded.social_links[0].platform, "instagram");
        assert_eq!(loaded.social_links[0].handle_or_url, "@shaniatwain");
        assert_eq!(loaded.bio, Some("Canadian singer-songwriter".to_string()));

        // Get all profiles
        let all = get_all_artist_profiles_conn(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].artist_key, "Shania Twain");

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
