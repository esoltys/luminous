//! Folders worth offering as library folders (#1871). A user who only ever
//! opens files straight from disk builds up `OpenedFile` rows (#1842); each
//! parent folder of such rows is a folder they already listen to and could add
//! to the library, upgrading those rows in place (ids, play history and ratings
//! intact — see `upsert_song`).
//!
//! [`suggested_folders`] owns every filtering rule, so the UI shows exactly
//! what it returns: folders on an unplugged drive can't be scanned, folders
//! already watched have nothing to add, and a dismissed folder or the global
//! hide stays dismissed (persisted in `app_state`). The global hide can be
//! turned back off ([`set_hidden`]); per-folder dismissals survive that.

use crate::models::SongSource;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// `app_state` key holding a JSON array of folder paths the user dismissed.
const DISMISSED_KEY: &str = "library_folder_suggestions_dismissed";
/// `app_state` key set to `"true"` once the user hides the suggestions for good.
const HIDDEN_KEY: &str = "library_folder_suggestions_hidden";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuggestedFolder {
    pub path: String,
    /// Opened files recorded in this folder (not its subfolders).
    pub song_count: u32,
    /// Most recent play of any of them, Unix seconds.
    pub last_played: Option<i64>,
}

/// Case- and separator-insensitive on Windows, exact elsewhere — the same
/// equivalence `WatchedFolders` uses, so one folder never gets two keys.
fn folder_key(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    } else {
        text.trim_end_matches('/').to_string()
    }
}

fn read_setting(conn: &rusqlite::Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()?)
}

fn write_setting(conn: &rusqlite::Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

fn dismissed_keys(conn: &rusqlite::Connection) -> Result<Vec<String>> {
    Ok(read_setting(conn, DISMISSED_KEY)?
        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .unwrap_or_default())
}

/// Parent folders of the user's opened files, most recently played first, that
/// are reachable, not yet watched and not dismissed — at most `limit`.
pub fn suggested_folders(
    conn: &rusqlite::Connection,
    limit: usize,
) -> Result<Vec<SuggestedFolder>> {
    if is_hidden(conn)? {
        return Ok(Vec::new());
    }
    let dismissed = dismissed_keys(conn)?;
    let watched = super::WatchedFolders::load(conn)?;

    let mut stmt = conn.prepare(
        "SELECT path, lastplayed FROM songs
         WHERE source = ?1 AND path IS NOT NULL AND unavailable = 0",
    )?;
    let rows = stmt.query_map(params![SongSource::OPENED_FILE_ID], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
    })?;

    let mut groups: HashMap<String, SuggestedFolder> = HashMap::new();
    for row in rows {
        let (path, last_played) = row?;
        let Some(parent) = Path::new(&path).parent() else {
            continue;
        };
        let key = folder_key(parent);
        let group = groups.entry(key).or_insert_with(|| SuggestedFolder {
            path: parent.to_string_lossy().to_string(),
            song_count: 0,
            last_played: None,
        });
        group.song_count += 1;
        group.last_played = group.last_played.max(last_played);
    }

    // A folder already watched is a library folder, not a suggestion. Both
    // checks so far are in memory.
    let mut candidates: Vec<SuggestedFolder> = groups
        .into_iter()
        .filter(|(key, _)| !dismissed.contains(key))
        .map(|(_, folder)| folder)
        .filter(|f| watched.source_for(Path::new(&f.path)) == SongSource::OpenedFile)
        .collect();
    // Most recent first, then busiest, then by path so the order is stable.
    candidates.sort_by(|a, b| {
        b.last_played
            .cmp(&a.last_played)
            .then(b.song_count.cmp(&a.song_count))
            .then(a.path.cmp(&b.path))
    });
    // Reachability touches the disk, and a dead network share can stall for
    // seconds, so check lazily and stop at `limit` rather than probing every
    // folder. An unplugged drive (or moved folder) can't be scanned.
    Ok(candidates
        .into_iter()
        .filter(|f| Path::new(&f.path).is_dir())
        .take(limit)
        .collect())
}

