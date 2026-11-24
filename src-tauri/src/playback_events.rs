//! Audio-event loop and two-phase notification fan-out (#2319).
//!
//! Splits every [`AudioEvent`] into two phases:
//! 1. [`apply_event`] mutates `Player`, captures snapshots (`PlaybackState`,
//!    pipeline info, station click UUID, Auto Continue seed), and drops the
//!    `player` lock before returning [`PlaybackChange`].
//! 2. [`publish`] dispatches all listeners through [`PlaybackEventSink`] in one
//!    fixed, documented order with no `player` lock held (guaranteeing the
//!    `playlists` lock in Auto Continue never nests inside `player`).

use crate::audio::{AudioEngine, AudioEvent, PipelineEqSnapshot};
use crate::models::{AudioPipelineInfo, PlayState, PlaybackState, Song};
use crate::now_playing::StationNowPlaying;
use crate::player::Player;
use crate::scrobbler::ScrobblerManager;
use crate::AppState;
use luminous_audio::{AudioBackendKind, OutputFallbackReason};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ScrobbleAction {
    #[default]
    None,
    /// A track started playing: submit "now playing" and update Discord Rich
    /// Presence (`on_playback_state_changed(..., true, ...)`).
    NowPlaying,
    /// Playback paused/resumed or live-stream announced metadata changed.
    StateChanged { playing: bool },
    /// Playback stopped explicitly or at the end of the playlist.
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlaybackErrorPayload {
    pub song_id: i64,
    pub title: Option<String>,
    pub path: Option<String>,
    /// Why it failed, e.g. a Subsonic server's "Wrong username or password"
    /// (#1163), or `"Station offline"` for live streams. Never holds a signed URL.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OutputFallbackPayload {
    pub requested: AudioBackendKind,
    pub reason: OutputFallbackReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AutoContinueSeed {
    pub playlist_id: i64,
    pub song_ids: Vec<i64>,
}

impl From<(i64, Vec<i64>)> for AutoContinueSeed {
    fn from((playlist_id, song_ids): (i64, Vec<i64>)) -> Self {
        Self {
            playlist_id,
            song_ids,
        }
    }
}

/// Everything an [`AudioEvent`] changed plus the snapshots captured before
/// releasing the `Player` lock so [`publish`] needs no `player` lock.
#[derive(Debug, Clone, Default)]
pub(crate) struct PlaybackChange {
    track_started: bool,
    sync_device_eq: bool,
    scrobble_action: ScrobbleAction,
    /// `None` leaves the OS window title untouched; `Some(playing)` syncs it.
    update_window_title: Option<bool>,
    mirror_media_session: bool,
    /// `None` skips `station-now-playing`; `Some(payload)` emits it (including
    /// `Some(None)` to clear the UI when a station drops or stops).
    station_now_playing: Option<Option<StationNowPlaying>>,
    pipeline: Option<AudioPipelineInfo>,
    emit_pipeline_changed: bool,
    emit_playback_state: bool,
    playback_error: Option<PlaybackErrorPayload>,
    library_changed: bool,
    output_fallback: Option<OutputFallbackPayload>,
    output_unavailable: Option<String>,
    state: Option<PlaybackState>,
    pending_station_click_uuid: Option<String>,
    auto_continue_seed: Option<AutoContinueSeed>,
}

/// Side-effect boundary driven by [`publish`]: implemented by
/// [`TauriPlaybackSink`] in production and by a recording mock in unit tests.
pub(crate) trait PlaybackEventSink {
    async fn sync_device_eq(&self) -> Option<PipelineEqSnapshot>;
    async fn scrobble_now_playing(&self, song: &Song);
    async fn scrobble_state_changed(
        &self,
        song: Option<&Song>,
        is_playing: bool,
        position_nanosec: i64,
        station_now_playing: Option<&StationNowPlaying>,
    );
    async fn scrobble_stopped(&self);
    fn spawn_station_click_report(&self, uuid: String);
    fn sync_window_title(&self, song: Option<&Song>, is_playing: bool);
    fn emit_station_now_playing(&self, now_playing: Option<&StationNowPlaying>);
    fn emit_track_changed(&self, song: Option<&Song>, pipeline: Option<&AudioPipelineInfo>);
    fn emit_pipeline_changed(&self, pipeline: Option<&AudioPipelineInfo>);
    async fn mirror_media_session(&self, state: &PlaybackState);
    fn emit_playback_state(&self, state: &PlaybackState);
    fn emit_playback_error(&self, error: &PlaybackErrorPayload);
    fn emit_library_changed(&self);
    fn emit_output_fallback(&self, payload: &OutputFallbackPayload);
    fn emit_output_unavailable(&self, message: &str);
    fn spawn_auto_continue(&self, seed: &AutoContinueSeed);
}

#[derive(Clone)]
struct TauriPlaybackSink {
    app: AppHandle,
    audio: Arc<Mutex<AudioEngine>>,
    scrobbler: Arc<ScrobblerManager>,
}

impl PlaybackEventSink for TauriPlaybackSink {
    async fn sync_device_eq(&self) -> Option<PipelineEqSnapshot> {
        let app_state = self.app.try_state::<AppState>()?;
        let device_name = self.audio.lock().await.output_info().device_name;
        if crate::commands::equalizer::sync_device_equalizer(&self.app, &app_state, device_name)
            .await
            .is_some()
        {
            return Some(
                self.audio
                    .lock()
                    .await
                    .with_equalizer(|eq| PipelineEqSnapshot::from_equalizer(eq)),
            );
        }
        None
    }

    async fn scrobble_now_playing(&self, song: &Song) {
        self.scrobbler.on_now_playing(song).await;
    }

    async fn scrobble_state_changed(
        &self,
        song: Option<&Song>,
        is_playing: bool,
        position_nanosec: i64,
        station_now_playing: Option<&StationNowPlaying>,
    ) {
        self.scrobbler
            .on_playback_state_changed(song, is_playing, position_nanosec, station_now_playing)
            .await;
    }

    async fn scrobble_stopped(&self) {
        self.scrobbler.on_playback_stopped().await;
    }

    /// Reports the pending Radio Browser station click off the event loop so
    /// the network call never blocks it.
    fn spawn_station_click_report(&self, uuid: String) {
        let Some(app_state) = self.app.try_state::<AppState>() else {
            return;
        };
        let db = app_state.db.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) =
                crate::commands::radio_directory::report_station_click_if_online(&db, uuid).await
            {
                log::debug!("Radio Browser click report failed: {e}");
            }
        });
    }

    fn sync_window_title(&self, song: Option<&Song>, is_playing: bool) {
        sync_window_title(&self.app, song, is_playing);
    }

    /// Tells the frontend which track the playing station announces (#2008),
    /// or `null` once it no longer announces one.
    fn emit_station_now_playing(&self, now_playing: Option<&StationNowPlaying>) {
        let _ = self.app.emit("station-now-playing", now_playing);
    }

    fn emit_track_changed(&self, song: Option<&Song>, pipeline: Option<&AudioPipelineInfo>) {
        let _ = self.app.emit(
            "track-changed",
            serde_json::json!({
                "song": song,
                "pipeline": pipeline,
            }),
        );
    }

    fn emit_pipeline_changed(&self, pipeline: Option<&AudioPipelineInfo>) {
        let _ = self.app.emit("audio-pipeline-changed", pipeline);
    }

    async fn mirror_media_session(&self, state: &PlaybackState) {
        crate::media_session::mirror_state(&self.app, state).await;
    }

    fn emit_playback_state(&self, state: &PlaybackState) {
        let _ = self.app.emit("playback-state", state);
    }

    fn emit_playback_error(&self, error: &PlaybackErrorPayload) {
        let _ = self.app.emit("playback-error", error);
    }

    fn emit_library_changed(&self) {
        let _ = self.app.emit("library-changed", ());
    }

    fn emit_output_fallback(&self, payload: &OutputFallbackPayload) {
        let _ = self.app.emit("audio-output-fallback", payload);
    }

    fn emit_output_unavailable(&self, message: &str) {
        let _ = self.app.emit(
            "audio-output-unavailable",
            serde_json::json!({ "message": message }),
        );
    }

    /// Starts an Auto Continue top-up (#1235) asynchronously outside the
    /// `player` lock so the top-up's `playlists` lock never nests inside it.
    fn spawn_auto_continue(&self, _seed: &AutoContinueSeed) {
        tauri::async_runtime::spawn(crate::continue_mix::maybe_extend(self.app.clone()));
    }
}

