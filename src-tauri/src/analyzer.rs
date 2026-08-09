//! Audio analysis primitives used by two unrelated consumers: a live
//! ring-buffer + FFT path for the real-time visualizer, and an offline
//! whole-file decode used wherever a module needs every sample of a track
//! at once (`loudness.rs`'s R128 analysis, `waveform.rs`'s peak envelope).
//! The two halves don't interact — they're grouped here because both are
//! "decode audio and do math on it", not because they share code.

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use rustfft::{num_complex::Complex, FftPlanner};
use std::collections::VecDeque;
use std::path::Path;

// ---------------------------------------------------------------------------
// Real-time Playback Buffer for Spectrum Analyzer
// ---------------------------------------------------------------------------

/// Fixed-capacity ring buffer of the most recently played samples, fed
/// continuously from the CPAL output callback (`audio.rs::build_output`) and
/// polled roughly 30 times/sec by the visualizer's spectrum loop (`lib.rs`).
/// Producer and consumer run on different threads/tasks — all access goes
/// through the internal mutex.
pub struct AudioVisualizerBuffer {
    buffer: Mutex<VecDeque<f32>>,
    max_size: usize,
}

impl AudioVisualizerBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            buffer: Mutex::new(VecDeque::with_capacity(max_size)),
            max_size,
        }
    }

    /// Append newly played samples, evicting the oldest ones once the
    /// buffer is at `max_size` (so it always holds the *most recent*
    /// window, not the earliest).
    pub fn push(&self, samples: &[f32]) {
        let mut buf = self.buffer.lock();
        for &s in samples {
            if buf.len() >= self.max_size {
                buf.pop_front();
            }
            buf.push_back(s);
        }
    }

    /// Return the most recent `size` samples, oldest-first. If fewer than
    /// `size` samples have ever been pushed (e.g. right after playback
    /// starts), the result is zero-padded at the front rather than
    /// shortened, so callers can always assume a fixed-length buffer.
    pub fn get_samples(&self, size: usize) -> Vec<f32> {
        let buf = self.buffer.lock();
        let len = buf.len();
        if len == 0 {
            return vec![0.0; size];
        }

        let start = len.saturating_sub(size);
        let mut result = Vec::with_capacity(size);
        for i in start..len {
            result.push(buf[i]);
        }

        while result.len() < size {
            result.push(0.0);
        }
        result
    }
}

