//! Portable music players and removable drives (#835, #840, epic #834):
//! which devices are connected, what they are called, the files on each of
//! their storages, the profile a user keeps for each, the marker file that
//! carries that profile between computers, and safe eject.
//!
//! Mass-storage volumes are supported on Windows and Linux (udisks2), MTP
//! players and phones on Windows (Windows Portable Devices; Linux is #1845).
//! Other platforms compile but see no devices.

mod backend;
mod marker;
mod naming;
mod profile;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(any(windows, target_os = "linux")))]
mod unsupported;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_mtp;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(any(windows, target_os = "linux")))]
use unsupported as platform;
#[cfg(windows)]
use windows as platform;

pub use backend::{DeviceBackend, DirEntry, FsBackend, Progress, Space};
pub use marker::{Marker, MARKER_FILE_NAME};
pub use profile::{ConvertMode, ConvertSettings, DeviceProfile, TargetFormat};

use parking_lot::Mutex;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;

/// How a device is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    MassStorage,
    Mtp,
}

/// A connected portable player or removable drive.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub kind: DeviceKind,
    /// The id a device has before a marker gives it one: `vol-<volume
    /// serial>` for mass storage, `mtp-<device serial>` for MTP, the same on
    /// every computer. An MTP device that reports no serial gets one from
    /// its Plug and Play id instead, which can change with the USB port or
    /// computer until a saved profile writes its marker.
    pub provisional_id: String,
    /// Vendor and model, e.g. "HiBy R1", when the device reports them.
    pub model: Option<String>,
    /// Never blank.
    pub default_name: String,
    /// At least one. The first holds the device's marker.
    pub storages: Vec<Storage>,
    access: Access,
    /// Lower-case audio format names the device says it plays; empty when
    /// it doesn't say (mass storage).
    reported_formats: Vec<String>,
    cache: Arc<DeviceCache>,
}

/// What a device's marker and default profile were read as, so devices that
/// are slow to read (MTP) are read once per connection. Not part of a
/// device's identity: two `Device`s compare equal whatever is cached.
#[derive(Default)]
pub(crate) struct DeviceCache {
    marker: Mutex<Option<Option<Marker>>>,
    default_profile: Mutex<Option<DeviceProfile>>,
}

impl PartialEq for DeviceCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for DeviceCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeviceCache")
    }
}

/// One storage of a device: the volume of a mass-storage device, or one of
/// an MTP device's storages (internal, SD card).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Storage {
    /// Stable across reconnects: the volume's provisional id, or the MTP
    /// storage's persistent unique id.
    pub id: String,
    /// The volume label, or what an MTP device calls the storage ("SD card").
    pub label: Option<String>,
    /// Where a mass-storage volume is mounted; `None` over MTP.
    pub mount_path: Option<PathBuf>,
    /// `FAT32`, `exFAT`, `vfat`... as the OS or device reports it.
    pub filesystem: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
enum Access {
    Volume(Volume),
    #[cfg(windows)]
    Mtp(windows_mtp::Handle),
}

impl Device {
    /// A mass-storage device on `volume`.
    pub fn from_volume(volume: Volume) -> Self {
        Self {
            kind: DeviceKind::MassStorage,
            provisional_id: volume.provisional_id(),
            model: volume.model_name(),
            default_name: volume.default_name(),
            storages: vec![Storage {
                id: volume.provisional_id(),
                label: volume.label.clone(),
                mount_path: Some(volume.mount_path.clone()),
                filesystem: volume.filesystem.clone(),
                total_bytes: volume.total_bytes,
                free_bytes: volume.free_bytes,
            }],
            access: Access::Volume(volume),
            reported_formats: Vec::new(),
            // Rebuilt on every poll, so a volume's marker is always read fresh.
            cache: Arc::default(),
        }
    }

    /// The files on the storage with this id, `None` if the device has no
    /// such storage. Fails per call once the device is gone.
    pub fn storage(&self, id: &str) -> Option<Box<dyn DeviceBackend>> {
        match &self.access {
            Access::Volume(volume) => {
                (self.storages[0].id == id).then(|| boxed(FsBackend::new(&volume.mount_path)))
            }
            #[cfg(windows)]
            Access::Mtp(handle) => self
                .storages
                .iter()
                .any(|s| s.id == id)
                .then(|| boxed(handle.storage(id))),
        }
    }

