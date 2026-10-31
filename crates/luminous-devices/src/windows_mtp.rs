//! Windows: MTP players and phones through Windows Portable Devices (#840).
//!
//! WPD also lists every mounted volume (the `WUDFWpdFs` driver), which the
//! drive-letter watcher already reports, and cameras. A device becomes a
//! device here only if it speaks MTP and has a storage Luminous can write
//! to: an iPhone reports MTP too, but its one storage is a read-only camera
//! roll, so it is left out.
//!
//! Threading: every WPD object is created through `PortableDeviceFTM` (or
//! is reached from one that was), so it is free-threaded and any thread that
//! has joined COM may call it. [`com`] joins the calling thread to the
//! multithreaded apartment and runs at every entry point; a per-device lock
//! serializes calls, since MTP handles one operation at a time anyway.

use crate::backend::{cancelled, components, PARTIAL_SUFFIX};
use crate::{naming, DeviceBackend, DeviceCache, DeviceKind, DirEntry, Progress, Space, Storage};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use windows::core::{Interface, GUID, PCWSTR, PWSTR};
use windows::Win32::Devices::PortableDevices::*;
use windows::Win32::Foundation::{PROPERTYKEY, RPC_E_CHANGED_MODE, S_OK};
use windows::Win32::System::Com::StructuredStorage::{
    PropVariantClear, PropVariantToGUID, PropVariantToStringAlloc, PROPVARIANT,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, CoTaskMemFree, CoUninitialize, IStream,
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, STGC_DEFAULT, STGM_READ,
};
use windows::Win32::System::Variant::{VT_DATE, VT_LPWSTR};

/// `SECURITY_IMPERSONATION << 16`, what WPD clients pass as their quality
/// of service.
const SECURITY_IMPERSONATION_QOS: u32 = 2 << 16;

struct ComGuard(bool);

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

thread_local! {
    static COM: ComGuard = {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        // A thread already in a single-threaded apartment can still call the
        // free-threaded WPD objects; it just must not be uninitialized here.
        if hr != RPC_E_CHANGED_MODE && hr.is_err() {
            log::warn!("COM initialization failed: {hr:?}");
        }
        ComGuard(hr.is_ok())
    };
}

fn com() {
    COM.with(|_| {});
}

/// Polls WPD for MTP devices, keeping a connection open to each one.
pub(crate) struct Watcher {
    manager: Option<IPortableDeviceManager>,
    /// Every WPD device seen and still present, by PnP id: its connection,
    /// or `None` when it turned out not to be an MTP device (checked once).
    known: HashMap<String, Option<Handle>>,
}

impl Watcher {
    pub(crate) fn new() -> Self {
        Self {
            manager: None,
            known: HashMap::new(),
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<crate::Device> {
        com();
        let ids = match self.device_ids() {
            Ok(ids) => ids,
            Err(e) => {
                if self.manager.take().is_some() {
                    log::warn!("Listing portable devices failed: {e}");
                }
                return Vec::new();
            }
        };
        self.known.retain(|id, _| ids.contains(id));

        let mut devices = Vec::new();
        for id in ids.iter().filter(|id| !is_volume_shim(id)) {
            let entry = self.known.entry(id.clone()).or_insert_with(|| {
                Connection::open(id, true)
                    .inspect_err(|e| log::info!("Not using portable device {id}: {e}"))
                    .ok()
                    .map(|c| Handle(Arc::new(c)))
            });
            let Some(handle) = entry else { continue };
            if handle.0.ejected.load(Ordering::Relaxed) {
                continue;
            }
            match handle.device() {
                Ok(Some(device)) => devices.push(device),
                // Kept open: a phone shows its storages once it is unlocked.
                Ok(None) => {}
                Err(e) => {
                    // Probably unplugged mid-poll; reopen if it is still there.
                    log::info!("Lost portable device {id}: {e}");
                    self.known.remove(id);
                }
            }
        }
        devices
    }

    fn device_ids(&mut self) -> windows::core::Result<Vec<String>> {
        if self.manager.is_none() {
            self.manager = Some(unsafe {
                CoCreateInstance(&PortableDeviceManager, None, CLSCTX_INPROC_SERVER)
            }?);
        }
        let manager = self.manager.as_ref().expect("created above");
        unsafe {
            manager.RefreshDeviceList()?;
            let mut count = 0u32;
            manager.GetDevices(std::ptr::null_mut(), &mut count)?;
            let mut ids = vec![PWSTR::null(); count as usize];
            manager.GetDevices(ids.as_mut_ptr(), &mut count)?;
            Ok(ids
                .into_iter()
                .take(count as usize)
                .filter_map(|id| take_string(id))
                .collect())
        }
    }
}

/// WPD's view of a mounted volume (`\\?\swd#wpdbusenum#...`), which the
/// drive-letter watcher already reports.
fn is_volume_shim(pnp_id: &str) -> bool {
    pnp_id.to_ascii_lowercase().contains("#wpdbusenum#")
}

/// Whether a device's `WPD_DEVICE_PROTOCOL` ("MTP: 1.00", "PTP: 1.00",
/// "MSC:") is MTP.
fn is_mtp_protocol(protocol: &str) -> bool {
    protocol.trim().to_ascii_uppercase().starts_with("MTP")
}

/// Whether a storage's `WPD_STORAGE_ACCESS_CAPABILITY` lets Luminous write
/// to it. A storage that does not say is assumed writable.
fn is_writable(access: Option<u32>) -> bool {
    access.is_none_or(|a| a == WPD_STORAGE_ACCESS_CAPABILITY_READWRITE.0 as u32)
}

/// A shared connection to one device. Equal when it is the same device.
#[derive(Clone)]
pub(crate) struct Handle(Arc<Connection>);

impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        self.0.pnp_id == other.0.pnp_id
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.pnp_id.fmt(f)
    }
}

