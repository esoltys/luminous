//! MTP players and phones, on any platform that can reach them (#1893):
//! Windows Portable Devices on Windows (`windows_mtp`), the desktop's gvfs
//! or kio-mtp daemon on Linux (`linux_mtp`).
//!
//! Each platform serves a device as a [`Transport`]: path-addressed,
//! exact-name operations on one device. Everything MTP needs on top of that
//! (case-insensitive names, writes under a partial name, devices that cannot
//! rename, removing a folder with its contents) is here, once, for all of
//! them.

use crate::backend::{components, PARTIAL_SUFFIX};
use crate::{DeviceBackend, DeviceCache, DeviceKind, DirEntry, Progress, Space, Storage};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One storage as a transport reports it.
pub(crate) struct StorageInfo {
    /// What the transport calls the storage in every other call.
    pub(crate) key: String,
    /// The storage's public id (see [`Storage::id`]) when the device keeps
    /// one across reconnects; otherwise one is made from `name`.
    pub(crate) id: Option<String>,
    /// What the device calls it ("Internal shared storage", "SD card"),
    /// empty when it does not say.
    pub(crate) name: String,
    pub(crate) filesystem: String,
    pub(crate) total_bytes: u64,
    pub(crate) free_bytes: u64,
    pub(crate) writable: bool,
}

/// One MTP device as its platform serves it. Paths are the exact names of
/// each folder below a storage's root, never empty for a file. Calls may
/// come from any thread, one at a time.
pub(crate) trait Transport: Send + Sync {
    fn storages(&self) -> io::Result<Vec<StorageInfo>>;

    /// The folder's entries, `None` when there is no folder at `dir`.
    fn list(&self, storage: &str, dir: &[String]) -> io::Result<Option<Vec<DirEntry>>>;

    fn read(&self, storage: &str, path: &[String]) -> io::Result<Vec<u8>>;

    /// Creates the file at `path`, which does not exist, from `len` bytes of
    /// `source`. A name ending in [`PARTIAL_SUFFIX`] is a file still being
    /// written, renamed to the name without it once complete: a transport
    /// that types files by their name types it by that final name. A
    /// cancelled upload fails with [`io::ErrorKind::Interrupted`] and may
    /// leave part of the file behind, which the caller removes.
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

    /// Deletes a file or an empty folder. Must not delete a folder with
    /// anything in it, where the platform can refuse that itself.
    fn delete(&self, storage: &str, path: &[String]) -> io::Result<()>;

    fn space(&self, storage: &str) -> io::Result<Space>;

    /// Lets go of the device. Never fails: MTP commits each file as it is
    /// written, so there is nothing to flush.
    fn eject(&self);
}

/// Who a device is, as its platform describes it.
pub(crate) struct Identity {
    pub(crate) provisional_id: String,
    pub(crate) model: Option<String>,
    pub(crate) default_name: String,
    /// Lower-case names of the audio formats the device says it plays.
    pub(crate) formats: Vec<String>,
}

/// A shared connection to one device. Equal when it is the same connection.
#[derive(Clone)]
pub(crate) struct Handle(Arc<Connection>);

impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.identity.provisional_id.fmt(f)
    }
}

impl Handle {
    pub(crate) fn new(identity: Identity, transport: Box<dyn Transport>) -> Self {
        Self(Arc::new(Connection {
            transport,
            identity,
            lock: Mutex::new(()),
            ejected: AtomicBool::new(false),
            folders: Mutex::new(HashMap::new()),
            storage_keys: Mutex::new(HashMap::new()),
            last_storages: Mutex::new(Vec::new()),
            can_rename: Mutex::new(None),
            cache: Arc::default(),
        }))
    }

    /// What the device is called, for logs.
    pub(crate) fn name(&self) -> &str {
        &self.0.identity.default_name
    }

    pub(crate) fn is_ejected(&self) -> bool {
        self.0.ejected.load(Ordering::Relaxed)
    }

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

