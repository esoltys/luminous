use crate::analyzer::decode_all_samples;
use crate::db::Database;
use anyhow::Result;
use rusqlite::params;
use rustfft::{num_complex::Complex, FftPlanner};
use std::path::Path;

/// Generate 150 spectral colors (RGB sequence) representing the song mood, save to SQLite, and return.
pub fn generate_moodbar(db: &Database, song_id: i64, path: &Path) -> Result<Vec<u8>> {
    let (samples, sample_rate) = decode_all_samples(path)?;

    let points = 150;
    let moodbar_data = if samples.is_empty() {
        vec![0; points * 3]
    } else {
        let block_size = samples.len() / points;
        if block_size < 16 {
            vec![0; points * 3]
        } else {
            let fft_size = (block_size.next_power_of_two() >> 1).clamp(64, 1024);
            let mut planner = FftPlanner::new();
            let fft = planner.plan_fft_forward(fft_size);

            let mut data = Vec::with_capacity(points * 3);
            for i in 0..points {
                let start = i * block_size;
                let end = start + fft_size;
                if end > samples.len() {
                    data.extend_from_slice(&[0, 0, 0]);
                    continue;
                }

                let chunk = &samples[start..end];
                let mut buffer: Vec<Complex<f32>> =
                    chunk.iter().map(|&s| Complex { re: s, im: 0.0 }).collect();

                while buffer.len() < fft_size {
                    buffer.push(Complex { re: 0.0, im: 0.0 });
                }

                fft.process(&mut buffer);

                let mut bass_sum = 0.0;
                let mut mid_sum = 0.0;
                let mut treble_sum = 0.0;

                let mut bass_count = 0;
                let mut mid_count = 0;
                let mut treble_count = 0;

                for k in 1..(fft_size / 2) {
                    let freq = (k as f32 * sample_rate as f32) / fft_size as f32;
                    let magnitude =
                        (buffer[k].re * buffer[k].re + buffer[k].im * buffer[k].im).sqrt();

                    if freq < 250.0 {
                        bass_sum += magnitude;
                        bass_count += 1;
                    } else if freq < 2000.0 {
                        mid_sum += magnitude;
                        mid_count += 1;
                    } else {
                        treble_sum += magnitude;
                        treble_count += 1;
                    }
                }

                let r = if bass_count > 0 {
                    bass_sum / bass_count as f32
                } else {
                    0.0
                };
                let g = if mid_count > 0 {
                    mid_sum / mid_count as f32
                } else {
                    0.0
                };
                let b = if treble_count > 0 {
                    treble_sum / treble_count as f32
                } else {
                    0.0
                };

                // Scale spectral energy to RGB values with log offset boosting
                let r_u8 = ((r * 150.0).clamp(0.0, 255.0)) as u8;
                let g_u8 = ((g * 150.0).clamp(0.0, 255.0)) as u8;
                let b_u8 = ((b * 150.0).clamp(0.0, 255.0)) as u8;

                data.push(r_u8);
                data.push(g_u8);
                data.push(b_u8);
            }
            data
        }
    };

    let conn = db.pool.get()?;
    conn.execute(
        "INSERT OR REPLACE INTO moodbars (song_id, data, style) VALUES (?1, ?2, 0)",
        params![song_id, moodbar_data],
    )?;

    Ok(moodbar_data)
}

/// Retrieve the cached moodbar RGB data from the database.
pub fn get_cached_moodbar(db: &Database, song_id: i64) -> Result<Option<Vec<u8>>> {
    let conn = db.pool.get()?;
    let data: Option<Vec<u8>> = conn
        .query_row(
            "SELECT data FROM moodbars WHERE song_id = ?1",
            params![song_id],
            |row| row.get(0),
        )
        .ok();
    Ok(data)
}
