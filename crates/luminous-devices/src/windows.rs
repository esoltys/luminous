//! Windows: drive letters, classified by bus, ejected through Plug and Play.

use crate::{EjectError, Volume};
use std::collections::HashMap;
use std::path::PathBuf;
use windows::core::PCWSTR;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Parent, CM_Request_Device_EjectW, PNP_VetoTypeUnknown, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
    CR_SUCCESS, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, PNP_VETO_TYPE,
    SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    BusTypeMmc, BusTypeSd, BusTypeUsb, CreateFileW, FlushFileBuffers, GetDiskFreeSpaceExW,
    GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, FILE_FLAGS_AND_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Diagnostics::Debug::{
    SetThreadErrorMode, SEM_FAILCRITICALERRORS, SEM_NOOPENFILEERRORBOX,
};
use windows::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceProperty, FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME,
    GUID_DEVINTERFACE_DISK, IOCTL_STORAGE_GET_DEVICE_NUMBER, IOCTL_STORAGE_QUERY_PROPERTY,
    STORAGE_DEVICE_DESCRIPTOR, STORAGE_DEVICE_NUMBER, STORAGE_PROPERTY_QUERY,
};
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows::Win32::System::IO::DeviceIoControl;

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;

/// What the storage stack says about the disk behind a drive letter.
#[derive(Clone, Default)]
struct Hardware {
    on_removable_bus: bool,
    vendor: Option<String>,
    model: Option<String>,
}

pub(crate) struct Watcher {
    system_drive: Option<char>,
    /// Keyed by letter and volume serial, so a different volume getting the
    /// same letter is looked up again. Fixed internal drives stay cached, so
    /// a poll never wakes them with an IOCTL.
    hardware: HashMap<(char, u32), Hardware>,
}

impl Watcher {
    pub(crate) fn new() -> Self {
        // An empty card reader must never put up "There is no disk in the
        // drive" while we poll it.
        unsafe {
            let _ = SetThreadErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX, None);
        }
        Self {
            system_drive: system_drive(),
            hardware: HashMap::new(),
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<Volume> {
        let mask = unsafe { GetLogicalDrives() };
        let mut volumes = Vec::new();
        for (i, letter) in ('A'..='Z').enumerate() {
            if mask & (1 << i) == 0 || Some(letter) == self.system_drive {
                continue;
            }
            if let Some(volume) = self.read_drive(letter) {
                volumes.push(volume);
            }
        }
        self.hardware
            .retain(|(letter, _), _| mask & (1 << (*letter as u32 - 'A' as u32)) != 0);
        volumes
    }

    fn read_drive(&mut self, letter: char) -> Option<Volume> {
        let root = wide(&format!("{letter}:\\"));
        let drive_type = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
        if drive_type != DRIVE_REMOVABLE && drive_type != DRIVE_FIXED {
            return None;
        }

        let mut label = [0u16; 261];
        let mut fs_name = [0u16; 261];
        let mut serial = 0u32;
        // Fails for a card reader with no card: not a device until one is in.
        unsafe {
            GetVolumeInformationW(
                PCWSTR(root.as_ptr()),
                Some(&mut label),
                Some(&mut serial),
                None,
                None,
                Some(&mut fs_name),
            )
        }
        .ok()?;

        let hardware = self
            .hardware
            .entry((letter, serial))
            .or_insert_with(|| query_hardware(letter).unwrap_or_default())
            .clone();
        if drive_type == DRIVE_FIXED && !hardware.on_removable_bus {
            return None;
        }

        let (mut free, mut total) = (0u64, 0u64);
        unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(root.as_ptr()),
                Some(&mut free),
                Some(&mut total),
                None,
            )
        }
        .ok()?;

        Some(Volume {
            mount_path: PathBuf::from(format!("{letter}:\\")),
            label: Some(from_wide(&label)).filter(|l| !l.trim().is_empty()),
            filesystem: from_wide(&fs_name),
            serial: Some(format!("{:04X}-{:04X}", serial >> 16, serial & 0xFFFF)),
            vendor: hardware.vendor,
            model: hardware.model,
            total_bytes: total,
            free_bytes: free,
            os_ref: letter.to_string(),
        })
    }
}

