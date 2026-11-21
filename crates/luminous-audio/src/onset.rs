//! Bass onset ("a kick just happened") detection for the visualizers.
//!
//! Fed the mono stream the spectrum reads, in order and without gaps, it
//! reports how strongly a low-frequency attack happened in what it was just
//! given. Everything is measured in samples, so it needs no clock and behaves
//! the same at every output sample rate.
//!
//! Method: a low-pass (two cascaded biquads at [`CUTOFF_HZ`]) isolates the
//! kick, a one-pole envelope smooths its power, and every [`HOP_SECS`] the
//! envelope's rise in dB over the last [`FLUX_LAG_HOPS`] hops is the flux. An
//! onset is flux above `mean + K·σ` of the last second of flux and above
//! [`MIN_FLUX_DB`]; after one, none fires for [`REFRACTORY_SECS`]. Working in dB
//! makes the threshold independent of playback volume, which scales the
//! visualizer copy. A sudden volume change looks exactly like an attack to
//! anything that only sees samples, so the caller resets the detector when
//! the volume jumps (`VisualizerAnalyzer::frame`).

use std::collections::VecDeque;

/// Kick drums sit roughly in 40–150 Hz; the low-pass keeps that and drops
/// the rest of the mix.
const CUTOFF_HZ: f32 = 150.0;
/// Envelope time constant: long enough to smooth the low band's ripple,
/// short enough to keep a kick's attack inside one or two hops.
const ENVELOPE_SECS: f32 = 0.020;
/// Analysis step. Every hop since the last call is analysed, so this sets the
/// timing resolution, not the emission rate.
const HOP_SECS: f32 = 0.010;
/// Flux compares against the envelope this many hops back, so an attack
/// split across two hops still counts in full.
const FLUX_LAG_HOPS: usize = 3;
/// The adaptive threshold's memory.
const HISTORY_SECS: f32 = 1.0;
/// Standard deviations above the recent mean flux an onset must reach.
const THRESHOLD_K: f32 = 3.0;
/// No rise smaller than this is a beat, however quiet the passage.
const MIN_FLUX_DB: f32 = 8.0;
/// Below this envelope level the band is treated as silent.
const SILENCE_DB: f32 = -70.0;
/// After an onset, no other fires for this long.
const REFRACTORY_SECS: f32 = 0.100;
/// Flux this far above the threshold reports full strength.
const FULL_STRENGTH_DB: f32 = 12.0;
/// After a start or reset nothing fires for this long, so the threshold has
/// history to stand on and audio already queued for output at an old gain
/// (see `VisualizerAnalyzer::frame`) has played out.
const WARMUP_SECS: f32 = 0.25;

