//! OpenSubsonic / Subsonic REST API integration (#916, #1767).
//!
//! Protocol wire mechanics, URL construction, and response parsing live in
//! [`luminous_remote::subsonic`]. This module hosts SQLite credential loading,
//! track URI stream URL resolution, and the SQLite sync / background reporting
//! pipelines.

use anyhow::{anyhow, Context, Result};
pub use luminous_remote::subsonic::*;

pub mod report;
pub mod sync;

/// A saved server's `(url, credentials)`. `Ok(None)` when no server has that id.
pub fn load_auth(conn: &rusqlite::Connection, server_id: i64) -> Result<Option<(String, Auth)>> {
    use rusqlite::OptionalExtension;
    let row: Option<(String, String, Option<String>, String)> = conn
        .query_row(
            "SELECT url, username, password, auth_mode FROM subsonic_servers WHERE id = ?1",
            rusqlite::params![server_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    Ok(row.map(|(url, username, secret, mode)| {
        let auth = Auth::new(
            AuthMode::from_db(&mode),
            &username,
            &secret.unwrap_or_default(),
        );
        (url, auth)
    }))
}

/// Resolves a stored `subsonic://{server_id}/{track_id}` path to a freshly
/// signed stream URL, using the server's saved credentials. Called by
/// `media_source::open` at open time (#1163).
pub fn resolve_stream_url(conn: &rusqlite::Connection, path: &str) -> Result<String> {
    let (server_id, track_id) =
        parse_track_uri(path).ok_or_else(|| anyhow!("Invalid Subsonic track path '{path}'"))?;
    // Every error names the track by `path` (never the signed URL), so the
    // opener can pass them through without wrapping them again.
    let (url, auth) = load_auth(conn, server_id)
        .with_context(|| format!("Couldn't read the Subsonic server for '{path}'"))?
        .ok_or_else(|| anyhow!("The Subsonic server for '{path}' has been removed"))?;
    stream_url(&url, &auth, &track_id)
        .with_context(|| format!("Couldn't build a stream URL for '{path}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(url: &str) -> std::collections::HashMap<String, String> {
        reqwest::Url::parse(url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect()
    }

    fn test_password() -> String {
        format!("{}_{}", "mock", "pwd")
    }

    fn test_api_key() -> String {
        format!("{}_{}", "mock", "key")
    }

    fn servers_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE subsonic_servers (id INTEGER PRIMARY KEY, url TEXT, username TEXT, password TEXT,
                auth_mode TEXT NOT NULL DEFAULT 'token');",
        )
        .unwrap();
        let pwd = test_password();
        let key = test_api_key();
        conn.execute(
            "INSERT INTO subsonic_servers (id, url, username, password) VALUES (3, 'https://music.example.com', 'alice', ?1)",
            [&pwd],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO subsonic_servers VALUES (4, 'https://keys.example.com', '', ?1, 'apiKey')",
            [&key],
        )
        .unwrap();
        conn
    }

    #[test]
    fn resolve_stream_url_signs_with_the_saved_server() {
        let conn = servers_db();
        let url = resolve_stream_url(&conn, &track_uri(3, "a b")).unwrap();
        assert!(url.starts_with("https://music.example.com/rest/stream.view?"));
        assert!(url.contains("id=a+b") || url.contains("id=a%20b"));
        assert!(url.contains("u=alice"));
    }

    #[test]
    fn resolve_stream_url_uses_the_saved_auth_mode() {
        let conn = servers_db();
        let q = query(&resolve_stream_url(&conn, &track_uri(4, "x")).unwrap());
        assert_eq!(q["apiKey"], test_api_key());
        assert!(!q.contains_key("u"));
    }

    #[test]
    fn resolve_stream_url_reports_a_removed_server() {
        let conn = servers_db();
        let err = resolve_stream_url(&conn, "subsonic://9/abc").unwrap_err();
        assert!(err.to_string().contains("has been removed"), "{err}");
        assert!(resolve_stream_url(&conn, "/music/a.flac").is_err());
    }
}
