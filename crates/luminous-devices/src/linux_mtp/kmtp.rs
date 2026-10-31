//! KDE Plasma: MTP devices through kio-mtp's daemon, `kmtpd`
//! (`org.kde.kmtpd5` on the session bus, part of kio-extras).
//!
//! kmtpd paths are `/`-separated below a storage's root and match names
//! exactly. Transfers go through a file descriptor and return at once; the
//! storage then signals `copyProgress` and `copyFinished`. A transfer cannot
//! be cancelled once it has started, so a cancel takes effect when the file
//! is complete and the caller removes it.

use super::{Found, Spool, StorageInfo, Transport, UsbInfo};
use crate::backend::cancelled;
use crate::{DirEntry, Progress, Space};
use std::io::{self, Read};
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use zbus::blocking::{fdo::DBusProxy, Connection, MessageIterator, Proxy};
use zbus::message::Type as MessageType;
use zbus::proxy::CacheProperties;
use zbus::zvariant::{Fd, OwnedObjectPath, OwnedValue};
use zbus::MatchRule;

const SERVICE: &str = "org.kde.kmtpd5";
const DAEMON_PATH: &str = "/modules/kmtpd";
const DAEMON: &str = "org.kde.kmtp.Daemon";
const DEVICE: &str = "org.kde.kmtp.Device";
const STORAGE: &str = "org.kde.kmtp.Storage";
/// Solid prefixes a udev device's sysfs path with this to make its UDI.
const SOLID_UDEV_PREFIX: &str = "/org/kde/solid/udev";
/// A transfer that reports nothing for this long is taken as lost (the
/// daemon crashed or the device hung).
const STALL: Duration = Duration::from_secs(60);

/// One file as `getFilesAndFolders` returns it: item, parent and storage
/// ids, name, size, modified (Unix seconds) and MIME type.
type KmtpFile = (u32, u32, u32, String, u64, i64, String);

pub(super) struct Watcher {
    connection: Option<Connection>,
}

impl Watcher {
    pub(super) fn new() -> Self {
        Self { connection: None }
    }

    pub(super) fn poll(&mut self) -> Vec<Found> {
        match self.devices() {
            Ok(found) => found,
            Err(e) => {
                if self.connection.take().is_some() {
                    log::warn!("kmtpd query failed: {e}");
                }
                Vec::new()
            }
        }
    }

    fn devices(&mut self) -> zbus::Result<Vec<Found>> {
        if self.connection.is_none() {
            self.connection = Some(Connection::session()?);
        }
        let connection = self.connection.as_ref().expect("connected above");
        if !should_ask(connection)? {
            return Ok(Vec::new());
        }
        let daemon = proxy(connection, DAEMON_PATH, DAEMON)?;
        let paths: Vec<OwnedObjectPath> = daemon.call("listDevices", &())?;
        let mut found = Vec::new();
        for path in paths {
            let (udi, name) = {
                let device = proxy(connection, path.as_str(), DEVICE)?;
                let udi: String = device.get_property("udi").unwrap_or_default();
                (udi, device.get_property::<String>("friendlyName").ok())
            };
            found.push(Found {
                key: format!("kmtp:{udi}:{}", path.as_str()),
                identity: UsbInfo::read(&sysfs_dir(&udi)).identity(name, &udi),
                transport: Box::new(KmtpDevice {
                    connection: connection.clone(),
                    device: path,
                }),
            });
        }
        Ok(found)
    }
}

/// Whether to ask kmtpd for devices: when it is running, or on KDE, where
/// asking starts it. Never started elsewhere, where it would fight the
/// desktop's own daemon (gvfs) for the device.
fn should_ask(connection: &Connection) -> zbus::Result<bool> {
    let running = DBusProxy::new(connection)?.name_has_owner(SERVICE.try_into()?)?;
    Ok(running || is_kde(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref()))
}

fn is_kde(current_desktop: Option<&str>) -> bool {
    current_desktop.is_some_and(|d| d.split(':').any(|d| d.eq_ignore_ascii_case("KDE")))
}

/// The sysfs folder of the USB device a Solid UDI names
/// (`/org/kde/solid/udev/sys/devices/.../1-2`).
fn sysfs_dir(udi: &str) -> PathBuf {
    let path = udi.strip_prefix(SOLID_UDEV_PREFIX).unwrap_or(udi);
    if path.starts_with("/sys/") {
        PathBuf::from(path)
    } else {
        PathBuf::from(format!("/sys{path}"))
    }
}