/// Formats the OS window title for the current track and playback state.
pub(crate) fn format_window_title(
    song: Option<&Song>,
    is_playing: bool,
    unknown_song: &str,
    app_name: &str,
) -> String {
    match (song, is_playing) {
        (Some(s), true) => {
            let song_title = s.title.as_deref().unwrap_or("").trim();
            let display_title = if song_title.is_empty() {
                let fallback = unknown_song.trim();
                if fallback.is_empty() {
                    "Unknown Song"
                } else {
                    fallback
                }
            } else {
                song_title
            };
            let artist = s.artist.as_deref().unwrap_or("").trim();
            if artist.is_empty() {
                format!("{display_title} - {app_name}")
            } else {
                format!("{display_title} - {artist} - {app_name}")
            }
        }
        _ => app_name.to_string(),
    }
}

/// Keeps the native OS window title in sync with the current track directly
/// from the backend, ensuring taskbar and titlebar stay accurate even when
/// the frontend window is minimized or occluded (#892, #1052, #2317).
pub(crate) fn sync_window_title(app: &AppHandle, song: Option<&Song>, is_playing: bool) {
    if let Some(w) = app.get_webview_window("main") {
        let app_name = if crate::remote_devtools_enabled() {
            "Luminous Debug"
        } else {
            "Luminous"
        };
        let unknown_song = crate::native_labels::unknown_song_label();
        let title = format_window_title(song, is_playing, &unknown_song, app_name);
        let _ = w.set_title(&title);
    }
}

async fn build_track_started(
    p: &mut Player,
    audio: &Arc<Mutex<AudioEngine>>,
    sync_device_eq: bool,
) -> PlaybackChange {
    let state = p.get_state().await;
    let pipeline = {
        let a = audio.lock().await;
        p.get_pipeline_info(&a)
    };
    let pending_station_click_uuid = p.take_pending_station_click_uuid();
    let auto_continue_seed = p.auto_continue_seed().map(AutoContinueSeed::from);
    PlaybackChange {
        track_started: true,
        sync_device_eq,
        scrobble_action: ScrobbleAction::NowPlaying,
        update_window_title: Some(true),
        mirror_media_session: true,
        pipeline,
        emit_playback_state: true,
        state: Some(state),
        pending_station_click_uuid,
        auto_continue_seed,
        ..Default::default()
    }
}

