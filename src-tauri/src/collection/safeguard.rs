//! Mass-removal safeguard module (#1582).
//!
//! Protects against catastrophic library loss when external storage media
//! (USB flash drives, external HDDs/SSDs, SD cards, network shares) are unmounted,
//! disconnected, or temporarily unavailable.
//!
//! Automatic scans or filesystem watchers encountering an empty mount point or
//! missing directory could otherwise mistake hundreds or thousands of missing files
//! for intentional user deletions, wiping play counts, ratings, favorites, dynamic range
//! metrics, and playlist items.
//!
//! This module defines the threshold policies and evaluation engine that quarantine
//! suspected unmount events and require explicit user confirmation before purging records.

use anyhow::Result;
use rusqlite::params;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use crate::models::MassRemovalIncident;

/// Minimum percentage of tracks in a watched directory that must be missing
/// to trigger mass-removal protection (25%).
pub const MASS_REMOVAL_MIN_PERCENT: f64 = 0.25;

/// Minimum absolute count of missing tracks required when triggering via the percentage threshold.
/// Prevents false alarms on tiny directories (e.g. 5 of 10 tracks deleted).
pub const MASS_REMOVAL_MIN_COUNT: usize = 25;

/// Threshold for a complete directory wipe (100% missing).
/// If a directory has at least 10 tracks and all 100% vanish, it triggers immediately.
pub const MASS_REMOVAL_TOTAL_WIPE_MIN_COUNT: usize = 10;

/// Absolute missing count that triggers the safeguard regardless of directory percentage.
pub const MASS_REMOVAL_ABSOLUTE_COUNT: usize = 500;

/// The result of evaluating a set of candidate missing song IDs.
#[derive(Debug, Clone, Default)]
pub struct SafeguardVerdict {
    /// Track IDs that passed evaluation and can safely be soft-flagged unavailable or deleted.
    pub safe_ids: Vec<i64>,
    /// Track IDs that triggered mass-removal protection and must be preserved.
    pub blocked_ids: Vec<i64>,
    /// Incident details for each directory that exceeded the safety threshold.
    pub incidents: Vec<MassRemovalIncident>,
    /// Mapping of incident ID to the quarantined track IDs.
    pub incident_song_map: HashMap<String, Vec<i64>>,
}

impl SafeguardVerdict {
    /// Returns true if no mass removal was detected and all candidate tracks are safe.
    pub fn is_all_safe(&self) -> bool {
        self.incidents.is_empty()
    }

    /// Returns the quarantined track IDs associated with a specific incident ID.
    pub fn blocked_ids_for_incident(&self, incident_id: &str) -> Vec<i64> {
        self.incident_song_map
            .get(incident_id)
            .cloned()
            .unwrap_or_default()
    }
}

/// Evaluates whether a given missing count vs total count qualifies as a mass removal.
pub fn is_mass_removal(missing_count: usize, total_count: usize) -> bool {
    if missing_count == 0 {
        return false;
    }
    if missing_count >= MASS_REMOVAL_ABSOLUTE_COUNT {
        return true;
    }
    if total_count >= MASS_REMOVAL_TOTAL_WIPE_MIN_COUNT && missing_count == total_count {
        return true;
    }
    if missing_count >= MASS_REMOVAL_MIN_COUNT && total_count > 0 {
        let ratio = missing_count as f64 / total_count as f64;
        if ratio >= MASS_REMOVAL_MIN_PERCENT {
            return true;
        }
    }
    false
}

/// Trims trailing directory separators (`/` or `\`).
fn clean_dir_path(path_str: &str) -> String {
    path_str.trim_end_matches(['/', '\\']).to_string()
}

