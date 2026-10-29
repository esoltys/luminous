//! Planning a sync: from the songs wanted on a device, what is on it now and
//! its manifest, to the [`SyncPlan`] the preview shows and the [`Steps`] the
//! worker runs. Pure: it reads no files and no database, so the live preview
//! can re-plan on every change and tests need neither.

use super::manifest::{EntryKind, Manifest, ManifestEntry, SongLabel, SourceStamp};
use super::selection::{SourceRef, UnselectedSongs, WantedPlaylist, WantedSong};
use crate::devices::layout::{DeviceLayout, DevicePlaylist, SyncedTrack};
use crate::models::Song;
use luminous_devices::{DeviceProfile, Space};
use luminous_transcode::{ConversionTarget, Decision};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What sync assumes a folder-art file takes before it is written: art is
/// encoded only when written, so planning stays cheap.
const FOLDER_ART_ESTIMATE_BYTES: u64 = 150_000;
/// How many layouts to try while placing songs beside files the user put at
/// their paths. Each pass only adds occupied paths, and real devices settle
/// in one or two.
const MAX_LAYOUT_PASSES: usize = 8;

/// What sync found on the storage: the files under the folders it syncs to,
/// and its manifest.
#[derive(Debug, Clone)]
pub struct Inventory {
    /// Lower-case path relative to the storage root to size in bytes.
    pub files: HashMap<String, u64>,
    /// The folders sync writes into, `/`-separated, `""` for the whole
    /// storage. Everything under them was listed; files outside them are not
    /// known and assumed present. Never pruned when they empty.
    pub roots: Vec<String>,
    pub manifest: Manifest,
    pub space: Space,
}

impl Inventory {
    /// The size of the file at `path`, `None` when there is none. A path
    /// outside the listed folders is assumed present at `fallback` bytes:
    /// right for a file sync wrote, which this is for.
    fn size_at(&self, path: &str, fallback: u64) -> Option<u64> {
        if self.is_listed(path) {
            self.known_size(path)
        } else {
            Some(fallback)
        }
    }

    /// The size of a file found at `path`; `None` when there is none or its
    /// folder wasn't listed.
    fn known_size(&self, path: &str) -> Option<u64> {
        self.files.get(&path.to_lowercase()).copied()
    }

    fn is_listed(&self, path: &str) -> bool {
        let lower = path.to_lowercase();
        self.roots
            .iter()
            .any(|dir| dir.is_empty() || lower.starts_with(&format!("{}/", dir.to_lowercase())))
    }
}

/// A song wanted on the device, as it will be written there.
#[derive(Debug, Clone)]
pub struct TrackInput {
    pub wanted: WantedSong,
    pub stamp: SourceStamp,
    /// How it is prepared for the device.
    pub decision: Decision,
    /// Bytes on the device: exact once prepared, else estimated.
    pub size: u64,
    /// Bytes if everything that can be were converted to save space; `None`
    /// when that is already the Convert setting.
    pub size_converting_all: Option<u64>,
}

impl TrackInput {
    /// Extension on the device, without the dot.
    fn extension(&self) -> String {
        self.decision.extension(Path::new(&self.stamp.path))
    }

    fn converts(&self) -> bool {
        matches!(self.decision, Decision::Convert { .. })
    }
}

/// Library files a manifest entry may refer to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryFile {
    Available,
    /// In the library but can't be read now (its drive is offline). Its
    /// copy on the device is left alone, never removed.
    Unavailable,
}

pub struct PlanInput<'a> {
    pub profile: &'a DeviceProfile,
    pub unselected: UnselectedSongs,
    pub tracks: Vec<TrackInput>,
    /// Songs the selection includes that couldn't be read to plan them.
    pub unreadable: Vec<Song>,
    pub playlists: &'a [WantedPlaylist],
    pub sources: &'a [SourceRef],
    pub inventory: &'a Inventory,
    /// Every local library file by path.
    pub library: &'a HashMap<String, LibraryFile>,
    /// The format and bitrate conversions use on this computer.
    pub conversion_target: ConversionTarget,
}

// --- The plan the UI shows -------------------------------------------------