/// A proxy that never caches properties: kmtpd refreshes free space only
/// when it is read.
fn proxy<'a>(
    connection: &Connection,
    path: &'a str,
    interface: &'a str,
) -> zbus::Result<Proxy<'a>> {
    zbus::blocking::proxy::Builder::new(connection)
        .destination(SERVICE)?
        .path(path)?
        .interface(interface)?
        .cache_properties(CacheProperties::No)
        .build()
}

struct KmtpDevice {
    connection: Connection,
    device: OwnedObjectPath,
}

fn dbus_error(e: zbus::Error) -> io::Error {
    io::Error::other(e.to_string())
}

/// kmtpd's result codes: 0 is success.
fn check(result: i32, what: &str) -> io::Result<()> {
    match result {
        0 => Ok(()),
        _ => Err(io::Error::other(format!("{what} failed ({result})"))),
    }
}

fn device_path(path: &[String]) -> String {
    format!("/{}", path.join("/"))
}

impl KmtpDevice {
    fn storage(&self, storage: &str) -> io::Result<Proxy<'static>> {
        let path =
            OwnedObjectPath::try_from(storage.to_string()).map_err(|e| dbus_error(e.into()))?;
        zbus::blocking::proxy::Builder::new(&self.connection)
            .destination(SERVICE)
            .and_then(|b| b.path(path))
            .and_then(|b| b.interface(STORAGE))
            .map(|b| b.cache_properties(CacheProperties::No))
            .and_then(|b| b.build())
            .map_err(dbus_error)
    }

    /// Starts a transfer with `start` and waits for the storage to finish
    /// it, reporting progress. A cancel waits for the end of the file, then
    /// fails with [`io::ErrorKind::Interrupted`].
    fn transfer(
        &self,
        storage: &str,
        start: impl FnOnce(&Proxy<'static>) -> zbus::Result<i32>,
        progress: Progress<'_>,
    ) -> io::Result<()> {
        // Its own connection, so the signals of this transfer are all that
        // reach the iterator, subscribed before the transfer starts.
        let connection = Connection::session().map_err(dbus_error)?;
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender(SERVICE)
            .and_then(|b| b.path(storage.to_string()))
            .and_then(|b| b.interface(STORAGE))
            .map_err(dbus_error)?
            .build();
        let signals =
            MessageIterator::for_match_rule(rule, &connection, None).map_err(dbus_error)?;
        let (sender, received) = mpsc::channel();
        // Blocks until the next signal, so it outlives a stalled transfer
        // until the storage signals again or the connection closes.
        std::thread::Builder::new()
            .name("kmtpd-transfer".into())
            .spawn(move || {
                for message in signals.flatten() {
                    if sender.send(message).is_err() {
                        break;
                    }
                }
            })?;

        let storage = KmtpDevice {
            connection,
            device: self.device.clone(),
        }
        .storage(storage)?;
        check(
            start(&storage).map_err(dbus_error)?,
            "starting the transfer",
        )?;

        let mut cancel = false;
        loop {
            let message = received
                .recv_timeout(STALL)
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "transfer stalled"))?;
            let header = message.header();
            match header.member().map(|m| m.as_str()) {
                Some("copyProgress") => {
                    if let Ok((sent, _total)) = message.body().deserialize::<(u64, u64)>() {
                        cancel |= progress(sent).is_break();
                    }
                }
                Some("copyFinished") => {
                    let result: i32 = message.body().deserialize().map_err(dbus_error)?;
                    check(result, "the transfer")?;
                    return if cancel { Err(cancelled()) } else { Ok(()) };
                }
                _ => {}
            }
        }
    }
}

impl Transport for KmtpDevice {
    fn storages(&self) -> io::Result<Vec<StorageInfo>> {
        let device = proxy(&self.connection, self.device.as_str(), DEVICE).map_err(dbus_error)?;
        let paths: Vec<OwnedObjectPath> = device.call("listStorages", &()).map_err(dbus_error)?;
        let mut storages = Vec::new();
        for path in paths {
            let storage = self.storage(path.as_str())?;
            let number = |name: &str| -> u64 {
                storage
                    .get_property::<OwnedValue>(name)
                    .ok()
                    .and_then(|v| u64::try_from(v).ok())
                    .unwrap_or(0)
            };
            storages.push(StorageInfo {
                name: storage
                    .get_property::<String>("description")
                    .ok()
                    .filter(|d| !d.trim().is_empty())
                    .unwrap_or_else(|| "Storage".to_string()),
                total_bytes: number("maxCapacity"),
                free_bytes: number("freeSpaceInBytes"),
                // kmtpd does not expose the storage's access capability;
                // like WPD, a storage that does not say is taken as writable.
                writable: true,
                key: path.as_str().to_string(),
            });
        }
        Ok(storages)
    }

