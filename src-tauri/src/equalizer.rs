use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub const EQ_BANDS: [f32; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// Number of bands in the parametric equalizer mode.
pub const PARAMETRIC_BAND_COUNT: usize = 20;

/// Default Q for graphic-mode bands (octave-friendly overlap).
const GRAPHIC_Q: f32 = 1.2;

// ---------------------------------------------------------------------------
// Biquad Filter (Peaking EQ)
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

    /// Calculate peaking EQ coefficients using Robert Bristow-Johnson's formulas
    pub fn calculate_coefficients(&mut self, f0: f32, fs: f32, gain_db: f32) {
        self.calculate_coefficients_q(f0, fs, gain_db, GRAPHIC_Q);
    }

    /// Peaking EQ coefficients with an explicit Q (parametric mode).
    pub fn calculate_coefficients_q(&mut self, f0: f32, fs: f32, gain_db: f32, q: f32) {
        // Flat response if gain is zero
        if gain_db.abs() < 0.05 {
            self.b0 = 1.0;
            self.b1 = 0.0;
            self.b2 = 0.0;
            self.a1 = 0.0;
            self.a2 = 0.0;
            return;
        }

        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / fs;
        let alpha = w0.sin() / (2.0 * q);

        let cos_w0 = w0.cos();

        // Peaking EQ coefficients
        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        // Normalize
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
}

// ---------------------------------------------------------------------------
// Equalizer modes & parametric band definition
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqMode {
    Graphic10,
    Parametric20,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParametricBand {
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
}

pub const PARAMETRIC_FREQ_MIN: f32 = 20.0;
pub const PARAMETRIC_FREQ_MAX: f32 = 20000.0;
pub const PARAMETRIC_Q_MIN: f32 = 0.1;
pub const PARAMETRIC_Q_MAX: f32 = 10.0;
const PARAMETRIC_DEFAULT_Q: f32 = 1.1;

/// 20 default center frequencies, log-spaced across the same 31.25 Hz – 16 kHz
/// span as the graphic bands (9 octaves / 19 steps ≈ half-octave spacing).
pub fn default_parametric_bands() -> [ParametricBand; PARAMETRIC_BAND_COUNT] {
    let mut bands = [ParametricBand {
        freq: 0.0,
        gain_db: 0.0,
        q: PARAMETRIC_DEFAULT_Q,
    }; PARAMETRIC_BAND_COUNT];
    let octaves = (16000.0f32 / 31.25).log2(); // = 9 octaves
    for (i, band) in bands.iter_mut().enumerate() {
        let exp = octaves * i as f32 / (PARAMETRIC_BAND_COUNT - 1) as f32;
        band.freq = (31.25 * 2.0f32.powf(exp)).round();
    }
    bands
}

/// Interpolate a 10-band graphic preset (gains defined at the `EQ_BANDS`
/// frequencies) at an arbitrary frequency, in log-frequency space. Values
/// below the first / above the last band clamp to the endpoint gains.
fn interp_preset_gain(gains: &[f32; 10], freq: f32) -> f32 {
    let lf = freq.max(1.0).log2();
    let first = EQ_BANDS[0].log2();
    let last = EQ_BANDS[9].log2();
    if lf <= first {
        return gains[0];
    }
    if lf >= last {
        return gains[9];
    }
    for i in 0..9 {
        let f0 = EQ_BANDS[i].log2();
        let f1 = EQ_BANDS[i + 1].log2();
        if lf >= f0 && lf <= f1 {
            let t = (lf - f0) / (f1 - f0);
            return gains[i] + t * (gains[i + 1] - gains[i]);
        }
    }
    gains[9]
}

// ---------------------------------------------------------------------------
// Equalizer — 10-band graphic or 20-band parametric cascade
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct Equalizer {
    pub enabled: bool,
    pub mode: EqMode,
    pub gains: [f32; 10], // graphic dB gains per band (-12.0 to +12.0)
    pub preamp: f32,      // Pre-amp gain (-12.0 to +12.0), shared by both modes
    pub parametric: [ParametricBand; PARAMETRIC_BAND_COUNT],
    channels: usize,
    channel_filters: Vec<Vec<BiquadFilter>>, // graphic cascade, per channel
    parametric_filters: Vec<Vec<BiquadFilter>>, // parametric cascade, per channel
    sample_rate: u32,
}

impl Default for Equalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Equalizer {
    pub fn new() -> Self {
        Self {
            enabled: false,
            mode: EqMode::Graphic10,
            gains: [0.0; 10],
            preamp: 0.0,
            parametric: default_parametric_bands(),
            channels: 2,
            channel_filters: vec![vec![BiquadFilter::new(); 10]; 2],
            parametric_filters: vec![vec![BiquadFilter::new(); PARAMETRIC_BAND_COUNT]; 2],
            sample_rate: 44100,
        }
    }

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
                vec![vec![BiquadFilter::new(); PARAMETRIC_BAND_COUNT]; channels];
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
                vec![vec![BiquadFilter::new(); PARAMETRIC_BAND_COUNT]; self.channels];
            self.recalculate();
        }
    }

    pub fn set_gain(&mut self, band_idx: usize, gain_db: f32) {
        if band_idx < 10 {
            self.gains[band_idx] = gain_db.clamp(-12.0, 12.0);
            self.recalculate_band(band_idx);
        }
    }

    /// Update a parametric band's gain and Q. The center frequency is fixed
    /// (set from `default_parametric_bands`) and is not user-adjustable.
    pub fn set_parametric_band(&mut self, band_idx: usize, gain_db: f32, q: f32) {
        if band_idx < PARAMETRIC_BAND_COUNT {
            self.parametric[band_idx].gain_db = gain_db.clamp(-12.0, 12.0);
            self.parametric[band_idx].q = q.clamp(PARAMETRIC_Q_MIN, PARAMETRIC_Q_MAX);
            self.recalculate_parametric_band(band_idx);
        }
    }

    pub fn set_preamp(&mut self, preamp_db: f32) {
        self.preamp = preamp_db.clamp(-12.0, 12.0);
    }

    pub fn load_preset(&mut self, gains: [f32; 10]) {
        self.gains = gains;
        self.recalculate();
    }

    /// Apply a 10-band graphic preset to the parametric bands by interpolating
    /// the preset curve (in log-frequency space) at each parametric band's
    /// center frequency, resetting Q to the default. Lets the parametric mode
    /// reuse the same named presets as the graphic mode.
    pub fn load_preset_into_parametric(&mut self, gains: [f32; 10]) {
        for band in self.parametric.iter_mut() {
            band.gain_db = interp_preset_gain(&gains, band.freq).clamp(-12.0, 12.0);
            band.q = PARAMETRIC_DEFAULT_Q;
        }
        self.recalculate();
    }

    pub fn load_parametric(&mut self, bands: [ParametricBand; PARAMETRIC_BAND_COUNT]) {
        for (idx, band) in bands.iter().enumerate() {
            self.parametric[idx] = ParametricBand {
                freq: band.freq.clamp(PARAMETRIC_FREQ_MIN, PARAMETRIC_FREQ_MAX),
                gain_db: band.gain_db.clamp(-12.0, 12.0),
                q: band.q.clamp(PARAMETRIC_Q_MIN, PARAMETRIC_Q_MAX),
            };
        }
        self.recalculate();
    }

    pub fn recalculate(&mut self) {
        for idx in 0..10 {
            self.recalculate_band(idx);
        }
        for idx in 0..PARAMETRIC_BAND_COUNT {
            self.recalculate_parametric_band(idx);
        }
    }

    fn recalculate_band(&mut self, idx: usize) {
        let f0 = EQ_BANDS[idx];
        let gain_db = self.gains[idx];
        let fs = self.sample_rate as f32;

        for ch in 0..self.channels {
            if let Some(filters) = self.channel_filters.get_mut(ch) {
                filters[idx].calculate_coefficients(f0, fs, gain_db);
            }
        }
    }

    fn recalculate_parametric_band(&mut self, idx: usize) {
        let band = self.parametric[idx];
        let fs = self.sample_rate as f32;

        for ch in 0..self.channels {
            if let Some(filters) = self.parametric_filters.get_mut(ch) {
                filters[idx].calculate_coefficients_q(band.freq, fs, band.gain_db, band.q);
            }
        }
    }

    pub fn process_interleaved(&mut self, output: &mut [f32]) {
        if !self.enabled {
            return;
        }

        let preamp_linear = 10.0f32.powf(self.preamp / 20.0);
        let filters = match self.mode {
            EqMode::Graphic10 => &mut self.channel_filters,
            EqMode::Parametric20 => &mut self.parametric_filters,
        };

        for (i, sample) in output.iter_mut().enumerate() {
            let ch = i % self.channels;
            let mut out = *sample * preamp_linear;

            if let Some(filters) = filters.get_mut(ch) {
                for filter in filters {
                    out = filter.process(out);
                }
            }

            // Clip prevention/limiting
            *sample = out.clamp(-1.0, 1.0);
        }
    }
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

    #[test]
    fn default_parametric_bands_span_graphic_range() {
        let bands = default_parametric_bands();
        assert_eq!(bands.len(), PARAMETRIC_BAND_COUNT);
        assert!((bands[0].freq - 31.0).abs() < 2.0);
        assert!((bands[19].freq - 16000.0).abs() < 50.0);
        // Strictly ascending
        for pair in bands.windows(2) {
            assert!(pair[1].freq > pair[0].freq);
        }
    }

    #[test]
    fn parametric_zero_gain_is_passthrough() {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        eq.enabled = true;
        eq.set_mode(EqMode::Parametric20);

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
        eq.set_mode(EqMode::Parametric20);
        // Boost band 10 and probe at its fixed center frequency.
        let center = eq.parametric[10].freq;
        eq.set_parametric_band(10, 9.0, 2.0);

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
        let fixed_freq = eq.parametric[0].freq;
        eq.set_parametric_band(0, 40.0, 100.0);
        let band = eq.parametric[0];
        // Frequency is fixed — gain and Q clamp to their limits.
        assert_eq!(band.freq, fixed_freq);
        assert_eq!(band.gain_db, 12.0);
        assert_eq!(band.q, PARAMETRIC_Q_MAX);
    }

    #[test]
    fn preset_maps_onto_parametric_bands() {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        // Bass-boost-ish preset: strong low end, flat top.
        let gains = [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        eq.load_preset_into_parametric(gains);
        // Lowest parametric band (~31 Hz) should track the preset's low gain,
        // and the highest (~16 kHz) should sit at the flat top.
        assert!(eq.parametric[0].gain_db > 4.0);
        assert!(eq.parametric[19].gain_db.abs() < 0.5);
        // Q reset to default on every band.
        for band in eq.parametric.iter() {
            assert!((band.q - 1.1).abs() < 1e-4);
        }
    }

    #[test]
    fn interp_preset_gain_clamps_and_interpolates() {
        let gains = [3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 9.0];
        // Below first band clamps to gains[0]
        assert_eq!(interp_preset_gain(&gains, 10.0), 3.0);
        // Above last band clamps to gains[9]
        assert_eq!(interp_preset_gain(&gains, 20000.0), 9.0);
        // Midpoint between 8 kHz (3.0) and 16 kHz (9.0) in log space ≈ 6.0
        let mid = interp_preset_gain(&gains, (8000.0f32 * 16000.0).sqrt());
        assert!((mid - 6.0).abs() < 0.2);
    }

    #[test]
    fn mode_switch_keeps_graphic_settings() {
        let mut eq = Equalizer::new();
        eq.update_format(44100, 2);
        eq.set_gain(3, 6.0);
        eq.set_mode(EqMode::Parametric20);
        eq.set_mode(EqMode::Graphic10);
        assert_eq!(eq.gains[3], 6.0);
        assert_eq!(eq.mode, EqMode::Graphic10);
    }
}
