use crate::AppState;
use std::collections::HashMap;
use tauri::State;

#[tauri::command]
pub async fn set_app_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_all_app_settings(
    state: State<'_, AppState>,
) -> Result<HashMap<String, String>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT key, value FROM app_state")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;

    let mut settings = HashMap::new();
    for (k, v) in rows.flatten() {
        settings.insert(k, v);
    }
    Ok(settings)
}

#[tauri::command]
pub fn get_commit_hash() -> String {
    option_env!("BUILD_COMMIT_HASH").unwrap_or("").to_string()
}

pub fn get_fade_settings_from_db(
    db: &crate::db::Database,
) -> Result<crate::models::FadeSettings, String> {
    let conn = db.pool.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT key, value FROM app_state WHERE key LIKE 'fade_%' OR key LIKE 'crossfade_%'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;

    let mut map = HashMap::new();
    for (k, v) in rows.flatten() {
        map.insert(k, v);
    }

    let defaults = crate::models::FadeSettings::default();
    Ok(crate::models::FadeSettings {
        fade_pause_enabled: map
            .get("fade_pause_enabled")
            .map(|v| v == "true")
            .unwrap_or(defaults.fade_pause_enabled),
        fade_pause_duration_ms: map
            .get("fade_pause_duration_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.fade_pause_duration_ms),
        crossfade_manual_enabled: map
            .get("crossfade_manual_enabled")
            .map(|v| v == "true")
            .unwrap_or(defaults.crossfade_manual_enabled),
        crossfade_manual_duration_ms: map
            .get("crossfade_manual_duration_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.crossfade_manual_duration_ms),
        crossfade_auto_enabled: map
            .get("crossfade_auto_enabled")
            .map(|v| v == "true")
            .unwrap_or(defaults.crossfade_auto_enabled),
        crossfade_auto_duration_secs: map
            .get("crossfade_auto_duration_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.crossfade_auto_duration_secs),
        crossfade_suppress_same_album: map
            .get("crossfade_suppress_same_album")
            .map(|v| v == "true")
            .unwrap_or(defaults.crossfade_suppress_same_album),
    })
}

#[tauri::command]
pub async fn get_fade_settings(
    state: State<'_, AppState>,
) -> Result<crate::models::FadeSettings, String> {
    get_fade_settings_from_db(&state.db)
}

#[tauri::command]
pub async fn set_fade_settings(
    state: State<'_, AppState>,
    settings: crate::models::FadeSettings,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let pairs = [
        (
            "fade_pause_enabled",
            settings.fade_pause_enabled.to_string(),
        ),
        (
            "fade_pause_duration_ms",
            settings.fade_pause_duration_ms.to_string(),
        ),
        (
            "crossfade_manual_enabled",
            settings.crossfade_manual_enabled.to_string(),
        ),
        (
            "crossfade_manual_duration_ms",
            settings.crossfade_manual_duration_ms.to_string(),
        ),
        (
            "crossfade_auto_enabled",
            settings.crossfade_auto_enabled.to_string(),
        ),
        (
            "crossfade_auto_duration_secs",
            settings.crossfade_auto_duration_secs.to_string(),
        ),
        (
            "crossfade_suppress_same_album",
            settings.crossfade_suppress_same_album.to_string(),
        ),
    ];

    for (k, v) in pairs {
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
            rusqlite::params![k, v],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}
