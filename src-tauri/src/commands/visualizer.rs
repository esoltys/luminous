use crate::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

/// Queries the audio file path for a song, returning `None` if the song does not exist
/// or is from a remote source (WebDAV, Subsonic).
///
/// For remote sources, generating visualizer data on a cache miss requires streaming
/// and decoding the entire multi-megabyte audio file over HTTP. This saturates the remote
/// server connection and starves the audio playback stream, causing multi-minute buffering
/// delays (#1785). Returning `None` allows the UI to render a clean standard seek bar without
/// stalling playback.
pub(crate) fn query_local_song_path(conn: &rusqlite::Connection, song_id: i64) -> Option<PathBuf> {
    let song_info: Option<(String, i64)> = conn
        .query_row(
            "SELECT path, source FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();

    let (path_str, source_raw) = song_info?;
    if crate::models::SongSource::from(source_raw).is_remote() {
        return None;
    }
    Some(PathBuf::from(path_str))
}

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

        let local_path = query_local_song_path(conn, song_id);
        Ok((None, local_path))
    })
    .await
    .map_err(|e| e.to_string())?;

    let (cached, local_path) = query_result;
    if let Some(data) = cached {
        return Ok(Some(data));
    }

    let Some(path) = local_path else {
        return Ok(None);
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

        let local_path = query_local_song_path(conn, song_id);
        Ok((None, local_path))
    })
    .await
    .map_err(|e| e.to_string())?;

    let (cached, local_path) = query_result;
    if let Some(data) = cached {
        return Ok(Some(data));
    }

    let Some(path) = local_path else {
        return Ok(None);
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
    fn test_query_local_song_path_skips_remote_sources() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO songs (id, path, source) VALUES (1, 'https://nas/track.flac', 11)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO songs (id, path, source) VALUES (2, '/music/local.flac', 1)",
            [],
        )
        .unwrap();

        assert_eq!(super::query_local_song_path(&conn, 1), None);
        assert_eq!(
            super::query_local_song_path(&conn, 2),
            Some(std::path::PathBuf::from("/music/local.flac"))
        );
        assert_eq!(super::query_local_song_path(&conn, 999), None);
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
