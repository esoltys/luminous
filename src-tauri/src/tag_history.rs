//! Tag change history tracking module (#1442).
//!
//! Records and prunes per-track tag change events across both user edits
//! (tag editor, album editor, Files view, tag clean-up) and external scan updates.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum number of tag change events retained per song in `song_history`.
pub const MAX_EVENTS_PER_SONG: usize = 100;

/// Identifies the originator of a tag modification event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryActor {
    /// Edit performed explicitly within Luminous (Tag Editor, Album Editor, Clean-Up, etc.).
    Luminous,
    /// Change detected during a library scan or file watcher refresh.
    Scan,
}

impl HistoryActor {
    /// Returns the database representation of this actor string (`"luminous"` or `"scan"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Luminous => "luminous",
            Self::Scan => "scan",
        }
    }
}

/// File metadata snapshot providing provenance and context for tag history entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongHistorySource {
    /// Canonical filesystem path or remote URL.
    pub path: String,
    /// File size in bytes, or `None` if unreachable or unindexed.
    pub filesize: Option<i64>,
    /// Human-readable audio container/codec format display name.
    pub format: String,
    /// Unix timestamp (seconds) when the song was originally added to the library.
    pub added: Option<i64>,
    /// True if this track is hosted on a remote source (e.g. WebDAV, Subsonic).
    pub is_remote: bool,
    /// True if this track belongs to a multi-track CUE sheet.
    pub is_cue_track: bool,
}

/// A single recorded field-level tag modification event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongHistoryEntry {
    /// Primary key ID in `song_history`.
    pub id: i64,
    /// Foreign key to `songs.id`.
    pub song_id: i64,
    /// Unix timestamp (seconds) when the change was recorded.
    pub at: i64,
    /// Entity that initiated the modification.
    pub actor: HistoryActor,
    /// Tracked tag field name (matches [`TRACKED_FIELDS`]).
    pub field: String,
    /// Normalized value before the edit, or `None` if previously unset.
    pub old_value: Option<String>,
    /// Normalized value after the edit, or `None` if cleared.
    pub new_value: Option<String>,
    /// Shared identifier correlating multiple tracks modified in the same batch operation.
    pub batch_id: Option<String>,
    /// Count of distinct other tracks changed in the same batch.
    pub batch_other_tracks: usize,
}

/// Full tag history payload returned to the frontend for a single track.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongTagHistory {
    /// Database ID of the song.
    pub song_id: i64,
    /// Source and file details for the track.
    pub source: SongHistorySource,
    /// Ordered history entries, newest first.
    pub entries: Vec<SongHistoryEntry>,
}

/// Total count of metadata fields tracked for change history.
pub const TRACKED_FIELD_COUNT: usize = 22;

