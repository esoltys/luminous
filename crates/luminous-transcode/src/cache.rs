//! The host-side transcode cache: one immutable file per prepared source,
//! named by a key that changes whenever its content would. There is no
//! index; the directory is the index and each file's mtime is when it was
//! last used.
//!
//! There is no size cap to configure. A cap smaller than what a device
//! syncs would evict every entry before the next sync reuses it, and one
//! larger is just a number. Instead the cache keeps what syncs keep using
//! and gives space back when it isn't:
//! - entries unused for [`MAX_IDLE`] go (a device not synced in that long,
//!   or songs no longer selected);
//! - least-recently-used entries go whenever the cache's disk has less
//!   free than [`free_floor`];
//! - where free space can't be read, [`FALLBACK_CAP`] bounds it instead.

use crate::rule::Decision;
use crate::source::SourceInfo;
use crate::Error;
use luminous_devices::{DeviceBackend, FsBackend};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// Bump when the pipeline would produce different output for the same
/// source and decision, so stale entries stop matching.
const PIPELINE_VERSION: u32 = 1;

/// Marks files still being written. A leftover from a crash is removed once
/// it is older than any conversion takes.
const PARTIAL: &str = ".partial";
const STALE_PARTIAL: Duration = Duration::from_secs(60 * 60);

/// Entries unused this long are removed.
const MAX_IDLE: Duration = Duration::from_secs(60 * 24 * 60 * 60);

/// Free space the cache leaves on its disk: 10%, and at least 10 GB.
fn free_floor(disk_total: u64) -> u64 {
    (disk_total / 10).max(10_000_000_000)
}

/// The cache's ceiling where its disk's free space can't be read.
const FALLBACK_CAP: u64 = 50_000_000_000;

/// `(total, free)` bytes of the disk holding a path.
type DiskSpace = dyn Fn(&Path) -> Option<(u64, u64)> + Send + Sync;

/// The cache key for `info` prepared per `decision`.
pub(crate) fn key(info: &SourceInfo, decision: &Decision) -> String {
    let mut hash = Sha256::new();
    hash.update(format!(
        "{PIPELINE_VERSION}\0{}\0{}\0{}\0{decision:?}",
        info.path.display(),
        info.stamp.size,
        info.stamp.mtime_ns
    ));
    hash.finalize()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) struct Cache {
    dir: PathBuf,
    disk: Box<DiskSpace>,
    /// Idle entries and crash leftovers are swept on the first write of a
    /// run, not every write: a sweep lists the whole directory.
    swept: AtomicBool,
    /// Serializes eviction; reads and writes of entries need no lock.
    evicting: Mutex<()>,
    partial_seq: AtomicU64,
}

impl Cache {
    pub fn new(dir: PathBuf) -> Self {
        Self::with_disk(
            dir,
            Box::new(|path| {
                let space = FsBackend::new(path).space().ok()?;
                Some((space.total_bytes, space.free_bytes))
            }),
        )
    }

    fn with_disk(dir: PathBuf, disk: Box<DiskSpace>) -> Self {
        Self {
            dir,
            disk,
            swept: AtomicBool::new(false),
            evicting: Mutex::new(()),
            partial_seq: AtomicU64::new(0),
        }
    }

    fn entry(&self, key: &str, extension: &str) -> PathBuf {
        self.dir.join(format!("{key}.{extension}"))
    }

    /// Size of the entry for `key`, if there is one.
    pub fn size_of(&self, key: &str, extension: &str) -> Option<u64> {
        std::fs::metadata(self.entry(key, extension))
            .ok()
            .map(|m| m.len())
    }

    /// The entry for `key`, made by `produce` (which writes the file at the
    /// path it is given) when there isn't one. A failed or cancelled
    /// `produce` leaves nothing behind.
    pub fn get_or_insert(
        &self,
        key: &str,
        extension: &str,
        produce: impl FnOnce(&Path) -> Result<(), Error>,
    ) -> Result<PathBuf, Error> {
        let path = self.entry(key, extension);
        if path.exists() {
            touch(&path);
            return Ok(path);
        }
        std::fs::create_dir_all(&self.dir)?;
        // The extension stays last: encoders and tag writers go by it.
        let seq = self.partial_seq.fetch_add(1, Ordering::Relaxed);
        let partial = self.dir.join(format!(
            "{key}.{}-{seq}{PARTIAL}.{extension}",
            std::process::id()
        ));
        if let Err(e) = produce(&partial) {
            let _ = std::fs::remove_file(&partial);
            return Err(e);
        }
        if let Err(e) = std::fs::rename(&partial, &path) {
            let _ = std::fs::remove_file(&partial);
            // Another thread finished the same entry first.
            if !path.exists() {
                return Err(e.into());
            }
        }
        self.make_room(&path);
        Ok(path)
    }

    /// Apply the eviction rules after writing `keep`, which is spared. Cheap
    /// unless there is something to do: one free-space query per write, and
    /// a directory listing only on the first write of a run or when the
    /// disk is short of space.
    fn make_room(&self, keep: &Path) {
        let _guard = self.evicting.lock();
        let sweep = !self.swept.swap(true, Ordering::Relaxed);
        let space = (self.disk)(&self.dir);
        let short = space.map(|(total, free)| free_floor(total).saturating_sub(free));
        if !sweep && short == Some(0) {
            return;
        }
        let Ok(dir) = std::fs::read_dir(&self.dir) else {
            return;
        };

        let now = SystemTime::now();
        let age = |time: SystemTime| now.duration_since(time).unwrap_or_default();
        let mut entries = Vec::new();
        let mut total = 0u64;
        let mut freed = 0u64;
        for entry in dir.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let path = entry.path();
            let used = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let partial = path.to_string_lossy().contains(PARTIAL);
            if partial {
                if sweep && age(used) > STALE_PARTIAL && std::fs::remove_file(&path).is_ok() {
                    freed += meta.len();
                }
                continue;
            }
            if sweep && age(used) > MAX_IDLE && path != keep && std::fs::remove_file(&path).is_ok()
            {
                freed += meta.len();
                continue;
            }
            total += meta.len();
            entries.push((used, meta.len(), path));
        }

