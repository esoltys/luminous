//! Pitch-preserving playback speed (#2142).
//!
//! `SpeedStage` sits between a track's decoded, resampled, channel-matched
//! samples and the ring buffer. At speed 1.0 it is an exact passthrough, so
//! output stays bit-identical when every other stage is neutral. At any other
//! speed it time-stretches with WSOLA, which changes duration and leaves pitch
//! alone. It runs on the decode thread only, never in the output callback.
//!
//! The engine's position bookkeeping (`played_samples`, `pushed_samples`) is in
//! *source-timeline* samples. The decode thread counts what it feeds in, and
//! the output callback scales what it plays out by the speed, so seek bar,
//! lyrics, scrobbling and gapless handover all stay on the track's own clock.

use std::num::{NonZeroU16, NonZeroU32};
use wsola::{Config, TimeStretch};

/// Slowest and fastest playback speed (the same range as YouTube).
pub const MIN_SPEED: f32 = 0.25;
pub const MAX_SPEED: f32 = 3.0;
/// Speeds are whole multiples of this (1.05, 1.10, ...).
pub const SPEED_STEP: f32 = 0.05;

/// The speed the engine will actually use for a request: clamped to
/// `MIN_SPEED..=MAX_SPEED` and snapped to the `SPEED_STEP` grid. A
/// non-finite request means normal speed, so no caller can produce an invalid
/// speed and there is no error to handle.
pub fn snap_speed(requested: f32) -> f32 {
    if !requested.is_finite() {
        return 1.0;
    }
    let steps = (requested.clamp(MIN_SPEED, MAX_SPEED) / SPEED_STEP).round();
    // Dividing by 20 rather than multiplying by 0.05 keeps 1.05, 1.10, ...
    // as the nearest f32 to the decimal, not one ulp off.
    steps / (1.0 / SPEED_STEP)
}

/// Time-stretches interleaved samples at a settable speed.
pub(crate) struct SpeedStage {
    speed: f32,
    /// Built on first use at a non-1.0 speed, and rebuilt when the output
    /// format changes underneath it (a device rebuild at another rate).
    stretch: Option<TimeStretch>,
}

impl SpeedStage {
    pub(crate) fn new(speed: f32) -> Self {
        Self {
            speed: snap_speed(speed),
            stretch: None,
        }
    }

    /// Change speed. Drops the stretcher's buffered audio, so callers do this
    /// only where the ring buffer is being restarted anyway (a seek).
    pub(crate) fn set_speed(&mut self, speed: f32) {
        self.speed = snap_speed(speed);
        self.stretch = None;
    }

    /// Forget buffered audio (after a seek or a track boundary).
    pub(crate) fn reset(&mut self) {
        if let Some(s) = self.stretch.as_mut() {
            s.reset();
        }
    }

    /// Stretch `input` (interleaved, `channels` wide, at `rate` Hz). The
    /// result is shorter or longer by the speed, and may be empty while the
    /// stretcher fills its window; it catches up on later calls.
    pub(crate) fn process(&mut self, input: Vec<f32>, rate: u32, channels: usize) -> Vec<f32> {
        if self.speed == 1.0 {
            return input;
        }
        let Some(stretch) = self.stretcher(rate, channels) else {
            return input;
        };
        stretch.push(&input);
        Self::pull_all(stretch)
    }

    /// The audio still inside the stretcher once no more input will come
    /// (end of a track). Leaves the stretcher ready for the next track.
    pub(crate) fn flush(&mut self) -> Vec<f32> {
        let Some(stretch) = self.stretch.as_mut() else {
            return Vec::new();
        };
        stretch.set_draining();
        let out = Self::pull_all(stretch);
        stretch.reset();
        out
    }

    fn stretcher(&mut self, rate: u32, channels: usize) -> Option<&mut TimeStretch> {
        let (rate, channels) = (NonZeroU32::new(rate)?, NonZeroU16::new(channels as u16)?);
        let stale = self
            .stretch
            .as_ref()
            .is_none_or(|s| s.sample_rate() != rate || s.channels() != channels);
        if stale {
            // Longer frames than the speech default are smoother on music.
            let config = Config {
                hop_ms: 20.0,
                search_ms: 15.0,
            };
            let mut s = TimeStretch::with_config(rate, channels, config).ok()?;
            s.set_tempo(self.speed);
            self.stretch = Some(s);
        }
        self.stretch.as_mut()
    }