    /// The device as it is now, with fresh storage capacities, or `None`
    /// while it has no storage Luminous can write to (a read-only camera
    /// roll, or a phone that is locked or not set to file transfer).
    pub(crate) fn device(&self) -> io::Result<Option<crate::Device>> {
        let storages = self.0.storages()?;
        if storages.is_empty() {
            return Ok(None);
        }
        let identity = &self.0.identity;
        Ok(Some(crate::Device {
            kind: DeviceKind::Mtp,
            provisional_id: identity.provisional_id.clone(),
            model: identity.model.clone(),
            default_name: identity.default_name.clone(),
            storages,
            access: crate::Access::Mtp(self.clone()),
            reported_formats: identity.formats.clone(),
            cache: Arc::clone(&self.0.cache),
        }))
    }
}

struct Connection {
    transport: Box<dyn Transport>,
    identity: Identity,
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
    /// Shared with every `Device` built from this connection, so the marker
    /// and default profile are read once per connection.
    cache: Arc<DeviceCache>,
}

impl Connection {
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
            // Without a persistent id from the device, the name a storage
            // has is the same every time the device connects.
            let id = info
                .id
                .unwrap_or_else(|| format!("{}/{}", self.identity.provisional_id, info.name));
            keys.insert(id.clone(), info.key);
            storages.push(Storage {
                id,
                label: Some(info.name).filter(|n| !n.is_empty()),
                mount_path: None,
                filesystem: info.filesystem,
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
                self.identity.default_name
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
            // Checked under the device lock, so nothing Luminous writes lands
            // in between. Where the platform can, `delete` itself refuses a
            // folder that is not empty (WPD); the Linux daemons cannot, so a
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::cancelled;
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
                    id: None,
                    name: "Internal shared storage".into(),
                    filesystem: "MTP".into(),
                    total_bytes: 100,
                    free_bytes: 40,
                    writable: true,
                },
                StorageInfo {
                    key: "s2".into(),
                    id: None,
                    name: "Camera roll".into(),
                    filesystem: "MTP".into(),
                    total_bytes: 10,
                    free_bytes: 0,
                    writable: false,
                },
                StorageInfo {
                    key: "s3".into(),
                    id: Some("{persistent-sd}".into()),
                    name: String::new(),
                    filesystem: "FAT32".into(),
                    total_bytes: 50,
                    free_bytes: 5,
                    writable: true,
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
        let identity = Identity {
            provisional_id: "mtp-ABC123".into(),
            model: None,
            default_name: "Pixel 7".into(),
            formats: vec!["mp3".into()],
        };
        let handle = Handle::new(identity, Box::new(Arc::clone(&fake)));
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
        assert_eq!(device.reported_formats, vec!["mp3"]);
        assert_eq!(device.storages.len(), 2);
        assert_eq!(device.storages[0].id, "mtp-ABC123/Internal shared storage");
        assert_eq!(
            device.storages[0].label.as_deref(),
            Some("Internal shared storage")
        );
        assert_eq!(device.storages[0].filesystem, "MTP");
        assert_eq!(device.storages[0].free_bytes, 40);
        // A persistent id the device keeps is the id as it is.
        assert_eq!(device.storages[1].id, "{persistent-sd}");
        assert_eq!(device.storages[1].label, None);
        assert_eq!(device.storages[1].filesystem, "FAT32");
        assert!(device
            .storage("mtp-ABC123/Internal shared storage")
            .is_some());
        assert!(device.storage("{persistent-sd}").is_some());
        assert!(device.storage("mtp-ABC123/Camera roll").is_none());
    }

    #[test]
    fn handles_are_equal_only_for_the_same_connection() {
        let (_, a) = device(true);
        let (_, b) = device(true);
        assert_eq!(a, a.clone());
        assert_ne!(a, b);
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
        assert!(handle.is_ejected());
        assert_eq!(
            storage.list("").unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
    }
}
