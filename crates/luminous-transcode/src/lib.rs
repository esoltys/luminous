//! Converting library files for portable players (#838, epic #834).
//!
//! A [`Transcoder`] answers the questions device sync asks of each file: what
//! will happen to it on the way to the device ([`Transcoder::decide`]), how
//! big it will be there ([`Transcoder::estimated_size`]), and which file to
//! copy ([`Transcoder::materialize`]). The rule for "can't play" reads the
//! device's [`DeviceProfile`]; conversions and art-only rewrites are kept in a
//! host-side cache so the next sync doesn't encode them again.
//!
//! Opus is encoded everywhere with the vendored libopus. AAC and MP3 use
//! Media Foundation on Windows and an `ffmpeg` binary on `PATH` elsewhere,
//! when there is one; [`Capabilities`] says what this computer can do.

mod art;
mod cache;
mod capabilities;
mod encode;
mod pipeline;
mod rule;
mod source;

pub use capabilities::{Capabilities, FormatCapability};
pub use luminous_devices::{ConvertMode, ConvertSettings, DeviceProfile, TargetFormat};
pub use rule::Decision;

use parking_lot::Mutex;
use source::SourceInfo;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

/// Why a file could not be prepared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The caller's cancel flag was raised. Nothing was cached.
    Cancelled,
    /// The source couldn't be read or decoded, or the encoder failed. The
    /// message is for logs and the sync report.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Failed(e.to_string())
    }
}

/// A file ready to copy to the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The source itself for [`Decision::Copy`], otherwise a cache entry.
    /// Entries are never modified, but a later `materialize` can evict the
    /// least recently used ones to stay under the cap, so copy it before
    /// preparing much more: an entry is only sure to exist until then.
    pub path: PathBuf,
    pub decision: Decision,
    /// Extension the file should have on the device, without the dot.
    pub extension: String,
    pub size: u64,
}

/// Prepares library files for devices. Cheap to share between threads; one
/// per app.
pub struct Transcoder {
    cache: cache::Cache,
    /// Detected on first use: probing encoders can start a process.
    capabilities: OnceLock<Capabilities>,
    /// Probed sources, keyed by path and checked against size and mtime, so
    /// a live sync preview over thousands of files only reads tags once.
    sources: Mutex<HashMap<PathBuf, Arc<SourceInfo>>>,
}

impl Transcoder {
    /// A transcoder caching into `cache_dir` (created when first needed),
    /// keeping at most `cache_cap_bytes` there. Cheap: nothing is read or
    /// probed until first used.
    pub fn new(cache_dir: PathBuf, cache_cap_bytes: u64) -> Self {
        Self {
            cache: cache::Cache::new(cache_dir, cache_cap_bytes),
            capabilities: OnceLock::new(),
            sources: Mutex::new(HashMap::new()),
        }
    }

    /// The formats and bitrates conversions can use on this computer.
    /// Blocks on first call while the encoders are probed.
    pub fn capabilities(&self) -> &Capabilities {
        self.capabilities.get_or_init(Capabilities::detect)
    }

    /// Change the cache's size cap, evicting least-recently-used entries
    /// now if it is over.
    pub fn set_cache_cap(&self, bytes: u64) {
        self.cache.set_cap(bytes);
    }

    /// What sync does with `source` for a device with `profile`. Reads the
    /// file's tags (once per change to the file).
    pub fn decide(&self, source: &Path, profile: &DeviceProfile) -> Result<Decision, Error> {
        let info = self.source_info(source)?;
        Ok(rule::decide(&info, profile, self.capabilities()))
    }

    /// The size `source` will have on the device: exact once it has been
    /// prepared, otherwise an estimate from its duration and the target
    /// bitrate. Never encodes.
    pub fn estimated_size(&self, source: &Path, profile: &DeviceProfile) -> Result<u64, Error> {
        let info = self.source_info(source)?;
        let decision = rule::decide(&info, profile, self.capabilities());
        if decision == Decision::Copy {
            return Ok(info.stamp.size);
        }
        let key = cache::key(&info, &decision);
        Ok(self
            .cache
            .size_of(&key, &decision.extension(source))
            .unwrap_or_else(|| rule::estimate(&info, &decision)))
    }

    /// The file to copy to the device for `source`: the source itself, or its
    /// converted or art-shrunk copy from the cache, made now if it isn't
    /// there. Blocks while encoding; check `cancel` is raised to stop early.
    pub fn materialize(
        &self,
        source: &Path,
        profile: &DeviceProfile,
        cancel: &AtomicBool,
    ) -> Result<Prepared, Error> {
        let info = self.source_info(source)?;
        let decision = rule::decide(&info, profile, self.capabilities());
        let extension = decision.extension(source);
        if decision == Decision::Copy {
            return Ok(Prepared {
                path: source.to_path_buf(),
                decision,
                extension,
                size: info.stamp.size,
            });
        }
        let key = cache::key(&info, &decision);
        let path = self.cache.get_or_insert(&key, &extension, |out| {
            pipeline::produce(&info, &decision, out, cancel)
        })?;
        let size = std::fs::metadata(&path)?.len();
        Ok(Prepared {
            path,
            decision,
            extension,
            size,
        })
    }

    fn source_info(&self, path: &Path) -> Result<Arc<SourceInfo>, Error> {
        let stamp = source::Stamp::of(path)?;
        if let Some(info) = self.sources.lock().get(path) {
            if info.stamp == stamp {
                return Ok(info.clone());
            }
        }
        let info = Arc::new(SourceInfo::probe(path, stamp)?);
        self.sources.lock().insert(path.to_path_buf(), info.clone());
        Ok(info)
    }
}
