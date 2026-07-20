use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use rustfft::{num_complex::Complex, FftPlanner};
use std::collections::VecDeque;
use std::path::Path;

// ---------------------------------------------------------------------------
// Real-time Playback Buffer for Spectrum Analyzer
// ---------------------------------------------------------------------------

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

    pub fn push(&self, samples: &[f32]) {
        let mut buf = self.buffer.lock();
        for &s in samples {
            if buf.len() >= self.max_size {
                buf.pop_front();
            }
            buf.push_back(s);
        }
    }

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

        // Zero-pad if needed
        while result.len() < size {
            result.push(0.0);
        }
        result
    }
}

/// Calculate 32 frequency bins from the audio buffer using FFT with Hann windowing and log scaling.
pub fn calculate_spectrum(visualizer_buf: &AudioVisualizerBuffer, fft_size: usize) -> Vec<f32> {
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

    // Downsample/group the spectrum into 32 bins logarithmically
    let mut bins = vec![0.0f32; 32];
    let num_bins = bins.len();

    for (i, bin) in bins.iter_mut().enumerate() {
        let start_pct = i as f32 / num_bins as f32;
        let end_pct = (i + 1) as f32 / num_bins as f32;

        // Logarithmic scaling maps index bounds to better match human hearing spacing
        let start_idx = (start_pct.powi(2) * half_size as f32) as usize;
        let end_idx =
            ((end_pct.powi(2) * half_size as f32) as usize).clamp(start_idx + 1, half_size);

        let count = end_idx - start_idx;
        let sum: f32 = spectrum[start_idx..end_idx].iter().sum();

        *bin = if count > 0 { sum / count as f32 } else { 0.0 };
    }

    bins
}

// ---------------------------------------------------------------------------
// Offline Fast Audio Decoder
// ---------------------------------------------------------------------------

/// Decode an entire audio file as fast as the CPU allows and average it to mono.
pub fn decode_all_samples(path: &Path) -> Result<(Vec<f32>, u32)> {
    use symphonia::core::{
        audio::SampleBuffer, codecs::DecoderOptions, errors::Error as SymphoniaError,
        formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
    };

    let file = std::fs::File::open(path).context("failed to open audio file for offline decode")?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("format probe failed during offline decode")?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .cloned()
        .ok_or_else(|| anyhow!("no active audio track found for offline decode"))?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("failed to create decoder for offline decode")?;

    let mut samples = Vec::new();

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                sample_buf.copy_interleaved_ref(decoded);

                let channels = spec.channels.count();
                let decoded_samples = sample_buf.samples();

                if channels == 1 {
                    samples.extend_from_slice(decoded_samples);
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