/// Direct-form-I biquad low-pass (RBJ cookbook, Butterworth Q).
#[derive(Clone, Copy)]
struct LowPass {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl LowPass {
    fn new(sample_rate: u32, cutoff_hz: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate as f32;
        let alpha = w0.sin() / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Detects bass onsets in a continuous mono stream at one sample rate.
pub(crate) struct OnsetDetector {
    filters: [LowPass; 2],
    envelope: f32,
    envelope_coeff: f32,
    hop_len: usize,
    in_hop: usize,
    /// Envelope level in dB at the end of the last `FLUX_LAG_HOPS` hops.
    recent_db: VecDeque<f32>,
    /// Flux of the last `HISTORY_SECS`, for the adaptive threshold.
    flux_history: VecDeque<f32>,
    history_len: usize,
    refractory_hops: usize,
    refractory_left: usize,
    warmup_hops: usize,
}

impl OnsetDetector {
    pub(crate) fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(1);
        let rate = sample_rate as f32;
        let hops_per_sec = 1.0 / HOP_SECS;
        let history_len = (HISTORY_SECS * hops_per_sec).round() as usize;
        let filter = LowPass::new(sample_rate, CUTOFF_HZ);
        Self {
            filters: [filter, filter],
            envelope: 0.0,
            envelope_coeff: (-1.0 / (ENVELOPE_SECS * rate)).exp(),
            hop_len: ((HOP_SECS * rate).round() as usize).max(1),
            in_hop: 0,
            recent_db: VecDeque::with_capacity(FLUX_LAG_HOPS + 1),
            flux_history: VecDeque::with_capacity(history_len + 1),
            history_len,
            refractory_hops: (REFRACTORY_SECS * hops_per_sec).round() as usize,
            refractory_left: 0,
            warmup_hops: (WARMUP_SECS * hops_per_sec).round() as usize,
        }
    }

    /// Forget everything heard so far, as after a seek or pause: the next
    /// samples are a fresh start, and their arrival is never itself an onset.
    pub(crate) fn reset(&mut self) {
        for f in &mut self.filters {
            f.reset();
        }
        self.envelope = 0.0;
        self.in_hop = 0;
        self.recent_db.clear();
        self.flux_history.clear();
        self.refractory_left = 0;
    }

    /// Feed the next samples of the stream. Returns the strength in (0, 1]
    /// of the strongest onset among them, or 0 if there was none.
    pub(crate) fn process(&mut self, samples: &[f32]) -> f32 {
        let mut strongest = 0.0f32;
        for &s in samples {
            let first = self.filters[0].process(s);
            let low = self.filters[1].process(first);
            self.envelope =
                self.envelope_coeff * self.envelope + (1.0 - self.envelope_coeff) * low * low;
            self.in_hop += 1;
            if self.in_hop == self.hop_len {
                self.in_hop = 0;
                strongest = strongest.max(self.end_hop());
            }
        }
        strongest
    }

    /// Close one hop: score its flux against the recent history.
    fn end_hop(&mut self) -> f32 {
        let level_db = 10.0 * (self.envelope + 1e-12).log10();

        let flux = if self.recent_db.len() == FLUX_LAG_HOPS && level_db > SILENCE_DB {
            let oldest = self.recent_db[0].max(SILENCE_DB);
            (level_db - oldest).max(0.0)
        } else {
            0.0
        };
        if self.recent_db.len() == FLUX_LAG_HOPS {
            self.recent_db.pop_front();
        }
        self.recent_db.push_back(level_db);

        // The threshold comes from the history before this hop, so a beat
        // doesn't raise its own bar.
        let mut strength = 0.0;
        if self.refractory_left > 0 {
            self.refractory_left -= 1;
        } else if self.flux_history.len() >= self.warmup_hops {
            let n = self.flux_history.len() as f32;
            let mean = self.flux_history.iter().sum::<f32>() / n;
            let var = self
                .flux_history
                .iter()
                .map(|f| (f - mean) * (f - mean))
                .sum::<f32>()
                / n;
            let threshold = (mean + THRESHOLD_K * var.sqrt()).max(MIN_FLUX_DB);
            if flux > threshold {
                strength = ((flux - threshold) / FULL_STRENGTH_DB).min(1.0);
                self.refractory_left = self.refractory_hops;
            }
        }

        if self.flux_history.len() == self.history_len {
            self.flux_history.pop_front();
        }
        self.flux_history.push_back(flux);
        strength
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kick: a 55 Hz sine dropping toward 45 Hz with a fast decay.
    fn add_kick(out: &mut [f32], at: usize, sample_rate: u32, gain: f32) {
        let rate = sample_rate as f32;
        let len = (0.25 * rate) as usize;
        let mut phase = 0.0f32;
        for i in 0..len.min(out.len().saturating_sub(at)) {
            let t = i as f32 / rate;
            let freq = 45.0 + 10.0 * (-t / 0.03).exp();
            phase += 2.0 * std::f32::consts::PI * freq / rate;
            out[at + i] += gain * 0.8 * (-t / 0.08).exp() * phase.sin();
        }
    }

    /// Deterministic white noise in [-amp, amp].
    fn noise(len: usize, amp: f32, seed: u32) -> Vec<f32> {
        let mut state = seed.max(1);
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                amp * ((state as f32 / u32::MAX as f32) * 2.0 - 1.0)
            })
            .collect()
    }

    fn sine(len: usize, freq: f32, amp: f32, sample_rate: u32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                amp * (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin()
            })
            .collect()
    }

    /// A mix bed (a mid tone and a little noise) with kicks every `period_secs`.
    fn kick_train(sample_rate: u32, secs: f32, period_secs: f32, gain: f32) -> (Vec<f32>, usize) {
        let len = (secs * sample_rate as f32) as usize;
        let mut out = sine(len, 880.0, 0.2 * gain, sample_rate);
        for (o, n) in out.iter_mut().zip(noise(len, 0.02 * gain, 7)) {
            *o += n;
        }
        let period = (period_secs * sample_rate as f32) as usize;
        let mut kicks = 0;
        // First kick once the detector has warmed up.
        let mut at = sample_rate as usize / 2;
        while at < len {
            add_kick(&mut out, at, sample_rate, gain);
            kicks += 1;
            at += period;
        }
        (out, kicks)
    }

