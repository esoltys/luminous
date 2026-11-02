use crate::curation::{compute_curation_overview, CurationOverview, DuplicateTrackGroup};
use crate::AppState;
use tauri::{AppHandle, Emitter, State};

/// Retrieves the comprehensive collection curation overview across local library files.
#[tauri::command]
pub async fn get_curation_overview(state: State<'_, AppState>) -> Result<CurationOverview, String> {
    crate::db::run_blocking(&state.db, compute_curation_overview)
        .await
        .map_err(|e| e.to_string())
}

/// Retrieves clusters of duplicate candidate tracks detected across local library files (#1678).
#[tauri::command]
pub async fn get_duplicate_track_groups(
    state: State<'_, AppState>,
) -> Result<Vec<DuplicateTrackGroup>, String> {
    crate::db::run_blocking(&state.db, crate::curation::query_duplicate_track_groups)
        .await
        .map_err(|e| e.to_string())
}

/// Resolves a duplicate track cluster by merging history, rating, loved status, tags, and playlist spots
/// from retired copies into the preserved copy, optionally deleting redundant physical files (#1678).
#[tauri::command]
pub async fn resolve_duplicate_group(
    app: AppHandle,
    state: State<'_, AppState>,
    keep_song_id: i64,
    remove_song_ids: Vec<i64>,
    delete_files_from_disk: bool,
) -> Result<(), String> {
    crate::db::run_blocking(&state.db, move |conn| {
        crate::curation::resolve_duplicate_group_in_db(
            conn,
            keep_song_id,
            &remove_song_ids,
            delete_files_from_disk,
        )
    })
    .await
    .map_err(|e| e.to_string())?;

    crate::playlist::reconcile_and_sync(app.clone()).await;

    let _ = app.emit("library-changed", ());
    let _ = app.emit("song-stats-changed", ());

    Ok(())
}
