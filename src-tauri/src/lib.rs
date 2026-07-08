// Luminous Music Player — Backend Entry Point
//
// Module structure:
//   db        — SQLite schema, connection pool, migrations
//   models    — Core data types (Song, PlaylistItem, etc.)
//   commands  — All #[tauri::command] handlers
//   audio     — Symphonia + CPAL audio pipeline
//   player    — Playback state machine (shuffle, repeat, queue)
//   collection — Library scanner + file watcher
//   playlist  — Playlist CRUD + undo/redo

mod analyzer;
mod audio;
mod collection;
mod commands;
mod covermanager;
mod db;
pub mod equalizer;
mod lyrics;
mod models;
mod moodbar;
mod player;
mod playlist;
mod tageditor;
mod waveform;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

pub use audio::AudioEngine;
pub use covermanager::CoverManager;
pub use db::Database;
pub use player::Player;
pub use playlist::PlaylistManager;

/// Shared application state injected into every Tauri command.
pub struct AppState {
    pub db: Arc<Database>,
    pub audio: Arc<Mutex<AudioEngine>>,
    pub player: Arc<Mutex<Player>>,
    pub playlists: Arc<Mutex<PlaylistManager>>,
    pub cover_manager: Arc<CoverManager>,
    pub watcher: Arc<parking_lot::Mutex<Option<notify::RecommendedWatcher>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    tauri::Builder::default()
        .register_uri_scheme_protocol("luminous-art", move |ctx, request| {
            let app_handle = ctx.app_handle();
            let covers_dir = app_handle.path().app_data_dir().unwrap().join("covers");
            
            // Extract the resource path directly from the full URI string
            let uri_str = request.uri().to_string();
            let mut trimmed = &uri_str[..];
            if let Some(t) = uri_str.strip_prefix("http://luminous-art.localhost/") {
                trimmed = t;
            } else if let Some(t) = uri_str.strip_prefix("luminous-art://") {
                trimmed = t;
            }
            
            // If the webview prepends localhost/ to the authority, strip it
            if trimmed.starts_with("localhost/") {
                trimmed = trimmed.strip_prefix("localhost/").unwrap_or(trimmed);
            }
            
            // Webviews normalize empty paths to trailing slashes (e.g. URI/ -> path/)
            trimmed = trimmed.trim_end_matches('/');

            let file_path = if trimmed.starts_with("local/") {
                let local_path = trimmed.strip_prefix("local/").unwrap_or(trimmed);
                let decoded = percent_encoding::percent_decode_str(local_path).decode_utf8_lossy().into_owned();
                std::path::PathBuf::from(decoded)
            } else {
                let decoded = percent_encoding::percent_decode_str(trimmed).decode_utf8_lossy().into_owned();
                covers_dir.join(decoded)
            };

            eprintln!("[Luminous Backend] Custom protocol: URI = {}, Resolved path = {:?} (exists: {})", uri_str, file_path, file_path.exists());

            if file_path.exists() && file_path.is_file() {
                if let Ok(data) = std::fs::read(&file_path) {
                    let mime = if file_path.extension().map_or(false, |e| e == "png") {
                        "image/png"
                    } else {
                        "image/jpeg"
                    };
                    tauri::http::Response::builder()
                        .status(200)
                        .header("content-type", mime)
                        .header("access-control-allow-origin", "*")
                        .body(data)
                        .unwrap()
                } else {
                    tauri::http::Response::builder()
                        .status(500)
                        .header("access-control-allow-origin", "*")
                        .body(Vec::new())
                        .unwrap()
                }
            } else {
                tauri::http::Response::builder()
                    .status(404)
                    .header("access-control-allow-origin", "*")
                    .body(Vec::new())
                    .unwrap()
            }
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Initialize database (creates file + runs migrations)
            let db = Arc::new(
                Database::new(app.path().app_data_dir().expect("no app data dir"))
                    .expect("failed to initialize database"),
            );

            // Initialize audio engine
            let audio_engine = AudioEngine::new();

            // Load and restore equalizer settings on startup
            if let Ok(conn) = db.pool.get() {
                if let Ok((enabled, preamp, gains_str)) = conn.query_row(
                    "SELECT enabled, preamp, gains FROM equalizer_settings WHERE id = 1",
                    [],
                    |row| Ok((row.get::<_, i32>(0)? != 0, row.get::<_, f64>(1)? as f32, row.get::<_, String>(2)?)),
                ) {
                    let mut gains = [0.0f32; 10];
                    for (i, val) in gains_str.split(',').enumerate() {
                        if i < 10 {
                            if let Ok(gain) = val.parse::<f32>() {
                                gains[i] = gain;
                            }
                        }
                    }
                    if let Ok(mut eq) = audio_engine.equalizer.lock() {
                        eq.enabled = enabled;
                        eq.preamp = preamp;
                        eq.load_preset(gains);
                    }
                }
            }

            let audio = Arc::new(Mutex::new(audio_engine));

            // Initialize player (needs db + audio refs)
            let player = Arc::new(Mutex::new(Player::new(
                Arc::clone(&db),
                Arc::clone(&audio),
            )));

            // Initialize playlist manager
            let playlists = Arc::new(Mutex::new(
                PlaylistManager::new(Arc::clone(&db)).expect("failed to init playlists"),
            ));

            // Initialize cover manager
            let cover_manager = Arc::new(CoverManager::new(
                Arc::clone(&db),
                app.path().app_data_dir().expect("no app data dir"),
            ));

            // Spawn position tick loop (Tokio)
            let app_handle_ticks = app.handle().clone();
            let audio_ticks = Arc::clone(&audio);
            let player_ticks = Arc::clone(&player);
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
                loop {
                    interval.tick().await;
                    let (pos, state) = {
                        let engine = audio_ticks.lock().await;
                        (engine.current_position_nanosec(), engine.current_state())
                    };
                    if state == crate::models::PlayState::Playing {
                        let mut p = player_ticks.lock().await;
                        p.on_position_update(pos);
                        let _ = app_handle_ticks.emit("playback-position", serde_json::json!({
                            "position_nanosec": pos
                        }));
                    }
                }
            });

