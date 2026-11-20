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
//! ## Playback speed (#2142)
//! `decode → resample → channel match → speed stage → ring buffer`. The speed
//! stage (`speed.rs`) is a passthrough at 1.0 and a pitch-preserving
//! time-stretch otherwise; it runs on the decode thread, so the DSP chain below
//! only ever sees already-stretched audio. Position counters stay on the
//! track's own timeline: the decode thread counts what it feeds the stage and
//! the output callback scales what it plays by the speed.
//!
//! ## DSP chain (contract shared with #77/#79)
//! `decode → loudness gain (#77) → EQ preamp → EQ bands → fade envelope (#79)
//! → volume → clip guard → output`. Each gain stage is a single precomputed
//! multiplier read from an atomic — nothing in the output callback allocates
//! or blocks. When every stage is neutral (EQ off, gains at 1.0) samples pass
//! through bit-perfect.

mod aac;
pub mod asio;
mod codecs;
pub mod pcm;
pub mod sink;
mod source;
mod spectrum;
mod speed;
// Each sink's platform-neutral helpers (failure mapping, device naming) are
// only called on its own platform, but are built and tested everywhere.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod alsa_direct;
#[cfg(windows)]
mod mf_aac;
#[cfg(target_os = "linux")]
mod reserve_device;
#[cfg_attr(not(windows), allow(dead_code))]
mod wasapi;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use source::ProbedTrack;
use std::path::Path;

pub use aac::decodes_full_he_aac;
pub use codecs::{can_play_stream, plays_at_reduced_quality};
pub use sink::{
    enumerate_output_devices, AudioBackendKind, AudioDeviceInfo, AudioOutputConfig, AudioSink,
    OutputFallbackReason,
};
pub use source::{normalize_probe_mime, IntegrityCheck, OfflineDecoder};
pub use speed::{snap_speed, MAX_SPEED, MIN_SPEED, SPEED_STEP};
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

/// Ramp the fade envelope down from its current level over `duration_ms`,
/// run `then` (pause or stop the stream), and restore the envelope for the
/// next start. Bumps `shared.fade_generation` so any in-flight fade-in ramp
/// is cleanly cancelled. With no duration the envelope is reset to 1.0 at once,
/// so an instant pause stays bit-identical to playing up to that point.
fn fade_out_then(shared: &AudioShared, duration_ms: u32, then: impl FnOnce()) {
    if duration_ms == 0 {
        shared.reset_fade_gain();
        then();
        return;
    }
    let generation = shared.fade_generation.fetch_add(1, Ordering::AcqRel) + 1;
    let start = f32::from_bits(shared.fade_gain.load(Ordering::Relaxed));
    let (steps, step_dur) = ramp_steps(duration_ms);
    for i in 0..=steps {
        if shared.fade_generation.load(Ordering::Acquire) != generation {
            break;
        }
        let g = ramp_gain_at_step(start, 0.0, steps, i);
        shared.fade_gain.store(g.to_bits(), Ordering::Relaxed);
        std::thread::sleep(step_dur);
    }
    then();
    shared.reset_fade_gain();
}

// ---------------------------------------------------------------------------
// Control messages sent to the decode thread
// ---------------------------------------------------------------------------

enum AudioCommand {
    Play(PlayRequest),
    Cue(PlayRequest),
    /// Fade out over this many ms (0 = at once), then pause.
    Pause(u32),
    /// Fade in over this many ms (0 = at once), then resume a paused track (#1718).
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
    /// Push the current master volume (`AudioShared::volume`) to the active
    /// sink's hardware mixer. It carries no level so a stale command can never
    /// apply one the safety ceiling has since lowered.
    SyncHardwareVolume,
    /// Play at this speed (already snapped) from now on. Restarts the ring
    /// buffer at the audible position when something is playing (#2142).
    SetSpeed(f32),
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
    /// True for unbounded live streams (Icecast/Shoutcast internet radio, #2002).
    /// Disables initial and mid-stream seeking (making `seek_to` a no-op),
    /// suppresses `AboutToFinish` and auto-crossfade, handles Ogg chained-stream
    /// resets, and treats a stream termination as [`AudioEvent::Error`] rather
    /// than [`AudioEvent::TrackFinished`].
    pub live: bool,
}

/// Station metadata headers sent by an Icecast/Shoutcast server at connect time (#2002).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IcyHeaders {
    pub name: Option<String>,
    pub genre: Option<String>,
    pub bitrate_kbps: Option<u32>,
}

/// An opened media source together with transport-discovered probe hints and
/// optional live ICY metadata (#2002).
pub struct OpenedSource {
    pub reader: Box<dyn MediaSource>,
    /// MIME type reported by the transport (e.g. HTTP `Content-Type`), if any,
    /// used to hint Symphonia's format probe when `location` has no extension.
    pub mime_type: Option<String>,
    /// Icecast/Shoutcast station headers discovered on the HTTP response.
    pub icy_headers: Option<IcyHeaders>,
    /// Receiver of deduplicated in-stream `StreamTitle` updates parsed from
    /// interleaved ICY metadata blocks.
    pub metadata_rx: Option<mpsc::Receiver<String>>,
}

impl OpenedSource {
    /// Wraps a bare [`MediaSource`] with no transport MIME hint or ICY metadata.
    pub fn new(reader: Box<dyn MediaSource>) -> Self {
        Self {
            reader,
            mime_type: None,
            icy_headers: None,
            metadata_rx: None,
        }
    }
}

impl From<Box<dyn MediaSource>> for OpenedSource {
    fn from(reader: Box<dyn MediaSource>) -> Self {
        Self::new(reader)
    }
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
type SourceOpener = dyn Fn(&str) -> Result<OpenedSource, String> + Send + Sync;

fn open_file(path: &str) -> Result<OpenedSource, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Cannot open file '{path}': {e}"))?;
    Ok(OpenedSource::new(Box::new(file)))
}

/// Facts about the output stage, for the app's pipeline display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub device_name: String,
    pub backend_name: String,
    /// Whether the open output bypasses the system mixer (see
    /// [`AudioBackendKind::bypasses_mixer`]); false after a fallback to
    /// shared output, whatever backend is configured.
    pub bypasses_mixer: bool,
    /// The configured bit-perfect backend couldn't open, so this is shared
    /// output standing in for it.
    pub fell_back: bool,
    /// The sample format the device receives, e.g. "24-bit integer PCM".
    pub format: String,
    /// Whether volume control occurs in device hardware (keeping software gain at unity).
    pub hardware_volume: bool,
}