impl Handle {
    pub(crate) fn storage(&self, storage_id: &str) -> MtpStorage {
        // A storage that has since gone (an SD card taken out) keeps its
        // public id as its object id, so every call on it fails.
        let storage_object = self
            .0
            .storage_objects
            .lock()
            .get(storage_id)
            .cloned()
            .unwrap_or_else(|| storage_id.to_string());
        MtpStorage {
            connection: Arc::clone(&self.0),
            storage_object,
        }
    }

    pub(crate) fn eject(&self) {
        com();
        self.0.ejected.store(true, Ordering::Relaxed);
        let _guard = self.0.lock.lock();
        unsafe {
            let _ = self.0.device.Close();
        }
    }

    /// The device as it is now, with fresh storage capacities, or `None`
    /// while it has no storage Luminous can write to (a read-only camera
    /// roll, or a phone that is locked or not set to file transfer).
    fn device(&self) -> windows::core::Result<Option<crate::Device>> {
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
            reported_formats: self.0.formats.clone(),
            cache: Arc::clone(&self.0.cache),
        }))
    }
}

pub(crate) struct Connection {
    pnp_id: String,
    device: IPortableDevice,
    content: IPortableDeviceContent,
    properties: IPortableDeviceProperties,
    provisional_id: String,
    model: Option<String>,
    default_name: String,
    /// Lower-case names of the audio formats the device says it plays.
    formats: Vec<String>,
    lock: Mutex<()>,
    ejected: AtomicBool,
    /// Object ids of folders already found, by storage and lower-case path.
    folders: Mutex<HashMap<(String, String), String>>,
    /// Storage object ids by public storage id (see [`Storage::id`]).
    storage_objects: Mutex<HashMap<String, String>>,
    /// What `storages` last read, reused while a transfer holds `lock`.
    last_storages: Mutex<Vec<Storage>>,
    /// Whether renaming works on this device: unknown until the first write
    /// tries it. When it does not, writes go straight to the final name.
    can_rename: Mutex<Option<bool>>,
    /// Shared with every `Device` built from this connection, so the marker
    /// and default profile are read once per connection.
    cache: Arc<DeviceCache>,
}

// SAFETY: see the module comment. The interfaces come from
// `PortableDeviceFTM` and are free-threaded, every entry point calls `com()`
// first, and `lock` serializes their use.
unsafe impl Send for Connection {}
unsafe impl Sync for Connection {}

impl Connection {
    /// Opens a device. With `require_mtp`, fails for anything that is not
    /// an MTP device.
    fn open(pnp_id: &str, require_mtp: bool) -> windows::core::Result<Self> {
        com();
        unsafe {
            let client: IPortableDeviceValues =
                CoCreateInstance(&PortableDeviceValues, None, CLSCTX_INPROC_SERVER)?;
            client.SetStringValue(&WPD_CLIENT_NAME, windows::core::w!("Luminous"))?;
            client.SetUnsignedIntegerValue(&WPD_CLIENT_MAJOR_VERSION, 1)?;
            client.SetUnsignedIntegerValue(&WPD_CLIENT_MINOR_VERSION, 0)?;
            client.SetUnsignedIntegerValue(&WPD_CLIENT_REVISION, 0)?;
            client.SetUnsignedIntegerValue(
                &WPD_CLIENT_SECURITY_QUALITY_OF_SERVICE,
                SECURITY_IMPERSONATION_QOS,
            )?;
            let device: IPortableDevice =
                CoCreateInstance(&PortableDeviceFTM, None, CLSCTX_INPROC_SERVER)?;
            let pnp = wide(pnp_id);
            device.Open(PCWSTR(pnp.as_ptr()), &client)?;
            let content = device.Content()?;
            let properties = content.Properties()?;

            let values = properties.GetValues(
                WPD_DEVICE_OBJECT_ID,
                &keys(&[
                    WPD_DEVICE_PROTOCOL,
                    WPD_DEVICE_SERIAL_NUMBER,
                    WPD_DEVICE_MANUFACTURER,
                    WPD_DEVICE_MODEL,
                    WPD_DEVICE_FRIENDLY_NAME,
                ])?,
            )?;
            let protocol = string(&values, &WPD_DEVICE_PROTOCOL).unwrap_or_default();
            if require_mtp && !is_mtp_protocol(&protocol) {
                let _ = device.Close();
                return Err(windows::core::Error::new(
                    windows::Win32::Foundation::E_NOTIMPL,
                    format!("not an MTP device ({protocol})"),
                ));
            }
            let model = naming::model_name(
                string(&values, &WPD_DEVICE_MANUFACTURER).as_deref(),
                string(&values, &WPD_DEVICE_MODEL).as_deref(),
            );
            let default_name = string(&values, &WPD_DEVICE_FRIENDLY_NAME)
                .or_else(|| model.clone())
                .unwrap_or_else(|| "Portable device".to_string());
            let formats = supported_audio_formats(&device);
            Ok(Self {
                pnp_id: pnp_id.to_string(),
                provisional_id: naming::mtp_provisional_id(
                    string(&values, &WPD_DEVICE_SERIAL_NUMBER).as_deref(),
                    pnp_id,
                ),
                device,
                content,
                properties,
                model,
                default_name,
                formats,
                lock: Mutex::new(()),
                ejected: AtomicBool::new(false),
                folders: Mutex::new(HashMap::new()),
                storage_objects: Mutex::new(HashMap::new()),
                last_storages: Mutex::new(Vec::new()),
                can_rename: Mutex::new(None),
                cache: Arc::default(),
            })
        }
    }

