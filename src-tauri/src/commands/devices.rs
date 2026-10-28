//! Portable players and removable drives (#835). The device list itself
//! arrives through the `devices-changed` event (see `crate::devices`); these
//! commands read it once at startup, save a profile and eject.

use crate::devices::{is_on_volume, ConnectedDevice};
use crate::AppState;
use luminous_devices::{DeviceProfile, EjectError};
use tauri::State;

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

/// Safely removes a device. Stops playback first when the current track is
/// on it, so Luminous never blocks its own eject. Resolves when the device
/// can be unplugged (or was already gone); rejects with `device-in-use` when
/// another program holds files open on it, else `device-eject-failed`.
#[tauri::command]
pub async fn eject_device(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let Some(volume) = state.devices.volume(&id) else {
        return Ok(());
    };

    {
        let mut player = state.player.lock().await;
        let playing_from_device = player
            .current_song
            .as_ref()
            .and_then(|song| song.path.as_deref())
            .is_some_and(|path| is_on_volume(path, &volume.mount_path));
        if playing_from_device {
            player.stop().await.map_err(|e| e.to_string())?;
        }
    }

    let result = tokio::task::spawn_blocking(move || luminous_devices::eject(&volume))
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
