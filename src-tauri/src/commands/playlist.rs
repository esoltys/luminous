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
pub async fn sync_genre_auto_playlists(state: State<'_, AppState>) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .sync_genre_auto_playlists()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_decade_auto_playlists(state: State<'_, AppState>) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .sync_decade_auto_playlists()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_decade(
    state: State<'_, AppState>,
    decade: String,
    limit: Option<i64>,
) -> Result<Vec<crate::models::Song>, String> {
    let scanner = crate::collection::CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_decade(&decade, limit.unwrap_or(50))
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
pub async fn reorder_playlist_items(
    playlist_id: i64,
    from_indices: Vec<i32>,
    to_index: i32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .reorder_playlist_items_batch(playlist_id, &from_indices, to_index)
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

/// Toggle the Auto-Play flag on a dynamic playlist.  When enabled, the player
/// will automatically append the next batch of matching songs as playback
/// approaches the end of the current batch.
#[tauri::command]
pub async fn set_playlist_auto_play(
    playlist_id: i64,
    auto_play: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .set_playlist_auto_play(playlist_id, auto_play)
        .map_err(|e| e.to_string())
}

/// Fetch the next batch of songs for an auto-playlist's refill pass and append
/// them both to the DB playlist and to the live player queue.  Called by the
/// player when `remaining < threshold` and `auto_play` is true.
#[tauri::command]
pub async fn refill_auto_playlist(
    playlist_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::Song>, String> {
    // Read the playlist's dynamic_spec
    let (dynamic_spec, _auto_play) = {
        let pm = state.playlists.lock().await;
        let playlists = pm.get_playlists().map_err(|e| e.to_string())?;
        let pl = playlists
            .iter()
            .find(|p| p.id == playlist_id)
            .ok_or_else(|| format!("Playlist {} not found", playlist_id))?;
        (pl.dynamic_spec.clone().unwrap_or_default(), pl.auto_play)
    };

    if dynamic_spec.is_empty() {
        return Ok(vec![]);
    }

    // Get refill songs (excludes already-present tracks)
    let new_songs = {
        let pm = state.playlists.lock().await;
        pm.get_auto_playlist_refill_songs(playlist_id, &dynamic_spec, 25)
            .map_err(|e| e.to_string())?
    };

    if new_songs.is_empty() {
        return Ok(vec![]);
    }

    // Append new songs to the playlist_items table
    {
        let mut pm = state.playlists.lock().await;
        let ids: Vec<i64> = new_songs.iter().map(|s| s.id).collect();
        pm.add_songs_to_playlist(playlist_id, &ids)
            .map_err(|e| e.to_string())?;
    }

    Ok(new_songs)
}
