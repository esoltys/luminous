//! Tauri IPC commands for remote WebDAV server management and synchronization (#682).

use crate::covermanager::CoverManager;
use crate::db::Database;
use crate::models::{SongSource, WebDavServer, WebDavSyncStats};
use crate::webdav::{detect_filetype_from_url, WebDavClient};
use crate::AppState;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
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
        s.next_auto_sync_at = state.webdav_auto_sync.next_run_at(s.id);
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
        state.webdav_auto_sync.reschedule(
            app,
            Arc::clone(&state.db),
            Arc::clone(&state.cover_manager),
            saved.id,
            saved.sync_interval_minutes,
        );
    } else {
        state.webdav_auto_sync.cancel(saved.id);
    }
    saved.next_auto_sync_at = state.webdav_auto_sync.next_run_at(saved.id);

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
                .prepare("SELECT song_id FROM webdav_cache WHERE server_id = ?1 AND song_id IS NOT NULL")
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map(params![id], |r| r.get(0)).map_err(|e| e.to_string())?;
            rows.flatten().collect()
        };

        let server_url: Option<String> = tx
            .query_row("SELECT url FROM webdav_servers WHERE id = ?1", params![id], |r| r.get(0))
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
                if crate::collection::song_matches_webdav_server(&path, url) {
                    if !song_ids.contains(&song_id) {
                        song_ids.push(song_id);
                    }
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

    state.webdav_auto_sync.cancel(id);
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
    sync_webdav_server_inner(id, app, state.db.clone(), state.cover_manager.clone()).await
}