/// Why a song is in one of the plan's lists. Codes with parameters, so the
/// UI words them in the user's language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SyncReason {
    /// Copied because this source includes it.
    Selected {
        source: SourceRef,
    },
    /// Copied beside a file the user put where it would go, which stays.
    RenamedBesideExisting {
        source: SourceRef,
        existing_path: String,
    },
    /// A file the user put at its path is the same song, so it is counted as
    /// synced instead of copied again.
    Adopted,
    /// Removed: it left this source, which is still selected.
    LeftSource {
        source: SourceRef,
    },
    /// Removed: the source that brought it in was unselected.
    Unselected {
        source: Option<SourceRef>,
    },
    /// Removed: it is no longer in the library.
    DeletedFromLibrary,
    TagsChanged,
    FileChanged,
    ConvertChanged,
    /// Its place on the device changed (the folder template or format).
    Moved {
        from: String,
    },
    /// Its file can't be read, so it is not copied.
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncItem {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub title: Option<String>,
    /// Where it is (or goes) on the device, relative to the storage root.
    pub path: String,
    pub bytes: u64,
    pub reason: SyncReason,
}

/// One of the plan's lists.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeList {
    pub songs: usize,
    pub bytes: u64,
    /// Every item in the stored plan; the first few in a preview (see
    /// [`SyncPlan::preview`]).
    pub items: Vec<SyncItem>,
}

impl ChangeList {
    fn push(&mut self, item: SyncItem) {
        self.songs += 1;
        self.bytes += item.bytes;
        self.items.push(item);
    }
}

/// The storage now and after the sync, for the Now and After bars: Now is
/// `stays + removed + other` used, After is `stays + copied + other`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageProjection {
    pub capacity_bytes: u64,
    /// "Kept free for the device".
    pub reserve_bytes: u64,
    /// Everything sync didn't write.
    pub other_bytes: u64,
    /// Synced files that stay as they are.
    pub stays_bytes: u64,
    /// Files removed, including the old copies of updated songs.
    pub removed_bytes: u64,
    /// Files written: copies, updates, playlist files and folder art.
    pub copied_bytes: u64,
    pub free_bytes: u64,
    /// Negative when the selection runs past the storage's capacity.
    pub free_after_bytes: i64,
    /// Whether `free_after_bytes` keeps the reserve.
    pub fits: bool,
}

/// The size a selected source takes on the device after converting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSize {
    pub source: SourceRef,
    pub songs: usize,
    pub bytes: u64,
}

/// How far a selection that doesn't fit is over, and what would help.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortfall {
    pub bytes: u64,
    /// The selected source that takes the most space.
    pub largest_source: Option<SourceSize>,
    /// About how much converting everything to save space would free;
    /// `None` when that is the Convert setting already or saves nothing.
    pub convert_all_saving_bytes: Option<u64>,
}

/// Everything the sync preview shows. The UI displays these values and
/// never recomputes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    /// Pass to `start_device_sync` to run exactly this plan.
    pub plan_id: String,
    pub device_id: String,
    pub storage_id: String,
    pub storage: StorageProjection,
    pub copy: ChangeList,
    pub remove: ChangeList,
    pub update: ChangeList,
    pub adopted: ChangeList,
    /// Selected songs that can't be read and are left out.
    pub skipped: ChangeList,
    /// Copies and updates that are converted.
    pub converted_songs: usize,
    pub playlist_files_written: usize,
    pub playlist_files_removed: usize,
    pub folder_art_written: usize,
    /// The format and bitrate conversions use ("AAC 256"), whenever any
    /// copy is converted or converting everything is offered as the fix.
    /// When `substituted` is set or `device_plays` isn't, the preview warns
    /// before Start sync; the sync converts with exactly this.
    pub conversion_target: Option<ConversionTarget>,
    pub sources: Vec<SourceSize>,
    pub shortfall: Option<Shortfall>,
}

/// How many items of each list a preview carries.
pub const PREVIEW_ITEMS: usize = 50;

