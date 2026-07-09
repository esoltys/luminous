use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_cover_art_uri(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<Option<String>, String> {
    state
        .cover_manager
        .get_cover_art_uri(song_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_remote_cover(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<Option<String>, String> {
    state
        .cover_manager
        .fetch_remote_cover(song_id)
        .await
        .map_err(|e| e.to_string())
}
