//! Tauri IPC commands for remote WebDAV server management and synchronization (#682).

use crate::covermanager::CoverManager;
use crate::db::Database;
use crate::models::{SongSource, WebDavServer, WebDavSyncStats};
use crate::remote_scheduler::{RemoteKind, SyncGuard};
use crate::webdav::{detect_filetype_from_url, WebDavClient};
use crate::AppState;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Progress update emitted during WebDAV library synchronization (#1087).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDavSyncProgressPayload {
    pub server_id: i64,
    pub server_name: String,
    pub current_path: String,
    pub current_count: usize,
    pub added: usize,
    pub updated: usize,
    pub errors: usize,
    pub done: bool,
    /// True while an auto-sync is doing its once-a-day full listing (#1483).
    pub daily_check: bool,
}

/// List all configured WebDAV servers.
#[tauri::command]
pub async fn list_webdav_servers(state: State<'_, AppState>) -> Result<Vec<WebDavServer>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, url, username, remote_path, enabled, sync_status, last_synced_at, created_at, nickname, icon, color, auto_sync_enabled, sync_interval_minutes
             FROM webdav_servers
             ORDER BY created_at ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(WebDavServer {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                username: row.get(3)?,
                password: None,
                remote_path: row.get(4)?,
                enabled: row.get(5)?,
                sync_status: row.get(6)?,
                last_synced_at: row.get(7)?,
                created_at: row.get(8)?,
                nickname: row.get(9)?,
                icon: row.get(10)?,
                color: row.get(11)?,
                auto_sync_enabled: row.get(12)?,
                sync_interval_minutes: row.get(13)?,
                next_auto_sync_at: None,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut servers = Vec::new();
    for mut s in rows.flatten() {
        s.next_auto_sync_at = state.remote_auto_sync.next_run_at(RemoteKind::WebDav, s.id);
        servers.push(s);
    }
    Ok(servers)
}

/// Fields for [`save_webdav_server`], bundled into one struct so the command
/// doesn't take a clippy-flagged number of individual arguments.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWebDavServerInput {
    pub id: Option<i64>,
    pub name: String,
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub remote_path: Option<String>,
    pub enabled: Option<bool>,
    pub nickname: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub auto_sync_enabled: Option<bool>,
    pub sync_interval_minutes: Option<i64>,
}

const WEBDAV_SERVER_COLUMNS: &str = "id, name, url, username, remote_path, enabled, sync_status, last_synced_at, created_at, nickname, icon, color, auto_sync_enabled, sync_interval_minutes";

fn row_to_webdav_server(row: &rusqlite::Row) -> rusqlite::Result<WebDavServer> {
    Ok(WebDavServer {
        id: row.get(0)?,
        name: row.get(1)?,
        url: row.get(2)?,
        username: row.get(3)?,
        password: None,
        remote_path: row.get(4)?,
        enabled: row.get(5)?,
        sync_status: row.get(6)?,
        last_synced_at: row.get(7)?,
        created_at: row.get(8)?,
        nickname: row.get(9)?,
        icon: row.get(10)?,
        color: row.get(11)?,
        auto_sync_enabled: row.get(12)?,
        sync_interval_minutes: row.get(13)?,
        next_auto_sync_at: None,
    })
}

/// Save (create or update) a WebDAV server profile.
#[tauri::command]
pub async fn save_webdav_server(
    input: SaveWebDavServerInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WebDavServer, String> {
    let SaveWebDavServerInput {
        id,
        name,
        url,
        username,
        password,
        remote_path,
        enabled,
        nickname,
        icon,
        color,
        auto_sync_enabled,
        sync_interval_minutes,
    } = input;
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let remote_path_val = remote_path.unwrap_or_else(|| "/".to_string());
    let enabled_val = enabled.unwrap_or(true);
    let auto_sync_enabled_val = auto_sync_enabled.unwrap_or(false);
    let sync_interval_minutes_val = sync_interval_minutes.unwrap_or(60).max(1);

    let mut saved = if let Some(server_id) = id {
        if let Some(pass) = password {
            conn.execute(
                "UPDATE webdav_servers
                 SET name = ?1, url = ?2, username = ?3, password = ?4, remote_path = ?5, enabled = ?6,
                     nickname = ?7, icon = ?8, color = ?9, auto_sync_enabled = ?10, sync_interval_minutes = ?11
                 WHERE id = ?12",
                params![
                    name, url, username, pass, remote_path_val, enabled_val, nickname, icon, color,
                    auto_sync_enabled_val, sync_interval_minutes_val, server_id
                ],
            )
            .map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "UPDATE webdav_servers
                 SET name = ?1, url = ?2, username = ?3, remote_path = ?4, enabled = ?5,
                     nickname = ?6, icon = ?7, color = ?8, auto_sync_enabled = ?9, sync_interval_minutes = ?10
                 WHERE id = ?11",
                params![
                    name,
                    url,
                    username,
                    remote_path_val,
                    enabled_val,
                    nickname,
                    icon,
                    color,
                    auto_sync_enabled_val,
                    sync_interval_minutes_val,
                    server_id
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        // A changed URL, credentials or remote path invalidates what the
        // folder etags were recorded against: the next sync lists everything.
        let _ = conn.execute(
            "DELETE FROM webdav_dir_cache WHERE server_id = ?1",
            params![server_id],
        );

        conn.query_row(
            &format!("SELECT {WEBDAV_SERVER_COLUMNS} FROM webdav_servers WHERE id = ?1"),
            params![server_id],
            row_to_webdav_server,
        )
        .map_err(|e| e.to_string())?
    } else {
        conn.execute(
            "INSERT INTO webdav_servers (name, url, username, password, remote_path, enabled, nickname, icon, color, auto_sync_enabled, sync_interval_minutes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                name, url, username, password, remote_path_val, enabled_val, nickname, icon, color,
                auto_sync_enabled_val, sync_interval_minutes_val
            ],
        )
        .map_err(|e| e.to_string())?;

        let new_id = conn.last_insert_rowid();
        conn.query_row(
            &format!("SELECT {WEBDAV_SERVER_COLUMNS} FROM webdav_servers WHERE id = ?1"),
            params![new_id],
            row_to_webdav_server,
        )
        .map_err(|e| e.to_string())?
    };

    // Reschedule (or cancel) this server's auto-sync timer to reflect the
    // settings just saved — takes effect immediately, no app restart needed.
    if saved.enabled && saved.auto_sync_enabled {
        state.remote_auto_sync.reschedule(
            app,
            Arc::clone(&state.db),
            Arc::clone(&state.cover_manager),
            RemoteKind::WebDav,
            saved.id,
            saved.sync_interval_minutes,
        );
    } else {
        state.remote_auto_sync.cancel(RemoteKind::WebDav, saved.id);
    }
    saved.next_auto_sync_at = state
        .remote_auto_sync
        .next_run_at(RemoteKind::WebDav, saved.id);

    Ok(saved)
}

/// Delete a WebDAV server profile and its associated cache.
/// Marks associated songs unavailable (soft-deleted), mirroring `remove_directory`
/// for watched folders. The explicit "Clean Up" button permanently removes them.
#[tauri::command]
pub async fn delete_webdav_server(
    id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    {
        let mut song_ids: Vec<i64> = {
            let mut stmt = tx
                .prepare(
                    "SELECT song_id FROM webdav_cache WHERE server_id = ?1 AND song_id IS NOT NULL",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![id], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            rows.flatten().collect()
        };

        let server_url: Option<String> = tx
            .query_row(
                "SELECT url FROM webdav_servers WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .ok();

        if let Some(ref url) = server_url {
            let mut stmt = tx
                .prepare(&format!(
                    "SELECT id, path FROM songs WHERE source = {} AND unavailable = 0 AND path IS NOT NULL",
                    SongSource::WEBDAV_ID
                ))
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| e.to_string())?;
            for (song_id, path) in rows.flatten() {
                if crate::collection::song_matches_webdav_server(&path, url)
                    && !song_ids.contains(&song_id)
                {
                    song_ids.push(song_id);
                }
            }
        }

        if !song_ids.is_empty() {
            let mut upd = tx
                .prepare("UPDATE songs SET unavailable = 1 WHERE id = ?1")
                .map_err(|e| e.to_string())?;
            for song_id in song_ids {
                let _ = upd.execute(params![song_id]);
            }
        }

        tx.execute("DELETE FROM webdav_cache WHERE server_id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM webdav_servers WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    state.remote_auto_sync.cancel(RemoteKind::WebDav, id);
    let _ = app.emit("library-changed", ());
    Ok(())
}

/// Test connection to a remote WebDAV server without saving.
#[tauri::command]
pub async fn test_webdav_connection(
    url: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        let client = WebDavClient::new(url, username, password).map_err(|e| e.to_string())?;
        client.test_connection().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Live-checks reachability of an already-saved server, using its stored
/// credentials — the WebDAV counterpart to how a watched folder's
/// `is_available` is recomputed from `Path::exists()` on every fetch (#682's
/// settings redesign). Unlike a local path check this is a network call, so
/// the frontend runs it asynchronously per-server rather than blocking the
/// server list on it.
#[tauri::command]
pub async fn check_webdav_connection(id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.pool.get().map_err(|e| e.to_string())?;
        let (url, username, password): (String, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT url, username, password FROM webdav_servers WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| e.to_string())?;
        let client = WebDavClient::new(url, username, password).map_err(|e| e.to_string())?;
        client.test_connection().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Synchronize a WebDAV server into the library.
#[tauri::command]
pub async fn sync_webdav_server(
    id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WebDavSyncStats, String> {
    // "Sync Now" is the user asking for a thorough check: list every folder.
    sync_webdav_server_inner(id, app, state.db.clone(), state.cover_manager.clone(), true).await
}

/// Core sync routine shared by the [`sync_webdav_server`] command (manual
/// "Sync Now" clicks) and `remote_scheduler::AutoSyncScheduler` (periodic
/// auto-sync, #1082) — the scheduler runs as a background task with only an
/// `AppHandle` and `Arc<Database>`/`Arc<CoverManager>`, not a `State<AppState>`.
///
/// `thorough` lists every folder. Otherwise (auto-sync) a folder whose etag is
/// unchanged is skipped, but a full listing is still forced once a day: that
/// bounds how long a server whose folder etags don't reflect changes deeper in
/// the tree (not every WebDAV server does) can hide them.
pub async fn sync_webdav_server_inner(
    id: i64,
    app: AppHandle,
    db: Arc<Database>,
    cover_manager: Arc<CoverManager>,
    thorough: bool,
) -> Result<WebDavSyncStats, String> {
    let app_clone = app.clone();
    let guard = SyncGuard::acquire(RemoteKind::WebDav, id)?;

    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        let conn = db.pool.get().map_err(|e| e.to_string())?;

        // Retrieve server credentials & config
        let (server_name, url, username, password, remote_path): (String, String, Option<String>, Option<String>, String) = conn
            .query_row(
                "SELECT name, url, username, password, remote_path FROM webdav_servers WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .map_err(|e| e.to_string())?;

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let last_full_listing: Option<i64> = conn
            .query_row(
                "SELECT last_full_listing_at FROM webdav_servers WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        let may_skip_folders = !thorough
            && last_full_listing.is_some_and(|t| now_secs - t < FULL_LISTING_INTERVAL_SECS);
        // Saved either way; only consulted for skipping when allowed.
        let saved_dir_etags = load_dir_cache(&conn, id).unwrap_or_default();
        let dir_cache = if may_skip_folders {
            saved_dir_etags.clone()
        } else {
            HashMap::new()
        };
        // An auto-sync that lists everything only because the daily interval
        // lapsed, on a server that has folder etags to skip with: say so in the
        // progress label so a slow run isn't mistaken for the new normal.
        let daily_check =
            !thorough && !may_skip_folders && last_full_listing.is_some() && !saved_dir_etags.is_empty();

        // Update status to 'syncing'
        let _ = conn.execute(
            "UPDATE webdav_servers SET sync_status = 'syncing' WHERE id = ?1",
            params![id],
        );

        let _ = app_clone.emit(
            "webdav-sync-progress",
            WebDavSyncProgressPayload {
                server_id: id,
                server_name: server_name.clone(),
                current_path: remote_path.clone(),
                current_count: 0,
                added: 0,
                updated: 0,
                errors: 0,
                done: false,
                daily_check,
            },
        );

        let client = match WebDavClient::new(url.clone(), username, password) {
            Ok(c) => c,
            Err(e) => {
                let _ = conn.execute(
                    "UPDATE webdav_servers SET sync_status = 'idle' WHERE id = ?1",
                    params![id],
                );
                return Err(e.to_string());
            }
        };

        // Everything the previous syncs recorded for this server, loaded once
        // rather than queried per file (#1483).
        let cache = match load_remote_cache(&conn, id) {
            Ok(c) => c,
            Err(e) => {
                let _ = conn.execute(
                    "UPDATE webdav_servers SET sync_status = 'idle' WHERE id = ?1",
                    params![id],
                );
                return Err(e.to_string());
            }
        };

        // Folders whose etag matched the last complete sync: not listed again.
        let mut skipped_dirs: HashSet<String> = HashSet::new();
        // Etags seen this sync, saved if the sync completes.
        let mut new_dir_etags: HashMap<String, String> = HashMap::new();

        let mut stats = WebDavSyncStats::default();
        let mut current_count = 0usize;
        let mut progress = ProgressThrottle::new(daily_check);
        // Audio files the server listed this sync, for remote-deletion detection.
        let mut seen: HashSet<String> = HashSet::new();
        // Set false by any failed listing or write: a partial view of the
        // server must never be read as "these files were deleted".
        let mut sync_complete = true;
        // Timing for the diagnostics export (#1482): where a slow sync spends its time.
        let sync_started = std::time::Instant::now();
        let mut list_time = std::time::Duration::ZERO;
        let mut probe_time = std::time::Duration::ZERO;
        let mut dirs_listed = 0usize;
        let mut files_probed = 0usize;
        // Files that failed to probe or store, for the diagnostics export (#1495).
        let mut failures: Vec<(String, String)> = Vec::new();

        // Walk the tree one depth level at a time so each level's directories
        // are listed concurrently (bounded) instead of one PROPFIND at a time.
        let mut level = vec![remote_path];
        while !level.is_empty() {
            let list_started = std::time::Instant::now();
            let listings = run_bounded(level, LIST_CONCURRENCY, |path| {
                let listing = client.list_directory(&path);
                (path, listing)
            });
            list_time += list_started.elapsed();
            let mut next_level = Vec::new();

            for (current_path, listing) in listings {
                dirs_listed += 1;
                let items = match listing {
                    Ok(it) => it,
                    Err(err) => {
                        log::warn!("Failed to list WebDAV directory {current_path}: {err}");
                        stats.errors += 1;
                        sync_complete = false;
                        continue;
                    }
                };

                // Standalone folder-art image (`album.png`, `cover.jpg`, etc.)
                // for this directory, if any — the WebDAV counterpart to
                // `CoverManager::scan_folder_art`'s local-filesystem `read_dir`
                // scan, resolved from this directory's own PROPFIND listing
                // instead since there's no filesystem to scan (#1082 follow-up).
                // Downloaded lazily (only if some song in the directory actually
                // needs it) and at most once per directory, since every song
                // here shares the same folder image.
                let folder_art_item = items
                    .iter()
                    .find(|it| {
                        !it.is_directory
                            && std::path::Path::new(&it.href)
                                .file_stem()
                                .zip(std::path::Path::new(&it.href).extension())
                                .map(|(stem, ext)| {
                                    CoverManager::is_folder_art_filename(
                                        &stem.to_string_lossy(),
                                        &ext.to_string_lossy(),
                                    )
                                })
                                .unwrap_or(false)
                    })
                    .cloned();

                // Classify the directory's files. Unchanged ones only need a
                // (rare) row fix-up; new or changed ones go to the probe list.
                let mut to_probe: Vec<ProbeTask> = Vec::new();
                let mut fixups: Vec<(i64, String)> = Vec::new();
                for item in items {
                    // Avoid infinite loops matching the directory itself
                    if crate::webdav::is_listed_collection(&item.href, &current_path) {
                        continue;
                    }
                    if item.is_directory {
                        // A server that reports a folder etag changing whenever
                        // anything beneath it changes lets an unchanged folder
                        // be skipped without listing it. No etag, no skip.
                        if let Some(etag) = &item.etag {
                            let key = item.href.trim_end_matches('/').to_string();
                            let unchanged = dir_cache.get(&key) == Some(etag);
                            new_dir_etags.insert(key.clone(), etag.clone());
                            if unchanged {
                                skipped_dirs.insert(key);
                                continue;
                            }
                        }
                        next_level.push(item.href);
                        continue;
                    }
                    if detect_filetype_from_url(&item.href) == crate::models::FileType::Unknown {
                        continue;
                    }
                    seen.insert(item.href.clone());

                    let file_size = item.content_length.unwrap_or(0);
                    // Stored as path/url/stream_url. Credential-free: playback looks the
                    // server's credentials up when it opens the track (#1492).
                    let playback_url = client.playback_url(&item.href);

                    match cache.get(&item.href) {
                        Some(cached) if !remote_file_changed(cached, &item) => {
                            // The remote file itself is unchanged, so skip re-probing tags —
                            // but the stored playback URL may still be stale (e.g. the
                            // server's URL changed since) and the song may have been flagged
                            // unavailable while the file was missing. Repair both, so a rescan
                            // fixes previously-synced songs, not just new ones.
                            let path_stale = cached.song_path.as_deref() != Some(playback_url.as_str());
                            if path_stale || cached.unavailable {
                                if path_stale {
                                    stats.updated += 1;
                                }
                                fixups.push((cached.song_id, playback_url));
                            }
                            current_count += 1;
                            progress.maybe_emit(&app_clone, id, &server_name, &item.href, current_count, &stats);
                        }
                        _ => to_probe.push(ProbeTask {
                            // Used for internal probing (Authorization header set explicitly by the client).
                            probe_url: client.build_url(&item.href),
                            item,
                            file_size,
                            playback_url,
                        }),
                    }
                }

                // Probe remote tags using byte ranges, concurrently.
                let probe_started = std::time::Instant::now();
                files_probed += to_probe.len();
                let probed = run_bounded(to_probe, PROBE_CONCURRENCY, |task| {
                    let result = client.probe_song_tags(&task.probe_url, task.file_size);
                    (task, result)
                });
                probe_time += probe_started.elapsed();

                // No embedded-picture extraction over WebDAV yet, so
                // `art_automatic` is always still unset: fall back to this
                // directory's folder-art image, same as a local scan's
                // `scan_folder_art` fallback (#1082 follow-up). Fetched before
                // the write transaction opens so the network never holds the
                // database's write lock.
                let mut folder_art_bytes: Option<Vec<u8>> = None;
                if let Some(art_item) = &folder_art_item {
                    let needs_art = probed
                        .iter()
                        .any(|(_, r)| matches!(r, Ok(song) if song.art_automatic.is_none()));
                    if needs_art {
                        let art_url = client.build_url(&art_item.href);
                        match client.fetch_full(&art_url) {
                            Ok(bytes) => folder_art_bytes = Some(bytes),
                            Err(e) => log::warn!(
                                "Failed to download WebDAV folder art {}: {e}",
                                art_item.href
                            ),
                        }
                    }
                }

                // One transaction per directory instead of an autocommit per row.
                let tx = match conn.unchecked_transaction() {
                    Ok(tx) => tx,
                    Err(e) => {
                        log::warn!("Failed to open a WebDAV sync transaction for {current_path}: {e}");
                        stats.errors += 1;
                        sync_complete = false;
                        continue;
                    }
                };

                for (song_id, playback_url) in &fixups {
                    let _ = tx.execute(
                        "UPDATE songs SET path = ?1, url = ?1, stream_url = ?1, unavailable = 0 WHERE id = ?2",
                        params![playback_url, song_id],
                    );
                }

                for (task, result) in probed {
                    let ProbeTask { item, file_size, playback_url, .. } = task;
                    match result {
                        Ok(mut song) => {
                            song.path = Some(playback_url.clone());
                            song.url = Some(playback_url.clone());
                            song.stream_url = Some(playback_url.clone());

                            if song.art_automatic.is_none() {
                                if let Some(bytes) = &folder_art_bytes {
                                    let artist = song
                                        .album_artist
                                        .clone()
                                        .filter(|a| !a.trim().is_empty())
                                        .or_else(|| song.artist.clone())
                                        .unwrap_or_default();
                                    let album = song
                                        .album
                                        .clone()
                                        .filter(|a| !a.trim().is_empty())
                                        .or_else(|| song.title.clone())
                                        .unwrap_or_default();
                                    match cover_manager.cache_art_bytes(&artist, &album, bytes) {
                                        Ok(filename) => song.art_automatic = Some(filename),
                                        Err(e) => log::warn!(
                                            "Failed to cache WebDAV folder art for {}: {e}",
                                            item.href
                                        ),
                                    }
                                }
                            }

                            if let Err(e) = crate::collection::upsert_song(&tx, &song) {
                                log::warn!("Failed to upsert WebDAV song {}: {e}", item.href);
                                failures.push((item.href.clone(), format!("could not store song: {e}")));
                                stats.errors += 1;
                                continue;
                            }

                            let song_id: i64 = tx
                                .query_row(
                                    "SELECT id FROM songs WHERE path = ?1",
                                    params![playback_url],
                                    |r| r.get(0),
                                )
                                .unwrap_or(0);

                            let _ = tx.execute(
                                "INSERT INTO webdav_cache (server_id, remote_path, etag, size, last_modified, song_id)
                                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                                 ON CONFLICT(server_id, remote_path) DO UPDATE SET
                                   etag = excluded.etag,
                                   size = excluded.size,
                                   last_modified = excluded.last_modified,
                                   song_id = excluded.song_id,
                                   cached_at = (strftime('%s', 'now'))",
                                params![id, item.href, item.etag, file_size as i64, item.last_modified, song_id],
                            );

                            if cache.contains_key(&item.href) {
                                stats.updated += 1;
                            } else {
                                stats.added += 1;
                            }
                        }
                        Err(err) => {
                            log::warn!("Failed to probe WebDAV file {}: {err}", item.href);
                            failures.push((item.href.clone(), format!("tag probe failed: {err}")));
                            stats.errors += 1;
                        }
                    }

                    current_count += 1;
                    progress.maybe_emit(&app_clone, id, &server_name, &item.href, current_count, &stats);
                }

                if let Err(e) = tx.commit() {
                    log::warn!("Failed to commit WebDAV sync of {current_path}: {e}");
                    stats.errors += 1;
                    sync_complete = false;
                }
            }

            level = next_level;
        }

        // Files inside skipped folders weren't listed but are known from the
        // last sync: count them as seen so they aren't mistaken for deletions.
        if !skipped_dirs.is_empty() {
            for href in cache.keys() {
                if is_under_any(href, &skipped_dirs) {
                    seen.insert(href.clone());
                    current_count += 1;
                }
            }
        }
        if sync_complete {
            if let Err(e) =
                save_dir_cache(&conn, id, &saved_dir_etags, &new_dir_etags, &skipped_dirs)
            {
                log::warn!("Failed to save WebDAV folder etags: {e}");
            }
            if skipped_dirs.is_empty() {
                let _ = conn.execute(
                    "UPDATE webdav_servers SET last_full_listing_at = ?1 WHERE id = ?2",
                    params![now_secs, id],
                );
            }
        }

        // Files the server no longer lists: flag their songs unavailable (like a
        // local scan does for missing files) so they stop showing as playable.
        // Only after a sync that listed every directory and every write landed;
        // a server that answered with no audio at all is treated as a failed
        // listing rather than as "everything was deleted".
        if sync_complete && !seen.is_empty() {
            match mark_remote_deletions(&conn, &cache, &seen) {
                Ok(removed) => stats.removed = removed,
                Err(e) => log::warn!("Failed to flag deleted WebDAV songs: {e}"),
            }
        } else if !sync_complete {
            log::warn!("WebDAV sync of '{server_name}' was incomplete; not checking for remote deletions");
        }

        // Update server status to idle and update last_synced_at timestamp
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let _ = conn.execute(
            "UPDATE webdav_servers SET sync_status = 'idle', last_synced_at = ?1 WHERE id = ?2",
            params![now_ts, id],
        );

        let _ = app_clone.emit(
            "webdav-sync-progress",
            WebDavSyncProgressPayload {
                server_id: id,
                server_name: server_name.clone(),
                current_path: String::new(),
                current_count,
                added: stats.added,
                updated: stats.updated,
                errors: stats.errors,
                done: true,
                daily_check,
            },
        );

        let mut summary = format!(
            concat!(
                "webdav sync ({}): total {} ms | {} file(s) seen, ",
                "{} probed, {} added, {} updated, {} removed, {} error(s) | ",
                "{} dir listing(s) {} ms ({} unchanged folder(s) skipped), tag probes {} ms"
            ),
            server_name,
            sync_started.elapsed().as_millis(),
            current_count,
            files_probed,
            stats.added,
            stats.updated,
            stats.removed,
            stats.errors,
            dirs_listed,
            list_time.as_millis(),
            skipped_dirs.len(),
            probe_time.as_millis(),
        );
        summary.push_str(&failure_report(&failures));
        log::info!("{summary}");
        crate::diagnostics::record_operation(&summary);

        let _ = app_clone.emit("library-changed", ());
        Ok(stats)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Failed files listed per sync in the diagnostics export; the rest are only counted (#1495).
const MAX_REPORTED_FAILURES: usize = 20;

/// Lines naming the files that failed during a sync and why (English: the diagnostics
/// log is for bug reports, not the UI). Empty when nothing failed.
fn failure_report(failures: &[(String, String)]) -> String {
    let mut out = String::new();
    for (href, reason) in failures.iter().take(MAX_REPORTED_FAILURES) {
        out.push_str(&format!(
            "
    failed: {href} ({reason})"
        ));
    }
    if failures.len() > MAX_REPORTED_FAILURES {
        out.push_str(&format!(
            "
    ... and {} more failed file(s)",
            failures.len() - MAX_REPORTED_FAILURES
        ));
    }
    out
}

/// Longest an auto-sync may go without listing every folder (#1483).
const FULL_LISTING_INTERVAL_SECS: i64 = 24 * 60 * 60;
/// Directory listings in flight at once during a sync (#1483).
const LIST_CONCURRENCY: usize = 8;
/// Tag probes in flight at once during a sync (#1483).
const PROBE_CONCURRENCY: usize = 6;
/// Minimum gap between `webdav-sync-progress` events while a sync runs.
const PROGRESS_EMIT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// What a previous sync recorded for one remote file, joined with its song.
struct CachedRemoteFile {
    etag: Option<String>,
    size: i64,
    song_id: i64,
    song_path: Option<String>,
    unavailable: bool,
}

/// A new or changed remote file waiting for its tags to be read.
struct ProbeTask {
    item: crate::webdav::WebDavItem,
    probe_url: String,
    file_size: u64,
    playback_url: String,
}

/// Loads every cached remote file of `server_id` (that still has a song) in one query.
fn load_remote_cache(
    conn: &rusqlite::Connection,
    server_id: i64,
) -> rusqlite::Result<HashMap<String, CachedRemoteFile>> {
    let mut stmt = conn.prepare(
        "SELECT c.remote_path, c.etag, c.size, c.song_id, s.path, COALESCE(s.unavailable, 0)
         FROM webdav_cache c LEFT JOIN songs s ON s.id = c.song_id
         WHERE c.server_id = ?1 AND c.song_id IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![server_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            CachedRemoteFile {
                etag: r.get(1)?,
                size: r.get(2)?,
                song_id: r.get(3)?,
                song_path: r.get(4)?,
                unavailable: r.get::<_, i64>(5)? != 0,
            },
        ))
    })?;
    rows.collect()
}

/// Folder etags recorded by the last complete sync, keyed by href without a trailing slash.
fn load_dir_cache(
    conn: &rusqlite::Connection,
    server_id: i64,
) -> rusqlite::Result<HashMap<String, String>> {
    let mut stmt =
        conn.prepare("SELECT remote_path, etag FROM webdav_dir_cache WHERE server_id = ?1")?;
    let rows = stmt.query_map(params![server_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// Whether `path` is one of `dirs` or lies beneath one (hrefs compared without trailing slashes).
fn is_under_any(path: &str, dirs: &HashSet<String>) -> bool {
    let mut current = path.trim_end_matches('/');
    loop {
        if dirs.contains(current) {
            return true;
        }
        match current.rfind('/') {
            Some(index) => current = &current[..index],
            None => return false,
        }
    }
}

/// Replaces the saved folder etags with this sync's, keeping the rows beneath
/// skipped folders (their contents weren't visited, so their old etags stand).
fn save_dir_cache(
    conn: &rusqlite::Connection,
    server_id: i64,
    old: &HashMap<String, String>,
    new: &HashMap<String, String>,
    skipped: &HashSet<String>,
) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for path in old.keys() {
        if !new.contains_key(path) && !is_under_any(path, skipped) {
            tx.execute(
                "DELETE FROM webdav_dir_cache WHERE server_id = ?1 AND remote_path = ?2",
                params![server_id, path],
            )?;
        }
    }
    for (path, etag) in new {
        if old.get(path) != Some(etag) {
            tx.execute(
                "INSERT INTO webdav_dir_cache (server_id, remote_path, etag) VALUES (?1, ?2, ?3)
                 ON CONFLICT(server_id, remote_path) DO UPDATE SET etag = excluded.etag",
                params![server_id, path, etag],
            )?;
        }
    }
    tx.commit()
}

/// Whether a listed file differs from what the last sync recorded: by etag when
/// both sides have one, otherwise by size.
fn remote_file_changed(cached: &CachedRemoteFile, item: &crate::webdav::WebDavItem) -> bool {
    match (&cached.etag, &item.etag) {
        (Some(c), Some(i)) => c != i,
        _ => cached.size != item.content_length.unwrap_or(0) as i64,
    }
}

/// Flags the songs of cached files missing from `seen` as unavailable; returns
/// how many were newly flagged.
fn mark_remote_deletions(
    conn: &rusqlite::Connection,
    cache: &HashMap<String, CachedRemoteFile>,
    seen: &HashSet<String>,
) -> rusqlite::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut removed = 0;
    for (href, cached) in cache {
        if cached.unavailable || seen.contains(href) {
            continue;
        }
        removed += tx.execute(
            "UPDATE songs SET unavailable = 1 WHERE id = ?1 AND unavailable = 0",
            params![cached.song_id],
        )?;
    }
    tx.commit()?;
    Ok(removed)
}

/// Runs `f` over `items` on at most `workers` threads and returns the results
/// in input order. The caller's thread blocks until all are done.
fn run_bounded<T: Send, R: Send>(
    items: Vec<T>,
    workers: usize,
    f: impl Fn(T) -> R + Sync,
) -> Vec<R> {
    let total = items.len();
    let workers = workers.clamp(1, total.max(1));
    if workers == 1 {
        return items.into_iter().map(f).collect();
    }
    let queue = parking_lot::Mutex::new(items.into_iter().enumerate());
    let results = parking_lot::Mutex::new((0..total).map(|_| None).collect::<Vec<Option<R>>>());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let next = queue.lock().next();
                let Some((index, item)) = next else { break };
                let result = f(item);
                results.lock()[index] = Some(result);
            });
        }
    });
    results
        .into_inner()
        .into_iter()
        .map(|r| r.expect("every queued item is processed before the scope ends"))
        .collect()
}

/// Rate-limits `webdav-sync-progress` events: a 3,000-file sync used to send
/// one per file, each a cross-process message the UI re-rendered on.
struct ProgressThrottle {
    last_emit: Option<std::time::Instant>,
    daily_check: bool,
}

impl ProgressThrottle {
    fn new(daily_check: bool) -> Self {
        Self {
            last_emit: None,
            daily_check,
        }
    }

    fn maybe_emit(
        &mut self,
        app: &AppHandle,
        server_id: i64,
        server_name: &str,
        current_path: &str,
        current_count: usize,
        stats: &WebDavSyncStats,
    ) {
        let due = self
            .last_emit
            .is_none_or(|t| t.elapsed() >= PROGRESS_EMIT_INTERVAL);
        if !due {
            return;
        }
        self.last_emit = Some(std::time::Instant::now());
        let _ = app.emit(
            "webdav-sync-progress",
            WebDavSyncProgressPayload {
                server_id,
                server_name: server_name.to_string(),
                current_path: current_path.to_string(),
                current_count,
                added: stats.added,
                updated: stats.updated,
                errors: stats.errors,
                done: false,
                daily_check: self.daily_check,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Song, SongSource};
    use crate::webdav::WebDavItem;

    #[test]
    fn failure_report_lists_files_and_caps_the_rest() {
        assert_eq!(failure_report(&[]), "");

        let one = vec![("/a.mp3".to_string(), "tag probe failed: 404".to_string())];
        assert_eq!(
            failure_report(&one),
            "
    failed: /a.mp3 (tag probe failed: 404)"
        );

        let many: Vec<_> = (0..MAX_REPORTED_FAILURES + 3)
            .map(|i| (format!("/{i}.mp3"), "boom".to_string()))
            .collect();
        let report = failure_report(&many);
        assert_eq!(report.matches("failed: /").count(), MAX_REPORTED_FAILURES);
        assert!(report.ends_with("... and 3 more failed file(s)"));
    }

    fn item(etag: Option<&str>, len: u64) -> WebDavItem {
        WebDavItem {
            href: "/a.mp3".to_string(),
            is_directory: false,
            content_length: Some(len),
            last_modified: None,
            etag: etag.map(str::to_string),
        }
    }

    fn cached(etag: Option<&str>, size: i64) -> CachedRemoteFile {
        CachedRemoteFile {
            etag: etag.map(str::to_string),
            size,
            song_id: 1,
            song_path: None,
            unavailable: false,
        }
    }

    #[test]
    fn remote_file_changed_prefers_etag_and_falls_back_to_size() {
        assert!(!remote_file_changed(
            &cached(Some("a"), 10),
            &item(Some("a"), 99)
        ));
        assert!(remote_file_changed(
            &cached(Some("a"), 10),
            &item(Some("b"), 10)
        ));
        assert!(!remote_file_changed(
            &cached(None, 10),
            &item(Some("b"), 10)
        ));
        assert!(remote_file_changed(&cached(Some("a"), 10), &item(None, 11)));
    }

    #[test]
    fn is_under_any_matches_the_folder_itself_and_its_descendants_only() {
        let dirs: HashSet<String> = ["/music/a".to_string()].into();
        assert!(is_under_any("/music/a", &dirs));
        assert!(is_under_any("/music/a/", &dirs));
        assert!(is_under_any("/music/a/b/c.mp3", &dirs));
        assert!(!is_under_any("/music/ab/c.mp3", &dirs));
        assert!(!is_under_any("/music", &dirs));
        assert!(!is_under_any("/other/a/c.mp3", &dirs));
    }

    #[test]
    fn dir_cache_round_trips_and_keeps_rows_beneath_skipped_folders() {
        let dir = tempfile::Builder::new()
            .prefix("luminous_webdav_dir_")
            .tempdir()
            .unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO webdav_servers (id, name, url, remote_path) VALUES (1, 'NAS', 'http://nas/dav', '/')",
            [],
        )
        .unwrap();
        let map = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(p, e)| (p.to_string(), e.to_string()))
                .collect()
        };

        let first = map(&[("/a", "e1"), ("/a/x", "e2"), ("/b", "e3"), ("/gone", "e9")]);
        save_dir_cache(&conn, 1, &HashMap::new(), &first, &HashSet::new()).unwrap();
        assert_eq!(load_dir_cache(&conn, 1).unwrap(), first);

        // Second sync: /a unchanged (skipped, so /a/x wasn't visited), /b changed,
        // /c new, and /gone no longer exists.
        let new = map(&[("/a", "e1"), ("/b", "e4"), ("/c", "e5")]);
        let skipped: HashSet<String> = ["/a".to_string()].into();
        save_dir_cache(&conn, 1, &first, &new, &skipped).unwrap();
        assert_eq!(
            load_dir_cache(&conn, 1).unwrap(),
            map(&[("/a", "e1"), ("/a/x", "e2"), ("/b", "e4"), ("/c", "e5")])
        );
    }

    #[test]
    fn run_bounded_keeps_input_order_and_never_exceeds_the_worker_cap() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let running = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let out = run_bounded((0..40).collect::<Vec<_>>(), 4, |n| {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(2));
            running.fetch_sub(1, Ordering::SeqCst);
            n * 2
        });
        assert_eq!(out, (0..40).map(|n| n * 2).collect::<Vec<_>>());
        assert!(peak.load(Ordering::SeqCst) <= 4);
        assert!(run_bounded(Vec::<i32>::new(), 4, |n| n).is_empty());
    }

    #[test]
    fn mark_remote_deletions_flags_only_files_missing_from_the_listing() {
        let dir = tempfile::Builder::new()
            .prefix("luminous_webdav_del_")
            .tempdir()
            .unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO webdav_servers (id, name, url, remote_path) VALUES (1, 'NAS', 'http://nas/dav', '/')",
            [],
        )
        .unwrap();

        let mut cache = HashMap::new();
        for name in ["kept", "gone"] {
            let path = format!("http://nas/dav/{name}.mp3");
            crate::collection::upsert_song(
                &conn,
                &Song {
                    path: Some(path.clone()),
                    source: SongSource::WebDav,
                    ..Default::default()
                },
            )
            .unwrap();
            let song_id: i64 = conn
                .query_row("SELECT id FROM songs WHERE path = ?1", params![path], |r| {
                    r.get(0)
                })
                .unwrap();
            cache.insert(
                format!("/{name}.mp3"),
                CachedRemoteFile {
                    song_id,
                    song_path: Some(path),
                    ..cached(None, 1)
                },
            );
        }

        let seen: HashSet<String> = ["/kept.mp3".to_string()].into();
        assert_eq!(mark_remote_deletions(&conn, &cache, &seen).unwrap(), 1);
        // A second pass finds nothing new to flag.
        assert_eq!(mark_remote_deletions(&conn, &cache, &seen).unwrap(), 0);

        let flag = |name: &str| -> bool {
            conn.query_row(
                "SELECT unavailable FROM songs WHERE path = ?1",
                params![format!("http://nas/dav/{name}.mp3")],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert!(!flag("kept"));
        assert!(flag("gone"));

        // load_remote_cache reports the flag so a returning file can be repaired.
        conn.execute(
            "INSERT INTO webdav_cache (server_id, remote_path, size, song_id) SELECT 1, '/gone.mp3', 1, id FROM songs WHERE path LIKE '%gone%'",
            [],
        )
        .unwrap();
        let loaded = load_remote_cache(&conn, 1).unwrap();
        assert!(loaded["/gone.mp3"].unavailable);
    }
}
