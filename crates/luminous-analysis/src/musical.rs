//! Tempo (BPM) and musical key estimation from decoded mono PCM (#1943).
//!
//! The whole module is one deep entry point, [`analyze_musical_properties`]: hand it mono samples
//! and get back whatever could be estimated with confidence. Everything else (onset envelope,
//! autocorrelation, chromagram, key profiles) is private. Speech, ambient and noisy material, or
//! recordings too short to judge, produce `None` for the affected field rather than a guess, so
//! callers have no error case to handle and never persist a made-up value.

use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;

/// What could be estimated for one track. `None` means "not confident", never "failed".
#[derive(Debug, Clone, PartialEq)]
pub struct MusicalAnalysis {
    /// Average tempo in beats per minute, rounded to 0.1.
    pub bpm: Option<f32>,
    /// Global key (tonic and mode).
    pub key: Option<KeySignature>,
}

/// A major or minor key. Only the tonic and mode are stored; the standard (`Am`, `C#m`) and Camelot
/// (`8A`) spellings are derived, so the two can never disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySignature {
    tonic: u8,
    minor: bool,
}

const MAJOR_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];
const MINOR_NAMES: [&str; 12] = [
    "Cm", "C#m", "Dm", "Ebm", "Em", "Fm", "F#m", "Gm", "G#m", "Am", "Bbm", "Bm",
];
/// Every accepted letter spelling of each pitch class (sharp and flat), used when parsing and when
/// expanding a key to the spellings a tag might use.
const PITCH_SPELLINGS: [&[&str]; 12] = [
    &["C", "B#"],
    &["C#", "Db"],
    &["D"],
    &["D#", "Eb"],
    &["E", "Fb"],
    &["F", "E#"],
    &["F#", "Gb"],
    &["G"],
    &["G#", "Ab"],
    &["A"],
    &["A#", "Bb"],
    &["B", "Cb"],
];

impl KeySignature {
    /// `tonic` is a pitch class with C = 0 (taken modulo 12).
    pub fn new(tonic: u8, minor: bool) -> Self {
        Self {
            tonic: tonic % 12,
            minor,
        }
    }

    pub fn tonic(&self) -> u8 {
        self.tonic
    }

    pub fn is_minor(&self) -> bool {
        self.minor
    }

    /// Standard notation as written in tags: `C`, `Eb`, `Am`, `C#m`.
    pub fn standard(&self) -> String {
        let names = if self.minor {
            &MINOR_NAMES
        } else {
            &MAJOR_NAMES
        };
        names[self.tonic as usize].to_string()
    }

    /// Camelot wheel code: `8B` for C major, `8A` for A minor.
    pub fn camelot(&self) -> String {
        let relative_major = if self.minor {
            (self.tonic + 3) % 12
        } else {
            self.tonic
        };
        let number = (8 + 7 * relative_major as u32) % 12;
        let number = if number == 0 { 12 } else { number };
        format!("{}{}", number, if self.minor { 'A' } else { 'B' })
    }

    /// Parses a tag value in any common spelling: `Am`, `A minor`, `Amin`, `Db`, `C# major`, or a
    /// Camelot code (`8A`, `12b`). Returns `None` for anything else (including empty text).
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if t.is_empty() {
            return None;
        }
        if let Some(key) = Self::parse_camelot(t) {
            return Some(key);
        }
        let mut chars = t.chars();
        let letter = chars.next()?.to_ascii_uppercase();
        if !('A'..='G').contains(&letter) {
            return None;
        }
        let rest = chars.as_str();
        let (accidental, mode) = match rest.chars().next() {
            Some('#') | Some('♯') => ("#", &rest[rest.chars().next()?.len_utf8()..]),
            Some('b') | Some('♭') => ("b", &rest[rest.chars().next()?.len_utf8()..]),
            _ => ("", rest),
        };
        let minor = match mode.trim().to_ascii_lowercase().as_str() {
            "" | "maj" | "major" => false,
            "m" | "min" | "minor" => true,
            _ => return None,
        };
        let name = format!("{letter}{accidental}");
        let tonic = PITCH_SPELLINGS
            .iter()
            .position(|names| names.contains(&name.as_str()))?;
        Some(Self::new(tonic as u8, minor))
    }

    fn parse_camelot(t: &str) -> Option<Self> {
        let (digits, letter) = t.split_at(t.len().checked_sub(1)?);
        let number: u32 = digits.parse().ok()?;
        if !(1..=12).contains(&number) {
            return None;
        }
        let minor = match letter {
            "A" | "a" => true,
            "B" | "b" => false,
            _ => return None,
        };
        // Inverse of `camelot()`: relative major tonic = (number - 8) * 7 mod 12 (7 is self-inverse mod 12).
        let relative_major = ((number + 12 - 8) * 7) % 12;
        let tonic = if minor {
            (relative_major + 9) % 12
        } else {
            relative_major
        };
        Some(Self::new(tonic as u8, minor))
    }

    /// Every text a tag could plausibly use for this key (standard, enharmonic, long-form and
    /// Camelot), for matching against free-text `initial_key` values.
    pub fn spellings(&self) -> Vec<String> {
        let mut out = vec![self.camelot()];
        for name in PITCH_SPELLINGS[self.tonic as usize] {
            if self.minor {
                out.extend([
                    format!("{name}m"),
                    format!("{name}min"),
                    format!("{name} minor"),
                ]);
            } else {
                out.extend([
                    name.to_string(),
                    format!("{name}maj"),
                    format!("{name} major"),
                ]);
            }
        }
        out
    }
}

