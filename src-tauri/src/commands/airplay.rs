//! `get_airplay_now`: the right panel's "On the radio now" section (#1998).
//! See `crate::airplay` for the workflow, cache and quota protection.

use crate::airplay::AirplayNow;
use crate::AppState;
use tauri::State;

/// Which internet radio stations are playing `song_id` right now. Never fails:
/// opt-in off, offline, unsupported songs and TunePing outages are all reported
/// in the result's `status`. `force_refresh` skips the 60-second cache.
#[tauri::command]
pub async fn get_airplay_now(
    song_id: i64,
    force_refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<AirplayNow, String> {
    Ok(crate::airplay::get_airplay_now(&state.db, song_id, force_refresh.unwrap_or(false)).await)
}
