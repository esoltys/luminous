use tauri::{AppHandle, Manager};

/// Fire-and-forget like `set_app_setting` — logs failures internally and
/// never rejects, so a broken diagnostics path can't itself throw inside
/// the frontend error boundary that calls this.
#[tauri::command]
pub async fn log_frontend_error(
    app: AppHandle,
    message: String,
    stack: Option<String>,
) -> Result<(), String> {
    let Ok(app_data_dir) = app.path().app_data_dir() else {
        log::error!("Failed to resolve app data dir for frontend error log");
        return Ok(());
    };
    crate::diagnostics::log_frontend_error(&app_data_dir, &message, stack.as_deref());
    Ok(())
}

/// Writes the crash log(s) plus app/OS metadata to `export_path`, which the
/// frontend has already resolved via a native save dialog (same pattern as
/// `export_playlist`).
#[tauri::command]
pub async fn export_diagnostics(app: AppHandle, export_path: String) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let version = app.package_info().version.to_string();
    let bundle = crate::diagnostics::build_diagnostics_bundle(&app_data_dir, &version);
    std::fs::write(&export_path, bundle).map_err(|e| e.to_string())
}
