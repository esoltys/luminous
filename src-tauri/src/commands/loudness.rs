use crate::models::{LoudnessMode, LoudnessSettings};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_loudness_settings(state: State<'_, AppState>) -> Result<LoudnessSettings, String> {
    crate::loudness::get_settings(&state.db).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_loudness_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let mut settings = crate::loudness::get_settings(&state.db).map_err(|e| e.to_string())?;
    settings.enabled = enabled;
    crate::loudness::save_settings(&state.db, &settings).map_err(|e| e.to_string())?;
    state.player.lock().await.refresh_loudness_gain().await;
    Ok(())
}

#[tauri::command]
pub async fn set_loudness_target_lufs(
    state: State<'_, AppState>,
    target_lufs: f32,
) -> Result<(), String> {
    let mut settings = crate::loudness::get_settings(&state.db).map_err(|e| e.to_string())?;
    settings.target_lufs = target_lufs.clamp(-24.0, -14.0);
    crate::loudness::save_settings(&state.db, &settings).map_err(|e| e.to_string())?;
    state.player.lock().await.refresh_loudness_gain().await;
    Ok(())
}

#[tauri::command]
pub async fn set_loudness_mode(
    state: State<'_, AppState>,
    mode: LoudnessMode,
) -> Result<(), String> {
    let mut settings = crate::loudness::get_settings(&state.db).map_err(|e| e.to_string())?;
    settings.mode = mode;
    crate::loudness::save_settings(&state.db, &settings).map_err(|e| e.to_string())?;
    state.player.lock().await.refresh_loudness_gain().await;
    Ok(())
}

#[tauri::command]
pub async fn set_loudness_fallback_gain(
    state: State<'_, AppState>,
    fallback_gain_db: f32,
) -> Result<(), String> {
    let mut settings = crate::loudness::get_settings(&state.db).map_err(|e| e.to_string())?;
    settings.fallback_gain_db = fallback_gain_db.clamp(-24.0, 0.0);
    crate::loudness::save_settings(&state.db, &settings).map_err(|e| e.to_string())?;
    state.player.lock().await.refresh_loudness_gain().await;
    Ok(())
}

/// Count of local/collection tracks still awaiting R128 analysis, for the
/// settings UI's background-progress line.
#[tauri::command]
pub async fn get_loudness_analysis_remaining(state: State<'_, AppState>) -> Result<i64, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT COUNT(*) FROM songs
         WHERE source IN (1, 2) AND unavailable = 0 AND path IS NOT NULL
           AND ebur128_integrated_loudness_lufs IS NULL",
        [],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}
