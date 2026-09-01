use crate::{
    models::{PlayContext, PlaybackState, RepeatMode, ShuffleMode},
    AppState, CoverManager,
};
use std::path::{Path, PathBuf};
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

/// Recursively collects audio files under a dropped directory, mirroring the
/// library scanner's walk (`collection::scan_all`) but scoped to this one
/// ad-hoc drop rather than a watched folder. Sorted so a dropped album folder
/// plays back in a stable, predictable track order.
fn collect_dir_audio_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(true)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && crate::collection::is_audio_file(path) {
            out.push(path.to_path_buf());
        }
    }
}

/// Expands user-dropped/opened `paths` — audio files, playlist files
/// (M3U/M3U8/PLS/XSPF), and directories (recursed) — into concrete audio file
/// paths. Returns the resolved paths plus a count of every candidate entry
/// considered, so callers can report `skipped = attempted -
/// resolved.len()` (further narrowed once DB/tag resolution runs below).
fn resolve_dropped_paths(paths: Vec<String>) -> (Vec<PathBuf>, usize) {
    let mut resolved_paths = Vec::new();
    let mut attempted: usize = 0;

    for path_str in paths {
        let path = Path::new(&path_str);
        if !path.exists() {
            attempted += 1;
            continue;
        }

        if path.is_dir() {
            let mut dir_files = Vec::new();
            collect_dir_audio_files(path, &mut dir_files);
            // An empty/non-audio folder still counts as one attempted (and
            // therefore skipped) candidate, rather than silently vanishing.
            attempted += dir_files.len().max(1);
            resolved_paths.extend(dir_files);
        } else if path.is_file() {
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
                    let mut item_path = PathBuf::from(clean_rel);
                    if item_path.is_relative() {
                        if let Some(parent) = parent_dir {
                            item_path = parent.join(&item_path);
                        }
                    }

                    if item_path.exists()
                        && item_path.is_file()
                        && crate::collection::is_audio_file(&item_path)
                    {
                        resolved_paths.push(item_path);
                    } else {
                        log::warn!(
                            "Playlist track path could not be resolved or does not exist: {}",
                            item_path.display()
                        );
                    }
                }
            } else {
                attempted += 1;
                if crate::collection::is_audio_file(path) {
                    resolved_paths.push(path.to_path_buf());
                }
            }
        }
    }

    (resolved_paths, attempted)
}

/// Resolves already-expanded file paths into `Song` rows: matches existing
/// library entries by path (normalizing slashes/case), or reads tags and
/// upserts a fresh row for anything not yet in the library. Shared by every
/// command that turns raw filesystem paths into Queue entries, so an
/// unmanaged file dropped or opened from outside the library is always
/// treated identically.
fn resolve_songs_from_paths(
    conn: &rusqlite::Connection,
    cover_manager: &CoverManager,
    resolved_paths: Vec<PathBuf>,
) -> Result<Vec<crate::models::Song>, String> {
    use rusqlite::params;
    use rusqlite::OptionalExtension;

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
            continue;
        }

        if let Err(e) = crate::collection::read_and_upsert_song(conn, cover_manager, &item_path) {
            log::warn!("Failed to read tags for '{}': {}", item_path_str, e);
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

    Ok(songs)
}

