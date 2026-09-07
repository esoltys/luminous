use crate::AppState;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn set_song_rating(
    song_id: i64,
    rating: f32,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<f32, String> {
    let (normalized, payload) = {
        let conn = state.db.pool.get().map_err(|e| e.to_string())?;
        let normalized =
            crate::stats::set_rating(&conn, song_id, rating).map_err(|e| e.to_string())?;
        (normalized, crate::stats::stats_payload(&conn, song_id))
    };

    // Keep the in-memory current song in sync so playback state snapshots
    // reflect the new rating immediately.
    let rated_song = {
        let mut player = state.player.lock().await;
        if let Some(song) = player.current_song.as_mut() {
            if song.id == song_id {
                song.rating = normalized;
                Some(song.clone())
            } else {
                None
            }
        } else {
            None
        }
    };

    let song_for_scrobbler = match rated_song {
        Some(s) => Some(s),
        None => {
            if let Ok(conn) = state.db.pool.get() {
                let sql = format!("SELECT {} FROM songs WHERE id = ?1", crate::collection::SONG_SELECT_COLS);
                conn.query_row(&sql, rusqlite::params![song_id], crate::collection::row_to_song).ok()
            } else {
                None
            }
        }
    };

    if let Some(song) = song_for_scrobbler {
        state.scrobbler.on_song_rating(&song, normalized).await;
    }

    let _ = app.emit("song-stats-changed", payload);

    Ok(normalized)
}

#[tauri::command]
pub async fn set_album_rating(
    album: String,
    rating: f32,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<f32, String> {
    let normalized = {
        let conn = state.db.pool.get().map_err(|e| e.to_string())?;
        crate::stats::set_album_rating(&conn, &album, rating).map_err(|e| e.to_string())?
    };

    let _ = app.emit(
        "album-stats-changed",
        serde_json::json!({ "album": album, "rating": normalized }),
    );

    Ok(normalized)
}
