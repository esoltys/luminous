//! Re-linking a watched folder whose location changed (#1403) — an external
//! or USB drive that mounts under a different letter, or a library moved to
//! another disk. Every absolute path the database holds for the folder is
//! rewritten in place, so songs keep their ids and with them their play
//! counts, ratings and playlist membership. A plain "add the new folder"
//! would insert fresh rows instead and leave all of that behind.

use super::reconcile::merge_song_rows;
use super::LOCAL_SOURCES_SQL;
use anyhow::{anyhow, Result};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

/// `app_state` key holding the volume the portable executable last ran from
/// (e.g. `E:\`), so a launch from a different drive letter can tell what moved.
pub const PORTABLE_VOLUME_KEY: &str = "portable_volume_root";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RelocateResult {
    /// Song rows whose paths now point under the new location.
    pub songs_relocated: usize,
    /// Rows already scanned at the new location (the folder had also been
    /// added there) that were folded into the relocated rows.
    pub songs_merged: usize,
    /// Smart playlists whose folder rules were rewritten.
    pub playlists_updated: usize,
    /// The default library's new path, when it was the relocated folder (or
    /// inside it) — the caller re-attaches the hierarchy sidecar there.
    pub default_library: Option<String>,
}

/// `path` re-rooted from `old_root` to `new_root`, or `None` if it isn't
/// `old_root` itself or inside it.
fn rebase(path: &str, old_root: &Path, new_root: &Path) -> Option<String> {
    let rest = Path::new(path).strip_prefix(old_root).ok()?;
    let rebased = if rest.as_os_str().is_empty() {
        new_root.to_path_buf()
    } else {
        new_root.join(rest)
    };
    Some(rebased.to_string_lossy().to_string())
}