/// Estimates tempo and key from mono PCM. Pure computation: no I/O, no shared state.
pub fn analyze_musical_properties(pcm: &[f32], sample_rate: u32) -> MusicalAnalysis {
    MusicalAnalysis {
        bpm: estimate_bpm(pcm, sample_rate),
        key: estimate_key(pcm, sample_rate),
    }
}

/// Shortest recording either estimator will judge; anything shorter is too little evidence.
const MIN_SECONDS: usize = 8;
/// Longest excerpt analysed (taken from the middle of the track): bounds the cost for long files
/// without hurting accuracy, since tempo and key are stable properties of a track.
const MAX_SECONDS: usize = 120;

fn middle_excerpt(pcm: &[f32], sample_rate: u32) -> Option<&[f32]> {
    let sr = sample_rate as usize;
    if sr == 0 || pcm.len() < MIN_SECONDS * sr {
        return None;
    }
    let max = MAX_SECONDS * sr;
    if pcm.len() <= max {
        return Some(pcm);
    }
    let start = (pcm.len() - max) / 2;
    Some(&pcm[start..start + max])
}

fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / n as f32).cos())
        .collect()
}

const BPM_MIN: f32 = 60.0;
const BPM_MAX: f32 = 200.0;
/// Candidate tempos are scored every quarter BPM.
const BPM_STEP: f32 = 0.25;
/// Number of beat multiples summed when scoring a candidate tempo.
const BEAT_HARMONICS: usize = 4;
/// Minimum mean normalised autocorrelation at the winning tempo's beat multiples.
const MIN_PERIODICITY: f32 = 0.12;
/// The winning tempo must beat the average candidate by this factor.
const MIN_PEAK_RATIO: f32 = 2.0;

fn estimate_bpm(pcm: &[f32], sample_rate: u32) -> Option<f32> {
    let pcm = middle_excerpt(pcm, sample_rate)?;
    let envelope = onset_envelope(pcm, sample_rate)?;
    let fps = envelope.frame_rate;
    let onset = &envelope.values;

    let max_lag = (BEAT_HARMONICS as f32 * fps * 60.0 / BPM_MIN).ceil() as usize + 2;
    if onset.len() <= max_lag * 2 {
        return None;
    }
    let acf = autocorrelation(onset, max_lag)?;

    let at = |lag: f32| -> f32 {
        let i = lag.floor() as usize;
        let frac = lag - i as f32;
        acf[i] * (1.0 - frac) + acf[i + 1] * frac
    };

    let mut best: Option<(f32, f32, f32)> = None; // (weighted score, bpm, raw score)
    let mut raw_sum = 0.0f32;
    let mut count = 0.0f32;
    let mut bpm = BPM_MIN;
    while bpm <= BPM_MAX {
        let lag = fps * 60.0 / bpm;
        let raw = (1..=BEAT_HARMONICS)
            .map(|k| at(lag * k as f32))
            .sum::<f32>()
            / BEAT_HARMONICS as f32;
        let octaves_from_centre = (bpm / 120.0).log2() / 0.8;
        let weighted = raw * (-0.5 * octaves_from_centre * octaves_from_centre).exp();
        raw_sum += raw.max(0.0);
        count += 1.0;
        if best.is_none_or(|(w, _, _)| weighted > w) {
            best = Some((weighted, bpm, raw));
        }
        bpm += BPM_STEP;
    }

    let (_, bpm, raw) = best?;
    let mean_raw = raw_sum / count;
    if raw < MIN_PERIODICITY || raw < MIN_PEAK_RATIO * mean_raw {
        return None;
    }
    // Many tracks are programmed at a whole BPM; snap when the estimate is already that close.
    let snapped = bpm.round();
    let bpm = if (bpm - snapped).abs() <= 0.3 {
        snapped
    } else {
        (bpm * 10.0).round() / 10.0
    };
    Some(bpm)
}

