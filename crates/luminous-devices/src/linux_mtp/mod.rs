//! Linux: MTP players and phones through the desktop's own MTP daemon
//! (#1845): gvfs (`gvfs-mtp`, GNOME and most GTK desktops) through GIO, or
//! KDE's kio-mtp daemon (`kmtpd`) over D-Bus.
//!
//! The daemon claims a device as soon as it is plugged in, so anything that
//! opens the USB device itself (libmtp) gets "device busy"; going through
//! the daemon cooperates with that claim. A device is owned by one daemon,
//! so it is listed once; a desktop running neither sees no MTP devices.
//!
//! Each daemon is a [`Transport`]: path-addressed, exact-name operations on
//! one device. Everything MTP needs on top of that (case-insensitive names,
//! writes under a partial name, devices that cannot rename, removing a
//! folder with its contents) is here, once, for both.

mod gvfs;
mod kmtp;

use crate::backend::{cancelled, components, PARTIAL_SUFFIX};
use crate::{naming, DeviceBackend, DeviceCache, DeviceKind, DirEntry, Progress, Space, Storage};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// One storage as a daemon reports it.
pub(crate) struct StorageInfo {
    /// What the transport calls the storage in every other call.
    key: String,
    /// What the device calls it ("Internal shared storage", "SD card").
    name: String,
    total_bytes: u64,
    free_bytes: u64,
    writable: bool,
}

/// One MTP device as its daemon serves it. Paths are the exact names of
/// each folder below a storage's root, never empty for a file. Calls may
/// come from any thread, one at a time.
pub(crate) trait Transport: Send + Sync {
    fn storages(&self) -> io::Result<Vec<StorageInfo>>;

    /// The folder's entries, `None` when there is no folder at `dir`.
    fn list(&self, storage: &str, dir: &[String]) -> io::Result<Option<Vec<DirEntry>>>;

    fn read(&self, storage: &str, path: &[String]) -> io::Result<Vec<u8>>;

    /// Creates the file at `path`, which does not exist, from `len` bytes of
    /// `source`. A cancelled upload fails with [`io::ErrorKind::Interrupted`]
    /// and may leave part of the file behind, which the caller removes.
    fn upload(
        &self,
        storage: &str,
        path: &[String],
        source: &mut dyn Read,
        len: u64,
        progress: Progress<'_>,
    ) -> io::Result<()>;

    /// Creates the folder at `path`, whose parent exists.
    fn create_folder(&self, storage: &str, path: &[String]) -> io::Result<()>;

    fn rename(&self, storage: &str, path: &[String], name: &str) -> io::Result<()>;

    /// Deletes a file or an empty folder.
    fn delete(&self, storage: &str, path: &[String]) -> io::Result<()>;

    fn space(&self, storage: &str) -> io::Result<Space>;

    /// Lets go of the device. Never fails: MTP commits each file as it is
    /// written, so there is nothing to flush.
    fn eject(&self);
}

/// Who a device is, as its daemon and the USB device describe it.
pub(crate) struct Identity {
    provisional_id: String,
    model: Option<String>,
    default_name: String,
}

/// A device a daemon reports right now.
pub(crate) struct Found {
    /// Unique among everything the daemon reports, stable while connected.
    key: String,
    identity: Identity,
    transport: Box<dyn Transport>,
}

/// Lists the MTP devices of whichever daemons are running.
pub(crate) struct Watcher {
    gvfs: gvfs::Watcher,
    kmtp: kmtp::Watcher,
    /// Every device still connected, by daemon key, so its cache and lock
    /// last as long as its connection.
    known: HashMap<String, Handle>,
}

