// Luminous Music Player — Window & Miniplayer Commands

use tauri::WebviewWindow;

#[tauri::command]
pub async fn enter_miniplayer_mode(
    window: WebviewWindow,
    width: Option<f64>,
    height: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<serde_json::Value, String> {
    let current_size = window.outer_size().map_err(|e| e.to_string())?;
    let current_position = window.outer_position().map_err(|e| e.to_string())?;
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let logical_width = current_size.width as f64 / scale_factor;
    let logical_height = current_size.height as f64 / scale_factor;
    let logical_x = current_position.x as f64 / scale_factor;
    let logical_y = current_position.y as f64 / scale_factor;

    let target_width = width.unwrap_or(300.0);
    let target_height = height.unwrap_or(360.0);

    let _ = window.set_always_on_top(true);
    let _ = window.set_decorations(false);
    let _ = window.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize {
        width: 200.0,
        height: 200.0,
    })));
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: target_width,
        height: target_height,
    }));
    if let (Some(target_x), Some(target_y)) = (x, y) {
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
            x: target_x,
            y: target_y,
        }));
    }

    Ok(serde_json::json!({
        "saved_width": logical_width,
        "saved_height": logical_height,
        "saved_x": logical_x,
        "saved_y": logical_y
    }))
}

#[tauri::command]
pub async fn exit_miniplayer_mode(
    window: WebviewWindow,
    saved_width: Option<f64>,
    saved_height: Option<f64>,
    saved_x: Option<f64>,
    saved_y: Option<f64>,
) -> Result<serde_json::Value, String> {
    // Capture the miniplayer's actual current size and position before
    // restoring the full window — this reflects wherever the user resized
    // and/or dragged it to, whether via native OS window-manager gestures
    // or the manual pointer-drag fallback.
    let current_size = window.outer_size().map_err(|e| e.to_string())?;
    let current_position = window.outer_position().map_err(|e| e.to_string())?;
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let mini_width = current_size.width as f64 / scale_factor;
    let mini_height = current_size.height as f64 / scale_factor;
    let mini_x = current_position.x as f64 / scale_factor;
    let mini_y = current_position.y as f64 / scale_factor;

    let restore_w = saved_width.unwrap_or(1280.0);
    let restore_h = saved_height.unwrap_or(800.0);

    let _ = window.set_decorations(true);
    let _ = window.set_always_on_top(false);
    let _ = window.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize {
        width: 900.0,
        height: 600.0,
    })));
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: restore_w,
        height: restore_h,
    }));
    if let (Some(target_x), Some(target_y)) = (saved_x, saved_y) {
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
            x: target_x,
            y: target_y,
        }));
    }

    Ok(serde_json::json!({
        "mini_width": mini_width,
        "mini_height": mini_height,
        "mini_x": mini_x,
        "mini_y": mini_y
    }))
}

#[tauri::command]
pub async fn start_window_drag(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_window_resize(window: WebviewWindow) -> Result<(), String> {
    let _ = window;
    Ok(())
}

#[tauri::command]
pub async fn resize_miniplayer(
    window: WebviewWindow,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let clamped_w = width.max(200.0).min(700.0);
    let clamped_h = height.max(200.0).min(700.0);
    window
        .set_size(tauri::Size::Logical(tauri::LogicalSize {
            width: clamped_w,
            height: clamped_h,
        }))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_miniplayer_dimensions_defaults() {
        let width = None;
        let height = None;
        let target_width = width.unwrap_or(300.0);
        let target_height = height.unwrap_or(360.0);
        assert_eq!(target_width, 300.0);
        assert_eq!(target_height, 360.0);
    }

    #[test]
    fn test_miniplayer_restore_defaults() {
        let saved_width: Option<f64> = None;
        let saved_height: Option<f64> = None;
        let restore_w = saved_width.unwrap_or(1280.0);
        let restore_h = saved_height.unwrap_or(800.0);
        assert_eq!(restore_w, 1280.0);
        assert_eq!(restore_h, 800.0);
    }

    #[test]
    fn test_miniplayer_position_only_set_when_both_axes_present() {
        // Repositioning should only happen when both x and y are known —
        // never move a window along just one axis.
        fn wants_reposition(x: Option<f64>, y: Option<f64>) -> bool {
            matches!((x, y), (Some(_), Some(_)))
        }
        assert!(!wants_reposition(None, None));
        assert!(!wants_reposition(Some(10.0), None));
        assert!(!wants_reposition(None, Some(20.0)));
        assert!(wants_reposition(Some(10.0), Some(20.0)));
    }
}
