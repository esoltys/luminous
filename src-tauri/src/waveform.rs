use crate::analyzer::decode_all_samples;
use crate::db::Database;
use anyhow::Result;
use rusqlite::params;
use std::path::Path;

/// Generate peak waveform amplitudes for a song, save to SQLite, and return them.
pub fn generate_waveform(db: &Database, song_id: i64, path: &Path) -> Result<Vec<u8>> {
    let (samples, _) = decode_all_samples(path)?;

    let points = 150;
    let peaks = if samples.is_empty() {
        vec![0; points]
    } else {
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

            let val = (max_val * 255.0).clamp(0.0, 255.0) as u8;
            data.push(val);
        }
        data
    };

    let conn = db.pool.get()?;
    conn.execute(
        "INSERT OR REPLACE INTO waveforms (song_id, data) VALUES (?1, ?2)",
        params![song_id, peaks],
    )?;

    Ok(peaks)
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
