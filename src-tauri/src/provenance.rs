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
use std::time::Duration;
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

fn parse_verdict_str(s: &str) -> ProvenanceVerdict {
    match s {
        "authentic_hires" => ProvenanceVerdict::AuthenticHiRes,
        "suspected_lossy" => ProvenanceVerdict::SuspectedLossy,
        "suspected_upsampled" => ProvenanceVerdict::SuspectedUpsampled,
        "padded_bit_depth" => ProvenanceVerdict::PaddedBitDepth,
        _ => ProvenanceVerdict::Authentic,
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

/// Spawn a low-priority background thread that analyzes unverified local lossless tracks.
/// Throttles between tracks so it never interferes with playback or UI responsiveness.
pub fn spawn_background_analyzer(app: AppHandle, db: Arc<Database>) {
    std::thread::Builder::new()
        .name("luminous-provenance".to_string())
        .spawn(move || {
            loop {
                let next: Option<(i64, String)> = match db.pool.get() {
                    Ok(conn) => conn
                        .query_row(
                            &format!(
                                "SELECT s.id, s.path FROM songs s
                                 LEFT JOIN song_provenance p ON s.id = p.song_id
                                 WHERE s.source IN ({lib}) AND s.unavailable = 0 AND s.path IS NOT NULL
                                   AND s.filetype IN ({lossless})
                                   AND (p.song_id IS NULL OR p.mtime != s.mtime)
                                 ORDER BY s.added DESC, s.id DESC LIMIT 1",
                                lib = *LOCAL_SOURCES_SQL,
                                lossless = LOSSLESS_FILETYPES_SQL
                            ),
                            [],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()
                        .unwrap_or(None),
                    Err(_) => None,
                };

                let Some((song_id, _path)) = next else {
                    std::thread::sleep(Duration::from_secs(15));
                    continue;
                };

                match verify_song_provenance(&db, song_id) {
                    Ok(prov) => {
                        let _ = app.emit("song-provenance-changed", &prov);
                    }
                    Err(e) => {
                        log::debug!("Background provenance skipped song {song_id}: {e}");
                    }
                }

                // Throttle between tracks so background analysis is gentle on disk/CPU
                std::thread::sleep(Duration::from_millis(250));
            }
        })
        .expect("failed to spawn provenance analyzer thread");
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

            Ok(SongProvenance {
                song_id,
                verdict: parse_verdict_str(&verdict_str),
                cutoff_hz,
                effective_bit_depth,
                effective_sample_rate,
                description,
                is_stale,
                analyzed_at,
            })
        })
        .ok();

    Ok(row)
}

/// Verify a song's audio provenance, saving the result to `song_provenance`.
pub fn verify_song_provenance(db: &Database, song_id: i64) -> Result<SongProvenance> {
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

    let mtime = mtime_val.unwrap_or(0);

    let decoder = crate::media_source::open_offline(db, Path::new(&path))
        .with_context(|| format!("Failed to open audio decoder for song {song_id} at {path}"))?;

    let analysis = luminous_analysis::analyze_provenance(decoder)
        .map_err(|e| anyhow::anyhow!("Provenance analysis failed: {e}"))?;

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

    Ok(SongProvenance {
        song_id,
        verdict: analysis.verdict,
        cutoff_hz: analysis.cutoff_hz,
        effective_bit_depth: analysis.effective_bit_depth,
        effective_sample_rate: analysis.effective_sample_rate,
        description: analysis.description,
        is_stale: false,
        analyzed_at,
    })
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
}
