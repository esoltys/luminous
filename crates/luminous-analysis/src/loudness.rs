//! ITU-R BS.1770-4 / EBU R128 integrated loudness analysis and loudness gain law.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Sentinel written when analysis fails (corrupt/unsupported file) so the
/// background worker doesn't retry it forever. Clearly outside any real
/// loudness measurement range, and filtered out by `compute_gain`.
pub const ANALYSIS_FAILED_SENTINEL: f64 = -100.0;

/// Plausible integrated-loudness range for a successfully analyzed track.
/// Anything outside this (including the failure sentinel) is treated as "no
/// usable measurement" rather than fed into the gain calculation.
pub const PLAUSIBLE_LUFS_RANGE: std::ops::RangeInclusive<f64> = -70.0..=0.0;

/// Which ReplayGain value to prefer when no R128 analysis is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoudnessMode {
    #[default]
    Track,
    Album,
}

impl LoudnessMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            LoudnessMode::Track => "track",
            LoudnessMode::Album => "album",
        }
    }
}

impl From<&str> for LoudnessMode {
    fn from(s: &str) -> Self {
        match s {
            "album" => LoudnessMode::Album,
            _ => LoudnessMode::Track,
        }
    }
}

/// Inclusive bounds of a numeric audio setting. The backend clamps with
/// these on load and save, and the settings UI reads them over IPC
/// (`get_audio_setting_ranges`) for its slider/knob min and max, so the
/// range is stated once, at the layer that enforces it (#1249).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SettingRange {
    pub min: f32,
    pub max: f32,
}

impl SettingRange {
    /// Clamps `value` into range; NaN (e.g. a hand-edited `"NaN"` row)
    /// falls to `min` rather than passing through `f32::clamp` unchanged.
    pub fn clamp(self, value: f32) -> f32 {
        if value.is_nan() {
            self.min
        } else {
            value.clamp(self.min, self.max)
        }
    }
}

pub const TARGET_LUFS_RANGE: SettingRange = SettingRange {
    min: -23.0,
    max: -9.0,
};

pub const FALLBACK_GAIN_DB_RANGE: SettingRange = SettingRange {
    min: -12.0,
    max: 0.0,
};

/// Persisted loudness-normalization settings (`loudness_settings` table).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LoudnessSettings {
    pub enabled: bool,
    pub target_lufs: f32,
    pub mode: LoudnessMode,
    /// Gain applied (in dB) when a track has neither R128 analysis nor a
    /// ReplayGain tag. Defaults to a conservative negative value to avoid
    /// clipping unanalyzed, potentially loud tracks.
    pub fallback_gain_db: f32,
}

impl LoudnessSettings {
    /// This settings value with every numeric field inside its range.
    pub fn clamped(mut self) -> Self {
        self.target_lufs = TARGET_LUFS_RANGE.clamp(self.target_lufs);
        self.fallback_gain_db = FALLBACK_GAIN_DB_RANGE.clamp(self.fallback_gain_db);
        self
    }
}

impl Default for LoudnessSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            target_lufs: -16.0,
            mode: LoudnessMode::Track,
            fallback_gain_db: -6.0,
        }
    }
}

/// Where the currently applied loudness gain came from, for UI display next
/// to the currently playing track (e.g. an "R128"/"RG" badge in the player
/// bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoudnessGainSource {
    /// Normalization is off, or nothing is playing.
    #[default]
    Disabled,
    /// Gain computed from this track's own R128 analysis.
    Analyzed,
    /// Gain derived from a ReplayGain tag (no R128 analysis yet).
    ReplayGain,
    /// Gain derived from a foo_dr.txt DR Meter log's Peak/RMS (#57) — no
    /// R128 analysis or ReplayGain tag is available.
    DynamicRangeLog,
    /// Neither analysis, a tag, nor a DR log is available — the fixed
    /// fallback gain.
    Fallback,
}

