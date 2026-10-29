//! Portable players and removable drives (#835). The device list itself
//! arrives through the `devices-changed` event (see `crate::devices`); these
//! commands read it once at startup, save a profile and eject.

use crate::devices::ConnectedDevice;
use crate::AppState;
use luminous_devices::{DeviceProfile, EjectError};
use luminous_transcode::{Capabilities, ConversionTarget};
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