/// "Not now": never offer `path` again.
pub fn dismiss_folder(conn: &rusqlite::Connection, path: &str) -> Result<()> {
    let key = folder_key(Path::new(path));
    let mut dismissed = dismissed_keys(conn)?;
    if !dismissed.contains(&key) {
        dismissed.push(key);
        write_setting(conn, DISMISSED_KEY, &serde_json::to_string(&dismissed)?)?;
    }
    Ok(())
}

/// Whether the user has turned suggestions off (the Home card's "Hide
/// suggestions" link, or the Settings toggle).
pub fn is_hidden(conn: &rusqlite::Connection) -> Result<bool> {
    Ok(read_setting(conn, HIDDEN_KEY)?.as_deref() == Some("true"))
}

/// Turn suggestions off (`true`) or back on (`false`). Per-folder "Not now"
/// dismissals are left alone, so folders the user declined stay declined.
pub fn set_hidden(conn: &rusqlite::Connection, hidden: bool) -> Result<()> {
    write_setting(conn, HIDDEN_KEY, if hidden { "true" } else { "false" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::Song;
    use std::path::PathBuf;

    struct Fixture {
        _guard: tempfile::TempDir,
        root: PathBuf,
        db: Database,
    }

    fn fixture() -> Fixture {
        let guard = tempfile::Builder::new()
            .prefix("luminous_suggestions_test_")
            .tempdir()
            .unwrap();
        let root = guard.path().to_path_buf();
        let db = Database::new(root.join("data")).unwrap();
        Fixture {
            _guard: guard,
            root,
            db,
        }
    }

    /// Creates `dir` on disk and records one opened file in it.
    fn open_file(
        conn: &rusqlite::Connection,
        dir: &Path,
        name: &str,
        last_played: Option<i64>,
    ) -> i64 {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        let song = Song {
            path: Some(path.to_string_lossy().to_string()),
            title: Some(name.to_string()),
            source: SongSource::OpenedFile,
            ..Default::default()
        };
        super::super::upsert_song(conn, &song).unwrap();
        let id: i64 = conn
            .query_row(
                "SELECT id FROM songs WHERE path = ?1",
                params![path.to_string_lossy()],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "UPDATE songs SET lastplayed = ?1 WHERE id = ?2",
            params![last_played, id],
        )
        .unwrap();
        id
    }

    #[test]
    fn groups_by_parent_folder_most_recent_first() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        let old = f.root.join("old");
        let recent = f.root.join("recent");
        open_file(&conn, &old, "a.mp3", Some(100));
        open_file(&conn, &old, "b.mp3", Some(300));
        open_file(&conn, &recent, "c.mp3", Some(500));

        let folders = suggested_folders(&conn, 10).unwrap();
        assert_eq!(
            folders,
            vec![
                SuggestedFolder {
                    path: recent.to_string_lossy().to_string(),
                    song_count: 1,
                    last_played: Some(500),
                },
                SuggestedFolder {
                    path: old.to_string_lossy().to_string(),
                    song_count: 2,
                    last_played: Some(300),
                },
            ]
        );
        assert_eq!(
            suggested_folders(&conn, 1).unwrap().len(),
            1,
            "limit applies"
        );
    }

    #[test]
    fn skips_unreachable_watched_dismissed_and_library_rows() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        let gone = f.root.join("gone");
        let watched = f.root.join("watched");
        let dismissed = f.root.join("dismissed");
        let kept = f.root.join("kept");
        open_file(&conn, &gone, "a.mp3", Some(1));
        open_file(&conn, &watched, "a.mp3", Some(1));
        open_file(&conn, &dismissed, "a.mp3", Some(1));
        open_file(&conn, &kept, "a.mp3", Some(1));

        // An unplugged drive: the folder no longer exists.
        std::fs::remove_dir_all(&gone).unwrap();
        // A watched folder (and its subfolders) is already a library folder.
        conn.execute(
            "INSERT INTO directories (path) VALUES (?1)",
            params![watched.to_string_lossy()],
        )
        .unwrap();
        open_file(&conn, &watched.join("sub"), "b.mp3", Some(1));
        dismiss_folder(&conn, &dismissed.to_string_lossy()).unwrap();
        // A library row is never an opened file.
        let library = f.root.join("library");
        std::fs::create_dir_all(&library).unwrap();
        let song = Song {
            path: Some(library.join("l.mp3").to_string_lossy().to_string()),
            source: SongSource::LocalFile,
            ..Default::default()
        };
        super::super::upsert_song(&conn, &song).unwrap();

        let folders = suggested_folders(&conn, 10).unwrap();
        assert_eq!(
            folders.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            vec![kept.to_string_lossy().to_string()]
        );
    }

    #[test]
    fn dismissal_persists_and_is_idempotent() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        let dir = f.root.join("music");
        open_file(&conn, &dir, "a.mp3", None);
        assert_eq!(suggested_folders(&conn, 10).unwrap().len(), 1);

        dismiss_folder(&conn, &dir.to_string_lossy()).unwrap();
        dismiss_folder(&conn, &dir.to_string_lossy()).unwrap();
        assert!(suggested_folders(&conn, 10).unwrap().is_empty());
        assert_eq!(dismissed_keys(&conn).unwrap().len(), 1);
    }

    #[test]
    fn hiding_silences_every_suggestion() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        open_file(&conn, &f.root.join("a"), "a.mp3", None);
        open_file(&conn, &f.root.join("b"), "b.mp3", None);
        assert!(!is_hidden(&conn).unwrap());
        assert_eq!(suggested_folders(&conn, 10).unwrap().len(), 2);

        set_hidden(&conn, true).unwrap();
        assert!(is_hidden(&conn).unwrap());
        assert!(suggested_folders(&conn, 10).unwrap().is_empty());
    }

    #[test]
    fn showing_again_keeps_dismissed_folders_dismissed() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        let declined = f.root.join("declined");
        let other = f.root.join("other");
        open_file(&conn, &declined, "a.mp3", None);
        open_file(&conn, &other, "b.mp3", None);
        dismiss_folder(&conn, &declined.to_string_lossy()).unwrap();

        set_hidden(&conn, true).unwrap();
        set_hidden(&conn, false).unwrap();

        assert!(!is_hidden(&conn).unwrap());
        let folders = suggested_folders(&conn, 10).unwrap();
        assert_eq!(
            folders.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            vec![other.to_string_lossy().to_string()]
        );
    }

    /// The nudge's promise: adding the folder keeps the opened files' ids, so
    /// play history, ratings and stats survive the upgrade (#1842 upgrade path).
    #[test]
    fn watching_a_suggested_folder_upgrades_rows_in_place() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        let dir = f.root.join("music");
        let id = open_file(&conn, &dir, "a.mp3", Some(42));
        conn.execute(
            "UPDATE songs SET playcount = 7, rating = 0.8 WHERE id = ?1",
            params![id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO directories (path) VALUES (?1)",
            params![dir.to_string_lossy()],
        )
        .unwrap();

        // The scan re-upserts the file as a library row.
        let song = Song {
            path: Some(dir.join("a.mp3").to_string_lossy().to_string()),
            title: Some("a.mp3".to_string()),
            source: SongSource::LocalFile,
            ..Default::default()
        };
        super::super::upsert_song(&conn, &song).unwrap();

        let (source, playcount, rating, lastplayed, rows): (i32, i64, f64, i64, i64) = conn
            .query_row(
                "SELECT source, playcount, rating, lastplayed,
                        (SELECT COUNT(*) FROM songs) FROM songs WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(source, SongSource::LOCAL_FILE_ID);
        assert_eq!((playcount, lastplayed, rows), (7, 42, 1));
        assert!((rating - 0.8).abs() < 1e-9);
        assert!(suggested_folders(&conn, 10).unwrap().is_empty());
    }
}