pub(crate) fn eject(volume: &Volume) -> Result<(), EjectError> {
    let letter = volume
        .os_ref
        .chars()
        .next()
        .ok_or_else(|| EjectError::Failed("no drive letter".into()))?;
    let device_number = {
        let volume_handle = open_volume(letter, GENERIC_READ.0 | GENERIC_WRITE.0)
            .map_err(|e| EjectError::Failed(e.to_string()))?;
        unsafe {
            let _ = FlushFileBuffers(volume_handle.0);
        }
        // Locking fails while other programs hold files open. Carry on: the
        // Plug and Play request below names the blocker.
        if ioctl_no_buffers(&volume_handle, FSCTL_LOCK_VOLUME) {
            ioctl_no_buffers(&volume_handle, FSCTL_DISMOUNT_VOLUME);
        }
        device_number(&volume_handle).map_err(|e| EjectError::Failed(e.to_string()))?
        // The volume handle closes here, before the eject request: an open
        // handle of our own would veto it.
    };

    let disk =
        disk_dev_inst(device_number).ok_or_else(|| EjectError::Failed("disk not found".into()))?;
    // The disk's parent is the USB (or card reader) device: removing it is
    // what "Safely Remove Hardware" does. Retry briefly, since the dismount
    // above can still be settling.
    let mut parent = 0u32;
    let target = if unsafe { CM_Get_Parent(&mut parent, disk, 0) } == CR_SUCCESS {
        parent
    } else {
        disk
    };
    let mut last = EjectError::Failed("eject was not attempted".into());
    for _ in 0..3 {
        let mut veto_type = PNP_VETO_TYPE::default();
        let mut veto_name = [0u16; 260];
        let result = unsafe {
            CM_Request_Device_EjectW(target, Some(&mut veto_type), Some(&mut veto_name), 0)
        };
        if result == CR_SUCCESS && veto_type == PNP_VetoTypeUnknown {
            return Ok(());
        }
        last = if veto_type != PNP_VetoTypeUnknown {
            EjectError::InUse(Some(from_wide(&veto_name)).filter(|n| !n.is_empty()))
        } else {
            EjectError::Failed(format!("Plug and Play error {}", result.0))
        };
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Err(last)
}

/// A Win32 handle closed on drop.
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn open_path(path: &str, access: u32) -> windows::core::Result<OwnedHandle> {
    let path = wide(path);
    unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map(OwnedHandle)
}

fn open_volume(letter: char, access: u32) -> windows::core::Result<OwnedHandle> {
    open_path(&format!("\\\\.\\{letter}:"), access)
}

fn ioctl_no_buffers(handle: &OwnedHandle, code: u32) -> bool {
    let mut returned = 0u32;
    unsafe { DeviceIoControl(handle.0, code, None, 0, None, 0, Some(&mut returned), None) }.is_ok()
}

fn device_number(handle: &OwnedHandle) -> windows::core::Result<STORAGE_DEVICE_NUMBER> {
    let mut number = STORAGE_DEVICE_NUMBER::default();
    let mut returned = 0u32;
    unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            None,
            0,
            Some(&mut number as *mut _ as *mut _),
            size_of::<STORAGE_DEVICE_NUMBER>() as u32,
            Some(&mut returned),
            None,
        )
    }?;
    Ok(number)
}

/// Bus, vendor and product of the disk behind `letter`. Opening the volume
/// with no access rights is enough for this query and never spins up media.
fn query_hardware(letter: char) -> Option<Hardware> {
    let handle = open_volume(letter, 0).ok()?;
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        ..Default::default()
    };
    // u64 elements keep the buffer aligned for the descriptor read below.
    let mut buffer = [0u64; 128];
    let buffer_len = size_of_val(&buffer);
    let mut returned = 0u32;
    unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const _ as *const _),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(buffer.as_mut_ptr() as *mut _),
            buffer_len as u32,
            Some(&mut returned),
            None,
        )
    }
    .ok()?;
    let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u8, buffer_len) };
    let descriptor = unsafe { &*(buffer.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };
    let ascii_at = |offset: u32| -> Option<String> {
        let start = offset as usize;
        if start == 0 || start >= bytes.len() {
            return None;
        }
        let end = bytes[start..].iter().position(|&b| b == 0)? + start;
        let text = String::from_utf8_lossy(&bytes[start..end])
            .trim()
            .to_string();
        (!text.is_empty()).then_some(text)
    };
    Some(Hardware {
        on_removable_bus: [BusTypeUsb, BusTypeSd, BusTypeMmc].contains(&descriptor.BusType),
        vendor: ascii_at(descriptor.VendorIdOffset),
        model: ascii_at(descriptor.ProductIdOffset),
    })
}

/// The device instance of the disk with this storage device number.
fn disk_dev_inst(number: STORAGE_DEVICE_NUMBER) -> Option<u32> {
    struct DevInfo(HDEVINFO);
    impl Drop for DevInfo {
        fn drop(&mut self) {
            unsafe {
                let _ = SetupDiDestroyDeviceInfoList(self.0);
            }
        }
    }

    let set = DevInfo(
        unsafe {
            SetupDiGetClassDevsW(
                Some(&GUID_DEVINTERFACE_DISK),
                PCWSTR::null(),
                None,
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        }
        .ok()?,
    );
    for index in 0.. {
        let mut interface = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        if unsafe {
            SetupDiEnumDeviceInterfaces(set.0, None, &GUID_DEVINTERFACE_DISK, index, &mut interface)
        }
        .is_err()
        {
            return None;
        }
        let mut required = 0u32;
        unsafe {
            let _ = SetupDiGetDeviceInterfaceDetailW(
                set.0,
                &interface,
                None,
                0,
                Some(&mut required),
                None,
            );
        }
        // u64 elements keep the variable-length detail struct aligned.
        let mut detail = vec![0u64; (required as usize).div_ceil(8).max(1)];
        let detail_ptr = detail.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        unsafe {
            (*detail_ptr).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        }
        let mut info = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        if unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                set.0,
                &interface,
                Some(detail_ptr),
                required,
                None,
                Some(&mut info),
            )
        }
        .is_err()
        {
            continue;
        }
        let path = unsafe { PCWSTR((*detail_ptr).DevicePath.as_ptr()).to_string() }.ok()?;
        let Ok(disk) = open_path(&path, 0) else {
            continue;
        };
        if let Ok(disk_number) = device_number(&disk) {
            if disk_number.DeviceNumber == number.DeviceNumber
                && disk_number.DeviceType == number.DeviceType
            {
                return Some(info.DevInst);
            }
        }
    }
    None
}

fn system_drive() -> Option<char> {
    let mut buffer = [0u16; 261];
    let len = unsafe { GetSystemWindowsDirectoryW(Some(&mut buffer)) } as usize;
    String::from_utf16_lossy(&buffer[..len.min(buffer.len())])
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buffer: &[u16]) -> String {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}
