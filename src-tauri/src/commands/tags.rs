use crate::{
    models::{GenreGroup, QueuePopulationMode, Song, Tag},
    tags::TagManager,
    AppState,
};
use tauri::State;

#[derive(serde::Serialize)]
pub struct TagsOverview {
    pub tags: Vec<Tag>,
    pub graph: Vec<GenreGroup>,
    pub no_genre_count: i64,
}

/// Combined `list_all_tags` + `get_genre_graph` in one call — the pair the
/// Genres tab always needs together, sharing a single DB scan instead of
/// each independently re-scanning every song's genre column. Also reports
/// how many songs carry no genre at all, so the tab can surface them as a
/// "No Genre" group.
#[tauri::command]
pub async fn get_tags_overview(state: State<'_, AppState>) -> Result<TagsOverview, String> {
    let manager = TagManager::new(state.db.clone());
    let (tags, graph, no_genre_count) = manager.get_tags_overview().map_err(|e| e.to_string())?;
    Ok(TagsOverview {
        tags,
        graph,
        no_genre_count,
    })
}

#[tauri::command]
pub async fn get_songs_without_genre(
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .get_songs_without_genre(limit.unwrap_or(50), mode.unwrap_or_default())
        .map_err(|e| e.to_string())
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
pub async fn get_songs_by_main_tag(
    tag_name: String,
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .get_songs_by_main_tag(&tag_name, limit.unwrap_or(50), mode.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_genre_edge(
    root_tag: String,
    child_tag: String,
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .get_songs_by_genre_edge(&root_tag, &child_tag, limit.unwrap_or(50), mode.unwrap_or_default())
        .map_err(|e| e.to_string())
}