/// Result of a gain computation: the linear multiplier for the DSP chain,
/// the underlying dB value, and where it came from (for a UI indicator).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GainResult {
    pub linear: f32,
    pub gain_db: f32,
    pub source: LoudnessGainSource,
}

/// Compute the gain to apply for a track, given whatever loudness
/// information is available and the user's settings. Priority: measured R128
/// loudness -> ReplayGain tag (track/album per `settings.mode`, falling back
/// to the other if the preferred one is missing) -> DR Meter log Peak/RMS
/// (#57) -> fixed fallback gain. The result is clamped to a sane range to
/// guard against runaway gain from bad tags or measurements.
#[allow(clippy::too_many_arguments)]
pub fn compute_gain(
    measured_lufs: Option<f64>,
    rg_track_gain: Option<f64>,
    rg_album_gain: Option<f64>,
    dr_rms_db: Option<f64>,
    dr_peak_db: Option<f64>,
    settings: &LoudnessSettings,
) -> GainResult {
    let target = settings.target_lufs as f64;
    let usable_measurement =
        measured_lufs.filter(|l| l.is_finite() && PLAUSIBLE_LUFS_RANGE.contains(l));

    let (gain_db, source) = if let Some(lufs) = usable_measurement {
        (target - lufs, LoudnessGainSource::Analyzed)
    } else {
        let rg = match settings.mode {
            LoudnessMode::Album => rg_album_gain.or(rg_track_gain),
            LoudnessMode::Track => rg_track_gain.or(rg_album_gain),
        };
        match rg {
            // ReplayGain tags are stored normalized to the -18 LUFS RG
            // reference; adjust by the difference to the user's target.
            Some(rg_db) => (rg_db + (target + 18.0), LoudnessGainSource::ReplayGain),
            None => match (dr_rms_db, dr_peak_db) {
                // Aim for the target loudness from RMS, but never boost
                // past the point where Peak would clip (0 dBFS headroom).
                (Some(rms), Some(peak)) if rms.is_finite() && peak.is_finite() => (
                    f64::min(target - rms, -peak),
                    LoudnessGainSource::DynamicRangeLog,
                ),
                _ => (
                    settings.fallback_gain_db as f64,
                    LoudnessGainSource::Fallback,
                ),
            },
        }
    };

    let clamped_db = gain_db.clamp(-30.0, 12.0);
    GainResult {
        linear: 10f64.powf(clamped_db / 20.0) as f32,
        gain_db: clamped_db as f32,
        source,
    }
}

/// Analyze a track's integrated loudness (LUFS) per ITU-R BS.1770-4.
/// Channels beyond L/R are not measured: BS.1770 surround weighting is a
/// rare edge case not worth the added complexity here. Samples stream
/// straight into the meters, so memory doesn't grow with the track's length.
pub fn analyze_integrated_loudness(decoder: luminous_audio::OfflineDecoder) -> Result<f64, String> {
    let sample_rate = decoder.sample_rate();
    // One meter per channel, sized by the first decoded packet.
    let mut meters: Vec<bs1770::ChannelLoudnessMeter> = Vec::new();
    decoder.decode_blocks(2, |planes| {
        if meters.is_empty() {
            meters = planes
                .iter()
                .map(|_| bs1770::ChannelLoudnessMeter::new(sample_rate))
                .collect();
        }
        for (meter, plane) in meters.iter_mut().zip(planes) {
            meter.push(plane.iter().copied());
        }
    });

    let mut meters = meters.into_iter();
    let loudness_lkfs = match (meters.next(), meters.next()) {
        (None, _) => return Err("no decodable audio channels".to_string()),
        (Some(mono), None) => {
            let windows = mono.into_100ms_windows();
            bs1770::gated_mean(windows.as_ref()).loudness_lkfs()
        }
        (Some(left), Some(right)) => {
            let stereo = bs1770::reduce_stereo(
                left.into_100ms_windows().as_ref(),
                right.into_100ms_windows().as_ref(),
            );
            bs1770::gated_mean(stereo.as_ref()).loudness_lkfs()
        }
    };

    let lufs = loudness_lkfs as f64;
    if !lufs.is_finite() {
        return Err("non-finite loudness result (likely silent/empty track)".to_string());
    }
    Ok(lufs)
}

