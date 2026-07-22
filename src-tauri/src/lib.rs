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

pub mod analyzer;
pub mod audio;
pub mod collection;
pub mod commands;
pub mod covermanager;
pub mod db;
pub mod equalizer;
pub mod filter_parser;
pub mod loudness;
pub mod lyrics;
pub mod media_session;
pub mod models;
pub mod moodbar;
pub mod player;
pub mod playlist;
pub mod playlist_parsers;
pub mod stats;
pub mod tageditor;
pub mod waveform;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, ShortcutState};
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
    pub volume_before_mute: Arc<Mutex<f32>>,
    pub playlists: Arc<Mutex<PlaylistManager>>,
    pub cover_manager: Arc<CoverManager>,
    pub watcher: Arc<parking_lot::Mutex<Option<notify::RecommendedWatcher>>>,
    pub startup_file: Mutex<Option<String>>,
    /// OS "Now Playing" integration handle (#80) — `None` when the platform
    /// integration failed to initialize (unsupported desktop, no session
    /// bus, etc), in which case Luminous simply runs without it.
    pub media_session: Option<media_session::MediaSessionHandle>,
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
                let decoded = percent_encoding::percent_decode_str(local_path)
                    .decode_utf8_lossy()
                    .into_owned();
                std::path::PathBuf::from(decoded)
            } else {
                let decoded = percent_encoding::percent_decode_str(trimmed)
                    .decode_utf8_lossy()
                    .into_owned();
                covers_dir.join(decoded)
            };

            eprintln!(
                "[Luminous Backend] Custom protocol: URI = {}, Resolved path = {:?} (exists: {})",
                uri_str,
                file_path,
                file_path.exists()
            );

            if file_path.exists() && file_path.is_file() {
                if let Ok(data) = std::fs::read(&file_path) {
                    let mime = if file_path.extension().is_some_and(|e| e == "png") {
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
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
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
                if let Ok((enabled, preamp, gains_str, mode_str, parametric_json)) = conn.query_row(
                    "SELECT enabled, preamp, gains, mode, parametric
                         FROM equalizer_settings WHERE id = 1",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, i32>(0)? != 0,
                            row.get::<_, f64>(1)? as f32,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
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
                        if let Ok(bands) = serde_json::from_str::<
                            Vec<crate::equalizer::ParametricBand>,
                        >(&parametric_json)
                        {
                            if bands.len() == crate::equalizer::PARAMETRIC_BAND_COUNT {
                                let mut arr = crate::equalizer::default_parametric_bands();
                                arr.copy_from_slice(&bands);
                                eq.load_parametric(arr);
                            }
                        }
                        if mode_str == "parametric20" {
                            eq.set_mode(crate::equalizer::EqMode::Parametric20);
                        }
                    }
                }
            }

            let audio = Arc::new(Mutex::new(audio_engine));

            // Initialize player (needs db + audio refs)
            let player = Arc::new(Mutex::new(Player::new(Arc::clone(&db), Arc::clone(&audio))));
            let volume_before_mute = Arc::new(Mutex::new(1.0));

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
                let mut tick_counter: u32 = 0;
                loop {
                    interval.tick().await;
                    let (pos, state) = {
                        let engine = audio_ticks.lock().await;
                        (engine.current_position_nanosec(), engine.current_state())
                    };
                    if state == crate::models::PlayState::Playing {
                        let mut p = player_ticks.lock().await;
                        if let Some(stats) = p.on_position_update(pos) {
                            let _ = app_handle_ticks.emit("song-stats-changed", stats);
                        }
                        tick_counter = tick_counter.wrapping_add(1);
                        if tick_counter.is_multiple_of(4) {
                            p.persist_position(pos);
                            // MPRIS2's Position property isn't push-updated by
                            // souvlaki's D-Bus backend — it just returns
                            // whatever we last set, so without this the OS
                            // media session's seek bar freezes at the
                            // position from the last state transition (#80).
                            // SMTC interpolates its own timeline, so this is
                            // a no-op cost there beyond the periodic refresh.
                            let playback_snapshot = p.get_state().await;
                            crate::media_session::mirror_state(
                                &app_handle_ticks,
                                &playback_snapshot,
                            )
                            .await;
                        }
                        let _ = app_handle_ticks.emit(
                            "playback-position",
                            serde_json::json!({
                                "position_nanosec": pos
                            }),
                        );
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
                        let enabled = engine
                            .spectrum_enabled
                            .load(std::sync::atomic::Ordering::Relaxed);
                        let state = engine.current_state();
                        let spectrum = if enabled && state == crate::models::PlayState::Playing {
                            Some(crate::analyzer::calculate_spectrum(
                                &engine.visualizer_buf,
                                1024,
                            ))
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
                        let engine =
                            tauri::async_runtime::block_on(async { audio_events.lock().await });
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
                                    let _ = app.emit(
                                        "track-changed",
                                        serde_json::json!({
                                            "song": p.current_song.clone()
                                        }),
                                    );
                                    let state = p.get_state().await;
                                    crate::media_session::mirror_state(&app, &state).await;
                                    let _ = app.emit("playback-state", state);
                                }
                                crate::audio::AudioEvent::Paused => {
                                    let state = p.get_state().await;
                                    crate::media_session::mirror_state(&app, &state).await;
                                    let _ = app.emit("playback-state", state);
                                }
                                crate::audio::AudioEvent::Stopped => {
                                    let state = p.get_state().await;
                                    crate::media_session::mirror_state(&app, &state).await;
                                    let _ = app.emit("playback-state", state);
                                }
                                crate::audio::AudioEvent::TrackFinished { .. } => {
                                    let _ = p.on_track_finished().await;
                                    let state = p.get_state().await;
                                    crate::media_session::mirror_state(&app, &state).await;
                                    let _ = app.emit("playback-state", state);
                                }
                                crate::audio::AudioEvent::AboutToFinish { .. } => {
                                    // Prime the next track so the engine can
                                    // hand over gaplessly at the boundary.
                                    if let Err(e) = p.prepare_gapless_next().await {
                                        log::warn!("Gapless preload failed: {e}");
                                    }
                                }
                                crate::audio::AudioEvent::TrackTransitioned { song_id, .. } => {
                                    let _ = p.on_gapless_transition(song_id).await;
                                    let _ = app.emit(
                                        "track-changed",
                                        serde_json::json!({
                                            "song": p.current_song.clone()
                                        }),
                                    );
                                    let state = p.get_state().await;
                                    crate::media_session::mirror_state(&app, &state).await;
                                    let _ = app.emit("playback-state", state);
                                }
                                crate::audio::AudioEvent::Error { message } => {
                                    eprintln!(
                                        "[Luminous Backend] ERROR from audio engine: {}",
                                        message
                                    );
                                    let mut p = player.lock().await;
                                    let _ = p.next_track().await;
                                    let state = p.get_state().await;
                                    let _ = app.emit("playback-state", state);
                                }
                                _ => {}
                            }
                        });
                    }
                })
                .expect("failed to spawn event thread");

            let args: Vec<String> = std::env::args().collect();
            let startup_path = if args.len() > 1 {
                let p = &args[1];
                let path = std::path::Path::new(p);
                if path.exists() && path.is_file() {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if ext == "m3u" || crate::collection::AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                        Some(p.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let watcher = Arc::new(parking_lot::Mutex::new(None));

            // souvlaki needs an HWND on Windows to register SMTC for our window.
            #[cfg(target_os = "windows")]
            let media_hwnd: Option<*mut std::ffi::c_void> = app
                .get_webview_window("main")
                .and_then(|w| w.hwnd().ok())
                .map(|h| h.0 as *mut std::ffi::c_void);
            #[cfg(not(target_os = "windows"))]
            let media_hwnd: Option<*mut std::ffi::c_void> = None;

            let media_session = media_session::spawn(app.handle().clone(), media_hwnd);
            if media_session.is_none() {
                eprintln!(
                    "[Luminous Backend] OS media session integration (SMTC/MPRIS2/Now Playing) unavailable; continuing without it."
                );
            }

            let state = AppState {
                db,
                audio,
                player,
                volume_before_mute,
                playlists,
                cover_manager,
                watcher,
                startup_file: Mutex::new(startup_path),
                media_session,
            };

            // Start background directory watcher
            crate::collection::start_watcher(app.handle().clone(), &state);

            // Start background EBU R128 loudness analyzer (#77)
            crate::loudness::spawn_background_analyzer(app.handle().clone(), Arc::clone(&state.db));

            app.manage(state);

            let media_shortcuts = [
                "MediaPlayPause",
                "MediaTrackNext",
                "MediaTrackPrevious",
                "AudioVolumeUp",
                "AudioVolumeDown",
                "AudioVolumeMute",
            ];
            if let Err(err) =
                app.global_shortcut()
                    .on_shortcuts(media_shortcuts, |app, shortcut, event| {
                        if event.state != ShortcutState::Pressed {
                            return;
                        }

                        let key = shortcut.key;
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app_handle.state::<AppState>();
                            let mut player = state.player.lock().await;
                            let result = match key {
                                Code::MediaPlayPause => {
                                    let playback_state = player.get_state().await.state;
                                    if playback_state == crate::models::PlayState::Playing {
                                        player.pause().await
                                    } else {
                                        player.resume().await
                                    }
                                }
                                Code::MediaTrackNext => {
                                    if let Some(stats) = player.note_manual_skip() {
                                        let _ = app_handle.emit("song-stats-changed", stats);
                                    }
                                    player.next_track().await
                                }
                                Code::MediaTrackPrevious => player.previous_track().await,
                                Code::AudioVolumeUp => {
                                    let volume = player.get_state().await.volume;
                                    player.set_volume((volume + 0.05).min(1.0)).await
                                }
                                Code::AudioVolumeDown => {
                                    let volume = player.get_state().await.volume;
                                    player.set_volume((volume - 0.05).max(0.0)).await
                                }
                                Code::AudioVolumeMute => {
                                    let volume = player.get_state().await.volume;
                                    if volume > 0.0 {
                                        let mut volume_before_mute =
                                            state.volume_before_mute.lock().await;
                                        *volume_before_mute = volume;
                                        player.set_volume(0.0).await
                                    } else {
                                        let volume_before_mute =
                                            *state.volume_before_mute.lock().await;
                                        player.set_volume(volume_before_mute.max(0.05)).await
                                    }
                                }
                                _ => Ok(()),
                            };

                            if let Err(err) = result {
                                eprintln!(
                                    "[Luminous Backend] Failed to handle media key {:?}: {}",
                                    key, err
                                );
                            } else {
                                let playback_state = player.get_state().await;
                                crate::media_session::mirror_state(&app_handle, &playback_state)
                                    .await;
                                let _ = app_handle.emit("playback-state", playback_state);
                            }
                        });
                    })
            {
                eprintln!(
                    "[Luminous Backend] Failed to register media key shortcuts: {}",
                    err
                );
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Collection commands
            commands::collection::scan_directories,
            commands::collection::prune_missing_songs,
            commands::collection::add_directory,
            commands::collection::remove_directory,
            commands::collection::get_directories,
            commands::collection::get_library_stats,
            commands::collection::search_songs,
            commands::collection::get_songs,
            commands::collection::get_songs_by_album,
            commands::collection::get_songs_by_artist,
            commands::collection::get_albums,
            commands::collection::get_artists,
            commands::collection::get_favourite_songs,
            commands::collection::get_recently_added_songs,
            commands::collection::get_songs_by_genre,
            commands::collection::get_recently_played,
            commands::collection::get_most_frequently_played,
            commands::collection::get_recently_added,
            // Playback commands
            commands::player::play_song,
            commands::player::play_songs,
            commands::player::play_playlist_item,
            commands::player::open_and_play,
            commands::player::get_startup_file,
            commands::player::append_songs_to_player_playlist,
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
            commands::playlist::sync_genre_auto_playlists,
            commands::playlist::sync_decade_auto_playlists,
            commands::playlist::get_songs_by_decade,
            commands::playlist::get_playlists_by_artist,
            commands::playlist::get_playlist_tracks,
            commands::playlist::add_to_playlist,
            commands::playlist::remove_from_playlist,
            commands::playlist::reorder_playlist_item,
            commands::playlist::reorder_playlist_items,
            commands::playlist::clear_playlist,
            commands::playlist::undo_playlist,
            commands::playlist::redo_playlist,
            commands::playlist::import_playlist,
            commands::playlist::export_playlist,
            commands::playlist::set_playlist_auto_play,
            commands::playlist::set_playlist_dynamic_spec,
            commands::playlist::refill_auto_playlist,
            commands::playlist::refresh_auto_playlist,
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
            commands::equalizer::set_equalizer_mode,
            commands::equalizer::set_equalizer_band,
            commands::equalizer::set_parametric_band,
            commands::equalizer::reset_parametric_bands,
            commands::equalizer::set_equalizer_preamp,
            commands::equalizer::load_equalizer_preset,
            // Loudness normalization commands
            commands::loudness::get_loudness_settings,
            commands::loudness::set_loudness_enabled,
            commands::loudness::set_loudness_target_lufs,
            commands::loudness::set_loudness_mode,
            commands::loudness::set_loudness_fallback_gain,
            commands::loudness::get_loudness_analysis_remaining,
            // Lyrics commands
            commands::lyrics::get_lyrics,
            commands::lyrics::save_lyrics,
            commands::lyrics::set_instrumental,
            // Tag Editor commands
            commands::tageditor::get_song_details,
            commands::tageditor::lookup_acoustid_tags,
            commands::tageditor::save_song_tags,
            commands::tageditor::save_album_tags,
            // Settings commands
            commands::settings::set_app_setting,
            commands::settings::get_all_app_settings,
            commands::settings::get_commit_hash,
            commands::settings::get_fade_settings,
            commands::settings::set_fade_settings,
            // Stats commands
            commands::stats::set_song_rating,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Luminous");
}
