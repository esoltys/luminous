//! `.luminous-device.json`: the device's id and profile, kept on the device
//! itself so another computer (or a fresh install) recognizes it.
//!
//! Written only when the user saves a profile, never on detection, so
//! plugging in a drive never writes to it.

use crate::{DeviceBackend, DeviceProfile};
use serde::{Deserialize, Serialize};
use std::ops::ControlFlow;

pub const MARKER_FILE_NAME: &str = ".luminous-device.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub id: String,
    #[serde(default)]
    pub profile: DeviceProfile,
}

/// The marker at the root of `storage`, or `None` if there is none or it
/// can't be read or parsed (a damaged marker is treated as absent rather
/// than as an error).
pub(crate) fn read_marker(storage: &dyn DeviceBackend) -> Option<Marker> {
    let bytes = match storage.read(MARKER_FILE_NAME) {
        Ok(bytes) => bytes?,
        Err(e) => {
            log::warn!("Could not read a device marker: {e}");
            return None;
        }
    };
    match serde_json::from_slice::<Marker>(&bytes) {
        Ok(marker) if !marker.id.trim().is_empty() => Some(marker),
        Ok(_) => None,
        Err(e) => {
            log::warn!("Ignoring an unreadable device marker: {e}");
            None
        }
    }
}

/// Writes the marker at the root of `storage`. Like every backend write, an
/// unplug mid-write leaves the old marker or none, never half of one.
pub(crate) fn write_marker(storage: &dyn DeviceBackend, marker: &Marker) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(marker).map_err(std::io::Error::other)?;
    storage.write(
        MARKER_FILE_NAME,
        &mut json.as_slice(),
        json.len() as u64,
        &mut |_| ControlFlow::Continue(()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FsBackend;

    #[test]
    fn round_trips_and_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FsBackend::new(dir.path());
        assert_eq!(read_marker(&storage), None);

        let mut marker = Marker {
            id: "vol-0088-5572".into(),
            profile: DeviceProfile {
                nickname: Some("R1".into()),
                ..DeviceProfile::default()
            },
        };
        write_marker(&storage, &marker).unwrap();
        assert_eq!(read_marker(&storage), Some(marker.clone()));

        marker.profile.nickname = Some("Gym".into());
        write_marker(&storage, &marker).unwrap();
        assert_eq!(read_marker(&storage), Some(marker));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn damaged_or_idless_markers_read_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FsBackend::new(dir.path());
        std::fs::write(dir.path().join(MARKER_FILE_NAME), "{not json").unwrap();
        assert_eq!(read_marker(&storage), None);
        std::fs::write(dir.path().join(MARKER_FILE_NAME), r#"{"id":" "}"#).unwrap();
        assert_eq!(read_marker(&storage), None);
    }
}