/// Convenience helper to open a local file and analyze its integrated loudness.
pub fn analyze_integrated_loudness_file(path: &Path) -> Result<f64, String> {
    let decoder = luminous_audio::OfflineDecoder::open_file(path)
        .map_err(|e| format!("failed to open audio file for R128 analysis: {e}"))?;
    analyze_integrated_loudness(decoder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq_hz: f32, amplitude: f32, sample_rate: u32, seconds: u32) -> Vec<i16> {
        (0..sample_rate * seconds)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (amplitude * (2.0 * std::f32::consts::PI * freq_hz * t).sin() * 32_767.0) as i16
            })
            .collect()
    }

    /// Pinned against the measurement made before the decode loop was shared
    /// with the waveform path (#1555): a stereo file with a different tone on
    /// each side, so a swapped, dropped or downmixed channel would move it.
    #[test]
    fn analyze_integrated_loudness_measures_both_stereo_channels() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("stereo.wav");
        let left = sine(1_000.0, 0.5, 48_000, 3);
        let right = sine(250.0, 0.125, 48_000, 3);
        luminous_audio::write_test_pcm16_wav(&path, 48_000, &[left, right]);

        let lufs = analyze_integrated_loudness_file(&path).expect("analysis");

        assert!((lufs - -8.807_231_903_076).abs() < 1e-6, "{lufs}");
    }

    /// A mono file is measured from its single channel. A 0.5-amplitude
    /// 1 kHz tone sits about 9 dB below full scale (K-weighting is near
    /// unity at 1 kHz).
    #[test]
    fn analyze_integrated_loudness_measures_a_mono_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("mono.wav");
        luminous_audio::write_test_pcm16_wav(&path, 48_000, &[sine(1_000.0, 0.5, 48_000, 3)]);

        let lufs = analyze_integrated_loudness_file(&path).expect("analysis");

        assert!((-10.0..=-8.0).contains(&lufs), "{lufs}");
    }

    fn settings(target_lufs: f32, mode: LoudnessMode, fallback_gain_db: f32) -> LoudnessSettings {
        LoudnessSettings {
            enabled: true,
            target_lufs,
            mode,
            fallback_gain_db,
        }
    }

    #[test]
    fn measured_loudness_takes_priority() {
        let s = settings(-18.0, LoudnessMode::Track, -6.0);
        // Track measured at -12 LUFS needs -6 dB to reach -18 target.
        let result = compute_gain(Some(-12.0), Some(3.0), None, None, None, &s);
        let expected = 10f32.powf(-6.0 / 20.0);
        assert!(
            (result.linear - expected).abs() < 1e-3,
            "gain was {}",
            result.linear
        );
        assert_eq!(result.source, LoudnessGainSource::Analyzed);
    }

    #[test]
    fn replaygain_tag_used_when_unanalyzed() {
        let s = settings(-18.0, LoudnessMode::Track, -6.0);
        // At the -18 LUFS reference (== target), RG gain applies unmodified.
        let result = compute_gain(None, Some(-4.0), None, None, None, &s);
        let expected = 10f32.powf(-4.0 / 20.0);
        assert!(
            (result.linear - expected).abs() < 1e-3,
            "gain was {}",
            result.linear
        );
        assert_eq!(result.source, LoudnessGainSource::ReplayGain);
    }

    #[test]
    fn replaygain_tag_adjusted_for_non_default_target() {
        // Target is 4 dB louder than the RG reference (-18 -> -14), so the
        // effective gain shifts up by 4 dB.
        let s = settings(-14.0, LoudnessMode::Track, -6.0);
        let result = compute_gain(None, Some(-4.0), None, None, None, &s);
        let expected = 10f32.powf(0.0 / 20.0);
        assert!(
            (result.linear - expected).abs() < 1e-3,
            "gain was {}",
            result.linear
        );
    }

    #[test]
    fn falls_back_when_nothing_available() {
        let s = settings(-18.0, LoudnessMode::Track, -6.0);
        let result = compute_gain(None, None, None, None, None, &s);
        let expected = 10f32.powf(-6.0 / 20.0);
        assert!(
            (result.linear - expected).abs() < 1e-3,
            "gain was {}",
            result.linear
        );
        assert_eq!(result.source, LoudnessGainSource::Fallback);
    }

    #[test]
    fn analysis_failure_sentinel_is_ignored() {
        let s = settings(-18.0, LoudnessMode::Track, -6.0);
        let result = compute_gain(
            Some(ANALYSIS_FAILED_SENTINEL),
            Some(-2.0),
            None,
            None,
            None,
            &s,
        );
        let expected = 10f32.powf(-2.0 / 20.0);
        assert!(
            (result.linear - expected).abs() < 1e-3,
            "sentinel should fall through to RG tag"
        );
        assert_eq!(result.source, LoudnessGainSource::ReplayGain);
    }

    #[test]
    fn album_mode_prefers_album_gain() {
        let s = settings(-18.0, LoudnessMode::Album, -6.0);
        let result = compute_gain(None, Some(-4.0), Some(-2.0), None, None, &s);
        let expected = 10f32.powf(-2.0 / 20.0);
        assert!(
            (result.linear - expected).abs() < 1e-3,
            "album mode should prefer album gain"
        );
    }

    #[test]
    fn dr_log_used_when_no_analysis_or_replaygain() {
        let s = settings(-18.0, LoudnessMode::Track, -6.0);
        // RMS -17.46 dB needs -0.54 dB to reach -18 target; Peak -0.80 dB
        // only allows +0.80 dB before clipping — RMS-driven gain wins here.
        let result = compute_gain(None, None, None, Some(-17.46), Some(-0.80), &s);
        let expected = 10f32.powf(-0.54 / 20.0);
        assert!(
            (result.linear - expected).abs() < 1e-3,
            "gain was {}",
            result.linear
        );
        assert_eq!(result.source, LoudnessGainSource::DynamicRangeLog);
    }

    #[test]
    fn dr_log_gain_capped_by_peak_to_avoid_clipping() {
        let s = settings(-14.0, LoudnessMode::Track, -6.0);
        // Target-vs-RMS wants +8 dB, but Peak at -3 dB only leaves 3 dB of
        // headroom before clipping — the smaller (Peak-derived) gain wins.
        let result = compute_gain(None, None, None, Some(-22.0), Some(-3.0), &s);
        let expected = 10f32.powf(3.0 / 20.0);
        assert!(
            (result.linear - expected).abs() < 1e-3,
            "gain was {}",
            result.linear
        );
        assert_eq!(result.source, LoudnessGainSource::DynamicRangeLog);
    }

    #[test]
    fn dr_log_falls_back_to_fixed_gain_when_incomplete() {
        let s = settings(-18.0, LoudnessMode::Track, -6.0);
        let result = compute_gain(None, None, None, Some(-17.46), None, &s);
        assert_eq!(result.source, LoudnessGainSource::Fallback);
    }

    #[test]
    fn setting_range_clamp_handles_nan() {
        let r = SettingRange {
            min: -20.0,
            max: -5.0,
        };
        assert_eq!(r.clamp(f32::NAN), -20.0);
        assert_eq!(r.clamp(-25.0), -20.0);
        assert_eq!(r.clamp(-10.0), -10.0);
        assert_eq!(r.clamp(0.0), -5.0);
    }

    #[test]
    fn settings_clamped_bounds() {
        let s = LoudnessSettings {
            enabled: true,
            target_lufs: -100.0,
            mode: LoudnessMode::Track,
            fallback_gain_db: 50.0,
        }
        .clamped();
        assert_eq!(s.target_lufs, TARGET_LUFS_RANGE.min);
        assert_eq!(s.fallback_gain_db, FALLBACK_GAIN_DB_RANGE.max);
    }
}
