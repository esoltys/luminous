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

mod audio;
mod collection;
mod commands;
mod db;
mod models;
mod player;
mod playlist;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

pub use audio::AudioEngine;
pub use db::Database;
pub use player::Player;
pub use playlist::PlaylistManager;

/// Shared application state injected into every Tauri command.
pub struct AppState {
    pub db: Arc<Database>,
    pub audio: Arc<Mutex<AudioEngine>>,
    pub player: Arc<Mutex<Player>>,
    pub playlists: Arc<Mutex<PlaylistManager>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
            let audio = Arc::new(Mutex::new(AudioEngine::new()));

            // Initialize player (needs db + audio refs)
            let player = Arc::new(Mutex::new(Player::new(
                Arc::clone(&db),
                Arc::clone(&audio),
            )));

            // Initialize playlist manager
            let playlists = Arc::new(Mutex::new(
                PlaylistManager::new(Arc::clone(&db)).expect("failed to init playlists"),
            ));

            app.manage(AppState {
                db,
                audio,
                player,
                playlists,
            });

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Luminous");
}
