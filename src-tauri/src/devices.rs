//! Connected portable players and removable drives (#835, epic #834).
//!
//! A background thread polls `luminous_devices::VolumeWatcher` every couple
//! of seconds and, whenever a device connects, disconnects, its free space
//! changes or a profile is saved, emits `devices-changed` with the full list.
//! Volumes holding a watched music folder are library storage, not devices,
//! and are never listed.
//!
//! Profiles: SQLite owns them on this computer. A device's
//! `.luminous-device.json` marker is written only when the user saves a
//! profile, and only seeds a new row when the device shows up here for the
//! first time (another computer, a fresh install).

use crate::db::Database;
use anyhow::{anyhow, Result};
use luminous_devices::{DeviceKind, DeviceProfile, Marker, Volume, VolumeWatcher};
use parking_lot::{Condvar, Mutex};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const DEVICES_CHANGED_EVENT: &str = "devices-changed";

const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Rebuild at least this often even when no volume changed, so a newly
/// watched music folder on a device, or a marker edited elsewhere, shows up.
const REBUILD_EVERY_POLLS: u32 = 15;

/// What the UI shows for one device.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedDevice {
    pub id: String,
    pub kind: DeviceKind,
    /// Vendor and model, e.g. "HiBy R1", when the device reports them.
    pub model: Option<String>,
    /// Shown when `profile.nickname` is `None`. Never blank.
    pub default_name: String,
    /// One entry for a mass-storage device; MTP players can have several (#840).
    pub storages: Vec<DeviceStorage>,
    pub profile: DeviceProfile,
    /// Unix seconds of the last completed sync (#836).
    pub last_synced_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStorage {
    pub id: String,
    pub label: Option<String>,
    pub mount_path: String,
    pub filesystem: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

struct Entry {
    device: ConnectedDevice,
    volume: Volume,
}

/// The current device list and the thread that keeps it current.
pub struct DeviceMonitor {
    entries: Mutex<Vec<Entry>>,
    rebuild_requested: Mutex<bool>,
    wake: Condvar,
}

impl DeviceMonitor {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            rebuild_requested: Mutex::new(true),
            wake: Condvar::new(),
        }
    }

    /// Starts the polling thread. Call once, after `AppState` is managed.
    pub fn spawn(self: &Arc<Self>, app: AppHandle, db: Arc<Database>) {
        let monitor = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("device-monitor".into())
            .spawn(move || monitor.run(&app, &db));
        if let Err(e) = spawned {
            log::error!("Could not start the device monitor: {e}");
        }
    }

    pub fn devices(&self) -> Vec<ConnectedDevice> {
        self.entries
            .lock()
            .iter()
            .map(|e| e.device.clone())
            .collect()
    }

    /// The volume of a connected device, `None` once it is gone.
    pub fn volume(&self, id: &str) -> Option<Volume> {
        self.entries
            .lock()
            .iter()
            .find(|e| e.device.id == id)
            .map(|e| e.volume.clone())
    }

    /// Asks the polling thread to rebuild and emit now rather than at its
    /// next tick.
    pub fn request_rebuild(&self) {
        *self.rebuild_requested.lock() = true;
        self.wake.notify_one();
    }

    /// Saves `profile` for a connected device: stores it, writes the
    /// device's marker (logged, not an error, on read-only media) and
    /// emits the updated list.
    pub fn save_profile(
        &self,
        conn: &rusqlite::Connection,
        id: &str,
        profile: DeviceProfile,
        now: i64,
    ) -> Result<()> {
        let volume = self
            .volume(id)
            .ok_or_else(|| anyhow!("device-not-connected"))?;
        let profile = profile.normalized();
        upsert_profile(conn, id, &profile, now)?;
        let marker = Marker {
            id: id.to_string(),
            profile,
        };
        if let Err(e) = luminous_devices::write_marker(&volume.mount_path, &marker) {
            log::warn!(
                "Saved the profile for {id} but could not write its marker to {}: {e}",
                volume.mount_path.display()
            );
        }
        self.request_rebuild();
        Ok(())
    }

    fn run(&self, app: &AppHandle, db: &Database) {
        let mut watcher = VolumeWatcher::new();
        let mut last_volumes: Option<Vec<Volume>> = None;
        let mut polls_since_rebuild = 0u32;
        loop {
            let volumes = watcher.poll();
            let requested = std::mem::take(&mut *self.rebuild_requested.lock());
            if requested
                || last_volumes.as_ref() != Some(&volumes)
                || polls_since_rebuild >= REBUILD_EVERY_POLLS
            {
                match db
                    .pool
                    .get()
                    .map_err(anyhow::Error::from)
                    .and_then(|conn| build_entries(&conn, &volumes, chrono::Utc::now().timestamp()))
                {
                    Ok(entries) => self.replace(app, entries),
                    Err(e) => log::warn!("Could not refresh connected devices: {e}"),
                }
                last_volumes = Some(volumes);
                polls_since_rebuild = 0;
            } else {
                polls_since_rebuild += 1;
            }

            let mut requested = self.rebuild_requested.lock();
            if !*requested {
                self.wake.wait_for(&mut requested, POLL_INTERVAL);
            }
        }
    }

    fn replace(&self, app: &AppHandle, entries: Vec<Entry>) {
        let devices: Vec<ConnectedDevice> = entries.iter().map(|e| e.device.clone()).collect();
        {
            let mut current = self.entries.lock();
            if current.iter().map(|e| &e.device).eq(devices.iter()) {
                return;
            }
            *current = entries;
        }
        let _ = app.emit(DEVICES_CHANGED_EVENT, &devices);
    }
}