    /// The device's writable storages with their current free space. While
    /// a transfer holds the device, returns what the last call read, so
    /// polling never waits for a copy to finish.
    fn storages(&self) -> windows::core::Result<Vec<Storage>> {
        com();
        let Some(_guard) = self.lock.try_lock() else {
            return Ok(self.last_storages.lock().clone());
        };
        unsafe {
            let ids = self
                .device
                .Capabilities()?
                .GetFunctionalObjects(&WPD_FUNCTIONAL_CATEGORY_STORAGE)?;
            let wanted = keys(&[
                WPD_OBJECT_PERSISTENT_UNIQUE_ID,
                WPD_STORAGE_DESCRIPTION,
                WPD_OBJECT_NAME,
                WPD_STORAGE_FILE_SYSTEM_TYPE,
                WPD_STORAGE_CAPACITY,
                WPD_STORAGE_FREE_SPACE_IN_BYTES,
                WPD_STORAGE_ACCESS_CAPABILITY,
            ])?;
            let mut storages = Vec::new();
            let mut objects = HashMap::new();
            for object in variant_strings(&ids)? {
                let values = self
                    .properties
                    .GetValues(&windows::core::HSTRING::from(&object), &wanted)?;
                if !is_writable(
                    values
                        .GetUnsignedIntegerValue(&WPD_STORAGE_ACCESS_CAPABILITY)
                        .ok(),
                ) {
                    continue;
                }
                // Object ids last one session; the persistent unique id is
                // what WPD keeps across reconnects.
                let id = string(&values, &WPD_OBJECT_PERSISTENT_UNIQUE_ID)
                    .unwrap_or_else(|| object.clone());
                objects.insert(id.clone(), object);
                storages.push(Storage {
                    label: string(&values, &WPD_STORAGE_DESCRIPTION)
                        .or_else(|| string(&values, &WPD_OBJECT_NAME)),
                    mount_path: None,
                    filesystem: string(&values, &WPD_STORAGE_FILE_SYSTEM_TYPE).unwrap_or_default(),
                    total_bytes: values
                        .GetUnsignedLargeIntegerValue(&WPD_STORAGE_CAPACITY)
                        .unwrap_or(0),
                    free_bytes: values
                        .GetUnsignedLargeIntegerValue(&WPD_STORAGE_FREE_SPACE_IN_BYTES)
                        .unwrap_or(0),
                    id,
                });
            }
            *self.storage_objects.lock() = objects;
            *self.last_storages.lock() = storages.clone();
            Ok(storages)
        }
    }

    /// The objects in a folder (or storage), with what a listing shows.
    /// Caller holds the lock.
    fn children(&self, parent: &str) -> io::Result<Vec<(String, DirEntry)>> {
        unsafe {
            let parent = windows::core::HSTRING::from(parent);
            let objects = self
                .content
                .EnumObjects(0, &parent, None)
                .map_err(io_error)?;
            let mut ids = Vec::new();
            loop {
                let mut batch = [PWSTR::null(); 64];
                let mut fetched = 0u32;
                let hr = objects.Next(&mut batch, &mut fetched);
                ids.extend(
                    batch
                        .into_iter()
                        .take(fetched as usize)
                        .filter_map(|id| take_string(id)),
                );
                if hr != S_OK || fetched == 0 {
                    hr.ok().map_err(io_error)?;
                    break;
                }
            }
            let wanted = keys(&[
                WPD_OBJECT_ORIGINAL_FILE_NAME,
                WPD_OBJECT_NAME,
                WPD_OBJECT_CONTENT_TYPE,
                WPD_OBJECT_SIZE,
                WPD_OBJECT_DATE_MODIFIED,
            ])
            .map_err(io_error)?;
            let mut children = Vec::with_capacity(ids.len());
            for id in ids {
                let Ok(values) = self
                    .properties
                    .GetValues(&windows::core::HSTRING::from(&id), &wanted)
                else {
                    continue;
                };
                let Some(name) = string(&values, &WPD_OBJECT_ORIGINAL_FILE_NAME)
                    .or_else(|| string(&values, &WPD_OBJECT_NAME))
                else {
                    continue;
                };
                let content_type = values
                    .GetGuidValue(&WPD_OBJECT_CONTENT_TYPE)
                    .unwrap_or_default();
                let is_dir = content_type == WPD_CONTENT_TYPE_FOLDER
                    || content_type == WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT;
                children.push((
                    id,
                    DirEntry {
                        name,
                        is_dir,
                        size: if is_dir {
                            0
                        } else {
                            values
                                .GetUnsignedLargeIntegerValue(&WPD_OBJECT_SIZE)
                                .unwrap_or(0)
                        },
                        modified: date_value(&values, &WPD_OBJECT_DATE_MODIFIED),
                    },
                ));
            }
            Ok(children)
        }
    }

    /// The child of `parent` called `name`: an exact match, else one that
    /// differs only in case. Caller holds the lock.
    fn child(&self, parent: &str, name: &str) -> io::Result<Option<(String, DirEntry)>> {
        let children = self.children(parent)?;
        let exact = children.iter().position(|(_, e)| e.name == name);
        let index = exact.or_else(|| {
            children
                .iter()
                .position(|(_, e)| e.name.eq_ignore_ascii_case(name))
        });
        Ok(index.map(|i| children.into_iter().nth(i).expect("index from position")))
    }

