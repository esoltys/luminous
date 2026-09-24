//! Tauri IPC commands for OpenSubsonic servers (#916): server CRUD and
//! connection testing (#1161), plus library sync and auto-sync scheduling
//! (#1162). Playback and play/rating reporting follow in #1163 and #1165;
//! per-server sign-in methods in #1167.

use crate::covermanager::CoverManager;
use crate::db::Database;
use crate::models::{SongSource, SubsonicServer, SubsonicSyncStats};
use crate::remote_scheduler::RemoteKind;
use crate::subsonic::sync;
use crate::subsonic::{
    load_auth, Auth, AuthMode, ServerProbe, SubsonicApiError, SubsonicClient, URI_SCHEME,
};
use crate::AppState;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

const SUBSONIC_SERVER_COLUMNS: &str = "id, name, url, username, enabled, sync_status, last_synced_at, created_at, nickname, icon, color, auto_sync_enabled, sync_interval_minutes, report_plays, server_type, server_version, extensions_json, auth_mode";

fn row_to_subsonic_server(row: &rusqlite::Row) -> rusqlite::Result<SubsonicServer> {
    let extensions_json: String = row.get(16)?;
    let auth_mode: String = row.get(17)?;
    Ok(SubsonicServer {
        id: row.get(0)?,
        name: row.get(1)?,
        url: row.get(2)?,
        username: row.get(3)?,
        password: None,
        auth_mode: AuthMode::from_db(&auth_mode),
        enabled: row.get(4)?,
        sync_status: row.get(5)?,
        last_synced_at: row.get(6)?,
        created_at: row.get(7)?,
        nickname: row.get(8)?,
        icon: row.get(9)?,
        color: row.get(10)?,
        auto_sync_enabled: row.get(11)?,
        sync_interval_minutes: row.get(12)?,
        report_plays: row.get(13)?,
        server_type: row.get(14)?,
        server_version: row.get(15)?,
        extensions: serde_json::from_str(&extensions_json).unwrap_or_default(),
        next_auto_sync_at: None,
    })
}

fn load_server(conn: &rusqlite::Connection, id: i64) -> Result<SubsonicServer, String> {
    conn.query_row(
        &format!("SELECT {SUBSONIC_SERVER_COLUMNS} FROM subsonic_servers WHERE id = ?1"),
        params![id],
        row_to_subsonic_server,
    )
    .map_err(|e| e.to_string())
}

/// Stored `(url, credentials)` for a saved server.
fn load_credentials(conn: &rusqlite::Connection, id: i64) -> Result<(String, Auth), String> {
    load_auth(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("OpenSubsonic server {id} not found"))
}

/// Whether a saved secret still fits after a mode change: a password works
/// for both token and password sign-in, but an API key is a different secret.
fn secret_carries_over(from: AuthMode, to: AuthMode) -> bool {
    (from == AuthMode::ApiKey) == (to == AuthMode::ApiKey)
}

fn stored_auth_mode(conn: &rusqlite::Connection, id: i64) -> Result<AuthMode, String> {
    let mode: Option<String> = conn
        .query_row(
            "SELECT auth_mode FROM subsonic_servers WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    mode.map(|m| AuthMode::from_db(&m))
        .ok_or_else(|| format!("OpenSubsonic server {id} not found"))
}

fn missing_secret_message(mode: AuthMode) -> &'static str {
    if mode == AuthMode::ApiKey {
        "Enter your API key for the server"
    } else {
        "Enter your password for the server"
    }
}

/// List all configured OpenSubsonic servers.
#[tauri::command]
pub async fn list_subsonic_servers(
    state: State<'_, AppState>,
) -> Result<Vec<SubsonicServer>, String> {
    let mut servers: Vec<SubsonicServer> = crate::db::run_blocking(&state.db, |conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {SUBSONIC_SERVER_COLUMNS} FROM subsonic_servers ORDER BY created_at ASC, id ASC"
        ))?;
        let rows = stmt.query_map([], row_to_subsonic_server)?;
        Ok(rows.flatten().collect())
    })
    .await
    .map_err(|e| e.to_string())?;
    for s in &mut servers {
        s.next_auto_sync_at = state
            .remote_auto_sync
            .next_run_at(RemoteKind::Subsonic, s.id);
    }
    Ok(servers)
}

