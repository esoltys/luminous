//! Windows taskbar thumbnail toolbar (`ITaskbarList3`) — #852.
//!
//! Adds Previous/Play-Pause/Next buttons beneath the taskbar thumbnail
//! preview of the main window, and replaces that preview (and the larger
//! hover flyout) with the current track's cover art via DWM's "iconic
//! bitmap" mechanism, so hovering the taskbar icon reads as a "now playing"
//! card rather than a screenshot of whatever tab happens to be open.
//!
//! Both pieces are best-effort, mirroring `media_session::windows` (SMTC):
//! if COM/DWM setup fails, this logs a warning and Luminous just runs
//! without the feature. Button clicks are dispatched through the same
//! `state.player` command paths as the tray menu (`tray.rs`) and global
//! media-key shortcuts (`lib.rs::register_media_shortcuts`).
//!
//! `ITaskbarList3` is an apartment-threaded COM object: it's created here in
//! `.setup()` (the main/UI thread, which owns the window's message pump) and
//! must only ever be called from that same thread again. The window is
//! subclassed via `SetWindowSubclass` (comctl32) rather than replacing its
//! `WNDPROC` outright, since that's the additive, chaining-safe way to hook
//! messages without clobbering tao's own window procedure — the same
//! consideration that led `media_session::windows` to run SMTC on its own
//! dedicated COM thread instead. Any code that isn't already running on the
//! main thread (e.g. the async task that decodes cover art) hands taskbar
//! button/icon updates back to it via `AppHandle::run_on_main_thread`.

use crate::models::{PlayState, PlaybackState};
use crate::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Listener, Manager};
use windows::core::{w, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmInvalidateIconicBitmaps, DwmSetIconicLivePreviewBitmap, DwmSetIconicThumbnail,
    DwmSetWindowAttribute, DWMWA_FORCE_ICONIC_REPRESENTATION, DWMWA_HAS_ICONIC_BITMAP,
};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, GetDC, RedrawWindow, ReleaseDC, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE,
    RDW_UPDATENOW,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{
    DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, THBF_DISABLED, THBF_ENABLED,
    THBN_CLICKED, THB_FLAGS, THB_ICON, THB_TOOLTIP, THUMBBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, GetClientRect, GetSystemMetrics, RegisterWindowMessageW, HICON, ICONINFO,
    SIZE_RESTORED, SM_CXSMICON, WM_COMMAND, WM_SIZE,
};

/// Not currently exported by the `windows` crate's `Win32_Graphics_Dwm`
/// bindings — values are stable Win32 constants from `dwmapi.h`.
const WM_DWMSENDICONICTHUMBNAIL: u32 = 0x0323;
const WM_DWMSENDICONICLIVEPREVIEWBITMAP: u32 = 0x0326;

const BTN_PREVIOUS: u32 = 100;
const BTN_PLAY_PAUSE: u32 = 101;
const BTN_NEXT: u32 = 102;

/// Arbitrary but stable id identifying our subclass among any others chained
/// on the same window (comctl32 requires each subclass to register a unique
/// id) — the issue number doubles as a memorable, collision-unlikely value.
const SUBCLASS_ID: usize = 852;

#[derive(Clone, Copy)]
enum ButtonAction {
    Previous,
    PlayPause,
    Next,
}

