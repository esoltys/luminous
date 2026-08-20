use crate::{
    models::{GenreGroup, QueuePopulationMode, Song, Tag},
    tags::TagManager,
    AppState,
};
use tauri::State;

#[tauri::command]
pub async fn list_all_tags(state: State<'_, AppState>) -> Result<Vec<Tag>, String> {
    let manager = TagManager::new(state.db.clone());
    manager.list_all_tags().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_tag(
    tag_name: String,
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .get_songs_by_tag(&tag_name, limit.unwrap_or(50), mode.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_genre_graph(state: State<'_, AppState>) -> Result<Vec<GenreGroup>, String> {
    let manager = TagManager::new(state.db.clone());
    manager.get_genre_graph().map_err(|e| e.to_string())
}
