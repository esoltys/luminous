use crate::models::SettingRange;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub const EQ_BANDS: [f32; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// Default Q for graphic-mode bands. The 10 bands are spaced 1 octave apart
/// (9 octaves / 9 gaps across 31.25 Hz – 16 kHz), and the textbook Q for a
/// constant-Q filter whose -3 dB points meet its neighbors at that spacing is
/// `Q = 1 / (2 * sinh((ln2/2) * BW_octaves))`, which for `BW = 1` octave is
/// `sqrt(2) ≈ 1.414` — the standard value used by octave-band graphic EQs.
const GRAPHIC_Q: f32 = std::f32::consts::SQRT_2;

/// Shelf Q giving the RBJ cookbook's slope `S = 1` shelf — "as steep as it
/// can be without overshoot", a smooth monotonic corner. The graphic mode's
/// edge bands use it, as do default/migrated parametric shelves; a
/// parametric shelf may set any Q (higher overshoots, lower is gentler).
pub const SHELF_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Hard bounds of every EQ value. `Equalizer` clamps with these, and the UI
/// reads them over IPC (`get_audio_setting_ranges().eq`) for its controls'
/// min and max (#1249).
pub const EQ_FREQ_RANGE: SettingRange = SettingRange {
    min: 20.0,
    max: 20000.0,
};
pub const EQ_GAIN_RANGE: SettingRange = SettingRange {
    min: -12.0,
    max: 12.0,
};
pub const EQ_Q_RANGE: SettingRange = SettingRange {
    min: 0.1,
    max: 10.0,
};
pub const EQ_PREAMP_RANGE: SettingRange = SettingRange {
    min: -12.0,
    max: 12.0,
};

/// A parametric layout holds between `PARAMETRIC_MIN_BANDS` and
/// `PARAMETRIC_MAX_BANDS` bands. The cap keeps the audio thread's cascade a
/// fixed, pre-allocated size.
pub const PARAMETRIC_MAX_BANDS: usize = 20;
pub const PARAMETRIC_MIN_BANDS: usize = 1;

/// The EQ bounds in one payload, nested in `AudioSettingRanges`.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct EqualizerRanges {
    pub freq: SettingRange,
    pub gain_db: SettingRange,
    pub q: SettingRange,
    pub preamp: SettingRange,
    pub max_bands: usize,
    pub min_bands: usize,
}

pub const EQUALIZER_RANGES: EqualizerRanges = EqualizerRanges {
    freq: EQ_FREQ_RANGE,
    gain_db: EQ_GAIN_RANGE,
    q: EQ_Q_RANGE,
    preamp: EQ_PREAMP_RANGE,
    max_bands: PARAMETRIC_MAX_BANDS,
    min_bands: PARAMETRIC_MIN_BANDS,
};

/// Which RBJ cookbook filter shape a band uses: a peak (bell that rolls back
/// to 0 dB away from center) or a shelf (holds its gain flat below/above the
/// corner, like a bass/treble control). The same set as Equalizer APO's
/// `PK` / `LSC` / `HSC`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParametricKind {
    #[default]
    Peak,
    LowShelf,
    HighShelf,
}