fn button_action(id: u32) -> Option<ButtonAction> {
    match id {
        BTN_PREVIOUS => Some(ButtonAction::Previous),
        BTN_PLAY_PAUSE => Some(ButtonAction::PlayPause),
        BTN_NEXT => Some(ButtonAction::Next),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct ButtonIcons {
    previous: HICON,
    play: HICON,
    pause: HICON,
    next: HICON,
}

#[derive(Default)]
struct NowPlayingArt {
    song_id: Option<i64>,
    image: Option<Arc<image::DynamicImage>>,
}

struct TaskbarContext {
    app: AppHandle,
    hwnd: HWND,
    /// `None` until a taskbar button actually exists for our window (see
    /// `try_register_thumbbar`) — `ThumbBarAddButtons` is a no-op if called
    /// before that, which is exactly what a window created hidden
    /// (`"visible": false`, shown later from the frontend) hits if it's
    /// called eagerly at `.setup()` time.
    taskbar: parking_lot::Mutex<Option<ITaskbarList3>>,
    icons: ButtonIcons,
    /// The most recently applied (playing, has_song) pair, so a taskbar
    /// button created *after* playback already started (or after Explorer
    /// restarts and re-broadcasts `TaskbarButtonCreated`) can be seeded
    /// correctly instead of starting from "no track loaded".
    last_known_state: parking_lot::Mutex<(bool, bool)>,
    now_playing: parking_lot::Mutex<NowPlayingArt>,
    fallback_art: Arc<image::DynamicImage>,
    taskbar_button_created_msg: u32,
    /// Bumped once per `apply_playback_state` call. Lets an art-decode task
    /// that's still running when a *newer* track change comes in tell it's
    /// been superseded and skip writing stale art into `now_playing` —
    /// otherwise two rapid track changes could race and leave the slower
    /// (now outdated) decode as the one that "wins".
    art_request_seq: std::sync::atomic::AtomicU64,
}

// SAFETY: `taskbar` (an apartment-threaded COM pointer, behind its own
// Mutex) and `icons` (raw HICON handles) are only ever touched from the
// main thread — the thread that created them in `try_init`/
// `try_register_thumbbar`, and the only thread the window's message pump
// (and therefore every `SetWindowSubclass` callback) runs on. Code on other
// threads reaches this struct only to read/write `now_playing`/
// `last_known_state`, each guarded by its own `Mutex`, or to hand work back
// to the main thread via `AppHandle::run_on_main_thread`.
unsafe impl Send for TaskbarContext {}
unsafe impl Sync for TaskbarContext {}

/// Best-effort: adds the thumbnail toolbar and iconic (cover art) preview to
/// the main window. Logs and returns on any failure — Luminous runs fine
/// without this, the same way it does when SMTC/MPRIS init fails.
pub fn init(app: &tauri::App) {
    if let Err(e) = try_init(app) {
        log::warn!("Failed to initialize taskbar thumbnail toolbar: {e:?}");
    }
}

fn try_init(app: &tauri::App) -> windows::core::Result<()> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    let Ok(raw_hwnd) = window.hwnd() else {
        return Ok(());
    };
    let hwnd = HWND(raw_hwnd.0);

    let icon_size = (unsafe { GetSystemMetrics(SM_CXSMICON) }).max(16) as u32;
    let icons = ButtonIcons {
        previous: build_icon(ButtonGlyph::Previous, icon_size)?,
        play: build_icon(ButtonGlyph::Play, icon_size)?,
        pause: build_icon(ButtonGlyph::Pause, icon_size)?,
        next: build_icon(ButtonGlyph::Next, icon_size)?,
    };

    force_iconic_representation(hwnd)?;

    // Registered once here (rather than hardcoding WM_APP+N) so Explorer can
    // tell every top-level window when a taskbar button becomes available
    // for it — see `try_register_thumbbar`.
    let taskbar_button_created_msg = unsafe { RegisterWindowMessageW(w!("TaskbarButtonCreated")) };

    let ctx = Box::new(TaskbarContext {
        app: app.handle().clone(),
        hwnd,
        taskbar: parking_lot::Mutex::new(None),
        icons,
        last_known_state: parking_lot::Mutex::new((false, false)),
        now_playing: parking_lot::Mutex::new(NowPlayingArt::default()),
        fallback_art: load_fallback_art(),
        taskbar_button_created_msg,
        art_request_seq: std::sync::atomic::AtomicU64::new(0),
    });
    let ctx_ptr = Box::into_raw(ctx) as usize;

    unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, ctx_ptr) }.ok()?;

    // Covers the (unlikely, given the window starts hidden) case where a
    // taskbar button already exists by the time we get here; the normal
    // path is via `TaskbarButtonCreated` in `subclass_proc`.
    let ctx = unsafe { &*(ctx_ptr as *const TaskbarContext) };
    register_thumbbar(ctx);

    listen_playback_state(app, ctx_ptr);
    seed_playback_state(app.handle().clone(), ctx_ptr);

    Ok(())
}

/// (Re-)creates the `ITaskbarList3` for our window and registers the
/// thumbbar buttons, seeded with whatever playback state was last applied.
/// Called once a taskbar button actually exists for the window — see the
/// `TaskbarButtonCreated` handling in `subclass_proc` — and, in principle,
/// again if Explorer restarts and rebroadcasts that message.
fn register_thumbbar(ctx: &TaskbarContext) {
    if let Err(e) = try_register_thumbbar(ctx) {
        log::warn!("Failed to register taskbar thumbbar buttons: {e:?}");
    }
}

