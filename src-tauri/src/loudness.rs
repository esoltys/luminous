//! Loudness normalization (#77) — EBU R128 (BS.1770) integrated-loudness
//! persistence and background analyzer orchestration.
//!
//! DSP measurement and gain calculation live in `luminous_analysis`.
//! A low-priority background reader (`LoudnessReader`, run by
//! `crate::background_reads`) decodes each not-yet-analyzed local track once
//! (through `luminous_audio::OfflineDecoder`,
//! the same decode path as the band_waveform/waveform analyzers) and writes
//! `songs.ebur128_integrated_loudness_lufs`. Playback consults, in order:
//! measured R128 loudness -> ReplayGain tag (track or album, per settings) ->
//! a fixed fallback gain. The resulting linear multiplier is written to
//! `AudioEngine::set_loudness_gain`, the dormant slot already wired into the
//! DSP chain by #47.

use crate::db::Database;
use crate::models::{local_song_where, LoudnessMode, LoudnessSettings};
use anyhow::{Context, Result};
pub use luminous_analysis::{
    compute_gain, GainResult, ANALYSIS_FAILED_SENTINEL, PLAUSIBLE_LUFS_RANGE,
};
use rusqlite::{params, OptionalExtension};
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// Settings persistence
// ---------------------------------------------------------------------------

pub fn get_settings(db: &Database) -> Result<LoudnessSettings> {
    let conn = db.pool.get().context("failed to get db connection")?;
    settings_on(&conn)
}

fn settings_on(conn: &rusqlite::Connection) -> Result<LoudnessSettings> {
    let (enabled, target_lufs, mode_str, fallback_gain_db) = conn.query_row(
        "SELECT enabled, target_lufs, mode, fallback_gain_db FROM loudness_settings WHERE id = 1",
        [],
        |row| {
            Ok((
                row.get::<_, i32>(0)? != 0,
                row.get::<_, f64>(1)? as f32,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)? as f32,
            ))
        },
    )?;
    Ok(LoudnessSettings {
        enabled,
        target_lufs,
        mode: LoudnessMode::from(mode_str.as_str()),
        fallback_gain_db,
    }
    .clamped())
}

pub fn save_settings(db: &Database, settings: &LoudnessSettings) -> Result<()> {
    let conn = db.pool.get().context("failed to get db connection")?;
    conn.execute(
        "UPDATE loudness_settings
         SET enabled = ?1, target_lufs = ?2, mode = ?3, fallback_gain_db = ?4
         WHERE id = 1",
        params![
            if settings.enabled { 1 } else { 0 },
            settings.target_lufs as f64,
            settings.mode.as_str(),
            settings.fallback_gain_db as f64,
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// EBU R128 / BS.1770 analysis delegation
// ---------------------------------------------------------------------------

/// Analyze a track's integrated loudness (LUFS) per ITU-R BS.1770-4.
pub fn analyze_integrated_loudness(path: &Path) -> Result<f64> {
    luminous_analysis::analyze_integrated_loudness_file(path).map_err(anyhow::Error::msg)
}

// ---------------------------------------------------------------------------
// Background analysis worker
// ---------------------------------------------------------------------------

/// Background R128 analysis, scheduled by `crate::background_reads`. Picks up
/// local/collection tracks lacking a loudness measurement, most-recently-added
/// first (a simple approximation of "newly scanned tracks first, then
/// backfill"). A track that fails to decode gets `ANALYSIS_FAILED_SENTINEL`,
/// so it isn't picked again.
pub struct LoudnessReader {
    app: AppHandle,
    db: Arc<Database>,
    analyzed: u64,
}

impl LoudnessReader {
    pub fn new(app: AppHandle, db: Arc<Database>) -> Self {
        Self {
            app,
            db,
            analyzed: 0,
        }
    }
}

impl crate::background_reads::BackgroundRead for LoudnessReader {
    fn name(&self) -> &'static str {
        "loudness"
    }

    fn next(
        &mut self,
        conn: &rusqlite::Connection,
        skipped: &crate::background_reads::Skipped,
    ) -> rusqlite::Result<Option<(i64, String)>> {
        // Only grind through the backlog while the user has actually opted
        // into loudness normalization — otherwise this would decode the
        // entire library (slow and disk-heavy on external drives) for a
        // feature that's off by default and never gets used.
        let enabled = settings_on(conn).map(|s| s.enabled).unwrap_or(false);
        if !enabled {
            return Ok(None);
        }
        conn.query_row(
            &format!(
                "SELECT id, path FROM songs
                 WHERE {local_where} AND path IS NOT NULL
                   AND ebur128_integrated_loudness_lufs IS NULL AND {skip}
                 ORDER BY added DESC, id DESC LIMIT 1",
                local_where = local_song_where(""),
                skip = skipped.sql_excluding("id")
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
    }

    fn read(&mut self, song_id: i64, path: &Path) -> crate::background_reads::ReadOutcome {
        let lufs = match analyze_integrated_loudness(path) {
            Ok(lufs) => lufs,
            Err(e) => {
                log::warn!(
                    "R128 analysis failed for song {song_id} ({}): {e}",
                    path.display()
                );
                ANALYSIS_FAILED_SENTINEL
            }
        };

        // A failed decode stores the sentinel, so only a failed write leaves
        // the song unmeasured.
        let stored = self.db.pool.get().ok().and_then(|conn| {
            conn.execute(
                "UPDATE songs SET ebur128_integrated_loudness_lufs = ?1 WHERE id = ?2",
                params![lufs, song_id],
            )
            .ok()?;
            conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM songs
                     WHERE {local_where} AND path IS NOT NULL
                       AND ebur128_integrated_loudness_lufs IS NULL",
                    local_where = local_song_where("")
                ),
                [],
                |row| row.get::<_, i64>(0),
            )
            .ok()
            .or(Some(0))
        });
        let Some(remaining) = stored else {
            log::warn!("Couldn't store the R128 loudness of song {song_id}");
            return crate::background_reads::ReadOutcome::NothingStored;
        };

        self.analyzed += 1;
        let _ = self.app.emit(
            "loudness-analysis-progress",
            crate::models::LoudnessAnalysisProgress {
                analyzed: self.analyzed,
                remaining: remaining.max(0) as u64,
            },
        );
        crate::background_reads::ReadOutcome::Stored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fallback gain saved under the old -24 dB floor loads inside the
    /// range the UI can show and set (#1249).
    #[test]
    fn out_of_range_settings_row_is_clamped_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        db.pool
            .get()
            .unwrap()
            .execute(
                "UPDATE loudness_settings SET target_lufs = -30.0, fallback_gain_db = -20.0 WHERE id = 1",
                [],
            )
            .unwrap();
        let s = get_settings(&db).unwrap();
        assert_eq!(s.target_lufs, crate::models::TARGET_LUFS_RANGE.min);
        assert_eq!(
            s.fallback_gain_db,
            crate::models::FALLBACK_GAIN_DB_RANGE.min
        );
    }
}
