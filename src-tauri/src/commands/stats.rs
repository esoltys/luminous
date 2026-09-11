use crate::models::{ListenEvent, StatsSummary};
use crate::stats_summary::StatsRange;
use crate::AppState;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn get_stats_summary(
    range: String,
    state: State<'_, AppState>,
) -> Result<StatsSummary, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let range = StatsRange::parse(&range).ok_or_else(|| format!("invalid range: {range}"))?;
    crate::stats_summary::get_summary(&conn, range).map_err(|e| e.to_string())
}

/// Raw listen events for the past `days` days, for the daily listening
/// heatmap (#890) to bucket into local calendar days client-side.
#[tauri::command]
pub async fn get_listening_activity(
    days: i64,
    state: State<'_, AppState>,
) -> Result<Vec<ListenEvent>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let since_unix = chrono::Utc::now().timestamp() - days * 86_400;
    crate::stats_summary::listening_activity(&conn, since_unix).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_stats_exclusions(
    state: State<'_, AppState>,
) -> Result<Vec<(String, String)>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    crate::stats::get_stats_exclusions(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_stats_excluded(
    entity_type: String,
    entity_key: String,
    excluded: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    crate::stats::set_stats_excluded(&conn, &entity_type, &entity_key, excluded)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("stats-exclusions-changed", ());
    Ok(())
}

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
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                conn.query_row(
                    &sql,
                    rusqlite::params![song_id],
                    crate::collection::row_to_song,
                )
                .ok()
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