/// Core sync routine shared by the [`sync_webdav_server`] command (manual
/// "Sync Now" clicks) and `webdav_scheduler::AutoSyncScheduler` (periodic
/// auto-sync, #1082) — the scheduler runs as a background task with only an
/// `AppHandle` and `Arc<Database>`/`Arc<CoverManager>`, not a `State<AppState>`.
pub async fn sync_webdav_server_inner(
    id: i64,
    app: AppHandle,
    db: Arc<Database>,
    cover_manager: Arc<CoverManager>,
) -> Result<WebDavSyncStats, String> {
    let app_clone = app.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db.pool.get().map_err(|e| e.to_string())?;

        // Retrieve server credentials & config
        let (server_name, url, username, password, remote_path): (String, String, Option<String>, Option<String>, String) = conn
            .query_row(
                "SELECT name, url, username, password, remote_path FROM webdav_servers WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .map_err(|e| e.to_string())?;

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

        let mut queue = VecDeque::new();
        queue.push_back(remote_path);

        let mut stats = WebDavSyncStats::default();
        let mut current_count = 0usize;

        while let Some(current_path) = queue.pop_front() {
            let items = match client.list_directory(&current_path) {
                Ok(it) => it,
                Err(err) => {
                    log::warn!("Failed to list WebDAV directory {current_path}: {err}");
                    stats.errors += 1;
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
            let mut folder_art_bytes: Option<Vec<u8>> = None;
            let mut folder_art_fetch_attempted = false;

            for item in items {
                // Avoid infinite loops matching the directory itself
                let norm_item_href = item.href.trim_end_matches('/');
                let norm_cur = current_path.trim_end_matches('/');
                if norm_item_href == norm_cur || norm_item_href.ends_with(norm_cur) && norm_item_href.len() == norm_cur.len() {
                    continue;
                }

                if item.is_directory {
                    queue.push_back(item.href);
                } else {
                    let filetype = detect_filetype_from_url(&item.href);
                    if filetype == crate::models::FileType::Unknown {
                        continue;
                    }

                    let file_size = item.content_length.unwrap_or(0);
                    // Used for internal probing (Authorization header set explicitly by the client).
                    let probe_url = client.build_url(&item.href);
                    // Used for the stored path/stream_url: the audio engine has no separate
                    // credential lookup at playback time, so credentials travel embedded in
                    // the URL itself (see `WebDavClient::build_authenticated_url`).
                    let playback_url = client.build_authenticated_url(&item.href);

                    // Check cache for existing etag/size match
                    let cached_info: Option<(i64, Option<String>, i64, i64)> = conn
                        .query_row(
                            "SELECT id, etag, size, song_id FROM webdav_cache WHERE server_id = ?1 AND remote_path = ?2",
                            params![id, item.href],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                        )
                        .ok();

                    let needs_rescan = match cached_info {
                        Some((_, ref cached_etag, cached_size, _)) => {
                            if let (Some(c_etag), Some(ref i_etag)) = (cached_etag, &item.etag) {
                                c_etag != i_etag
                            } else {
                                cached_size != file_size as i64
                            }
                        }
                        None => true,
                    };

                    if !needs_rescan {
                        // The remote file itself is unchanged, so skip re-probing tags —
                        // but the stored playback URL may still be stale (e.g. it predates
                        // #682's fix to embed credentials for playback, or the server's
                        // credentials changed since). Refresh it unconditionally so a
                        // rescan actually repairs previously-synced songs, not just new ones.
                        if let Some((_, _, _, cached_song_id)) = cached_info {
                            let current_path: Option<String> = conn
                                .query_row(
                                    "SELECT path FROM songs WHERE id = ?1",
                                    params![cached_song_id],
                                    |r| r.get(0),
                                )
                                .ok();
                            if current_path.as_deref() != Some(playback_url.as_str()) {
                                let _ = conn.execute(
                                    "UPDATE songs SET path = ?1, url = ?1, stream_url = ?1 WHERE id = ?2",
                                    params![playback_url, cached_song_id],
                                );
                                stats.updated += 1;
                            }
                        }
                        current_count += 1;
                        let _ = app_clone.emit(
                            "webdav-sync-progress",
                            WebDavSyncProgressPayload {
                                server_id: id,
                                server_name: server_name.clone(),
                                current_path: item.href.clone(),
                                current_count,
                                added: stats.added,
                                updated: stats.updated,
                                errors: stats.errors,
                                done: false,
                            },
                        );
                        continue;
                    }

                    // Probe remote tags using byte ranges
                    match client.probe_song_tags(&probe_url, file_size) {
                        Ok(mut song) => {
                            song.path = Some(playback_url.clone());
                            song.url = Some(playback_url.clone());
                            song.stream_url = Some(playback_url.clone());

                            // No embedded-picture extraction over WebDAV yet, so
                            // `art_automatic` is always still unset here — fall
                            // back to this directory's folder-art image, same as
                            // a local scan's `scan_folder_art` fallback (#1082
                            // follow-up).
                            if song.art_automatic.is_none() {
                                if let Some(art_item) = &folder_art_item {
                                    if !folder_art_fetch_attempted {
                                        folder_art_fetch_attempted = true;
                                        let art_url = client.build_url(&art_item.href);
                                        match client.fetch_full(&art_url) {
                                            Ok(bytes) => folder_art_bytes = Some(bytes),
                                            Err(e) => log::warn!(
                                                "Failed to download WebDAV folder art {}: {e}",
                                                art_item.href
                                            ),
                                        }
                                    }
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
                            }

                            if let Err(e) = crate::collection::upsert_song(&conn, &song) {
                                log::warn!("Failed to upsert WebDAV song {}: {e}", item.href);
                                stats.errors += 1;
                                continue;
                            }

                            let song_id: i64 = conn
                                .query_row(
                                    "SELECT id FROM songs WHERE path = ?1",
                                    params![playback_url],
                                    |r| r.get(0),
                                )
                                .unwrap_or(0);

                            let _ = conn.execute(
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

                            if cached_info.is_some() {
                                stats.updated += 1;
                            } else {
                                stats.added += 1;
                            }
                        }
                        Err(err) => {
                            log::warn!("Failed to probe WebDAV file {}: {err}", item.href);
                            stats.errors += 1;
                        }
                    }

                    current_count += 1;
                    let _ = app_clone.emit(
                        "webdav-sync-progress",
                        WebDavSyncProgressPayload {
                            server_id: id,
                            server_name: server_name.clone(),
                            current_path: item.href.clone(),
                            current_count,
                            added: stats.added,
                            updated: stats.updated,
                            errors: stats.errors,
                            done: false,
                        },
                    );
                }
            }
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
            },
        );

        let _ = app_clone.emit("library-changed", ());
        Ok(stats)
    })
    .await
    .map_err(|e| e.to_string())?
}
