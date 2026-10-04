//! IPC handlers for Store-gated add-on themes (#1414). They only start flows;
//! results arrive as `addon-state-changed` / `addon-theme-defined` events.

use crate::addons::entitlement::{AddonManager, AddonState, Events, ProductPrice, ThemeDefinition};
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PriceDefined<'a> {
    id: &'a str,
    formatted: &'a str,
    is_free: bool,
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

    fn price_defined(&self, id: &str, price: &ProductPrice) {
        let payload = PriceDefined {
            id,
            formatted: &price.formatted,
            is_free: price.is_free,
        };
        if let Err(e) = self.0.emit("addon-price-defined", payload) {
            log::warn!("failed to emit addon-price-defined: {e}");
        }
    }
}

/// Build the manager for this run. Cached bundles live under `<data>/addons`.
pub fn build_manager(app: &AppHandle) -> Arc<AddonManager> {
    use crate::addons::entitlement::{default_backend, default_provisioner};
    let cache_dir = crate::paths::resolve_app_data_dir(app).join("addons");
    #[cfg(debug_assertions)]
    watch_dev_folder(app);
    Arc::new(AddonManager::new(
        default_backend(),
        default_provisioner(cache_dir),
        Arc::new(TauriEvents(app.clone())),
    ))
}

/// Debug builds only (#1426): re-announce an unpacked add-on whenever its folder
/// changes, and tell the overlay frame to reload.
#[cfg(debug_assertions)]
fn watch_dev_folder(app: &AppHandle) {
    use crate::addons::devloader;
    let Some(root) = devloader::dev_dir() else {
        return;
    };
    let app = app.clone();
    let events = TauriEvents(app.clone());
    let watching = devloader::watch(root.clone(), move |id, loaded| match loaded {
        Ok(manifest) => {
            log::info!("add-on {id}: reloaded from disk");
            events.theme_defined(&ThemeDefinition::from(&manifest));
            events.state(id, AddonState::Owned, None);
            let _ = app.emit("addon-dev-reloaded", id);
        }
        Err(e) => log::warn!("add-on {id}: reload failed, keeping the previous copy: {e}"),
    });
    match watching {
        Ok(()) => log::info!("watching {} for add-on changes", root.display()),
        Err(e) => log::warn!("could not watch {}: {e}", root.display()),
    }
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