impl Watcher {
    pub(crate) fn new() -> Self {
        Self {
            gvfs: gvfs::Watcher::new(),
            kmtp: kmtp::Watcher::new(),
            known: HashMap::new(),
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<crate::Device> {
        let found: Vec<Found> = self
            .gvfs
            .poll()
            .into_iter()
            .chain(self.kmtp.poll())
            .collect();
        self.known
            .retain(|key, _| found.iter().any(|f| &f.key == key));
        let mut devices: Vec<crate::Device> = Vec::new();
        for f in found {
            let handle = self
                .known
                .entry(f.key)
                .or_insert_with(|| Handle(Arc::new(Connection::new(f.identity, f.transport))));
            if handle.0.ejected.load(Ordering::Relaxed) {
                continue;
            }
            match handle.device() {
                Ok(Some(device)) => {
                    if !devices
                        .iter()
                        .any(|d| d.provisional_id == device.provisional_id)
                    {
                        devices.push(device);
                    }
                }
                // A phone shows its storages once it is unlocked.
                Ok(None) => {}
                Err(e) => log::info!("Lost MTP device {}: {e}", handle.0.default_name),
            }
        }
        devices
    }
}

/// A shared connection to one device. Equal when it is the same device.
#[derive(Clone)]
pub(crate) struct Handle(Arc<Connection>);

impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.provisional_id.fmt(f)
    }
}

impl Handle {
    pub(crate) fn storage(&self, storage_id: &str) -> MtpStorage {
        // A storage that has since gone (an SD card taken out) keeps its
        // public id as its key, so every call on it fails.
        let key = self
            .0
            .storage_keys
            .lock()
            .get(storage_id)
            .cloned()
            .unwrap_or_else(|| storage_id.to_string());
        MtpStorage {
            connection: Arc::clone(&self.0),
            key,
        }
    }

    pub(crate) fn eject(&self) {
        self.0.ejected.store(true, Ordering::Relaxed);
        let _guard = self.0.lock.lock();
        self.0.transport.eject();
    }

    /// The device as it is now, or `None` while it has no storage Luminous
    /// can write to (a phone that is locked or not set to file transfer).
    fn device(&self) -> io::Result<Option<crate::Device>> {
        let storages = self.0.storages()?;
        if storages.is_empty() {
            return Ok(None);
        }
        Ok(Some(crate::Device {
            kind: DeviceKind::Mtp,
            provisional_id: self.0.provisional_id.clone(),
            model: self.0.model.clone(),
            default_name: self.0.default_name.clone(),
            storages,
            access: crate::Access::Mtp(self.clone()),
            // Neither daemon says which formats a device plays.
            reported_formats: Vec::new(),
            cache: Arc::clone(&self.0.cache),
        }))
    }
}

pub(crate) struct Connection {
    transport: Box<dyn Transport>,
    provisional_id: String,
    model: Option<String>,
    default_name: String,
    /// One operation at a time, as MTP handles them anyway.
    lock: Mutex<()>,
    ejected: AtomicBool,
    /// Exact folder paths already found, by storage and lower-case path.
    folders: Mutex<HashMap<(String, String), Vec<String>>>,
    /// Transport storage keys by public storage id (see [`Storage::id`]).
    storage_keys: Mutex<HashMap<String, String>>,
    /// What `storages` last read, reused while a transfer holds `lock`.
    last_storages: Mutex<Vec<Storage>>,
    /// Whether renaming works on this device: unknown until the first write
    /// tries it. When it does not, writes go straight to the final name.
    can_rename: Mutex<Option<bool>>,
    cache: Arc<DeviceCache>,
}

impl Connection {
    fn new(identity: Identity, transport: Box<dyn Transport>) -> Self {
        Self {
            transport,
            provisional_id: identity.provisional_id,
            model: identity.model,
            default_name: identity.default_name,
            lock: Mutex::new(()),
            ejected: AtomicBool::new(false),
            folders: Mutex::new(HashMap::new()),
            storage_keys: Mutex::new(HashMap::new()),
            last_storages: Mutex::new(Vec::new()),
            can_rename: Mutex::new(None),
            cache: Arc::default(),
        }
    }

    /// The device's writable storages with their current free space. While
    /// a transfer holds the device, returns what the last call read, so
    /// polling never waits for a copy to finish.
    fn storages(&self) -> io::Result<Vec<Storage>> {
        let Some(_guard) = self.lock.try_lock() else {
            return Ok(self.last_storages.lock().clone());
        };
        let mut storages = Vec::new();
        let mut keys = HashMap::new();
        for info in self.transport.storages()? {
            if !info.writable {
                continue;
            }
            // Neither daemon exposes MTP's persistent storage id; the name
            // a storage has is the same every time the device connects.
            let id = format!("{}/{}", self.provisional_id, info.name);
            keys.insert(id.clone(), info.key);
            storages.push(Storage {
                id,
                label: Some(info.name),
                mount_path: None,
                filesystem: "MTP".to_string(),
                total_bytes: info.total_bytes,
                free_bytes: info.free_bytes,
            });
        }
        *self.storage_keys.lock() = keys;
        *self.last_storages.lock() = storages.clone();
        Ok(storages)
    }