/// Compute 32 log-spaced frequency-bin magnitudes (bass→treble, roughly
/// perceptually normalized to `[0, 1]`) from the last `fft_size` samples in
/// `visualizer_buf`, for driving the real-time spectrum visualizer.
///
/// `fft_size` must be a power of two (required by `rustfft`'s planner) and
/// should stay small enough to run every animation frame — callers use 1024.
pub fn calculate_spectrum(
    visualizer_buf: &AudioVisualizerBuffer,
    fft_size: usize,
    sample_rate: u32,
) -> Vec<f32> {
    let samples = visualizer_buf.get_samples(fft_size);

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    // Apply a Hann window to reduce spectral leakage
    let mut complex_samples: Vec<Complex<f32>> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let window =
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos());
            Complex {
                re: s * window,
                im: 0.0,
            }
        })
        .collect();

    fft.process(&mut complex_samples);

    // Calculate magnitudes of the first half (positive frequencies)
    let half_size = fft_size / 2;
    let mut spectrum = Vec::with_capacity(half_size);
    for sample in complex_samples.iter().take(half_size) {
        let magnitude = (sample.re * sample.re + sample.im * sample.im).sqrt();
        spectrum.push(magnitude);
    }

    // Downsample/group the spectrum into 32 bins, log-spaced by actual
    // frequency (not raw FFT index) so each output bin corresponds to a
    // consistent, real Hz range regardless of sample rate. The 62.5 Hz -
    // 16 kHz window is chosen so bin 8 lands exactly on 250 Hz and bin 20
    // on 2 kHz — the same bass/mid/treble cutoffs `band_waveform.rs` uses — which
    // is what the frontend's slice(0,8)/slice(8,20)/slice(20,32) split
    // assumes.
    const LOW_FREQ: f32 = 62.5;
    const HIGH_FREQ: f32 = 16_000.0;
    let num_bins = 32;
    let mut bins = vec![0.0f32; num_bins];
    let nyquist = sample_rate as f32 / 2.0;
    let hz_per_fft_bin = sample_rate as f32 / fft_size as f32;

    for (i, bin) in bins.iter_mut().enumerate() {
        let freq_lo = LOW_FREQ * (HIGH_FREQ / LOW_FREQ).powf(i as f32 / num_bins as f32);
        let freq_hi =
            (LOW_FREQ * (HIGH_FREQ / LOW_FREQ).powf((i + 1) as f32 / num_bins as f32)).min(nyquist);

        let start_idx = ((freq_lo / hz_per_fft_bin) as usize).min(half_size.saturating_sub(1));
        let end_idx = ((freq_hi / hz_per_fft_bin) as usize).clamp(start_idx + 1, half_size);

        let count = end_idx - start_idx;
        let sum: f32 = spectrum[start_idx..end_idx].iter().sum();

        *bin = if count > 0 { sum / count as f32 } else { 0.0 };
    }

    // Raw FFT magnitude falls off steeply with frequency for typical music,
    // so treated linearly a wide swath of the loudest (usually low-end)
    // bins all clip to the same "maxed out" display value while quieter
    // bins crater to near zero — a flat plateau + cliff instead of a
    // smooth decreasing shape. Normalizing every bin relative to the
    // frame's own peak and applying a cube-root curve compresses that
    // dynamic range: the loudest bin lands at 1.0, and quieter bins are
    // boosted (more so the quieter they are) instead of being swallowed.
    // This is self-calibrating — it doesn't depend on absolute FFT
    // magnitude scale or playback volume — and it means no single band
    // is structurally destined to stay pinned at maximum.
    let peak = bins.iter().cloned().fold(0.0f32, f32::max);
    if peak > 0.0 {
        for bin in bins.iter_mut() {
            *bin = (*bin / peak).cbrt();
        }
    }

    bins
}

// ---------------------------------------------------------------------------
// Offline Fast Audio Decoder
// ---------------------------------------------------------------------------

/// Decode an entire audio file as fast as the CPU allows (no real-time
/// pacing, unlike playback) and downmix to mono by averaging all channels.
/// Returns `(samples, sample_rate)`. Individual corrupt packets are skipped
/// rather than aborting the whole decode; any other decoder error stops
/// early and returns whatever was decoded so far. Used by `waveform.rs` for
/// its peak envelope; `loudness.rs` needs channels kept separate for BS.1770
/// weighting and so has its own near-identical `decode_channels` instead of
/// calling this.
pub fn decode_all_samples(path: &Path) -> Result<(Vec<f32>, u32)> {
    use symphonia::core::{
        codecs::audio::AudioDecoderOptions,
        errors::Error as SymphoniaError,
        formats::{probe::Hint, FormatOptions, TrackType},
        io::MediaSourceStream,
        meta::MetadataOptions,
    };

    let file = std::fs::File::open(path).context("failed to open audio file for offline decode")?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut format = symphonia::default::get_probe()
        .probe(
            &Hint::new(),
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("format probe failed during offline decode")?;

    let track = format
        .default_track(TrackType::Audio)
        .cloned()
        .ok_or_else(|| anyhow!("no active audio track found for offline decode"))?;

    let track_id = track.id;
    let audio_params = track
        .codec_params
        .as_ref()
        .and_then(|c| c.audio())
        .ok_or_else(|| anyhow!("no audio codec parameters for offline decode"))?;
    let sample_rate = audio_params.sample_rate.unwrap_or(44100);

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .context("failed to create decoder for offline decode")?;

    let mut samples = Vec::new();

    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let channels = decoded.spec().channels().count();
                let mut decoded_samples: Vec<f32> = Vec::new();
                decoded.copy_to_vec_interleaved(&mut decoded_samples);

                if channels == 1 {
                    samples.extend_from_slice(&decoded_samples);
                } else {
                    for chunk in decoded_samples.chunks(channels) {
                        let sum: f32 = chunk.iter().sum();
                        samples.push(sum / channels as f32);
                    }
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(_) => break,
        }
    }

    Ok((samples, sample_rate))
}
