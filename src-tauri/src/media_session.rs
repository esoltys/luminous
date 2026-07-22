//! OS "Now Playing" integration (SMTC on Windows, MPRIS2 on Linux,
//! MPNowPlayingInfoCenter on macOS) via the `souvlaki` crate. — #80
//!
//! `souvlaki::MediaControls` is thread-affine on at least one supported
//! platform (Windows drives it through WinRT/COM objects created for a
//! specific HWND), so it is created and driven entirely from one dedicated
//! OS thread. Outbound updates (metadata/playback state) are sent to that
//! thread through a channel via [`MediaSessionHandle`]; inbound OS control
//! events (play/pause/next/…) are routed straight from souvlaki's callback
//! onto the Tauri async runtime, reusing the same `state.player` command
//! paths the IPC handlers and global media-key shortcuts use.

use crate::models::{PlayState, PlaybackState};
use crate::AppState;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Default hop size for a bare `Seek` event (no explicit duration), mirroring
/// typical desktop media-key seek granularity.
const DEFAULT_SEEK_STEP: Duration = Duration::from_secs(10);

enum Command {
    Update {
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
        duration: Option<Duration>,
        cover_url: Option<String>,
        status: PlayState,
        position: Duration,
    },
}

/// Handle for pushing "Now Playing" updates to the dedicated media-session
/// thread from anywhere in the app.
#[derive(Clone)]
pub struct MediaSessionHandle {
    tx: mpsc::Sender<Command>,
}

impl MediaSessionHandle {
    fn send_update(&self, playback: &PlaybackState, cover_path: Option<PathBuf>) {
        let song = playback.current_song.as_ref();
        let cover_url = cover_path
            .filter(|p| p.exists())
            .map(|p| format!("file://{}", p.display()));
        let cmd = Command::Update {
            title: song.map(|s| s.display_title().to_string()),
            artist: song.and_then(|s| s.artist.clone()),
            album: song.and_then(|s| s.album.clone()),
            duration: song
                .and_then(|s| s.length_nanosec)
                .filter(|&ns| ns > 0)
                .map(|ns| Duration::from_nanos(ns as u64)),
            cover_url,
            status: playback.state,
            position: Duration::from_nanos(playback.position_nanosec.max(0) as u64),
        };
        let _ = self.tx.send(cmd);
    }
}

/// Recompute the OS media session snapshot from an already-fetched
/// [`PlaybackState`] and push it. No-op if the platform integration failed
/// to initialize (unsupported desktop, no session bus, etc).
///
/// Takes the snapshot by reference rather than re-locking `state.player`
/// itself, since callers typically already hold that lock when they learn
/// the new state.
pub async fn mirror_state(app: &AppHandle, playback: &PlaybackState) {
    let state = app.state::<AppState>();
    let Some(handle) = state.media_session.as_ref() else {
        return;
    };
    let cover_path = playback.current_song.as_ref().and_then(|song| {
        state
            .cover_manager
            .get_cover_art_path(song.id)
            .ok()
            .flatten()
    });
    handle.send_update(playback, cover_path);
}

