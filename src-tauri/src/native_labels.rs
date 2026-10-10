//! Text the backend puts in native surfaces (the tray menu, the Windows taskbar
//! thumbnail buttons). The frontend owns the locale catalogs, so it pushes the
//! translated labels with `set_native_labels` on launch and on every language
//! change; the English defaults in `tray.rs` / `taskbar.rs` only show until the
//! first push.

use tauri::AppHandle;

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
}

pub fn apply(app: &AppHandle, labels: &NativeLabels) {
    crate::tray::set_labels(app, labels);
    #[cfg(target_os = "windows")]
    crate::taskbar::set_labels(app, labels);
}
