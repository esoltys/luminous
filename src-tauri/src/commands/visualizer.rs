use crate::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_waveform_data(state: State<'_, AppState>, song_id: i64) -> Result<Option<Vec<u8>>, String> {
    // 1. Check cache in SQLite
    if let Ok(Some(cached)) = crate::waveform::get_cached_waveform(&state.db, song_id) {
        return Ok(Some(cached));
    }

    // 2. Fetch song path from SQLite
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let path_str: Option<String> = conn
        .query_row("SELECT path FROM songs WHERE id = ?1", rusqlite::params![song_id], |row| row.get(0))
        .ok();

    let path = match path_str {
        Some(p) => std::path::PathBuf::from(p),
        None => return Ok(None),
    };

    // 3. Generate waveform in a blocking threadpool task (so we don't block tokio)
    let db_clone = Arc::clone(&state.db);
    let waveform_data = tauri::async_runtime::spawn_blocking(move || {
        crate::waveform::generate_waveform(&db_clone, song_id, &path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    Ok(Some(waveform_data))
}

#[tauri::command]
pub async fn get_moodbar_data(state: State<'_, AppState>, song_id: i64) -> Result<Option<Vec<u8>>, String> {
    // 1. Check cache in SQLite
    if let Ok(Some(cached)) = crate::moodbar::get_cached_moodbar(&state.db, song_id) {
        return Ok(Some(cached));
    }

    // 2. Fetch song path from SQLite
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let path_str: Option<String> = conn
        .query_row("SELECT path FROM songs WHERE id = ?1", rusqlite::params![song_id], |row| row.get(0))
        .ok();

    let path = match path_str {
        Some(p) => std::path::PathBuf::from(p),
        None => return Ok(None),
    };

    // 3. Generate moodbar in a blocking threadpool task
    let db_clone = Arc::clone(&state.db);
    let moodbar_data = tauri::async_runtime::spawn_blocking(move || {
        crate::moodbar::generate_moodbar(&db_clone, song_id, &path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    Ok(Some(moodbar_data))
}

#[tauri::command]
pub async fn set_spectrum_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let engine = state.audio.lock().await;
    engine.spectrum_enabled.store(enabled, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}
