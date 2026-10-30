//! Portable players and removable drives (#835). The device list itself
//! arrives through the `devices-changed` event (see `crate::devices`); these
//! commands read it once at startup, save a profile and eject. Sync (#836)
//! is previewed, started and stopped here; its progress and outcome arrive
//! as `device-sync-progress` and `device-sync-finished` events.

use crate::devices::sync::plan::{SyncItem, SyncPlan};
use crate::devices::sync::selection::SyncSettings;
use crate::devices::sync::worker::{SyncProgress, SyncSummary};
use crate::devices::sync::{
    DeviceSyncState, PlanList, SyncEvents, SYNC_FINISHED_EVENT, SYNC_PROGRESS_EVENT,
};
use crate::devices::ConnectedDevice;
use crate::AppState;
use luminous_devices::{DeviceProfile, EjectError};
use luminous_transcode::{Capabilities, ConversionTarget};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

/// Longest an eject waits for a running sync to stop.
const SYNC_STOP_TIMEOUT: Duration = Duration::from_secs(15);

#[tauri::command]
pub async fn get_connected_devices(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectedDevice>, String> {
    Ok(state.devices.devices())
}

/// Saves the profile of a connected device and writes its marker. Rejects
/// with `device-not-connected` when the device is gone.
#[tauri::command]
pub async fn update_device_profile(
    id: String,
    profile: DeviceProfile,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let devices = state.devices.clone();
    crate::db::run_blocking(&state.db, move |conn| {
        devices.save_profile(conn, &id, profile, chrono::Utc::now().timestamp())
    })
    .await
    .map_err(|e| e.to_string())
}

/// The formats and bitrates sync can convert to on this computer (#838).
/// The Convert picker offers only these.
#[tauri::command]
pub async fn get_transcode_capabilities(
    state: State<'_, AppState>,
) -> Result<Capabilities, String> {
    let transcoder = state.transcoder.clone();
    tokio::task::spawn_blocking(move || transcoder.capabilities().clone())
        .await
        .map_err(|e| e.to_string())
}

/// What files converted for `profile` (saved or still being edited) get on
/// this computer: a format substituted for one it can't encode, or one the
/// device doesn't list, is flagged so the UI can warn before a sync (#838).
#[tauri::command]
pub async fn get_conversion_target(
    profile: DeviceProfile,
    state: State<'_, AppState>,
) -> Result<ConversionTarget, String> {
    let transcoder = state.transcoder.clone();
    tokio::task::spawn_blocking(move || transcoder.conversion_target(&profile))
        .await
        .map_err(|e| e.to_string())
}

/// The example path under the "Folders on the device" template for
/// `profile`, saved or still being edited (#837); see
/// `layout::preview_track_path`.
#[tauri::command]
pub async fn preview_device_path(
    profile: DeviceProfile,
    song_id: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let transcoder = state.transcoder.clone();
    crate::db::run_blocking(&state.db, move |conn| {
        crate::devices::layout::preview_track_path(
            conn,
            &transcoder,
            &profile.normalized(),
            song_id,
        )
    })
    .await
    .map_err(|e| e.to_string())
}

/// Safely removes a device. Stops playback first when the current track is
/// on it, so Luminous never blocks its own eject. Resolves when the device
/// can be unplugged (or was already gone; an MTP device always can be);
/// rejects with `device-in-use` when another program holds files open on
/// it, else `device-eject-failed`.
#[tauri::command]
pub async fn eject_device(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let Some(device) = state.devices.device(&id) else {
        return Ok(());
    };

    let sync = state.device_sync.clone();
    let sync_id = id.clone();
    tokio::task::spawn_blocking(move || sync.stop(&sync_id, SYNC_STOP_TIMEOUT))
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut player = state.player.lock().await;
        let playing_from_device = player
            .current_song
            .as_ref()
            .and_then(|song| song.path.as_deref())
            .is_some_and(|path| device.contains(path));
        if playing_from_device {
            player.stop().await.map_err(|e| e.to_string())?;
        }
    }

    let result = tokio::task::spawn_blocking(move || device.eject())
        .await
        .map_err(|e| e.to_string())?;
    state.devices.request_rebuild();
    result.map_err(|e| {
        log::warn!("Eject of {id} failed: {e}");
        match e {
            EjectError::InUse(_) => "device-in-use".to_string(),
            EjectError::Failed(_) => "device-eject-failed".to_string(),
        }
    })
}

/// The device's saved sync selection, the summary of its last sync and
/// whether one is running.
#[tauri::command]
pub async fn get_device_sync_state(
    device_id: String,
    state: State<'_, AppState>,
) -> Result<DeviceSyncState, String> {
    let sync = state.device_sync.clone();
    crate::db::run_blocking(&state.db, move |conn| sync.state(conn, &device_id))
        .await
        .map_err(|e| e.to_string())
}

/// Saves `settings` for the device and plans the sync they describe: what
/// is copied, converted, updated and removed, and whether it fits. Cheap to
/// call as the selection changes. Rejects with `device-not-connected`.
#[tauri::command]
pub async fn preview_device_sync(
    device_id: String,
    settings: SyncSettings,
    state: State<'_, AppState>,
) -> Result<SyncPlan, String> {
    let sync = state.device_sync.clone();
    crate::db::run_blocking(&state.db, move |conn| {
        sync.preview(conn, &device_id, settings)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Every item of one of a previewed plan's lists ("Show all"). Rejects with
/// `sync-plan-stale` once the plan was replaced or started.
#[tauri::command]
pub async fn get_device_sync_items(
    plan_id: String,
    list: PlanList,
    state: State<'_, AppState>,
) -> Result<Vec<SyncItem>, String> {
    state
        .device_sync
        .items(&plan_id, list)
        .ok_or_else(|| "sync-plan-stale".to_string())
}

/// Runs the previewed plan in the background. Rejects with
/// `sync-plan-stale`, `sync-does-not-fit` or `sync-in-progress`.
#[tauri::command]
pub async fn start_device_sync(plan_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.device_sync.start(&plan_id).map_err(|e| e.to_string())
}

/// Stops the device's running sync, keeping every file completed so far.
/// Resolves at once; `device-sync-finished` follows when it has stopped.
#[tauri::command]
pub async fn cancel_device_sync(
    device_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.device_sync.cancel(&device_id);
    Ok(())
}

/// Sync events as the app's `device-sync-*` events.
pub(crate) struct AppEvents(pub AppHandle);

impl SyncEvents for AppEvents {
    fn progress(&self, progress: &SyncProgress) {
        let _ = self.0.emit(SYNC_PROGRESS_EVENT, progress);
    }

    fn finished(&self, summary: &SyncSummary) {
        let _ = self.0.emit(SYNC_FINISHED_EVENT, summary);
    }
}