/// Applies `event` to `Player`, captures snapshots needed by downstream
/// listeners, and releases the `player` lock before returning `PlaybackChange`.
///
/// `OutputFallback` and `OutputUnavailable` are informational device events and
/// never acquire the `player` lock.
///
/// Policy for non-obvious events:
/// - `VolumeChanged`: mirrors OS media session and emits `playback-state`,
///   intentionally omitting window-title updates (`update_window_title: None`)
///   because the title bar does not display volume.
/// - `Error`: updates the window title (`update_window_title: Some(is_playing)`)
///   and mirrors the OS media session (`mirror_media_session: true`) so native
///   OS integration stays aligned with the post-error `PlaybackState`.
pub(crate) async fn apply_event(
    event: AudioEvent,
    player: &Arc<Mutex<Player>>,
    audio: &Arc<Mutex<AudioEngine>>,
) -> PlaybackChange {
    match event {
        // Exclusive/ASIO couldn't open; playback goes on through shared output.
        AudioEvent::OutputFallback { requested, reason } => {
            log::warn!("Audio output fell back from {requested}: {reason:?}");
            PlaybackChange {
                output_fallback: Some(OutputFallbackPayload { requested, reason }),
                ..Default::default()
            }
        }
        // The engine already parked the track paused (and sent `Paused`);
        // this is a device problem, not a bad track, so nothing is skipped or
        // counted as a playback error.
        AudioEvent::OutputUnavailable { message } => {
            log::error!("Audio output unavailable: {}", message);
            PlaybackChange {
                output_unavailable: Some(message),
                ..Default::default()
            }
        }
        AudioEvent::Playing { .. } => {
            let mut p = player.lock().await;
            p.reset_playback_errors();
            build_track_started(&mut p, audio, true).await
        }
        AudioEvent::TrackTransitioned { track_id, .. } => {
            let mut p = player.lock().await;
            let _ = p.on_gapless_transition(track_id).await;
            build_track_started(&mut p, audio, false).await
        }
        AudioEvent::Paused => {
            let p = player.lock().await;
            let state = p.get_state().await;
            PlaybackChange {
                scrobble_action: ScrobbleAction::StateChanged { playing: false },
                update_window_title: Some(false),
                mirror_media_session: true,
                emit_playback_state: true,
                state: Some(state),
                ..Default::default()
            }
        }
        AudioEvent::Stopped => {
            let mut p = player.lock().await;
            let cleared_station = p.clear_station_now_playing();
            let state = p.get_state().await;
            PlaybackChange {
                scrobble_action: ScrobbleAction::Stopped,
                update_window_title: Some(false),
                mirror_media_session: true,
                station_now_playing: cleared_station.then_some(None),
                emit_playback_state: true,
                state: Some(state),
                ..Default::default()
            }
        }
        AudioEvent::TrackFinished { .. } => {
            let mut p = player.lock().await;
            let _ = p.on_track_finished().await;
            let state = p.get_state().await;
            let is_playing = state.state == PlayState::Playing;
            PlaybackChange {
                scrobble_action: if is_playing {
                    ScrobbleAction::None
                } else {
                    ScrobbleAction::Stopped
                },
                update_window_title: Some(is_playing),
                mirror_media_session: true,
                emit_playback_state: true,
                state: Some(state),
                ..Default::default()
            }
        }
        AudioEvent::AboutToFinish { .. } => {
            let mut p = player.lock().await;
            // Prime the next track so the engine can hand over gaplessly at the boundary.
            if let Err(e) = p.prepare_gapless_next().await {
                log::warn!("Gapless preload failed: {e}");
            }
            PlaybackChange::default()
        }
        // Never reaches ListenBrainz/Last.fm scrobbling or stats: the announced
        // track is display-only (#2008); Discord Rich Presence still follows it.
        AudioEvent::StreamTitle { track_id, title } => {
            let mut p = player.lock().await;
            if p.note_stream_title(track_id, &title) {
                let state = p.get_state().await;
                let playing = state.state == PlayState::Playing;
                PlaybackChange {
                    scrobble_action: ScrobbleAction::StateChanged { playing },
                    mirror_media_session: true,
                    station_now_playing: Some(state.station_now_playing.clone()),
                    emit_playback_state: true,
                    state: Some(state),
                    ..Default::default()
                }
            } else {
                PlaybackChange::default()
            }
        }
        AudioEvent::PipelineChanged => {
            let p = player.lock().await;
            let pipeline = {
                let a = audio.lock().await;
                p.get_pipeline_info(&a)
            };
            PlaybackChange {
                sync_device_eq: true,
                pipeline,
                emit_pipeline_changed: true,
                ..Default::default()
            }
        }
        AudioEvent::VolumeChanged(vol) => {
            let mut p = player.lock().await;
            p.update_external_volume(vol);
            let state = p.get_state().await;
            // Intentionally omit `update_window_title`: the title bar does not
            // show volume, so only media session and `playback-state` update.
            PlaybackChange {
                mirror_media_session: true,
                emit_playback_state: true,
                state: Some(state),
                ..Default::default()
            }
        }
        AudioEvent::Error { message } => {
            log::error!("Audio engine error: {}", message);
            let mut p = player.lock().await;
            // A dropped station stream no longer announces anything.
            let cleared_station = p.clear_station_now_playing();
            let mut playback_error = None;
            let mut library_changed = false;

            if p.try_heal_and_retry_current_track().await {
                // Stale-cased path (Linux/case-sensitive filesystem quirk) —
                // repointed and retried in place, nothing to surface to the user.
                library_changed = true;
            } else {
                let outcome = p.note_playback_error();
                if let Some(song) = &outcome.failed_song {
                    let display_message = if crate::audio::is_live_song(song) {
                        "Station offline".to_string()
                    } else {
                        message
                    };
                    playback_error = Some(PlaybackErrorPayload {
                        song_id: song.id,
                        title: song.title.clone(),
                        path: song.path.clone(),
                        message: display_message,
                    });
                }
                if outcome.flagged_unavailable {
                    library_changed = true;
                }

                if outcome.should_stop {
                    log::error!(
                        "Stopping playback after {} consecutive audio errors \
                         — likely a disconnected drive or dead playlist",
                        crate::player::MAX_CONSECUTIVE_PLAYBACK_ERRORS
                    );
                    let _ = p.stop().await;
                } else {
                    let _ = p.next_track().await;
                }
            }
            let state = p.get_state().await;
            let is_playing = state.state == PlayState::Playing;
            // Sync window title and mirror media session to match the post-error state.
            PlaybackChange {
                update_window_title: Some(is_playing),
                mirror_media_session: true,
                station_now_playing: cleared_station.then_some(None),
                emit_playback_state: true,
                playback_error,
                library_changed,
                state: Some(state),
                ..Default::default()
            }
        }
    }
}