    /// The entry called `name` in `dir`: an exact match, else one that
    /// differs only in case. Caller holds the lock.
    fn child(&self, storage: &str, dir: &[String], name: &str) -> io::Result<Option<DirEntry>> {
        let entries = self.transport.list(storage, dir)?.unwrap_or_default();
        let exact = entries.iter().position(|e| e.name == name);
        let index = exact.or_else(|| {
            entries
                .iter()
                .position(|e| e.name.eq_ignore_ascii_case(name))
        });
        Ok(index.map(|i| entries.into_iter().nth(i).expect("index from position")))
    }

    /// The exact path of the folder at `parts`, creating missing folders
    /// when `create`. `None` when it does not exist (or is a file). Caller
    /// holds the lock.
    fn folder(
        &self,
        storage: &str,
        parts: &[&str],
        create: bool,
    ) -> io::Result<Option<Vec<String>>> {
        let mut current: Vec<String> = Vec::new();
        let mut lower = String::new();
        for part in parts {
            if !lower.is_empty() {
                lower.push('/');
            }
            lower.push_str(&part.to_lowercase());
            let key = (storage.to_string(), lower.clone());
            if let Some(path) = self.folders.lock().get(&key) {
                current = path.clone();
                continue;
            }
            match self.child(storage, &current, part)? {
                Some(entry) if entry.is_dir => current.push(entry.name),
                Some(_) => return Ok(None),
                None if create => {
                    current.push(part.to_string());
                    self.transport.create_folder(storage, &current)?;
                }
                None => return Ok(None),
            }
            self.folders.lock().insert(key, current.clone());
        }
        Ok(Some(current))
    }

    /// The exact path of the entry at `path`, and the entry. Caller holds
    /// the lock.
    fn find(&self, storage: &str, path: &str) -> io::Result<Option<(Vec<String>, DirEntry)>> {
        let parts = components(path);
        let Some((name, parents)) = parts.split_last() else {
            return Ok(None);
        };
        let Some(parent) = self.folder(storage, parents, false)? else {
            return Ok(None);
        };
        Ok(self.child(storage, &parent, name)?.map(|entry| {
            let mut full = parent;
            full.push(entry.name.clone());
            (full, entry)
        }))
    }

    /// Deletes a file, or a folder after everything in it. Caller holds the
    /// lock.
    fn delete_tree(&self, storage: &str, path: &[String], is_dir: bool) -> io::Result<()> {
        if is_dir {
            for entry in self.transport.list(storage, path)?.unwrap_or_default() {
                let mut child = path.to_vec();
                child.push(entry.name);
                self.delete_tree(storage, &child, entry.is_dir)?;
            }
        }
        self.transport.delete(storage, path)
    }

    /// Whether files in `folder` can be renamed. Learned once per
    /// connection by renaming a one-byte probe file, so a device that cannot
    /// rename is found out before any real file is replaced. Caller holds
    /// the lock.
    fn can_rename(&self, storage: &str, folder: &[String]) -> bool {
        if let Some(known) = *self.can_rename.lock() {
            return known;
        }
        let probe = format!(".luminous-rename-probe{PARTIAL_SUFFIX}");
        let renamed_to = format!("{probe}2");
        let at = |name: &str| {
            let mut path = folder.to_vec();
            path.push(name.to_string());
            path
        };
        let works = self
            .transport
            .upload(storage, &at(&probe), &mut &b"L"[..], 1, &mut |_| {
                std::ops::ControlFlow::Continue(())
            })
            .and_then(|()| self.transport.rename(storage, &at(&probe), &renamed_to));
        for name in [&renamed_to, &probe] {
            if let Ok(Some(_)) = self.child(storage, folder, name) {
                let _ = self.transport.delete(storage, &at(name));
            }
        }
        if let Err(e) = &works {
            log::warn!(
                "{} cannot rename files ({e}); writing in place instead",
                self.default_name
            );
        }
        *self.can_rename.lock() = Some(works.is_ok());
        works.is_ok()
    }
}

