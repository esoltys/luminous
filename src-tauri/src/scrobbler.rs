//! ListenBrainz scrobbling service with persistent offline cache (#83).
//!
//! Submits listens to ListenBrainz via their HTTP API with offline caching in
//! SQLite. Scrobbles are written to `scrobble_cache` first and drained asynchronously,
//! ensuring listens survive offline sessions and application restarts.

use crate::db::Database;
use crate::models::{Song, SongSource};
use anyhow::Result;
pub use luminous_remote::lastfm::{
    LastFmClient, LastFmLovedTrack, LastFmScrobbleResponse, LastFmSession, LastFmTrack,
    DEFAULT_LASTFM_API_KEY, DEFAULT_LASTFM_API_SECRET,
};
pub use luminous_remote::scrobbler::*;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// User configuration for scrobbling services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrobblerSettings {
    pub listenbrainz_enabled: bool,
    pub listenbrainz_token: String,
    pub listenbrainz_username: Option<String>,
    /// CritiqueBrainz profile URL or user UUID, pasted by the user: CritiqueBrainz
    /// can't resolve a username to the UUID its review API filters on (#1386).
    pub critiquebrainz_user_id: String,

    // Last.fm settings (#1954)
    pub lastfm_enabled: bool,
    pub lastfm_session_key: String,
    pub lastfm_username: Option<String>,
    pub lastfm_api_key: String,
    pub lastfm_api_secret: String,

    pub scrobble_now_playing: bool,
    pub scrobble_ratings: bool,
    pub scrobble_paused: bool,
    pub min_duration_secs: u32,
    // Discord Rich Presence settings (#958)
    pub discord_enabled: bool,
    pub discord_client_id: String,
    pub discord_show_album: bool,
    pub discord_show_time: bool,
}

impl Default for ScrobblerSettings {
    fn default() -> Self {
        Self {
            listenbrainz_enabled: false,
            listenbrainz_token: String::new(),
            listenbrainz_username: None,
            critiquebrainz_user_id: String::new(),
            lastfm_enabled: false,
            lastfm_session_key: String::new(),
            lastfm_username: None,
            lastfm_api_key: String::new(),
            lastfm_api_secret: String::new(),
            scrobble_now_playing: true,
            scrobble_ratings: true,
            scrobble_paused: false,
            min_duration_secs: 30,
            discord_enabled: false,
            discord_client_id: crate::discord::DEFAULT_DISCORD_CLIENT_ID.to_string(),
            discord_show_album: true,
            discord_show_time: true,
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
    pub listenbrainz_pending_count: i64,
    pub lastfm_pending_count: i64,
    pub last_error: Option<String>,
    pub last_attempt: Option<i64>,
}

/// Outcome of a two-way ratings sync (#1386). `pulled_*` counts are local
/// changes made from remote data; `pushed` counts loves/hates sent to
/// ListenBrainz. `critiquebrainz_checked` is false when no CritiqueBrainz
/// account is configured, so the UI can say stars weren't looked at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRatingsResult {
    pub pulled_loved: u32,
    pub pulled_hated: u32,
    pub pulled_song_ratings: u32,
    pub pulled_album_ratings: u32,
    pub pushed: u32,
    pub failed: u32,
    pub critiquebrainz_checked: bool,
    /// Songs whose love/stars changed locally; the command layer emits
    /// `song-stats-changed` for each so views and dynamic playlists catch up.
    #[serde(skip)]
    pub changed_song_ids: Vec<i64>,
}

/// ListenBrainz feedback score for a Luminous rating (0.5–5, negative =
/// unrated): "love" (1) at the same 4-star threshold that stars a track on
/// a Subsonic server, otherwise neutral (0).
fn feedback_score(rating: f32) -> i32 {
    if rating >= crate::subsonic::report::STAR_THRESHOLD {
        1
    } else {
        0
    }
}

/// Central manager orchestrating ListenBrainz API calls, offline cache, and Discord Rich Presence.
pub struct ScrobblerManager {
    db: Arc<Database>,
    listenbrainz: ListenBrainzClient,
    critiquebrainz: CritiqueBrainzClient,
    settings: Arc<Mutex<ScrobblerSettings>>,
    paused: Arc<std::sync::atomic::AtomicBool>,
    discord: Arc<Mutex<crate::discord::DiscordManager>>,
    /// Mirrors the Online/Offline master toggle (#1398). While `false`,
    /// ListenBrainz traffic and Discord presence are suspended; the persisted
    /// per-service settings are left untouched so they resume on re-enable.
    online: Arc<std::sync::atomic::AtomicBool>,
}

impl ScrobblerManager {
    pub fn new(db: Arc<Database>) -> Self {
        let listenbrainz = ListenBrainzClient::new();
        let critiquebrainz = CritiqueBrainzClient::new();

        let initial_settings = Self::load_settings_from_db(&db);
        let paused = Arc::new(std::sync::atomic::AtomicBool::new(
            initial_settings.scrobble_paused,
        ));
        let discord = Arc::new(Mutex::new(crate::discord::DiscordManager::new()));
        let online = db
            .pool
            .get()
            .map(|conn| crate::commands::context::is_online_enabled(&conn))
            .unwrap_or(true);

        Self {
            db,
            listenbrainz,
            critiquebrainz,
            settings: Arc::new(Mutex::new(initial_settings)),
            paused,
            discord,
            online: Arc::new(std::sync::atomic::AtomicBool::new(online)),
        }
    }

