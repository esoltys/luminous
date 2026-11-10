//! Detected tempo and key: storage rules and the background worker (#1943).
//!
//! `songs.bpm` and `songs.initial_key` hold whatever the file's tags say. Detection only ever
//! *fills gaps*: a value a tag (or the user) supplied is never overwritten. The
//! `song_musical_analysis` table remembers which values were detected, so the Tag Editor can show
//! them as estimates, and which file version was analysed, so each file is analysed once.
//! Saving tags from the Tag Editor makes the song user-curated (see [`mark_user_curated`]): its
//! values, including deliberately empty ones, are left alone by detection until its file changes
//! on disk.

use crate::db::Database;
use crate::models::LOCAL_SOURCES_SQL;
use luminous_analysis::MusicalAnalysis;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Bump when the estimators change enough that earlier results should be recomputed.
pub const MUSICAL_ANALYSIS_VERSION: i64 = 1;

/// Which of a song's values were detected rather than tagged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DetectedFlags {
    pub bpm: bool,
    pub key: bool,
}

/// The flags for a song, `false`/`false` if it was never analysed or its file changed since
/// (a rescan may have replaced the detected values with the file's own tags).
pub fn detected_flags(conn: &Connection, song_id: i64) -> rusqlite::Result<DetectedFlags> {
    Ok(conn
        .query_row(
            "SELECT a.bpm_detected, a.key_detected FROM song_musical_analysis a
             JOIN songs s ON s.id = a.song_id
             WHERE a.song_id = ?1 AND a.mtime IS s.mtime",
            params![song_id],
            |row| {
                Ok(DetectedFlags {
                    bpm: row.get::<_, i64>(0)? != 0,
                    key: row.get::<_, i64>(1)? != 0,
                })
            },
        )
        .optional()?
        .unwrap_or_default())
}

/// Stores `analysis` for the song: fills `bpm` / `initial_key` where the song has no value (or
/// where its value is an earlier detection) and records the analysis, so the song is not
/// analysed again until its file changes. An analysis with nothing in it is still recorded.
pub fn record_analysis(
    conn: &Connection,
    song_id: i64,
    analysis: &MusicalAnalysis,
) -> rusqlite::Result<()> {
    if is_curated(conn, song_id)? {
        return Ok(());
    }
    let previous = detected_flags(conn, song_id)?;

    let bpm_written = match analysis.bpm {
        Some(bpm) => {
            conn.execute(
                "UPDATE songs SET bpm = ?1 WHERE id = ?2 AND (bpm IS NULL OR bpm <= 0 OR ?3)",
                params![bpm, song_id, previous.bpm],
            )? > 0
        }
        None => false,
    };
    let key_written = match &analysis.key {
        Some(key) => {
            conn.execute(
                "UPDATE songs SET initial_key = ?1
                 WHERE id = ?2 AND (initial_key IS NULL OR TRIM(initial_key) = '' OR ?3)",
                params![key.standard(), song_id, previous.key],
            )? > 0
        }
        None => false,
    };

    upsert_marker(
        conn,
        song_id,
        false,
        DetectedFlags {
            // A value detected earlier and not re-estimated this time is still a detection.
            bpm: bpm_written || (previous.bpm && analysis.bpm.is_none()),
            key: key_written || (previous.key && analysis.key.is_none()),
        },
    )
}

/// The user has set (or deliberately cleared) this song's tags: whatever is there is theirs, and
/// detection must not fill or replace it. Call after any Tag Editor save.
pub fn mark_user_curated(conn: &Connection, song_id: i64) -> rusqlite::Result<()> {
    upsert_marker(conn, song_id, true, DetectedFlags::default())
}

/// Whether the user's own tags for this song are current (set on its present file version).
fn is_curated(conn: &Connection, song_id: i64) -> rusqlite::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT a.curated FROM song_musical_analysis a JOIN songs s ON s.id = a.song_id
             WHERE a.song_id = ?1 AND a.mtime IS s.mtime",
            params![song_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0)
        != 0)
}

