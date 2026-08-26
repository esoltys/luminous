//! Realtime filesystem watcher — watches the library's directories for
//! changes while the app is running and reconciles them into the DB, as a
//! companion to `collection`'s explicit `scan_all`. Split out of
//! `collection.rs` (#577 item 17).

use crate::{
    covermanager::CoverManager,
    db::Database,
    models::{BatchPhase, BatchProgress},
};
use anyhow::Result;
use notify::Watcher;
use rusqlite::params;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc,
    },
};
use tauri::{AppHandle, Emitter, Manager};

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
        let mtime = super::get_mtime(new_path).unwrap_or(0);
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
                    let scanner = super::CollectionScanner::new(Arc::clone(&db_for_thread));
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
                        } else if path.is_file() && super::is_audio_file(&path) {
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
                    let removed_song_count =
                        still_removed.iter().filter(|p| super::is_audio_file(p)).count();
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
                    let unreachable_roots = super::reconcile::unreachable_watched_roots(&conn);

                    for path in &still_removed {
                        let path_str = path.to_string_lossy().to_string();
                        let is_song = super::is_audio_file(path);

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
                            Some(cm) => super::read_and_upsert_song(&conn, cm, path),
                            None => super::read_tags(path).and_then(|song| super::upsert_song(&conn, &song)),
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
                    let scanner = super::CollectionScanner::new(Arc::clone(&db_for_thread));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Song, SongSource};
    use std::sync::Arc;

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
            super::super::upsert_song(&conn, &song).unwrap();
        };

        insert_test_song(r"C:\Music\ArtistName\AlbumOne\song1.mp3");
        insert_test_song(r"C:\Music\ArtistName\AlbumOne\song2.mp3");
        insert_test_song(r"C:\Music\OtherArtist\song3.mp3");

        let pruned = delete_path_and_subpaths(&db, r"C:\Music\ArtistName").unwrap();
        assert_eq!(pruned, 2);

        let scanner = super::super::CollectionScanner::new(db.clone());
        let songs = scanner.get_songs(100, 0).unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(
            songs[0].path.as_deref(),
            Some(r"C:\Music\OtherArtist\song3.mp3")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