/// Fans out a `PlaybackChange` to every listener in one fixed order without
/// holding the `Player` lock:
/// 1. Device EQ sync (updating the captured pipeline snapshot if EQ changed)
/// 2. Scrobbler / Discord Rich Presence
/// 3. Radio Browser station click report
/// 4. OS window title (skipped on `VolumeChanged`, updated on `Error`)
/// 5. `station-now-playing`
/// 6. `track-changed` / `audio-pipeline-changed`
/// 7. OS media session (`mirror_state`, updated on `Error` as well)
/// 8. `playback-state`
/// 9. `playback-error` / `library-changed` / `audio-output-fallback` / `audio-output-unavailable`
/// 10. Auto Continue top-up (spawns outside `player` so `playlists` never nests under it)
pub(crate) async fn publish<S: PlaybackEventSink>(sink: &S, mut change: PlaybackChange) {
    // 1. Device EQ sync
    if change.sync_device_eq {
        if let Some(eq_snapshot) = sink.sync_device_eq().await {
            if let Some(ref mut pipeline) = change.pipeline {
                eq_snapshot.apply_to(pipeline);
            }
        }
    }

    // 2. Scrobbler
    match change.scrobble_action {
        ScrobbleAction::None => {}
        ScrobbleAction::NowPlaying => {
            if let Some(state) = change.state.as_ref() {
                if let Some(song) = state.current_song.as_ref() {
                    sink.scrobble_now_playing(song).await;
                    sink.scrobble_state_changed(
                        Some(song),
                        true,
                        state.position_nanosec,
                        state.station_now_playing.as_ref(),
                    )
                    .await;
                }
            }
        }
        ScrobbleAction::StateChanged { playing } => {
            if let Some(state) = change.state.as_ref() {
                sink.scrobble_state_changed(
                    state.current_song.as_ref(),
                    playing,
                    state.position_nanosec,
                    state.station_now_playing.as_ref(),
                )
                .await;
            }
        }
        ScrobbleAction::Stopped => {
            sink.scrobble_stopped().await;
        }
    }

    // 3. Station click report
    if let Some(uuid) = change.pending_station_click_uuid {
        sink.spawn_station_click_report(uuid);
    }

    // 4. OS window title
    if let Some(is_playing) = change.update_window_title {
        let song = is_playing
            .then(|| change.state.as_ref().and_then(|s| s.current_song.as_ref()))
            .flatten();
        sink.sync_window_title(song, is_playing);
    }

    // 5. station-now-playing
    if let Some(ref now_playing) = change.station_now_playing {
        sink.emit_station_now_playing(now_playing.as_ref());
    }

    // 6. track-changed / audio-pipeline-changed
    if change.track_started {
        let song = change.state.as_ref().and_then(|s| s.current_song.as_ref());
        sink.emit_track_changed(song, change.pipeline.as_ref());
    }
    if change.emit_pipeline_changed {
        sink.emit_pipeline_changed(change.pipeline.as_ref());
    }

    // 7. OS media session
    if change.mirror_media_session {
        if let Some(ref state) = change.state {
            sink.mirror_media_session(state).await;
        }
    }

    // 8. playback-state
    if change.emit_playback_state {
        if let Some(ref state) = change.state {
            sink.emit_playback_state(state);
        }
    }

    // 9. playback-error / library-changed / output alerts
    if let Some(ref error) = change.playback_error {
        sink.emit_playback_error(error);
    }
    if change.library_changed {
        sink.emit_library_changed();
    }
    if let Some(ref fallback) = change.output_fallback {
        sink.emit_output_fallback(fallback);
    }
    if let Some(ref message) = change.output_unavailable {
        sink.emit_output_unavailable(message);
    }

    // 10. Auto Continue
    if let Some(ref seed) = change.auto_continue_seed {
        sink.spawn_auto_continue(seed);
    }
}