/// [`mark_user_curated`] for a song identified by path; a path not in the library is a no-op.
pub fn mark_user_curated_by_path(conn: &Connection, path: &str) {
    let id: Option<i64> = conn
        .query_row("SELECT id FROM songs WHERE path = ?1", params![path], |r| {
            r.get(0)
        })
        .optional()
        .unwrap_or(None);
    if let Some(id) = id {
        let _ = mark_user_curated(conn, id);
    }
}

/// Clears the tempo/key of `song` that were detected rather than tagged, so writing the song's
/// tags back to its file never turns an estimate into a tag the user didn't choose.
pub fn strip_detected(conn: &Connection, song: &mut crate::models::Song) {
    let flags = detected_flags(conn, song.id).unwrap_or_default();
    if flags.bpm {
        song.bpm = None;
    }
    if flags.key {
        song.initial_key = None;
    }
}

fn upsert_marker(
    conn: &Connection,
    song_id: i64,
    curated: bool,
    flags: DetectedFlags,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO song_musical_analysis
            (song_id, mtime, version, bpm_detected, key_detected, curated)
         VALUES (?1, (SELECT mtime FROM songs WHERE id = ?1), ?2, ?3, ?4, ?5)
         ON CONFLICT(song_id) DO UPDATE SET
            mtime = excluded.mtime,
            version = excluded.version,
            bpm_detected = excluded.bpm_detected,
            key_detected = excluded.key_detected,
            curated = excluded.curated",
        params![
            song_id,
            MUSICAL_ANALYSIS_VERSION,
            flags.bpm as i64,
            flags.key as i64,
            curated as i64
        ],
    )?;
    Ok(())
}

/// SQL condition (over `songs s` left-joined to `song_musical_analysis a`, with the current
/// version as `?1`) for a song that needs analysing: never analysed, its file changed since, or
/// analysed by an older version, unless the user has curated its tags on this file version.
const NEEDS_ANALYSIS_SQL: &str =
    "(a.song_id IS NULL OR a.mtime IS NOT s.mtime OR (a.version != ?1 AND a.curated = 0))";

/// Whether the song still lacks a current analysis (see [`NEEDS_ANALYSIS_SQL`]).
pub fn needs_analysis(conn: &Connection, song_id: i64) -> rusqlite::Result<bool> {
    conn.query_row(
        &format!(
            "SELECT {NEEDS_ANALYSIS_SQL} FROM songs s
             LEFT JOIN song_musical_analysis a ON a.song_id = s.id
             WHERE s.id = ?2"
        ),
        params![MUSICAL_ANALYSIS_VERSION, song_id],
        |row| row.get(0),
    )
}

