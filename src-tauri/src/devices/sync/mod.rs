//! Syncing a selection of the library to a device (#836, epic #834).
//!
//! The UI previews a sync as the user picks what goes on the device and
//! starts exactly the plan it previewed: [`DeviceSync::preview`] plans and
//! keeps the plan, [`DeviceSync::start`] runs that plan or refuses with
//! `sync-plan-stale` when the device or its profile changed since. So a
//! sync never converts to a format, or touches a file, the preview didn't
//! show.
//!
//! What is on a device is read once and reused by later previews until a
//! sync ends or the storage's free space changes, so a live preview costs a
//! database query and cached size estimates.

pub mod manifest;
pub mod plan;
pub mod selection;
pub mod worker;

use crate::db::Database;
use crate::devices::DeviceMonitor;
use crate::models::LOCAL_SOURCES_SQL;
use anyhow::{anyhow, Result};
use luminous_devices::{ConvertMode, ConvertSettings, DeviceBackend, DeviceProfile};
use luminous_transcode::{Decision, Transcoder};
use manifest::{Manifest, SourceStamp, MANIFEST_FILE_NAME};
use parking_lot::{Condvar, Mutex};
use plan::{Inventory, LibraryFile, PlanInput, Steps, SyncItem, SyncPlan, TrackInput};
use rusqlite::{params, OptionalExtension};
use selection::SyncSettings;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use worker::{Clock, Job, SyncOutcome, SyncProgress, SyncSummary};

pub const SYNC_PROGRESS_EVENT: &str = "device-sync-progress";
pub const SYNC_FINISHED_EVENT: &str = "device-sync-finished";

/// Where a running sync reports to.
pub trait SyncEvents: Send + Sync {
    fn progress(&self, progress: &SyncProgress);
    fn finished(&self, summary: &SyncSummary);
}

/// A device's sync setup as the sync screen opens it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSyncState {
    pub settings: SyncSettings,
    /// What the last sync on this computer did, kept across restarts.
    pub last_summary: Option<SyncSummary>,
    pub running: bool,
}

/// Which of a plan's lists to read in full.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanList {
    Copy,
    Remove,
    Update,
    Adopted,
    Skipped,
}

struct StoredPlan {
    plan: SyncPlan,
    steps: Steps,
    profile: DeviceProfile,
    inventory: Arc<CachedInventory>,
}

struct CachedInventory {
    inventory: Inventory,
    /// What the storage looked like when it was read: it is read again when
    /// any of these change.
    storage_id: String,
    total_bytes: u64,
    free_bytes: u64,
    roots: Vec<String>,
}

/// Plans and runs device syncs. One per app.
pub struct DeviceSync {
    /// The latest plan per device id.
    plans: Mutex<HashMap<String, Arc<StoredPlan>>>,
    /// Cancel flags of running syncs by device id.
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
    sync_ended: Condvar,
    inventories: Mutex<HashMap<String, Arc<CachedInventory>>>,
}

impl Default for DeviceSync {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceSync {
    pub fn new() -> Self {
        Self {
            plans: Mutex::new(HashMap::new()),
            running: Mutex::new(HashMap::new()),
            sync_ended: Condvar::new(),
            inventories: Mutex::new(HashMap::new()),
        }
    }

