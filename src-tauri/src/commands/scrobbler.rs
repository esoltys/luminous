use crate::scrobbler::{ScrobbleCacheStatus, ScrobblerSettings};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_scrobbler_settings(
    state: State<'_, AppState>,
) -> Result<ScrobblerSettings, String> {
    Ok(state.scrobbler.get_settings().await)
}

use tauri::Emitter;

#[tauri::command]
pub async fn set_scrobbler_settings(
    settings: ScrobblerSettings,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state
        .scrobbler
        .save_settings(settings.clone())
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("scrobbler-settings-changed", &settings);
    Ok(())
}

#[tauri::command]
pub async fn toggle_scrobble_pause(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let paused = state.scrobbler.toggle_paused().await;
    let settings = state.scrobbler.get_settings().await;
    let _ = app.emit("scrobbler-settings-changed", &settings);
    Ok(paused)
}

#[tauri::command]
pub async fn validate_listenbrainz_token(
    token: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state.scrobbler.validate_token(&token).await
}

#[tauri::command]
pub async fn get_scrobble_cache_status(
    state: State<'_, AppState>,
) -> Result<ScrobbleCacheStatus, String> {
    Ok(state.scrobbler.get_cache_status())
}

#[tauri::command]
pub async fn flush_scrobble_cache(state: State<'_, AppState>) -> Result<u32, String> {
    state.scrobbler.flush_cache_now().await
}

#[tauri::command]
pub async fn sync_ratings_to_listenbrainz(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::scrobbler::SyncRatingsResult, String> {
    let result = state.scrobbler.sync_ratings().await?;

    // Patch the in-memory current song, then announce each change; the
    // `song-stats-changed` listener coalesces the burst into one dynamic
    // playlist reconcile, so Favourites reflects the pull immediately.
    for &song_id in &result.changed_song_ids {
        let payload = crate::db::run_blocking(&state.db, move |conn| {
            Ok(crate::stats::stats_payload(conn, song_id))
        })
        .await
        .map_err(|e| e.to_string())?;
        {
            let mut player = state.player.lock().await;
            if let Some(song) = player.current_song.as_mut().filter(|s| s.id == song_id) {
                if let Some(loved) = payload["loved"].as_i64() {
                    song.loved = loved as i32;
                }
                if let Some(rating) = payload["rating"].as_f64() {
                    song.rating = rating as f32;
                }
            }
        }
        let _ = app.emit("song-stats-changed", payload);
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_discord_status(
    state: State<'_, AppState>,
) -> Result<crate::discord::DiscordStatus, String> {
    Ok(state.scrobbler.get_discord_status().await)
}
