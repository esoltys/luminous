use crate::db::Database;
pub use luminous_remote::musicbrainz::*;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

#[allow(dead_code)]
struct InFlightSession {
    code_verifier: String,
    state: String,
    redirect_uri: String,
    cancel_tx: Option<oneshot::Sender<()>>,
}

#[derive(Clone)]
pub struct MusicBrainzManager {
    remote: MusicBrainzClient,
    db: Arc<Database>,
    active_session: Arc<parking_lot::Mutex<Option<InFlightSession>>>,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl MusicBrainzManager {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            remote: MusicBrainzClient::new(),
            db,
            active_session: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    pub fn resolve_client_id(&self) -> String {
        // Priority:
        // 1. Stored custom client ID in app_state
        // 2. Runtime environment variable MUSICBRAINZ_CLIENT_ID
        // 3. Compile-time option_env!("MUSICBRAINZ_CLIENT_ID")
        // 4. Default built-in client ID
        if let Ok(Some(stored)) = self.get_stored_setting("mb_client_id") {
            let trimmed = stored.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        if let Ok(env_id) = std::env::var("MUSICBRAINZ_CLIENT_ID") {
            let trimmed = env_id.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        if let Some(opt_id) = option_env!("MUSICBRAINZ_CLIENT_ID") {
            let trimmed = opt_id.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        DEFAULT_MUSICBRAINZ_CLIENT_ID.to_string()
    }

    pub fn resolve_client_secret(&self) -> Option<String> {
        if let Ok(Some(stored)) = self.get_stored_setting("mb_client_secret") {
            let trimmed = stored.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }

        if let Ok(env_sec) = std::env::var("MUSICBRAINZ_CLIENT_SECRET") {
            let trimmed = env_sec.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }

        if let Some(opt_sec) = option_env!("MUSICBRAINZ_CLIENT_SECRET") {
            let trimmed = opt_sec.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }

        if !DEFAULT_MUSICBRAINZ_CLIENT_SECRET.trim().is_empty() {
            return Some(DEFAULT_MUSICBRAINZ_CLIENT_SECRET.to_string());
        }

        None
    }

    fn get_stored_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.db.pool.get().map_err(|e| e.to_string())?;
        let result: Result<String, rusqlite::Error> = conn.query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        );
        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set_stored_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.db.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn remove_stored_setting(&self, key: &str) -> Result<(), String> {
        let conn = self.db.pool.get().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM app_state WHERE key = ?1",
            rusqlite::params![key],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Master Online/Offline toggle (#1398); account traffic is suspended while Offline.
    fn is_online(&self) -> bool {
        self.db
            .pool
            .get()
            .map(|conn| crate::commands::context::is_online_enabled(&conn))
            .unwrap_or(true)
    }

    pub async fn start_login(
        &self,
        app: AppHandle,
        prefer_loopback: bool,
    ) -> Result<String, String> {
        if !self.is_online() {
            return Err(crate::commands::context::OFFLINE_ERROR.to_string());
        }
        let client_id = self.resolve_client_id();
        let code_verifier = generate_pkce_verifier();
        let code_challenge = generate_pkce_challenge(&code_verifier);
        let state = generate_state();

        let redirect_uri = if prefer_loopback {
            LOOPBACK_REDIRECT_URI.to_string()
        } else {
            OOB_REDIRECT_URI.to_string()
        };

        let auth_url = build_authorization_url(&client_id, &redirect_uri, &state, &code_challenge)?;

        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

        {
            let mut session_guard = self.active_session.lock();
            if let Some(prev) = session_guard.take() {
                if let Some(tx) = prev.cancel_tx {
                    let _ = tx.send(());
                }
            }
            *session_guard = Some(InFlightSession {
                code_verifier,
                state: state.clone(),
                redirect_uri: redirect_uri.clone(),
                cancel_tx: Some(cancel_tx),
            });
        }

        // If loopback is preferred, launch the local HTTP listener
        if prefer_loopback {
            let manager = self.clone();
            let app_clone = app.clone();
            let expected_state = state.clone();

            tokio::spawn(async move {
                let listener = match TcpListener::bind(("127.0.0.1", LOOPBACK_PORT)).await {
                    Ok(l) => l,
                    Err(e) => {
                        log::warn!("MusicBrainz loopback listener could not bind to port {LOOPBACK_PORT}: {e}");
                        return;
                    }
                };

                log::info!("MusicBrainz loopback listener running on 127.0.0.1:{LOOPBACK_PORT}");

                tokio::select! {
                    _ = &mut cancel_rx => {
                        log::debug!("MusicBrainz loopback listener cancelled");
                    }
                    accept_res = listener.accept() => {
                        if let Ok((mut stream, _)) = accept_res {
                            let mut buf = [0u8; 4096];
                            let n = match stream.read(&mut buf).await {
                                Ok(n) if n > 0 => n,
                                _ => return,
                            };
                            let req = String::from_utf8_lossy(&buf[..n]);

                            // Parse request line e.g. GET /oauth/callback?code=xxx&state=yyy HTTP/1.1
                            let query_str = req
                                .lines()
                                .next()
                                .and_then(|line| line.split_whitespace().nth(1))
                                .and_then(|uri| uri.split('?').nth(1))
                                .unwrap_or("");

                            let mut parsed_code = None;
                            let mut parsed_state = None;

                            for part in query_str.split('&') {
                                if let Some((k, v)) = part.split_once('=') {
                                    if k == "code" {
                                        parsed_code = Some(v.to_string());
                                    } else if k == "state" {
                                        parsed_state = Some(v.to_string());
                                    }
                                }
                            }

                            let html_success = r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Luminous - MusicBrainz Authorization</title>
  <style>
    body { font-family: system-ui, -apple-system, sans-serif; background: #0c0e14; color: #f3f4f6; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }
    .card { background: #161922; border: 1px solid rgba(255,255,255,0.1); border-radius: 16px; padding: 32px 40px; text-align: center; max-width: 420px; box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.5); }
    h1 { font-size: 20px; color: #ba478f; margin: 0 0 12px; }
    p { font-size: 14px; color: #9ca3af; margin: 0 0 20px; line-height: 1.5; }
    .badge { display: inline-block; padding: 6px 14px; background: rgba(186, 71, 143, 0.15); color: #eb743b; border-radius: 9999px; font-size: 12px; font-weight: 600; }
  </style>
</head>
<body>
  <div class="card">
    <div class="badge">Luminous Music Player</div>
    <h1>Authorization Successful</h1>
    <p>Your MusicBrainz account is connected! You can close this tab and return to Luminous.</p>
  </div>
</body>
</html>"#;

                            let html_error = r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Luminous - Authorization Failed</title>
  <style>
    body { font-family: system-ui, -apple-system, sans-serif; background: #0c0e14; color: #f3f4f6; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }
    .card { background: #161922; border: 1px solid rgba(255,255,255,0.1); border-radius: 16px; padding: 32px 40px; text-align: center; max-width: 420px; }
    h1 { font-size: 20px; color: #ef4444; margin: 0 0 12px; }
    p { font-size: 14px; color: #9ca3af; margin: 0; }
  </style>
</head>
<body>
  <div class="card">
    <h1>Authorization Failed</h1>
    <p>The state returned did not match or authorization was denied. Please return to Luminous and try again.</p>
  </div>
</body>
</html>"#;

                            if let (Some(code), Some(st)) = (parsed_code, parsed_state) {
                                if st == expected_state {
                                    let response = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        html_success.len(),
                                        html_success
                                    );
                                    let _ = stream.write_all(response.as_bytes()).await;
                                    let _ = stream.flush().await;

                                    match manager.submit_auth_code(&code, &app_clone).await {
                                        Ok(auth_state) => {
                                            let _ = app_clone.emit("musicbrainz-auth-changed", &auth_state);
                                        }
                                        Err(err) => {
                                            log::error!("Failed to complete MusicBrainz code exchange: {err}");
                                            let _ = app_clone.emit("musicbrainz-auth-error", &err);
                                        }
                                    }
                                    return;
                                }
                            }

                            let response = format!(
                                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                html_error.len(),
                                html_error
                            );
                            let _ = stream.write_all(response.as_bytes()).await;
                            let _ = stream.flush().await;
                        }
                    }
                }
            });
        }

        Ok(auth_url)
    }

    pub fn cancel_login(&self) {
        let mut session_guard = self.active_session.lock();
        if let Some(session) = session_guard.take() {
            if let Some(tx) = session.cancel_tx {
                let _ = tx.send(());
            }
        }
    }

    pub async fn submit_auth_code(
        &self,
        code: &str,
        app: &AppHandle,
    ) -> Result<MusicBrainzAuthState, String> {
        if !self.is_online() {
            return Err(crate::commands::context::OFFLINE_ERROR.to_string());
        }
        let clean_code = code.trim();
        if clean_code.is_empty() {
            return Err("Authorization code cannot be empty".to_string());
        }

        let session = {
            let mut guard = self.active_session.lock();
            guard.take()
        };

        let (code_verifier, redirect_uri) = match session {
            Some(s) => (s.code_verifier, s.redirect_uri),
            None => {
                // If submitted without active session (e.g. app restart or manual paste),
                // use OOB redirect URI
                ("".to_string(), OOB_REDIRECT_URI.to_string())
            }
        };

        let client_id = self.resolve_client_id();
        let client_secret = self.resolve_client_secret();

        let token_data = self
            .remote
            .exchange_code(
                &client_id,
                client_secret.as_deref(),
                clean_code,
                &redirect_uri,
                if code_verifier.is_empty() {
                    None
                } else {
                    Some(&code_verifier)
                },
            )
            .await?;

        let expires_at = token_data
            .expires_in
            .map(|sec| now_unix() + sec)
            .unwrap_or_else(|| now_unix() + 3600);

        self.set_stored_setting("mb_access_token", &token_data.access_token)?;
        if let Some(ref refresh) = token_data.refresh_token {
            self.set_stored_setting("mb_refresh_token", refresh)?;
        }
        self.set_stored_setting("mb_token_expires_at", &expires_at.to_string())?;

        // Fetch user info with the access token
        let user_info = self
            .remote
            .fetch_user_info(&token_data.access_token)
            .await?;
        if let Some(ref u) = user_info.sub {
            self.set_stored_setting("mb_username", u)?;
        }
        if let Some(ref email) = user_info.email {
            self.set_stored_setting("mb_email", email)?;
        }

        // Fetch initial user stats
        if let Some(ref u) = user_info.sub {
            let _ = self
                .fetch_and_cache_stats(u, &token_data.access_token)
                .await;
        }

        let auth_state = MusicBrainzAuthState {
            is_logged_in: true,
            username: user_info.sub,
            email: user_info.email,
        };

        let _ = app.emit("musicbrainz-auth-changed", &auth_state);
        Ok(auth_state)
    }

    async fn ensure_valid_token(&self) -> Result<String, String> {
        let access_token = self
            .get_stored_setting("mb_access_token")?
            .ok_or_else(|| "Not logged in to MusicBrainz".to_string())?;

        let expires_at = self
            .get_stored_setting("mb_token_expires_at")?
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);

        // If token expires in less than 5 minutes, attempt refresh
        if now_unix() + 300 >= expires_at {
            if let Ok(Some(refresh_token)) = self.get_stored_setting("mb_refresh_token") {
                if let Ok(new_token) = self.refresh_access_token(&refresh_token).await {
                    return Ok(new_token);
                }
            }
        }

        Ok(access_token)
    }

    async fn refresh_access_token(&self, refresh_token: &str) -> Result<String, String> {
        let client_id = self.resolve_client_id();
        let client_secret = self.resolve_client_secret();

        let token_data = self
            .remote
            .refresh_access_token(&client_id, client_secret.as_deref(), refresh_token)
            .await?;

        let expires_at = token_data
            .expires_in
            .map(|sec| now_unix() + sec)
            .unwrap_or_else(|| now_unix() + 3600);

        self.set_stored_setting("mb_access_token", &token_data.access_token)?;
        if let Some(ref refresh) = token_data.refresh_token {
            self.set_stored_setting("mb_refresh_token", refresh)?;
        }
        self.set_stored_setting("mb_token_expires_at", &expires_at.to_string())?;

        Ok(token_data.access_token)
    }

    pub async fn get_auth_state(&self) -> MusicBrainzAuthState {
        let token = match self.get_stored_setting("mb_access_token") {
            Ok(Some(t)) if !t.is_empty() => t,
            _ => return MusicBrainzAuthState::default(),
        };

        // Try ensuring token is refreshed if expired
        let is_valid = if let Ok(ref refreshed) = self.ensure_valid_token().await {
            !refreshed.is_empty()
        } else {
            !token.is_empty()
        };

        if !is_valid {
            return MusicBrainzAuthState::default();
        }

        let username = self.get_stored_setting("mb_username").ok().flatten();
        let email = self.get_stored_setting("mb_email").ok().flatten();

        MusicBrainzAuthState {
            is_logged_in: true,
            username,
            email,
        }
    }

    pub async fn get_user_stats(
        &self,
        force_refresh: bool,
    ) -> Result<MusicBrainzUserStats, String> {
        let username = self
            .get_stored_setting("mb_username")?
            .ok_or_else(|| "Not logged in to MusicBrainz".to_string())?;

        if !force_refresh {
            if let Ok(Some(cached_json)) = self.get_stored_setting("mb_cached_stats") {
                if let Ok(stats) = serde_json::from_str::<MusicBrainzUserStats>(&cached_json) {
                    // Return cached stats if younger than 1 hour
                    if now_unix() - stats.cached_at < 3600 {
                        return Ok(stats);
                    }
                }
            }
        }

        if !self.is_online() {
            return Err(crate::commands::context::OFFLINE_ERROR.to_string());
        }
        let token = self.ensure_valid_token().await?;
        self.fetch_and_cache_stats(&username, &token).await
    }

    async fn fetch_and_cache_stats(
        &self,
        username: &str,
        access_token: &str,
    ) -> Result<MusicBrainzUserStats, String> {
        let (collections_count, releases_count) = self
            .remote
            .fetch_collections(username, access_token)
            .await?;

        let stats = MusicBrainzUserStats {
            username: username.to_string(),
            collections_count,
            releases_count,
            cached_at: now_unix(),
        };

        if let Ok(serialized) = serde_json::to_string(&stats) {
            let _ = self.set_stored_setting("mb_cached_stats", &serialized);
        }

        Ok(stats)
    }

    pub async fn logout(&self, app: &AppHandle) -> Result<(), String> {
        // Token revocation is best-effort and skipped offline; local credentials
        // are cleared either way.
        let stored_token = if self.is_online() {
            self.get_stored_setting("mb_refresh_token")
                .or_else(|_| self.get_stored_setting("mb_access_token"))
        } else {
            Ok(None)
        };
        if let Ok(Some(token)) = stored_token {
            let client_id = self.resolve_client_id();
            let client_secret = self.resolve_client_secret();
            let _ = self
                .remote
                .revoke_token(&client_id, client_secret.as_deref(), &token)
                .await;
        }

        let _ = self.remove_stored_setting("mb_access_token");
        let _ = self.remove_stored_setting("mb_refresh_token");
        let _ = self.remove_stored_setting("mb_token_expires_at");
        let _ = self.remove_stored_setting("mb_username");
        let _ = self.remove_stored_setting("mb_email");
        let _ = self.remove_stored_setting("mb_cached_stats");

        let auth_state = MusicBrainzAuthState::default();
        let _ = app.emit("musicbrainz-auth-changed", &auth_state);
        Ok(())
    }

    pub fn get_app_credentials(&self) -> (String, bool) {
        let id = self.resolve_client_id();
        let has_secret = self.resolve_client_secret().is_some();
        (id, has_secret)
    }

    pub fn set_app_credentials(
        &self,
        client_id: String,
        client_secret: String,
    ) -> Result<(), String> {
        self.set_stored_setting("mb_client_id", client_id.trim())?;
        if !client_secret.trim().is_empty() {
            self.set_stored_setting("mb_client_secret", client_secret.trim())?;
        } else {
            let _ = self.remove_stored_setting("mb_client_secret");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let verifier = generate_pkce_verifier();
        assert!(!verifier.is_empty());
        assert_eq!(verifier.len(), 43);

        let challenge = generate_pkce_challenge(&verifier);
        assert!(!challenge.is_empty());
        assert_eq!(challenge.len(), 43);

        let challenge2 = generate_pkce_challenge(&verifier);
        assert_eq!(challenge, challenge2);
    }

    #[test]
    fn test_state_generation() {
        let state1 = generate_state();
        let state2 = generate_state();
        assert_eq!(state1.len(), 22);
        assert_ne!(state1, state2);
    }

    #[test]
    fn test_auth_state_default() {
        let state = MusicBrainzAuthState::default();
        assert!(!state.is_logged_in);
        assert!(state.username.is_none());
        assert!(state.email.is_none());
    }

    #[test]
    fn test_user_stats_serialization() {
        let stats = MusicBrainzUserStats {
            username: "testuser".to_string(),
            collections_count: 5,
            releases_count: 42,
            cached_at: 123456789,
        };
        let json = serde_json::to_string(&stats).expect("serialize stats");
        let deserialized: MusicBrainzUserStats =
            serde_json::from_str(&json).expect("deserialize stats");
        assert_eq!(deserialized, stats);
    }
}
