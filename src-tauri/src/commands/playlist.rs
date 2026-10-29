use crate::{
    models::{Playlist, PlaylistItem, QueuePopulationMode},
    playlist::PlaylistManager,
    AppState,
};
use tauri::State;

/// Thin `String`-error adapter over [`crate::playlist::with_playlists`] for
/// this module's `#[tauri::command]` handlers, which all return
/// `Result<_, String>` per this project's IPC command convention.
async fn with_playlists<F, R>(state: &State<'_, AppState>, f: F) -> Result<R, String>
where
    F: FnOnce(&mut PlaylistManager) -> anyhow::Result<R>,
{
    crate::playlist::with_playlists(&state.playlists, f)
        .await
        .map_err(|e| e.to_string())
}

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
    with_playlists(&state, |pm| pm.create_playlist(&name)).await
}

#[tauri::command]
pub async fn delete_playlist(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    with_playlists(&state, |pm| pm.delete_playlist(id)).await
}

#[tauri::command]
pub async fn rename_playlist(
    id: i64,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_playlists(&state, |pm| pm.rename_playlist(id, &name)).await
}

#[tauri::command]
pub async fn get_playlists(state: State<'_, AppState>) -> Result<Vec<Playlist>, String> {
    with_playlists(&state, |pm| pm.get_playlists()).await
}