struct OnsetEnvelope {
    values: Vec<f32>,
    frame_rate: f32,
}

/// Half-wave-rectified, log-compressed spectral flux, with its slow trend removed.
fn onset_envelope(pcm: &[f32], sample_rate: u32) -> Option<OnsetEnvelope> {
    let hop = ((sample_rate as f32 * 0.0116) as usize).max(64);
    let win = (hop * 2).next_power_of_two();
    if pcm.len() < win * 4 {
        return None;
    }
    let window = hann(win);
    let fft = FftPlanner::new().plan_fft_forward(win);
    let bins = win / 2;
    let mut buf = vec![Complex::new(0.0f32, 0.0); win];
    let mut prev = vec![0.0f32; bins];
    let mut flux = Vec::with_capacity(pcm.len() / hop);

    let mut start = 0;
    while start + win <= pcm.len() {
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = Complex::new(pcm[start + i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        let mut sum = 0.0f32;
        for (k, p) in prev.iter_mut().enumerate() {
            let mag = (1.0 + 10.0 * buf[k].norm()).ln();
            sum += (mag - *p).max(0.0);
            *p = mag;
        }
        flux.push(sum);
        start += hop;
    }
    // The first frame has no predecessor and would read as one huge onset.
    flux.remove(0);

    let frame_rate = sample_rate as f32 / hop as f32;
    let half = (frame_rate * 0.25) as usize; // ±0.25 s moving average
    let n = flux.len();
    let mut prefix = vec![0.0f32; n + 1];
    for i in 0..n {
        prefix[i + 1] = prefix[i] + flux[i];
    }
    let values = (0..n)
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(n);
            let mean = (prefix[hi] - prefix[lo]) / (hi - lo) as f32;
            (flux[i] - mean).max(0.0)
        })
        .collect();
    Some(OnsetEnvelope { values, frame_rate })
}

/// Autocorrelation normalised so lag 0 is 1. `None` for a silent (all-zero) envelope.
fn autocorrelation(x: &[f32], max_lag: usize) -> Option<Vec<f32>> {
    let n = x.len();
    let zero_lag: f32 = x.iter().map(|v| v * v).sum::<f32>() / n as f32;
    if zero_lag < 1e-9 {
        return None;
    }
    Some(
        (0..=max_lag)
            .map(|lag| {
                let sum: f32 = x[..n - lag].iter().zip(&x[lag..]).map(|(a, b)| a * b).sum();
                sum / (n - lag) as f32 / zero_lag
            })
            .collect(),
    )
}

/// Krumhansl-Schmuckler probe-tone profiles, indexed by semitones above the tonic.
const KS_MAJOR: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const KS_MINOR: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];
/// Minimum correlation between the track's chroma and the best key profile.
const MIN_KEY_CORRELATION: f32 = 0.6;
/// The best key must lead the runner-up by this much, or the key is ambiguous.
const MIN_KEY_MARGIN: f32 = 0.02;

