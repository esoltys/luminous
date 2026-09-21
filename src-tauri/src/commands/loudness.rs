use crate::models::{LoudnessSettings, LOCAL_SOURCES_SQL};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_loudness_settings(state: State<'_, AppState>) -> Result<LoudnessSettings, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || crate::loudness::get_settings(&db))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Persists the full loudness settings struct in one call and refreshes the
/// live gain — mirrors `set_fade_settings` (`commands/settings.rs`), which
/// takes its whole settings struct rather than exposing a setter per field.
#[tauri::command]
pub async fn set_loudness_settings(
    state: State<'_, AppState>,
    mut settings: LoudnessSettings,
) -> Result<(), String> {
    settings.target_lufs = settings.target_lufs.clamp(-23.0, -9.0);
    settings.fallback_gain_db = settings.fallback_gain_db.clamp(-24.0, 0.0);
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || crate::loudness::save_settings(&db, &settings))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    // `refresh_loudness_gain` is genuinely async (it awaits a gain ramp), not
    // a sync call to offload — its own internal DB read already goes through
    // `Player::load_loudness_settings`'s `spawn_blocking` (#1097).
    state.player.lock().await.refresh_loudness_gain().await;
    Ok(())
}

/// Count of local/collection tracks still awaiting R128 analysis, for the
/// settings UI's background-progress line.
#[tauri::command]
pub async fn get_loudness_analysis_remaining(state: State<'_, AppState>) -> Result<i64, String> {
    crate::db::run_blocking(&state.db, |conn| {
        conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM songs
             WHERE source IN ({lib}) AND unavailable = 0 AND path IS NOT NULL
               AND ebur128_integrated_loudness_lufs IS NULL",
                lib = *LOCAL_SOURCES_SQL
            ),
            [],
            |row| row.get(0),
        )
        .map_err(anyhow::Error::from)
    })
    .await
    .map_err(|e| e.to_string())
}
