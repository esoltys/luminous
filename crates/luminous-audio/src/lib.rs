//! Audio engine — Symphonia decoder + CPAL output pipeline.
//!
//! The decode loop runs in a plain `std::thread` (not tokio) because
//! `cpal::Stream` is `!Send` and cannot be held across `.await` points.
//! Communication uses `std::sync::mpsc` channels.
//!
//! ## Gapless playback
//! The engine emits `AboutToFinish` when the playing track is within
//! [`PRELOAD_LEAD_NS`] of its end boundary. The player responds with a
//! `PreloadNext` command; the decode thread opens and primes the next track
//! immediately, and when the current file is exhausted it continues decoding
//! the preloaded track into the same ring buffer without pausing the CPAL
//! stream — the buffer never drains, so there is no audible gap. When the
//! last sample of the finished track is actually consumed by the output
//! callback, `TrackTransitioned` is emitted so the player can advance its
//! bookkeeping without issuing a new `Play`.
//!
//! ## Auto-crossfade (#1238)
//! When the player answers with `PreloadNextCrossfade` instead, the handover
//! starts that many seconds before the current track's end: the preloaded
//! track becomes current and the old one's tail is mixed into it on the
//! decode thread with an equal-power curve. The handover is tracked exactly
//! like a gapless one, so `TrackTransitioned` fires when the overlap becomes
//! audible. The loudness gain switches to the incoming track's at the
//! overlap's first sample (scheduled into the callback), and the tail is
//! pre-scaled by the ratio of the two gains so it keeps its own level.
//!
//! ## DSP chain (contract shared with #77/#79)
//! `decode → loudness gain (#77) → EQ preamp → EQ bands → fade envelope (#79)
//! → volume → clip guard → output`. Each gain stage is a single precomputed
//! multiplier read from an atomic — nothing in the output callback allocates
//! or blocks. When every stage is neutral (EQ off, gains at 1.0) samples pass
//! through bit-perfect.

mod codecs;
pub mod pcm;
pub mod sink;
pub mod asio;
mod source;
mod spectrum;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use source::ProbedTrack;
use std::path::Path;

pub use sink::{
    enumerate_output_devices, AudioBackendKind, AudioDeviceInfo, AudioOutputConfig, AudioSink,
};
pub use source::OfflineDecoder;
pub use symphonia::core::io::MediaSource;

use ringbuf::{
    traits::{Consumer, Observer, Producer, Split},
    HeapRb,
};
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    mpsc, Arc,
};
use symphonia::core::{
    codecs::audio::AudioDecoder, errors::Error as SymphoniaError, formats::FormatReader,
};

/// How far before the end boundary the `AboutToFinish` signal fires.
/// Consumers: gapless preload (here), auto-crossfade (#79), CUE
/// sibling-track continuation (#78). Measured from the *audible* position,
/// while decode runs up to ~1.5s ahead of it, so this must exceed the longest
/// auto-crossfade (`CROSSFADE_AUTO_DURATION_SECS_RANGE`, 8s) plus that lead
/// plus time to open the next track — otherwise the crossfade window has
/// already been decoded by the time the preload arrives (#1238).
const PRELOAD_LEAD_NS: u64 = 12_000_000_000;

/// The longest auto-crossfade the engine can honour (#79): the preload must
/// arrive before the crossfade window has been decoded, so `PRELOAD_LEAD_NS`
/// is sized from this. The app's crossfade setting takes its upper bound
/// from here rather than retyping it.
pub const MAX_AUTO_CROSSFADE_SECS: f32 = 8.0;

/// Decode runs up to ~1.5 s ahead of what is audible, and opening the next
/// track takes time too; this much must remain after the longest crossfade.
const _: () = assert!(PRELOAD_LEAD_NS >= (MAX_AUTO_CROSSFADE_SECS as u64 + 3) * 1_000_000_000);

/// Maximum linear amplitude allowed when dynamic processing (loudness normalization
/// or equalizer) is active. Set to -1.0 dBTP (True Peak) ≈ 0.8912509 linear amplitude
/// (10^(-1/20)) to prevent inter-sample clipping during digital-to-analog reconstruction.
const TRUE_PEAK_CEILING: f32 = 0.891_250_9;

/// Step count and per-step sleep duration for a gain ramp spread over
/// `duration_ms` in ~10ms increments. Shared by `fade_out_then` (on the
/// decode thread) and `AudioEngine::ramp_loudness_gain` (on its own thread).
fn ramp_steps(duration_ms: u32) -> (u32, std::time::Duration) {
    let steps = (duration_ms / 10).max(1);
    let step_dur = std::time::Duration::from_millis((duration_ms / steps) as u64);
    (steps, step_dur)
}

/// Linear interpolation from `start_gain` to `end_gain` at step `i` of
/// `steps` total (`i == steps` yields exactly `end_gain`).
fn ramp_gain_at_step(start_gain: f32, end_gain: f32, steps: u32, i: u32) -> f32 {
    let t = i as f32 / steps as f32;
    start_gain + (end_gain - start_gain) * t
}

/// Ramp the fade envelope down over `duration_ms`, run `then` (pause or
/// stop the stream), and restore the envelope for the next start. With no
/// duration the envelope is never touched, so an instant pause stays
/// bit-identical to playing up to that point.
fn fade_out_then(fade_gain: &AtomicU32, duration_ms: u32, then: impl FnOnce()) {
    if duration_ms == 0 {
        then();
        return;
    }
    let (steps, step_dur) = ramp_steps(duration_ms);
    for i in 0..=steps {
        let g = ramp_gain_at_step(1.0, 0.0, steps, i);
        fade_gain.store(g.to_bits(), Ordering::Relaxed);
        std::thread::sleep(step_dur);
    }
    then();
    fade_gain.store(1.0f32.to_bits(), Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Control messages sent to the decode thread
// ---------------------------------------------------------------------------

enum AudioCommand {
    Play(PlayRequest),
    Cue(PlayRequest),
    /// Fade out over this many ms (0 = at once), then pause.
    Pause(u32),
    // The fade-in length is accepted but not yet honoured (#1718).
    #[allow(dead_code)]
    Resume(u32),
    /// Fade out over this many ms (0 = at once), then stop.
    Stop(u32),
    SeekTo(u64), // target position in nanoseconds
    /// Prime the next track for a gapless transition after the current one.
    PreloadNext(PlayRequest),
    /// Prime the next track for an auto-crossfade transition (#79).
    /// Carries the crossfade length in seconds and the incoming track's
    /// loudness-normalization gain, which takes over when the overlap starts.
    PreloadNextCrossfade(PlayRequest, f32, f32),
    /// Drop a primed next track (playback context changed) and re-arm the
    /// `AboutToFinish` signal so a fresh preload can be requested.
    ClearPreload,
    /// Force a rebuild of the audio output stream (e.g. backend or device changed).
    RebuildOutput,
}

struct PlayRequest {
    track: Track,
    start_nanosec: u64,
}

/// What the engine needs to know to play one track; the app builds it from
/// its own song model.
#[derive(Debug, Clone, Default)]
pub struct Track {
    /// The app's id for the track, echoed back in [`AudioEvent`]s.
    pub id: i64,
    /// Handed to the source opener: a file path for the default opener, or
    /// whatever the app's opener understands (URLs, custom schemes). Its
    /// extension, if any, hints the format probe.
    pub location: String,
    /// Extension (without the dot) to hint the format probe when `location`
    /// has none, e.g. a stream URL for a song whose file is known to be FLAC.
    pub extension_hint: Option<String>,
    /// Hard stop inside the file (a CUE sheet track's end); `None` plays to EOF.
    pub end_ns: Option<u64>,
    /// The tagged length, used for the `AboutToFinish` signal when there is
    /// no `end_ns`.
    pub length_ns: Option<u64>,
}

/// Playback state as the engine sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EngineState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// Opens the bytes behind a [`Track::location`]. The engine's default opener
/// reads local files; the app injects one that also knows its remote sources.
type SourceOpener = dyn Fn(&str) -> Result<Box<dyn MediaSource>, String> + Send + Sync;

fn open_file(path: &str) -> Result<Box<dyn MediaSource>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Cannot open file '{path}': {e}"))?;
    Ok(Box::new(file))
}

/// Facts about the output stage, for the app's pipeline display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub device_name: String,
    pub backend_name: String,
}