        let mut excess = match short {
            Some(short) => short.saturating_sub(freed),
            None => total.saturating_sub(FALLBACK_CAP),
        };
        entries.sort_by_key(|(used, ..)| *used);
        for (_, len, path) in entries {
            if excess == 0 {
                break;
            }
            if path != keep && std::fs::remove_file(&path).is_ok() {
                excess = excess.saturating_sub(len);
            }
        }
    }
}

/// Mark `path` as just used.
fn touch(path: &Path) {
    if let Ok(file) = std::fs::File::options().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(bytes: usize) -> impl FnOnce(&Path) -> Result<(), Error> {
        move |path| Ok(std::fs::write(path, vec![0u8; bytes])?)
    }

    /// Sets `path`'s last use to `secs` into 2023: long idle.
    fn age(path: &Path, secs: u64) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs))
            .unwrap();
    }

    /// A 100 GB disk whose free space is `room` bytes above the floor,
    /// minus what the cache holds.
    fn disk_with_room(room: u64) -> Box<DiskSpace> {
        const TOTAL: u64 = 100_000_000_000;
        Box::new(move |dir| {
            let used: u64 = std::fs::read_dir(dir)
                .ok()?
                .flatten()
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum();
            Some((TOTAL, (free_floor(TOTAL) + room).saturating_sub(used)))
        })
    }

    #[test]
    fn a_second_request_is_a_hit() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::with_disk(dir.path().join("t"), disk_with_room(1_000));
        let first = cache.get_or_insert("k", "m4a", write(10)).unwrap();
        let second = cache
            .get_or_insert("k", "m4a", |_| panic!("must not produce again"))
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(cache.size_of("k", "m4a"), Some(10));
        assert_eq!(cache.size_of("other", "m4a"), None);
    }

    #[test]
    fn a_failed_produce_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().to_path_buf());
        let err = cache
            .get_or_insert("k", "opus", |path| {
                std::fs::write(path, b"half")?;
                Err(Error::Cancelled)
            })
            .unwrap_err();
        assert_eq!(err, Error::Cancelled);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_disk_short_of_space_drops_least_recently_used_first() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::with_disk(dir.path().to_path_buf(), disk_with_room(25));
        let a = cache.get_or_insert("a", "mp3", write(10)).unwrap();
        let b = cache.get_or_insert("b", "mp3", write(10)).unwrap();
        age(&a, 2);
        age(&b, 1);
        // Using `b` makes `a` the oldest.
        cache.get_or_insert("b", "mp3", write(10)).unwrap();
        let c = cache.get_or_insert("c", "mp3", write(10)).unwrap();
        assert!(!a.exists());
        assert!(b.exists());
        assert!(c.exists());
    }

    #[test]
    fn the_new_entry_survives_until_the_next_write() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::with_disk(dir.path().to_path_buf(), disk_with_room(5));
        let a = cache.get_or_insert("a", "mp3", write(10)).unwrap();
        assert!(a.exists());
        let b = cache.get_or_insert("b", "mp3", write(10)).unwrap();
        assert!(!a.exists());
        assert!(b.exists());
    }

    #[test]
    fn the_first_write_sweeps_idle_entries_and_stale_partials() {
        let dir = tempfile::tempdir().unwrap();
        let idle = dir.path().join("idle.mp3");
        let recent = dir.path().join("recent.mp3");
        let stale = dir.path().join("x.1-0.partial.m4a");
        let fresh = dir.path().join("y.1-1.partial.m4a");
        for path in [&idle, &recent, &stale, &fresh] {
            std::fs::write(path, b"x").unwrap();
        }
        age(&idle, 0);
        age(&stale, 0);

        let cache = Cache::with_disk(dir.path().to_path_buf(), disk_with_room(1_000));
        cache.get_or_insert("new", "mp3", write(1)).unwrap();
        assert!(!idle.exists());
        assert!(!stale.exists());
        assert!(recent.exists());
        assert!(fresh.exists());
    }

    #[test]
    fn without_free_space_the_fallback_cap_bounds_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::with_disk(dir.path().to_path_buf(), Box::new(|_| None));
        let a = cache.get_or_insert("a", "mp3", write(10)).unwrap();
        let b = cache.get_or_insert("b", "mp3", write(10)).unwrap();
        assert!(a.exists() && b.exists(), "far below the cap");
    }

    #[test]
    fn keys_change_with_the_file_and_the_decision() {
        let info = SourceInfo {
            path: "a.flac".into(),
            stamp: crate::source::Stamp {
                size: 1,
                mtime_ns: 1,
            },
            codec: "flac",
            sample_rate: 44_100,
            bit_depth: Some(16),
            channels: 2,
            bitrate_kbps: None,
            duration: Duration::from_secs(1),
            art: Default::default(),
        };
        let shrink = Decision::ShrinkArt { max_px: 500 };
        let base = key(&info, &shrink);
        assert_eq!(base.len(), 32);
        assert_eq!(key(&info, &shrink), base);
        assert_ne!(key(&info, &Decision::ShrinkArt { max_px: 800 }), base);
        let mut touched = info.clone();
        touched.stamp.mtime_ns = 2;
        assert_ne!(key(&touched, &shrink), base);
    }
}
