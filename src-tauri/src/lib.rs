// Luminous Music Player — Backend Entry Point
//
// Module structure:
//   db        — SQLite schema, connection pool, migrations
//   models    — Core data types (Song, PlaylistItem, etc.)
//   commands  — All #[tauri::command] handlers
//   audio     — Symphonia + CPAL audio pipeline
//   player    — Playback state machine (shuffle, repeat, queue)
//   collection — Library scanner + file watcher
//   playlist  — Playlist CRUD + undo/redo

pub mod addons;
pub mod analyzer;
pub mod artist_image;
pub mod audio;
pub mod band_waveform;
pub mod biomanager;
pub mod bridge;
pub mod codecs;
pub mod collection;
pub mod commands;
pub mod context;
pub mod continue_mix;
pub mod covermanager;
pub mod cue;
pub mod db;
pub mod default_apps;
pub mod diagnostics;
pub mod discord;
pub mod dr_parser;
pub mod eq_import;
pub mod eq_presets;
pub mod equalizer;
pub mod fade;
pub mod filter_parser;
pub mod hierarchy_sidecar;
pub mod install_format;
pub mod loudness;
pub mod lyrics;
pub mod media_session;
pub mod models;
pub mod musicbrainz;
pub mod native_labels;
pub mod organizer;
pub mod paths;
pub mod picard;
pub mod pins;
pub mod player;
pub mod playlist;
pub mod playlist_parsers;
pub mod ratings_sync;
pub mod remote_scheduler;
pub mod restart_manager;
pub mod scrobbler;
pub mod stall_monitor;
pub mod stats;
pub mod stats_summary;
pub mod subsonic;
pub mod tageditor;
pub mod tags;
#[cfg(target_os = "windows")]
pub mod taskbar;
pub mod tray;
pub mod waveform;
pub mod webdav;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, ShortcutState};
use tokio::sync::Mutex;

pub use audio::AudioEngine;
pub use covermanager::CoverManager;
pub use db::Database;
pub use player::Player;
pub use playlist::PlaylistManager;

/// Shared application state injected into every Tauri command.
pub struct AppState {
    pub db: Arc<Database>,
    pub audio: Arc<Mutex<AudioEngine>>,
    pub player: Arc<Mutex<Player>>,
    pub volume_before_mute: Arc<Mutex<f32>>,
    pub playlists: Arc<Mutex<PlaylistManager>>,
    pub cover_manager: Arc<CoverManager>,
    pub watcher: Arc<parking_lot::Mutex<Option<notify::RecommendedWatcher>>>,
    /// Pause depth, not a bool — see `collection::WatcherPauseGuard`. A count
    /// (rather than a flag) lets overlapping self-inflicted writes (e.g. a
    /// scan and a tag-editor save close together) each hold their own pause
    /// without one's release re-arming the watcher while the other is still
    /// writing.
    pub watcher_paused: Arc<std::sync::atomic::AtomicU32>,
    /// Path-aware companion to `watcher_paused` — see `collection::SelfWriteTracker` (#514).
    pub self_writes: Arc<collection::SelfWriteTracker>,
    pub startup_file: Mutex<Option<String>>,
    /// OS "Now Playing" integration handle (#80) — `None` when the platform
    /// integration failed to initialize (unsupported desktop, no session
    /// bus, etc), in which case Luminous simply runs without it.
    pub media_session: Option<media_session::MediaSessionHandle>,
    /// Whether closing the main window hides it to the tray instead of
    /// quitting (off by default — see `tray.rs`). An atomic rather than a DB
    /// read on every close event, since `tray.rs`'s `CloseRequested` handler
    /// needs this synchronously; kept in sync with the `app_state` row of
    /// the same name by `commands::settings::set_minimize_to_tray_enabled`.
    pub minimize_to_tray: Arc<std::sync::atomic::AtomicBool>,
    pub scrobbler: Arc<scrobbler::ScrobblerManager>,
    pub musicbrainz: Arc<musicbrainz::MusicBrainzManager>,
    /// Per-server periodic auto-sync timers for remote servers (WebDAV #1082, OpenSubsonic #1162).
    pub remote_auto_sync: Arc<remote_scheduler::AutoSyncScheduler>,
    /// Portable Genres/Artist Tags hierarchy in the default library (#1312).
    pub hierarchy_sidecar: Arc<hierarchy_sidecar::HierarchySidecar>,
    /// Store entitlement and delivery for add-on themes (#1414).
    pub addons: Arc<addons::entitlement::AddonManager>,
}

/// Suppresses stock webview browser chrome — reload/find/print keybindings and
/// the native right-click menu — so Luminous reads as a native app rather than
/// a webview (#212). The only carve-out is `DEV_TOOLS` in debug builds, so
/// `Ctrl+Shift+I` still opens devtools under `cargo tauri dev`; everything
/// else (including the context menu and reload) is suppressed in both debug
/// and release builds.
///
/// Zoom (`Ctrl+Plus/Minus`, `Ctrl+MouseWheel`, pinch-to-zoom) isn't covered by
/// the plugin's cross-platform JS flags, so on Windows it's additionally
/// killed at the WebView2 settings level via the `platform-windows` feature.
/// `browser_accelerator_keys` is WebView2's blanket switch for the same
/// shortcuts `Flags` targets (plus zoom, plus devtools) — only flipped off in
/// release so it doesn't fight the devtools carve-out above.
fn build_prevent_default_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    use tauri_plugin_prevent_default::Flags;

    let flags = if cfg!(debug_assertions) {
        Flags::all().difference(Flags::DEV_TOOLS)
    } else {
        Flags::all()
    };
    let builder = tauri_plugin_prevent_default::Builder::new().with_flags(flags);

    #[cfg(target_os = "windows")]
    let builder = {
        use tauri_plugin_prevent_default::PlatformOptions;
        let mut platform = PlatformOptions::new().pinch_zoom(false).zoom_control(false);
        if !cfg!(debug_assertions) {
            platform = platform.browser_accelerator_keys(false);
        }
        builder.platform(platform)
    };

    builder.build()
}

/// Env vars that turn off WebKitGTK's GPU rendering. Luminous never sets
/// them itself: the AppImage used to (#370, #383), against a blank-window
/// bug on Mesa 25+/Wayland that tauri-bundler 2.10 fixed at the source
/// (#1301). A user can still set them by hand on a broken GPU stack, and the
/// frontend then swaps in its no-compositing fallbacks.
#[cfg(target_os = "linux")]
const LINUX_WEBKITGTK_RENDERING_ENV_VARS: &[&str] = &[
    "WEBKIT_DISABLE_COMPOSITING_MODE",
    "WEBKIT_DISABLE_DMABUF_RENDERER",
];

/// Whether any of `LINUX_WEBKITGTK_RENDERING_ENV_VARS` is set for this
/// process (WebKitGTK treats any value other than "0" as set). Takes the env
/// lookup as a parameter so it's testable without mutating the process env.
#[cfg(target_os = "linux")]
fn webkitgtk_gpu_rendering_disabled_by(get: impl Fn(&str) -> Option<std::ffi::OsString>) -> bool {
    LINUX_WEBKITGTK_RENDERING_ENV_VARS
        .iter()
        .any(|key| get(key).is_some_and(|value| value != "0"))
}