    /// Object id of the folder at `parts` under `storage`, creating missing
    /// folders when `create`. `None` when it does not exist (or is a file).
    /// Caller holds the lock.
    fn folder(&self, storage: &str, parts: &[&str], create: bool) -> io::Result<Option<String>> {
        let mut current = storage.to_string();
        let mut path = String::new();
        for part in parts {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(&part.to_lowercase());
            let key = (storage.to_string(), path.clone());
            if let Some(id) = self.folders.lock().get(&key) {
                current = id.clone();
                continue;
            }
            current = match self.child(&current, part)? {
                Some((id, entry)) if entry.is_dir => id,
                Some(_) => return Ok(None),
                None if create => self.create_folder(&current, part)?,
                None => return Ok(None),
            };
            self.folders.lock().insert(key, current.clone());
        }
        Ok(Some(current))
    }

    /// Forgets cached folder ids, after a failure that may mean they are
    /// stale (the folder was removed on the device itself).
    fn forget_folders(&self) {
        self.folders.lock().clear();
    }

    fn create_folder(&self, parent: &str, name: &str) -> io::Result<String> {
        unsafe {
            let values = new_object_values(parent, name)?;
            values
                .SetGuidValue(&WPD_OBJECT_CONTENT_TYPE, &WPD_CONTENT_TYPE_FOLDER)
                .map_err(io_error)?;
            values
                .SetGuidValue(&WPD_OBJECT_FORMAT, &WPD_OBJECT_FORMAT_PROPERTIES_ONLY)
                .map_err(io_error)?;
            let mut id = PWSTR::null();
            self.content
                .CreateObjectWithPropertiesOnly(&values, &mut id)
                .map_err(io_error)?;
            take_string(id).ok_or_else(|| io::Error::other("device returned no folder id"))
        }
    }

    /// Sends `len` bytes as a new object called `name` in `parent`; returns
    /// its id. The object's type comes from `final_name`, the name it will
    /// have once renamed. An uncommitted object is discarded by the device.
    /// Caller holds the lock.
    fn upload(
        &self,
        parent: &str,
        name: &str,
        final_name: &str,
        source: &mut dyn Read,
        len: u64,
        progress: Progress<'_>,
    ) -> io::Result<String> {
        let (content_type, format) = object_kind(final_name);
        unsafe {
            let values = new_object_values(parent, name)?;
            values
                .SetUnsignedLargeIntegerValue(&WPD_OBJECT_SIZE, len)
                .map_err(io_error)?;
            values
                .SetGuidValue(&WPD_OBJECT_CONTENT_TYPE, &content_type)
                .map_err(io_error)?;
            values
                .SetGuidValue(&WPD_OBJECT_FORMAT, &format)
                .map_err(io_error)?;

            let mut stream: Option<IStream> = None;
            let mut optimal = 0u32;
            self.content
                .CreateObjectWithPropertiesAndData(
                    &values,
                    &mut stream,
                    &mut optimal,
                    std::ptr::null_mut(),
                )
                .map_err(io_error)?;
            let stream = stream.ok_or_else(|| io::Error::other("device returned no stream"))?;
            let data: IPortableDeviceDataStream = stream.cast().map_err(io_error)?;

            let mut buffer = vec![0u8; (optimal as usize).clamp(64 * 1024, 1024 * 1024)];
            let mut written = 0u64;
            let sent: io::Result<()> = (|| {
                loop {
                    let n = match source.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) => return Err(e),
                    };
                    let mut offset = 0;
                    while offset < n {
                        let mut done = 0u32;
                        stream
                            .Write(
                                buffer[offset..].as_ptr().cast(),
                                (n - offset) as u32,
                                Some(&mut done),
                            )
                            .ok()
                            .map_err(io_error)?;
                        if done == 0 {
                            return Err(io::Error::other("device accepted no data"));
                        }
                        offset += done as usize;
                    }
                    written += n as u64;
                    if progress(written).is_break() {
                        return Err(cancelled());
                    }
                }
                if written != len {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        format!("expected {len} bytes, read {written}"),
                    ));
                }
                Ok(())
            })();
            if let Err(e) = sent {
                let _ = data.Cancel();
                return Err(e);
            }
            stream.Commit(STGC_DEFAULT).map_err(io_error)?;
            take_string(data.GetObjectID().map_err(io_error)?)
                .ok_or_else(|| io::Error::other("device returned no object id"))
        }
    }

    /// Whether objects in `folder` can be renamed. Learned once per
    /// connection by renaming a one-byte probe file, so a device that cannot
    /// rename is found out before any real file is replaced. Caller holds
    /// the lock.
    fn can_rename(&self, folder: &str) -> bool {
        if let Some(known) = *self.can_rename.lock() {
            return known;
        }
        let probe = format!(".luminous-rename-probe{PARTIAL_SUFFIX}");
        let works = self
            .upload(folder, &probe, &probe, &mut &b"L"[..], 1, &mut |_| {
                std::ops::ControlFlow::Continue(())
            })
            .and_then(|id| {
                let renamed_to = format!("{probe}2");
                let renamed = self.rename(&id, &renamed_to);
                // Some devices give a renamed object a new id, so find the
                // probe by whichever name it ended up with.
                for name in [&renamed_to, &probe] {
                    if let Ok(Some((id, _))) = self.child(folder, name) {
                        let _ = self.delete(&id);
                    }
                }
                renamed
            });
        if let Err(e) = &works {
            log::warn!(
                "{} cannot rename files ({e}); writing in place instead",
                self.default_name
            );
        }
        *self.can_rename.lock() = Some(works.is_ok());
        works.is_ok()
    }

    fn rename(&self, id: &str, name: &str) -> io::Result<()> {
        unsafe {
            let values: IPortableDeviceValues =
                CoCreateInstance(&PortableDeviceValues, None, CLSCTX_INPROC_SERVER)
                    .map_err(io_error)?;
            values
                .SetStringValue(
                    &WPD_OBJECT_ORIGINAL_FILE_NAME,
                    &windows::core::HSTRING::from(name),
                )
                .map_err(io_error)?;
            let results = self
                .properties
                .SetValues(&windows::core::HSTRING::from(id), &values)
                .map_err(io_error)?;
            // Per-property failures come back as error values, not an Err.
            match results.GetErrorValue(&WPD_OBJECT_ORIGINAL_FILE_NAME) {
                Ok(hr) if hr.is_err() => Err(io_error(hr.into())),
                _ => Ok(()),
            }
        }
    }

    fn delete(&self, id: &str) -> io::Result<()> {
        self.delete_with(id, PORTABLE_DEVICE_DELETE_WITH_RECURSION)
    }

    /// Deletes `id`, failing if it has anything in it: the device checks.
    fn delete_if_empty(&self, id: &str) -> io::Result<()> {
        self.delete_with(id, PORTABLE_DEVICE_DELETE_NO_RECURSION)
    }

    fn delete_with(&self, id: &str, options: DELETE_OBJECT_OPTIONS) -> io::Result<()> {
        unsafe {
            let ids: IPortableDevicePropVariantCollection = CoCreateInstance(
                &PortableDevicePropVariantCollection,
                None,
                CLSCTX_INPROC_SERVER,
            )
            .map_err(io_error)?;
            let mut variant = string_variant(id);
            let added = ids.Add(&variant);
            let _ = PropVariantClear(&mut variant);
            added.map_err(io_error)?;
            self.content
                .Delete(options.0 as u32, &ids, std::ptr::null_mut())
                .map_err(io_error)
        }
    }

    fn read_object(&self, id: &str) -> io::Result<Vec<u8>> {
        unsafe {
            let resources = self.content.Transfer().map_err(io_error)?;
            let mut optimal = 0u32;
            let mut stream: Option<IStream> = None;
            resources
                .GetStream(
                    &windows::core::HSTRING::from(id),
                    &WPD_RESOURCE_DEFAULT,
                    STGM_READ.0,
                    &mut optimal,
                    &mut stream,
                )
                .map_err(io_error)?;
            let stream = stream.ok_or_else(|| io::Error::other("device returned no stream"))?;
            let mut bytes = Vec::new();
            let mut buffer = vec![0u8; (optimal as usize).clamp(64 * 1024, 1024 * 1024)];
            loop {
                let mut read = 0u32;
                let hr = stream.Read(
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    Some(&mut read),
                );
                hr.ok().map_err(io_error)?;
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..read as usize]);
            }
            Ok(bytes)
        }
    }
}

