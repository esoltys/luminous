use crate::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_waveform_data(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<Option<Vec<u8>>, String> {
    let db = &state.db;
    let query_result = crate::db::run_blocking(db, move |conn| {
        let row: Option<(Vec<u8>, i64)> = conn
            .query_row(
                "SELECT data, style FROM waveforms WHERE song_id = ?1",
                rusqlite::params![song_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        if let Some((data, style)) = row {
            if style == crate::waveform::WAVEFORM_STYLE_VERSION {
                return Ok((Some(data), None));
            }
        }

        let path: Option<String> = conn
            .query_row(
                "SELECT path FROM songs WHERE id = ?1",
                rusqlite::params![song_id],
                |r| r.get(0),
            )
            .ok();

        Ok((None, path))
    })
    .await
    .map_err(|e| e.to_string())?;

    let (cached, path_str) = query_result;
    if let Some(data) = cached {
        return Ok(Some(data));
    }

    let path = match path_str {
        Some(p) => std::path::PathBuf::from(p),
        None => return Ok(None),
    };

    // Generate waveform in a blocking threadpool task
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
pub async fn get_band_waveform_data(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<Option<Vec<u8>>, String> {
    let db = &state.db;
    let query_result = crate::db::run_blocking(db, move |conn| {
        let row: Option<(Vec<u8>, i64)> = conn
            .query_row(
                "SELECT data, style FROM band_waveforms WHERE song_id = ?1",
                rusqlite::params![song_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        if let Some((data, style)) = row {
            if style == crate::band_waveform::BAND_WAVEFORM_STYLE_VERSION {
                return Ok((Some(data), None));
            }
        }

        let path: Option<String> = conn
            .query_row(
                "SELECT path FROM songs WHERE id = ?1",
                rusqlite::params![song_id],
                |r| r.get(0),
            )
            .ok();

        Ok((None, path))
    })
    .await
    .map_err(|e| e.to_string())?;

    let (cached, path_str) = query_result;
    if let Some(data) = cached {
        return Ok(Some(data));
    }

    let path = match path_str {
        Some(p) => std::path::PathBuf::from(p),
        None => return Ok(None),
    };

    // Generate band waveform in a blocking threadpool task
    let db_clone = Arc::clone(&state.db);
    let band_waveform_data = tauri::async_runtime::spawn_blocking(move || {
        crate::band_waveform::generate_band_waveform(&db_clone, song_id, &path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    Ok(Some(band_waveform_data))
}

#[tauri::command]
pub async fn set_spectrum_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let engine = state.audio.lock().await;
    engine.set_spectrum_enabled(enabled);
    Ok(())
}