/// [`DeviceBackend`] for one storage of an MTP device.
pub(crate) struct MtpStorage {
    connection: Arc<Connection>,
    /// The transport's key for the storage.
    key: String,
}

impl MtpStorage {
    /// Runs `op` with the device locked. A failure forgets cached folders,
    /// in case one went stale (removed on the device itself).
    fn locked<T>(&self, op: impl FnOnce(&Connection) -> io::Result<T>) -> io::Result<T> {
        let connection = &*self.connection;
        if connection.ejected.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "device ejected",
            ));
        }
        let _guard = connection.lock.lock();
        op(connection).inspect_err(|_| connection.folders.lock().clear())
    }
}

impl DeviceBackend for MtpStorage {
    fn list(&self, dir: &str) -> io::Result<Vec<DirEntry>> {
        self.locked(|c| match c.folder(&self.key, &components(dir), false)? {
            Some(folder) => Ok(c.transport.list(&self.key, &folder)?.unwrap_or_default()),
            None => Ok(Vec::new()),
        })
    }

    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        self.locked(|c| match c.find(&self.key, path)? {
            Some((full, entry)) if !entry.is_dir => c.transport.read(&self.key, &full).map(Some),
            _ => Ok(None),
        })
    }

    fn write(
        &self,
        path: &str,
        source: &mut dyn Read,
        len: u64,
        progress: Progress<'_>,
    ) -> io::Result<()> {
        self.locked(|c| {
            let storage = &self.key;
            let parts = components(path);
            let Some((name, parents)) = parts.split_last() else {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "no file name"));
            };
            let parent = c
                .folder(storage, parents, true)?
                .ok_or_else(|| io::Error::other("a file is in the way of the folder"))?;
            let at = |name: &str| {
                let mut path = parent.clone();
                path.push(name.to_string());
                path
            };
            let remove_existing = |name: &str| -> io::Result<()> {
                match c.child(storage, &parent, name)? {
                    Some(old) => c.delete_tree(storage, &at(&old.name), old.is_dir),
                    None => Ok(()),
                }
            };

            if !c.can_rename(storage, &parent) {
                remove_existing(name)?;
                return c
                    .transport
                    .upload(storage, &at(name), source, len, progress)
                    .inspect_err(|_| {
                        let _ = c.transport.delete(storage, &at(name));
                    });
            }

            let partial = format!("{name}{PARTIAL_SUFFIX}");
            remove_existing(&partial)?;
            c.transport
                .upload(storage, &at(&partial), source, len, progress)
                .and_then(|()| remove_existing(name))
                .and_then(|()| c.transport.rename(storage, &at(&partial), name))
                .inspect_err(|_| {
                    let _ = c.transport.delete(storage, &at(&partial));
                })
        })
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        if components(path).is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot remove the root",
            ));
        }
        self.locked(|c| {
            if let Some((full, entry)) = c.find(&self.key, path)? {
                c.delete_tree(&self.key, &full, entry.is_dir)?;
            }
            // The folder may have been cached, along with everything in it.
            c.folders.lock().clear();
            Ok(())
        })
    }

    fn remove_empty_folder(&self, path: &str) -> io::Result<bool> {
        if components(path).is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot remove the root",
            ));
        }
        self.locked(|c| {
            let Some((full, entry)) = c.find(&self.key, path)? else {
                return Ok(false);
            };
            // Neither daemon can delete only-if-empty, and MTP devices may
            // delete a folder with its contents. Checked under the device
            // lock, so nothing Luminous writes lands in between; only a
            // file added on the device itself in that moment could be lost.
            if !entry.is_dir
                || !c
                    .transport
                    .list(&self.key, &full)?
                    .unwrap_or_default()
                    .is_empty()
            {
                return Ok(false);
            }
            c.transport.delete(&self.key, &full)?;
            c.folders.lock().clear();
            Ok(true)
        })
    }

    fn space(&self) -> io::Result<Space> {
        self.locked(|c| c.transport.space(&self.key))
    }
}

