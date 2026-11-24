//! Lossless audio provenance verification & fake FLAC detection (#1373, epic #1441).
//!
//! Handles SQLite persistence (`song_provenance` table), cache validation keyed on
//! `songs.mtime`, and offline DSP execution via `luminous_analysis`.

use crate::db::Database;
use crate::models::{FileType, SongSource, LOCAL_SOURCES_SQL};
use anyhow::{bail, Context, Result};
pub use luminous_analysis::{analyze_provenance, ProvenanceAnalysis, ProvenanceVerdict};
use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Lossless file types in SQLite songs table (flac: 2, oggflac: 3, alac: 8, aiff: 9, wav: 10, wavpack: 11, trueaudio: 13, ape: 14, dsf: 15, dsdiff: 16).
pub(crate) const LOSSLESS_FILETYPES_SQL: &str = "2, 3, 8, 9, 10, 11, 13, 14, 15, 16";

/// Advisory provenance verification verdict and spectral metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongProvenance {
    pub song_id: i64,
    pub verdict: ProvenanceVerdict,
    pub cutoff_hz: Option<u32>,
    pub effective_bit_depth: Option<u32>,
    pub effective_sample_rate: Option<u32>,
    pub description: String,
    pub is_stale: bool,
    pub analyzed_at: i64,
}

/// Sentinel verdict stored in `song_provenance` when a reachable local file
/// fails to open or decode, suppressing retries until its `mtime` changes (#2354).
const UNREADABLE_VERDICT: &str = "unreadable";

fn parse_verdict_str(s: &str) -> Option<ProvenanceVerdict> {
    match s {
        "authentic" => Some(ProvenanceVerdict::Authentic),
        "authentic_hires" => Some(ProvenanceVerdict::AuthenticHiRes),
        "suspected_lossy" => Some(ProvenanceVerdict::SuspectedLossy),
        "suspected_upsampled" => Some(ProvenanceVerdict::SuspectedUpsampled),
        "padded_bit_depth" => Some(ProvenanceVerdict::PaddedBitDepth),
        _ => None,
    }
}

fn verdict_to_str(v: ProvenanceVerdict) -> &'static str {
    match v {
        ProvenanceVerdict::Authentic => "authentic",
        ProvenanceVerdict::AuthenticHiRes => "authentic_hires",
        ProvenanceVerdict::SuspectedLossy => "suspected_lossy",
        ProvenanceVerdict::SuspectedUpsampled => "suspected_upsampled",
        ProvenanceVerdict::PaddedBitDepth => "padded_bit_depth",
    }
}

/// Background analysis of unverified local lossless tracks, newest first,
/// scheduled by `crate::background_reads`.
///
/// Reachable files that cannot be decoded are recorded in `song_provenance`
/// with `verdict = 'unreadable'` keyed by `mtime`, so they are not retried
/// across launches until the file changes (#2354). Unreachable files (e.g.
/// asleep drive or disconnected share) store nothing and are skipped only for
/// the rest of the session.
pub struct ProvenanceReader {
    app: AppHandle,
    db: Arc<Database>,
}

impl ProvenanceReader {
    pub fn new(app: AppHandle, db: Arc<Database>) -> Self {
        Self { app, db }
    }
}

