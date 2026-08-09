use crate::{
    models::{Playlist, PlaylistItem, QueuePopulationMode},
    AppState,
};
use tauri::State;

#[derive(serde::Serialize)]
pub struct PlaylistNameCheck {
    pub valid: bool,
    /// Why the name was rejected ("reserved"), when invalid.
    pub reason: Option<String>,
}

/// Answers "would this playlist name be rejected?" so the frontend can
/// validate before submitting, instead of matching substrings of the
/// create/rename error message after the fact.
#[tauri::command]
pub fn validate_playlist_name(name: String) -> PlaylistNameCheck {
    if crate::playlist::is_reserved_playlist_name(&name) {
        PlaylistNameCheck {
            valid: false,
            reason: Some("reserved".to_string()),
        }
    } else {
        PlaylistNameCheck {
            valid: true,
            reason: None,
        }
    }
}

#[tauri::command]
pub async fn create_playlist(name: String, state: State<'_, AppState>) -> Result<Playlist, String> {
    state
        .playlists
        .lock()
        .await
        .create_playlist(&name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_playlist(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .delete_playlist(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_playlist(
    id: i64,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .rename_playlist(id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playlists(state: State<'_, AppState>) -> Result<Vec<Playlist>, String> {
    state
        .playlists
        .lock()
        .await
        .get_playlists()
        .map_err(|e| e.to_string())
}

/// Runs the genre, decade, and BPM auto-playlist syncs together — the
/// frontend used to invoke these as three separate round trips in lockstep
/// at every call site.
#[tauri::command]
pub async fn sync_all_auto_playlists(state: State<'_, AppState>) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .sync_all_auto_playlists()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_decade(
    state: State<'_, AppState>,
    decade: String,
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
) -> Result<Vec<crate::models::Song>, String> {
    let scanner = crate::collection::CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_decade(&decade, limit.unwrap_or(50), mode.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_bpm(
    state: State<'_, AppState>,
    spec: String,
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
) -> Result<Vec<crate::models::Song>, String> {
    let (min, max) = crate::collection::parse_bpm_range_spec(&spec)
        .ok_or_else(|| format!("Invalid BPM range spec: {spec}"))?;
    let scanner = crate::collection::CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_bpm_range(min, max, limit.unwrap_or(50), mode.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playlists_by_artist(
    artist: String,
    state: State<'_, AppState>,
) -> Result<Vec<Playlist>, String> {
    state
        .playlists
        .lock()
        .await
        .get_playlists_by_artist(&artist)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playlist_tracks(
    playlist_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<PlaylistItem>, String> {
    let player = state.player.lock().await;
    if player.current_playlist_id == Some(playlist_id) {
        let items = player.get_playlist_tracks_in_playback_order();
        if !items.is_empty() {
            return Ok(items);
        }
    }
    drop(player);

    state
        .playlists
        .lock()
        .await
        .get_playlist_tracks(playlist_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_to_playlist(
    playlist_id: i64,
    song_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .add_songs_to_playlist(playlist_id, &song_ids)
        .map_err(|e| e.to_string())
}

/// Cascades a set of removed playlist-item uuids into the live playback
/// queue and broadcasts the resulting state. The live-queue side is keyed
/// by item UUID, which is never shared across playlists, so this is a safe
/// no-op whenever none of `uuids` are currently loaded in the queue (or the
/// list is empty).
async fn sync_player_after_removal(
    state: &State<'_, AppState>,
    app: &tauri::AppHandle,
    uuids: &[String],
) {
    use tauri::Emitter;
    if uuids.is_empty() {
        return;
    }
    let mut player = state.player.lock().await;
    player.remove_songs_from_playlist_items(uuids);
    let playback_state = player.get_state().await;
    let _ = app.emit("playback-state", playback_state);
}

/// Removes rows from a playlist and mirrors the removal into the live play
/// queue in one call — callers no longer need to separately pair this with
/// a live-queue removal.
#[tauri::command]
pub async fn remove_from_playlist(
    playlist_id: i64,
    uuids: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .remove_from_playlist(playlist_id, &uuids)
        .map_err(|e| e.to_string())?;

    sync_player_after_removal(&state, &app, &uuids).await;
    Ok(())
}

/// Removes duplicate items from a playlist (see
/// `PlaylistManager::deduplicate_playlist` for the identity rule) and
/// mirrors the removal into the live play queue in one call. Returns the
/// removed uuids.
#[tauri::command]
pub async fn deduplicate_playlist(
    playlist_id: i64,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let removed_uuids = state
        .playlists
        .lock()
        .await
        .deduplicate_playlist(playlist_id)
        .map_err(|e| e.to_string())?;

    sync_player_after_removal(&state, &app, &removed_uuids).await;
    Ok(removed_uuids)
}

/// Removes every item ahead of `uuid` in `playlist_id` and mirrors the
/// removal into the live play queue in one call. Resolves `uuid`'s position
/// and removes it atomically server-side (see
/// `PlaylistManager::trim_playlist_before_uuid`), so there's no window for
/// the Queue to change shape between reading a position and cutting it.
#[tauri::command]
pub async fn trim_playlist_before_uuid(
    playlist_id: i64,
    uuid: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let removed_uuids = state
        .playlists
        .lock()
        .await
        .trim_playlist_before_uuid(playlist_id, &uuid)
        .map_err(|e| e.to_string())?;

    sync_player_after_removal(&state, &app, &removed_uuids).await;
    Ok(())
}

/// Reorders a playlist row and mirrors the move into the live play queue in
/// one call. The live-queue side is keyed by item UUID (never shared across
/// playlists), so it's a safe no-op whenever `playlist_id` isn't the Queue
/// or either UUID isn't currently loaded.
#[tauri::command]
pub async fn reorder_playlist_item_by_uuid(
    playlist_id: i64,
    source_uuid: String,
    target_uuid: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Emitter;

    state
        .playlists
        .lock()
        .await
        .reorder_playlist_item_by_uuid(playlist_id, &source_uuid, &target_uuid)
        .map_err(|e| e.to_string())?;

    let mut player = state.player.lock().await;
    player.reorder_playlist_item_by_uuid(&source_uuid, &target_uuid);
    let playback_state = player.get_state().await;
    let _ = app.emit("playback-state", playback_state);

    Ok(())
}

/// Reorders a playlist row and, only when `playlist_id` is the Queue, mirrors
/// the move into the live play queue by index. Unlike the UUID-keyed reorder
/// above, an index pair is meaningless outside the playlist it was computed
/// against, so this check (rather than an unconditional call) is required to
/// avoid corrupting live playback when editing an unrelated playlist.
#[tauri::command]
pub async fn reorder_playlist_item(
    playlist_id: i64,
    from: i32,
    to: i32,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Emitter;

    let is_queue = {
        let mut playlists = state.playlists.lock().await;
        playlists
            .reorder_playlist_item(playlist_id, from, to)
            .map_err(|e| e.to_string())?;
        playlists
            .queue()
            .map(|q| q.id == playlist_id)
            .unwrap_or(false)
    };

    if is_queue {
        let mut player = state.player.lock().await;
        player.reorder_playlist_items(from as usize, to as usize);
        let playback_state = player.get_state().await;
        let _ = app.emit("playback-state", playback_state);
    }

    Ok(())
}

#[tauri::command]
pub async fn reorder_playlist_items(
    playlist_id: i64,
    from_indices: Vec<i32>,
    to_index: i32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .reorder_playlist_items_batch(playlist_id, &from_indices, to_index)
        .map_err(|e| e.to_string())
}

/// Clears a playlist's rows and, only when `playlist_id` is the Queue, also
/// clears the live play queue. `Player::clear_playlist_items` has no target
/// scoping of its own (it just wipes everything), so this check is required
/// to avoid stopping live playback when clearing an unrelated playlist.
#[tauri::command]
pub async fn clear_playlist(
    playlist_id: i64,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Emitter;

    let is_queue = {
        let mut playlists = state.playlists.lock().await;
        playlists
            .clear_playlist(playlist_id)
            .map_err(|e| e.to_string())?;
        playlists
            .queue()
            .map(|q| q.id == playlist_id)
            .unwrap_or(false)
    };

    if is_queue {
        let mut player = state.player.lock().await;
        player.clear_playlist_items();
        let playback_state = player.get_state().await;
        let _ = app.emit("playback-state", playback_state);
    }

    Ok(())
}

/// Returns whether an op was actually undone — `false` means the stack was
/// already empty, a routine state rather than an error (see
/// `PlaylistManager::undo`).
#[tauri::command]
pub async fn undo_playlist(state: State<'_, AppState>) -> Result<bool, String> {
    state
        .playlists
        .lock()
        .await
        .undo()
        .map_err(|e| e.to_string())
}

/// See [`undo_playlist`] — same `false`-means-empty-stack contract.
#[tauri::command]
pub async fn redo_playlist(state: State<'_, AppState>) -> Result<bool, String> {
    state
        .playlists
        .lock()
        .await
        .redo()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_playlist(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<Playlist, String> {
    state
        .playlists
        .lock()
        .await
        .import_playlist(&file_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_playlist(
    playlist_id: i64,
    export_path: String,
    relative: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .export_playlist(playlist_id, &export_path, relative)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_playlist_dynamic_spec(
    playlist_id: i64,
    spec: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .set_playlist_dynamic_spec(playlist_id, &spec)
        .map_err(|e| e.to_string())
}

/// Set a dynamic playlist's population-mode bias and its rule spec together,
/// populating its tracks once. Replaces the frontend's old two-call
/// sequence (set-mode, which repopulated via `set_playlist_population_mode`
/// below, then set-spec, which populated again) that regenerated the
/// playlist's tracks twice per edit.
#[tauri::command]
pub async fn set_playlist_dynamic_config(
    playlist_id: i64,
    mode: QueuePopulationMode,
    spec: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .set_playlist_dynamic_config(playlist_id, mode, &spec)
        .map_err(|e| e.to_string())
}

/// Persist the `population_mode` bias (All/Favourites/Familiar/Discover/Deep
/// Cuts, see #120) for a playlist, and immediately regenerate its tracks so
/// the change takes effect right away rather than waiting for the next
/// scheduled/manual refresh.
#[tauri::command]
pub async fn set_playlist_population_mode(
    playlist_id: i64,
    mode: QueuePopulationMode,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut pm = state.playlists.lock().await;
    pm.set_playlist_population_mode(playlist_id, mode)
        .map_err(|e| e.to_string())?;
    pm.refresh_auto_playlist(playlist_id)
        .map_err(|e| e.to_string())
}

/// Force-regenerates one auto-playlist's tracks from the library (e.g. when
/// the user clicks the "Refresh" button in a single auto-playlist's header).
#[tauri::command]
pub async fn refresh_auto_playlist(
    playlist_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .refresh_auto_playlist(playlist_id)
        .map_err(|e| e.to_string())
}

/// Force-regenerates every dynamic/auto playlist's tracks from the library
/// (e.g. when the user clicks the "Refresh All" button) — one call in place
/// of the frontend's old per-playlist fan-out.
#[tauri::command]
pub async fn refresh_all_auto_playlists(state: State<'_, AppState>) -> Result<(), String> {
    state
        .playlists
        .lock()
        .await
        .refresh_all_dynamic_playlists()
        .map_err(|e| e.to_string())
}