            // Spawn real-time visualizer spectrum emission loop (Tokio)
            let app_handle_visualizer = app.handle().clone();
            let audio_visualizer = Arc::clone(&audio);
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(33)); // ~30 FPS
                loop {
                    interval.tick().await;
                    let (enabled, spectrum) = {
                        let engine = audio_visualizer.lock().await;
                        let enabled = engine.spectrum_enabled.load(std::sync::atomic::Ordering::Relaxed);
                        let state = engine.current_state();
                        let spectrum = if enabled && state == crate::models::PlayState::Playing {
                            Some(crate::analyzer::calculate_spectrum(&engine.visualizer_buf, 1024))
                        } else {
                            None
                        };
                        (enabled, spectrum)
                    };

                    if enabled {
                        if let Some(spec) = spectrum {
                            let _ = app_handle_visualizer.emit("spectrum-data", spec);
                        }
                    }
                }
            });

            // Spawn event receiver loop (OS thread)
            let app_handle_events = app.handle().clone();
            let audio_events = Arc::clone(&audio);
            let player_events = Arc::clone(&player);
            std::thread::Builder::new()
                .name("luminous-events".to_string())
                .spawn(move || {
                    let rx = {
                        let engine = tauri::async_runtime::block_on(async {
                            audio_events.lock().await
                        });
                        engine.event_rx.clone()
                    };

                    let rx = rx.lock().unwrap();
                    for event in rx.iter() {
                        eprintln!("[Luminous Backend] Received event: {:?}", event);
                        let app = app_handle_events.clone();
                        let player = player_events.clone();
                        tauri::async_runtime::block_on(async move {
                            let mut p = player.lock().await;
                            match event {
                                crate::audio::AudioEvent::Playing { .. } => {
                                    let _ = app.emit("track-changed", serde_json::json!({
                                        "song": p.current_song.clone()
                                    }));
                                    let state = p.get_state().await;
                                    let _ = app.emit("playback-state", state);
                                }
                                crate::audio::AudioEvent::Paused => {
                                    let state = p.get_state().await;
                                    let _ = app.emit("playback-state", state);
                                }
                                crate::audio::AudioEvent::Stopped => {
                                    let state = p.get_state().await;
                                    let _ = app.emit("playback-state", state);
                                }
                                crate::audio::AudioEvent::TrackFinished { .. } => {
                                    let _ = p.on_track_finished().await;
                                    let state = p.get_state().await;
                                    let _ = app.emit("playback-state", state);
                                }
                                crate::audio::AudioEvent::Error { message } => {
                                    eprintln!("[Luminous Backend] ERROR from audio engine: {}", message);
                                }
                                _ => {}
                            }
                        });
                    }
                })
                .expect("failed to spawn event thread");

            let watcher = Arc::new(parking_lot::Mutex::new(None));
            let state = AppState {
                db,
                audio,
                player,
                playlists,
                cover_manager,
                watcher,
            };

            // Start background directory watcher
            crate::collection::start_watcher(app.handle().clone(), &state);

            app.manage(state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Collection commands
            commands::collection::scan_directories,
            commands::collection::add_directory,
            commands::collection::remove_directory,
            commands::collection::get_directories,
            commands::collection::get_library_stats,
            commands::collection::search_songs,
            commands::collection::get_songs,
            commands::collection::get_songs_by_album,
            commands::collection::get_albums,
            commands::collection::get_artists,
            // Playback commands
            commands::player::play_song,
            commands::player::play_songs,
            commands::player::play_playlist_item,
            commands::player::pause,
            commands::player::resume,
            commands::player::stop,
            commands::player::next_track,
            commands::player::previous_track,
            commands::player::seek_to,
            commands::player::set_volume,
            commands::player::get_playback_state,
            commands::player::set_shuffle_mode,
            commands::player::set_repeat_mode,
            // Playlist commands
            commands::playlist::create_playlist,
            commands::playlist::delete_playlist,
            commands::playlist::rename_playlist,
            commands::playlist::get_playlists,
            commands::playlist::get_playlist_tracks,
            commands::playlist::add_to_playlist,
            commands::playlist::remove_from_playlist,
            commands::playlist::reorder_playlist_item,
            commands::playlist::clear_playlist,
            commands::playlist::undo_playlist,
            commands::playlist::redo_playlist,
            // Cover Art commands
            commands::cover::get_cover_art_uri,
            commands::cover::fetch_remote_cover,
            // Visualizer commands
            commands::visualizer::get_waveform_data,
            commands::visualizer::get_moodbar_data,
            commands::visualizer::set_spectrum_enabled,
            // Equalizer commands
            commands::equalizer::get_equalizer_state,
            commands::equalizer::set_equalizer_enabled,
            commands::equalizer::set_equalizer_band,
            commands::equalizer::set_equalizer_preamp,
            commands::equalizer::load_equalizer_preset,
            // Lyrics commands
            commands::lyrics::get_lyrics,
            commands::lyrics::save_lyrics,
            // Tag Editor commands
            commands::tageditor::get_song_details,
            commands::tageditor::lookup_acoustid_tags,
            commands::tageditor::save_song_tags,
            // Settings commands
            commands::settings::set_app_setting,
            commands::settings::get_all_app_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Luminous");
}