/// [`DeviceBackend`] for one storage of an MTP device.
pub(crate) struct MtpStorage {
    connection: Arc<Connection>,
    /// The storage's object id for this connection (its public id is the
    /// persistent one; see [`Storage::id`]).
    storage_object: String,
}

impl MtpStorage {
    /// Runs `op` with the device locked. A failure forgets cached folder
    /// ids, in case one went stale.
    fn locked<T>(&self, op: impl FnOnce(&Connection) -> io::Result<T>) -> io::Result<T> {
        com();
        let connection = &*self.connection;
        if connection.ejected.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "device ejected",
            ));
        }
        let _guard = connection.lock.lock();
        op(connection).inspect_err(|_| connection.forget_folders())
    }

    /// The object at `path`, `None` when there is none.
    fn find(&self, c: &Connection, path: &str) -> io::Result<Option<(String, DirEntry)>> {
        let parts = components(path);
        let Some((name, parents)) = parts.split_last() else {
            return Ok(None);
        };
        match c.folder(&self.storage_object, parents, false)? {
            Some(parent) => c.child(&parent, name),
            None => Ok(None),
        }
    }
}

impl DeviceBackend for MtpStorage {
    fn list(&self, dir: &str) -> io::Result<Vec<DirEntry>> {
        self.locked(
            |c| match c.folder(&self.storage_object, &components(dir), false)? {
                Some(folder) => Ok(c.children(&folder)?.into_iter().map(|(_, e)| e).collect()),
                None => Ok(Vec::new()),
            },
        )
    }

    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        self.locked(|c| match self.find(c, path)? {
            Some((id, entry)) if !entry.is_dir => c.read_object(&id).map(Some),
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
            let parts = components(path);
            let Some((name, parents)) = parts.split_last() else {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "no file name"));
            };
            let parent = c
                .folder(&self.storage_object, parents, true)?
                .ok_or_else(|| io::Error::other("a file is in the way of the folder"))?;

            if !c.can_rename(&parent) {
                if let Some((old, _)) = c.child(&parent, name)? {
                    c.delete(&old)?;
                }
                c.upload(&parent, name, name, source, len, progress)?;
                return Ok(());
            }

