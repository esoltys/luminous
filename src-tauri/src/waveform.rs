use crate::analyzer::decode_all_samples;
use crate::band_waveform::compute_band_waveform_data;
use crate::db::Database;
use anyhow::{Context, Result};
use rusqlite::params;
use std::path::Path;

/// Point resolution for the cached waveform peak envelope.
pub const WAVEFORM_POINTS: usize = 150;

/// Format version of the data produced by `compute_waveform_peaks`, stored
/// alongside each cached blob in `waveforms.style`. Bump whenever the
/// computation changes so stale cached blobs (written by an older build) are
/// regenerated rather than displayed under the new interpretation.
///
/// Version 1: RMS energy per chunk (was: max abs sample per chunk, which read
/// as a flat, uniformly-tall rectangle for loudness-maximized masters instead
/// of a shaped envelope).
pub const WAVEFORM_STYLE_VERSION: i64 = 1;

/// Compute waveform amplitude envelope (0..255) for a sample buffer, normalized
/// relative to the loudest chunk.
///
/// Uses RMS (root-mean-square) energy per chunk rather than each chunk's max
/// sample. Modern loudness-maximized masters keep their true peak sitting
/// near the ceiling almost everywhere except actual silence, so a per-chunk
/// max-peak envelope reads as a flat, uniformly-tall rectangle instead of a
/// shaped contour — RMS reflects the track's perceived loudness envelope
/// instead (the same approach used by Spotify/SoundCloud-style waveforms).
pub fn compute_waveform_peaks(samples: &[f32], points: usize) -> Vec<u8> {
    if samples.is_empty() {
        return vec![0; points];
    }

    let chunk_size = (samples.len() as f64 / points as f64).max(1.0);
    let mut rms_per_chunk = Vec::with_capacity(points);

    for i in 0..points {
        let start = (i as f64 * chunk_size) as usize;
        let end = (((i + 1) as f64 * chunk_size) as usize).min(samples.len());
        if start >= samples.len() {
            rms_per_chunk.push(0.0f32);
            continue;
        }

        let chunk = &samples[start..end];
        let sum_sq: f32 = chunk.iter().map(|&s| s * s).sum();
        rms_per_chunk.push((sum_sq / chunk.len() as f32).sqrt());
    }

    let overall_max_rms = rms_per_chunk.iter().cloned().fold(0.0f32, f32::max);

    rms_per_chunk
        .into_iter()
        .map(|rms| {
            let normalized = if overall_max_rms > 1e-6 {
                rms / overall_max_rms
            } else {
                0.0
            };
            (normalized * 255.0).clamp(0.0, 255.0) as u8
        })
        .collect()
}

use parking_lot::Mutex;

static VISUALIZER_GEN_LOCK: Mutex<()> = Mutex::new(());