impl SyncPlan {
    /// This plan with at most [`PREVIEW_ITEMS`] items per list (the counts
    /// and bytes still cover them all).
    pub fn preview(&self) -> Self {
        let mut plan = self.clone();
        for list in [
            &mut plan.copy,
            &mut plan.remove,
            &mut plan.update,
            &mut plan.adopted,
            &mut plan.skipped,
        ] {
            list.items.truncate(PREVIEW_ITEMS);
        }
        plan
    }
}

// --- The steps the worker runs -------------------------------------------------

/// What to write at a path on the device.
#[derive(Debug, Clone)]
pub enum WriteSource {
    /// A library file, converted or not as the profile says.
    Track { source: PathBuf, converts: bool },
    /// A playlist file, rendered when it is written from the songs then on
    /// the device: a song that failed to copy is left out of it.
    Playlist {
        layout: Arc<DeviceLayout>,
        playlist: WantedPlaylist,
    },
    /// Folder art taken from this album track.
    FolderArt { album_track: PathBuf },
}

#[derive(Debug, Clone)]
pub struct Write {
    pub source: WriteSource,
    /// Recorded in the manifest once written, with its real size.
    pub entry: ManifestEntry,
    /// Estimated bytes, for progress.
    pub estimate: u64,
}

/// The work a plan stands for, in the order the worker does it.
#[derive(Debug, Clone, Default)]
pub struct Steps {
    /// Manifest changes that need no writing: adopted files and synced
    /// songs now brought in by other sources.
    pub record: Vec<ManifestEntry>,
    /// Paths to remove first, so the writes have room.
    pub remove: Vec<String>,
    /// Song writes (updates, then copies), then playlist files and art.
    pub write: Vec<Write>,
    /// [`Inventory::roots`], never pruned when they empty.
    pub roots: Vec<String>,
}

impl Steps {
    pub fn song_writes(&self) -> usize {
        self.write
            .iter()
            .filter(|w| matches!(w.source, WriteSource::Track { .. }))
            .count()
    }
}

/// A plan before it is given an id.
pub struct Built {
    pub plan: SyncPlan,
    pub steps: Steps,
}