fn try_register_thumbbar(ctx: &TaskbarContext) -> windows::core::Result<()> {
    let taskbar: ITaskbarList3 =
        unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) }?;
    unsafe { taskbar.HrInit() }?;

    let (playing, has_song) = *ctx.last_known_state.lock();
    let buttons = build_buttons(ctx, playing, has_song);
    unsafe { taskbar.ThumbBarAddButtons(ctx.hwnd, &buttons) }?;

    *ctx.taskbar.lock() = Some(taskbar);
    Ok(())
}

fn force_iconic_representation(hwnd: HWND) -> windows::core::Result<()> {
    let enabled = BOOL(1);
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_HAS_ICONIC_BITMAP,
            &enabled as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<BOOL>() as u32,
        )?;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_FORCE_ICONIC_REPRESENTATION,
            &enabled as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<BOOL>() as u32,
        )?;
    }
    Ok(())
}

fn thumb_button(id: u32, icon: HICON, tip: &str, enabled: bool) -> THUMBBUTTON {
    let mut sz_tip = [0u16; 260];
    for (dst, c) in sz_tip.iter_mut().zip(tip.encode_utf16()) {
        *dst = c;
    }
    THUMBBUTTON {
        dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
        iId: id,
        iBitmap: 0,
        hIcon: icon,
        szTip: sz_tip,
        dwFlags: if enabled { THBF_ENABLED } else { THBF_DISABLED },
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    ref_data: usize,
) -> LRESULT {
    let ctx = &*(ref_data as *const TaskbarContext);

    if msg == ctx.taskbar_button_created_msg {
        register_thumbbar(ctx);
        // Not a message with a meaningful return value or default handling
        // of its own, but fall through to `DefSubclassProc` anyway rather
        // than swallowing a registered message other subclasses might care
        // about.
    }

    match msg {
        WM_COMMAND => {
            let notification = ((wparam.0 >> 16) & 0xFFFF) as u32;
            let button_id = (wparam.0 & 0xFFFF) as u32;
            if notification == THBN_CLICKED {
                if let Some(action) = button_action(button_id) {
                    dispatch_button_action(&ctx.app, action);
                }
                return LRESULT(0);
            }
        }
        WM_DWMSENDICONICTHUMBNAIL => {
            let width = ((lparam.0 >> 16) & 0xFFFF) as u32;
            let height = (lparam.0 & 0xFFFF) as u32;
            if width > 0 && height > 0 {
                send_iconic_thumbnail(ctx, hwnd, width, height);
            }
            return LRESULT(0);
        }
        WM_DWMSENDICONICLIVEPREVIEWBITMAP => {
            send_live_preview(ctx, hwnd);
            return LRESULT(0);
        }
        WM_SIZE if wparam.0 as u32 == SIZE_RESTORED => {
            invalidate_iconic_representation(hwnd);
        }
        _ => {}
    }

    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Forces DWM to drop any cached iconic (taskbar thumbnail / live preview)
/// representation of the window and repaint its real client area, then asks
/// WebView2 to redraw. Windows only sends `WM_SIZE`/`SIZE_RESTORED` when the
/// window transitions *out* of the minimized state, so this only fires on
/// restore, not on every resize.
///
/// `force_iconic_representation`'s `DWMWA_FORCE_ICONIC_REPRESENTATION` tells
/// DWM to always ask us for iconic bitmaps instead of taking its own live
/// capture of the window. That capture is also what DWM's compositor falls
/// back on to reconnect a window's surface after it's been minimized —
/// without it, restoring can leave the window's real content unpainted for
/// up to a minute until something else invalidates it (#884, a regression
/// introduced by #852's taskbar integration; the window itself never
/// previously opted into forced-iconic mode). Explicitly invalidating and
/// forcing a synchronous repaint on restore keeps that reconnection from
/// stalling.
fn invalidate_iconic_representation(hwnd: HWND) {
    unsafe {
        let _ = DwmInvalidateIconicBitmaps(hwnd);
        let _ = RedrawWindow(
            Some(hwnd),
            None,
            None,
            RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW | RDW_ERASE,
        );
    }
}

/// Mirrors `tray::handle_menu_event` / `lib.rs::register_media_shortcuts`:
/// lock the player, run the command, then propagate the result the same way
/// every other playback-control entry point does.
fn dispatch_button_action(app: &AppHandle, action: ButtonAction) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut player = state.player.lock().await;

        let result = match action {
            ButtonAction::PlayPause => {
                if player.get_state().await.state == PlayState::Playing {
                    player.pause().await
                } else {
                    player.resume().await
                }
            }
            ButtonAction::Next => {
                if let Some(stats) = player.note_manual_skip() {
                    let _ = app.emit("song-stats-changed", stats);
                }
                player.next_track().await
            }
            ButtonAction::Previous => player.previous_track().await,
        };

        if result.is_ok() {
            let playback_state = player.get_state().await;
            crate::media_session::mirror_state(&app, &playback_state).await;
            let _ = app.emit("playback-state", playback_state);
        }
    });
}

