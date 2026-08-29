//! Filesystem-reconciliation helpers shared by the scanner (`collection.rs`)
//! and the realtime watcher (`collection::watcher`) — matching a DB row whose
//! file has moved against where it actually is now, instead of treating the
//! move as a delete + a fresh insert. Split out of `collection.rs` (#577 item
//! 17).

use anyhow::Result;
use rusqlite::params;
use std::path::{Component, Path, PathBuf};

/// Resolves `path` to its real on-disk path when it differs only in case from
/// what's stored — e.g. a tag-driven rename shifted an album folder's casing.
/// `Path::exists()` treats a stale-cased path as a hit on a case-insensitive-
/// but-case-preserving filesystem (Windows, macOS), which is why this drift
/// only ever surfaces as a real playback failure on Linux/ext4. Returns
/// `None` if some component genuinely doesn't exist on disk under any case.
pub(crate) fn resolve_case_insensitive_path(path: &Path) -> Option<PathBuf> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => {
                // Don't shortcut on `current.join(name).exists()` — on
                // case-insensitive filesystems (Windows, default macOS) that
                // returns true even when `name`'s casing doesn't match what's
                // really on disk, which would defeat the whole point of this
                // function. Always resolve the real on-disk casing via
                // `read_dir`, preferring an exact match if one exists.
                if !current.is_dir() {
                    return None;
                }
                let target = name.to_string_lossy().to_lowercase();
                let mut case_insensitive_match = None;
                let real_name = std::fs::read_dir(&current)
                    .ok()?
                    .filter_map(|e| e.ok())
                    .find_map(|e| {
                        let entry_name = e.file_name();
                        if entry_name == name {
                            return Some(entry_name);
                        }
                        if case_insensitive_match.is_none()
                            && entry_name.to_string_lossy().to_lowercase() == target
                        {
                            case_insensitive_match = Some(entry_name);
                        }
                        None
                    })
                    .or(case_insensitive_match)?;
                current.push(real_name);
            }
            _ => current.push(component),
        }
    }
    Some(current)
}

/// Matches DB rows whose file has vanished from its recorded path ("orphans")
/// against freshly discovered files that have no DB row yet ("candidates"),
/// using filename + file size as the identity heuristic, and repoints the
/// orphan's `path`/`mtime` in place instead of leaving it for delete+reinsert.
///
/// This is what lets a folder move/split (including across two different
/// watched roots) survive a rescan without losing the song's id — and with
/// it, its play count, rating, and playlist membership. A key only
/// reconciles when exactly one orphan and exactly one candidate share it;
/// ambiguous matches (e.g. duplicate files) are left alone and fall back to
/// the normal insert/prune behavior.
pub(crate) fn reconcile_moved_songs(
    conn: &rusqlite::Connection,
    all_paths: &[PathBuf],
) -> Result<usize> {
    let mut existing_paths_stmt = conn.prepare("SELECT path FROM songs WHERE path IS NOT NULL")?;
    let existing_paths: std::collections::HashSet<String> = existing_paths_stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();
    drop(existing_paths_stmt);

    // Exact-cased paths this scan actually found on disk. Used below instead of
    // `Path::exists()` — on a case-insensitive-but-case-preserving filesystem
    // (Windows, macOS), `exists()` on a stale-cased stored path still resolves
    // to the real file, so a case-only rename (e.g. an album folder renamed to
    // match a retagged album name) would never be recognized as needing a
    // repath, and the next scan would insert a second row for the same file
    // instead of updating the first.
    let disk_paths: std::collections::HashSet<String> = all_paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    let mut candidates: std::collections::HashMap<(String, i64), Vec<&PathBuf>> =
        std::collections::HashMap::new();
    for path in all_paths {
        if existing_paths.contains(&path.to_string_lossy().to_string()) {
            continue;
        }
        let (Some(filename), Ok(metadata)) = (
            path.file_name().map(|f| f.to_string_lossy().to_lowercase()),
            std::fs::metadata(path),
        ) else {
            continue;
        };
        candidates
            .entry((filename, metadata.len() as i64))
            .or_default()
            .push(path);
    }

    if candidates.is_empty() {
        return Ok(0);
    }

    let mut stmt =
        conn.prepare("SELECT id, path, filesize, art_automatic FROM songs WHERE path IS NOT NULL")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;

    type OrphanKey = (String, i64);
    type OrphanRecord = (i64, Option<String>);
    let mut orphans: std::collections::HashMap<OrphanKey, Vec<OrphanRecord>> =
        std::collections::HashMap::new();
    for (id, path, filesize, art_automatic) in rows.flatten() {
        let Some(filesize) = filesize else { continue };
        let p = Path::new(&path);
        if disk_paths.contains(&path) {
            continue;
        }
        let Some(filename) = p.file_name().map(|f| f.to_string_lossy().to_lowercase()) else {
            continue;
        };
        orphans
            .entry((filename, filesize))
            .or_default()
            .push((id, art_automatic));
    }

    let mut reconciled = 0usize;
    let mut update_stmt =
        conn.prepare("UPDATE songs SET path = ?1, mtime = ?2, art_automatic = ?3 WHERE id = ?4")?;
    for (key, orphan_records) in orphans {
        if orphan_records.len() != 1 {
            continue; // ambiguous — multiple missing songs share this filename+size
        }
        let Some(candidate_paths) = candidates.get(&key) else {
            continue;
        };
        if candidate_paths.len() != 1 {
            continue; // ambiguous — multiple new files share this filename+size
        }

        let (orphan_id, old_art_automatic) = &orphan_records[0];
        let new_path = candidate_paths[0];
        let new_path_str = new_path.to_string_lossy().to_string();
        let mtime = super::get_mtime(new_path).unwrap_or(0);

        let new_art_automatic = match old_art_automatic.as_deref() {
            Some(auto) if auto.starts_with("album-") => Some(auto.to_string()),
            _ => crate::covermanager::CoverManager::scan_folder_art_static(new_path)
                .map(|p| p.to_string_lossy().to_string()),
        };

        update_stmt.execute(params![new_path_str, mtime, new_art_automatic, orphan_id])?;
        reconciled += 1;
    }

    Ok(reconciled)
}