    pub fn is_online(&self) -> bool {
        self.online.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Apply the Online/Offline master toggle: going offline drops the Discord
    /// connection; going online reconnects it when the user has it enabled.
    pub async fn set_online(&self, online: bool) {
        self.online
            .store(online, std::sync::atomic::Ordering::Relaxed);
        let settings = self.get_settings().await;
        let discord = Arc::clone(&self.discord);
        let client_id = settings.discord_client_id.clone();
        if online {
            if settings.discord_enabled && !settings.scrobble_paused {
                tauri::async_runtime::spawn(async move {
                    let mut d = discord.lock().await;
                    let _ = d.connect(&client_id).await;
                });
            }
            self.trigger_flush();
        } else {
            tauri::async_runtime::spawn(async move {
                let mut d = discord.lock().await;
                let _ = d.clear_activity(&client_id).await;
                d.disconnect();
            });
        }
    }

    /// Load persisted scrobbler settings from the `app_state` key-value table.
    fn load_settings_from_db(db: &Database) -> ScrobblerSettings {
        let mut settings = ScrobblerSettings::default();
        if let Ok(conn) = db.pool.get() {
            let mut stmt = conn
                .prepare("SELECT key, value FROM app_state WHERE key LIKE 'scrobbler_%' OR key LIKE 'listenbrainz_%' OR key LIKE 'discord_%' OR key LIKE 'lastfm_%'")
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
                            "listenbrainz_enabled" => {
                                settings.listenbrainz_enabled = v == "true" || v == "1"
                            }
                            "listenbrainz_token" => settings.listenbrainz_token = v,
                            "listenbrainz_username" => {
                                settings.listenbrainz_username =
                                    if v.is_empty() { None } else { Some(v) }
                            }
                            "listenbrainz_critiquebrainz_user_id" => {
                                settings.critiquebrainz_user_id = v
                            }
                            "lastfm_enabled" => settings.lastfm_enabled = v == "true" || v == "1",
                            "lastfm_session_key" => settings.lastfm_session_key = v,
                            "lastfm_username" => {
                                settings.lastfm_username = if v.is_empty() { None } else { Some(v) }
                            }
                            "lastfm_api_key" => settings.lastfm_api_key = v,
                            "lastfm_api_secret" => settings.lastfm_api_secret = v,
                            "scrobbler_now_playing" => {
                                settings.scrobble_now_playing = v != "false" && v != "0"
                            }
                            "scrobbler_ratings" => {
                                settings.scrobble_ratings = v != "false" && v != "0"
                            }
                            "scrobbler_paused" => {
                                settings.scrobble_paused = v == "true" || v == "1"
                            }
                            "scrobbler_min_duration_secs" => {
                                if let Ok(n) = v.parse::<u32>() {
                                    settings.min_duration_secs = n;
                                }
                            }
                            "discord_enabled" => settings.discord_enabled = v == "true" || v == "1",
                            "discord_client_id" => {
                                if !v.trim().is_empty() {
                                    settings.discord_client_id = v;
                                }
                            }
                            "discord_show_album" => {
                                settings.discord_show_album = v != "false" && v != "0"
                            }
                            "discord_show_time" => {
                                settings.discord_show_time = v != "false" && v != "0"
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
                (
                    "listenbrainz_enabled",
                    new_settings.listenbrainz_enabled.to_string(),
                ),
                (
                    "listenbrainz_token",
                    new_settings.listenbrainz_token.clone(),
                ),
                (
                    "listenbrainz_username",
                    new_settings
                        .listenbrainz_username
                        .clone()
                        .unwrap_or_default(),
                ),
                (
                    "listenbrainz_critiquebrainz_user_id",
                    new_settings.critiquebrainz_user_id.clone(),
                ),
                ("lastfm_enabled", new_settings.lastfm_enabled.to_string()),
                (
                    "lastfm_session_key",
                    new_settings.lastfm_session_key.clone(),
                ),
                (
                    "lastfm_username",
                    new_settings.lastfm_username.clone().unwrap_or_default(),
                ),
                ("lastfm_api_key", new_settings.lastfm_api_key.clone()),
                ("lastfm_api_secret", new_settings.lastfm_api_secret.clone()),
                (
                    "scrobbler_now_playing",
                    new_settings.scrobble_now_playing.to_string(),
                ),
                (
                    "scrobbler_ratings",
                    new_settings.scrobble_ratings.to_string(),
                ),
                ("scrobbler_paused", new_settings.scrobble_paused.to_string()),
                (
                    "scrobbler_min_duration_secs",
                    new_settings.min_duration_secs.to_string(),
                ),
                ("discord_enabled", new_settings.discord_enabled.to_string()),
                ("discord_client_id", new_settings.discord_client_id.clone()),
                (
                    "discord_show_album",
                    new_settings.discord_show_album.to_string(),
                ),
                (
                    "discord_show_time",
                    new_settings.discord_show_time.to_string(),
                ),
            ];
            for (k, v) in pairs {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
                    params![k, v],
                );
            }
        }
        self.paused.store(
            new_settings.scrobble_paused,
            std::sync::atomic::Ordering::Relaxed,
        );
        let mut s = self.settings.lock().await;
        *s = new_settings.clone();

        if new_settings.discord_enabled && !new_settings.scrobble_paused && self.is_online() {
            let discord = Arc::clone(&self.discord);
            let client_id = new_settings.discord_client_id.clone();
            tauri::async_runtime::spawn(async move {
                let mut d = discord.lock().await;
                let _ = d.connect(&client_id).await;
            });
        } else {
            let discord = Arc::clone(&self.discord);
            let client_id = new_settings.discord_client_id.clone();
            tauri::async_runtime::spawn(async move {
                let mut d = discord.lock().await;
                let _ = d.clear_activity(&client_id).await;
                d.disconnect();
            });
        }

        Ok(())
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub async fn toggle_paused(&self) -> bool {
        let mut s = self.get_settings().await;
        s.scrobble_paused = !s.scrobble_paused;
        let new_paused = s.scrobble_paused;
        let _ = self.save_settings(s).await;
        if new_paused {
            self.clear_discord().await;
        }
        new_paused
    }

    pub async fn get_settings(&self) -> ScrobblerSettings {
        self.settings.lock().await.clone()
    }

    /// Settings as the network/presence paths must see them: ListenBrainz, Last.fm,
    /// and Discord read as disabled while the master toggle is Offline.
    async fn effective_settings(&self) -> ScrobblerSettings {
        let mut s = self.get_settings().await;
        if !self.is_online() {
            s.listenbrainz_enabled = false;
            s.lastfm_enabled = false;
            s.discord_enabled = false;
        }
        s
    }

    /// Validate a ListenBrainz user token by hitting `/1/validate-token`.
    pub async fn validate_token(&self, token: &str) -> Result<String, String> {
        if !self.is_online() {
            return Err(crate::commands::context::OFFLINE_ERROR.into());
        }
        let trimmed = token.trim();
        let username = self.listenbrainz.validate_token(trimmed).await?;

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

    /// Update or clear Discord Rich Presence based on playback state.
    pub async fn update_discord(
        &self,
        song: Option<&Song>,
        is_playing: bool,
        position_nanosec: i64,
    ) {
        let settings = self.effective_settings().await;
        if !settings.discord_enabled || settings.scrobble_paused {
            let discord = Arc::clone(&self.discord);
            let client_id = settings.discord_client_id.clone();
            tauri::async_runtime::spawn(async move {
                let mut d = discord.lock().await;
                let _ = d.clear_activity(&client_id).await;
            });
            return;
        }

        let song = match song {
            Some(s) => s,
            None => {
                let discord = Arc::clone(&self.discord);
                let client_id = settings.discord_client_id.clone();
                tauri::async_runtime::spawn(async move {
                    let mut d = discord.lock().await;
                    let _ = d.clear_activity(&client_id).await;
                });
                return;
            }
        };

        let artist = song.artist.as_deref().unwrap_or("Unknown Artist");
        let title = song.title.as_deref().unwrap_or("Unknown Track");
        let album = song.album.as_deref();

        let state_str = if is_playing {
            if settings.discord_show_album {
                if let Some(alb) = album {
                    if !alb.trim().is_empty() {
                        format!("by {} • {}", artist, alb)
                    } else {
                        format!("by {}", artist)
                    }
                } else {
                    format!("by {}", artist)
                }
            } else {
                format!("by {}", artist)
            }
        } else {
            // Paused state (Option A)
            format!("by {} (Paused)", artist)
        };

        let timestamps = if is_playing && settings.discord_show_time {
            let now = chrono::Utc::now().timestamp() as u64;
            let pos_secs = (position_nanosec.max(0) as u64) / 1_000_000_000;
            let start = now.saturating_sub(pos_secs);
            let end = song
                .length_nanosec
                .filter(|&ns| ns > 0)
                .map(|ns| start + ((ns as u64) / 1_000_000_000));
            Some(crate::discord::DiscordTimestamps {
                start: Some(start),
                end,
            })
        } else {
            None
        };

        let large_text = if let Some(alb) = album {
            if !alb.trim().is_empty() {
                crate::discord::truncate_activity_str(alb)
            } else {
                "Luminous Music Player".into()
            }
        } else {
            "Luminous Music Player".into()
        };

        let (small_image, small_text) = if is_playing {
            (Some("play".into()), Some("Playing".into()))
        } else {
            (Some("pause".into()), Some("Paused".into()))
        };

        let activity = crate::discord::DiscordActivity {
            details: Some(crate::discord::truncate_activity_str(title)),
            state: Some(crate::discord::truncate_activity_str(&state_str)),
            timestamps,
            assets: Some(crate::discord::DiscordAssets {
                large_image: Some("luminous_logo".into()),
                large_text: Some(large_text),
                small_image,
                small_text,
            }),
        };

        let discord = Arc::clone(&self.discord);
        let client_id = settings.discord_client_id.clone();
        tauri::async_runtime::spawn(async move {
            let mut d = discord.lock().await;
            if let Err(e) = d.set_activity(&client_id, Some(activity)).await {
                log::debug!("Discord presence update: {e}");
            }
        });
    }

    /// Clear Discord Rich Presence activity.
    pub async fn clear_discord(&self) {
        let settings = self.get_settings().await;
        let discord = Arc::clone(&self.discord);
        let client_id = settings.discord_client_id.clone();
        tauri::async_runtime::spawn(async move {
            let mut d = discord.lock().await;
            let _ = d.clear_activity(&client_id).await;
        });
    }

    /// Called when playback state changes between playing and paused.
    pub async fn on_playback_state_changed(
        &self,
        song: Option<&Song>,
        is_playing: bool,
        position_nanosec: i64,
    ) {
        self.update_discord(song, is_playing, position_nanosec)
            .await;
    }

    /// Called when playback has completely stopped.
    pub async fn on_playback_stopped(&self) {
        self.clear_discord().await;
    }

    /// Query the current Discord connection status.
    pub async fn get_discord_status(&self) -> crate::discord::DiscordStatus {
        let mut d = self.discord.lock().await;
        let settings = self.effective_settings().await;
        if settings.discord_enabled
            && !settings.scrobble_paused
            && d.status() != crate::discord::DiscordStatus::Connected
        {
            let _ = d.connect(&settings.discord_client_id).await;
        }
        d.status()
    }

    /// Resolve a LastFmClient using custom credentials from settings, env vars, or defaults.
    pub fn resolve_lastfm_client(&self, settings: &ScrobblerSettings) -> LastFmClient {
        let api_key = if !settings.lastfm_api_key.trim().is_empty() {
            settings.lastfm_api_key.trim().to_string()
        } else if let Ok(k) = std::env::var("LASTFM_API_KEY") {
            k.trim().to_string()
        } else if let Some(k) = option_env!("LASTFM_API_KEY") {
            k.trim().to_string()
        } else {
            DEFAULT_LASTFM_API_KEY.to_string()
        };

        let api_secret = if !settings.lastfm_api_secret.trim().is_empty() {
            settings.lastfm_api_secret.trim().to_string()
        } else if let Ok(s) = std::env::var("LASTFM_API_SECRET") {
            s.trim().to_string()
        } else if let Some(s) = option_env!("LASTFM_API_SECRET") {
            s.trim().to_string()
        } else {
            DEFAULT_LASTFM_API_SECRET.to_string()
        };

        LastFmClient::new(api_key, api_secret)
    }

    /// Initiate Last.fm desktop authentication: acquires a request token and returns
    /// (token, authorization_url) for the user to authorize in their browser.
    pub async fn start_lastfm_auth(&self) -> Result<(String, String), String> {
        let settings = self.get_settings().await;
        let client = self.resolve_lastfm_client(&settings);
        let token = client.get_auth_token().await.map_err(|e| e.to_string())?;
        let auth_url = client.build_auth_url(&token);
        Ok((token, auth_url))
    }

    /// Complete Last.fm desktop authentication: exchanges the authorized token for a permanent session.
    pub async fn complete_lastfm_auth(&self, token: &str) -> Result<LastFmSession, String> {
        let mut settings = self.get_settings().await;
        let client = self.resolve_lastfm_client(&settings);
        let session = client.get_session(token).await.map_err(|e| e.to_string())?;

        settings.lastfm_session_key = session.session_key.clone();
        settings.lastfm_username = Some(session.username.clone());
        settings.lastfm_enabled = true;

        self.save_settings(settings)
            .await
            .map_err(|e| e.to_string())?;
        self.trigger_flush();

        Ok(session)
    }

    /// Disconnect from Last.fm: clears credentials and disables Last.fm scrobbling.
    pub async fn disconnect_lastfm(&self) -> Result<(), String> {
        let mut settings = self.get_settings().await;
        settings.lastfm_enabled = false;
        settings.lastfm_session_key = String::new();
        settings.lastfm_username = None;
        self.save_settings(settings)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Submit a "Playing Now" listen to active scrobbler services when track playback starts.
    pub async fn on_now_playing(&self, song: &Song) {
        let settings = self.effective_settings().await;
        if !settings.scrobble_paused {
            crate::subsonic::report::spawn_now_playing(self.db.clone(), song);
        }

        let lb_now_playing = settings.listenbrainz_enabled
            && !settings.scrobble_paused
            && settings.scrobble_now_playing
            && !settings.listenbrainz_token.trim().is_empty();

        let lf_now_playing = settings.lastfm_enabled
            && !settings.scrobble_paused
            && settings.scrobble_now_playing
            && !settings.lastfm_session_key.trim().is_empty();

        if !lb_now_playing && !lf_now_playing {
            return;
        }

        // Exclude radio streams
        if matches!(
            song.source,
            SongSource::Stream
                | SongSource::SomaFm
                | SongSource::RadioParadise
                | SongSource::RadioBrowser
        ) {
            return;
        }

        let artist = song.artist.as_deref().unwrap_or("Unknown Artist");
        let title = song.title.as_deref().unwrap_or("Unknown Track");
        let release = song.album.as_deref();
        let duration_ms = song.length_nanosec.map(|ns| ns / 1_000_000);
        let tracknumber = song.track;

        if lb_now_playing {
            let track = ListenTrackInfo {
                artist: artist.to_string(),
                track: title.to_string(),
                album: release.map(|s| s.to_string()),
                duration_ms,
                track_number: tracknumber,
                recording_mbid: song.musicbrainz_recording_id.clone(),
                release_mbid: song.musicbrainz_album_id.clone(),
                artist_mbids: song.musicbrainz_artist_id.clone(),
                release_group_mbid: song.musicbrainz_release_group_id.clone(),
                track_mbid: song.musicbrainz_track_id.clone(),
                listened_at: None,
            };

            let token = settings.listenbrainz_token.trim().to_string();
            let lb = self.listenbrainz.clone();

            tauri::async_runtime::spawn(async move {
                if let Err(e) = lb.submit_now_playing(&token, &track).await {
                    log::warn!("Failed to submit now-playing to ListenBrainz: {e}");
                } else {
                    log::debug!("ListenBrainz now-playing submitted successfully");
                }
            });
        }

        if lf_now_playing {
            let lastfm = self.resolve_lastfm_client(&settings);
            let sk = settings.lastfm_session_key.clone();
            let lf_track = LastFmTrack {
                artist: artist.to_string(),
                track: title.to_string(),
                album: release.map(|s| s.to_string()),
                duration_secs: duration_ms.map(|ms| (ms / 1000) as u32),
                track_number: tracknumber,
                timestamp: chrono::Utc::now().timestamp(),
                mbid: song.musicbrainz_recording_id.clone(),
            };

            tauri::async_runtime::spawn(async move {
                if let Err(e) = lastfm.update_now_playing(&sk, &lf_track).await {
                    log::warn!("Failed to submit now-playing to Last.fm: {e}");
                } else {
                    log::debug!("Last.fm now-playing submitted successfully");
                }
            });
        }
    }

    /// Enqueue a scrobble when the 50% scrobble point is reached, then trigger a flush.
    pub async fn on_scrobble_point(&self, song: &Song, listened_at: i64) {
        let settings = self.get_settings().await;
        if !settings.scrobble_paused {
            crate::subsonic::report::spawn_play(self.db.clone(), song, listened_at);
        }

        let lb_active = settings.listenbrainz_enabled && !settings.scrobble_paused;
        let lf_active = settings.lastfm_enabled && !settings.scrobble_paused;

        if !lb_active && !lf_active {
            return;
        }

        // Radio / live stream tracks are not scrobbled
        if matches!(
            song.source,
            SongSource::Stream
                | SongSource::SomaFm
                | SongSource::RadioParadise
                | SongSource::RadioBrowser
        ) {
            return;
        }

        // Guard: check minimum duration
        if let Some(ns) = song.length_nanosec {
            let secs = (ns as u64) / 1_000_000_000;
            if secs < (settings.min_duration_secs as u64) {
                log::debug!(
                    "Song duration ({}s) is below scrobble minimum ({}s); skipping scrobble",
                    secs,
                    settings.min_duration_secs
                );
                return;
            }
        }

        let artist = song
            .artist
            .clone()
            .unwrap_or_else(|| "Unknown Artist".into());
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
        if let Ok(conn) = self.db.pool.get() {
            if lb_active {
                let _ = conn.execute(
                    "INSERT INTO scrobble_cache (
                        service, artist, track, album, duration_ms, track_number,
                        recording_mbid, release_mbid, artist_mbids, release_group_mbid, track_mbid,
                        listened_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        "listenbrainz",
                        &artist,
                        &track,
                        &album,
                        duration_ms,
                        track_number,
                        &recording_mbid,
                        &release_mbid,
                        &artist_mbids,
                        &release_group_mbid,
                        &track_mbid,
                        listened_at
                    ],
                );
            }
            if lf_active {
                let _ = conn.execute(
                    "INSERT INTO scrobble_cache (
                        service, artist, track, album, duration_ms, track_number,
                        recording_mbid, release_mbid, artist_mbids, release_group_mbid, track_mbid,
                        listened_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        "lastfm",
                        &artist,
                        &track,
                        &album,
                        duration_ms,
                        track_number,
                        &recording_mbid,
                        &release_mbid,
                        &artist_mbids,
                        &release_group_mbid,
                        &track_mbid,
                        listened_at
                    ],
                );
            }
        }

        log::info!(
            "Scrobble enqueued for '{} - {}' at timestamp {}",
            artist,
            track,
            listened_at
        );

        // Trigger asynchronous cache drain
        self.trigger_flush();
    }

    /// Submit love/feedback when song rating changes.
    pub async fn on_song_rating(&self, song: &Song, rating: f32) {
        let settings = self.effective_settings().await;
        if settings.scrobble_paused || !settings.scrobble_ratings {
            return;
        }

        // ListenBrainz submission (requires recording_mbid)
        if settings.listenbrainz_enabled && !settings.listenbrainz_token.trim().is_empty() {
            if let Some(mbid) = &song.musicbrainz_recording_id {
                if !mbid.trim().is_empty() {
                    let score = feedback_score(rating);
                    let token = settings.listenbrainz_token.trim().to_string();
                    let lb = self.listenbrainz.clone();
                    let mbid_clone = mbid.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = lb.submit_feedback(&token, &mbid_clone, score).await {
                            log::warn!("Failed to submit rating feedback to ListenBrainz: {e}");
                        }
                    });
                }
            }
        }