fn listen_playback_state(app: &tauri::App, ctx_ptr: usize) {
    let app_handle = app.handle().clone();
    app.listen("playback-state", move |event| {
        if let Ok(state) = serde_json::from_str::<PlaybackState>(event.payload()) {
            apply_playback_state(app_handle.clone(), ctx_ptr, state);
        }
    });
}

/// The listener above only fires on playback-state *changes*, so a session
/// restored with a track already loaded wouldn't show buttons/art until the
/// next play/pause — seed it once up front, the same way
/// `tray::seed_tooltip` seeds the tray tooltip.
fn seed_playback_state(app: AppHandle, ctx_ptr: usize) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let snapshot = state.player.lock().await.get_state().await;
        apply_playback_state(app, ctx_ptr, snapshot);
    });
}

fn apply_playback_state(app: AppHandle, ctx_ptr: usize, state: PlaybackState) {
    let has_song = state.current_song.is_some();
    let playing = state.state == PlayState::Playing;

    // Fast, no I/O: sync the thumbbar buttons on the main thread, since
    // `ITaskbarList3` may only be called from the thread that created it.
    let _ = app.run_on_main_thread(move || {
        // SAFETY: `ctx_ptr` was leaked from a `Box<TaskbarContext>` in
        // `try_init` and lives for the rest of the process.
        let ctx = unsafe { &*(ctx_ptr as *const TaskbarContext) };
        sync_thumbbar_buttons(ctx, playing, has_song);
    });

    // Slower: resolve and decode cover art off the main thread, caching it
    // for the next WM_DWMSENDICONICTHUMBNAIL/LIVEPREVIEWBITMAP request.
    let song_id = state.current_song.as_ref().map(|s| s.id);
    let ctx = unsafe { &*(ctx_ptr as *const TaskbarContext) };
    let my_seq = ctx
        .art_request_seq
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;
    tauri::async_runtime::spawn(async move {
        // SAFETY: same as above.
        let ctx = unsafe { &*(ctx_ptr as *const TaskbarContext) };
        if ctx.now_playing.lock().song_id == song_id {
            return;
        }

        let cover_path = song_id.and_then(|id| {
            app.state::<AppState>()
                .cover_manager
                .get_cover_art_path(id)
                .ok()
                .flatten()
        });
        let decoded = cover_path.and_then(|p| image::open(p).ok()).map(Arc::new);

        // A newer track change may have started (and possibly already
        // finished) while this decode was running — if so, don't let this
        // now-stale result clobber it.
        if ctx
            .art_request_seq
            .load(std::sync::atomic::Ordering::SeqCst)
            != my_seq
        {
            return;
        }

        {
            let mut now_playing = ctx.now_playing.lock();
            now_playing.song_id = song_id;
            now_playing.image = decoded;
        }

        // The art just changed — tell DWM its cached thumbnail/live-preview
        // bitmaps are stale so it re-requests them (via
        // WM_DWMSENDICONICTHUMBNAIL/LIVEPREVIEWBITMAP) instead of continuing
        // to show whatever was last handed to it, which could otherwise
        // persist until some unrelated event happens to invalidate it.
        // `HWND` wraps a raw pointer and isn't `Send`; it's just an opaque
        // handle value here; reconstructed on the main thread it's actually
        // used on.
        let hwnd_value = ctx.hwnd.0 as isize;
        let _ = app.run_on_main_thread(move || unsafe {
            let _ = DwmInvalidateIconicBitmaps(HWND(hwnd_value as *mut core::ffi::c_void));
        });
    });
}