/// Fields for [`save_subsonic_server`], bundled into one struct like
/// `SaveWebDavServerInput`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSubsonicServerInput {
    pub id: Option<i64>,
    pub name: String,
    pub url: String,
    /// Ignored (stored empty) in API-key mode.
    pub username: String,
    /// The password, or the API key in API-key mode. Required for a new
    /// server; `None` on an edit keeps the stored secret.
    pub password: Option<String>,
    /// Defaults to token sign-in.
    pub auth_mode: Option<AuthMode>,
    pub enabled: Option<bool>,
    pub nickname: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub auto_sync_enabled: Option<bool>,
    pub sync_interval_minutes: Option<i64>,
    pub report_plays: Option<bool>,
}

/// Normalizes and validates the user-entered fields of a save.
fn validate_input(input: &SaveSubsonicServerInput) -> Result<(String, String, String), String> {
    let name = input.name.trim().to_string();
    let url = input.url.trim().trim_end_matches('/').to_string();
    let mode = input.auth_mode.unwrap_or_default();
    let username = if mode == AuthMode::ApiKey {
        String::new()
    } else {
        input.username.trim().to_string()
    };
    if name.is_empty() {
        return Err("Enter a name for the server".into());
    }
    if username.is_empty() && mode != AuthMode::ApiKey {
        return Err("Enter your username for the server".into());
    }
    match reqwest::Url::parse(&url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => {}
        _ => return Err("Enter a full server URL, e.g. https://music.example.com".into()),
    }
    if input.id.is_none() && input.password.as_deref().unwrap_or("").is_empty() {
        return Err(missing_secret_message(mode).into());
    }
    Ok((name, url, username))
}

/// Comparison key for a server URL: scheme and host are case-insensitive,
/// and a pasted `/rest` suffix or trailing `/` points at the same server.
fn server_url_key(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix("/rest").unwrap_or(url);
    match reqwest::Url::parse(url) {
        Ok(u) => u.as_str().trim_end_matches('/').to_string(),
        Err(_) => url.to_ascii_lowercase(),
    }
}

/// Rejects a save that would add the same account on the same server twice
/// (other than the server being edited): each profile syncs its own copy of
/// the library, so a duplicate shows every song twice. An API key doesn't
/// name its account, so API-key profiles aren't checked.
fn ensure_not_duplicate(
    conn: &rusqlite::Connection,
    url: &str,
    username: &str,
    editing_id: Option<i64>,
) -> Result<(), String> {
    if username.is_empty() {
        return Ok(());
    }
    let key = server_url_key(url);
    let mut stmt = conn
        .prepare("SELECT id, name, url, username FROM subsonic_servers")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, name, other_url, other_user) = row.map_err(|e| e.to_string())?;
        if Some(id) != editing_id
            && other_user.eq_ignore_ascii_case(username)
            && server_url_key(&other_url) == key
        {
            return Err(format!(
                "This account on this server is already added as \"{name}\""
            ));
        }
    }
    Ok(())
}

