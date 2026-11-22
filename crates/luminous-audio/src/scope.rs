//! The stereo vectorscope feed: a copy of what plays, taken before master
//! volume and kept as L/R pairs, and the meters a mastering engineer reads
//! beside a goniometer (BS.1770 loudness, true peak, phase correlation and
//! balance). Callers see only `AudioEngine::scope_frame`.
//!
//! The output callback only copies samples (`OutputRenderer::tap_stereo`). Every meter
//! runs on the reader side, ~30 times a second, and keeps running state
//! between reads: a 3 s short-term window can't be re-read from a ring that
//! holds a fraction of a second at high sample rates.

use crate::spectrum::AudioVisualizerBuffer;
use crate::OutputRenderer;
use serde::Serialize;

/// Samples (interleaved L/R) the stereo ring holds: about 340 ms at 384 kHz,
/// so a ~33 ms reader tick never loses samples to eviction.
const STEREO_CAPACITY: usize = 1 << 18;
/// Most L/R pairs a frame carries; what played since the previous frame is
/// evenly thinned to this many for the scope trace.
const MAX_POINTS: usize = 512;
/// Meter blocks: BS.1770 gates and windows in 100 ms steps.
const BLOCKS_PER_SECOND: u32 = 10;
/// Momentary loudness spans 400 ms.
const MOMENTARY_BLOCKS: usize = 4;
/// Short-term loudness spans 3 s.
const SHORT_TERM_BLOCKS: usize = 30;
/// Correlation and balance span 300 ms: steady enough to read, quick enough
/// to follow a stereo change.
const PHASE_BLOCKS: usize = 3;

/// Lowest value any level readout takes (LUFS and dBTP). Silence reads this
/// instead of −∞, so every frame is finite.
pub const METER_FLOOR_DB: f32 = -70.0;
/// Balance readouts are clamped to ± this many dB (one silent channel).
const BALANCE_LIMIT_DB: f32 = 60.0;

/// One frame of vectorscope data, everything measured before master volume
/// so no readout moves with the volume slider.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeFrame {
    /// Up to 512 L/R pairs, interleaved `[l0, r0, l1, r1, …]`, evenly
    /// sampled from what played since the previous frame. Full scale is ±1;
    /// overs are kept.
    pub points: Vec<f32>,
    /// Phase correlation over the last 300 ms, in `[-1, 1]`: +1 is mono, 0 is
    /// unrelated channels (or silence), −1 is one channel inverted.
    pub correlation: f32,
    /// BS.1770 loudness over the last 400 ms, in LUFS.
    pub momentary_lufs: f32,
    /// BS.1770 loudness over the last 3 s, in LUFS.
    pub short_term_lufs: f32,
    /// Highest 4× oversampled peak since the previous frame, in dBTP. Peak
    /// hold is the reader's: this is per frame.
    pub true_peak_db: f32,
    /// Right-minus-left level over the last 300 ms, in dB: positive leans
    /// right, negative left, 0 centred (or silent).
    pub balance_db: f32,
    /// The meters started afresh since the previous frame (the scope was
    /// switched on, playback jumped, or the reader fell behind), so anything
    /// a reader accumulates from them, such as peak hold or a loudness
    /// history, should start afresh too.
    pub restarted: bool,
}

/// The ring buffer the output callback fills with L/R pairs.
pub(crate) fn new_stereo_buffer() -> AudioVisualizerBuffer {
    AudioVisualizerBuffer::with_capacity(STEREO_CAPACITY)
}

impl OutputRenderer {
    /// Append one rendered block, interleaved at the device's channel count,
    /// to the stereo ring as L/R pairs. Stereo goes straight in; every other
    /// layout folds through [`fold_to_stereo`] into `stereo_scratch`, which
    /// is reserved for twice the largest block.
    ///
    /// Runs on the audio thread: never allocates.
    pub(crate) fn tap_stereo(&mut self, block: &[f32]) {
        let channels = self.channels as usize;
        if channels == 2 {
            // Whole frames only, so the ring always holds complete pairs.
            // Fixed-size ring buffer: overwrites the oldest samples, never allocates.
            // ast-grep-ignore: audio-callback-no-alloc
            self.shared.scope_buf.push(&block[..block.len() & !1]);
            return;
        }
        if channels == 0 {
            return;
        }
        self.stereo_scratch.clear();
        for frame in block.chunks_exact(channels) {
            let (l, r) = fold_to_stereo(frame);
            // Reserved to twice the largest block up front, cleared per callback.
            // ast-grep-ignore: audio-callback-no-alloc
            self.stereo_scratch.push(l);
            // ast-grep-ignore: audio-callback-no-alloc
            self.stereo_scratch.push(r);
        }
        // Fixed-size ring buffer: overwrites the oldest samples, never allocates.
        // ast-grep-ignore: audio-callback-no-alloc
        self.shared.scope_buf.push(&self.stereo_scratch);
    }
}