/// Replaces every occurrence of `old` in `text` that ends at a path boundary
/// (a separator, quote, whitespace, `)` or the end), so `E:\Music` never
/// rewrites `E:\Music2`.
fn replace_path_prefix(text: &str, old: &str, new: &str) -> String {
    if old.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(old) {
        let after = &rest[i + old.len()..];
        let at_boundary = after
            .chars()
            .next()
            .is_none_or(|c| matches!(c, '\\' | '/' | '"' | ')') || c.is_whitespace());
        out.push_str(&rest[..i]);
        out.push_str(if at_boundary { new } else { old });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Rewrites folder paths inside a smart playlist spec (`folder:="E:\\Music"`)
/// in each spelling a user or "Create Smart Playlist from folder" may have
/// written: escaped backslashes, raw backslashes, forward slashes.
fn rewrite_spec(spec: &str, old_root: &Path, new_root: &Path) -> String {
    let old = old_root.to_string_lossy();
    let new = new_root.to_string_lossy();
    let mut spellings: Vec<(String, String)> = vec![
        (old.replace('\\', "\\\\"), new.replace('\\', "\\\\")),
        (old.to_string(), new.to_string()),
        (old.replace('\\', "/"), new.replace('\\', "/")),
    ];
    spellings.dedup();
    spellings.iter().fold(spec.to_string(), |acc, (o, n)| {
        replace_path_prefix(&acc, o, n)
    })
}

/// Moves watched folder `old_root` to `new_root` in one transaction: the
/// `directories` row, every local song's `path` plus its folder-art, manual-art
/// and CUE paths, folder rules in smart playlists, and the default library
/// setting. Relocated songs are flagged available or not by whether their file
/// exists at the new path; the next scan picks up anything else.
///
/// If `new_root` is already a watched folder in its own right (the user added
/// the drive's new letter before relocating), the two are merged: the old
/// folder's row goes, keeping its nickname/icon/colour where the new one has
/// none, and a song scanned at both locations keeps the old row — the one with
/// the history — with the new row's stats folded in.
pub fn relocate_root(
    conn: &rusqlite::Connection,
    old_root: &Path,
    new_root: &Path,
) -> Result<RelocateResult> {
    let old_str = old_root.to_string_lossy().to_string();
    let new_str = new_root.to_string_lossy().to_string();
    if old_root == new_root {
        return Err(anyhow!("{new_str} is already this folder's location"));
    }
    if !new_root.is_dir() {
        return Err(anyhow!("{new_str} is not an available folder"));
    }

    let tx = conn.unchecked_transaction()?;

    type DirRow = (i64, Option<String>, Option<String>, Option<String>);
    let old_dir: Option<DirRow> = tx
        .query_row(
            "SELECT id, nickname, icon, color FROM directories WHERE path = ?1",
            params![old_str],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((old_id, nickname, icon, color)) = old_dir else {
        return Err(anyhow!("{old_str} is not a watched folder"));
    };
    let existing_new_id: Option<i64> = tx
        .query_row(
            "SELECT id FROM directories WHERE path = ?1",
            params![new_str],
            |r| r.get(0),
        )
        .optional()?;
    match existing_new_id {
        Some(new_id) => {
            tx.execute(
                "UPDATE directories SET
                    nickname = COALESCE(nickname, ?1),
                    icon = COALESCE(icon, ?2),
                    color = COALESCE(color, ?3)
                 WHERE id = ?4",
                params![nickname, icon, color, new_id],
            )?;
            tx.execute("DELETE FROM directories WHERE id = ?1", params![old_id])?;
        }
        None => {
            tx.execute(
                "UPDATE directories SET path = ?1 WHERE id = ?2",
                params![new_str, old_id],
            )?;
            tx.execute(
                "DELETE FROM subdirectories WHERE directory_id = ?1",
                params![old_id],
            )?;
        }
    }

    let mut result = RelocateResult::default();

    type SongPaths = (
        i64,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let songs: Vec<SongPaths> = {
        let sql = format!(
            "SELECT id, path, beginning_nanosec, art_automatic, art_manual, cue_path
             FROM songs WHERE path IS NOT NULL AND source IN ({lib})",
            lib = *LOCAL_SOURCES_SQL
        );
        let mut stmt = tx.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    {
        let mut find_existing = tx.prepare(
            "SELECT id FROM songs WHERE path = ?1 AND beginning_nanosec = ?2 AND id != ?3",
        )?;
        let mut update = tx.prepare(
            "UPDATE songs SET path = ?1, art_automatic = ?2, art_manual = ?3, cue_path = ?4,
                    unavailable = ?5
             WHERE id = ?6",
        )?;
        let rebase_opt = |p: &Option<String>| -> Option<String> {
            p.as_deref()
                .map(|s| rebase(s, old_root, new_root).unwrap_or_else(|| s.to_string()))
        };
        for (id, path, beginning, art_automatic, art_manual, cue_path) in &songs {
            let Some(new_path) = rebase(path, old_root, new_root) else {
                continue;
            };
            let already_there: Option<i64> = find_existing
                .query_row(params![new_path, beginning, id], |r| r.get(0))
                .optional()?;
            if let Some(dup) = already_there {
                merge_song_rows(&tx, *id, dup)?;
                result.songs_merged += 1;
            }
            let unavailable = !Path::new(&new_path).exists();
            update.execute(params![
                new_path,
                rebase_opt(art_automatic),
                rebase_opt(art_manual),
                rebase_opt(cue_path),
                unavailable,
                id
            ])?;
            result.songs_relocated += 1;
        }
    }

    {
        let specs: Vec<(i64, String)> = {
            let mut stmt = tx
                .prepare("SELECT id, dynamic_spec FROM playlists WHERE dynamic_spec IS NOT NULL")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut update = tx.prepare("UPDATE playlists SET dynamic_spec = ?1 WHERE id = ?2")?;
        for (id, spec) in specs {
            let rewritten = rewrite_spec(&spec, old_root, new_root);
            if rewritten != spec {
                update.execute(params![rewritten, id])?;
                result.playlists_updated += 1;
            }
        }
    }

    let default_library: Option<String> = tx
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![crate::hierarchy_sidecar::SETTING_KEY],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(new_default) = default_library
        .filter(|d| !d.is_empty())
        .and_then(|d| rebase(&d, old_root, new_root))
    {
        tx.execute(
            "UPDATE app_state SET value = ?1 WHERE key = ?2",
            params![new_default, crate::hierarchy_sidecar::SETTING_KEY],
        )?;
        result.default_library = Some(new_default);
    }

    tx.commit()?;
    log::info!(
        "Relocated watched folder {old_str} -> {new_str}: {} song(s), {} merged, {} smart playlist(s)",
        result.songs_relocated,
        result.songs_merged,
        result.playlists_updated
    );
    Ok(result)
}

/// The volume `path` lives on, as a root path (`E:\`, `\\server\share\`).
/// `None` on platforms without path prefixes, where there's no drive letter
/// to change.
pub fn volume_root(path: &Path) -> Option<PathBuf> {
    match path.components().next()? {
        Component::Prefix(prefix) => {
            let mut root = PathBuf::from(prefix.as_os_str());
            root.push(std::path::MAIN_SEPARATOR_STR);
            Some(root)
        }
        _ => None,
    }
}

/// Portable mode: watched folders on the same drive as the executable follow
/// it when that drive comes back under a different letter. Records
/// `current_root` as the executable's volume; if the previous launch ran from
/// another one, each watched folder under that old volume is relocated to the
/// same path on `current_root` — but only when the old location can no longer
/// be read and the new one exists, so a different drive that now holds the
/// old letter (or a folder that happens to exist on both) is never hijacked.
/// Runs at startup, before anything scans, watches or plays.
pub fn relink_portable_volume(
    conn: &rusqlite::Connection,
    current_root: &Path,
) -> Result<Vec<(String, String)>> {
    let current = current_root.to_string_lossy().to_string();
    let previous: Option<String> = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![PORTABLE_VOLUME_KEY],
            |r| r.get(0),
        )
        .optional()?;

    let mut relinked = Vec::new();
    if let Some(previous) = previous.filter(|p| !p.eq_ignore_ascii_case(&current)) {
        let previous_root = PathBuf::from(&previous);
        let dirs: Vec<String> = {
            let mut stmt = conn.prepare("SELECT path FROM directories")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for dir in dirs {
            let Some(candidate) = rebase(&dir, &previous_root, current_root) else {
                continue;
            };
            if std::fs::read_dir(&dir).is_ok() || !Path::new(&candidate).is_dir() {
                continue;
            }
            match relocate_root(conn, Path::new(&dir), Path::new(&candidate)) {
                Ok(_) => relinked.push((dir, candidate)),
                Err(e) => log::warn!("Couldn't re-link {dir} to {candidate}: {e:#}"),
            }
        }
    }

    conn.execute(
        "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
        params![PORTABLE_VOLUME_KEY, current],
    )?;
    Ok(relinked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{Song, SongSource};

    struct Fixture {
        _guard: tempfile::TempDir,
        root: PathBuf,
        db: Database,
    }

    fn fixture() -> Fixture {
        let guard = tempfile::Builder::new()
            .prefix("luminous_relocate_test_")
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

    fn add_song(conn: &rusqlite::Connection, path: &Path) -> i64 {
        let song = Song {
            path: Some(path.to_string_lossy().to_string()),
            title: Some("Track".to_string()),
            source: SongSource::LocalFile,
            ..Default::default()
        };
        super::super::upsert_song(conn, &song).unwrap();
        conn.query_row(
            "SELECT id FROM songs WHERE path = ?1",
            params![path.to_string_lossy()],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn watch(conn: &rusqlite::Connection, dir: &Path) {
        conn.execute(
            "INSERT INTO directories (path, subdirs) VALUES (?1, 1)",
            params![dir.to_string_lossy()],
        )
        .unwrap();
    }

    fn song_row(conn: &rusqlite::Connection, id: i64) -> (String, bool, i64) {
        conn.query_row(
            "SELECT path, unavailable, playcount FROM songs WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
    }

    #[test]
    fn test_relocate_root_keeps_song_ids_and_rewrites_paths() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        let old_root = f.root.join("E").join("Music");
        let new_root = f.root.join("F").join("Music");
        std::fs::create_dir_all(new_root.join("Album")).unwrap();
        std::fs::write(new_root.join("Album").join("a.mp3"), b"x").unwrap();

        watch(&conn, &old_root);
        let id = add_song(&conn, &old_root.join("Album").join("a.mp3"));
        let gone = add_song(&conn, &old_root.join("Album").join("gone.mp3"));
        let elsewhere = add_song(&conn, &f.root.join("Other").join("b.mp3"));
        conn.execute(
            "UPDATE songs SET playcount = 5, unavailable = 1, cue_path = ?1 WHERE id = ?2",
            params![old_root.join("Album").join("a.cue").to_string_lossy(), id],
        )
        .unwrap();

        let result = relocate_root(&conn, &old_root, &new_root).unwrap();
        assert_eq!(result.songs_relocated, 2);
        assert_eq!(result.songs_merged, 0);

        let (path, unavailable, playcount) = song_row(&conn, id);
        assert_eq!(path, new_root.join("Album").join("a.mp3").to_string_lossy());
        assert!(
            !unavailable,
            "a file present at the new location is available"
        );
        assert_eq!(playcount, 5, "stats stay with the song id");
        let cue: String = conn
            .query_row(
                "SELECT cue_path FROM songs WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cue, new_root.join("Album").join("a.cue").to_string_lossy());

        let (_, gone_unavailable, _) = song_row(&conn, gone);
        assert!(
            gone_unavailable,
            "a file missing at the new location stays unavailable"
        );
        let (other_path, _, _) = song_row(&conn, elsewhere);
        assert_eq!(
            other_path,
            f.root.join("Other").join("b.mp3").to_string_lossy()
        );

        let dirs: Vec<String> = conn
            .prepare("SELECT path FROM directories")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(dirs, vec![new_root.to_string_lossy().to_string()]);
    }

    #[test]
    fn test_relocate_root_merges_rows_already_scanned_at_new_location() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        let old_root = f.root.join("E").join("Music");
        let new_root = f.root.join("F").join("Music");
        std::fs::create_dir_all(&new_root).unwrap();
        std::fs::write(new_root.join("a.mp3"), b"x").unwrap();

        watch(&conn, &old_root);
        watch(&conn, &new_root);
        conn.execute(
            "UPDATE directories SET nickname = 'Stick' WHERE path = ?1",
            params![old_root.to_string_lossy()],
        )
        .unwrap();
        let old_id = add_song(&conn, &old_root.join("a.mp3"));
        let new_id = add_song(&conn, &new_root.join("a.mp3"));
        conn.execute(
            "UPDATE songs SET playcount = 5, rating = 0.8 WHERE id = ?1",
            params![old_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE songs SET playcount = 2, loved = 1 WHERE id = ?1",
            params![new_id],
        )
        .unwrap();
        conn.execute("INSERT INTO playlists (name) VALUES ('Mix')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type)
             VALUES (last_insert_rowid(), ?1, 0, 'u1', 0)",
            params![new_id],
        )
        .unwrap();

        let result = relocate_root(&conn, &old_root, &new_root).unwrap();
        assert_eq!(result.songs_merged, 1);

        let (path, _, playcount) = song_row(&conn, old_id);
        assert_eq!(path, new_root.join("a.mp3").to_string_lossy());
        assert_eq!(playcount, 7, "play counts from both rows add up");
        let (rating, loved): (f64, bool) = conn
            .query_row(
                "SELECT rating, loved FROM songs WHERE id = ?1",
                params![old_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(rating, 0.8);
        assert!(loved);
        let survivors: i64 = conn
            .query_row("SELECT COUNT(*) FROM songs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(survivors, 1);
        let item_song: i64 = conn
            .query_row("SELECT song_id FROM playlist_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            item_song, old_id,
            "playlist entries follow the surviving row"
        );

        let (count, nickname): (i64, Option<String>) = conn
            .query_row("SELECT COUNT(*), MAX(nickname) FROM directories", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(nickname.as_deref(), Some("Stick"));
    }

    #[test]
    fn test_relocate_root_rewrites_smart_playlists_and_default_library() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        let old_root = f.root.join("E").join("Music");
        let new_root = f.root.join("F").join("Music");
        std::fs::create_dir_all(&new_root).unwrap();
        watch(&conn, &old_root);

        let old = old_root.to_string_lossy().to_string();
        let new = new_root.to_string_lossy().to_string();
        let escaped = |s: &str| s.replace('\\', "\\\\");
        let spec = format!("folder:=\"{}\" genre:Rock", escaped(&old));
        let sibling = format!("folder:=\"{}2\"", escaped(&old));
        conn.execute(
            "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec) VALUES ('A', 1, ?1), ('B', 1, ?2)",
            params![spec, sibling],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_state (key, value) VALUES (?1, ?2)",
            params![crate::hierarchy_sidecar::SETTING_KEY, old],
        )
        .unwrap();

        let result = relocate_root(&conn, &old_root, &new_root).unwrap();
        assert_eq!(result.playlists_updated, 1);
        assert_eq!(result.default_library.as_deref(), Some(new.as_str()));

        let specs: Vec<String> = conn
            .prepare("SELECT dynamic_spec FROM playlists ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            specs[0],
            format!("folder:=\"{}\" genre:Rock", escaped(&new))
        );
        assert_eq!(
            specs[1], sibling,
            "a sibling folder sharing the prefix is untouched"
        );
    }

    #[test]
    fn test_relocate_root_refuses_unwatched_or_missing_folders() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        let old_root = f.root.join("E").join("Music");
        let new_root = f.root.join("F").join("Music");

        watch(&conn, &old_root);
        assert!(
            relocate_root(&conn, &old_root, &new_root).is_err(),
            "the new location must exist"
        );
        std::fs::create_dir_all(&new_root).unwrap();
        assert!(
            relocate_root(&conn, &f.root.join("Unwatched"), &new_root).is_err(),
            "only a watched folder can be relocated"
        );
        assert!(relocate_root(&conn, &new_root, &new_root).is_err());
    }

    #[test]
    fn test_replace_path_prefix_respects_boundaries() {
        assert_eq!(
            replace_path_prefix("E:/Music", "E:/Music", "F:/Music"),
            "F:/Music"
        );
        assert_eq!(
            replace_path_prefix("a E:/Music/x E:/Music2", "E:/Music", "F:/Music"),
            "a F:/Music/x E:/Music2"
        );
    }

    #[test]
    fn test_relink_portable_volume_follows_the_executables_drive() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        // Stand-ins for two drive letters the same stick mounted under.
        let old_volume = f.root.join("E");
        let new_volume = f.root.join("F");
        std::fs::create_dir_all(new_volume.join("Music")).unwrap();
        std::fs::create_dir_all(f.root.join("D").join("Other")).unwrap();
        let on_stick = old_volume.join("Music");
        let on_other_drive = f.root.join("D").join("Other");
        watch(&conn, &on_stick);
        watch(&conn, &on_other_drive);

        // First launch only records the volume.
        assert!(relink_portable_volume(&conn, &old_volume)
            .unwrap()
            .is_empty());
        // Same volume again: nothing to do.
        assert!(relink_portable_volume(&conn, &old_volume)
            .unwrap()
            .is_empty());

        let relinked = relink_portable_volume(&conn, &new_volume).unwrap();
        assert_eq!(
            relinked,
            vec![(
                on_stick.to_string_lossy().to_string(),
                new_volume.join("Music").to_string_lossy().to_string()
            )]
        );
        let stored: String = conn
            .query_row(
                "SELECT value FROM app_state WHERE key = ?1",
                params![PORTABLE_VOLUME_KEY],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, new_volume.to_string_lossy());
    }

    #[test]
    fn test_relink_portable_volume_leaves_a_still_readable_folder_alone() {
        let f = fixture();
        let conn = f.db.pool.get().unwrap();
        let old_volume = f.root.join("E");
        let new_volume = f.root.join("F");
        // Both exist: the old letter now belongs to some other drive that
        // happens to hold the same folder — never hijack it.
        std::fs::create_dir_all(old_volume.join("Music")).unwrap();
        std::fs::create_dir_all(new_volume.join("Music")).unwrap();
        watch(&conn, &old_volume.join("Music"));

        relink_portable_volume(&conn, &old_volume).unwrap();
        assert!(relink_portable_volume(&conn, &new_volume)
            .unwrap()
            .is_empty());
    }

    #[test]
    #[cfg(windows)]
    fn test_volume_root_is_the_drive() {
        assert_eq!(
            volume_root(Path::new(r"E:\PortableApps\Luminous")),
            Some(PathBuf::from(r"E:\"))
        );
        assert_eq!(
            volume_root(Path::new(r"\\nas\share\Luminous")),
            Some(PathBuf::from(r"\\nas\share\"))
        );
    }
}