fn next_song_to_verify(
    conn: &rusqlite::Connection,
    skipped: &crate::background_reads::Skipped,
) -> rusqlite::Result<Option<(i64, String)>> {
    if !crate::commands::settings::background_verification_enabled(conn) {
        return Ok(None);
    }
    conn.query_row(
        &format!(
            "SELECT s.id, s.path FROM songs s
             LEFT JOIN song_provenance p ON s.id = p.song_id
             WHERE s.source IN ({lib}) AND s.unavailable = 0 AND s.path IS NOT NULL
               AND s.filetype IN ({lossless})
               AND (p.song_id IS NULL OR p.mtime != s.mtime)
               AND {skip}
             ORDER BY s.added DESC, s.id DESC LIMIT 1",
            lib = *LOCAL_SOURCES_SQL,
            lossless = LOSSLESS_FILETYPES_SQL,
            skip = skipped.sql_excluding("s.id")
        ),
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

impl crate::background_reads::BackgroundRead for ProvenanceReader {
    fn name(&self) -> &'static str {
        "provenance"
    }

    fn next(
        &mut self,
        conn: &rusqlite::Connection,
        skipped: &crate::background_reads::Skipped,
    ) -> rusqlite::Result<Option<(i64, String)>> {
        next_song_to_verify(conn, skipped)
    }

    fn read(&mut self, song_id: i64, _path: &Path) -> crate::background_reads::ReadOutcome {
        let (outcome, prov) = verify_for_background(&self.db, song_id);
        if let Some(prov) = prov {
            let _ = self.app.emit("song-provenance-changed", &prov);
        }
        outcome
    }
}

fn verify_for_background(
    db: &Database,
    song_id: i64,
) -> (crate::background_reads::ReadOutcome, Option<SongProvenance>) {
    match verify_song_provenance_inner(db, song_id) {
        Ok(VerifyAttempt::Verified(prov)) => {
            (crate::background_reads::ReadOutcome::Stored, Some(prov))
        }
        Ok(VerifyAttempt::RecordedUnreadable(e)) => {
            log::debug!("Background provenance marked song {song_id} unreadable: {e}");
            (crate::background_reads::ReadOutcome::Stored, None)
        }
        Err(e) => {
            log::debug!("Background provenance skipped song {song_id}: {e}");
            (crate::background_reads::ReadOutcome::NothingStored, None)
        }
    }
}

/// Retrieve the cached provenance analysis for a song, checking if the file has modified on disk since.
pub fn get_cached_provenance(db: &Database, song_id: i64) -> Result<Option<SongProvenance>> {
    let conn = db.pool.get().context("failed to get db connection")?;
    let mut stmt = conn.prepare_cached(
        "SELECT p.mtime, p.verdict, p.cutoff_hz, p.effective_bit_depth, p.effective_sample_rate, p.details, p.analyzed_at, s.mtime
         FROM song_provenance p
         JOIN songs s ON s.id = p.song_id
         WHERE p.song_id = ?1",
    )?;

    let row = stmt
        .query_row(params![song_id], |row| {
            let p_mtime: i64 = row.get(0)?;
            let verdict_str: String = row.get(1)?;
            let Some(verdict) = parse_verdict_str(&verdict_str) else {
                return Ok(None);
            };
            let cutoff_hz: Option<u32> = row.get(2)?;
            let effective_bit_depth: Option<u32> = row.get(3)?;
            let effective_sample_rate: Option<u32> = row.get(4)?;
            let description: String = row.get(5)?;
            let analyzed_at: i64 = row.get(6)?;
            let s_mtime: Option<i64> = row.get(7)?;

            let is_stale = match s_mtime {
                Some(current_mtime) => current_mtime != p_mtime,
                None => false,
            };

            Ok(Some(SongProvenance {
                song_id,
                verdict,
                cutoff_hz,
                effective_bit_depth,
                effective_sample_rate,
                description,
                is_stale,
                analyzed_at,
            }))
        })
        .optional()?
        .flatten();

    Ok(row)
}

/// Retrieve all cached provenance verdicts mapped by song_id.
pub fn get_all_provenance_verdicts(
    db: &Database,
) -> Result<std::collections::HashMap<i64, String>> {
    let conn = db.pool.get().context("failed to get db connection")?;
    let mut stmt =
        conn.prepare("SELECT song_id, verdict FROM song_provenance WHERE verdict != ?1")?;
    let rows = stmt.query_map(params![UNREADABLE_VERDICT], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut map = std::collections::HashMap::new();
    for (id, verdict) in rows.flatten() {
        map.insert(id, verdict);
    }
    Ok(map)
}

enum VerifyAttempt {
    Verified(SongProvenance),
    RecordedUnreadable(anyhow::Error),
}

/// Verify a song's audio provenance, saving the result to `song_provenance`.
pub fn verify_song_provenance(db: &Database, song_id: i64) -> Result<SongProvenance> {
    match verify_song_provenance_inner(db, song_id)? {
        VerifyAttempt::Verified(prov) => Ok(prov),
        VerifyAttempt::RecordedUnreadable(err) => Err(err),
    }
}

fn verify_song_provenance_inner(db: &Database, song_id: i64) -> Result<VerifyAttempt> {
    let conn = db.pool.get().context("failed to get db connection")?;

    let (path_str, source_raw, filetype_raw, mtime_val): (Option<String>, i64, i64, Option<i64>) =
        conn.query_row(
            "SELECT path, source, filetype, mtime FROM songs WHERE id = ?1",
            params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;

    let source = SongSource::from(source_raw);
    if !source.is_local() {
        bail!("Provenance verification is only supported for local audio files");
    }

    let Some(path) = path_str else {
        bail!("Song has no file path");
    };

    let filetype = FileType::from(filetype_raw);
    if !filetype.is_lossless() {
        bail!(
            "Provenance verification applies only to lossless formats ({})",
            filetype.display_name()
        );
    }

    let file_path = Path::new(&path);
    if !luminous_analysis::is_file_reachable(file_path) {
        bail!("Couldn't reach {path} to verify provenance");
    }

    let mtime = mtime_val.unwrap_or(0);

    let analysis_res = crate::media_source::open_offline(db, file_path)
        .with_context(|| format!("Failed to open audio decoder for song {song_id} at {path}"))
        .and_then(|decoder| {
            luminous_analysis::analyze_provenance(decoder)
                .map_err(|e| anyhow::anyhow!("Provenance analysis failed: {e}"))
        });

    let analysis = match analysis_res {
        Ok(a) => a,
        Err(err) => {
            // If the drive went away mid-read, treat it as unreachable rather
            // than recording a permanent failure (#2162, #2354).
            if !luminous_analysis::is_file_reachable(file_path) {
                return Err(err);
            }
            let detail = format!("{err:#}");
            conn.execute(
                "INSERT OR REPLACE INTO song_provenance
                 (song_id, mtime, verdict, cutoff_hz, effective_bit_depth, effective_sample_rate, details, analyzed_at)
                 VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4, strftime('%s', 'now'))",
                params![song_id, mtime, UNREADABLE_VERDICT, detail],
            )?;
            return Ok(VerifyAttempt::RecordedUnreadable(err));
        }
    };

    let verdict_str = verdict_to_str(analysis.verdict);

    conn.execute(
        "INSERT OR REPLACE INTO song_provenance
         (song_id, mtime, verdict, cutoff_hz, effective_bit_depth, effective_sample_rate, details, analyzed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%s', 'now'))",
        params![
            song_id,
            mtime,
            verdict_str,
            analysis.cutoff_hz,
            analysis.effective_bit_depth,
            analysis.effective_sample_rate,
            analysis.description,
        ],
    )?;

    let analyzed_at: i64 = conn.query_row(
        "SELECT analyzed_at FROM song_provenance WHERE song_id = ?1",
        params![song_id],
        |row| row.get(0),
    )?;

    Ok(VerifyAttempt::Verified(SongProvenance {
        song_id,
        verdict: analysis.verdict,
        cutoff_hz: analysis.cutoff_hz,
        effective_bit_depth: analysis.effective_bit_depth,
        effective_sample_rate: analysis.effective_sample_rate,
        description: analysis.description,
        is_stale: false,
        analyzed_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provenance_roundtrip_and_staleness() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_prov_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir_guard.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, title, source, filetype, mtime) VALUES ('/test.flac', 'Test', 2, 2, 1000)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        // Initially no cached provenance
        let initial = get_cached_provenance(&db, song_id).unwrap();
        assert!(initial.is_none());

        // Insert cached record
        conn.execute(
            "INSERT INTO song_provenance (song_id, mtime, verdict, cutoff_hz, effective_bit_depth, effective_sample_rate, details)
             VALUES (?1, 1000, 'authentic', 21800, 16, 44100, 'Authentic CD')",
            params![song_id],
        )
        .unwrap();

        let cached = get_cached_provenance(&db, song_id)
            .unwrap()
            .expect("cached entry");
        assert_eq!(cached.verdict, ProvenanceVerdict::Authentic);
        assert_eq!(cached.cutoff_hz, Some(21800));
        assert_eq!(cached.effective_bit_depth, Some(16));
        assert!(!cached.is_stale);

        // Update song mtime to simulate file edit
        conn.execute(
            "UPDATE songs SET mtime = 2000 WHERE id = ?1",
            params![song_id],
        )
        .unwrap();

        let updated = get_cached_provenance(&db, song_id)
            .unwrap()
            .expect("cached entry");
        assert!(
            updated.is_stale,
            "cached provenance should be marked stale when mtime changes"
        );
    }

    #[test]
    fn background_provenance_skips_when_toggle_is_off() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_prov_toggle_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir_guard.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, title, source, filetype, mtime) VALUES ('/test.flac', 'Test', 2, 2, 1000)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();
        let skipped = crate::background_reads::Skipped::default();

        // Off by default: background reader offers nothing.
        assert_eq!(next_song_to_verify(&conn, &skipped).unwrap(), None);

        // Opting in lets the background reader pick the unverified lossless song.
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('verify_audio_integrity_and_provenance', 'true')",
            [],
        )
        .unwrap();
        assert_eq!(
            next_song_to_verify(&conn, &skipped).unwrap(),
            Some((song_id, "/test.flac".to_string()))
        );
    }

    #[test]
    fn background_provenance_records_unreadable_file_and_skips_until_mtime_changes() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_prov_unreadable_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir_guard.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        let corrupt_path = temp_dir_guard.path().join("corrupt.flac");
        std::fs::write(&corrupt_path, b"not a valid flac stream").unwrap();
        let path_str = corrupt_path.to_string_lossy().into_owned();

        conn.execute(
            "INSERT INTO songs (path, title, source, filetype, mtime) VALUES (?1, 'Corrupt', 2, 2, 1000)",
            params![path_str],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('verify_audio_integrity_and_provenance', 'true')",
            [],
        )
        .unwrap();

        // Manual verification still reports an error.
        assert!(verify_song_provenance(&db, song_id).is_err());

        // Background reader persists the failure marker and reports Stored so
        // the runner does not retry it on every launch.
        let (outcome, emitted) = verify_for_background(&db, song_id);
        assert_eq!(outcome, crate::background_reads::ReadOutcome::Stored);
        assert!(emitted.is_none());

        let (verdict, mtime): (String, i64) = conn
            .query_row(
                "SELECT verdict, mtime FROM song_provenance WHERE song_id = ?1",
                params![song_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(verdict, UNREADABLE_VERDICT);
        assert_eq!(mtime, 1000);

        // Suppressed on subsequent launches (fresh Skipped set) and hidden from
        // user-facing provenance lookups.
        let skipped = crate::background_reads::Skipped::default();
        assert_eq!(next_song_to_verify(&conn, &skipped).unwrap(), None);
        assert!(get_cached_provenance(&db, song_id).unwrap().is_none());
        assert!(get_all_provenance_verdicts(&db).unwrap().is_empty());

        // Changing mtime re-queues the file for verification.
        conn.execute(
            "UPDATE songs SET mtime = 2000 WHERE id = ?1",
            params![song_id],
        )
        .unwrap();
        assert_eq!(
            next_song_to_verify(&conn, &skipped).unwrap(),
            Some((song_id, path_str))
        );
    }

    #[test]
    fn background_provenance_stores_nothing_for_unreachable_file() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_prov_unreachable_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir_guard.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        let missing_path = temp_dir_guard.path().join("unplugged.flac");
        let path_str = missing_path.to_string_lossy().into_owned();

        conn.execute(
            "INSERT INTO songs (path, title, source, filetype, mtime) VALUES (?1, 'Unplugged', 2, 2, 1000)",
            params![path_str],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('verify_audio_integrity_and_provenance', 'true')",
            [],
        )
        .unwrap();

        let (outcome, emitted) = verify_for_background(&db, song_id);
        assert_eq!(outcome, crate::background_reads::ReadOutcome::NothingStored);
        assert!(emitted.is_none());

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM song_provenance WHERE song_id = ?1",
                params![song_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);

        let skipped = crate::background_reads::Skipped::default();
        assert_eq!(
            next_song_to_verify(&conn, &skipped).unwrap(),
            Some((song_id, path_str))
        );
    }
}