/// One frame of any layout as L/R: mono is duplicated to both sides; wider
/// layouts fold to the front pair plus centre at −3 dB (centre is the third
/// channel in every standard layout but quad, which has none).
fn fold_to_stereo(frame: &[f32]) -> (f32, f32) {
    match *frame {
        [] => (0.0, 0.0),
        [mono] => (mono, mono),
        [l, r] | [l, r, _, _] => (l, r),
        [l, r, c, ..] => {
            let c = c * std::f32::consts::FRAC_1_SQRT_2;
            (l + c, r + c)
        }
    }
}

// ---------------------------------------------------------------------------
// K-weighting (BS.1770-4 §2.1)
// ---------------------------------------------------------------------------

/// A biquad in transposed direct form II.
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// The BS.1770 K-weighting pre-filter: a high shelf (head effects) then a
/// high pass (RLB). Coefficients come from the analogue prototypes, so any
/// sample rate gets the response the standard tabulates for 48 kHz.
#[derive(Clone, Copy, Debug, Default)]
struct KWeighting {
    shelf: Biquad,
    high_pass: Biquad,
}

impl KWeighting {
    fn new(sample_rate: u32) -> Self {
        let fs = sample_rate.max(1) as f64;

        let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
        let k = (std::f64::consts::PI * f0 / fs).tan();
        let vh = 10f64.powf(gain_db / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        let shelf = Biquad {
            b0: (vh + vb * k / q + k * k) / a0,
            b1: 2.0 * (k * k - vh) / a0,
            b2: (vh - vb * k / q + k * k) / a0,
            a1: 2.0 * (k * k - 1.0) / a0,
            a2: (1.0 - k / q + k * k) / a0,
            ..Biquad::default()
        };

        let (f0, q) = (38.13547087602444, 0.5003270373238773);
        let k = (std::f64::consts::PI * f0 / fs).tan();
        let a0 = 1.0 + k / q + k * k;
        let high_pass = Biquad {
            b0: 1.0,
            b1: -2.0,
            b2: 1.0,
            a1: 2.0 * (k * k - 1.0) / a0,
            a2: (1.0 - k / q + k * k) / a0,
            ..Biquad::default()
        };
        Self { shelf, high_pass }
    }

    fn process(&mut self, x: f32) -> f64 {
        self.high_pass.process(self.shelf.process(x as f64))
    }
}

// ---------------------------------------------------------------------------
// True peak (BS.1770-4 Annex 2)
// ---------------------------------------------------------------------------

/// The standard's 48-tap, 4-phase interpolation filter for 4× oversampling.
const TRUE_PEAK_TAPS: usize = 12;
#[rustfmt::skip]
const TRUE_PEAK_PHASES: [[f64; TRUE_PEAK_TAPS]; 4] = [
    [ 0.001708984375,  0.010986328125, -0.0196533203125,  0.033203125,
     -0.0594482421875, 0.1373291015625, 0.97216796875,   -0.102294921875,
      0.047607421875, -0.026611328125,  0.014892578125,  -0.00830078125],
    [-0.0291748046875, 0.029296875,    -0.0517578125,     0.089111328125,
     -0.16650390625,   0.465087890625,  0.77978515625,   -0.2003173828125,
      0.1015625,       -0.0582275390625, 0.0330810546875, -0.0189208984375],
    [-0.0189208984375, 0.0330810546875, -0.0582275390625, 0.1015625,
     -0.2003173828125, 0.77978515625,   0.465087890625,  -0.16650390625,
      0.089111328125, -0.0517578125,    0.029296875,      -0.0291748046875],
    [-0.00830078125,   0.014892578125, -0.026611328125,   0.047607421875,
     -0.102294921875,  0.97216796875,   0.1373291015625, -0.0594482421875,
      0.033203125,     -0.0196533203125, 0.010986328125,   0.001708984375],
];

/// One channel's oversampling history.
#[derive(Clone, Copy, Debug, Default)]
struct TruePeak {
    history: [f32; TRUE_PEAK_TAPS],
    /// Where the next sample goes; the newest is just before it.
    next: usize,
}

impl TruePeak {
    /// Feed one sample; returns the largest magnitude among it and the
    /// four interpolated points ending at it.
    fn push(&mut self, x: f32) -> f32 {
        self.history[self.next] = x;
        let newest = self.next;
        self.next = (self.next + 1) % TRUE_PEAK_TAPS;
        let mut peak = x.abs() as f64;
        for phase in &TRUE_PEAK_PHASES {
            let mut y = 0.0;
            for (k, h) in phase.iter().enumerate() {
                y += h * self.history[(newest + TRUE_PEAK_TAPS - k) % TRUE_PEAK_TAPS] as f64;
            }
            peak = peak.max(y.abs());
        }
        peak as f32
    }
}

// ---------------------------------------------------------------------------
// Analyzer
// ---------------------------------------------------------------------------

/// What one finished 100 ms block contributes to the meters.
#[derive(Clone, Copy, Debug, Default)]
struct Block {
    /// K-weighted mean square, summed over L and R (BS.1770 weights both 1).
    loudness_power: f64,
    /// Unweighted Σ L², Σ R² and Σ L·R, for correlation and balance.
    left: f64,
    right: f64,
    cross: f64,
}

/// Turns the stereo ring into [`ScopeFrame`]s: remembers how far it has read
/// so every played sample reaches the meters exactly once, and keeps the
/// filter and window state between frames.
pub(crate) struct ScopeAnalyzer {
    /// Output rate the filters were built for; 0 until the first frame.
    sample_rate: u32,
    read_pos: Option<u64>,
    /// The engine's `scope_epoch` the meters belong to.
    epoch: u64,
    /// The next frame reports `restarted`.
    restarted: bool,
    unread: Vec<f32>,
    weighting: [KWeighting; 2],
    true_peak: [TruePeak; 2],
    /// Pairs per 100 ms block at `sample_rate`.
    block_len: usize,
    /// The block being filled, and how many pairs it has.
    current: Block,
    current_len: usize,
    /// The last `SHORT_TERM_BLOCKS` finished blocks, a ring; `filled` of
    /// them are real (fewer just after a reset).
    blocks: [Block; SHORT_TERM_BLOCKS],
    newest: usize,
    filled: usize,
}

impl ScopeAnalyzer {
    pub(crate) fn new() -> Self {
        let mut analyzer = Self {
            sample_rate: 0,
            read_pos: None,
            epoch: 0,
            restarted: true,
            unread: Vec::new(),
            weighting: [KWeighting::default(); 2],
            true_peak: [TruePeak::default(); 2],
            block_len: 1,
            current: Block::default(),
            current_len: 0,
            blocks: [Block::default(); SHORT_TERM_BLOCKS],
            newest: 0,
            filled: 0,
        };
        analyzer.configure(0);
        analyzer
    }

