use crate::musicbrainz::{MusicBrainzAuthState, MusicBrainzUserStats};
use crate::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn start_musicbrainz_login(
    prefer_loopback: Option<bool>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, String> {
    state
        .musicbrainz
        .start_login(app, prefer_loopback.unwrap_or(true))
        .await
}

#[tauri::command]
pub async fn submit_musicbrainz_auth_code(
    code: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MusicBrainzAuthState, String> {
    state.musicbrainz.submit_auth_code(&code, &app).await
}

#[tauri::command]
pub async fn cancel_musicbrainz_login(state: State<'_, AppState>) -> Result<(), String> {
    state.musicbrainz.cancel_login();
    Ok(())
}

#[tauri::command]
pub async fn get_musicbrainz_auth_state(
    state: State<'_, AppState>,
) -> Result<MusicBrainzAuthState, String> {
    Ok(state.musicbrainz.get_auth_state().await)
}

#[tauri::command]
pub async fn get_musicbrainz_user_stats(
    force_refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<MusicBrainzUserStats, String> {
    state
        .musicbrainz
        .get_user_stats(force_refresh.unwrap_or(false))
        .await
}

#[tauri::command]
pub async fn logout_musicbrainz(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    state.musicbrainz.logout(&app).await
}

#[tauri::command]
pub async fn get_musicbrainz_app_credentials(
    state: State<'_, AppState>,
) -> Result<(String, bool), String> {
    Ok(state.musicbrainz.get_app_credentials())
}

#[tauri::command]
pub async fn set_musicbrainz_app_credentials(
    client_id: String,
    client_secret: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .musicbrainz
        .set_app_credentials(client_id, client_secret)
}
