use crate::{
    collection::CollectionScanner,
    models::{LibraryStats, MusicDirectory, Song},
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
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.scan_all(app).await.map_err(|e| e.to_string())
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
    album_artist: String,
    album: String,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_album(&album_artist, &album)
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