fn build_buttons(ctx: &TaskbarContext, playing: bool, has_song: bool) -> [THUMBBUTTON; 3] {
    let (play_pause_icon, play_pause_tip) = if playing {
        (ctx.icons.pause, "Pause")
    } else {
        (ctx.icons.play, "Play")
    };
    [
        thumb_button(BTN_PREVIOUS, ctx.icons.previous, "Previous", has_song),
        thumb_button(BTN_PLAY_PAUSE, play_pause_icon, play_pause_tip, has_song),
        thumb_button(BTN_NEXT, ctx.icons.next, "Next", has_song),
    ]
}

fn sync_thumbbar_buttons(ctx: &TaskbarContext, playing: bool, has_song: bool) {
    *ctx.last_known_state.lock() = (playing, has_song);

    let taskbar_guard = ctx.taskbar.lock();
    let Some(taskbar) = taskbar_guard.as_ref() else {
        // No taskbar button yet — `register_thumbbar` will apply
        // `last_known_state` once `TaskbarButtonCreated` arrives.
        return;
    };
    let buttons = build_buttons(ctx, playing, has_song);
    if let Err(e) = unsafe { taskbar.ThumbBarUpdateButtons(ctx.hwnd, &buttons) } {
        log::warn!("Failed to update taskbar thumbbar buttons: {e:?}");
    }
}

fn send_iconic_thumbnail(ctx: &TaskbarContext, hwnd: HWND, width: u32, height: u32) {
    match build_iconic_bitmap(ctx, width, height) {
        Ok(hbitmap) => unsafe {
            let _ = DwmSetIconicThumbnail(hwnd, hbitmap, 0);
            let _ = DeleteObject(hbitmap.into());
        },
        Err(e) => log::warn!("Failed to build taskbar iconic thumbnail: {e:?}"),
    }
}

fn send_live_preview(ctx: &TaskbarContext, hwnd: HWND) {
    let mut rect = Default::default();
    let (width, height) = if unsafe { GetClientRect(hwnd, &mut rect) }.is_ok() {
        (
            (rect.right - rect.left).max(1) as u32,
            (rect.bottom - rect.top).max(1) as u32,
        )
    } else {
        (320, 320)
    };

    match build_iconic_bitmap(ctx, width, height) {
        Ok(hbitmap) => unsafe {
            let _ = DwmSetIconicLivePreviewBitmap(hwnd, hbitmap, None, 0);
            let _ = DeleteObject(hbitmap.into());
        },
        Err(e) => log::warn!("Failed to build taskbar live preview bitmap: {e:?}"),
    }
}

fn build_iconic_bitmap(
    ctx: &TaskbarContext,
    box_w: u32,
    box_h: u32,
) -> windows::core::Result<HBITMAP> {
    let image = {
        let now_playing = ctx.now_playing.lock();
        now_playing
            .image
            .clone()
            .unwrap_or_else(|| ctx.fallback_art.clone())
    };
    render_premultiplied_bitmap(&image, box_w, box_h)
}

/// Scales `(src_w, src_h)` down (or up) to fit within `(max_w, max_h)`
/// while preserving aspect ratio. Returns `(0, 0)` for degenerate input.
fn fit_within(src_w: u32, src_h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if src_w == 0 || src_h == 0 || max_w == 0 || max_h == 0 {
        return (0, 0);
    }
    let scale = (max_w as f64 / src_w as f64).min(max_h as f64 / src_h as f64);
    let w = ((src_w as f64 * scale).round() as u32).clamp(1, max_w);
    let h = ((src_h as f64 * scale).round() as u32).clamp(1, max_h);
    (w, h)
}

