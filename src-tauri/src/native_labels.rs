//! Text the backend puts in native surfaces (the tray menu, the Windows taskbar
//! thumbnail buttons, and the OS window title fallback). The frontend owns the
//! locale catalogs, so it pushes the translated labels with `set_native_labels`
//! on launch and on every language change; the English defaults in `tray.rs`,
//! `taskbar.rs`, and `unknown_song_label()` only show until the first push.

use parking_lot::RwLock;
use tauri::{AppHandle, Manager};

static UNKNOWN_SONG_LABEL: RwLock<Option<String>> = RwLock::new(None);

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeLabels {
    pub play_pause: String,
    pub play: String,
    pub pause: String,
    pub previous: String,
    pub next: String,
    pub pause_scrobbling: String,
    pub show_hide_window: String,
    pub quit: String,
    pub unknown_song: String,
}

/// Returns the latest localized fallback for an untitled song pushed by the
/// frontend, or an empty string before the first `set_native_labels` call.
pub fn unknown_song_label() -> String {
    UNKNOWN_SONG_LABEL.read().clone().unwrap_or_default()
}

pub fn apply(app: &AppHandle, labels: &NativeLabels) {
    *UNKNOWN_SONG_LABEL.write() = Some(labels.unknown_song.clone());
    crate::tray::set_labels(app, labels);
    #[cfg(target_os = "windows")]
    crate::taskbar::set_labels(app, labels);

    // Re-sync the OS window title immediately so switching languages while an
    // untitled track is playing updates the taskbar/titlebar without waiting
    // for the next playback event.
    if app.try_state::<crate::AppState>().is_some() {
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let state = app_handle.state::<crate::AppState>();
            let snapshot = state.player.lock().await.get_state().await;
            crate::sync_window_title(
                &app_handle,
                snapshot.current_song.as_ref(),
                snapshot.state == crate::models::PlayState::Playing,
            );
        });
    }
}
