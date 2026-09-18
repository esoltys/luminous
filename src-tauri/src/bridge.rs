use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

use crate::models::{PlayContext, PlayState, PlaybackState, RepeatMode, ShuffleMode, Song};
use crate::AppState;

pub const DEFAULT_BRIDGE_PORT: u16 = 21849;
pub const BRIDGE_HOST: &str = "127.0.0.1";

/// Caps how many bridge connections are handled at once. The bridge is
/// loopback-only and each connection is short-lived (one request/response),
/// so this is a generous ceiling meant to stop a runaway or misbehaving
/// local client from spawning an unbounded number of tasks — not a tuned
/// capacity limit.
const MAX_CONCURRENT_BRIDGE_CONNECTIONS: usize = 64;

#[derive(serde::Serialize)]
pub struct BridgeTrack {
    pub id: i64,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_seconds: Option<f64>,
    pub path: Option<String>,
}

impl From<Song> for BridgeTrack {
    fn from(song: Song) -> Self {
        Self {
            id: song.id,
            title: song.title,
            artist: song.artist,
            album: song.album,
            duration_seconds: song.length_nanosec.map(|ns| ns as f64 / 1_000_000_000.0),
            path: song.path,
        }
    }
}

#[derive(serde::Serialize)]
pub struct BridgePlaybackState {
    pub status: String,
    pub current_track: Option<BridgeTrack>,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub volume: f32,
    pub shuffle: String,
    pub repeat: String,
    pub queue_count: Option<usize>,
    pub queue_index: Option<usize>,
    pub native: PlaybackState,
}

pub fn format_bridge_state(state: PlaybackState, queue_len: Option<usize>) -> BridgePlaybackState {
    let status = match state.state {
        PlayState::Playing => "playing",
        PlayState::Paused => "paused",
        PlayState::Stopped => "stopped",
    }
    .to_string();

    let duration_seconds = state
        .current_song
        .as_ref()
        .and_then(|s| s.length_nanosec)
        .map(|ns| ns as f64 / 1_000_000_000.0);

    let position_seconds = (state.position_nanosec as f64 / 1_000_000_000.0).max(0.0);

    let shuffle = match state.shuffle_mode {
        ShuffleMode::Off => "off",
        ShuffleMode::All => "all",
        ShuffleMode::InsideAlbum => "inside_album",
        ShuffleMode::Albums => "albums",
        ShuffleMode::Artists => "artists",
    }
    .to_string();

    let repeat = match state.repeat_mode {
        RepeatMode::Off => "off",
        RepeatMode::Track => "track",
        RepeatMode::Album => "album",
        RepeatMode::Playlist => "all",
        RepeatMode::Intro => "intro",
    }
    .to_string();

    let current_track = state.current_song.clone().map(BridgeTrack::from);

    BridgePlaybackState {
        status,
        current_track,
        position_seconds,
        duration_seconds,
        volume: state.volume,
        shuffle,
        repeat,
        queue_count: queue_len,
        queue_index: None,
        native: state,
    }
}

#[derive(serde::Deserialize)]
struct ControlPayload {
    action: String,
    position_seconds: Option<f64>,
    volume: Option<f32>,
    shuffle: Option<serde_json::Value>,
    repeat: Option<String>,
}

#[derive(serde::Deserialize)]
struct PlayPayload {
    track_ids: Vec<i64>,
    start_index: Option<usize>,
}

#[derive(serde::Deserialize)]
struct EventPayload {
    event: String,
    data: Option<serde_json::Value>,
}

fn http_response(code: u16, status: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\n\r\n{}",
        code,
        status,
        body.len(),
        body
    )
    .into_bytes()
}

fn http_cors_options() -> Vec<u8> {
    b"HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nConnection: close\r\n\r\n".to_vec()
}

