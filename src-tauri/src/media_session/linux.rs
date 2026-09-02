//! Linux MPRIS2 backend — #576.
//!
//! Hand-rolled `org.mpris.MediaPlayer2` + `org.mpris.MediaPlayer2.Player`
//! D-Bus interfaces via `zbus`, replacing souvlaki's `use_zbus` backend.
//! The connection is driven through `tauri::async_runtime::block_on` from
//! the dedicated media-session thread (same pattern already used elsewhere
//! in the app to call async code from a plain OS thread, e.g.
//! `collection/watcher.rs`), rather than spinning up a second executor.

use super::{handle_event, MediaCommand, PlatformMediaSession, SeekDirection};
use crate::models::PlayState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::{interface, Connection};

const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const BUS_NAME: &str = "org.mpris.MediaPlayer2.luminous";

#[derive(Default)]
struct SharedState {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration: Option<Duration>,
    cover_url: Option<String>,
    status: Option<PlayState>,
    position: Duration,
}

struct MediaPlayer2Iface;

#[interface(name = "org.mpris.MediaPlayer2")]
impl MediaPlayer2Iface {
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        "Luminous"
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }

    async fn quit(&self) {}
    async fn raise(&self) {}
}

struct PlayerIface {
    app_handle: AppHandle,
    state: Arc<Mutex<SharedState>>,
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerIface {
    async fn play(&self) {
        handle_event(self.app_handle.clone(), MediaCommand::Play);
    }

    async fn pause(&self) {
        handle_event(self.app_handle.clone(), MediaCommand::Pause);
    }

    #[zbus(name = "PlayPause")]
    async fn play_pause(&self) {
        handle_event(self.app_handle.clone(), MediaCommand::Toggle);
    }

    async fn stop(&self) {
        handle_event(self.app_handle.clone(), MediaCommand::Stop);
    }

    async fn next(&self) {
        handle_event(self.app_handle.clone(), MediaCommand::Next);
    }

    async fn previous(&self) {
        handle_event(self.app_handle.clone(), MediaCommand::Previous);
    }

    async fn seek(&self, offset: i64) {
        let direction = if offset >= 0 {
            SeekDirection::Forward
        } else {
            SeekDirection::Backward
        };
        let amount = Duration::from_micros(offset.unsigned_abs());
        handle_event(
            self.app_handle.clone(),
            MediaCommand::SeekBy(direction, amount),
        );
    }

    #[zbus(name = "SetPosition")]
    async fn set_position(&self, _track_id: ObjectPath<'_>, position: i64) {
        handle_event(
            self.app_handle.clone(),
            MediaCommand::SetPosition(Duration::from_micros(position.max(0) as u64)),
        );
    }

    async fn open_uri(&self, _uri: String) {}

    #[zbus(property)]
    fn playback_status(&self) -> String {
        match self.state.lock().unwrap().status {
            Some(PlayState::Playing) => "Playing",
            Some(PlayState::Paused) => "Paused",
            Some(PlayState::Stopped) | None => "Stopped",
        }
        .to_string()
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let state = self.state.lock().unwrap();
        let mut map = HashMap::new();
        // MPRIS requires a trackid even without a real tracklist; a fixed
        // path is fine since we never expose org.mpris.MediaPlayer2.TrackList.
        if let Ok(path) = ObjectPath::try_from("/org/luminous/CurrentTrack") {
            if let Ok(value) = OwnedValue::try_from(Value::from(path)) {
                map.insert("mpris:trackid".to_string(), value);
            }
        }
        if let Some(title) = &state.title {
            if let Ok(value) = OwnedValue::try_from(Value::from(title.clone())) {
                map.insert("xesam:title".to_string(), value);
            }
        }
        if let Some(artist) = &state.artist {
            if let Ok(value) = OwnedValue::try_from(Value::from(vec![artist.clone()])) {
                map.insert("xesam:artist".to_string(), value);
            }
        }
        if let Some(album) = &state.album {
            if let Ok(value) = OwnedValue::try_from(Value::from(album.clone())) {
                map.insert("xesam:album".to_string(), value);
            }
        }
        if let Some(duration) = state.duration {
            if let Ok(value) = OwnedValue::try_from(Value::from(duration.as_micros() as i64)) {
                map.insert("mpris:length".to_string(), value);
            }
        }
        if let Some(cover_url) = &state.cover_url {
            if let Ok(value) = OwnedValue::try_from(Value::from(cover_url.clone())) {
                map.insert("mpris:artUrl".to_string(), value);
            }
        }
        map
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        self.state.lock().unwrap().position.as_micros() as i64
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

pub struct LinuxMediaSession {
    connection: Connection,
    state: Arc<Mutex<SharedState>>,
}

impl PlatformMediaSession for LinuxMediaSession {
    fn init(app_handle: AppHandle, _hwnd: Option<*mut std::ffi::c_void>) -> Option<Self> {
        if !is_dbus_session_available() {
            log::warn!("No D-Bus session bus available; skipping OS media session initialization");
            return None;
        }

        let state = Arc::new(Mutex::new(SharedState::default()));
        let player_iface = PlayerIface {
            app_handle,
            state: state.clone(),
        };

        let connect = async {
            zbus::connection::Builder::session()?
                .name(BUS_NAME)?
                .serve_at(OBJECT_PATH, MediaPlayer2Iface)?
                .serve_at(OBJECT_PATH, player_iface)?
                .build()
                .await
        };

        match tauri::async_runtime::block_on(connect) {
            Ok(connection) => Some(Self { connection, state }),
            Err(e) => {
                log::warn!("Failed to initialize MPRIS D-Bus session: {e:?}");
                None
            }
        }
    }

    fn set_metadata(
        &mut self,
        title: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        duration: Option<Duration>,
        cover_url: Option<&str>,
    ) {
        {
            let mut state = self.state.lock().unwrap();
            state.title = title.map(str::to_string);
            state.artist = artist.map(str::to_string);
            state.album = album.map(str::to_string);
            state.duration = duration;
            state.cover_url = cover_url.map(str::to_string);
        }
        self.emit_property_changed("Metadata");
    }

    fn set_playback(&mut self, status: PlayState, position: Duration) {
        {
            let mut state = self.state.lock().unwrap();
            state.status = Some(status);
            state.position = position;
        }
        self.emit_property_changed("PlaybackStatus");
        self.emit_property_changed("Position");
    }
}

impl LinuxMediaSession {
    fn emit_property_changed(&self, property: &'static str) {
        let connection = self.connection.clone();
        let emit = async move {
            let Ok(iface_ref) = connection
                .object_server()
                .interface::<_, PlayerIface>(OBJECT_PATH)
                .await
            else {
                return;
            };
            let ctx = SignalEmitter::new(&connection, OBJECT_PATH).ok();
            let Some(ctx) = ctx else { return };
            let iface = iface_ref.get().await;
            let result = match property {
                "Metadata" => iface.metadata_changed(&ctx).await,
                "PlaybackStatus" => iface.playback_status_changed(&ctx).await,
                "Position" => {
                    // MPRIS explicitly excludes Position from PropertiesChanged
                    // (clients are expected to poll/interpolate it); emitting
                    // the Seeked signal instead is the spec-correct notification.
                    iface.seeked(&ctx, iface.position()).await
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                log::warn!("Failed to emit MPRIS property change for {property}: {e:?}");
            }
        };
        tauri::async_runtime::block_on(emit);
    }
}

/// Helper function to check if a D-Bus session bus is available based on env variables
/// and socket paths.
#[cfg(any(target_os = "linux", test))]
fn check_dbus_session_available<F>(
    dbus_addr: Option<String>,
    xdg_runtime_dir: Option<String>,
    uid: u32,
    path_exists: F,
) -> bool
where
    F: Fn(&std::path::Path) -> bool,
{
    if let Some(addr) = dbus_addr {
        let addr = addr.trim();
        if !addr.is_empty() {
            if let Some(path) = addr.strip_prefix("unix:path=") {
                let socket_path = path.split(',').next().unwrap_or(path);
                if path_exists(std::path::Path::new(socket_path)) {
                    return true;
                }
            } else {
                // Non unix:path address (e.g. abstract socket unix:abstract= or tcp:)
                return true;
            }
        }
    }

    if let Some(runtime_dir) = xdg_runtime_dir {
        let bus_path = std::path::Path::new(&runtime_dir).join("bus");
        if path_exists(&bus_path) {
            return true;
        }
    }

    let fallback = format!("/run/user/{uid}/bus");
    if path_exists(std::path::Path::new(&fallback)) {
        return true;
    }

    false
}

/// Check if a D-Bus session bus is likely available on Linux.
///
/// Under minimal or headless Linux environments (e.g. fresh WSL2 without a running session bus),
/// connecting without this check would otherwise surface a connection error deep in zbus's async
/// handshake instead of a clean early skip. Historically (with souvlaki, #537) the equivalent
/// zbus-backed path would panic instead of erroring; hand-rolling the connection now means a
/// missing bus surfaces as a normal `Err` from `Builder::build()`, but this pre-check is kept for
/// the fast, allocation-free early exit and its existing test coverage.
fn is_dbus_session_available() -> bool {
    let dbus_addr = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
    let xdg_runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();
    let uid = unsafe { libc::getuid() };
    check_dbus_session_available(dbus_addr, xdg_runtime_dir, uid, |p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_check_dbus_session_existing_unix_path() {
        let mut existing = HashSet::new();
        existing.insert(PathBuf::from("/run/user/1000/bus"));

        let exists = |p: &Path| existing.contains(p);
        assert!(check_dbus_session_available(
            Some("unix:path=/run/user/1000/bus".to_string()),
            None,
            1000,
            exists
        ));
    }

    #[test]
    fn test_check_dbus_session_missing_unix_path() {
        let existing: HashSet<PathBuf> = HashSet::new();
        let exists = |p: &Path| existing.contains(p);
        assert!(!check_dbus_session_available(
            Some("unix:path=/run/user/1000/bus".to_string()),
            None,
            1000,
            exists
        ));
    }

    #[test]
    fn test_check_dbus_session_abstract_socket() {
        let existing: HashSet<PathBuf> = HashSet::new();
        let exists = |p: &Path| existing.contains(p);
        assert!(check_dbus_session_available(
            Some("unix:abstract=/tmp/dbus-test".to_string()),
            None,
            1000,
            exists
        ));
    }

    #[test]
    fn test_check_dbus_session_fallback_xdg_runtime_dir() {
        let mut existing = HashSet::new();
        let expected_path = PathBuf::from("/var/run/user/1000").join("bus");
        existing.insert(expected_path);

        let exists = |p: &Path| existing.contains(p);
        assert!(check_dbus_session_available(
            None,
            Some("/var/run/user/1000".to_string()),
            1000,
            exists
        ));
    }

    #[test]
    fn test_check_dbus_session_fallback_uid() {
        let mut existing = HashSet::new();
        existing.insert(PathBuf::from("/run/user/1000/bus"));

        let exists = |p: &Path| existing.contains(p);
        assert!(check_dbus_session_available(None, None, 1000, exists));
    }

    #[test]
    fn test_check_dbus_session_none_available() {
        let existing: HashSet<PathBuf> = HashSet::new();
        let exists = |p: &Path| existing.contains(p);
        assert!(!check_dbus_session_available(None, None, 1000, exists));
    }
}
