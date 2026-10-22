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

        let song_info: Option<(String, i64)> = conn
            .query_row(
                "SELECT path, source FROM songs WHERE id = ?1",
                rusqlite::params![song_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        Ok((None, song_info))
    })
    .await
    .map_err(|e| e.to_string())?;

    let (cached, song_info) = query_result;
    if let Some(data) = cached {
        return Ok(Some(data));
    }

    let Some((path_str, source_raw)) = song_info else {
        return Ok(None);
    };

    // For remote sources (WebDAV, Subsonic), generating visualizer data on a cache
    // miss requires streaming and decoding the entire multi-megabyte audio file over
    // HTTP. This saturates the remote server connection and starves the audio playback
    // stream, causing multi-minute buffering delays (#1785). Return None so the UI
    // falls back to a clean standard seek bar without stalling playback.
    if crate::models::SongSource::from(source_raw).is_remote() {
        return Ok(None);
    }

    let path = std::path::PathBuf::from(path_str);

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

        let song_info: Option<(String, i64)> = conn
            .query_row(
                "SELECT path, source FROM songs WHERE id = ?1",
                rusqlite::params![song_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        Ok((None, song_info))
    })
    .await
    .map_err(|e| e.to_string())?;

    let (cached, song_info) = query_result;
    if let Some(data) = cached {
        return Ok(Some(data));
    }

    let Some((path_str, source_raw)) = song_info else {
        return Ok(None);
    };

    // For remote sources (WebDAV, Subsonic), generating visualizer data on a cache
    // miss requires streaming and decoding the entire multi-megabyte audio file over
    // HTTP. This saturates the remote server connection and starves the audio playback
    // stream, causing multi-minute buffering delays (#1785). Return None so the UI
    // falls back to a clean standard seek bar without stalling playback.
    if crate::models::SongSource::from(source_raw).is_remote() {
        return Ok(None);
    }

    let path = std::path::PathBuf::from(path_str);

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

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE songs (id INTEGER PRIMARY KEY, path TEXT, source INTEGER DEFAULT 1);
             CREATE TABLE waveforms (song_id INTEGER PRIMARY KEY, data BLOB, style INTEGER);
             CREATE TABLE band_waveforms (song_id INTEGER PRIMARY KEY, data BLOB, style INTEGER);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_remote_song_without_cached_waveform_is_detected() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO songs (id, path, source) VALUES (1, 'https://nas/track.flac', 11)",
            [],
        )
        .unwrap();

        let song_info: Option<(String, i64)> = conn
            .query_row(
                "SELECT path, source FROM songs WHERE id = ?1",
                rusqlite::params![1],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        let (_, source_raw) = song_info.unwrap();
        assert!(crate::models::SongSource::from(source_raw).is_remote());
    }

    #[test]
    fn test_remote_song_with_cached_waveform_serves_cache() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO songs (id, path, source) VALUES (1, 'https://nas/track.flac', 11)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO waveforms (song_id, data, style) VALUES (1, X'010203', ?1)",
            rusqlite::params![crate::waveform::WAVEFORM_STYLE_VERSION],
        )
        .unwrap();

        let row: Option<(Vec<u8>, i64)> = conn
            .query_row(
                "SELECT data, style FROM waveforms WHERE song_id = ?1",
                rusqlite::params![1],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        let (data, style) = row.unwrap();
        assert_eq!(style, crate::waveform::WAVEFORM_STYLE_VERSION);
        assert_eq!(data, vec![1, 2, 3]);
    }
}