/// Save (create or update) an OpenSubsonic server profile. Doesn't contact
/// the server — the settings UI tests the connection before saving, and
/// [`check_subsonic_connection`] refreshes the stored server details. The
/// server's auto-sync timer is (re)scheduled or cancelled to match.
#[tauri::command]
pub async fn save_subsonic_server(
    input: SaveSubsonicServerInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SubsonicServer, String> {
    let (name, url, username) = validate_input(&input)?;
    let SaveSubsonicServerInput {
        id,
        password,
        auth_mode,
        enabled,
        nickname,
        icon,
        color,
        auto_sync_enabled,
        sync_interval_minutes,
        report_plays,
        ..
    } = input;
    let enabled = enabled.unwrap_or(true);
    let auto_sync_enabled = auto_sync_enabled.unwrap_or(false);
    let sync_interval_minutes = sync_interval_minutes.unwrap_or(60).max(1);
    let report_plays = report_plays.unwrap_or(true);
    // An empty password on an edit means "unchanged", same as `None`.
    let password = password.filter(|p| !p.is_empty());
    let auth_mode = auth_mode.unwrap_or_default();

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    ensure_not_duplicate(&conn, &url, &username, id)?;
    let saved_id = if let Some(server_id) = id {
        if password.is_none() {
            let stored = stored_auth_mode(&conn, server_id)?;
            if !secret_carries_over(stored, auth_mode) {
                return Err(missing_secret_message(auth_mode).into());
            }
        }
        conn.execute(
            "UPDATE subsonic_servers
             SET name = ?1, url = ?2, username = ?3, password = COALESCE(?4, password), enabled = ?5,
                 nickname = ?6, icon = ?7, color = ?8, auto_sync_enabled = ?9,
                 sync_interval_minutes = ?10, report_plays = ?11, auth_mode = ?12
             WHERE id = ?13",
            params![
                name,
                url,
                username,
                password,
                enabled,
                nickname,
                icon,
                color,
                auto_sync_enabled,
                sync_interval_minutes,
                report_plays,
                auth_mode.as_str(),
                server_id
            ],
        )
        .map_err(|e| e.to_string())?;
        server_id
    } else {
        conn.execute(
            "INSERT INTO subsonic_servers (name, url, username, password, enabled, nickname, icon, color, auto_sync_enabled, sync_interval_minutes, report_plays, auth_mode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                name,
                url,
                username,
                password,
                enabled,
                nickname,
                icon,
                color,
                auto_sync_enabled,
                sync_interval_minutes,
                report_plays,
                auth_mode.as_str()
            ],
        )
        .map_err(|e| e.to_string())?;
        conn.last_insert_rowid()
    };

    let mut saved = load_server(&conn, saved_id)?;
    drop(conn);
    if saved.enabled && saved.auto_sync_enabled {
        state.remote_auto_sync.reschedule(
            app,
            Arc::clone(&state.db),
            Arc::clone(&state.cover_manager),
            RemoteKind::Subsonic,
            saved.id,
            saved.sync_interval_minutes,
        );
    } else {
        state
            .remote_auto_sync
            .cancel(RemoteKind::Subsonic, saved.id);
    }
    saved.next_auto_sync_at = state
        .remote_auto_sync
        .next_run_at(RemoteKind::Subsonic, saved.id);
    Ok(saved)
}

