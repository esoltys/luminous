//! OS "Now Playing" integration (SMTC on Windows, MPRIS2 on Linux) — #80, #576.
//!
//! Hand-rolled per-platform backends (no `souvlaki`, see #576 for why: the
//! obvious modern replacement crate was too immature to depend on). Each
//! platform backend implements [`PlatformMediaSession`] and is driven
//! entirely from one dedicated OS thread, since the underlying native APIs
//! are thread-affine (Windows SMTC is tied to the COM apartment that
//! registered it). Outbound updates (metadata/playback state) are sent to
//! that thread through a channel via [`MediaSessionHandle`]; inbound OS
//! control events (play/pause/next/…) are routed from the platform
//! backend's own callback onto the Tauri async runtime, reusing the same
//! `state.player` command paths the IPC handlers and global media-key
//! shortcuts use.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

use crate::models::{PlayState, PlaybackState};
use crate::AppState;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// An inbound OS media control event, routed to `state.player`.
///
/// `Toggle`/`SeekBy`/`SetVolume` are only ever constructed on Linux — SMTC's
/// Windows API only exposes discrete buttons and an absolute `SetPosition`;
/// volume is controlled by the Windows system mixer, not SMTC. Hence the
/// `dead_code` allowance on non-Linux targets.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
enum MediaCommand {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    SetPosition(Duration),
    SeekBy(SeekDirection, Duration),
    /// MPRIS `Volume` (0.0-1.0). Some desktop environments (e.g. GNOME's
    /// media-keys handler) route hardware volume-up/down keys to the
    /// active MPRIS player's `Volume` property instead of the system
    /// mixer, so this needs to be wired to something real — see #576.
    SetVolume(f64),
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy)]
enum SeekDirection {
    Forward,
    Backward,
}

enum Command {
    Update {
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
        duration: Option<Duration>,
        cover_url: Option<String>,
        status: PlayState,
        position: Duration,
        volume: f32,
    },
}

/// A platform-specific "Now Playing" backend. Constructed and driven
/// entirely on the dedicated media-session thread.
trait PlatformMediaSession: Sized {
    /// Attempt to initialize the platform backend. `app_handle` is used to
    /// route inbound OS control events back onto the Tauri async runtime.
    /// Returns `None` (having logged a warning) if unavailable.
    fn init(app_handle: AppHandle, hwnd: Option<*mut std::ffi::c_void>) -> Option<Self>;

    fn set_metadata(
        &mut self,
        title: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        duration: Option<Duration>,
        cover_url: Option<&str>,
    );

    fn set_playback(&mut self, status: PlayState, position: Duration, volume: f32);
}

#[cfg(target_os = "windows")]
type Platform = windows::WindowsMediaSession;
#[cfg(target_os = "linux")]
type Platform = linux::LinuxMediaSession;
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
type Platform = NoopMediaSession;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
struct NoopMediaSession;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
impl PlatformMediaSession for NoopMediaSession {
    fn init(_app_handle: AppHandle, _hwnd: Option<*mut std::ffi::c_void>) -> Option<Self> {
        None
    }

    fn set_metadata(
        &mut self,
        _title: Option<&str>,
        _artist: Option<&str>,
        _album: Option<&str>,
        _duration: Option<Duration>,
        _cover_url: Option<&str>,
    ) {
    }

    fn set_playback(&mut self, _status: PlayState, _position: Duration, _volume: f32) {}
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
            volume: playback.volume,
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
    // thread we're about to spawn, to construct the platform backend.
    struct SendableHwnd(Option<*mut std::ffi::c_void>);
    unsafe impl Send for SendableHwnd {}
    let hwnd = SendableHwnd(hwnd);

    let (tx, rx) = mpsc::channel::<Command>();
    let (ready_tx, ready_rx) = mpsc::channel::<bool>();

    let spawn_result = std::thread::Builder::new()
        .name("luminous-media-session".to_string())
        .spawn(move || {
            let hwnd = hwnd;

            // Defense-in-depth: a platform backend's init should never
            // panic, but keep a thin catch_unwind around it during rollout
            // of this hand-rolled replacement for souvlaki (which itself
            // needed this, see #537).
            let init_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Platform::init(app_handle.clone(), hwnd.0)
            }));

            let mut platform = match init_result {
                Ok(Some(p)) => p,
                Ok(None) => {
                    let _ = ready_tx.send(false);
                    return;
                }
                Err(panic_payload) => {
                    let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                        *s
                    } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                        s.as_str()
                    } else {
                        "unknown panic"
                    };
                    log::warn!(
                        "OS media session initialization panicked ({panic_msg}); continuing without OS media integration"
                    );
                    let _ = ready_tx.send(false);
                    return;
                }
            };

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
                        volume,
                    } => {
                        platform.set_metadata(
                            title.as_deref(),
                            artist.as_deref(),
                            album.as_deref(),
                            duration,
                            cover_url.as_deref(),
                        );
                        platform.set_playback(status, position, volume);
                    }
                }
            }
        });

    if let Err(e) = spawn_result {
        log::warn!("Failed to spawn OS media session thread: {e}");
        return None;
    }

    // A stuck/absent backend should fail fast inside its own init(); this
    // timeout just guarantees app startup can't hang on it.
    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(true) => Some(MediaSessionHandle { tx }),
        _ => None,
    }
}

/// Route an inbound OS media control event onto the same `state.player`
/// command paths the IPC handlers and global media-key shortcuts use.
fn handle_event(app: AppHandle, event: MediaCommand) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut player = state.player.lock().await;

        let result = match event {
            MediaCommand::Play => player.resume().await,
            MediaCommand::Pause => player.pause().await,
            MediaCommand::Toggle => {
                if player.get_state().await.state == PlayState::Playing {
                    player.pause().await
                } else {
                    player.resume().await
                }
            }
            MediaCommand::Next => {
                if let Some(stats) = player.note_manual_skip() {
                    let _ = app.emit("song-stats-changed", stats);
                }
                player.next_track().await
            }
            MediaCommand::Previous => player.previous_track().await,
            MediaCommand::Stop => player.stop().await,
            MediaCommand::SetPosition(pos) => player.seek_to(pos.as_nanos() as u64).await,
            MediaCommand::SeekBy(direction, amount) => {
                let current = player.get_state().await.position_nanosec;
                player
                    .seek_to(seek_target(current, direction, amount))
                    .await
            }
            MediaCommand::SetVolume(vol) => player.set_volume(vol.clamp(0.0, 1.0) as f32).await,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seek_target_forward() {
        let current = 5_000_000_000i64; // 5s
        let step = Duration::from_secs(10);
        let target = seek_target(current, SeekDirection::Forward, step);
        assert_eq!(target, 15_000_000_000);
    }

    #[test]
    fn test_seek_target_backward() {
        let current = 15_000_000_000i64; // 15s
        let step = Duration::from_secs(10);
        let target = seek_target(current, SeekDirection::Backward, step);
        assert_eq!(target, 5_000_000_000);
    }

    #[test]
    fn test_seek_target_backward_underflow_clamps_to_zero() {
        let current = 3_000_000_000i64; // 3s
        let step = Duration::from_secs(10);
        let target = seek_target(current, SeekDirection::Backward, step);
        assert_eq!(target, 0);
    }

    #[test]
    fn test_seek_target_negative_current_clamps_to_zero() {
        let current = -1_000_000_000i64;
        let step = Duration::from_secs(5);
        let target = seek_target(current, SeekDirection::Backward, step);
        assert_eq!(target, 0);
    }
}