    /// Forget the stream so far, as when playback pauses, the scope is
    /// switched off, or the reader fell too far behind: the next frame reads
    /// from then, and every meter starts from silence.
    pub(crate) fn reset(&mut self) {
        self.read_pos = None;
        self.clear_meters();
    }

    fn configure(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
        self.block_len = (sample_rate / BLOCKS_PER_SECOND).max(1) as usize;
        self.reset();
    }

    fn clear_meters(&mut self) {
        self.weighting = [KWeighting::new(self.sample_rate); 2];
        self.true_peak = [TruePeak::default(); 2];
        self.current = Block::default();
        self.current_len = 0;
        self.filled = 0;
        self.restarted = true;
    }

    /// The next frame, from what `buffer` received at `sample_rate` since the
    /// previous one; a new `epoch` means playback jumped. The first frame
    /// after a reset carries no points.
    pub(crate) fn frame(
        &mut self,
        buffer: &AudioVisualizerBuffer,
        sample_rate: u32,
        epoch: u64,
    ) -> ScopeFrame {
        if sample_rate != self.sample_rate {
            self.configure(sample_rate);
        }
        if epoch != self.epoch {
            self.epoch = epoch;
            self.clear_meters();
        }
        let pos = match self.read_pos {
            Some(pos) => pos,
            None => buffer.written(),
        };
        let read = buffer.read_since(pos, STEREO_CAPACITY, &mut self.unread);
        self.read_pos = Some(read.next);
        if read.gap {
            self.clear_meters();
        }
        let mut unread = std::mem::take(&mut self.unread);
        let frame = self.measure(&unread);
        unread.clear();
        self.unread = unread;
        frame
    }

