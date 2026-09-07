//! ListenBrainz scrobbling service with persistent offline cache (#83).
//!
//! Submits listens to ListenBrainz via their HTTP API with offline caching in
//! SQLite. Scrobbles are written to `scrobble_cache` first and drained asynchronously,
//! ensuring listens survive offline sessions and application restarts.

use crate::db::Database;
use crate::models::{Song, SongSource};
use anyhow::Result;
use reqwest::Client;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const LISTENBRAINZ_API_BASE: &str = "https://api.listenbrainz.org/1";
const SUBMISSION_CLIENT_NAME: &str = "Luminous";

/// User configuration for scrobbling services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrobblerSettings {
    pub listenbrainz_enabled: bool,
    pub listenbrainz_token: String,
    pub listenbrainz_username: Option<String>,
    pub scrobble_now_playing: bool,
    pub scrobble_ratings: bool,
    pub scrobble_paused: bool,
    pub min_duration_secs: u32,
}

impl Default for ScrobblerSettings {
    fn default() -> Self {
        Self {
            listenbrainz_enabled: false,
            listenbrainz_token: String::new(),
            listenbrainz_username: None,
            scrobble_now_playing: true,
            scrobble_ratings: true,
            scrobble_paused: false,
            min_duration_secs: 30,
        }
    }
}

/// A cached scrobble waiting to be sent to ListenBrainz.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrobbleCacheEntry {
    pub id: i64,
    pub service: String,
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub track_number: Option<i32>,
    pub recording_mbid: Option<String>,
    pub release_mbid: Option<String>,
    pub artist_mbids: Option<String>,
    pub release_group_mbid: Option<String>,
    pub track_mbid: Option<String>,
    pub listened_at: i64,
    pub attempts: i32,
    pub last_attempt: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
}

/// Live status of the offline scrobble cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrobbleCacheStatus {
    pub pending_count: i64,
    pub last_error: Option<String>,
    pub last_attempt: Option<i64>,
}

#[derive(Deserialize)]
struct ValidateTokenResponse {
    valid: bool,
    user_name: Option<String>,
    message: Option<String>,
}

/// JSON payload structure for ListenBrainz `/1/submit-listens`.
#[derive(Serialize)]
struct ListenBrainzSubmitRequest<'a> {
    listen_type: &'a str,
    payload: Vec<ListenBrainzPayload<'a>>,
}

#[derive(Serialize)]
struct ListenBrainzPayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    listened_at: Option<i64>,
    track_metadata: ListenBrainzTrackMetadata<'a>,
}

#[derive(Serialize)]
struct ListenBrainzTrackMetadata<'a> {
    artist_name: &'a str,
    track_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_name: Option<&'a str>,
    additional_info: ListenBrainzAdditionalInfo<'a>,
}

#[derive(Serialize)]
struct ListenBrainzAdditionalInfo<'a> {
    submission_client: &'a str,
    submission_client_version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tracknumber: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recording_mbid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_mbid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    artist_mbids: Option<Vec<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_group_mbid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    track_mbid: Option<&'a str>,
}

#[derive(Serialize)]
struct FeedbackRequest {
    recording_mbid: String,
    score: i32,
}

/// Central manager orchestrating ListenBrainz API calls and the offline cache.
pub struct ScrobblerManager {
    db: Arc<Database>,
    client: Client,
    settings: Arc<Mutex<ScrobblerSettings>>,
}

