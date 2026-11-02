use crate::curation::{compute_curation_overview, CurationOverview};
use crate::AppState;
use tauri::State;

/// Retrieves the comprehensive collection curation overview across local library files.
#[tauri::command]
pub async fn get_curation_overview(state: State<'_, AppState>) -> Result<CurationOverview, String> {
    crate::db::run_blocking(&state.db, compute_curation_overview)
        .await
        .map_err(|e| e.to_string())
}
