use crate::db::Database;
use crate::waveform::generate_visualizer_data;
use anyhow::Result;
use rusqlite::params;
use rustfft::{num_complex::Complex, FftPlanner};
use std::path::Path;

/// Compute 150 spectral colors (RGB sequence) representing song mood from decoded samples.
pub fn compute_moodbar_data(samples: &[f32], sample_rate: u32, points: usize) -> Vec<u8> {
    if samples.is_empty() {
        return vec![0; points * 3];
    }

    let block_size = samples.len() / points;
    if block_size < 16 {
        return vec![0; points * 3];
    }

    let fft_size = (block_size.next_power_of_two() >> 1).clamp(64, 1024);
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    let mut bands = Vec::with_capacity(points);
    for i in 0..points {
        let start = i * block_size;
        let end = start + fft_size;
        if end > samples.len() {
            bands.push((0.0, 0.0, 0.0));
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

        for (k, sample) in buffer.iter().enumerate().skip(1).take(fft_size / 2 - 1) {
            let freq = (k as f32 * sample_rate as f32) / fft_size as f32;
            let magnitude = (sample.re * sample.re + sample.im * sample.im).sqrt();

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

        bands.push((r, g, b));
    }

    let channel_range = |sel: fn(&(f32, f32, f32)) -> f32| -> (f32, f32) {
        let min = bands.iter().map(&sel).fold(f32::INFINITY, f32::min);
        let max = bands.iter().map(&sel).fold(f32::NEG_INFINITY, f32::max);
        (min, max)
    };
    let (r_min, r_max) = channel_range(|b| b.0);
    let (g_min, g_max) = channel_range(|b| b.1);
    let (b_min, b_max) = channel_range(|b| b.2);

    let stretch = |v: f32, min: f32, max: f32| -> u8 {
        let range = max - min;
        if range < f32::EPSILON {
            0
        } else {
            (((v - min) / range) * 255.0).clamp(0.0, 255.0) as u8
        }
    };

    let mut data = Vec::with_capacity(points * 3);
    for (r, g, b) in bands {
        data.push(stretch(r, r_min, r_max));
        data.push(stretch(g, g_min, g_max));
        data.push(stretch(b, b_min, b_max));
    }
    data
}

/// Generate 150 spectral colors (RGB sequence) representing the song mood, save to SQLite, and return.
pub fn generate_moodbar(db: &Database, song_id: i64, path: &Path) -> Result<Vec<u8>> {
    match generate_visualizer_data(db, song_id, path) {
        Ok((_, moodbar)) => Ok(moodbar),
        Err(e) => {
            log::error!(
                "Failed to generate moodbar for song_id {} at {:?}: {:?}",
                song_id,
                path,
                e
            );
            Err(e)
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_moodbar_data_empty() {
        let moodbar = compute_moodbar_data(&[], 44100, 150);
        assert_eq!(moodbar.len(), 450);
        assert!(moodbar.iter().all(|&v| v == 0));
    }
}
