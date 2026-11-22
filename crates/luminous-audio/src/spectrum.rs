//! The real-time visualizer feed: a ring buffer of recently played samples
//! fed by the audio callback, and the analyzer that turns it into spectrum
//! bands and bass onsets. Callers see only `AudioEngine::visualizer_frame`.

use crate::onset::OnsetDetector;
use parking_lot::Mutex;
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use serde::Serialize;
use std::sync::Arc;

/// Samples the ring buffer holds: about 340 ms at 384 kHz, so a ~33 ms
/// visualizer tick never loses samples to eviction at any supported rate.
const BUFFER_CAPACITY: usize = 1 << 17;
/// FFT length for the spectrum bands.
const FFT_SIZE: usize = 1024;
/// Unread samples older than this are skipped rather than analysed late, and
/// the onset detector restarts: the reader was away (window minimized, say)
/// and a beat from that long ago is no use to anyone.
const MAX_BACKLOG_MS: u64 = 250;
/// A gain change bigger than this restarts onset detection.
const GAIN_JUMP_DB: f32 = 3.0;

// ---------------------------------------------------------------------------
// Real-time playback buffer
// ---------------------------------------------------------------------------

struct Ring {
    data: Box<[f32]>,
    /// Samples ever pushed; the next sample's position in the stream.
    written: u64,
}

/// Fixed-capacity ring buffer of the most recently played mono samples, fed
/// continuously from the CPAL output callback (`OutputRenderer::render`) and
/// read roughly 30 times/sec by the visualizer loop. Every sample has a
/// position in the stream since the buffer was created, so a reader can ask
/// for everything after the last sample it saw. Producer and consumer run on
/// different threads; all access goes through the internal mutex.
pub(crate) struct AudioVisualizerBuffer {
    ring: Mutex<Ring>,
}

/// Outcome of [`AudioVisualizerBuffer::read_since`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ReadSince {
    /// Position to pass to the next `read_since`.
    pub(crate) next: u64,
    /// The samples between the requested position and the buffer's oldest
    /// were already gone, so nothing was returned.
    pub(crate) gap: bool,
}