/// What the kernel says about a USB device: `/sys/bus/usb/devices/<port>`.
#[derive(Debug, Default, PartialEq, Eq)]
struct UsbInfo {
    serial: Option<String>,
    manufacturer: Option<String>,
    product: Option<String>,
}

impl UsbInfo {
    fn read(sysfs_dir: &Path) -> Self {
        let attribute = |name: &str| {
            std::fs::read_to_string(sysfs_dir.join(name))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        Self {
            serial: attribute("serial"),
            manufacturer: attribute("manufacturer"),
            product: attribute("product"),
        }
    }

    /// The device's identity: `name` is what the daemon calls it, if
    /// anything, and `fallback` stands in for a missing serial.
    fn identity(&self, name: Option<String>, fallback: &str) -> Identity {
        let model = naming::model_name(self.manufacturer.as_deref(), self.product.as_deref());
        Identity {
            provisional_id: naming::mtp_provisional_id(self.serial.as_deref(), fallback),
            default_name: name
                .filter(|n| !n.trim().is_empty())
                .or_else(|| model.clone())
                .unwrap_or_else(|| "Portable device".to_string()),
            model,
        }
    }
}

/// A local copy of an upload's data, for a daemon that sends a file rather
/// than a stream. Lives in the user cache folder, which a Flatpak shares
/// with the host's daemons at the same path, and is removed when dropped.
struct Spool {
    path: PathBuf,
    file: std::fs::File,
}

impl Spool {
    /// An empty spool file, opened for reading and writing.
    fn new() -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .unwrap_or_else(std::env::temp_dir)
            .join("luminous-mtp");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!(
            "{}-{}{PARTIAL_SUFFIX}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self { path, file })
    }

    /// Copies exactly `len` bytes of `source` into a new spool. Reports no
    /// bytes as written (nothing is on the device yet) but lets `progress`
    /// cancel between chunks.
    fn fill(source: &mut dyn Read, len: u64, progress: Progress<'_>) -> io::Result<Self> {
        let mut spool = Self::new()?;
        let mut buffer = vec![0u8; 256 * 1024];
        let mut copied = 0u64;
        loop {
            let n = match source.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            spool.file.write_all(&buffer[..n])?;
            copied += n as u64;
            if progress(0).is_break() {
                return Err(cancelled());
            }
        }
        if copied != len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("expected {len} bytes, read {copied}"),
            ));
        }
        spool.file.flush()?;
        spool.file.rewind()?;
        Ok(spool)
    }
}

