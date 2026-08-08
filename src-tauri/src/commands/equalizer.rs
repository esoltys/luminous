use crate::equalizer::{Equalizer, EqualizerConfig};
use crate::AppState;
use tauri::State;

/// Fire-and-forget persistence — a failed EQ write is invisible to the user
/// mid-drag and nothing the UI can act on.
fn save_eq_settings(db: &crate::db::Database, eq: &Equalizer) {
    if let Ok(conn) = db.pool.get() {
        let gains_str = eq
            .gains
            .iter()
            .map(|g| g.to_string())
            .collect::<Vec<String>>()
            .join(",");
        let mode_str = match eq.mode {
            crate::equalizer::EqMode::Graphic10 => "graphic10",
            crate::equalizer::EqMode::Parametric20 => "parametric20",
        };
        let parametric_json = serde_json::to_string(&eq.parametric.to_vec()).unwrap_or_default();
        let _ = conn.execute(
            "UPDATE equalizer_settings
             SET enabled = ?1, preamp = ?2, gains = ?3, mode = ?4, parametric = ?5
             WHERE id = 1",
            rusqlite::params![
                if eq.enabled { 1 } else { 0 },
                eq.preamp as f64,
                gains_str,
                mode_str,
                parametric_json
            ],
        );
    }
}

#[tauri::command]
pub async fn get_equalizer_state(state: State<'_, AppState>) -> Result<EqualizerConfig, String> {
    let engine = state.audio.lock().await;
    let eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    Ok(EqualizerConfig::snapshot(&eq))
}

/// The one EQ mutation entry point: the frontend edits a config and applies
/// it whole; the engine clamps/normalizes and echoes the canonical state.
#[tauri::command]
pub async fn apply_equalizer_config(
    state: State<'_, AppState>,
    config: EqualizerConfig,
) -> Result<EqualizerConfig, String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    let canonical = eq.apply(&config);
    save_eq_settings(&state.db, &eq);
    Ok(canonical)
}

#[tauri::command]
pub async fn reset_parametric_bands(state: State<'_, AppState>) -> Result<EqualizerConfig, String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.load_parametric(crate::equalizer::default_parametric_bands());
    save_eq_settings(&state.db, &eq);
    Ok(EqualizerConfig::snapshot(&eq))
}

#[tauri::command]
pub async fn load_equalizer_preset(
    state: State<'_, AppState>,
    preset_name: String,
) -> Result<EqualizerConfig, String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;

    let gains = crate::equalizer::preset_gains(&preset_name);

    // Always update the graphic gains so the preset is intact if the user
    // switches back to 10-band; additionally map it onto the parametric bands
    // when that mode is active so the same named presets work there too.
    eq.load_preset(gains);
    if eq.mode == crate::equalizer::EqMode::Parametric20 {
        eq.load_preset_into_parametric(gains);
    }
    save_eq_settings(&state.db, &eq);

    Ok(EqualizerConfig::snapshot(&eq))
}