        // Last.fm submission (rating >= 4.0 treated as loved)
        if settings.lastfm_enabled && !settings.lastfm_session_key.trim().is_empty() {
            if let (Some(artist), Some(track)) = (&song.artist, &song.title) {
                if !artist.trim().is_empty() && !track.trim().is_empty() && rating >= 4.0 {
                    let lastfm = self.resolve_lastfm_client(&settings);
                    let sk = settings.lastfm_session_key.clone();
                    let artist_clone = artist.clone();
                    let track_clone = track.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = lastfm
                            .set_loved(&sk, &artist_clone, &track_clone, true)
                            .await
                        {
                            log::warn!("Failed to submit rating love to Last.fm: {e}");
                        }
                    });
                }
            }
        }
    }

    /// Submit love/hate tri-state feedback when song loved state changes.
    pub async fn on_song_loved(&self, song: &Song, loved: i32) {
        let settings = self.effective_settings().await;
        if settings.scrobble_paused || !settings.scrobble_ratings {
            return;
        }

        // ListenBrainz submission (requires recording_mbid)
        if settings.listenbrainz_enabled && !settings.listenbrainz_token.trim().is_empty() {
            if let Some(mbid) = &song.musicbrainz_recording_id {
                if !mbid.trim().is_empty() {
                    let token = settings.listenbrainz_token.trim().to_string();
                    let lb = self.listenbrainz.clone();
                    let mbid_clone = mbid.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = lb.submit_feedback(&token, &mbid_clone, loved).await {
                            log::warn!("Failed to submit loved feedback to ListenBrainz: {e}");
                        }
                    });
                }
            }
        }

        // Last.fm submission: loved=1 loves, loved<=0 unloves
        if settings.lastfm_enabled && !settings.lastfm_session_key.trim().is_empty() {
            if let (Some(artist), Some(track)) = (&song.artist, &song.title) {
                if !artist.trim().is_empty() && !track.trim().is_empty() {
                    let lastfm = self.resolve_lastfm_client(&settings);
                    let sk = settings.lastfm_session_key.clone();
                    let artist_clone = artist.clone();
                    let track_clone = track.clone();
                    let is_loved = loved > 0;
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = lastfm
                            .set_loved(&sk, &artist_clone, &track_clone, is_loved)
                            .await
                        {
                            log::warn!("Failed to submit loved feedback to Last.fm: {e}");
                        }
                    });
                }
            }
        }
    }

    /// Two-way ratings sync (#1386, #1954). Reconciles local ratings with
    /// ListenBrainz/CritiqueBrainz and/or Last.fm loved tracks.
    pub async fn sync_ratings(&self) -> Result<SyncRatingsResult, String> {
        let settings = self.effective_settings().await;
        if !settings.listenbrainz_enabled && !settings.lastfm_enabled {
            return Err("ListenBrainz scrobbling is not enabled".into());
        }

        let mut outcome = crate::ratings_sync::PullOutcome::default();
        let mut pushed = 0u32;
        let mut failed = 0u32;
        let mut critiquebrainz_checked = false;

        // 1. Sync ListenBrainz & CritiqueBrainz if enabled
        if settings.listenbrainz_enabled {
            let token = settings.listenbrainz_token.trim().to_string();
            if token.is_empty() {
                return Err("ListenBrainz user token is not configured".into());
            }
            let username = settings
                .listenbrainz_username
                .as_deref()
                .filter(|u| !u.trim().is_empty())
                .ok_or("ListenBrainz username is unknown: validate your token first")?
                .to_string();

            let cb_input = settings.critiquebrainz_user_id.trim();
            let cb_user = if cb_input.is_empty() {
                None
            } else {
                Some(
                    crate::ratings_sync::parse_critiquebrainz_user_id(cb_input)
                        .ok_or("CritiqueBrainz user ID must be a profile URL or UUID")?,
                )
            };

            let remote = self
                .listenbrainz
                .fetch_user_feedback(&username, &token)
                .await?;
            let critique: Option<Vec<crate::ratings_sync::CritiqueRating>> = match &cb_user {
                Some(id) => {
                    let items = self.critiquebrainz.fetch_user_ratings(id).await?;
                    Some(
                        items
                            .into_iter()
                            .map(|item| crate::ratings_sync::CritiqueRating {
                                entity_type: item.entity_type,
                                entity_mbid: item.entity_mbid,
                                stars: item.stars,
                            })
                            .collect(),
                    )
                }
                None => None,
            };

            let (lb_outcome, lb_pushes) = {
                let mut conn = self.db.pool.get().map_err(|e| e.to_string())?;
                let mut out = crate::ratings_sync::apply_feedback(&mut conn, &remote)
                    .map_err(|e| e.to_string())?;
                if let Some(ratings) = &critique {
                    let stars = crate::ratings_sync::apply_critique_ratings(&mut conn, ratings)
                        .map_err(|e| e.to_string())?;
                    out.song_ratings = stars.song_ratings;
                    out.album_ratings = stars.album_ratings;
                    out.song_ids.extend(stars.song_ids);
                }
                let pushes = crate::ratings_sync::pending_pushes(&conn, &remote)
                    .map_err(|e| e.to_string())?;
                (out, pushes)
            };

            outcome.loved += lb_outcome.loved;
            outcome.hated += lb_outcome.hated;
            outcome.song_ratings += lb_outcome.song_ratings;
            outcome.album_ratings += lb_outcome.album_ratings;
            outcome.song_ids.extend(lb_outcome.song_ids);
            critiquebrainz_checked = critique.is_some();

            for (mbid, score) in lb_pushes {
                match self
                    .listenbrainz
                    .submit_feedback(&token, &mbid, score)
                    .await
                {
                    Ok(()) => pushed += 1,
                    Err(e) => {
                        log::warn!("Failed to submit feedback to ListenBrainz: {e}");
                        failed += 1;
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        // 2. Sync Last.fm loved tracks if enabled
        if settings.lastfm_enabled {
            let sk = settings.lastfm_session_key.trim();
            let username = settings.lastfm_username.as_deref().unwrap_or("").trim();
            if !sk.is_empty() && !username.is_empty() {
                let lastfm = self.resolve_lastfm_client(&settings);
                match lastfm.fetch_all_loved_tracks(username).await {
                    Ok(remote_loved) => {
                        let (lf_outcome, lf_pushes) = {
                            let mut conn = self.db.pool.get().map_err(|e| e.to_string())?;
                            crate::ratings_sync::reconcile_lastfm_loved(&mut conn, &remote_loved)
                                .map_err(|e| e.to_string())?
                        };

                        outcome.loved += lf_outcome.loved;
                        outcome.song_ids.extend(lf_outcome.song_ids);

                        for (artist, track) in lf_pushes {
                            match lastfm.set_loved(sk, &artist, &track, true).await {
                                Ok(()) => pushed += 1,
                                Err(e) => {
                                    log::warn!("Failed to push loved track to Last.fm: {e}");
                                    failed += 1;
                                }
                            }
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to fetch loved tracks from Last.fm: {e}");
                        failed += 1;
                    }
                }
            }
        }

        outcome.song_ids.sort_unstable();
        outcome.song_ids.dedup();
        Ok(SyncRatingsResult {
            pulled_loved: outcome.loved,
            pulled_hated: outcome.hated,
            pulled_song_ratings: outcome.song_ratings,
            pulled_album_ratings: outcome.album_ratings,
            pushed,
            failed,
            critiquebrainz_checked,
            changed_song_ids: outcome.song_ids,
        })
    }

    /// Retrieve live scrobble cache statistics.
    pub fn get_cache_status(&self) -> ScrobbleCacheStatus {
        let conn = match self.db.pool.get() {
            Ok(c) => c,
            Err(_) => {
                return ScrobbleCacheStatus {
                    pending_count: 0,
                    listenbrainz_pending_count: 0,
                    lastfm_pending_count: 0,
                    last_error: None,
                    last_attempt: None,
                }
            }
        };

        let pending_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM scrobble_cache", [], |r| r.get(0))
            .unwrap_or(0);

        let listenbrainz_pending_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scrobble_cache WHERE service = 'listenbrainz'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let lastfm_pending_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scrobble_cache WHERE service = 'lastfm'",
                [],
                |r| r.get(0),
            )
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
            listenbrainz_pending_count,
            lastfm_pending_count,
            last_error,
            last_attempt,
        }
    }

    /// Spawn a flush in the background.
    pub fn trigger_flush(&self) {
        let lb = self.listenbrainz.clone();
        let db = self.db.clone();
        let settings_arc = self.settings.clone();
        let online = self.online.clone();

        tauri::async_runtime::spawn(async move {
            let settings = settings_arc.lock().await.clone();
            if !online.load(std::sync::atomic::Ordering::Relaxed) || settings.scrobble_paused {
                return;
            }

            let lb_token = if settings.listenbrainz_enabled
                && !settings.listenbrainz_token.trim().is_empty()
            {
                Some(settings.listenbrainz_token.clone())
            } else {
                None
            };

            let lastfm_opt =
                if settings.lastfm_enabled && !settings.lastfm_session_key.trim().is_empty() {
                    let api_key = if !settings.lastfm_api_key.trim().is_empty() {
                        settings.lastfm_api_key.trim().to_string()
                    } else if let Ok(k) = std::env::var("LASTFM_API_KEY") {
                        k.trim().to_string()
                    } else if let Some(k) = option_env!("LASTFM_API_KEY") {
                        k.trim().to_string()
                    } else {
                        DEFAULT_LASTFM_API_KEY.to_string()
                    };

                    let api_secret = if !settings.lastfm_api_secret.trim().is_empty() {
                        settings.lastfm_api_secret.trim().to_string()
                    } else if let Ok(s) = std::env::var("LASTFM_API_SECRET") {
                        s.trim().to_string()
                    } else if let Some(s) = option_env!("LASTFM_API_SECRET") {
                        s.trim().to_string()
                    } else {
                        DEFAULT_LASTFM_API_SECRET.to_string()
                    };

                    let client = LastFmClient::new(api_key, api_secret);
                    Some((client, settings.lastfm_session_key.clone()))
                } else {
                    None
                };

            if lb_token.is_none() && lastfm_opt.is_none() {
                return;
            }

            if let Err(e) = Self::flush_cache_internal(
                &db,
                &lb,
                lb_token.as_deref(),
                lastfm_opt.as_ref().map(|(c, sk)| (c, sk.as_str())),
            )
            .await
            {
                log::warn!("Scrobble cache flush finished with error: {e}");
            }
        });
    }

    /// Drain pending scrobbles from the database cache and submit them to configured services.
    pub async fn flush_cache_now(&self) -> Result<u32, String> {
        let settings = self.effective_settings().await;
        if !self.is_online() {
            return Err(crate::commands::context::OFFLINE_ERROR.into());
        }

        let lb_token =
            if settings.listenbrainz_enabled && !settings.listenbrainz_token.trim().is_empty() {
                Some(settings.listenbrainz_token.clone())
            } else {
                None
            };

        let lastfm_opt =
            if settings.lastfm_enabled && !settings.lastfm_session_key.trim().is_empty() {
                let client = self.resolve_lastfm_client(&settings);
                Some((client, settings.lastfm_session_key.clone()))
            } else {
                None
            };

        if lb_token.is_none() && lastfm_opt.is_none() {
            return Err("Neither ListenBrainz nor Last.fm scrobbler is configured".into());
        }

        Self::flush_cache_internal(
            &self.db,
            &self.listenbrainz,
            lb_token.as_deref(),
            lastfm_opt.as_ref().map(|(c, sk)| (c, sk.as_str())),
        )
        .await
    }

    fn fetch_pending_scrobbles(
        db: &Database,
        service: &str,
    ) -> Result<Vec<ScrobbleCacheEntry>, String> {
        let conn = db.pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, service, artist, track, album, duration_ms, track_number,
                        recording_mbid, release_mbid, artist_mbids, release_group_mbid, track_mbid,
                        listened_at, attempts, last_attempt, last_error, created_at
                 FROM scrobble_cache
                 WHERE service = ?1
                 ORDER BY created_at ASC
                 LIMIT 50",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![service], |row| {
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
            .filter_map(std::result::Result::ok)
            .collect();

        Ok(rows)
    }

    fn delete_cached_entries(db: &Database, entries: &[ScrobbleCacheEntry]) {
        if let Ok(conn) = db.pool.get() {
            for e in entries {
                let _ = conn.execute("DELETE FROM scrobble_cache WHERE id = ?1", params![e.id]);
            }
        }
    }

    fn record_cache_error(db: &Database, entries: &[ScrobbleCacheEntry], now: i64, err: &str) {
        if let Ok(conn) = db.pool.get() {
            for e in entries {
                let _ = conn.execute(
                    "UPDATE scrobble_cache SET attempts = attempts + 1, last_attempt = ?1, last_error = ?2 WHERE id = ?3",
                    params![now, err, e.id],
                );
            }
        }
    }

    async fn flush_cache_internal(
        db: &Database,
        lb: &ListenBrainzClient,
        lb_token: Option<&str>,
        lastfm_opt: Option<(&LastFmClient, &str)>,
    ) -> Result<u32, String> {
        let mut total_flushed = 0u32;
        let mut last_err = None;

        // 1. Drain ListenBrainz queue
        if let Some(token) = lb_token {
            let entries = Self::fetch_pending_scrobbles(db, "listenbrainz")?;
            if !entries.is_empty() {
                let payload_items: Vec<ListenTrackInfo> = entries
                    .iter()
                    .map(|e| ListenTrackInfo {
                        artist: e.artist.clone(),
                        track: e.track.clone(),
                        album: e.album.clone(),
                        duration_ms: e.duration_ms,
                        track_number: e.track_number,
                        recording_mbid: e.recording_mbid.clone(),
                        release_mbid: e.release_mbid.clone(),
                        artist_mbids: e.artist_mbids.clone(),
                        release_group_mbid: e.release_group_mbid.clone(),
                        track_mbid: e.track_mbid.clone(),
                        listened_at: Some(e.listened_at),
                    })
                    .collect();

                let now = chrono::Utc::now().timestamp();
                match lb.submit_listens(token, &payload_items).await {
                    Ok(()) => {
                        let count = entries.len() as u32;
                        Self::delete_cached_entries(db, &entries);
                        log::info!("Successfully flushed {count} scrobbles to ListenBrainz");
                        total_flushed += count;
                    }
                    Err(e) => {
                        Self::record_cache_error(db, &entries, now, &e);
                        last_err = Some(e);
                    }
                }
            }
        }

        // 2. Drain Last.fm queue
        if let Some((client, session_key)) = lastfm_opt {
            let entries = Self::fetch_pending_scrobbles(db, "lastfm")?;
            if !entries.is_empty() {
                let lf_tracks: Vec<LastFmTrack> = entries
                    .iter()
                    .map(|e| LastFmTrack {
                        artist: e.artist.clone(),
                        track: e.track.clone(),
                        album: e.album.clone(),
                        track_number: e.track_number,
                        duration_secs: e.duration_ms.map(|ms| (ms / 1000) as u32),
                        timestamp: e.listened_at,
                        mbid: e.recording_mbid.clone(),
                    })
                    .collect();

                let now = chrono::Utc::now().timestamp();
                match client.scrobble_batch(session_key, &lf_tracks).await {
                    Ok(resp) => {
                        let count = resp.accepted;
                        Self::delete_cached_entries(db, &entries);
                        log::info!("Successfully flushed {count} scrobbles to Last.fm");
                        total_flushed += count;
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        Self::record_cache_error(db, &entries, now, &err_str);
                        last_err = Some(err_str);
                    }
                }
            }
        }

        if total_flushed > 0 || last_err.is_none() {
            Ok(total_flushed)
        } else {
            Err(last_err.unwrap_or_else(|| "Failed to flush scrobble cache".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Offline master toggle (#1398): ListenBrainz entry points refuse before
    /// any request, and the effective settings read ListenBrainz/Discord as off
    /// without touching what the user saved.
    #[tokio::test]
    async fn offline_suspends_listenbrainz_and_discord_without_changing_saved_settings() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).unwrap());
        let manager = ScrobblerManager::new(Arc::clone(&db));
        let saved = ScrobblerSettings {
            listenbrainz_enabled: true,
            listenbrainz_token: "token".into(),
            discord_enabled: true,
            ..ScrobblerSettings::default()
        };
        manager.save_settings(saved).await.unwrap();

        manager.set_online(false).await;

        assert!(!manager.is_online());
        let effective = manager.effective_settings().await;
        assert!(!effective.listenbrainz_enabled && !effective.discord_enabled);
        let persisted = manager.get_settings().await;
        assert!(persisted.listenbrainz_enabled && persisted.discord_enabled);
        assert_eq!(
            manager.validate_token("token").await,
            Err(crate::commands::context::OFFLINE_ERROR.to_string())
        );
        assert_eq!(
            manager.flush_cache_now().await,
            Err(crate::commands::context::OFFLINE_ERROR.to_string())
        );
        assert_eq!(
            manager.sync_ratings().await.map(|_| ()),
            Err("ListenBrainz scrobbling is not enabled".to_string())
        );
    }

    #[test]
    fn feedback_loves_only_four_stars_and_up() {
        assert_eq!(feedback_score(-1.0), 0);
        assert_eq!(feedback_score(0.5), 0);
        assert_eq!(feedback_score(1.0), 0);
        assert_eq!(feedback_score(3.5), 0);
        assert_eq!(feedback_score(4.0), 1);
        assert_eq!(feedback_score(5.0), 1);
    }

    #[test]
    fn test_discord_settings_defaults_and_serialization() {
        let settings = ScrobblerSettings::default();
        assert!(!settings.discord_enabled);
        assert_eq!(
            settings.discord_client_id,
            crate::discord::DEFAULT_DISCORD_CLIENT_ID
        );
        assert!(settings.discord_show_album);
        assert!(settings.discord_show_time);

        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"discord_enabled\":false"));
        assert!(json.contains("\"discord_show_album\":true"));
        assert!(json.contains("\"discord_show_time\":true"));
        assert!(json.contains(&format!(
            "\"discord_client_id\":\"{}\"",
            crate::discord::DEFAULT_DISCORD_CLIENT_ID
        )));

        let deserialized: ScrobblerSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.discord_enabled, settings.discord_enabled);
        assert_eq!(deserialized.discord_client_id, settings.discord_client_id);
    }

    #[tokio::test]
    async fn test_lastfm_settings_persistence_and_cache_enqueue() {
        let dir = tempfile::Builder::new()
            .prefix("luminous_lastfm_scrobbler_test_")
            .tempdir()
            .unwrap();
        let db = Arc::new(crate::db::Database::new(dir.path().to_path_buf()).unwrap());
        let manager = ScrobblerManager::new(Arc::clone(&db));

        let initial = manager.get_settings().await;
        assert!(!initial.lastfm_enabled);
        assert!(initial.lastfm_session_key.is_empty());
        assert!(initial.lastfm_username.is_none());

        let mut saved = initial.clone();
        saved.lastfm_enabled = true;
        saved.lastfm_session_key = "test_sk_12345".into();
        saved.lastfm_username = Some("test_lastfm_user".into());
        manager.save_settings(saved.clone()).await.unwrap();

        let reloaded = ScrobblerManager::load_settings_from_db(&db);
        assert!(reloaded.lastfm_enabled);
        assert_eq!(reloaded.lastfm_session_key, "test_sk_12345");
        assert_eq!(
            reloaded.lastfm_username.as_deref(),
            Some("test_lastfm_user")
        );

        // Enqueue scrobble
        let song = Song {
            artist: Some("The Beatles".into()),
            title: Some("Hey Jude".into()),
            album: Some("Hey Jude".into()),
            length_nanosec: Some(420 * 1_000_000_000), // 7 mins
            ..Default::default()
        };

        manager.on_scrobble_point(&song, 1700000000).await;

        let status = manager.get_cache_status();
        assert_eq!(status.lastfm_pending_count, 1);
        assert_eq!(status.listenbrainz_pending_count, 0);
        assert_eq!(status.pending_count, 1);
    }
}
