use crate::AppState;
use tauri::State;
use std::collections::HashMap;

#[tauri::command]
pub async fn set_app_setting(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_all_app_settings(state: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT key, value FROM app_state").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }).map_err(|e| e.to_string())?;

    let mut settings = HashMap::new();
    for row in rows {
        if let Ok((k, v)) = row {
            settings.insert(k, v);
        }
    }
    Ok(settings)
}