/// Builds a `box_w x box_h` 32bpp premultiplied-alpha DIB with `image`
/// letterboxed (aspect-preserving, centered) inside it — the format
/// `DwmSetIconicThumbnail`/`DwmSetIconicLivePreviewBitmap` require.
fn render_premultiplied_bitmap(
    image: &image::DynamicImage,
    box_w: u32,
    box_h: u32,
) -> windows::core::Result<HBITMAP> {
    let (target_w, target_h) = fit_within(image.width(), image.height(), box_w, box_h);
    let x_off = box_w.saturating_sub(target_w) / 2;
    let y_off = box_h.saturating_sub(target_h) / 2;

    let (hbitmap, bits) = create_argb_dib(box_w, box_h)?;
    // SAFETY: `bits` points to a freshly allocated, zero-initialized
    // `box_w * box_h * 4`-byte DIB section owned by `hbitmap`.
    let buf = unsafe { std::slice::from_raw_parts_mut(bits, (box_w * box_h * 4) as usize) };

    if target_w > 0 && target_h > 0 {
        let resized = image::imageops::resize(
            image,
            target_w,
            target_h,
            image::imageops::FilterType::Triangle,
        );
        for y in 0..target_h {
            for x in 0..target_w {
                let px = resized.get_pixel(x, y).0;
                let a = px[3] as u32;
                let idx = (((y + y_off) * box_w + (x + x_off)) * 4) as usize;
                buf[idx] = ((px[2] as u32 * a) / 255) as u8;
                buf[idx + 1] = ((px[1] as u32 * a) / 255) as u8;
                buf[idx + 2] = ((px[0] as u32 * a) / 255) as u8;
                buf[idx + 3] = a as u8;
            }
        }
    }

    Ok(hbitmap)
}

/// Allocates a top-down, 32bpp, zero-initialized BGRA DIB section of the
/// given size. The returned pointer stays valid for the lifetime of the
/// returned `HBITMAP`.
fn create_argb_dib(width: u32, height: u32) -> windows::core::Result<(HBITMAP, *mut u8)> {
    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = width as i32;
    bmi.bmiHeader.biHeight = -(height as i32);
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB.0;

    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let hdc = unsafe { GetDC(None) };
    let result = unsafe { CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) };
    unsafe {
        ReleaseDC(None, hdc);
    }
    result.map(|hbitmap| (hbitmap, bits as *mut u8))
}

#[derive(Clone, Copy)]
enum ButtonGlyph {
    Previous,
    Play,
    Pause,
    Next,
}

/// Renders a flat, filled glyph (no external icon assets) into an `HICON`
/// at `size x size` — straight (non-premultiplied) alpha, which is what a
/// 32bpp icon bitmap expects (unlike the DWM iconic bitmaps above).
fn build_icon(glyph: ButtonGlyph, size: u32) -> windows::core::Result<HICON> {
    let (color_bitmap, bits) = create_argb_dib(size, size)?;
    {
        // SAFETY: see `render_premultiplied_bitmap` — same allocation shape.
        let buf = unsafe { std::slice::from_raw_parts_mut(bits, (size * size * 4) as usize) };
        draw_glyph(glyph, size, buf);
    }
    // A 32bpp color bitmap with a real alpha channel only needs a blank
    // (all-zero) 1bpp mask — the alpha channel alone drives transparency.
    let mask_bitmap = unsafe { CreateBitmap(size as i32, size as i32, 1, 1, None) };

    let icon_info = ICONINFO {
        fIcon: BOOL(1),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask_bitmap,
        hbmColor: color_bitmap,
    };
    let result = unsafe { CreateIconIndirect(&icon_info) };
    unsafe {
        let _ = DeleteObject(color_bitmap.into());
        let _ = DeleteObject(mask_bitmap.into());
    }
    result
}

fn draw_glyph(glyph: ButtonGlyph, size: u32, buf: &mut [u8]) {
    let s = size as f32;
    let margin = s * 0.28;
    match glyph {
        ButtonGlyph::Play => {
            fill_triangle(
                buf,
                size,
                (margin, margin),
                (margin, s - margin),
                (s - margin, s / 2.0),
            );
        }
        ButtonGlyph::Pause => {
            let bar_w = s * 0.16;
            let gap = s * 0.14;
            let left1 = s / 2.0 - gap / 2.0 - bar_w;
            let left2 = s / 2.0 + gap / 2.0;
            fill_rect(buf, size, left1, margin, left1 + bar_w, s - margin);
            fill_rect(buf, size, left2, margin, left2 + bar_w, s - margin);
        }
        ButtonGlyph::Next => {
            let mid = s / 2.0;
            fill_triangle(
                buf,
                size,
                (margin, margin),
                (margin, s - margin),
                (mid, s / 2.0),
            );
            let bar_w = s * 0.12;
            fill_rect(buf, size, mid, margin, mid + bar_w, s - margin);
        }
        ButtonGlyph::Previous => {
            let mid = s / 2.0;
            fill_triangle(
                buf,
                size,
                (s - margin, margin),
                (s - margin, s - margin),
                (mid, s / 2.0),
            );
            let bar_w = s * 0.12;
            fill_rect(buf, size, mid - bar_w, margin, mid, s - margin);
        }
    }
}