/// Plans a sync. See the module docs.
pub fn build(input: PlanInput) -> Built {
    let PlanInput {
        profile,
        unselected,
        tracks,
        unreadable,
        playlists,
        sources,
        inventory,
        library,
        conversion_target,
    } = input;
    let manifest = &inventory.manifest;
    let mut plan = SyncPlan {
        plan_id: String::new(),
        device_id: String::new(),
        storage_id: String::new(),
        storage: StorageProjection::default(),
        copy: ChangeList::default(),
        remove: ChangeList::default(),
        update: ChangeList::default(),
        adopted: ChangeList::default(),
        skipped: ChangeList::default(),
        converted_songs: 0,
        playlist_files_written: 0,
        playlist_files_removed: 0,
        folder_art_written: 0,
        conversion_target: None,
        sources: Vec::new(),
        shortfall: None,
    };
    let mut steps = Steps {
        roots: inventory.roots.clone(),
        ..Steps::default()
    };
    let mut song_updates = Vec::new();
    let mut song_copies = Vec::new();
    // Manifest paths (lower-case) that stay or are replaced in place.
    let mut kept: HashSet<String> = HashSet::new();
    let mut stays_bytes = 0u64;
    let mut adopted_bytes = 0u64;
    let mut convert_all_saving = 0u64;

    // Selected but unreadable: not copied, and a copy already on the
    // device is left alone rather than counted as unselected.
    let unreadable_paths: HashSet<&str> = unreadable
        .iter()
        .filter_map(|s| s.path.as_deref())
        .collect();
    for song in &unreadable {
        plan.skipped
            .push(item(song, String::new(), 0, SyncReason::Unreadable));
    }

    let (layout, renamed_from) = place_tracks(profile, &tracks, inventory);
    let layout = Arc::new(layout);
    let previous = previous_entries(manifest);

    for track in &tracks {
        let song = &track.wanted.song;
        let Some(path) = layout.track_path(song.id).map(str::to_string) else {
            continue;
        };
        let reason_source = track.wanted.sources[0].clone();
        let entry = ManifestEntry {
            path: path.clone(),
            kind: EntryKind::Track,
            size: track.size,
            source: Some(track.stamp.clone()),
            conversion: Some(track.decision),
            fingerprint: Some(tag_fingerprint(song)),
            sources: track.wanted.sources.clone(),
            label: label(song),
        };
        let write = Write {
            source: WriteSource::Track {
                source: PathBuf::from(&track.stamp.path),
                converts: track.converts(),
            },
            entry: entry.clone(),
            estimate: track.size,
        };
        let candidates = previous.get(&track.stamp.path).cloned().unwrap_or_default();
        let at_path = candidates
            .iter()
            .find(|e| e.path.eq_ignore_ascii_case(&path))
            .copied();

        if let Some(prev) = at_path {
            kept.insert(prev.path.to_lowercase());
            let present = inventory.size_at(&prev.path, prev.size).is_some();
            let reason = if !present {
                None
            } else if !same_file(prev, &track.stamp) {
                Some(if prev.fingerprint == entry.fingerprint {
                    SyncReason::FileChanged
                } else {
                    SyncReason::TagsChanged
                })
            } else if prev.conversion != entry.conversion {
                Some(SyncReason::ConvertChanged)
            } else {
                stays_bytes += prev.size;
                if prev.sources != entry.sources || prev.label != entry.label {
                    steps.record.push(ManifestEntry {
                        size: prev.size,
                        conversion: prev.conversion,
                        ..entry
                    });
                }
                continue;
            };
            match reason {
                Some(reason) => {
                    plan.storage.removed_bytes += prev.size;
                    plan.update
                        .push(item(song, path.clone(), track.size, reason));
                    song_updates.push(write);
                }
                None => {
                    plan.copy.push(item(
                        song,
                        path.clone(),
                        track.size,
                        SyncReason::Selected {
                            source: reason_source,
                        },
                    ));
                    song_copies.push(write);
                }
            }
        } else if let Some(prev) = candidates.first() {
            // Synced before under another path: write it at the new one and
            // remove the old copy first.
            kept.insert(prev.path.to_lowercase());
            if inventory.size_at(&prev.path, prev.size).is_some() {
                plan.storage.removed_bytes += prev.size;
                steps.remove.push(prev.path.clone());
            }
            plan.update.push(item(
                song,
                path.clone(),
                track.size,
                SyncReason::Moved {
                    from: prev.path.clone(),
                },
            ));
            song_updates.push(write);
        } else if let Some(size) = inventory
            .known_size(&path)
            .filter(|_| manifest.get(&path).is_none())
        {
            // `place_tracks` only leaves a song on a file the user put there
            // when that file is the same song.
            adopted_bytes += size;
            stays_bytes += size;
            plan.adopted
                .push(item(song, path.clone(), size, SyncReason::Adopted));
            steps.record.push(ManifestEntry { size, ..entry });
            continue;
        } else {
            let reason = match renamed_from.get(&song.id) {
                Some(existing) => SyncReason::RenamedBesideExisting {
                    source: reason_source,
                    existing_path: existing.clone(),
                },
                None => SyncReason::Selected {
                    source: reason_source,
                },
            };
            plan.copy.push(item(song, path.clone(), track.size, reason));
            song_copies.push(write);
        }
        // Reached only for songs that are written.
        if track.converts() {
            plan.converted_songs += 1;
        }
        if let Some(converted) = track.size_converting_all {
            convert_all_saving += track.size.saturating_sub(converted);
        }
    }

    // Synced songs nothing selects any more.
    let selected: HashSet<(_, _)> = sources.iter().map(|s| (s.kind, &s.key)).collect();
    for prev in manifest.entries().filter(|e| e.kind == EntryKind::Track) {
        if kept.contains(&prev.path.to_lowercase()) {
            continue;
        }
        let Some(present_size) = inventory.size_at(&prev.path, prev.size) else {
            continue;
        };
        let in_library = prev
            .source
            .as_ref()
            .and_then(|s| library.get(&s.path).copied());
        let unreadable = prev
            .source
            .as_ref()
            .is_some_and(|s| unreadable_paths.contains(s.path.as_str()));
        if unselected == UnselectedSongs::Leave
            || unreadable
            || in_library == Some(LibraryFile::Unavailable)
        {
            stays_bytes += present_size;
            continue;
        }
        let reason = if in_library.is_none() {
            SyncReason::DeletedFromLibrary
        } else if let Some(still) = prev
            .sources
            .iter()
            .find(|s| selected.contains(&(s.kind, &s.key)))
        {
            SyncReason::LeftSource {
                source: still.clone(),
            }
        } else {
            SyncReason::Unselected {
                source: prev.sources.first().cloned(),
            }
        };
        plan.storage.removed_bytes += present_size;
        plan.remove.push(SyncItem {
            artist: prev.label.artist.clone(),
            album: prev.label.album.clone(),
            title: prev.label.title.clone(),
            path: prev.path.clone(),
            bytes: present_size,
            reason,
        });
        steps.remove.push(prev.path.clone());
    }

    let mut writes: Vec<Write> = song_updates.into_iter().chain(song_copies).collect();

    // Playlist files and folder art: only songs that end up on the device.
    let on_device: HashSet<i64> = tracks.iter().map(|t| t.wanted.song.id).collect();
    let mut extra = Vec::new();
    let mut extra_kept: HashSet<String> = HashSet::new();
    plan_playlists(
        &layout,
        playlists,
        &on_device,
        inventory,
        &mut extra,
        &mut extra_kept,
    );
    plan_folder_art(&layout, &tracks, inventory, &mut extra, &mut extra_kept);
    for prev in manifest.entries().filter(|e| e.kind != EntryKind::Track) {
        let Some(size) = inventory.size_at(&prev.path, prev.size) else {
            continue;
        };
        let lower = prev.path.to_lowercase();
        let rewritten = extra.iter().any(|w| w.entry.path.to_lowercase() == lower);
        if rewritten {
            plan.storage.removed_bytes += size;
        } else if extra_kept.contains(&lower) || unselected == UnselectedSongs::Leave {
            stays_bytes += size;
        } else {
            plan.storage.removed_bytes += size;
            if prev.kind == EntryKind::Playlist {
                plan.playlist_files_removed += 1;
            }
            steps.remove.push(prev.path.clone());
        }
    }
    for write in &extra {
        match write.entry.kind {
            EntryKind::Playlist => plan.playlist_files_written += 1,
            _ => plan.folder_art_written += 1,
        }
    }
    writes.extend(extra);

    // Space.
    let space = inventory.space;
    let manifest_present: u64 = manifest
        .entries()
        .filter_map(|e| inventory.size_at(&e.path, e.size))
        .sum();
    let used = space.total_bytes.saturating_sub(space.free_bytes);
    plan.storage.capacity_bytes = space.total_bytes;
    plan.storage.free_bytes = space.free_bytes;
    plan.storage.reserve_bytes = profile.reserve_bytes;
    plan.storage.other_bytes = used.saturating_sub(manifest_present + adopted_bytes);
    plan.storage.stays_bytes = stays_bytes;
    plan.storage.copied_bytes = writes.iter().map(|w| w.estimate).sum();
    plan.storage.free_after_bytes = clamp_i64(
        space.free_bytes as i128 + plan.storage.removed_bytes as i128
            - plan.storage.copied_bytes as i128,
    );
    plan.storage.fits = plan.storage.free_after_bytes as i128 >= profile.reserve_bytes as i128;

    plan.sources = source_sizes(sources, &tracks);
    if !plan.storage.fits {
        plan.shortfall = Some(Shortfall {
            bytes: (profile.reserve_bytes as i128 - plan.storage.free_after_bytes as i128).max(0)
                as u64,
            largest_source: plan.sources.iter().max_by_key(|s| s.bytes).cloned(),
            convert_all_saving_bytes: (convert_all_saving > 0).then_some(convert_all_saving),
        });
    }
    let convert_all_offered = plan
        .shortfall
        .as_ref()
        .is_some_and(|s| s.convert_all_saving_bytes.is_some());
    if plan.converted_songs > 0 || convert_all_offered {
        plan.conversion_target = Some(conversion_target);
    }

    steps.write = writes;
    Built { plan, steps }
}

