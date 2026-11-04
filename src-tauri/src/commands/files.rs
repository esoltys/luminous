//! The Files view (#1841): browsing folders, servers and devices as they are
//! laid out. See `crate::files` for what a listing holds.

use crate::collection::{CollectionScanner, ScanTrigger};
use crate::files::{
    self, DirListing, FileNavigationTarget, FileSource, FolderSearchResult, SourceSummary,
};
use crate::models::Song;
use crate::shell_open::{self, OpenOutcome};
use crate::AppState;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

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
        files::list_dir(&db, &source, &path, |id, storage| {
            devices.open_storage(id, storage)
        })
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

/// Where a library song is in the Files view, for View in Files; `None` when
/// it has no place there (a stream, a file outside every watched folder).
#[tauri::command]
pub async fn locate_song_in_files(
    song_id: i64,
    state: State<'_, AppState>,
) -> Result<Option<FileNavigationTarget>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || files::locate_song(&db, song_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// The folder an album is in, in the source holding most of its songs.
#[tauri::command]
pub async fn locate_album_in_files(
    album: String,
    state: State<'_, AppState>,
) -> Result<Option<FileNavigationTarget>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || files::locate_album(&db, &album))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Opens a sidecar file (`path` ends with its name) in the app its type
/// opens with, or shows it in the file manager when there is none. Only a
/// listed non-audio file in a watched folder opens; anything else rejects
/// with `not-openable`.
#[tauri::command]
pub async fn open_source_file(
    app: AppHandle,
    source: FileSource,
    path: Vec<String>,
    state: State<'_, AppState>,
) -> Result<OpenOutcome, String> {
    let file = crate::db::run_blocking(&state.db, move |conn| {
        files::openable_file(conn, &source, &path)
    })
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "not-openable".to_string())?;
    // Asking Windows about the type reads the registry: off the runtime.
    tokio::task::spawn_blocking(move || {
        let opener = app.opener();
        shell_open::open_or_reveal(
            &file,
            |f| Ok(opener.open_path(f.to_string_lossy(), None::<&str>)?),
            |f| Ok(opener.reveal_item_in_dir(f)?),
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| {
        log::warn!("Failed to open a file: {e:#}");
        e.to_string()
    })
}

/// Searches watched folders and media server directories for folders whose
/// names match `query` (#1886).
#[tauri::command]
pub async fn search_source_folders(
    query: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<FolderSearchResult>, String> {
    let db = state.db.clone();
    let max = limit.unwrap_or(5);
    crate::db::run_blocking(&state.db, move |conn| {
        files::search_folders(conn, &db, &query, max)
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