    /// Run `samples` (interleaved L/R) through every meter and build a frame.
    fn measure(&mut self, samples: &[f32]) -> ScopeFrame {
        let mut peak = 0.0f32;
        let (pairs, _) = samples.as_chunks::<2>();
        for &[l, r] in pairs {
            peak = peak
                .max(self.true_peak[0].push(l))
                .max(self.true_peak[1].push(r));
            let (kl, kr) = (self.weighting[0].process(l), self.weighting[1].process(r));
            let (l, r) = (l as f64, r as f64);
            self.current.loudness_power += kl * kl + kr * kr;
            self.current.left += l * l;
            self.current.right += r * r;
            self.current.cross += l * r;
            self.current_len += 1;
            if self.current_len == self.block_len {
                self.finish_block();
            }
        }

        let stride = pairs.len().div_ceil(MAX_POINTS).max(1);
        let points = pairs
            .iter()
            .step_by(stride)
            .flat_map(|&[l, r]| [round_point(l), round_point(r)])
            .collect();

        let (correlation, balance_db) = self.phase();
        ScopeFrame {
            points,
            correlation,
            momentary_lufs: self.loudness(MOMENTARY_BLOCKS),
            short_term_lufs: self.loudness(SHORT_TERM_BLOCKS),
            true_peak_db: to_db(peak as f64, 20.0),
            balance_db,
            restarted: std::mem::take(&mut self.restarted),
        }
    }

    fn finish_block(&mut self) {
        let n = self.block_len as f64;
        self.current.loudness_power /= n;
        self.newest = (self.newest + 1) % SHORT_TERM_BLOCKS;
        self.blocks[self.newest] = self.current;
        self.filled = (self.filled + 1).min(SHORT_TERM_BLOCKS);
        self.current = Block::default();
        self.current_len = 0;
    }

    /// The last `count` finished blocks (fewer right after a reset).
    fn recent(&self, count: usize) -> impl Iterator<Item = &Block> {
        let count = count.min(self.filled);
        (0..count)
            .map(move |i| &self.blocks[(self.newest + SHORT_TERM_BLOCKS - i) % SHORT_TERM_BLOCKS])
    }

    /// Ungated BS.1770 loudness over the last `blocks` blocks, in LUFS.
    fn loudness(&self, blocks: usize) -> f32 {
        let count = blocks.min(self.filled);
        if count == 0 {
            return METER_FLOOR_DB;
        }
        let mean = self.recent(count).map(|b| b.loudness_power).sum::<f64>() / count as f64;
        if mean <= 0.0 {
            return METER_FLOOR_DB;
        }
        ((-0.691 + 10.0 * mean.log10()) as f32).max(METER_FLOOR_DB)
    }

