//! GNOME and most GTK desktops: MTP devices through gvfs (`gvfs-mtp`), via
//! GIO. A device is an `mtp://` mount whose top-level folders are its
//! storages.
//!
//! Desktops that do not mount MTP devices on their own (KDE with gvfs
//! installed, some GNOME setups) leave an unmounted `mtp://` volume, which
//! the watcher mounts. A locked phone refuses; it is tried again later.
//!
//! Threading: GIO objects stay on the polling thread, apart from the mount
//! root's URI, which the transport resolves on whatever thread calls it
//! (gvfs's blocking calls need no main loop). Volume monitor signals and
//! mount results arrive in the polling thread's own main context, iterated
//! at each poll; the GTK main loop is never touched.

use super::{copy_exactly, Found, Spool, StorageInfo, Transport, UsbInfo};
use crate::backend::cancelled;
use crate::{DirEntry, Progress, Space};
use gio::glib;
use gio::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How long to wait before mounting a volume again after a failed attempt.
const MOUNT_RETRY: Duration = Duration::from_secs(10);
const LIST_ATTRIBUTES: &str = "standard::name,standard::type,standard::size,time::modified";
const SPACE_ATTRIBUTES: &str = "filesystem::size,filesystem::free";

enum Attempt {
    Mounting,
    RetryAt(Instant),
}

pub(super) struct Watcher {
    context: glib::MainContext,
    monitor: Option<gio::VolumeMonitor>,
    /// Mount attempts by volume activation URI.
    attempts: Rc<RefCell<HashMap<String, Attempt>>>,
}

impl Watcher {
    pub(super) fn new() -> Self {
        Self {
            context: glib::MainContext::new(),
            monitor: None,
            attempts: Rc::default(),
        }
    }

    pub(super) fn poll(&mut self) -> Vec<Found> {
        while self.context.iteration(false) {}
        let context = &self.context;
        let monitor = self.monitor.get_or_insert_with(|| {
            context
                .with_thread_default(gio::VolumeMonitor::get)
                .unwrap_or_else(|_| gio::VolumeMonitor::get())
        });

        for volume in monitor.volumes() {
            let Some(uri) = volume.activation_root().map(|root| root.uri().to_string()) else {
                continue;
            };
            if is_mtp(&uri) && volume.get_mount().is_none() {
                mount(context, &self.attempts, &volume, uri);
            }
        }

        monitor
            .mounts()
            .into_iter()
            .filter_map(|mount| {
                let root = mount.root().uri().to_string();
                if !is_mtp(&root) {
                    return None;
                }
                self.attempts.borrow_mut().remove(&root);
                let usb = mount
                    .volume()
                    .and_then(|v| v.identifier("unix-device"))
                    .and_then(|dev| usb_sysfs_dir(Path::new("/sys/bus/usb/devices"), &dev))
                    .map(|dir| UsbInfo::read(&dir))
                    .unwrap_or_default();
                Some(Found {
                    key: format!("gvfs:{root}"),
                    identity: usb.identity(Some(mount.name().to_string()), uri_host(&root)),
                    transport: Box::new(GvfsDevice { root }),
                })
            })
            .collect()
    }
}

fn is_mtp(uri: &str) -> bool {
    uri.starts_with("mtp://")
}

/// `mtp://Google_Pixel_7_0123456789/` is `Google_Pixel_7_0123456789`.
fn uri_host(uri: &str) -> &str {
    let rest = uri.strip_prefix("mtp://").unwrap_or(uri);
    rest.split('/').next().unwrap_or(rest)
}

/// Starts mounting `volume` unless an attempt is under way or failed too
/// recently. The result arrives in `context`.
fn mount(
    context: &glib::MainContext,
    attempts: &Rc<RefCell<HashMap<String, Attempt>>>,
    volume: &gio::Volume,
    uri: String,
) {
    match attempts.borrow().get(&uri) {
        Some(Attempt::Mounting) => return,
        Some(Attempt::RetryAt(at)) if Instant::now() < *at => return,
        _ => {}
    }
    attempts.borrow_mut().insert(uri.clone(), Attempt::Mounting);
    let attempts = Rc::clone(attempts);
    let started = context.with_thread_default(|| {
        volume.mount(
            gio::MountMountFlags::NONE,
            None::<&gio::MountOperation>,
            None::<&gio::Cancellable>,
            move |result| {
                if let Err(e) = result {
                    log::info!("Could not mount {uri}: {e}");
                    attempts
                        .borrow_mut()
                        .insert(uri, Attempt::RetryAt(Instant::now() + MOUNT_RETRY));
                }
            },
        )
    });
    if let Err(e) = started {
        log::warn!("Could not mount an MTP volume: {e}");
    }
}

