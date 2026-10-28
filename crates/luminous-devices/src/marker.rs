//! `.luminous-device.json`: the device's id and profile, kept on the device
//! itself so another computer (or a fresh install) recognizes it.
//!
//! Written only when the user saves a profile, never on detection, so
//! plugging in a drive never writes to it.

use crate::DeviceProfile;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MARKER_FILE_NAME: &str = ".luminous-device.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub id: String,
    #[serde(default)]
    pub profile: DeviceProfile,
}

/// The marker at `root`, or `None` if there is none or it can't be read or
/// parsed (a damaged marker is treated as absent rather than as an error).
pub fn read_marker(root: &Path) -> Option<Marker> {
    let text = std::fs::read_to_string(root.join(MARKER_FILE_NAME)).ok()?;
    match serde_json::from_str::<Marker>(&text) {
        Ok(marker) if !marker.id.trim().is_empty() => Some(marker),
        Ok(_) => None,
        Err(e) => {
            log::warn!(
                "Ignoring unreadable device marker in {}: {e}",
                root.display()
            );
            None
        }
    }
}

/// Writes the marker at `root` through a temporary file and a rename, so an
/// unplug mid-write leaves the old marker or none, never half of one.
pub fn write_marker(root: &Path, marker: &Marker) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(marker).map_err(std::io::Error::other)?;
    let tmp = root.join(format!("{MARKER_FILE_NAME}.tmp"));
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, root.join(MARKER_FILE_NAME)).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_marker(dir.path()), None);

        let mut marker = Marker {
            id: "vol-0088-5572".into(),
            profile: DeviceProfile {
                nickname: Some("R1".into()),
                ..DeviceProfile::default()
            },
        };
        write_marker(dir.path(), &marker).unwrap();
        assert_eq!(read_marker(dir.path()), Some(marker.clone()));

        marker.profile.nickname = Some("Gym".into());
        write_marker(dir.path(), &marker).unwrap();
        assert_eq!(read_marker(dir.path()), Some(marker));
        assert!(!dir.path().join(format!("{MARKER_FILE_NAME}.tmp")).exists());
    }

    #[test]
    fn damaged_or_idless_markers_read_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MARKER_FILE_NAME), "{not json").unwrap();
        assert_eq!(read_marker(dir.path()), None);
        std::fs::write(dir.path().join(MARKER_FILE_NAME), r#"{"id":" "}"#).unwrap();
        assert_eq!(read_marker(dir.path()), None);
    }
}