/// See `webkitgtk_gpu_rendering_disabled_by()`. Always false off Linux.
pub(crate) fn webkitgtk_gpu_rendering_disabled() -> bool {
    #[cfg(target_os = "linux")]
    {
        webkitgtk_gpu_rendering_disabled_by(|key| std::env::var_os(key))
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Appends WebView2's occlusion-calculation-disabling flag to an existing
/// `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` value, without duplicating it if
/// already present. Factored out of `run()`'s `cfg(target_os = "windows")`
/// block as a pure string function so it's unit-testable on any host.
///
/// Regression test for the Windows "blank app after being minimized/
/// occluded" failure: Chromium's CalculateNativeWinOcclusion feature
/// suspends WebView2's rendering pipeline when the window is minimized or
/// occluded, and it can fail to resume/repaint on restore. Without this
/// flag set before the webview is created, restoring the window shows a
/// blank surface.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn with_webview2_occlusion_disabled(current: &str) -> String {
    let feature = "CalculateNativeWinOcclusion";
    if current.contains(feature) {
        return current.to_string();
    }
    if current.is_empty() {
        return format!("--disable-features={}", feature);
    }
    const PREFIX: &str = "--disable-features=";
    if let Some(pos) = current.find(PREFIX) {
        let value_start = pos + PREFIX.len();
        let value_end = current[value_start..]
            .find(char::is_whitespace)
            .map(|offset| value_start + offset)
            .unwrap_or(current.len());

        let before = &current[..value_end];
        let after = &current[value_end..];
        if current[value_start..value_end].is_empty() {
            format!("{}{}{}", before, feature, after)
        } else {
            format!("{},{}{}", before, feature, after)
        }
    } else {
        format!("{} --disable-features={}", current, feature)
    }
}

/// Appends the Chromium switches that keep a minimized/hidden WebView2's
/// renderer process from being deprioritized, to an existing
/// `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` value, without duplicating any
/// that are already present.
///
/// Regression test for #884: Chromium's own renderer-backgrounding drops a
/// hidden/minimized page's renderer process to background OS scheduling
/// priority (Windows 11 shows this as "Efficiency Mode" in Task Manager) and
/// throttles its timers. Luminous's audio plays natively via CPAL, never
/// through the DOM, so Chromium has no signal that the page still matters
/// while minimized — and un-throttling after a long minimize isn't instant,
/// leaving the window blank for up to a minute after restore. This is a
/// distinct mechanism from `with_webview2_occlusion_disabled`'s
/// `CalculateNativeWinOcclusion` (a rendering-pipeline feature): backgrounding
/// is a process-priority/timer-throttling behavior that persists even with
/// occlusion calculation disabled.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn with_webview2_backgrounding_disabled(current: &str) -> String {
    const SWITCHES: &[&str] = &[
        "--disable-renderer-backgrounding",
        "--disable-backgrounding-occluded-windows",
        "--disable-background-timer-throttling",
    ];
    let mut result = current.to_string();
    for switch in SWITCHES {
        if !result.contains(switch) {
            if result.is_empty() {
                result = switch.to_string();
            } else {
                result.push(' ');
                result.push_str(switch);
            }
        }
    }
    result
}

/// Loopback port both platforms' remote-devtools channels listen on when
/// `LUMINOUS_REMOTE_DEVTOOLS` is set — see `remote_devtools_enabled()`. One
/// fixed port keeps the instructions identical across OSes (navigate to
/// `http://127.0.0.1:9222`) rather than needing per-platform documentation.
const REMOTE_DEVTOOLS_PORT: u16 = 9222;

/// Whether the opt-in remote-devtools channel (see `REMOTE_DEVTOOLS_PORT`)
/// should be wired up for this run. Gated on both a debug build *and* an
/// explicit env var so it can never ship enabled in a release build or
/// surprise a user who didn't ask for an unauthenticated loopback inspector
/// server — set `LUMINOUS_REMOTE_DEVTOOLS=true` before `bun run tauri dev` to
/// turn it on.
pub(crate) fn remote_devtools_enabled() -> bool {
    cfg!(debug_assertions) && std::env::var_os("LUMINOUS_REMOTE_DEVTOOLS").is_some()
}

/// Appends WebView2's Chrome DevTools Protocol remote-debugging switch to an
/// existing `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` value, without
/// duplicating it if already present. Mirrors `with_webview2_occlusion_disabled`
/// / `with_webview2_backgrounding_disabled`'s append-only pattern.
///
/// This is the Windows half of the opt-in remote-devtools channel gated by
/// `remote_devtools_enabled()`: with the port open, `http://localhost:<port>/json`
/// lists inspectable targets and each one's `devtoolsFrontendUrl` serves the
/// full Chrome DevTools UI (Console/DOM/Network) as a plain webpage, which an
/// agent without a GUI can load and read.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn with_webview2_remote_debugging(current: &str, port: u16) -> String {
    let flag = format!("--remote-debugging-port={}", port);
    if current.contains("--remote-debugging-port=") {
        return current.to_string();
    }
    if current.is_empty() {
        flag
    } else {
        format!("{} {}", current, flag)
    }
}

/// Reads persisted equalizer settings (linear + parametric) from the DB and
/// applies them to a freshly-constructed `AudioEngine`, so playback starts
/// with the user's last-saved EQ state instead of engine defaults.
fn restore_equalizer_from_db(db: &Database, audio_engine: &AudioEngine) {
    if let Ok(conn) = db.pool.get() {
        if let Ok((
            (enabled, preamp, gains_str, mode_str, parametric_json, active_preset),
            (inactive_preamp, inactive_preset),
        )) = conn.query_row(
            "SELECT enabled, preamp, gains, mode, parametric, active_preset,
                    inactive_preamp, inactive_preset
             FROM equalizer_settings WHERE id = 1",
            [],
            |row| {
                Ok((
                    (
                        row.get::<_, i32>(0)? != 0,
                        row.get::<_, f64>(1)? as f32,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ),
                    (row.get::<_, f64>(6)? as f32, row.get::<_, String>(7)?),
                ))
            },
        ) {
            let mut gains = [0.0f32; 10];
            for (i, val) in gains_str.split(',').enumerate() {
                if i < 10 {
                    if let Ok(gain) = val.parse::<f32>() {
                        gains[i] = gain;
                    }
                }
            }
            // '' is Custom, and so is a built-in that has since been
            // removed (e.g. "Headphones"): the bands themselves are kept.
            let preset = |p: String| {
                Some(p).filter(|p| {
                    crate::equalizer::parse_user_preset_key(p).is_some()
                        || crate::equalizer::builtin_preset_name(p).is_some()
                })
            };
            audio_engine.with_equalizer(|eq| {
                use crate::equalizer::EqMode;
                eq.enabled = enabled;
                eq.load_preset(gains);
                // `load_parametric` bounds the band count and clamps every
                // field; an empty/unparseable row keeps the default layout.
                if let Ok(bands) =
                    serde_json::from_str::<Vec<crate::equalizer::ParametricBand>>(&parametric_json)
                {
                    if !bands.is_empty() {
                        eq.load_parametric(&bands);
                    }
                }
                // "parametric20" is the pre-#1332 name (migration 53 rewrites it).
                let (mode, other) = if mode_str == "parametric" || mode_str == "parametric20" {
                    (EqMode::Parametric, EqMode::Graphic10)
                } else {
                    (EqMode::Graphic10, EqMode::Parametric)
                };
                eq.set_mode(mode);
                eq.set_mode_state(mode, preamp, preset(active_preset));
                eq.set_mode_state(other, inactive_preamp, preset(inactive_preset));
            });
        }
    }
}

/// Spawns the ~30 FPS spectrum-emission loop that pushes `spectrum-data`
/// events to the frontend while the spectrum visualizer is enabled and
/// something is playing.
fn spawn_visualizer_loop(app_handle: tauri::AppHandle, audio: Arc<Mutex<AudioEngine>>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(33)); // ~30 FPS
        let mut is_minimized_or_hidden = false;
        let mut ticks_until_visibility_check: u32 = 0;
        loop {
            interval.tick().await;

            // When the main window is minimized or hidden, skip calculating spectrum and emitting IPC.
            // When minimized, Chromium disables compositor frame commits. Continuous Canvas2D drawing
            // would pile up uncommitted PaintOpBuffers in Skia, leading to severe memory ballooning
            // and an unresponsive renderer thread upon restore (#1052).
            //
            // `is_minimized()`/`is_visible()` block until the main (UI) thread answers, so they run
            // on the blocking pool — never on a Tokio worker, where waiting on a busy main thread
            // stalled other tasks queued behind this one (including MPRIS's D-Bus connect, which the
            // main thread was itself waiting on at startup) — and only every ~0.5s rather than on
            // every frame, which was ~60 main-thread round trips a second.
            if ticks_until_visibility_check == 0 {
                ticks_until_visibility_check = 15;
                let handle = app_handle.clone();
                is_minimized_or_hidden = tokio::task::spawn_blocking(move || {
                    handle
                        .get_webview_window("main")
                        .map(|w| {
                            w.is_minimized().unwrap_or(false) || !w.is_visible().unwrap_or(true)
                        })
                        .unwrap_or(false)
                })
                .await
                .unwrap_or(false);
            }
            ticks_until_visibility_check -= 1;
            if is_minimized_or_hidden {
                continue;
            }

            let (enabled, spectrum) = {
                let engine = audio.lock().await;
                let enabled = engine.spectrum_enabled();
                let state = engine.current_state();
                let spectrum = if enabled && state == crate::models::PlayState::Playing {
                    Some(engine.spectrum_snapshot(1024))
                } else {
                    None
                };
                (enabled, spectrum)
            };

            if enabled {
                if let Some(spec) = spectrum {
                    let _ = app_handle.emit("spectrum-data", spec);
                }
            }
        }
    });
}

