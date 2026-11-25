//! Tauri IPC commands for the local Radio Browser station directory mirror (#2003).

use crate::models::Playlist;
use crate::playlist::RadioStationListSpec;
use crate::radio_directory::{
    self, RadioDirectoryFacets, RadioDirectoryQuery, RadioDirectoryStation, RadioDirectoryStatus,
    RadioDirectorySyncStats, RADIO_BROWSER_SCHEDULER_ID, RADIO_BROWSER_SYNC_INTERVAL_MINUTES,
    RADIO_BROWSER_USER_AGENT,
};
use crate::remote_scheduler::{RemoteKind, SyncGuard};
use crate::AppState;
use luminous_remote::radiobrowser::{RadioBrowserClient, RadioBrowserFacetEntry};
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

/// Global-search station lookup (#2007): ranked, playable-only and capped at
/// [`radio_directory::MAX_STATION_SEARCH_RESULTS`]. Local mirror only, so it works
/// offline; returns an empty list when the directory isn't opted in.
#[tauri::command]
pub async fn search_stations(
    query: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<RadioDirectoryStation>, String> {
    let limit = limit.unwrap_or(radio_directory::MAX_STATION_SEARCH_RESULTS);
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::search_stations(conn, &query, limit)
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
    query: Option<RadioDirectoryQuery>,
    state: State<'_, AppState>,
) -> Result<RadioDirectoryFacets, String> {
    let hide_unsupported = hide_unsupported.unwrap_or(true);
    let tag_limit = tag_limit.unwrap_or(500);
    crate::db::run_blocking(&state.db, move |conn| match query {
        Some(q) => radio_directory::compute_filtered_facets(conn, &q, tag_limit),
        None => radio_directory::compute_local_facets(conn, hide_unsupported, tag_limit),
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

/// Saves a radio station to `radio_saved` (idempotent) (#2005).
#[tauri::command]
pub async fn save_radio_station(
    stationuuid: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::save_station(conn, &stationuuid)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Removes a radio station from `radio_saved` (idempotent) (#2005).
#[tauri::command]
pub async fn unsave_radio_station(
    stationuuid: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::unsave_station(conn, &stationuuid)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Returns all user-saved radio stations ordered by custom position (#2005, #2209).
#[tauri::command]
pub async fn get_saved_radio_stations(
    hide_unsupported: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<RadioDirectoryStation>, String> {
    let hide_unsupported = hide_unsupported.unwrap_or(true);
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::list_saved_stations(conn, hide_unsupported)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Persists a custom display order for saved radio stations (#2209).
#[tauri::command]
pub async fn reorder_saved_stations(
    order: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::reorder_saved_stations(conn, &order)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Returns the radio stations the user actually played, most recent first (#2005, #2279).
#[tauri::command]
pub async fn get_recent_radio_stations(
    limit: Option<usize>,
    hide_unsupported: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<RadioDirectoryStation>, String> {
    let limit = limit.unwrap_or(20);
    let hide_unsupported = hide_unsupported.unwrap_or(true);
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::list_recent_stations(conn, limit, hide_unsupported)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Returns IDs of custom playlists containing at least one radio station (#2005).
#[tauri::command]
pub async fn get_radio_playlist_ids(state: State<'_, AppState>) -> Result<Vec<i64>, String> {
    crate::db::run_blocking(&state.db, radio_directory::list_radio_playlist_ids)
        .await
        .map_err(|e| e.to_string())
}

/// Materializes `stationuuid` as a `songs` row, replaces the Queue and starts
/// playback, and dispatches click reporting asynchronously when online (#2005).
#[tauri::command]
pub async fn play_radio_station(
    stationuuid: String,
    state: State<'_, AppState>,
) -> Result<crate::models::Song, String> {
    let uuid_for_db = stationuuid.clone();
    let song = crate::db::run_blocking(&state.db, move |conn| {
        let song = crate::radio_directory::materialize_station(conn, &uuid_for_db)?;
        let _ = crate::radio_directory::gc_unreferenced_stations(
            conn,
            crate::radio_directory::DEFAULT_RECENT_STATION_TAIL,
            Some(song.id),
        );
        Ok(song)
    })
    .await
    .map_err(|e| e.to_string())?;

    crate::commands::player::replace_queue_and_play(
        &state,
        &[song.id],
        0,
        Some(crate::models::PlayContext::Song),
    )
    .await?;

    if song.source == crate::models::SongSource::RadioBrowser {
        let db = Arc::clone(&state.db);
        tauri::async_runtime::spawn(async move {
            let _ = report_station_click_if_online(&db, stationuuid).await;
        });
    }

    Ok(song)
}

/// Canonicalized radio directory tag counts plus library-backed genre suggestions (#2006).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadioStationListTags {
    pub tags: Vec<RadioBrowserFacetEntry>,
    pub suggested_from_library: Vec<RadioBrowserFacetEntry>,
}

/// Returns canonicalized radio directory tag counts (via `GenreAliases::canonical_genres`)
/// and suggested genres from the user's most-played library tracks (#2006).
#[tauri::command]
pub async fn get_radio_station_list_tags(
    hide_unsupported: Option<bool>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<RadioStationListTags, String> {
    let hide_unsupported = hide_unsupported.unwrap_or(true);
    let limit = limit.unwrap_or(200);
    crate::db::run_blocking(&state.db, move |conn| {
        let aliases = crate::genre_aliases::GenreAliases::load(conn)?;
        let tags =
            radio_directory::get_canonical_radio_tags(conn, &aliases, hide_unsupported, limit)?;
        let suggested_from_library = radio_directory::suggest_radio_genres_from_library(
            conn,
            &aliases,
            hide_unsupported,
            12,
        )?;
        Ok(RadioStationListTags {
            tags,
            suggested_from_library,
        })
    })
    .await
    .map_err(|e| e.to_string())
}

fn apply_radio_spec_and_fetch(
    pm: &mut crate::playlist::PlaylistManager,
    playlist_id: i64,
    spec_str: &str,
) -> anyhow::Result<Playlist> {
    pm.set_playlist_dynamic_spec(playlist_id, spec_str)?;
    pm.get_playlists()?
        .into_iter()
        .find(|p| p.id == playlist_id)
        .ok_or_else(|| anyhow::anyhow!("Station list {playlist_id} not found"))
}

/// Creates a new dynamic radio station list and populates it from the directory (#2006).
#[tauri::command]
pub async fn create_radio_station_list(
    name: String,
    spec: RadioStationListSpec,
    state: State<'_, AppState>,
) -> Result<Playlist, String> {
    let spec_str = spec.to_spec_string();
    crate::playlist::with_playlists(&state.playlists, move |pm| {
        let created = pm.create_playlist(&name)?;
        apply_radio_spec_and_fetch(pm, created.id, &spec_str)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Updates an existing dynamic radio station list's name and filters, and
/// repopulates it immediately (#2006).
#[tauri::command]
pub async fn update_radio_station_list(
    playlist_id: i64,
    name: String,
    spec: RadioStationListSpec,
    state: State<'_, AppState>,
) -> Result<Playlist, String> {
    let spec_str = spec.to_spec_string();
    crate::playlist::with_playlists(&state.playlists, move |pm| {
        pm.rename_playlist(playlist_id, &name)?;
        apply_radio_spec_and_fetch(pm, playlist_id, &spec_str)
    })
    .await
    .map_err(|e| e.to_string())
}

async fn resolve_station_inputs(
    name: String,
    url: String,
    tags: Option<String>,
) -> Result<(String, String, String), String> {
    let resolved = radio_directory::resolve_station_url(&url)
        .await
        .map_err(|e| e.to_string())?;
    let final_name = if name.trim().is_empty() {
        resolved
            .title
            .unwrap_or_else(|| resolved.stream_url.clone())
    } else {
        name
    };
    Ok((final_name, resolved.stream_url, tags.unwrap_or_default()))
}

/// Adds a user-defined custom station by stream URL or `.pls`/`.m3u`/`.m3u8`
/// playlist URL, saving it outside the `radio_directory` mirror (#2009).
#[tauri::command]
pub async fn add_custom_radio_station(
    name: String,
    url: String,
    tags: Option<String>,
    state: State<'_, AppState>,
) -> Result<RadioDirectoryStation, String> {
    let (final_name, stream_url, tags) = resolve_station_inputs(name, url, tags).await?;
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::add_custom_station(conn, &final_name, &stream_url, &tags)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Updates an existing custom radio station's name, URL (resolving `.pls`/`.m3u`/`.m3u8`
/// playlists if needed), and tags (#2009).
#[tauri::command]
pub async fn update_custom_radio_station(
    stationuuid: String,
    name: String,
    url: String,
    tags: Option<String>,
    state: State<'_, AppState>,
) -> Result<RadioDirectoryStation, String> {
    let (final_name, stream_url, tags) = resolve_station_inputs(name, url, tags).await?;
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::update_custom_station(conn, &stationuuid, &final_name, &stream_url, &tags)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Imports all `http(s)://` stream entries from a local `.pls`/`.m3u`/`.m3u8`/`.xspf`
/// file as saved custom radio stations (#2009).
#[tauri::command]
pub async fn import_custom_radio_stations(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<RadioDirectoryStation>, String> {
    let path = std::path::PathBuf::from(&file_path);
    let parsed = luminous_metadata::playlist::parse_playlist(&path).map_err(|e| e.to_string())?;
    let fallback_name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Custom Station".to_string());

    crate::db::run_blocking(&state.db, move |conn| {
        let mut added = Vec::new();
        for track in parsed.tracks {
            if !luminous_metadata::playlist::is_http_url(&track.path_or_url) {
                continue;
            }
            let title = track
                .title
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(&fallback_name);
            let station = radio_directory::add_custom_station(conn, title, &track.path_or_url, "")?;
            added.push(station);
        }
        if added.is_empty() {
            anyhow::bail!("No http(s) stream URLs found in playlist file");
        }
        Ok(added)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Exports all saved stations (`radio_saved`) to an M3U, M3U8, or PLS file (#2009).
#[tauri::command]
pub async fn export_saved_radio_stations(
    export_path: String,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    crate::db::run_blocking(&state.db, move |conn| {
        radio_directory::export_saved_stations(conn, &export_path)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Probes a radio station stream or playlist URL and returns connection status
/// and stream headers without downloading the stream body (#2393).
#[tauri::command]
pub async fn test_radio_station_url(
    url: String,
) -> Result<radio_directory::RadioConnectionTestResult, String> {
    Ok(radio_directory::test_station_url(&url).await)
}
