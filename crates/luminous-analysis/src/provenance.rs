//! Lossless provenance verification & fake FLAC detection (#1373, epic #1441).
//!
//! Evaluates whether an audio track claiming to be lossless is authentic or
//! exhibits signatures of:
//! 1. A lossy transcode (sharp spectral cutoff shelf characteristic of MP3/AAC/Vorbis).
//! 2. An upsampled standard-rate file (high sample rate >48 kHz with zero ultrasonic content).
//! 3. Padded bit depth (24-bit container containing zero-padded 16-bit or 8-bit samples).
//!
//! Must never depend on Tauri, SQLite, reqwest, or Tokio (#1765, epic #1762).

use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Advisory provenance classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceVerdict {
    /// Authentic lossless audio (CD quality or standard sample rate).
    Authentic,
    /// Authentic high-resolution lossless audio with ultrasonic harmonic content.
    AuthenticHiRes,
    /// Suspected lossy transcode with a detected low-pass cutoff shelf.
    SuspectedLossy,
    /// Suspected upsampled audio (e.g. 44.1 kHz source upsampled into a >48 kHz container).
    SuspectedUpsampled,
    /// Padded bit depth (e.g. 16-bit audio in a 24-bit container with lower bits zeroed).
    PaddedBitDepth,
}

/// Analysis results summarizing the track's spectral and bit-depth authenticity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceAnalysis {
    pub verdict: ProvenanceVerdict,
    /// Detected spectral cutoff frequency in Hz, if identifiable.
    pub cutoff_hz: Option<u32>,
    /// Effective bit depth determined from PCM sample quantization.
    pub effective_bit_depth: Option<u32>,
    /// Effective original sample rate in Hz (e.g. 44100 if upsampled into 96000).
    pub effective_sample_rate: Option<u32>,
    /// Brief description explaining the verdict and spectral observations.
    pub description: String,
}

/// Number of samples per FFT window.
const FFT_SIZE: usize = 2048;

/// Minimum non-silent window RMS required for spectral evaluation.
const MIN_WINDOW_RMS: f32 = 0.005;