/// Spawns the 250ms playback-position tick loop: emits `playback-position`
/// while playing, and every 4th tick (~1s) persists position and mirrors it
/// to the OS media session — MPRIS2's Position property isn't push-updated
/// by souvlaki's D-Bus backend, so without this the seek bar would freeze at
/// the position from the last state transition (#80). SMTC interpolates its
/// own timeline, so this is a no-op cost there beyond the periodic refresh.
fn spawn_position_tick_loop(
    app_handle: tauri::AppHandle,
    audio: Arc<Mutex<AudioEngine>>,
    player: Arc<Mutex<Player>>,
) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
        let mut tick_counter: u32 = 0;
        loop {
            interval.tick().await;
            let (pos, state) = {
                let engine = audio.lock().await;
                (engine.current_position_nanosec(), engine.current_state())
            };
            if state == crate::models::PlayState::Playing {
                let mut p = player.lock().await;
                // `pos` is the audio engine's absolute position within the
                // current file; the UI and play-stats logic below both deal
                // in track-relative time (0 at the start of the song), which
                // only differs from `pos` for a CUE sheet track (#78) — for
                // a plain song `beginning_nanosec` is 0 and this is exact.
                let relative_pos = pos.saturating_sub(p.current_song_beginning_nanosec());
                if let Some(stats) = p.on_position_update(relative_pos) {
                    let _ = app_handle.emit("song-stats-changed", stats);
                }
                tick_counter = tick_counter.wrapping_add(1);
                if tick_counter.is_multiple_of(4) {
                    p.persist_position(pos).await;
                    let playback_snapshot = p.get_state().await;
                    crate::media_session::mirror_state(&app_handle, &playback_snapshot).await;
                }
                let _ = app_handle.emit(
                    "playback-position",
                    serde_json::json!({
                        "position_nanosec": relative_pos
                    }),
                );
            }
        }
    });
}