            let partial = format!("{name}{PARTIAL_SUFFIX}");
            if let Some((stale, _)) = c.child(&parent, &partial)? {
                c.delete(&stale)?;
            }
            let new = c.upload(&parent, &partial, name, source, len, progress)?;
            if let Some((old, _)) = c.child(&parent, name)? {
                c.delete(&old).inspect_err(|_| {
                    let _ = c.delete(&new);
                })?;
            }
            c.rename(&new, name)
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
            if let Some((id, _)) = self.find(c, path)? {
                c.delete(&id)?;
            }
            // The folder may have been cached, along with everything in it.
            c.forget_folders();
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
            let Some((id, entry)) = self.find(c, path)? else {
                return Ok(false);
            };
            // Saves a refused delete in the common case; the device's own
            // refusal is what guarantees nothing in it is removed.
            if !entry.is_dir || !c.children(&id)?.is_empty() {
                return Ok(false);
            }
            c.delete_if_empty(&id)?;
            c.forget_folders();
            Ok(true)
        })
    }

    fn space(&self) -> io::Result<Space> {
        self.locked(|c| unsafe {
            let values = c
                .properties
                .GetValues(
                    &windows::core::HSTRING::from(&self.storage_object),
                    &keys(&[WPD_STORAGE_CAPACITY, WPD_STORAGE_FREE_SPACE_IN_BYTES])
                        .map_err(io_error)?,
                )
                .map_err(io_error)?;
            Ok(Space {
                total_bytes: values
                    .GetUnsignedLargeIntegerValue(&WPD_STORAGE_CAPACITY)
                    .map_err(io_error)?,
                free_bytes: values
                    .GetUnsignedLargeIntegerValue(&WPD_STORAGE_FREE_SPACE_IN_BYTES)
                    .map_err(io_error)?,
            })
        })
    }
}

/// The lower-case format names (as the profile's `supportedFormats` uses
/// them) of the audio formats the device reports. Empty when it reports none
/// that Luminous knows.
fn supported_audio_formats(device: &IPortableDevice) -> Vec<String> {
    let guids = unsafe {
        device
            .Capabilities()
            .and_then(|caps| caps.GetSupportedFormats(&WPD_CONTENT_TYPE_AUDIO))
            .and_then(|formats| variant_guids(&formats))
    };
    let mut names: Vec<String> = Vec::new();
    for name in guids.unwrap_or_default().iter().filter_map(format_name) {
        if !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    names
}

fn format_name(format: &GUID) -> Option<&'static str> {
    Some(match *format {
        WPD_OBJECT_FORMAT_MP3 => "mp3",
        WPD_OBJECT_FORMAT_AAC | WPD_OBJECT_FORMAT_M4A | WPD_OBJECT_FORMAT_MP4 => "aac",
        WPD_OBJECT_FORMAT_FLAC => "flac",
        WPD_OBJECT_FORMAT_OGG => "ogg",
        WPD_OBJECT_FORMAT_WAVE => "wav",
        WPD_OBJECT_FORMAT_AIFF => "aiff",
        WPD_OBJECT_FORMAT_WMA => "wma",
        _ => return None,
    })
}

/// Content type and format to create a file with, from its extension.
/// Devices index audio by these; anything else is a generic file.
fn object_kind(name: &str) -> (GUID, GUID) {
    let extension = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    let audio = |format| (WPD_CONTENT_TYPE_AUDIO, format);
    match extension.as_str() {
        "mp3" => audio(WPD_OBJECT_FORMAT_MP3),
        "m4a" => audio(WPD_OBJECT_FORMAT_M4A),
        "aac" => audio(WPD_OBJECT_FORMAT_AAC),
        "flac" => audio(WPD_OBJECT_FORMAT_FLAC),
        "ogg" | "oga" | "opus" => audio(WPD_OBJECT_FORMAT_OGG),
        "wav" => audio(WPD_OBJECT_FORMAT_WAVE),
        "aif" | "aiff" => audio(WPD_OBJECT_FORMAT_AIFF),
        "wma" => audio(WPD_OBJECT_FORMAT_WMA),
        _ => (WPD_CONTENT_TYPE_GENERIC_FILE, WPD_OBJECT_FORMAT_UNSPECIFIED),
    }
}

/// Properties for a new object called `name` in `parent`.
unsafe fn new_object_values(parent: &str, name: &str) -> io::Result<IPortableDeviceValues> {
    let values: IPortableDeviceValues =
        CoCreateInstance(&PortableDeviceValues, None, CLSCTX_INPROC_SERVER).map_err(io_error)?;
    let parent = windows::core::HSTRING::from(parent);
    let name = windows::core::HSTRING::from(name);
    values
        .SetStringValue(&WPD_OBJECT_PARENT_ID, &parent)
        .map_err(io_error)?;
    values
        .SetStringValue(&WPD_OBJECT_NAME, &name)
        .map_err(io_error)?;
    values
        .SetStringValue(&WPD_OBJECT_ORIGINAL_FILE_NAME, &name)
        .map_err(io_error)?;
    Ok(values)
}

unsafe fn keys(list: &[PROPERTYKEY]) -> windows::core::Result<IPortableDeviceKeyCollection> {
    let collection: IPortableDeviceKeyCollection =
        CoCreateInstance(&PortableDeviceKeyCollection, None, CLSCTX_INPROC_SERVER)?;
    for key in list {
        collection.Add(key)?;
    }
    Ok(collection)
}

