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
        let other_mode = match eq.mode {
            EqMode::Graphic10 => EqMode::Parametric,
            EqMode::Parametric => EqMode::Graphic10,
        };
        let (inactive_preamp, inactive_preset) = eq.mode_state(other_mode);
        let _ = conn.execute(
            "UPDATE equalizer_settings
             SET enabled = ?1, preamp = ?2, gains = ?3, mode = ?4, parametric = ?5,
                 active_preset = ?6, inactive_preamp = ?7, inactive_preset = ?8
             WHERE id = 1",
            rusqlite::params![
                if eq.enabled { 1 } else { 0 },
                eq.preamp as f64,
                gains_str,
                mode_str,
                parametric_json,
                eq.active_preset.as_deref().unwrap_or(""),
                inactive_preamp as f64,
                inactive_preset.unwrap_or("")
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
            eq.reset_parametric();
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

/// Import an Equalizer APO / AutoEq parametric profile (#1336) as a new user
/// preset named `name`, which becomes active with the EQ switched on.
///
/// All-or-nothing: the text is parsed and the preset saved before the engine
/// is touched, so a bad file or a taken name changes nothing. Errors are a
/// JSON `{ "code": ... }` string — an `eq_import::ImportError`, or an
/// `eq_presets::ERR_*` code.
pub fn import_profile_into(
    db: &crate::db::Database,
    eq: &mut Equalizer,
    text: &str,
    name: &str,
) -> Result<EqualizerConfig, String> {
    let profile = crate::eq_import::parse_parametric_profile(text).map_err(|e| e.to_json())?;
    let preset_err = |code: String| serde_json::json!({ "code": code }).to_string();
    let conn = db.pool.get().map_err(|e| preset_err(e.to_string()))?;
    let id = eq_presets::create(&conn, name, &profile.bands, profile.preamp).map_err(preset_err)?;
    eq.load_user_preset(id, &profile.bands, profile.preamp);
    eq.enabled = true;
    save_eq_settings(db, eq);
    Ok(EqualizerConfig::snapshot(eq))
}

#[tauri::command]
pub async fn import_parametric_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    name: String,
) -> Result<EqualizerConfig, String> {
    let db = state.db.clone();
    let canonical = crate::audio::with_audio(&state.audio, move |engine| {
        engine.with_equalizer(|eq| import_profile_into(&db, eq, &text, &name))
    })
    .await?;
    emit_pipeline_changed(&app, &state).await;
    Ok(canonical)
}

/// Read a profile file the user picked, for the import panel to show and
/// then pass to `import_parametric_profile`. Read here rather than by the
/// webview because the picked file can be anywhere on disk. Errors are the
/// same JSON `{ "code": ... }` string.
#[tauri::command]
pub async fn read_eq_profile_file(path: String) -> Result<String, String> {
    crate::eq_import::read_profile_file(std::path::Path::new(&path)).map_err(|e| e.to_json())
}

/// Write the parametric bands and the parametric preamp to `path` as an
/// Equalizer APO profile, whichever mode is active.
#[tauri::command]
pub async fn export_parametric_profile(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let text = crate::audio::with_audio(&state.audio, |engine| {
        engine.with_equalizer(|eq| {
            let (preamp, _) = eq.mode_state(EqMode::Parametric);
            Ok::<_, String>(crate::eq_import::format_parametric_profile(
                preamp,
                eq.parametric_bands(),
            ))
        })
    })
    .await?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    const PROFILE: &str = "Preamp: -6.3 dB
                           Filter 1: ON LSC Fc 105 Hz Gain 6.5 dB Q 0.70
                           Filter 2: OFF PK Fc 125 Hz Gain -2.7 dB Q 0.55
";

    fn setup() -> (tempfile::TempDir, Database, Equalizer) {
        let dir = tempfile::Builder::new()
            .prefix("luminous_eq_import_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let mut eq = Equalizer::new();
        eq.update_format(48_000, 2);
        // A non-flat graphic starting point, so any partial apply would show.
        eq.load_preset([3.0; 10]);
        eq.set_preamp(-1.0);
        (dir, db, eq)
    }

    fn code(err: &str) -> String {
        let v: serde_json::Value = serde_json::from_str(err).unwrap();
        v["code"].as_str().unwrap().to_string()
    }

    #[test]
    fn import_saves_exact_profile_as_the_active_user_preset() {
        let (_dir, db, mut eq) = setup();
        let echo = import_profile_into(&db, &mut eq, PROFILE, " HD 600 ").unwrap();

        let conn = db.pool.get().unwrap();
        let presets = eq_presets::list(&conn).unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "HD 600");
        let stored = eq_presets::get(&conn, presets[0].id).unwrap();
        let parsed = crate::eq_import::parse_parametric_profile(PROFILE).unwrap();
        assert_eq!(stored.bands, parsed.bands);
        assert_eq!(stored.preamp, -6.3);

        assert!(echo.enabled);
        assert_eq!(echo.mode, EqMode::Parametric);
        assert_eq!(echo.parametric, parsed.bands);
        assert_eq!(echo.preamp, -6.3);
        assert_eq!(
            echo.active_preset,
            Some(crate::equalizer::user_preset_key(presets[0].id))
        );
    }

    #[test]
    fn unparseable_profile_saves_nothing_and_leaves_eq_alone() {
        let (_dir, db, mut eq) = setup();
        let before = EqualizerConfig::snapshot(&eq);
        let err = import_profile_into(&db, &mut eq, "Filter 1: ON LP Fc 100 Hz", "X").unwrap_err();
        assert_eq!(code(&err), "unsupported_filters");
        assert_eq!(EqualizerConfig::snapshot(&eq), before);
        assert!(eq_presets::list(&db.pool.get().unwrap())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn taken_or_empty_name_saves_nothing_and_leaves_eq_alone() {
        let (_dir, db, mut eq) = setup();
        eq_presets::create(&db.pool.get().unwrap(), "HD 600", &[], 0.0).unwrap();
        let before = EqualizerConfig::snapshot(&eq);

        let err = import_profile_into(&db, &mut eq, PROFILE, "hd 600").unwrap_err();
        assert_eq!(code(&err), eq_presets::ERR_DUPLICATE_NAME);
        let err = import_profile_into(&db, &mut eq, PROFILE, "  ").unwrap_err();
        assert_eq!(code(&err), eq_presets::ERR_EMPTY_NAME);

        assert_eq!(EqualizerConfig::snapshot(&eq), before);
        assert_eq!(eq_presets::list(&db.pool.get().unwrap()).unwrap().len(), 1);
    }
}
