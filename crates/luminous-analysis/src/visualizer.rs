//! Waveform peak envelope and 3-band FFT visualizer calculation.

use rustfft::{num_complex::Complex, FftPlanner};

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

/// Format version of the data produced by `compute_band_waveform_data`, stored
/// alongside each cached blob in `band_waveforms.style`. Bump this whenever the
/// byte layout or semantics of the output changes so stale cached blobs
/// (written by an older build) are treated as a cache miss and regenerated,
/// rather than being handed to the frontend under a new interpretation (or
/// point count) they were never computed for.
///
/// Version 1: three independent per-band amplitude layers (low/mid/high), each
/// channel-normalized 0-255, rendered by the frontend as a fixed blue/amber/white
/// layered waveform (#217), at `WAVEFORM_POINTS` (150) resolution. Version 0
/// (legacy) was the same byte layout but rendered/interpreted as a single
/// blended mood color, under the table's original name (`moodbars`).
///
/// Version 2: same layered format, raised to 300-point resolution for finer
/// transient (hi-hat/percussion) detail.
///
/// Version 3: raised to `BAND_WAVEFORM_POINTS` (512) resolution — the seek
/// bar's rendered width is capped by PlayerBar's layout (max-w-[1200px]
/// footer, 1/3 column) at ~285 CSS px regardless of window/monitor size, so
/// 512 points covers up to ~200% OS display scaling (dpr 2, ~570 physical px)
/// before additional resolution stops being visually distinguishable.
pub const BAND_WAVEFORM_STYLE_VERSION: i64 = 3;

/// Point resolution for the cached layered low/mid/high band waveform. Higher
/// than `WAVEFORM_POINTS` — transient detail benefits from finer time
/// resolution than the waveform's coarser peak envelope needs.
pub const BAND_WAVEFORM_POINTS: usize = 512;

/// Combined visualizer outputs computed from a single audio decode pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualizerAnalysis {
    /// 150-point RMS peak envelope (0..255).
    pub waveform_peaks: Vec<u8>,
    /// 512-point layered low/mid/high frequency band amplitudes (byte triples).
    pub band_waveform_data: Vec<u8>,
}

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

/// Compute per-point low/mid/high band amplitudes (RGB-flavored byte triples)
/// representing the song's frequency-band structure from decoded samples.
pub fn compute_band_waveform_data(samples: &[f32], sample_rate: u32, points: usize) -> Vec<u8> {
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

/// Decode a whole file and downmix it to mono by averaging every channel.
/// Returns `(samples, sample_rate)`. Only the mono result grows with the
/// track's length; each packet's channels are folded in as it decodes.
pub fn decode_mono(decoder: luminous_audio::OfflineDecoder) -> (Vec<f32>, u32) {
    let sample_rate = decoder.sample_rate();
    let mut mono = Vec::new();
    decoder.decode_blocks(usize::MAX, |planes| match planes {
        [] => {}
        [only] => mono.extend_from_slice(only),
        [first, rest @ ..] => {
            let start = mono.len();
            mono.extend_from_slice(first);
            for plane in rest {
                for (out, &s) in mono[start..].iter_mut().zip(plane) {
                    *out += s;
                }
            }
            let count = planes.len() as f32;
            for out in &mut mono[start..] {
                *out /= count;
            }
        }
    });
    (mono, sample_rate)
}

/// Decodes audio file once and generates both waveform and band waveform data in one pass.
pub fn analyze_visualizer(decoder: luminous_audio::OfflineDecoder) -> VisualizerAnalysis {
    let (samples, sample_rate) = decode_mono(decoder);
    let waveform_peaks = compute_waveform_peaks(&samples, WAVEFORM_POINTS);
    let band_waveform_data =
        compute_band_waveform_data(&samples, sample_rate, BAND_WAVEFORM_POINTS);
    VisualizerAnalysis {
        waveform_peaks,
        band_waveform_data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_mono_averages_channels() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("three.wav");
        let left: Vec<i16> = (0..4_000).map(|i| (i % 2_000) as i16 * 8).collect();
        let centre: Vec<i16> = (0..4_000).map(|i| -((i % 1_000) as i16) * 16).collect();
        let right: Vec<i16> = vec![12_000; 4_000];
        luminous_audio::write_test_pcm16_wav(
            &path,
            8_000,
            &[left.clone(), centre.clone(), right.clone()],
        );

        let (mono, sample_rate) =
            decode_mono(luminous_audio::OfflineDecoder::open_file(&path).expect("open"));

        assert_eq!(sample_rate, 8_000);
        assert_eq!(mono.len(), 4_000);
        for (i, &sample) in mono.iter().enumerate() {
            let expected = (left[i] as f32 + centre[i] as f32 + right[i] as f32) / 32_768.0 / 3.0;
            assert!(
                (sample - expected).abs() < 1e-6,
                "frame {i}: {sample} != {expected}"
            );
        }
    }

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
    fn test_compute_band_waveform_data_empty() {
        let band_waveform = compute_band_waveform_data(&[], 44100, 150);
        assert_eq!(band_waveform.len(), 450);
        assert!(band_waveform.iter().all(|&v| v == 0));
    }

    #[test]
    fn test_analyze_visualizer_generates_both_outputs() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.wav");
        let samples: Vec<i16> = (0..8000)
            .map(|i| {
                let t = i as f32 / 8000.0;
                (0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 32_767.0) as i16
            })
            .collect();
        luminous_audio::write_test_pcm16_wav(&path, 8000, &[samples]);

        let decoder = luminous_audio::OfflineDecoder::open_file(&path).expect("open");
        let analysis = analyze_visualizer(decoder);

        assert_eq!(analysis.waveform_peaks.len(), WAVEFORM_POINTS);
        assert_eq!(analysis.band_waveform_data.len(), BAND_WAVEFORM_POINTS * 3);
    }
}
