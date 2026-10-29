//! Linux: mounted filesystems from udisks2, ejected through udisks2.
//!
//! Under Flatpak this needs `--system-talk-name=org.freedesktop.UDisks2` and
//! filesystem access to `/run/media` and `/media`.

use crate::{EjectError, Volume};
use std::collections::HashMap;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

const UDISKS: &str = "org.freedesktop.UDisks2";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
const DRIVE: &str = "org.freedesktop.UDisks2.Drive";

type Properties = HashMap<String, OwnedValue>;
type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, Properties>>;

pub(crate) struct Watcher {
    connection: Option<Connection>,
}

impl Watcher {
    pub(crate) fn new() -> Self {
        Self { connection: None }
    }

    pub(crate) fn poll(&mut self) -> Vec<Volume> {
        match self.managed_objects() {
            Ok(objects) => volumes(&objects),
            Err(e) => {
                // Reconnect next time: udisks may have restarted, or not be
                // installed at all (then there are simply no devices).
                if self.connection.take().is_some() {
                    log::warn!("udisks2 query failed: {e}");
                }
                Vec::new()
            }
        }
    }

    fn managed_objects(&mut self) -> zbus::Result<ManagedObjects> {
        if self.connection.is_none() {
            self.connection = Some(Connection::system()?);
        }
        let connection = self.connection.as_ref().expect("connected above");
        let proxy = Proxy::new(
            connection,
            UDISKS,
            "/org/freedesktop/UDisks2",
            "org.freedesktop.DBus.ObjectManager",
        )?;
        proxy.call("GetManagedObjects", &())
    }
}

fn volumes(objects: &ManagedObjects) -> Vec<Volume> {
    objects
        .iter()
        .filter_map(|(path, interfaces)| volume(objects, path, interfaces))
        .collect()
}

fn volume(
    objects: &ManagedObjects,
    path: &OwnedObjectPath,
    interfaces: &HashMap<String, Properties>,
) -> Option<Volume> {
    let block = interfaces.get(BLOCK)?;
    let filesystem = interfaces.get(FILESYSTEM)?;
    if flag(block, "HintSystem") || flag(block, "HintIgnore") {
        return None;
    }
    let mount_path = mount_points(filesystem).into_iter().next()?;

    let drive = string_path(block, "Drive")
        .and_then(|drive| objects.get(&drive))
        .and_then(|interfaces| interfaces.get(DRIVE));
    let on_removable_bus = drive.is_some_and(|d| {
        matches!(text(d, "ConnectionBus").as_deref(), Some("usb" | "sdio"))
            || flag(d, "Removable")
            || flag(d, "MediaRemovable")
    });
    if !on_removable_bus {
        return None;
    }

    let (total_bytes, free_bytes) = space(&mount_path)?;
    Some(Volume {
        label: text(block, "IdLabel"),
        filesystem: text(block, "IdType").unwrap_or_default(),
        serial: text(block, "IdUUID").map(|s| s.to_ascii_uppercase()),
        vendor: drive.and_then(|d| text(d, "Vendor")),
        model: drive.and_then(|d| text(d, "Model")),
        total_bytes,
        free_bytes,
        os_ref: path.to_string(),
        mount_path,
    })
}

pub(crate) fn eject(volume: &Volume) -> Result<(), EjectError> {
    let failed = |e: zbus::Error| EjectError::Failed(e.to_string());
    let connection = Connection::system().map_err(failed)?;
    let options: HashMap<&str, Value> = HashMap::new();

    let block_path = ObjectPath::try_from(volume.os_ref.as_str()).map_err(|e| failed(e.into()))?;
    let filesystem = Proxy::new(&connection, UDISKS, &block_path, FILESYSTEM).map_err(failed)?;
    if let Err(e) = filesystem.call_method("Unmount", &(&options,)) {
        return Err(match &e {
            zbus::Error::MethodError(name, _, _) if name.as_str().ends_with("DeviceBusy") => {
                EjectError::InUse(None)
            }
            _ => failed(e),
        });
    }

    // Powering the drive off is what makes it safe to pull; a card reader or
    // drive that can't power off is already safe once unmounted.
    let block = Proxy::new(&connection, UDISKS, &block_path, BLOCK).map_err(failed)?;
    if let Ok(drive_path) = block.get_property::<OwnedObjectPath>("Drive") {
        if drive_path.as_str() != "/" {
            let drive = Proxy::new(&connection, UDISKS, &drive_path, DRIVE).map_err(failed)?;
            if drive.get_property::<bool>("CanPowerOff").unwrap_or(false) {
                if let Err(e) = drive.call_method("PowerOff", &(&options,)) {
                    log::warn!(
                        "Unmounted {} but could not power it off: {e}",
                        volume.os_ref
                    );
                }
            }
        }
    }
    Ok(())
}