    /// Correlation and balance over the last 300 ms.
    fn phase(&self) -> (f32, f32) {
        let (mut left, mut right, mut cross) = (0.0, 0.0, 0.0);
        for block in self.recent(PHASE_BLOCKS) {
            left += block.left;
            right += block.right;
            cross += block.cross;
        }
        // Below this energy a channel is silent, and neither ratio means anything.
        const SILENT: f64 = 1e-10;
        let correlation = if left > SILENT && right > SILENT {
            (cross / (left * right).sqrt()).clamp(-1.0, 1.0) as f32
        } else {
            0.0
        };
        let balance_db = if left > SILENT || right > SILENT {
            (10.0 * ((right + SILENT) / (left + SILENT)).log10())
                .clamp(-BALANCE_LIMIT_DB as f64, BALANCE_LIMIT_DB as f64) as f32
        } else {
            0.0
        };
        (correlation, balance_db)
    }
}

/// `value` in dB (`factor` 20 for amplitude, 10 for power), never below the floor.
fn to_db(value: f64, factor: f64) -> f32 {
    if value <= 0.0 {
        return METER_FLOOR_DB;
    }
    ((factor * value.log10()) as f32).max(METER_FLOOR_DB)
}

/// Three decimals is finer than a scope pixel and keeps the JSON short.
fn round_point(x: f32) -> f32 {
    (x * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// `seconds` of interleaved L/R from `f(t)`, t in seconds.
    fn stereo(seconds: f32, f: impl Fn(f32) -> (f32, f32)) -> Vec<f32> {
        let n = (seconds * RATE as f32) as usize;
        (0..n)
            .flat_map(|i| {
                let (l, r) = f(i as f32 / RATE as f32);
                [l, r]
            })
            .collect()
    }

    fn sine(freq: f32, amplitude: f32) -> impl Fn(f32) -> f32 {
        move |t| amplitude * (2.0 * std::f32::consts::PI * freq * t).sin()
    }

    /// Deterministic white noise in [-amp, amp] (xorshift).
    fn noise(seed: u32, amplitude: f32) -> impl FnMut() -> f32 {
        let mut state = seed.max(1);
        move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * amplitude
        }
    }

    fn analyze(pairs: &[f32]) -> ScopeFrame {
        let mut analyzer = ScopeAnalyzer::new();
        analyzer.configure(RATE);
        analyzer.measure(pairs)
    }

    #[test]
    fn reference_tone_reads_minus_23_lufs() {
        // EBU Tech 3341: a 1 kHz sine at −23 dBFS in both channels is −23 LUFS.
        let amp = 10f32.powf(-23.0 / 20.0);
        let tone = sine(1000.0, amp);
        let frame = analyze(&stereo(4.0, |t| (tone(t), tone(t))));
        assert!((frame.momentary_lufs + 23.0).abs() < 0.1, "{frame:?}");
        assert!((frame.short_term_lufs + 23.0).abs() < 0.1, "{frame:?}");
    }

    #[test]
    fn k_weighting_matches_the_bs1770_crate() {
        let mut gen = noise(7, 0.5);
        let mut left = Vec::new();
        let mut right = Vec::new();
        for i in 0..(RATE as usize * 3) {
            let t = i as f32 / RATE as f32;
            left.push(gen() * 0.3 + sine(60.0, 0.3)(t));
            right.push(gen() * 0.3 + sine(9000.0, 0.2)(t));
        }
        let pairs: Vec<f32> = left
            .iter()
            .zip(&right)
            .flat_map(|(l, r)| [*l, *r])
            .collect();
        let frame = analyze(&pairs);

        let mut ml = bs1770::ChannelLoudnessMeter::new(RATE);
        let mut mr = bs1770::ChannelLoudnessMeter::new(RATE);
        ml.push(left.iter().copied());
        mr.push(right.iter().copied());
        let windows = bs1770::reduce_stereo(ml.as_100ms_windows(), mr.as_100ms_windows());
        let powers = windows.inner;
        let mean = powers.iter().map(|p| p.0 as f64).sum::<f64>() / powers.len() as f64;
        let expected = -0.691 + 10.0 * mean.log10();
        assert!(
            (frame.short_term_lufs as f64 - expected).abs() < 0.05,
            "ours {} vs bs1770 {expected}",
            frame.short_term_lufs
        );
    }

    #[test]
    fn in_phase_signal_correlates_fully() {
        let tone = sine(440.0, 0.5);
        let frame = analyze(&stereo(0.5, |t| (tone(t), tone(t))));
        assert!(frame.correlation > 0.999, "{frame:?}");
        assert!(frame.balance_db.abs() < 0.01, "{frame:?}");
    }

    #[test]
    fn inverted_channel_anticorrelates() {
        let tone = sine(440.0, 0.5);
        let frame = analyze(&stereo(0.5, |t| (tone(t), -tone(t))));
        assert!(frame.correlation < -0.999, "{frame:?}");
    }

    #[test]
    fn unrelated_noise_is_near_zero_correlation() {
        let mut a = noise(1, 0.5);
        let mut b = noise(99, 0.5);
        let n = RATE as usize / 2;
        let pairs: Vec<f32> = (0..n).flat_map(|_| [a(), b()]).collect();
        let frame = analyze(&pairs);
        assert!(frame.correlation.abs() < 0.05, "{frame:?}");
    }

    #[test]
    fn louder_right_channel_leans_right() {
        let tone = sine(440.0, 0.5);
        let frame = analyze(&stereo(0.5, |t| (tone(t) * 0.5, tone(t))));
        assert!((frame.balance_db - 6.02).abs() < 0.05, "{frame:?}");
    }

    #[test]
    fn true_peak_finds_the_peak_between_samples() {
        // A full-scale fs/4 sine sampled at 45°: every sample is ±0.707
        // (−3 dBFS) but the waveform reaches 1.0 between them.
        let pairs: Vec<f32> = (0..4800)
            .flat_map(|n| {
                let x =
                    (std::f32::consts::FRAC_PI_2 * n as f32 + std::f32::consts::FRAC_PI_4).sin();
                [x, x]
            })
            .collect();
        let frame = analyze(&pairs);
        assert!(
            frame.true_peak_db > -1.0 && frame.true_peak_db < 0.5,
            "{frame:?}"
        );
    }

    #[test]
    fn silence_reads_the_floor_not_infinity() {
        let frame = analyze(&vec![0.0; 2 * RATE as usize]);
        assert_eq!(frame.momentary_lufs, METER_FLOOR_DB);
        assert_eq!(frame.short_term_lufs, METER_FLOOR_DB);
        assert_eq!(frame.true_peak_db, METER_FLOOR_DB);
        assert_eq!(frame.correlation, 0.0);
        assert_eq!(frame.balance_db, 0.0);
    }

    #[test]
    fn points_are_thinned_to_at_most_512_pairs() {
        let tone = sine(440.0, 0.5);
        let frame = analyze(&stereo(0.1, |t| (tone(t), tone(t))));
        assert!(frame.points.len() <= 2 * MAX_POINTS && frame.points.len() > MAX_POINTS);
        assert_eq!(frame.points.len() % 2, 0);
    }

    #[test]
    fn reader_reads_only_new_pairs_and_resets_after_a_gap() {
        let buf = AudioVisualizerBuffer::with_capacity(64);
        let mut analyzer = ScopeAnalyzer::new();
        buf.push(&[0.5; 8]);
        // The first frame starts reading from now.
        assert!(analyzer.frame(&buf, RATE, 0).points.is_empty());
        buf.push(&[0.25, -0.25, 0.25, -0.25]);
        assert_eq!(
            analyzer.frame(&buf, RATE, 0).points,
            vec![0.25, -0.25, 0.25, -0.25]
        );
        // More than the ring holds arrived: a gap, so nothing stale is shown.
        buf.push(&[0.1; 80]);
        let after_gap = analyzer.frame(&buf, RATE, 0);
        assert!(after_gap.points.is_empty());
        assert!(after_gap.restarted);
    }

    #[test]
    fn a_new_epoch_restarts_the_meters_once() {
        let buf = AudioVisualizerBuffer::with_capacity(1 << 16);
        let mut analyzer = ScopeAnalyzer::new();
        assert!(
            analyzer.frame(&buf, RATE, 0).restarted,
            "the first frame starts afresh"
        );
        let tone: Vec<f32> = stereo(0.5, |t| (sine(440.0, 0.5)(t), sine(440.0, 0.5)(t)));
        buf.push(&tone);
        let steady = analyzer.frame(&buf, RATE, 0);
        assert!(!steady.restarted);
        assert!(steady.short_term_lufs > METER_FLOOR_DB);

        let jumped = analyzer.frame(&buf, RATE, 1);
        assert!(jumped.restarted);
        assert_eq!(jumped.short_term_lufs, METER_FLOOR_DB);
        assert!(!analyzer.frame(&buf, RATE, 1).restarted);
    }

    #[test]
    fn every_layout_folds_to_left_and_right() {
        let c = std::f32::consts::FRAC_1_SQRT_2;
        assert_eq!(fold_to_stereo(&[0.3]), (0.3, 0.3));
        assert_eq!(fold_to_stereo(&[0.1, 0.2]), (0.1, 0.2));
        assert_eq!(fold_to_stereo(&[0.1, 0.2, 1.0]), (0.1 + c, 0.2 + c));
        assert_eq!(
            fold_to_stereo(&[0.1, 0.2, 0.7, 0.7]),
            (0.1, 0.2),
            "quad has no centre to fold in"
        );
        assert_eq!(
            fold_to_stereo(&[0.1, 0.2, 1.0, 0.0, 0.5, 0.5]),
            (0.1 + c, 0.2 + c)
        );
    }
}
