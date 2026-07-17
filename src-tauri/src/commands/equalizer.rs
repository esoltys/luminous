use crate::equalizer::{EqMode, Equalizer, ParametricBand, PARAMETRIC_BAND_COUNT};
use crate::AppState;
use tauri::State;

#[derive(serde::Serialize)]
pub struct EqualizerConfig {
    pub enabled: bool,
    pub mode: EqMode,
    pub gains: [f32; 10],
    pub preamp: f32,
    pub parametric: Vec<ParametricBand>,
}

impl EqualizerConfig {
    fn from_eq(eq: &Equalizer) -> Self {
        Self {
            enabled: eq.enabled,
            mode: eq.mode,
            gains: eq.gains,
            preamp: eq.preamp,
            parametric: eq.parametric.to_vec(),
        }
    }
}

fn save_eq_settings(db: &crate::db::Database, eq: &Equalizer) {
    if let Ok(conn) = db.pool.get() {
        let gains_str = eq
            .gains
            .iter()
            .map(|g| g.to_string())
            .collect::<Vec<String>>()
            .join(",");
        let mode_str = match eq.mode {
            EqMode::Graphic10 => "graphic10",
            EqMode::Parametric20 => "parametric20",
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
    Ok(EqualizerConfig::from_eq(&eq))
}

#[tauri::command]
pub async fn set_equalizer_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.enabled = enabled;
    save_eq_settings(&state.db, &eq);
    Ok(())
}

#[tauri::command]
pub async fn set_equalizer_mode(state: State<'_, AppState>, mode: EqMode) -> Result<(), String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.set_mode(mode);
    save_eq_settings(&state.db, &eq);
    Ok(())
}

#[tauri::command]
pub async fn set_equalizer_band(
    state: State<'_, AppState>,
    band_idx: usize,
    gain_db: f32,
) -> Result<(), String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.set_gain(band_idx, gain_db);
    save_eq_settings(&state.db, &eq);
    Ok(())
}

#[tauri::command]
pub async fn set_parametric_band(
    state: State<'_, AppState>,
    band_idx: usize,
    freq: f32,
    gain_db: f32,
    q: f32,
) -> Result<(), String> {
    if band_idx >= PARAMETRIC_BAND_COUNT {
        return Err(format!("band index {band_idx} out of range"));
    }
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.set_parametric_band(band_idx, freq, gain_db, q);
    save_eq_settings(&state.db, &eq);
    Ok(())
}

#[tauri::command]
pub async fn reset_parametric_bands(state: State<'_, AppState>) -> Result<EqualizerConfig, String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.load_parametric(crate::equalizer::default_parametric_bands());
    save_eq_settings(&state.db, &eq);
    Ok(EqualizerConfig::from_eq(&eq))
}

#[tauri::command]
pub async fn set_equalizer_preamp(
    state: State<'_, AppState>,
    preamp_db: f32,
) -> Result<(), String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.set_preamp(preamp_db);
    save_eq_settings(&state.db, &eq);
    Ok(())
}

#[tauri::command]
pub async fn load_equalizer_preset(
    state: State<'_, AppState>,
    preset_name: String,
) -> Result<EqualizerConfig, String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;

    let gains = match preset_name.to_lowercase().as_str() {
        "rock" => [4.0, 3.0, 2.0, -1.0, -2.0, -1.0, 1.0, 2.0, 3.0, 4.0],
        "pop" => [-2.0, -1.0, 0.0, 2.0, 4.0, 4.0, 2.0, 0.0, -1.0, -2.0],
        "classical" => [5.0, 3.0, 2.0, 2.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0],
        "jazz" => [3.0, 2.0, 1.0, 2.0, -1.0, -1.0, 0.0, 1.0, 2.0, 3.0],
        "bass boost" | "bassboost" => [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        "vocal boost" | "vocalboost" => [-2.0, -2.0, -1.0, 1.0, 3.0, 4.0, 3.0, 1.0, -1.0, -2.0],
        _ => [0.0; 10], // Flat
    };

    eq.load_preset(gains);
    save_eq_settings(&state.db, &eq);

    Ok(EqualizerConfig::from_eq(&eq))
}
