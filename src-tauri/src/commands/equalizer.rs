use crate::AppState;
use tauri::State;

#[derive(serde::Serialize)]
pub struct EqualizerConfig {
    pub enabled: bool,
    pub gains: [f32; 10],
    pub preamp: f32,
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
pub async fn set_equalizer_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.enabled = enabled;
    Ok(())
}

#[tauri::command]
pub async fn set_equalizer_band(state: State<'_, AppState>, band_idx: usize, gain_db: f32) -> Result<(), String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.set_gain(band_idx, gain_db);
    Ok(())
}

#[tauri::command]
pub async fn set_equalizer_preamp(state: State<'_, AppState>, preamp_db: f32) -> Result<(), String> {
    let engine = state.audio.lock().await;
    let mut eq = engine.equalizer.lock().map_err(|e| e.to_string())?;
    eq.set_preamp(preamp_db);
    Ok(())
}

#[tauri::command]
pub async fn load_equalizer_preset(state: State<'_, AppState>, preset_name: String) -> Result<EqualizerConfig, String> {
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
    Ok(EqualizerConfig {
        enabled: eq.enabled,
        gains: eq.gains,
        preamp: eq.preamp,
    })
}
