use crate::AppState;
use tauri::State;

#[derive(serde::Serialize)]
pub struct EqualizerConfig {
    pub enabled: bool,
    pub gains: [f32; 10],
    pub preamp: f32,
}

fn save_eq_settings(db: &crate::db::Database, enabled: bool, preamp: f32, gains: &[f32; 10]) {
    if let Ok(conn) = db.pool.get() {
        let gains_str = gains
            .iter()
            .map(|g| g.to_string())
            .collect::<Vec<String>>()
            .join(",");
        let _ = conn.execute(
            "UPDATE equalizer_settings SET enabled = ?1, preamp = ?2, gains = ?3 WHERE id = 1",
            rusqlite::params![if enabled { 1 } else { 0 }, preamp as f64, gains_str],
        );
    }
}

#[tauri::command]
pub async fn get_equalizer_state(state: State<'_, AppState>) -> Result<EqualizerConfig, String> {
    let engine = state.audio.lock().await;
    let eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    Ok(EqualizerConfig {
        enabled: eq.enabled,
        gains: eq.gains,
        preamp: eq.preamp,
    })
}

#[tauri::command]
pub async fn set_equalizer_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.enabled = enabled;
    save_eq_settings(&state.db, eq.enabled, eq.preamp, &eq.gains);
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
    save_eq_settings(&state.db, eq.enabled, eq.preamp, &eq.gains);
    Ok(())
}

#[tauri::command]
pub async fn set_equalizer_preamp(
    state: State<'_, AppState>,
    preamp_db: f32,
) -> Result<(), String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.set_preamp(preamp_db);
    save_eq_settings(&state.db, eq.enabled, eq.preamp, &eq.gains);
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
    save_eq_settings(&state.db, eq.enabled, eq.preamp, &eq.gains);

    Ok(EqualizerConfig {
        enabled: eq.enabled,
        gains: eq.gains,
        preamp: eq.preamp,
    })
}