/// Shared tail of every "append these song ids to the Queue" command: appends
/// the DB rows AND mirrors them into the live in-memory play order, so the
/// two sides of that invariant can't drift.
async fn append_song_ids_to_queue(
    state: &State<'_, AppState>,
    song_ids: &[i64],
    app: &tauri::AppHandle,
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
            .add_songs_to_playlist(queue.id, song_ids)
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

/// What actually happened to the files the user dropped/added to the Queue.
/// Same shape as `OpenAndPlayOutcome` conceptually, but `added` (not
/// `played`) since these tracks were appended, not started playing.
#[derive(serde::Serialize)]
pub struct AddPathsOutcome {
    pub added: usize,
    pub skipped: usize,
}

#[tauri::command]
pub async fn open_and_play(
    paths: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<OpenAndPlayOutcome, String> {
    use tauri::Emitter;

    let (resolved_paths, attempted) = resolve_dropped_paths(paths);

    if resolved_paths.is_empty() {
        return Ok(OpenAndPlayOutcome {
            played: 0,
            skipped: attempted,
        });
    }

    let songs = {
        let conn = state.db.pool.get().map_err(|e| e.to_string())?;
        resolve_songs_from_paths(&conn, &state.cover_manager, resolved_paths)?
    };

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

/// Drag-and-dropped (or otherwise opened) paths, resolved and appended to the
/// end of the Queue instead of replacing it — the "hold Shift to append"
/// counterpart to `open_and_play`. Playback is left untouched.
#[tauri::command]
pub async fn add_paths_to_queue(
    paths: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AddPathsOutcome, String> {
    let (resolved_paths, attempted) = resolve_dropped_paths(paths);

    if resolved_paths.is_empty() {
        return Ok(AddPathsOutcome {
            added: 0,
            skipped: attempted,
        });
    }

    let songs = {
        let conn = state.db.pool.get().map_err(|e| e.to_string())?;
        resolve_songs_from_paths(&conn, &state.cover_manager, resolved_paths)?
    };

    if songs.is_empty() {
        return Ok(AddPathsOutcome {
            added: 0,
            skipped: attempted,
        });
    }

    let song_ids: Vec<i64> = songs.iter().map(|s| s.id).collect();
    append_song_ids_to_queue(&state, &song_ids, &app).await?;

    Ok(AddPathsOutcome {
        added: songs.len(),
        skipped: attempted.saturating_sub(songs.len()),
    })
}

/// Whether Shift is physically held right now, queried from the OS rather
/// than the DOM. A native OS file drag doesn't give the target window
/// keyboard focus while hovering, so the frontend's keydown/keyup listeners
/// never fire during a drag-and-drop — this is the only reliable way to tell
/// a plain drop (replace Queue) apart from a Shift-drop (append to Queue).
#[tauri::command]
pub fn is_shift_key_held() -> bool {
    platform_is_shift_key_held()
}

#[cfg(windows)]
fn platform_is_shift_key_held() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};

    // High bit set means the key is currently down. Safe to call from any
    // thread; doesn't require window focus.
    (unsafe { GetAsyncKeyState(VK_SHIFT as i32) } as u16 & 0x8000) != 0
}

// No X11/Wayland key-state API is wired up on Linux, so this degrades to
// "not held" rather than pulling in an X11 dependency for a minor
// drag-and-drop modifier — Shift-drop just isn't available there yet.
#[cfg(not(windows))]
fn platform_is_shift_key_held() -> bool {
    false
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
    append_song_ids_to_queue(&state, &song_ids, &app).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolve_dropped_paths_expands_a_directory_recursively_and_sorted() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("disc2")).unwrap();
        fs::write(dir.path().join("b.mp3"), b"").unwrap();
        fs::write(dir.path().join("a.flac"), b"").unwrap();
        fs::write(dir.path().join("notes.txt"), b"").unwrap();
        fs::write(dir.path().join("disc2/c.wav"), b"").unwrap();

        let (resolved, attempted) =
            resolve_dropped_paths(vec![dir.path().to_string_lossy().to_string()]);

        assert_eq!(attempted, 3);
        let names: Vec<String> = resolved
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["a.flac", "b.mp3", "c.wav"]);
    }

    #[test]
    fn resolve_dropped_paths_counts_an_empty_directory_as_one_skipped_candidate() {
        let dir = tempfile::tempdir().unwrap();

        let (resolved, attempted) =
            resolve_dropped_paths(vec![dir.path().to_string_lossy().to_string()]);

        assert!(resolved.is_empty());
        assert_eq!(attempted, 1);
    }

    #[test]
    fn resolve_dropped_paths_accepts_a_single_audio_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("song.mp3");
        fs::write(&file_path, b"").unwrap();

        let (resolved, attempted) =
            resolve_dropped_paths(vec![file_path.to_string_lossy().to_string()]);

        assert_eq!(attempted, 1);
        assert_eq!(resolved, vec![file_path]);
    }

    #[test]
    fn resolve_dropped_paths_skips_non_audio_files_and_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let text_path = dir.path().join("readme.txt");
        fs::write(&text_path, b"").unwrap();
        let missing_path = dir.path().join("missing.mp3");

        let (resolved, attempted) = resolve_dropped_paths(vec![
            text_path.to_string_lossy().to_string(),
            missing_path.to_string_lossy().to_string(),
        ]);

        assert!(resolved.is_empty());
        assert_eq!(attempted, 2);
    }

    #[test]
    fn is_shift_key_held_does_not_panic() {
        // On Windows the actual boolean depends on real input state and
        // isn't something a unit test can control; just assert it returns.
        // On every other platform it's a fixed `false` (see
        // `platform_is_shift_key_held`).
        #[cfg(not(windows))]
        assert!(!is_shift_key_held());
        #[cfg(windows)]
        let _ = is_shift_key_held();
    }
}