impl Default for DeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `path` is `root` or inside it. Case-insensitive on Windows.
pub fn is_on_volume(path: &str, root: &Path) -> bool {
    let normalize = |p: &str| {
        let p = p.replace('\\', "/");
        let p = p.trim_end_matches('/').to_string();
        if cfg!(windows) {
            p.to_lowercase()
        } else {
            p
        }
    };
    let path = normalize(path);
    let root = normalize(&root.to_string_lossy());
    path == root || path.starts_with(&format!("{root}/"))
}

fn build_entries(conn: &rusqlite::Connection, volumes: &[Volume], now: i64) -> Result<Vec<Entry>> {
    let library_folders: Vec<String> = conn
        .prepare("SELECT path FROM directories")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    volumes
        .iter()
        .filter(|v| {
            !library_folders
                .iter()
                .any(|folder| is_on_volume(folder, &v.mount_path))
        })
        .map(|volume| {
            Ok(Entry {
                device: resolve_device(conn, volume, now)?,
                volume: volume.clone(),
            })
        })
        .collect()
}

fn resolve_device(
    conn: &rusqlite::Connection,
    volume: &Volume,
    now: i64,
) -> Result<ConnectedDevice> {
    let marker = luminous_devices::read_marker(&volume.mount_path);
    let id = marker
        .as_ref()
        .map(|m| m.id.clone())
        .unwrap_or_else(|| volume.provisional_id());
    let (profile, last_synced_at) = match (load_profile(conn, &id)?, marker) {
        (Some(stored), _) => stored,
        (None, Some(marker)) => {
            let profile = marker.profile.normalized();
            upsert_profile(conn, &id, &profile, now)?;
            (profile, None)
        }
        (None, None) => (DeviceProfile::defaults_for(&volume.mount_path), None),
    };
    Ok(ConnectedDevice {
        id: id.clone(),
        kind: DeviceKind::MassStorage,
        model: volume.model_name(),
        default_name: volume.default_name(),
        storages: vec![DeviceStorage {
            id: volume.provisional_id(),
            label: volume.label.clone(),
            mount_path: volume.mount_path.to_string_lossy().into_owned(),
            filesystem: volume.filesystem.clone(),
            total_bytes: volume.total_bytes,
            free_bytes: volume.free_bytes,
        }],
        profile,
        last_synced_at,
    })
}

fn load_profile(
    conn: &rusqlite::Connection,
    id: &str,
) -> Result<Option<(DeviceProfile, Option<i64>)>> {
    conn.query_row(
        "SELECT nickname, icon, color, music_path, playlists_path, reserve_bytes,
                supported_formats, max_sample_rate, max_bit_depth, last_synced_at
           FROM device_profiles WHERE id = ?1",
        [id],
        |row| {
            let formats: String = row.get(6)?;
            Ok((
                DeviceProfile {
                    nickname: row.get(0)?,
                    icon: row.get(1)?,
                    color: row.get(2)?,
                    music_path: row.get(3)?,
                    playlists_path: row.get(4)?,
                    reserve_bytes: row.get::<_, i64>(5)?.max(0) as u64,
                    supported_formats: serde_json::from_str(&formats).unwrap_or_default(),
                    max_sample_rate: row.get(7)?,
                    max_bit_depth: row.get(8)?,
                },
                row.get(9)?,
            ))
        },
    )
    .optional()
    .map_err(Into::into)
}