/// A string value, `None` when missing or blank.
unsafe fn string(values: &IPortableDeviceValues, key: &PROPERTYKEY) -> Option<String> {
    take_string(values.GetStringValue(key).ok()?)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A `VT_DATE` value as Unix seconds.
unsafe fn date_value(values: &IPortableDeviceValues, key: &PROPERTYKEY) -> Option<i64> {
    let mut variant = values.GetValue(key).ok()?;
    let date = (variant.Anonymous.Anonymous.vt == VT_DATE)
        .then(|| variant.Anonymous.Anonymous.Anonymous.date);
    let _ = PropVariantClear(&mut variant);
    // OLE dates count days from 1899-12-30; the Unix epoch is day 25569.
    date.map(|days| ((days - 25_569.0) * 86_400.0).round() as i64)
}

/// Takes ownership of a `CoTaskMemAlloc`ed string.
unsafe fn take_string(s: PWSTR) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let text = s.to_string().ok();
    CoTaskMemFree(Some(s.0 as *const _));
    text
}

unsafe fn variant_strings(
    collection: &IPortableDevicePropVariantCollection,
) -> windows::core::Result<Vec<String>> {
    variants(collection, |v| {
        PropVariantToStringAlloc(v)
            .ok()
            .and_then(|s| take_string(s))
    })
}

unsafe fn variant_guids(
    collection: &IPortableDevicePropVariantCollection,
) -> windows::core::Result<Vec<GUID>> {
    variants(collection, |v| PropVariantToGUID(v).ok())
}

unsafe fn variants<T>(
    collection: &IPortableDevicePropVariantCollection,
    convert: impl Fn(&PROPVARIANT) -> Option<T>,
) -> windows::core::Result<Vec<T>> {
    let mut count = 0u32;
    collection.GetCount(std::ptr::addr_of_mut!(count))?;
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let mut variant = PROPVARIANT::default();
        collection.GetAt(i, std::ptr::addr_of_mut!(variant))?;
        if let Some(value) = convert(&variant) {
            out.push(value);
        }
        let _ = PropVariantClear(&mut variant);
    }
    Ok(out)
}

/// A `VT_LPWSTR` variant owning a copy of `s`; clear it with
/// `PropVariantClear`.
unsafe fn string_variant(s: &str) -> PROPVARIANT {
    let text = wide(s);
    let copy = CoTaskMemAlloc(text.len() * 2) as *mut u16;
    std::ptr::copy_nonoverlapping(text.as_ptr(), copy, text.len());
    let mut variant = PROPVARIANT::default();
    let inner = &mut variant.Anonymous.Anonymous;
    inner.vt = VT_LPWSTR;
    inner.Anonymous.pwszVal = PWSTR(copy);
    variant
}