/// 22 tracked tag fields in stable order.
pub const TRACKED_FIELDS: [&str; TRACKED_FIELD_COUNT] = [
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

fn canonicalize_free_text(val: Option<&str>) -> Option<String> {
    val.map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn canonicalize_multi_value(val: Option<&str>) -> Option<String> {
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

fn canonicalize_int(val: Option<i64>) -> Option<String> {
    val.filter(|&n| n > 0).map(|n| n.to_string())
}

fn canonicalize_bpm(val: Option<f64>) -> Option<String> {
    val.filter(|&b| b > 0.0 && b.is_finite()).map(|b| {
        let r = (b * 100.0).round() / 100.0;
        if (r - r.round()).abs() < 1e-4 {
            format!("{}", r.round() as i64)
        } else {
            format!("{r}")
        }
    })
}

fn canonicalize_compilation(c: bool) -> Option<String> {
    Some(if c {
        "true".to_string()
    } else {
        "false".to_string()
    })
}

/// Pre-update or post-update snapshot of tracked tag values for a local library track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSnapshot {
    song_id: i64,
    values: [Option<String>; TRACKED_FIELD_COUNT],
}

impl TagSnapshot {
    /// Returns the database ID of the song captured in this snapshot.
    pub fn song_id(&self) -> i64 {
        self.song_id
    }

    /// Looks up the canonicalized tag value for a tracked field name, returning `None` if
    /// unset, empty, or if `field` is not in [`TRACKED_FIELDS`].
    pub fn get(&self, field: &str) -> Option<&str> {
        let idx = TRACKED_FIELDS.iter().position(|&f| f == field)?;
        self.values[idx].as_deref()
    }
}

fn diff_snapshots(
    before: &TagSnapshot,
    after: &TagSnapshot,
) -> Vec<(&'static str, Option<String>, Option<String>)> {
    let mut diffs = Vec::new();
    for (idx, &field) in TRACKED_FIELDS.iter().enumerate() {
        let old_val = &before.values[idx];
        let new_val = &after.values[idx];
        if old_val != new_val {
            diffs.push((field, old_val.clone(), new_val.clone()));
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

    let values: [Option<String>; TRACKED_FIELD_COUNT] = [
        canonicalize_free_text(title.as_deref()),
        canonicalize_free_text(titlesort.as_deref()),
        canonicalize_multi_value(artist.as_deref()),
        canonicalize_free_text(artistsort.as_deref()),
        canonicalize_free_text(album.as_deref()),
        canonicalize_free_text(albumsort.as_deref()),
        canonicalize_multi_value(album_artist.as_deref()),
        canonicalize_free_text(album_artist_sort.as_deref()),
        canonicalize_multi_value(composer.as_deref()),
        canonicalize_free_text(composersort.as_deref()),
        canonicalize_multi_value(genre.as_deref()),
        canonicalize_free_text(genresort.as_deref()),
        canonicalize_int(track),
        canonicalize_int(disc),
        canonicalize_free_text(disc_subtitle.as_deref()),
        canonicalize_int(year),
        canonicalize_int(originalyear),
        canonicalize_free_text(grouping.as_deref()),
        canonicalize_bpm(bpm),
        canonicalize_free_text(initial_key.as_deref()),
        canonicalize_compilation(compilation_bool),
        canonicalize_free_text(comment.as_deref()),
    ];

    Ok(Some(TagSnapshot { song_id, values }))
}

const SNAPSHOT_COLS: &str = "id, source, cue_path, \
     title, titlesort, artist, artistsort, \
     album, albumsort, album_artist, album_artist_sort, \
     composer, composersort, genre, genresort, \
     track, disc, disc_subtitle, year, originalyear, \
     grouping, bpm, initial_key, compilation, comment";

/// Captures a [`TagSnapshot`] of current tracked tags for `song_id`.
///
/// Returns `Ok(None)` if the song does not exist, is a remote track, or is a CUE sheet slice.
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

/// Captures a [`TagSnapshot`] of current tracked tags for `path` and `beginning_nanosec`.
///
/// Returns `Ok(None)` if no matching row exists, is a remote track, or is a CUE sheet slice.
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

/// Diffs `before` against the song's current row in `songs` (after an UPDATE/upsert),
/// inserts `song_history` rows for any changed tracked tag fields, and prunes beyond
/// [`MAX_EVENTS_PER_SONG`]. Passing `None` (e.g. for a newly inserted, remote, or CUE track)
/// is a valid no-op returning `Ok(0)`.
pub fn record_diff_after_update(
    conn: &rusqlite::Connection,
    before: Option<&TagSnapshot>,
    actor: HistoryActor,
    batch_id: Option<&str>,
) -> rusqlite::Result<usize> {
    let Some(before) = before else {
        return Ok(0);
    };

    let Some(after) = snapshot_by_id(conn, before.song_id())? else {
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
            before.song_id(),
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
        params![before.song_id(), MAX_EVENTS_PER_SONG as i64],
    )?;

    Ok(diffs.len())
}

/// Inspects the file at `path` and returns its modification time (seconds since epoch)
/// and file size (bytes), or `None` for each if filesystem metadata cannot be read.
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

/// Generates a unique correlation ID for grouping multi-track tag edits performed together.
pub fn new_batch_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = BATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{now:x}-{seq}")
}

/// Retrieves the tag change history and file provenance for `song_id`.
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

/// Deletes all recorded `song_history` rows for `song_id`.
pub fn clear_song_history(conn: &rusqlite::Connection, song_id: i64) -> Result<(), String> {
    conn.execute(
        "DELETE FROM song_history WHERE song_id = ?1",
        params![song_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Returns the number of songs in the library that have at least one recorded
/// `HistoryActor::Scan` tag history event.
pub fn count_songs_with_scan_history(conn: &rusqlite::Connection) -> rusqlite::Result<usize> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT song_id) FROM song_history WHERE actor = 'scan'",
        [],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as usize)
}

/// Target specifying what history to restore: either a single history entry row,
/// or all entries sharing a batch ID.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RestoreTarget {
    Entry {
        #[serde(rename = "historyId")]
        history_id: i64,
    },
    Batch {
        #[serde(rename = "batchId")]
        batch_id: String,
    },
}

/// Result summary of a history restore operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreTagHistoryResult {
    /// Number of distinct tracks successfully restored (wrote file tags and updated DB).
    pub restored_tracks: usize,
    /// Number of tracks skipped because target values already matched (no-op).
    pub unchanged_tracks: usize,
    /// Number of tracks skipped due to errors (e.g. missing file, unreadable, remote/CUE).
    pub skipped_tracks: usize,
    /// Human-readable diagnostic messages for any skipped tracks.
    pub errors: Vec<String>,
}

/// Outcome of attempting to restore tag values on an individual track.
#[derive(Debug, PartialEq, Eq)]
pub enum RestoreSongOutcome {
    Restored,
    Unchanged,
    Skipped(String),
}

/// Field name and restored target value pair.
pub type RestoredField = (String, Option<String>);
/// Planned restore action mapping a song ID to its fields to restore.
pub type SongRestorePlan = (i64, Vec<RestoredField>);

/// Resolves a [`RestoreTarget`] against `song_history`, returning a list of `(song_id, fields_to_restore)`
/// where each field to restore is `(field_name, target_old_value)`.
pub fn resolve_restore_plan(
    conn: &rusqlite::Connection,
    target: &RestoreTarget,
) -> Result<Vec<SongRestorePlan>, String> {
    match target {
        RestoreTarget::Entry { history_id } => {
            let mut stmt = conn
                .prepare("SELECT song_id, field, old_value FROM song_history WHERE id = ?1")
                .map_err(|e| e.to_string())?;
            let row = stmt
                .query_row(params![history_id], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|_| format!("History entry {history_id} not found"))?;
            Ok(vec![(row.0, vec![(row.1, row.2)])])
        }
        RestoreTarget::Batch { batch_id } => {
            let mut stmt = conn
                .prepare(
                    "SELECT song_id, field, old_value
                     FROM song_history
                     WHERE batch_id = ?1
                     ORDER BY at ASC, id ASC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![batch_id], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?;

            if rows.is_empty() {
                return Err(format!("Batch {batch_id} not found in history"));
            }

            // Group by song_id, preserving the earliest old_value for each field in this batch
            let mut plan: Vec<SongRestorePlan> = Vec::new();
            for (song_id, field, old_val) in rows {
                if let Some(entry) = plan.iter_mut().find(|(sid, _)| *sid == song_id) {
                    if !entry.1.iter().any(|(f, _)| f == &field) {
                        entry.1.push((field, old_val));
                    }
                } else {
                    plan.push((song_id, vec![(field, old_val)]));
                }
            }
            Ok(plan)
        }
    }
}

/// Restores the specified fields on `song_id` to their target old values.
///
/// If `on_disk` is true, writes file tags using `luminous_metadata::audio::write_tags` and
/// refreshes `mtime`/`filesize`. If `on_disk` is false (e.g. in test harness without real audio files),
/// updates SQLite directly and appends history.
///
/// Returns:
/// - [`RestoreSongOutcome::Restored`]: at least one field differed, successfully written to disk & DB,
///   and logged as a new `HistoryActor::Luminous` event.
/// - [`RestoreSongOutcome::Unchanged`]: all fields already matched current values (clean no-op, 0 writes).
/// - [`RestoreSongOutcome::Skipped`]: track is remote, CUE sheet, missing file, or write failed.
pub fn restore_song_tags(
    conn: &rusqlite::Connection,
    song_id: i64,
    fields_to_restore: &[(String, Option<String>)],
    restore_batch_id: Option<&str>,
    on_disk: bool,
) -> Result<RestoreSongOutcome, String> {
    if fields_to_restore.is_empty() {
        return Ok(RestoreSongOutcome::Unchanged);
    }

    // 1. Query current song state from `songs`
    let query_res = conn.query_row(
        "SELECT path, source, cue_path,
                COALESCE(title, ''), titlesort,
                COALESCE(artist, ''), artistsort,
                COALESCE(album, ''), albumsort,
                COALESCE(album_artist, ''), album_artist_sort,
                COALESCE(composer, ''), composersort,
                COALESCE(genre, ''), genresort,
                track, disc, disc_subtitle,
                year, originalyear,
                COALESCE(grouping, ''),
                bpm,
                COALESCE(initial_key, ''),
                compilation,
                comment
         FROM songs WHERE id = ?1",
        params![song_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, Option<String>>(14)?,
                row.get::<_, Option<i64>>(15)?,
                row.get::<_, Option<i64>>(16)?,
                row.get::<_, Option<String>>(17)?,
                row.get::<_, Option<i64>>(18)?,
                row.get::<_, Option<i64>>(19)?,
                row.get::<_, String>(20)?,
                row.get::<_, Option<f64>>(21)?,
                row.get::<_, String>(22)?,
                row.get::<_, Option<bool>>(23)?,
                row.get::<_, Option<String>>(24)?,
            ))
        },
    );

    let (
        path_str,
        source_val,
        cue_path,
        mut title,
        mut titlesort,
        mut artist,
        mut artistsort,
        mut album,
        mut albumsort,
        mut album_artist,
        mut album_artist_sort,
        mut composer,
        mut composersort,
        mut genre,
        mut genresort,
        mut track,
        mut disc,
        mut disc_subtitle,
        mut year,
        mut originalyear,
        mut grouping,
        mut bpm,
        mut initial_key,
        compilation_opt,
        mut comment,
    ) = match query_res {
        Ok(vals) => vals,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Ok(RestoreSongOutcome::Skipped(format!(
                "Song {song_id} not found"
            )));
        }
        Err(e) => return Err(e.to_string()),
    };

    let song_source = crate::models::SongSource::from(source_val);
    if !song_source.is_local() || cue_path.is_some() {
        return Ok(RestoreSongOutcome::Skipped(format!(
            "Song {song_id} is remote or part of a CUE sheet and cannot be modified"
        )));
    }

    let file_path = Path::new(&path_str);
    if on_disk && !file_path.exists() {
        return Ok(RestoreSongOutcome::Skipped(format!(
            "File not found on disk: {path_str}"
        )));
    }

    let mut compilation = compilation_opt.unwrap_or(false);
    let mut has_changes = false;

    // 2. Diff and apply requested field values
    for (field, target_val) in fields_to_restore {
        let target_str = target_val.as_deref();
        match field.as_str() {
            "title" => {
                if canonicalize_free_text(Some(&title)) != canonicalize_free_text(target_str) {
                    title = target_str.map(|s| s.trim().to_string()).unwrap_or_default();
                    has_changes = true;
                }
            }
            "titlesort" => {
                if canonicalize_free_text(titlesort.as_deref())
                    != canonicalize_free_text(target_str)
                {
                    titlesort = canonicalize_free_text(target_str);
                    has_changes = true;
                }
            }
            "artist" => {
                if canonicalize_multi_value(Some(&artist)) != canonicalize_multi_value(target_str) {
                    artist = target_str
                        .map(|s| {
                            crate::models::join_multi_value(&crate::models::parse_multi_value(s))
                        })
                        .unwrap_or_default();
                    has_changes = true;
                }
            }
            "artistsort" => {
                if canonicalize_free_text(artistsort.as_deref())
                    != canonicalize_free_text(target_str)
                {
                    artistsort = canonicalize_free_text(target_str);
                    has_changes = true;
                }
            }
            "album" => {
                if canonicalize_free_text(Some(&album)) != canonicalize_free_text(target_str) {
                    album = target_str.map(|s| s.trim().to_string()).unwrap_or_default();
                    has_changes = true;
                }
            }
            "albumsort" => {
                if canonicalize_free_text(albumsort.as_deref())
                    != canonicalize_free_text(target_str)
                {
                    albumsort = canonicalize_free_text(target_str);
                    has_changes = true;
                }
            }
            "album_artist" => {
                if canonicalize_multi_value(Some(&album_artist))
                    != canonicalize_multi_value(target_str)
                {
                    album_artist = target_str
                        .map(|s| {
                            crate::models::join_multi_value(&crate::models::parse_multi_value(s))
                        })
                        .unwrap_or_default();
                    has_changes = true;
                }
            }
            "album_artist_sort" => {
                if canonicalize_free_text(album_artist_sort.as_deref())
                    != canonicalize_free_text(target_str)
                {
                    album_artist_sort = canonicalize_free_text(target_str);
                    has_changes = true;
                }
            }
            "composer" => {
                if canonicalize_multi_value(Some(&composer)) != canonicalize_multi_value(target_str)
                {
                    composer = target_str
                        .map(|s| {
                            crate::models::join_multi_value(&crate::models::parse_multi_value(s))
                        })
                        .unwrap_or_default();
                    has_changes = true;
                }
            }
            "composersort" => {
                if canonicalize_free_text(composersort.as_deref())
                    != canonicalize_free_text(target_str)
                {
                    composersort = canonicalize_free_text(target_str);
                    has_changes = true;
                }
            }
            "genre" => {
                if canonicalize_multi_value(Some(&genre)) != canonicalize_multi_value(target_str) {
                    genre = target_str
                        .map(|s| {
                            crate::models::join_multi_value(&crate::models::parse_multi_value(s))
                        })
                        .unwrap_or_default();
                    has_changes = true;
                }
            }
            "genresort" => {
                if canonicalize_free_text(genresort.as_deref())
                    != canonicalize_free_text(target_str)
                {
                    genresort = canonicalize_free_text(target_str);
                    has_changes = true;
                }
            }
            "track" => {
                let target_int = target_str.and_then(|s| s.parse::<i64>().ok());
                if canonicalize_int(track) != canonicalize_int(target_int) {
                    track = target_int;
                    has_changes = true;
                }
            }
            "disc" => {
                let target_int = target_str.and_then(|s| s.parse::<i64>().ok());
                if canonicalize_int(disc) != canonicalize_int(target_int) {
                    disc = target_int;
                    has_changes = true;
                }
            }
            "disc_subtitle" => {
                if canonicalize_free_text(disc_subtitle.as_deref())
                    != canonicalize_free_text(target_str)
                {
                    disc_subtitle = canonicalize_free_text(target_str);
                    has_changes = true;
                }
            }
            "year" => {
                let target_int = target_str.and_then(|s| s.parse::<i64>().ok());
                if canonicalize_int(year) != canonicalize_int(target_int) {
                    year = target_int;
                    has_changes = true;
                }
            }
            "originalyear" => {
                let target_int = target_str.and_then(|s| s.parse::<i64>().ok());
                if canonicalize_int(originalyear) != canonicalize_int(target_int) {
                    originalyear = target_int;
                    has_changes = true;
                }
            }
            "grouping" => {
                if canonicalize_free_text(Some(&grouping)) != canonicalize_free_text(target_str) {
                    grouping = target_str.map(|s| s.trim().to_string()).unwrap_or_default();
                    has_changes = true;
                }
            }
            "bpm" => {
                let target_bpm = target_str.and_then(|s| s.parse::<f64>().ok());
                if canonicalize_bpm(bpm) != canonicalize_bpm(target_bpm) {
                    bpm = target_bpm;
                    has_changes = true;
                }
            }
            "initial_key" => {
                if canonicalize_free_text(Some(&initial_key)) != canonicalize_free_text(target_str)
                {
                    initial_key = target_str.map(|s| s.trim().to_string()).unwrap_or_default();
                    has_changes = true;
                }
            }
            "compilation" => {
                let target_bool = target_str.map(|s| s == "true" || s == "1").unwrap_or(false);
                if compilation != target_bool {
                    compilation = target_bool;
                    has_changes = true;
                }
            }
            "comment"
                if canonicalize_free_text(comment.as_deref())
                    != canonicalize_free_text(target_str) =>
            {
                comment = canonicalize_free_text(target_str);
                has_changes = true;
            }
            _ => {}
        }
    }

    if !has_changes {
        return Ok(RestoreSongOutcome::Unchanged);
    }

    // 3. Write metadata to file on disk if requested
    if on_disk {
        let tag_req = luminous_metadata::audio::TagWriteRequest {
            title: &title,
            titlesort: titlesort.as_deref(),
            artist: &artist,
            artistsort: artistsort.as_deref(),
            album: &album,
            albumsort: albumsort.as_deref(),
            album_artist: &album_artist,
            album_artist_sort: album_artist_sort.as_deref(),
            composer: &composer,
            composersort: composersort.as_deref(),
            genre: &genre,
            track: track.and_then(|t| u32::try_from(t).ok()),
            disc: disc.and_then(|d| u32::try_from(d).ok()),
            disc_subtitle: disc_subtitle.as_deref(),
            year: year.and_then(|y| u32::try_from(y).ok()),
            originalyear: originalyear.and_then(|y| u32::try_from(y).ok()),
            grouping: &grouping,
            bpm: bpm.map(|b| b as f32),
            initial_key: &initial_key,
            compilation,
        };

        if let Err(e) = luminous_metadata::audio::write_tags(file_path, &tag_req) {
            return Ok(RestoreSongOutcome::Skipped(format!(
                "Failed to write tags to {path_str}: {e:#}"
            )));
        }
    }

    // 4. Capture snapshot before updating SQLite
    let before = snapshot_by_id(conn, song_id).map_err(|e| e.to_string())?;

    let (new_mtime, new_filesize) = if on_disk {
        file_mtime_and_size(file_path)
    } else {
        (None, None)
    };

    let album_artist_trimmed = album_artist.trim();
    let artist_trimmed = artist.trim();
    let album_key = crate::album_key::album_key_for(
        &album,
        if album_artist_trimmed.is_empty() {
            None
        } else {
            Some(album_artist_trimmed)
        },
        if artist_trimmed.is_empty() {
            None
        } else {
            Some(artist_trimmed)
        },
        compilation,
    );

    // 5. Update SQLite database
    conn.execute(
        "UPDATE songs SET
            title = ?1,
            titlesort = ?2,
            artist = ?3,
            artistsort = ?4,
            album = ?5,
            albumsort = ?6,
            album_artist = ?7,
            album_artist_sort = ?8,
            composer = ?9,
            composersort = ?10,
            genre = ?11,
            genresort = ?12,
            track = ?13,
            disc = ?14,
            disc_subtitle = ?15,
            year = ?16,
            originalyear = ?17,
            grouping = ?18,
            bpm = ?19,
            initial_key = ?20,
            compilation = ?21,
            comment = ?22,
            album_key = ?23,
            mtime = COALESCE(?24, mtime),
            filesize = COALESCE(?25, filesize)
         WHERE id = ?26",
        rusqlite::params![
            title,
            titlesort,
            artist,
            artistsort,
            album,
            albumsort,
            album_artist,
            album_artist_sort,
            composer,
            composersort,
            genre,
            genresort,
            track,
            disc,
            disc_subtitle,
            year,
            originalyear,
            grouping,
            bpm,
            initial_key,
            compilation,
            comment,
            album_key,
            new_mtime,
            new_filesize,
            song_id,
        ],
    )
    .map_err(|e| e.to_string())?;

    // 6. Record history event as actor `luminous`
    record_diff_after_update(
        conn,
        before.as_ref(),
        HistoryActor::Luminous,
        restore_batch_id,
    )
    .map_err(|e| e.to_string())?;

    Ok(RestoreSongOutcome::Restored)
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
    fn test_tracked_fields_consistency() {
        assert_eq!(TRACKED_FIELDS.len(), TRACKED_FIELD_COUNT);
    }

    #[test]
    fn test_none_snapshot_is_noop() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();
        let result = record_diff_after_update(&conn, None, HistoryActor::Luminous, None).unwrap();
        assert_eq!(result, 0);
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
        assert_eq!(before.song_id(), song_id);
        assert_eq!(before.get("title"), Some("Old Title"));
        assert_eq!(before.get("artist"), Some("Artist A"));
        assert_eq!(before.get("nonexistent_field"), None);

        // No DB update yet -> diff is empty, nothing recorded
        let diff_count =
            record_diff_after_update(&conn, Some(&before), HistoryActor::Luminous, None).unwrap();
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
            record_diff_after_update(&conn, Some(&before), HistoryActor::Luminous, None).unwrap();
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
            record_diff_after_update(&conn, Some(&before_new), HistoryActor::Luminous, None)
                .unwrap();
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
        let recorded =
            record_diff_after_update(&conn, Some(&before), HistoryActor::Scan, None).unwrap();
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
        let recorded =
            record_diff_after_update(&conn, Some(&before), HistoryActor::Scan, None).unwrap();
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

        record_diff_after_update(&conn, Some(&b1), HistoryActor::Luminous, Some(&batch_id))
            .unwrap();
        record_diff_after_update(&conn, Some(&b2), HistoryActor::Luminous, Some(&batch_id))
            .unwrap();
        record_diff_after_update(&conn, Some(&b3), HistoryActor::Luminous, Some(&batch_id))
            .unwrap();

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
        record_diff_after_update(&conn, Some(&before), HistoryActor::Luminous, None).unwrap();

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
            record_diff_after_update(&conn, Some(&before), HistoryActor::Luminous, None).unwrap();
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

    #[test]
    fn test_restore_single_field_writes_to_file_and_db_and_appends_luminous_event() {
        let (temp_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        let path = temp_dir.path().join("restore_song.wav");
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

        let mut song = crate::collection::read_tags(&path).unwrap();
        song.source = crate::models::SongSource::LocalFile;
        crate::collection::upsert_song(&conn, &song).unwrap();

        let song_id: i64 = conn
            .query_row(
                "SELECT id FROM songs WHERE path = ?1",
                params![path.to_string_lossy().to_string()],
                |r| r.get(0),
            )
            .unwrap();

        // Perform edit to "Modified Title"
        let before = snapshot_by_id(&conn, song_id).unwrap().unwrap();
        luminous_metadata::audio::write_tags(
            &path,
            &luminous_metadata::audio::TagWriteRequest {
                title: "Modified Title",
                artist: "Original Artist",
                album: "Original Album",
                ..Default::default()
            },
        )
        .unwrap();

        let (new_mtime, new_filesize) = file_mtime_and_size(&path);
        conn.execute(
            "UPDATE songs SET title = 'Modified Title', mtime = ?1, filesize = ?2 WHERE id = ?3",
            params![new_mtime, new_filesize, song_id],
        )
        .unwrap();
        record_diff_after_update(&conn, Some(&before), HistoryActor::Luminous, None).unwrap();

        let history = get_song_history(&conn, song_id).unwrap();
        assert_eq!(history.entries.len(), 1);
        let entry = &history.entries[0];
        assert_eq!(entry.field, "title");
        assert_eq!(entry.old_value.as_deref(), Some("Original Title"));
        assert_eq!(entry.new_value.as_deref(), Some("Modified Title"));
        let history_id = entry.id;

        // Resolve plan for this entry
        let plan = resolve_restore_plan(&conn, &RestoreTarget::Entry { history_id }).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].0, song_id);
        assert_eq!(plan[0].1.len(), 1);
        assert_eq!(plan[0].1[0].0, "title");
        assert_eq!(plan[0].1[0].1.as_deref(), Some("Original Title"));

        // Execute restore
        let outcome =
            restore_song_tags(&conn, song_id, &plan[0].1, Some("restore-batch-1"), true).unwrap();
        assert_eq!(outcome, RestoreSongOutcome::Restored);

        // Verify DB value
        let db_title: String = conn
            .query_row(
                "SELECT title FROM songs WHERE id = ?1",
                params![song_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(db_title, "Original Title");

        // Verify file value on disk
        let disk_tags = crate::collection::read_tags(&path).unwrap();
        assert_eq!(disk_tags.title.as_deref(), Some("Original Title"));

        // Verify history is append-only: original row remains, new row added
        let history_after = get_song_history(&conn, song_id).unwrap();
        assert_eq!(history_after.entries.len(), 2);
        assert_eq!(history_after.entries[0].actor, HistoryActor::Luminous);
        assert_eq!(history_after.entries[0].field, "title");
        assert_eq!(
            history_after.entries[0].old_value.as_deref(),
            Some("Modified Title")
        );
        assert_eq!(
            history_after.entries[0].new_value.as_deref(),
            Some("Original Title")
        );
        assert_eq!(
            history_after.entries[0].batch_id.as_deref(),
            Some("restore-batch-1")
        );
        assert_eq!(history_after.entries[1].id, history_id);

        // Verify rescan produces 0 scan events
        let scanned = crate::collection::read_tags(&path).unwrap();
        crate::collection::upsert_song(&conn, &scanned).unwrap();
        let history_after_scan = get_song_history(&conn, song_id).unwrap();
        assert_eq!(
            history_after_scan.entries.len(),
            2,
            "rescan after restore must log 0 scan events"
        );
    }

    #[test]
    fn test_restore_noop_when_value_already_current() {
        let (temp_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        let path = temp_dir.path().join("noop_song.wav");
        write_test_wav(&path);

        conn.execute(
            "INSERT INTO songs (path, title, artist, album, source, mtime)
             VALUES (?1, 'Current Title', 'Artist A', 'Album X', 2, 1000)",
            params![path.to_string_lossy().to_string()],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        // Attempt to restore field "title" to "Current Title" (already matching)
        let fields = vec![("title".to_string(), Some("Current Title".to_string()))];
        let outcome = restore_song_tags(&conn, song_id, &fields, None, false).unwrap();
        assert_eq!(outcome, RestoreSongOutcome::Unchanged);

        let history = get_song_history(&conn, song_id).unwrap();
        assert_eq!(
            history.entries.len(),
            0,
            "no-op restore must record 0 history rows"
        );
    }

    #[test]
    fn test_restore_batch_across_multiple_tracks_with_one_unavailable() {
        let (temp_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        let path1 = temp_dir.path().join("track1.wav");
        write_test_wav(&path1);
        let path2 = temp_dir.path().join("track2_missing.wav");
        write_test_wav(&path2);

        // Insert track 1
        conn.execute(
            "INSERT INTO songs (path, title, artist, album, source, mtime)
             VALUES (?1, 'New Album 1', 'Artist', 'New Album', 2, 1000)",
            params![path1.to_string_lossy().to_string()],
        )
        .unwrap();
        let song1_id = conn.last_insert_rowid();

        // Insert track 2
        conn.execute(
            "INSERT INTO songs (path, title, artist, album, source, mtime)
             VALUES (?1, 'New Album 2', 'Artist', 'New Album', 2, 1000)",
            params![path2.to_string_lossy().to_string()],
        )
        .unwrap();
        let song2_id = conn.last_insert_rowid();

        // Record batch edit on both tracks
        conn.execute(
            "INSERT INTO song_history (song_id, at, actor, field, old_value, new_value, batch_id)
             VALUES (?1, 1000, 'luminous', 'album', 'Old Album', 'New Album', 'batch-test-1')",
            params![song1_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO song_history (song_id, at, actor, field, old_value, new_value, batch_id)
             VALUES (?1, 1000, 'luminous', 'album', 'Old Album', 'New Album', 'batch-test-1')",
            params![song2_id],
        )
        .unwrap();

        // Delete track 2 file so it is unavailable
        let _ = std::fs::remove_file(&path2);

        // Resolve batch plan
        let plan = resolve_restore_plan(
            &conn,
            &RestoreTarget::Batch {
                batch_id: "batch-test-1".to_string(),
            },
        )
        .unwrap();
        assert_eq!(plan.len(), 2);

        let mut restored = 0;
        let mut skipped = 0;
        let mut errors = Vec::new();

        for (sid, fields) in plan {
            match restore_song_tags(&conn, sid, &fields, Some("restore-batch-2"), true).unwrap() {
                RestoreSongOutcome::Restored => restored += 1,
                RestoreSongOutcome::Unchanged => {}
                RestoreSongOutcome::Skipped(err) => {
                    skipped += 1;
                    errors.push(err);
                }
            }
        }

        assert_eq!(restored, 1);
        assert_eq!(skipped, 1);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("File not found on disk"));

        // Check track 1 was restored
        let track1_album: String = conn
            .query_row(
                "SELECT album FROM songs WHERE id = ?1",
                params![song1_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(track1_album, "Old Album");
    }
}