fn upsert_profile(
    conn: &rusqlite::Connection,
    id: &str,
    profile: &DeviceProfile,
    now: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO device_profiles (id, nickname, icon, color, music_path, playlists_path,
             reserve_bytes, supported_formats, max_sample_rate, max_bit_depth, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT(id) DO UPDATE SET
             nickname = excluded.nickname, icon = excluded.icon, color = excluded.color,
             music_path = excluded.music_path, playlists_path = excluded.playlists_path,
             reserve_bytes = excluded.reserve_bytes, supported_formats = excluded.supported_formats,
             max_sample_rate = excluded.max_sample_rate, max_bit_depth = excluded.max_bit_depth,
             updated_at = excluded.updated_at",
        params![
            id,
            profile.nickname,
            profile.icon,
            profile.color,
            profile.music_path,
            profile.playlists_path,
            i64::try_from(profile.reserve_bytes).unwrap_or(i64::MAX),
            serde_json::to_string(&profile.supported_formats)?,
            profile.max_sample_rate,
            profile.max_bit_depth,
            now,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_790_000_000;

    fn test_db() -> (tempfile::TempDir, Arc<Database>) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(dir.path().to_path_buf()).unwrap());
        (dir, db)
    }

    fn volume(root: &Path, serial: &str) -> Volume {
        Volume {
            mount_path: root.to_path_buf(),
            label: None,
            filesystem: "FAT32".into(),
            serial: Some(serial.into()),
            vendor: Some("HiBy".into()),
            model: Some("R1 U-DISK".into()),
            total_bytes: 31_251_120_128,
            free_bytes: 3_002_875_904,
            os_ref: "E".into(),
        }
    }

    #[test]
    fn unknown_device_gets_defaults_without_touching_the_device() {
        let (_db_dir, db) = test_db();
        let conn = db.pool.get().unwrap();
        let device_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(device_dir.path().join("Music")).unwrap();

        let entries = build_entries(&conn, &[volume(device_dir.path(), "0088-5572")], NOW).unwrap();
        let device = &entries[0].device;
        assert_eq!(device.id, "vol-0088-5572");
        assert_eq!(device.default_name, "HiBy R1");
        assert_eq!(device.profile.music_path, "Music");
        assert_eq!(device.storages[0].free_bytes, 3_002_875_904);

        let written: Vec<_> = std::fs::read_dir(device_dir.path()).unwrap().collect();
        assert_eq!(
            written.len(),
            1,
            "only the Music folder; no marker on detection"
        );
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM device_profiles", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn volumes_holding_a_watched_folder_are_not_devices() {
        let (_db_dir, db) = test_db();
        let conn = db.pool.get().unwrap();
        let device_dir = tempfile::tempdir().unwrap();
        let library = device_dir.path().join("Library");
        conn.execute(
            "INSERT INTO directories (path, subdirs) VALUES (?1, 1)",
            [library.to_string_lossy()],
        )
        .unwrap();

        let entries = build_entries(&conn, &[volume(device_dir.path(), "0088-5572")], NOW).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn saved_profile_writes_a_marker_that_seeds_another_computer() {
        let (_db_dir, db) = test_db();
        let conn = db.pool.get().unwrap();
        let device_dir = tempfile::tempdir().unwrap();
        let monitor = DeviceMonitor::new();
        *monitor.entries.lock() =
            build_entries(&conn, &[volume(device_dir.path(), "0088-5572")], NOW).unwrap();

        let profile = DeviceProfile {
            nickname: Some(" Gym ".into()),
            color: Some("#3b82f6".into()),
            music_path: "../Music".into(),
            ..DeviceProfile::default()
        };
        monitor
            .save_profile(&conn, "vol-0088-5572", profile, NOW)
            .unwrap();

        let marker = luminous_devices::read_marker(device_dir.path()).unwrap();
        assert_eq!(marker.id, "vol-0088-5572");
        assert_eq!(marker.profile.nickname.as_deref(), Some("Gym"));
        assert_eq!(marker.profile.music_path, "Music");

        // Another computer: an empty database and a reformat-proof id from the marker.
        let (_other_dir, other_db) = test_db();
        let other = other_db.pool.get().unwrap();
        let entries =
            build_entries(&other, &[volume(device_dir.path(), "FFFF-0000")], NOW).unwrap();
        assert_eq!(entries[0].device.id, "vol-0088-5572");
        assert_eq!(entries[0].device.profile.nickname.as_deref(), Some("Gym"));
        assert!(load_profile(&other, "vol-0088-5572").unwrap().is_some());
    }

    #[test]
    fn saving_for_a_disconnected_device_fails() {
        let (_db_dir, db) = test_db();
        let conn = db.pool.get().unwrap();
        let err = DeviceMonitor::new()
            .save_profile(&conn, "vol-0000-0000", DeviceProfile::default(), NOW)
            .unwrap_err();
        assert_eq!(err.to_string(), "device-not-connected");
    }

    #[test]
    fn database_profile_wins_over_the_marker() {
        let (_db_dir, db) = test_db();
        let conn = db.pool.get().unwrap();
        let device_dir = tempfile::tempdir().unwrap();
        let stored = DeviceProfile {
            nickname: Some("Here".into()),
            ..DeviceProfile::default()
        };
        upsert_profile(&conn, "vol-0088-5572", &stored, NOW).unwrap();
        luminous_devices::write_marker(
            device_dir.path(),
            &Marker {
                id: "vol-0088-5572".into(),
                profile: DeviceProfile {
                    nickname: Some("Elsewhere".into()),
                    ..DeviceProfile::default()
                },
            },
        )
        .unwrap();

        let entries = build_entries(&conn, &[volume(device_dir.path(), "0088-5572")], NOW).unwrap();
        assert_eq!(entries[0].device.profile.nickname.as_deref(), Some("Here"));
    }

    #[test]
    fn is_on_volume_matches_whole_components() {
        let root = Path::new(if cfg!(windows) { "E:\\" } else { "/media/R1" });
        let inside = if cfg!(windows) {
            "e:\\Music\\a.flac"
        } else {
            "/media/R1/Music/a.flac"
        };
        let sibling = if cfg!(windows) {
            "F:\\Music"
        } else {
            "/media/R10/Music"
        };
        assert!(is_on_volume(inside, root));
        assert!(!is_on_volume(sibling, root));
    }
}