/// Watched directory roots that can't currently be read (disconnected drive,
/// sleeping network share, etc). Shared by the scanner's missing-file check
/// (`CollectionScanner::find_missing_song_ids`) and the realtime watcher
/// so a temporary disconnect is never treated as a bulk deletion by either
/// path.
pub(crate) fn unreachable_watched_roots(conn: &rusqlite::Connection) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT path FROM directories") {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for path in rows.flatten() {
                let p = PathBuf::from(&path);
                if std::fs::read_dir(&p).is_err() {
                    roots.push(p);
                }
            }
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{Song, SongSource};
    use std::sync::Arc;

    #[test]
    fn test_reconcile_moved_songs_repaths_unique_match() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_reconcile_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let old_dir = temp_dir.join("old");
        let new_dir = temp_dir.join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        // The file now lives only at `new_path` — `old_path` is what the DB
        // still has recorded, simulating a folder moved to a different
        // watched root between scans.
        let old_path = old_dir.join("track.mp3");
        let new_path = new_dir.join("track.mp3");
        let content = b"pretend audio bytes";
        std::fs::write(&new_path, content).unwrap();

        let song = Song {
            path: Some(old_path.to_string_lossy().to_string()),
            title: Some("Moved Track".to_string()),
            source: SongSource::LocalFile,
            filesize: Some(content.len() as i64),
            ..Default::default()
        };
        super::super::upsert_song(&conn, &song).unwrap();
        let original_id: i64 = conn
            .query_row("SELECT id FROM songs", [], |r| r.get(0))
            .unwrap();
        // upsert_song() never touches playcount (it's only mutated via
        // stats::record_play) — set it directly to prove reconciliation's
        // UPDATE preserves stats tied to the id instead of resetting them.
        conn.execute(
            "UPDATE songs SET playcount = 7 WHERE id = ?1",
            params![original_id],
        )
        .unwrap();

        let reconciled = reconcile_moved_songs(&conn, std::slice::from_ref(&new_path)).unwrap();
        assert_eq!(reconciled, 1);

        let (id, path, playcount): (i64, String, i32) = conn
            .query_row("SELECT id, path, playcount FROM songs", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(id, original_id, "repath must preserve the song's id");
        assert_eq!(path, new_path.to_string_lossy().to_string());
        assert_eq!(
            playcount, 7,
            "repath must not reset stats tied to the song id"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    #[cfg(any(windows, target_os = "macos"))]
    fn test_reconcile_moved_songs_detects_case_only_rename() {
        // Regression test for a duplicate-song bug: renaming an album folder's
        // case only (e.g. "HERO" -> "Hero") on a case-insensitive-but-case-preserving
        // filesystem used to leave the stale-cased DB row undetected as an orphan,
        // because `Path::exists()` on the stale-cased path still resolved to the
        // real (renamed) file. The next scan would then insert a second row for
        // the same physical file instead of repathing the first.
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_reconcile_case_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let album_dir = temp_dir.join("Hero");
        std::fs::create_dir_all(&album_dir).unwrap();

        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let real_path = album_dir.join("track.mp3");
        let content = b"pretend audio bytes";
        std::fs::write(&real_path, content).unwrap();

        // The stale, differently-cased path the DB still has recorded.
        let stale_path = temp_dir.join("HERO").join("track.mp3");
        assert!(
            stale_path.exists(),
            "sanity check: this test requires a case-insensitive filesystem"
        );

        let song = Song {
            path: Some(stale_path.to_string_lossy().to_string()),
            title: Some("Sugar".to_string()),
            source: SongSource::LocalFile,
            filesize: Some(content.len() as i64),
            ..Default::default()
        };
        super::super::upsert_song(&conn, &song).unwrap();
        let original_id: i64 = conn
            .query_row("SELECT id FROM songs", [], |r| r.get(0))
            .unwrap();

        let reconciled = reconcile_moved_songs(&conn, std::slice::from_ref(&real_path)).unwrap();
        assert_eq!(
            reconciled, 1,
            "a case-only rename must be recognized as a repath, not left stale"
        );

        let (id, path): (i64, String) = conn
            .query_row("SELECT id, path FROM songs", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(id, original_id, "repath must preserve the song's id");
        assert_eq!(path, real_path.to_string_lossy().to_string());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_resolve_case_insensitive_path_finds_real_casing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_resolve_case_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let album_dir = temp_dir.join("HERO");
        std::fs::create_dir_all(&album_dir).unwrap();
        let real_path = album_dir.join("Track.mp3");
        std::fs::write(&real_path, b"pretend audio bytes").unwrap();

        // Same path but with the stale casing a stored DB row might have.
        let stale_path = temp_dir.join("Hero").join("track.mp3");

        let resolved = resolve_case_insensitive_path(&stale_path)
            .expect("a case-only mismatch should still resolve to the real file");
        assert_eq!(resolved, real_path);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_resolve_case_insensitive_path_none_when_truly_missing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_resolve_case_missing_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let missing_path = temp_dir.join("Nonexistent").join("track.mp3");
        assert!(resolve_case_insensitive_path(&missing_path).is_none());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_reconcile_moved_songs_skips_ambiguous_matches() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_reconcile_ambiguous_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let old_dir = temp_dir.join("old");
        let new_dir_a = temp_dir.join("new_a");
        let new_dir_b = temp_dir.join("new_b");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir_a).unwrap();
        std::fs::create_dir_all(&new_dir_b).unwrap();

        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        // Two missing rows and two newly discovered files all share the same
        // filename + size — there's no way to know which orphan maps to
        // which candidate, so reconciliation must leave both alone.
        let content = b"duplicate track bytes";
        let new_path_a = new_dir_a.join("track.mp3");
        let new_path_b = new_dir_b.join("track.mp3");
        std::fs::write(&new_path_a, content).unwrap();
        std::fs::write(&new_path_b, content).unwrap();

        for i in 0..2 {
            let song = Song {
                path: Some(
                    old_dir
                        .join(format!("orphan{i}/track.mp3"))
                        .to_string_lossy()
                        .to_string(),
                ),
                title: Some(format!("Orphan {i}")),
                source: SongSource::LocalFile,
                filesize: Some(content.len() as i64),
                ..Default::default()
            };
            super::super::upsert_song(&conn, &song).unwrap();
        }

        let reconciled =
            reconcile_moved_songs(&conn, &[new_path_a.clone(), new_path_b.clone()]).unwrap();
        assert_eq!(
            reconciled, 0,
            "ambiguous filename+size matches must not be guessed at"
        );

        let mut stmt = conn
            .prepare("SELECT path FROM songs ORDER BY path")
            .unwrap();
        let paths: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(
            paths
                .iter()
                .all(|p| p.starts_with(&old_dir.to_string_lossy().to_string())),
            "orphan rows must be untouched, got {paths:?}"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_reconcile_moved_songs_refreshes_folder_cover_art() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_reconcile_folder_art_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let old_dir = temp_dir.join("old");
        let new_dir = temp_dir.join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let old_path = old_dir.join("track.mp3");
        let old_cover = old_dir.join("cover.jpg");
        let new_path = new_dir.join("track.mp3");
        let new_cover = new_dir.join("cover.jpg");
        let content = b"audio bytes for folder art test";
        std::fs::write(&new_path, content).unwrap();
        std::fs::write(&new_cover, b"new image bytes").unwrap();

        let song = Song {
            path: Some(old_path.to_string_lossy().to_string()),
            title: Some("Folder Art Track".to_string()),
            source: SongSource::LocalFile,
            filesize: Some(content.len() as i64),
            art_automatic: Some(old_cover.to_string_lossy().to_string()),
            ..Default::default()
        };
        super::super::upsert_song(&conn, &song).unwrap();

        let reconciled = reconcile_moved_songs(&conn, std::slice::from_ref(&new_path)).unwrap();
        assert_eq!(reconciled, 1);

        let (path, art_automatic): (String, Option<String>) = conn
            .query_row("SELECT path, art_automatic FROM songs", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(path, new_path.to_string_lossy().to_string());
        assert!(art_automatic.is_some());
        let art_path = art_automatic.unwrap();
        assert!(
            art_path.starts_with(&new_dir.to_string_lossy().to_string()),
            "art_automatic must point to the new folder's cover image, got: {art_path}"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_reconcile_moved_songs_clears_folder_cover_art_when_new_folder_has_none() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_reconcile_clear_art_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let old_dir = temp_dir.join("old");
        let new_dir = temp_dir.join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let old_path = old_dir.join("track.mp3");
        let old_cover = old_dir.join("cover.jpg");
        let new_path = new_dir.join("track.mp3");
        let content = b"audio bytes for clear art test";
        std::fs::write(&new_path, content).unwrap();

        let song = Song {
            path: Some(old_path.to_string_lossy().to_string()),
            title: Some("No Art Track".to_string()),
            source: SongSource::LocalFile,
            filesize: Some(content.len() as i64),
            art_automatic: Some(old_cover.to_string_lossy().to_string()),
            ..Default::default()
        };
        super::super::upsert_song(&conn, &song).unwrap();

        let reconciled = reconcile_moved_songs(&conn, std::slice::from_ref(&new_path)).unwrap();
        assert_eq!(reconciled, 1);

        let (path, art_automatic): (String, Option<String>) = conn
            .query_row("SELECT path, art_automatic FROM songs", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(path, new_path.to_string_lossy().to_string());
        assert_eq!(
            art_automatic, None,
            "art_automatic must be cleared when new directory has no cover art"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_reconcile_moved_songs_preserves_embedded_cover_art() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_reconcile_embedded_art_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let old_dir = temp_dir.join("old");
        let new_dir = temp_dir.join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let old_path = old_dir.join("track.mp3");
        let new_path = new_dir.join("track.mp3");
        let content = b"audio bytes for embedded art test";
        std::fs::write(&new_path, content).unwrap();

        let song = Song {
            path: Some(old_path.to_string_lossy().to_string()),
            title: Some("Embedded Art Track".to_string()),
            source: SongSource::LocalFile,
            filesize: Some(content.len() as i64),
            art_embedded: true,
            art_automatic: Some("album-1234567890abcdef.jpg".to_string()),
            ..Default::default()
        };
        super::super::upsert_song(&conn, &song).unwrap();

        let reconciled = reconcile_moved_songs(&conn, std::slice::from_ref(&new_path)).unwrap();
        assert_eq!(reconciled, 1);

        let (path, art_automatic): (String, Option<String>) = conn
            .query_row("SELECT path, art_automatic FROM songs", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(path, new_path.to_string_lossy().to_string());
        assert_eq!(
            art_automatic.as_deref(),
            Some("album-1234567890abcdef.jpg"),
            "embedded cached artwork filename must be preserved on repath"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