/// Keeps the native OS window title in sync with the current track directly
/// from the backend, ensuring taskbar and titlebar stay accurate even when
/// the frontend window is minimized or occluded (#892, #1052).
fn sync_window_title(app: &tauri::AppHandle, song: Option<&crate::models::Song>, is_playing: bool) {
    if let Some(w) = app.get_webview_window("main") {
        let app_name = if crate::remote_devtools_enabled() {
            "Luminous Debug"
        } else {
            "Luminous"
        };
        let title = match (song, is_playing) {
            (Some(s), true) => {
                let song_title = s.title.as_deref().unwrap_or("").trim();
                let display_title = if song_title.is_empty() {
                    "Unknown Song"
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
        };
        let _ = w.set_title(&title);
    }
}

/// Starts an Auto Continue top-up (#1235) when one is due. Spawned rather
/// than awaited: the event loop holds the player lock here, and the top-up
/// takes the playlists lock, which must never nest inside it.
fn spawn_auto_continue_if_due(player: &Player, app: &tauri::AppHandle) {
    if player.auto_continue_seed().is_some() {
        tauri::async_runtime::spawn(crate::continue_mix::maybe_extend(app.clone()));
    }
}

/// Spawns the OS thread that drains `AudioEngine`'s event channel and turns
/// each `AudioEvent` into player-state transitions, OS media-session
/// mirroring, and frontend events. A blocking OS thread rather than a Tokio
/// task since it blocks on `rx.iter()`.
fn spawn_audio_event_loop(
    app_handle: tauri::AppHandle,
    audio: Arc<Mutex<AudioEngine>>,
    player: Arc<Mutex<Player>>,
) {
    std::thread::Builder::new()
        .name("luminous-events".to_string())
        .spawn(move || {
            let rx = {
                let engine = tauri::async_runtime::block_on(async { audio.lock().await });
                engine.events()
            };

            let rx = rx.lock();
            for event in rx.iter() {
                log::trace!("Received audio event: {:?}", event);
                let app = app_handle.clone();
                let player = player.clone();
                let audio = audio.clone();
                tauri::async_runtime::block_on(async move {
                    let mut p = player.lock().await;
                    match event {
                        crate::audio::AudioEvent::Playing { .. } => {
                            p.reset_playback_errors();
                            let state = p.get_state().await;
                            if let Some(ref song) = p.current_song {
                                if let Some(app_state) = app.try_state::<AppState>() {
                                    app_state.scrobbler.on_now_playing(song).await;
                                    app_state
                                        .scrobbler
                                        .on_playback_state_changed(
                                            Some(song),
                                            true,
                                            state.position_nanosec,
                                        )
                                        .await;
                                }
                            }
                            sync_window_title(&app, p.current_song.as_ref(), true);
                            let pipeline = {
                                let a = audio.lock().await;
                                p.get_pipeline_info(&a)
                            };
                            let _ = app.emit(
                                "track-changed",
                                serde_json::json!({
                                    "song": p.current_song.clone(),
                                    "pipeline": pipeline,
                                }),
                            );
                            crate::media_session::mirror_state(&app, &state).await;
                            let _ = app.emit("playback-state", state);
                            spawn_auto_continue_if_due(&p, &app);
                        }
                        crate::audio::AudioEvent::Paused => {
                            let state = p.get_state().await;
                            if let Some(app_state) = app.try_state::<AppState>() {
                                app_state
                                    .scrobbler
                                    .on_playback_state_changed(
                                        p.current_song.as_ref(),
                                        false,
                                        state.position_nanosec,
                                    )
                                    .await;
                            }
                            sync_window_title(&app, p.current_song.as_ref(), false);
                            crate::media_session::mirror_state(&app, &state).await;
                            let _ = app.emit("playback-state", state);
                        }
                        crate::audio::AudioEvent::Stopped => {
                            if let Some(app_state) = app.try_state::<AppState>() {
                                app_state.scrobbler.on_playback_stopped().await;
                            }
                            let state = p.get_state().await;
                            sync_window_title(&app, None, false);
                            crate::media_session::mirror_state(&app, &state).await;
                            let _ = app.emit("playback-state", state);
                        }
                        crate::audio::AudioEvent::TrackFinished { .. } => {
                            let _ = p.on_track_finished().await;
                            let state = p.get_state().await;
                            let is_playing = state.state == crate::models::PlayState::Playing;
                            if !is_playing {
                                if let Some(app_state) = app.try_state::<AppState>() {
                                    app_state.scrobbler.on_playback_stopped().await;
                                }
                            }
                            sync_window_title(&app, p.current_song.as_ref(), is_playing);
                            crate::media_session::mirror_state(&app, &state).await;
                            let _ = app.emit("playback-state", state);
                        }
                        crate::audio::AudioEvent::AboutToFinish { .. } => {
                            // Prime the next track so the engine can
                            // hand over gaplessly at the boundary.
                            if let Err(e) = p.prepare_gapless_next().await {
                                log::warn!("Gapless preload failed: {e}");
                            }
                        }
                        crate::audio::AudioEvent::TrackTransitioned { song_id, .. } => {
                            let _ = p.on_gapless_transition(song_id).await;
                            let state = p.get_state().await;
                            if let Some(ref song) = p.current_song {
                                if let Some(app_state) = app.try_state::<AppState>() {
                                    app_state.scrobbler.on_now_playing(song).await;
                                    app_state
                                        .scrobbler
                                        .on_playback_state_changed(
                                            Some(song),
                                            true,
                                            state.position_nanosec,
                                        )
                                        .await;
                                }
                            }
                            sync_window_title(&app, p.current_song.as_ref(), true);
                            let pipeline = {
                                let a = audio.lock().await;
                                p.get_pipeline_info(&a)
                            };
                            let _ = app.emit(
                                "track-changed",
                                serde_json::json!({
                                    "song": p.current_song.clone(),
                                    "pipeline": pipeline,
                                }),
                            );
                            crate::media_session::mirror_state(&app, &state).await;
                            let _ = app.emit("playback-state", state);
                            spawn_auto_continue_if_due(&p, &app);
                        }
                        crate::audio::AudioEvent::PipelineChanged => {
                            let pipeline = {
                                let a = audio.lock().await;
                                p.get_pipeline_info(&a)
                            };
                            let _ = app.emit("audio-pipeline-changed", pipeline);
                        }
                        crate::audio::AudioEvent::Error { message } => {
                            log::error!("Audio engine error: {}", message);

                            if p.try_heal_and_retry_current_track().await {
                                // Stale-cased path (Linux/case-sensitive
                                // filesystem quirk) — repointed and retried
                                // in place, nothing to surface to the user.
                                let _ = app.emit("library-changed", ());
                            } else {
                                let outcome = p.note_playback_error();

                                if let Some(song) = &outcome.failed_song {
                                    let _ = app.emit(
                                        "playback-error",
                                        serde_json::json!({
                                            "songId": song.id,
                                            "title": song.title,
                                            "path": song.path,
                                            // Why it failed, e.g. a Subsonic
                                            // server's "Wrong username or
                                            // password" (#1163). Never holds a
                                            // signed URL.
                                            "message": message,
                                        }),
                                    );
                                }
                                if outcome.flagged_unavailable {
                                    let _ = app.emit("library-changed", ());
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
                            let _ = app.emit("playback-state", state);
                        }
                        _ => {}
                    }
                });
            }
        })
        .expect("failed to spawn event thread");
}

/// Registers global OS media-key shortcuts (play/pause, next/prev track,
/// volume up/down/mute) so they work even when Luminous isn't focused.
/// Registration failures (e.g. another app already owns a key) are
/// swallowed per-shortcut — Luminous just runs without that one shortcut.
fn register_media_shortcuts(app: &tauri::App) {
    let media_shortcuts = [
        "MediaPlayPause",
        "MediaTrackNext",
        "MediaTrackPrevious",
        "AudioVolumeUp",
        "AudioVolumeDown",
        "AudioVolumeMute",
    ];
    for shortcut_str in media_shortcuts {
        if let Err(err) = app
            .global_shortcut()
            .on_shortcut(shortcut_str, |app, shortcut, event| {
                if event.state != ShortcutState::Pressed {
                    return;
                }

                let key = shortcut.key;
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    let mut player = state.player.lock().await;
                    let result = match key {
                        Code::MediaPlayPause => {
                            let playback_state = player.get_state().await.state;
                            if playback_state == crate::models::PlayState::Playing {
                                player.pause().await
                            } else {
                                player.resume().await
                            }
                        }
                        Code::MediaTrackNext => {
                            if let Some(stats) = player.note_manual_skip() {
                                let _ = app_handle.emit("song-stats-changed", stats);
                            }
                            player.next_track().await
                        }
                        Code::MediaTrackPrevious => player.previous_track().await,
                        Code::AudioVolumeUp => {
                            let volume = player.get_state().await.volume;
                            player.set_volume((volume + 0.05).min(1.0)).await
                        }
                        Code::AudioVolumeDown => {
                            let volume = player.get_state().await.volume;
                            player.set_volume((volume - 0.05).max(0.0)).await
                        }
                        Code::AudioVolumeMute => {
                            let volume = player.get_state().await.volume;
                            if volume > 0.0 {
                                let mut volume_before_mute = state.volume_before_mute.lock().await;
                                *volume_before_mute = volume;
                                player.set_volume(0.0).await
                            } else {
                                let volume_before_mute = *state.volume_before_mute.lock().await;
                                player.set_volume(volume_before_mute.max(0.05)).await
                            }
                        }
                        _ => Ok(()),
                    };

                    if let Err(err) = result {
                        log::warn!("Failed to handle media key {:?}: {}", key, err);
                    } else {
                        let playback_state = player.get_state().await;
                        crate::media_session::mirror_state(&app_handle, &playback_state).await;
                        let _ = app_handle.emit("playback-state", playback_state);
                    }
                });
            })
        {
            log::debug!(
                "Global shortcut registration skipped for '{}': {}",
                shortcut_str,
                err
            );
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Opt-in Tokio task/scheduler introspection (see docs/PERFORMANCE.md).
    // Must run before Tauri creates its async runtime, since it installs the
    // `tracing` subscriber that records every task's spawn/poll events —
    // anything spawned before this line wouldn't be visible in `tokio-console`.
    // Off by default: the `tokio-console` feature and its `tokio_unstable`
    // cfg flag are dev/debug-only and never part of a release build.
    #[cfg(feature = "tokio-console")]
    console_subscriber::init();

    // Without this, every log::info!/warn!/error! call across the backend
    // (including reconcile-failure diagnostics) is a silent no-op — `log`
    // is just a facade and needs a registered backend to actually emit
    // anywhere. `RUST_LOG` (standard env_logger override, e.g.
    // `RUST_LOG=debug` or per-module `RUST_LOG=luminous_lib::tags=debug`)
    // always wins when set. Otherwise `--verbose`/`-v` (or `LUMINOUS_VERBOSE=1`)
    // keeps every level for the whole run; without it, startup logs at `info`
    // so the terminal shows what's happening while the app comes up, then
    // drops to `warn`-and-up once `.setup()` finishes below — steady-state
    // chatter (e.g. the luminous-art:// protocol handler firing on every
    // cover art request) stays out of the terminal unless something breaks.
    let rust_log_explicit = std::env::var("RUST_LOG").is_ok();
    let verbose = std::env::var("LUMINOUS_VERBOSE").is_ok()
        || std::env::args().any(|a| a == "--verbose" || a == "-v");
    let default_filter = if verbose { "debug" } else { "info" };
    let mut logger_builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_filter));
    if !rust_log_explicit {
        // Malformed/spliced MP3s (common with ad-stitched podcasts, mp3-joiner
        // output, etc.) make symphonia's decoder start mid-bitstream, which it
        // handles by zeroing the affected granule and logging at `warn` — this
        // is already-recovered, inherent-to-the-source-file noise, not
        // something the user or Luminous can act on, so keep it out of the
        // logs unless someone explicitly asked for codec-level debugging via
        // `RUST_LOG`.
        logger_builder.filter_module("symphonia_bundle_mp3::layer3", log::LevelFilter::Error);
        // Same for lofty: its `warn`s ("MPEG: Using bitrate to estimate
        // duration", duplicate ID3v2 frames, empty MP4 atoms, ID3v2 in FLAC)
        // describe quirks it already recovered from, once per file per scan,
        // and never name the file. Real read failures come back as `Err`,
        // which Luminous logs itself with the path.
        logger_builder.filter_module("lofty", log::LevelFilter::Error);
    }
    logger_builder.init();

    // Opt-in remote-devtools channel for agents without a GUI (e.g. Claude
    // Code) to inspect the running webview — see `remote_devtools_enabled()`.
    // `WEBKIT_INSPECTOR_HTTP_SERVER` (not `WEBKIT_INSPECTOR_SERVER`, which
    // speaks WebKit's raw remote-inspector protocol over a websocket meant
    // for another WebKit-based inspector client) is the variant that serves
    // the Web Inspector frontend itself as plain HTTP, browsable from any
    // browser.
    #[cfg(target_os = "linux")]
    if remote_devtools_enabled() {
        std::env::set_var(
            "WEBKIT_INSPECTOR_HTTP_SERVER",
            format!("127.0.0.1:{}", REMOTE_DEVTOOLS_PORT),
        );
    }

    // On Windows, Chromium's CalculateNativeWinOcclusion feature puts the
    // WebView2 rendering pipeline into a suspended/discarded state when the
    // window is minimized or occluded for a period of time. Upon restore,
    // WebView2 fails to resume/repaint, leaving a blank window. Disabling
    // native window occlusion calculation keeps the rendering context intact.
    #[cfg(target_os = "windows")]
    {
        let key = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
        let current = std::env::var(key).unwrap_or_default();
        let current = with_webview2_occlusion_disabled(&current);
        let current = with_webview2_backgrounding_disabled(&current);
        let current = if remote_devtools_enabled() {
            with_webview2_remote_debugging(&current, REMOTE_DEVTOOLS_PORT)
        } else {
            current
        };
        std::env::set_var(key, current);
    }

    tauri::Builder::default()
        // Asynchronous so the file read happens off the main thread: on Windows
        // WebView2 a synchronous protocol handler runs on the UI thread, and
        // reading folder art from a slow or sleeping library drive would stall
        // the window (see `covermanager::serve_art_request`).
        .register_asynchronous_uri_scheme_protocol(
            "luminous-art",
            move |ctx, request, responder| {
                let covers_dir =
                    crate::paths::resolve_app_data_dir(ctx.app_handle()).join("covers");
                let uri = request.uri().to_string();
                tauri::async_runtime::spawn_blocking(move || {
                    responder.respond(crate::covermanager::serve_art_request(&covers_dir, &uri));
                });
            },
        )
        // Decrypted add-on overlay assets, served from memory only (#1413).
        .register_asynchronous_uri_scheme_protocol(
            "luminous-addon",
            move |_ctx, request, responder| {
                let uri = request.uri().to_string();
                tauri::async_runtime::spawn_blocking(move || {
                    responder.respond(crate::addons::serve_request(&uri));
                });
            },
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin({
            let window_state = tauri_plugin_window_state::Builder::default().with_state_flags(
                tauri_plugin_window_state::StateFlags::all()
                    & !tauri_plugin_window_state::StateFlags::VISIBLE,
            );
            match crate::paths::isolated_window_state_file() {
                Some(file) => window_state.with_filename(file),
                None => window_state,
            }
            .build()
        })
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // `MacosLauncher::LaunchAgent` is required by the plugin's cross-platform
        // API but inert on the platforms Luminous actually ships (Windows/Linux) —
        // it only takes effect on a macOS build, which this project doesn't target.
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(build_prevent_default_plugin())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            tray::restore_main_window(app);

            // A file association (or `luminous <file>` invocation) launched a
            // second instance; forward the opened paths to the running app.
            let opened_paths: Vec<String> = args
                .iter()
                .skip(1)
                .filter(|arg| {
                    let path = std::path::Path::new(arg);
                    if !path.is_file() {
                        return false;
                    }
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    crate::playlist_parsers::PlaylistFormat::from_path(path).is_some()
                        || crate::collection::AUDIO_EXTENSIONS.contains(&ext.as_str())
                })
                .cloned()
                .collect();

            if !opened_paths.is_empty() {
                let _ = app.emit("open-file-request", opened_paths);
            }
        }))
        .setup(move |app| {
            let app_data_dir = crate::paths::resolve_app_data_dir(app);
            diagnostics::install_panic_hook(app_data_dir);

            let db = Arc::new(
                Database::new(crate::paths::resolve_app_data_dir(app))
                    .expect("failed to initialize database"),
            );

            // Portable mode (#1403): watched folders on the executable's own
            // drive follow it to a new drive letter. Runs before the player,
            // playlists, watcher and sidecar read any library path.
            if crate::paths::resolve_app_data_dir_info(app).is_portable {
                if let Some(volume) = crate::paths::detect_executable_base_dir()
                    .and_then(|dir| collection::relocate::volume_root(&dir))
                {
                    match db.pool.get().map_err(anyhow::Error::from).and_then(|conn| {
                        collection::relocate::relink_portable_volume(&conn, &volume)
                    }) {
                        Ok(relinked) => {
                            for (from, to) in relinked {
                                log::info!("Portable drive moved: re-linked {from} to {to}");
                            }
                        }
                        Err(e) => log::error!("Portable drive re-link failed: {e:#}"),
                    }
                }
            }

            // `subsonic://` library paths are signed into stream URLs at open
            // time from the server's saved credentials (#1163).
            {
                let db = db.clone();
                audio::register_subsonic_resolver(move |path| {
                    let conn = db.pool.get().map_err(|e| e.to_string())?;
                    subsonic::resolve_stream_url(&conn, path).map_err(|e| e.to_string())
                });
            }

            // WebDAV song URLs are stored credential-free; the Basic auth
            // header comes from the saved server at open time (#1492).
            {
                let db = db.clone();
                audio::register_webdav_auth_resolver(move |url| {
                    let conn = db.pool.get().ok()?;
                    webdav::resolve_auth_header(&conn, url)
                });
            }

            // Graceful Store (MSIX) update handling (#744): register for
            // Restart Manager-driven relaunch, and fire a one-time "app
            // updated" OS notification if the previous launch's persisted
            // version differs from this one.
            #[cfg(target_os = "windows")]
            {
                restart_manager::register_for_restart();

                let info = crate::install_format::detect_install_format();
                if let Ok(conn) = db.pool.get() {
                    let stored: Option<String> = conn
                        .query_row(
                            "SELECT value FROM app_state WHERE key = 'launched_version'",
                            [],
                            |row| row.get(0),
                        )
                        .ok();
                    let current = env!("CARGO_PKG_VERSION");
                    log::debug!(
                        "MSIX update-notification check: format={:?} stored={:?} current={:?}",
                        info.format,
                        stored,
                        current
                    );
                    if restart_manager::should_notify_update(
                        &info.format,
                        stored.as_deref(),
                        current,
                    ) {
                        restart_manager::show_update_notification(
                            current,
                            restart_manager::load_update_template(&conn).as_deref(),
                        );
                    }
                }
            }

            let audio_engine = AudioEngine::new();
            restore_equalizer_from_db(&db, &audio_engine);

            let audio = Arc::new(Mutex::new(audio_engine));

            let minimize_to_tray = Arc::new(std::sync::atomic::AtomicBool::new(false));
            if let Ok(conn) = db.pool.get() {
                if let Ok(value) = conn.query_row(
                    "SELECT value FROM app_state WHERE key = 'minimize_to_tray'",
                    [],
                    |row| row.get::<_, String>(0),
                ) {
                    minimize_to_tray.store(value == "true", std::sync::atomic::Ordering::Relaxed);
                }
            }

            let player = Arc::new(Mutex::new(Player::new(Arc::clone(&db), Arc::clone(&audio))));
            let scrobbler = Arc::new(scrobbler::ScrobblerManager::new(Arc::clone(&db)));
            scrobbler.trigger_flush();
            {
                let mut p = player.blocking_lock();
                p.set_scrobbler(Arc::clone(&scrobbler));
            }
            let volume_before_mute = Arc::new(Mutex::new(1.0));

            let manager = PlaylistManager::new(Arc::clone(&db)).expect("failed to init playlists");
            // Bootstrap the built-in Queue before the window loads, so the
            // frontend can treat its existence as a guarantee rather than a
            // condition to re-check at every call site.
            if let Err(e) = manager.queue() {
                log::error!("Failed to bootstrap Queue playlist: {e}");
            }
            // Rebuild curated-tag ("tag:") genre auto-playlists once right
            // after migrations run (see db.rs migration 19, #548), rather
            // than waiting for the next library scan — an upgrading user's
            // old bare-genre-name rows were just discarded by that
            // migration, so without this they'd see an empty genre
            // auto-playlist section until they happened to trigger a scan.
            if let Err(e) = manager.sync_all_auto_playlists() {
                log::error!("Failed to sync auto-playlists at startup: {e}");
            }
            if let Ok(conn) = db.pool.get() {
                if let Err(e) = pins::init_default_pins(&conn) {
                    log::error!("Failed to initialize default pinned items: {e}");
                }
            }
            let playlists = Arc::new(Mutex::new(manager));

            let self_writes = Arc::new(collection::SelfWriteTracker::new());

            let cover_manager = Arc::new(
                CoverManager::new(Arc::clone(&db), crate::paths::resolve_app_data_dir(app))
                    .with_self_writes(Arc::clone(&self_writes)),
            );

            // Spawn real-time visualizer spectrum emission loop (Tokio)
            spawn_visualizer_loop(app.handle().clone(), Arc::clone(&audio));

            // Spawn Tokio/UI stall watchdog — see #1002.
            stall_monitor::spawn();

            let args: Vec<String> = std::env::args().collect();
            let startup_path = if args.len() > 1 {
                let p = &args[1];
                let path = std::path::Path::new(p);
                if path.exists() && path.is_file() {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if crate::playlist_parsers::PlaylistFormat::from_path(path).is_some()
                        || crate::collection::AUDIO_EXTENSIONS.contains(&ext.as_str())
                    {
                        Some(p.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let watcher = Arc::new(parking_lot::Mutex::new(None));

            // SMTC needs an HWND on Windows to register for our window (#576).
            #[cfg(target_os = "windows")]
            let media_hwnd: Option<*mut std::ffi::c_void> = app
                .get_webview_window("main")
                .and_then(|w| w.hwnd().ok())
                .map(|h| h.0);
            #[cfg(not(target_os = "windows"))]
            let media_hwnd: Option<*mut std::ffi::c_void> = None;

            let media_session = media_session::spawn(app.handle().clone(), media_hwnd);

            let watcher_paused = Arc::new(std::sync::atomic::AtomicU32::new(0));
            let musicbrainz = Arc::new(musicbrainz::MusicBrainzManager::new(Arc::clone(&db)));

            let state = AppState {
                db,
                audio,
                player,
                volume_before_mute,
                playlists,
                cover_manager,
                watcher,
                watcher_paused,
                self_writes,
                startup_file: Mutex::new(startup_path),
                media_session,
                minimize_to_tray,
                scrobbler,
                musicbrainz,
                remote_auto_sync: Arc::new(remote_scheduler::AutoSyncScheduler::new()),
                hierarchy_sidecar: Arc::new(hierarchy_sidecar::HierarchySidecar::new()),
                addons: commands::addons::build_manager(app.handle()),
            };

            crate::collection::start_watcher(app.handle().clone(), &state);

            // Start background EBU R128 loudness analyzer (#77)
            crate::loudness::spawn_background_analyzer(app.handle().clone(), Arc::clone(&state.db));

            app.manage(state);
            let managed_state = app.state::<AppState>();

            // Start each enabled remote server's periodic auto-sync timer (WebDAV #1082, OpenSubsonic #1162).
            managed_state.remote_auto_sync.start_all_from_db(
                app.handle().clone(),
                Arc::clone(&managed_state.db),
                Arc::clone(&managed_state.cover_manager),
            );

            // Self-healing sidecar artwork sweep resume (#1274): if the user has
            // opt-in folder artwork enabled, resume sweeping any un-exported cached
            // covers or artist images in the background.
            let is_save_artwork_enabled = managed_state
                .db
                .pool
                .get()
                .map(|conn| {
                    crate::commands::settings::load_ui_preferences(&conn).save_artwork_to_folders
                })
                .unwrap_or(false);

            if is_save_artwork_enabled {
                let db_clone = Arc::clone(&managed_state.db);
                let cover_mgr = Arc::clone(&managed_state.cover_manager);
                let covers_dir = cover_mgr.covers_dir().to_path_buf();
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let has_pending = db_clone
                        .pool
                        .get()
                        .map(|conn| {
                            let has_albums = conn
                                .query_row(
                                    "SELECT 1 FROM songs
                             WHERE art_automatic LIKE 'album-%'
                               AND (source IN (1, 2) OR source IS NULL)
                               AND album IS NOT NULL AND TRIM(album) != ''
                               AND path IS NOT NULL
                             LIMIT 1",
                                    [],
                                    |_| Ok(true),
                                )
                                .unwrap_or(false);
                            let has_artists = conn
                                .query_row(
                                    "SELECT 1 FROM artist_profiles
                             WHERE fetched_image_filename IS NOT NULL
                                OR fetched_logo_filename IS NOT NULL
                                OR fetched_background_filename IS NOT NULL
                             LIMIT 1",
                                    [],
                                    |_| Ok(true),
                                )
                                .unwrap_or(false);
                            has_albums || has_artists
                        })
                        .unwrap_or(false);

                    if has_pending {
                        log::info!("Resuming pending artwork sidecar sweep in background...");
                        let _ = crate::commands::collection::sweep_artwork_to_folders_core(
                            db_clone,
                            cover_mgr,
                            covers_dir,
                            move |payload| {
                                let _ = app_handle.emit("artwork-sweep-progress", payload);
                            },
                        )
                        .await;
                    }
                });
            }

            // Spawn position tick loop (Tokio). Spawned after app.manage()
            // above since it calls media_session::mirror_state(), which
            // reaches into app.state::<AppState>() — doing this before
            // manage() panics ("state() called before manage()") if a tick
            // fires that early.
            spawn_position_tick_loop(
                app.handle().clone(),
                Arc::clone(&managed_state.audio),
                Arc::clone(&managed_state.player),
            );

            // Spawn event receiver loop (OS thread). Spawned after
            // app.manage() for the same reason as the tick loop above — its
            // handler also reaches app.state::<AppState>() via
            // media_session::mirror_state().
            spawn_audio_event_loop(
                app.handle().clone(),
                Arc::clone(&managed_state.audio),
                Arc::clone(&managed_state.player),
            );

            // Spawn loopback HTTP bridge server for assistant and MCP playback control
            crate::bridge::spawn_bridge_server(app.handle().clone());

            if let Err(e) = tray::init(app) {
                log::warn!("Failed to initialize system tray: {e}");
            }

            #[cfg(target_os = "windows")]
            taskbar::init(app);

            register_media_shortcuts(app);

            // Keep every dynamic playlist's membership in line with its
            // definition the moment the library or song stats change —
            // additions from scans/tag edits and stat-driven moves
            // (favourite/unfavourite, deep-cut played) all land immediately.
            // `reconcile_and_sync` coalesces event bursts into at most one
            // follow-up pass, and holds the playlists mutex only to apply.
            {
                use tauri::Listener;
                let handle = app.handle().clone();
                for event in ["library-changed", "song-stats-changed"] {
                    let handle = handle.clone();
                    app.listen(event, move |_| {
                        tauri::async_runtime::spawn(playlist::reconcile_and_sync(handle.clone()));
                    });
                }
            }

            // Keep the persisted Genres curation hierarchy (#545) in step
            // with newly-seen or vanished tag names whenever the library
            // changes (scans, tag edits, bulk merge/delete). Belt-and-braces
            // alongside `get_tag_hierarchy`'s own reconcile-on-read: this is
            // what lets an already-open Genres tab pick up a change without
            // the user having to leave and reopen it.
            {
                use tauri::Listener;
                let handle = app.handle().clone();
                app.listen("library-changed", move |_| {
                    tauri::async_runtime::spawn(tags::reconcile_hierarchy_and_notify(
                        handle.clone(),
                    ));
                    tauri::async_runtime::spawn(tags::reconcile_artist_hierarchy_and_notify(
                        handle.clone(),
                    ));
                });
            }

            // Load the shared hierarchy sidecar (#1312) off the main thread —
            // the default library may be a slow network share. Registered
            // after the `library-changed` reconcile listener so a reconcile
            // it triggers is written back through the attached sidecar.
            {
                let handle = app.handle().clone();
                let _ = std::thread::Builder::new()
                    .name("luminous-hierarchy-init".into())
                    .spawn(move || hierarchy_sidecar::init(&handle));
            }

            // Startup is done: everything above (DB/migrations, playlist
            // bootstrap, watcher, tray, bridge server) has had a chance to
            // log at `info`. Drop to `warn`-and-up for the rest of the run
            // unless the user asked for full verbosity via `RUST_LOG` or
            // `--verbose`/`LUMINOUS_VERBOSE`.
            if !rust_log_explicit && !verbose {
                log::set_max_level(log::LevelFilter::Warn);
            }

            Ok(())
        })
        // Instrumented so stall warnings can name the commands around them (#1002).
        .invoke_handler(stall_monitor::instrument(tauri::generate_handler![
            // Collection commands
            commands::collection::scan_directories,
            commands::collection::rescan_songs,
            commands::collection::prune_missing_songs,
            commands::collection::add_directory,
            commands::collection::remove_directory,
            commands::collection::relocate_directory,
            commands::collection::get_directories,
            commands::collection::update_directory_metadata,
            commands::collection::get_library_stats,
            commands::collection::search_songs,
            commands::collection::get_library_snapshot,
            commands::collection::finish_scan,
            commands::collection::get_songs_by_album,
            commands::collection::get_songs_by_artist,
            commands::collection::get_compilations_by_artist,
            commands::collection::get_top_artists,
            commands::collection::get_favourite_songs,
            commands::collection::get_recently_added_songs,
            commands::collection::get_most_played_songs,
            commands::collection::get_recently_played,
            commands::collection::get_recently_played_songs,
            commands::collection::clear_play_history,
            commands::collection::get_recently_added,
            commands::collection::get_featured_albums,
            commands::collection::get_top_albums,
            commands::collection::get_artist_profile,
            commands::collection::set_artist_profile,
            commands::collection::get_all_artist_profiles,
            commands::collection::get_album_profile,
            commands::collection::set_album_profile,
            commands::collection::get_all_album_profiles,
            commands::collection::open_artist_bio_file,
            commands::collection::read_artist_bio_file,
            commands::collection::open_album_bio_file,
            commands::collection::read_album_bio_file,
            commands::collection::retrieve_album_details,
            commands::collection::retrieve_artist_details,
            commands::collection::retrieve_artist_image,
            commands::collection::retrieve_album_art,
            commands::collection::sweep_artwork_to_folders,
            commands::collection::has_fanart_env_key,
            commands::collection::validate_fanart_api_key,
            commands::collection::get_artist_tags_overview,
            commands::collection::set_songs_not_included,
            commands::collection::get_songs_missing_musicbrainz_id,
            commands::collection::get_songs_missing_metadata,
            // Playback commands
            commands::player::play_song,
            commands::player::play_songs,
            commands::player::play_playlist_item,
            commands::player::play_playlist_item_by_uuid,
            commands::player::open_and_play,
            commands::player::get_startup_file,
            commands::player::add_songs_to_queue,
            commands::player::add_paths_to_queue,
            commands::player::is_shift_key_held,
            commands::player::pause,
            commands::player::resume,
            commands::player::stop,
            commands::player::next_track,
            commands::player::previous_track,
            commands::player::seek_to,
            commands::player::set_volume,
            commands::player::get_playback_state,
            commands::player::refresh_playback_queue,
            commands::player::set_shuffle_mode,
            commands::player::set_repeat_mode,
            commands::player::set_auto_continue,
            commands::player::get_audio_pipeline_info,
            // Pinned Home shelf commands (#222)
            commands::pins::pin_item,
            commands::pins::unpin_item,
            commands::pins::get_pinned_items,
            commands::pins::reorder_pinned_items,
            // Social share card export (#97)
            commands::share::save_share_card_image,
            commands::share::copy_share_card_image,
            // Playlist commands
            commands::playlist::validate_playlist_name,
            commands::playlist::create_playlist,
            commands::playlist::delete_playlist,
            commands::playlist::rename_playlist,
            commands::playlist::get_playlists,
            commands::playlist::sync_all_auto_playlists,
            commands::playlist::get_songs_by_decade,
            commands::playlist::get_songs_by_bpm,
            commands::playlist::get_songs_by_artist_tag,
            commands::playlist::get_playlists_by_artist,
            commands::playlist::get_playlist_tracks,
            commands::playlist::add_to_playlist,
            commands::playlist::remove_from_playlist,
            commands::playlist::deduplicate_playlist,
            commands::playlist::reorder_playlist_item,
            commands::playlist::reorder_playlist_item_by_uuid,
            commands::playlist::reorder_playlist_items,
            commands::playlist::clear_playlist,
            commands::playlist::undo_playlist,
            commands::playlist::redo_playlist,
            commands::playlist::import_playlist,
            commands::playlist::export_playlist,
            commands::playlist::set_playlist_population_mode,
            commands::playlist::set_playlist_dynamic_spec,
            commands::playlist::set_playlist_dynamic_config,
            commands::playlist::refresh_auto_playlist,
            commands::playlist::refresh_all_auto_playlists,
            // Cover Art commands
            commands::cover::get_cover_art_uri,
            commands::cover::fetch_remote_cover,
            commands::cover::get_extended_artwork_for_song,
            commands::cover::get_extended_artwork_for_artist,
            commands::cover::open_artwork_path,
            // Visualizer commands
            commands::visualizer::get_waveform_data,
            commands::visualizer::get_band_waveform_data,
            commands::visualizer::set_spectrum_enabled,
            // Equalizer commands
            commands::equalizer::get_equalizer_state,
            commands::equalizer::apply_equalizer_config,
            commands::equalizer::get_parametric_response,
            commands::equalizer::get_eq_preset_previews,
            commands::equalizer::reset_parametric_bands,
            commands::equalizer::load_equalizer_preset,
            commands::equalizer::list_eq_presets,
            commands::equalizer::save_eq_user_preset,
            commands::equalizer::import_parametric_profile,
            commands::equalizer::read_eq_profile_file,
            commands::equalizer::export_parametric_profile,
            commands::equalizer::rename_eq_user_preset,
            commands::equalizer::delete_eq_user_preset,
            // Loudness normalization commands
            commands::loudness::get_loudness_settings,
            commands::loudness::set_loudness_settings,
            commands::loudness::get_loudness_analysis_remaining,
            // Lyrics commands
            commands::lyrics::get_lyrics,
            commands::lyrics::save_lyrics,
            commands::lyrics::set_instrumental,
            commands::lyrics::get_lyrics_offset,
            commands::lyrics::set_lyrics_offset,
            // Details pane context enrichment (#23)
            commands::context::get_song_context,
            commands::context::get_artist_events,
            commands::context::is_context_enrichment_enabled,
            commands::context::set_online_enabled,
            // Tag Editor commands
            commands::tageditor::get_song_details,
            commands::tageditor::save_song_tags,
            commands::tageditor::save_album_tags,
            commands::tageditor::clear_song_cover_art,
            commands::tageditor::clear_album_cover_art,
            commands::tageditor::open_song_folder,
            // MusicBrainz Picard bridge commands (#367)
            commands::picard::open_in_picard,
            commands::picard::get_picard_path,
            // Genre/tag browsing commands (#224) — read the existing
            // songs.genre column above; editing goes through save_song_tags.
            commands::tags::get_songs_by_tag,
            commands::tags::get_tags_overview,
            commands::tags::get_songs_by_curated_tag,
            commands::tags::get_songs_without_genre,
            // Persisted Genres curation hierarchy (#545)
            commands::tags::get_tag_hierarchy,
            commands::tags::set_tag_group_color,
            commands::tags::reparent_tag,
            commands::tags::promote_tag,
            commands::tags::demote_group_to_child,
            commands::tags::reorder_tag_in_group,
            commands::tags::merge_tags,
            commands::tags::delete_tags,
            // Persisted Artist Tags curation hierarchy (#1105)
            commands::tags::get_artist_tag_hierarchy,
            commands::tags::set_artist_group_color,
            commands::tags::reparent_artist_tag,
            commands::tags::promote_artist_tag,
            commands::tags::demote_artist_group_to_child,
            commands::tags::reorder_artist_tag_in_group,
            commands::tags::create_artist_tag_group,
            commands::tags::merge_artist_tags,
            commands::tags::delete_artist_tags,
            commands::tags::get_default_library,
            commands::tags::set_default_library,
            // Theme commands (#165)
            commands::addons::refresh_addons,
            commands::addons::acquire_addon,
            commands::theme::import_theme,
            commands::theme::export_theme,
            // Settings commands
            commands::settings::set_app_setting,
            commands::settings::open_default_apps_settings,
            commands::settings::get_all_app_settings,
            commands::settings::get_ui_preferences,
            commands::settings::set_ui_preferences,
            commands::settings::get_commit_hash,
            commands::settings::get_audio_setting_ranges,
            commands::settings::get_db_schema_status,
            commands::settings::get_fade_settings,
            commands::settings::set_fade_settings,
            commands::settings::set_native_labels,
            commands::settings::get_minimize_to_tray_enabled,
            commands::settings::set_minimize_to_tray_enabled,
            commands::settings::get_autostart_enabled,
            commands::settings::set_autostart_enabled,
            commands::diagnostics::log_frontend_error,
            commands::diagnostics::export_diagnostics,
            commands::diagnostics::get_data_directory_info,
            install_format::get_install_format,
            // Scrobbler commands (#83)
            commands::scrobbler::get_scrobbler_settings,
            commands::scrobbler::set_scrobbler_settings,
            commands::scrobbler::validate_listenbrainz_token,
            commands::scrobbler::get_scrobble_cache_status,
            commands::scrobbler::flush_scrobble_cache,
            commands::scrobbler::toggle_scrobble_pause,
            commands::scrobbler::sync_ratings_to_listenbrainz,
            commands::scrobbler::get_discord_status,
            // MusicBrainz OAuth commands (#1388)
            commands::musicbrainz::start_musicbrainz_login,
            commands::musicbrainz::submit_musicbrainz_auth_code,
            commands::musicbrainz::cancel_musicbrainz_login,
            commands::musicbrainz::get_musicbrainz_auth_state,
            commands::musicbrainz::get_musicbrainz_user_stats,
            commands::musicbrainz::logout_musicbrainz,
            commands::musicbrainz::get_musicbrainz_app_credentials,
            commands::musicbrainz::set_musicbrainz_app_credentials,
            // Stats commands
            commands::stats::set_song_rating,
            commands::stats::set_song_loved,
            commands::stats::set_album_rating,
            commands::stats::get_stats_summary,
            commands::stats::get_listening_activity,
            commands::stats::get_stats_exclusions,
            commands::stats::set_stats_excluded,
            // Organizer commands
            commands::organizer::preview_organize,
            commands::organizer::apply_organize,
            commands::organizer::get_organize_config,
            commands::organizer::set_organize_config,
            // WebDAV commands (#682)
            commands::subsonic::list_subsonic_servers,
            commands::subsonic::save_subsonic_server,
            commands::subsonic::delete_subsonic_server,
            commands::subsonic::test_subsonic_connection,
            commands::subsonic::get_subsonic_auth_support,
            commands::subsonic::check_subsonic_connection,
            commands::subsonic::sync_subsonic_server,
            commands::webdav::list_webdav_servers,
            commands::webdav::save_webdav_server,
            commands::webdav::delete_webdav_server,
            commands::webdav::test_webdav_connection,
            commands::webdav::check_webdav_connection,
            commands::webdav::sync_webdav_server,
            // Window & Miniplayer commands
            commands::window::is_remote_devtools_enabled,
            commands::window::geometry_capture_supported,
            commands::window::enter_miniplayer_mode,
            commands::window::exit_miniplayer_mode,
            commands::window::move_window_to_preset,
            commands::window::get_window_geometry,
            commands::window::webview_gpu_compositing,
            commands::window::start_window_drag,
            commands::window::start_window_resize,
        ]))
        .run(tauri::generate_context!())
        .expect("error while running Luminous");
}

#[cfg(test)]
mod startup_rendering_workaround_tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn test_webkitgtk_gpu_rendering_disabled_by_env() {
        use std::ffi::OsString;
        assert!(!webkitgtk_gpu_rendering_disabled_by(|_| None));
        assert!(webkitgtk_gpu_rendering_disabled_by(|key| {
            (key == "WEBKIT_DISABLE_DMABUF_RENDERER").then(|| OsString::from("1"))
        }));
        assert!(!webkitgtk_gpu_rendering_disabled_by(|_| Some(
            OsString::from("0")
        )));
    }

    #[test]
    fn test_webview2_occlusion_flag_added_to_empty_value() {
        assert_eq!(
            with_webview2_occlusion_disabled(""),
            "--disable-features=CalculateNativeWinOcclusion"
        );
    }

    #[test]
    fn test_webview2_occlusion_flag_appended_to_existing_args() {
        assert_eq!(
            with_webview2_occlusion_disabled("--some-other-flag"),
            "--some-other-flag --disable-features=CalculateNativeWinOcclusion"
        );
    }

    #[test]
    fn test_webview2_occlusion_flag_not_duplicated_if_already_present() {
        let already_set = "--disable-features=CalculateNativeWinOcclusion";
        assert_eq!(with_webview2_occlusion_disabled(already_set), already_set);

        let already_set_with_other = "--foo --disable-features=CalculateNativeWinOcclusion --bar";
        assert_eq!(
            with_webview2_occlusion_disabled(already_set_with_other),
            already_set_with_other
        );
    }

    #[test]
    fn test_webview2_occlusion_flag_merged_into_existing_disable_features() {
        assert_eq!(
            with_webview2_occlusion_disabled("--disable-features=msWebOOUI,msPdfOOUI"),
            "--disable-features=msWebOOUI,msPdfOOUI,CalculateNativeWinOcclusion"
        );

        assert_eq!(
            with_webview2_occlusion_disabled("--foo --disable-features=msWebOOUI --bar"),
            "--foo --disable-features=msWebOOUI,CalculateNativeWinOcclusion --bar"
        );
    }

    #[test]
    fn test_webview2_backgrounding_flags_added_to_empty_value() {
        assert_eq!(
            with_webview2_backgrounding_disabled(""),
            "--disable-renderer-backgrounding --disable-backgrounding-occluded-windows --disable-background-timer-throttling"
        );
    }

    #[test]
    fn test_webview2_backgrounding_flags_appended_to_existing_args() {
        assert_eq!(
            with_webview2_backgrounding_disabled("--disable-features=CalculateNativeWinOcclusion"),
            "--disable-features=CalculateNativeWinOcclusion --disable-renderer-backgrounding --disable-backgrounding-occluded-windows --disable-background-timer-throttling"
        );
    }

    #[test]
    fn test_webview2_backgrounding_flags_not_duplicated_if_already_present() {
        let already_set = "--disable-renderer-backgrounding --disable-backgrounding-occluded-windows --disable-background-timer-throttling";
        assert_eq!(
            with_webview2_backgrounding_disabled(already_set),
            already_set
        );
    }

    #[test]
    fn test_webview2_backgrounding_flags_only_add_missing_ones() {
        assert_eq!(
            with_webview2_backgrounding_disabled("--disable-renderer-backgrounding"),
            "--disable-renderer-backgrounding --disable-backgrounding-occluded-windows --disable-background-timer-throttling"
        );
    }

    #[test]
    fn test_webview2_remote_debugging_flag_added_to_empty_value() {
        assert_eq!(
            with_webview2_remote_debugging("", 9222),
            "--remote-debugging-port=9222"
        );
    }

    #[test]
    fn test_webview2_remote_debugging_flag_appended_to_existing_args() {
        assert_eq!(
            with_webview2_remote_debugging("--disable-renderer-backgrounding", 9222),
            "--disable-renderer-backgrounding --remote-debugging-port=9222"
        );
    }

    #[test]
    fn test_webview2_remote_debugging_flag_not_duplicated_if_already_present() {
        let already_set = "--remote-debugging-port=9222";
        assert_eq!(
            with_webview2_remote_debugging(already_set, 9222),
            already_set
        );

        let already_set_with_other = "--foo --remote-debugging-port=9222 --bar";
        assert_eq!(
            with_webview2_remote_debugging(already_set_with_other, 9222),
            already_set_with_other
        );
    }
}
