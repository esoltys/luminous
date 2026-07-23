use crate::{
    collection::CollectionScanner,
    models::{HomeItem, LibraryStats, MusicDirectory, Song},
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
        .scan_all(app, force.unwrap_or(false))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn prune_missing_songs(state: State<'_, AppState>) -> Result<usize, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.prune_missing_songs().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_library_stats(state: State<'_, AppState>) -> Result<LibraryStats, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_library_stats().map_err(|e| e.to_string())
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

#[tauri::command]
pub async fn get_songs(
    limit: Option<i64>,
    offset: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs(limit.unwrap_or(1000), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
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
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_genre(&genre, limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_albums(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_albums().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_artists(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_artists().map_err(|e| e.to_string())
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
