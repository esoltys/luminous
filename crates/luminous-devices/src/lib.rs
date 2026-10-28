//! Portable music players and removable drives (#835, epic #834): which
//! volumes are devices, what they are called, the profile a user keeps for
//! each, the marker file that carries that profile between computers, and
//! safe eject.
//!
//! Supported on Windows and Linux (udisks2). Other platforms compile but see
//! no devices.

mod marker;
mod naming;
mod profile;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(any(windows, target_os = "linux")))]
mod unsupported;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(any(windows, target_os = "linux")))]
use unsupported as platform;
#[cfg(windows)]
use windows as platform;

pub use marker::{read_marker, write_marker, Marker, MARKER_FILE_NAME};
pub use profile::DeviceProfile;

use serde::Serialize;
use std::path::PathBuf;

/// How a device is reached. Only mass storage is detected today; MTP players
/// arrive with #840.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    MassStorage,
    Mtp,
}

/// A mounted removable volume: a USB or SD/MMC volume, or one the OS flags
/// as removable. Never the system volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// Root the volume is mounted at (`E:\` on Windows, `/run/media/...` on Linux).
    pub mount_path: PathBuf,
    /// Volume label, `None` when the volume has none.
    pub label: Option<String>,
    /// Filesystem name as the OS reports it (`FAT32`, `exFAT`, `vfat`, `ext4`...).
    pub filesystem: String,
    /// Volume serial in `XXXX-XXXX` form for FAT/exFAT, which Windows and
    /// Linux report identically. Changes when the volume is reformatted.
    pub serial: Option<String>,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub total_bytes: u64,
    /// Bytes available to this user.
    pub free_bytes: u64,
    /// Opaque platform handle `eject` uses: the drive letter on Windows, the
    /// udisks2 block object path on Linux.
    pub os_ref: String,
}

impl Volume {
    /// The id a device has before a marker file gives it one: derived from
    /// the volume serial, so it is the same on every computer until the
    /// volume is reformatted.
    pub fn provisional_id(&self) -> String {
        match &self.serial {
            Some(serial) => format!("vol-{}", serial.to_ascii_uppercase()),
            None => format!("vol-{}", self.mount_path.to_string_lossy()),
        }
    }

    /// Vendor and model as a readable name with the generic storage words
    /// removed ("HiBy" + "R1 U-DISK" is "HiBy R1"), or `None` if nothing
    /// specific is left.
    pub fn model_name(&self) -> Option<String> {
        naming::model_name(self.vendor.as_deref(), self.model.as_deref())
    }

    /// A name that is never blank: the volume label, else the model name,
    /// else the mount point.
    pub fn default_name(&self) -> String {
        naming::default_name(self)
    }
}

/// Why an eject did not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EjectError {
    /// Another program has files open on the device. Carries what the OS
    /// named as the blocker, when it says.
    InUse(Option<String>),
    Failed(String),
}

impl std::fmt::Display for EjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EjectError::InUse(Some(by)) => write!(f, "device is in use ({by})"),
            EjectError::InUse(None) => write!(f, "device is in use"),
            EjectError::Failed(msg) => write!(f, "eject failed: {msg}"),
        }
    }
}

impl std::error::Error for EjectError {}

/// Lists the removable volumes that are mounted right now. Keeps whatever
/// it learns about fixed drives between polls, so polling every couple of
/// seconds stays cheap. Not `Sync`: own one per polling thread.
pub struct VolumeWatcher {
    inner: platform::Watcher,
}

impl VolumeWatcher {
    pub fn new() -> Self {
        Self {
            inner: platform::Watcher::new(),
        }
    }

    /// Mounted removable volumes, sorted by mount path. Platform errors are
    /// logged and yield the volumes that could still be read (possibly none).
    pub fn poll(&mut self) -> Vec<Volume> {
        let mut volumes = self.inner.poll();
        volumes.sort_by(|a, b| a.mount_path.cmp(&b.mount_path));
        volumes
    }
}

impl Default for VolumeWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Flushes and unmounts `volume`, then asks the OS to remove the device the
/// way its own "Safely Remove" does. Blocks for up to a few seconds.
pub fn eject(volume: &Volume) -> Result<(), EjectError> {
    platform::eject(volume)
}
