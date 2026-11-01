//! Linux: MTP players and phones through the desktop's own MTP daemon
//! (#1845): gvfs (`gvfs-mtp`, GNOME and most GTK desktops) through GIO, or
//! KDE's kio-mtp daemon (`kmtpd`) over D-Bus.
//!
//! The daemon claims a device as soon as it is plugged in, so anything that
//! opens the USB device itself (libmtp) gets "device busy"; going through
//! the daemon cooperates with that claim. A device is owned by one daemon,
//! so it is listed once; a desktop running neither sees no MTP devices.
//!
//! Each daemon is a [`Transport`]; what MTP needs on top of that is shared
//! with Windows in [`crate::mtp`].

mod gvfs;
mod kmtp;

use crate::backend::{cancelled, PARTIAL_SUFFIX};
use crate::mtp::{Handle, Identity, StorageInfo, Transport};
use crate::{naming, Progress};
use std::collections::HashMap;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
        // A device already known keeps its connection (lock, caches); the
        // transport just found for it, which did no I/O, is dropped.
        for f in found {
            let handle = self
                .known
                .entry(f.key)
                .or_insert_with(|| Handle::new(f.identity, f.transport));
            if handle.is_ejected() {
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
                Err(e) => log::info!("Lost MTP device {}: {e}", handle.name()),
            }
        }
        devices
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
            // Neither daemon says which formats a device plays.
            formats: Vec::new(),
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
        copy_exactly(
            source,
            len,
            |chunk| spool.file.write_all(chunk),
            &mut |_| progress(0),
        )?;
        spool.file.flush()?;
        spool.file.rewind()?;
        Ok(spool)
    }
}

/// Copies exactly `len` bytes from `source` to `sink`, telling `progress`
/// how many have been copied after each chunk. Fails with
/// [`io::ErrorKind::Interrupted`] when `progress` cancels, and with
/// [`io::ErrorKind::UnexpectedEof`] when `source` holds fewer or more bytes.
fn copy_exactly(
    source: &mut dyn Read,
    len: u64,
    mut sink: impl FnMut(&[u8]) -> io::Result<()>,
    progress: Progress<'_>,
) -> io::Result<()> {
    let mut buffer = vec![0u8; 256 * 1024];
    let mut copied = 0u64;
    loop {
        let n = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        sink(&buffer[..n])?;
        copied += n as u64;
        if progress(copied).is_break() {
            return Err(cancelled());
        }
    }
    if copied != len {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("expected {len} bytes, read {copied}"),
        ));
    }
    Ok(())
}

impl Drop for Spool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::ControlFlow;

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
