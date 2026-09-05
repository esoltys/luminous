use crate::{
    collection::CollectionScanner,
    models::{
        ArtistProfile, HomeItem, LibraryStats, MusicDirectory, PruneResult, Song, TopAlbumItem,
    },
    AppState,
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn add_directory(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<MusicDirectory, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    let res = scanner.add_directory(&path).map_err(|e| e.to_string())?;
    crate::collection::start_watcher(app, &state);
    Ok(res)
}

#[tauri::command]
pub async fn remove_directory(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.remove_directory(&path).map_err(|e| e.to_string())?;
    crate::collection::start_watcher(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn get_directories(state: State<'_, AppState>) -> Result<Vec<MusicDirectory>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_directories().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_directory_metadata(
    id: i64,
    nickname: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .update_directory_metadata(id, nickname, icon, color)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_directories(
    app: AppHandle,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .scan_all(app, force.unwrap_or(false), false)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn prune_missing_songs(state: State<'_, AppState>) -> Result<PruneResult, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.prune_missing_songs().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_library_stats(state: State<'_, AppState>) -> Result<LibraryStats, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_library_stats().map_err(|e| e.to_string())
}

/// Runs the backend-consistency steps a completed scan requires: persist
/// `last_scan_time`, resync the live playback queue with the DB (a scan can
/// repoint a moved file's path or drop a missing one out from under an
/// already-queued track), and resync auto-playlists (a scan can add new
/// genres/decades or shift which songs qualify). Callers no longer need to
/// remember to fire all three separately.
#[tauri::command]
pub async fn finish_scan(last_scan_time: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(conn) = state.db.pool.get() {
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_scan_time', ?1)",
            [&last_scan_time],
        ) {
            log::error!("Failed to persist last_scan_time: {e}");
        }
    }

    if let Err(e) = state.player.lock().await.resync_queue_with_db() {
        log::error!("Failed to resync playback queue after scan: {e}");
    }

    state
        .playlists
        .lock()
        .await
        .sync_all_auto_playlists()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_songs(
    query: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .search_songs(&query, limit.unwrap_or(500))
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct LibrarySnapshot {
    pub songs: Vec<Song>,
    pub albums: Vec<serde_json::Value>,
    pub artists: Vec<serde_json::Value>,
}

/// The Collection view's "give me everything" read — songs, albums, and
/// artists always get refreshed together (initial load, post-scan, on
/// library-changed events), so callers no longer have to remember to fire
/// all three round trips themselves.
#[tauri::command]
pub async fn get_library_snapshot(state: State<'_, AppState>) -> Result<LibrarySnapshot, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    Ok(LibrarySnapshot {
        songs: scanner.get_songs(-1, 0).map_err(|e| e.to_string())?,
        albums: scanner.get_albums().map_err(|e| e.to_string())?,
        artists: scanner.get_artists().map_err(|e| e.to_string())?,
    })
}

#[tauri::command]
pub async fn get_songs_by_album(
    album: String,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_album(&album)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_artist(
    artist: String,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_artist(&artist)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_compilations_by_artist(
    artist: String,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_compilations_by_artist(&artist)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_favourite_songs(state: State<'_, AppState>) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_favourite_songs().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_added_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_added_songs(limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_most_played_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_most_played_songs(limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_top_artists(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_top_artists(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_played(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_played(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_played_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_played_songs(limit.unwrap_or(100))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_play_history(state: State<'_, AppState>) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.clear_play_history().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_added(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_added(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_featured_albums(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_featured_albums(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_top_albums(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<TopAlbumItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_top_albums(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_artist_profile(
    artist: String,
    state: State<'_, AppState>,
) -> Result<ArtistProfile, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_artist_profile(&artist)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_artist_profile(
    profile: ArtistProfile,
    state: State<'_, AppState>,
) -> Result<ArtistProfile, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .set_artist_profile(&profile)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_all_artist_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<ArtistProfile>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_all_artist_profiles().map_err(|e| e.to_string())
}

/// Marks (or unmarks) one or more songs "Not included" (#104): excluded from
/// auto/smart-playlist generation and Auto-Play refill, but still fully
/// visible and playable in Album/Artist views. Fires a `song-stats-changed`
/// event per song (the same event ratings/playcounts use to patch cached
/// `Song` rows in place, e.g. `AlbumDetailView`'s badge/context-menu label)
/// so open views update immediately, plus `library-changed` so dynamic
/// playlists reconcile their membership.
#[tauri::command]
pub async fn set_songs_not_included(
    app: AppHandle,
    song_ids: Vec<i64>,
    not_included: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if song_ids.is_empty() {
        return Ok(());
    }
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let placeholders = song_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("UPDATE songs SET not_included = ?1 WHERE id IN ({placeholders})");
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&not_included];
    params.extend(song_ids.iter().map(|id| id as &dyn rusqlite::ToSql));
    conn.execute(&sql, params.as_slice())
        .map_err(|e| e.to_string())?;

    for song_id in &song_ids {
        let _ = app.emit(
            "song-stats-changed",
            serde_json::json!({ "song_id": song_id, "not_included": not_included }),
        );
    }
    let _ = app.emit("library-changed", ());
    Ok(())
}
