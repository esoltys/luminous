//! System tray icon: left-click toggles the main window, right-click shows a
//! menu with playback controls, and — when the "Minimize to tray" setting is
//! on (off by default) — closing the main window hides it instead of
//! quitting. The close-to-tray hook is only registered once the tray itself
//! has been built successfully, so a failed tray init (e.g. no notification
//! host on a minimal Linux desktop) can't strand the window with no way to
//! bring it back — the OS close button always quits normally in that case,
//! and also quits normally whenever the setting is off.
//!
//! Platform note: on Linux, `tray-icon`'s `TrayIconEvent` (including left
//! click) is not emitted at all — a libappindicator limitation, not
//! something we can work around here. Any click on Linux falls through to
//! the native context menu, where "Show/Hide Luminous" covers the same
//! toggle. Left-click-to-toggle only fires on Windows/macOS.

use crate::models::{PlayState, PlaybackState};
use crate::AppState;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Listener, Manager};

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/32x32.png");

pub fn init(app: &tauri::App) -> tauri::Result<()> {
    let play_pause = MenuItem::with_id(app, "tray-play-pause", "Play/Pause", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "tray-next", "Next", true, None::<&str>)?;
    let previous = MenuItem::with_id(app, "tray-previous", "Previous", true, None::<&str>)?;
    let toggle_window = MenuItem::with_id(
        app,
        "tray-toggle-window",
        "Show/Hide Luminous",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &play_pause,
            &next,
            &previous,
            &PredefinedMenuItem::separator(app)?,
            &toggle_window,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::with_id("main-tray")
        .icon(Image::from_bytes(TRAY_ICON_BYTES)?)
        .tooltip("Luminous")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    if let Some(window) = app.get_webview_window("main") {
        let app_handle = app.handle().clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let minimize_to_tray = app_handle
                    .state::<AppState>()
                    .minimize_to_tray
                    .load(std::sync::atomic::Ordering::Relaxed);
                if minimize_to_tray {
                    api.prevent_close();
                    hide_main_window(&app_handle);
                }
            }
        });
    }

    seed_tooltip(app.handle().clone(), tray.clone());

    app.listen("playback-state", move |event| {
        if let Ok(state) = serde_json::from_str::<PlaybackState>(event.payload()) {
            let _ = tray.set_tooltip(Some(&tooltip_text(&state)));
        }
    });

    Ok(())
}

/// Shows, unminimizes, and focuses the main window. The single place every
/// "bring Luminous back" path (tray left-click, tray menu, single-instance
/// relaunch) routes through, so they all restore the macOS Dock icon the
/// same way instead of only some of them remembering to.
pub fn restore_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
}

fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

fn toggle_main_window(app: &AppHandle) {
    let visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide_main_window(app);
    } else {
        restore_main_window(app);
    }
}

/// Dispatches tray menu clicks. Play/Pause/Next/Previous go straight through
/// `state.player`, mirroring the existing global media-key shortcut handler
/// in `lib.rs` and `media_session::handle_event` — no IPC round-trip needed.
fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "tray-toggle-window" => return toggle_main_window(app),
        "tray-quit" => return app.exit(0),
        _ => {}
    }

    let app = app.clone();
    let action = event.id().as_ref().to_string();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut player = state.player.lock().await;

        let result = match action.as_str() {
            "tray-play-pause" => {
                if player.get_state().await.state == PlayState::Playing {
                    player.pause().await
                } else {
                    player.resume().await
                }
            }
            "tray-next" => {
                if let Some(stats) = player.note_manual_skip() {
                    let _ = app.emit("song-stats-changed", stats);
                }
                player.next_track().await
            }
            "tray-previous" => player.previous_track().await,
            _ => Ok(()),
        };

        if result.is_ok() {
            let playback_state = player.get_state().await;
            crate::media_session::mirror_state(&app, &playback_state).await;
            let _ = app.emit("playback-state", playback_state);
        }
    });
}

/// The tray tooltip only updates on `"playback-state"`, which fires on
/// *changes* — so a track restored from a prior session at launch (loaded
/// but not yet touched) wouldn't otherwise show up until the next
/// play/pause/seek. Seed it once, right after the tray is built.
fn seed_tooltip(app: AppHandle, tray: TrayIcon) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let snapshot = state.player.lock().await.get_state().await;
        let _ = tray.set_tooltip(Some(&tooltip_text(&snapshot)));
    });
}

fn tooltip_text(state: &PlaybackState) -> String {
    match &state.current_song {
        Some(song) => match &song.artist {
            Some(artist) => format!("{} — {}", song.display_title(), artist),
            None => song.display_title().to_string(),
        },
        None => "Luminous".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Song;

    #[test]
    fn tooltip_shows_title_and_artist() {
        let state = PlaybackState {
            current_song: Some(Song {
                title: Some("Money".to_string()),
                artist: Some("Pink Floyd".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(tooltip_text(&state), "Money — Pink Floyd");
    }

    #[test]
    fn tooltip_falls_back_to_title_only_without_artist() {
        let state = PlaybackState {
            current_song: Some(Song {
                title: Some("Money".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(tooltip_text(&state), "Money");
    }

    #[test]
    fn tooltip_falls_back_to_app_name_when_idle() {
        let state = PlaybackState::default();
        assert_eq!(tooltip_text(&state), "Luminous");
    }
}