/// Runs the genre, decade, and BPM auto-playlist syncs together — the
/// frontend used to invoke these as three separate round trips in lockstep
/// at every call site.
#[tauri::command]
pub async fn sync_all_auto_playlists(state: State<'_, AppState>) -> Result<(), String> {
    crate::playlist::sync_all_auto_playlists_cooperative(&state.playlists)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_decade(
    state: State<'_, AppState>,
    decade: String,
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
) -> Result<Vec<crate::models::Song>, String> {
    let limit = limit.unwrap_or(50);
    let mode = mode.unwrap_or_default();
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_songs_by_decade(&decade, limit, mode)
    })
    .await
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
    let limit = limit.unwrap_or(50);
    let mode = mode.unwrap_or_default();
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_songs_by_bpm_range(min, max, limit, mode)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_artist_tag(
    state: State<'_, AppState>,
    tag: String,
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
) -> Result<Vec<crate::models::Song>, String> {
    let limit = limit.unwrap_or(50);
    let mode = mode.unwrap_or_default();
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_songs_by_artist_tag(&tag, limit, mode)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playlists_by_artist(
    artist: String,
    state: State<'_, AppState>,
) -> Result<Vec<Playlist>, String> {
    with_playlists(&state, |pm| pm.get_playlists_by_artist(&artist)).await
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

    with_playlists(&state, |pm| pm.get_playlist_tracks(playlist_id)).await
}

#[tauri::command]
pub async fn add_to_playlist(
    playlist_id: i64,
    song_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_playlists(&state, |pm| {
        pm.add_songs_to_playlist(playlist_id, &song_ids)
    })
    .await
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
    with_playlists(&state, |pm| pm.remove_from_playlist(playlist_id, &uuids)).await?;

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
    let removed_uuids = with_playlists(&state, |pm| pm.deduplicate_playlist(playlist_id)).await?;

    sync_player_after_removal(&state, &app, &removed_uuids).await;
    Ok(removed_uuids)
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

    with_playlists(&state, |pm| {
        pm.reorder_playlist_item_by_uuid(playlist_id, &source_uuid, &target_uuid)
    })
    .await?;

    let mut player = state.player.lock().await;
    player.reorder_playlist_item_by_uuid(&source_uuid, &target_uuid);
    let playback_state = player.get_state().await;
    let _ = app.emit("playback-state", playback_state);

    Ok(())
}

/// Brings the live player in line with `playlist_id`'s order after a sort or
/// move, when it is the playlist being played, and broadcasts the new state.
async fn sync_playing_order(
    playlist_id: i64,
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Emitter;

    if state.player.lock().await.current_playlist_id != Some(playlist_id) {
        return Ok(());
    }
    let order: Vec<String> = with_playlists(state, |pm| pm.get_playlist_tracks(playlist_id))
        .await?
        .into_iter()
        .map(|i| i.uuid)
        .collect();
    let mut player = state.player.lock().await;
    if player.current_playlist_id == Some(playlist_id) {
        player.apply_playlist_order(&order);
        let playback_state = player.get_state().await;
        let _ = app.emit("playback-state", playback_state);
    }
    Ok(())
}

/// Sorts a playlist by a `Song` field, or returns it to manual order with
/// `field: None`. Every reader of the playlist — its view, playback, export
/// and device sync — then sees this order. A no-op for the Queue.
#[tauri::command]
pub async fn set_playlist_sort(
    playlist_id: i64,
    field: Option<String>,
    asc: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_playlists(&state, |pm| {
        pm.set_playlist_sort(playlist_id, field.as_deref(), asc)
    })
    .await?;
    sync_playing_order(playlist_id, &app, &state).await
}

/// Moves items, in their current relative order, to just before
/// `before_uuid` in the playlist's order (`None`: the end). Moving within a
/// sorted playlist keeps the sorted order as the new manual order and
/// switches to manual order — one undo step. See
/// `PlaylistManager::move_playlist_items`.
#[tauri::command]
pub async fn move_playlist_items(
    playlist_id: i64,
    uuids: Vec<String>,
    before_uuid: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_playlists(&state, |pm| {
        pm.move_playlist_items(playlist_id, &uuids, before_uuid.as_deref())
    })
    .await?;
    sync_playing_order(playlist_id, &app, &state).await
}

/// Sets a playlist's size limit in bytes; `None` removes it. A no-op for
/// the Queue.
#[tauri::command]
pub async fn set_playlist_size_limit(
    playlist_id: i64,
    limit_bytes: Option<u64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_playlists(&state, |pm| {
        pm.set_playlist_size_limit(playlist_id, limit_bytes)
    })
    .await
}

/// Which of a playlist's songs fit its size limit, by library file size in
/// the playlist's order, plus the item order the answer is for. `None`
/// when the playlist has no limit.
#[tauri::command]
pub async fn get_playlist_size_fit(
    playlist_id: i64,
    state: State<'_, AppState>,
) -> Result<Option<crate::playlist::PlaylistSizeFit>, String> {
    with_playlists(&state, |pm| pm.playlist_size_fit(playlist_id)).await
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

    let is_queue = with_playlists(&state, |pm| {
        pm.reorder_playlist_item(playlist_id, from, to)?;
        Ok(pm.queue().map(|q| q.id == playlist_id).unwrap_or(false))
    })
    .await?;

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
    with_playlists(&state, |pm| {
        pm.reorder_playlist_items_batch(playlist_id, &from_indices, to_index)
    })
    .await
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

    let is_queue = with_playlists(&state, |pm| {
        pm.clear_playlist(playlist_id)?;
        Ok(pm.queue().map(|q| q.id == playlist_id).unwrap_or(false))
    })
    .await?;

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
    with_playlists(&state, |pm| pm.undo()).await
}

/// See [`undo_playlist`] — same `false`-means-empty-stack contract.
#[tauri::command]
pub async fn redo_playlist(state: State<'_, AppState>) -> Result<bool, String> {
    with_playlists(&state, |pm| pm.redo()).await
}

#[tauri::command]
pub async fn import_playlist(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<Playlist, String> {
    with_playlists(&state, |pm| pm.import_playlist(&file_path)).await
}

#[tauri::command]
pub async fn export_playlist(
    playlist_id: i64,
    export_path: String,
    relative: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_playlists(&state, |pm| {
        pm.export_playlist(playlist_id, &export_path, relative)
    })
    .await
}

#[tauri::command]
pub async fn set_playlist_dynamic_spec(
    playlist_id: i64,
    spec: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_playlists(&state, |pm| {
        pm.set_playlist_dynamic_spec(playlist_id, &spec)
    })
    .await
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
    with_playlists(&state, |pm| {
        pm.set_playlist_dynamic_config(playlist_id, mode, &spec)
    })
    .await
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
    with_playlists(&state, |pm| {
        pm.set_playlist_population_mode(playlist_id, mode)?;
        pm.refresh_auto_playlist(playlist_id)
    })
    .await
}

/// Force-regenerates one auto-playlist's tracks from the library (e.g. when
/// the user clicks the "Refresh" button in a single auto-playlist's header).
#[tauri::command]
pub async fn refresh_auto_playlist(
    playlist_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_playlists(&state, |pm| pm.refresh_auto_playlist(playlist_id)).await
}

/// Force-regenerates every dynamic/auto playlist's tracks from the library
/// (e.g. when the user clicks the "Refresh All" button) — one call in place
/// of the frontend's old per-playlist fan-out.
#[tauri::command]
pub async fn refresh_all_auto_playlists(state: State<'_, AppState>) -> Result<(), String> {
    with_playlists(&state, |pm| pm.refresh_all_dynamic_playlists()).await
}

/// Populates the Queue with radio songs derived from `seed` and begins playback.
#[tauri::command]
pub async fn start_radio(
    seed: crate::radio::RadioSeed,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<crate::radio::RadioResult, String> {
    crate::radio::start_radio_impl(
        state.inner(),
        seed,
        limit.unwrap_or(crate::radio::DEFAULT_RADIO_LIMIT),
    )
    .await
}

/// Generates a curated list of radio songs for `seed` without starting playback.
#[tauri::command]
pub async fn get_radio_songs(
    seed: crate::radio::RadioSeed,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<crate::radio::RadioResult, String> {
    crate::radio::generate_radio_songs(
        state.inner(),
        seed,
        limit.unwrap_or(crate::radio::DEFAULT_RADIO_LIMIT),
    )
    .await
}