    fn primary_storage(&self) -> Box<dyn DeviceBackend> {
        self.storage(&self.storages[0].id)
            .expect("a device always has its first storage")
    }

    /// The device's marker, `None` if it has none. Read once per MTP
    /// connection, since listing over MTP is slow.
    pub fn read_marker(&self) -> Option<Marker> {
        self.cache
            .marker
            .lock()
            .get_or_insert_with(|| marker::read_marker(&*self.primary_storage()))
            .clone()
    }

    /// Writes the device's marker to its first storage.
    pub fn write_marker(&self, marker: &Marker) -> std::io::Result<()> {
        marker::write_marker(&*self.primary_storage(), marker)?;
        *self.cache.marker.lock() = Some(Some(marker.clone()));
        Ok(())
    }

    /// The profile the device starts with before the user saves one: music
    /// in its top-level `Music` folder if it has one, playing the formats it
    /// reports (MTP) or a conservative default.
    pub fn default_profile(&self) -> DeviceProfile {
        self.cache
            .default_profile
            .lock()
            .get_or_insert_with(|| {
                DeviceProfile::defaults_for(&*self.primary_storage(), &self.reported_formats)
            })
            .clone()
    }

    /// Whether `path` (absolute, either separator) is a file on this device.
    /// Always false over MTP, which has no paths on this computer.
    pub fn contains(&self, path: &str) -> bool {
        match &self.access {
            Access::Volume(volume) => volume.contains(path),
            #[cfg(windows)]
            Access::Mtp(_) => false,
        }
    }

    /// Makes the device safe to unplug. A mass-storage volume is flushed,
    /// unmounted and removed the way the OS's own "Safely Remove" does
    /// (blocks for up to a few seconds). An MTP device has nothing to flush,
    /// since every write is complete when it returns: its connection is
    /// closed and it leaves the device list until it is plugged in again.
    pub fn eject(&self) -> Result<(), EjectError> {
        match &self.access {
            Access::Volume(volume) => platform::eject(volume),
            #[cfg(windows)]
            Access::Mtp(handle) => {
                handle.eject();
                Ok(())
            }
        }
    }
}

fn boxed(backend: impl DeviceBackend + 'static) -> Box<dyn DeviceBackend> {
    Box::new(backend)
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
    /// The id a device has before a marker gives it one: derived from
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

    /// Whether `path` (absolute, either separator) is on this volume.
    /// Case-insensitive on Windows; `/media/R1` does not contain `/media/R10`.
    pub fn contains(&self, path: &str) -> bool {
        let normalize = |p: &str| {
            let p = p.replace('\\', "/").trim_end_matches('/').to_string();
            if cfg!(windows) {
                p.to_lowercase()
            } else {
                p
            }
        };
        let path = normalize(path);
        let root = normalize(&self.mount_path.to_string_lossy());
        path == root || path.starts_with(&format!("{root}/"))
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

/// Lists the devices that are connected right now. Keeps what it learns
/// between polls (fixed drives, open MTP connections), so polling every
/// couple of seconds stays cheap. Not `Send`: create and poll it on one
/// thread.
pub struct DeviceWatcher {
    volumes: platform::Watcher,
    #[cfg(windows)]
    mtp: windows_mtp::Watcher,
}

impl DeviceWatcher {
    pub fn new() -> Self {
        Self {
            volumes: platform::Watcher::new(),
            #[cfg(windows)]
            mtp: windows_mtp::Watcher::new(),
        }
    }

    /// Connected devices: mass-storage volumes sorted by mount path, then
    /// MTP devices sorted by id. Platform errors are logged and yield the
    /// devices that could still be read (possibly none).
    pub fn poll(&mut self) -> Vec<Device> {
        let mut volumes = self.volumes.poll();
        volumes.sort_by(|a, b| a.mount_path.cmp(&b.mount_path));
        #[allow(unused_mut)]
        let mut devices: Vec<Device> = volumes.into_iter().map(Device::from_volume).collect();
        #[cfg(windows)]
        {
            let mut mtp = self.mtp.poll();
            mtp.sort_by(|a, b| a.provisional_id.cmp(&b.provisional_id));
            devices.extend(mtp);
        }
        devices
    }
}

impl Default for DeviceWatcher {
    fn default() -> Self {
        Self::new()
    }
}