fn flag(props: &Properties, key: &str) -> bool {
    props
        .get(key)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}

fn text(props: &Properties, key: &str) -> Option<String> {
    props
        .get(key)
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn string_path(props: &Properties, key: &str) -> Option<OwnedObjectPath> {
    let path = props.get(key)?.try_clone().ok()?;
    let path = OwnedObjectPath::try_from(path).ok()?;
    (path.as_str() != "/").then_some(path)
}

/// udisks2 sends mount points as NUL-terminated byte strings.
fn mount_points(filesystem: &Properties) -> Vec<PathBuf> {
    let Some(value) = filesystem
        .get("MountPoints")
        .and_then(|v| v.try_clone().ok())
    else {
        return Vec::new();
    };
    let points: Vec<Vec<u8>> = Vec::try_from(value).unwrap_or_default();
    points
        .into_iter()
        .map(|mut bytes| {
            if bytes.last() == Some(&0) {
                bytes.pop();
            }
            PathBuf::from(std::ffi::OsStr::from_bytes(&bytes))
        })
        .filter(|p| !p.as_os_str().is_empty())
        .collect()
}

/// Total and available bytes, or `None` if the mount point can't be read
/// (outside the Flatpak sandbox's view, or unmounted mid-poll).
pub(crate) fn space(mount: &Path) -> Option<(u64, u64)> {
    let path = CString::new(mount.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let block = stat.f_frsize as u64;
    Some((stat.f_blocks as u64 * block, stat.f_bavail as u64 * block))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(v: impl Into<Value<'static>>) -> OwnedValue {
        OwnedValue::try_from(v.into()).unwrap()
    }

    fn path(p: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(p).unwrap()
    }

    /// One udisks2 block device with a mounted filesystem on `drive`.
    fn objects(mount: &Path, system: bool, bus: &'static str, removable: bool) -> ManagedObjects {
        let mut mount_bytes = mount.as_os_str().as_bytes().to_vec();
        mount_bytes.push(0);
        let block: Properties = HashMap::from([
            ("HintSystem".into(), value(system)),
            ("HintIgnore".into(), value(false)),
            ("IdLabel".into(), value("")),
            ("IdType".into(), value("vfat")),
            ("IdUUID".into(), value("0088-5572")),
            (
                "Drive".into(),
                value(ObjectPath::try_from("/org/freedesktop/UDisks2/drives/R1").unwrap()),
            ),
        ]);
        let filesystem: Properties =
            HashMap::from([("MountPoints".into(), value(vec![mount_bytes]))]);
        let drive: Properties = HashMap::from([
            ("ConnectionBus".into(), value(bus)),
            ("Removable".into(), value(removable)),
            ("MediaRemovable".into(), value(false)),
            ("Vendor".into(), value("HiBy")),
            ("Model".into(), value("R1 U-DISK")),
        ]);
        HashMap::from([
            (
                path("/org/freedesktop/UDisks2/block_devices/sdb1"),
                HashMap::from([(BLOCK.into(), block), (FILESYSTEM.into(), filesystem)]),
            ),
            (
                path("/org/freedesktop/UDisks2/drives/R1"),
                HashMap::from([(DRIVE.into(), drive)]),
            ),
        ])
    }

    #[test]
    fn lists_a_mounted_usb_volume() {
        let mount = tempfile::tempdir().unwrap();
        let found = volumes(&objects(mount.path(), false, "usb", false));
        assert_eq!(found.len(), 1);
        let v = &found[0];
        assert_eq!(v.mount_path, mount.path());
        assert_eq!(v.label, None);
        assert_eq!(v.filesystem, "vfat");
        assert_eq!(v.serial.as_deref(), Some("0088-5572"));
        assert_eq!(v.default_name(), "HiBy R1");
        assert_eq!(v.os_ref, "/org/freedesktop/UDisks2/block_devices/sdb1");
        assert!(v.total_bytes > 0);
    }

    #[test]
    fn skips_system_and_internal_volumes() {
        let mount = tempfile::tempdir().unwrap();
        assert!(volumes(&objects(mount.path(), true, "usb", false)).is_empty());
        assert!(volumes(&objects(mount.path(), false, "", false)).is_empty());
        assert_eq!(volumes(&objects(mount.path(), false, "", true)).len(), 1);
    }
}
