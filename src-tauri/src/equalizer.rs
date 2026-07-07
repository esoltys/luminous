use std::f32::consts::PI;

pub const EQ_BANDS: [f32; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

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
        // Flat response if gain is zero
        if gain_db.abs() < 0.05 {
            self.b0 = 1.0;
            self.b1 = 0.0;
            self.b2 = 0.0;
            self.a1 = 0.0;
            self.a2 = 0.0;
            return;
        }

        let q = 1.2; // Overlap bandwidth factor (octave-friendly)
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
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;

        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;

        y
    }
}

// ---------------------------------------------------------------------------
// Cascaded 10-Band Equalizer
// ---------------------------------------------------------------------------

pub struct Equalizer {
    pub enabled: bool,
    pub gains: [f32; 10], // dB gains per band (-12.0 to +12.0)
    pub preamp: f32,      // Pre-amp gain (-12.0 to +12.0)
    channels: usize,
    channel_filters: Vec<Vec<BiquadFilter>>,
    sample_rate: u32,
}

impl Equalizer {
    pub fn new() -> Self {
        Self {
            enabled: false,
            gains: [0.0; 10],
            preamp: 0.0,
            channels: 2,
            channel_filters: vec![vec![BiquadFilter::new(); 10]; 2],
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
            changed = true;
        }
        if changed {
            self.recalculate();
        }
    }

    pub fn set_gain(&mut self, band_idx: usize, gain_db: f32) {
        if band_idx < 10 {
            self.gains[band_idx] = gain_db.clamp(-12.0, 12.0);
            self.recalculate_band(band_idx);
        }
    }

    pub fn set_preamp(&mut self, preamp_db: f32) {
        self.preamp = preamp_db.clamp(-12.0, 12.0);
    }

    pub fn load_preset(&mut self, gains: [f32; 10]) {
        self.gains = gains;
        self.recalculate();
    }

    pub fn recalculate(&mut self) {
        for idx in 0..10 {
            self.recalculate_band(idx);
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

    pub fn process_interleaved(&mut self, output: &mut [f32]) {
        if !self.enabled {
            return;
        }

        let preamp_linear = 10.0f32.powf(self.preamp / 20.0);

        for (i, sample) in output.iter_mut().enumerate() {
            let ch = i % self.channels;
            let mut out = *sample * preamp_linear;

            if let Some(filters) = self.channel_filters.get_mut(ch) {
                for filter in filters {
                    out = filter.process(out);
                }
            }

            // Clip prevention/limiting
            *sample = out.clamp(-1.0, 1.0);
        }
    }
}