/// Lays the tracks out, moving any that would land on a file sync didn't
/// write (and that isn't the same song) to a " (n)" name beside it. Returns
/// the layout and, for each moved song, the path of the file it avoided.
fn place_tracks(
    profile: &DeviceProfile,
    tracks: &[TrackInput],
    inventory: &Inventory,
) -> (DeviceLayout, HashMap<i64, String>) {
    let extensions: Vec<String> = tracks.iter().map(TrackInput::extension).collect();
    let synced: Vec<SyncedTrack> = tracks
        .iter()
        .zip(&extensions)
        .map(|(t, extension)| SyncedTrack {
            song: &t.wanted.song,
            extension,
        })
        .collect();
    let first = DeviceLayout::plan(profile, &synced);
    let mut occupied: HashSet<String> = HashSet::new();
    let mut layout = None;
    for _ in 0..MAX_LAYOUT_PASSES {
        let current = layout
            .take()
            .unwrap_or_else(|| DeviceLayout::plan_avoiding(profile, &synced, &occupied));
        let before = occupied.len();
        for track in tracks {
            let Some(path) = current.track_path(track.wanted.song.id) else {
                continue;
            };
            if inventory.manifest.get(path).is_some() {
                continue;
            }
            let Some(size) = inventory.known_size(path) else {
                continue;
            };
            if !is_same_song(track, size) {
                occupied.insert(path.to_lowercase());
            }
        }
        if occupied.len() == before {
            layout = Some(current);
            break;
        }
    }
    let layout = layout.unwrap_or_else(|| DeviceLayout::plan_avoiding(profile, &synced, &occupied));
    let renamed = tracks
        .iter()
        .filter_map(|t| {
            let id = t.wanted.song.id;
            let original = first.track_path(id)?;
            (layout.track_path(id)? != original && occupied.contains(&original.to_lowercase()))
                .then(|| (id, original.to_string()))
        })
        .collect();
    (layout, renamed)
}

