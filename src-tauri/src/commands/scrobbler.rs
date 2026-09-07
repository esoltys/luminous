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
pub async fn flush_scrobble_cache(
    state: State<'_, AppState>,
) -> Result<u32, String> {
    state.scrobbler.flush_cache_now().await
}

#[tauri::command]
pub async fn sync_favourites_to_listenbrainz(
    state: State<'_, AppState>,
) -> Result<crate::scrobbler::SyncFavouritesResult, String> {
    state.scrobbler.sync_favourites().await
}
