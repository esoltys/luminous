use crate::analyzer::decode_all_samples;
use crate::db::Database;
use crate::moodbar::compute_moodbar_data;
use anyhow::{Context, Result};
use rusqlite::params;
use std::path::Path;

/// Compute peak waveform amplitudes (0..255) for a sample buffer, normalized relative to overall peak.
pub fn compute_waveform_peaks(samples: &[f32], points: usize) -> Vec<u8> {
    if samples.is_empty() {
        return vec![0; points];
    }

    let overall_max_peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    let chunk_size = (samples.len() as f64 / points as f64).max(1.0);
    let mut data = Vec::with_capacity(points);

    for i in 0..points {
        let start = (i as f64 * chunk_size) as usize;
        let end = (((i + 1) as f64 * chunk_size) as usize).min(samples.len());
        if start >= samples.len() {
            data.push(0);
            continue;
        }

        let chunk = &samples[start..end];
        let mut max_val = 0.0f32;
        for &s in chunk {
            let abs_s = s.abs();
            if abs_s > max_val {
                max_val = abs_s;
            }
        }

        let normalized = if overall_max_peak > 1e-6 {
            max_val / overall_max_peak
        } else {
            0.0
        };

        let val = (normalized * 255.0).clamp(0.0, 255.0) as u8;
        data.push(val);
    }
    data
}

use parking_lot::Mutex;

static VISUALIZER_GEN_LOCK: Mutex<()> = Mutex::new(());

/// Decodes audio file once and generates/caches both waveform and moodbar data.
pub fn generate_visualizer_data(
    db: &Database,
    song_id: i64,
    path: &Path,
) -> Result<(Vec<u8>, Vec<u8>)> {
    if let (Ok(Some(w)), Ok(Some(m))) = (
        get_cached_waveform(db, song_id),
        crate::moodbar::get_cached_moodbar(db, song_id),
    ) {
        return Ok((w, m));
    }

    let _guard = VISUALIZER_GEN_LOCK.lock();

    if let (Ok(Some(w)), Ok(Some(m))) = (
        get_cached_waveform(db, song_id),
        crate::moodbar::get_cached_moodbar(db, song_id),
    ) {
        return Ok((w, m));
    }

    let (samples, sample_rate) = decode_all_samples(path).with_context(|| {
        format!(
            "failed to decode samples for song_id {} at path {:?}",
            song_id, path
        )
    })?;

    let points = 150;
    let waveform_peaks = compute_waveform_peaks(&samples, points);
    let moodbar_rgb = compute_moodbar_data(&samples, sample_rate, points);

    let mut conn = db.pool.get()?;
    let tx = conn.transaction()?;

    tx.execute(
        "INSERT OR REPLACE INTO waveforms (song_id, data) VALUES (?1, ?2)",
        params![song_id, waveform_peaks],
    )?;

    tx.execute(
        "INSERT OR REPLACE INTO moodbars (song_id, data, style) VALUES (?1, ?2, 0)",
        params![song_id, moodbar_rgb],
    )?;

    tx.commit()?;

    Ok((waveform_peaks, moodbar_rgb))
}

/// Generate peak waveform amplitudes for a song, save to SQLite, and return them.
pub fn generate_waveform(db: &Database, song_id: i64, path: &Path) -> Result<Vec<u8>> {
    match generate_visualizer_data(db, song_id, path) {
        Ok((waveform, _)) => Ok(waveform),
        Err(e) => {
            log::error!(
                "Failed to generate waveform for song_id {} at {:?}: {:?}",
                song_id,
                path,
                e
            );
            Err(e)
        }
    }
}

/// Retrieve the cached waveform data from the database.
pub fn get_cached_waveform(db: &Database, song_id: i64) -> Result<Option<Vec<u8>>> {
    let conn = db.pool.get()?;
    let data: Option<Vec<u8>> = conn
        .query_row(
            "SELECT data FROM waveforms WHERE song_id = ?1",
            params![song_id],
            |row| row.get(0),
        )
        .ok();
    Ok(data)
}

/// Queries database for songs missing either waveform or moodbar cache.
pub fn get_missing_visualizer_songs(db: &Database) -> Result<Vec<(i64, String)>> {
    let conn = db.pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.path FROM songs s
         LEFT JOIN waveforms w ON s.id = w.song_id
         LEFT JOIN moodbars m ON s.id = m.song_id
         WHERE s.unavailable = 0 AND s.path IS NOT NULL AND (w.song_id IS NULL OR m.song_id IS NULL)",
    )?;

    let missing_songs: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(missing_songs)
}

/// Scans database for songs missing visualizer cache and generates them, invoking a progress callback for each song.
pub fn backfill_missing_visualizers_with_progress<F>(
    db: &Database,
    mut progress_cb: F,
) -> Result<usize>
where
    F: FnMut(usize, usize, &str),
{
    let missing_songs = get_missing_visualizer_songs(db)?;
    if missing_songs.is_empty() {
        return Ok(0);
    }

    let total = missing_songs.len();
    log::info!("Backfilling missing visualizers for {total} songs");
    let mut count = 0;

    for (idx, (song_id, path_str)) in missing_songs.into_iter().enumerate() {
        let path = Path::new(&path_str);
        let file_name = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| path_str.clone());

        progress_cb(idx + 1, total, &file_name);

        if !path.exists() {
            continue;
        }

        if let Err(e) = generate_visualizer_data(db, song_id, path) {
            log::warn!(
                "Failed backfilling visualizer data for song {} ({}): {:?}",
                song_id,
                path_str,
                e
            );
        } else {
            count += 1;
        }
    }

    log::info!("Completed visualizer backfill for {count} songs");
    Ok(count)
}

/// Scans database for songs that are missing either waveform or moodbar cache and generates them.
pub fn backfill_missing_visualizers(db: &Database) -> Result<usize> {
    backfill_missing_visualizers_with_progress(db, |_, _, _| {})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_waveform_peaks_empty() {
        let peaks = compute_waveform_peaks(&[], 150);
        assert_eq!(peaks.len(), 150);
        assert!(peaks.iter().all(|&v| v == 0));
    }

    #[test]
    fn test_compute_waveform_peaks_peak_normalization() {
        // Quiet signal with max amplitude 0.05
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32 / 1000.0) * 0.05).collect();

        let peaks = compute_waveform_peaks(&samples, 10);
        assert_eq!(peaks.len(), 10);
        // The last chunk should reach peak normalized to ~255 despite max sample being 0.05
        let max_peak = *peaks.iter().max().unwrap();
        assert!(
            max_peak >= 250,
            "Max peak should be normalized near 255, got {}",
            max_peak
        );
        // First chunk should be near 0
        assert!(peaks[0] < 50);
    }

    #[test]
    fn test_backfill_missing_visualizers_empty_db() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        let backfilled = backfill_missing_visualizers(&db).unwrap();
        assert_eq!(backfilled, 0);
    }
}