/// Spawns the OS thread that drains `AudioEngine`'s event channel and turns
/// each `AudioEvent` into player-state transitions, OS media-session
/// mirroring, and frontend events. A blocking OS thread rather than a Tokio
/// task since it blocks on `rx.iter()`.
pub(crate) fn spawn(
    app_handle: AppHandle,
    audio: Arc<Mutex<AudioEngine>>,
    player: Arc<Mutex<Player>>,
    scrobbler: Arc<ScrobblerManager>,
) {
    std::thread::Builder::new()
        .name("luminous-events".to_string())
        .spawn(move || {
            let rx = {
                let engine = tauri::async_runtime::block_on(async { audio.lock().await });
                engine.take_events()
            };
            let Some(rx) = rx else {
                log::error!("Audio event stream was already taken; player events won't run");
                return;
            };

            let sink = TauriPlaybackSink {
                app: app_handle,
                audio: Arc::clone(&audio),
                scrobbler,
            };

            for event in rx.iter() {
                log::trace!("Received audio event: {:?}", event);
                let player = Arc::clone(&player);
                let audio = Arc::clone(&audio);
                let sink = sink.clone();
                tauri::async_runtime::block_on(async move {
                    let change = apply_event(event, &player, &audio).await;
                    publish(&sink, change).await;
                });
            }
        })
        .expect("failed to spawn event thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{FileType, PlaylistItem, RepeatMode, SongSource};
    use parking_lot::Mutex as SyncMutex;

    struct TestDir(Option<tempfile::TempDir>);

    impl Drop for TestDir {
        fn drop(&mut self) {
            let path = self.0.take().unwrap().keep();
            for _ in 0..100 {
                if std::fs::remove_dir_all(&path).is_ok() || !path.exists() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    fn setup_test_db() -> (TestDir, Arc<Database>) {
        let temp_dir = tempfile::Builder::new()
            .prefix("luminous_playback_events_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        (TestDir(Some(temp_dir)), Arc::new(db))
    }

    fn insert_test_songs(db: &Database, rows: &[(i64, SongSource, FileType, &str, &str, &str)]) {
        let conn = db.pool.get().unwrap();
        for &(id, source, filetype, path, title, artist) in rows {
            conn.execute(
                "INSERT INTO songs (id, source, filetype, path, title, artist, album, length_nanosec)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Test Album', 180000000000)",
                rusqlite::params![id, source as i32, filetype as i32, path, title, artist],
            )
            .unwrap();
        }
    }

    fn load_song(db: &Database, id: i64) -> Song {
        let conn = db.pool.get().unwrap();
        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );
        conn.query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
            .unwrap()
    }

    #[derive(Debug, Clone, PartialEq)]
    enum RecordedCall {
        SyncDeviceEq,
        ScrobbleNowPlaying {
            song_id: i64,
        },
        ScrobbleStateChanged {
            song_id: Option<i64>,
            is_playing: bool,
            station_now_playing: Option<StationNowPlaying>,
        },
        ScrobbleStopped,
        StationClickReport {
            uuid: String,
        },
        WindowTitle {
            song_id: Option<i64>,
            is_playing: bool,
        },
        StationNowPlaying(Option<StationNowPlaying>),
        TrackChanged {
            song_id: Option<i64>,
            has_pipeline: bool,
            eq_enabled: Option<bool>,
        },
        PipelineChanged {
            has_pipeline: bool,
            eq_enabled: Option<bool>,
        },
        MediaSession {
            state: PlayState,
            song_id: Option<i64>,
            volume: f32,
            station_now_playing: Option<StationNowPlaying>,
        },
        PlaybackState {
            state: PlayState,
            song_id: Option<i64>,
            volume: f32,
            station_now_playing: Option<StationNowPlaying>,
        },
        PlaybackError(PlaybackErrorPayload),
        LibraryChanged,
        OutputFallback(OutputFallbackPayload),
        OutputUnavailable(String),
        AutoContinue(AutoContinueSeed),
    }

    #[derive(Default)]
    struct MockSink {
        calls: SyncMutex<Vec<RecordedCall>>,
        eq_patch: SyncMutex<Option<PipelineEqSnapshot>>,
    }

    impl MockSink {
        fn take_calls(&self) -> Vec<RecordedCall> {
            std::mem::take(&mut *self.calls.lock())
        }
    }

    impl PlaybackEventSink for MockSink {
        async fn sync_device_eq(&self) -> Option<PipelineEqSnapshot> {
            self.calls.lock().push(RecordedCall::SyncDeviceEq);
            self.eq_patch.lock().clone()
        }

        async fn scrobble_now_playing(&self, song: &Song) {
            self.calls
                .lock()
                .push(RecordedCall::ScrobbleNowPlaying { song_id: song.id });
        }

        async fn scrobble_state_changed(
            &self,
            song: Option<&Song>,
            is_playing: bool,
            _position_nanosec: i64,
            station_now_playing: Option<&StationNowPlaying>,
        ) {
            self.calls.lock().push(RecordedCall::ScrobbleStateChanged {
                song_id: song.map(|s| s.id),
                is_playing,
                station_now_playing: station_now_playing.cloned(),
            });
        }

        async fn scrobble_stopped(&self) {
            self.calls.lock().push(RecordedCall::ScrobbleStopped);
        }

        fn spawn_station_click_report(&self, uuid: String) {
            self.calls
                .lock()
                .push(RecordedCall::StationClickReport { uuid });
        }

        fn sync_window_title(&self, song: Option<&Song>, is_playing: bool) {
            self.calls.lock().push(RecordedCall::WindowTitle {
                song_id: song.map(|s| s.id),
                is_playing,
            });
        }

        fn emit_station_now_playing(&self, now_playing: Option<&StationNowPlaying>) {
            self.calls
                .lock()
                .push(RecordedCall::StationNowPlaying(now_playing.cloned()));
        }

        fn emit_track_changed(&self, song: Option<&Song>, pipeline: Option<&AudioPipelineInfo>) {
            self.calls.lock().push(RecordedCall::TrackChanged {
                song_id: song.map(|s| s.id),
                has_pipeline: pipeline.is_some(),
                eq_enabled: pipeline.map(|p| p.eq_enabled),
            });
        }

        fn emit_pipeline_changed(&self, pipeline: Option<&AudioPipelineInfo>) {
            self.calls.lock().push(RecordedCall::PipelineChanged {
                has_pipeline: pipeline.is_some(),
                eq_enabled: pipeline.map(|p| p.eq_enabled),
            });
        }

        async fn mirror_media_session(&self, state: &PlaybackState) {
            self.calls.lock().push(RecordedCall::MediaSession {
                state: state.state,
                song_id: state.current_song.as_ref().map(|s| s.id),
                volume: state.volume,
                station_now_playing: state.station_now_playing.clone(),
            });
        }

        fn emit_playback_state(&self, state: &PlaybackState) {
            self.calls.lock().push(RecordedCall::PlaybackState {
                state: state.state,
                song_id: state.current_song.as_ref().map(|s| s.id),
                volume: state.volume,
                station_now_playing: state.station_now_playing.clone(),
            });
        }

        fn emit_playback_error(&self, error: &PlaybackErrorPayload) {
            self.calls
                .lock()
                .push(RecordedCall::PlaybackError(error.clone()));
        }

        fn emit_library_changed(&self) {
            self.calls.lock().push(RecordedCall::LibraryChanged);
        }

        fn emit_output_fallback(&self, payload: &OutputFallbackPayload) {
            self.calls
                .lock()
                .push(RecordedCall::OutputFallback(payload.clone()));
        }

        fn emit_output_unavailable(&self, message: &str) {
            self.calls
                .lock()
                .push(RecordedCall::OutputUnavailable(message.to_string()));
        }

        fn spawn_auto_continue(&self, seed: &AutoContinueSeed) {
            self.calls
                .lock()
                .push(RecordedCall::AutoContinue(seed.clone()));
        }
    }

    async fn dispatch(
        event: AudioEvent,
        player: &Arc<Mutex<Player>>,
        audio: &Arc<Mutex<AudioEngine>>,
        sink: &MockSink,
    ) -> Vec<RecordedCall> {
        let change = apply_event(event, player, audio).await;
        // Every arm must release `player` before returning `PlaybackChange`.
        assert!(
            player.try_lock().is_ok(),
            "player lock must be dropped before publish"
        );
        publish(sink, change).await;
        sink.take_calls()
    }

    #[test]
    fn test_format_window_title() {
        let song = Song {
            title: Some("Starboy".to_string()),
            artist: Some("The Weeknd".to_string()),
            ..Default::default()
        };

        // Paused / stopped / no song always revert to the app name.
        assert_eq!(
            format_window_title(Some(&song), false, "Chanson inconnue", "Luminous"),
            "Luminous"
        );
        assert_eq!(
            format_window_title(None, true, "Chanson inconnue", "Luminous Debug"),
            "Luminous Debug"
        );

        // Playing with title and artist.
        assert_eq!(
            format_window_title(Some(&song), true, "Chanson inconnue", "Luminous"),
            "Starboy - The Weeknd - Luminous"
        );

        // Playing with title and blank/missing artist.
        let no_artist = Song {
            title: Some("Prelude".to_string()),
            artist: Some("   ".to_string()),
            ..Default::default()
        };
        assert_eq!(
            format_window_title(Some(&no_artist), true, "Chanson inconnue", "Luminous"),
            "Prelude - Luminous"
        );

        // Playing with missing/whitespace title uses localized fallback.
        let untitled = Song {
            title: Some("   ".to_string()),
            artist: Some("Daft Punk".to_string()),
            ..Default::default()
        };
        assert_eq!(
            format_window_title(Some(&untitled), true, "Chanson inconnue", "Luminous"),
            "Chanson inconnue - Daft Punk - Luminous"
        );

        // Blank unknown_song fallback still falls back to English "Unknown Song".
        let untitled_no_artist = Song {
            title: None,
            artist: None,
            ..Default::default()
        };
        assert_eq!(
            format_window_title(Some(&untitled_no_artist), true, "   ", "Luminous Debug"),
            "Unknown Song - Luminous Debug"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn playing_paused_and_track_transitioned_share_track_started_fanout_and_order() {
        let (_dir, db) = setup_test_db();
        insert_test_songs(
            &db,
            &[
                (
                    1,
                    SongSource::RadioBrowser,
                    FileType::Stream,
                    "radio://station-uuid-1",
                    "Groove Salad",
                    "SomaFM",
                ),
                (
                    2,
                    SongSource::LocalFile,
                    FileType::Flac,
                    "/missing/track2.flac",
                    "Track Two",
                    "Artist Two",
                ),
            ],
        );
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut p = Player::new(Arc::clone(&db), Arc::clone(&audio));
        let items = vec![
            PlaylistItem::new_song(7, 0, load_song(&db, 1)),
            PlaylistItem::new_song(7, 1, load_song(&db, 2)),
        ];
        p.set_repeat_mode(RepeatMode::Off);
        p.set_auto_continue(true);
        p.play_playlist(items, 0, 7, None).await.unwrap();
        let player = Arc::new(Mutex::new(p));
        let sink = MockSink::default();

        // 1. Playing triggers device EQ sync + full track-started fan-out in order.
        let calls = dispatch(AudioEvent::Playing { track_id: 1 }, &player, &audio, &sink).await;
        assert_eq!(
            calls,
            vec![
                RecordedCall::SyncDeviceEq,
                RecordedCall::ScrobbleNowPlaying { song_id: 1 },
                RecordedCall::ScrobbleStateChanged {
                    song_id: Some(1),
                    is_playing: true,
                    station_now_playing: None,
                },
                RecordedCall::StationClickReport {
                    uuid: "station-uuid-1".to_string(),
                },
                RecordedCall::WindowTitle {
                    song_id: Some(1),
                    is_playing: true,
                },
                RecordedCall::TrackChanged {
                    song_id: Some(1),
                    has_pipeline: true,
                    eq_enabled: Some(false),
                },
                RecordedCall::MediaSession {
                    state: PlayState::Stopped,
                    song_id: Some(1),
                    volume: 1.0,
                    station_now_playing: None,
                },
                RecordedCall::PlaybackState {
                    state: PlayState::Stopped,
                    song_id: Some(1),
                    volume: 1.0,
                    station_now_playing: None,
                },
                RecordedCall::AutoContinue(AutoContinueSeed {
                    playlist_id: 7,
                    song_ids: vec![1],
                }),
            ]
        );

        // 2. Paused updates scrobbler, window title, media session, and playback-state.
        let calls = dispatch(AudioEvent::Paused, &player, &audio, &sink).await;
        assert_eq!(
            calls,
            vec![
                RecordedCall::ScrobbleStateChanged {
                    song_id: Some(1),
                    is_playing: false,
                    station_now_playing: None,
                },
                RecordedCall::WindowTitle {
                    song_id: None,
                    is_playing: false,
                },
                RecordedCall::MediaSession {
                    state: PlayState::Stopped,
                    song_id: Some(1),
                    volume: 1.0,
                    station_now_playing: None,
                },
                RecordedCall::PlaybackState {
                    state: PlayState::Stopped,
                    song_id: Some(1),
                    volume: 1.0,
                    station_now_playing: None,
                },
            ]
        );

        // 3. TrackTransitioned advances via on_gapless_transition and shares the
        // track-started fan-out (without SyncDeviceEq).
        let calls = dispatch(
            AudioEvent::TrackTransitioned {
                finished_track_id: 1,
                track_id: 2,
            },
            &player,
            &audio,
            &sink,
        )
        .await;
        assert_eq!(
            calls,
            vec![
                RecordedCall::ScrobbleNowPlaying { song_id: 2 },
                RecordedCall::ScrobbleStateChanged {
                    song_id: Some(2),
                    is_playing: true,
                    station_now_playing: None,
                },
                RecordedCall::WindowTitle {
                    song_id: Some(2),
                    is_playing: true,
                },
                RecordedCall::TrackChanged {
                    song_id: Some(2),
                    has_pipeline: true,
                    eq_enabled: Some(false),
                },
                RecordedCall::MediaSession {
                    state: PlayState::Stopped,
                    song_id: Some(2),
                    volume: 1.0,
                    station_now_playing: None,
                },
                RecordedCall::PlaybackState {
                    state: PlayState::Stopped,
                    song_id: Some(2),
                    volume: 1.0,
                    station_now_playing: None,
                },
                RecordedCall::AutoContinue(AutoContinueSeed {
                    playlist_id: 7,
                    song_ids: vec![2, 1],
                }),
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stream_title_pipeline_changed_about_to_finish_and_stopped_behavior() {
        let (_dir, db) = setup_test_db();
        insert_test_songs(
            &db,
            &[(
                1,
                SongSource::RadioBrowser,
                FileType::Stream,
                "radio://station-1",
                "Groove Salad",
                "SomaFM",
            )],
        );
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut p = Player::new(Arc::clone(&db), Arc::clone(&audio));
        p.play_playlist(
            vec![PlaylistItem::new_song(10, 0, load_song(&db, 1))],
            0,
            10,
            None,
        )
        .await
        .unwrap();
        let player = Arc::new(Mutex::new(p));
        let sink = MockSink::default();

        // StreamTitle emits station-now-playing, Discord state, media session, playback-state (no title).
        let np = StationNowPlaying {
            artist: Some("Air".to_string()),
            title: "Alone in Kyoto".to_string(),
        };
        let calls = dispatch(
            AudioEvent::StreamTitle {
                track_id: 1,
                title: "Air - Alone in Kyoto".to_string(),
            },
            &player,
            &audio,
            &sink,
        )
        .await;
        assert_eq!(
            calls,
            vec![
                RecordedCall::ScrobbleStateChanged {
                    song_id: Some(1),
                    is_playing: false,
                    station_now_playing: Some(np.clone()),
                },
                RecordedCall::StationNowPlaying(Some(np.clone())),
                RecordedCall::MediaSession {
                    state: PlayState::Stopped,
                    song_id: Some(1),
                    volume: 1.0,
                    station_now_playing: Some(np.clone()),
                },
                RecordedCall::PlaybackState {
                    state: PlayState::Stopped,
                    song_id: Some(1),
                    volume: 1.0,
                    station_now_playing: Some(np.clone()),
                },
            ]
        );

        // Duplicate or mismatched-track StreamTitle is a no-op.
        assert!(dispatch(
            AudioEvent::StreamTitle {
                track_id: 1,
                title: "Air - Alone in Kyoto".to_string(),
            },
            &player,
            &audio,
            &sink,
        )
        .await
        .is_empty());
        assert!(dispatch(
            AudioEvent::StreamTitle {
                track_id: 99,
                title: "Boards of Canada - Roygbiv".to_string(),
            },
            &player,
            &audio,
            &sink,
        )
        .await
        .is_empty());

        // PipelineChanged syncs device EQ and patches pipeline before emitting audio-pipeline-changed.
        *sink.eq_patch.lock() = Some(PipelineEqSnapshot {
            enabled: true,
            mode: "Parametric",
            preamp_db: -2.0,
            active_bands_count: 2,
        });
        let calls = dispatch(AudioEvent::PipelineChanged, &player, &audio, &sink).await;
        assert_eq!(
            calls,
            vec![
                RecordedCall::SyncDeviceEq,
                RecordedCall::PipelineChanged {
                    has_pipeline: true,
                    eq_enabled: Some(true),
                },
            ]
        );
        *sink.eq_patch.lock() = None;

        // AboutToFinish arms gapless preload without emitting downstream notifications.
        assert!(dispatch(
            AudioEvent::AboutToFinish { track_id: 1 },
            &player,
            &audio,
            &sink
        )
        .await
        .is_empty());

        // Stopped clears announced track and emits ScrobbleStopped -> WindowTitle -> StationNowPlaying(None).
        let calls = dispatch(AudioEvent::Stopped, &player, &audio, &sink).await;
        assert_eq!(
            calls,
            vec![
                RecordedCall::ScrobbleStopped,
                RecordedCall::WindowTitle {
                    song_id: None,
                    is_playing: false,
                },
                RecordedCall::StationNowPlaying(None),
                RecordedCall::MediaSession {
                    state: PlayState::Stopped,
                    song_id: Some(1),
                    volume: 1.0,
                    station_now_playing: None,
                },
                RecordedCall::PlaybackState {
                    state: PlayState::Stopped,
                    song_id: Some(1),
                    volume: 1.0,
                    station_now_playing: None,
                },
            ]
        );

        // A second Stopped when nothing was announced does not re-emit station-now-playing.
        let calls = dispatch(AudioEvent::Stopped, &player, &audio, &sink).await;
        assert!(!calls
            .iter()
            .any(|c| matches!(c, RecordedCall::StationNowPlaying(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn volume_changed_skips_window_title_and_mirrors_media_session() {
        let (_dir, db) = setup_test_db();
        insert_test_songs(
            &db,
            &[(
                1,
                SongSource::LocalFile,
                FileType::Mp3,
                "/missing/song.mp3",
                "Song One",
                "Artist One",
            )],
        );
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut p = Player::new(Arc::clone(&db), Arc::clone(&audio));
        p.play_playlist(
            vec![PlaylistItem::new_song(1, 0, load_song(&db, 1))],
            0,
            1,
            None,
        )
        .await
        .unwrap();
        let player = Arc::new(Mutex::new(p));
        let sink = MockSink::default();

        audio.lock().await.set_volume(0.42);
        let calls = dispatch(AudioEvent::VolumeChanged(0.42), &player, &audio, &sink).await;

        assert_eq!(player.lock().await.volume, 0.42);
        assert_eq!(
            calls,
            vec![
                RecordedCall::MediaSession {
                    state: PlayState::Stopped,
                    song_id: Some(1),
                    volume: 0.42,
                    station_now_playing: None,
                },
                RecordedCall::PlaybackState {
                    state: PlayState::Stopped,
                    song_id: Some(1),
                    volume: 0.42,
                    station_now_playing: None,
                },
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn error_updates_window_title_mirrors_media_session_and_resets_after_playing() {
        let (_dir, db) = setup_test_db();
        insert_test_songs(
            &db,
            &[
                (
                    1,
                    SongSource::RadioBrowser,
                    FileType::Stream,
                    "radio://dead-station",
                    "Dead Station",
                    "Radio",
                ),
                (
                    2,
                    SongSource::LocalFile,
                    FileType::Mp3,
                    "/missing/track2.mp3",
                    "Missing Two",
                    "Artist",
                ),
                (
                    3,
                    SongSource::LocalFile,
                    FileType::Mp3,
                    "/missing/track3.mp3",
                    "Missing Three",
                    "Artist",
                ),
                (
                    4,
                    SongSource::LocalFile,
                    FileType::Mp3,
                    "/missing/track4.mp3",
                    "Missing Four",
                    "Artist",
                ),
                (
                    5,
                    SongSource::LocalFile,
                    FileType::Mp3,
                    "/missing/track5.mp3",
                    "Missing Five",
                    "Artist",
                ),
                (
                    6,
                    SongSource::LocalFile,
                    FileType::Mp3,
                    "/missing/track6.mp3",
                    "Missing Six",
                    "Artist",
                ),
                (
                    7,
                    SongSource::LocalFile,
                    FileType::Mp3,
                    "/missing/track7.mp3",
                    "Missing Seven",
                    "Artist",
                ),
            ],
        );
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut p = Player::new(Arc::clone(&db), Arc::clone(&audio));
        p.set_repeat_mode(RepeatMode::Off);
        p.play_playlist(
            (1..=7)
                .map(|id| PlaylistItem::new_song(5, (id - 1) as i32, load_song(&db, id)))
                .collect(),
            0,
            5,
            None,
        )
        .await
        .unwrap();
        assert!(p.note_stream_title(1, "Artist - Track"));
        let player = Arc::new(Mutex::new(p));
        let sink = MockSink::default();

        // Error on live station: clears station_now_playing, maps message to "Station offline",
        // flags unavailable, advances to song 2, AND updates window title + media session.
        let calls = dispatch(
            AudioEvent::Error {
                message: "connection refused".to_string(),
            },
            &player,
            &audio,
            &sink,
        )
        .await;
        assert_eq!(
            calls,
            vec![
                RecordedCall::WindowTitle {
                    song_id: None,
                    is_playing: false,
                },
                RecordedCall::StationNowPlaying(None),
                RecordedCall::MediaSession {
                    state: PlayState::Stopped,
                    song_id: Some(2),
                    volume: 1.0,
                    station_now_playing: None,
                },
                RecordedCall::PlaybackState {
                    state: PlayState::Stopped,
                    song_id: Some(2),
                    volume: 1.0,
                    station_now_playing: None,
                },
                RecordedCall::PlaybackError(PlaybackErrorPayload {
                    song_id: 1,
                    title: Some("Dead Station".to_string()),
                    path: Some("radio://dead-station".to_string()),
                    message: "Station offline".to_string(),
                }),
                RecordedCall::LibraryChanged,
            ]
        );

        // Confirm Playing resets consecutive error count (error #1 above + Playing ->
        // next 4 errors stay below MAX_CONSECUTIVE_PLAYBACK_ERRORS = 5 and don't stop early).
        let _ = dispatch(AudioEvent::Playing { track_id: 2 }, &player, &audio, &sink).await;
        for _ in 0..(crate::player::MAX_CONSECUTIVE_PLAYBACK_ERRORS - 1) {
            let _ = dispatch(
                AudioEvent::Error {
                    message: "open failed".to_string(),
                },
                &player,
                &audio,
                &sink,
            )
            .await;
            assert!(
                player.lock().await.current_song.is_some(),
                "error count below ceiling should advance rather than stop"
            );
        }

        // Hitting MAX_CONSECUTIVE_PLAYBACK_ERRORS stops playback and updates title/media session.
        let calls = dispatch(
            AudioEvent::Error {
                message: "open failed".to_string(),
            },
            &player,
            &audio,
            &sink,
        )
        .await;
        assert!(player.lock().await.current_song.is_none());
        assert!(calls.contains(&RecordedCall::WindowTitle {
            song_id: None,
            is_playing: false,
        }));
        assert!(calls.contains(&RecordedCall::MediaSession {
            state: PlayState::Stopped,
            song_id: None,
            volume: 1.0,
            station_now_playing: None,
        }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn track_finished_stops_scrobbler_when_playback_ends() {
        let (_dir, db) = setup_test_db();
        insert_test_songs(
            &db,
            &[(
                1,
                SongSource::LocalFile,
                FileType::Mp3,
                "/missing/only.mp3",
                "Only Song",
                "Artist",
            )],
        );
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut p = Player::new(Arc::clone(&db), Arc::clone(&audio));
        p.play_playlist(
            vec![PlaylistItem::new_song(1, 0, load_song(&db, 1))],
            0,
            1,
            None,
        )
        .await
        .unwrap();
        let player = Arc::new(Mutex::new(p));
        let sink = MockSink::default();

        let calls = dispatch(
            AudioEvent::TrackFinished { track_id: 1 },
            &player,
            &audio,
            &sink,
        )
        .await;
        assert_eq!(
            calls,
            vec![
                RecordedCall::ScrobbleStopped,
                RecordedCall::WindowTitle {
                    song_id: None,
                    is_playing: false,
                },
                RecordedCall::MediaSession {
                    state: PlayState::Stopped,
                    song_id: None,
                    volume: 1.0,
                    station_now_playing: None,
                },
                RecordedCall::PlaybackState {
                    state: PlayState::Stopped,
                    song_id: None,
                    volume: 1.0,
                    station_now_playing: None,
                },
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn output_fallback_and_output_unavailable_skip_player_lock() {
        let (_dir, db) = setup_test_db();
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let player = Arc::new(Mutex::new(Player::new(db, Arc::clone(&audio))));
        let sink = MockSink::default();

        // Hold the player lock across both calls: either call trying to acquire
        // `player` would deadlock here.
        let _held_player_lock = player.lock().await;

        let fallback_change = apply_event(
            AudioEvent::OutputFallback {
                requested: AudioBackendKind::WasapiExclusive,
                reason: OutputFallbackReason::DeviceInUse,
            },
            &player,
            &audio,
        )
        .await;
        publish(&sink, fallback_change).await;

        let unavailable_change = apply_event(
            AudioEvent::OutputUnavailable {
                message: "No audio output device".to_string(),
            },
            &player,
            &audio,
        )
        .await;
        publish(&sink, unavailable_change).await;

        assert_eq!(
            sink.take_calls(),
            vec![
                RecordedCall::OutputFallback(OutputFallbackPayload {
                    requested: AudioBackendKind::WasapiExclusive,
                    reason: OutputFallbackReason::DeviceInUse,
                }),
                RecordedCall::OutputUnavailable("No audio output device".to_string()),
            ]
        );
    }
}
