//! Tauri command handlers for tag-based file organization.

use crate::organizer::{
    self, OrganizeApplyItem, OrganizeOptions, OrganizePreviewItem, OrganizeResult,
};
use crate::AppState;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn preview_organize(
    state: State<'_, AppState>,
    song_ids: Vec<i64>,
    template: String,
    options: OrganizeOptions,
) -> Result<Vec<OrganizePreviewItem>, String> {
    let db = &state.db;
    organizer::compute_preview(db, &song_ids, &template, &options).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_organize(
    app: AppHandle,
    state: State<'_, AppState>,
    items: Vec<OrganizeApplyItem>,
    clean_empty_dirs: bool,
    move_extra_files: bool,
) -> Result<OrganizeResult, String> {
    let db = &state.db;
    let watcher_paused = &state.watcher_paused;

    let res = organizer::execute_apply(
        db,
        watcher_paused,
        &items,
        clean_empty_dirs,
        move_extra_files,
    )
    .map_err(|e| e.to_string())?;

    let _ = app.emit("library-changed", ());

    Ok(res)
}
