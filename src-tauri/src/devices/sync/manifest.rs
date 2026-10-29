//! `.luminous-sync.json`: the files sync wrote to a storage, kept on the
//! storage itself.
//!
//! It is what makes "Remove from device" safe: sync only ever removes a file
//! listed here, so anything the user put on the device another way stays.
//! It travels with the device, so a sync from another computer sees (and,
//! in Remove mode, may remove) what this one copied. Entries name library
//! files by path, never by this computer's song ids.

use super::selection::SourceRef;
use luminous_devices::DeviceBackend;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::ControlFlow;

pub const MANIFEST_FILE_NAME: &str = ".luminous-sync.json";
const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    Track,
    Playlist,
    FolderArt,
}

/// A library file as it was when synced: a change in size or modification
/// time means the copy on the device is out of date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStamp {
    pub path: String,
    pub size: u64,
    /// Unix seconds, as the library records it.
    pub mtime: Option<i64>,
}

/// Artist, album and title of a synced song, so a removal can be shown even
/// once the song has left the library.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SongLabel {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub title: Option<String>,
}

/// One file sync wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    /// Relative to the storage root, `/`-separated.
    pub path: String,
    pub kind: EntryKind,
    /// Bytes on the device.
    pub size: u64,
    /// The library file it came from: the song for a track, the album track
    /// the art was taken from for folder art; `None` for playlist files.
    #[serde(default)]
    pub source: Option<SourceStamp>,
    /// How a track was prepared (`luminous_transcode::Decision`), so a
    /// changed Convert setting refreshes it.
    #[serde(default)]
    pub conversion: Option<String>,
    /// Fingerprint of a track's tags, telling "tags changed" from other
    /// changes to the file; of a playlist file's content.
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// The selected sources that brought a track in.
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    #[serde(default)]
    pub label: SongLabel,
}

/// The files sync wrote to one storage, keyed by path ignoring case (as FAT
/// and MTP compare names).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    entries: BTreeMap<String, ManifestEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    files: Vec<ManifestEntry>,
}

impl Manifest {
    /// The manifest on `storage`, empty when there is none. An unreadable one
    /// is treated as empty too, which can only make sync remove less.
    pub fn read(storage: &dyn DeviceBackend) -> std::io::Result<Self> {
        let Some(bytes) = storage.read(MANIFEST_FILE_NAME)? else {
            return Ok(Self::default());
        };
        match serde_json::from_slice::<Wire>(&bytes) {
            Ok(wire) => Ok(wire.files.into_iter().collect()),
            Err(e) => {
                log::warn!("Ignoring an unreadable sync manifest: {e}");
                Ok(Self::default())
            }
        }
    }

    /// Replaces the manifest on `storage`. An unplug mid-write leaves the
    /// previous manifest, never part of one.
    pub fn write(&self, storage: &dyn DeviceBackend) -> std::io::Result<()> {
        let wire = Wire {
            version: MANIFEST_VERSION,
            files: self.entries.values().cloned().collect(),
        };
        let json = serde_json::to_vec(&wire).map_err(std::io::Error::other)?;
        storage.write(
            MANIFEST_FILE_NAME,
            &mut json.as_slice(),
            json.len() as u64,
            &mut |_| ControlFlow::Continue(()),
        )
    }

    pub fn get(&self, path: &str) -> Option<&ManifestEntry> {
        self.entries.get(&path.to_lowercase())
    }

    pub fn insert(&mut self, entry: ManifestEntry) {
        self.entries.insert(entry.path.to_lowercase(), entry);
    }

    pub fn remove(&mut self, path: &str) {
        self.entries.remove(&path.to_lowercase());
    }

    pub fn entries(&self) -> impl Iterator<Item = &ManifestEntry> {
        self.entries.values()
    }
}

impl FromIterator<ManifestEntry> for Manifest {
    fn from_iter<I: IntoIterator<Item = ManifestEntry>>(iter: I) -> Self {
        let mut manifest = Self::default();
        for entry in iter {
            manifest.insert(entry);
        }
        manifest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luminous_devices::FsBackend;

    fn entry(path: &str) -> ManifestEntry {
        ManifestEntry {
            path: path.into(),
            kind: EntryKind::Track,
            size: 10,
            source: None,
            conversion: None,
            fingerprint: None,
            sources: Vec::new(),
            label: SongLabel::default(),
        }
    }

    #[test]
    fn round_trips_through_the_device() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FsBackend::new(dir.path());
        assert_eq!(Manifest::read(&storage).unwrap(), Manifest::default());

        let manifest: Manifest = [entry("Music/A/b.mp3"), entry("Music/c.mp3")]
            .into_iter()
            .collect();
        manifest.write(&storage).unwrap();
        assert_eq!(Manifest::read(&storage).unwrap(), manifest);
    }

    #[test]
    fn paths_match_ignoring_case() {
        let mut manifest = Manifest::default();
        manifest.insert(entry("Music/A/b.mp3"));
        assert!(manifest.get("music/a/B.MP3").is_some());
        manifest.remove("MUSIC/a/b.mp3");
        assert_eq!(manifest.entries().count(), 0);
    }

    #[test]
    fn a_damaged_manifest_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MANIFEST_FILE_NAME), b"{not json").unwrap();
        let manifest = Manifest::read(&FsBackend::new(dir.path())).unwrap();
        assert_eq!(manifest, Manifest::default());
    }
}