// ---------------------------------------------------------------------------
// Events emitted from the decode thread back to the player
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum AudioEvent {
    Playing {
        track_id: i64,
    },
    Paused,
    Stopped,
    TrackFinished {
        track_id: i64,
    },
    /// The playing track is within `PRELOAD_LEAD_NS` of its end boundary.
    AboutToFinish {
        track_id: i64,
    },
    /// A gapless transition completed: `finished_track_id` played to its end
    /// and `track_id` is now audible, with no interruption of the stream.
    TrackTransitioned {
        finished_track_id: i64,
        track_id: i64,
    },
    /// The audio pipeline has changed (e.g. output device reconnected or changed format).
    PipelineChanged,
    /// The output stream could not be (re)built — typically a device switch
    /// whose new device can't be opened. The track is already parked paused at
    /// its position (a `Paused` event precedes this one); informational only,
    /// so the player must not treat it as a bad track. Play retries.
    OutputUnavailable {
        message: String,
    },
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Shared audio-graph handles — bundled so the decode/output plumbing threads
// a single value instead of 6-10 individual Arcs through every layer.
// ---------------------------------------------------------------------------

pub(crate) struct AudioShared {
    pub(crate) open_source: Box<SourceOpener>,
    pub(crate) position: AtomicU64,
    pub(crate) volume: AtomicU32,
    pub(crate) play_state: Mutex<EngineState>,
    pub(crate) visualizer_buf: spectrum::AudioVisualizerBuffer,
    /// Whether the output callback feeds `visualizer_buf` at all: the
    /// downmix is skipped while nothing displays a spectrum.
    pub(crate) spectrum_enabled: std::sync::atomic::AtomicBool,
    pub(crate) output_sample_rate: AtomicU32,
    pub(crate) output_channels: std::sync::atomic::AtomicU16,
    pub(crate) output_device_name: parking_lot::RwLock<Option<String>>,
    pub(crate) output_backend_name: parking_lot::RwLock<String>,
    pub(crate) output_config: parking_lot::RwLock<AudioOutputConfig>,
    pub(crate) equalizer: Mutex<luminous_equalizer::Equalizer>,
    pub(crate) loudness_gain: AtomicU32,
    /// Bumped by every loudness-gain write that isn't a ramp step, so an
    /// in-flight `ramp_loudness_gain` stops instead of overwriting it: the
    /// newest write always wins.
    pub(crate) loudness_generation: AtomicU64,
    /// A loudness-gain change scheduled at an exact played-sample index
    /// (`AudioOutput::played_samples` space), applied by the output callback
    /// mid-buffer so a crossfade's incoming track takes over its gain at the
    /// sample the overlap starts (#1238). `NO_LOUDNESS_SWITCH` when idle.
    pub(crate) loudness_switch_at: AtomicU64,
    /// The gain `loudness_switch_at` switches to; written before it.
    pub(crate) loudness_switch_gain: AtomicU32,
    pub(crate) fade_gain: AtomicU32,
}

/// `AudioShared::loudness_switch_at` value meaning no switch is scheduled.
const NO_LOUDNESS_SWITCH: u64 = u64::MAX;

impl AudioShared {
    /// Switch the loudness gain to `gain` when the output callback plays
    /// sample `at`, replacing any switch already scheduled.
    fn schedule_loudness_switch(&self, at: u64, gain: f32) {
        self.loudness_generation.fetch_add(1, Ordering::AcqRel);
        self.loudness_switch_gain
            .store(gain.max(0.0).to_bits(), Ordering::Relaxed);
        self.loudness_switch_at.store(at, Ordering::Release);
    }

    /// Drop a scheduled loudness switch: the played-sample space it was
    /// measured in has been reset (seek, new track, device rebuild).
    fn cancel_loudness_switch(&self) {
        self.loudness_switch_at
            .store(NO_LOUDNESS_SWITCH, Ordering::Release);
    }
}

// ---------------------------------------------------------------------------
// AudioEngine — public handle
// ---------------------------------------------------------------------------

/// The playback engine. Commands are queued to a dedicated decode thread
/// and return immediately; what actually happened is reported as
/// [`AudioEvent`]s on [`AudioEngine::events`], so callers react to the engine
/// instead of assuming the outcome of a call.
pub struct AudioEngine {
    cmd_tx: mpsc::SyncSender<AudioCommand>,
    event_rx: Mutex<Option<mpsc::Receiver<AudioEvent>>>,
    shared: Arc<AudioShared>,
}

impl AudioEngine {
    /// An engine that opens [`Track::location`]s as local file paths.
    pub fn new() -> Self {
        Self::with_source_opener(open_file)
    }

    /// An engine that opens every [`Track::location`] through `open_source`,
    /// on the decode thread, each time a track starts or is reopened.
    pub fn with_source_opener(
        open_source: impl Fn(&str) -> Result<Box<dyn MediaSource>, String> + Send + Sync + 'static,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::sync_channel::<AudioCommand>(64);
        let (event_tx, event_rx) = mpsc::channel::<AudioEvent>();
        let shared = Arc::new(AudioShared {
            open_source: Box::new(open_source),
            position: AtomicU64::new(0),
            volume: AtomicU32::new(1.0f32.to_bits()),
            play_state: Mutex::new(EngineState::Stopped),
            visualizer_buf: spectrum::AudioVisualizerBuffer::new(4096),
            spectrum_enabled: std::sync::atomic::AtomicBool::new(false),
            output_sample_rate: AtomicU32::new(44100),
            output_channels: std::sync::atomic::AtomicU16::new(2),
            output_device_name: parking_lot::RwLock::new(get_default_device_name()),
            output_backend_name: parking_lot::RwLock::new("CPAL".to_string()),
            output_config: parking_lot::RwLock::new(AudioOutputConfig::default()),
            equalizer: Mutex::new(luminous_equalizer::Equalizer::new()),
            loudness_gain: AtomicU32::new(1.0f32.to_bits()),
            loudness_generation: AtomicU64::new(0),
            loudness_switch_at: AtomicU64::new(NO_LOUDNESS_SWITCH),
            loudness_switch_gain: AtomicU32::new(1.0f32.to_bits()),
            fade_gain: AtomicU32::new(1.0f32.to_bits()),
        });
        let shared_clone = Arc::clone(&shared);

        // Spawn a plain OS thread — no Send requirement on cpal::Stream
        std::thread::Builder::new()
            .name("luminous-audio".to_string())
            .spawn(move || {
                decode_thread(cmd_rx, event_tx, shared_clone);
            })
            .expect("failed to spawn audio thread");

        Self {
            cmd_tx,
            event_rx: Mutex::new(Some(event_rx)),
            shared,
        }
    }

    fn send_cmd(&self, cmd: AudioCommand) -> Result<()> {
        self.cmd_tx
            .send(cmd)
            .map_err(|_| anyhow!("audio thread shut down"))
    }

    pub fn play(&self, track: Track, start_nanosec: u64) -> Result<()> {
        self.send_cmd(AudioCommand::Play(PlayRequest {
            track,
            start_nanosec,
        }))
    }

    /// Load `track` paused at `start_nanosec`, ready for `resume`.
    pub fn cue(&self, track: Track, start_nanosec: u64) -> Result<()> {
        *self.shared.play_state.lock() = EngineState::Paused;
        self.shared.position.store(start_nanosec, Ordering::Relaxed);
        self.send_cmd(AudioCommand::Cue(PlayRequest {
            track,
            start_nanosec,
        }))
    }

    /// Prime the track that follows the current one, for a gapless handover.
    /// The usual answer to [`AudioEvent::AboutToFinish`].
    pub fn preload_next(&self, track: Track, start_nanosec: u64) -> Result<()> {
        self.send_cmd(AudioCommand::PreloadNext(PlayRequest {
            track,
            start_nanosec,
        }))
    }

    /// Like `preload_next`, but overlap the two tracks for `crossfade_secs`
    /// (at most [`MAX_AUTO_CROSSFADE_SECS`]); `loudness_gain` takes over
    /// when the overlap starts.
    pub fn preload_next_with_crossfade(
        &self,
        track: Track,
        start_nanosec: u64,
        crossfade_secs: f32,
        loudness_gain: f32,
    ) -> Result<()> {
        self.send_cmd(AudioCommand::PreloadNextCrossfade(
            PlayRequest {
                track,
                start_nanosec,
            },
            crossfade_secs,
            loudness_gain,
        ))
    }

    /// Drop a primed next track (the playback context changed) and re-arm
    /// `AboutToFinish` so a fresh preload can be requested.
    pub fn clear_preload(&self) -> Result<()> {
        self.send_cmd(AudioCommand::ClearPreload)
    }

    /// Pause, fading out over `fade_ms` first (0 pauses at once).
    pub fn pause(&self, fade_ms: u32) -> Result<()> {
        self.send_cmd(AudioCommand::Pause(fade_ms))
    }

    /// Resume a paused or cued track. `fade_ms` is the fade-in length
    /// (not yet honoured, #1718).
    pub fn resume(&self, fade_ms: u32) -> Result<()> {
        self.send_cmd(AudioCommand::Resume(fade_ms))
    }

    /// Stop, fading out over `fade_ms` first (0 stops at once).
    pub fn stop(&self, fade_ms: u32) -> Result<()> {
        self.send_cmd(AudioCommand::Stop(fade_ms))
    }

    pub fn seek_to(&self, position_nanosec: u64) -> Result<()> {
        self.send_cmd(AudioCommand::SeekTo(position_nanosec))
    }

    /// Master volume, clamped to 0.0–1.0.
    pub fn set_volume(&self, vol: f32) {
        let vol = vol.clamp(0.0, 1.0);
        self.shared.volume.store(vol.to_bits(), Ordering::Relaxed);
    }

    /// Step the per-track loudness-normalization multiplier (#77) at once,
    /// e.g. as a track starts. Cancels any ramp in progress.
    pub fn set_loudness_gain(&self, gain: f32) {
        self.shared
            .loudness_generation
            .fetch_add(1, Ordering::AcqRel);
        self.shared
            .loudness_gain
            .store(gain.max(0.0).to_bits(), Ordering::Relaxed);
    }

    /// Glide the loudness multiplier to `target` over `duration_ms`, for a
    /// change in the middle of continuous audio where a hard step would be
    /// an audible click. Returns at once; the ramp runs on its own thread
    /// and stops early if any newer loudness write (a set, another ramp, a
    /// crossfade's switch) lands first.
    pub fn ramp_loudness_gain(&self, target: f32, duration_ms: u32) {
        let target = target.max(0.0);
        let generation = self
            .shared
            .loudness_generation
            .fetch_add(1, Ordering::AcqRel)
            + 1;
        let start = f32::from_bits(self.shared.loudness_gain.load(Ordering::Relaxed));
        if duration_ms == 0 || (target - start).abs() < f32::EPSILON {
            self.shared
                .loudness_gain
                .store(target.to_bits(), Ordering::Relaxed);
            return;
        }
        let shared = Arc::clone(&self.shared);
        let spawned = std::thread::Builder::new()
            .name("luminous-loudness-ramp".into())
            .spawn(move || {
                let (steps, step_dur) = ramp_steps(duration_ms);
                for i in 0..=steps {
                    if shared.loudness_generation.load(Ordering::Acquire) != generation {
                        return;
                    }
                    let g = ramp_gain_at_step(start, target, steps, i);
                    shared.loudness_gain.store(g.to_bits(), Ordering::Relaxed);
                    std::thread::sleep(step_dur);
                }
            });
        if spawned.is_err() {
            // No thread to ramp on: land on the target rather than stay put.
            self.shared
                .loudness_gain
                .store(target.to_bits(), Ordering::Relaxed);
        }
    }

    /// Audible position in the current track.
    pub fn current_position_nanosec(&self) -> u64 {
        self.shared.position.load(Ordering::Relaxed)
    }

    pub fn current_volume(&self) -> f32 {
        f32::from_bits(self.shared.volume.load(Ordering::Relaxed))
    }

    /// The loudness multiplier in effect now (mid-ramp, the current step).
    pub fn current_loudness_gain(&self) -> f32 {
        f32::from_bits(self.shared.loudness_gain.load(Ordering::Relaxed))
    }

    pub fn current_state(&self) -> EngineState {
        *self.shared.play_state.lock()
    }

    /// One controlled escape hatch for equalizer mutation/inspection —
    /// centralizes locking instead of callers reaching into the field.
    pub fn with_equalizer<R>(&self, f: impl FnOnce(&mut luminous_equalizer::Equalizer) -> R) -> R {
        let mut eq = self.shared.equalizer.lock();
        f(&mut eq)
    }

    pub fn spectrum_enabled(&self) -> bool {
        self.shared.spectrum_enabled.load(Ordering::Relaxed)
    }

    /// Start or stop feeding the spectrum analyzer. While it is off the
    /// output callback skips the work, and `spectrum_snapshot` describes
    /// whatever played before it was turned off.
    pub fn set_spectrum_enabled(&self, enabled: bool) {
        self.shared
            .spectrum_enabled
            .store(enabled, Ordering::Relaxed);
    }

    /// Magnitude spectrum of what was most recently played, `fft_size / 2`
    /// bins scaled for display, computed at the output's actual sample rate.
    pub fn spectrum_snapshot(&self, fft_size: usize) -> Vec<f32> {
        let sample_rate = self.shared.output_sample_rate.load(Ordering::Relaxed);
        spectrum::calculate_spectrum(&self.shared.visualizer_buf, fft_size, sample_rate)
    }

    /// The output stage as currently configured, with sensible defaults
    /// before the first stream is built.
    pub fn output_info(&self) -> OutputInfo {
        let rate = self.shared.output_sample_rate.load(Ordering::Relaxed);
        let channels = self.shared.output_channels.load(Ordering::Relaxed);
        let device_name = self
            .shared
            .output_device_name
            .read()
            .clone()
            .or_else(get_default_device_name)
            .unwrap_or_else(|| "Default Audio Device".to_string());
        let backend_name = self.shared.output_backend_name.read().clone();
        OutputInfo {
            sample_rate: if rate > 0 { rate } else { 44100 },
            channels: if channels > 0 { channels } else { 2 },
            device_name,
            backend_name: if backend_name.is_empty() { "CPAL".to_string() } else { backend_name },
        }
    }

    /// Update the preferred audio output configuration (backend and device).
    pub fn set_output_config(&self, config: AudioOutputConfig) -> Result<()> {
        *self.shared.output_config.write() = config;
        self.send_cmd(AudioCommand::RebuildOutput)
    }

    /// Read the active/preferred audio output configuration.
    pub fn output_config(&self) -> AudioOutputConfig {
        self.shared.output_config.read().clone()
    }

    /// Open the hardware control panel for the currently configured ASIO driver.
    pub fn open_asio_control_panel(&self) -> Result<(), String> {
        let dev = self.shared.output_config.read().device_id.clone();
        #[cfg(windows)]
        {
            crate::asio::open_asio_control_panel_by_clsid(dev.as_deref().unwrap_or_default())
        }
        #[cfg(not(windows))]
        {
            let _ = dev;
            Err("ASIO is only supported on Windows".to_string())
        }
    }

    /// The engine's event stream, handed out once: the first call returns
    /// it, later calls `None`. Drain it on a dedicated thread (blocking on
    /// `Receiver::iter()`), not while holding a lock on the engine.
    pub fn take_events(&self) -> Option<mpsc::Receiver<AudioEvent>> {
        self.event_rx.lock().take()
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// A fully opened, probed, decode-ready track.
struct ActiveTrack {
    spec: Track,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    src_rate: u32,
    src_channels: usize,
    resampler: Resampler,
    start_ns: u64,
    /// Hard decode cutoff (CUE end boundary).
    end_ns: Option<u64>,
    /// Where `AboutToFinish` is measured from: the CUE end boundary if set,
    /// otherwise the tagged track length.
    about_end_ns: Option<u64>,
    /// Decoded-stream position (source timeline), advanced per packet.
    decoded_pos_ns: u64,
    about_to_finish_sent: bool,
    eof: bool,
}

impl ActiveTrack {
    fn open(
        open_source: &SourceOpener,
        spec: Track,
        start_nanosec: u64,
        target_rate: u32,
        target_channels: usize,
    ) -> Result<Self, String> {
        if spec.location.is_empty() {
            return Err("Track has no playable location".to_string());
        }
        let source = open_source(&spec.location)?;
        // The location's own extension wins; a stream URL has none, so the
        // spec's hint stands in.
        let extension = Path::new(&spec.location)
            .extension()
            .and_then(|e| e.to_str())
            .or(spec.extension_hint.as_deref());
        let ProbedTrack {
            mut format,
            mut decoder,
            track_id,
            sample_rate: src_rate,
            channels: src_channels,
        } = ProbedTrack::open(source, extension)?;

        if start_nanosec > 0 {
            let target_time = symphonia::core::units::Time::from_nanos(start_nanosec as i64);
            match format.seek(
                symphonia::core::formats::SeekMode::Accurate,
                symphonia::core::formats::SeekTo::Time {
                    time: target_time,
                    track_id: Some(track_id),
                },
            ) {
                Ok(_) => decoder.reset(),
                Err(e) => log::warn!("Initial seek to {start_nanosec}ns failed: {e:?}"),
            }
        }

        let end_ns = spec.end_ns.filter(|&ns| ns > 0);
        let about_end_ns = end_ns.or(spec.length_ns);

        Ok(Self {
            spec,
            format,
            decoder,
            track_id,
            src_rate,
            src_channels,
            resampler: Resampler::new(src_rate, target_rate, target_channels),
            start_ns: start_nanosec,
            end_ns,
            about_end_ns,
            decoded_pos_ns: start_nanosec,
            about_to_finish_sent: false,
            eof: false,
        })
    }
}

/// Convert an absolute track time to an interleaved-sample count at the
/// output device's rate/channel format.
fn samples_for_ns(ns: u64, rate: u32, channels: u16) -> u64 {
    (ns as f64 * rate as f64 * channels as f64 / 1_000_000_000.0) as u64
}

/// A gapless handover in progress: the finished track's tail is still
/// draining from the ring buffer while the next track is being decoded
/// behind it.
struct PendingTransition {
    /// Absolute pushed-sample count at which the next track's audio begins.
    boundary_samples: u64,
    finished: Track,
}

/// The outgoing track of an auto-crossfade (#1238). Once the crossfade
/// starts, the incoming track becomes `DecodeSession::current` and this
/// track's remaining audio is mixed into it on the decode thread — the
/// output callback only ever sees the already-mixed stream.
struct CrossfadeTail {
    track: ActiveTrack,
    /// Overlap length in interleaved output samples.
    total: usize,
    /// Interleaved samples of the overlap already mixed.
    mixed: usize,
    /// Pre-scale for the tail: its own loudness gain over the incoming
    /// track's. The output callback applies the incoming track's gain to the
    /// mixed stream, so this keeps the tail at its own normalized level.
    gain: f32,
    /// Decoded tail samples not yet consumed by a mix (packets from the two
    /// tracks don't line up).
    fifo: Vec<f32>,
}

/// Offset into a callback buffer of `played` samples, which starts at
/// played-sample index `played_before`, where a loudness switch scheduled at
/// `switch_at` takes effect; `None` if it lies beyond this buffer. The end is
/// inclusive so a switch landing exactly on a buffer boundary fires in the
/// buffer that reached it, before the decode thread can rebase the counter.
fn loudness_switch_offset(switch_at: u64, played_before: u64, played: usize) -> Option<usize> {
    if switch_at == NO_LOUDNESS_SWITCH || switch_at > played_before + played as u64 {
        return None;
    }
    Some(switch_at.saturating_sub(played_before) as usize)
}

/// Equal-power crossfade gains `(outgoing, incoming)` at progress `p` in
/// `0.0..=1.0`: `cos(p·π/2)` and `sin(p·π/2)`, whose powers always sum to 1
/// so the overlap keeps a constant perceived loudness.
fn equal_power_gains(p: f32) -> (f32, f32) {
    let angle = p.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

/// Mix the outgoing tail `a` into the incoming samples `b` in place, for the
/// part of `b` that falls inside the overlap. `mixed_before` is how many
/// interleaved samples of the `total`-sample overlap were mixed by earlier
/// calls. Missing `a` samples (a tail shorter than its tagged length) count
/// as silence. `a_gain` pre-scales `a` (see `CrossfadeTail::gain`). Returns
/// how many samples of `b` were inside the overlap.
fn mix_crossfade(
    b: &mut [f32],
    a: &[f32],
    a_gain: f32,
    mixed_before: usize,
    total: usize,
    channels: usize,
) -> usize {
    let n = b.len().min(total.saturating_sub(mixed_before));
    let total_frames = (total / channels.max(1)).max(1) as f32;
    for (i, sample) in b[..n].iter_mut().enumerate() {
        let frame = (mixed_before + i) / channels.max(1);
        let (ga, gb) = equal_power_gains(frame as f32 / total_frames);
        let a_val = a.get(i).copied().unwrap_or(0.0) * a_gain;
        *sample = a_val * ga + *sample * gb;
    }
    n
}

/// Overlap length (interleaved output samples) for a crossfade of `secs`
/// between an outgoing track with `outgoing_remaining_ns` left to decode and
/// an incoming track of `incoming_len_ns`. Returns 0 — a plain gapless
/// handover — when the crossfade is off, the outgoing end is unknown, or the
/// incoming track is shorter than twice the window (it would be fading out
/// before it had finished fading in). A late start shortens the overlap to
/// whatever of the outgoing track is left rather than cutting its tail.
fn crossfade_overlap_samples(
    secs: f32,
    outgoing_remaining_ns: Option<u64>,
    incoming_len_ns: Option<u64>,
    rate: u32,
    channels: u16,
) -> usize {
    if secs.is_nan() || secs <= 0.0 {
        return 0;
    }
    let window_ns = (secs as f64 * 1_000_000_000.0) as u64;
    let Some(remaining) = outgoing_remaining_ns else {
        return 0;
    };
    if incoming_len_ns.is_some_and(|len| len < window_ns.saturating_mul(2)) {
        return 0;
    }
    let ns = window_ns.min(remaining);
    // Whole frames only, so the gain curve stays aligned across channels.
    let frames = samples_for_ns(ns, rate, 1);
    (frames * channels as u64) as usize
}

// ---------------------------------------------------------------------------
// Decode thread — runs on a plain OS thread to avoid Send constraints
// ---------------------------------------------------------------------------

/// The CPAL output stream + its ring buffer, opened once and reused for the
/// lifetime of the decode thread. Rebuilding the native WASAPI/CPAL
/// device+stream on every track change is expensive, and after enough
/// rebuilds — observed at roughly a dozen track changes, even spaced well
/// apart, not just rapid bursts — it can wedge the OS audio subsystem
/// entirely, hanging inside `build_output_stream`/`stream.play()` with no
/// timeout and taking the whole player down with it (every playback command
/// funnels through this same thread). Track changes now clear/reseed this
/// same buffer and use `Stream::play()`/`pause()` instead of dropping and
/// rebuilding the stream.
struct AudioOutput {
    sink: Box<dyn AudioSink>,
    producer: ringbuf::HeapProd<f32>,
    consumer: Arc<Mutex<ringbuf::HeapCons<f32>>>,
    played_samples: Arc<AtomicU64>,
    sample_rate: u32,
    channels: u16,
    device_name: Option<String>,
    backend_name: String,
    stream_error_flag: Arc<std::sync::atomic::AtomicBool>,
}

fn get_default_device_name() -> Option<String> {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    Some(device.to_string())
}

/// The device to open for the system default output.
///
/// On Windows, cpal's `default_output_device()` is a virtual handle that
/// activates through `ActivateAudioInterfaceAsync` (cpal 0.18 reroute support).
/// For some Bluetooth endpoints that activation fails every time with
/// `RPC_E_CHANGED_MODE` ("Cannot change thread mode after it is set"),
/// whatever the calling thread's COM mode and however often it is retried
/// (#1617). So
/// resolve the current default to the concrete endpoint with the same id and
/// open that, which activates through `IMMDevice::Activate`. Rerouting is not
/// lost: `check_and_rebuild_output` already polls the default and rebuilds.
/// Falls back to the virtual default handle if the endpoint can't be matched.
fn select_output_device(host: &cpal::Host) -> Result<cpal::Device, String> {
    use cpal::traits::HostTrait;

    let default = host
        .default_output_device()
        .ok_or_else(|| "No audio output device".to_string())?;

    #[cfg(windows)]
    {
        use cpal::traits::DeviceTrait;

        let specific = default.id().ok().and_then(|default_id| {
            host.output_devices()
                .ok()?
                .find(|d| d.id().is_ok_and(|id| id == default_id))
        });
        match specific {
            Some(d) => {
                log::debug!("Opening default output as a specific endpoint");
                return Ok(d);
            }
            None => {
                log::warn!("Default output endpoint not found by id; using the virtual default")
            }
        }
    }

    Ok(default)
}

fn resolve_cpal_device(
    host: &cpal::Host,
    requested_id: Option<&str>,
) -> Result<(cpal::Device, Option<String>), String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let get_name = |d: &cpal::Device| -> String {
        d.description()
            .map(|desc| desc.name().to_string())
            .unwrap_or_else(|_| d.to_string())
    };
    if let Some(req_id) = requested_id {
        if !req_id.is_empty() {
            if let Ok(devices) = host.output_devices() {
                for d in devices {
                    let name = get_name(&d);
                    let id = d
                        .id()
                        .ok()
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| name.clone());
                    if id == req_id {
                        return Ok((d, Some(name)));
                    }
                }
            }
        }
    }
    let dev = select_output_device(host)?;
    let name = Some(get_name(&dev));
    Ok((dev, name))
}

/// The output callback's state: everything it reads, plus its preallocated
/// scratch. Kept out of the closure so the hot path can be driven (and timed)
/// without a CPAL device.
pub(crate) struct OutputRenderer {
    pub(crate) shared: Arc<AudioShared>,
    pub(crate) consumer: Arc<Mutex<ringbuf::HeapCons<f32>>>,
    pub(crate) played_samples: Arc<AtomicU64>,
    pub(crate) mono_scratch: Vec<f32>,
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
}

impl OutputRenderer {
    /// Fill one device buffer from the ring buffer and run the gain chain.
    /// Runs on the real-time audio thread: never allocates or blocks.
    pub(crate) fn render(&mut self, output: &mut [f32]) {
        let vol = f32::from_bits(self.shared.volume.load(Ordering::Relaxed));
        let mut loudness = f32::from_bits(self.shared.loudness_gain.load(Ordering::Relaxed));
        let played_before = self.played_samples.load(Ordering::Relaxed);
        let fade = f32::from_bits(self.shared.fade_gain.load(Ordering::Relaxed));
        let mut played = 0;

        // Non-blocking try_lock ensures CPAL callback never stalls
        if let Some(mut consumer) = self.consumer.try_lock() {
            for sample in output.iter_mut() {
                if let Some(s) = consumer.try_pop() {
                    *sample = s;
                    played += 1;
                } else {
                    *sample = 0.0;
                }
            }
        } else {
            for sample in output.iter_mut() {
                *sample = 0.0;
            }
        }

        // DSP chain: loudness gain → EQ preamp → EQ bands → fade
        // envelope → volume. Every stage is skipped when neutral, so
        // with the EQ disabled and all gains at 1.0 the decoded
        // samples reach the device untouched (bit-perfect).

        // 1) Per-track loudness normalization gain (#77). A switch
        // scheduled inside this buffer (crossfade start, #1238)
        // splits it: the old gain up to the switch sample, the new
        // one from there on.
        let loudness_before = loudness;
        let mut split = 0;
        let switch_at = self.shared.loudness_switch_at.load(Ordering::Acquire);
        if let Some(k) = loudness_switch_offset(switch_at, played_before, played) {
            if self
                .shared
                .loudness_switch_at
                .compare_exchange(
                    switch_at,
                    NO_LOUDNESS_SWITCH,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                let new_bits = self.shared.loudness_switch_gain.load(Ordering::Relaxed);
                self.shared.loudness_gain.store(new_bits, Ordering::Relaxed);
                loudness = f32::from_bits(new_bits);
                split = k;
            }
        }
        if loudness_before != 1.0 {
            for sample in output[..split].iter_mut() {
                *sample *= loudness_before;
            }
        }
        if loudness != 1.0 {
            for sample in output[split..played].iter_mut() {
                *sample *= loudness;
            }
        }

        // 2) Equalizer (preamp + band cascade; no-op when disabled)
        let mut eq_applied = false;
        if let Some(mut eq) = self.shared.equalizer.try_lock() {
            eq_applied = eq.process_interleaved(&mut output[..played]);
        }

        // 3) Fade envelope (#79)
        if fade != 1.0 {
            for sample in output[..played].iter_mut() {
                *sample *= fade;
            }
        }

        // 4) Master volume
        if vol != 1.0 {
            for sample in output[..played].iter_mut() {
                *sample *= vol;
            }
        }

        // 5) Final clip guard and true-peak ceiling limiter.
        // When loudness normalization or EQ is active, samples can be boosted
        // significantly past full scale (loudness boost up to +12 dB, or EQ
        // preamp up to +12 dB stacked on top of positive band gains).
        // We enforce a true-peak ceiling (-1.0 dBTP = 0.8912509) to prevent inter-sample
        // clipping during D/A reconstruction.
        // When both loudness normalization and EQ are neutral/disabled, the signal
        // passes through unaltered for bit-perfect output.
        if loudness_before != 1.0 || loudness != 1.0 || eq_applied {
            for sample in output[..played].iter_mut() {
                *sample = sample.clamp(-TRUE_PEAK_CEILING, TRUE_PEAK_CEILING);
            }
        }

        if played > 0 && self.shared.spectrum_enabled.load(Ordering::Relaxed) {
            let channels_u = self.channels as usize;
            self.mono_scratch.clear();
            for chunk in output[..played].chunks(channels_u) {
                let sum: f32 = chunk.iter().sum();
                // Reserved to buffer_capacity up front and cleared per callback.
                // ast-grep-ignore: audio-callback-no-alloc
                self.mono_scratch.push(sum / self.channels as f32);
            }
            // Bounded ring buffer: evicts the oldest block at max_size.
            // ast-grep-ignore: audio-callback-no-alloc
            self.shared.visualizer_buf.push(&self.mono_scratch);
        }

        let total_played = self
            .played_samples
            .fetch_add(played as u64, Ordering::Relaxed)
            + played as u64;
        let pos_ns = (total_played as f64 * 1_000_000_000.0
            / (self.sample_rate as f64 * self.channels as f64)) as u64;
        self.shared.position.store(pos_ns, Ordering::Relaxed);
    }
}

fn build_output(shared: &Arc<AudioShared>) -> Result<AudioOutput, String> {
    use cpal::traits::{DeviceTrait, StreamTrait};

    let output_cfg = shared.output_config.read().clone();

    #[cfg(windows)]
    if output_cfg.backend == AudioBackendKind::Asio {
        let target_sample_rate = shared.output_sample_rate.load(Ordering::Relaxed).max(44100);
        let buffer_capacity = target_sample_rate as usize * 2 * 2;
        let rb = HeapRb::<f32>::new(buffer_capacity);
        let (prod, cons) = rb.split();
        let shared_consumer = Arc::new(Mutex::new(cons));
        let played_samples = Arc::new(AtomicU64::new(0));

        match crate::asio::build_asio_sink(
            output_cfg.device_id.as_deref(),
            shared,
            Arc::clone(&shared_consumer),
            Arc::clone(&played_samples),
            target_sample_rate,
        ) {
            Ok(asio_sink) => {
                let sample_rate = asio_sink.sample_rate();
                let channels = asio_sink.channels();
                let device_name = asio_sink.device_name();
                let stream_error_flag = Arc::clone(&asio_sink.stream_error_flag);

                {
                    let mut eq = shared.equalizer.lock();
                    eq.update_format(sample_rate, channels as usize);
                }

                return Ok(AudioOutput {
                    sink: Box::new(asio_sink),
                    producer: prod,
                    consumer: shared_consumer,
                    played_samples,
                    sample_rate,
                    channels,
                    device_name,
                    backend_name: "ASIO".to_string(),
                    stream_error_flag,
                });
            }
            Err(e) => {
                log::warn!("Failed to initialize ASIO sink ({e}), falling back to CPAL default");
            }
        }
    }

    let host = cpal::default_host();
    let (device, dev_name) = resolve_cpal_device(&host, output_cfg.device_id.as_deref())?;
    let device_name = dev_name;
    let default_config = device
        .default_output_config()
        .map_err(|e| format!("Failed to get default output config: {e}"))?;
    let mut config = default_config.config();

    // Request a buffer size clamped to the device's supported range to prevent underruns
    config.buffer_size = match default_config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => {
            cpal::BufferSize::Fixed(4096.clamp(*min, *max))
        }
        cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Default,
    };

    let target_sample_rate = config.sample_rate;
    let target_channels = config.channels;

    {
        let mut eq = shared.equalizer.lock();
        eq.update_format(target_sample_rate, target_channels as usize);
    }

    // Buffer capacity based on target device format (approx. 2 seconds of audio)
    let buffer_capacity = target_sample_rate as usize * target_channels as usize * 2;
    let rb = HeapRb::<f32>::new(buffer_capacity);
    let (prod, cons) = rb.split();

    // Wrap consumer in a Mutex so that the decode thread can clear it upon
    // Seek/track-change, while the audio callback can perform a
    // non-blocking `try_lock()` on it.
    let shared_consumer = Arc::new(Mutex::new(cons));

    let played_samples = Arc::new(AtomicU64::new(0));
    let mut renderer = OutputRenderer {
        shared: Arc::clone(shared),
        consumer: Arc::clone(&shared_consumer),
        played_samples: Arc::clone(&played_samples),
        // Pre-allocated scratch for the visualizer's mono downmix — the
        // output callback must never allocate. Sized for the whole ring
        // buffer, far larger than any single callback burst.
        mono_scratch: Vec::with_capacity(buffer_capacity),
        sample_rate: target_sample_rate,
        channels: target_channels,
    };

    let stream_error_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let err_flag_cpal = Arc::clone(&stream_error_flag);

    let stream = device
        .build_output_stream(
            config,
            move |output: &mut [f32], _| renderer.render(output),
            move |err| {
                log::error!("CPAL stream error: {err}");
                err_flag_cpal.store(true, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| format!("CPAL stream build failed: {e}"))?;

    // Start paused — nothing decoded yet. The caller starts it once a track
    // is actually ready to play. Some ALSA backends (e.g. the "pulse" plugin,
    // used when routing through PulseAudio/WSLg) don't support pausing a
    // stream that hasn't started, so a failure here is expected and harmless
    // — log it instead of surfacing a spurious error to the frontend.
    if let Err(e) = stream.pause() {
        log::warn!(
            "CPAL stream pause at startup failed (harmless if unsupported by this backend): {e}"
        );
    }

    let cpal_sink = crate::sink::CpalSink::new(
        stream,
        target_sample_rate,
        target_channels,
        device_name.clone(),
        Arc::clone(&stream_error_flag),
    );

    Ok(AudioOutput {
        sink: Box::new(cpal_sink),
        producer: prod,
        consumer: shared_consumer,
        played_samples,
        sample_rate: target_sample_rate,
        channels: target_channels,
        device_name,
        backend_name: "CPAL".to_string(),
        stream_error_flag,
    })
}

/// Runs `build_output` on a brand-new, short-lived OS thread instead of the
/// caller's own thread. Used only for stream *rebuilds* (device switch /
/// stream error) — see #619: once `decode_thread`'s long-lived OS thread has
/// built and torn down a WASAPI stream for one device, re-querying
/// `default_output_config()` on that same thread for a *different* device can
/// fail with `RPC_E_CHANGED_MODE`. A fresh thread has no such history, so its
/// first COM touch (via cpal's own thread-local guard) starts clean.
/// `cpal::Stream` is `Send + Sync`, so the built `AudioOutput` can safely be
/// handed back to the caller. Bounded by a timeout so a wedged WASAPI call on
/// the scratch thread can't hang `decode_thread` — see the wedge risk
/// documented on `AudioOutput` above.
fn build_output_on_fresh_thread(shared: &Arc<AudioShared>) -> Result<AudioOutput, String> {
    let shared = Arc::clone(shared);

    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("luminous-audio-rebuild".into())
        .spawn(move || {
            let result = build_output(&shared);
            let _ = tx.send(result);
        });

    if let Err(e) = spawned {
        return Err(format!("Failed to spawn audio rebuild thread: {e}"));
    }

    rx.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap_or_else(|_| Err("Timed out rebuilding audio output stream".to_string()))
}

/// Mutable state that lives for the duration of one track's inner `'decode`
/// loop iteration in [`decode_thread`]. Kept separate from `output:
/// Option<AudioOutput>` because the output stream is reassigned wholesale on
/// device rebuild while these fields mutate in place — bundling both would
/// force simultaneous partial-borrows of one struct across the extracted
/// helpers below.
struct DecodeSession {
    current: ActiveTrack,
    next: Option<ActiveTrack>,
    transition: Option<PendingTransition>,
    /// Auto-crossfade length requested for the handover into `next`, in
    /// seconds; 0 for a plain gapless handover.
    crossfade_secs: f32,
    /// Loudness gain of the preloaded crossfade track, applied from the
    /// overlap's first sample.
    crossfade_incoming_gain: f32,
    /// The outgoing track while an auto-crossfade overlap is being mixed.
    outgoing: Option<CrossfadeTail>,
    /// Absolute count of samples pushed to the ring buffer, in the same
    /// "space" as `AudioOutput::played_samples` (kept in sync at every
    /// buffer clear).
    pushed_samples: u64,
    target_sample_rate: u32,
    target_channels: u16,
    last_device_check: std::time::Instant,
    force_device_rebuild: bool,
}

enum DeviceCheckOutcome {
    Ok,
    BreakDecode,
    /// The output couldn't be rebuilt; park this request (the interrupted
    /// track at its current position) via `park_without_output`.
    OutputLost(PlayRequest, String),
}

enum CmdOutcome {
    Continue,
    BreakDecode,
    RequeuePlay(PlayRequest),
}

enum EofOutcome {
    NotEof,
    ContinueDecode,
    BreakDecode,
}

enum DecodeStepOutcome {
    Decoded,
    ContinueDecode,
    BreakDecode,
}

fn decode_thread(
    cmd_rx: mpsc::Receiver<AudioCommand>,
    event_tx: mpsc::Sender<AudioEvent>,
    shared: Arc<AudioShared>,
) {
    // The persistent output stream/ring buffer — built lazily on the first
    // track this thread ever plays, then kept alive for every subsequent
    // track, pause, and resume. See `AudioOutput` above.
    let mut output: Option<AudioOutput> = None;

    let mut current_req = None;
    let mut paused_req: Option<PlayRequest> = None;

    'main: loop {
        let mut req = match current_req.take() {
            Some(r) => r,
            None => {
                match cmd_rx.recv() {
                    Ok(AudioCommand::Play(r)) => {
                        paused_req = None;
                        r
                    }
                    Ok(AudioCommand::Cue(r)) => {
                        if output.is_none() {
                            match build_output_on_fresh_thread(&shared) {
                                Ok(o) => {
                                    shared
                                        .output_sample_rate
                                        .store(o.sample_rate, Ordering::Relaxed);
                                    shared.output_channels.store(o.channels, Ordering::Relaxed);
                                    *shared.output_device_name.write() = o.device_name.clone();
                                    output = Some(o);
                                }
                                Err(message) => {
                                    park_without_output(
                                        &shared,
                                        &event_tx,
                                        &mut paused_req,
                                        r,
                                        message,
                                    );
                                    continue;
                                }
                            }
                        }
                        let out = output.as_mut().unwrap();
                        let target_sample_rate = out.sample_rate;
                        let target_channels = out.channels;

                        match ActiveTrack::open(
                            &shared.open_source,
                            r.track.clone(),
                            r.start_nanosec,
                            target_sample_rate,
                            target_channels as usize,
                        ) {
                            Ok(_) => {
                                {
                                    let mut consumer = out.consumer.lock();
                                    while consumer.try_pop().is_some() {}
                                }
                                let start_samples = samples_for_ns(
                                    r.start_nanosec,
                                    target_sample_rate,
                                    target_channels,
                                );
                                out.played_samples.store(start_samples, Ordering::Relaxed);
                                let _ = out.sink.pause();
                                {
                                    let mut s = shared.play_state.lock();
                                    *s = EngineState::Paused;
                                }
                                shared.position.store(r.start_nanosec, Ordering::Relaxed);
                                paused_req = Some(r);
                                let _ = event_tx.send(AudioEvent::Paused);
                            }
                            Err(message) => {
                                let _ = event_tx.send(AudioEvent::Error { message });
                            }
                        }
                        continue;
                    }
                    Ok(AudioCommand::Resume(_)) => {
                        if let Some(r) = paused_req.take() {
                            let cur_pos = shared.position.load(Ordering::Relaxed);
                            PlayRequest {
                                track: r.track,
                                start_nanosec: cur_pos,
                            }
                        } else {
                            continue;
                        }
                    }
                    Ok(AudioCommand::Stop(_)) => {
                        if let Some(out) = output.as_ref() {
                            let _ = out.sink.pause();
                        }
                        paused_req = None;
                        shared.position.store(0, Ordering::Relaxed);
                        {
                            let mut s = shared.play_state.lock();
                            *s = EngineState::Stopped;
                        }
                        let _ = event_tx.send(AudioEvent::Stopped);
                        continue;
                    }
                    Ok(AudioCommand::RebuildOutput) => {
                        if let Some(old_out) = output.take() {
                            let _ = old_out.sink.pause();
                        }
                        match build_output_on_fresh_thread(&shared) {
                            Ok(new_out) => {
                                shared
                                    .output_sample_rate
                                    .store(new_out.sample_rate, Ordering::Relaxed);
                                shared
                                    .output_channels
                                    .store(new_out.channels, Ordering::Relaxed);
                                *shared.output_device_name.write() = new_out.device_name.clone();
                                *shared.output_backend_name.write() = new_out.backend_name.clone();
                                output = Some(new_out);
                                let _ = event_tx.send(AudioEvent::PipelineChanged);
                            }
                            Err(e) => {
                                log::warn!("Rebuilding output stream failed: {e}");
                            }
                        }
                        continue;
                    }
                    // A seek on a cued (restored at startup, never resumed)
                    // track only moves the start point — `Resume` above
                    // opens the track from `shared.position`.
                    Ok(AudioCommand::SeekTo(target_ns)) if paused_req.is_some() => {
                        if let Some(out) = output.as_ref() {
                            out.played_samples.store(
                                samples_for_ns(target_ns, out.sample_rate, out.channels),
                                Ordering::Relaxed,
                            );
                        }
                        shared.position.store(target_ns, Ordering::Relaxed);
                        continue;
                    }
                    Ok(_) => continue, // Ignore other commands when stopped
                    Err(_) => break,   // Channel disconnected
                }
            }
        };

        // Coalesce a burst of rapid Play requests (e.g. mashing "skip") into
        // just the last one, so a burst of clicks decodes/discards at most
        // one superseded track instead of several in a row.
        loop {
            match cmd_rx.try_recv() {
                Ok(AudioCommand::Play(newer)) => req = newer,
                Ok(AudioCommand::Stop(_)) => {
                    if let Some(out) = output.as_ref() {
                        let _ = out.sink.pause();
                    }
                    paused_req = None;
                    shared.position.store(0, Ordering::Relaxed);
                    {
                        let mut s = shared.play_state.lock();
                        *s = EngineState::Stopped;
                    }
                    let _ = event_tx.send(AudioEvent::Stopped);
                    continue 'main;
                }
                // Pause/Resume/SeekTo/Preload target a track that hasn't
                // started playing yet at this point — nothing to apply.
                Ok(_) => {}
                Err(_) => break,
            }
        }

        // Ensure the persistent output stream exists (built lazily on the
        // first track this thread ever plays; reused for every track after).
        // Built on a fresh thread — `output` can already be `None` here after
        // a mid-session device-change rebuild (see `check_and_rebuild_output`),
        // and re-touching COM on `decode_thread`'s own long-lived OS thread a
        // second time risks the same RPC_E_CHANGED_MODE wedge #624 fixed.
        if output.is_none() {
            match build_output_on_fresh_thread(&shared) {
                Ok(o) => {
                    shared
                        .output_sample_rate
                        .store(o.sample_rate, Ordering::Relaxed);
                    shared.output_channels.store(o.channels, Ordering::Relaxed);
                    *shared.output_device_name.write() = o.device_name.clone();
                    output = Some(o);
                }
                Err(message) => {
                    park_without_output(&shared, &event_tx, &mut paused_req, req, message);
                    continue;
                }
            }
        }
        let out = output.as_mut().unwrap();
        let target_sample_rate = out.sample_rate;
        let target_channels = out.channels;

        let current = match ActiveTrack::open(
            &shared.open_source,
            req.track,
            req.start_nanosec,
            target_sample_rate,
            target_channels as usize,
        ) {
            Ok(t) => t,
            Err(message) => {
                let _ = event_tx.send(AudioEvent::Error { message });
                continue;
            }
        };
        let track_id = current.spec.id;

        // Clear whatever was left in the buffer from the previous track and
        // reset the played-sample counter for this track's start offset.
        {
            let mut consumer = out.consumer.lock();
            while consumer.try_pop().is_some() {}
        }
        let start_samples = samples_for_ns(current.start_ns, target_sample_rate, target_channels);
        out.played_samples.store(start_samples, Ordering::Relaxed);
        shared.cancel_loudness_switch();

        if let Err(e) = out.sink.play() {
            let _ = event_tx.send(AudioEvent::Error {
                message: format!("Audio stream play failed: {e}"),
            });
            continue;
        }

        {
            let mut s = shared.play_state.lock();
            *s = EngineState::Playing;
        }
        shared.position.store(current.start_ns, Ordering::Relaxed);
        let _ = event_tx.send(AudioEvent::Playing { track_id });

        let mut session = DecodeSession {
            current,
            next: None,
            transition: None,
            crossfade_secs: 0.0,
            crossfade_incoming_gain: 1.0,
            outgoing: None,
            pushed_samples: start_samples,
            target_sample_rate,
            target_channels,
            last_device_check: std::time::Instant::now(),
            force_device_rebuild: false,
        };

        'decode: loop {
            match check_and_rebuild_output(&mut output, &mut session, &shared, &event_tx) {
                DeviceCheckOutcome::Ok => {}
                DeviceCheckOutcome::BreakDecode => break 'decode,
                DeviceCheckOutcome::OutputLost(lost_req, message) => {
                    park_without_output(&shared, &event_tx, &mut paused_req, lost_req, message);
                    break 'decode;
                }
            }

            let out = output.as_mut().unwrap();

            match handle_decode_command(
                cmd_rx.try_recv(),
                out,
                &mut session,
                &shared,
                &event_tx,
                &mut paused_req,
            ) {
                CmdOutcome::Continue => {}
                CmdOutcome::BreakDecode => break 'decode,
                CmdOutcome::RequeuePlay(new_req) => {
                    current_req = Some(new_req);
                    break 'decode;
                }
            }

            advance_transition_and_preload_signal(out, &mut session, &shared.position, &event_tx);

            maybe_start_crossfade(&mut session, &shared);

            match handle_eof(out, &mut session, &shared.play_state, &event_tx) {
                EofOutcome::NotEof => {}
                EofOutcome::ContinueDecode => continue 'decode,
                EofOutcome::BreakDecode => break 'decode,
            }

            match decode_one_packet(out, &mut session, &event_tx) {
                DecodeStepOutcome::Decoded => {}
                DecodeStepOutcome::ContinueDecode => continue 'decode,
                DecodeStepOutcome::BreakDecode => break 'decode,
            }
        }
    }
}

/// Drives step 1 of one `'decode` iteration: the periodic (~500ms throttled)
/// default-output-device change / stream-error check, rebuilding `output`
/// and reseeking `session.current`/`session.next` onto it when needed.
fn check_and_rebuild_output(
    output: &mut Option<AudioOutput>,
    session: &mut DecodeSession,
    shared: &Arc<AudioShared>,
    event_tx: &mpsc::Sender<AudioEvent>,
) -> DeviceCheckOutcome {
    let now = std::time::Instant::now();
    let check_due =
        now.duration_since(session.last_device_check) >= std::time::Duration::from_millis(500);
    let stream_errored = output
        .as_ref()
        .map(|o| o.stream_error_flag.load(Ordering::Relaxed))
        .unwrap_or(false);

    if stream_errored || check_due {
        session.last_device_check = now;
        let current_dev_name = get_default_device_name();
        let old_dev_name = output.as_ref().and_then(|o| o.device_name.clone());
        let device_changed = current_dev_name != old_dev_name;

        if stream_errored || device_changed {
            log::info!(
                "Audio output device change/error detected (old: {:?}, new: {:?}, errored: {}). Rebuilding audio stream...",
                old_dev_name,
                current_dev_name,
                stream_errored
            );

            let cur_pos = shared.position.load(Ordering::Relaxed);
            // Position still belongs to the finished track while a transition
            // is draining, so that's the song to park on a failed rebuild.
            let track_if_lost = match session.transition.as_ref() {
                Some(t) => t.finished.clone(),
                None => session.current.spec.clone(),
            };
            if let Some(old_out) = output.as_ref() {
                let _ = old_out.sink.pause();
            }
            *output = None;

            match build_output_on_fresh_thread(shared) {
                Ok(new_out) => {
                    shared
                        .output_sample_rate
                        .store(new_out.sample_rate, Ordering::Relaxed);
                    shared
                        .output_channels
                        .store(new_out.channels, Ordering::Relaxed);
                    *shared.output_device_name.write() = new_out.device_name.clone();
                    *shared.output_backend_name.write() = new_out.backend_name.clone();
                    session.target_sample_rate = new_out.sample_rate;
                    session.target_channels = new_out.channels;

                    session.outgoing = None;
                    shared.cancel_loudness_switch();
                    if let Some(t) = session.transition.take() {
                        // Mid-handover: `cur_pos` is still on the finished
                        // track's timeline, so reopen that track there (as
                        // SeekTo does) rather than seeking the new one to it.
                        session.next = None;
                        match ActiveTrack::open(
                            &shared.open_source,
                            t.finished,
                            cur_pos,
                            session.target_sample_rate,
                            session.target_channels as usize,
                        ) {
                            Ok(t) => session.current = t,
                            Err(message) => {
                                let _ = event_tx.send(AudioEvent::Error { message });
                                return DeviceCheckOutcome::BreakDecode;
                            }
                        }
                    } else {
                        let target_time = symphonia::core::units::Time::from_nanos(cur_pos as i64);
                        let _ = session.current.format.seek(
                            symphonia::core::formats::SeekMode::Accurate,
                            symphonia::core::formats::SeekTo::Time {
                                time: target_time,
                                track_id: Some(session.current.track_id),
                            },
                        );
                        session.current.decoder.reset();
                        session.current.decoded_pos_ns = cur_pos;
                        session.current.eof = false;
                        session.current.resampler = Resampler::new(
                            session.current.src_rate,
                            session.target_sample_rate,
                            session.target_channels as usize,
                        );
                    }

                    if let Some(n) = session.next.as_mut() {
                        n.resampler = Resampler::new(
                            n.src_rate,
                            session.target_sample_rate,
                            session.target_channels as usize,
                        );
                    }

                    {
                        let mut consumer = new_out.consumer.lock();
                        while consumer.try_pop().is_some() {}
                    }
                    let start_samples = samples_for_ns(
                        cur_pos,
                        session.target_sample_rate,
                        session.target_channels,
                    );
                    new_out
                        .played_samples
                        .store(start_samples, Ordering::Relaxed);
                    session.pushed_samples = start_samples;

                    if let Err(e) = new_out.sink.play() {
                        let _ = event_tx.send(AudioEvent::Error {
                            message: format!("Audio sink play failed: {e}"),
                        });
                    }
                    *output = Some(new_out);
                    let _ = event_tx.send(AudioEvent::PipelineChanged);
                }
                Err(message) => {
                    return DeviceCheckOutcome::OutputLost(
                        PlayRequest {
                            track: track_if_lost,
                            start_nanosec: cur_pos,
                        },
                        message,
                    );
                }
            }
        }
    }
    DeviceCheckOutcome::Ok
}

/// Parks `req` as a paused track because there is no output to play it on
/// (a device switch or lazy build failed). Afterwards the track sits at
/// `req.start_nanosec` exactly as if the user had paused it, so `Resume`
/// reopens it and retries the output build; a repeated failure parks it
/// again. Emits `Paused` (so the UI, scrobbler and media session update as
/// for any pause) and then `OutputUnavailable`. The caller leaves the decode
/// loop; the player never sees a track error, so nothing is skipped.
fn park_without_output(
    shared: &AudioShared,
    event_tx: &mpsc::Sender<AudioEvent>,
    paused_req: &mut Option<PlayRequest>,
    req: PlayRequest,
    message: String,
) {
    shared.position.store(req.start_nanosec, Ordering::Relaxed);
    *shared.play_state.lock() = EngineState::Paused;
    *paused_req = Some(req);
    let _ = event_tx.send(AudioEvent::Paused);
    let _ = event_tx.send(AudioEvent::OutputUnavailable { message });
}

/// Drives step 2 of one `'decode` iteration: handle exactly one
/// `AudioCommand` variant received via `try_recv()` (or none pending).
fn handle_decode_command(
    cmd: Result<AudioCommand, mpsc::TryRecvError>,
    out: &mut AudioOutput,
    session: &mut DecodeSession,
    shared: &Arc<AudioShared>,
    event_tx: &mpsc::Sender<AudioEvent>,
    paused_req: &mut Option<PlayRequest>,
) -> CmdOutcome {
    let play_state = &shared.play_state;
    let fade_gain = &shared.fade_gain;
    let position = &shared.position;
    match cmd {
        Ok(AudioCommand::Pause(dur_ms)) => {
            fade_out_then(fade_gain, dur_ms, || {
                let _ = out.sink.pause();
            });
            {
                let mut s = play_state.lock();
                *s = EngineState::Paused;
            }
            let _ = event_tx.send(AudioEvent::Paused);
            let song_for_resume = match session.transition.as_ref() {
                Some(t) => t.finished.clone(),
                None => session.current.spec.clone(),
            };
            *paused_req = Some(PlayRequest {
                track: song_for_resume,
                start_nanosec: position.load(Ordering::Relaxed),
            });
            CmdOutcome::BreakDecode
        }
        Ok(AudioCommand::Stop(dur_ms)) => {
            fade_out_then(fade_gain, dur_ms, || {
                let _ = out.sink.pause();
            });
            {
                let mut s = play_state.lock();
                *s = EngineState::Stopped;
            }
            let _ = event_tx.send(AudioEvent::Stopped);
            CmdOutcome::BreakDecode
        }
        Ok(AudioCommand::Play(new_req)) => {
            let _ = event_tx.send(AudioEvent::Stopped);
            CmdOutcome::RequeuePlay(new_req)
        }
        Ok(AudioCommand::SeekTo(target_ns)) => {
            log::debug!("SeekTo command received. target_ns: {target_ns}");

            // A seek cuts any crossfade tail still being mixed in.
            session.outgoing = None;
            shared.cancel_loudness_switch();
            if let Some(t) = session.transition.take() {
                // Mid-handover seek: the audible position is still in
                // the finished track but its decoder is gone — reopen
                // it fresh at the target. The preload is dropped so
                // AboutToFinish re-arms naturally on the new track.
                session.next = None;
                match ActiveTrack::open(
                    &shared.open_source,
                    t.finished,
                    target_ns,
                    session.target_sample_rate,
                    session.target_channels as usize,
                ) {
                    Ok(t) => session.current = t,
                    Err(message) => {
                        let _ = event_tx.send(AudioEvent::Error { message });
                        return CmdOutcome::BreakDecode;
                    }
                }
            } else {
                let target_time = symphonia::core::units::Time::from_nanos(target_ns as i64);
                let seek_res = session.current.format.seek(
                    symphonia::core::formats::SeekMode::Accurate,
                    symphonia::core::formats::SeekTo::Time {
                        time: target_time,
                        track_id: Some(session.current.track_id),
                    },
                );
                match seek_res {
                    Ok(seeked_to) => {
                        session.current.decoder.reset();
                        log::info!("Seek successful: {seeked_to:?}");
                    }
                    Err(e) => {
                        log::error!("Seek failed: {e:?}");
                    }
                }
                session.current.decoded_pos_ns = target_ns;
                session.current.eof = false;
                session.current.resampler = Resampler::new(
                    session.current.src_rate,
                    session.target_sample_rate,
                    session.target_channels as usize,
                );
            }

            // Clear the buffer after seek to avoid stale audio
            {
                let mut consumer = out.consumer.lock();
                while consumer.try_pop().is_some() {}
            }
            let target_samples = samples_for_ns(
                target_ns,
                session.target_sample_rate,
                session.target_channels,
            );
            out.played_samples.store(target_samples, Ordering::Relaxed);
            session.pushed_samples = target_samples;
            position.store(target_ns, Ordering::Relaxed);
            CmdOutcome::Continue
        }
        Ok(AudioCommand::PreloadNext(preq)) => {
            match ActiveTrack::open(
                &shared.open_source,
                preq.track,
                preq.start_nanosec,
                session.target_sample_rate,
                session.target_channels as usize,
            ) {
                Ok(t) => {
                    log::debug!("Preloaded next track {} for gapless", t.spec.id);
                    session.next = Some(t);
                    session.crossfade_secs = 0.0;
                }
                Err(e) => {
                    // Fall back to the drain + TrackFinished path;
                    // the player will issue a normal Play.
                    log::warn!("Gapless preload failed: {e}");
                    session.next = None;
                }
            }
            CmdOutcome::Continue
        }
        Ok(AudioCommand::PreloadNextCrossfade(preq, secs, gain)) => {
            match ActiveTrack::open(
                &shared.open_source,
                preq.track,
                preq.start_nanosec,
                session.target_sample_rate,
                session.target_channels as usize,
            ) {
                Ok(t) => {
                    log::debug!("Preloaded next track {} for crossfade", t.spec.id);
                    session.next = Some(t);
                    session.crossfade_secs = secs;
                    session.crossfade_incoming_gain = gain;
                }
                Err(e) => {
                    log::warn!("Crossfade preload failed: {e}");
                    session.next = None;
                    session.crossfade_secs = 0.0;
                }
            }
            CmdOutcome::Continue
        }
        Ok(AudioCommand::ClearPreload) => {
            if session.transition.is_none() {
                session.next = None;
                session.crossfade_secs = 0.0;
                session.current.about_to_finish_sent = false;
            }
            CmdOutcome::Continue
        }
        Err(mpsc::TryRecvError::Empty) => CmdOutcome::Continue,
        Err(mpsc::TryRecvError::Disconnected) => CmdOutcome::BreakDecode,
        Ok(AudioCommand::Resume(_)) => CmdOutcome::Continue, // already playing
        Ok(AudioCommand::Cue(_)) => CmdOutcome::Continue,    // already playing
        Ok(AudioCommand::RebuildOutput) => {
            session.force_device_rebuild = true;
            CmdOutcome::Continue
        }
    }
}

/// Drives steps 3+4 of one `'decode` iteration: complete a pending gapless
/// handover once its boundary has been played, then fire the one-shot
/// `AboutToFinish` signal if the preload window has been entered. Neither
/// original block branches the loop's control flow, so both stay combined
/// here as plain side-effecting steps.
fn advance_transition_and_preload_signal(
    out: &AudioOutput,
    session: &mut DecodeSession,
    position: &AtomicU64,
    event_tx: &mpsc::Sender<AudioEvent>,
) {
    // Complete a pending gapless handover once the output callback has
    // actually consumed the finished track's last sample.
    if let Some(t) = session.transition.as_ref() {
        let played = out.played_samples.load(Ordering::Relaxed);
        if played >= t.boundary_samples {
            let next_start_samples = samples_for_ns(
                session.current.start_ns,
                session.target_sample_rate,
                session.target_channels,
            );
            // Rebase the sample counter onto the new track's timeline.
            // fetch_sub composes safely with the callback's concurrent
            // fetch_add.
            if t.boundary_samples >= next_start_samples {
                let delta = t.boundary_samples - next_start_samples;
                out.played_samples.fetch_sub(delta, Ordering::Relaxed);
                session.pushed_samples -= delta;
            } else {
                let delta = next_start_samples - t.boundary_samples;
                out.played_samples.fetch_add(delta, Ordering::Relaxed);
                session.pushed_samples += delta;
            }
            position.store(session.current.start_ns, Ordering::Relaxed);
            let finished_track_id = t.finished.id;
            let _ = event_tx.send(AudioEvent::TrackTransitioned {
                finished_track_id,
                track_id: session.current.spec.id,
            });
            session.transition = None;
        }
    }

    // "About to finish" signal: fires once per track when the audible
    // position enters the preload window before the end boundary.
    // Suppressed while a handover is draining (the position still belongs
    // to the previous track then).
    if !session.current.about_to_finish_sent && session.transition.is_none() {
        if let Some(about_end) = session.current.about_end_ns {
            let pos = position.load(Ordering::Relaxed);
            if pos + PRELOAD_LEAD_NS >= about_end {
                session.current.about_to_finish_sent = true;
                let _ = event_tx.send(AudioEvent::AboutToFinish {
                    track_id: session.current.spec.id,
                });
            }
        }
    }
}

/// Drives step 5 of one `'decode` iteration: end-of-track handling — wait
/// out an in-progress transition, start a new one when a preloaded `next`
/// track is ready, or drain the buffer then emit `TrackFinished`.
fn handle_eof(
    out: &mut AudioOutput,
    session: &mut DecodeSession,
    play_state: &Mutex<EngineState>,
    event_tx: &mpsc::Sender<AudioEvent>,
) -> EofOutcome {
    if !session.current.eof {
        return EofOutcome::NotEof;
    }
    // The incoming track ended inside its own crossfade window (its tagged
    // length overstated it) — nothing left to mix the tail into.
    session.outgoing = None;

    if session.transition.is_some() {
        // Waiting for the boundary to be consumed before the next (already
        // fully decoded) track can take over.
        std::thread::sleep(std::time::Duration::from_millis(10));
        return EofOutcome::ContinueDecode;
    }
    if let Some(n) = session.next.take() {
        // Gapless handover: continue decoding the preloaded track into the
        // same ring buffer — no drain, no pause. Also the fallback when a
        // crossfade never started (the track ended before its tagged length).
        session.crossfade_secs = 0.0;
        session.transition = Some(PendingTransition {
            boundary_samples: session.pushed_samples,
            finished: std::mem::replace(&mut session.current, n).spec,
        });
        return EofOutcome::ContinueDecode;
    }

    // No preloaded next — classic drain-then-finish path.
    let is_empty = out.producer.occupied_len() == 0;
    if is_empty {
        let _ = event_tx.send(AudioEvent::TrackFinished {
            track_id: session.current.spec.id,
        });
        let _ = out.sink.pause();
        {
            let mut s = play_state.lock();
            *s = EngineState::Stopped;
        }
        EofOutcome::BreakDecode
    } else {
        // Buffer still has remaining audio, wait for it to be played
        std::thread::sleep(std::time::Duration::from_millis(20));
        EofOutcome::ContinueDecode
    }
}

/// Drives steps 6+7 of one `'decode` iteration: the ring-buffer-full rate
/// limit, then decoding one packet (CUE end-boundary truncation, channel
/// conversion, resampling, and pushing into the ring buffer).
fn decode_one_packet(
    out: &mut AudioOutput,
    session: &mut DecodeSession,
    event_tx: &mpsc::Sender<AudioEvent>,
) -> DecodeStepOutcome {
    // Rate limit: if the buffer is full (more than 1.5 seconds of audio), sleep
    let is_full = out.producer.occupied_len()
        > (session.target_sample_rate as usize * session.target_channels as usize * 3 / 2);

    if is_full {
        std::thread::sleep(std::time::Duration::from_millis(20));
        return DecodeStepOutcome::ContinueDecode;
    }

    let channels = session.target_channels as usize;
    let mut resampled = match decode_track_packet(&mut session.current, channels) {
        PacketStep::Samples(samples) => samples,
        PacketStep::Skip => return DecodeStepOutcome::ContinueDecode,
        PacketStep::DecoderFailed => return DecodeStepOutcome::BreakDecode,
        PacketStep::ReadFailed(message) => {
            let _ = event_tx.send(AudioEvent::Error { message });
            return DecodeStepOutcome::BreakDecode;
        }
    };

    if let Some(tail) = session.outgoing.as_mut() {
        if mix_outgoing_tail(tail, &mut resampled, channels) {
            session.outgoing = None;
        }
    }

    let mut pushed = 0;
    while pushed < resampled.len() {
        let written = out.producer.push_slice(&resampled[pushed..]);
        if written == 0 {
            // Ring buffer is full, sleep a bit and try again
            std::thread::sleep(std::time::Duration::from_millis(5));
        } else {
            pushed += written;
        }
    }
    session.pushed_samples += resampled.len() as u64;
    DecodeStepOutcome::Decoded
}

/// Result of reading and decoding one packet of an `ActiveTrack`.
enum PacketStep {
    /// Output-format samples (target channel count and rate), ready to push.
    Samples(Vec<f32>),
    /// Nothing to push this time: a packet of another stream, a recoverable
    /// decode error, or end of track (`track.eof` is set).
    Skip,
    /// The decoder failed unrecoverably.
    DecoderFailed,
    /// Reading the container failed; carries the message for `AudioEvent::Error`.
    ReadFailed(String),
}

/// Read and decode one packet of `track`: CUE end-boundary truncation,
/// channel conversion and resampling to the output format. Sets `track.eof`
/// at the end of the stream or the CUE boundary. Shared by the current track
/// and the outgoing tail of a crossfade.
fn decode_track_packet(track: &mut ActiveTrack, target_channels: usize) -> PacketStep {
    match track.format.next_packet() {
        Ok(Some(packet)) => {
            if packet.track_id != track.track_id {
                return PacketStep::Skip;
            }
            match track.decoder.decode(&packet) {
                Ok(decoded) => {
                    let mut sample_vec: Vec<f32> = Vec::new();
                    decoded.copy_to_vec_interleaved(&mut sample_vec);
                    let mut samples: &[f32] = &sample_vec;

                    // Enforce the CUE end boundary (`end_nanosec`):
                    // truncate the packet at the cut and treat the
                    // remainder of the file as EOF.
                    let frames = samples.len() / track.src_channels;
                    let packet_ns =
                        (frames as f64 * 1_000_000_000.0 / track.src_rate as f64) as u64;
                    if let Some(end_ns) = track.end_ns {
                        if track.decoded_pos_ns >= end_ns {
                            track.eof = true;
                            return PacketStep::Skip;
                        }
                        if track.decoded_pos_ns + packet_ns > end_ns {
                            let keep_frames =
                                ((end_ns - track.decoded_pos_ns) as f64 * track.src_rate as f64
                                    / 1_000_000_000.0) as usize;
                            samples = &samples[..keep_frames * track.src_channels];
                            track.eof = true;
                        }
                    }
                    track.decoded_pos_ns += packet_ns;

                    let channel_converted =
                        convert_channels(samples, track.src_channels, target_channels);
                    PacketStep::Samples(track.resampler.resample(&channel_converted))
                }
                Err(SymphoniaError::DecodeError(_)) => PacketStep::Skip,
                Err(_) => PacketStep::DecoderFailed,
            }
        }
        Ok(None) => {
            track.eof = true;
            PacketStep::Skip
        }
        Err(SymphoniaError::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            track.eof = true;
            PacketStep::Skip
        }
        Err(e) => PacketStep::ReadFailed(format!("Decode error: {e}")),
    }
}

/// Mix the next stretch of a crossfade's outgoing tail into the incoming
/// samples `b`, decoding more of the tail as needed. A tail that ends or
/// fails early is mixed as silence, so the incoming track still ramps up
/// over the whole window. Returns true once the overlap is complete.
fn mix_outgoing_tail(tail: &mut CrossfadeTail, b: &mut [f32], channels: usize) -> bool {
    let need = b.len().min(tail.total.saturating_sub(tail.mixed));
    while tail.fifo.len() < need && !tail.track.eof {
        match decode_track_packet(&mut tail.track, channels) {
            PacketStep::Samples(samples) => tail.fifo.extend_from_slice(&samples),
            PacketStep::Skip => {}
            PacketStep::DecoderFailed | PacketStep::ReadFailed(_) => {
                log::warn!("Crossfade tail decode failed; fading in from silence");
                tail.track.eof = true;
            }
        }
    }
    let avail = tail.fifo.len().min(need);
    let n = mix_crossfade(
        b,
        &tail.fifo[..avail],
        tail.gain,
        tail.mixed,
        tail.total,
        channels,
    );
    tail.fifo.drain(..avail);
    tail.mixed += n;
    tail.mixed >= tail.total
}

/// Gain that keeps a crossfade tail at `old_gain` once the mix is scaled by
/// `new_gain`. A (near-)silent incoming gain leaves the tail unscaled rather
/// than blowing it up.
fn tail_gain_ratio(old_gain: f32, new_gain: f32) -> f32 {
    if new_gain > 1e-4 {
        old_gain / new_gain
    } else {
        1.0
    }
}

/// Start an auto-crossfade (#1238) once the current track's decode position
/// enters the crossfade window before its end: the preloaded `next` becomes
/// `current` and the old track is kept as `outgoing`, to be mixed into the
/// incoming audio by `decode_one_packet`. The handover is recorded as a
/// `PendingTransition` at the overlap's start, so the position rebase and
/// `TrackTransitioned` fire exactly as for a gapless handover — just when the
/// crossfade becomes audible. If the overlap works out to zero the preload
/// stays in `next` and `handle_eof` hands over gaplessly.
fn maybe_start_crossfade(session: &mut DecodeSession, shared: &AudioShared) {
    if session.crossfade_secs <= 0.0
        || session.outgoing.is_some()
        || session.transition.is_some()
        || session.current.eof
    {
        return;
    }
    let Some(next) = session.next.as_ref() else {
        return;
    };
    let Some(about_end) = session.current.about_end_ns else {
        return;
    };
    let window_ns = (session.crossfade_secs as f64 * 1_000_000_000.0) as u64;
    if session.current.decoded_pos_ns.saturating_add(window_ns) < about_end {
        return;
    }
    let total = crossfade_overlap_samples(
        session.crossfade_secs,
        Some(about_end.saturating_sub(session.current.decoded_pos_ns)),
        next.about_end_ns
            .map(|end| end.saturating_sub(next.start_ns)),
        session.target_sample_rate,
        session.target_channels,
    );
    session.crossfade_secs = 0.0;
    if total == 0 {
        return;
    }
    let Some(incoming) = session.next.take() else {
        return;
    };
    let old = std::mem::replace(&mut session.current, incoming);
    log::debug!(
        "Crossfading {} -> {} over {} samples",
        old.spec.id,
        session.current.spec.id,
        total
    );
    session.transition = Some(PendingTransition {
        boundary_samples: session.pushed_samples,
        finished: old.spec.clone(),
    });
    // The loudness gain is one global slot applied after mixing: switch it to
    // the incoming track's at the overlap's first sample, and pre-scale the
    // tail so it still plays at its own normalized level.
    let old_gain = f32::from_bits(shared.loudness_gain.load(Ordering::Relaxed));
    let new_gain = session.crossfade_incoming_gain;
    shared.schedule_loudness_switch(session.pushed_samples, new_gain);
    session.outgoing = Some(CrossfadeTail {
        track: old,
        total,
        mixed: 0,
        gain: tail_gain_ratio(old_gain, new_gain),
        fifo: Vec::with_capacity(total.min(1 << 16)),
    });
}

// ---------------------------------------------------------------------------
// Resampling & Channel Conversion Helpers
// ---------------------------------------------------------------------------

fn convert_channels(input: &[f32], from_channels: usize, to_channels: usize) -> Vec<f32> {
    if from_channels == to_channels {
        return input.to_vec();
    }
    let num_frames = input.len() / from_channels;
    let mut output = Vec::with_capacity(num_frames * to_channels);
    for frame_idx in 0..num_frames {
        let frame = &input[frame_idx * from_channels..(frame_idx + 1) * from_channels];
        match (from_channels, to_channels) {
            (1, 2) => {
                let val = frame[0];
                output.push(val);
                output.push(val);
            }
            (2, 1) => {
                let val = (frame[0] + frame[1]) * 0.5;
                output.push(val);
            }
            (1, n) => {
                let val = frame[0];
                for _ in 0..n {
                    output.push(val);
                }
            }
            (_, n) => {
                output.extend((0..n).map(|i| frame.get(i).copied().unwrap_or(0.0)));
            }
        }
    }
    output
}

struct Resampler {
    from_rate: u32,
    to_rate: u32,
    channels: usize,
    phase: f64,
    last_frame: Vec<f32>,
}

impl Resampler {
    fn new(from_rate: u32, to_rate: u32, channels: usize) -> Self {
        Self {
            from_rate,
            to_rate,
            channels,
            phase: 0.0,
            last_frame: vec![0.0; channels],
        }
    }

    fn resample(&mut self, input: &[f32]) -> Vec<f32> {
        if self.from_rate == self.to_rate {
            return input.to_vec();
        }
        let ratio = self.from_rate as f64 / self.to_rate as f64;
        let num_input_frames = input.len() / self.channels;
        let mut output = Vec::new();

        let get_frame = |idx: usize| -> &[f32] {
            if idx == 0 {
                &self.last_frame
            } else {
                let start = (idx - 1) * self.channels;
                &input[start..start + self.channels]
            }
        };

        let total_frames = num_input_frames + 1;
        let mut current_phase = self.phase;

        loop {
            let idx = current_phase.floor() as usize;
            if idx + 1 >= total_frames {
                break;
            }
            let frac = current_phase - idx as f64;
            let next_idx = idx + 1;

            let frame_now = get_frame(idx);
            let frame_next = get_frame(next_idx);

            for c in 0..self.channels {
                let val = frame_now[c] + frac as f32 * (frame_next[c] - frame_now[c]);
                output.push(val);
            }

            current_phase += ratio;
        }

        if num_input_frames > 0 {
            let last_start = (num_input_frames - 1) * self.channels;
            self.last_frame
                .copy_from_slice(&input[last_start..last_start + self.channels]);
            self.phase = current_phase - num_input_frames as f64;
        } else {
            self.phase = current_phase;
        }

        output
    }
}

/// Writes a 16-bit PCM WAV with one `Vec` of samples per channel (all the
/// same length), for tests that need a real file on the offline decode path.
/// Also used by the app crate's tests, through the `test-util` feature.
#[cfg(any(test, feature = "test-util"))]
pub fn write_test_pcm16_wav(path: &Path, sample_rate: u32, channels: &[Vec<i16>]) {
    let channel_count = channels.len() as u16;
    let frames = channels.first().map_or(0, Vec::len);
    let block_align = channel_count * 2;
    let mut data = Vec::with_capacity(frames * block_align as usize);
    for frame in 0..frames {
        for channel in channels {
            data.extend_from_slice(&channel[frame].to_le_bytes());
        }
    }

    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&channel_count.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * block_align as u32).to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::write(path, wav).expect("failed to write test wav fixture");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_shared() -> AudioShared {
        AudioShared {
            position: AtomicU64::new(0),
            volume: AtomicU32::new(1.0f32.to_bits()),
            open_source: Box::new(open_file),
            play_state: Mutex::new(EngineState::Playing),
            visualizer_buf: spectrum::AudioVisualizerBuffer::new(4096),
            spectrum_enabled: std::sync::atomic::AtomicBool::new(false),
            output_sample_rate: AtomicU32::new(44100),
            output_channels: std::sync::atomic::AtomicU16::new(2),
            output_device_name: parking_lot::RwLock::new(None),
            output_backend_name: parking_lot::RwLock::new("Test".to_string()),
            output_config: parking_lot::RwLock::new(crate::sink::AudioOutputConfig::default()),
            equalizer: Mutex::new(luminous_equalizer::Equalizer::new()),
            loudness_gain: AtomicU32::new(1.0f32.to_bits()),
            loudness_generation: AtomicU64::new(0),
            loudness_switch_at: AtomicU64::new(NO_LOUDNESS_SWITCH),
            loudness_switch_gain: AtomicU32::new(1.0f32.to_bits()),
            fade_gain: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    #[test]
    fn park_without_output_pauses_at_position_and_reports_device_failure() {
        let shared = test_shared();
        let (tx, rx) = mpsc::channel();
        let mut paused_req = None;
        let req = PlayRequest {
            track: Track::default(),
            start_nanosec: 42_000,
        };

        park_without_output(&shared, &tx, &mut paused_req, req, "no device".into());

        assert_eq!(shared.position.load(Ordering::Relaxed), 42_000);
        assert_eq!(*shared.play_state.lock(), EngineState::Paused);
        assert_eq!(paused_req.expect("parked").start_nanosec, 42_000);
        assert!(matches!(rx.try_recv(), Ok(AudioEvent::Paused)));
        match rx.try_recv() {
            Ok(AudioEvent::OutputUnavailable { message }) => assert_eq!(message, "no device"),
            other => panic!("expected OutputUnavailable, got {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "no track Error may be emitted");
    }

    #[test]
    fn equal_power_gains_keep_constant_power() {
        assert_eq!(equal_power_gains(0.0), (1.0, 0.0));
        let (a_end, b_end) = equal_power_gains(1.0);
        assert!(a_end.abs() < 1e-6 && (b_end - 1.0).abs() < 1e-6);
        for step in 0..=100 {
            let (a, b) = equal_power_gains(step as f32 / 100.0);
            assert!((a * a + b * b - 1.0).abs() < 1e-6, "p = {step}/100");
        }
    }

    #[test]
    fn mix_crossfade_overlap_matches_configured_length() {
        // Outgoing = 1.0, incoming = 0.0, stereo: a sample differs from pure
        // incoming exactly where the outgoing tail is still audible.
        let channels = 2;
        let total = 1_000 * channels;
        let mut mixed = 0;
        let mut out = Vec::new();
        // Uneven chunk sizes, as packets from two files never line up.
        for chunk in [74, 500, 2, 1_200, 800] {
            let mut b = vec![0.0f32; chunk];
            let a = vec![1.0f32; chunk];
            mixed += mix_crossfade(&mut b, &a, 1.0, mixed, total, channels);
            out.extend(b);
        }
        assert_eq!(mixed, total);
        assert!(out[..total].iter().all(|&s| s > 0.0));
        assert!(out[total..].iter().all(|&s| s == 0.0));
        // Both channels of a frame share one gain; the curve falls monotonically.
        for frame in out[..total].chunks(channels) {
            assert_eq!(frame[0], frame[1]);
        }
        assert!(out[..total]
            .windows(2 * channels)
            .step_by(channels)
            .all(|w| w[channels] <= w[0]));
        assert_eq!(out[0], 1.0);
        assert!(out[total - 1] < 0.01);
    }

    #[test]
    fn mix_crossfade_short_tail_fades_in_from_silence() {
        let mut b = vec![1.0f32; 8];
        let a = [1.0f32; 2];
        let n = mix_crossfade(&mut b, &a, 1.0, 0, 8, 2);
        assert_eq!(n, 8);
        // Past the tail the outgoing contribution is silence, so only the
        // incoming ramp remains.
        let (_, gb) = equal_power_gains(2.0 / 4.0);
        assert!((b[4] - gb).abs() < 1e-6);
        assert!(b[4] < 1.0);
    }

    #[test]
    fn crossfade_keeps_each_track_at_its_own_loudness_gain() {
        // A loud outgoing track (gain 0.475) into a quiet incoming one (gain
        // 1.41): after the callback scales the mix by the incoming gain, each
        // track must still sit at its own gain — not the tail at 1.41.
        let (old_gain, new_gain) = (0.475f32, 1.41f32);
        let (a_val, b_val) = (0.8f32, 0.3f32);
        let mut b = vec![b_val; 8];
        let a = [a_val; 8];
        mix_crossfade(&mut b, &a, tail_gain_ratio(old_gain, new_gain), 0, 8, 2);
        for (frame, pair) in b.chunks(2).enumerate() {
            let (ga, gb) = equal_power_gains(frame as f32 / 4.0);
            let expected = a_val * ga * old_gain + b_val * gb * new_gain;
            assert!((pair[0] * new_gain - expected).abs() < 1e-5);
        }
    }

    #[test]
    fn tail_gain_ratio_ignores_silent_incoming_gain() {
        assert_eq!(tail_gain_ratio(0.5, 0.0), 1.0);
        assert!((tail_gain_ratio(0.5, 2.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn loudness_switch_offset_lands_on_the_scheduled_sample() {
        // Buffer covers played-sample indices 1000..1512.
        assert_eq!(loudness_switch_offset(1200, 1000, 512), Some(200));
        // Exactly at the buffer's end: fires now (offset = whole buffer), so
        // the next buffer isn't racing the decode thread's counter rebase.
        assert_eq!(loudness_switch_offset(1512, 1000, 512), Some(512));
        assert_eq!(loudness_switch_offset(1513, 1000, 512), None);
        // Already passed (e.g. a buffer with no output): apply immediately.
        assert_eq!(loudness_switch_offset(900, 1000, 512), Some(0));
        assert_eq!(loudness_switch_offset(NO_LOUDNESS_SWITCH, 1000, 512), None);
    }

    #[test]
    fn crossfade_overlap_samples_follows_window_and_falls_back_to_gapless() {
        let s = 1_000_000_000u64;
        let minute = Some(60 * s);
        assert_eq!(
            crossfade_overlap_samples(3.0, minute, minute, 44_100, 2),
            264_600
        );
        assert_eq!(
            crossfade_overlap_samples(3.0, minute, minute, 48_000, 2),
            288_000
        );
        // Off, or unknown outgoing end: gapless.
        assert_eq!(crossfade_overlap_samples(0.0, minute, minute, 48_000, 2), 0);
        assert_eq!(
            crossfade_overlap_samples(-1.0, minute, minute, 48_000, 2),
            0
        );
        assert_eq!(
            crossfade_overlap_samples(f32::NAN, minute, minute, 48_000, 2),
            0
        );
        assert_eq!(crossfade_overlap_samples(3.0, None, minute, 48_000, 2), 0);
        // Incoming shorter than twice the window: gapless.
        assert_eq!(
            crossfade_overlap_samples(8.0, minute, Some(15 * s), 48_000, 2),
            0
        );
        assert_eq!(
            crossfade_overlap_samples(8.0, minute, Some(16 * s), 48_000, 2),
            768_000
        );
        // Unknown incoming length doesn't block the crossfade.
        assert_eq!(
            crossfade_overlap_samples(1.0, minute, None, 48_000, 2),
            96_000
        );
        // A late start clamps to what's left of the outgoing track.
        assert_eq!(
            crossfade_overlap_samples(8.0, Some(2 * s), minute, 48_000, 2),
            192_000
        );
    }

    #[test]
    fn samples_for_ns_maps_time_to_interleaved_samples() {
        // 1 second at 44100 Hz stereo = 88200 interleaved samples
        assert_eq!(samples_for_ns(1_000_000_000, 44100, 2), 88_200);
        assert_eq!(samples_for_ns(0, 44100, 2), 0);
        // 500 ms at 48000 Hz stereo
        assert_eq!(samples_for_ns(500_000_000, 48000, 2), 48_000);
    }

    #[test]
    fn resampler_passthrough_when_rates_match() {
        let mut r = Resampler::new(44100, 44100, 2);
        let input = vec![0.1, 0.2, 0.3, 0.4];
        assert_eq!(r.resample(&input), input);
    }

    #[test]
    fn convert_channels_mono_to_stereo_duplicates() {
        let out = convert_channels(&[0.5, -0.5], 1, 2);
        assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5]);
    }

    /// Times the output callback's hot path (gain chain, EQ, limiter,
    /// visualizer downmix) on a 48 kHz stereo, 1024-frame buffer, the
    /// comparison point for "no callback-timing regression" (#1556). Run it
    /// in release: `cargo test --release -p luminous-audio
    /// output_render_timing -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing benchmark; run manually in release"]
    fn output_render_timing() {
        const FRAMES: usize = 1024;
        const CHANNELS: u16 = 2;
        const ITERS: usize = 20_000;
        let shared = Arc::new(test_shared());
        {
            let mut eq = shared.equalizer.lock();
            eq.update_format(48_000, CHANNELS as usize);
            assert!(eq.load_builtin_preset("Rock"));
            eq.set_enabled(true);
        }
        shared
            .loudness_gain
            .store(0.8f32.to_bits(), Ordering::Relaxed);
        shared.volume.store(0.7f32.to_bits(), Ordering::Relaxed);
        // Measure the worst case: the spectrum downmix runs too.
        shared.spectrum_enabled.store(true, Ordering::Relaxed);
        let len = FRAMES * CHANNELS as usize;
        let (mut producer, consumer) = HeapRb::<f32>::new(len * 2).split();
        let mut renderer = OutputRenderer {
            shared: Arc::clone(&shared),
            consumer: Arc::new(Mutex::new(consumer)),
            played_samples: Arc::new(AtomicU64::new(0)),
            mono_scratch: Vec::with_capacity(len),
            sample_rate: 48_000,
            channels: CHANNELS,
        };
        let input: Vec<f32> = (0..len).map(|i| ((i as f32) * 0.01).sin() * 0.5).collect();
        let mut output = vec![0.0f32; len];
        let mut total = std::time::Duration::ZERO;
        for _ in 0..ITERS {
            producer.push_slice(&input);
            let t = std::time::Instant::now();
            renderer.render(&mut output);
            total += t.elapsed();
        }
        let per = total / ITERS as u32;
        let budget_us = FRAMES as f64 / 48_000.0 * 1e6;
        println!(
            "output render: {:.2} us per {FRAMES}-frame buffer ({:.3}% of the {:.0} us budget)",
            per.as_secs_f64() * 1e6,
            per.as_secs_f64() * 1e6 / budget_us * 100.0,
            budget_us
        );
        std::hint::black_box(&output);
    }

    #[test]
    fn set_loudness_gain_cancels_a_ramp_in_flight() {
        let engine = AudioEngine::new();
        engine.set_loudness_gain(1.0);
        engine.ramp_loudness_gain(0.0, 2_000);
        engine.set_loudness_gain(0.5);
        // Give the ramp thread several of its ~10 ms steps to (not) run.
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert_eq!(
            f32::from_bits(engine.shared.loudness_gain.load(Ordering::Relaxed)),
            0.5
        );
    }

    #[test]
    fn ramp_loudness_gain_lands_on_its_target() {
        let engine = AudioEngine::new();
        engine.set_loudness_gain(1.0);
        engine.ramp_loudness_gain(0.25, 30);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while f32::from_bits(engine.shared.loudness_gain.load(Ordering::Relaxed)) != 0.25 {
            assert!(
                std::time::Instant::now() < deadline,
                "ramp never reached its target"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn a_track_without_a_location_fails_to_open() {
        let err = ActiveTrack::open(&open_file, Track::default(), 0, 48_000, 2)
            .err()
            .expect("an empty location can't open");
        assert_eq!(err, "Track has no playable location");
    }

    #[test]
    fn active_track_reads_its_location_through_the_injected_opener() {
        let dir =
            std::env::temp_dir().join(format!("luminous-audio-opener-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("a.wav");
        write_test_pcm16_wav(&wav, 48_000, &[vec![0; 4_800]]);
        let real = wav.to_string_lossy().into_owned();
        let opener = move |location: &str| {
            assert_eq!(location, "custom://song/1");
            open_file(&real)
        };
        let spec = Track {
            id: 7,
            location: "custom://song/1".into(),
            extension_hint: Some("wav".into()),
            ..Track::default()
        };
        let track = ActiveTrack::open(&opener, spec, 0, 48_000, 2).expect("opens via the opener");
        assert_eq!(track.spec.id, 7);
        assert_eq!(track.src_rate, 48_000);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