// ---------------------------------------------------------------------------
// Biquad Filter (Peaking EQ / Shelf, RBJ Audio EQ Cookbook)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct BiquadFilter {
    // Coefficients
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,

    // State memory
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Default for BiquadFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl BiquadFilter {
    pub fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Set this filter's coefficients for the given band shape.
    pub fn calculate_for_kind(
        &mut self,
        kind: ParametricKind,
        f0: f32,
        fs: f32,
        gain_db: f32,
        q: f32,
    ) {
        match kind {
            ParametricKind::Peak => self.calculate_coefficients_q(f0, fs, gain_db, q),
            ParametricKind::LowShelf => self.calculate_low_shelf(f0, fs, gain_db, q),
            ParametricKind::HighShelf => self.calculate_high_shelf(f0, fs, gain_db, q),
        }
    }

    /// Peaking EQ coefficients with an explicit Q.
    pub fn calculate_coefficients_q(&mut self, f0: f32, fs: f32, gain_db: f32, q: f32) {
        if self.bypass_if_flat(gain_db) {
            return;
        }

        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / fs;
        let alpha = w0.sin() / (2.0 * q);

        let cos_w0 = w0.cos();

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        self.normalize(b0, b1, b2, a0, a1, a2);
    }

    /// Low-shelf coefficients (RBJ cookbook, Q form — the Equalizer APO /
    /// AutoEq `LSC` semantics). Holds `gain_db` flat below `f0` instead of
    /// rolling back to 0 dB; `q` sets the corner's steepness (`SHELF_Q` is
    /// the overshoot-free cookbook `S = 1` shelf).
    pub fn calculate_low_shelf(&mut self, f0: f32, fs: f32, gain_db: f32, q: f32) {
        if self.bypass_if_flat(gain_db) {
            return;
        }

        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / fs;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

        self.normalize(b0, b1, b2, a0, a1, a2);
    }

    /// High-shelf coefficients (RBJ cookbook, Q form — `HSC`). Holds
    /// `gain_db` flat above `f0` instead of rolling back to 0 dB.
    pub fn calculate_high_shelf(&mut self, f0: f32, fs: f32, gain_db: f32, q: f32) {
        if self.bypass_if_flat(gain_db) {
            return;
        }

        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / fs;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

        self.normalize(b0, b1, b2, a0, a1, a2);
    }

    /// Identity (pass-through) coefficients — a flat or disabled band.
    fn set_identity(&mut self) {
        self.b0 = 1.0;
        self.b1 = 0.0;
        self.b2 = 0.0;
        self.a1 = 0.0;
        self.a2 = 0.0;
    }

    /// Flat (identity) response if gain is zero — every filter shape
    /// converges to a no-op here, so short-circuiting avoids feeding the
    /// trig/sqrt work a degenerate `A = 1` case. Returns whether it bypassed.
    fn bypass_if_flat(&mut self, gain_db: f32) -> bool {
        if gain_db.abs() < 0.05 {
            self.set_identity();
            true
        } else {
            false
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn normalize(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;

        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;

        y
    }

    /// Magnitude of this filter's frequency response |H(e^jω)| at `freq`,
    /// in dB, evaluated from the coefficients `process` actually runs.
    pub fn magnitude_db(&self, freq: f32, fs: f32) -> f32 {
        let w = 2.0 * std::f64::consts::PI * freq as f64 / fs as f64;
        let (c1, s1) = (w.cos(), w.sin());
        let (c2, s2) = ((2.0 * w).cos(), (2.0 * w).sin());
        let (b0, b1, b2) = (self.b0 as f64, self.b1 as f64, self.b2 as f64);
        let (a1, a2) = (self.a1 as f64, self.a2 as f64);
        // H(z) = (b0 + b1 z^-1 + b2 z^-2) / (1 + a1 z^-1 + a2 z^-2), z = e^jω
        let num_re = b0 + b1 * c1 + b2 * c2;
        let num_im = -(b1 * s1 + b2 * s2);
        let den_re = 1.0 + a1 * c1 + a2 * c2;
        let den_im = -(a1 * s1 + a2 * s2);
        let num = num_re * num_re + num_im * num_im;
        let den = den_re * den_re + den_im * den_im;
        (10.0 * (num / den).log10()) as f32
    }
}

// ---------------------------------------------------------------------------
// Equalizer modes & parametric band definition
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqMode {
    Graphic10,
    /// Stored as `"parametric20"` before #1332; still accepted on load.
    #[serde(alias = "parametric20")]
    Parametric,
}

fn default_true() -> bool {
    true
}

/// One band of the parametric layout. `kind` and `enabled` default so a
/// pre-#1332 `{freq, gain_db, q}` band still parses (as an enabled peak).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParametricBand {
    #[serde(default)]
    pub kind: ParametricKind,
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl ParametricBand {
    /// A flat (0 dB) peak — the neutral band layouts are padded with.
    pub const fn flat_peak(freq: f32) -> Self {
        Self {
            kind: ParametricKind::Peak,
            freq,
            gain_db: 0.0,
            q: PARAMETRIC_DEFAULT_Q,
            enabled: true,
        }
    }

    /// This band with every numeric field inside its `EQ_*_RANGE`.
    pub fn clamped(self) -> Self {
        Self {
            freq: EQ_FREQ_RANGE.clamp(self.freq),
            gain_db: EQ_GAIN_RANGE.clamp(self.gain_db),
            q: EQ_Q_RANGE.clamp(self.q),
            ..self
        }
    }
}

/// The default layout's 20 bands are spaced 9/19 ≈ 0.474 octaves apart (see
/// `default_parametric_bands`). Plugging that into the same Q-vs-bandwidth
/// relation used for `GRAPHIC_Q` gives the "critically spaced" Q whose -3 dB
/// points just meet each neighbor (≈3.03) — rounded to a clean default.
const PARAMETRIC_DEFAULT_Q: f32 = 3.0;
const DEFAULT_LAYOUT_BANDS: usize = 20;

/// The default layout: 20 flat bands log-spaced across the same 31.25 Hz –
/// 16 kHz span as the graphic bands (≈ half-octave spacing), with a low
/// shelf at the bottom, a high shelf at the top and peaks in between.
pub fn default_parametric_bands() -> Vec<ParametricBand> {
    let octaves = (16000.0f32 / 31.25).log2(); // = 9 octaves
    (0..DEFAULT_LAYOUT_BANDS)
        .map(|i| {
            let exp = octaves * i as f32 / (DEFAULT_LAYOUT_BANDS - 1) as f32;
            let mut band = ParametricBand::flat_peak((31.25 * 2.0f32.powf(exp)).round());
            if i == 0 {
                band.kind = ParametricKind::LowShelf;
                band.q = SHELF_Q;
            } else if i == DEFAULT_LAYOUT_BANDS - 1 {
                band.kind = ParametricKind::HighShelf;
                band.q = SHELF_Q;
            }
            band
        })
        .collect()
}

/// Snapshot of the user-adjustable EQ state — the value type crossing the
/// IPC boundary in both directions. The frontend edits a config and applies
/// it whole; the echoed snapshot (post-clamping) is the canonical state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EqualizerConfig {
    pub enabled: bool,
    pub mode: EqMode,
    pub gains: [f32; 10],
    pub preamp: f32,
    pub parametric: Vec<ParametricBand>,
}

impl EqualizerConfig {
    pub fn snapshot(eq: &Equalizer) -> Self {
        Self {
            enabled: eq.enabled,
            mode: eq.mode,
            gains: eq.gains,
            preamp: eq.preamp,
            parametric: eq.parametric_bands().to_vec(),
        }
    }
}

/// Named 10-band graphic presets. The parametric mode has its own sparse
/// versions of the same names (`parametric_preset`). Unknown names fall back
/// to flat.
pub fn preset_gains(name: &str) -> [f32; 10] {
    match name.to_lowercase().as_str() {
        "rock" => [4.0, 3.0, 1.0, -1.0, -2.0, -1.0, 1.0, 3.0, 3.5, 3.5],
        "pop" => [1.5, 2.5, 1.0, -1.0, -0.5, 1.0, 2.5, 3.0, 2.5, 2.0],
        "bass boost" | "bassboost" => [9.0, 7.0, 4.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        "vocal boost" | "vocalboost" => [-3.0, -2.0, -1.0, 0.0, 2.0, 4.0, 4.5, 3.5, 1.0, -1.0],
        "headphones" => [2.0, 1.5, 0.5, 0.0, 0.0, 0.0, -0.5, -1.0, -0.5, 1.0],
        _ => [0.0; 10], // Flat
    }
}

/// Named presets for the parametric mode, written the way a parametric EQ is
/// used: a few broad moves (wide Q, shelves at the edges) rather than every
/// band nudged to trace the 10-band curve. Each entry is
/// `(band index, gain dB, Q)` into `default_parametric_bands`; unlisted bands
/// stay flat, and a Q of 0 keeps the band's default Q.
/// Band centers: 0=31 (low shelf), 2=60, 3=84, 5=161, 7=311, 8=432, 9=600,
/// 11=1.2k, 13=2.2k, 14=3.1k, 15=4.3k, 16=6k, 17=8.3k, 19=16k (high shelf) Hz.
fn parametric_preset_moves(name: &str) -> &'static [(usize, f32, f32)] {
    match name.to_lowercase().as_str() {
        "rock" => &[
            (0, 4.0, 0.0),
            (2, 2.5, 0.7),
            (9, -2.0, 0.7),
            (16, 3.0, 0.6),
            (19, 4.0, 0.0),
        ],
        "pop" => &[
            (2, 2.5, 0.6),
            (7, -1.0, 0.8),
            (14, 2.5, 0.5),
            (19, 2.5, 0.0),
        ],
        "bass boost" | "bassboost" => &[(0, 9.0, 0.0), (2, 4.5, 0.6)],
        "vocal boost" | "vocalboost" => &[
            (0, -2.0, 0.0),
            (3, -2.0, 0.6),
            (12, 4.5, 0.6),
            (19, -2.0, 0.0),
        ],
        "headphones" => &[
            (0, 2.0, 0.0),
            (2, 1.0, 0.7),
            (17, -1.0, 1.2),
            (19, 2.5, 0.0),
        ],
        _ => &[], // Flat
    }
}

/// The full band layout for a named parametric preset (see
/// `parametric_preset_moves`). Unknown names fall back to flat.
pub fn parametric_preset(name: &str) -> Vec<ParametricBand> {
    let mut bands = default_parametric_bands();
    for &(idx, gain_db, q) in parametric_preset_moves(name) {
        bands[idx].gain_db = gain_db;
        if q > 0.0 {
            bands[idx].q = q;
        }
    }
    bands
}

// ---------------------------------------------------------------------------
// Equalizer — 10-band graphic or up-to-20-band parametric cascade
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct Equalizer {
    pub enabled: bool,
    pub mode: EqMode,
    pub gains: [f32; 10], // graphic dB gains per band (EQ_GAIN_RANGE)
    pub preamp: f32,      // Pre-amp gain (EQ_PREAMP_RANGE), shared by both modes
    /// Fixed-size so the audio thread never allocates; only the first
    /// `parametric_len` entries are live.
    parametric: [ParametricBand; PARAMETRIC_MAX_BANDS],
    parametric_len: usize,
    channels: usize,
    channel_filters: Vec<Vec<BiquadFilter>>, // graphic cascade, per channel
    parametric_filters: Vec<Vec<BiquadFilter>>, // parametric cascade (MAX slots), per channel
    sample_rate: u32,
}

impl Default for Equalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Equalizer {
    pub fn new() -> Self {
        let mut eq = Self {
            enabled: false,
            mode: EqMode::Graphic10,
            gains: [0.0; 10],
            preamp: 0.0,
            parametric: [ParametricBand::flat_peak(1000.0); PARAMETRIC_MAX_BANDS],
            parametric_len: 0,
            channels: 2,
            channel_filters: vec![vec![BiquadFilter::new(); 10]; 2],
            parametric_filters: vec![vec![BiquadFilter::new(); PARAMETRIC_MAX_BANDS]; 2],
            sample_rate: 44100,
        };
        eq.load_parametric(&default_parametric_bands());
        eq
    }

    /// The live parametric layout, in the order the user arranged it.
    pub fn parametric_bands(&self) -> &[ParametricBand] {
        &self.parametric[..self.parametric_len]
    }

    /// Re-tune the filter cascade for the output device's actual sample
    /// rate/channel count. Called by `audio.rs`'s `build_output` whenever
    /// the CPAL output stream is (re)built — lazily for the first track a
    /// decode thread ever plays, and again if the output device changes —
    /// not on every track, since the stream (and its format) stays open
    /// across ordinary track changes. A channel-count change reallocates the
    /// whole cascade — new `BiquadFilter`s start with zeroed state memory —
    /// but a sample-rate-only change just recomputes coefficients in place,
    /// leaving each filter's existing `x1/x2/y1/y2` state as-is.
    pub fn update_format(&mut self, sample_rate: u32, channels: usize) {
        let mut changed = false;
        if self.sample_rate != sample_rate {
            self.sample_rate = sample_rate;
            changed = true;
        }
        if self.channels != channels {
            self.channels = channels;
            self.channel_filters = vec![vec![BiquadFilter::new(); 10]; channels];
            self.parametric_filters =
                vec![vec![BiquadFilter::new(); PARAMETRIC_MAX_BANDS]; channels];
            changed = true;
        }
        if changed {
            self.recalculate();
        }
    }

    pub fn set_mode(&mut self, mode: EqMode) {
        if self.mode != mode {
            self.mode = mode;
            // Recreate filters so the newly active cascade starts with clean
            // state memory instead of stale samples from a previous session.
            self.channel_filters = vec![vec![BiquadFilter::new(); 10]; self.channels];
            self.parametric_filters =
                vec![vec![BiquadFilter::new(); PARAMETRIC_MAX_BANDS]; self.channels];
            self.recalculate();
        }
    }

    pub fn set_gain(&mut self, band_idx: usize, gain_db: f32) {
        if band_idx < 10 {
            self.gains[band_idx] = EQ_GAIN_RANGE.clamp(gain_db);
            self.recalculate_band(band_idx);
        }
    }

    pub fn set_preamp(&mut self, preamp_db: f32) {
        self.preamp = EQ_PREAMP_RANGE.clamp(preamp_db);
    }

    /// Apply a whole config in one step, clamping every field. Returns the
    /// canonical post-clamp snapshot.
    pub fn apply(&mut self, config: &EqualizerConfig) -> EqualizerConfig {
        self.enabled = config.enabled;
        self.set_mode(config.mode);
        for (idx, gain_db) in config.gains.iter().enumerate() {
            self.set_gain(idx, *gain_db);
        }
        self.set_preamp(config.preamp);
        self.load_parametric(&config.parametric);
        EqualizerConfig::snapshot(self)
    }

    /// Replace the 10 graphic-mode band gains wholesale (e.g. from a named
    /// preset). The parametric mode loads its own presets via
    /// `load_parametric(&parametric_preset(name))`.
    pub fn load_preset(&mut self, gains: [f32; 10]) {
        self.gains = gains.map(|g| EQ_GAIN_RANGE.clamp(g));
        self.recalculate();
    }

    /// Replace the parametric layout wholesale. Keeps the given band order,
    /// truncates past `PARAMETRIC_MAX_BANDS`, pads an empty list up to
    /// `PARAMETRIC_MIN_BANDS` with a flat peak, and clamps every field.
    pub fn load_parametric(&mut self, bands: &[ParametricBand]) {
        let len = bands
            .len()
            .clamp(PARAMETRIC_MIN_BANDS, PARAMETRIC_MAX_BANDS);
        for idx in 0..PARAMETRIC_MAX_BANDS {
            self.parametric[idx] = match bands.get(idx) {
                Some(band) if idx < len => band.clamped(),
                _ => ParametricBand::flat_peak(1000.0),
            };
        }
        self.parametric_len = len;
        self.recalculate();
    }

    pub fn recalculate(&mut self) {
        for idx in 0..10 {
            self.recalculate_band(idx);
        }
        for idx in 0..PARAMETRIC_MAX_BANDS {
            self.recalculate_parametric_band(idx);
        }
    }

    fn recalculate_band(&mut self, idx: usize) {
        let f0 = EQ_BANDS[idx];
        let gain_db = self.gains[idx];
        let fs = self.sample_rate as f32;
        // The graphic bands' outermost corners shelve (a true bass/treble
        // control) at the overshoot-free `SHELF_Q`; the rest peak.
        let (kind, q) = if idx == 0 {
            (ParametricKind::LowShelf, SHELF_Q)
        } else if idx == EQ_BANDS.len() - 1 {
            (ParametricKind::HighShelf, SHELF_Q)
        } else {
            (ParametricKind::Peak, GRAPHIC_Q)
        };

        for ch in 0..self.channels {
            if let Some(filters) = self.channel_filters.get_mut(ch) {
                filters[idx].calculate_for_kind(kind, f0, fs, gain_db, q);
            }
        }
    }

    fn recalculate_parametric_band(&mut self, idx: usize) {
        let band = self.parametric[idx];
        let live = idx < self.parametric_len && band.enabled;
        let fs = self.sample_rate as f32;

        for ch in 0..self.channels {
            if let Some(filters) = self.parametric_filters.get_mut(ch) {
                if live {
                    filters[idx].calculate_for_kind(band.kind, band.freq, fs, band.gain_db, band.q);
                } else {
                    filters[idx].set_identity();
                }
            }
        }
    }

    /// Combined magnitude response of the parametric cascade (preamp
    /// excluded) at each of `freqs`, in dB — evaluated from the live filter
    /// coefficients at the engine's real sample rate, so the UI's curve
    /// preview shows exactly what `process_interleaved` applies (#1248).
    pub fn parametric_response_db(&self, freqs: &[f32]) -> Vec<f32> {
        let fs = self.sample_rate as f32;
        let nyquist = fs / 2.0;
        let Some(filters) = self.parametric_filters.first() else {
            return vec![0.0; freqs.len()];
        };
        let live = &filters[..self.parametric_len];
        freqs
            .iter()
            .map(|&f| {
                let f = f.clamp(1.0, nyquist * 0.999);
                live.iter().map(|flt| flt.magnitude_db(f, fs)).sum()
            })
            .collect()
    }

    /// Apply preamp + the active band cascade (graphic or parametric,
    /// whichever `mode` selects) to `output` in place, sample by sample —
    /// the EQ stage of the CPAL output callback's per-buffer DSP chain (see
    /// `audio.rs`'s module doc). A no-op when `enabled` is false.
    /// Internal 32-bit float headroom is preserved; downstream limiter in
    /// `audio.rs` enforces the true-peak ceiling before final output.
    pub fn process_interleaved(&mut self, output: &mut [f32]) {
        if !self.enabled {
            return;
        }

        let preamp_linear = 10.0f32.powf(self.preamp / 20.0);
        let (filters, active) = match self.mode {
            EqMode::Graphic10 => (&mut self.channel_filters, EQ_BANDS.len()),
            EqMode::Parametric => (&mut self.parametric_filters, self.parametric_len),
        };

        for (i, sample) in output.iter_mut().enumerate() {
            let ch = i % self.channels;
            let mut out = *sample * preamp_linear;

            if let Some(filters) = filters.get_mut(ch) {
                for filter in filters.iter_mut().take(active) {
                    out = filter.process(out);
                }
            }

            *sample = out;
        }
    }
}

/// Pre-#1332 slope-form (`S = 1`) shelf coefficients, kept only as the
/// reference that proves the Q-form shelves reproduce the old response.
#[cfg(test)]
pub(crate) fn legacy_slope_shelf(high: bool, f0: f32, fs: f32, gain_db: f32) -> BiquadFilter {
    let mut f = BiquadFilter::new();
    if f.bypass_if_flat(gain_db) {
        return f;
    }
    let a = 10.0f32.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * f0 / fs;
    let cos_w0 = w0.cos();
    let slope = 1.0f32;
    let alpha = w0.sin() / 2.0 * ((a + 1.0 / a) * (1.0 / slope - 1.0) + 2.0).sqrt();
    let t = 2.0 * a.sqrt() * alpha;
    if high {
        f.normalize(
            a * ((a + 1.0) + (a - 1.0) * cos_w0 + t),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
            a * ((a + 1.0) + (a - 1.0) * cos_w0 - t),
            (a + 1.0) - (a - 1.0) * cos_w0 + t,
            2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
            (a + 1.0) - (a - 1.0) * cos_w0 - t,
        );
    } else {
        f.normalize(
            a * ((a + 1.0) - (a - 1.0) * cos_w0 + t),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
            a * ((a + 1.0) - (a - 1.0) * cos_w0 - t),
            (a + 1.0) + (a - 1.0) * cos_w0 + t,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
            (a + 1.0) + (a - 1.0) * cos_w0 - t,
        );
    }
    f
}

/// Response (dB) of a pre-#1332 parametric layout — `{freq, gain_db, q}`
/// bands whose first and last entries were slope shelves ignoring `q`.
#[cfg(test)]
pub(crate) fn legacy_parametric_response_db(
    bands: &[(f32, f32, f32)],
    fs: f32,
    freqs: &[f32],
) -> Vec<f32> {
    let last = bands.len() - 1;
    let filters: Vec<BiquadFilter> = bands
        .iter()
        .enumerate()
        .map(|(i, &(freq, gain_db, q))| {
            if i == 0 || i == last {
                legacy_slope_shelf(i == last, freq, fs, gain_db)
            } else {
                let mut f = BiquadFilter::new();
                f.calculate_coefficients_q(freq, fs, gain_db, q);
                f
            }
        })
        .collect();
    freqs
        .iter()
        .map(|&f| filters.iter().map(|flt| flt.magnitude_db(f, fs)).sum())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, sample_rate: f32, frames: usize, channels: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(frames * channels);
        for n in 0..frames {
            let v = (2.0 * PI * freq * n as f32 / sample_rate).sin() * 0.5;
            for _ in 0..channels {
                out.push(v);
            }
        }
        out
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn band(kind: ParametricKind, freq: f32, gain_db: f32, q: f32) -> ParametricBand {
        ParametricBand {
            kind,
            freq,
            gain_db,
            q,
            enabled: true,
        }
    }

    fn log_freqs(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 20.0 * 1000f32.powf(i as f32 / (n - 1) as f32))
            .collect()
    }

    fn parametric_eq_48k() -> Equalizer {
        let mut eq = Equalizer::new();
        eq.update_format(48000, 2);
        eq.enabled = true;
        eq.set_mode(EqMode::Parametric);
        eq
    }

    #[test]
    fn default_parametric_bands_span_graphic_range_with_edge_shelves() {
        let bands = default_parametric_bands();
        assert_eq!(bands.len(), 20);
        assert!((bands[0].freq - 31.0).abs() < 2.0);
        assert!((bands[19].freq - 16000.0).abs() < 50.0);
        for pair in bands.windows(2) {
            assert!(pair[1].freq > pair[0].freq);
        }
        assert_eq!(bands[0].kind, ParametricKind::LowShelf);
        assert_eq!(bands[19].kind, ParametricKind::HighShelf);
        assert!(bands[1..19].iter().all(|b| b.kind == ParametricKind::Peak));
        assert!(bands.iter().all(|b| b.enabled && b.gain_db == 0.0));
    }

    #[test]
    fn legacy_band_json_parses_as_enabled_peak_and_old_mode_name_loads() {
        let bands: Vec<ParametricBand> =
            serde_json::from_str(r#"[{"freq":100.0,"gain_db":3.0,"q":2.0}]"#).unwrap();
        assert_eq!(bands[0], band(ParametricKind::Peak, 100.0, 3.0, 2.0));
        let mode: EqMode = serde_json::from_str(r#""parametric20""#).unwrap();
        assert_eq!(mode, EqMode::Parametric);
        assert_eq!(
            serde_json::to_string(&EqMode::Parametric).unwrap(),
            r#""parametric""#
        );
        assert_eq!(
            serde_json::to_string(&ParametricKind::LowShelf).unwrap(),
            r#""low_shelf""#
        );
    }

    #[test]
    fn parametric_zero_gain_is_passthrough() {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        eq.enabled = true;
        eq.set_mode(EqMode::Parametric);

        let original = sine(1000.0, 44100.0, 1024, 2);
        let mut processed = original.clone();
        eq.process_interleaved(&mut processed);

        for (p, o) in processed.iter().zip(original.iter()) {
            assert!((p - o).abs() < 1e-4, "flat parametric EQ altered samples");
        }
    }

    #[test]
    fn parametric_boost_raises_level_at_center_frequency() {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        eq.enabled = true;
        eq.set_mode(EqMode::Parametric);
        let center = 1234.0;
        eq.load_parametric(&[band(ParametricKind::Peak, center, 9.0, 2.0)]);

        let original = sine(center, 44100.0, 4096, 2);
        let mut processed = original.clone();
        eq.process_interleaved(&mut processed);

        // Skip the filter warm-up region, compare steady-state RMS
        let orig_rms = rms(&original[2048..]);
        let proc_rms = rms(&processed[2048..]);
        assert!(
            proc_rms > orig_rms * 1.5,
            "expected boost at {center} Hz: orig {orig_rms}, processed {proc_rms}"
        );
    }

    #[test]
    fn parametric_band_values_are_clamped() {
        let mut eq = Equalizer::new();
        eq.load_parametric(&[
            band(ParametricKind::Peak, 5.0, 40.0, 100.0),
            band(ParametricKind::HighShelf, 99999.0, -40.0, 0.0),
            band(ParametricKind::Peak, f32::NAN, f32::NAN, f32::NAN),
        ]);
        let got = eq.parametric_bands();
        assert_eq!(
            got[0],
            band(
                ParametricKind::Peak,
                EQ_FREQ_RANGE.min,
                12.0,
                EQ_Q_RANGE.max
            )
        );
        assert_eq!(
            got[1],
            band(
                ParametricKind::HighShelf,
                EQ_FREQ_RANGE.max,
                -12.0,
                EQ_Q_RANGE.min
            )
        );
        assert_eq!(
            got[2],
            band(
                ParametricKind::Peak,
                EQ_FREQ_RANGE.min,
                EQ_GAIN_RANGE.min,
                EQ_Q_RANGE.min
            )
        );
    }

    #[test]
    fn band_count_clamped_to_bounds() {
        let mut eq = Equalizer::new();
        let many: Vec<_> = (0..25)
            .map(|i| band(ParametricKind::Peak, 100.0 + i as f32, 1.0, 1.0))
            .collect();
        eq.load_parametric(&many);
        assert_eq!(eq.parametric_bands(), &many[..PARAMETRIC_MAX_BANDS]);

        eq.load_parametric(&[]);
        assert_eq!(eq.parametric_bands().len(), PARAMETRIC_MIN_BANDS);
        assert_eq!(eq.parametric_bands()[0].gain_db, 0.0);
    }

    #[test]
    fn apply_keeps_band_order_and_echoes_clamped_layout() {
        let mut eq = Equalizer::new();
        let bands = vec![
            band(ParametricKind::Peak, 5000.0, 3.0, 1.0),
            band(ParametricKind::LowShelf, 80.0, 50.0, 0.7),
        ];
        let echoed = eq.apply(&EqualizerConfig {
            enabled: true,
            mode: EqMode::Parametric,
            gains: [0.0; 10],
            preamp: -40.0,
            parametric: bands,
        });
        assert_eq!(echoed.parametric[0].freq, 5000.0);
        assert_eq!(echoed.parametric[1].kind, ParametricKind::LowShelf);
        assert_eq!(echoed.parametric[1].gain_db, 12.0);
        assert_eq!(echoed.preamp, EQ_PREAMP_RANGE.min);
    }

    #[test]
    fn parametric_len_limits_active_filters() {
        let mut eq = parametric_eq_48k();
        eq.load_parametric(&[band(ParametricKind::Peak, 1000.0, 6.0, 1.0); 20]);
        assert!(eq.parametric_response_db(&[1000.0])[0] > 100.0);

        // Shrinking the layout must leave the dropped slots silent — both in
        // the response and in what `process_interleaved` applies.
        eq.load_parametric(&[band(ParametricKind::Peak, 1000.0, 6.0, 1.0)]);
        let predicted = eq.parametric_response_db(&[1000.0])[0];
        assert!((predicted - 6.0).abs() < 0.1, "read {predicted} dB");

        let original = sine(1000.0, 48000.0, 8192, 2);
        let mut processed = original.clone();
        eq.process_interleaved(&mut processed);
        let measured = 20.0 * (rms(&processed[8192..]) / rms(&original[8192..])).log10();
        assert!((measured - 6.0).abs() < 0.2, "measured {measured} dB");
    }

    #[test]
    fn disabled_band_is_passthrough() {
        let mut eq = parametric_eq_48k();
        let mut boost = band(ParametricKind::Peak, 1000.0, 9.0, 1.0);
        boost.enabled = false;
        eq.load_parametric(&[boost]);
        assert!(eq.parametric_bands()[0].gain_db == 9.0, "gain is kept");
        for db in eq.parametric_response_db(&log_freqs(48)) {
            assert!(db.abs() < 1e-4, "disabled band read {db} dB");
        }

        let original = sine(1000.0, 48000.0, 1024, 2);
        let mut processed = original.clone();
        eq.process_interleaved(&mut processed);
        for (p, o) in processed.iter().zip(original.iter()) {
            assert!((p - o).abs() < 1e-5, "disabled band altered samples");
        }
    }

    #[test]
    fn parametric_presets_are_a_few_broad_moves() {
        for name in ["rock", "pop", "bass boost", "vocal boost", "headphones"] {
            let bands = parametric_preset(name);
            let moved: Vec<_> = bands.iter().filter(|b| b.gain_db != 0.0).collect();
            assert!(
                (2..=5).contains(&moved.len()),
                "{name}: {} bands moved",
                moved.len()
            );
            for band in moved {
                assert!(
                    band.q <= 1.2,
                    "{name}: {} Hz has narrow Q {}",
                    band.freq,
                    band.q
                );
            }
        }
        assert!(parametric_preset("flat").iter().all(|b| b.gain_db == 0.0));
    }

    #[test]
    fn parametric_presets_track_their_graphic_counterparts() {
        for name in ["rock", "pop", "bass boost", "vocal boost", "headphones"] {
            let mut eq = parametric_eq_48k();
            eq.load_parametric(&parametric_preset(name));
            let response = eq.parametric_response_db(&EQ_BANDS);
            for ((freq, got), want) in EQ_BANDS.iter().zip(response).zip(preset_gains(name)) {
                assert!(
                    (got - want).abs() <= 2.0,
                    "{name} @ {freq} Hz: parametric {got:.1} dB vs graphic {want:.1} dB"
                );
            }
        }
    }

    #[test]
    fn mode_switch_keeps_graphic_settings() {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        eq.set_gain(3, 6.0);
        eq.set_mode(EqMode::Parametric);
        eq.set_mode(EqMode::Graphic10);
        assert_eq!(eq.gains[3], 6.0);
        assert_eq!(eq.mode, EqMode::Graphic10);
    }

    #[test]
    fn graphic_low_band_shelves_instead_of_peaking() {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        eq.enabled = true;
        eq.set_gain(0, 9.0); // boost the 31.25 Hz band

        // A peaking filter centered at 31.25 Hz would have rolled back
        // toward 0 dB well before 20 Hz; a low shelf holds the boost.
        let probe = sine(20.0, 44100.0, 8192, 2);
        let mut processed = probe.clone();
        eq.process_interleaved(&mut processed);

        let orig_rms = rms(&probe[4096..]);
        let proc_rms = rms(&processed[4096..]);
        assert!(
            proc_rms > orig_rms * 2.0,
            "expected shelf boost to hold below 31.25 Hz: orig {orig_rms}, processed {proc_rms}"
        );
    }

    #[test]
    fn graphic_response_unchanged_by_q_shelf_refactor() {
        // The graphic edge bands moved from the slope-form shelf (S = 1) to
        // the Q-form shelf at SHELF_Q; the two must be the same filter.
        let fs = 48000.0;
        let freqs = log_freqs(64);
        for name in ["rock", "bass boost", "vocal boost", "headphones"] {
            let mut eq = Equalizer::new();
            eq.update_format(48000, 2);
            eq.load_preset(preset_gains(name));
            let gains = preset_gains(name);
            let legacy: Vec<BiquadFilter> = (0..10)
                .map(|i| match i {
                    0 => legacy_slope_shelf(false, EQ_BANDS[0], fs, gains[0]),
                    9 => legacy_slope_shelf(true, EQ_BANDS[9], fs, gains[9]),
                    _ => {
                        let mut f = BiquadFilter::new();
                        f.calculate_coefficients_q(EQ_BANDS[i], fs, gains[i], GRAPHIC_Q);
                        f
                    }
                })
                .collect();
            for &f in &freqs {
                let now: f32 = eq.channel_filters[0]
                    .iter()
                    .map(|flt| flt.magnitude_db(f, fs))
                    .sum();
                let was: f32 = legacy.iter().map(|flt| flt.magnitude_db(f, fs)).sum();
                assert!(
                    (now - was).abs() < 1e-3,
                    "{name} @ {f} Hz: {now} dB now vs {was} dB before"
                );
            }
        }
    }

    #[test]
    fn parametric_high_shelf_holds_boost_above_corner() {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        eq.enabled = true;
        eq.set_mode(EqMode::Parametric);
        eq.load_parametric(&[band(ParametricKind::HighShelf, 16000.0, 9.0, SHELF_Q)]);

        // A peaking filter centered at 16 kHz would have rolled back toward
        // 0 dB well past 20 kHz; a high shelf holds the boost. Probe near
        // Nyquist for a 44.1 kHz stream.
        let probe = sine(21000.0, 44100.0, 8192, 2);
        let mut processed = probe.clone();
        eq.process_interleaved(&mut processed);

        let orig_rms = rms(&probe[4096..]);
        let proc_rms = rms(&processed[4096..]);
        assert!(
            proc_rms > orig_rms * 2.0,
            "expected shelf boost to hold above 16 kHz: orig {orig_rms}, processed {proc_rms}"
        );
    }

    #[test]
    fn response_of_low_shelf_holds_gain_below_corner() {
        let mut eq = parametric_eq_48k();
        eq.load_parametric(&[band(ParametricKind::LowShelf, 31.0, 6.0, SHELF_Q)]);
        let resp = eq.parametric_response_db(&[10.0, 20.0]);
        assert!(resp[0] >= 5.5, "low shelf at 10 Hz read {} dB", resp[0]);
        assert!(resp[1] >= 4.5, "low shelf at 20 Hz read {} dB", resp[1]);
    }

    #[test]
    fn low_shelf_q_changes_transition_steepness() {
        let probes = [10.0, 100.0, 200.0, 400.0];
        let mut eq = parametric_eq_48k();
        eq.load_parametric(&[band(ParametricKind::LowShelf, 100.0, 6.0, 0.4)]);
        let gentle = eq.parametric_response_db(&probes);
        eq.load_parametric(&[band(ParametricKind::LowShelf, 100.0, 6.0, 2.0)]);
        let steep = eq.parametric_response_db(&probes);

        // Both shelves hold the full gain far below the corner and read half
        // of it at the corner...
        for resp in [&gentle, &steep] {
            assert!((resp[0] - 6.0).abs() < 0.3, "far below read {}", resp[0]);
            assert!((resp[1] - 3.0).abs() < 0.1, "corner read {}", resp[1]);
        }
        // ...but a higher Q falls off faster above it (overshooting past 0).
        assert!(
            steep[3] < gentle[3] - 0.5,
            "Q 2 read {} dB vs Q 0.4 {} dB at 400 Hz",
            steep[3],
            gentle[3]
        );
    }

    #[test]
    fn response_of_high_shelf_holds_gain_above_corner() {
        let mut eq = parametric_eq_48k();
        eq.load_parametric(&[band(ParametricKind::HighShelf, 16000.0, 6.0, SHELF_Q)]);
        let resp = eq.parametric_response_db(&[20000.0]);
        assert!(resp[0] >= 5.0, "high shelf at 20 kHz read {} dB", resp[0]);
    }

    #[test]
    fn response_of_peaking_band_reads_gain_at_center_only() {
        let mut eq = parametric_eq_48k();
        let center = 1000.0;
        eq.load_parametric(&[band(ParametricKind::Peak, center, 6.0, 3.0)]);
        let resp = eq.parametric_response_db(&[center, center / 100.0]);
        assert!(
            (resp[0] - 6.0).abs() < 0.1,
            "peak at center read {} dB",
            resp[0]
        );
        assert!(
            resp[1].abs() < 0.1,
            "peak two decades away read {} dB",
            resp[1]
        );
    }

    #[test]
    fn response_of_flat_eq_is_zero_everywhere() {
        let eq = parametric_eq_48k();
        for db in eq.parametric_response_db(&log_freqs(96)) {
            assert!(db.abs() < 1e-3, "flat EQ read {db} dB");
        }
    }

    #[test]
    fn response_matches_measured_gain_through_process() {
        let mut eq = parametric_eq_48k();
        eq.load_parametric(&[
            band(ParametricKind::LowShelf, 31.0, 6.0, 1.0),
            band(ParametricKind::Peak, 432.0, -4.0, 2.0),
        ]);
        let probe = 432.0 * 1.1;
        let predicted = eq.parametric_response_db(&[probe])[0];

        let original = sine(probe, 48000.0, 16384, 2);
        let mut processed = original.clone();
        eq.process_interleaved(&mut processed);
        let measured = 20.0 * (rms(&processed[16384..]) / rms(&original[16384..])).log10();
        assert!(
            (measured - predicted).abs() < 0.2,
            "predicted {predicted} dB, measured {measured} dB at {probe} Hz"
        );
    }
}