/// The sysfs folder of the USB device at `dev` (`/dev/bus/usb/001/005`),
/// found by its bus and device numbers under `devices`.
fn usb_sysfs_dir(devices: &Path, dev: &str) -> Option<PathBuf> {
    let mut parts = dev.trim_end_matches('/').rsplit('/');
    let devnum: u32 = parts.next()?.parse().ok()?;
    let busnum: u32 = parts.next()?.parse().ok()?;
    let number = |dir: &Path, name: &str| -> Option<u32> {
        std::fs::read_to_string(dir.join(name))
            .ok()?
            .trim()
            .parse()
            .ok()
    };
    std::fs::read_dir(devices)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|dir| number(dir, "busnum") == Some(busnum) && number(dir, "devnum") == Some(devnum))
}

fn io_error(e: glib::Error) -> io::Error {
    let kind = match e.kind::<gio::IOErrorEnum>() {
        Some(gio::IOErrorEnum::NotFound) => io::ErrorKind::NotFound,
        Some(gio::IOErrorEnum::Cancelled) => io::ErrorKind::Interrupted,
        Some(gio::IOErrorEnum::NotSupported) => io::ErrorKind::Unsupported,
        Some(gio::IOErrorEnum::PermissionDenied) => io::ErrorKind::PermissionDenied,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, e.message().to_string())
}

struct GvfsDevice {
    /// The mount's root URI.
    root: String,
}

impl GvfsDevice {
    fn file(&self, storage: &str, path: &[String]) -> gio::File {
        path.iter().fold(
            gio::File::for_uri(&self.root).child(storage),
            |file, name| file.child(name),
        )
    }

    /// Uploads through a stream, which gvfs-mtp supports on devices with
    /// Android's object-editing extensions. `Ok(false)` when the device
    /// refuses before anything is written.
    fn upload_stream(
        &self,
        file: &gio::File,
        source: &mut dyn Read,
        len: u64,
        progress: Progress<'_>,
    ) -> io::Result<bool> {
        let stream = match file.create(gio::FileCreateFlags::NONE, None::<&gio::Cancellable>) {
            Ok(stream) => stream,
            Err(e) if e.matches(gio::IOErrorEnum::NotSupported) => return Ok(false),
            Err(e) => return Err(io_error(e)),
        };
        copy_exactly(
            source,
            len,
            |chunk| {
                stream
                    .write_all(chunk, None::<&gio::Cancellable>)
                    .map(drop)
                    .map_err(io_error)
            },
            progress,
        )?;
        stream.close(None::<&gio::Cancellable>).map_err(io_error)?;
        Ok(true)
    }

    /// Uploads a local copy, which gvfs-mtp sends as a whole file.
    fn upload_copy(
        &self,
        file: &gio::File,
        source: &mut dyn Read,
        len: u64,
        progress: Progress<'_>,
    ) -> io::Result<()> {
        let spool = Spool::fill(source, len, &mut *progress)?;
        let cancellable = gio::Cancellable::new();
        let mut was_cancelled = false;
        let result = gio::File::for_path(&spool.path).copy(
            file,
            gio::FileCopyFlags::NONE,
            Some(&cancellable),
            Some(&mut |sent, _total| {
                if progress(sent.max(0) as u64).is_break() {
                    was_cancelled = true;
                    cancellable.cancel();
                }
            }),
        );
        match result {
            _ if was_cancelled => Err(cancelled()),
            Ok(()) => Ok(()),
            Err(e) => Err(io_error(e)),
        }
    }
}

impl Transport for GvfsDevice {
    fn storages(&self) -> io::Result<Vec<StorageInfo>> {
        let root = gio::File::for_uri(&self.root);
        let mut storages = Vec::new();
        for entry in list(&root)? {
            if !entry.is_dir {
                continue;
            }
            let storage = root.child(&entry.name);
            let writable = storage
                .query_info(
                    "access::can-write",
                    gio::FileQueryInfoFlags::NONE,
                    None::<&gio::Cancellable>,
                )
                .map(|info| info.boolean("access::can-write"))
                .unwrap_or(true);
            let space = space(&storage).unwrap_or(Space {
                total_bytes: 0,
                free_bytes: 0,
            });
            storages.push(StorageInfo {
                key: entry.name.clone(),
                name: entry.name,
                total_bytes: space.total_bytes,
                free_bytes: space.free_bytes,
                writable,
            });
        }
        Ok(storages)
    }