async fn execute_control_action(
    payload: &ControlPayload,
    app_state: &AppState,
    app: &AppHandle,
) -> Result<PlaybackState, String> {
    let mut player = app_state.player.lock().await;
    match payload.action.as_str() {
        "play" | "resume" => player.resume().await.map_err(|e| e.to_string())?,
        "pause" => player.pause().await.map_err(|e| e.to_string())?,
        "play_pause" => {
            let current = player.get_state().await;
            if current.state == PlayState::Playing {
                player.pause().await.map_err(|e| e.to_string())?;
            } else {
                player.resume().await.map_err(|e| e.to_string())?;
            }
        }
        "next" => {
            if let Some(stats) = player.note_manual_skip() {
                let _ = app.emit("song-stats-changed", stats);
            }
            player.next_track().await.map_err(|e| e.to_string())?;
        }
        "previous" => player.previous_track().await.map_err(|e| e.to_string())?,
        "seek" => {
            let pos = payload
                .position_seconds
                .ok_or_else(|| "Missing position_seconds for seek action".to_string())?;
            let nano = (pos.max(0.0) * 1_000_000_000.0) as u64;
            player.seek_to(nano).await.map_err(|e| e.to_string())?;
            let state = player.get_state().await;
            crate::media_session::mirror_state(app, &state).await;
        }
        "set_volume" => {
            let vol = payload
                .volume
                .ok_or_else(|| "Missing volume for set_volume action".to_string())?;
            let normalized = if vol > 1.0 { vol / 100.0 } else { vol };
            player
                .set_volume(normalized.clamp(0.0, 1.0))
                .await
                .map_err(|e| e.to_string())?;
        }
        "set_shuffle" => {
            let val = payload
                .shuffle
                .as_ref()
                .ok_or_else(|| "Missing shuffle for set_shuffle action".to_string())?;
            let mode = match val {
                serde_json::Value::Bool(true) => ShuffleMode::All,
                serde_json::Value::Bool(false) => ShuffleMode::Off,
                serde_json::Value::String(ref s) => match s.to_ascii_lowercase().as_str() {
                    "all" => ShuffleMode::All,
                    "inside_album" => ShuffleMode::InsideAlbum,
                    "albums" => ShuffleMode::Albums,
                    "artists" => ShuffleMode::Artists,
                    _ => ShuffleMode::Off,
                },
                _ => ShuffleMode::Off,
            };
            player.set_shuffle_mode(mode);
            let _ = app_state.audio.lock().await.clear_preload();
        }
        "set_repeat" => {
            let s = payload
                .repeat
                .as_deref()
                .ok_or_else(|| "Missing repeat for set_repeat action".to_string())?;
            let mode = match s.to_ascii_lowercase().as_str() {
                "all" | "playlist" => RepeatMode::Playlist,
                "one" | "track" => RepeatMode::Track,
                "album" => RepeatMode::Album,
                "intro" => RepeatMode::Intro,
                _ => RepeatMode::Off,
            };
            player.set_repeat_mode(mode);
            let _ = app_state.audio.lock().await.clear_preload();
        }
        other => return Err(format!("Unsupported control action: {}", other)),
    }

    Ok(player.get_state().await)
}