fn estimate_key(pcm: &[f32], sample_rate: u32) -> Option<KeySignature> {
    let pcm = middle_excerpt(pcm, sample_rate)?;
    let chroma = chromagram(pcm, sample_rate)?;

    let mut scored: Vec<(f32, KeySignature)> = Vec::with_capacity(24);
    for tonic in 0..12u8 {
        for minor in [false, true] {
            let profile = if minor { &KS_MINOR } else { &KS_MAJOR };
            let rotated: Vec<f32> = (0..12)
                .map(|pc| profile[(pc + 12 - tonic as usize) % 12])
                .collect();
            scored.push((pearson(&chroma, &rotated), KeySignature::new(tonic, minor)));
        }
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (best, key) = scored[0];
    let runner_up = scored[1].0;
    (best >= MIN_KEY_CORRELATION && best - runner_up >= MIN_KEY_MARGIN).then_some(key)
}

/// Time-averaged pitch-class energy (C = 0). Each frame is normalised before it is added, so loud
/// passages do not drown out quiet ones. `None` if the recording is effectively silent.
fn chromagram(pcm: &[f32], sample_rate: u32) -> Option<[f32; 12]> {
    let win = ((sample_rate as f32 * 0.19) as usize)
        .next_power_of_two()
        .clamp(2048, 16384);
    if pcm.len() < win * 4 {
        return None;
    }
    let hop = win / 2;
    let window = hann(win);
    let fft = FftPlanner::new().plan_fft_forward(win);
    let bin_hz = sample_rate as f32 / win as f32;

    // Pitch class of every FFT bin in the musical range (None outside it).
    let pitch_class: Vec<Option<usize>> = (0..win / 2)
        .map(|k| {
            let f = k as f32 * bin_hz;
            (100.0..=5000.0)
                .contains(&f)
                .then(|| ((12.0 * (f / 440.0).log2()).round() as i32 + 9).rem_euclid(12) as usize)
        })
        .collect();

    let mut buf = vec![Complex::new(0.0f32, 0.0); win];
    let mut total = [0.0f32; 12];
    let mut start = 0;
    while start + win <= pcm.len() {
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = Complex::new(pcm[start + i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        let mut frame = [0.0f32; 12];
        for (k, pc) in pitch_class.iter().enumerate() {
            if let Some(pc) = pc {
                frame[*pc] += buf[k].norm();
            }
        }
        let energy: f32 = frame.iter().sum();
        if energy > 1e-3 {
            for (t, f) in total.iter_mut().zip(frame) {
                *t += f / energy;
            }
        }
        start += hop;
    }
    (total.iter().sum::<f32>() > 0.0).then_some(total)
}

fn pearson(a: &[f32; 12], b: &[f32]) -> f32 {
    let mean_a = a.iter().sum::<f32>() / 12.0;
    let mean_b = b.iter().sum::<f32>() / 12.0;
    let (mut cov, mut var_a, mut var_b) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        cov += (x - mean_a) * (y - mean_b);
        var_a += (x - mean_a).powi(2);
        var_b += (y - mean_b).powi(2);
    }
    let denom = (var_a * var_b).sqrt();
    if denom < 1e-9 {
        0.0
    } else {
        cov / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 22_050;

    /// A decaying 1 kHz click on every beat, plus a quieter off-beat tick, for `seconds`.
    fn click_track(bpm: f32, seconds: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; SR as usize * seconds];
        let beat = SR as f32 * 60.0 / bpm;
        let mut n = 0u32;
        loop {
            for (offset, amp) in [(0.0, 0.8), (0.5, 0.25)] {
                let start = ((n as f32 + offset) * beat) as usize;
                if start >= out.len() {
                    return out;
                }
                for i in 0..(SR as usize / 25).min(out.len() - start) {
                    let t = i as f32 / SR as f32;
                    out[start + i] += amp * (-t / 0.006).exp() * (2.0 * PI * 1000.0 * t).sin();
                }
            }
            n += 1;
        }
    }

    /// Plays `chords` (each a set of pitch classes) in turn, each note with three harmonics.
    fn chord_progression(chords: &[&[u8]], seconds: usize) -> Vec<f32> {
        let total = SR as usize * seconds;
        let per_chord = total / chords.len();
        let mut out = vec![0.0f32; total];
        for (c, chord) in chords.iter().enumerate() {
            for &pc in *chord {
                // Pitch class (C = 0) in the octave starting at C3.
                let f0 = 130.81 * 2f32.powf(pc as f32 / 12.0);
                for h in 1..=3 {
                    let amp = 0.2 / h as f32;
                    for i in 0..per_chord {
                        let t = i as f32 / SR as f32;
                        out[c * per_chord + i] += amp * (2.0 * PI * f0 * h as f32 * t).sin();
                    }
                }
            }
        }
        out
    }

    fn noise(len: usize) -> Vec<f32> {
        // Deterministic LCG so the test never flakes.
        let mut state = 0x1234_5678u32;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
            })
            .collect()
    }

    #[test]
    fn detects_120_bpm() {
        let bpm = estimate_bpm(&click_track(120.0, 30), SR).expect("tempo");
        assert!((bpm - 120.0).abs() <= 1.0, "got {bpm}");
    }

    #[test]
    fn detects_90_bpm() {
        let bpm = estimate_bpm(&click_track(90.0, 30), SR).expect("tempo");
        assert!((bpm - 90.0).abs() <= 1.0, "got {bpm}");
    }

    #[test]
    fn detects_a_fractional_tempo() {
        let bpm = estimate_bpm(&click_track(133.5, 40), SR).expect("tempo");
        assert!((bpm - 133.5).abs() <= 1.0, "got {bpm}");
    }

    #[test]
    fn detects_c_major() {
        // C-F-G-C
        let pcm = chord_progression(&[&[0, 4, 7], &[5, 9, 0], &[7, 11, 2], &[0, 4, 7]], 24);
        let key = estimate_key(&pcm, SR).expect("key");
        assert_eq!(key, KeySignature::new(0, false), "got {}", key.standard());
        assert_eq!(key.camelot(), "8B");
    }

    #[test]
    fn detects_a_minor() {
        // Am-Dm-Em-Am
        let pcm = chord_progression(&[&[9, 0, 4], &[2, 5, 9], &[4, 7, 11], &[9, 0, 4]], 24);
        let key = estimate_key(&pcm, SR).expect("key");
        assert_eq!(key, KeySignature::new(9, true), "got {}", key.standard());
        assert_eq!(key.standard(), "Am");
        assert_eq!(key.camelot(), "8A");
    }

    #[test]
    fn detects_a_transposed_key() {
        // E-A-B-E: E major (12B)
        let pcm = chord_progression(&[&[4, 8, 11], &[9, 1, 4], &[11, 3, 6], &[4, 8, 11]], 24);
        let key = estimate_key(&pcm, SR).expect("key");
        assert_eq!(key.standard(), "E", "got {}", key.standard());
        assert_eq!(key.camelot(), "12B");
    }

    #[test]
    fn silence_has_no_tempo_or_key() {
        let analysis = analyze_musical_properties(&vec![0.0; SR as usize * 20], SR);
        assert_eq!(
            analysis,
            MusicalAnalysis {
                bpm: None,
                key: None
            }
        );
    }

    #[test]
    fn noise_has_no_tempo_or_key() {
        let analysis = analyze_musical_properties(&noise(SR as usize * 20), SR);
        assert_eq!(
            analysis,
            MusicalAnalysis {
                bpm: None,
                key: None
            }
        );
    }

    #[test]
    fn too_short_to_judge_is_none() {
        let analysis = analyze_musical_properties(&click_track(120.0, 3), SR);
        assert_eq!(
            analysis,
            MusicalAnalysis {
                bpm: None,
                key: None
            }
        );
    }

    #[test]
    fn camelot_matches_the_wheel() {
        let cases = [
            ("C", "8B"),
            ("G", "9B"),
            ("D", "10B"),
            ("A", "11B"),
            ("E", "12B"),
            ("B", "1B"),
            ("F#", "2B"),
            ("Db", "3B"),
            ("Ab", "4B"),
            ("Eb", "5B"),
            ("Bb", "6B"),
            ("F", "7B"),
            ("Am", "8A"),
            ("Em", "9A"),
            ("Bm", "10A"),
            ("F#m", "11A"),
            ("C#m", "12A"),
            ("G#m", "1A"),
            ("Ebm", "2A"),
            ("Bbm", "3A"),
            ("Fm", "4A"),
            ("Cm", "5A"),
            ("Gm", "6A"),
            ("Dm", "7A"),
        ];
        for (standard, camelot) in cases {
            let key = KeySignature::parse(standard).expect(standard);
            assert_eq!(key.camelot(), camelot, "{standard}");
            assert_eq!(KeySignature::parse(camelot), Some(key), "{camelot}");
            assert_eq!(key.standard(), standard);
        }
    }

    #[test]
    fn parse_accepts_common_spellings() {
        let am = KeySignature::new(9, true);
        for text in ["Am", "A minor", "a min", "8A", "8a", " Am "] {
            assert_eq!(KeySignature::parse(text), Some(am), "{text}");
        }
        let db = KeySignature::new(1, false);
        for text in ["Db", "C#", "C# major", "Dbmaj", "3B"] {
            assert_eq!(KeySignature::parse(text), Some(db), "{text}");
        }
        assert_eq!(
            KeySignature::parse("Bb"),
            Some(KeySignature::new(10, false))
        );
        assert_eq!(KeySignature::parse("B"), Some(KeySignature::new(11, false)));
        for text in ["", "H", "13A", "0B", "Am7", "hello"] {
            assert_eq!(KeySignature::parse(text), None, "{text}");
        }
    }

    #[test]
    fn spellings_round_trip_through_parse() {
        for tonic in 0..12 {
            for minor in [false, true] {
                let key = KeySignature::new(tonic, minor);
                for spelling in key.spellings() {
                    assert_eq!(KeySignature::parse(&spelling), Some(key), "{spelling}");
                }
            }
        }
    }
}
