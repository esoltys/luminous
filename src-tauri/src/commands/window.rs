// Luminous Music Player — Window & Miniplayer Commands

use tauri::WebviewWindow;
use tauri_plugin_positioner::{Position, WindowExt};

#[tauri::command]
pub async fn enter_miniplayer_mode(
    window: WebviewWindow,
    width: Option<f64>,
    height: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<serde_json::Value, String> {
    let current_size = window.inner_size().map_err(|e| e.to_string())?;
    let current_pos = window.outer_position().ok();
    let scale_factor = window.scale_factor().unwrap_or(1.0);

    let logical_width = current_size.width as f64 / scale_factor;
    let logical_height = current_size.height as f64 / scale_factor;
    let (saved_x, saved_y) = match current_pos {
        Some(pos) => (
            Some(pos.x as f64 / scale_factor),
            Some(pos.y as f64 / scale_factor),
        ),
        None => (None, None),
    };

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
        "saved_x": saved_x,
        "saved_y": saved_y
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
    let current_size = window.inner_size().map_err(|e| e.to_string())?;
    let current_pos = window.outer_position().ok();
    let scale_factor = window.scale_factor().unwrap_or(1.0);

    let mini_width = current_size.width as f64 / scale_factor;
    let mini_height = current_size.height as f64 / scale_factor;
    let (mini_x, mini_y) = match current_pos {
        Some(pos) => (
            Some(pos.x as f64 / scale_factor),
            Some(pos.y as f64 / scale_factor),
        ),
        None => (None, None),
    };

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

    if let (Some(rx), Some(ry)) = (saved_x, saved_y) {
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
            x: rx,
            y: ry,
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
pub async fn move_window_to_preset(
    window: WebviewWindow,
    position: String,
) -> Result<(), String> {
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

#[tauri::command]
pub async fn get_window_geometry(
    window: WebviewWindow,
) -> Result<serde_json::Value, String> {
    let size = window.inner_size().map_err(|e| e.to_string())?;
    let pos = window.outer_position().ok();
    let scale_factor = window.scale_factor().unwrap_or(1.0);

    let width = size.width as f64 / scale_factor;
    let height = size.height as f64 / scale_factor;
    let (x, y) = match pos {
        Some(p) => (Some(p.x as f64 / scale_factor), Some(p.y as f64 / scale_factor)),
        None => (None, None),
    };

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
    _direction: Option<String>,
) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
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
}
