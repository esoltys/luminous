// Luminous Music Player — Window & Miniplayer Commands
//
// Per-mode (miniplayer vs. full player) geometry memory is only attempted
// where reading a window's position back is actually reliable. Wayland's
// protocol doesn't expose a window's absolute screen position to the
// client at all, so `outer_position()`/`inner_position()` are permanently
// stuck at (0,0) there — confirmed in tao's GTK backend, where both are
// driven by GDK's `frame_extents()`, which Wayland can't populate. This
// affects any Tauri app equally, including the official
// `tauri-plugin-window-state` (it calls the same API) — that plugin is
// still registered in lib.rs for cross-*restart* geometry persistence
// (which degrades gracefully to a no-op on Wayland, same limitation), but
// can't express "two geometries for one window depending on app mode",
// which is what this file's per-mode memory is for.
//
// `geometry_capture_supported()` reports whether this session can reliably
// read geometry back (false only for Linux-under-Wayland); the frontend
// gates its debounced resize/move capture on this. When unsupported, or
// before any geometry has been captured yet, enter/exit_miniplayer_mode
// fall back to fixed defaults.
//
// Size in particular also needs the caller-independent `settle_size` retry:
// GTK recalculates frame extents asynchronously after `set_decorations()`
// and can overwrite an explicit `set_size()` call with a fixed offset
// shortly after it lands (measured via diagnostic logging: -90 logical px
// on both axes undecorating, +180 width / +228 height redecorating, on
// Linux/GTK). Polling after the initial apply and reapplying the target
// until it actually sticks absorbs whatever offset or timing that
// recalculation introduces, regardless of whether the target is a fixed
// default or a remembered custom size.
const MINIPLAYER_WIDTH: f64 = 300.0;
const MINIPLAYER_HEIGHT: f64 = 360.0;
const FULL_PLAYER_WIDTH: f64 = 1280.0;
const FULL_PLAYER_HEIGHT: f64 = 800.0;

const SETTLE_CHECK_INTERVAL_MS: u64 = 120;
const SETTLE_MAX_CHECKS: u32 = 8;
const SETTLE_REQUIRED_STABLE_HITS: u32 = 2;

use std::time::Duration;
use tauri::{Manager, WebviewWindow};
use tauri_plugin_positioner::{Position, WindowExt};

#[tauri::command]
pub fn geometry_capture_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var("WAYLAND_DISPLAY").is_err()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

async fn settle_size(window: &WebviewWindow, target_w: f64, target_h: f64) {
    let mut stable_hits = 0;
    for _ in 0..SETTLE_MAX_CHECKS {
        tokio::time::sleep(Duration::from_millis(SETTLE_CHECK_INTERVAL_MS)).await;
        let scale = window.scale_factor().unwrap_or(1.0);
        let matches = match window.inner_size() {
            Ok(size) => {
                let w = (size.width as f64 / scale).round();
                let h = (size.height as f64 / scale).round();
                (w - target_w).abs() < 1.0 && (h - target_h).abs() < 1.0
            }
            Err(_) => false,
        };
        if matches {
            stable_hits += 1;
            if stable_hits >= SETTLE_REQUIRED_STABLE_HITS {
                break;
            }
        } else {
            stable_hits = 0;
            let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
                width: target_w,
                height: target_h,
            }));
        }
    }
}

#[tauri::command]
pub async fn enter_miniplayer_mode(
    window: WebviewWindow,
    width: Option<f64>,
    height: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), String> {
    let target_w = width.unwrap_or(MINIPLAYER_WIDTH).round().max(200.0);
    let target_h = height.unwrap_or(MINIPLAYER_HEIGHT).round().max(200.0);

    let _ = window.set_always_on_top(true);
    let _ = window.set_decorations(false);
    let _ = window.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize {
        width: 200.0,
        height: 200.0,
    })));
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: target_w,
        height: target_h,
    }));
    if let (Some(px), Some(py)) = (x, y) {
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
            x: px,
            y: py,
        }));
    } else {
        let _ = window.move_window(Position::TopRight);
    }

    settle_size(&window, target_w, target_h).await;

    Ok(())
}

#[tauri::command]
pub async fn exit_miniplayer_mode(
    window: WebviewWindow,
    width: Option<f64>,
    height: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), String> {
    let target_w = width.unwrap_or(FULL_PLAYER_WIDTH).round().max(320.0);
    let target_h = height.unwrap_or(FULL_PLAYER_HEIGHT).round().max(120.0);

    let _ = window.set_decorations(true);
    let _ = window.set_always_on_top(false);
    let _ = window.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize {
        width: 320.0,
        height: 120.0,
    })));
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: target_w,
        height: target_h,
    }));
    if let (Some(px), Some(py)) = (x, y) {
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
            x: px,
            y: py,
        }));
    } else {
        let _ = window.move_window(Position::Center);
    }

    settle_size(&window, target_w, target_h).await;

    Ok(())
}