async fn handle_connection(mut stream: TcpStream, app: AppHandle) {
    let mut buffer = Vec::with_capacity(4096);
    let mut temp = [0u8; 1024];

    let header_end = loop {
        match stream.read(&mut temp).await {
            Ok(0) => return, // connection closed
            Ok(n) => {
                buffer.extend_from_slice(&temp[..n]);
                if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos + 4;
                }
                if buffer.len() > 16384 {
                    let _ = stream
                        .write_all(&http_response(413, "Payload Too Large", "{\"error\":\"Header too large\"}"))
                        .await;
                    return;
                }
            }
            Err(_) => return,
        }
    };

    let header_bytes = &buffer[..header_end];
    let header_str = match std::str::from_utf8(header_bytes) {
        Ok(s) => s,
        Err(_) => {
            let _ = stream
                .write_all(&http_response(400, "Bad Request", "{\"error\":\"Invalid UTF-8 in headers\"}"))
                .await;
            return;
        }
    };

    let mut lines = header_str.lines();
    let request_line = match lines.next() {
        Some(l) => l,
        None => return,
    };

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        let _ = stream
            .write_all(&http_response(400, "Bad Request", "{\"error\":\"Malformed request line\"}"))
            .await;
        return;
    }

    let method = parts[0];
    let raw_path = parts[1];
    let path = raw_path.split('?').next().unwrap_or(raw_path);

    if method == "OPTIONS" {
        let _ = stream.write_all(&http_cors_options()).await;
        return;
    }

    // Determine Content-Length
    let mut content_length: usize = 0;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }

    if content_length > 10 * 1024 * 1024 {
        let _ = stream
            .write_all(&http_response(413, "Payload Too Large", "{\"error\":\"Body too large\"}"))
            .await;
        return;
    }

    // Read remaining body bytes if needed
    let body_start = header_end;
    let mut body_bytes = buffer[body_start..].to_vec();
    while body_bytes.len() < content_length {
        match stream.read(&mut temp).await {
            Ok(0) => break,
            Ok(n) => body_bytes.extend_from_slice(&temp[..n]),
            Err(_) => break,
        }
    }

    let app_state = match app.try_state::<AppState>() {
        Some(s) => s,
        None => {
            let _ = stream
                .write_all(&http_response(503, "Service Unavailable", "{\"error\":\"App state not yet initialized\"}"))
                .await;
            return;
        }
    };

    let response = match (method, path) {
        ("GET", "/health") | ("GET", "/api/health") => {
            http_response(200, "OK", "{\"status\":\"ok\"}")
        }

        ("GET", "/playback") | ("GET", "/api/playback") => {
            let player_state = app_state.player.lock().await.get_state().await;
            let queue_len = {
                let playlists = app_state.playlists.lock().await;
                playlists
                    .queue()
                    .ok()
                    .and_then(|q| playlists.get_playlist_tracks(q.id).ok())
                    .map(|t| t.len())
            };
            let bridge_state = format_bridge_state(player_state, queue_len);
            let json = serde_json::to_string(&bridge_state).unwrap_or_else(|_| "{}".to_string());
            http_response(200, "OK", &json)
        }

        ("POST", "/playback/control") | ("POST", "/api/playback/control") => {
            match serde_json::from_slice::<ControlPayload>(&body_bytes) {
                Err(e) => {
                    http_response(400, "Bad Request", &format!("{{\"error\":\"Invalid JSON payload: {}\"}}", e))
                }
                Ok(payload) => {
                    match execute_control_action(&payload, &app_state, &app).await {
                        Err(err) => {
                            http_response(400, "Bad Request", &format!("{{\"error\":\"{}\"}}", err))
                        }
                        Ok(player_state) => {
                            let bridge_state = format_bridge_state(player_state, None);
                            let resp_json = serde_json::json!({
                                "success": true,
                                "state": bridge_state,
                            });
                            http_response(200, "OK", &resp_json.to_string())
                        }
                    }
                }
            }
        }

        ("POST", "/playback/play") | ("POST", "/api/playback/play") => {
            match serde_json::from_slice::<PlayPayload>(&body_bytes) {
                Err(e) => {
                    http_response(400, "Bad Request", &format!("{{\"error\":\"Invalid JSON payload: {}\"}}", e))
                }
                Ok(payload) => {
                    if payload.track_ids.is_empty() {
                        http_response(400, "Bad Request", "{\"error\":\"track_ids cannot be empty\"}")
                    } else {
                        let res = crate::commands::player::replace_queue_and_play_state(
                            &app_state,
                            &payload.track_ids,
                            payload.start_index.unwrap_or(0),
                            Some(PlayContext::Song),
                        )
                        .await;

                        match res {
                            Err(e) => {
                                http_response(400, "Bad Request", &format!("{{\"error\":\"{}\"}}", e))
                            }
                            Ok(()) => {
                                let player_state = app_state.player.lock().await.get_state().await;
                                let bridge_state =
                                    format_bridge_state(player_state, Some(payload.track_ids.len()));
                                let resp_json = serde_json::json!({
                                    "success": true,
                                    "message": format!("Queued {} tracks and started playback", payload.track_ids.len()),
                                    "state": bridge_state,
                                });
                                http_response(200, "OK", &resp_json.to_string())
                            }
                        }
                    }
                }
            }
        }

        ("POST", "/events/notify") | ("POST", "/api/events/notify") => {
            match serde_json::from_slice::<EventPayload>(&body_bytes) {
                Err(e) => {
                    http_response(400, "Bad Request", &format!("{{\"error\":\"Invalid JSON payload: {}\"}}", e))
                }
                Ok(payload) => {
                    if payload.event == "playlists-changed" {
                        let mut changed_ids: Vec<i64> = Vec::new();
                        if let Some(ref data) = payload.data {
                            if let Some(arr) = data.get("playlist_ids").and_then(|v| v.as_array()) {
                                for item in arr {
                                    if let Some(id) = item.as_i64() {
                                        changed_ids.push(id);
                                    }
                                }
                            } else if let Some(id) = data.get("playlist_id").and_then(|v| v.as_i64()) {
                                changed_ids.push(id);
                            }
                        }
                        let _ = app.emit("playlists-changed", changed_ids);
                        http_response(200, "OK", "{\"success\":true}")
                    } else {
                        // Forward generic event
                        let _ = app.emit(&payload.event, payload.data);
                        http_response(200, "OK", "{\"success\":true}")
                    }
                }
            }
        }

        _ => http_response(404, "Not Found", "{\"error\":\"Not Found\"}"),
    };

    let _ = stream.write_all(&response).await;
}

