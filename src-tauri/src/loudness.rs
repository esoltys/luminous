//! Loudness normalization (#77) — EBU R128 (BS.1770) integrated-loudness
//! analysis, with a ReplayGain 2.0 tag fallback for tracks not yet analyzed.
//!
//! A low-priority background thread (`spawn_background_analyzer`) decodes
//! each not-yet-analyzed local track once (reusing the offline Symphonia
//! decode path, same pattern as the moodbar/waveform analyzers) and writes
//! `songs.ebur128_integrated_loudness_lufs`. Playback consults, in order:
//! measured R128 loudness -> ReplayGain tag (track or album, per settings) ->
//! a fixed fallback gain. The resulting linear multiplier is written to
//! `AudioEngine::set_loudness_gain`, the dormant slot already wired into the
//! DSP chain by #47.

use crate::db::Database;
use crate::models::{LoudnessMode, LoudnessSettings};
use anyhow::{anyhow, Context, Result};
use rusqlite::{params, OptionalExtension};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Sentinel written when analysis fails (corrupt/unsupported file) so the
/// background worker doesn't retry it forever. Clearly outside any real
/// loudness measurement range, and filtered out by `compute_gain_linear`.
const ANALYSIS_FAILED_SENTINEL: f64 = -100.0;

/// Plausible integrated-loudness range for a successfully analyzed track.
/// Anything outside this (including the failure sentinel) is treated as "no
/// usable measurement" rather than fed into the gain calculation.
const PLAUSIBLE_LUFS_RANGE: std::ops::RangeInclusive<f64> = -70.0..=0.0;

// ---------------------------------------------------------------------------
// Settings persistence
// ---------------------------------------------------------------------------

pub fn get_settings(db: &Database) -> Result<LoudnessSettings> {
    let conn = db.pool.get().context("failed to get db connection")?;
    let (enabled, target_lufs, mode_str, fallback_gain_db) = conn.query_row(
        "SELECT enabled, target_lufs, mode, fallback_gain_db FROM loudness_settings WHERE id = 1",
        [],
        |row| {
            Ok((
                row.get::<_, i32>(0)? != 0,
                row.get::<_, f64>(1)? as f32,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)? as f32,
            ))
        },
    )?;
    Ok(LoudnessSettings {
        enabled,
        target_lufs,
        mode: LoudnessMode::from(mode_str.as_str()),
        fallback_gain_db,
    })
}