    /// Feed in tick-sized chunks, like the visualizer loop, and count onsets.
    fn count_onsets(detector: &mut OnsetDetector, samples: &[f32], sample_rate: u32) -> usize {
        let tick = (sample_rate as usize * 33) / 1000;
        samples
            .chunks(tick)
            .filter(|chunk| detector.process(chunk) > 0.0)
            .count()
    }

    #[test]
    fn counts_each_kick_of_a_120_bpm_train() {
        let rate = 48_000;
        let (signal, kicks) = kick_train(rate, 8.0, 0.5, 1.0);
        let found = count_onsets(&mut OnsetDetector::new(rate), &signal, rate);
        assert!(found.abs_diff(kicks) <= 1, "found {found} of {kicks} kicks");
    }

    #[test]
    fn same_count_at_every_sample_rate() {
        let counts: Vec<usize> = [44_100, 48_000, 96_000, 192_000]
            .iter()
            .map(|&rate| {
                let (signal, _) = kick_train(rate, 6.0, 0.5, 1.0);
                count_onsets(&mut OnsetDetector::new(rate), &signal, rate)
            })
            .collect();
        assert!(counts.iter().all(|&c| c == counts[0]), "{counts:?}");
    }

    #[test]
    fn same_count_at_any_volume() {
        let rate = 48_000;
        let loud = {
            let (s, _) = kick_train(rate, 6.0, 0.5, 1.0);
            count_onsets(&mut OnsetDetector::new(rate), &s, rate)
        };
        let quiet = {
            let (s, _) = kick_train(rate, 6.0, 0.5, 0.05);
            count_onsets(&mut OnsetDetector::new(rate), &s, rate)
        };
        assert_eq!(loud, quiet);
    }

    #[test]
    fn steady_tone_and_noise_have_no_onsets() {
        let rate = 48_000;
        let len = 4 * rate as usize;
        let bass_tone = sine(len, 60.0, 0.5, rate);
        assert_eq!(
            count_onsets(&mut OnsetDetector::new(rate), &bass_tone, rate),
            0
        );
        for seed in [1, 42, 1234, 99_999, 7_654_321] {
            let hiss = noise(len, 0.5, seed);
            let found = count_onsets(&mut OnsetDetector::new(rate), &hiss, rate);
            assert_eq!(found, 0, "seed {seed}");
        }
    }

    #[test]
    fn kicks_inside_the_refractory_window_count_once() {
        let rate = 48_000;
        let len = rate as usize;
        let mut signal = noise(len, 0.01, 3);
        let at = rate as usize / 2;
        add_kick(&mut signal, at, rate, 1.0);
        add_kick(&mut signal, at + (0.04 * rate as f32) as usize, rate, 1.0);
        assert_eq!(
            count_onsets(&mut OnsetDetector::new(rate), &signal, rate),
            1
        );
    }

    #[test]
    fn volume_ramp_is_not_an_onset() {
        let rate = 48_000;
        let len = 3 * rate as usize;
        let ramp_start = rate as usize;
        let ramp_len = rate as usize / 5;
        let signal: Vec<f32> = sine(len, 70.0, 0.25, rate)
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                let t = ((i.saturating_sub(ramp_start)) as f32 / ramp_len as f32).min(1.0);
                s * (1.0 + t) // +6 dB over 200 ms, like a volume drag
            })
            .collect();
        assert_eq!(
            count_onsets(&mut OnsetDetector::new(rate), &signal, rate),
            0
        );
    }

    #[test]
    fn loud_start_after_reset_is_not_an_onset() {
        let rate = 48_000;
        let mut detector = OnsetDetector::new(rate);
        count_onsets(&mut detector, &vec![0.0; rate as usize], rate);
        detector.reset();
        // Resuming mid-song: loud bass from the first sample.
        let resumed = sine(rate as usize, 60.0, 0.8, rate);
        assert_eq!(count_onsets(&mut detector, &resumed, rate), 0);
    }

    #[test]
    fn strength_is_within_unit_range() {
        let rate = 48_000;
        let (signal, _) = kick_train(rate, 4.0, 0.5, 1.0);
        let mut detector = OnsetDetector::new(rate);
        for chunk in signal.chunks(1600) {
            let s = detector.process(chunk);
            assert!((0.0..=1.0).contains(&s), "{s}");
        }
    }
}