/// Delete an OpenSubsonic server profile. Its songs are marked unavailable
/// (soft-deleted), mirroring `delete_webdav_server` and `remove_directory`;
/// the explicit "Clean Up" action removes them for good. The server's cache
/// rows go with it via `ON DELETE CASCADE`.
#[tauri::command]
pub async fn delete_subsonic_server(
    id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    crate::db::run_blocking(&state.db, move |conn| {
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE songs SET unavailable = 1 WHERE source = ?1 AND substr(path, 1, length(?2)) = ?2",
            params![SongSource::SUBSONIC_ID, format!("{URI_SCHEME}{id}/")],
        )?;
        tx.execute("DELETE FROM subsonic_servers WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?;

    state.remote_auto_sync.cancel(RemoteKind::Subsonic, id);
    let _ = app.emit("library-changed", ());
    Ok(())
}

/// A failed connection test. `code` is the Subsonic API error code when the
/// server answered with one, so the UI can react to it (e.g. offer password
/// sign-in on 41).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestError {
    pub message: String,
    pub code: Option<i64>,
}

impl From<String> for ConnectionTestError {
    fn from(message: String) -> Self {
        Self {
            message,
            code: None,
        }
    }
}

impl From<&str> for ConnectionTestError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

impl From<anyhow::Error> for ConnectionTestError {
    fn from(e: anyhow::Error) -> Self {
        Self {
            code: e.downcast_ref::<SubsonicApiError>().map(|api| api.0.code),
            message: format!("{e:#}"),
        }
    }
}

/// Test a connection without saving: pings the server and discovers its
/// OpenSubsonic extensions. When editing a saved server (`id` set) and the
/// password/API key field was left blank, the stored secret is used — unless
/// the sign-in method changed to or from API key, which needs a new one.
#[tauri::command]
pub async fn test_subsonic_connection(
    url: String,
    username: String,
    password: Option<String>,
    auth_mode: Option<AuthMode>,
    id: Option<i64>,
    state: State<'_, AppState>,
) -> Result<ServerProbe, ConnectionTestError> {
    let mode = auth_mode.unwrap_or_default();
    let secret = match (password.filter(|p| !p.is_empty()), id) {
        (Some(p), _) => p,
        (None, Some(id)) => {
            let conn = state.db.pool.get().map_err(|e| e.to_string())?;
            let (_, stored) = load_credentials(&conn, id)?;
            if !secret_carries_over(stored.mode, mode) {
                return Err(missing_secret_message(mode).into());
            }
            stored.secret
        }
        (None, None) => return Err(missing_secret_message(mode).into()),
    };
    tokio::task::spawn_blocking(move || {
        let client = SubsonicClient::new(&url, Auth::new(mode, username.trim(), &secret))?;
        Ok(client.probe()?)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Sign-in methods a server offers beyond token/password.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubsonicAuthSupport {
    pub api_key: bool,
}

/// Asks a server — without credentials — which sign-in methods it supports,
/// so the settings UI only offers API-key sign-in where it will work. A
/// server that can't be reached simply reports no extras.
#[tauri::command]
pub async fn get_subsonic_auth_support(url: String) -> Result<SubsonicAuthSupport, String> {
    tokio::task::spawn_blocking(move || {
        let api_key = SubsonicClient::anonymous(url.trim())
            .and_then(|c| c.supports_api_key())
            .unwrap_or(false);
        Ok(SubsonicAuthSupport { api_key })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Live-checks a saved server with its stored credentials — the Subsonic
/// counterpart to `check_webdav_connection` — and records the server
/// software/version and extension list it reports, so later features can
/// branch on capabilities without re-probing.
#[tauri::command]
pub async fn check_subsonic_connection(
    id: i64,
    state: State<'_, AppState>,
) -> Result<ServerProbe, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.pool.get().map_err(|e| e.to_string())?;
        let (url, auth) = load_credentials(&conn, id)?;
        drop(conn);

        let client = SubsonicClient::new(&url, auth).map_err(|e| format!("{e:#}"))?;
        let probe = client.probe().map_err(|e| format!("{e:#}"))?;

        let extensions_json =
            serde_json::to_string(&probe.extension_names()).unwrap_or_else(|_| "[]".into());
        let conn = db.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE subsonic_servers SET server_type = ?1, server_version = ?2, extensions_json = ?3 WHERE id = ?4",
            params![
                probe.info.server_type,
                probe.info.server_version,
                extensions_json,
                id
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(probe)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Progress update emitted as `subsonic-sync-progress` during a sync.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubsonicSyncProgressPayload {
    pub server_id: i64,
    pub server_name: String,
    /// `listing`, `artwork`, `saving`, or `done`.
    pub phase: &'static str,
    /// Songs listed so far (`listing`) or albums checked for artwork (`artwork`).
    pub current_count: usize,
    pub stats: SubsonicSyncStats,
    pub done: bool,
    /// Set on the final event when the sync failed.
    pub error: Option<String>,
}

/// Synchronize an OpenSubsonic server into the library ("Sync Now").
#[tauri::command]
pub async fn sync_subsonic_server(
    id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SubsonicSyncStats, String> {
    sync_subsonic_server_inner(id, app, state.db.clone(), state.cover_manager.clone()).await
}

/// Core sync routine shared by [`sync_subsonic_server`] and the auto-sync
/// scheduler (`remote_scheduler`), which only has an `AppHandle` and the
/// shared `Arc`s rather than a `State<AppState>`.
pub async fn sync_subsonic_server_inner(
    id: i64,
    app: AppHandle,
    db: Arc<Database>,
    cover_manager: Arc<CoverManager>,
) -> Result<SubsonicSyncStats, String> {
    tokio::task::spawn_blocking(move || {
        let emit = |name: &str, phase: &'static str, count: usize, stats: &SubsonicSyncStats, done: bool, error: Option<String>| {
            let _ = app.emit(
                "subsonic-sync-progress",
                SubsonicSyncProgressPayload {
                    server_id: id,
                    server_name: name.to_string(),
                    phase,
                    current_count: count,
                    stats: stats.clone(),
                    done,
                    error,
                },
            );
        };

        // Pooled connections are only held for DB work, never across the
        // network requests below — the pool is small (see `Database::new`).
        let (name, url, auth, existing_art) = {
            let conn = db.pool.get().map_err(|e| e.to_string())?;
            let name: String = conn
                .query_row(
                    "SELECT name FROM subsonic_servers WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .map_err(|_| format!("OpenSubsonic server {id} not found"))?;
            let (url, auth) = load_credentials(&conn, id)?;
            conn.execute(
                "UPDATE subsonic_servers SET sync_status = 'syncing' WHERE id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
            let existing_art = sync::existing_album_art(&conn, id).unwrap_or_default();
            (name, url, auth, existing_art)
        };

        let empty = SubsonicSyncStats::default();
        emit(&name, "listing", 0, &empty, false, None);

        let result = (|| -> anyhow::Result<SubsonicSyncStats> {
            let client = SubsonicClient::new(&url, auth)?;
            let library = sync::fetch_library(&client, |n| {
                emit(&name, "listing", n, &empty, false, None)
            })?;
            let (art, art_errors) =
                sync::fetch_album_art(&existing_art, &client, &cover_manager, id, &library, |n| {
                    emit(&name, "artwork", n, &empty, false, None)
                });
            emit(&name, "saving", library.songs.len(), &empty, false, None);
            let conn = db.pool.get()?;
            let mut stats = sync::apply_library(&conn, id, &library, &art)?;
            stats.errors = art_errors;
            Ok(stats)
        })();

        let conn = db.pool.get().map_err(|e| e.to_string())?;
        match result {
            Ok(stats) => {
                let _ = conn.execute(
                    "UPDATE subsonic_servers SET sync_status = 'idle', last_synced_at = strftime('%s', 'now') WHERE id = ?1",
                    params![id],
                );
                emit(&name, "done", 0, &stats, true, None);
                let _ = app.emit("library-changed", ());
                Ok(stats)
            }
            Err(e) => {
                let message = format!("{e:#}");
                log::warn!("OpenSubsonic sync of server {id} failed: {message}");
                let _ = conn.execute(
                    "UPDATE subsonic_servers SET sync_status = 'idle' WHERE id = ?1",
                    params![id],
                );
                emit(&name, "done", 0, &empty, true, Some(message.clone()));
                Err(message)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: Option<i64>, url: &str, password: Option<&str>) -> SaveSubsonicServerInput {
        SaveSubsonicServerInput {
            id,
            name: " Home ".into(),
            url: url.into(),
            username: " alice ".into(),
            password: password.map(String::from),
            auth_mode: None,
            enabled: None,
            nickname: None,
            icon: None,
            color: None,
            auto_sync_enabled: None,
            sync_interval_minutes: None,
            report_plays: None,
        }
    }

    #[test]
    fn validate_trims_and_normalizes() {
        let (name, url, user) =
            validate_input(&input(None, " https://music.example.com/ ", Some("pw"))).unwrap();
        assert_eq!(name, "Home");
        assert_eq!(url, "https://music.example.com");
        assert_eq!(user, "alice");
    }

    #[test]
    fn validate_requires_password_only_for_new_servers() {
        assert!(validate_input(&input(None, "https://m.example.com", None)).is_err());
        assert!(validate_input(&input(None, "https://m.example.com", Some(""))).is_err());
        assert!(validate_input(&input(Some(1), "https://m.example.com", None)).is_ok());
    }

    #[test]
    fn validate_api_key_mode_needs_no_username() {
        let mut i = input(None, "https://m.example.com", Some("k3y"));
        i.auth_mode = Some(AuthMode::ApiKey);
        let (_, _, user) = validate_input(&i).unwrap();
        assert_eq!(user, "", "username is dropped in API-key mode");

        i.password = None;
        assert_eq!(
            validate_input(&i).unwrap_err(),
            "Enter your API key for the server"
        );

        let mut i = input(None, "https://m.example.com", Some("pw"));
        i.username = " ".into();
        assert!(validate_input(&i).is_err(), "token mode needs a username");
    }

    #[test]
    fn only_api_key_changes_need_a_new_secret() {
        use AuthMode::*;
        assert!(secret_carries_over(Token, Password));
        assert!(secret_carries_over(Password, Token));
        assert!(secret_carries_over(ApiKey, ApiKey));
        assert!(!secret_carries_over(Token, ApiKey));
        assert!(!secret_carries_over(ApiKey, Password));
    }

    #[test]
    fn connection_test_error_carries_the_api_code() {
        let err: ConnectionTestError = anyhow::Error::new(SubsonicApiError(
            crate::subsonic::ApiError {
                code: 41,
                message: "x".into(),
            },
        ))
        .into();
        assert_eq!(err.code, Some(41));
        assert!(err.message.contains("Password sign-in method"));
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"code\":41"), "{json}");

        let plain: ConnectionTestError = anyhow::anyhow!("HTTP 404").into();
        assert_eq!(plain.code, None);
    }

    #[test]
    fn validate_rejects_non_http_urls() {
        for bad in ["music.example.com", "ftp://music.example.com", ""] {
            assert!(
                validate_input(&input(None, bad, Some("pw"))).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn server_row_round_trips_and_never_exposes_password() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE subsonic_servers (
                id INTEGER PRIMARY KEY, name TEXT, url TEXT, username TEXT, password TEXT,
                enabled BOOLEAN DEFAULT 1, sync_status TEXT DEFAULT 'idle', last_synced_at INTEGER,
                created_at INTEGER DEFAULT 0, nickname TEXT, icon TEXT, color TEXT,
                auto_sync_enabled INTEGER DEFAULT 0, sync_interval_minutes INTEGER DEFAULT 60,
                report_plays INTEGER DEFAULT 1, server_type TEXT, server_version TEXT,
                extensions_json TEXT DEFAULT '[]', auth_mode TEXT NOT NULL DEFAULT 'token');
             INSERT INTO subsonic_servers (id, name, url, username, password, server_type, extensions_json, auth_mode)
             VALUES (1, 'Home', 'https://m.example.com', 'alice', 'secret', 'navidrome', '[\"songLyrics\"]', 'password');",
        )
        .unwrap();

        let server = load_server(&conn, 1).unwrap();
        assert_eq!(server.username, "alice");
        assert_eq!(server.auth_mode, AuthMode::Password);
        assert_eq!(server.server_type.as_deref(), Some("navidrome"));
        assert_eq!(server.extensions, vec!["songLyrics".to_string()]);
        assert!(server.report_plays);
        assert_eq!(server.password, None);
        let json = serde_json::to_string(&server).unwrap();
        assert!(!json.contains("secret"));
        assert!(json.contains("\"reportPlays\":true"));
        assert!(json.contains("\"authMode\":\"password\""));

        let (url, auth) = load_credentials(&conn, 1).unwrap();
        assert_eq!(url, "https://m.example.com");
        assert_eq!(auth.mode, AuthMode::Password);
        assert_eq!(auth.username, "alice");
        assert_eq!(auth.secret, "secret");
        assert_eq!(stored_auth_mode(&conn, 1).unwrap(), AuthMode::Password);
        assert!(load_credentials(&conn, 9).is_err());
    }

    #[test]
    fn duplicate_server_is_rejected_but_editing_itself_is_not() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE subsonic_servers (id INTEGER PRIMARY KEY, name TEXT, url TEXT, username TEXT);
             INSERT INTO subsonic_servers VALUES (1, 'Home', 'https://Music.example.com', 'alice');",
        )
        .unwrap();

        for url in [
            "https://music.example.com",
            "https://music.example.com/",
            "https://MUSIC.example.com/rest",
        ] {
            let err = ensure_not_duplicate(&conn, url, "Alice", None).unwrap_err();
            assert!(err.contains("\"Home\""), "{url}: {err}");
        }
        // Editing server 1 itself is fine.
        ensure_not_duplicate(&conn, "https://music.example.com", "alice", Some(1)).unwrap();
        // A different account or server is fine.
        ensure_not_duplicate(&conn, "https://music.example.com", "bob", None).unwrap();
        ensure_not_duplicate(&conn, "https://other.example.com", "alice", None).unwrap();
        ensure_not_duplicate(&conn, "https://music.example.com/navidrome", "alice", None).unwrap();
        // API-key profiles have no username to compare.
        ensure_not_duplicate(&conn, "https://music.example.com", "", None).unwrap();
    }
}