impl Drop for Spool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::ops::ControlFlow;

    /// A device in memory: files and folders by exact path.
    #[derive(Default)]
    struct FakeDevice {
        /// `None` for a folder.
        entries: Mutex<BTreeMap<Vec<String>, Option<Vec<u8>>>>,
        can_rename: bool,
    }

    fn missing() -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, "missing")
    }

    impl Transport for Arc<FakeDevice> {
        fn storages(&self) -> io::Result<Vec<StorageInfo>> {
            Ok(vec![
                StorageInfo {
                    key: "s1".into(),
                    name: "Internal shared storage".into(),
                    total_bytes: 100,
                    free_bytes: 40,
                    writable: true,
                },
                StorageInfo {
                    key: "s2".into(),
                    name: "Camera roll".into(),
                    total_bytes: 10,
                    free_bytes: 0,
                    writable: false,
                },
            ])
        }

        fn list(&self, _: &str, dir: &[String]) -> io::Result<Option<Vec<DirEntry>>> {
            let entries = self.entries.lock();
            if !dir.is_empty() && entries.get(dir) != Some(&None) {
                return Ok(None);
            }
            Ok(Some(
                entries
                    .iter()
                    .filter(|(p, _)| p.len() == dir.len() + 1 && p.starts_with(dir))
                    .map(|(p, data)| DirEntry {
                        name: p.last().unwrap().clone(),
                        is_dir: data.is_none(),
                        size: data.as_ref().map_or(0, |d| d.len() as u64),
                        modified: None,
                    })
                    .collect(),
            ))
        }

        fn read(&self, _: &str, path: &[String]) -> io::Result<Vec<u8>> {
            self.entries
                .lock()
                .get(path)
                .cloned()
                .flatten()
                .ok_or_else(missing)
        }

        fn upload(
            &self,
            _: &str,
            path: &[String],
            source: &mut dyn Read,
            _len: u64,
            progress: Progress<'_>,
        ) -> io::Result<()> {
            assert!(
                !self.entries.lock().contains_key(path),
                "uploads never replace"
            );
            let mut data = Vec::new();
            source.read_to_end(&mut data)?;
            // Part of the file lands before a cancel, as on a real device.
            self.entries
                .lock()
                .insert(path.to_vec(), Some(data.clone()));
            if progress(data.len() as u64).is_break() {
                return Err(cancelled());
            }
            Ok(())
        }

        fn create_folder(&self, _: &str, path: &[String]) -> io::Result<()> {
            self.entries.lock().insert(path.to_vec(), None);
            Ok(())
        }

        fn rename(&self, _: &str, path: &[String], name: &str) -> io::Result<()> {
            if !self.can_rename {
                return Err(io::Error::other("rename not supported"));
            }
            let mut entries = self.entries.lock();
            let data = entries.remove(path).ok_or_else(missing)?;
            let mut to = path.to_vec();
            *to.last_mut().unwrap() = name.to_string();
            entries.insert(to, data);
            Ok(())
        }

        fn delete(&self, _: &str, path: &[String]) -> io::Result<()> {
            let mut entries = self.entries.lock();
            assert!(
                !entries
                    .keys()
                    .any(|p| p.len() > path.len() && p.starts_with(path)),
                "folders are deleted only once empty"
            );
            entries.remove(path).map(drop).ok_or_else(missing)
        }

        fn space(&self, _: &str) -> io::Result<Space> {
            Ok(Space {
                total_bytes: 100,
                free_bytes: 40,
            })
        }

        fn eject(&self) {}
    }

    fn device(can_rename: bool) -> (Arc<FakeDevice>, Handle) {
        let fake = Arc::new(FakeDevice {
            can_rename,
            ..Default::default()
        });
        let identity = UsbInfo {
            serial: Some("abc123".into()),
            ..Default::default()
        }
        .identity(Some("Pixel 7".into()), "x");
        let handle = Handle(Arc::new(Connection::new(
            identity,
            Box::new(Arc::clone(&fake)),
        )));
        (fake, handle)
    }

    fn write(storage: &MtpStorage, path: &str, text: &str) -> io::Result<()> {
        storage.write(path, &mut text.as_bytes(), text.len() as u64, &mut |_| {
            ControlFlow::Continue(())
        })
    }

    fn paths(fake: &FakeDevice) -> Vec<String> {
        fake.entries.lock().keys().map(|p| p.join("/")).collect()
    }

    #[test]
    fn lists_only_writable_storages_with_stable_ids() {
        let (_, handle) = device(true);
        let device = handle.device().unwrap().unwrap();
        assert_eq!(device.kind, DeviceKind::Mtp);
        assert_eq!(device.provisional_id, "mtp-ABC123");
        assert_eq!(device.default_name, "Pixel 7");
        assert_eq!(device.storages.len(), 1);
        assert_eq!(device.storages[0].id, "mtp-ABC123/Internal shared storage");
        assert_eq!(device.storages[0].free_bytes, 40);
        assert!(device
            .storage("mtp-ABC123/Internal shared storage")
            .is_some());
        assert!(device.storage("mtp-ABC123/Camera roll").is_none());
    }

    #[test]
    fn writes_through_a_partial_name_and_replaces() {
        let (fake, handle) = device(true);
        let storage = handle.storage("s1");
        write(&storage, "Music/Artist/a.flac", "first").unwrap();
        // Folders match whatever their case; the file takes the new name.
        write(&storage, "music/artist/A.flac", "second").unwrap();
        assert_eq!(
            paths(&fake),
            vec!["Music", "Music/Artist", "Music/Artist/A.flac"]
        );
        assert_eq!(
            storage.read("MUSIC/Artist/a.flac").unwrap().as_deref(),
            Some(&b"second"[..])
        );
    }

    #[test]
    fn cancelled_write_leaves_nothing_behind() {
        let (fake, handle) = device(true);
        let storage = handle.storage("s1");
        write(&storage, "a.mp3", "old").unwrap();
        let err = storage
            .write("a.mp3", &mut &b"new"[..], 3, &mut |n| {
                if n > 0 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert_eq!(paths(&fake), vec!["a.mp3"]);
        assert_eq!(storage.read("a.mp3").unwrap().as_deref(), Some(&b"old"[..]));
    }

    #[test]
    fn a_device_that_cannot_rename_is_written_in_place() {
        let (fake, handle) = device(false);
        let storage = handle.storage("s1");
        write(&storage, "Music/a.mp3", "first").unwrap();
        write(&storage, "Music/a.mp3", "second").unwrap();
        assert_eq!(paths(&fake), vec!["Music", "Music/a.mp3"]);
        assert_eq!(
            storage.read("Music/a.mp3").unwrap().as_deref(),
            Some(&b"second"[..])
        );
    }

    #[test]
    fn remove_takes_folders_with_their_contents() {
        let (fake, handle) = device(true);
        let storage = handle.storage("s1");
        write(&storage, "Music/Album/a.mp3", "a").unwrap();
        write(&storage, "Music/b.mp3", "b").unwrap();
        storage.remove("music/album").unwrap();
        storage.remove("Music/gone.mp3").unwrap();
        assert_eq!(paths(&fake), vec!["Music", "Music/b.mp3"]);
        assert!(storage.remove("").is_err());
    }

    #[test]
    fn only_empty_folders_are_removed_as_empty() {
        let (fake, handle) = device(true);
        let storage = handle.storage("s1");
        write(&storage, "Music/Kept/mine.txt", "x").unwrap();
        fake.entries
            .lock()
            .insert(vec!["Music".into(), "Empty".into()], None);
        assert!(!storage.remove_empty_folder("Music/Kept").unwrap());
        assert!(!storage.remove_empty_folder("Music/Kept/mine.txt").unwrap());
        assert!(!storage.remove_empty_folder("Music/Gone").unwrap());
        assert!(storage.remove_empty_folder("Music/Empty").unwrap());
        assert_eq!(
            paths(&fake),
            vec!["Music", "Music/Kept", "Music/Kept/mine.txt"]
        );
    }

    #[test]
    fn missing_things_are_not_errors() {
        let (_, handle) = device(true);
        let storage = handle.storage("s1");
        assert!(storage.list("Nope").unwrap().is_empty());
        assert_eq!(storage.read("Nope/a.mp3").unwrap(), None);
        assert_eq!(storage.read("").unwrap(), None);
    }

    #[test]
    fn ejected_devices_refuse_calls() {
        let (_, handle) = device(true);
        let storage = handle.storage("s1");
        handle.eject();
        assert_eq!(
            storage.list("").unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
    }

    #[test]
    fn identity_reads_the_usb_device() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("serial"), "0123456789ABCDEF\n").unwrap();
        std::fs::write(dir.path().join("manufacturer"), "Google\n").unwrap();
        std::fs::write(dir.path().join("product"), "Pixel 7\n").unwrap();
        let usb = UsbInfo::read(dir.path());
        let id = usb.identity(None, "fallback");
        assert_eq!(id.provisional_id, "mtp-0123456789ABCDEF");
        assert_eq!(id.model.as_deref(), Some("Google Pixel 7"));
        assert_eq!(id.default_name, "Google Pixel 7");

        let none = UsbInfo::default().identity(Some(" ".into()), "Google_Pixel_7");
        assert_eq!(none.provisional_id, "mtp-GOOGLE_PIXEL_7");
        assert_eq!(none.default_name, "Portable device");
    }

    #[test]
    fn spool_copies_exactly_and_cleans_up() {
        let data = vec![9u8; 300 * 1024];
        let mut spool = Spool::fill(&mut data.as_slice(), data.len() as u64, &mut |n| {
            assert_eq!(n, 0);
            ControlFlow::Continue(())
        })
        .unwrap();
        let mut back = Vec::new();
        spool.file.read_to_end(&mut back).unwrap();
        assert_eq!(back, data);
        let path = spool.path.clone();
        drop(spool);
        assert!(!path.exists());

        let short = Spool::fill(&mut &b"abc"[..], 5, &mut |_| ControlFlow::Continue(()));
        assert_eq!(short.err().unwrap().kind(), io::ErrorKind::UnexpectedEof);
    }

    /// Prints what each daemon reports. Run with
    /// `cargo test -p luminous-devices -- --ignored --nocapture mtp_poll_report`.
    #[test]
    #[ignore = "reads the desktop's MTP daemons"]
    fn mtp_poll_report() {
        let mut watcher = Watcher::new();
        // The first poll may only start mounting a gvfs volume.
        for _ in 0..3 {
            watcher.poll();
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        for device in watcher.poll() {
            println!(
                "{} ({:?}) id {}",
                device.default_name, device.model, device.provisional_id
            );
            for s in &device.storages {
                println!(
                    "  {} {:?}: {} of {} bytes free",
                    s.id, s.label, s.free_bytes, s.total_bytes
                );
                let backend = device.storage(&s.id).unwrap();
                for entry in backend.list("").unwrap() {
                    println!("    {}{}", entry.name, if entry.is_dir { "/" } else { "" });
                }
            }
        }
    }

    /// Exercises a real device through whichever daemon owns it. Set
    /// `LUMINOUS_MTP_TEST_DEVICE` to part of its name or id (e.g. `pixel`) and
    /// run `cargo test -p luminous-devices -- --ignored mtp_backend`. Works
    /// only inside a `luminous-mtp-test` folder it removes afterwards.
    #[test]
    #[ignore = "needs a connected MTP device; see the doc comment"]
    fn mtp_backend_round_trip() {
        let needle = std::env::var("LUMINOUS_MTP_TEST_DEVICE")
            .expect("set LUMINOUS_MTP_TEST_DEVICE")
            .to_lowercase();
        let mut watcher = Watcher::new();
        let mut device = None;
        for _ in 0..10 {
            device = watcher.poll().into_iter().find(|d| {
                d.default_name.to_lowercase().contains(&needle)
                    || d.provisional_id.to_lowercase().contains(&needle)
            });
            if device.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        let device = device.expect("no MTP device matches LUMINOUS_MTP_TEST_DEVICE");
        println!("{device:#?}");
        let storage = device.storage(&device.storages[0].id).unwrap();
        let space = storage.space().unwrap();
        assert!(space.total_bytes >= space.free_bytes && space.total_bytes > 0);

        let root = "luminous-mtp-test";
        storage.remove(root).unwrap();
        let data: Vec<u8> = (0..200_000u32).map(|i| i as u8).collect();
        let write = |path: &str, bytes: &[u8]| {
            storage.write(path, &mut &bytes[..], bytes.len() as u64, &mut |_| {
                ControlFlow::Continue(())
            })
        };
        write("luminous-mtp-test/Artist/Album/01 Track.mp3", &data).unwrap();
        write("luminous-mtp-test/Artist/Album/01 Track.mp3", b"replaced").unwrap();
        assert_eq!(
            storage
                .read("luminous-mtp-test/artist/ALBUM/01 Track.mp3")
                .unwrap()
                .as_deref(),
            Some(&b"replaced"[..])
        );
        let listing = storage.list("luminous-mtp-test/Artist/Album").unwrap();
        assert_eq!(listing.len(), 1, "{listing:?}");
        assert_eq!(listing[0].name, "01 Track.mp3");
        assert_eq!(listing[0].size, 8);

        let err = storage
            .write(
                "luminous-mtp-test/cancelled.mp3",
                &mut &data[..],
                data.len() as u64,
                &mut |n| {
                    if n > 0 {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                },
            )
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        let names: Vec<_> = storage
            .list(root)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, vec!["Artist"]);

        assert!(!storage
            .remove_empty_folder("luminous-mtp-test/Artist")
            .unwrap());
        assert_eq!(storage.read("luminous-mtp-test/missing.mp3").unwrap(), None);
        storage.remove(root).unwrap();
        storage.remove(root).unwrap();
        assert!(storage.list(root).unwrap().is_empty());
    }
}
