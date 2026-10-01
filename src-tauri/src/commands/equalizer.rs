use crate::eq_presets::{self, UserPreset};
use crate::equalizer::{EqMode, Equalizer, EqualizerConfig, BUILTIN_PRESETS};
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

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
            crate::equalizer::EqMode::Parametric => "parametric",
        };
        let parametric_json = serde_json::to_string(eq.parametric_bands()).unwrap_or_default();
        let _ = conn.execute(
            "UPDATE equalizer_settings
             SET enabled = ?1, preamp = ?2, gains = ?3, mode = ?4, parametric = ?5,
                 active_preset = ?6
             WHERE id = 1",
            rusqlite::params![
                if eq.enabled { 1 } else { 0 },
                eq.preamp as f64,
                gains_str,
                mode_str,
                parametric_json,
                eq.active_preset.as_deref().unwrap_or("")
            ],
        );
    }
}

/// The pipeline popover summarises the EQ (on/off, mode, active bands), but
/// the engine only re-emits pipeline info on track change or stream rebuild —
/// so every command that can change that summary pushes a fresh copy.
async fn emit_pipeline_changed(app: &AppHandle, state: &AppState) {
    let player = state.player.lock().await;
    let audio = state.audio.lock().await;
    let _ = app.emit("audio-pipeline-changed", player.get_pipeline_info(&audio));
}

#[tauri::command]
pub async fn get_equalizer_state(state: State<'_, AppState>) -> Result<EqualizerConfig, String> {
    Ok(crate::audio::with_audio(&state.audio, |engine| {
        engine.with_equalizer(|eq| EqualizerConfig::snapshot(eq))
    })
    .await)
}

/// The one EQ mutation entry point: the frontend edits a config and applies
/// it whole; the engine clamps/normalizes and echoes the canonical state.
#[tauri::command]
pub async fn apply_equalizer_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: EqualizerConfig,
) -> Result<EqualizerConfig, String> {
    let db = state.db.clone();
    let canonical = crate::audio::with_audio(&state.audio, move |engine| {
        engine.with_equalizer(|eq| {
            let canonical = eq.apply(&config);
            save_eq_settings(&db, eq);
            canonical
        })
    })
    .await;
    emit_pipeline_changed(&app, &state).await;
    Ok(canonical)
}

#[tauri::command]
pub async fn reset_parametric_bands(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<EqualizerConfig, String> {
    let db = state.db.clone();
    let canonical = crate::audio::with_audio(&state.audio, move |engine| {
        engine.with_equalizer(|eq| {
            eq.load_parametric(&crate::equalizer::default_parametric_bands());
            // The default layout is the parametric Flat preset.
            if eq.mode == EqMode::Parametric {
                eq.active_preset = Some("Flat".to_string());
            }
            save_eq_settings(&db, eq);
            EqualizerConfig::snapshot(eq)
        })
    })
    .await;
    emit_pipeline_changed(&app, &state).await;
    Ok(canonical)
}

/// Load a preset by picker key: a `BUILTIN_PRESETS` name or `user:<id>`.
#[tauri::command]
pub async fn load_equalizer_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    preset_name: String,
) -> Result<EqualizerConfig, String> {
    let db = state.db.clone();
    let user = match crate::equalizer::parse_user_preset_key(&preset_name) {
        Some(id) => {
            let conn = db.pool.get().map_err(|e| e.to_string())?;
            Some((id, eq_presets::get(&conn, id)?))
        }
        None => None,
    };
    let canonical = crate::audio::with_audio(&state.audio, move |engine| {
        engine.with_equalizer(|eq| {
            match user {
                Some((id, preset)) => eq.load_user_preset(id, &preset.bands, preset.preamp),
                None if eq.load_builtin_preset(&preset_name) => {}
                None => return Err(format!("unknown preset: {preset_name}")),
            }
            save_eq_settings(&db, eq);
            Ok(EqualizerConfig::snapshot(eq))
        })
    })
    .await?;
    emit_pipeline_changed(&app, &state).await;
    Ok(canonical)
}

#[derive(Serialize)]
pub struct EqPresetList {
    pub builtin: Vec<&'static str>,
    pub user: Vec<UserPreset>,
}

#[tauri::command]
pub async fn list_eq_presets(state: State<'_, AppState>) -> Result<EqPresetList, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    Ok(EqPresetList {
        builtin: BUILTIN_PRESETS.to_vec(),
        user: eq_presets::list(&conn)?,
    })
}