pub fn spawn_bridge_server(app: AppHandle) {
    let port = std::env::var("LUMINOUS_BRIDGE_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(DEFAULT_BRIDGE_PORT);

    let bind_addr = format!("{}:{}", BRIDGE_HOST, port);

    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind(&bind_addr).await {
            Ok(l) => {
                log::info!("Luminous desktop bridge server listening on http://{}", bind_addr);
                l
            }
            Err(e) => {
                log::warn!(
                    "Failed to bind Luminous desktop bridge server to {}: {}. Bridge features will be unavailable.",
                    bind_addr,
                    e
                );
                return;
            }
        };

        let connection_limit = Arc::new(Semaphore::new(MAX_CONCURRENT_BRIDGE_CONNECTIONS));

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    // Spawn unconditionally and acquire the permit *inside*
                    // the task, not here — acquiring before spawning would
                    // block this accept loop itself once the limit is hit,
                    // which stalls every subsequent connection (including
                    // ones that have nothing to do with the backlog) rather
                    // than just delaying this one. A burst past
                    // MAX_CONCURRENT_BRIDGE_CONNECTIONS reproduces this: the
                    // whole bridge server stops accepting new connections
                    // until the earlier ones drain.
                    let app_clone = app.clone();
                    let limit = Arc::clone(&connection_limit);
                    tokio::spawn(async move {
                        let Ok(_permit) = limit.acquire_owned().await else {
                            // Semaphore is only ever closed by dropping it,
                            // which doesn't happen while the server is alive.
                            return;
                        };
                        handle_connection(stream, app_clone).await;
                    });
                }
                Err(e) => {
                    log::debug!("Bridge listener accept error: {}", e);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FileType, PlayState, PlaybackState, RepeatMode, ShuffleMode, Song, SongSource};

    #[test]
    fn test_http_response_formatting() {
        let resp = http_response(200, "OK", "{\"status\":\"ok\"}");
        let s = String::from_utf8(resp).expect("valid utf-8");
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(s.contains("Content-Type: application/json; charset=utf-8\r\n"));
        assert!(s.contains("Content-Length: 15\r\n"));
        assert!(s.contains("Access-Control-Allow-Origin: *\r\n"));
        assert!(s.ends_with("\r\n\r\n{\"status\":\"ok\"}"));
    }

    #[test]
    fn test_http_cors_options() {
        let cors = http_cors_options();
        let s = String::from_utf8(cors).expect("valid utf-8");
        assert!(s.starts_with("HTTP/1.1 204 No Content\r\n"));
        assert!(s.contains("Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n"));
    }

    #[test]
    fn test_format_bridge_state() {
        let song = Song {
            id: 42,
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            path: Some("/music/test.flac".to_string()),
            url: None,
            stream_url: None,
            title: Some("Test Song".to_string()),
            artist: Some("Test Artist".to_string()),
            album: Some("Test Album".to_string()),
            length_nanosec: Some(240_000_000_000), // 240s
            ..Default::default()
        };

        let state = PlaybackState {
            state: PlayState::Playing,
            current_song: Some(song),
            playlist_id: Some(1),
            playlist_item_uuid: Some("uuid-123".to_string()),
            position_nanosec: 60_000_000_000, // 60s
            volume: 0.8,
            shuffle_mode: ShuffleMode::All,
            repeat_mode: RepeatMode::Playlist,
            ..Default::default()
        };

        let bridge = format_bridge_state(state, Some(15));
        assert_eq!(bridge.status, "playing");
        assert_eq!(bridge.position_seconds, 60.0);
        assert_eq!(bridge.duration_seconds, Some(240.0));
        assert_eq!(bridge.volume, 0.8);
        assert_eq!(bridge.shuffle, "all");
        assert_eq!(bridge.repeat, "all");
        assert_eq!(bridge.queue_count, Some(15));

        let track = bridge.current_track.expect("track exists");
        assert_eq!(track.id, 42);
        assert_eq!(track.title, Some("Test Song".to_string()));
        assert_eq!(track.artist, Some("Test Artist".to_string()));
        assert_eq!(track.album, Some("Test Album".to_string()));
        assert_eq!(track.duration_seconds, Some(240.0));
    }

    #[test]
    fn test_control_payload_deserialization() {
        let json = r#"{"action":"seek","position_seconds":125.5}"#;
        let payload: ControlPayload = serde_json::from_str(json).expect("valid control payload");
        assert_eq!(payload.action, "seek");
        assert_eq!(payload.position_seconds, Some(125.5));
        assert!(payload.volume.is_none());

        let json_vol = r#"{"action":"set_volume","volume":75}"#;
        let payload_vol: ControlPayload = serde_json::from_str(json_vol).expect("valid volume payload");
        assert_eq!(payload_vol.action, "set_volume");
        assert_eq!(payload_vol.volume, Some(75.0));

        let json_shuf = r#"{"action":"set_shuffle","shuffle":true}"#;
        let payload_shuf: ControlPayload = serde_json::from_str(json_shuf).expect("valid shuffle payload");
        assert_eq!(payload_shuf.action, "set_shuffle");
        assert_eq!(payload_shuf.shuffle, Some(serde_json::Value::Bool(true)));
    }

    #[test]
    fn test_play_payload_deserialization() {
        let json = r#"{"track_ids":[1,2,3,4],"start_index":2}"#;
        let payload: PlayPayload = serde_json::from_str(json).expect("valid play payload");
        assert_eq!(payload.track_ids, vec![1, 2, 3, 4]);
        assert_eq!(payload.start_index, Some(2));
    }

    #[test]
    fn test_event_payload_deserialization() {
        let json = r#"{"event":"playlists-changed","data":{"playlist_id":99}}"#;
        let payload: EventPayload = serde_json::from_str(json).expect("valid event payload");
        assert_eq!(payload.event, "playlists-changed");
        let data = payload.data.expect("data present");
        assert_eq!(data.get("playlist_id").and_then(|v| v.as_i64()), Some(99));

        let json_no_data = r#"{"event":"library-changed"}"#;
        let payload_no_data: EventPayload =
            serde_json::from_str(json_no_data).expect("valid event payload without data");
        assert_eq!(payload_no_data.event, "library-changed");
        assert!(payload_no_data.data.is_none());
    }
}
