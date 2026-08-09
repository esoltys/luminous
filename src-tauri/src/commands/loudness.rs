use crate::models::LoudnessSettings;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_loudness_settings(state: State<'_, AppState>) -> Result<LoudnessSettings, String> {
    crate::loudness::get_settings(&state.db).map_err(|e| e.to_string())
}

/// Persists the full loudness settings struct in one call and refreshes the
/// live gain — mirrors `set_fade_settings` (`commands/settings.rs`), which
/// takes its whole settings struct rather than exposing a setter per field.
#[tauri::command]
pub async fn set_loudness_settings(
    state: State<'_, AppState>,
    mut settings: LoudnessSettings,
) -> Result<(), String> {
    settings.target_lufs = settings.target_lufs.clamp(-24.0, -14.0);
    settings.fallback_gain_db = settings.fallback_gain_db.clamp(-24.0, 0.0);
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
