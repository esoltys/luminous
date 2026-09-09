//! Tauri IPC commands for remote WebDAV server management and synchronization (#682).

use crate::models::{WebDavServer, WebDavSyncStats};
use crate::webdav::{detect_filetype_from_url, WebDavClient};
use crate::AppState;
use rusqlite::params;
use std::collections::VecDeque;
use tauri::{AppHandle, Emitter, State};

/// List all configured WebDAV servers.
#[tauri::command]
pub async fn list_webdav_servers(state: State<'_, AppState>) -> Result<Vec<WebDavServer>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, url, username, remote_path, enabled, sync_status, last_synced_at, created_at
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
            })
        })
        .map_err(|e| e.to_string())?;

    let mut servers = Vec::new();
    for s in rows.flatten() {
        servers.push(s);
    }
    Ok(servers)
}

/// Save (create or update) a WebDAV server profile.
#[tauri::command]
pub async fn save_webdav_server(
    id: Option<i64>,
    name: String,
    url: String,
    username: Option<String>,
    password: Option<String>,
    remote_path: Option<String>,
    enabled: Option<bool>,
    state: State<'_, AppState>,
) -> Result<WebDavServer, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let remote_path_val = remote_path.unwrap_or_else(|| "/".to_string());
    let enabled_val = enabled.unwrap_or(true);

    if let Some(server_id) = id {
        if let Some(pass) = password {
            conn.execute(
                "UPDATE webdav_servers
                 SET name = ?1, url = ?2, username = ?3, password = ?4, remote_path = ?5, enabled = ?6
                 WHERE id = ?7",
                params![name, url, username, pass, remote_path_val, enabled_val, server_id],
            )
            .map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "UPDATE webdav_servers
                 SET name = ?1, url = ?2, username = ?3, remote_path = ?4, enabled = ?5
                 WHERE id = ?6",
                params![name, url, username, remote_path_val, enabled_val, server_id],
            )
            .map_err(|e| e.to_string())?;
        }

        let s: WebDavServer = conn
            .query_row(
                "SELECT id, name, url, username, remote_path, enabled, sync_status, last_synced_at, created_at
                 FROM webdav_servers WHERE id = ?1",
                params![server_id],
                |row| {
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
                    })
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(s)
    } else {
        conn.execute(
            "INSERT INTO webdav_servers (name, url, username, password, remote_path, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![name, url, username, password, remote_path_val, enabled_val],
        )
        .map_err(|e| e.to_string())?;

        let new_id = conn.last_insert_rowid();
        let s: WebDavServer = conn
            .query_row(
                "SELECT id, name, url, username, remote_path, enabled, sync_status, last_synced_at, created_at
                 FROM webdav_servers WHERE id = ?1",
                params![new_id],
                |row| {
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
                    })
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(s)
    }
}

/// Delete a WebDAV server profile and its associated cache.
#[tauri::command]
pub async fn delete_webdav_server(
    id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM webdav_servers WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
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

/// Synchronize a WebDAV server into the library.
#[tauri::command]
pub async fn sync_webdav_server(
    id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WebDavSyncStats, String> {
    let db = state.db.clone();
    let app_clone = app.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db.pool.get().map_err(|e| e.to_string())?;

        // Retrieve server credentials & config
        let (url, username, password, remote_path): (String, Option<String>, Option<String>, String) = conn
            .query_row(
                "SELECT url, username, password, remote_path FROM webdav_servers WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|e| e.to_string())?;

        // Update status to 'syncing'
        let _ = conn.execute(
            "UPDATE webdav_servers SET sync_status = 'syncing' WHERE id = ?1",
            params![id],
        );

        let client = WebDavClient::new(url.clone(), username, password).map_err(|e| e.to_string())?;

        let mut queue = VecDeque::new();
        queue.push_back(remote_path);

        let mut stats = WebDavSyncStats::default();

        while let Some(current_path) = queue.pop_front() {
            let items = match client.list_directory(&current_path) {
                Ok(it) => it,
                Err(err) => {
                    log::warn!("Failed to list WebDAV directory {current_path}: {err}");
                    stats.errors += 1;
                    continue;
                }
            };

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
                        continue;
                    }

                    // Probe remote tags using byte ranges
                    match client.probe_song_tags(&probe_url, file_size) {
                        Ok(mut song) => {
                            song.path = Some(playback_url.clone());
                            song.url = Some(playback_url.clone());
                            song.stream_url = Some(playback_url.clone());

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

        let _ = app_clone.emit("library-changed", ());
        Ok(stats)
    })
    .await
    .map_err(|e| e.to_string())?
}