/// Start the dedicated media-session OS thread. Returns `None` (logging a
/// warning) if the platform integration can't be initialized — callers
/// should treat this as a best-effort feature, not a hard requirement.
pub fn spawn(
    app_handle: AppHandle,
    hwnd: Option<*mut std::ffi::c_void>,
) -> Option<MediaSessionHandle> {
    // `*mut c_void` isn't `Send`, but we only ever read it once, on the
    // thread we're about to spawn, to construct the platform config.
    struct SendableHwnd(Option<*mut std::ffi::c_void>);
    unsafe impl Send for SendableHwnd {}
    let hwnd = SendableHwnd(hwnd);

    let (tx, rx) = mpsc::channel::<Command>();
    let (ready_tx, ready_rx) = mpsc::channel::<bool>();

    let spawn_result = std::thread::Builder::new()
        .name("luminous-media-session".to_string())
        .spawn(move || {
            let hwnd = hwnd;
            let config = PlatformConfig {
                dbus_name: "com.luminous.player",
                display_name: "Luminous",
                hwnd: hwnd.0,
            };

            let mut controls = match MediaControls::new(config) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("Failed to initialize OS media session: {e:?}");
                    let _ = ready_tx.send(false);
                    return;
                }
            };

            let event_app = app_handle.clone();
            if let Err(e) = controls.attach(move |event| handle_event(event_app.clone(), event)) {
                log::warn!("Failed to attach OS media session event handler: {e:?}");
                let _ = ready_tx.send(false);
                return;
            }

            let _ = ready_tx.send(true);

            for cmd in rx {
                match cmd {
                    Command::Update {
                        title,
                        artist,
                        album,
                        duration,
                        cover_url,
                        status,
                        position,
                    } => {
                        let _ = controls.set_metadata(MediaMetadata {
                            title: title.as_deref(),
                            artist: artist.as_deref(),
                            album: album.as_deref(),
                            cover_url: cover_url.as_deref(),
                            duration,
                        });
                        let progress = Some(MediaPosition(position));
                        let playback = match status {
                            PlayState::Playing => MediaPlayback::Playing { progress },
                            PlayState::Paused => MediaPlayback::Paused { progress },
                            PlayState::Stopped => MediaPlayback::Stopped,
                        };
                        let _ = controls.set_playback(playback);
                    }
                }
            }

            let _ = controls.detach();
        });

    if let Err(e) = spawn_result {
        log::warn!("Failed to spawn OS media session thread: {e}");
        return None;
    }

    // A stuck/absent session bus should fail fast inside souvlaki; this
    // timeout just guarantees app startup can't hang on it.
    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(true) => Some(MediaSessionHandle { tx }),
        _ => None,
    }
}

/// Route an inbound OS media control event onto the same `state.player`
/// command paths the IPC handlers and global media-key shortcuts use.
fn handle_event(app: AppHandle, event: MediaControlEvent) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut player = state.player.lock().await;

        let result = match event {
            MediaControlEvent::Play => player.resume().await,
            MediaControlEvent::Pause => player.pause().await,
            MediaControlEvent::Toggle => {
                if player.get_state().await.state == PlayState::Playing {
                    player.pause().await
                } else {
                    player.resume().await
                }
            }
            MediaControlEvent::Next => {
                if let Some(stats) = player.note_manual_skip() {
                    let _ = app.emit("song-stats-changed", stats);
                }
                player.next_track().await
            }
            MediaControlEvent::Previous => player.previous_track().await,
            MediaControlEvent::Stop => player.stop().await,
            MediaControlEvent::SetPosition(MediaPosition(pos)) => {
                player.seek_to(pos.as_nanos() as u64).await
            }
            MediaControlEvent::Seek(direction) => {
                let current = player.get_state().await.position_nanosec;
                player
                    .seek_to(seek_target(current, direction, DEFAULT_SEEK_STEP))
                    .await
            }
            MediaControlEvent::SeekBy(direction, amount) => {
                let current = player.get_state().await.position_nanosec;
                player
                    .seek_to(seek_target(current, direction, amount))
                    .await
            }
            MediaControlEvent::SetVolume(_)
            | MediaControlEvent::OpenUri(_)
            | MediaControlEvent::Raise
            | MediaControlEvent::Quit => Ok(()),
        };

        match result {
            Ok(()) => {
                let playback_state = player.get_state().await;
                mirror_state(&app, &playback_state).await;
                let _ = app.emit("playback-state", playback_state);
            }
            Err(e) => {
                log::warn!("Failed to handle OS media control event: {e}");
            }
        }
    });
}

fn seek_target(current_nanosec: i64, direction: SeekDirection, amount: Duration) -> u64 {
    let delta = amount.as_nanos().min(i64::MAX as u128) as i64;
    let target = match direction {
        SeekDirection::Forward => current_nanosec.saturating_add(delta),
        SeekDirection::Backward => current_nanosec.saturating_sub(delta).max(0),
    };
    target as u64
}