fn set_pixel_opaque(buf: &mut [u8], size: u32, x: i64, y: i64) {
    if x < 0 || y < 0 || x as u32 >= size || y as u32 >= size {
        return;
    }
    let idx = ((y as u32 * size + x as u32) * 4) as usize;
    buf[idx] = 255;
    buf[idx + 1] = 255;
    buf[idx + 2] = 255;
    buf[idx + 3] = 255;
}

fn fill_rect(buf: &mut [u8], size: u32, x0: f32, y0: f32, x1: f32, y1: f32) {
    let (x0, x1) = (x0.min(x1).round() as i64, x0.max(x1).round() as i64);
    let (y0, y1) = (y0.min(y1).round() as i64, y0.max(y1).round() as i64);
    for y in y0..y1 {
        for x in x0..x1 {
            set_pixel_opaque(buf, size, x, y);
        }
    }
}

fn fill_triangle(buf: &mut [u8], size: u32, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32)) {
    let min_x = p0.0.min(p1.0).min(p2.0).floor() as i64;
    let max_x = p0.0.max(p1.0).max(p2.0).ceil() as i64;
    let min_y = p0.1.min(p1.1).min(p2.1).floor() as i64;
    let max_y = p0.1.max(p1.1).max(p2.1).ceil() as i64;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            if point_in_triangle(p, p0, p1, p2) {
                set_pixel_opaque(buf, size, x, y);
            }
        }
    }
}

fn triangle_sign(p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)) -> f32 {
    (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
}

fn point_in_triangle(pt: (f32, f32), v0: (f32, f32), v1: (f32, f32), v2: (f32, f32)) -> bool {
    let d1 = triangle_sign(pt, v0, v1);
    let d2 = triangle_sign(pt, v1, v2);
    let d3 = triangle_sign(pt, v2, v0);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

fn load_fallback_art() -> Arc<image::DynamicImage> {
    const FALLBACK_ART_BYTES: &[u8] = include_bytes!("../icons/128x128.png");
    Arc::new(
        image::load_from_memory(FALLBACK_ART_BYTES)
            .unwrap_or_else(|_| image::DynamicImage::new_rgba8(1, 1)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_action_maps_known_ids() {
        assert!(matches!(
            button_action(BTN_PREVIOUS),
            Some(ButtonAction::Previous)
        ));
        assert!(matches!(
            button_action(BTN_PLAY_PAUSE),
            Some(ButtonAction::PlayPause)
        ));
        assert!(matches!(button_action(BTN_NEXT), Some(ButtonAction::Next)));
    }

    #[test]
    fn button_action_ignores_unknown_ids() {
        assert!(button_action(0).is_none());
        assert!(button_action(999).is_none());
    }

    #[test]
    fn fit_within_downscales_preserving_aspect_ratio() {
        assert_eq!(fit_within(1000, 1000, 64, 64), (64, 64));
        assert_eq!(fit_within(1000, 500, 64, 64), (64, 32));
        assert_eq!(fit_within(500, 1000, 64, 64), (32, 64));
    }

    #[test]
    fn fit_within_upscales_small_art_to_fill_the_box() {
        assert_eq!(fit_within(10, 10, 64, 64), (64, 64));
    }

    #[test]
    fn fit_within_handles_degenerate_input() {
        assert_eq!(fit_within(0, 100, 64, 64), (0, 0));
        assert_eq!(fit_within(100, 0, 64, 64), (0, 0));
        assert_eq!(fit_within(100, 100, 0, 64), (0, 0));
    }

    #[test]
    fn point_in_triangle_matches_expected_containment() {
        let tri = ((0.0, 0.0), (10.0, 0.0), (0.0, 10.0));
        assert!(point_in_triangle((1.0, 1.0), tri.0, tri.1, tri.2));
        assert!(!point_in_triangle((9.0, 9.0), tri.0, tri.1, tri.2));
    }
}