    fn pull_all(stretch: &mut TimeStretch) -> Vec<f32> {
        let chunk = stretch.min_output_buff_size();
        let mut out = Vec::new();
        let mut scratch = vec![0.0f32; chunk];
        // `pull` fails with `NotEnoughInput` once the window is empty.
        while let Ok(n) = stretch.pull(&mut scratch) {
            out.extend_from_slice(&scratch[..n]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    fn sine(freq: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let v = (i as f32 / RATE as f32 * freq * std::f32::consts::TAU).sin() * 0.5;
                [v, v]
            })
            .collect()
    }

    /// Zero crossings per second of the left channel: a pitch proxy.
    fn crossings_per_sec(interleaved: &[f32]) -> f32 {
        let left: Vec<f32> = interleaved.iter().step_by(2).copied().collect();
        let n = left
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        n as f32 / (left.len() as f32 / RATE as f32)
    }

    /// Feeds `seconds` of a 440 Hz tone in packet-sized pieces, like the decoder.
    fn run(speed: f32, seconds: usize) -> Vec<f32> {
        let mut stage = SpeedStage::new(speed);
        let input = sine(440.0, RATE as usize * seconds);
        let mut out = Vec::new();
        for packet in input.chunks(2 * 1152) {
            out.extend(stage.process(packet.to_vec(), RATE, 2));
        }
        out.extend(stage.flush());
        out
    }

    #[test]
    fn snap_clamps_to_the_range_and_the_step_grid() {
        assert_eq!(snap_speed(1.0), 1.0);
        assert_eq!(snap_speed(1.05), 1.05);
        assert_eq!(snap_speed(1.12), 1.10);
        assert_eq!(snap_speed(1.13), 1.15);
        assert_eq!(snap_speed(0.0), MIN_SPEED);
        assert_eq!(snap_speed(-4.0), MIN_SPEED);
        assert_eq!(snap_speed(9.0), MAX_SPEED);
        assert_eq!(snap_speed(f32::NAN), 1.0);
        assert_eq!(snap_speed(f32::INFINITY), 1.0);
    }

    #[test]
    fn normal_speed_is_an_exact_passthrough() {
        let mut stage = SpeedStage::new(1.0);
        let input = sine(440.0, 4096);
        assert_eq!(stage.process(input.clone(), RATE, 2), input);
        assert!(stage.flush().is_empty());
    }

    #[test]
    fn output_length_follows_speed() {
        for speed in [0.5, 1.25, 2.0, 3.0] {
            let out = run(speed, 6);
            let frames = out.len() as f32 / 2.0;
            let expected = RATE as f32 * 6.0 / speed;
            assert!(
                (frames - expected).abs() / expected < 0.02,
                "speed {speed}: {frames} frames, expected about {expected}"
            );
        }
    }

    #[test]
    fn pitch_is_preserved_at_other_speeds() {
        for speed in [0.5, 1.5, 2.0] {
            let out = run(speed, 6);
            // Skip the start-up and tail, where the window is filling.
            let mid = &out[out.len() / 4..out.len() * 3 / 4];
            let hz = crossings_per_sec(mid);
            assert!((hz - 440.0).abs() < 15.0, "speed {speed}: {hz} Hz");
        }
    }

    #[test]
    fn changing_speed_drops_buffered_audio() {
        let mut stage = SpeedStage::new(2.0);
        let _ = stage.process(sine(440.0, 2000), RATE, 2);
        stage.set_speed(1.0);
        assert!(stage.flush().is_empty());
    }

    #[test]
    fn a_new_output_rate_rebuilds_the_stretcher() {
        let mut stage = SpeedStage::new(1.5);
        let _ = stage.process(sine(440.0, 4000), RATE, 2);
        let out = stage.process(sine(440.0, 48_000), 48_000, 2);
        assert!(!out.is_empty());
    }
}