/// The next local song worth decoding: one missing a tempo or key and not yet analysed.
/// Songs that already carry both tags are never decoded for this.
fn next_song_to_analyze(conn: &Connection) -> rusqlite::Result<Option<(i64, String)>> {
    conn.query_row(
        &format!(
            "SELECT s.id, s.path FROM songs s
             LEFT JOIN song_musical_analysis a ON a.song_id = s.id
             WHERE s.source IN ({lib}) AND s.unavailable = 0 AND s.path IS NOT NULL
               AND (s.bpm IS NULL OR s.bpm <= 0
                    OR s.initial_key IS NULL OR TRIM(s.initial_key) = '')
               AND {needs}
             ORDER BY s.added DESC, s.id DESC LIMIT 1",
            lib = *LOCAL_SOURCES_SQL,
            needs = NEEDS_ANALYSIS_SQL
        ),
        params![MUSICAL_ANALYSIS_VERSION],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

/// How often the library views are told about detected values while the worker is busy.
const LIBRARY_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Spawn the low-priority background worker that fills in tempo and key for tracks lacking them.
/// One track at a time, throttled, and decoded through the same pass that caches the waveforms,
/// so each file is read once for all of it.
pub fn spawn_background_analyzer(app: AppHandle, db: Arc<Database>) {
    std::thread::Builder::new()
        .name("luminous-musical".to_string())
        .spawn(move || {
            let mut unannounced = false;
            let mut last_announce = Instant::now();
            loop {
                let next = db
                    .pool
                    .get()
                    .ok()
                    .and_then(|conn| next_song_to_analyze(&conn).unwrap_or(None));

                let Some((song_id, path_str)) = next else {
                    if std::mem::take(&mut unannounced) {
                        let _ = app.emit("library-changed", ());
                        last_announce = Instant::now();
                    }
                    std::thread::sleep(Duration::from_secs(15));
                    continue;
                };

                let path = std::path::Path::new(&path_str);
                if let Err(e) = crate::waveform::analyze_song(&db, song_id, path) {
                    log::debug!("Musical analysis skipped song {song_id}: {e}");
                    // Remember the attempt so an undecodable file is not retried in a loop.
                    if let Ok(conn) = db.pool.get() {
                        let _ = record_analysis(
                            &conn,
                            song_id,
                            &MusicalAnalysis {
                                bpm: None,
                                key: None,
                            },
                        );
                    }
                }
                unannounced = true;

                if last_announce.elapsed() >= LIBRARY_REFRESH_INTERVAL {
                    let _ = app.emit("library-changed", ());
                    unannounced = false;
                    last_announce = Instant::now();
                }

                // Low-priority: yield between tracks so playback and scanning are never starved.
                std::thread::sleep(Duration::from_millis(250));
            }
        })
        .expect("failed to spawn musical analyzer thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use luminous_analysis::KeySignature;

    fn setup() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        (dir, db)
    }

    fn add_song(db: &Database, bpm: Option<f32>, key: Option<&str>, mtime: i64) -> i64 {
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO songs (path, unavailable, source, bpm, initial_key, mtime, added)
             VALUES (?1, 0, ?2, ?3, ?4, ?5, 1)",
            params![
                format!("/music/{mtime}-{bpm:?}-{key:?}.flac"),
                crate::models::SongSource::LocalFile as i32,
                bpm,
                key,
                mtime
            ],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn values(db: &Database, id: i64) -> (Option<f32>, Option<String>) {
        db.pool
            .get()
            .unwrap()
            .query_row(
                "SELECT bpm, initial_key FROM songs WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    }

    fn found(bpm: f32, key: &str) -> MusicalAnalysis {
        MusicalAnalysis {
            bpm: Some(bpm),
            key: KeySignature::parse(key),
        }
    }

    #[test]
    fn fills_empty_values_and_marks_them_detected() {
        let (_dir, db) = setup();
        let id = add_song(&db, None, None, 10);
        let conn = db.pool.get().unwrap();
        record_analysis(&conn, id, &found(120.0, "Am")).unwrap();

        assert_eq!(values(&db, id), (Some(120.0), Some("Am".to_string())));
        assert_eq!(
            detected_flags(&conn, id).unwrap(),
            DetectedFlags {
                bpm: true,
                key: true
            }
        );
        assert!(!needs_analysis(&conn, id).unwrap());
    }

    #[test]
    fn never_overwrites_tagged_values() {
        let (_dir, db) = setup();
        let id = add_song(&db, Some(98.0), Some("8B"), 10);
        let conn = db.pool.get().unwrap();
        record_analysis(&conn, id, &found(120.0, "Am")).unwrap();

        assert_eq!(values(&db, id), (Some(98.0), Some("8B".to_string())));
        assert_eq!(detected_flags(&conn, id).unwrap(), DetectedFlags::default());
    }

    #[test]
    fn fills_only_the_missing_half() {
        let (_dir, db) = setup();
        let id = add_song(&db, Some(98.0), Some("  "), 10);
        let conn = db.pool.get().unwrap();
        record_analysis(&conn, id, &found(120.0, "C")).unwrap();

        assert_eq!(values(&db, id), (Some(98.0), Some("C".to_string())));
        assert_eq!(
            detected_flags(&conn, id).unwrap(),
            DetectedFlags {
                bpm: false,
                key: true
            }
        );
    }

    #[test]
    fn an_empty_analysis_is_recorded_so_it_is_not_repeated() {
        let (_dir, db) = setup();
        let id = add_song(&db, None, None, 10);
        let conn = db.pool.get().unwrap();
        assert!(needs_analysis(&conn, id).unwrap());
        record_analysis(
            &conn,
            id,
            &MusicalAnalysis {
                bpm: None,
                key: None,
            },
        )
        .unwrap();

        assert_eq!(values(&db, id), (None, None));
        assert!(!needs_analysis(&conn, id).unwrap());
        assert_eq!(next_song_to_analyze(&conn).unwrap(), None);
    }

    #[test]
    fn a_changed_file_is_analysed_again() {
        let (_dir, db) = setup();
        let id = add_song(&db, None, None, 10);
        let conn = db.pool.get().unwrap();
        record_analysis(&conn, id, &found(120.0, "Am")).unwrap();

        // A rescan picks up a modified file whose tags carry no tempo or key.
        conn.execute(
            "UPDATE songs SET bpm = NULL, initial_key = NULL, mtime = 20 WHERE id = ?1",
            params![id],
        )
        .unwrap();

        assert_eq!(detected_flags(&conn, id).unwrap(), DetectedFlags::default());
        assert!(needs_analysis(&conn, id).unwrap());
        assert_eq!(next_song_to_analyze(&conn).unwrap().map(|s| s.0), Some(id));
    }

    #[test]
    fn user_curated_songs_are_left_alone() {
        let (_dir, db) = setup();
        let id = add_song(&db, None, None, 10);
        let conn = db.pool.get().unwrap();
        record_analysis(&conn, id, &found(120.0, "Am")).unwrap();

        // The user clears the detected tempo in the Tag Editor and saves.
        conn.execute("UPDATE songs SET bpm = NULL WHERE id = ?1", params![id])
            .unwrap();
        mark_user_curated(&conn, id).unwrap();

        assert_eq!(detected_flags(&conn, id).unwrap(), DetectedFlags::default());
        assert_eq!(next_song_to_analyze(&conn).unwrap(), None);
        // Neither a decode for another reason nor a version bump refills the gap they left.
        record_analysis(&conn, id, &found(130.0, "Am")).unwrap();
        assert_eq!(values(&db, id).0, None);
        conn.execute("UPDATE song_musical_analysis SET version = 0", [])
            .unwrap();
        assert_eq!(next_song_to_analyze(&conn).unwrap(), None);

        // But a changed file starts over.
        conn.execute("UPDATE songs SET mtime = 99 WHERE id = ?1", params![id])
            .unwrap();
        assert_eq!(next_song_to_analyze(&conn).unwrap().map(|s| s.0), Some(id));
    }

    #[test]
    fn songs_with_both_tags_are_never_queued() {
        let (_dir, db) = setup();
        add_song(&db, Some(100.0), Some("Am"), 10);
        let missing = add_song(&db, Some(100.0), None, 11);
        let conn = db.pool.get().unwrap();
        assert_eq!(
            next_song_to_analyze(&conn).unwrap().map(|s| s.0),
            Some(missing)
        );
    }

    #[test]
    fn detected_values_are_stripped_before_writing_tags() {
        let (_dir, db) = setup();
        let id = add_song(&db, None, Some("Gm"), 10);
        let conn = db.pool.get().unwrap();
        record_analysis(&conn, id, &found(120.0, "Am")).unwrap();

        let mut song = conn
            .query_row(
                &format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                ),
                [id],
                crate::collection::row_to_song,
            )
            .unwrap();
        strip_detected(&conn, &mut song);
        assert_eq!(song.bpm, None);
        // The key was tagged, so it stays.
        assert_eq!(song.initial_key.as_deref(), Some("Gm"));
    }
}