fn io_error(e: windows::core::Error) -> io::Error {
    io::Error::other(e)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::ControlFlow;

    #[test]
    fn volume_shims_are_recognized_by_their_pnp_id() {
        assert!(is_volume_shim(
            r"\\?\SWD#WPDBUSENUM#_??_USBSTOR#DISK&VEN_HIBY&PROD_R1_U-DISK&REV_000#6&1C473F73&0#{53F56307-B6BF-11D0-94F2-00A0C91EFB8B}#{6AC27878-A6FA-4155-BA85-F98F491D4F33}"
        ));
        assert!(is_volume_shim(
            r"\\?\swd#wpdbusenum#{e2f0aea8-dd01-11f0-b47d-8cb87e0ddfa8}#0000000000100000#{6ac27878-a6fa-4155-ba85-f98f491d4f33}"
        ));
        assert!(!is_volume_shim(
            r"\\?\usb#vid_054c&pid_0d9a#5a2b6c1d#{6ac27878-a6fa-4155-ba85-f98f491d4f33}"
        ));
    }

    #[test]
    fn only_mtp_protocols_count() {
        assert!(is_mtp_protocol("MTP: 1.00"));
        assert!(is_mtp_protocol(" mtp"));
        assert!(!is_mtp_protocol("PTP: 1.00"));
        assert!(!is_mtp_protocol("MSC:"));
        assert!(!is_mtp_protocol(""));
    }

    #[test]
    fn read_only_storages_are_not_sync_targets() {
        assert!(is_writable(Some(0)));
        assert!(is_writable(None));
        // An iPhone's camera roll reports 1, read-only without deletion.
        assert!(!is_writable(Some(1)));
        assert!(!is_writable(Some(2)));
    }

    #[test]
    fn audio_formats_map_to_profile_names() {
        assert_eq!(format_name(&WPD_OBJECT_FORMAT_FLAC), Some("flac"));
        assert_eq!(format_name(&WPD_OBJECT_FORMAT_M4A), Some("aac"));
        assert_eq!(format_name(&WPD_OBJECT_FORMAT_UNSPECIFIED), None);
    }

    #[test]
    fn files_are_created_with_a_kind_from_their_extension() {
        assert_eq!(
            object_kind("01 Wail.M4A"),
            (WPD_CONTENT_TYPE_AUDIO, WPD_OBJECT_FORMAT_M4A)
        );
        assert_eq!(
            object_kind("cover.jpg"),
            (WPD_CONTENT_TYPE_GENERIC_FILE, WPD_OBJECT_FORMAT_UNSPECIFIED)
        );
        assert_eq!(
            object_kind("Playlist.m3u8").0,
            WPD_CONTENT_TYPE_GENERIC_FILE
        );
    }

    /// Prints every portable device Windows lists, its protocol, and what a
    /// poll makes of them, for checking a real phone or player by eye:
    /// `cargo test -p luminous-devices -- --ignored wpd_poll --nocapture`.
    #[test]
    #[ignore = "reports on connected devices; see the doc comment"]
    fn wpd_poll_report() {
        com();
        let mut watcher = Watcher::new();
        for id in watcher.device_ids().unwrap() {
            println!("{id}\n  volume shim {}", is_volume_shim(&id));
            if is_volume_shim(&id) {
                continue;
            }
            let c = Connection::open(&id, false).unwrap();
            unsafe {
                let values = c
                    .properties
                    .GetValues(WPD_DEVICE_OBJECT_ID, &keys(&[WPD_DEVICE_PROTOCOL]).unwrap())
                    .unwrap();
                println!("  protocol {:?}", string(&values, &WPD_DEVICE_PROTOCOL));
                let storages = c
                    .device
                    .Capabilities()
                    .unwrap()
                    .GetFunctionalObjects(&WPD_FUNCTIONAL_CATEGORY_STORAGE)
                    .unwrap();
                for storage in variant_strings(&storages).unwrap() {
                    let values = c
                        .properties
                        .GetValues(
                            &windows::core::HSTRING::from(&storage),
                            &keys(&[WPD_STORAGE_ACCESS_CAPABILITY, WPD_STORAGE_FILE_SYSTEM_TYPE])
                                .unwrap(),
                        )
                        .unwrap();
                    println!(
                        "  storage {storage}: access {:?}, filesystem {:?}",
                        values.GetUnsignedIntegerValue(&WPD_STORAGE_ACCESS_CAPABILITY),
                        string(&values, &WPD_STORAGE_FILE_SYSTEM_TYPE),
                    );
                    // Two levels, with file counts by extension below that.
                    for (folder_id, folder) in c.children(&storage).unwrap() {
                        println!(
                            "    {}{}",
                            folder.name,
                            if folder.is_dir { "/" } else { "" }
                        );
                        if !folder.is_dir {
                            continue;
                        }
                        let mut extensions: HashMap<String, usize> = HashMap::new();
                        let children = c.children(&folder_id).unwrap();
                        for (child_id, child) in &children {
                            let entries = if child.is_dir {
                                c.children(child_id).unwrap()
                            } else {
                                vec![(child_id.clone(), child.clone())]
                            };
                            for (_, entry) in entries.iter().filter(|(_, e)| !e.is_dir) {
                                let ext = entry
                                    .name
                                    .rsplit_once('.')
                                    .map(|(_, e)| e.to_ascii_lowercase())
                                    .unwrap_or_default();
                                *extensions.entry(ext).or_default() += 1;
                            }
                        }
                        println!(
                            "      {} entries, files by type {extensions:?}",
                            children.len()
                        );
                    }
                }
            }
        }
        for device in watcher.poll() {
            println!("polled: {} ({:?})", device.default_name, device.model);
        }
    }

    /// Exercises the WPD backend on real hardware. WPD's view of a mounted
    /// volume behaves like a device here, so set
    /// `LUMINOUS_WPD_TEST_DEVICE` to part of its PnP id (e.g. `hiby`) and run
    /// `cargo test -p luminous-devices -- --ignored wpd_backend`. Works only
    /// inside a `luminous-wpd-test` folder it removes afterwards.
    #[test]
    #[ignore = "needs a connected device; see the doc comment"]
    fn wpd_backend_round_trip() {
        com();
        let needle = std::env::var("LUMINOUS_WPD_TEST_DEVICE")
            .expect("set LUMINOUS_WPD_TEST_DEVICE")
            .to_ascii_lowercase();
        let mut watcher = Watcher::new();
        let pnp_id = watcher
            .device_ids()
            .unwrap()
            .into_iter()
            .find(|id| id.to_ascii_lowercase().contains(&needle))
            .expect("no portable device matches LUMINOUS_WPD_TEST_DEVICE");
        let handle = Handle(Arc::new(Connection::open(&pnp_id, false).unwrap()));
        let device = handle.device().unwrap().expect("a writable storage");
        println!("{device:#?} formats {:?}", handle.0.formats);
        let storage = handle.storage(&device.storages[0].id);
        let space = storage.space().unwrap();
        assert!(space.total_bytes >= space.free_bytes && space.total_bytes > 0);

        let root = "luminous-wpd-test";
        storage.remove(root).unwrap();
        let data: Vec<u8> = (0..200_000u32).map(|i| i as u8).collect();
        let write = |path: &str, bytes: &[u8]| {
            storage.write(path, &mut &bytes[..], bytes.len() as u64, &mut |_| {
                ControlFlow::Continue(())
            })
        };
        write("luminous-wpd-test/Artist/Album/01 Track.mp3", &data).unwrap();
        write("luminous-wpd-test/Artist/Album/01 Track.mp3", b"replaced").unwrap();
        assert_eq!(
            storage
                .read("luminous-wpd-test/artist/ALBUM/01 Track.mp3")
                .unwrap()
                .as_deref(),
            Some(&b"replaced"[..])
        );
        let listing = storage.list("luminous-wpd-test/Artist/Album").unwrap();
        assert_eq!(listing.len(), 1, "{listing:?}");
        assert_eq!(listing[0].name, "01 Track.mp3");
        assert_eq!(listing[0].size, 8);

        let err = storage
            .write(
                "luminous-wpd-test/cancelled.mp3",
                &mut &data[..],
                data.len() as u64,
                &mut |_| ControlFlow::Break(()),
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

        assert_eq!(storage.read("luminous-wpd-test/missing.mp3").unwrap(), None);
        assert!(storage
            .list("luminous-wpd-test/missing")
            .unwrap()
            .is_empty());
        storage.remove(root).unwrap();
        storage.remove(root).unwrap();
        assert!(storage.list(root).unwrap().is_empty());
    }
}
