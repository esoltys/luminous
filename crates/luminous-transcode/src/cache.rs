//! The host-side transcode cache: one immutable file per prepared source,
//! named by a key that changes whenever its content would. There is no
//! index; the directory is the index and each file's mtime is when it was
//! last used.

use crate::rule::Decision;
use crate::source::SourceInfo;
use crate::Error;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// Bump when the pipeline would produce different output for the same
/// source and decision, so stale entries stop matching.
const PIPELINE_VERSION: u32 = 1;

/// Marks files still being written. A leftover from a crash is removed once
/// it is older than any conversion takes.
const PARTIAL: &str = ".partial";
const STALE_PARTIAL: Duration = Duration::from_secs(60 * 60);

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
    cap_bytes: AtomicU64,
    /// Serializes eviction scans; reads and writes of entries need no lock.
    evicting: Mutex<()>,
    partial_seq: AtomicU64,
}

impl Cache {
    pub fn new(dir: PathBuf, cap_bytes: u64) -> Self {
        Self {
            dir,
            cap_bytes: AtomicU64::new(cap_bytes),
            evicting: Mutex::new(()),
            partial_seq: AtomicU64::new(0),
        }
    }

    pub fn set_cap(&self, bytes: u64) {
        self.cap_bytes.store(bytes, Ordering::Relaxed);
        self.evict(None);
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
        self.evict(Some(&path));
        Ok(path)
    }

    /// Remove least-recently-used entries until the cache fits its cap,
    /// sparing `keep`, plus partial files left by a crash.
    fn evict(&self, keep: Option<&Path>) {
        let _guard = self.evicting.lock();
        let Ok(dir) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let now = SystemTime::now();
        let mut entries = Vec::new();
        let mut total = 0u64;
        for entry in dir.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let path = entry.path();
            let used = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if path.to_string_lossy().contains(PARTIAL) {
                if now.duration_since(used).unwrap_or_default() > STALE_PARTIAL {
                    let _ = std::fs::remove_file(&path);
                }
                continue;
            }
            total += meta.len();
            entries.push((used, meta.len(), path));
        }
        let cap = self.cap_bytes.load(Ordering::Relaxed);
        entries.sort_by_key(|(used, ..)| *used);
        for (_, len, path) in entries {
            if total <= cap {
                break;
            }
            if Some(path.as_path()) == keep {
                continue;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= len;
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

    fn age(path: &Path, secs: u64) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs))
            .unwrap();
    }

    #[test]
    fn a_second_request_is_a_hit() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().join("t"), 1_000);
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
        let cache = Cache::new(dir.path().to_path_buf(), 1_000);
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
    fn eviction_drops_least_recently_used_first() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().to_path_buf(), 25);
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
    fn the_new_entry_survives_a_cap_smaller_than_itself() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().to_path_buf(), 5);
        let a = cache.get_or_insert("a", "mp3", write(10)).unwrap();
        assert!(a.exists());
        cache.set_cap(0);
        assert!(!a.exists());
    }

    #[test]
    fn stale_partials_are_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().to_path_buf(), 1_000);
        let stale = dir.path().join("x.1-0.partial.m4a");
        let fresh = dir.path().join("y.1-1.partial.m4a");
        std::fs::write(&stale, b"x").unwrap();
        std::fs::write(&fresh, b"y").unwrap();
        age(&stale, 0);
        cache.set_cap(1_000);
        assert!(!stale.exists());
        assert!(fresh.exists());
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
