//! Tag change history tracking module (#1442).
//!
//! Records and prunes per-track tag change events across both user edits
//! (tag editor, album editor, Files view, tag clean-up) and external scan updates.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_EVENTS_PER_SONG: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryActor {
    Luminous,
    Scan,
}

impl HistoryActor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Luminous => "luminous",
            Self::Scan => "scan",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongHistorySource {
    pub path: String,
    pub filesize: Option<i64>,
    pub format: String,
    pub added: Option<i64>,
    pub is_remote: bool,
    pub is_cue_track: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongHistoryEntry {
    pub id: i64,
    pub song_id: i64,
    pub at: i64,
    pub actor: HistoryActor,
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub batch_id: Option<String>,
    pub batch_other_tracks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongTagHistory {
    pub song_id: i64,
    pub source: SongHistorySource,
    pub entries: Vec<SongHistoryEntry>,
}

/// 22 tracked tag fields in stable order.
pub const TRACKED_FIELDS: &[&str] = &[
    "title",
    "titlesort",
    "artist",
    "artistsort",
    "album",
    "albumsort",
    "album_artist",
    "album_artist_sort",
    "composer",
    "composersort",
    "genre",
    "genresort",
    "track",
    "disc",
    "disc_subtitle",
    "year",
    "originalyear",
    "grouping",
    "bpm",
    "initial_key",
    "compilation",
    "comment",
];

pub fn canonicalize_free_text(val: Option<&str>) -> Option<String> {
    val.map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub fn canonicalize_multi_value(val: Option<&str>) -> Option<String> {
    let val = val.unwrap_or_default();
    let parsed = crate::models::parse_multi_value(val);
    if parsed.is_empty() {
        None
    } else {
        let joined = crate::models::join_multi_value(&parsed);
        let trimmed = joined.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

pub fn canonicalize_int(val: Option<i64>) -> Option<String> {
    val.filter(|&n| n > 0).map(|n| n.to_string())
}

pub fn canonicalize_bpm(val: Option<f64>) -> Option<String> {
    val.filter(|&b| b > 0.0 && b.is_finite()).map(|b| {
        let r = (b * 100.0).round() / 100.0;
        if (r - r.round()).abs() < 1e-4 {
            format!("{}", r.round() as i64)
        } else {
            format!("{r}")
        }
    })
}

pub fn canonicalize_compilation(c: bool) -> Option<String> {
    Some(if c {
        "true".to_string()
    } else {
        "false".to_string()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSnapshot {
    pub song_id: i64,
    pub fields: HashMap<&'static str, Option<String>>,
}

impl TagSnapshot {
    pub fn get(&self, field: &str) -> Option<&str> {
        self.fields.get(field).and_then(|v| v.as_deref())
    }
}

pub fn diff_snapshots(
    before: &TagSnapshot,
    after: &TagSnapshot,
) -> Vec<(&'static str, Option<String>, Option<String>)> {
    let mut diffs = Vec::new();
    for &field in TRACKED_FIELDS {
        let old_val = before.fields.get(field).cloned().flatten();
        let new_val = after.fields.get(field).cloned().flatten();
        if old_val != new_val {
            diffs.push((field, old_val, new_val));
        }
    }
    diffs
}

fn row_to_snapshot(row: &rusqlite::Row) -> rusqlite::Result<Option<TagSnapshot>> {
    let song_id: i64 = row.get(0)?;
    let source_val: i64 = row.get(1)?;
    let cue_path: Option<String> = row.get(2)?;

    let source = crate::models::SongSource::from(source_val);
    if !source.is_local() || cue_path.is_some() {
        return Ok(None);
    }

    let title: Option<String> = row.get(3)?;
    let titlesort: Option<String> = row.get(4)?;
    let artist: Option<String> = row.get(5)?;
    let artistsort: Option<String> = row.get(6)?;
    let album: Option<String> = row.get(7)?;
    let albumsort: Option<String> = row.get(8)?;
    let album_artist: Option<String> = row.get(9)?;
    let album_artist_sort: Option<String> = row.get(10)?;
    let composer: Option<String> = row.get(11)?;
    let composersort: Option<String> = row.get(12)?;
    let genre: Option<String> = row.get(13)?;
    let genresort: Option<String> = row.get(14)?;
    let track: Option<i64> = row.get(15)?;
    let disc: Option<i64> = row.get(16)?;
    let disc_subtitle: Option<String> = row.get(17)?;
    let year: Option<i64> = row.get(18)?;
    let originalyear: Option<i64> = row.get(19)?;
    let grouping: Option<String> = row.get(20)?;
    let bpm: Option<f64> = row.get(21)?;
    let initial_key: Option<String> = row.get(22)?;
    let compilation: Option<bool> = row.get(23)?;
    let comment: Option<String> = row.get(24)?;

    let compilation_bool = compilation.unwrap_or(false);

    let mut fields = HashMap::with_capacity(22);
    fields.insert("title", canonicalize_free_text(title.as_deref()));
    fields.insert("titlesort", canonicalize_free_text(titlesort.as_deref()));
    fields.insert("artist", canonicalize_multi_value(artist.as_deref()));
    fields.insert("artistsort", canonicalize_free_text(artistsort.as_deref()));
    fields.insert("album", canonicalize_free_text(album.as_deref()));
    fields.insert("albumsort", canonicalize_free_text(albumsort.as_deref()));
    fields.insert(
        "album_artist",
        canonicalize_multi_value(album_artist.as_deref()),
    );
    fields.insert(
        "album_artist_sort",
        canonicalize_free_text(album_artist_sort.as_deref()),
    );
    fields.insert("composer", canonicalize_multi_value(composer.as_deref()));
    fields.insert(
        "composersort",
        canonicalize_free_text(composersort.as_deref()),
    );
    fields.insert("genre", canonicalize_multi_value(genre.as_deref()));
    fields.insert("genresort", canonicalize_free_text(genresort.as_deref()));
    fields.insert("track", canonicalize_int(track));
    fields.insert("disc", canonicalize_int(disc));
    fields.insert(
        "disc_subtitle",
        canonicalize_free_text(disc_subtitle.as_deref()),
    );
    fields.insert("year", canonicalize_int(year));
    fields.insert("originalyear", canonicalize_int(originalyear));
    fields.insert("grouping", canonicalize_free_text(grouping.as_deref()));
    fields.insert("bpm", canonicalize_bpm(bpm));
    fields.insert(
        "initial_key",
        canonicalize_free_text(initial_key.as_deref()),
    );
    fields.insert("compilation", canonicalize_compilation(compilation_bool));
    fields.insert("comment", canonicalize_free_text(comment.as_deref()));

    Ok(Some(TagSnapshot { song_id, fields }))
}

const SNAPSHOT_COLS: &str = "id, source, cue_path, \
     title, titlesort, artist, artistsort, \
     album, albumsort, album_artist, album_artist_sort, \
     composer, composersort, genre, genresort, \
     track, disc, disc_subtitle, year, originalyear, \
     grouping, bpm, initial_key, compilation, comment";

pub fn snapshot_by_id(
    conn: &rusqlite::Connection,
    song_id: i64,
) -> rusqlite::Result<Option<TagSnapshot>> {
    let sql = format!("SELECT {SNAPSHOT_COLS} FROM songs WHERE id = ?1");
    let mut stmt = conn.prepare_cached(&sql)?;
    match stmt.query_row(params![song_id], row_to_snapshot) {
        Ok(snapshot) => Ok(snapshot),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn snapshot_by_path(
    conn: &rusqlite::Connection,
    path: &str,
    beginning_nanosec: i64,
) -> rusqlite::Result<Option<TagSnapshot>> {
    let sql =
        format!("SELECT {SNAPSHOT_COLS} FROM songs WHERE path = ?1 AND beginning_nanosec = ?2");
    let mut stmt = conn.prepare_cached(&sql)?;
    match stmt.query_row(params![path, beginning_nanosec], row_to_snapshot) {
        Ok(snapshot) => Ok(snapshot),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn record_diff_after_update(
    conn: &rusqlite::Connection,
    before: &TagSnapshot,
    actor: HistoryActor,
    batch_id: Option<&str>,
) -> rusqlite::Result<usize> {
    let Some(after) = snapshot_by_id(conn, before.song_id)? else {
        return Ok(0);
    };

    let diffs = diff_snapshots(before, &after);
    if diffs.is_empty() {
        return Ok(0);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut stmt = conn.prepare_cached(
        "INSERT INTO song_history (song_id, at, actor, field, old_value, new_value, batch_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;

    for (field, old_val, new_val) in &diffs {
        stmt.execute(params![
            before.song_id,
            now,
            actor.as_str(),
            field,
            old_val,
            new_val,
            batch_id,
        ])?;
    }

    conn.execute(
        "DELETE FROM song_history
         WHERE song_id = ?1
           AND id NOT IN (
               SELECT id FROM song_history
               WHERE song_id = ?1
               ORDER BY at DESC, id DESC
               LIMIT ?2
           )",
        params![before.song_id, MAX_EVENTS_PER_SONG as i64],
    )?;

    Ok(diffs.len())
}

pub fn file_mtime_and_size(path: &Path) -> (Option<i64>, Option<i64>) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (None, None);
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok());
    let size = i64::try_from(meta.len()).ok();
    (mtime, size)
}

static BATCH_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn new_batch_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = BATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{now:x}-{seq}")
}

pub fn get_song_history(
    conn: &rusqlite::Connection,
    song_id: i64,
) -> Result<SongTagHistory, String> {
    let (path, filesize, filetype, added, source, cue_path): (
        String,
        Option<i64>,
        i64,
        Option<i64>,
        i64,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT COALESCE(path, ''), filesize, filetype, added, source, cue_path FROM songs WHERE id = ?1",
            params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .map_err(|e| format!("Song not found: {song_id} ({e})"))?;

    let song_source = crate::models::SongSource::from(source);
    let song_filetype = crate::models::FileType::from(filetype);

    let source = SongHistorySource {
        path,
        filesize,
        format: song_filetype.display_name().to_string(),
        added,
        is_remote: song_source.is_remote(),
        is_cue_track: cue_path.is_some(),
    };

    let mut stmt = conn
        .prepare(
            "SELECT h.id, h.song_id, h.at, h.actor, h.field, h.old_value, h.new_value, h.batch_id,
                    CASE
                        WHEN h.batch_id IS NULL THEN 0
                        ELSE (
                            SELECT COUNT(DISTINCT b.song_id)
                            FROM song_history b
                            WHERE b.batch_id = h.batch_id AND b.song_id != h.song_id
                        )
                    END AS batch_other_tracks
             FROM song_history h
             WHERE h.song_id = ?1
             ORDER BY h.at DESC, h.id DESC",
        )
        .map_err(|e| e.to_string())?;

    let entries = stmt
        .query_map(params![song_id], |row| {
            let actor_str: String = row.get(3)?;
            let actor = match actor_str.as_str() {
                "luminous" => HistoryActor::Luminous,
                "scan" => HistoryActor::Scan,
                _ => HistoryActor::Luminous,
            };
            let batch_other_tracks_i64: i64 = row.get(8)?;
            Ok(SongHistoryEntry {
                id: row.get(0)?,
                song_id: row.get(1)?,
                at: row.get(2)?,
                actor,
                field: row.get(4)?,
                old_value: row.get(5)?,
                new_value: row.get(6)?,
                batch_id: row.get(7)?,
                batch_other_tracks: batch_other_tracks_i64.max(0) as usize,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;

    Ok(SongTagHistory {
        song_id,
        source,
        entries,
    })
}

pub fn clear_song_history(conn: &rusqlite::Connection, song_id: i64) -> Result<(), String> {
    conn.execute(
        "DELETE FROM song_history WHERE song_id = ?1",
        params![song_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::Builder;

    fn setup_test_db() -> (tempfile::TempDir, Database) {
        let temp_dir = Builder::new()
            .prefix("luminous_tag_history_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        (temp_dir, db)
    }

    #[test]
    fn test_canonicalization_rules() {
        assert_eq!(
            canonicalize_free_text(Some("  hello world  ")),
            Some("hello world".to_string())
        );
        assert_eq!(canonicalize_free_text(Some("   ")), None);
        assert_eq!(canonicalize_free_text(None), None);

        assert_eq!(
            canonicalize_multi_value(Some("Rock; Jazz Fusion; Live")),
            Some("Rock; Jazz Fusion; Live".to_string())
        );
        assert_eq!(
            canonicalize_multi_value(Some("Rock; rock; ROCK")),
            Some("Rock".to_string())
        );
        assert_eq!(
            canonicalize_multi_value(Some("Rock;Pop")),
            Some("Rock; Pop".to_string())
        );
        assert_eq!(canonicalize_multi_value(Some("")), None);

        assert_eq!(canonicalize_int(Some(5)), Some("5".to_string()));
        assert_eq!(canonicalize_int(Some(0)), None);
        assert_eq!(canonicalize_int(Some(-1)), None);
        assert_eq!(canonicalize_int(None), None);

        assert_eq!(canonicalize_bpm(Some(120.0)), Some("120".to_string()));
        assert_eq!(canonicalize_bpm(Some(120.45)), Some("120.45".to_string()));
        assert_eq!(canonicalize_bpm(Some(0.0)), None);
        assert_eq!(canonicalize_bpm(Some(-5.0)), None);
        assert_eq!(canonicalize_bpm(None), None);

        assert_eq!(canonicalize_compilation(true), Some("true".to_string()));
        assert_eq!(canonicalize_compilation(false), Some("false".to_string()));
    }

    #[test]
    fn test_unchanged_fields_write_nothing_and_first_change_stores_old_and_new() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, title, artist, album, source, mtime)
             VALUES ('/test.flac', 'Old Title', 'Artist A', 'Album X', 2, 1000)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        let before = snapshot_by_id(&conn, song_id)
            .unwrap()
            .expect("snapshot exists");

        // No DB update yet -> diff is empty, nothing recorded
        let diff_count =
            record_diff_after_update(&conn, &before, HistoryActor::Luminous, None).unwrap();
        assert_eq!(diff_count, 0);

        let history = get_song_history(&conn, song_id).unwrap();
        assert_eq!(history.entries.len(), 0);

        // Update title
        conn.execute(
            "UPDATE songs SET title = 'New Title' WHERE id = ?1",
            params![song_id],
        )
        .unwrap();

        let diff_count =
            record_diff_after_update(&conn, &before, HistoryActor::Luminous, None).unwrap();
        assert_eq!(diff_count, 1);

        let history = get_song_history(&conn, song_id).unwrap();
        assert_eq!(history.entries.len(), 1);
        let entry = &history.entries[0];
        assert_eq!(entry.field, "title");
        assert_eq!(entry.old_value.as_deref(), Some("Old Title"));
        assert_eq!(entry.new_value.as_deref(), Some("New Title"));
        assert_eq!(entry.actor, HistoryActor::Luminous);
        assert_eq!(entry.batch_id, None);
        assert_eq!(entry.batch_other_tracks, 0);

        // Recording again with updated before snapshot produces 0 diffs
        let before_new = snapshot_by_id(&conn, song_id).unwrap().unwrap();
        let diff_count2 =
            record_diff_after_update(&conn, &before_new, HistoryActor::Luminous, None).unwrap();
        assert_eq!(diff_count2, 0);
        assert_eq!(get_song_history(&conn, song_id).unwrap().entries.len(), 1);
    }

    #[test]
    fn test_lazy_baseline_on_first_insert_writes_zero_rows() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        // Non-existent song
        let before = snapshot_by_path(&conn, "/nonexistent.flac", 0).unwrap();
        assert!(
            before.is_none(),
            "non-existent path should return None snapshot"
        );
    }

    #[test]
    fn test_multi_value_fields_compare_in_normalized_form() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, title, artist, genre, source, mtime)
             VALUES ('/mv.flac', 'Song', 'Rock;Pop', 'Electronic;Ambient', 2, 1000)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        let before = snapshot_by_id(&conn, song_id).unwrap().unwrap();
        assert_eq!(before.get("artist"), Some("Rock; Pop"));
        assert_eq!(before.get("genre"), Some("Electronic; Ambient"));

        // Update artist with different whitespace / casing duplicate: "Rock; Pop; rock"
        // Normalizes to "Rock; Pop", exactly matching before
        conn.execute(
            "UPDATE songs SET artist = 'Rock; Pop; rock' WHERE id = ?1",
            params![song_id],
        )
        .unwrap();
        let recorded = record_diff_after_update(&conn, &before, HistoryActor::Scan, None).unwrap();
        assert_eq!(
            recorded, 0,
            "normalized equivalent multi-value should not log changes"
        );

        // Now update to genuinely new multi-value
        conn.execute(
            "UPDATE songs SET artist = 'Rock; Pop; Jazz' WHERE id = ?1",
            params![song_id],
        )
        .unwrap();
        let recorded = record_diff_after_update(&conn, &before, HistoryActor::Scan, None).unwrap();
        assert_eq!(recorded, 1);

        let history = get_song_history(&conn, song_id).unwrap();
        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.entries[0].field, "artist");
        assert_eq!(history.entries[0].old_value.as_deref(), Some("Rock; Pop"));
        assert_eq!(
            history.entries[0].new_value.as_deref(),
            Some("Rock; Pop; Jazz")
        );
        assert_eq!(history.entries[0].actor, HistoryActor::Scan);
    }

    #[test]
    fn test_album_batch_edit_shared_batch_id_and_other_tracks_count() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, title, album, source, mtime) VALUES ('/s1.flac', 'S1', 'Old Alb', 2, 1000)",
            [],
        )
        .unwrap();
        let s1 = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO songs (path, title, album, source, mtime) VALUES ('/s2.flac', 'S2', 'Old Alb', 2, 1000)",
            [],
        )
        .unwrap();
        let s2 = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO songs (path, title, album, source, mtime) VALUES ('/s3.flac', 'S3', 'Old Alb', 2, 1000)",
            [],
        )
        .unwrap();
        let s3 = conn.last_insert_rowid();

        let b1 = snapshot_by_id(&conn, s1).unwrap().unwrap();
        let b2 = snapshot_by_id(&conn, s2).unwrap().unwrap();
        let b3 = snapshot_by_id(&conn, s3).unwrap().unwrap();

        let batch_id = new_batch_id();

        for id in [s1, s2, s3] {
            conn.execute(
                "UPDATE songs SET album = 'New Album' WHERE id = ?1",
                params![id],
            )
            .unwrap();
        }

        record_diff_after_update(&conn, &b1, HistoryActor::Luminous, Some(&batch_id)).unwrap();
        record_diff_after_update(&conn, &b2, HistoryActor::Luminous, Some(&batch_id)).unwrap();
        record_diff_after_update(&conn, &b3, HistoryActor::Luminous, Some(&batch_id)).unwrap();

        let h1 = get_song_history(&conn, s1).unwrap();
        assert_eq!(h1.entries.len(), 1);
        assert_eq!(h1.entries[0].batch_id.as_deref(), Some(batch_id.as_str()));
        assert_eq!(h1.entries[0].batch_other_tracks, 2);

        let h2 = get_song_history(&conn, s2).unwrap();
        assert_eq!(h2.entries[0].batch_other_tracks, 2);
    }

    #[test]
    fn test_remote_and_cue_tracks_record_no_history() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        // Remote WebDAV track (source = 11)
        conn.execute(
            "INSERT INTO songs (path, title, source, mtime) VALUES ('webdav://srv/a.flac', 'Remote', 11, 1000)",
            [],
        )
        .unwrap();
        let remote_id = conn.last_insert_rowid();

        // Local track cut from CUE sheet (cue_path IS NOT NULL)
        conn.execute(
            "INSERT INTO songs (path, title, source, cue_path, mtime) VALUES ('/cue.flac', 'CUE Track', 2, '/cue.cue', 1000)",
            [],
        )
        .unwrap();
        let cue_id = conn.last_insert_rowid();

        assert!(snapshot_by_id(&conn, remote_id).unwrap().is_none());
        assert!(snapshot_by_id(&conn, cue_id).unwrap().is_none());

        assert!(snapshot_by_path(&conn, "webdav://srv/a.flac", 0)
            .unwrap()
            .is_none());
        assert!(snapshot_by_path(&conn, "/cue.flac", 0).unwrap().is_none());
    }

    #[test]
    fn test_history_pruning_past_max_events() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, title, source, mtime) VALUES ('/prune.flac', 'Prune', 2, 1000)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        // Insert 105 events directly
        for i in 0..105 {
            conn.execute(
                "INSERT INTO song_history (song_id, at, actor, field, old_value, new_value, batch_id)
                 VALUES (?1, ?2, 'luminous', 'title', ?3, ?4, NULL)",
                params![song_id, 1000 + i, format!("Old {i}"), format!("New {i}")],
            )
            .unwrap();
        }

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM song_history WHERE song_id = ?1",
                params![song_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 105);

        // A diff recording prunes beyond MAX_EVENTS_PER_SONG (100)
        let before = snapshot_by_id(&conn, song_id).unwrap().unwrap();
        conn.execute(
            "UPDATE songs SET title = 'Trigger Prune' WHERE id = ?1",
            params![song_id],
        )
        .unwrap();
        record_diff_after_update(&conn, &before, HistoryActor::Luminous, None).unwrap();

        let count_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM song_history WHERE song_id = ?1",
                params![song_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count_after, MAX_EVENTS_PER_SONG as i64);

        // Verify newest remains: get_song_history returns exactly 100 entries, first entry is "Trigger Prune"
        let history = get_song_history(&conn, song_id).unwrap();
        assert_eq!(history.entries.len(), 100);
        assert_eq!(history.entries[0].field, "title");
        assert_eq!(
            history.entries[0].new_value.as_deref(),
            Some("Trigger Prune")
        );
    }

    #[test]
    fn test_clear_song_history() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, title, source, mtime) VALUES ('/clear.flac', 'Clear', 2, 1000)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        for i in 0..3 {
            conn.execute(
                "INSERT INTO song_history (song_id, at, actor, field, old_value, new_value, batch_id)
                 VALUES (?1, ?2, 'luminous', 'title', 'A', 'B', NULL)",
                params![song_id, 1000 + i],
            )
            .unwrap();
        }

        assert_eq!(get_song_history(&conn, song_id).unwrap().entries.len(), 3);
        clear_song_history(&conn, song_id).unwrap();
        assert_eq!(get_song_history(&conn, song_id).unwrap().entries.len(), 0);
    }

    fn write_test_wav(path: &Path) {
        let sample_rate = 8_000u32;
        let channels = 1u16;
        let bits_per_sample = 16u16;
        let data = vec![0u8; (sample_rate / 10) as usize * 2];
        let byte_rate = sample_rate * channels as u32 * (bits_per_sample / 8) as u32;
        let block_align = channels * (bits_per_sample / 8);

        let mut wav = Vec::with_capacity(44 + data.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);

        std::fs::write(path, wav).expect("failed to write test wav fixture");
    }

    #[test]
    fn test_save_then_rescan_and_external_edit() {
        let (temp_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        let path = temp_dir.path().join("test_song.wav");
        write_test_wav(&path);

        luminous_metadata::audio::write_tags(
            &path,
            &luminous_metadata::audio::TagWriteRequest {
                title: "Original Title",
                artist: "Original Artist",
                album: "Original Album",
                ..Default::default()
            },
        )
        .unwrap();

        // 1. Initial scan / upsert — lazy baseline writes 0 history rows
        let mut initial_song = crate::collection::read_tags(&path).unwrap();
        initial_song.source = crate::models::SongSource::LocalFile;
        crate::collection::upsert_song(&conn, &initial_song).unwrap();

        let song_id: i64 = conn
            .query_row(
                "SELECT id FROM songs WHERE path = ?1",
                params![path.to_string_lossy().to_string()],
                |r| r.get(0),
            )
            .unwrap();

        let history = get_song_history(&conn, song_id).unwrap();
        assert_eq!(
            history.entries.len(),
            0,
            "lazy baseline on first insert must write 0 rows"
        );

        // 2. User edits tags via Luminous (simulate save_song_tags):
        let before = snapshot_by_id(&conn, song_id).unwrap().unwrap();
        luminous_metadata::audio::write_tags(
            &path,
            &luminous_metadata::audio::TagWriteRequest {
                title: "Luminous Title",
                artist: "Original Artist",
                album: "Original Album",
                ..Default::default()
            },
        )
        .unwrap();

        let (new_mtime, new_filesize) = file_mtime_and_size(&path);
        conn.execute(
            "UPDATE songs SET title = 'Luminous Title', mtime = ?1, filesize = ?2 WHERE id = ?3",
            params![new_mtime, new_filesize, song_id],
        )
        .unwrap();
        let recorded =
            record_diff_after_update(&conn, &before, HistoryActor::Luminous, None).unwrap();
        assert_eq!(recorded, 1);

        let history = get_song_history(&conn, song_id).unwrap();
        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.entries[0].actor, HistoryActor::Luminous);
        assert_eq!(history.entries[0].field, "title");
        assert_eq!(
            history.entries[0].old_value.as_deref(),
            Some("Original Title")
        );
        assert_eq!(
            history.entries[0].new_value.as_deref(),
            Some("Luminous Title")
        );

        // 3. Rescan without changes: mtime in DB matches disk, tags match DB -> 0 new rows
        let scanned_song = crate::collection::read_tags(&path).unwrap();
        crate::collection::upsert_song(&conn, &scanned_song).unwrap();

        let history_after_rescan = get_song_history(&conn, song_id).unwrap();
        assert_eq!(
            history_after_rescan.entries.len(),
            1,
            "rescan after luminous save must not log any scan event"
        );

        // 4. External edit on disk: tag is modified outside Luminous
        luminous_metadata::audio::write_tags(
            &path,
            &luminous_metadata::audio::TagWriteRequest {
                title: "External Title",
                artist: "Original Artist",
                album: "Original Album",
                ..Default::default()
            },
        )
        .unwrap();

        let externally_edited_song = crate::collection::read_tags(&path).unwrap();
        crate::collection::upsert_song(&conn, &externally_edited_song).unwrap();

        let history_after_external = get_song_history(&conn, song_id).unwrap();
        assert_eq!(
            history_after_external.entries.len(),
            2,
            "external edit rescan must log a scan event"
        );
        assert_eq!(history_after_external.entries[0].actor, HistoryActor::Scan);
        assert_eq!(history_after_external.entries[0].field, "title");
        assert_eq!(
            history_after_external.entries[0].old_value.as_deref(),
            Some("Luminous Title")
        );
        assert_eq!(
            history_after_external.entries[0].new_value.as_deref(),
            Some("External Title")
        );

        // 5. Forced rescan after an mtime-preserving edit logs a scan event too
        let current_mtime = file_mtime_and_size(&path).0;
        let mut forced_song = externally_edited_song;
        forced_song.title = Some("Forced Rescan Title".to_string());
        forced_song.mtime = current_mtime;
        crate::collection::upsert_song(&conn, &forced_song).unwrap();

        let history_after_forced = get_song_history(&conn, song_id).unwrap();
        assert_eq!(
            history_after_forced.entries.len(),
            3,
            "forced rescan with mtime-preserving edit must log a scan event"
        );
        assert_eq!(history_after_forced.entries[0].actor, HistoryActor::Scan);
        assert_eq!(history_after_forced.entries[0].field, "title");
        assert_eq!(
            history_after_forced.entries[0].old_value.as_deref(),
            Some("External Title")
        );
        assert_eq!(
            history_after_forced.entries[0].new_value.as_deref(),
            Some("Forced Rescan Title")
        );
    }
}