/// Save the running parametric bands and preamp as a new user preset, which
/// becomes the active one. Errors with an `eq_presets::ERR_*` code.
#[tauri::command]
pub async fn save_eq_user_preset(
    state: State<'_, AppState>,
    name: String,
) -> Result<EqualizerConfig, String> {
    let db = state.db.clone();
    crate::audio::with_audio(&state.audio, move |engine| {
        engine.with_equalizer(|eq| {
            if eq.mode != EqMode::Parametric {
                return Err("user presets are parametric-only".to_string());
            }
            let conn = db.pool.get().map_err(|e| e.to_string())?;
            let id = eq_presets::create(&conn, &name, eq.parametric_bands(), eq.preamp)?;
            eq.active_preset = Some(crate::equalizer::user_preset_key(id));
            save_eq_settings(&db, eq);
            Ok(EqualizerConfig::snapshot(eq))
        })
    })
    .await
}

#[tauri::command]
pub async fn rename_eq_user_preset(
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    eq_presets::rename(&conn, id, &name)
}

/// Delete a user preset. If it was active, the current bands become Custom.
#[tauri::command]
pub async fn delete_eq_user_preset(
    state: State<'_, AppState>,
    id: i64,
) -> Result<EqualizerConfig, String> {
    let db = state.db.clone();
    crate::audio::with_audio(&state.audio, move |engine| {
        engine.with_equalizer(|eq| {
            let conn = db.pool.get().map_err(|e| e.to_string())?;
            eq_presets::delete(&conn, id)?;
            if eq
                .active_preset
                .as_deref()
                .and_then(crate::equalizer::parse_user_preset_key)
                == Some(id)
            {
                eq.active_preset = None;
                save_eq_settings(&db, eq);
            }
            Ok(EqualizerConfig::snapshot(eq))
        })
    })
    .await
}

/// Evaluated magnitude response (dB, preamp excluded) of the parametric
/// cascade the engine is running, at each requested frequency — the curve
/// preview plots this instead of re-deriving the filter law (#1248). With
/// `band`, only that band's filter is evaluated (the selected-band curve).
#[tauri::command]
pub async fn get_parametric_response(
    state: State<'_, AppState>,
    frequencies: Vec<f32>,
    band: Option<usize>,
) -> Result<Vec<f32>, String> {
    Ok(crate::audio::with_audio(&state.audio, move |engine| {
        engine.with_equalizer(|eq| match band {
            Some(idx) => eq.band_response_db(idx, &frequencies),
            None => eq.parametric_response_db(&frequencies),
        })
    })
    .await)
}

#[derive(Clone, Debug, Serialize)]
pub struct EqPresetPreview {
    pub key: String,
    pub response_db: Vec<f32>,
}

/// Evaluated magnitude response curves for built-in and user presets across
/// the requested frequencies, computed offline without touching the live audio
/// engine (#1344). Previews reflect the given EQ `mode` (graphic or parametric).
#[tauri::command]
pub async fn get_eq_preset_previews(
    state: State<'_, AppState>,
    frequencies: Vec<f32>,
    mode: Option<EqMode>,
) -> Result<Vec<EqPresetPreview>, String> {
    let mode = mode.unwrap_or(EqMode::Graphic10);
    let sample_rate = crate::audio::with_audio(&state.audio, |engine| {
        engine.with_equalizer(|eq| eq.sample_rate())
    })
    .await;

    let mut offline_eq = Equalizer::new();
    offline_eq.update_format(sample_rate, 2);

    let mut previews = Vec::with_capacity(BUILTIN_PRESETS.len());

    for &name in &BUILTIN_PRESETS {
        let response = match mode {
            EqMode::Graphic10 => {
                offline_eq.load_preset(crate::equalizer::preset_gains(name));
                offline_eq.graphic_response_db(&frequencies)
            }
            EqMode::Parametric => {
                if let Some(bands) = crate::equalizer::parametric_preset(name) {
                    offline_eq.load_parametric(&bands);
                }
                offline_eq.parametric_response_db(&frequencies)
            }
        };
        previews.push(EqPresetPreview {
            key: name.to_string(),
            response_db: response,
        });
    }

    if mode == EqMode::Parametric {
        let conn = state.db.pool.get().map_err(|e| e.to_string())?;
        let user_presets = eq_presets::list_with_bands(&conn)?;
        for user_preset in user_presets {
            offline_eq.load_parametric(&user_preset.bands);
            let response = offline_eq.parametric_response_db(&frequencies);
            previews.push(EqPresetPreview {
                key: crate::equalizer::user_preset_key(user_preset.id),
                response_db: response,
            });
        }
    }

    Ok(previews)
}