/// Whether a file of `size` bytes the user put at a track's path is that
/// song as sync would write it: only an unconverted copy can be told by size.
fn is_same_song(track: &TrackInput, size: u64) -> bool {
    !track.converts() && size == track.size
}

/// Manifest tracks by the library file they came from.
fn previous_entries(manifest: &Manifest) -> HashMap<String, Vec<&ManifestEntry>> {
    let mut by_source: HashMap<String, Vec<&ManifestEntry>> = HashMap::new();
    for entry in manifest.entries().filter(|e| e.kind == EntryKind::Track) {
        if let Some(source) = &entry.source {
            by_source
                .entry(source.path.clone())
                .or_default()
                .push(entry);
        }
    }
    by_source
}

fn same_file(entry: &ManifestEntry, stamp: &SourceStamp) -> bool {
    entry
        .source
        .as_ref()
        .is_some_and(|s| s.size == stamp.size && s.mtime == stamp.mtime)
}

fn plan_playlists(
    layout: &Arc<DeviceLayout>,
    playlists: &[WantedPlaylist],
    on_device: &HashSet<i64>,
    inventory: &Inventory,
    writes: &mut Vec<Write>,
    kept: &mut HashSet<String>,
) {
    let songs: Vec<Vec<Song>> = playlists
        .iter()
        .map(|p| {
            p.songs
                .iter()
                .filter(|s| on_device.contains(&s.id))
                .cloned()
                .collect()
        })
        .collect();
    let device_playlists: Vec<DevicePlaylist> = playlists
        .iter()
        .zip(&songs)
        .map(|(p, songs)| DevicePlaylist {
            name: &p.name,
            songs,
        })
        .collect();
    // Like songs, a playlist never replaces a file the user put there.
    let mut occupied = HashSet::new();
    let mut files = layout.playlist_files_avoiding(&device_playlists, &occupied);
    for _ in 0..MAX_LAYOUT_PASSES {
        let before = occupied.len();
        for file in &files {
            if inventory.manifest.get(&file.path).is_none()
                && inventory.known_size(&file.path).is_some()
            {
                occupied.insert(file.path.to_lowercase());
            }
        }
        if occupied.len() == before {
            break;
        }
        files = layout.playlist_files_avoiding(&device_playlists, &occupied);
    }
    for ((file, playlist), songs) in files.into_iter().zip(playlists).zip(songs) {
        let fingerprint = content_fingerprint(&file.content);
        let prev = inventory.manifest.get(&file.path);
        let unchanged = prev.is_some_and(|p| {
            p.fingerprint.as_deref() == Some(fingerprint.as_str())
                && inventory.size_at(&p.path, p.size).is_some()
        });
        if unchanged {
            kept.insert(file.path.to_lowercase());
            continue;
        }
        let size = file.content.len() as u64;
        writes.push(Write {
            entry: ManifestEntry {
                path: file.path,
                kind: EntryKind::Playlist,
                size,
                source: None,
                conversion: None,
                fingerprint: Some(fingerprint),
                sources: Vec::new(),
                label: SongLabel::default(),
            },
            source: WriteSource::Playlist {
                layout: layout.clone(),
                playlist: WantedPlaylist {
                    name: playlist.name.clone(),
                    songs,
                },
            },
            estimate: size,
        });
    }
}

