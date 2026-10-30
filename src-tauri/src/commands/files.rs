//! The Files view (#1841): browsing folders, servers and devices as they are
//! laid out. See `crate::files` for what a listing holds.

use crate::collection::{CollectionScanner, ScanTrigger};
use crate::files::{self, DirListing, FileSource, SourceSummary};
use crate::models::Song;
use crate::AppState;
use tauri::{AppHandle, State};

/// Every watched folder and media server, with its library song count.
/// Devices come from `get_connected_devices`.
#[tauri::command]
pub async fn list_file_sources(state: State<'_, AppState>) -> Result<Vec<SourceSummary>, String> {
    let db = state.db.clone();
    crate::db::run_blocking(&state.db, move |conn| files::list_sources(conn, &db))
        .await
        .map_err(|e| e.to_string())
}

/// One folder of a source. Rejects with `source-not-found` when the source
/// was removed or the device disconnected, and with `server-unreachable`
/// when a WebDAV server can't be listed.
#[tauri::command]
pub async fn list_source_dir(
    source: FileSource,
    path: Vec<String>,
    state: State<'_, AppState>,
) -> Result<DirListing, String> {
    let devices = state.devices.clone();
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        files::list_dir(&db, &source, &path, |id| devices.device(id))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| {
        // `{e}` is the outermost message, the error code; `{e:#}` adds the
        // cause (e.g. the HTTP failure) for the log.
        log::warn!("Failed to list a source folder: {e:#}");
        e.to_string()
    })
}

/// The library songs in a folder of a source and below it, in layout
/// order, for Play folder.
#[tauri::command]
pub async fn get_source_songs(
    source: FileSource,
    path: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    crate::db::run_blocking(&state.db, move |conn| {
        files::songs_under(conn, &source, &path)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Looks for new and changed files in one folder of a folder source; the
/// usual `scan-progress` events follow. Other sources do nothing.
#[tauri::command]
pub async fn scan_source_folder(
    app: AppHandle,
    source: FileSource,
    path: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let folder = crate::db::run_blocking(&state.db, move |conn| {
        files::local_folder(conn, &source, &path)
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(folder) = folder else {
        return Ok(());
    };
    CollectionScanner::new(state.db.clone())
        .scoped_to(folder)
        .scan_all(app, false, false, ScanTrigger::Folder)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}
