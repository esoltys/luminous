//! IPC handlers for Store-gated add-on themes (#1414). They only start flows;
//! results arrive as `addon-state-changed` / `addon-theme-defined` events.

use crate::addons::entitlement::{AddonManager, AddonState, Events, ThemeDefinition};
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;
#[cfg(target_os = "windows")]
use tauri::Manager;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Serialize)]
struct StateChanged<'a> {
    id: &'a str,
    state: AddonState,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

/// Emits the add-on events to the frontend.
pub struct TauriEvents(pub AppHandle);

impl Events for TauriEvents {
    fn state(&self, id: &str, state: AddonState, error: Option<&str>) {
        if let Err(e) = self
            .0
            .emit("addon-state-changed", StateChanged { id, state, error })
        {
            log::warn!("failed to emit addon-state-changed: {e}");
        }
    }

    fn theme_defined(&self, theme: &ThemeDefinition) {
        if let Err(e) = self.0.emit("addon-theme-defined", theme) {
            log::warn!("failed to emit addon-theme-defined: {e}");
        }
    }
}

/// Build the manager for this run. Cached bundles live under `<data>/addons`.
pub fn build_manager(app: &AppHandle) -> Arc<AddonManager> {
    use crate::addons::entitlement::{default_backend, NetworkProvisioner};
    let cache_dir = crate::paths::resolve_app_data_dir(app).join("addons");
    Arc::new(AddonManager::new(
        default_backend(),
        Arc::new(NetworkProvisioner::new(cache_dir)),
        Arc::new(TauriEvents(app.clone())),
    ))
}

/// Re-check every known add-on and announce its state.
#[tauri::command]
pub async fn refresh_addons(state: State<'_, AppState>) -> Result<(), String> {
    state.addons.refresh_all().await;
    Ok(())
}

/// Open the Store purchase dialog for an add-on.
#[tauri::command]
pub async fn acquire_addon(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let hwnd = app
        .get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as isize)
        .unwrap_or(0);
    #[cfg(not(target_os = "windows"))]
    let hwnd = {
        let _ = &app;
        0
    };
    state.addons.acquire(&id, hwnd).await;
    Ok(())
}