impl AudioVisualizerBuffer {
    pub(crate) fn new() -> Self {
        Self::with_capacity(BUFFER_CAPACITY)
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            ring: Mutex::new(Ring {
                data: vec![0.0; capacity.max(1)].into_boxed_slice(),
                written: 0,
            }),
        }
    }

    /// Append newly played samples, overwriting the oldest once full.
    /// Allocation-free: this runs on the audio thread.
    pub(crate) fn push(&self, samples: &[f32]) {
        let mut ring = self.ring.lock();
        let cap = ring.data.len();
        // Only the last `cap` samples can survive this push.
        let skip = samples.len().saturating_sub(cap);
        ring.written += skip as u64;
        let mut rest = &samples[skip..];
        while !rest.is_empty() {
            let at = (ring.written % cap as u64) as usize;
            let n = rest.len().min(cap - at);
            ring.data[at..at + n].copy_from_slice(&rest[..n]);
            ring.written += n as u64;
            rest = &rest[n..];
        }
    }

    /// Position the next pushed sample will have.
    pub(crate) fn written(&self) -> u64 {
        self.ring.lock().written
    }

    /// Fill `out` with the most recent `out.len()` samples, oldest first,
    /// zero-padded at the front if fewer have ever been pushed.
    pub(crate) fn latest(&self, out: &mut [f32]) {
        let ring = self.ring.lock();
        let cap = ring.data.len();
        let have = (ring.written.min(cap as u64) as usize).min(out.len());
        let pad = out.len() - have;
        out[..pad].fill(0.0);
        for (i, slot) in out[pad..].iter_mut().enumerate() {
            let pos = ring.written - have as u64 + i as u64;
            *slot = ring.data[(pos % cap as u64) as usize];
        }
    }

    /// Replace `out` with every sample from position `pos` to now, at most
    /// `max_len` of them. If more than that (or more than the buffer holds)
    /// arrived since `pos`, or `pos` is not a position this buffer has
    /// reached, `out` is left empty and the result reports a gap; reading
    /// resumes from now.
    pub(crate) fn read_since(&self, pos: u64, max_len: usize, out: &mut Vec<f32>) -> ReadSince {
        out.clear();
        let ring = self.ring.lock();
        let cap = ring.data.len();
        let keep = cap.min(max_len) as u64;
        if pos > ring.written || ring.written - pos > keep {
            return ReadSince {
                next: ring.written,
                gap: true,
            };
        }
        out.extend((pos..ring.written).map(|p| ring.data[(p % cap as u64) as usize]));
        ReadSince {
            next: ring.written,
            gap: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Analyzer
// ---------------------------------------------------------------------------

/// One frame of visualizer data.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VisualizerFrame {
    /// 32 log-spaced spectrum bands, bass to treble, in `[0, 1]` relative to
    /// the frame's own loudest band.
    pub bands: Vec<f32>,
    /// Strength in `(0, 1]` of the strongest bass onset played since the
    /// previous frame; 0 when there was none.
    pub onset: f32,
}

/// Turns the ring buffer into [`VisualizerFrame`]s: remembers how far it has
/// read so every played sample reaches the onset detector exactly once, and
/// keeps its FFT plan and scratch buffers between frames.
pub(crate) struct VisualizerAnalyzer {
    /// Output rate `onset` was built for; 0 until the first frame.
    sample_rate: u32,
    /// Playback volume at the previous frame; `None` until the first.
    gain: Option<f32>,
    read_pos: Option<u64>,
    onset: OnsetDetector,
    unread: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    recent: Vec<f32>,
    fft_buf: Vec<Complex<f32>>,
    magnitudes: Vec<f32>,
}

impl VisualizerAnalyzer {
    pub(crate) fn new() -> Self {
        let window = (0..FFT_SIZE)
            .map(|i| {
                // Hann window, to reduce spectral leakage.
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos())
            })
            .collect();
        Self {
            sample_rate: 0,
            gain: None,
            read_pos: None,
            // Rebuilt by the first frame, for the real output rate.
            onset: OnsetDetector::new(0),
            unread: Vec::new(),
            fft: FftPlanner::new().plan_fft_forward(FFT_SIZE),
            window,
            recent: vec![0.0; FFT_SIZE],
            fft_buf: vec![Complex { re: 0.0, im: 0.0 }; FFT_SIZE],
            magnitudes: vec![0.0; FFT_SIZE / 2],
        }
    }

    /// Forget the stream so far, as when playback pauses or the spectrum is
    /// switched off: the next frame starts reading from then, and what
    /// arrives first is never counted as an onset.
    pub(crate) fn reset(&mut self) {
        self.read_pos = None;
        self.onset.reset();
    }

    /// The next frame, from what `buffer` received at `sample_rate` since the
    /// previous one, played at volume `gain` (linear, whoever set it).
    pub(crate) fn frame(
        &mut self,
        buffer: &AudioVisualizerBuffer,
        sample_rate: u32,
        gain: f32,
    ) -> VisualizerFrame {
        if sample_rate != self.sample_rate {
            self.sample_rate = sample_rate;
            self.onset = OnsetDetector::new(sample_rate);
            self.read_pos = None;
        }
        // A volume jump scales the samples like an attack would, so it
        // restarts detection; a drag's small steps don't.
        if self.gain.is_some_and(|before| is_gain_jump(before, gain)) {
            self.reset();
        }
        self.gain = Some(gain);
        let onset = match self.read_pos {
            None => {
                self.read_pos = Some(buffer.written());
                0.0
            }
            Some(pos) => {
                let max_backlog = (u64::from(sample_rate) * MAX_BACKLOG_MS / 1000) as usize;
                let read = buffer.read_since(pos, max_backlog, &mut self.unread);
                self.read_pos = Some(read.next);
                if read.gap {
                    self.onset.reset();
                    0.0
                } else {
                    self.onset.process(&self.unread)
                }
            }
        };
        buffer.latest(&mut self.recent);
        VisualizerFrame {
            bands: self.bands(sample_rate),
            onset,
        }
    }

    /// 32 log-spaced frequency-bin magnitudes (bass→treble, roughly
    /// perceptually normalized to `[0, 1]`) of the samples in `recent`.
    fn bands(&mut self, sample_rate: u32) -> Vec<f32> {
        for ((slot, &s), &w) in self.fft_buf.iter_mut().zip(&self.recent).zip(&self.window) {
            *slot = Complex { re: s * w, im: 0.0 };
        }
        self.fft.process(&mut self.fft_buf);

        // Magnitudes of the first half (positive frequencies).
        let half_size = FFT_SIZE / 2;
        for (m, c) in self.magnitudes.iter_mut().zip(&self.fft_buf[..half_size]) {
            *m = (c.re * c.re + c.im * c.im).sqrt();
        }
        let spectrum = &self.magnitudes;

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
        let hz_per_fft_bin = sample_rate as f32 / FFT_SIZE as f32;

        for (i, bin) in bins.iter_mut().enumerate() {
            let freq_lo = LOW_FREQ * (HIGH_FREQ / LOW_FREQ).powf(i as f32 / num_bins as f32);
            let freq_hi = (LOW_FREQ
                * (HIGH_FREQ / LOW_FREQ).powf((i + 1) as f32 / num_bins as f32))
            .min(nyquist);

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
}

/// Whether playback volume moving from `before` to `after` (linear) is a
/// jump: more than [`GAIN_JUMP_DB`], or to or from silence (mute).
fn is_gain_jump(before: f32, after: f32) -> bool {
    if before <= 0.0 || after <= 0.0 {
        before != after
    } else {
        (20.0 * (after / before).log10()).abs() > GAIN_JUMP_DB
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_since_returns_every_sample_across_wraparound() {
        let buf = AudioVisualizerBuffer::with_capacity(8);
        let mut out = Vec::new();
        buf.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let first = buf.read_since(0, usize::MAX, &mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        buf.push(&[7.0, 8.0, 9.0, 10.0]);
        let second = buf.read_since(first.next, usize::MAX, &mut out);
        assert_eq!(out, [7.0, 8.0, 9.0, 10.0]);
        assert_eq!(
            second,
            ReadSince {
                next: 10,
                gap: false
            }
        );
    }

    #[test]
    fn read_since_reports_evicted_samples_as_a_gap() {
        let buf = AudioVisualizerBuffer::with_capacity(4);
        let mut out = vec![99.0];
        buf.push(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(
            buf.read_since(0, usize::MAX, &mut out),
            ReadSince { next: 5, gap: true }
        );
        assert!(out.is_empty());
    }

    #[test]
    fn read_since_treats_a_backlog_beyond_max_len_as_a_gap() {
        let buf = AudioVisualizerBuffer::with_capacity(16);
        let mut out = Vec::new();
        buf.push(&[0.0; 10]);
        assert!(buf.read_since(0, 4, &mut out).gap);
        assert!(!buf.read_since(6, 4, &mut out).gap);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn read_since_a_future_position_is_a_gap() {
        let buf = AudioVisualizerBuffer::with_capacity(4);
        let mut out = Vec::new();
        buf.push(&[1.0]);
        assert!(buf.read_since(5, usize::MAX, &mut out).gap);
    }

    #[test]
    fn latest_pads_the_front_until_enough_samples_arrive() {
        let buf = AudioVisualizerBuffer::with_capacity(4);
        let mut out = [9.0; 3];
        buf.push(&[1.0, 2.0]);
        buf.latest(&mut out);
        assert_eq!(out, [0.0, 1.0, 2.0]);
        buf.push(&[3.0, 4.0, 5.0]);
        buf.latest(&mut out);
        assert_eq!(out, [3.0, 4.0, 5.0]);
    }

    #[test]
    fn push_longer_than_capacity_keeps_the_newest() {
        let buf = AudioVisualizerBuffer::with_capacity(3);
        let mut out = [0.0; 3];
        buf.push(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        buf.latest(&mut out);
        assert_eq!(out, [3.0, 4.0, 5.0]);
        assert_eq!(buf.written(), 5);
    }

    /// One second of silence with a kick in the middle.
    fn kick_after_silence(rate: u32) -> Vec<f32> {
        let mut samples = vec![0.0f32; rate as usize];
        let at = rate as usize / 2;
        for i in 0..(rate as usize / 5) {
            let t = i as f32 / rate as f32;
            samples[at + i] =
                0.8 * (-t / 0.08).exp() * (2.0 * std::f32::consts::PI * 55.0 * t).sin();
        }
        samples
    }

    #[test]
    fn analyzer_reports_an_onset_played_between_frames() {
        let rate = 48_000;
        let buf = AudioVisualizerBuffer::new();
        let mut analyzer = VisualizerAnalyzer::new();
        analyzer.frame(&buf, rate, 1.0); // starts reading from here
        let mut onsets = 0;
        for tick in kick_after_silence(rate).chunks(1600) {
            buf.push(tick);
            let frame = analyzer.frame(&buf, rate, 1.0);
            assert_eq!(frame.bands.len(), 32);
            if frame.onset > 0.0 {
                onsets += 1;
            }
        }
        assert_eq!(onsets, 1);
    }

    #[test]
    fn analyzer_skips_a_backlog_instead_of_reporting_it_late() {
        let rate = 48_000;
        let buf = AudioVisualizerBuffer::new();
        let mut analyzer = VisualizerAnalyzer::new();
        analyzer.frame(&buf, rate, 1.0);
        // A whole second (kick included) played while nobody read.
        buf.push(&kick_after_silence(rate));
        assert_eq!(analyzer.frame(&buf, rate, 1.0).onset, 0.0);
    }

    /// Feeds `samples` played at volume `gain` to `analyzer` in ~33 ms
    /// ticks; counts frames with an onset.
    fn count_frames_with_onsets(
        analyzer: &mut VisualizerAnalyzer,
        buf: &AudioVisualizerBuffer,
        samples: &[f32],
        rate: u32,
        gain: f32,
    ) -> usize {
        samples
            .chunks(1600)
            .filter(|tick| {
                buf.push(tick);
                analyzer.frame(buf, rate, gain).onset > 0.0
            })
            .count()
    }

    fn bass_tone(secs: f32, gain: f32, rate: u32) -> Vec<f32> {
        (0..(secs * rate as f32) as usize)
            .map(|i| {
                gain * 0.1 * (2.0 * std::f32::consts::PI * 70.0 * i as f32 / rate as f32).sin()
            })
            .collect()
    }

    #[test]
    fn a_volume_jump_is_not_an_onset() {
        let rate = 48_000;
        let buf = AudioVisualizerBuffer::new();
        let mut analyzer = VisualizerAnalyzer::new();
        analyzer.frame(&buf, rate, 1.0);
        let mut onsets =
            count_frames_with_onsets(&mut analyzer, &buf, &bass_tone(1.0, 1.0, rate), rate, 1.0);
        // +12 dB, with 50 ms still queued for output at the old gain.
        onsets +=
            count_frames_with_onsets(&mut analyzer, &buf, &bass_tone(0.05, 1.0, rate), rate, 4.0);
        onsets +=
            count_frames_with_onsets(&mut analyzer, &buf, &bass_tone(1.0, 4.0, rate), rate, 4.0);
        assert_eq!(onsets, 0);
    }

    #[test]
    fn unmuting_is_not_an_onset() {
        let rate = 48_000;
        let buf = AudioVisualizerBuffer::new();
        let mut analyzer = VisualizerAnalyzer::new();
        analyzer.frame(&buf, rate, 1.0);
        let mut onsets =
            count_frames_with_onsets(&mut analyzer, &buf, &bass_tone(1.0, 5.0, rate), rate, 1.0);
        let muted = vec![0.0; rate as usize / 2];
        onsets += count_frames_with_onsets(&mut analyzer, &buf, &muted, rate, 0.0);
        // Unmuted, with 50 ms of silence still queued for output.
        onsets += count_frames_with_onsets(&mut analyzer, &buf, &vec![0.0; 2400], rate, 1.0);
        onsets +=
            count_frames_with_onsets(&mut analyzer, &buf, &bass_tone(1.0, 5.0, rate), rate, 1.0);
        assert_eq!(onsets, 0);
    }

    #[test]
    fn a_small_volume_step_keeps_detecting() {
        let rate = 48_000;
        let buf = AudioVisualizerBuffer::new();
        let mut analyzer = VisualizerAnalyzer::new();
        analyzer.frame(&buf, rate, 1.0);
        let quiet = vec![0.0; rate as usize / 2];
        count_frames_with_onsets(&mut analyzer, &buf, &quiet, rate, 1.0);
        // A 1 dB nudge, then a kick 100 ms later: a restart would hide it.
        let kick = &kick_after_silence(rate)[rate as usize * 4 / 10..];
        assert_eq!(
            count_frames_with_onsets(&mut analyzer, &buf, kick, rate, 1.12),
            1
        );
    }

    #[test]
    fn is_gain_jump_flags_big_steps_and_mute_only() {
        assert!(!is_gain_jump(1.0, 1.12)); // +1 dB
        assert!(is_gain_jump(1.0, 2.0)); // +6 dB
        assert!(is_gain_jump(1.0, 0.25)); // -12 dB
        assert!(is_gain_jump(0.5, 0.0) && is_gain_jump(0.0, 0.5));
        assert!(!is_gain_jump(0.0, 0.0));
    }

    #[test]
    fn analyzer_first_frame_has_no_onset() {
        let rate = 48_000;
        let buf = AudioVisualizerBuffer::new();
        buf.push(&kick_after_silence(rate));
        assert_eq!(VisualizerAnalyzer::new().frame(&buf, rate, 1.0).onset, 0.0);
    }
}