/// Analyze the audio provenance of a track using `OfflineDecoder`.
pub fn analyze_provenance(
    decoder: luminous_audio::OfflineDecoder,
) -> Result<ProvenanceAnalysis, String> {
    let sample_rate = decoder.sample_rate();
    let reported_bits = decoder.bits_per_sample();

    // Skip up to 3 seconds of leading pre-gap/silence if track allows, then accumulate up to 35 seconds of audio
    let skip_samples = (sample_rate as f32 * 3.0) as usize;
    let target_samples = (sample_rate as f32 * 35.0) as usize;
    let max_read_samples = (sample_rate as f32 * 75.0) as usize;

    let mut mono_samples: Vec<f32> = Vec::with_capacity(target_samples.min(131_072));
    let mut total_scanned = 0;

    decoder.try_decode_blocks(2, |planes| {
        match planes {
            [] => {}
            [only] => {
                for &s in only.iter() {
                    total_scanned += 1;
                    if total_scanned > skip_samples && mono_samples.len() < target_samples {
                        mono_samples.push(s);
                    }
                }
            }
            [first, second, ..] => {
                let len = first.len().min(second.len());
                for i in 0..len {
                    total_scanned += 1;
                    if total_scanned > skip_samples && mono_samples.len() < target_samples {
                        mono_samples.push((first[i] + second[i]) * 0.5);
                    }
                }
            }
        }
        if mono_samples.len() >= target_samples || total_scanned >= max_read_samples {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    })?;

    if mono_samples.is_empty() {
        return Err("No audio decoded from track".to_string());
    }

    Ok(analyze_provenance_from_samples(
        &mono_samples,
        sample_rate,
        reported_bits,
    ))
}

/// Evaluate provenance from in-memory mono PCM samples and stream parameters.
pub fn analyze_provenance_from_samples(
    samples: &[f32],
    sample_rate: u32,
    reported_bits: Option<u32>,
) -> ProvenanceAnalysis {
    if samples.is_empty() || sample_rate == 0 {
        return ProvenanceAnalysis {
            verdict: ProvenanceVerdict::Authentic,
            cutoff_hz: None,
            effective_bit_depth: reported_bits,
            effective_sample_rate: Some(sample_rate),
            description: "Authentic lossless audio".to_string(),
        };
    }

    // 1. Bit depth analysis
    let effective_bits = detect_effective_bit_depth(samples, reported_bits);
    let is_padded_bits = reported_bits.unwrap_or(16) >= 24 && effective_bits <= 16;

    // 2. High sample rate upsample check (> 48 kHz)
    if sample_rate > 48_000 {
        let (is_upsampled, ultrasonic_cutoff) = detect_upsampled_hires(samples, sample_rate);
        if is_upsampled {
            return ProvenanceAnalysis {
                verdict: ProvenanceVerdict::SuspectedUpsampled,
                cutoff_hz: Some(ultrasonic_cutoff),
                effective_bit_depth: Some(effective_bits),
                effective_sample_rate: Some(if ultrasonic_cutoff <= 22_500 {
                    44_100
                } else {
                    48_000
                }),
                description: format!(
                    "Suspected upsampled audio (cutoff ~{:.1} kHz in {} kHz container)",
                    ultrasonic_cutoff as f32 / 1000.0,
                    sample_rate / 1000
                ),
            };
        } else if is_padded_bits {
            return ProvenanceAnalysis {
                verdict: ProvenanceVerdict::PaddedBitDepth,
                cutoff_hz: None,
                effective_bit_depth: Some(effective_bits),
                effective_sample_rate: Some(sample_rate),
                description: format!(
                    "Padded bit depth: {}-bit precision inside {}-bit container",
                    effective_bits,
                    reported_bits.unwrap_or(24)
                ),
            };
        } else {
            return ProvenanceAnalysis {
                verdict: ProvenanceVerdict::AuthenticHiRes,
                cutoff_hz: None,
                effective_bit_depth: Some(effective_bits),
                effective_sample_rate: Some(sample_rate),
                description: format!(
                    "Authentic high-resolution lossless audio ({} kHz, {}-bit)",
                    sample_rate / 1000,
                    effective_bits
                ),
            };
        }
    }

    // 3. Standard sample rate (<= 48 kHz) spectral cutoff check
    let (spectral_verdict, cutoff_hz) = detect_lossy_cutoff(samples, sample_rate);

    match spectral_verdict {
        SpectralVerdict::LossyCutoff(cutoff) => ProvenanceAnalysis {
            verdict: ProvenanceVerdict::SuspectedLossy,
            cutoff_hz: Some(cutoff),
            effective_bit_depth: Some(effective_bits),
            effective_sample_rate: Some(sample_rate),
            description: format!(
                "Suspected lossy transcode with spectral shelf at ~{:.1} kHz",
                cutoff as f32 / 1000.0
            ),
        },
        SpectralVerdict::Authentic => {
            if is_padded_bits {
                ProvenanceAnalysis {
                    verdict: ProvenanceVerdict::PaddedBitDepth,
                    cutoff_hz,
                    effective_bit_depth: Some(effective_bits),
                    effective_sample_rate: Some(sample_rate),
                    description: format!(
                        "Padded bit depth: {}-bit precision inside {}-bit container",
                        effective_bits,
                        reported_bits.unwrap_or(24)
                    ),
                }
            } else {
                ProvenanceAnalysis {
                    verdict: ProvenanceVerdict::Authentic,
                    cutoff_hz,
                    effective_bit_depth: Some(effective_bits),
                    effective_sample_rate: Some(sample_rate),
                    description: format!(
                        "Authentic lossless audio ({}-bit, {:.1} kHz cutoff)",
                        effective_bits,
                        cutoff_hz.unwrap_or(sample_rate / 2) as f32 / 1000.0
                    ),
                }
            }
        }
    }
}

/// Detects effective bit depth by testing lower bits of non-zero samples.
fn detect_effective_bit_depth(samples: &[f32], reported_bits: Option<u32>) -> u32 {
    let target_bits = reported_bits.unwrap_or(16);
    if target_bits < 24 {
        return target_bits;
    }

    let mut non_zero_count = 0;
    let mut zero_lower_8 = 0;
    let mut zero_lower_16 = 0;

    for &s in samples {
        if s.abs() < 1e-7 {
            continue;
        }
        // Multiply by 2^23 to scale unit float to 24-bit integer
        let int_val = (s * 8_388_608.0).round() as i64;
        if int_val == 0 {
            continue;
        }
        non_zero_count += 1;
        if (int_val & 0xFF) == 0 {
            zero_lower_8 += 1;
        }
        if (int_val & 0xFFFF) == 0 {
            zero_lower_16 += 1;
        }

        if non_zero_count >= 10_000 {
            break;
        }
    }

    if non_zero_count < 100 {
        return target_bits;
    }

    if zero_lower_16 as f64 / non_zero_count as f64 > 0.995 {
        8
    } else if zero_lower_8 as f64 / non_zero_count as f64 > 0.995 {
        16
    } else {
        24
    }
}

enum SpectralVerdict {
    LossyCutoff(u32),
    Authentic,
}

struct SpectralStats {
    avg_spectrum: Vec<f32>,
    window_count: usize,
    /// Fraction of active musical windows with energy persisting above 20 kHz.
    high_freq_continuity: f32,
}

/// Computes average linear power spectrum across energetic windows and frame continuity statistics.
fn compute_average_linear_spectrum(samples: &[f32], sample_rate: u32) -> SpectralStats {
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);

    let window: Vec<f32> = (0..FFT_SIZE)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / FFT_SIZE as f32).cos()))
        .collect();

    let step = FFT_SIZE / 2;
    let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
    let hz_to_bin = |hz: f32| ((hz / bin_hz) as usize).min(FFT_SIZE / 2 - 1);
    let mid_start = hz_to_bin(2_000.0);
    let mid_end = hz_to_bin(10_000.0);
    let high_start = hz_to_bin(20_000.0);
    let high_end = hz_to_bin(21_500.0);

    // First attempt with MIN_WINDOW_RMS, fallback to 0.0001 if track is exceptionally quiet
    for &min_rms in &[MIN_WINDOW_RMS, 0.0001] {
        let mut avg_linear_spectrum = vec![0.0f32; FFT_SIZE / 2];
        let mut window_count = 0;
        let mut energetic_windows = 0;
        let mut high_energy_windows = 0;
        let mut scratch = vec![Complex::new(0.0f32, 0.0f32); FFT_SIZE];

        for chunk_start in (0..samples.len().saturating_sub(FFT_SIZE)).step_by(step) {
            let chunk = &samples[chunk_start..chunk_start + FFT_SIZE];

            let sum_sq: f32 = chunk.iter().map(|&s| s * s).sum();
            let rms = (sum_sq / FFT_SIZE as f32).sqrt();
            if rms < min_rms {
                continue;
            }

            for (i, (&s, &w)) in chunk.iter().zip(&window).enumerate() {
                scratch[i] = Complex::new(s * w, 0.0);
            }

            fft.process(&mut scratch);

            let mut mid_sum = 0.0f32;
            let mut high_sum = 0.0f32;

            for i in 0..FFT_SIZE / 2 {
                let mag_sq = scratch[i].norm_sqr() / (FFT_SIZE as f32 * FFT_SIZE as f32);
                avg_linear_spectrum[i] += mag_sq;
                if i >= mid_start && i < mid_end {
                    mid_sum += mag_sq;
                }
                if i >= high_start && i < high_end {
                    high_sum += mag_sq;
                }
            }

            let mid_power = 10.0 * (mid_sum / (mid_end - mid_start).max(1) as f32 + 1e-12).log10();
            if mid_power > -50.0 {
                energetic_windows += 1;
                if high_end > high_start {
                    let high_power =
                        10.0 * (high_sum / (high_end - high_start) as f32 + 1e-12).log10();
                    if high_power > -72.0 {
                        high_energy_windows += 1;
                    }
                }
            }
            window_count += 1;
        }

        if window_count > 0 {
            for val in &mut avg_linear_spectrum {
                *val /= window_count as f32;
            }
            let high_freq_continuity = if energetic_windows >= 8 {
                high_energy_windows as f32 / energetic_windows as f32
            } else {
                1.0
            };
            return SpectralStats {
                avg_spectrum: avg_linear_spectrum,
                window_count,
                high_freq_continuity,
            };
        }
    }

    SpectralStats {
        avg_spectrum: vec![0.0f32; FFT_SIZE / 2],
        window_count: 0,
        high_freq_continuity: 0.0,
    }
}