    fn list(&self, storage: &str, dir: &[String]) -> io::Result<Option<Vec<DirEntry>>> {
        match list(&self.file(storage, dir)) {
            Ok(entries) => Ok(Some(entries)),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    fn read(&self, storage: &str, path: &[String]) -> io::Result<Vec<u8>> {
        self.file(storage, path)
            .load_contents(None::<&gio::Cancellable>)
            .map(|(bytes, _etag)| bytes.to_vec())
            .map_err(io_error)
    }

    fn upload(
        &self,
        storage: &str,
        path: &[String],
        source: &mut dyn Read,
        len: u64,
        progress: Progress<'_>,
    ) -> io::Result<()> {
        let file = self.file(storage, path);
        if self.upload_stream(&file, source, len, &mut *progress)? {
            return Ok(());
        }
        self.upload_copy(&file, source, len, progress)
    }

    fn create_folder(&self, storage: &str, path: &[String]) -> io::Result<()> {
        self.file(storage, path)
            .make_directory(None::<&gio::Cancellable>)
            .map_err(io_error)
    }

    fn rename(&self, storage: &str, path: &[String], name: &str) -> io::Result<()> {
        self.file(storage, path)
            .set_display_name(name, None::<&gio::Cancellable>)
            .map(drop)
            .map_err(io_error)
    }

    fn delete(&self, storage: &str, path: &[String]) -> io::Result<()> {
        self.file(storage, path)
            .delete(None::<&gio::Cancellable>)
            .map_err(io_error)
    }

    fn space(&self, storage: &str) -> io::Result<Space> {
        space(&gio::File::for_uri(&self.root).child(storage))
    }

    /// Unmounts the device, waiting up to a few seconds for gvfs.
    fn eject(&self) {
        let Ok(mount) =
            gio::File::for_uri(&self.root).find_enclosing_mount(None::<&gio::Cancellable>)
        else {
            return;
        };
        let context = glib::MainContext::new();
        let done = Rc::new(std::cell::Cell::new(false));
        let finished = Rc::clone(&done);
        let root = self.root.clone();
        let started = context.with_thread_default(|| {
            mount.unmount_with_operation(
                gio::MountUnmountFlags::NONE,
                None::<&gio::MountOperation>,
                None::<&gio::Cancellable>,
                move |result| {
                    if let Err(e) = result {
                        log::warn!("Could not unmount {root}: {e}");
                    }
                    finished.set(true);
                },
            )
        });
        if started.is_err() {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done.get() && Instant::now() < deadline {
            if !context.iteration(false) {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn list(folder: &gio::File) -> io::Result<Vec<DirEntry>> {
    let entries = folder
        .enumerate_children(
            LIST_ATTRIBUTES,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            None::<&gio::Cancellable>,
        )
        .map_err(io_error)?;
    let mut listing = Vec::new();
    while let Some(info) = entries
        .next_file(None::<&gio::Cancellable>)
        .map_err(io_error)?
    {
        let is_dir = info.file_type() == gio::FileType::Directory;
        let modified = info.attribute_uint64("time::modified");
        listing.push(DirEntry {
            name: info.name().to_string_lossy().into_owned(),
            is_dir,
            size: if is_dir { 0 } else { info.size().max(0) as u64 },
            modified: (modified > 0).then_some(modified as i64),
        });
    }
    Ok(listing)
}

fn space(storage: &gio::File) -> io::Result<Space> {
    let info = storage
        .query_filesystem_info(SPACE_ATTRIBUTES, None::<&gio::Cancellable>)
        .map_err(io_error)?;
    Ok(Space {
        total_bytes: info.attribute_uint64("filesystem::size"),
        free_bytes: info.attribute_uint64("filesystem::free"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_mtp_uris_are_devices() {
        assert!(is_mtp("mtp://Google_Pixel_7_0123/"));
        assert!(!is_mtp("file:///run/media/me/R1"));
        assert!(!is_mtp("gphoto2://Apple_iPhone/"));
        assert_eq!(
            uri_host("mtp://Google_Pixel_7_0123/Internal/"),
            "Google_Pixel_7_0123"
        );
    }

    #[test]
    fn finds_the_usb_device_by_bus_and_device_number() {
        let devices = tempfile::tempdir().unwrap();
        for (port, bus, dev) in [("1-1", "1", "4"), ("1-2", "1\n", "5\n"), ("2-2", "2", "5")] {
            let dir = devices.path().join(port);
            std::fs::create_dir(&dir).unwrap();
            std::fs::write(dir.join("busnum"), bus).unwrap();
            std::fs::write(dir.join("devnum"), dev).unwrap();
        }
        assert_eq!(
            usb_sysfs_dir(devices.path(), "/dev/bus/usb/001/005"),
            Some(devices.path().join("1-2"))
        );
        assert_eq!(usb_sysfs_dir(devices.path(), "/dev/bus/usb/003/001"), None);
        assert_eq!(usb_sysfs_dir(devices.path(), "nonsense"), None);
    }
}