/// Facts about the input stage (decoded stream), for the app's pipeline display.
/// Keyed by [`Track::id`] so the UI never pairs one track's format with another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputInfo {
    pub track_id: i64,
    /// Codec short name reported by the active decoder (e.g. `"aac"`, `"mp3"`, `"opus"`, `"flac"`).
    pub codec: String,
    /// Human-readable format name derived from `codec` (e.g. `"AAC"`, `"MP3"`, `"Opus"`, `"FLAC"`).
    pub format: String,
    /// Decoded sample rate in Hz.
    pub sample_rate: u32,
    /// Decoded channel count.
    pub channels: u16,
    /// Decoder chosen by the engine (e.g. `"Symphonia FLAC decoder"`, `"libopus (via Symphonia)"`, `"Media Foundation AAC"`).
    pub decoder_name: String,
    /// True when an HE-AAC stream is decoded by Symphonia's core-only decoder at <= 24 kHz (no SBR/PS).
    pub aac_core_only: bool,
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
    /// Hardware volume changed externally (e.g. via alsamixer or a physical DAC knob).
    VolumeChanged(f32),
    /// The configured bit-perfect backend couldn't be opened, so playback
    /// continues through shared output. Sent with each output (re)build that
    /// falls back; informational only.
    OutputFallback {
        requested: AudioBackendKind,
        reason: OutputFallbackReason,
    },
    /// The output stream could not be (re)built — typically a device switch
    /// whose new device can't be opened. The track is already parked paused at
    /// its position (a `Paused` event precedes this one); informational only,
    /// so the player must not treat it as a bad track. Play retries.
    OutputUnavailable {
        message: String,
    },
    /// A live stream's in-band metadata announced a new `StreamTitle` (#2008):
    /// the raw, untrimmed text the station sent, for the track `track_id`.
    /// Sent only when the title changes; parsing it is the app's job.
    StreamTitle {
        track_id: i64,
        title: String,
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
    /// Master volume, never above `volume_ceiling`. Write it only through
    /// `store_volume`, which keeps that true.
    pub(crate) volume: AtomicU32,
    /// The volume safety ceiling (#1890); 1.0 while there is none.
    pub(crate) volume_ceiling: AtomicU32,
    pub(crate) play_state: Mutex<EngineState>,
    pub(crate) visualizer_buf: spectrum::AudioVisualizerBuffer,
    /// Whether the output callback feeds `visualizer_buf` at all: the
    /// downmix is skipped while nothing displays a spectrum.
    pub(crate) spectrum_enabled: std::sync::atomic::AtomicBool,
    pub(crate) output_sample_rate: AtomicU32,
    pub(crate) output_channels: std::sync::atomic::AtomicU16,
    pub(crate) output_device_name: parking_lot::RwLock<Option<String>>,
    pub(crate) output_backend_name: parking_lot::RwLock<String>,
    pub(crate) output_bypasses_mixer: std::sync::atomic::AtomicBool,
    pub(crate) output_fell_back: std::sync::atomic::AtomicBool,
    pub(crate) output_format: parking_lot::RwLock<&'static str>,
    pub(crate) output_config: parking_lot::RwLock<AudioOutputConfig>,
    pub(crate) input_info: parking_lot::RwLock<Option<InputInfo>>,
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
    /// Playback speed (#2142), in effect for the audio in the ring buffer.
    /// Only the decode thread writes it, together with a ring restart, so
    /// the output callback never sees audio decoded at another speed.
    pub(crate) speed: AtomicU32,
    /// Fraction of a timeline sample the output callback has played but not
    /// yet added to `played_samples` (it only counts whole samples).
    pub(crate) speed_carry: AtomicU64,
    pub(crate) fade_gain: AtomicU32,
    /// Bumped by every fade-gain write that isn't a ramp step, so an
    /// in-flight `ramp_fade_gain` stops instead of overwriting it: the
    /// newest fade write always wins.
    pub(crate) fade_generation: AtomicU64,
    pub(crate) hardware_volume_active: std::sync::atomic::AtomicBool,
    pub(crate) output_hardware_volume: std::sync::atomic::AtomicBool,
    pub(crate) event_tx: mpsc::Sender<AudioEvent>,
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

    /// Store `vol` as the master volume, clamped to 0.0–1.0 and to the safety
    /// ceiling, and return the level stored.
    fn store_volume(&self, vol: f32) -> f32 {
        // The ceiling is re-checked after the store so a concurrent
        // `set_volume_ceiling` can't be overtaken: whichever runs second sees
        // the other's write (both are `SeqCst`), so the volume never stays
        // above the newest ceiling.
        let ceiling = f32::from_bits(self.volume_ceiling.load(Ordering::SeqCst));
        let vol = vol.clamp(0.0, 1.0).min(ceiling);
        self.volume.store(vol.to_bits(), Ordering::SeqCst);
        self.enforce_volume_ceiling()
    }

    /// Set (or, with `None`, lift) the safety ceiling and lower the master
    /// volume to it if needed; returns the volume afterwards.
    fn set_volume_ceiling(&self, ceiling: Option<f32>) -> f32 {
        let ceiling = ceiling.map_or(1.0, |c| c.clamp(0.0, 1.0));
        self.volume_ceiling
            .store(ceiling.to_bits(), Ordering::SeqCst);
        self.enforce_volume_ceiling()
    }

    /// Lower the master volume to the safety ceiling if it is above it, and
    /// return the volume afterwards.
    fn enforce_volume_ceiling(&self) -> f32 {
        let ceiling = f32::from_bits(self.volume_ceiling.load(Ordering::SeqCst));
        let mut bits = self.volume.load(Ordering::SeqCst);
        while f32::from_bits(bits) > ceiling {
            match self.volume.compare_exchange_weak(
                bits,
                ceiling.to_bits(),
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return ceiling,
                Err(now) => bits = now,
            }
        }
        f32::from_bits(bits)
    }

    /// Take in a volume change made outside the app (a DAC knob, alsamixer,
    /// the OS endpoint volume): store it under the safety ceiling and report
    /// it as [`AudioEvent::VolumeChanged`]. Returns the level kept, which is
    /// below `reading` when the ceiling cut it; the hardware listener then
    /// writes that level back to the device.
    pub(crate) fn accept_external_volume(&self, reading: f32) -> f32 {
        let kept = self.store_volume(reading);
        let _ = self.event_tx.send(AudioEvent::VolumeChanged(kept));
        kept
    }

    fn current_volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    /// Drop a scheduled loudness switch: the played-sample space it was
    /// measured in has been reset (seek, new track, device rebuild).
    fn cancel_loudness_switch(&self) {
        self.loudness_switch_at
            .store(NO_LOUDNESS_SWITCH, Ordering::Release);
    }

    /// Immediately set `fade_gain` to `gain`, cancelling any in-flight fade ramp.
    pub(crate) fn set_fade_gain(&self, gain: f32) {
        self.fade_generation.fetch_add(1, Ordering::AcqRel);
        self.fade_gain
            .store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Reset `fade_gain` to unity (1.0), cancelling any in-flight fade ramp.
    pub(crate) fn reset_fade_gain(&self) {
        self.set_fade_gain(1.0);
    }

    /// Glide `fade_gain` from `start` to `target` over `duration_ms`.
    /// Immediately sets `fade_gain` to `start` and bumps `fade_generation`.
    /// If `duration_ms == 0` or `start == target`, stores `target` and returns.
    /// Otherwise, spawns a dedicated worker thread to step the gain envelope.
    /// Cancels early if any newer fade write bumps `fade_generation`.
    pub(crate) fn ramp_fade_gain(self: &Arc<Self>, start: f32, target: f32, duration_ms: u32) {
        let start = start.clamp(0.0, 1.0);
        let target = target.clamp(0.0, 1.0);
        let generation = self.fade_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.fade_gain.store(start.to_bits(), Ordering::Relaxed);

        if duration_ms == 0 || (target - start).abs() < f32::EPSILON {
            self.fade_gain.store(target.to_bits(), Ordering::Relaxed);
            return;
        }

        let shared = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("luminous-fade-ramp".into())
            .spawn(move || {
                let (steps, step_dur) = ramp_steps(duration_ms);
                for i in 0..=steps {
                    if shared.fade_generation.load(Ordering::Acquire) != generation {
                        return;
                    }
                    let g = ramp_gain_at_step(start, target, steps, i);
                    shared.fade_gain.store(g.to_bits(), Ordering::Relaxed);
                    std::thread::sleep(step_dur);
                }
            });
        if spawned.is_err() {
            // No thread to ramp on: land on target immediately.
            self.fade_gain.store(target.to_bits(), Ordering::Relaxed);
        }
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
    pub fn with_source_opener<R>(
        open_source: impl Fn(&str) -> Result<R, String> + Send + Sync + 'static,
    ) -> Self
    where
        R: Into<OpenedSource>,
    {
        let open_source: Box<SourceOpener> = Box::new(move |loc| open_source(loc).map(Into::into));
        let (cmd_tx, cmd_rx) = mpsc::sync_channel::<AudioCommand>(64);
        let (event_tx, event_rx) = mpsc::channel::<AudioEvent>();
        let shared = Arc::new(AudioShared {
            open_source,
            position: AtomicU64::new(0),
            volume: AtomicU32::new(1.0f32.to_bits()),
            volume_ceiling: AtomicU32::new(1.0f32.to_bits()),
            play_state: Mutex::new(EngineState::Stopped),
            visualizer_buf: spectrum::AudioVisualizerBuffer::new(4096),
            spectrum_enabled: std::sync::atomic::AtomicBool::new(false),
            output_sample_rate: AtomicU32::new(44100),
            output_channels: std::sync::atomic::AtomicU16::new(2),
            output_device_name: parking_lot::RwLock::new(get_default_device_name()),
            output_backend_name: parking_lot::RwLock::new("CPAL".to_string()),
            output_bypasses_mixer: std::sync::atomic::AtomicBool::new(false),
            output_fell_back: std::sync::atomic::AtomicBool::new(false),
            output_format: parking_lot::RwLock::new("32-bit float PCM"),
            output_config: parking_lot::RwLock::new(AudioOutputConfig::default()),
            input_info: parking_lot::RwLock::new(None),
            equalizer: Mutex::new(luminous_equalizer::Equalizer::new()),
            loudness_gain: AtomicU32::new(1.0f32.to_bits()),
            loudness_generation: AtomicU64::new(0),
            loudness_switch_at: AtomicU64::new(NO_LOUDNESS_SWITCH),
            loudness_switch_gain: AtomicU32::new(1.0f32.to_bits()),
            speed: AtomicU32::new(1.0f32.to_bits()),
            speed_carry: AtomicU64::new(0.0f64.to_bits()),
            fade_gain: AtomicU32::new(1.0f32.to_bits()),
            fade_generation: AtomicU64::new(0),
            hardware_volume_active: std::sync::atomic::AtomicBool::new(false),
            output_hardware_volume: std::sync::atomic::AtomicBool::new(false),
            event_tx: event_tx.clone(),
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
    /// For live streams (`track.live`), `start_nanosec` is ignored (always 0).
    pub fn cue(&self, track: Track, start_nanosec: u64) -> Result<()> {
        let start_nanosec = if track.live { 0 } else { start_nanosec };
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
    /// (0 resumes at once).
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

    /// Play at `speed` times normal without changing pitch (#2142). The
    /// request is clamped to `MIN_SPEED..=MAX_SPEED` and snapped to
    /// `SPEED_STEP`; the speed actually used is returned, so a caller never
    /// needs to validate. Position, seeking and handover stay on the track's
    /// own timeline at any speed, and 1.0 leaves samples untouched.
    pub fn set_playback_speed(&self, speed: f32) -> Result<f32> {
        let speed = snap_speed(speed);
        self.send_cmd(AudioCommand::SetSpeed(speed))?;
        Ok(speed)
    }

    /// Set the master volume, clamped to 0.0–1.0 and to the safety ceiling
    /// (see [`set_volume_ceiling`](Self::set_volume_ceiling)). A level above
    /// the ceiling sets the ceiling. Returns the volume applied.
    pub fn set_volume(&self, vol: f32) -> f32 {
        let applied = self.shared.store_volume(vol);
        let _ = self.send_cmd(AudioCommand::SyncHardwareVolume);
        applied
    }

    /// Cap the master volume at `ceiling` (0.0–1.0), or lift the cap with
    /// `None` (#1890). While a cap is set, no volume command and no external
    /// hardware change can exceed it, on any output backend; a current
    /// volume above it is lowered at once. Lifting the cap leaves the volume
    /// where it is. Returns the volume afterwards.
    pub fn set_volume_ceiling(&self, ceiling: Option<f32>) -> f32 {
        let before = self.shared.current_volume();
        let after = self.shared.set_volume_ceiling(ceiling);
        if after != before {
            let _ = self.send_cmd(AudioCommand::SyncHardwareVolume);
        }
        after
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
        self.shared.current_volume()
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
            backend_name: if backend_name.is_empty() {
                "CPAL".to_string()
            } else {
                backend_name
            },
            bypasses_mixer: self.shared.output_bypasses_mixer.load(Ordering::Relaxed),
            fell_back: self.shared.output_fell_back.load(Ordering::Relaxed),
            format: self.shared.output_format.read().to_string(),
            hardware_volume: self.shared.output_hardware_volume.load(Ordering::Relaxed),
        }
    }

    /// The input stage for the currently playing track, if a decoder is open.
    pub fn input_info(&self) -> Option<InputInfo> {
        self.shared.input_info.read().clone()
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

    /// When following the system default device, refresh the cached device name
    /// so idle callers (e.g. opening Equalizer settings while paused) see OS
    /// default changes without waiting for playback to resume. Returns the new
    /// device name if it changed.
    pub fn refresh_default_device(&self) -> Option<String> {
        let cfg = self.shared.output_config.read().clone();
        if !is_tracking_default(&cfg) {
            return None;
        }
        let current = get_default_device_name()?;
        let mut guard = self.shared.output_device_name.write();
        if guard.as_deref() == Some(current.as_str()) {
            return None;
        }
        *guard = Some(current.clone());
        Some(current)
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

/// Extracts the bare file extension (without dot) from a file path or a URL
/// whose query string / fragment has been stripped.
pub fn extension_from_location(location: &str) -> Option<&str> {
    let without_fragment = location.split('#').next().unwrap_or(location);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    Path::new(without_query)
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty())
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
    /// Hard decode cutoff (CUE end boundary); always `None` for live streams.
    end_ns: Option<u64>,
    /// Where `AboutToFinish` is measured from: the CUE end boundary if set,
    /// otherwise the tagged track length; always `None` for live streams.
    about_end_ns: Option<u64>,
    /// Decoded-stream position (source timeline), advanced per packet.
    decoded_pos_ns: u64,
    about_to_finish_sent: bool,
    eof: bool,
    /// Transport-reported Icecast/Shoutcast station headers, if any (#2002).
    #[allow(dead_code)]
    icy_headers: Option<IcyHeaders>,
    /// Live in-stream `StreamTitle` updates from an ICY reader (#2002),
    /// forwarded as [`AudioEvent::StreamTitle`] by the decode loop (#2008).
    metadata_rx: Option<mpsc::Receiver<String>>,
    /// Decoded input stage details for this track. Held per-track rather than
    /// written directly to `AudioShared` from `decode_track_packet` so an
    /// outgoing crossfade tail never overwrites the incoming track's format.
    input_info: InputInfo,
    /// Set when `decode_track_packet` updates `input_info` mid-stream.
    input_changed: bool,
}

fn display_format_name(codec: &str) -> String {
    match codec {
        "mp3" => "MP3".to_string(),
        "flac" => "FLAC".to_string(),
        "vorbis" => "Vorbis".to_string(),
        "opus" => "Opus".to_string(),
        "speex" => "Speex".to_string(),
        "aac" => "AAC".to_string(),
        "alac" => "ALAC".to_string(),
        "wavpack" => "WavPack".to_string(),
        "musepack" | "mpc" => "Musepack".to_string(),
        "trueaudio" | "tta" => "TrueAudio".to_string(),
        "monkeys_audio" | "ape" => "Monkey's Audio".to_string(),
        "wma" | "wmav1" | "wmav2" => "WMA/ASF".to_string(),
        s if s.starts_with("pcm_") => "PCM".to_string(),
        other => other.to_ascii_uppercase(),
    }
}

fn build_input_info(
    track_id: i64,
    decoder: &dyn symphonia::core::codecs::audio::AudioDecoder,
    sample_rate: u32,
    channels: usize,
) -> InputInfo {
    input_info_from_codec(track_id, decoder.codec_info(), sample_rate, channels)
}

fn input_info_from_codec(
    track_id: i64,
    info: &symphonia::core::codecs::CodecInfo,
    sample_rate: u32,
    channels: usize,
) -> InputInfo {
    let codec = info.short_name.to_string();
    let format = display_format_name(&codec);
    let is_mf_aac = info.long_name == "Media Foundation AAC";
    let aac_core_only = codec == "aac" && !is_mf_aac && sample_rate <= aac::HE_AAC_MAX_CORE_RATE;
    let decoder_name = if is_mf_aac {
        "Media Foundation AAC".to_string()
    } else if codec == "opus" {
        "libopus (via Symphonia)".to_string()
    } else {
        format!("Symphonia {format} decoder")
    };
    InputInfo {
        track_id,
        codec,
        format,
        sample_rate,
        channels: channels as u16,
        decoder_name,
        aac_core_only,
    }
}

impl ActiveTrack {
    /// Resample to `rate`/`channels` from here on (the track hasn't been
    /// decoded yet, so there's no resampler state to keep).
    fn retarget(&mut self, rate: u32, channels: u16) {
        self.resampler = Resampler::new(self.src_rate, rate, channels as usize);
    }

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
        let OpenedSource {
            reader: source,
            mime_type,
            icy_headers,
            metadata_rx,
        } = open_source(&spec.location)?;
        // The location's own extension wins; a stream URL has none, so the
        // spec's hint stands in.
        let extension = extension_from_location(&spec.location).or(spec.extension_hint.as_deref());
        let ProbedTrack {
            mut format,
            mut decoder,
            track_id,
            sample_rate: src_rate,
            channels: src_channels,
            ..
        } = ProbedTrack::open(source, extension, mime_type.as_deref())?;

        // Live streams are non-seekable; ignore any initial seek offset.
        let effective_start_ns = if spec.live { 0 } else { start_nanosec };
        if effective_start_ns > 0 {
            let target_time = symphonia::core::units::Time::from_nanos(effective_start_ns as i64);
            match format.seek(
                symphonia::core::formats::SeekMode::Accurate,
                symphonia::core::formats::SeekTo::Time {
                    time: target_time,
                    track_id: Some(track_id),
                },
            ) {
                Ok(_) => decoder.reset(),
                Err(e) => log::warn!("Initial seek to {effective_start_ns}ns failed: {e:?}"),
            }
        }

        let end_ns = if spec.live {
            None
        } else {
            spec.end_ns.filter(|&ns| ns > 0)
        };
        let about_end_ns = if spec.live {
            None
        } else {
            end_ns.or(spec.length_ns)
        };

        let input_info = build_input_info(spec.id, &*decoder, src_rate, src_channels);

        Ok(Self {
            spec,
            format,
            decoder,
            track_id,
            src_rate,
            src_channels,
            resampler: Resampler::new(src_rate, target_rate, target_channels),
            start_ns: effective_start_ns,
            end_ns,
            about_end_ns,
            decoded_pos_ns: effective_start_ns,
            about_to_finish_sent: false,
            eof: false,
            icy_headers,
            metadata_rx,
            input_info,
            input_changed: false,
        })
    }
}

/// The rate to reopen an output at for a track at `track_rate`, or `None` to
/// keep it. Only a bit-perfect sink follows the track's rate, and only when it
/// isn't already running at it (`opened_rate`) and wasn't already asked for it
/// (`requested_rate`: a device that couldn't do it got the closest rate it
/// could, and keeps it). Decided on the sink actually open, so a bit-perfect
/// backend that fell back to shared output doesn't retry on every rate change.
fn rate_switch(
    open_backend: AudioBackendKind,
    requested_rate: u32,
    opened_rate: u32,
    track_rate: u32,
) -> Option<u32> {
    (open_backend != AudioBackendKind::Cpal
        && track_rate != requested_rate
        && track_rate != opened_rate)
        .then_some(track_rate)
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
/// `speed` converts timeline samples to the buffer's output samples (#2142).
fn loudness_switch_offset(
    switch_at: u64,
    played_before: u64,
    played: usize,
    speed: f32,
) -> Option<usize> {
    // `switch_at` and `played_before` are timeline samples; the buffer's
    // `played` samples cover `played * speed` of them.
    let covered = (played as f64 * speed as f64).round() as u64;
    if switch_at == NO_LOUDNESS_SWITCH || switch_at > played_before + covered {
        return None;
    }
    let offset = (switch_at.saturating_sub(played_before) as f64 / speed as f64).round() as usize;
    Some(offset.min(played))
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
    /// Set when the configured backend failed and this is the shared-output
    /// stand-in: what was asked for, and why it couldn't be had.
    fallback: Option<(AudioBackendKind, OutputFallbackReason)>,
    /// Set when the chosen device wasn't connected (see
    /// `chosen_device_missing`) and this output is on the system default
    /// instead; `check_and_rebuild_output` switches back once the chosen
    /// device returns.
    stands_in: bool,
    /// The rate this output was opened for. A bit-perfect device may not run
    /// at it (`sample_rate` is what it got); remembering the request keeps a
    /// rate the device can't do from reopening it on every track.
    requested_rate: u32,
}

impl AudioOutput {
    /// The rate to reopen this output at so a track at `track_rate` plays at
    /// its own rate, or `None` to keep it as it is.
    fn rate_switch_for(&self, track_rate: u32) -> Option<u32> {
        rate_switch(
            self.sink.backend(),
            self.requested_rate,
            self.sample_rate,
            track_rate,
        )
    }

    /// Make this the output `AudioEngine::output_info` describes, and report
    /// a backend fallback if it is one.
    fn publish(&self, shared: &AudioShared, event_tx: &mpsc::Sender<AudioEvent>) {
        shared
            .output_sample_rate
            .store(self.sample_rate, Ordering::Relaxed);
        shared
            .output_channels
            .store(self.channels, Ordering::Relaxed);
        *shared.output_device_name.write() = self.device_name.clone();
        *shared.output_backend_name.write() = self.backend_name.clone();
        shared
            .output_bypasses_mixer
            .store(self.sink.backend().bypasses_mixer(), Ordering::Relaxed);
        shared
            .output_fell_back
            .store(self.fallback.is_some(), Ordering::Relaxed);
        shared
            .output_hardware_volume
            .store(self.sink.has_hardware_volume(), Ordering::Relaxed);
        *shared.output_format.write() = self.sink.output_format();
        if let Some((requested, reason)) = self.fallback {
            let _ = event_tx.send(AudioEvent::OutputFallback { requested, reason });
        }
    }
}

/// Both modes on the WASAPI endpoints follow the system default when no
/// device is chosen; an ASIO driver is never "the default".
fn is_tracking_default(cfg: &AudioOutputConfig) -> bool {
    matches!(
        cfg.backend,
        AudioBackendKind::Cpal | AudioBackendKind::WasapiExclusive
    ) && cfg.device_id.is_none()
}

fn get_default_device_name() -> Option<String> {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    let dev = host
        .default_output_device()
        .map(|device| resolve_default_device_name(&device));
    #[cfg(target_os = "linux")]
    {
        dev.or_else(|| query_wpctl_default_sink().or_else(query_pactl_default_sink))
    }
    #[cfg(not(target_os = "linux"))]
    {
        dev
    }
}

#[cfg(target_os = "linux")]
fn is_virtual_sound_server_alias(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "default"
            | "default audio device"
            | "pipewire"
            | "pipewire sound server"
            | "pulse"
            | "pulseaudio"
            | "pulseaudio sound server"
            | "alsa:default"
            | "alsa:pipewire"
            | "alsa:pulse"
    ) || lower.starts_with("default:")
        || lower.starts_with("pipewire:")
        || lower.starts_with("pulse:")
        || lower.starts_with("default alsa output")
}

#[cfg(target_os = "linux")]
fn is_virtual_default_device(device: &cpal::Device) -> bool {
    use cpal::traits::DeviceTrait;
    let id_is_virtual = device
        .id()
        .ok()
        .is_some_and(|id| is_virtual_sound_server_alias(&id.to_string()));
    id_is_virtual || is_virtual_sound_server_alias(&device_display_name(device))
}

/// On Linux, CPAL's default device is usually the virtual ALSA `"default"` /
/// `"Default Audio Device"` or `"pipewire"`/`"pulse"` PCM, whose name never
/// changes when PipeWire/PulseAudio switches between speakers and Bluetooth
/// headphones. Resolve the active default sink's human-readable name so
/// device-change tracking works.
#[cfg(target_os = "linux")]
fn resolve_default_device_name(device: &cpal::Device) -> String {
    let cpal_name = device_display_name(device);
    if !is_virtual_default_device(device) {
        return cpal_name;
    }
    query_wpctl_default_sink()
        .or_else(query_pactl_default_sink)
        .unwrap_or(cpal_name)
}

#[cfg(not(target_os = "linux"))]
fn resolve_default_device_name(device: &cpal::Device) -> String {
    device_display_name(device)
}

#[cfg(target_os = "linux")]
fn parse_wpctl_default_sink(text: &str) -> Option<String> {
    let mut node_name = None;
    for line in text.lines() {
        let trimmed = line.trim().trim_start_matches('*').trim();
        if let Some(val) = trimmed.strip_prefix("node.description = ") {
            let desc = val.trim().trim_matches('"').trim();
            if !desc.is_empty() {
                return Some(desc.to_string());
            }
        } else if let Some(val) = trimmed.strip_prefix("node.name = ") {
            let name = val.trim().trim_matches('"').trim();
            if !name.is_empty() {
                node_name = Some(name.to_string());
            }
        }
    }
    node_name
}

#[cfg(target_os = "linux")]
fn query_wpctl_default_sink() -> Option<String> {
    let out = std::process::Command::new("wpctl")
        .args(["inspect", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_wpctl_default_sink(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(target_os = "linux")]
fn query_pactl_default_sink() -> Option<String> {
    let out = std::process::Command::new("pactl")
        .arg("get-default-sink")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!name.is_empty()).then_some(name)
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

fn device_display_name(d: &cpal::Device) -> String {
    use cpal::traits::DeviceTrait;
    d.description()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|_| d.to_string())
}

/// The connected output device whose id is `id`, with its display name.
fn find_output_device(host: &cpal::Host, id: &str) -> Option<(cpal::Device, String)> {
    use cpal::traits::{DeviceTrait, HostTrait};
    host.output_devices().ok()?.find_map(|d| {
        let name = device_display_name(&d);
        let dev_id = d
            .id()
            .ok()
            .map(|i| i.to_string())
            .unwrap_or_else(|| name.clone());
        (dev_id == id).then_some((d, name))
    })
}

/// Whether the device chosen in `config` isn't connected, so an output opened
/// for it stands in until it returns. False when no device is chosen, and for
/// ASIO, whose drivers aren't watched.
fn chosen_device_missing(config: &AudioOutputConfig) -> bool {
    device_missing(config, |backend, id| match backend {
        #[cfg(target_os = "linux")]
        AudioBackendKind::Alsa => crate::alsa_direct::list_hw_devices()
            .iter()
            .any(|d| d.id == id),
        // No ALSA on this platform, so nothing stands in for it.
        #[cfg(not(target_os = "linux"))]
        AudioBackendKind::Alsa => true,
        _ => find_output_device(&cpal::default_host(), id).is_some(),
    })
}

/// [`chosen_device_missing`], asking `is_connected` whether the chosen id is in
/// its backend's device list.
fn device_missing(
    config: &AudioOutputConfig,
    is_connected: impl FnOnce(AudioBackendKind, &str) -> bool,
) -> bool {
    match config.device_id.as_deref().filter(|id| !id.is_empty()) {
        Some(_) if config.backend == AudioBackendKind::Asio => false,
        Some(id) => !is_connected(config.backend, id),
        None => false,
    }
}

/// The device to open for `requested_id`.
struct ResolvedDevice {
    device: cpal::Device,
    name: Option<String>,
    /// The requested device isn't connected; this is the system default.
    stands_in: bool,
}

fn resolve_cpal_device(
    host: &cpal::Host,
    requested_id: Option<&str>,
) -> Result<ResolvedDevice, String> {
    let requested_id = requested_id.filter(|id| !id.is_empty());
    if let Some(found) = requested_id.and_then(|id| find_output_device(host, id)) {
        let (device, name) = found;
        return Ok(ResolvedDevice {
            device,
            name: Some(name),
            stands_in: false,
        });
    }
    if let Some(id) = requested_id {
        log::warn!("Chosen output device {id} isn't connected; using the system default");
    }
    let device = select_output_device(host)?;
    Ok(ResolvedDevice {
        name: Some(resolve_default_device_name(&device)),
        device,
        stands_in: requested_id.is_some(),
    })
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
        let speed = f32::from_bits(self.shared.speed.load(Ordering::Relaxed));
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
        if let Some(k) = loudness_switch_offset(switch_at, played_before, played, speed) {
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

        // 4) Master volume: bypassed when hardware volume is active (keeps samples bit-perfect)
        let hw_vol = self.shared.hardware_volume_active.load(Ordering::Relaxed);
        if !hw_vol && vol != 1.0 {
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
            // When hardware volume is active, `output` remains bit-perfect at full scale,
            // so scale the visualizer copy by `vol` so spectrum and reactive logo match listening level.
            let vis_scale = if hw_vol { vol } else { 1.0 };
            for chunk in output[..played].chunks(channels_u) {
                let sum: f32 = chunk.iter().sum();
                // Reserved to buffer_capacity up front and cleared per callback.
                // ast-grep-ignore: audio-callback-no-alloc
                self.mono_scratch
                    .push((sum / self.channels as f32) * vis_scale);
            }
            // Bounded ring buffer: evicts the oldest block at max_size.
            // ast-grep-ignore: audio-callback-no-alloc
            self.shared.visualizer_buf.push(&self.mono_scratch);
        }

        // `played_samples` is on the track's own timeline, so at another
        // speed each output sample advances it by `speed` samples. Only whole
        // samples are added; the remainder carries to the next callback.
        let advance = if speed == 1.0 {
            played as u64
        } else {
            let carried = f64::from_bits(self.shared.speed_carry.load(Ordering::Relaxed))
                + played as f64 * speed as f64;
            let whole = carried.floor();
            self.shared
                .speed_carry
                .store((carried - whole).to_bits(), Ordering::Relaxed);
            whole as u64
        };
        let total_played = self.played_samples.fetch_add(advance, Ordering::Relaxed) + advance;
        let pos_ns = (total_played as f64 * 1_000_000_000.0
            / (self.sample_rate as f64 * self.channels as f64)) as u64;
        self.shared.position.store(pos_ns, Ordering::Relaxed);
    }
}

/// The configured backend that couldn't be opened, and why.
type Fallback = (AudioBackendKind, OutputFallbackReason);

/// Opens the configured bit-perfect backend (ASIO, WASAPI exclusive or ALSA
/// hardware) with its ring buffer, retuning the EQ to its format. `None` when
/// the configured backend is shared output or doesn't exist on this platform;
/// `Err` with the reason when it couldn't be opened.
#[cfg(any(windows, target_os = "linux"))]
fn open_bit_perfect(
    shared: &Arc<AudioShared>,
    config: &AudioOutputConfig,
    requested_rate: u32,
) -> Option<Result<AudioOutput, Fallback>> {
    let backend = config.backend;
    if backend == AudioBackendKind::Cpal {
        return None;
    }

    // Two seconds of stereo at the highest rate the device may open at: the
    // request, or its mix rate (at most 192 kHz on real hardware).
    let ring_rate = requested_rate.max(192_000) as usize;
    let (producer, consumer) = HeapRb::<f32>::new(ring_rate * 2 * 2).split();
    let consumer = Arc::new(Mutex::new(consumer));
    let played_samples = Arc::new(AtomicU64::new(0));

    // Only WASAPI exclusive can stand in for a missing device.
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut stands_in = false;
    let opened: Result<Box<dyn AudioSink>, crate::sink::OpenError> = match backend {
        #[cfg(windows)]
        AudioBackendKind::Asio => crate::asio::build_asio_sink(
            config.device_id.as_deref(),
            shared,
            Arc::clone(&consumer),
            Arc::clone(&played_samples),
            requested_rate,
        )
        .map(|sink| Box::new(sink) as Box<dyn AudioSink>)
        .map_err(|message| crate::sink::OpenError {
            reason: OutputFallbackReason::Unavailable,
            message,
        }),
        #[cfg(windows)]
        AudioBackendKind::WasapiExclusive => {
            // Exclusive mode shares the CPAL endpoint ids, so the same lookup
            // (falling back to the default when the device is gone) applies.
            use cpal::traits::DeviceTrait;
            let host = cpal::default_host();
            resolve_cpal_device(&host, config.device_id.as_deref())
                .and_then(|resolved| {
                    let endpoint_id = resolved
                        .device
                        .id()
                        .map(|id| id.id().to_string())
                        .map_err(|e| format!("Output endpoint has no id: {e}"))?;
                    stands_in = resolved.stands_in;
                    Ok((endpoint_id, resolved.name))
                })
                .map_err(|message| crate::sink::OpenError {
                    reason: OutputFallbackReason::Unavailable,
                    message,
                })
                .and_then(|(endpoint_id, name)| {
                    crate::wasapi::build_exclusive_sink(
                        endpoint_id,
                        requested_rate,
                        name,
                        shared,
                        Arc::clone(&consumer),
                        Arc::clone(&played_samples),
                    )
                })
                .map(|sink| Box::new(sink) as Box<dyn AudioSink>)
        }
        #[cfg(target_os = "linux")]
        AudioBackendKind::Alsa => crate::alsa_direct::build_alsa_sink(
            config.device_id.as_deref(),
            requested_rate,
            shared,
            Arc::clone(&consumer),
            Arc::clone(&played_samples),
        )
        .map(|sink| Box::new(sink) as Box<dyn AudioSink>),
        // A bit-perfect backend this platform doesn't have.
        _ => return None,
    };
    Some(
        opened
            .map(|sink| {
                let sample_rate = sink.sample_rate();
                let channels = sink.channels();
                shared
                    .equalizer
                    .lock()
                    .update_format(sample_rate, channels as usize);
                AudioOutput {
                    device_name: sink.device_name(),
                    backend_name: sink.backend().to_string(),
                    sink,
                    producer,
                    consumer,
                    played_samples,
                    sample_rate,
                    channels,
                    fallback: None,
                    stands_in,
                    requested_rate,
                }
            })
            .map_err(|e| {
                log::warn!(
                    "{backend} unavailable ({}), falling back to shared output",
                    e.message
                );
                (backend, e.reason)
            }),
    )
}

#[cfg(not(any(windows, target_os = "linux")))]
fn open_bit_perfect(
    _shared: &Arc<AudioShared>,
    _config: &AudioOutputConfig,
    _requested_rate: u32,
) -> Option<Result<AudioOutput, Fallback>> {
    None
}

/// Opens the configured backend. The bit-perfect backends (ASIO, WASAPI
/// exclusive, ALSA hardware) open at `requested_rate` or the closest rate the
/// device takes (44.1 kHz when `None`); shared output runs at the system mix
/// rate. When a bit-perfect backend can't be opened, falls back to shared CPAL output (on the same
/// endpoint where there is one) and records why in `AudioOutput::fallback`.
fn build_output(
    shared: &Arc<AudioShared>,
    requested_rate: Option<u32>,
) -> Result<AudioOutput, String> {
    use cpal::traits::{DeviceTrait, StreamTrait};

    let requested_rate = requested_rate.unwrap_or(crate::sink::DEFAULT_BIT_PERFECT_RATE);
    let output_cfg = shared.output_config.read().clone();
    let fallback = match open_bit_perfect(shared, &output_cfg, requested_rate) {
        Some(Ok(output)) => return Ok(output),
        Some(Err(fallback)) => Some(fallback),
        None => None,
    };

    let host = cpal::default_host();
    let ResolvedDevice {
        device,
        name: device_name,
        stands_in,
    } = resolve_cpal_device(&host, output_cfg.shared_device_id())?;
    // Shared output only looked for its own device. A bit-perfect fallback's
    // chosen device may be on another backend's list (an ALSA `hw:` PCM), so
    // ask that backend: one still connected (in use, no usable format) isn't
    // standing in, since its device appearing wouldn't cure it.
    let stands_in = stands_in || (fallback.is_some() && chosen_device_missing(&output_cfg));
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
                if is_fatal_cpal_error(&err) {
                    log::error!("CPAL stream fatal error: {err}");
                    err_flag_cpal.store(true, Ordering::Relaxed);
                } else {
                    log::warn!("CPAL stream non-fatal glitch: {err}");
                }
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
        stream_error_flag,
    );

    Ok(AudioOutput {
        sink: Box::new(cpal_sink),
        producer: prod,
        consumer: shared_consumer,
        played_samples,
        sample_rate: target_sample_rate,
        channels: target_channels,
        device_name,
        backend_name: AudioBackendKind::Cpal.to_string(),
        fallback,
        stands_in,
        requested_rate,
    })
}

/// Determines whether a CPAL stream error represents a fatal stream breakdown
/// requiring the audio engine to tear down and rebuild the stream.
///
/// Transient conditions like `Xrun` (buffer underrun/overrun) and `RealtimeDenied`
/// (advisory scheduling notice) are recoverable or non-fatal glitches that the host
/// audio backend continues streaming through. Tearing down and rebuilding the stream
/// on such transient hiccups causes audible dropouts, resampler resets, and can trap
/// playback in an underrun/rebuild loop (#1800).
pub(crate) fn is_fatal_cpal_error(err: &cpal::Error) -> bool {
    !matches!(
        err.kind(),
        cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied
    )
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
fn build_output_on_fresh_thread(
    shared: &Arc<AudioShared>,
    requested_rate: Option<u32>,
) -> Result<AudioOutput, String> {
    let shared = Arc::clone(shared);

    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("luminous-audio-rebuild".into())
        .spawn(move || {
            let result = build_output(&shared, requested_rate);
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
    /// Playback speed stage (#2142). `pushed_samples` counts what goes into
    /// it (timeline samples), not what comes out.
    speed: speed::SpeedStage,
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
    /// The rebuilt output skips the system volume the old one went through
    /// (see `bypasses_system_volume`): pause on it with this request, so
    /// the jump in level waits for the user to press play.
    PauseOnLouder(PlayRequest),
}

/// Whether moving playback from `old` to `new` drops the system volume out
/// of the path: shared output goes through the OS mixer's volume, the
/// bit-perfect backends don't, so the same signal can come out far louder.
fn bypasses_system_volume(old: AudioBackendKind, new: AudioBackendKind) -> bool {
    !old.bypasses_mixer() && new.bypasses_mixer()
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
        let (mut req, mut resume_fade_ms) = match current_req.take() {
            Some(r) => (r, 0u32),
            None => {
                match cmd_rx.recv() {
                    Ok(AudioCommand::Play(r)) => {
                        paused_req = None;
                        (r, 0u32)
                    }
                    Ok(AudioCommand::Cue(r)) => {
                        match start_on_output(
                            &mut output,
                            &shared,
                            &event_tx,
                            r.track.clone(),
                            r.start_nanosec,
                        ) {
                            Ok(_) => {
                                let out = output.as_mut().unwrap();
                                let target_sample_rate = out.sample_rate;
                                let target_channels = out.channels;
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
                            Err(StartFailure::Track(message)) => {
                                let _ = event_tx.send(AudioEvent::Error { message });
                            }
                            Err(StartFailure::NoOutput(_, message)) => {
                                park_without_output(
                                    &shared,
                                    &event_tx,
                                    &mut paused_req,
                                    r,
                                    message,
                                );
                            }
                        }
                        continue;
                    }
                    Ok(AudioCommand::Resume(fade_ms)) => {
                        if let Some(r) = paused_req.take() {
                            let cur_pos = shared.position.load(Ordering::Relaxed);
                            (
                                PlayRequest {
                                    track: r.track,
                                    start_nanosec: cur_pos,
                                },
                                fade_ms,
                            )
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
                        let requested_rate = output.as_ref().map(|o| o.requested_rate);
                        if let Some(old_out) = output.take() {
                            let _ = old_out.sink.pause();
                        }
                        match build_output_on_fresh_thread(&shared, requested_rate) {
                            Ok(new_out) => {
                                new_out.publish(&shared, &event_tx);
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
                    // opens the track from `shared.position`. Seeking a live
                    // track is a no-op (#2002).
                    Ok(AudioCommand::SeekTo(target_ns)) if paused_req.is_some() => {
                        if paused_req.as_ref().is_some_and(|r| r.track.live) {
                            continue;
                        }
                        if let Some(out) = output.as_ref() {
                            out.played_samples.store(
                                samples_for_ns(target_ns, out.sample_rate, out.channels),
                                Ordering::Relaxed,
                            );
                        }
                        shared.position.store(target_ns, Ordering::Relaxed);
                        continue;
                    }
                    Ok(AudioCommand::SyncHardwareVolume) => {
                        if let Some(out) = output.as_ref() {
                            out.sink.set_hardware_volume(shared.current_volume());
                        }
                        continue;
                    }
                    // Nothing is playing, so there is no ring buffer to
                    // restart: the next track starts at this speed.
                    Ok(AudioCommand::SetSpeed(speed)) => {
                        shared.speed.store(speed.to_bits(), Ordering::Relaxed);
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
                Ok(AudioCommand::Play(newer)) => {
                    req = newer;
                    resume_fade_ms = 0;
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
                    continue 'main;
                }
                Ok(AudioCommand::SyncHardwareVolume) => {
                    if let Some(out) = output.as_ref() {
                        out.sink.set_hardware_volume(shared.current_volume());
                    }
                }
                Ok(AudioCommand::SetSpeed(speed)) => {
                    shared.speed.store(speed.to_bits(), Ordering::Relaxed);
                }
                // Pause/Resume/SeekTo/Preload target a track that hasn't
                // started playing yet at this point — nothing to apply.
                Ok(_) => {}
                Err(_) => break,
            }
        }

        // Ensure the persistent output stream exists (built lazily on the
        // first track this thread ever plays; reused for every track after,
        // unless a bit-perfect one must reopen at this track's rate).
        // Built on a fresh thread — `output` can already be `None` here after
        // a mid-session device-change rebuild (see `check_and_rebuild_output`),
        // and re-touching COM on `decode_thread`'s own long-lived OS thread a
        // second time risks the same RPC_E_CHANGED_MODE wedge #624 fixed.
        let current = match start_on_output(
            &mut output,
            &shared,
            &event_tx,
            req.track,
            req.start_nanosec,
        ) {
            Ok(t) => t,
            Err(StartFailure::Track(message)) => {
                let _ = event_tx.send(AudioEvent::Error { message });
                continue;
            }
            Err(StartFailure::NoOutput(track, message)) => {
                let req = PlayRequest {
                    track,
                    start_nanosec: req.start_nanosec,
                };
                park_without_output(&shared, &event_tx, &mut paused_req, req, message);
                continue;
            }
        };
        let out = output.as_mut().unwrap();
        let target_sample_rate = out.sample_rate;
        let target_channels = out.channels;
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

        if resume_fade_ms > 0 {
            shared.ramp_fade_gain(0.0, 1.0, resume_fade_ms);
        } else {
            shared.reset_fade_gain();
        }

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
        *shared.input_info.write() = Some(current.input_info.clone());
        let _ = event_tx.send(AudioEvent::Playing { track_id });

        let mut session = DecodeSession {
            current,
            next: None,
            transition: None,
            crossfade_secs: 0.0,
            crossfade_incoming_gain: 1.0,
            outgoing: None,
            pushed_samples: start_samples,
            speed: speed::SpeedStage::new(f32::from_bits(shared.speed.load(Ordering::Relaxed))),
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
                DeviceCheckOutcome::PauseOnLouder(req) => {
                    shared.position.store(req.start_nanosec, Ordering::Relaxed);
                    *shared.play_state.lock() = EngineState::Paused;
                    paused_req = Some(req);
                    let _ = event_tx.send(AudioEvent::Paused);
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

            advance_transition_and_preload_signal(out, &mut session, &shared, &event_tx);
            forward_stream_title(&session.current, &event_tx);

            maybe_start_crossfade(&mut session, &shared);

            match handle_eof(out, &mut session, &shared.play_state, &event_tx) {
                EofOutcome::NotEof => {}
                EofOutcome::ContinueDecode => continue 'decode,
                EofOutcome::BreakDecode => break 'decode,
            }

            match decode_one_packet(out, &mut session, &shared, &event_tx) {
                DecodeStepOutcome::Decoded => {}
                DecodeStepOutcome::ContinueDecode => continue 'decode,
                DecodeStepOutcome::BreakDecode => break 'decode,
            }
        }
    }
}

/// Why a track couldn't be readied to play.
enum StartFailure {
    /// The track itself couldn't be opened.
    Track(String),
    /// No output could be built for it; carries the track back so it can be
    /// parked and retried on Resume.
    NoOutput(Track, String),
}

/// Opens `track` at `start_nanosec` and readies the output to play it: builds
/// one if there is none, and reopens a bit-perfect one at the track's own
/// rate when it was opened for another (see `rate_switch`). The track comes
/// back decoding for the format the output actually runs at. Only called
/// where playback starts afresh, so nothing queued in the ring buffer is lost.
fn start_on_output(
    output: &mut Option<AudioOutput>,
    shared: &Arc<AudioShared>,
    event_tx: &mpsc::Sender<AudioEvent>,
    track: Track,
    start_nanosec: u64,
) -> Result<ActiveTrack, StartFailure> {
    // The output format isn't settled until the track's rate is known, so
    // open for the current one and retarget below.
    let (rate, channels) = output
        .as_ref()
        .map_or((crate::sink::DEFAULT_BIT_PERFECT_RATE, 2), |o| {
            (o.sample_rate, o.channels)
        });
    let mut active = ActiveTrack::open(
        &shared.open_source,
        track,
        start_nanosec,
        rate,
        channels as usize,
    )
    .map_err(StartFailure::Track)?;

    let reopen_at = match output.as_ref() {
        None => Some(active.src_rate),
        Some(out) => out.rate_switch_for(active.src_rate),
    };
    if let Some(rate) = reopen_at {
        let reopening = output.is_some();
        if let Some(old_out) = output.take() {
            log::info!("Reopening the output at {rate} Hz for the next track");
            let _ = old_out.sink.pause();
        }
        let new_out = build_output_on_fresh_thread(shared, Some(rate))
            .map_err(|message| StartFailure::NoOutput(active.spec.clone(), message))?;
        new_out.publish(shared, event_tx);
        *output = Some(new_out);
        if reopening {
            let _ = event_tx.send(AudioEvent::PipelineChanged);
        }
    }
    let out = output.as_ref().expect("output was just built");
    active.retarget(out.sample_rate, out.channels);
    Ok(active)
}

/// Opens the queued next track for a handover from the current one, or
/// `None` when there can be none: it failed to open, or it needs the output
/// at another rate (see `rate_switch`). Either way the current track then
/// drains, the engine reports `TrackFinished`, and the player's `Play` opens
/// the next track (at its own rate).
fn open_preload(
    out: &AudioOutput,
    session: &DecodeSession,
    shared: &AudioShared,
    preq: PlayRequest,
) -> Option<ActiveTrack> {
    match ActiveTrack::open(
        &shared.open_source,
        preq.track,
        preq.start_nanosec,
        session.target_sample_rate,
        session.target_channels as usize,
    ) {
        Ok(t) if out.rate_switch_for(t.src_rate).is_some() => {
            log::debug!("Next track is at {} Hz: no seamless handover", t.src_rate);
            None
        }
        Ok(t) => {
            log::debug!("Preloaded next track {}", t.spec.id);
            Some(t)
        }
        Err(e) => {
            log::warn!("Preloading the next track failed: {e}");
            None
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
    let force_rebuild = std::mem::take(&mut session.force_device_rebuild);
    let stream_errored = output
        .as_ref()
        .map(|o| o.sink.is_errored())
        .unwrap_or(false);

    let output_cfg = shared.output_config.read().clone();
    let is_tracking_default = is_tracking_default(&output_cfg);

    let check_due =
        now.duration_since(session.last_device_check) >= std::time::Duration::from_millis(500);

    let (default_changed, new_default_name) =
        if is_tracking_default && (check_due || stream_errored) {
            session.last_device_check = now;
            let current_dev_name = get_default_device_name();
            let old_dev_name = output.as_ref().and_then(|o| o.device_name.clone());
            (current_dev_name != old_dev_name, current_dev_name)
        } else {
            if check_due {
                session.last_device_check = now;
            }
            (false, None)
        };

    #[cfg(not(target_os = "linux"))]
    let _ = &new_default_name;

    // On Linux, PipeWire/PulseAudio reroutes the virtual "default" ALSA stream
    // seamlessly when the default sink changes, so we only need to publish the
    // new device name and notify the app (no stream teardown/glitch).
    #[cfg(target_os = "linux")]
    if default_changed && !force_rebuild && !stream_errored {
        use cpal::traits::HostTrait;
        let is_virtual = cpal::default_host()
            .default_output_device()
            .is_some_and(|d| is_virtual_default_device(&d));
        if is_virtual {
            if let Some(out) = output.as_mut() {
                out.device_name = new_default_name.clone();
                *shared.output_device_name.write() = new_default_name;
                let _ = event_tx.send(AudioEvent::PipelineChanged);
                return DeviceCheckOutcome::Ok;
            }
        }
    }

    // Only polled while standing in, so the enumeration costs nothing in the
    // usual case.
    let chosen_returned = check_due
        && output.as_ref().is_some_and(|o| o.stands_in)
        && !chosen_device_missing(&output_cfg);

    if force_rebuild || stream_errored || default_changed || chosen_returned {
        log::info!(
            "Audio output rebuild triggered (forced: {force_rebuild}, errored: {stream_errored}, default_changed: {default_changed}, chosen device returned: {chosen_returned}). Rebuilding audio stream..."
        );

        let cur_pos = shared.position.load(Ordering::Relaxed);
        // Position still belongs to the finished track while a transition
        // is draining, so that's the song to park on a failed rebuild.
        let track_if_lost = match session.transition.as_ref() {
            Some(t) => t.finished.clone(),
            None => session.current.spec.clone(),
        };
        let old_backend = output.as_ref().map(|o| o.sink.backend());
        if let Some(old_out) = output.as_ref() {
            let _ = old_out.sink.pause();
        }
        *output = None;

        match build_output_on_fresh_thread(shared, Some(session.current.src_rate)) {
            Ok(new_out)
                if old_backend
                    .is_some_and(|old| bypasses_system_volume(old, new_out.sink.backend())) =>
            {
                log::info!(
                    "Output moved to {} (no system volume): pausing until playback resumes",
                    new_out.sink.backend()
                );
                new_out.publish(shared, event_tx);
                *output = Some(new_out);
                let _ = event_tx.send(AudioEvent::PipelineChanged);
                return DeviceCheckOutcome::PauseOnLouder(PlayRequest {
                    track: track_if_lost,
                    start_nanosec: cur_pos,
                });
            }
            Ok(new_out) => {
                new_out.publish(shared, event_tx);
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
                    if !session.current.spec.live {
                        let target_time = symphonia::core::units::Time::from_nanos(cur_pos as i64);
                        let _ = session.current.format.seek(
                            symphonia::core::formats::SeekMode::Accurate,
                            symphonia::core::formats::SeekTo::Time {
                                time: target_time,
                                track_id: Some(session.current.track_id),
                            },
                        );
                        session.current.decoder.reset();
                    }
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
                let start_samples =
                    samples_for_ns(cur_pos, session.target_sample_rate, session.target_channels);
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
    let position = &shared.position;
    match cmd {
        Ok(AudioCommand::Pause(dur_ms)) => {
            fade_out_then(shared, dur_ms, || {
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
            fade_out_then(shared, dur_ms, || {
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
            if session.current.spec.live && session.transition.is_none() {
                return CmdOutcome::Continue;
            }
            seek_session(target_ns, out, session, shared, event_tx)
        }
        // Already at this speed (a preset that is already selected): nothing to
        // restart, so no audible hiccup.
        Ok(AudioCommand::SetSpeed(speed))
            if speed == f32::from_bits(shared.speed.load(Ordering::Relaxed)) =>
        {
            CmdOutcome::Continue
        }
        Ok(AudioCommand::SetSpeed(speed)) => {
            // The ring buffer holds audio decoded at the old speed, so
            // restart it at the audible position, then publish the new
            // speed for the output callback (#2142). For a live stream
            // we cannot seek back, so only update the speed stage.
            let outcome = if session.current.spec.live && session.transition.is_none() {
                CmdOutcome::Continue
            } else {
                seek_session(
                    position.load(Ordering::Relaxed),
                    out,
                    session,
                    shared,
                    event_tx,
                )
            };
            session.speed.set_speed(speed);
            shared
                .speed_carry
                .store(0.0f64.to_bits(), Ordering::Relaxed);
            shared.speed.store(speed.to_bits(), Ordering::Relaxed);
            outcome
        }
        Ok(AudioCommand::PreloadNext(preq)) => {
            if session.current.spec.live {
                return CmdOutcome::Continue;
            }
            session.next = open_preload(out, session, shared, preq);
            session.crossfade_secs = 0.0;
            CmdOutcome::Continue
        }
        Ok(AudioCommand::PreloadNextCrossfade(preq, secs, gain)) => {
            if session.current.spec.live {
                return CmdOutcome::Continue;
            }
            session.next = open_preload(out, session, shared, preq);
            session.crossfade_secs = if session.next.is_some() { secs } else { 0.0 };
            session.crossfade_incoming_gain = gain;
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
        Ok(AudioCommand::SyncHardwareVolume) => {
            out.sink.set_hardware_volume(shared.current_volume());
            CmdOutcome::Continue
        }
    }
}

/// Move the live session to `target_ns` on the current track: reopen or seek
/// the decoder, restart the ring buffer and rebase the position counters.
fn seek_session(
    target_ns: u64,
    out: &mut AudioOutput,
    session: &mut DecodeSession,
    shared: &Arc<AudioShared>,
    event_tx: &mpsc::Sender<AudioEvent>,
) -> CmdOutcome {
    let position = &shared.position;
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
    // Whatever the speed stage buffered belongs to the old position.
    session.speed.reset();
    CmdOutcome::Continue
}

/// Forwards the newest `StreamTitle` the track's ICY reader has queued since
/// the last iteration as [`AudioEvent::StreamTitle`] (#2008). Runs on the
/// decode thread, so it is checked even while the ring buffer is full.
fn forward_stream_title(track: &ActiveTrack, event_tx: &mpsc::Sender<AudioEvent>) {
    if let Some(title) = track.metadata_rx.as_ref().and_then(latest_queued) {
        let _ = event_tx.send(AudioEvent::StreamTitle {
            track_id: track.spec.id,
            title,
        });
    }
}

/// Drains `rx` without blocking and returns the last value, if any: only the
/// newest title matters once several have queued up.
fn latest_queued(rx: &mpsc::Receiver<String>) -> Option<String> {
    rx.try_iter().last()
}

/// Drives steps 3+4 of one `'decode` iteration: complete a pending gapless
/// handover once its boundary has been played, then fire the one-shot
/// `AboutToFinish` signal if the preload window has been entered. Neither
/// original block branches the loop's control flow, so both stay combined
/// here as plain side-effecting steps.
fn advance_transition_and_preload_signal(
    out: &AudioOutput,
    session: &mut DecodeSession,
    shared: &AudioShared,
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
            shared
                .position
                .store(session.current.start_ns, Ordering::Relaxed);
            *shared.input_info.write() = Some(session.current.input_info.clone());
            session.current.input_changed = false;
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
            let pos = shared.position.load(Ordering::Relaxed);
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

    // Whatever the speed stage still holds belongs to this track: play it
    // out before a handover or the drain, then start the next track clean.
    let tail = session.speed.flush();
    if !tail.is_empty() {
        push_to_ring(out, &tail);
    }

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
    shared: &AudioShared,
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
    let step = decode_track_packet(&mut session.current, channels);
    if session.current.input_changed && session.transition.is_none() {
        session.current.input_changed = false;
        *shared.input_info.write() = Some(session.current.input_info.clone());
        let _ = event_tx.send(AudioEvent::PipelineChanged);
    }
    let mut resampled = match step {
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

    // Timeline samples consumed, before the speed stage changes the length.
    let timeline_samples = resampled.len() as u64;
    let resampled = session.speed.process(
        resampled,
        session.target_sample_rate,
        session.target_channels as usize,
    );
    push_to_ring(out, &resampled);
    session.pushed_samples += timeline_samples;
    DecodeStepOutcome::Decoded
}

/// Push `samples` into the ring buffer, waiting for the output to make room.
fn push_to_ring(out: &mut AudioOutput, samples: &[f32]) {
    let mut pushed = 0;
    while pushed < samples.len() {
        let written = out.producer.push_slice(&samples[pushed..]);
        if written == 0 {
            // Ring buffer is full, sleep a bit and try again
            std::thread::sleep(std::time::Duration::from_millis(5));
        } else {
            pushed += written;
        }
    }
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
                    // A decoder can settle on its real output format only
                    // after a few packets (HE-AAC through Media Foundation,
                    // #2011); follow it rather than resample at a stale rate.
                    let spec = decoded.spec();
                    let (rate, channels) = (spec.rate(), spec.channels().count());
                    let mut sample_vec: Vec<f32> = Vec::new();
                    decoded.copy_to_vec_interleaved(&mut sample_vec);
                    if rate != track.src_rate || channels != track.src_channels {
                        track.src_rate = rate;
                        track.src_channels = channels;
                        track.resampler =
                            Resampler::new(rate, track.resampler.to_rate, target_channels);
                        let updated =
                            build_input_info(track.spec.id, &*track.decoder, rate, channels);
                        if updated != track.input_info {
                            track.input_info = updated;
                            track.input_changed = true;
                        }
                    }
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
                Err(SymphoniaError::ResetRequired) => {
                    track.decoder.reset();
                    PacketStep::Skip
                }
                Err(_) => PacketStep::DecoderFailed,
            }
        }
        Err(SymphoniaError::ResetRequired) => {
            // Chained Ogg streams (common on Icecast Vorbis/Opus radio stations)
            // start a new logical bitstream with a new serial number / track_id
            // at song boundaries (#2002). Re-bind to the new default audio track.
            match ProbedTrack::build_decoder_for_default_track(
                &*track.format,
                &symphonia::core::codecs::audio::AudioDecoderOptions::default(),
            ) {
                Ok(source::DefaultTrackDecoder {
                    decoder,
                    track_id,
                    sample_rate,
                    channels,
                    ..
                }) => {
                    track.decoder = decoder;
                    track.track_id = track_id;
                    if sample_rate != track.src_rate || channels != track.src_channels {
                        track.src_rate = sample_rate;
                        track.src_channels = channels;
                        track.resampler =
                            Resampler::new(sample_rate, track.resampler.to_rate, target_channels);
                    }
                    let updated = build_input_info(
                        track.spec.id,
                        &*track.decoder,
                        track.src_rate,
                        track.src_channels,
                    );
                    if updated != track.input_info {
                        track.input_info = updated;
                        track.input_changed = true;
                    }
                    PacketStep::Skip
                }
                Err(message) => PacketStep::ReadFailed(message),
            }
        }
        Ok(None) if track.spec.live => {
            PacketStep::ReadFailed("Live stream disconnected".to_string())
        }
        Ok(None) => {
            track.eof = true;
            PacketStep::Skip
        }
        Err(SymphoniaError::IoError(ref e))
            if e.kind() == std::io::ErrorKind::UnexpectedEof && !track.spec.live =>
        {
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

    fn chosen(backend: AudioBackendKind, device_id: &str) -> AudioOutputConfig {
        AudioOutputConfig {
            backend,
            device_id: Some(device_id.to_string()),
        }
    }

    #[test]
    fn an_unplugged_chosen_device_is_missing_until_it_is_listed_again() {
        for backend in [
            AudioBackendKind::Alsa,
            AudioBackendKind::WasapiExclusive,
            AudioBackendKind::Cpal,
        ] {
            let cfg = chosen(backend, "hw:CARD=EarPods,DEV=0");
            assert!(device_missing(&cfg, |_, _| false), "{backend} unplugged");
            assert!(!device_missing(&cfg, |_, _| true), "{backend} back");
        }
    }

    #[test]
    fn the_chosen_id_is_looked_up_in_its_own_backends_list() {
        let cfg = chosen(AudioBackendKind::Alsa, "hw:CARD=EarPods,DEV=0");
        assert!(!device_missing(&cfg, |backend, id| {
            backend == AudioBackendKind::Alsa && id == "hw:CARD=EarPods,DEV=0"
        }));
    }

    #[test]
    fn nothing_is_missing_without_a_chosen_device_or_for_asio() {
        let unreachable = |_: AudioBackendKind, _: &str| -> bool {
            panic!("no device list should be enumerated")
        };
        for backend in [AudioBackendKind::Alsa, AudioBackendKind::Cpal] {
            for device_id in [None, Some(String::new())] {
                let cfg = AudioOutputConfig { backend, device_id };
                assert!(!device_missing(&cfg, unreachable), "{backend}");
            }
        }
        let asio = chosen(
            AudioBackendKind::Asio,
            "462F2ABF-5278-436A-95B6-72CBF65482AE",
        );
        assert!(!device_missing(&asio, unreachable));
    }

    #[test]
    fn moving_from_shared_to_bit_perfect_output_bypasses_system_volume() {
        for new in [
            AudioBackendKind::Asio,
            AudioBackendKind::WasapiExclusive,
            AudioBackendKind::Alsa,
        ] {
            assert!(bypasses_system_volume(AudioBackendKind::Cpal, new));
        }
    }

    #[test]
    fn other_output_moves_keep_the_level() {
        use AudioBackendKind::*;
        // Rebuilding the same kind, or falling back to shared output (quieter
        // at worst), never jumps up in level.
        for (old, new) in [
            (Cpal, Cpal),
            (Asio, Asio),
            (Asio, Cpal),
            (WasapiExclusive, Cpal),
        ] {
            assert!(!bypasses_system_volume(old, new), "{old} -> {new}");
        }
    }

    fn test_shared() -> AudioShared {
        AudioShared {
            position: AtomicU64::new(0),
            volume: AtomicU32::new(1.0f32.to_bits()),
            volume_ceiling: AtomicU32::new(1.0f32.to_bits()),
            open_source: Box::new(open_file),
            play_state: Mutex::new(EngineState::Playing),
            visualizer_buf: spectrum::AudioVisualizerBuffer::new(4096),
            spectrum_enabled: std::sync::atomic::AtomicBool::new(false),
            output_sample_rate: AtomicU32::new(44100),
            output_channels: std::sync::atomic::AtomicU16::new(2),
            output_device_name: parking_lot::RwLock::new(None),
            output_backend_name: parking_lot::RwLock::new("Test".to_string()),
            output_bypasses_mixer: std::sync::atomic::AtomicBool::new(false),
            output_fell_back: std::sync::atomic::AtomicBool::new(false),
            output_format: parking_lot::RwLock::new("32-bit float PCM"),
            output_config: parking_lot::RwLock::new(crate::sink::AudioOutputConfig::default()),
            input_info: parking_lot::RwLock::new(None),
            equalizer: Mutex::new(luminous_equalizer::Equalizer::new()),
            loudness_gain: AtomicU32::new(1.0f32.to_bits()),
            loudness_generation: AtomicU64::new(0),
            loudness_switch_at: AtomicU64::new(NO_LOUDNESS_SWITCH),
            loudness_switch_gain: AtomicU32::new(1.0f32.to_bits()),
            speed: AtomicU32::new(1.0f32.to_bits()),
            speed_carry: AtomicU64::new(0.0f64.to_bits()),
            fade_gain: AtomicU32::new(1.0f32.to_bits()),
            fade_generation: AtomicU64::new(0),
            hardware_volume_active: std::sync::atomic::AtomicBool::new(false),
            output_hardware_volume: std::sync::atomic::AtomicBool::new(false),
            event_tx: mpsc::channel().0,
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
    fn latest_queued_keeps_only_the_newest_stream_title() {
        let (tx, rx) = mpsc::channel::<String>();
        assert_eq!(latest_queued(&rx), None, "nothing queued");
        tx.send("A - One".into()).unwrap();
        tx.send("B - Two".into()).unwrap();
        assert_eq!(latest_queued(&rx).as_deref(), Some("B - Two"));
        assert_eq!(latest_queued(&rx), None, "drained");
        drop(tx);
        assert_eq!(latest_queued(&rx), None, "a closed reader is not an error");
    }

    #[test]
    fn volume_never_exceeds_the_safety_ceiling() {
        let shared = test_shared();
        assert_eq!(shared.store_volume(0.9), 0.9, "no ceiling: any level");
        assert_eq!(shared.store_volume(1.5), 1.0, "still clamped to 1.0");

        // Setting a ceiling below the current level lowers it at once.
        assert_eq!(shared.set_volume_ceiling(Some(0.7)), 0.7);
        assert_eq!(shared.current_volume(), 0.7);

        // A request above the ceiling sets the ceiling; below it is kept.
        assert_eq!(shared.store_volume(0.95), 0.7);
        assert_eq!(shared.store_volume(0.4), 0.4);

        // Raising the ceiling doesn't raise the volume, nor does lifting it.
        assert_eq!(shared.set_volume_ceiling(Some(0.9)), 0.4);
        assert_eq!(shared.set_volume_ceiling(None), 0.4);
        assert_eq!(shared.store_volume(1.0), 1.0, "lifted: full range again");
    }

    #[test]
    fn external_volume_is_kept_under_the_ceiling_and_reported() {
        let (tx, rx) = mpsc::channel();
        let shared = AudioShared {
            event_tx: tx,
            ..test_shared()
        };
        shared.set_volume_ceiling(Some(0.6));

        assert_eq!(shared.accept_external_volume(0.5), 0.5);
        assert!(matches!(rx.try_recv(), Ok(AudioEvent::VolumeChanged(v)) if v == 0.5));

        // A knob turned past the ceiling is held at it, and the held level is
        // what the app hears about.
        assert_eq!(shared.accept_external_volume(0.9), 0.6);
        assert_eq!(shared.current_volume(), 0.6);
        assert!(matches!(rx.try_recv(), Ok(AudioEvent::VolumeChanged(v)) if v == 0.6));
    }

    #[test]
    fn hardware_volume_bypasses_software_gain_multiplier() {
        let shared = test_shared();
        shared.volume.store(0.5f32.to_bits(), Ordering::Relaxed);
        let shared = Arc::new(shared);
        let rb = HeapRb::<f32>::new(16);
        let (mut prod, cons) = rb.split();
        let _ = prod.try_push(0.8);
        let _ = prod.try_push(0.8);
        let _ = prod.try_push(-0.4);
        let _ = prod.try_push(-0.4);

        let mut renderer = OutputRenderer {
            shared: Arc::clone(&shared),
            consumer: Arc::new(Mutex::new(cons)),
            played_samples: Arc::new(AtomicU64::new(0)),
            mono_scratch: Vec::new(),
            sample_rate: 44100,
            channels: 2,
        };

        // 1) With hardware volume inactive, 0.5 software gain is applied: 0.8 * 0.5 = 0.4
        let mut out = [0.0f32; 2];
        renderer.render(&mut out);
        assert!((out[0] - 0.4).abs() < 1e-6);
        assert!((out[1] - 0.4).abs() < 1e-6);

        // 2) With hardware volume active, samples pass through at unity: -0.4 * 1.0 = -0.4
        shared.hardware_volume_active.store(true, Ordering::Relaxed);
        let mut out2 = [0.0f32; 2];
        renderer.render(&mut out2);
        assert!((out2[0] - (-0.4)).abs() < 1e-6);
        assert!((out2[1] - (-0.4)).abs() < 1e-6);
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
        assert_eq!(loudness_switch_offset(1200, 1000, 512, 1.0), Some(200));
        // Exactly at the buffer's end: fires now (offset = whole buffer), so
        // the next buffer isn't racing the decode thread's counter rebase.
        assert_eq!(loudness_switch_offset(1512, 1000, 512, 1.0), Some(512));
        assert_eq!(loudness_switch_offset(1513, 1000, 512, 1.0), None);
        // Already passed (e.g. a buffer with no output): apply immediately.
        assert_eq!(loudness_switch_offset(900, 1000, 512, 1.0), Some(0));
        assert_eq!(
            loudness_switch_offset(NO_LOUDNESS_SWITCH, 1000, 512, 1.0),
            None
        );
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
    fn bit_perfect_outputs_reopen_only_for_a_rate_not_yet_asked_for() {
        use AudioBackendKind::*;
        for backend in [Asio, WasapiExclusive] {
            assert_eq!(rate_switch(backend, 44_100, 44_100, 96_000), Some(96_000));
            // Same rate as the open output: reuse it (gapless stays gapless).
            assert_eq!(rate_switch(backend, 96_000, 96_000, 96_000), None);
            // Asked for 352.8 kHz and got 176.4: don't ask again, and a
            // 176.4 kHz track already plays natively.
            assert_eq!(rate_switch(backend, 352_800, 176_400, 352_800), None);
            assert_eq!(rate_switch(backend, 352_800, 176_400, 176_400), None);
            // A 48 kHz-only device asked for 44.1: neither rate reopens it.
            assert_eq!(rate_switch(backend, 44_100, 48_000, 48_000), None);
            assert_eq!(rate_switch(backend, 44_100, 48_000, 44_100), None);
        }
    }

    #[test]
    fn shared_output_never_follows_the_track_rate() {
        // Also the stand-in when a bit-perfect backend fell back.
        assert_eq!(
            rate_switch(AudioBackendKind::Cpal, 44_100, 48_000, 96_000),
            None
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

    /// A renderer over a ring buffer pre-filled with `frames` of silence.
    fn renderer_with_audio(
        shared: &Arc<AudioShared>,
        frames: usize,
    ) -> (OutputRenderer, Arc<AtomicU64>) {
        let len = frames * 2;
        let (mut producer, consumer) = HeapRb::<f32>::new(len).split();
        producer.push_slice(&vec![0.0f32; len]);
        let played = Arc::new(AtomicU64::new(0));
        let renderer = OutputRenderer {
            shared: Arc::clone(shared),
            consumer: Arc::new(Mutex::new(consumer)),
            played_samples: Arc::clone(&played),
            mono_scratch: Vec::with_capacity(len),
            sample_rate: 48_000,
            channels: 2,
        };
        (renderer, played)
    }

    #[test]
    fn position_runs_on_the_track_timeline_at_any_speed() {
        for (speed, expected_secs) in [(1.0f32, 1.0f64), (2.0, 2.0), (0.5, 0.5), (1.05, 1.05)] {
            let shared = Arc::new(test_shared());
            shared.speed.store(speed.to_bits(), Ordering::Relaxed);
            // One second of output, rendered in odd-sized buffers so the
            // fractional carry is exercised.
            let (mut renderer, _) = renderer_with_audio(&shared, 48_000);
            let mut buf = vec![0.0f32; 2 * 441];
            let mut rendered = 0;
            while rendered + buf.len() <= 2 * 48_000 {
                renderer.render(&mut buf);
                rendered += buf.len();
            }
            let secs = shared.position.load(Ordering::Relaxed) as f64 / 1e9;
            let played_secs = rendered as f64 / 2.0 / 48_000.0;
            assert!(
                (secs - played_secs * expected_secs).abs() < 0.001,
                "speed {speed}: position {secs}s after {played_secs}s of output"
            );
        }
    }

    #[test]
    fn loudness_switch_offset_converts_timeline_samples_at_speed() {
        // A 512-sample buffer at 2x covers 1024 timeline samples.
        assert_eq!(loudness_switch_offset(1400, 1000, 512, 2.0), Some(200));
        assert_eq!(loudness_switch_offset(2024, 1000, 512, 2.0), Some(512));
        assert_eq!(loudness_switch_offset(2025, 1000, 512, 2.0), None);
        // At half speed the same buffer covers 256 timeline samples.
        assert_eq!(loudness_switch_offset(1100, 1000, 512, 0.5), Some(200));
        assert_eq!(loudness_switch_offset(1257, 1000, 512, 0.5), None);
    }

    #[test]
    fn setting_the_speed_returns_the_snapped_value() {
        let engine = AudioEngine::new();
        assert_eq!(engine.set_playback_speed(1.12).unwrap(), 1.10);
        assert_eq!(engine.set_playback_speed(99.0).unwrap(), MAX_SPEED);
        assert_eq!(engine.set_playback_speed(0.0).unwrap(), MIN_SPEED);
        assert_eq!(engine.set_playback_speed(f32::NAN).unwrap(), 1.0);
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
    fn ramp_fade_gain_lands_on_its_target() {
        let shared = Arc::new(test_shared());
        shared.ramp_fade_gain(0.0, 1.0, 30);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while f32::from_bits(shared.fade_gain.load(Ordering::Relaxed)) != 1.0 {
            assert!(
                std::time::Instant::now() < deadline,
                "fade ramp never reached unity"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn set_fade_gain_cancels_a_ramp_in_flight() {
        let shared = Arc::new(test_shared());
        shared.ramp_fade_gain(0.0, 1.0, 2_000);
        shared.set_fade_gain(0.4);
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert_eq!(
            f32::from_bits(shared.fade_gain.load(Ordering::Relaxed)),
            0.4
        );
    }

    #[test]
    fn fade_out_then_ramps_from_current_gain_and_preempts_ramp() {
        let shared = Arc::new(test_shared());
        shared.ramp_fade_gain(0.0, 1.0, 2_000);
        std::thread::sleep(std::time::Duration::from_millis(40));
        let mid_gain = f32::from_bits(shared.fade_gain.load(Ordering::Relaxed));
        assert!(mid_gain > 0.0 && mid_gain < 1.0);

        let mut then_called = false;
        fade_out_then(&shared, 30, || {
            then_called = true;
            assert_eq!(
                f32::from_bits(shared.fade_gain.load(Ordering::Relaxed)),
                0.0
            );
        });
        assert!(then_called);
        assert_eq!(
            f32::from_bits(shared.fade_gain.load(Ordering::Relaxed)),
            1.0
        );
    }

    #[test]
    fn resume_with_fade_leaves_fade_gain_below_one_at_first_rendered_buffer() {
        let shared = Arc::new(test_shared());
        shared.ramp_fade_gain(0.0, 1.0, 300);

        const FRAMES: usize = 256;
        const CHANNELS: u16 = 2;
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
        let input = vec![1.0f32; len];
        producer.push_slice(&input);

        let mut output = vec![0.0f32; len];
        renderer.render(&mut output);

        let current_fade = f32::from_bits(shared.fade_gain.load(Ordering::Relaxed));
        assert!(
            current_fade < 1.0,
            "expected initial fade gain to be below 1.0, got {current_fade}"
        );
        assert!(
            output[0] < 1.0,
            "expected rendered audio to be attenuated, got {}",
            output[0]
        );
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

    #[test]
    fn live_track_disables_initial_seek_end_cutoffs_and_treats_disconnect_as_error() {
        let dir = std::env::temp_dir().join(format!("luminous-audio-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("live.wav");
        write_test_pcm16_wav(&wav, 48_000, &[vec![100; 480]]);
        let real = wav.to_string_lossy().into_owned();
        let opener = move |_: &str| open_file(&real);
        let spec = Track {
            id: 99,
            location: "http://radio.example/live".into(),
            extension_hint: Some("wav".into()),
            end_ns: Some(500_000),
            length_ns: Some(1_000_000),
            live: true,
        };
        // Even when passed a non-zero start_nanosec, end_ns, and length_ns,
        // a live track starts at 0 and has no end_ns or about_end_ns.
        let mut track =
            ActiveTrack::open(&opener, spec, 500_000_000, 48_000, 2).expect("opens live track");
        assert_eq!(track.start_ns, 0);
        assert_eq!(track.decoded_pos_ns, 0);
        assert!(track.end_ns.is_none());
        assert!(track.about_end_ns.is_none());

        // Drain packets until the underlying stream ends: a live track must report
        // ReadFailed (which emits AudioEvent::Error), never setting `track.eof = true`.
        let mut saw_read_failed = false;
        for _ in 0..32 {
            match decode_track_packet(&mut track, 2) {
                PacketStep::Samples(_) | PacketStep::Skip => {}
                PacketStep::ReadFailed(msg) => {
                    assert!(
                        msg.contains("Live stream disconnected") || msg.contains("Decode error"),
                        "unexpected live disconnect message: {msg}"
                    );
                    saw_read_failed = true;
                    break;
                }
                PacketStep::DecoderFailed => panic!("unexpected decoder failure"),
            }
        }
        assert!(
            saw_read_failed,
            "live track disconnect must surface ReadFailed"
        );
        assert!(
            !track.eof,
            "live track disconnect must never be treated as normal EOF"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extension_from_location_strips_query_and_fragment() {
        assert_eq!(
            extension_from_location("https://stream.example.com/radio.mp3?token=abc#live"),
            Some("mp3")
        );
        assert_eq!(
            extension_from_location("https://stream.example.com/live"),
            None
        );
        assert_eq!(extension_from_location("/music/track.flac"), Some("flac"));
    }

    #[test]
    fn cpal_error_fatality_classification() {
        assert!(!is_fatal_cpal_error(&cpal::Error::new(
            cpal::ErrorKind::Xrun
        )));
        assert!(!is_fatal_cpal_error(&cpal::Error::new(
            cpal::ErrorKind::RealtimeDenied
        )));

        assert!(is_fatal_cpal_error(&cpal::Error::new(
            cpal::ErrorKind::DeviceNotAvailable
        )));
        assert!(is_fatal_cpal_error(&cpal::Error::new(
            cpal::ErrorKind::StreamInvalidated
        )));
        assert!(is_fatal_cpal_error(&cpal::Error::new(
            cpal::ErrorKind::DeviceBusy
        )));
        assert!(is_fatal_cpal_error(&cpal::Error::new(
            cpal::ErrorKind::DeviceChanged
        )));
        assert!(is_fatal_cpal_error(&cpal::Error::new(
            cpal::ErrorKind::BackendError
        )));
        assert!(is_fatal_cpal_error(&cpal::Error::new(
            cpal::ErrorKind::Other
        )));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_virtual_alias_and_wpctl_parsing() {
        assert!(is_virtual_sound_server_alias("Default Audio Device"));
        assert!(is_virtual_sound_server_alias("alsa:default"));
        assert!(is_virtual_sound_server_alias("PipeWire Sound Server"));
        assert!(is_virtual_sound_server_alias("default"));
        assert!(is_virtual_sound_server_alias(
            "Default ALSA Output (currently PipeWire Media Server)"
        ));
        assert!(!is_virtual_sound_server_alias("soundcore Liberty 5"));
        assert!(!is_virtual_sound_server_alias(
            "HDA Intel PCH, ALC289 Analog"
        ));

        let sample = "\
id 76, type PipeWire:Interface:Node
  * node.description = \"soundcore Liberty 5\"
  * node.name = \"bluez_output.90_BF_D9_68_B3_7E.1\"
";
        assert_eq!(
            parse_wpctl_default_sink(sample).as_deref(),
            Some("soundcore Liberty 5")
        );

        let no_desc = "  * node.name = \"alsa_output.pci-0000_00_1f.3.analog-stereo\"\n";
        assert_eq!(
            parse_wpctl_default_sink(no_desc).as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo")
        );
    }

    #[test]
    fn active_track_populates_input_info_and_detects_format_changes_per_track() {
        let dir = std::env::temp_dir().join(format!(
            "luminous_input_info_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("tone.wav");
        write_test_pcm16_wav(&wav, 44_100, &[vec![100; 441]]);
        let real = wav.to_string_lossy().into_owned();
        let location = real.clone();
        let opener = move |_: &str| open_file(&real);
        let spec = Track {
            id: 77,
            location,
            extension_hint: None,
            end_ns: None,
            length_ns: None,
            live: false,
        };
        let mut track = ActiveTrack::open(&opener, spec, 0, 48_000, 2).expect("opens wav");
        assert_eq!(track.input_info.track_id, 77);
        assert_eq!(track.input_info.codec, "pcm_s16le");
        assert_eq!(track.input_info.format, "PCM");
        assert_eq!(track.input_info.sample_rate, 44_100);
        assert_eq!(track.input_info.channels, 1);
        assert_eq!(track.input_info.decoder_name, "Symphonia PCM decoder");
        assert!(!track.input_info.aac_core_only);
        assert!(!track.input_changed);

        // Simulate a stale initial rate before the first packet settles on 44.1 kHz:
        // decode_track_packet marks `track.input_changed` without touching `AudioShared`.
        track.src_rate = 22_050;
        track.input_info.sample_rate = 22_050;
        let step = decode_track_packet(&mut track, 2);
        assert!(matches!(step, PacketStep::Samples(_)));
        assert!(track.input_changed);
        assert_eq!(track.input_info.sample_rate, 44_100);

        // Verify AAC core-only flag logic via build_input_info.
        let mf_info = symphonia::core::codecs::CodecInfo {
            short_name: "aac",
            long_name: "Media Foundation AAC",
            profiles: &[],
        };
        let core_info = symphonia::core::codecs::CodecInfo {
            short_name: "aac",
            long_name: "Advanced Audio Coding",
            profiles: &[],
        };
        let opus_info = symphonia::core::codecs::CodecInfo {
            short_name: "opus",
            long_name: "Opus",
            profiles: &[],
        };
        assert_eq!(
            input_info_from_codec(1, &core_info, 24_000, 2).aac_core_only,
            true
        );
        assert_eq!(
            input_info_from_codec(1, &core_info, 44_100, 2).aac_core_only,
            false
        );
        let mf = input_info_from_codec(1, &mf_info, 24_000, 2);
        assert!(!mf.aac_core_only);
        assert_eq!(mf.decoder_name, "Media Foundation AAC");
        assert_eq!(
            input_info_from_codec(1, &opus_info, 48_000, 2).decoder_name,
            "libopus (via Symphonia)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