#[tauri::command]
pub async fn move_window_to_preset(window: WebviewWindow, position: String) -> Result<(), String> {
    let pos = match position.as_str() {
        "TopLeft" => Position::TopLeft,
        "TopRight" => Position::TopRight,
        "BottomLeft" => Position::BottomLeft,
        "BottomRight" => Position::BottomRight,
        "TopCenter" => Position::TopCenter,
        "BottomCenter" => Position::BottomCenter,
        "LeftCenter" => Position::LeftCenter,
        "RightCenter" => Position::RightCenter,
        "Center" => Position::Center,
        _ => Position::TopRight,
    };
    window.move_window(pos).map_err(|e| e.to_string())
}

/// True if `(width, height, x, y)` looks like a minimized-window placeholder
/// rather than real geometry — either a 0x0 size, or Win32's offscreen
/// minimized coordinates (around -32000). A pure function of the already-
/// scaled values, factored out of `get_window_geometry` so the boundary
/// logic is unit-testable without a live `WebviewWindow`.
///
/// Regression test for #441/#442: before this check existed,
/// `get_window_geometry` happily returned the placeholder geometry for a
/// minimized window, and the frontend restored the window to that
/// placeholder (0x0, or offscreen at -32000) on next launch — a
/// completely blank, invisible app the user had no way to recover without
/// external tooling.
fn is_placeholder_minimized_geometry(
    width: f64,
    height: f64,
    x: Option<f64>,
    y: Option<f64>,
) -> bool {
    width <= 0.0
        || height <= 0.0
        || x.is_some_and(|coord| coord <= -10000.0)
        || y.is_some_and(|coord| coord <= -10000.0)
}

#[tauri::command]
pub async fn get_window_geometry(window: WebviewWindow) -> Result<serde_json::Value, String> {
    if window.is_minimized().unwrap_or(false) {
        return Ok(serde_json::Value::Null);
    }

    let size = window.inner_size().map_err(|e| e.to_string())?;
    let pos = window.outer_position().ok();
    let scale_factor = window.scale_factor().unwrap_or(1.0);

    let width = (size.width as f64 / scale_factor).round();
    let height = (size.height as f64 / scale_factor).round();
    let (x, y) = match pos {
        Some(p) => (
            Some((p.x as f64 / scale_factor).round()),
            Some((p.y as f64 / scale_factor).round()),
        ),
        None => (None, None),
    };

    // Ignore placeholder minimized sizes (0x0) or Win32 offscreen minimized coordinates (-32000)
    if is_placeholder_minimized_geometry(width, height, x, y) {
        return Ok(serde_json::Value::Null);
    }

    Ok(serde_json::json!({
        "width": width,
        "height": height,
        "x": x,
        "y": y
    }))
}

#[tauri::command]
pub async fn start_window_drag(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_window_resize(
    window: WebviewWindow,
    direction: Option<String>,
) -> Result<(), String> {
    let dir = match direction.as_deref() {
        Some("north") => tauri_runtime::ResizeDirection::North,
        Some("south") => tauri_runtime::ResizeDirection::South,
        Some("east") => tauri_runtime::ResizeDirection::East,
        Some("west") => tauri_runtime::ResizeDirection::West,
        Some("north-east") => tauri_runtime::ResizeDirection::NorthEast,
        Some("north-west") => tauri_runtime::ResizeDirection::NorthWest,
        Some("south-east") => tauri_runtime::ResizeDirection::SouthEast,
        Some("south-west") => tauri_runtime::ResizeDirection::SouthWest,
        _ => return window.start_dragging().map_err(|e| e.to_string()),
    };
    // `start_resize_dragging` lives on the lower-level `Window`, not on
    // `WebviewWindow` (which only wraps `start_dragging`), so fetch it via
    // `Manager::get_window`.
    let inner = Manager::get_window(&window, window.label())
        .ok_or_else(|| "window not found".to_string())?;
    inner.start_resize_dragging(dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_geometry_is_not_a_placeholder() {
        assert!(!is_placeholder_minimized_geometry(
            1024.0,
            768.0,
            Some(100.0),
            Some(50.0)
        ));
    }

    #[test]
    fn test_normal_geometry_with_no_position_is_not_a_placeholder() {
        // outer_position() can legitimately fail/be unavailable (e.g. under
        // Wayland) independent of minimized state — None coordinates alone
        // must not be treated as a placeholder.
        assert!(!is_placeholder_minimized_geometry(
            1024.0, 768.0, None, None
        ));
    }

    #[test]
    fn test_zero_size_is_a_placeholder() {
        assert!(is_placeholder_minimized_geometry(
            0.0,
            0.0,
            Some(100.0),
            Some(50.0)
        ));
    }

    #[test]
    fn test_win32_offscreen_minimized_coordinates_are_a_placeholder() {
        // Win32 parks minimized windows at (-32000, -32000).
        assert!(is_placeholder_minimized_geometry(
            160.0,
            28.0,
            Some(-32000.0),
            Some(-32000.0)
        ));
    }

    #[test]
    fn test_negative_but_onscreen_coordinates_are_not_a_placeholder() {
        // A window can legitimately sit partly off a monitor's top-left edge
        // (e.g. a multi-monitor setup) without being minimized — only the
        // extreme Win32 offscreen-parking coordinates should trip this.
        assert!(!is_placeholder_minimized_geometry(
            1024.0,
            768.0,
            Some(-50.0),
            Some(-20.0)
        ));
    }
}