/// Decodes audio file once and generates/caches both waveform and band waveform data.
pub fn generate_visualizer_data(
    db: &Database,
    song_id: i64,
    path: &Path,
) -> Result<(Vec<u8>, Vec<u8>)> {
    if let (Ok(Some(w)), Ok(Some(m))) = (
        get_cached_waveform(db, song_id),
        crate::band_waveform::get_cached_band_waveform(db, song_id),
    ) {
        return Ok((w, m));
    }

    let _guard = VISUALIZER_GEN_LOCK.lock();

    if let (Ok(Some(w)), Ok(Some(m))) = (
        get_cached_waveform(db, song_id),
        crate::band_waveform::get_cached_band_waveform(db, song_id),
    ) {
        return Ok((w, m));
    }

    let (samples, sample_rate) = decode_all_samples(path).with_context(|| {
        format!(
            "failed to decode samples for song_id {} at path {:?}",
            song_id, path
        )
    })?;

    let waveform_peaks = compute_waveform_peaks(&samples, WAVEFORM_POINTS);
    // Higher point resolution than the waveform: transient (hi-hat/percussion)
    // detail in the layered band waveform benefits from finer time resolution
    // than the waveform's coarser peak envelope needs (#217).
    let band_waveform_data = compute_band_waveform_data(
        &samples,
        sample_rate,
        crate::band_waveform::BAND_WAVEFORM_POINTS,
    );

    let mut conn = db.pool.get()?;
    let tx = conn.transaction()?;

    tx.execute(
        "INSERT OR REPLACE INTO waveforms (song_id, data, style) VALUES (?1, ?2, ?3)",
        params![song_id, waveform_peaks, WAVEFORM_STYLE_VERSION],
    )?;

    tx.execute(
        "INSERT OR REPLACE INTO band_waveforms (song_id, data, style) VALUES (?1, ?2, ?3)",
        params![
            song_id,
            band_waveform_data,
            crate::band_waveform::BAND_WAVEFORM_STYLE_VERSION
        ],
    )?;

    tx.commit()?;

    Ok((waveform_peaks, band_waveform_data))
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

/// Retrieve the cached waveform data from the database, if it was written by
/// the current `WAVEFORM_STYLE_VERSION`. A blob cached under an older style is
/// treated as a cache miss so callers regenerate it rather than misinterpret it.
pub fn get_cached_waveform(db: &Database, song_id: i64) -> Result<Option<Vec<u8>>> {
    let conn = db.pool.get()?;
    let row: Option<(Vec<u8>, i64)> = conn
        .query_row(
            "SELECT data, style FROM waveforms WHERE song_id = ?1",
            params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    Ok(row.and_then(|(data, style)| {
        if style == WAVEFORM_STYLE_VERSION {
            Some(data)
        } else {
            None
        }
    }))
}

/// Queries database for songs missing either waveform or band waveform cache, or whose
/// cached waveform/band waveform was written by an older style version and needs
/// regenerating under the current format.
pub fn get_missing_visualizer_songs(db: &Database) -> Result<Vec<(i64, String)>> {
    let conn = db.pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.path FROM songs s
         LEFT JOIN waveforms w ON s.id = w.song_id
         LEFT JOIN band_waveforms m ON s.id = m.song_id
         WHERE s.unavailable = 0 AND s.path IS NOT NULL
           AND (w.song_id IS NULL OR m.song_id IS NULL OR w.style != ?1 OR m.style != ?2)",
    )?;

    let missing_songs: Vec<(i64, String)> = stmt
        .query_map(
            params![
                WAVEFORM_STYLE_VERSION,
                crate::band_waveform::BAND_WAVEFORM_STYLE_VERSION
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
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

/// Scans database for songs that are missing either waveform or band waveform cache and generates them.
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
    fn test_compute_waveform_peaks_rms_reflects_loudness_not_just_peak() {
        // A sparse single loud sample in an otherwise silent chunk (as in a
        // transient click) vs. a chunk that's consistently near that same
        // peak amplitude throughout (a brickwall-limited/loud passage). A
        // peak-only envelope would render both chunks at the same height;
        // RMS should show the sparse-transient chunk as much quieter.
        let mut samples = vec![0.0f32; 500];
        samples[250] = 0.9;
        samples.extend(vec![0.9f32; 500]);

        let peaks = compute_waveform_peaks(&samples, 2);
        assert_eq!(peaks.len(), 2);
        assert!(
            peaks[1] > peaks[0] * 3,
            "sustained-loud chunk ({}) should read far louder than a single transient click in silence ({})",
            peaks[1],
            peaks[0]
        );
    }

    #[test]
    fn test_stale_waveform_style_is_treated_as_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, unavailable) VALUES ('/tmp/song.mp3', 0)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        // Cached under a stale style (pre-RMS-fix peak-based format).
        conn.execute(
            "INSERT INTO waveforms (song_id, data, style) VALUES (?1, ?2, 0)",
            params![song_id, vec![0u8; 150]],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO band_waveforms (song_id, data, style) VALUES (?1, ?2, ?3)",
            params![
                song_id,
                vec![0u8; crate::band_waveform::BAND_WAVEFORM_POINTS * 3],
                crate::band_waveform::BAND_WAVEFORM_STYLE_VERSION
            ],
        )
        .unwrap();
        drop(conn);

        assert_eq!(get_cached_waveform(&db, song_id).unwrap(), None);

        let missing = get_missing_visualizer_songs(&db).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].0, song_id);
    }

    #[test]
    fn test_backfill_missing_visualizers_empty_db() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        let backfilled = backfill_missing_visualizers(&db).unwrap();
        assert_eq!(backfilled, 0);
    }

    #[test]
    fn test_stale_band_waveform_style_is_treated_as_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, unavailable) VALUES ('/tmp/song.mp3', 0)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO waveforms (song_id, data) VALUES (?1, ?2)",
            params![song_id, vec![0u8; 150]],
        )
        .unwrap();
        // Cached under a stale style (pre-#217 blended-color format).
        conn.execute(
            "INSERT INTO band_waveforms (song_id, data, style) VALUES (?1, ?2, 0)",
            params![song_id, vec![0u8; 450]],
        )
        .unwrap();
        drop(conn);

        assert_eq!(
            crate::band_waveform::get_cached_band_waveform(&db, song_id).unwrap(),
            None
        );

        let missing = get_missing_visualizer_songs(&db).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].0, song_id);
    }
}