pub fn save_settings(db: &Database, settings: &LoudnessSettings) -> Result<()> {
    let conn = db.pool.get().context("failed to get db connection")?;
    conn.execute(
        "UPDATE loudness_settings
         SET enabled = ?1, target_lufs = ?2, mode = ?3, fallback_gain_db = ?4
         WHERE id = 1",
        params![
            if settings.enabled { 1 } else { 0 },
            settings.target_lufs as f64,
            settings.mode.as_str(),
            settings.fallback_gain_db as f64,
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Gain calculation
// ---------------------------------------------------------------------------

/// Compute the linear gain multiplier to apply for a track, given whatever
/// loudness information is available and the user's settings. Priority:
/// measured R128 loudness -> ReplayGain tag (track/album per `settings.mode`,
/// falling back to the other if the preferred one is missing) -> fixed
/// fallback gain. The result is clamped to a sane range to guard against
/// runaway gain from bad tags or measurements.
pub fn compute_gain_linear(
    measured_lufs: Option<f64>,
    rg_track_gain: Option<f64>,
    rg_album_gain: Option<f64>,
    settings: &LoudnessSettings,
) -> f32 {
    let target = settings.target_lufs as f64;
    let usable_measurement =
        measured_lufs.filter(|l| l.is_finite() && PLAUSIBLE_LUFS_RANGE.contains(l));

    let gain_db = if let Some(lufs) = usable_measurement {
        target - lufs
    } else {
        let rg = match settings.mode {
            LoudnessMode::Album => rg_album_gain.or(rg_track_gain),
            LoudnessMode::Track => rg_track_gain.or(rg_album_gain),
        };
        match rg {
            // ReplayGain tags are stored normalized to the -18 LUFS RG
            // reference; adjust by the difference to the user's target.
            Some(rg_db) => rg_db + (target + 18.0),
            None => settings.fallback_gain_db as f64,
        }
    };

    let clamped_db = gain_db.clamp(-30.0, 12.0);
    10f64.powf(clamped_db / 20.0) as f32
}

// ---------------------------------------------------------------------------
// EBU R128 / BS.1770 analysis
// ---------------------------------------------------------------------------

/// Decode an entire audio file, deinterleaved per channel (capped at stereo —
/// BS.1770 channel weighting beyond L/R is a rare-surround edge case not
/// worth the added complexity here). Mirrors `analyzer::decode_all_samples`'s
/// decode loop but keeps channels separate instead of downmixing to mono.
fn decode_channels(path: &Path) -> Result<(Vec<Vec<f32>>, u32)> {
    use symphonia::core::{
        audio::SampleBuffer, codecs::DecoderOptions, errors::Error as SymphoniaError,
        formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
    };

    let file = std::fs::File::open(path).context("failed to open audio file for R128 analysis")?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("format probe failed during R128 analysis")?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .cloned()
        .ok_or_else(|| anyhow!("no active audio track found for R128 analysis"))?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("failed to create decoder for R128 analysis")?;

    let mut out_channels: Vec<Vec<f32>> = Vec::new();

    loop {
        match format.next_packet() {
            Ok(packet) => {
                if packet.track_id() != track_id {
                    continue;
                }
                match decoder.decode(&packet) {
                    Ok(decoded) => {
                        let spec = *decoded.spec();
                        let mut sample_buf =
                            SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                        sample_buf.copy_interleaved_ref(decoded);

                        let channels = spec.channels.count().max(1);
                        let out_count = channels.min(2);
                        if out_channels.is_empty() {
                            out_channels = vec![Vec::new(); out_count];
                        }
                        let decoded_samples = sample_buf.samples();

                        for chunk in decoded_samples.chunks(channels) {
                            for (c, out) in out_channels.iter_mut().enumerate() {
                                out.push(chunk[c]);
                            }
                        }
                    }
                    Err(SymphoniaError::DecodeError(_)) => continue,
                    Err(_) => break,
                }
            }
            Err(_) => break,
        }
    }

    Ok((out_channels, sample_rate))
}

/// Analyze a track's integrated loudness (LUFS) per ITU-R BS.1770-4.
pub fn analyze_integrated_loudness(path: &Path) -> Result<f64> {
    let (channels, sample_rate) = decode_channels(path)?;

    let loudness_lkfs = match channels.len() {
        0 => return Err(anyhow!("no decodable audio channels")),
        1 => {
            let mut meter = bs1770::ChannelLoudnessMeter::new(sample_rate);
            meter.push(channels[0].iter().copied());
            let windows = meter.into_100ms_windows();
            bs1770::gated_mean(windows.as_ref()).loudness_lkfs()
        }
        _ => {
            let mut left = bs1770::ChannelLoudnessMeter::new(sample_rate);
            left.push(channels[0].iter().copied());
            let mut right = bs1770::ChannelLoudnessMeter::new(sample_rate);
            right.push(channels[1].iter().copied());
            let stereo = bs1770::reduce_stereo(
                left.into_100ms_windows().as_ref(),
                right.into_100ms_windows().as_ref(),
            );
            bs1770::gated_mean(stereo.as_ref()).loudness_lkfs()
        }
    };

    let lufs = loudness_lkfs as f64;
    if !lufs.is_finite() {
        return Err(anyhow!(
            "non-finite loudness result (likely silent/empty track)"
        ));
    }
    Ok(lufs)
}

// ---------------------------------------------------------------------------
// Background analysis worker
// ---------------------------------------------------------------------------

/// Spawn the low-priority background R128 analyzer. Picks up local/collection
/// tracks lacking a loudness measurement, most-recently-added first (a simple
/// approximation of "newly scanned tracks first, then backfill"), analyzes
/// one at a time, and throttles between tracks so it never competes with
/// playback or scanning for CPU.
pub fn spawn_background_analyzer(app: AppHandle, db: Arc<Database>) {
    std::thread::Builder::new()
        .name("luminous-loudness".to_string())
        .spawn(move || {
            let mut analyzed: u64 = 0;
            loop {
                let next: Option<(i64, String)> = match db.pool.get() {
                    Ok(conn) => conn
                        .query_row(
                            "SELECT id, path FROM songs
                         WHERE source IN (1, 2) AND unavailable = 0 AND path IS NOT NULL
                           AND ebur128_integrated_loudness_lufs IS NULL
                         ORDER BY added DESC, id DESC LIMIT 1",
                            [],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()
                        .unwrap_or(None),
                    Err(_) => None,
                };

                let Some((song_id, path_str)) = next else {
                    // Nothing to do right now — check back periodically for newly
                    // scanned tracks.
                    std::thread::sleep(Duration::from_secs(15));
                    continue;
                };

                let path = Path::new(&path_str);
                let lufs = match analyze_integrated_loudness(path) {
                    Ok(lufs) => lufs,
                    Err(e) => {
                        log::warn!("R128 analysis failed for song {song_id} ({path_str}): {e}");
                        ANALYSIS_FAILED_SENTINEL
                    }
                };

                let remaining: i64 = db
                    .pool
                    .get()
                    .ok()
                    .and_then(|conn| {
                        conn.execute(
                            "UPDATE songs SET ebur128_integrated_loudness_lufs = ?1 WHERE id = ?2",
                            params![lufs, song_id],
                        )
                        .ok()?;
                        conn.query_row(
                            "SELECT COUNT(*) FROM songs
                         WHERE source IN (1, 2) AND unavailable = 0 AND path IS NOT NULL
                           AND ebur128_integrated_loudness_lufs IS NULL",
                            [],
                            |row| row.get(0),
                        )
                        .ok()
                    })
                    .unwrap_or(0);

                analyzed += 1;
                let _ = app.emit(
                    "loudness-analysis-progress",
                    crate::models::LoudnessAnalysisProgress {
                        analyzed,
                        remaining: remaining.max(0) as u64,
                    },
                );

                // Low-priority: yield generously between tracks.
                std::thread::sleep(Duration::from_millis(250));
            }
        })
        .expect("failed to spawn loudness analyzer thread");
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let gain = compute_gain_linear(Some(-12.0), Some(3.0), None, &s);
        let expected = 10f32.powf(-6.0 / 20.0);
        assert!((gain - expected).abs() < 1e-3, "gain was {gain}");
    }

    #[test]
    fn replaygain_tag_used_when_unanalyzed() {
        let s = settings(-18.0, LoudnessMode::Track, -6.0);
        // At the -18 LUFS reference (== target), RG gain applies unmodified.
        let gain = compute_gain_linear(None, Some(-4.0), None, &s);
        let expected = 10f32.powf(-4.0 / 20.0);
        assert!((gain - expected).abs() < 1e-3, "gain was {gain}");
    }

    #[test]
    fn replaygain_tag_adjusted_for_non_default_target() {
        // Target is 4 dB louder than the RG reference (-18 -> -14), so the
        // effective gain shifts up by 4 dB.
        let s = settings(-14.0, LoudnessMode::Track, -6.0);
        let gain = compute_gain_linear(None, Some(-4.0), None, &s);
        let expected = 10f32.powf(0.0 / 20.0);
        assert!((gain - expected).abs() < 1e-3, "gain was {gain}");
    }

    #[test]
    fn falls_back_when_nothing_available() {
        let s = settings(-18.0, LoudnessMode::Track, -6.0);
        let gain = compute_gain_linear(None, None, None, &s);
        let expected = 10f32.powf(-6.0 / 20.0);
        assert!((gain - expected).abs() < 1e-3, "gain was {gain}");
    }

    #[test]
    fn analysis_failure_sentinel_is_ignored() {
        let s = settings(-18.0, LoudnessMode::Track, -6.0);
        let gain = compute_gain_linear(Some(ANALYSIS_FAILED_SENTINEL), Some(-2.0), None, &s);
        let expected = 10f32.powf(-2.0 / 20.0);
        assert!(
            (gain - expected).abs() < 1e-3,
            "sentinel should fall through to RG tag"
        );
    }

    #[test]
    fn album_mode_prefers_album_gain() {
        let s = settings(-18.0, LoudnessMode::Album, -6.0);
        let gain = compute_gain_linear(None, Some(-4.0), Some(-2.0), &s);
        let expected = 10f32.powf(-2.0 / 20.0);
        assert!(
            (gain - expected).abs() < 1e-3,
            "album mode should prefer album gain"
        );
    }
}