fn band_power_db(spectrum_linear: &[f32], start_bin: usize, end_bin: usize) -> f32 {
    let end = end_bin.min(spectrum_linear.len());
    if end <= start_bin {
        return -120.0;
    }
    let sum: f32 = spectrum_linear[start_bin..end].iter().sum();
    let avg = sum / (end - start_bin) as f32;
    10.0 * (avg + 1e-12).log10()
}

/// Detects if high sample rate (>48 kHz) material lacks content above 22.05 kHz.
fn detect_upsampled_hires(samples: &[f32], sample_rate: u32) -> (bool, u32) {
    let stats = compute_average_linear_spectrum(samples, sample_rate);
    if stats.window_count == 0 {
        return (false, 0);
    }
    let spectrum = &stats.avg_spectrum;

    let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
    let hz_to_bin = |hz: f32| ((hz / bin_hz) as usize).min(spectrum.len() - 1);

    // Reference audible band: 5 kHz to 15 kHz
    let ref_start = hz_to_bin(5_000.0);
    let ref_end = hz_to_bin(15_000.0);
    let ref_energy = band_power_db(&spectrum, ref_start, ref_end);

    // Ultrasonic band: 25 kHz up to 0.85 * Nyquist
    let nyquist = sample_rate as f32 / 2.0;
    let ultra_start = hz_to_bin(25_000.0);
    let ultra_end = hz_to_bin(nyquist * 0.85);

    let ultra_energy = band_power_db(&spectrum, ultra_start, ultra_end);

    // If audible band has clear energy (>-60 dBFS) but ultrasonic is down near noise floor (<-75 dBFS and >35 dB down)
    if ref_energy > -65.0 && ultra_energy < -75.0 && (ref_energy - ultra_energy) > 35.0 {
        // Detect whether cutoff is near 22.05 kHz (upsampled from 44.1k) or 24 kHz (upsampled from 48k)
        let bin_22k = hz_to_bin(22_050.0);
        let energy_at_22k = band_power_db(&spectrum, bin_22k.saturating_sub(2), bin_22k + 2);
        let cutoff = if energy_at_22k < -70.0 {
            22_050
        } else {
            24_000
        };
        (true, cutoff)
    } else {
        (false, 0)
    }
}