    pub fn state(&self, conn: &rusqlite::Connection, device_id: &str) -> Result<DeviceSyncState> {
        let last_summary: Option<String> = conn
            .query_row(
                "SELECT last_summary FROM device_sync_settings WHERE device_id = ?1",
                [device_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(DeviceSyncState {
            settings: selection::load_settings(conn, device_id)?,
            last_summary: last_summary.and_then(|j| serde_json::from_str(&j).ok()),
            running: self.running.lock().contains_key(device_id),
        })
    }

    /// Saves `settings` for a connected device and plans syncing them. The
    /// returned plan carries the first items of each list; [`items`] has
    /// the rest. Rejects with `device-not-connected` when the device is gone.
    ///
    /// [`items`]: Self::items
    pub fn preview(
        &self,
        conn: &rusqlite::Connection,
        monitor: &DeviceMonitor,
        transcoder: &Transcoder,
        device_id: &str,
        settings: SyncSettings,
        now: i64,
    ) -> Result<SyncPlan> {
        let not_connected = || anyhow!("device-not-connected");
        let shown = monitor.shown(device_id).ok_or_else(not_connected)?;
        let device = monitor.device(device_id).ok_or_else(not_connected)?;
        let storage = settings
            .storage_id
            .as_ref()
            .and_then(|id| shown.storages.iter().find(|s| &s.id == id))
            .or_else(|| shown.storages.first())
            .ok_or_else(not_connected)?
            .clone();
        selection::save_settings(conn, device_id, &settings, now)?;

        let profile = shown.profile.clone();
        let backend = device.storage(&storage.id).ok_or_else(not_connected)?;
        let inventory = self.inventory(device_id, &storage, &profile, &*backend)?;

        let selection = selection::resolve(conn, &settings, |song| {
            transcoder
                .estimated_size(Path::new(song.path.as_deref()?), &profile)
                .ok()
        })?;
        let save_space = (profile.convert.mode != ConvertMode::SaveSpace).then(|| DeviceProfile {
            convert: ConvertSettings {
                mode: ConvertMode::SaveSpace,
                ..profile.convert.clone()
            },
            ..profile.clone()
        });
        let mut tracks = Vec::with_capacity(selection.songs.len());
        let mut unreadable = Vec::new();
        for wanted in selection.songs {
            let path = Path::new(wanted.song.path.as_deref().unwrap_or_default());
            let decision = match transcoder.decide(path, &profile) {
                Ok(decision) => decision,
                Err(e) => {
                    log::warn!("Sync can't plan {}: {e}", path.display());
                    unreadable.push(wanted.song);
                    continue;
                }
            };
            let library_size = wanted.song.filesize.unwrap_or(0).max(0) as u64;
            tracks.push(TrackInput {
                stamp: SourceStamp {
                    path: path.to_string_lossy().into_owned(),
                    size: library_size,
                    mtime: wanted.song.mtime,
                },
                extension: decision.extension(path),
                conversion: format!("{decision:?}"),
                converts: matches!(decision, Decision::Convert { .. }),
                size: transcoder
                    .estimated_size(path, &profile)
                    .unwrap_or(library_size),
                size_converting_all: save_space
                    .as_ref()
                    .and_then(|p| transcoder.estimated_size(path, p).ok()),
                wanted,
            });
        }
        let library = library_files(conn)?;
        let built = plan::build(PlanInput {
            profile: &profile,
            unselected: settings.unselected,
            tracks,
            unreadable,
            playlists: &selection.playlists,
            sources: &selection.sources,
            inventory: &inventory.inventory,
            library: &library,
            conversion_target: transcoder.conversion_target(&profile),
        });

        let mut plan = built.plan;
        plan.plan_id = uuid::Uuid::new_v4().to_string();
        plan.device_id = device_id.to_string();
        plan.storage_id = storage.id.clone();
        let preview = plan.preview();
        self.plans.lock().insert(
            device_id.to_string(),
            Arc::new(StoredPlan {
                plan,
                steps: built.steps,
                profile,
                inventory,
            }),
        );
        Ok(preview)
    }

    /// One list of a previewed plan in full; `None` once the plan is gone
    /// (replaced by a newer preview, or started).
    pub fn items(&self, plan_id: &str, list: PlanList) -> Option<Vec<SyncItem>> {
        let stored = self.plan(plan_id)?;
        let list = match list {
            PlanList::Copy => &stored.plan.copy,
            PlanList::Remove => &stored.plan.remove,
            PlanList::Update => &stored.plan.update,
            PlanList::Adopted => &stored.plan.adopted,
            PlanList::Skipped => &stored.plan.skipped,
        };
        Some(list.items.clone())
    }

    /// Starts the previewed plan `plan_id` on a background thread. Rejects
    /// with `sync-plan-stale` when the plan is gone or the device, its
    /// profile or its contents changed since the preview,
    /// `sync-does-not-fit`, or `sync-in-progress`.
    pub fn start(
        self: &Arc<Self>,
        plan_id: &str,
        monitor: Arc<DeviceMonitor>,
        transcoder: Arc<Transcoder>,
        db: Arc<Database>,
        events: Arc<dyn SyncEvents>,
        clock: Arc<dyn Clock>,
    ) -> Result<()> {
        let stale = || anyhow!("sync-plan-stale");
        let stored = self.plan(plan_id).ok_or_else(stale)?;
        let device_id = stored.plan.device_id.clone();
        if !stored.plan.storage.fits {
            return Err(anyhow!("sync-does-not-fit"));
        }
        let shown = monitor.shown(&device_id).ok_or_else(stale)?;
        let current_inventory = self.inventories.lock().get(&device_id).cloned();
        if shown.profile != stored.profile
            || !current_inventory.is_some_and(|i| Arc::ptr_eq(&i, &stored.inventory))
        {
            return Err(stale());
        }
        let storage = monitor
            .device(&device_id)
            .and_then(|d| d.storage(&stored.plan.storage_id))
            .ok_or_else(stale)?;

        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut running = self.running.lock();
            if running.contains_key(&device_id) {
                return Err(anyhow!("sync-in-progress"));
            }
            running.insert(device_id.clone(), cancel.clone());
        }
        self.plans.lock().remove(&device_id);

        let job = Job {
            device_id: device_id.clone(),
            storage,
            profile: stored.profile.clone(),
            manifest: stored.inventory.inventory.manifest.clone(),
            steps: stored.steps.clone(),
            planned_free_after_bytes: stored.plan.storage.free_after_bytes,
        };
        let sync = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("device-sync".into())
            .spawn(move || {
                let summary = worker::run(job, &transcoder, &cancel, &*clock, &mut |p| {
                    events.progress(p)
                });
                sync.finish(&summary, &monitor, &db);
                events.finished(&summary);
            });
        if let Err(e) = spawned {
            self.running.lock().remove(&device_id);
            return Err(e.into());
        }
        Ok(())
    }

    /// Asks a running sync to stop; every file completed so far is kept.
    /// Does nothing when none is running.
    pub fn cancel(&self, device_id: &str) {
        if let Some(cancel) = self.running.lock().get(device_id) {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// [`cancel`](Self::cancel)s a running sync and waits up to `timeout`
    /// for it to end, so the device can be ejected.
    pub fn stop(&self, device_id: &str, timeout: Duration) {
        let mut running = self.running.lock();
        let Some(cancel) = running.get(device_id) else {
            return;
        };
        cancel.store(true, Ordering::Relaxed);
        let deadline = std::time::Instant::now() + timeout;
        while running.contains_key(device_id) {
            if self
                .sync_ended
                .wait_until(&mut running, deadline)
                .timed_out()
            {
                log::warn!("The sync to {device_id} did not stop in time");
                return;
            }
        }
    }

    fn finish(&self, summary: &SyncSummary, monitor: &DeviceMonitor, db: &Database) {
        let device_id = &summary.device_id;
        // The device changed: the next preview reads it again.
        self.inventories.lock().remove(device_id);
        let stored = db.pool.get().map_err(anyhow::Error::from).and_then(|conn| {
            conn.execute(
                "INSERT INTO device_sync_settings (device_id, last_summary, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(device_id) DO UPDATE SET last_summary = excluded.last_summary",
                params![
                    device_id,
                    serde_json::to_string(summary)?,
                    summary.finished_at
                ],
            )?;
            if summary.outcome == SyncOutcome::Completed {
                monitor.record_sync(&conn, device_id, summary.finished_at)?;
            } else {
                monitor.request_rebuild();
            }
            Ok(())
        });
        if let Err(e) = stored {
            log::warn!("Could not record the sync to {device_id}: {e}");
        }
        self.running.lock().remove(device_id);
        self.sync_ended.notify_all();
    }

    fn plan(&self, plan_id: &str) -> Option<Arc<StoredPlan>> {
        self.plans
            .lock()
            .values()
            .find(|p| p.plan.plan_id == plan_id)
            .cloned()
    }

    /// What is on `storage`, from the cache while it still applies.
    fn inventory(
        &self,
        device_id: &str,
        storage: &crate::devices::DeviceStorage,
        profile: &DeviceProfile,
        backend: &dyn DeviceBackend,
    ) -> Result<Arc<CachedInventory>> {
        let roots = sync_roots(profile);
        if let Some(cached) = self.inventories.lock().get(device_id) {
            if cached.storage_id == storage.id
                && cached.total_bytes == storage.total_bytes
                && cached.free_bytes == storage.free_bytes
                && cached.roots == roots
            {
                return Ok(cached.clone());
            }
        }
        let inventory = Arc::new(CachedInventory {
            inventory: read_inventory(backend, &roots)?,
            storage_id: storage.id.clone(),
            total_bytes: storage.total_bytes,
            free_bytes: storage.free_bytes,
            roots,
        });
        self.inventories
            .lock()
            .insert(device_id.to_string(), inventory.clone());
        Ok(inventory)
    }
}

/// The folders sync writes into, `/`-separated, without one that is inside
/// another (ignoring case); `""` is the whole storage.
fn sync_roots(profile: &DeviceProfile) -> Vec<String> {
    let mut roots: Vec<String> = [&profile.music_path, &profile.playlists_path]
        .into_iter()
        .map(|p| {
            p.split(['/', '\\'])
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    roots.sort_by_key(|r| r.to_lowercase());
    roots.dedup_by_key(|r| r.to_lowercase());
    let all: Vec<String> = roots.iter().map(|r| r.to_lowercase()).collect();
    roots.retain(|r| {
        let r = r.to_lowercase();
        !all.iter()
            .any(|other| *other != r && (other.is_empty() || r.starts_with(&format!("{other}/"))))
    });
    roots
}

/// Lists every file under `roots` and reads the manifest and free space.
/// A root that can't be listed is an error; a folder inside one that can't
/// (`System Volume Information`, a folder an MTP device refuses) is
/// skipped, since sync never writes there.
fn read_inventory(backend: &dyn DeviceBackend, roots: &[String]) -> Result<Inventory> {
    let mut files = HashMap::new();
    let mut pending: Vec<(String, bool)> = roots.iter().map(|r| (r.clone(), true)).collect();
    while let Some((dir, is_root)) = pending.pop() {
        let entries = match backend.list(&dir) {
            Ok(entries) => entries,
            Err(e) if !is_root => {
                log::warn!("Sync skips {dir}, which can't be listed: {e}");
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let path = if dir.is_empty() {
                entry.name.clone()
            } else {
                format!("{dir}/{}", entry.name)
            };
            if entry.is_dir {
                pending.push((path, false));
            } else if !entry.name.starts_with(".luminous-") && entry.name != MANIFEST_FILE_NAME {
                files.insert(path.to_lowercase(), entry.size);
            }
        }
    }
    Ok(Inventory {
        files,
        listed: roots.iter().map(|r| r.to_lowercase()).collect(),
        manifest: Manifest::read(backend)?,
        space: backend.space()?,
    })
}

/// Every local library file by path, and whether it can be read now.
fn library_files(conn: &rusqlite::Connection) -> Result<HashMap<String, LibraryFile>> {
    let sql = format!(
        "SELECT path, unavailable FROM songs WHERE source IN ({}) AND path IS NOT NULL",
        *LOCAL_SOURCES_SQL
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let unavailable: bool = row.get(1)?;
        Ok((
            row.get::<_, String>(0)?,
            if unavailable {
                LibraryFile::Unavailable
            } else {
                LibraryFile::Available
            },
        ))
    })?;
    let mut files = HashMap::new();
    for row in rows {
        let (path, state) = row?;
        // A path listed twice (CUE tracks) is available if any row is.
        files
            .entry(path)
            .and_modify(|s| {
                if state == LibraryFile::Available {
                    *s = state
                }
            })
            .or_insert(state);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::sync::plan::SyncReason;
    use crate::devices::sync::selection::SyncSource;
    use crate::models::SongSource;
    use luminous_devices::{Device, FsBackend, Volume};
    use std::sync::mpsc;

    const NOW: i64 = 1_790_000_000;

    struct Channel {
        finished: Mutex<mpsc::Sender<SyncSummary>>,
        steps: Mutex<Vec<worker::SyncStep>>,
    }

    impl SyncEvents for Channel {
        fn progress(&self, progress: &SyncProgress) {
            self.steps.lock().push(progress.step.clone());
        }
        fn finished(&self, summary: &SyncSummary) {
            let _ = self.finished.lock().send(summary.clone());
        }
    }

    struct World {
        _dirs: Vec<tempfile::TempDir>,
        device_root: std::path::PathBuf,
        db: Arc<Database>,
        monitor: Arc<DeviceMonitor>,
        transcoder: Arc<Transcoder>,
        sync: Arc<DeviceSync>,
    }

    /// A library of two WAV songs and a connected FAT volume on temp dirs.
    /// The device plays `format`; WAV is converted to Opus when it doesn't.
    fn world(format: &str) -> World {
        let (db_dir, library, device, cache) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        let db = Arc::new(Database::new(db_dir.path().to_path_buf()).unwrap());
        let conn = db.pool.get().unwrap();
        for (id, title) in [(1, "Halo"), (2, "Northbound")] {
            let path = library.path().join(format!("{title}.wav"));
            luminous_audio::write_test_pcm16_wav(&path, 44_100, &[vec![0; 4_410], vec![0; 4_410]]);
            let size = std::fs::metadata(&path).unwrap().len() as i64;
            conn.execute(
                "INSERT INTO songs (id, source, path, title, artist, album_artist, album, track,
                                    filesize, mtime, unavailable)
                 VALUES (?1, ?2, ?3, ?4, 'Amorphis', 'Amorphis', 'Halo', ?1, ?5, 100, 0)",
                params![
                    id,
                    SongSource::Collection as i32,
                    path.to_string_lossy(),
                    title,
                    size
                ],
            )
            .unwrap();
        }
        let monitor = Arc::new(DeviceMonitor::new());
        let volume = Device::from_volume(Volume {
            mount_path: device.path().to_path_buf(),
            label: None,
            filesystem: "FAT32".into(),
            serial: Some("0088-5572".into()),
            vendor: Some("HiBy".into()),
            model: Some("R1".into()),
            total_bytes: 32_000_000_000,
            free_bytes: 10_000_000_000,
            os_ref: "E".into(),
        });
        *monitor.entries.lock() = crate::devices::build_entries(&conn, &[volume], NOW).unwrap();
        let mut profile = monitor.shown("vol-0088-5572").unwrap().profile;
        profile.supported_formats = vec![format.into()];
        profile.convert.format = luminous_devices::TargetFormat::Opus;
        profile.max_sample_rate = 192_000;
        profile.max_bit_depth = 24;
        profile.layout.playlist_format = luminous_devices::PlaylistFileFormat::None;
        monitor.entries.lock()[0].shown.profile = profile;
        drop(conn);
        World {
            device_root: device.path().to_path_buf(),
            transcoder: Arc::new(Transcoder::new(cache.path().to_path_buf())),
            _dirs: vec![db_dir, library, device, cache],
            db,
            monitor,
            sync: Arc::new(DeviceSync::new()),
        }
    }

    impl World {
        fn preview(&self, settings: SyncSettings) -> Result<SyncPlan> {
            let conn = self.db.pool.get().unwrap();
            self.sync.preview(
                &conn,
                &self.monitor,
                &self.transcoder,
                "vol-0088-5572",
                settings,
                NOW,
            )
        }

        /// Runs the plan; its summary and the steps it reported.
        fn start(&self, plan_id: &str) -> Result<(SyncSummary, Vec<worker::SyncStep>)> {
            let (tx, rx) = mpsc::channel();
            let events = Arc::new(Channel {
                finished: Mutex::new(tx),
                steps: Mutex::new(Vec::new()),
            });
            self.sync.start(
                plan_id,
                self.monitor.clone(),
                self.transcoder.clone(),
                self.db.clone(),
                events.clone(),
                Arc::new(worker::TickClock::new(NOW)),
            )?;
            let summary = rx.recv_timeout(Duration::from_secs(60)).unwrap();
            let steps = std::mem::take(&mut *events.steps.lock());
            Ok((summary, steps))
        }
    }

    fn whole_library() -> SyncSettings {
        SyncSettings {
            sources: vec![SyncSource::Library],
            ..SyncSettings::default()
        }
    }

    #[test]
    fn a_previewed_sync_runs_and_the_next_preview_has_nothing_to_do() {
        let world = world("wav");
        let plan = world.preview(whole_library()).unwrap();
        assert_eq!(plan.copy.songs, 2);
        assert!(plan.storage.fits);
        assert_eq!(plan.conversion_target, None, "nothing is converted");

        let (summary, _) = world.start(&plan.plan_id).unwrap();
        assert_eq!(summary.outcome, SyncOutcome::Completed);
        assert_eq!(summary.songs_written, 2);
        for item in &plan.copy.items {
            assert!(world.device_root.join(&item.path).exists(), "{}", item.path);
        }

        let conn = world.db.pool.get().unwrap();
        let state = world.sync.state(&conn, "vol-0088-5572").unwrap();
        assert_eq!(state.settings, whole_library());
        assert_eq!(state.last_summary, Some(summary));
        assert!(!state.running);
        let synced_at: Option<i64> = conn
            .query_row(
                "SELECT last_synced_at FROM device_profiles WHERE id = 'vol-0088-5572'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(synced_at, Some(NOW));
        drop(conn);

        let again = world.preview(whole_library()).unwrap();
        assert_eq!(
            (again.copy.songs, again.update.songs, again.remove.songs),
            (0, 0, 0)
        );

        // Unchecking the library removes what sync copied.
        let none = world.preview(SyncSettings::default()).unwrap();
        assert_eq!(none.remove.songs, 2);
        assert!(matches!(
            none.remove.items[0].reason,
            SyncReason::Unselected { .. }
        ));
    }

    #[test]
    fn converted_songs_sync_once_and_then_stay() {
        let world = world("opus");
        let plan = world.preview(whole_library()).unwrap();
        assert_eq!(plan.converted_songs, 2);
        let target = plan.conversion_target.unwrap();
        assert_eq!(target.format, luminous_devices::TargetFormat::Opus);
        assert!(target.device_plays);
        assert!(plan.copy.items.iter().all(|i| i.path.ends_with(".opus")));

        let (summary, steps) = world.start(&plan.plan_id).unwrap();
        assert_eq!(summary.outcome, SyncOutcome::Completed);
        assert_eq!(summary.converted, 2);
        assert!(steps.iter().any(|s| matches!(
            s,
            worker::SyncStep::Converting {
                format: luminous_devices::TargetFormat::Opus,
                ..
            }
        )));
        for item in &plan.copy.items {
            assert!(world.device_root.join(&item.path).exists(), "{}", item.path);
        }

        // The worker records conversions the way the planner compares them,
        // so nothing is converted again.
        let again = world.preview(whole_library()).unwrap();
        assert_eq!((again.copy.songs, again.update.songs), (0, 0));
        assert_eq!(again.storage.stays_bytes, summary.written_bytes);
    }

    #[test]
    fn previews_reuse_what_was_read_from_the_device() {
        let world = world("wav");
        world.preview(whole_library()).unwrap();
        let first = world.sync.inventories.lock()["vol-0088-5572"].clone();
        world.preview(SyncSettings::default()).unwrap();
        let second = world.sync.inventories.lock()["vol-0088-5572"].clone();
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn a_folder_that_cant_be_listed_inside_a_root_is_skipped() {
        struct Refusing(FsBackend);
        impl DeviceBackend for Refusing {
            fn list(&self, dir: &str) -> std::io::Result<Vec<luminous_devices::DirEntry>> {
                if dir.ends_with("System Volume Information") {
                    return Err(std::io::ErrorKind::PermissionDenied.into());
                }
                self.0.list(dir)
            }
            fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
                self.0.read(path)
            }
            fn write(
                &self,
                path: &str,
                source: &mut dyn std::io::Read,
                len: u64,
                progress: luminous_devices::Progress<'_>,
            ) -> std::io::Result<()> {
                self.0.write(path, source, len, progress)
            }
            fn remove(&self, path: &str) -> std::io::Result<()> {
                self.0.remove(path)
            }
            fn space(&self) -> std::io::Result<luminous_devices::Space> {
                self.0.space()
            }
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("System Volume Information")).unwrap();
        std::fs::write(dir.path().join("song.mp3"), b"1").unwrap();
        let inventory =
            read_inventory(&Refusing(FsBackend::new(dir.path())), &[String::new()]).unwrap();
        assert_eq!(inventory.files.len(), 1);
    }

    #[test]
    fn only_the_latest_unchanged_plan_can_start() {
        let world = world("wav");
        let first = world.preview(whole_library()).unwrap();
        let second = world.preview(whole_library()).unwrap();
        let err = world.start(&first.plan_id).unwrap_err();
        assert_eq!(err.to_string(), "sync-plan-stale");

        // The profile changed after the preview.
        world.monitor.entries.lock()[0].shown.profile.music_path = "Other".into();
        let err = world.start(&second.plan_id).unwrap_err();
        assert_eq!(err.to_string(), "sync-plan-stale");
        assert!(!world.device_root.join("Music").exists());
    }

    #[test]
    fn roots_inside_another_are_listed_once() {
        let profile = |music: &str, playlists: &str| DeviceProfile {
            music_path: music.into(),
            playlists_path: playlists.into(),
            ..DeviceProfile::default()
        };
        assert_eq!(sync_roots(&profile("Music", "music/Playlists")), ["Music"]);
        assert_eq!(
            sync_roots(&profile("Music", "Playlists")),
            ["Music", "Playlists"]
        );
        assert_eq!(sync_roots(&profile("", "Playlists")), [""]);
    }

    #[test]
    fn inventory_lists_files_under_the_roots_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Music/A")).unwrap();
        std::fs::create_dir_all(root.join("DCIM")).unwrap();
        std::fs::write(root.join("Music/A/One.mp3"), b"12345").unwrap();
        std::fs::write(root.join("DCIM/photo.jpg"), b"1").unwrap();
        std::fs::write(root.join(MANIFEST_FILE_NAME), b"{}").unwrap();

        let inventory = read_inventory(&FsBackend::new(root), &["Music".into()]).unwrap();
        assert_eq!(inventory.files.len(), 1);
        assert_eq!(inventory.files.get("music/a/one.mp3"), Some(&5));
    }
}