    fn list(&self, storage: &str, dir: &[String]) -> io::Result<Option<Vec<DirEntry>>> {
        let (files, exists): (Vec<KmtpFile>, i32) = self
            .storage(storage)?
            .call("getFilesAndFolders", &(device_path(dir),))
            .map_err(dbus_error)?;
        // 1: no such entry, 2: a file.
        if exists != 0 {
            return Ok(None);
        }
        Ok(Some(
            files
                .into_iter()
                .map(|(_, _, _, name, size, modified, mime)| {
                    let is_dir = mime == "inode/directory";
                    DirEntry {
                        name,
                        is_dir,
                        size: if is_dir { 0 } else { size },
                        modified: (modified > 0).then_some(modified),
                    }
                })
                .collect(),
        ))
    }

    fn read(&self, storage: &str, path: &[String]) -> io::Result<Vec<u8>> {
        let spool = Spool::new()?;
        let source = device_path(path);
        self.transfer(
            storage,
            |s| {
                s.call(
                    "getFileToFileDescriptor",
                    &(Fd::from(spool.file.as_fd()), &source),
                )
            },
            &mut |_| std::ops::ControlFlow::Continue(()),
        )?;
        let mut bytes = Vec::new();
        std::fs::File::open(&spool.path)?.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn upload(
        &self,
        storage: &str,
        path: &[String],
        source: &mut dyn Read,
        len: u64,
        progress: Progress<'_>,
    ) -> io::Result<()> {
        let spool = Spool::fill(source, len, &mut *progress)?;
        let destination = device_path(path);
        self.transfer(
            storage,
            |s| {
                s.call(
                    "sendFileFromFileDescriptor",
                    &(Fd::from(spool.file.as_fd()), &destination),
                )
            },
            progress,
        )
    }

    fn create_folder(&self, storage: &str, path: &[String]) -> io::Result<()> {
        let id: u32 = self
            .storage(storage)?
            .call("createFolder", &(device_path(path),))
            .map_err(dbus_error)?;
        // 0 is a failure, or a folder kmtpd already knew of.
        if id == 0 {
            let (parent, name) = path.split_at(path.len() - 1);
            let exists = self
                .list(storage, parent)?
                .unwrap_or_default()
                .iter()
                .any(|e| e.is_dir && e.name == name[0]);
            if !exists {
                return Err(io::Error::other("creating the folder failed"));
            }
        }
        Ok(())
    }

    fn rename(&self, storage: &str, path: &[String], name: &str) -> io::Result<()> {
        let result: i32 = self
            .storage(storage)?
            .call("setFileName", &(device_path(path), name))
            .map_err(dbus_error)?;
        check(result, "renaming")
    }

    fn delete(&self, storage: &str, path: &[String]) -> io::Result<()> {
        let result: i32 = self
            .storage(storage)?
            .call("deleteObject", &(device_path(path),))
            .map_err(dbus_error)?;
        check(result, "deleting")
    }

    fn space(&self, storage: &str) -> io::Result<Space> {
        let storage = self.storage(storage)?;
        let number = |name: &str| -> io::Result<u64> {
            let value: OwnedValue = storage.get_property(name).map_err(dbus_error)?;
            u64::try_from(value).map_err(|e| dbus_error(e.into()))
        };
        Ok(Space {
            total_bytes: number("maxCapacity")?,
            free_bytes: number("freeSpaceInBytes")?,
        })
    }

    fn eject(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udis_name_the_usb_device_in_sysfs() {
        assert_eq!(
            sysfs_dir("/org/kde/solid/udev/sys/devices/pci0000:00/0000:00:14.0/usb1/1-2"),
            PathBuf::from("/sys/devices/pci0000:00/0000:00:14.0/usb1/1-2")
        );
        assert_eq!(
            sysfs_dir("/org/kde/solid/udev/devices/pci0000:00/usb1/1-2"),
            PathBuf::from("/sys/devices/pci0000:00/usb1/1-2")
        );
    }

    #[test]
    fn kmtpd_is_started_only_on_kde() {
        assert!(is_kde(Some("KDE")));
        assert!(is_kde(Some("ubuntu:KDE")));
        assert!(!is_kde(Some("GNOME")));
        assert!(!is_kde(None));
    }

    #[test]
    fn device_paths_start_at_the_storage_root() {
        assert_eq!(device_path(&[]), "/");
        assert_eq!(
            device_path(&["Music".into(), "a.flac".into()]),
            "/Music/a.flac"
        );
    }
}