fn plan_folder_art(
    layout: &DeviceLayout,
    tracks: &[TrackInput],
    inventory: &Inventory,
    writes: &mut Vec<Write>,
    kept: &mut HashSet<String>,
) {
    let stamps: HashMap<&str, &SourceStamp> = tracks
        .iter()
        .map(|t| (t.stamp.path.as_str(), &t.stamp))
        .collect();
    for art in layout.folder_art() {
        let album_track = art.album_track.to_string_lossy().into_owned();
        let Some(stamp) = stamps.get(album_track.as_str()) else {
            continue;
        };
        match inventory.manifest.get(&art.path) {
            Some(prev) if inventory.size_at(&prev.path, prev.size).is_some() => {
                if prev.source.as_ref() == Some(*stamp) {
                    kept.insert(art.path.to_lowercase());
                    continue;
                }
            }
            // Art the user put in the folder is theirs, and as good.
            None if inventory.known_size(&art.path).is_some() => continue,
            _ => {}
        }
        writes.push(Write {
            entry: ManifestEntry {
                path: art.path.clone(),
                kind: EntryKind::FolderArt,
                size: FOLDER_ART_ESTIMATE_BYTES,
                source: Some((*stamp).clone()),
                conversion: None,
                fingerprint: None,
                sources: Vec::new(),
                label: SongLabel::default(),
            },
            source: WriteSource::FolderArt {
                album_track: art.album_track.clone(),
            },
            estimate: FOLDER_ART_ESTIMATE_BYTES,
        });
    }
}

fn source_sizes(sources: &[SourceRef], tracks: &[TrackInput]) -> Vec<SourceSize> {
    sources
        .iter()
        .map(|source| {
            let (songs, bytes) = tracks
                .iter()
                .filter(|t| t.wanted.sources.contains(source))
                .fold((0, 0), |(n, b), t| (n + 1, b + t.size));
            SourceSize {
                source: source.clone(),
                songs,
                bytes,
            }
        })
        .collect()
}

fn item(song: &Song, path: String, bytes: u64, reason: SyncReason) -> SyncItem {
    SyncItem {
        artist: song.artist.clone(),
        album: song.album.clone(),
        title: song.title.clone(),
        path,
        bytes,
        reason,
    }
}

fn label(song: &Song) -> SongLabel {
    SongLabel {
        artist: song.artist.clone(),
        album: song.album.clone(),
        title: song.title.clone(),
    }
}

/// Fingerprint of the tags that end up in a synced file.
pub fn tag_fingerprint(song: &Song) -> String {
    let fields = [
        song.title.as_deref(),
        song.artist.as_deref(),
        song.album.as_deref(),
        song.album_artist.as_deref(),
        song.genre.as_deref(),
        song.composer.as_deref(),
    ];
    let numbers = [song.track, song.disc, song.year];
    content_fingerprint(format!("{fields:?}{numbers:?}").as_bytes())
}

pub(super) fn content_fingerprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes)[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn clamp_i64(value: i128) -> i64 {
    value.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
