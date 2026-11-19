//! Tauri IPC commands for the local Radio Browser station directory mirror (#2003).

use crate::radio_directory::{
    self, RadioDirectoryFacets, RadioDirectoryQuery, RadioDirectoryStation, RadioDirectoryStatus,
    RadioDirectorySyncStats, RADIO_BROWSER_SCHEDULER_ID, RADIO_BROWSER_SYNC_INTERVAL_MINUTES,
    RADIO_BROWSER_USER_AGENT,
};
use crate::remote_scheduler::{RemoteKind, SyncGuard};
use crate::AppState;
use luminous_remote::radiobrowser::RadioBrowserClient;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Returns the current opt-in state, station counts, and sync timestamps of the local
/// Radio Browser station directory mirror.
#[tauri::command]
pub async fn get_radio_directory_status(
    state: State<'_, AppState>,
) -> Result<RadioDirectoryStatus, String> {
    let next_auto_sync_at = state
        .remote_auto_sync
        .next_run_at(RemoteKind::RadioBrowser, RADIO_BROWSER_SCHEDULER_ID);
    let sync_in_progress =
        SyncGuard::is_active(RemoteKind::RadioBrowser, RADIO_BROWSER_SCHEDULER_ID);

    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::get_status(conn, next_auto_sync_at, sync_in_progress)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Opts in to the Radio Browser directory (if not already opted in), runs an
/// incremental or full sync, schedules daily background refreshes, and emits
/// `radio-directory-sync-progress` and `radio-directory-changed` events.
#[tauri::command]
pub async fn sync_radio_directory(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RadioDirectorySyncStats, String> {
    let stats =
        radio_directory::sync_radio_directory_inner(app.clone(), Arc::clone(&state.db)).await?;

    state.remote_auto_sync.reschedule(
        app,
        Arc::clone(&state.db),
        Arc::clone(&state.cover_manager),
        RemoteKind::RadioBrowser,
        RADIO_BROWSER_SCHEDULER_ID,
        RADIO_BROWSER_SYNC_INTERVAL_MINUTES,
    );

    Ok(stats)
}

/// Queries stations from the local `radio_directory` mirror (full-text search + filters).
#[tauri::command]
pub async fn query_radio_directory(
    query: Option<RadioDirectoryQuery>,
    state: State<'_, AppState>,
) -> Result<Vec<RadioDirectoryStation>, String> {
    let query = query.unwrap_or_default();
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::query_stations(conn, &query)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Looks up a single station in the local `radio_directory` mirror by UUID.
#[tauri::command]
pub async fn get_radio_directory_station(
    stationuuid: String,
    state: State<'_, AppState>,
) -> Result<Option<RadioDirectoryStation>, String> {
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::get_station_by_uuid(conn, &stationuuid)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Returns locally aggregated tag, country, language, and codec counts from `radio_directory`.
#[tauri::command]
pub async fn get_radio_directory_facets(
    hide_unsupported: Option<bool>,
    tag_limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<RadioDirectoryFacets, String> {
    let hide_unsupported = hide_unsupported.unwrap_or(true);
    let tag_limit = tag_limit.unwrap_or(500);
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::compute_local_facets(conn, hide_unsupported, tag_limit)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Materializes a directory station as a `songs` row (`source = RadioBrowser`,
/// `path = radio://<stationuuid>`) so Queue, playlists, and pins treat it as a
/// normal playable item (#2004).
#[tauri::command]
pub async fn materialize_radio_station(
    stationuuid: String,
    state: State<'_, AppState>,
) -> Result<crate::models::Song, String> {
    crate::db::run_blocking(&state.db, move |conn| {
        let song = crate::radio_directory::materialize_station(conn, &stationuuid)?;
        let _ = crate::radio_directory::gc_unreferenced_stations(
            conn,
            crate::radio_directory::DEFAULT_RECENT_STATION_TAIL,
            Some(song.id),
        );
        Ok(song)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Sends a per-play click report (`/json/url/{stationuuid}`) to Radio Browser when online
/// lookups are enabled, returning the resolved stream URL.
pub async fn report_station_click_if_online(
    db: &std::sync::Arc<crate::db::Database>,
    stationuuid: String,
) -> Result<Option<String>, String> {
    let online = crate::db::run_blocking(db, |conn| {
        Ok(crate::commands::context::is_online_enabled(conn))
    })
    .await
    .map_err(|e| e.to_string())?;

    if !online {
        return Ok(None);
    }

    let client = RadioBrowserClient::new(RADIO_BROWSER_USER_AGENT);
    let url = client.report_station_click(&stationuuid).await?;
    Ok(Some(url))
}

/// Sends a per-play click report (`/json/url/{stationuuid}`) to Radio Browser when online
/// lookups are enabled, returning the resolved stream URL.
#[tauri::command]
pub async fn report_radio_station_click(
    stationuuid: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    report_station_click_if_online(&state.db, stationuuid).await
}
