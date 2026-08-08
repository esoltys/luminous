use crate::{
    models::{PlayContext, PlaybackState, RepeatMode, ShuffleMode},
    AppState,
};
use tauri::State;

/// Shared tail of every "replace the Queue and start playing" command:
/// swap the Queue's contents for `song_ids`, then hand the fresh items to
/// the player.
async fn replace_queue_and_play(
    state: &State<'_, AppState>,
    song_ids: &[i64],
    start_index: usize,
    context: Option<PlayContext>,
) -> Result<(), String> {
    let (queue_id, items) = {
        let mut playlists = state.playlists.lock().await;
        playlists
            .replace_queue(song_ids)
            .map_err(|e| e.to_string())?
    };
    let mut player = state.player.lock().await;
    player
        .play_playlist(items, start_index, queue_id, context)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn play_song(song_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    use rusqlite::params;
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    // Reject unknown ids up front so the Queue isn't cleared for nothing.
    let sql = format!(
        "SELECT {} FROM songs WHERE id = ?1",
        crate::collection::SONG_SELECT_COLS
    );
    let _song = conn
        .query_row(&sql, params![song_id], crate::collection::row_to_song)
        .map_err(|e| e.to_string())?;

    replace_queue_and_play(&state, &[song_id], 0, Some(PlayContext::Song)).await
}

#[tauri::command]
pub async fn play_songs(
    song_ids: Vec<i64>,
    start_index: usize,
    playlist_id: Option<i64>,
    context: Option<PlayContext>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if song_ids.is_empty() {
        return Ok(());
    }

    // Playing "into" the Queue (explicitly or by default) replaces its
    // contents; playing a real playlist just plays that playlist's rows.
    let target = match playlist_id {
        None => None,
        Some(pid) => {
            let playlists = state.playlists.lock().await;
            let queue = playlists.queue().map_err(|e| e.to_string())?;
            if pid == queue.id {
                None
            } else {
                Some(pid)
            }
        }
    };

    match target {
        None => replace_queue_and_play(&state, &song_ids, start_index, context).await,
        Some(pid) => {
            let items = {
                let playlists = state.playlists.lock().await;
                playlists
                    .get_playlist_tracks(pid)
                    .map_err(|e| e.to_string())?
            };
            let mut player = state.player.lock().await;
            player
                .play_playlist(items, start_index, pid, context)
                .await
                .map_err(|e| e.to_string())
        }
    }
}

#[tauri::command]
pub async fn play_playlist_item(
    playlist_id: i64,
    item_index: usize,
    context: Option<PlayContext>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let items = {
        let playlists = state.playlists.lock().await;
        playlists
            .get_playlist_tracks(playlist_id)
            .map_err(|e| e.to_string())?
    };
    let mut player = state.player.lock().await;
    player
        .play_playlist(items, item_index, playlist_id, context)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn play_playlist_item_by_uuid(
    playlist_id: i64,
    uuid: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let playlists = state.playlists.lock().await;
    let mut player = state.player.lock().await;
    player
        .play_item_by_uuid(playlist_id, &uuid, &playlists)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause(state: State<'_, AppState>) -> Result<(), String> {
    state
        .player
        .lock()
        .await
        .pause()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resume(state: State<'_, AppState>) -> Result<(), String> {
    state
        .player
        .lock()
        .await
        .resume()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop(state: State<'_, AppState>) -> Result<(), String> {
    state
        .player
        .lock()
        .await
        .stop()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn next_track(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    use tauri::Emitter;
    let mut player = state.player.lock().await;
    if let Some(stats) = player.note_manual_skip() {
        let _ = app.emit("song-stats-changed", stats);
    }
    player.next_track().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn previous_track(state: State<'_, AppState>) -> Result<(), String> {
    state
        .player
        .lock()
        .await
        .previous_track()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn seek_to(
    position_nanosec: i64,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let player = state.player.lock().await;
    player
        .seek_to(position_nanosec as u64)
        .await
        .map_err(|e| e.to_string())?;
    let playback_state = player.get_state().await;
    crate::media_session::mirror_state(&app, &playback_state).await;
    Ok(())
}

#[tauri::command]
pub async fn set_volume(volume: f32, state: State<'_, AppState>) -> Result<(), String> {
    state
        .player
        .lock()
        .await
        .set_volume(volume)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playback_state(state: State<'_, AppState>) -> Result<PlaybackState, String> {
    Ok(state.player.lock().await.get_state().await)
}

/// Re-syncs the active playback queue (and current song) with the database.
/// Call after a library rescan so a track that was already queued before a
/// move/deletion picks up its corrected path (or gets marked unavailable)
/// without requiring the queue to be rebuilt or the app restarted.
#[tauri::command]
pub async fn refresh_playback_queue(state: State<'_, AppState>) -> Result<PlaybackState, String> {
    let mut player = state.player.lock().await;
    player.resync_queue_with_db().map_err(|e| e.to_string())?;
    Ok(player.get_state().await)
}

#[tauri::command]
pub async fn set_shuffle_mode(mode: ShuffleMode, state: State<'_, AppState>) -> Result<(), String> {
    state.player.lock().await.set_shuffle_mode(mode);
    // A primed gapless next track may no longer match the new order — drop it
    // and let the engine re-request a preload.
    let _ = state.audio.lock().await.clear_preload();
    Ok(())
}

#[tauri::command]
pub async fn set_repeat_mode(mode: RepeatMode, state: State<'_, AppState>) -> Result<(), String> {
    state.player.lock().await.set_repeat_mode(mode);
    let _ = state.audio.lock().await.clear_preload();
    Ok(())
}

/// What actually happened to the files the user opened. Unplayable inputs
/// are an outcome to report (the store shows a toast), not an error to
/// throw — a Result::Err here would escape as an unhandled rejection.
#[derive(serde::Serialize)]
pub struct OpenAndPlayOutcome {
    pub played: usize,
    pub skipped: usize,
}

#[tauri::command]
pub async fn open_and_play(
    paths: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<OpenAndPlayOutcome, String> {
    use rusqlite::params;
    use rusqlite::OptionalExtension;
    use std::path::Path;
    use tauri::Emitter;

    let mut resolved_paths = Vec::new();
    // Candidate entries considered (input files + playlist tracks); whatever
    // doesn't make it into `songs` below counts as skipped.
    let mut attempted: usize = 0;

    for path_str in paths {
        let path = Path::new(&path_str);
        if !path.exists() {
            attempted += 1;
            continue;
        }

        if path.is_file() {
            if crate::playlist_parsers::PlaylistFormat::from_path(path).is_some() {
                // Parse M3U/M3U8/PLS/XSPF playlist file
                let parsed = match crate::playlist_parsers::parse_playlist(path) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        log::warn!("Failed to read playlist file '{}': {}", path_str, e);
                        attempted += 1;
                        continue;
                    }
                };
                let parent_dir = path.parent();

                for track in parsed.tracks {
                    attempted += 1;
                    let raw_path = track.path_or_url.trim();
                    let clean_rel = raw_path
                        .trim_start_matches("./")
                        .trim_start_matches(".\\")
                        .trim();
                    let mut item_path = std::path::PathBuf::from(clean_rel);
                    if item_path.is_relative() {
                        if let Some(parent) = parent_dir {
                            item_path = parent.join(&item_path);
                        }
                    }

                    if item_path.exists() && item_path.is_file() {
                        let is_audio = item_path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| {
                                crate::collection::AUDIO_EXTENSIONS
                                    .contains(&e.to_ascii_lowercase().as_str())
                            })
                            .unwrap_or(false);

                        if is_audio {
                            resolved_paths.push(item_path);
                        }
                    } else {
                        log::warn!(
                            "Playlist track path could not be resolved or does not exist: {}",
                            item_path.display()
                        );
                    }
                }
            } else {
                attempted += 1;
                let is_audio = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| {
                        crate::collection::AUDIO_EXTENSIONS
                            .contains(&e.to_ascii_lowercase().as_str())
                    })
                    .unwrap_or(false);

                if is_audio {
                    resolved_paths.push(path.to_path_buf());
                }
            }
        }
    }

    if resolved_paths.is_empty() {
        return Ok(OpenAndPlayOutcome {
            played: 0,
            skipped: attempted,
        });
    }

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let mut songs = Vec::with_capacity(resolved_paths.len());

    for item_path in resolved_paths {
        let item_path_str = item_path.to_string_lossy().to_string();
        let norm_path_1 = item_path_str.replace('/', "\\");
        let norm_path_2 = item_path_str.replace('\\', "/");

        // Check if exists in db (handling slash normalization and case variations)
        let existing: Option<crate::models::Song> = conn
            .query_row(
                &format!(
                    "SELECT {} FROM songs WHERE path = ?1 OR path = ?2 OR path = ?3 OR LOWER(path) = LOWER(?1)",
                    crate::collection::SONG_SELECT_COLS
                ),
                params![item_path_str, norm_path_1, norm_path_2],
                crate::collection::row_to_song,
            )
            .optional()
            .map_err(|e| e.to_string())?;

        if let Some(mut s) = existing {
            if s.unavailable {
                let _ = conn.execute(
                    "UPDATE songs SET unavailable = 0 WHERE id = ?1",
                    params![s.id],
                );
                s.unavailable = false;
            }
            songs.push(s);
        } else {
            // Read tags and upsert
            match crate::collection::read_tags(&item_path) {
                Ok(mut song) => {
                    // Extract or scan artwork
                    if song.art_embedded {
                        let artist = song
                            .album_artist
                            .as_deref()
                            .unwrap_or(song.artist.as_deref().unwrap_or(""));
                        let album = song.album.as_deref().unwrap_or("");
                        if let Ok(Some(cached_filename)) = state
                            .cover_manager
                            .extract_embedded_art(&item_path, artist, album)
                        {
                            song.art_automatic = Some(cached_filename);
                            song.art_unset = false;
                        }
                    } else if let Some(folder_art_path) =
                        state.cover_manager.scan_folder_art(&item_path)
                    {
                        song.art_automatic = Some(folder_art_path.to_string_lossy().to_string());
                        song.art_unset = false;
                    }

                    if let Err(e) = crate::collection::upsert_song(&conn, &song) {
                        log::error!("Failed to upsert song {}: {}", item_path_str, e);
                        continue;
                    }

                    // Fetch the inserted song to get its ID and full state
                    let inserted: Result<crate::models::Song, _> = conn.query_row(
                        &format!(
                            "SELECT {} FROM songs WHERE path = ?1",
                            crate::collection::SONG_SELECT_COLS
                        ),
                        params![item_path_str],
                        crate::collection::row_to_song,
                    );

                    match inserted {
                        Ok(s) => songs.push(s),
                        Err(e) => {
                            log::error!("Failed to fetch upserted song {}: {}", item_path_str, e);
                        }
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to read tags for local file {}: {}",
                        item_path_str,
                        e
                    );
                }
            }
        }
    }

    if songs.is_empty() {
        // Nothing decodable — leave the current Queue untouched.
        return Ok(OpenAndPlayOutcome {
            played: 0,
            skipped: attempted,
        });
    }

    let song_ids: Vec<i64> = songs.iter().map(|s| s.id).collect();

    replace_queue_and_play(&state, &song_ids, 0, Some(PlayContext::Song)).await?;
    let _ = app.emit("library-changed", ());
    Ok(OpenAndPlayOutcome {
        played: songs.len(),
        skipped: attempted.saturating_sub(songs.len()),
    })
}

#[tauri::command]
pub async fn get_startup_file(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let mut lock = state.startup_file.lock().await;
    Ok(lock.take())
}

/// Add songs to the built-in Queue: appends the DB rows AND mirrors them
/// into the live in-memory play order, so "Add to Queue" is one call and the
/// two sides of that invariant can't drift (previously the frontend had to
/// remember to pair add_to_playlist with a live-queue append).
#[tauri::command]
pub async fn add_songs_to_queue(
    song_ids: Vec<i64>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Emitter;

    let added = {
        let mut playlists = state.playlists.lock().await;
        let queue = playlists.queue().map_err(|e| e.to_string())?;
        let before: std::collections::HashSet<String> = playlists
            .get_playlist_tracks(queue.id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|i| i.uuid)
            .collect();
        playlists
            .add_songs_to_playlist(queue.id, &song_ids)
            .map_err(|e| e.to_string())?;
        playlists
            .get_playlist_tracks(queue.id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|i| !before.contains(&i.uuid))
            .collect::<Vec<_>>()
    };

    if !added.is_empty() {
        let mut player = state.player.lock().await;
        player.append_songs_to_playlist_items(added);
        let playback_state = player.get_state().await;
        let _ = app.emit("playback-state", playback_state);
    }

    Ok(())
}

/// Remove items from the live in-memory player playlist by uuid, so songs
/// removed from the Queue's DB playlist stop being up-next during playback.
/// Called by the frontend after `remove_from_playlist` has deleted the rows
/// from the DB (see #262).
#[tauri::command]
pub async fn remove_songs_from_player_playlist(
    uuids: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Emitter;
    let mut player = state.player.lock().await;
    player.remove_songs_from_playlist_items(&uuids);
    let playback_state = player.get_state().await;
    let _ = app.emit("playback-state", playback_state);
    Ok(())
}

#[tauri::command]
pub async fn reorder_player_playlist_items(
    from_index: usize,
    to_index: usize,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Emitter;
    let mut player = state.player.lock().await;
    player.reorder_playlist_items(from_index, to_index);
    let playback_state = player.get_state().await;
    let _ = app.emit("playback-state", playback_state);
    Ok(())
}

#[tauri::command]
pub async fn reorder_player_item_by_uuid(
    source_uuid: String,
    target_uuid: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Emitter;
    let mut player = state.player.lock().await;
    player.reorder_playlist_item_by_uuid(&source_uuid, &target_uuid);
    let playback_state = player.get_state().await;
    let _ = app.emit("playback-state", playback_state);
    Ok(())
}

#[tauri::command]
pub async fn clear_player_playlist(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri::Emitter;
    let mut player = state.player.lock().await;
    player.clear_playlist_items();
    let playback_state = player.get_state().await;
    let _ = app.emit("playback-state", playback_state);
    Ok(())
}
