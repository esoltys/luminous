use crate::{
    collection::CollectionScanner,
    models::{HomeItem, LibraryStats, MusicDirectory, PruneResult, QueuePopulationMode, Song},
    AppState,
};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn add_directory(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<MusicDirectory, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    let res = scanner.add_directory(&path).map_err(|e| e.to_string())?;
    crate::collection::start_watcher(app, &state);
    Ok(res)
}

#[tauri::command]
pub async fn remove_directory(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.remove_directory(&path).map_err(|e| e.to_string())?;
    crate::collection::start_watcher(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn get_directories(state: State<'_, AppState>) -> Result<Vec<MusicDirectory>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_directories().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_directories(
    app: AppHandle,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .scan_all(app, force.unwrap_or(false), false)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn prune_missing_songs(state: State<'_, AppState>) -> Result<PruneResult, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.prune_missing_songs().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_library_stats(state: State<'_, AppState>) -> Result<LibraryStats, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_library_stats().map_err(|e| e.to_string())
}

/// Runs the backend-consistency steps a completed scan requires: persist
/// `last_scan_time`, resync the live playback queue with the DB (a scan can
/// repoint a moved file's path or drop a missing one out from under an
/// already-queued track), and resync auto-playlists (a scan can add new
/// genres/decades or shift which songs qualify). Callers no longer need to
/// remember to fire all three separately.
#[tauri::command]
pub async fn finish_scan(last_scan_time: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(conn) = state.db.pool.get() {
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_scan_time', ?1)",
            [&last_scan_time],
        ) {
            log::error!("Failed to persist last_scan_time: {e}");
        }
    }

    if let Err(e) = state.player.lock().await.resync_queue_with_db() {
        log::error!("Failed to resync playback queue after scan: {e}");
    }

    state
        .playlists
        .lock()
        .await
        .sync_all_auto_playlists()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_songs(
    query: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .search_songs(&query, limit.unwrap_or(500))
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct LibrarySnapshot {
    pub songs: Vec<Song>,
    pub albums: Vec<serde_json::Value>,
    pub artists: Vec<serde_json::Value>,
}

/// The Collection view's "give me everything" read — songs, albums, and
/// artists always get refreshed together (initial load, post-scan, on
/// library-changed events), so callers no longer have to remember to fire
/// all three round trips themselves.
#[tauri::command]
pub async fn get_library_snapshot(state: State<'_, AppState>) -> Result<LibrarySnapshot, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    Ok(LibrarySnapshot {
        songs: scanner.get_songs(-1, 0).map_err(|e| e.to_string())?,
        albums: scanner.get_albums().map_err(|e| e.to_string())?,
        artists: scanner.get_artists().map_err(|e| e.to_string())?,
    })
}

#[tauri::command]
pub async fn get_songs_by_album(
    album: String,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_album(&album)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_artist(
    artist: String,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_artist(&artist)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_favourite_songs(state: State<'_, AppState>) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_favourite_songs().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_added_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_added_songs(limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_genre(
    genre: String,
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_genre(&genre, limit.unwrap_or(50), mode.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_top_artists(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_top_artists(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_played(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_played(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_played_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_played_songs(limit.unwrap_or(100))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_play_history(state: State<'_, AppState>) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.clear_play_history().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_most_frequently_played(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_most_frequently_played(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_added(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_added(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}