impl ScrobblerManager {
    pub fn new(db: Arc<Database>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(12))
            .user_agent(concat!("LuminousMusicPlayer/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();

        let initial_settings = Self::load_settings_from_db(&db);

        Self {
            db,
            client,
            settings: Arc::new(Mutex::new(initial_settings)),
        }
    }

    /// Load persisted scrobbler settings from the `app_state` key-value table.
    fn load_settings_from_db(db: &Database) -> ScrobblerSettings {
        let mut settings = ScrobblerSettings::default();
        if let Ok(conn) = db.pool.get() {
            let mut stmt = conn
                .prepare("SELECT key, value FROM app_state WHERE key LIKE 'scrobbler_%' OR key LIKE 'listenbrainz_%'")
                .ok();
            if let Some(mut stmt) = stmt.take() {
                let rows = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .ok();
                if let Some(rows) = rows {
                    for (k, v) in rows.flatten() {
                        match k.as_str() {
                            "listenbrainz_enabled" => settings.listenbrainz_enabled = v == "true" || v == "1",
                            "listenbrainz_token" => settings.listenbrainz_token = v,
                            "listenbrainz_username" => settings.listenbrainz_username = if v.is_empty() { None } else { Some(v) },
                            "scrobbler_now_playing" => settings.scrobble_now_playing = v != "false" && v != "0",
                            "scrobbler_ratings" => settings.scrobble_ratings = v != "false" && v != "0",
                            "scrobbler_paused" => settings.scrobble_paused = v == "true" || v == "1",
                            "scrobbler_min_duration_secs" => {
                                if let Ok(n) = v.parse::<u32>() {
                                    settings.min_duration_secs = n;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        settings
    }

    /// Persist scrobbler settings into `app_state`.
    pub async fn save_settings(&self, new_settings: ScrobblerSettings) -> Result<()> {
        if let Ok(conn) = self.db.pool.get() {
            let pairs: &[(&str, String)] = &[
                ("listenbrainz_enabled", new_settings.listenbrainz_enabled.to_string()),
                ("listenbrainz_token", new_settings.listenbrainz_token.clone()),
                ("listenbrainz_username", new_settings.listenbrainz_username.clone().unwrap_or_default()),
                ("scrobbler_now_playing", new_settings.scrobble_now_playing.to_string()),
                ("scrobbler_ratings", new_settings.scrobble_ratings.to_string()),
                ("scrobbler_paused", new_settings.scrobble_paused.to_string()),
                ("scrobbler_min_duration_secs", new_settings.min_duration_secs.to_string()),
            ];
            for (k, v) in pairs {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
                    params![k, v],
                );
            }
        }
        let mut s = self.settings.lock().await;
        *s = new_settings;
        Ok(())
    }

    pub async fn get_settings(&self) -> ScrobblerSettings {
        self.settings.lock().await.clone()
    }

    /// Validate a ListenBrainz user token by hitting `/1/validate-token`.
    pub async fn validate_token(&self, token: &str) -> Result<String, String> {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return Err("Token cannot be empty".into());
        }

        let url = format!("{LISTENBRAINZ_API_BASE}/validate-token?token={trimmed}");
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Network request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("ListenBrainz returned HTTP {}", resp.status()));
        }

        let body: ValidateTokenResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {e}"))?;

        if !body.valid {
            let msg = body.message.unwrap_or_else(|| "Invalid token".into());
            return Err(msg);
        }

        let username = body
            .user_name
            .ok_or_else(|| "Validated, but no username returned".to_string())?;

        // If validation succeeds and matches current token, update username
        let updated_settings = {
            let mut s = self.settings.lock().await;
            if s.listenbrainz_token == trimmed {
                s.listenbrainz_username = Some(username.clone());
                Some(s.clone())
            } else {
                None
            }
        };

        if let Some(s) = updated_settings {
            let _ = self.save_settings(s).await;
        }

        Ok(username)
    }

    /// Submit a "Playing Now" listen to ListenBrainz when track playback starts.
    pub async fn on_now_playing(&self, song: &Song) {
        let settings = self.get_settings().await;
        if !settings.listenbrainz_enabled
            || settings.scrobble_paused
            || !settings.scrobble_now_playing
            || settings.listenbrainz_token.trim().is_empty()
        {
            return;
        }

        // Exclude radio streams
        if matches!(song.source, SongSource::Stream | SongSource::SomaFm | SongSource::RadioParadise | SongSource::RadioBrowser) {
            return;
        }

        let artist = song.artist.as_deref().unwrap_or("Unknown Artist");
        let title = song.title.as_deref().unwrap_or("Unknown Track");
        let release = song.album.as_deref();
        let duration_ms = song.length_nanosec.map(|ns| ns / 1_000_000);
        let tracknumber = song.track;

        let artist_mbids_vec: Option<Vec<&str>> = song
            .musicbrainz_artist_id
            .as_deref()
            .map(|id| vec![id]);

        let payload = ListenBrainzSubmitRequest {
            listen_type: "playing_now",
            payload: vec![ListenBrainzPayload {
                listened_at: None,
                track_metadata: ListenBrainzTrackMetadata {
                    artist_name: artist,
                    track_name: title,
                    release_name: release,
                    additional_info: ListenBrainzAdditionalInfo {
                        submission_client: SUBMISSION_CLIENT_NAME,
                        submission_client_version: env!("CARGO_PKG_VERSION"),
                        duration_ms,
                        tracknumber,
                        recording_mbid: song.musicbrainz_recording_id.as_deref(),
                        release_mbid: song.musicbrainz_album_id.as_deref(),
                        artist_mbids: artist_mbids_vec,
                        release_group_mbid: song.musicbrainz_release_group_id.as_deref(),
                        track_mbid: song.musicbrainz_track_id.as_deref(),
                    },
                },
            }],
        };

        let payload_json = match serde_json::to_value(&payload) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("Failed to serialize ListenBrainz now-playing payload: {e}");
                return;
            }
        };

        let token = settings.listenbrainz_token.trim().to_string();
        let client = self.client.clone();

        tauri::async_runtime::spawn(async move {
            let res = client
                .post(format!("{LISTENBRAINZ_API_BASE}/submit-listens"))
                .header("Authorization", format!("Token {token}"))
                .json(&payload_json)
                .send()
                .await;

            match res {
                Ok(resp) if resp.status().is_success() => {
                    log::debug!("ListenBrainz now-playing submitted successfully");
                }
                Ok(resp) => {
                    log::warn!("ListenBrainz now-playing returned status: {}", resp.status());
                }
                Err(e) => {
                    log::warn!("Failed to submit now-playing to ListenBrainz: {e}");
                }
            }
        });
    }

    /// Enqueue a scrobble when the 50% scrobble point is reached, then trigger a flush.
    pub async fn on_scrobble_point(&self, song: &Song, listened_at: i64) {
        let settings = self.get_settings().await;
        if !settings.listenbrainz_enabled || settings.scrobble_paused {
            return;
        }

        // Radio / live stream tracks are not scrobbled
        if matches!(song.source, SongSource::Stream | SongSource::SomaFm | SongSource::RadioParadise | SongSource::RadioBrowser) {
            return;
        }

        // Guard: check minimum duration
        if let Some(ns) = song.length_nanosec {
            let secs = (ns as u64) / 1_000_000_000;
            if secs < (settings.min_duration_secs as u64) {
                log::debug!("Song duration ({}s) is below scrobble minimum ({}s); skipping scrobble", secs, settings.min_duration_secs);
                return;
            }
        }

        let artist = song.artist.clone().unwrap_or_else(|| "Unknown Artist".into());
        let track = song.title.clone().unwrap_or_else(|| "Unknown Track".into());
        let album = song.album.clone();
        let duration_ms = song.length_nanosec.map(|ns| ns / 1_000_000);
        let track_number = song.track;

        let recording_mbid = song.musicbrainz_recording_id.clone();
        let release_mbid = song.musicbrainz_album_id.clone();
        let artist_mbids = song.musicbrainz_artist_id.clone();
        let release_group_mbid = song.musicbrainz_release_group_id.clone();
        let track_mbid = song.musicbrainz_track_id.clone();

        // Always write to SQLite cache first
        let enqueue_res = if let Ok(conn) = self.db.pool.get() {
            conn.execute(
                "INSERT INTO scrobble_cache (
                    service, artist, track, album, duration_ms, track_number,
                    recording_mbid, release_mbid, artist_mbids, release_group_mbid, track_mbid,
                    listened_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    "listenbrainz",
                    artist,
                    track,
                    album,
                    duration_ms,
                    track_number,
                    recording_mbid,
                    release_mbid,
                    artist_mbids,
                    release_group_mbid,
                    track_mbid,
                    listened_at
                ],
            )
            .map_err(|e| e.to_string())
        } else {
            Err("Failed to acquire DB connection".into())
        };

        if let Err(e) = enqueue_res {
            log::error!("Failed to enqueue scrobble to scrobble_cache: {e}");
            return;
        }

        log::info!("Scrobble enqueued for '{} - {}' at timestamp {}", artist, track, listened_at);

        // Trigger asynchronous cache drain
        self.trigger_flush();
    }

    /// Submit love/feedback when song rating changes.
    pub async fn on_song_rating(&self, song: &Song, rating: f32) {
        let settings = self.get_settings().await;
        if !settings.listenbrainz_enabled || settings.scrobble_paused || !settings.scrobble_ratings {
            return;
        }

        let mbid = match &song.musicbrainz_recording_id {
            Some(id) if !id.trim().is_empty() => id.clone(),
            _ => return, // ListenBrainz recording feedback requires recording_mbid
        };

        let score = if rating >= 0.8 { 1 } else { 0 };
        let token = settings.listenbrainz_token.trim().to_string();
        if token.is_empty() {
            return;
        }

        let client = self.client.clone();
        tauri::async_runtime::spawn(async move {
            let payload = FeedbackRequest {
                recording_mbid: mbid,
                score,
            };
            let res = client
                .post(format!("{LISTENBRAINZ_API_BASE}/feedback/recording-feedback"))
                .header("Authorization", format!("Token {token}"))
                .json(&payload)
                .send()
                .await;

            if let Err(e) = res {
                log::warn!("Failed to submit rating feedback to ListenBrainz: {e}");
            }
        });
    }

    /// Retrieve live scrobble cache statistics.
    pub fn get_cache_status(&self) -> ScrobbleCacheStatus {
        let conn = match self.db.pool.get() {
            Ok(c) => c,
            Err(_) => {
                return ScrobbleCacheStatus {
                    pending_count: 0,
                    last_error: None,
                    last_attempt: None,
                }
            }
        };

        let pending_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM scrobble_cache", [], |r| r.get(0))
            .unwrap_or(0);

        let (last_error, last_attempt): (Option<String>, Option<i64>) = conn
            .query_row(
                "SELECT last_error, last_attempt FROM scrobble_cache WHERE last_attempt IS NOT NULL ORDER BY last_attempt DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((None, None));

        ScrobbleCacheStatus {
            pending_count,
            last_error,
            last_attempt,
        }
    }

    /// Spawn a flush in the background.
    pub fn trigger_flush(&self) {
        let client = self.client.clone();
        let db = self.db.clone();
        let settings_arc = self.settings.clone();

        tauri::async_runtime::spawn(async move {
            let settings = settings_arc.lock().await.clone();
            if !settings.listenbrainz_enabled
                || settings.scrobble_paused
                || settings.listenbrainz_token.trim().is_empty()
            {
                return;
            }
            if let Err(e) = Self::flush_cache_internal(&db, &client, &settings.listenbrainz_token).await {
                log::warn!("Scrobble cache flush finished with error: {e}");
            }
        });
    }

    /// Drain pending scrobbles from the database cache and submit them to ListenBrainz.
    pub async fn flush_cache_now(&self) -> Result<u32, String> {
        let settings = self.get_settings().await;
        if settings.listenbrainz_token.trim().is_empty() {
            return Err("ListenBrainz user token is not configured".into());
        }
        Self::flush_cache_internal(&self.db, &self.client, &settings.listenbrainz_token).await
    }

    async fn flush_cache_internal(db: &Database, client: &Client, token: &str) -> Result<u32, String> {
        let entries: Vec<ScrobbleCacheEntry> = {
            let conn = db.pool.get().map_err(|e| e.to_string())?;

            // Read up to 50 pending scrobbles
            let mut stmt = conn
                .prepare(
                    "SELECT id, service, artist, track, album, duration_ms, track_number,
                            recording_mbid, release_mbid, artist_mbids, release_group_mbid, track_mbid,
                            listened_at, attempts, last_attempt, last_error, created_at
                     FROM scrobble_cache
                     ORDER BY created_at ASC
                     LIMIT 50",
                )
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(ScrobbleCacheEntry {
                        id: row.get(0)?,
                        service: row.get(1)?,
                        artist: row.get(2)?,
                        track: row.get(3)?,
                        album: row.get(4)?,
                        duration_ms: row.get(5)?,
                        track_number: row.get(6)?,
                        recording_mbid: row.get(7)?,
                        release_mbid: row.get(8)?,
                        artist_mbids: row.get(9)?,
                        release_group_mbid: row.get(10)?,
                        track_mbid: row.get(11)?,
                        listened_at: row.get(12)?,
                        attempts: row.get(13)?,
                        last_attempt: row.get(14)?,
                        last_error: row.get(15)?,
                        created_at: row.get(16)?,
                    })
                })
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .collect();

            rows
        };

        if entries.is_empty() {
            return Ok(0);
        }

        let listen_type = if entries.len() == 1 { "single" } else { "import" };

        let payload_items: Vec<ListenBrainzPayload<'_>> = entries
            .iter()
            .map(|e| {
                let artist_mbids_vec = e.artist_mbids.as_deref().map(|id| vec![id]);
                ListenBrainzPayload {
                    listened_at: Some(e.listened_at),
                    track_metadata: ListenBrainzTrackMetadata {
                        artist_name: &e.artist,
                        track_name: &e.track,
                        release_name: e.album.as_deref(),
                        additional_info: ListenBrainzAdditionalInfo {
                            submission_client: SUBMISSION_CLIENT_NAME,
                            submission_client_version: env!("CARGO_PKG_VERSION"),
                            duration_ms: e.duration_ms,
                            tracknumber: e.track_number,
                            recording_mbid: e.recording_mbid.as_deref(),
                            release_mbid: e.release_mbid.as_deref(),
                            artist_mbids: artist_mbids_vec,
                            release_group_mbid: e.release_group_mbid.as_deref(),
                            track_mbid: e.track_mbid.as_deref(),
                        },
                    },
                }
            })
            .collect();

        let request = ListenBrainzSubmitRequest {
            listen_type,
            payload: payload_items,
        };

        let now = chrono::Utc::now().timestamp();
        let resp = client
            .post(format!("{LISTENBRAINZ_API_BASE}/submit-listens"))
            .header("Authorization", format!("Token {token}"))
            .json(&request)
            .send()
            .await;

        let conn = db.pool.get().map_err(|e| e.to_string())?;

        match resp {
            Ok(r) if r.status().is_success() => {
                let ids: Vec<i64> = entries.iter().map(|e| e.id).collect();
                let flushed_count = ids.len() as u32;

                // Delete sent entries
                for id in ids {
                    let _ = conn.execute("DELETE FROM scrobble_cache WHERE id = ?1", params![id]);
                }
                log::info!("Successfully flushed {flushed_count} scrobbles to ListenBrainz");
                Ok(flushed_count)
            }
            Ok(r) => {
                let status = r.status().as_u16();
                let err_text = r.text().await.unwrap_or_else(|_| format!("HTTP error {status}"));
                log::warn!("ListenBrainz returned {status} on submit: {err_text}");

                for e in &entries {
                    let _ = conn.execute(
                        "UPDATE scrobble_cache SET attempts = attempts + 1, last_attempt = ?1, last_error = ?2 WHERE id = ?3",
                        params![now, format!("HTTP {status}: {err_text}"), e.id],
                    );
                }
                Err(format!("HTTP {status}: {err_text}"))
            }
            Err(e) => {
                let err_msg = e.to_string();
                log::warn!("Failed to send scrobbles to ListenBrainz: {err_msg}");

                for e in &entries {
                    let _ = conn.execute(
                        "UPDATE scrobble_cache SET attempts = attempts + 1, last_attempt = ?1, last_error = ?2 WHERE id = ?3",
                        params![now, err_msg, e.id],
                    );
                }
                Err(err_msg)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_listenbrainz_payload_serialization() {
        let payload = ListenBrainzSubmitRequest {
            listen_type: "single",
            payload: vec![ListenBrainzPayload {
                listened_at: Some(1725690000),
                track_metadata: ListenBrainzTrackMetadata {
                    artist_name: "Radiohead",
                    track_name: "Karma Police",
                    release_name: Some("OK Computer"),
                    additional_info: ListenBrainzAdditionalInfo {
                        submission_client: SUBMISSION_CLIENT_NAME,
                        submission_client_version: "2.0.0",
                        duration_ms: Some(264000),
                        tracknumber: Some(6),
                        recording_mbid: Some("0946b553-7a96-4a49-9cfd-f952fdbbe7e8"),
                        release_mbid: Some("e7011d8d-bf34-4530-9b44-325d2b781bc5"),
                        artist_mbids: Some(vec!["a74b1b7f-71a5-4011-9441-d0b5e4122711"]),
                        release_group_mbid: None,
                        track_mbid: None,
                    },
                },
            }],
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"listen_type\":\"single\""));
        assert!(json.contains("\"artist_name\":\"Radiohead\""));
        assert!(json.contains("\"track_name\":\"Karma Police\""));
        assert!(json.contains("\"recording_mbid\":\"0946b553-7a96-4a49-9cfd-f952fdbbe7e8\""));
        assert!(json.contains("\"listened_at\":1725690000"));
    }

    #[test]
    fn test_validate_token_json_parsing() {
        let json_success = r#"{"valid": true, "user_name": "soltys", "message": "Token is valid."}"#;
        let parsed: ValidateTokenResponse = serde_json::from_str(json_success).unwrap();
        assert!(parsed.valid);
        assert_eq!(parsed.user_name.as_deref(), Some("soltys"));

        let json_invalid = r#"{"valid": false, "user_name": null, "message": "Invalid token."}"#;
        let parsed_invalid: ValidateTokenResponse = serde_json::from_str(json_invalid).unwrap();
        assert!(!parsed_invalid.valid);
        assert_eq!(parsed_invalid.user_name, None);
    }
}
