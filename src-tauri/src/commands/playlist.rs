use crate::{
    models::{Playlist, PlaylistItem},
    AppState,
};
use tauri::State;

#[tauri::command]
pub async fn create_playlist(name: String, state: State<'_, AppState>) -> Result<Playlist, String> {
    state
        .playlists
        .lock()
        .await
        .create_playlist(&name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_playlist(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .delete_playlist(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_playlist(
    id: i64,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .rename_playlist(id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playlists(state: State<'_, AppState>) -> Result<Vec<Playlist>, String> {
    state
        .playlists
        .lock()
        .await
        .get_playlists()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playlists_by_artist(
    artist: String,
    state: State<'_, AppState>,
) -> Result<Vec<Playlist>, String> {
    state
        .playlists
        .lock()
        .await
        .get_playlists_by_artist(&artist)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playlist_tracks(
    playlist_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<PlaylistItem>, String> {
    state
        .playlists
        .lock()
        .await
        .get_playlist_tracks(playlist_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_to_playlist(
    playlist_id: i64,
    song_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .add_songs_to_playlist(playlist_id, &song_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_from_playlist(
    playlist_id: i64,
    uuids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .remove_from_playlist(playlist_id, &uuids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reorder_playlist_item(
    playlist_id: i64,
    from: i32,
    to: i32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .reorder_playlist_item(playlist_id, from, to)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_playlist(playlist_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .clear_playlist(playlist_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn undo_playlist(state: State<'_, AppState>) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .undo()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn redo_playlist(state: State<'_, AppState>) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .redo()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_playlist(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<Playlist, String> {
    state
        .playlists
        .lock()
        .await
        .import_playlist(&file_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_playlist(
    playlist_id: i64,
    export_path: String,
    relative: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .export_playlist(playlist_id, &export_path, relative)
        .map_err(|e| e.to_string())
}
