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
    {
        let mut player = state.player.lock().await;
        if let Some(song) = player.current_song.as_mut() {
            if song.id == song_id {
                song.rating = normalized;
            }
        }
    }

    let _ = app.emit("song-stats-changed", payload);

    Ok(normalized)
}