/// Detects lossy cutoff shelves for standard sample rates (<= 48 kHz).
fn detect_lossy_cutoff(samples: &[f32], sample_rate: u32) -> (SpectralVerdict, Option<u32>) {
    let stats = compute_average_linear_spectrum(samples, sample_rate);
    let nyquist = sample_rate as f32 / 2.0;
    let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
    let hz_to_bin = |hz: f32| ((hz / bin_hz) as usize).min(stats.avg_spectrum.len() - 1);

    if stats.window_count == 0 {
        return (SpectralVerdict::Authentic, Some(nyquist as u32));
    }
    let spectrum = &stats.avg_spectrum;

    // Passband reference level (2 kHz - 10 kHz)
    let ref_start = hz_to_bin(2_000.0);
    let ref_end = hz_to_bin(10_000.0);
    let ref_level = band_power_db(spectrum, ref_start, ref_end);

    let test_start_hz = 14_000.0;
    let test_end_hz = nyquist - 300.0;
    let step_hz = 250.0;

    // Scan frequency bands of 500 Hz width from 14 kHz to Nyquist
    let drop_threshold = (ref_level - 36.0).max(-78.0);

    let mut detected_shelf_hz = None;
    let mut h = test_start_hz;
    while h < test_end_hz {
        let b_start = hz_to_bin(h);
        let b_end = hz_to_bin(h + 500.0);
        let current_band_db = band_power_db(spectrum, b_start, b_end);

        if current_band_db <= drop_threshold {
            // Verify that all subsequent bands up to Nyquist remain down, OR that energy above this band
            // is non-continuous (bit reservoir bursts / spectral holes characteristic of lossy codecs)
            let remainder_down =
                band_power_db(spectrum, b_start, hz_to_bin(test_end_hz)) <= drop_threshold + 4.0;
            let fragmented_bursts = stats.high_freq_continuity < 0.15;
            if remainder_down || fragmented_bursts {
                detected_shelf_hz = Some(h as u32);
                break;
            }
        }
        h += step_hz;
    }

    if let Some(cutoff_hz) = detected_shelf_hz {
        if cutoff_hz <= 20_700 {
            // Confirm steep cliff: energy 1.5 kHz below cutoff should be significantly higher (> 10 dB drop)
            let prev_start = hz_to_bin((cutoff_hz.saturating_sub(1_500)) as f32);
            let prev_end = hz_to_bin((cutoff_hz.saturating_sub(500)) as f32);
            let prev_level = band_power_db(spectrum, prev_start, prev_end);

            let steep_cliff = prev_level - drop_threshold >= 10.0;
            let bit_reservoir_artifact = cutoff_hz >= 19_500 && stats.high_freq_continuity < 0.15;

            if steep_cliff || bit_reservoir_artifact {
                return (SpectralVerdict::LossyCutoff(cutoff_hz), Some(cutoff_hz));
            }
        }
    }

    // High frequency energy persists up to near Nyquist
    let near_nyquist_bin = hz_to_bin((nyquist - 1_000.0).max(18_000.0));
    let near_nyquist_hz = (near_nyquist_bin as f32 * bin_hz).round() as u32;
    (SpectralVerdict::Authentic, Some(near_nyquist_hz))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_sine(freq: f32, sample_rate: u32, duration_secs: f32, amp: f32) -> Vec<f32> {
        let n = (sample_rate as f32 * duration_secs) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * PI * freq * t).sin() * amp
            })
            .collect()
    }

    /// Generate band-limited pseudo-noise up to `max_freq` Hz.
    fn generate_band_limited_signal(
        sample_rate: u32,
        max_freq: f32,
        duration_secs: f32,
    ) -> Vec<f32> {
        let n = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = vec![0.0f32; n];
        let mut f = 200.0;
        let step = 100.0;
        while f <= max_freq {
            let amp = 0.5 / (1.0 + f / 8000.0);
            for (i, s) in samples.iter_mut().enumerate() {
                let t = i as f32 / sample_rate as f32;
                *s += (2.0 * PI * f * t).sin() * amp;
            }
            f += step;
        }
        let max_val = samples.iter().cloned().fold(0.0f32, f32::max);
        if max_val > 0.0 {
            for s in &mut samples {
                *s = (*s / max_val) * 0.7;
            }
        }
        samples
    }

    #[test]
    fn test_authentic_cd_lossless() {
        let samples = generate_band_limited_signal(44_100, 21_800.0, 2.0);
        let result = analyze_provenance_from_samples(&samples, 44_100, Some(16));
        assert_eq!(result.verdict, ProvenanceVerdict::Authentic);
        assert!(result.cutoff_hz.unwrap() >= 20_000);
        assert_eq!(result.effective_bit_depth, Some(16));
    }

    #[test]
    fn test_lossy_transcode_16khz_shelf() {
        let samples = generate_band_limited_signal(44_100, 16_000.0, 2.0);
        let result = analyze_provenance_from_samples(&samples, 44_100, Some(16));
        assert_eq!(result.verdict, ProvenanceVerdict::SuspectedLossy);
        let cutoff = result.cutoff_hz.expect("cutoff hz");
        assert!(
            (15_000..=17_500).contains(&cutoff),
            "expected ~16 kHz cutoff, got {cutoff}"
        );
    }

    #[test]
    fn test_lossy_transcode_20khz_shelf() {
        let samples = generate_band_limited_signal(44_100, 20_000.0, 2.0);
        let result = analyze_provenance_from_samples(&samples, 44_100, Some(16));
        assert_eq!(result.verdict, ProvenanceVerdict::SuspectedLossy);
        let cutoff = result.cutoff_hz.expect("cutoff hz");
        assert!(
            (19_500..=20_700).contains(&cutoff),
            "expected ~20 kHz cutoff, got {cutoff}"
        );
    }

    #[test]
    fn test_lossy_transcode_bit_reservoir_bursts() {
        let mut samples = generate_band_limited_signal(44_100, 20_000.0, 3.0);
        // Add a transient 21.2 kHz burst to simulate MP3 bit reservoir burst in a single frame
        let burst = generate_sine(21_200.0, 44_100, 0.03, 0.03);
        for (i, &s) in burst.iter().enumerate() {
            if i + 44_100 < samples.len() {
                samples[i + 44_100] += s;
            }
        }
        let result = analyze_provenance_from_samples(&samples, 44_100, Some(16));
        assert_eq!(result.verdict, ProvenanceVerdict::SuspectedLossy);
        let cutoff = result.cutoff_hz.expect("cutoff hz");
        assert!(
            (19_500..=20_700).contains(&cutoff),
            "expected ~20 kHz cutoff with bit reservoir bursts, got {cutoff}"
        );
    }

    #[test]
    fn test_padded_bit_depth_detection() {
        let mut samples = generate_band_limited_signal(44_100, 21_500.0, 2.0);
        // Quantize strictly to 16 bits (dividing by 32768.0)
        for s in &mut samples {
            *s = (*s * 32768.0).round() / 32768.0;
        }

        let result = analyze_provenance_from_samples(&samples, 44_100, Some(24));
        assert_eq!(result.effective_bit_depth, Some(16));
        assert_eq!(result.verdict, ProvenanceVerdict::PaddedBitDepth);
    }

    #[test]
    fn test_authentic_24bit_detection() {
        let mut samples = generate_band_limited_signal(44_100, 21_500.0, 2.0);
        for (i, s) in samples.iter_mut().enumerate() {
            let dither = (((i % 17) as f32 - 8.0) / 8_388_608.0) * 5.0;
            *s += dither;
        }

        let result = analyze_provenance_from_samples(&samples, 44_100, Some(24));
        assert_eq!(result.effective_bit_depth, Some(24));
        assert_eq!(result.verdict, ProvenanceVerdict::Authentic);
    }

    #[test]
    fn test_upsampled_hires_detection() {
        let samples = generate_band_limited_signal(96_000, 21_000.0, 2.0);
        let result = analyze_provenance_from_samples(&samples, 96_000, Some(24));
        assert_eq!(result.verdict, ProvenanceVerdict::SuspectedUpsampled);
        assert_eq!(result.effective_sample_rate, Some(44_100));
    }

    #[test]
    fn test_authentic_hires_detection() {
        let samples = generate_band_limited_signal(96_000, 40_000.0, 2.0);
        let result = analyze_provenance_from_samples(&samples, 96_000, Some(24));
        assert_eq!(result.verdict, ProvenanceVerdict::AuthenticHiRes);
        assert_eq!(result.effective_sample_rate, Some(96_000));
    }

    #[test]
    fn test_low_frequency_acoustic_is_authentic() {
        // Low-frequency/bass/mellow audio without an artificial brickwall filter is authentic
        let samples = generate_sine(200.0, 44_100, 2.0, 0.7);
        let result = analyze_provenance_from_samples(&samples, 44_100, Some(16));
        assert_eq!(result.verdict, ProvenanceVerdict::Authentic);
    }
}
