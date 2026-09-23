//! Tauri IPC commands for OpenSubsonic server management (#916, #1161).
//!
//! Library sync, playback, and play/rating reporting build on these in
//! follow-up changes (#1162, #1163, #1165); this module only covers server
//! CRUD plus connection testing/capability discovery.

use crate::models::{SongSource, SubsonicServer};
use crate::subsonic::{ServerProbe, SubsonicClient, URI_SCHEME};
use crate::AppState;
use rusqlite::params;
use tauri::{AppHandle, Emitter, State};

const SUBSONIC_SERVER_COLUMNS: &str = "id, name, url, username, enabled, sync_status, last_synced_at, created_at, nickname, icon, color, auto_sync_enabled, sync_interval_minutes, report_plays, server_type, server_version, extensions_json";

fn row_to_subsonic_server(row: &rusqlite::Row) -> rusqlite::Result<SubsonicServer> {
    let extensions_json: String = row.get(16)?;
    Ok(SubsonicServer {
        id: row.get(0)?,
        name: row.get(1)?,
        url: row.get(2)?,
        username: row.get(3)?,
        password: None,
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

/// Stored `(url, username, password)` for a saved server.
fn load_credentials(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<(String, String, String), String> {
    let (url, username, password): (String, String, Option<String>) = conn
        .query_row(
            "SELECT url, username, password FROM subsonic_servers WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    Ok((url, username, password.unwrap_or_default()))
}

/// List all configured OpenSubsonic servers.
#[tauri::command]
pub async fn list_subsonic_servers(
    state: State<'_, AppState>,
) -> Result<Vec<SubsonicServer>, String> {
    crate::db::run_blocking(&state.db, |conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {SUBSONIC_SERVER_COLUMNS} FROM subsonic_servers ORDER BY created_at ASC, id ASC"
        ))?;
        let rows = stmt.query_map([], row_to_subsonic_server)?;
        Ok(rows.flatten().collect())
    })
    .await
    .map_err(|e| e.to_string())
}

/// Fields for [`save_subsonic_server`], bundled into one struct like
/// `SaveWebDavServerInput`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSubsonicServerInput {
    pub id: Option<i64>,
    pub name: String,
    pub url: String,
    pub username: String,
    /// Required for a new server; `None` on an edit keeps the stored password.
    pub password: Option<String>,
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
    let username = input.username.trim().to_string();
    if name.is_empty() {
        return Err("Enter a name for the server".into());
    }
    if username.is_empty() {
        return Err("Enter your username for the server".into());
    }
    match reqwest::Url::parse(&url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => {}
        _ => return Err("Enter a full server URL, e.g. https://music.example.com".into()),
    }
    if input.id.is_none() && input.password.as_deref().unwrap_or("").is_empty() {
        return Err("Enter your password for the server".into());
    }
    Ok((name, url, username))
}

/// Save (create or update) an OpenSubsonic server profile. Doesn't contact
/// the server — the settings UI tests the connection before saving, and
/// [`check_subsonic_connection`] refreshes the stored server details.
#[tauri::command]
pub async fn save_subsonic_server(
    input: SaveSubsonicServerInput,
    state: State<'_, AppState>,
) -> Result<SubsonicServer, String> {
    let (name, url, username) = validate_input(&input)?;
    let SaveSubsonicServerInput {
        id,
        password,
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

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let saved_id = if let Some(server_id) = id {
        conn.execute(
            "UPDATE subsonic_servers
             SET name = ?1, url = ?2, username = ?3, password = COALESCE(?4, password), enabled = ?5,
                 nickname = ?6, icon = ?7, color = ?8, auto_sync_enabled = ?9,
                 sync_interval_minutes = ?10, report_plays = ?11
             WHERE id = ?12",
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
                server_id
            ],
        )
        .map_err(|e| e.to_string())?;
        server_id
    } else {
        conn.execute(
            "INSERT INTO subsonic_servers (name, url, username, password, enabled, nickname, icon, color, auto_sync_enabled, sync_interval_minutes, report_plays)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                report_plays
            ],
        )
        .map_err(|e| e.to_string())?;
        conn.last_insert_rowid()
    };

    load_server(&conn, saved_id)
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

    let _ = app.emit("library-changed", ());
    Ok(())
}

/// Test a connection without saving: pings the server and discovers its
/// OpenSubsonic extensions. When editing a saved server (`id` set) and the
/// password field was left blank, the stored password is used.
#[tauri::command]
pub async fn test_subsonic_connection(
    url: String,
    username: String,
    password: Option<String>,
    id: Option<i64>,
    state: State<'_, AppState>,
) -> Result<ServerProbe, String> {
    let password = match (password.filter(|p| !p.is_empty()), id) {
        (Some(p), _) => p,
        (None, Some(id)) => {
            let conn = state.db.pool.get().map_err(|e| e.to_string())?;
            load_credentials(&conn, id)?.2
        }
        (None, None) => return Err("Enter your password for the server".into()),
    };
    tokio::task::spawn_blocking(move || {
        let client =
            SubsonicClient::new(&url, username.trim(), &password).map_err(|e| format!("{e:#}"))?;
        client.probe().map_err(|e| format!("{e:#}"))
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
        let (url, username, password) = load_credentials(&conn, id)?;
        drop(conn);

        let client =
            SubsonicClient::new(&url, &username, &password).map_err(|e| format!("{e:#}"))?;
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
                extensions_json TEXT DEFAULT '[]');
             INSERT INTO subsonic_servers (id, name, url, username, password, server_type, extensions_json)
             VALUES (1, 'Home', 'https://m.example.com', 'alice', 'secret', 'navidrome', '[\"songLyrics\"]');",
        )
        .unwrap();

        let server = load_server(&conn, 1).unwrap();
        assert_eq!(server.username, "alice");
        assert_eq!(server.server_type.as_deref(), Some("navidrome"));
        assert_eq!(server.extensions, vec!["songLyrics".to_string()]);
        assert!(server.report_plays);
        assert_eq!(server.password, None);
        let json = serde_json::to_string(&server).unwrap();
        assert!(!json.contains("secret"));
        assert!(json.contains("\"reportPlays\":true"));

        assert_eq!(
            load_credentials(&conn, 1).unwrap(),
            (
                "https://m.example.com".to_string(),
                "alice".to_string(),
                "secret".to_string()
            )
        );
    }
}
