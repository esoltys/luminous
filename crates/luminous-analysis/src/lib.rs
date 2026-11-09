//! Offline audio analysis DSP: ITU-R BS.1770 loudness, ReplayGain/DR gain law,
//! RMS waveform envelopes, and 3-band FFT visualizers.
//!
//! Must never depend on Tauri, SQLite, reqwest, or Tokio (#1765, epic #1762).

pub mod integrity;
pub mod loudness;
pub mod provenance;
pub mod visualizer;

pub use integrity::{verify_audio_file, IntegrityVerdict};

pub use loudness::{
    analyze_integrated_loudness, analyze_integrated_loudness_file, compute_gain, GainResult,
    LoudnessGainSource, LoudnessMode, LoudnessSettings, SettingRange, ANALYSIS_FAILED_SENTINEL,
    FALLBACK_GAIN_DB_RANGE, PLAUSIBLE_LUFS_RANGE, TARGET_LUFS_RANGE,
};
pub use provenance::{
    analyze_provenance, analyze_provenance_from_samples, ProvenanceAnalysis, ProvenanceVerdict,
};
pub use visualizer::{
    analyze_visualizer, compute_band_waveform_data, compute_waveform_peaks, decode_mono,
    VisualizerAnalysis, BAND_WAVEFORM_POINTS, BAND_WAVEFORM_STYLE_VERSION, WAVEFORM_POINTS,
    WAVEFORM_STYLE_VERSION,
};