/// Counts the number of active songs recorded in the database under `dir_path`.
pub fn count_songs_under_dir(conn: &rusqlite::Connection, dir_path: &str) -> Result<usize> {
    let clean = clean_dir_path(dir_path);
    let path_fw = clean.replace('\\', "/");
    let path_bw = clean.replace('/', "\\");

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM songs
         WHERE path IS NOT NULL AND (
            path = ?1 OR path = ?2
            OR path LIKE ?1 || '/%'
            OR path LIKE ?1 || '\\%'
            OR path LIKE ?2 || '/%'
            OR path LIKE ?2 || '\\%'
         )",
        params![path_fw, path_bw],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

/// Checks if a song's path falls under a watched directory root.
fn song_matches_dir(song_path: &Path, dir_root: &Path) -> bool {
    if song_path.starts_with(dir_root) {
        return true;
    }
    #[cfg(windows)]
    {
        let song_str = song_path.to_string_lossy().to_lowercase();
        let dir_str = dir_root.to_string_lossy().to_lowercase();
        let clean_dir = dir_str.trim_end_matches(['/', '\\']);
        song_str.starts_with(clean_dir)
            && (song_str.len() == clean_dir.len()
                || song_str[clean_dir.len()..].starts_with('/')
                || song_str[clean_dir.len()..].starts_with('\\'))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Evaluates candidate missing track IDs against watched directory populations.
///
/// Quarantines any tracks belonging to a directory that exhibits mass-removal
/// symptoms (e.g. unmounted volume or mass deletion exceeding thresholds).
pub fn evaluate_missing_tracks(
    conn: &rusqlite::Connection,
    candidate_missing_ids: &[i64],
    force: bool,
) -> Result<SafeguardVerdict> {
    let mut verdict = SafeguardVerdict::default();
    if candidate_missing_ids.is_empty() {
        return Ok(verdict);
    }

    if force {
        verdict.safe_ids = candidate_missing_ids.to_vec();
        return Ok(verdict);
    }

    // Load watched directories from SQLite
    let mut dirs_stmt = conn.prepare("SELECT id, path FROM directories")?;
    let dirs: Vec<(i64, PathBuf)> = dirs_stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            Ok((id, PathBuf::from(clean_dir_path(&path))))
        })?
        .filter_map(|r| r.ok())
        .collect();
    drop(dirs_stmt);

    // Retrieve paths for candidate missing IDs in chunks of 500
    let mut song_paths: Vec<(i64, String)> = Vec::with_capacity(candidate_missing_ids.len());
    for chunk in candidate_missing_ids.chunks(500) {
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql =
            format!("SELECT id, path FROM songs WHERE id IN ({placeholders}) AND path IS NOT NULL");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for item in rows.flatten() {
            song_paths.push(item);
        }
    }

    // Group missing tracks by watched directory
    let mut dir_missing: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    let mut orphan_missing: Vec<(i64, String)> = Vec::new();

    for (id, path_str) in song_paths {
        let p = Path::new(&path_str);
        // Find most specific matching directory (longest path prefix)
        let matched = dirs
            .iter()
            .filter(|(_, dir_path)| song_matches_dir(p, dir_path))
            .max_by_key(|(_, dir_path)| dir_path.as_os_str().len());

        if let Some((dir_id, _)) = matched {
            dir_missing.entry(*dir_id).or_default().push((id, path_str));
        } else {
            orphan_missing.push((id, path_str));
        }
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    // Evaluate each watched directory
    for (dir_id, dir_path) in &dirs {
        let is_unreachable = std::fs::read_dir(dir_path).is_err();
        let missing_songs_opt = dir_missing.remove(dir_id);

        let total_count = count_songs_under_dir(conn, &dir_path.to_string_lossy())?;
        if total_count == 0 {
            continue;
        }

        if is_unreachable {
            // Unmounted drive or disconnected network share: all tracked songs are unreachable.
            let incident_id = format!("incident-{}-{}", dir_id, now_ms);
            let clean = clean_dir_path(&dir_path.to_string_lossy());
            let path_fw = clean.replace('\\', "/");
            let path_bw = clean.replace('/', "\\");

            let mut stmt = conn.prepare(
                "SELECT id, path FROM songs WHERE path IS NOT NULL AND (
                    path = ?1 OR path = ?2
                    OR path LIKE ?1 || '/%'
                    OR path LIKE ?1 || '\\%'
                    OR path LIKE ?2 || '/%'
                    OR path LIKE ?2 || '\\%'
                )",
            )?;
            let rows = stmt.query_map(params![path_fw, path_bw], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            let all_songs: Vec<(i64, String)> = rows.flatten().collect();
            let sample_paths: Vec<String> =
                all_songs.iter().take(5).map(|(_, p)| p.clone()).collect();
            let missing_ids: Vec<i64> = all_songs.into_iter().map(|(id, _)| id).collect();

            let incident = MassRemovalIncident {
                id: incident_id.clone(),
                directory_id: Some(*dir_id),
                directory_path: dir_path.to_string_lossy().to_string(),
                missing_count: total_count,
                total_count,
                sample_missing_paths: sample_paths,
            };

            verdict.blocked_ids.extend(missing_ids.iter().copied());
            verdict.incident_song_map.insert(incident_id, missing_ids);
            verdict.incidents.push(incident);
            continue;
        }

        let Some(missing_songs) = missing_songs_opt else {
            continue;
        };

        let missing_count = missing_songs.len();

        if is_mass_removal(missing_count, total_count) {
            let incident_id = format!("incident-{}-{}", dir_id, now_ms);
            let sample_paths: Vec<String> = missing_songs
                .iter()
                .take(5)
                .map(|(_, p)| p.clone())
                .collect();
            let missing_ids: Vec<i64> = missing_songs.into_iter().map(|(id, _)| id).collect();

            let incident = MassRemovalIncident {
                id: incident_id.clone(),
                directory_id: Some(*dir_id),
                directory_path: dir_path.to_string_lossy().to_string(),
                missing_count,
                total_count,
                sample_missing_paths: sample_paths,
            };

            verdict.blocked_ids.extend(missing_ids.iter().copied());
            verdict.incident_song_map.insert(incident_id, missing_ids);
            verdict.incidents.push(incident);
        } else {
            verdict
                .safe_ids
                .extend(missing_songs.into_iter().map(|(id, _)| id));
        }
    }

    // Evaluate orphaned missing songs
    let orphan_count = orphan_missing.len();
    if orphan_count >= MASS_REMOVAL_ABSOLUTE_COUNT {
        let incident_id = format!("incident-orphan-{}", now_ms);
        let sample_paths: Vec<String> = orphan_missing
            .iter()
            .take(5)
            .map(|(_, p)| p.clone())
            .collect();
        let missing_ids: Vec<i64> = orphan_missing.into_iter().map(|(id, _)| id).collect();

        let incident = MassRemovalIncident {
            id: incident_id.clone(),
            directory_id: None,
            directory_path: "Orphaned Tracks".to_string(),
            missing_count: orphan_count,
            total_count: orphan_count,
            sample_missing_paths: sample_paths,
        };

        verdict.blocked_ids.extend(missing_ids.iter().copied());
        verdict.incident_song_map.insert(incident_id, missing_ids);
        verdict.incidents.push(incident);
    } else {
        verdict
            .safe_ids
            .extend(orphan_missing.into_iter().map(|(id, _)| id));
    }

    Ok(verdict)
}

/// The result of evaluating paths removed during a file-watcher batch.
#[derive(Debug, Clone, Default)]
pub struct WatcherSafeguardVerdict {
    /// Removed paths that are safe to delete from the database.
    pub safe_paths: Vec<PathBuf>,
    /// Removed paths that triggered mass-removal protection and must NOT be hard-deleted.
    pub blocked_paths: Vec<PathBuf>,
    /// Incidents generated for protected directories.
    pub incidents: Vec<MassRemovalIncident>,
}

/// Evaluates file paths removed in a realtime watcher batch against watched directory totals.
pub fn evaluate_watcher_removals(
    conn: &rusqlite::Connection,
    removed_audio_paths: &[PathBuf],
) -> Result<WatcherSafeguardVerdict> {
    let mut verdict = WatcherSafeguardVerdict::default();
    if removed_audio_paths.is_empty() {
        return Ok(verdict);
    }

    let mut dirs_stmt = conn.prepare("SELECT id, path FROM directories")?;
    let dirs: Vec<(i64, PathBuf)> = dirs_stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            Ok((id, PathBuf::from(clean_dir_path(&path))))
        })?
        .filter_map(|r| r.ok())
        .collect();
    drop(dirs_stmt);

    let mut dir_removed: HashMap<i64, Vec<PathBuf>> = HashMap::new();
    let mut orphan_removed: Vec<PathBuf> = Vec::new();

    for path in removed_audio_paths {
        let matched = dirs
            .iter()
            .filter(|(_, dir_path)| song_matches_dir(path, dir_path))
            .max_by_key(|(_, dir_path)| dir_path.as_os_str().len());

        if let Some((dir_id, _)) = matched {
            dir_removed.entry(*dir_id).or_default().push(path.clone());
        } else {
            orphan_removed.push(path.clone());
        }
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    for (dir_id, dir_path) in &dirs {
        let Some(removed_paths) = dir_removed.remove(dir_id) else {
            continue;
        };

        let removed_count = removed_paths.len();
        let total_count = count_songs_under_dir(conn, &dir_path.to_string_lossy())?;

        let is_unreachable = std::fs::read_dir(dir_path).is_err();
        if is_unreachable || is_mass_removal(removed_count, total_count) {
            let incident_id = format!("incident-watcher-{}-{}", dir_id, now_ms);
            let sample_paths: Vec<String> = removed_paths
                .iter()
                .take(5)
                .map(|p| p.to_string_lossy().to_string())
                .collect();

            let incident = MassRemovalIncident {
                id: incident_id,
                directory_id: Some(*dir_id),
                directory_path: dir_path.to_string_lossy().to_string(),
                missing_count: removed_count,
                total_count,
                sample_missing_paths: sample_paths,
            };

            verdict.blocked_paths.extend(removed_paths);
            verdict.incidents.push(incident);
        } else {
            verdict.safe_paths.extend(removed_paths);
        }
    }

    // Evaluate orphan removals
    let orphan_count = orphan_removed.len();
    if orphan_count >= MASS_REMOVAL_ABSOLUTE_COUNT {
        let incident_id = format!("incident-watcher-orphan-{}", now_ms);
        let sample_paths: Vec<String> = orphan_removed
            .iter()
            .take(5)
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let incident = MassRemovalIncident {
            id: incident_id,
            directory_id: None,
            directory_path: "Orphaned Tracks".to_string(),
            missing_count: orphan_count,
            total_count: orphan_count,
            sample_missing_paths: sample_paths,
        };

        verdict.blocked_paths.extend(orphan_removed);
        verdict.incidents.push(incident);
    } else {
        verdict.safe_paths.extend(orphan_removed);
    }

    Ok(verdict)
}

/// In-memory registry for pending mass-removal incidents.
#[derive(Debug, Default)]
pub struct MassRemovalRegistry {
    incidents: HashMap<String, MassRemovalIncidentContext>,
}

#[derive(Debug, Clone)]
pub struct MassRemovalIncidentContext {
    pub incident: MassRemovalIncident,
    pub missing_song_ids: Vec<i64>,
}

impl MassRemovalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, incident: MassRemovalIncident, song_ids: Vec<i64>) {
        self.incidents.insert(
            incident.id.clone(),
            MassRemovalIncidentContext {
                incident,
                missing_song_ids: song_ids,
            },
        );
    }

    pub fn get_all(&self) -> Vec<MassRemovalIncident> {
        self.incidents
            .values()
            .map(|c| c.incident.clone())
            .collect()
    }

    pub fn remove(&mut self, incident_id: &str) -> Option<MassRemovalIncidentContext> {
        self.incidents.remove(incident_id)
    }

    pub fn clear(&mut self) {
        self.incidents.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{Song, SongSource};
    use std::sync::Arc;

    #[test]
    fn test_is_mass_removal_logic() {
        // Zero missing
        assert!(!is_mass_removal(0, 100));

        // Total wipe: 10 of 10 missing -> true
        assert!(is_mass_removal(10, 10));
        // Total wipe: 9 of 9 missing -> false (under TOTAL_WIPE_MIN_COUNT)
        assert!(!is_mass_removal(9, 9));

        // Proportional: 25 of 100 missing (25% and >= 25 count) -> true
        assert!(is_mass_removal(25, 100));
        // Proportional: 24 of 80 missing (30% but count < 25) -> false
        assert!(!is_mass_removal(24, 80));
        // Proportional: 25 of 120 missing (~20.8% < 25%) -> false
        assert!(!is_mass_removal(25, 120));

        // Absolute: >= 500 missing anywhere -> true
        assert!(is_mass_removal(500, 100_000));
        assert!(is_mass_removal(600, 5000));
    }

    #[test]
    fn test_evaluate_missing_tracks_quarantines_mass_removal() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_safeguard_test_")
            .tempdir()
            .unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let dir_usb = temp_dir.join("usb_music");
        let dir_local = temp_dir.join("local_music");
        std::fs::create_dir_all(&dir_usb).unwrap();
        std::fs::create_dir_all(&dir_local).unwrap();

        conn.execute(
            "INSERT INTO directories (path) VALUES (?1)",
            params![dir_usb.to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO directories (path) VALUES (?1)",
            params![dir_local.to_string_lossy()],
        )
        .unwrap();

        // Populate USB with 30 tracks
        let mut usb_ids = Vec::new();
        for i in 0..30 {
            let p = dir_usb.join(format!("track_{i}.mp3"));
            let song = Song {
                path: Some(p.to_string_lossy().to_string()),
                title: Some(format!("USB Track {i}")),
                source: SongSource::LocalFile,
                ..Default::default()
            };
            crate::collection::upsert_song(&conn, &song).unwrap();
            let id: i64 = conn
                .query_row(
                    "SELECT id FROM songs WHERE path = ?1",
                    params![p.to_string_lossy()],
                    |r| r.get(0),
                )
                .unwrap();
            usb_ids.push(id);
        }

        // Populate Local with 100 tracks
        let mut local_ids = Vec::new();
        for i in 0..100 {
            let p = dir_local.join(format!("track_{i}.mp3"));
            let song = Song {
                path: Some(p.to_string_lossy().to_string()),
                title: Some(format!("Local Track {i}")),
                source: SongSource::LocalFile,
                ..Default::default()
            };
            crate::collection::upsert_song(&conn, &song).unwrap();
            let id: i64 = conn
                .query_row(
                    "SELECT id FROM songs WHERE path = ?1",
                    params![p.to_string_lossy()],
                    |r| r.get(0),
                )
                .unwrap();
            local_ids.push(id);
        }

        // Scenario: All 30 USB tracks missing (unmounted), and 2 local tracks missing (normal deletion)
        let mut candidate_missing = usb_ids.clone();
        candidate_missing.push(local_ids[0]);
        candidate_missing.push(local_ids[1]);

        let verdict = evaluate_missing_tracks(&conn, &candidate_missing, false).unwrap();

        // USB tracks should be quarantined
        assert_eq!(verdict.incidents.len(), 1);
        let incident = &verdict.incidents[0];
        assert_eq!(incident.missing_count, 30);
        assert_eq!(incident.total_count, 30);
        assert_eq!(verdict.blocked_ids.len(), 30);

        // Local 2 tracks should be safe
        assert_eq!(verdict.safe_ids.len(), 2);
        assert!(verdict.safe_ids.contains(&local_ids[0]));
        assert!(verdict.safe_ids.contains(&local_ids[1]));

        // Force = true allows all
        let forced_verdict = evaluate_missing_tracks(&conn, &candidate_missing, true).unwrap();
        assert!(forced_verdict.is_all_safe());
        assert_eq!(forced_verdict.safe_ids.len(), 32);
    }
}
