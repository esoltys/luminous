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
//! ## DSP chain (contract shared with #77/#79)
//! `decode → loudness gain (#77) → EQ preamp → EQ bands → fade envelope (#79)
//! → volume → clip guard → output`. Each gain stage is a single precomputed
//! multiplier read from an atomic — nothing in the output callback allocates
//! or blocks. When every stage is neutral (EQ off, gains at 1.0) samples pass
//! through bit-perfect.

use crate::models::{
    AudioPipelineInfo, FileType, LoudnessGainSource, PlayState, QualityTier, Song,
};
use anyhow::{anyhow, Result};
use cpal::traits::StreamTrait;
use parking_lot::Mutex;

/// Pure function mapping codec + bitrate + sample rate + bit depth to a `QualityTier` (#1041).
/// - LQ — lossy codec below 256 kbps
/// - SQ — lossy codec at 256 kbps or higher
/// - HQ — lossless codec (standard resolution: <= 48 kHz and <= 16-bit)
/// - Hi-Res — lossless codec with sample rate above 48 kHz or bit depth above 16-bit
pub fn classify_quality_tier(
    filetype: FileType,
    bitrate_kbps: Option<i32>,
    sample_rate: Option<u32>,
    bit_depth: Option<i32>,
) -> QualityTier {
    if filetype.is_lossless() {
        let is_hires =
            sample_rate.is_some_and(|r| r > 48_000) || bit_depth.is_some_and(|d| d > 16);
        if is_hires {
            QualityTier::HiRes
        } else {
            QualityTier::Hq
        }
    } else {
        let kbps = bitrate_kbps.unwrap_or(0);
        if kbps >= 256 {
            QualityTier::Sq
        } else {
            QualityTier::Lq
        }
    }
}

use ringbuf::{
    traits::{Consumer, Observer, Producer, Split},
    HeapRb,
};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    mpsc, Arc,
};
use symphonia::core::{
    codecs::audio::{AudioDecoder, AudioDecoderOptions},
    errors::Error as SymphoniaError,
    formats::{probe::Hint, FormatOptions, FormatReader, TrackType},
    io::MediaSource,
    io::MediaSourceStream,
    meta::MetadataOptions,
};

/// How far before the end boundary the `AboutToFinish` signal fires.
/// Consumers: gapless preload (here), auto-crossfade (#79), CUE
/// sibling-track continuation (#78).
pub const PRELOAD_LEAD_NS: u64 = 8_000_000_000;

/// Maximum linear amplitude allowed when dynamic processing (loudness normalization
/// or equalizer) is active. Set to -1.0 dBTP (True Peak) ≈ 0.8912509 linear amplitude
/// (10^(-1/20)) to prevent inter-sample clipping during digital-to-analog reconstruction.
pub const TRUE_PEAK_CEILING: f32 = 0.891_250_9;

/// Step count and per-step sleep duration for a gain ramp spread over
/// `duration_ms` in ~10ms increments. Shared by `apply_fade_ramp`
/// (decode-thread, `std::thread::sleep`) and `ramp_gain` (async callers,
/// `tokio::time::sleep`) — only the sleep primitive differs between them.
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

fn apply_fade_ramp(fade_gain: &Arc<AtomicU32>, start_gain: f32, end_gain: f32, duration_ms: u32) {
    if duration_ms == 0 {
        fade_gain.store(end_gain.to_bits(), Ordering::Relaxed);
        return;
    }
    let (steps, step_dur) = ramp_steps(duration_ms);
    for i in 0..=steps {
        let g = ramp_gain_at_step(start_gain, end_gain, steps, i);
        fade_gain.store(g.to_bits(), Ordering::Relaxed);
        std::thread::sleep(step_dur);
    }
}

/// Ramp a gain atomic smoothly from its current value to `target` over
/// `duration_ms`, for a caller already on an async task (e.g.
/// `Player::refresh_loudness_gain`, which uses this on `AudioEngine`'s
/// `loudness_gain` handle after a mid-playback settings change — unlike a
/// track-start's instant `set_loudness_gain`, stepping the level hard here
/// would be an audible click/zipper in the middle of continuous audio).
/// Shares its step math with `apply_fade_ramp`; uses `tokio::time::sleep`
/// instead of blocking the decode thread, and operates on a handle
/// (`AudioEngine::loudness_gain_handle`) rather than the engine itself so
/// the caller isn't holding `AudioEngine`'s lock for the ramp's duration.
pub async fn ramp_gain(handle: &Arc<AtomicU32>, target: f32, duration_ms: u32) {
    let start = f32::from_bits(handle.load(Ordering::Relaxed));
    if (target - start).abs() < f32::EPSILON {
        return;
    }
    if duration_ms == 0 {
        handle.store(target.to_bits(), Ordering::Relaxed);
        return;
    }
    let (steps, step_dur) = ramp_steps(duration_ms);
    for i in 0..=steps {
        let g = ramp_gain_at_step(start, target, steps, i);
        handle.store(g.to_bits(), Ordering::Relaxed);
        tokio::time::sleep(step_dur).await;
    }
}

// ---------------------------------------------------------------------------
// Control messages sent to the decode thread
// ---------------------------------------------------------------------------

pub enum AudioCommand {
    Play(PlayRequest),
    Cue(PlayRequest),
    Pause,
    PauseWithFade(u32),
    Resume,
    ResumeWithFade(u32),
    Stop,
    StopWithFade(u32),
    SeekTo(u64), // target position in nanoseconds
    /// Prime the next track for a gapless transition after the current one.
    PreloadNext(PlayRequest),
    /// Prime the next track for an auto-crossfade transition (#79).
    PreloadNextCrossfade(PlayRequest, f32),
    /// Drop a primed next track (playback context changed) and re-arm the
    /// `AboutToFinish` signal so a fresh preload can be requested.
    ClearPreload,
}

pub struct PlayRequest {
    pub song: Box<Song>,
    pub start_nanosec: u64,
}

// ---------------------------------------------------------------------------
// Events emitted from the decode thread back to the player
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum AudioEvent {
    Playing {
        song_id: i64,
    },
    Paused,
    Stopped,
    PositionChanged {
        position_nanosec: u64,
    },
    TrackFinished {
        song_id: i64,
    },
    /// The playing track is within `PRELOAD_LEAD_NS` of its end boundary.
    AboutToFinish {
        song_id: i64,
    },
    /// A gapless transition completed: `finished_song_id` played to its end
    /// and `song_id` is now audible, with no interruption of the stream.
    TrackTransitioned {
        finished_song_id: i64,
        song_id: i64,
    },
    /// The audio pipeline has changed (e.g. output device reconnected or changed format).
    PipelineChanged,
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Shared audio-graph handles — bundled so the decode/output plumbing threads
// a single value instead of 6-10 individual Arcs through every layer.
// ---------------------------------------------------------------------------

pub(crate) struct AudioShared {
    position: Arc<AtomicU64>,
    volume: Arc<AtomicU32>,
    play_state: Arc<Mutex<PlayState>>,
    visualizer_buf: Arc<crate::analyzer::AudioVisualizerBuffer>,
    output_sample_rate: Arc<AtomicU32>,
    output_channels: Arc<std::sync::atomic::AtomicU16>,
    output_device_name: Arc<parking_lot::RwLock<Option<String>>>,
    active_decoder_name: Arc<parking_lot::RwLock<Option<String>>>,
    equalizer: Arc<Mutex<crate::equalizer::Equalizer>>,
    loudness_gain: Arc<AtomicU32>,
    fade_gain: Arc<AtomicU32>,
}

// ---------------------------------------------------------------------------
// AudioEngine — public handle
// ---------------------------------------------------------------------------

pub struct AudioEngine {
    cmd_tx: mpsc::SyncSender<AudioCommand>,
    event_rx: Arc<Mutex<mpsc::Receiver<AudioEvent>>>,
    pub position_nanosec: Arc<AtomicU64>,
    pub volume: Arc<AtomicU32>,
    pub play_state: Arc<Mutex<PlayState>>,
    visualizer_buf: Arc<crate::analyzer::AudioVisualizerBuffer>,
    spectrum_enabled: Arc<std::sync::atomic::AtomicBool>,
    /// Actual output device sample rate, updated once the CPAL stream is
    /// built. The spectrum analyzer needs this to convert FFT bin indices
    /// to real Hz instead of assuming a fixed rate.
    output_sample_rate: Arc<AtomicU32>,
    equalizer: Arc<Mutex<crate::equalizer::Equalizer>>,
    /// Per-track loudness-normalization multiplier (#77). f32 bits in an
    /// atomic so the audio callback reads it without locking. 1.0 = neutral.
    loudness_gain: Arc<AtomicU32>,
    /// Fade-envelope multiplier slot (#79). 1.0 = neutral.
    pub fade_gain: Arc<AtomicU32>,
    pub(crate) shared: Arc<AudioShared>,
}

/// Runs a synchronous `AudioEngine` operation while `audio`'s async mutex is
/// held, via `block_in_place` rather than directly — some `AudioEngine`
/// callers (e.g. equalizer preset persistence) do rusqlite work, and running
/// that straight on the tokio worker would both stall the runtime and block
/// every other task waiting on the same mutex for the duration (#1097, #1102).
/// Mirrors `playlist::with_playlists`. Note: this locks `AppState.audio`
/// (`tokio::sync::Mutex<AudioEngine>`), distinct from the `parking_lot::Mutex`
/// fields inside `AudioEngine` used on the allocation-free audio callback path.
pub async fn with_audio<F, R>(audio: &tokio::sync::Mutex<AudioEngine>, f: F) -> R
where
    F: FnOnce(&mut AudioEngine) -> R,
{
    let mut a = audio.lock().await;
    tokio::task::block_in_place(move || f(&mut a))
}

impl AudioEngine {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::sync_channel::<AudioCommand>(64);
        let (event_tx, event_rx) = mpsc::channel::<AudioEvent>();
        let position = Arc::new(AtomicU64::new(0));
        let volume = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let play_state = Arc::new(Mutex::new(PlayState::Stopped));
        let visualizer_buf = Arc::new(crate::analyzer::AudioVisualizerBuffer::new(4096));
        let spectrum_enabled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let output_sample_rate = Arc::new(AtomicU32::new(44100));
        let output_channels = Arc::new(std::sync::atomic::AtomicU16::new(2));
        let output_device_name = Arc::new(parking_lot::RwLock::new(get_default_device_name()));
        let active_decoder_name = Arc::new(parking_lot::RwLock::new(None));
        let equalizer = Arc::new(Mutex::new(crate::equalizer::Equalizer::new()));
        let loudness_gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let fade_gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));

        let shared = Arc::new(AudioShared {
            position: Arc::clone(&position),
            volume: Arc::clone(&volume),
            play_state: Arc::clone(&play_state),
            visualizer_buf: Arc::clone(&visualizer_buf),
            output_sample_rate: Arc::clone(&output_sample_rate),
            output_channels: Arc::clone(&output_channels),
            output_device_name: Arc::clone(&output_device_name),
            active_decoder_name: Arc::clone(&active_decoder_name),
            equalizer: Arc::clone(&equalizer),
            loudness_gain: Arc::clone(&loudness_gain),
            fade_gain: Arc::clone(&fade_gain),
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
            event_rx: Arc::new(Mutex::new(event_rx)),
            position_nanosec: position,
            volume,
            play_state,
            visualizer_buf,
            spectrum_enabled,
            output_sample_rate,
            equalizer,
            loudness_gain,
            fade_gain,
            shared,
        }
    }

    fn send_cmd(&self, cmd: AudioCommand) -> Result<()> {
        self.cmd_tx
            .send(cmd)
            .map_err(|_| anyhow!("audio thread shut down"))
    }

    pub fn play(&self, song: Box<Song>, start_nanosec: u64) -> Result<()> {
        self.send_cmd(AudioCommand::Play(PlayRequest {
            song,
            start_nanosec,
        }))
    }

    pub fn cue(&self, song: Box<Song>, start_nanosec: u64) -> Result<()> {
        {
            let mut s = self.play_state.lock();
            *s = crate::models::PlayState::Paused;
        }
        self.position_nanosec
            .store(start_nanosec, Ordering::Relaxed);
        self.send_cmd(AudioCommand::Cue(PlayRequest {
            song,
            start_nanosec,
        }))
    }

    pub fn preload_next(&self, song: Box<Song>, start_nanosec: u64) -> Result<()> {
        self.send_cmd(AudioCommand::PreloadNext(PlayRequest {
            song,
            start_nanosec,
        }))
    }

    pub fn preload_next_with_crossfade(
        &self,
        song: Box<Song>,
        start_nanosec: u64,
        crossfade_secs: f32,
    ) -> Result<()> {
        self.send_cmd(AudioCommand::PreloadNextCrossfade(
            PlayRequest {
                song,
                start_nanosec,
            },
            crossfade_secs,
        ))
    }

    pub fn clear_preload(&self) -> Result<()> {
        self.send_cmd(AudioCommand::ClearPreload)
    }

    pub fn pause(&self) -> Result<()> {
        self.send_cmd(AudioCommand::Pause)
    }

    pub fn pause_with_fade(&self, fade_ms: u32) -> Result<()> {
        self.send_cmd(AudioCommand::PauseWithFade(fade_ms))
    }

    pub fn resume(&self) -> Result<()> {
        self.send_cmd(AudioCommand::Resume)
    }

    pub fn resume_with_fade(&self, fade_ms: u32) -> Result<()> {
        self.send_cmd(AudioCommand::ResumeWithFade(fade_ms))
    }

    pub fn stop(&self) -> Result<()> {
        self.send_cmd(AudioCommand::Stop)
    }

    pub fn stop_with_fade(&self, fade_ms: u32) -> Result<()> {
        self.send_cmd(AudioCommand::StopWithFade(fade_ms))
    }

    pub fn seek_to(&self, position_nanosec: u64) -> Result<()> {
        self.send_cmd(AudioCommand::SeekTo(position_nanosec))
    }

    pub fn set_volume(&self, vol: f32) -> Result<()> {
        let vol = vol.clamp(0.0, 1.0);
        self.volume.store(vol.to_bits(), Ordering::Relaxed);
        Ok(())
    }

    /// Set the per-track loudness-normalization multiplier (#77).
    pub fn set_loudness_gain(&self, gain: f32) {
        self.loudness_gain
            .store(gain.max(0.0).to_bits(), Ordering::Relaxed);
    }

    /// Set the fade-envelope multiplier (#79).
    pub fn set_fade_gain(&self, gain: f32) {
        self.fade_gain
            .store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn current_position_nanosec(&self) -> u64 {
        self.position_nanosec.load(Ordering::Relaxed)
    }

    pub fn current_volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub fn current_state(&self) -> PlayState {
        *self.play_state.lock()
    }

    /// One controlled escape hatch for equalizer mutation/inspection —
    /// centralizes locking instead of callers reaching into the field.
    pub fn with_equalizer<R>(&self, f: impl FnOnce(&mut crate::equalizer::Equalizer) -> R) -> R {
        let mut eq = self.equalizer.lock();
        f(&mut eq)
    }

    pub fn spectrum_enabled(&self) -> bool {
        self.spectrum_enabled.load(Ordering::Relaxed)
    }

    pub fn set_spectrum_enabled(&self, enabled: bool) {
        self.spectrum_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Compute a spectrum snapshot from the current visualizer buffer at the
    /// engine's actual output sample rate. Returns `None` before the output
    /// stream has been built (sample rate not yet known to be accurate).
    pub fn spectrum_snapshot(&self, fft_size: usize) -> Vec<f32> {
        let sample_rate = self.output_sample_rate.load(Ordering::Relaxed);
        crate::analyzer::calculate_spectrum(&self.visualizer_buf, fft_size, sample_rate)
    }

    /// Snapshots the current audio pipeline configuration for the active track (#1041).
    pub fn get_pipeline_info(
        &self,
        current_song: Option<&Song>,
        loudness_source: LoudnessGainSource,
        loudness_gain_db: Option<f32>,
    ) -> Option<AudioPipelineInfo> {
        let song = current_song?;

        let bitrate = song.bitrate;
        let sample_rate = song.samplerate.map(|r| r as u32);
        let bit_depth = song.bitdepth;
        let quality_tier = classify_quality_tier(song.filetype, bitrate, sample_rate, bit_depth);

        let input_source = song.source;
        let input_format = song.filetype.display_name().to_string();
        let input_codec = match song.filetype {
            FileType::Mp3 => "mp3",
            FileType::Flac | FileType::OggFlac => "flac",
            FileType::OggVorbis => "vorbis",
            FileType::OggOpus => "opus",
            FileType::OggSpeex => "speex",
            FileType::Aac => "aac",
            FileType::Alac => "alac",
            FileType::Aiff => "pcm_s16be",
            FileType::Wav => "pcm_s16le",
            FileType::WavPack => "wavpack",
            FileType::Mpc => "musepack",
            FileType::TrueAudio => "trueaudio",
            FileType::Ape => "monkeys_audio",
            FileType::Dsf | FileType::Dsdiff => "dsd",
            FileType::Asf => "wma",
            FileType::Stream => "stream",
            FileType::Unknown => "unknown",
        }
        .to_string();

        let decoder_name = self
            .shared
            .active_decoder_name
            .read()
            .clone()
            .unwrap_or_else(|| format!("Symphonia {} decoder", song.filetype.display_name()));
        let headroom = "32-bit float PCM".to_string();

        let out_rate = self.shared.output_sample_rate.load(Ordering::Relaxed);
        let resample_rate = if let Some(in_rate) = sample_rate {
            if in_rate != out_rate && out_rate > 0 {
                Some(out_rate)
            } else {
                None
            }
        } else {
            None
        };

        let (eq_enabled, eq_mode, eq_preamp_db, eq_active_bands_count) = {
            let eq = self.equalizer.lock();
            let mode_str = match eq.mode {
                crate::equalizer::EqMode::Graphic10 => "10-band Graphic",
                crate::equalizer::EqMode::Parametric20 => "20-band Parametric",
            };
            let active_bands = match eq.mode {
                crate::equalizer::EqMode::Graphic10 => {
                    eq.gains.iter().filter(|&&g| g.abs() > 0.01).count()
                }
                crate::equalizer::EqMode::Parametric20 => eq
                    .parametric
                    .iter()
                    .filter(|f| f.gain_db.abs() > 0.01)
                    .count(),
            };
            (
                eq.enabled,
                Some(mode_str.to_string()),
                Some(eq.preamp),
                active_bands,
            )
        };

        let limiter = "-1.0 dBTP True Peak limiter".to_string();
        let output_sample_rate = if out_rate > 0 { out_rate } else { 44100 };
        let output_channels = {
            let ch = self.shared.output_channels.load(Ordering::Relaxed);
            if ch > 0 {
                ch
            } else {
                2
            }
        };
        let output_format = "32-bit float PCM".to_string();
        let output_device_name = self
            .shared
            .output_device_name
            .read()
            .clone()
            .unwrap_or_else(|| {
                get_default_device_name().unwrap_or_else(|| "Default Audio Device".to_string())
            });
        let output_backend = "CPAL".to_string();

        Some(AudioPipelineInfo {
            quality_tier,
            input_source,
            input_format,
            input_codec,
            input_bitrate_kbps: bitrate,
            input_sample_rate: sample_rate,
            input_bit_depth: bit_depth,
            input_channels: song.channels.map(|c| c as u16),
            input_path: song.path.clone().or_else(|| song.url.clone()),
            decoder_name,
            headroom,
            resample_rate,
            loudness_source,
            loudness_gain_db,
            eq_enabled,
            eq_mode,
            eq_preamp_db,
            eq_active_bands_count,
            limiter,
            output_sample_rate,
            output_channels,
            output_format,
            output_device_name,
            output_backend,
        })
    }

    /// A cheaply-cloneable handle to the event receiver. Callers lock it
    /// themselves and block on `Receiver::iter()` on their own thread —
    /// cloning the handle (rather than blocking here) lets the caller drop
    /// its `AudioEngine` lock before entering that blocking loop.
    pub fn events(&self) -> Arc<Mutex<mpsc::Receiver<AudioEvent>>> {
        Arc::clone(&self.event_rx)
    }

    /// A cheaply-cloneable handle to the loudness-gain atomic, for
    /// subsystems that need shared cross-thread read/write access (e.g.
    /// ramping it smoothly rather than stepping it via `set_loudness_gain`).
    pub fn loudness_gain_handle(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.loudness_gain)
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Media source & track opening (source-agnostic seam for #682 WebDAV / #82 streaming)
// ---------------------------------------------------------------------------

/// Splits a `user:pass@` prefix out of a URL's authority, if present, returning
/// the credential-free URL and a ready-to-use `Authorization: Basic ...` header
/// value. WebDAV playback URLs carry credentials embedded as userinfo (see
/// `WebDavClient::build_authenticated_url`) since there's no separate credential
/// lookup available here — just the bare URL string stored on the `Song`.
fn extract_basic_auth(url: &str) -> (String, Option<String>) {
    let Ok(mut parsed) = reqwest::Url::parse(url) else {
        return (url.to_string(), None);
    };
    let username = parsed.username().to_string();
    let password = parsed.password().map(|p| p.to_string());
    if username.is_empty() && password.is_none() {
        return (url.to_string(), None);
    }

    use percent_encoding::percent_decode_str;
    let decoded_user = percent_decode_str(&username)
        .decode_utf8_lossy()
        .to_string();
    let decoded_pass = password
        .as_deref()
        .map(|p| percent_decode_str(p).decode_utf8_lossy().to_string())
        .unwrap_or_default();

    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);

    use base64::Engine;
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(format!("{decoded_user}:{decoded_pass}"));
    (parsed.to_string(), Some(format!("Basic {encoded}")))
}

/// A seekable HTTP media source using HTTP Range requests (`Range: bytes=start-end`).
/// Enables streaming audio from WebDAV and remote HTTP endpoints without full downloads.
pub struct HttpRangeReader {
    url: String,
    auth_header: Option<String>,
    client: reqwest::blocking::Client,
    content_length: u64,
    position: u64,
    buffer: Vec<u8>,
    buffer_start: u64,
    chunk_size: usize,
}

impl HttpRangeReader {
    pub fn new(url: &str) -> Result<Self, String> {
        let (url, auth_header) = extract_basic_auth(url);
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        // Issue a HEAD request to discover Content-Length and verify reachability
        let mut head_req = client.head(&url);
        if let Some(ref h) = auth_header {
            head_req = head_req.header(reqwest::header::AUTHORIZATION, h);
        }
        let resp = head_req
            .send()
            .map_err(|e| format!("HEAD request failed for '{url}': {e}"))?;

        if !resp.status().is_success() {
            return Err(format!(
                "HTTP error {} when accessing '{url}'",
                resp.status()
            ));
        }

        let content_length = resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let mut reader = Self {
            url: url.to_string(),
            auth_header,
            client,
            content_length,
            position: 0,
            buffer: Vec::new(),
            buffer_start: 0,
            chunk_size: 256 * 1024, // 256 KB buffer chunk
        };

        // Pre-fetch initial chunk so first read is immediate
        reader.fill_buffer(0)?;
        Ok(reader)
    }

    fn fill_buffer(&mut self, start: u64) -> Result<(), String> {
        let end = if self.content_length > 0 {
            (start + self.chunk_size as u64 - 1).min(self.content_length - 1)
        } else {
            start + self.chunk_size as u64 - 1
        };

        let range_header = format!("bytes={start}-{end}");
        let mut req = self
            .client
            .get(&self.url)
            .header(reqwest::header::RANGE, range_header);
        if let Some(ref h) = self.auth_header {
            req = req.header(reqwest::header::AUTHORIZATION, h);
        }
        let mut resp = req
            .send()
            .map_err(|e| format!("Range request failed for '{}': {e}", self.url))?;

        if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            self.buffer_start = start;
            self.buffer.clear();
            return Ok(());
        }

        if !resp.status().is_success() && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(format!(
                "Range request returned unexpected status {}",
                resp.status()
            ));
        }

        let mut data = Vec::new();
        resp.copy_to(&mut data)
            .map_err(|e| format!("Failed to read stream chunk: {e}"))?;

        if self.content_length == 0 && resp.status() == reqwest::StatusCode::OK {
            self.content_length = data.len() as u64;
        }

        self.buffer_start = start;
        self.buffer = data;
        Ok(())
    }
}

impl Read for HttpRangeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.content_length > 0 && self.position >= self.content_length {
            return Ok(0); // EOF
        }

        let in_buffer = self.position >= self.buffer_start
            && self.position < self.buffer_start + self.buffer.len() as u64;

        if !in_buffer {
            if let Err(e) = self.fill_buffer(self.position) {
                return Err(std::io::Error::other(e));
            }
            if self.buffer.is_empty() {
                return Ok(0);
            }
        }

        let offset_in_buffer = (self.position - self.buffer_start) as usize;
        let available = self.buffer.len().saturating_sub(offset_in_buffer);
        if available == 0 {
            return Ok(0);
        }

        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&self.buffer[offset_in_buffer..offset_in_buffer + to_read]);
        self.position += to_read as u64;
        Ok(to_read)
    }
}

impl Seek for HttpRangeReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::Current(offset) => self.position as i64 + offset,
            SeekFrom::End(offset) => {
                if self.content_length > 0 {
                    self.content_length as i64 + offset
                } else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Cannot seek from end of stream with unknown length",
                    ));
                }
            }
        };

        if new_pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot seek to a negative position",
            ));
        }

        self.position = new_pos as u64;
        Ok(self.position)
    }
}

impl MediaSource for HttpRangeReader {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        if self.content_length > 0 {
            Some(self.content_length)
        } else {
            None
        }
    }
}

/// Open a playable media source. Local files or remote HTTP/WebDAV endpoints (#682).
/// Shared with the offline analyzers (`analyzer::decode_all_samples`,
/// `loudness::decode_channels`) so waveform/band-waveform generation and R128
/// loudness analysis also work against WebDAV songs, not just live playback.
pub(crate) fn open_media_source(path: &str) -> Result<Box<dyn MediaSource>, String> {
    if path.starts_with("http://") || path.starts_with("https://") {
        let reader = HttpRangeReader::new(path)?;
        Ok(Box::new(reader))
    } else {
        let file =
            std::fs::File::open(path).map_err(|e| format!("Cannot open file '{path}': {e}"))?;
        Ok(Box::new(file))
    }
}

/// A fully opened, probed, decode-ready track.
struct ActiveTrack {
    song: Box<Song>,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    src_rate: u32,
    src_channels: usize,
    resampler: Resampler,
    start_ns: u64,
    /// Hard decode cutoff (CUE end boundary), from `songs.end_nanosec`.
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
        song: Box<Song>,
        start_nanosec: u64,
        target_rate: u32,
        target_channels: usize,
    ) -> Result<Self, String> {
        let path = song
            .path
            .as_deref()
            .or(song.stream_url.as_deref())
            .or(song.url.as_deref())
            .ok_or("Song has no playable path or URL".to_string())?
            .to_owned();

        let source = open_media_source(&path)?;
        let mut hint = Hint::new();
        if let Some(ext) = Path::new(&path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        } else if let Some(ext) = song
            .path
            .as_deref()
            .and_then(|p| Path::new(p).extension())
            .and_then(|e| e.to_str())
        {
            hint.with_extension(ext);
        }

        let mss = MediaSourceStream::new(source, Default::default());
        let mut format = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| format!("Format probe failed: {e}"))?;

        let track = format
            .default_track(TrackType::Audio)
            .cloned()
            .ok_or_else(|| "No audio track found".to_string())?;

        let track_id = track.id;
        let audio_params = track
            .codec_params
            .as_ref()
            .and_then(|c| c.audio())
            .ok_or_else(|| "No audio codec parameters".to_string())?;
        let mut decoder = symphonia::default::get_codecs()
            .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
            .map_err(|e| format!("Decoder init failed: {e}"))?;

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

        let src_rate = audio_params.sample_rate.unwrap_or(44100);
        let src_channels = audio_params
            .channels
            .as_ref()
            .map(|c| c.count())
            .unwrap_or(2);

        let end_ns = (song.end_nanosec > 0).then_some(song.end_nanosec as u64);
        let about_end_ns = end_ns.or_else(|| song.length_nanosec.map(|ns| ns.max(0) as u64));

        Ok(Self {
            song,
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
    finished_song: Box<Song>,
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
    stream: cpal::Stream,
    producer: ringbuf::HeapProd<f32>,
    consumer: Arc<Mutex<ringbuf::HeapCons<f32>>>,
    played_samples: Arc<AtomicU64>,
    sample_rate: u32,
    channels: u16,
    device_name: Option<String>,
    stream_error_flag: Arc<std::sync::atomic::AtomicBool>,
}

fn get_default_device_name() -> Option<String> {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    Some(device.to_string())
}

fn build_output(shared: &Arc<AudioShared>) -> Result<AudioOutput, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "No audio output device".to_string())?;
    let device_name = Some(device.to_string());
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
    let shared_consumer_reader = Arc::clone(&shared_consumer);

    let played_samples = Arc::new(AtomicU64::new(0));
    let played_samples_cpal = Arc::clone(&played_samples);
    let vol_ref = Arc::clone(&shared.volume);
    let position_cpal = Arc::clone(&shared.position);
    let visualizer_buf_cpal = Arc::clone(&shared.visualizer_buf);
    let eq_cpal = Arc::clone(&shared.equalizer);
    let loudness_cpal = Arc::clone(&shared.loudness_gain);
    let fade_cpal = Arc::clone(&shared.fade_gain);

    // Pre-allocated scratch for the visualizer's mono downmix — the output
    // callback must never allocate. Sized for the whole ring buffer, far
    // larger than any single callback burst.
    let mut mono_scratch: Vec<f32> = Vec::with_capacity(buffer_capacity);

    let stream_error_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let err_flag_cpal = Arc::clone(&stream_error_flag);

    let stream = device
        .build_output_stream(
            config,
            move |output: &mut [f32], _| {
                let vol = f32::from_bits(vol_ref.load(Ordering::Relaxed));
                let loudness = f32::from_bits(loudness_cpal.load(Ordering::Relaxed));
                let fade = f32::from_bits(fade_cpal.load(Ordering::Relaxed));
                let mut played = 0;

                // Non-blocking try_lock ensures CPAL callback never stalls
                if let Some(mut consumer) = shared_consumer_reader.try_lock() {
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

                // 1) Per-track loudness normalization gain (#77)
                if loudness != 1.0 {
                    for sample in output[..played].iter_mut() {
                        *sample *= loudness;
                    }
                }

                // 2) Equalizer (preamp + band cascade; no-op when disabled)
                let mut eq_applied = false;
                if let Some(mut eq) = eq_cpal.try_lock() {
                    eq_applied = eq.enabled;
                    eq.process_interleaved(&mut output[..played]);
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
                if loudness != 1.0 || eq_applied {
                    for sample in output[..played].iter_mut() {
                        *sample = sample.clamp(-TRUE_PEAK_CEILING, TRUE_PEAK_CEILING);
                    }
                }

                if played > 0 {
                    let channels_u = target_channels as usize;
                    mono_scratch.clear();
                    for chunk in output[..played].chunks(channels_u) {
                        let sum: f32 = chunk.iter().sum();
                        mono_scratch.push(sum / target_channels as f32);
                    }
                    visualizer_buf_cpal.push(&mono_scratch);
                }

                let total_played =
                    played_samples_cpal.fetch_add(played as u64, Ordering::Relaxed) + played as u64;
                let pos_ns = (total_played as f64 * 1_000_000_000.0
                    / (target_sample_rate as f64 * target_channels as f64))
                    as u64;
                position_cpal.store(pos_ns, Ordering::Relaxed);
            },
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

    Ok(AudioOutput {
        stream,
        producer: prod,
        consumer: shared_consumer,
        played_samples,
        sample_rate: target_sample_rate,
        channels: target_channels,
        device_name,
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
    /// Absolute count of samples pushed to the ring buffer, in the same
    /// "space" as `AudioOutput::played_samples` (kept in sync at every
    /// buffer clear).
    pushed_samples: u64,
    target_sample_rate: u32,
    target_channels: u16,
    last_device_check: std::time::Instant,
}

enum DeviceCheckOutcome {
    Ok,
    BreakDecode,
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
                                    let _ = event_tx.send(AudioEvent::Error { message });
                                    continue;
                                }
                            }
                        }
                        let out = output.as_mut().unwrap();
                        let target_sample_rate = out.sample_rate;
                        let target_channels = out.channels;

                        match ActiveTrack::open(
                            r.song.clone(),
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
                                let _ = out.stream.pause();
                                {
                                    let mut s = shared.play_state.lock();
                                    *s = PlayState::Paused;
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
                    Ok(AudioCommand::Resume) | Ok(AudioCommand::ResumeWithFade(_)) => {
                        if let Some(r) = paused_req.take() {
                            let cur_pos = shared.position.load(Ordering::Relaxed);
                            PlayRequest {
                                song: r.song,
                                start_nanosec: cur_pos,
                            }
                        } else {
                            continue;
                        }
                    }
                    Ok(AudioCommand::Stop) | Ok(AudioCommand::StopWithFade(_)) => {
                        if let Some(out) = output.as_ref() {
                            let _ = out.stream.pause();
                        }
                        paused_req = None;
                        shared.position.store(0, Ordering::Relaxed);
                        {
                            let mut s = shared.play_state.lock();
                            *s = PlayState::Stopped;
                        }
                        let _ = event_tx.send(AudioEvent::Stopped);
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
                Ok(AudioCommand::Stop) | Ok(AudioCommand::StopWithFade(_)) => {
                    if let Some(out) = output.as_ref() {
                        let _ = out.stream.pause();
                    }
                    paused_req = None;
                    shared.position.store(0, Ordering::Relaxed);
                    {
                        let mut s = shared.play_state.lock();
                        *s = PlayState::Stopped;
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
                    let _ = event_tx.send(AudioEvent::Error { message });
                    continue;
                }
            }
        }
        let out = output.as_mut().unwrap();
        let target_sample_rate = out.sample_rate;
        let target_channels = out.channels;

        let current = match ActiveTrack::open(
            req.song,
            req.start_nanosec,
            target_sample_rate,
            target_channels as usize,
        ) {
            Ok(t) => {
                *shared.active_decoder_name.write() = Some(format!(
                    "Symphonia {} decoder",
                    t.song.filetype.display_name()
                ));
                t
            }
            Err(message) => {
                let _ = event_tx.send(AudioEvent::Error { message });
                continue;
            }
        };
        let song_id = current.song.id;

        // Clear whatever was left in the buffer from the previous track and
        // reset the played-sample counter for this track's start offset.
        {
            let mut consumer = out.consumer.lock();
            while consumer.try_pop().is_some() {}
        }
        let start_samples = samples_for_ns(current.start_ns, target_sample_rate, target_channels);
        out.played_samples.store(start_samples, Ordering::Relaxed);

        if let Err(e) = out.stream.play() {
            let _ = event_tx.send(AudioEvent::Error {
                message: format!("CPAL stream play failed: {e}"),
            });
            continue;
        }

        {
            let mut s = shared.play_state.lock();
            *s = PlayState::Playing;
        }
        shared.position.store(current.start_ns, Ordering::Relaxed);
        let _ = event_tx.send(AudioEvent::Playing { song_id });

        let mut session = DecodeSession {
            current,
            next: None,
            transition: None,
            pushed_samples: start_samples,
            target_sample_rate,
            target_channels,
            last_device_check: std::time::Instant::now(),
        };

        'decode: loop {
            match check_and_rebuild_output(&mut output, &mut session, &shared, &event_tx) {
                DeviceCheckOutcome::Ok => {}
                DeviceCheckOutcome::BreakDecode => break 'decode,
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
            if let Some(old_out) = output.as_ref() {
                let _ = old_out.stream.pause();
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
                    session.target_sample_rate = new_out.sample_rate;
                    session.target_channels = new_out.channels;

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
                    session.transition = None;

                    if let Err(e) = new_out.stream.play() {
                        let _ = event_tx.send(AudioEvent::Error {
                            message: format!("CPAL stream play failed: {e}"),
                        });
                    }
                    *output = Some(new_out);
                    let _ = event_tx.send(AudioEvent::PipelineChanged);
                }
                Err(message) => {
                    let _ = event_tx.send(AudioEvent::Error { message });
                    return DeviceCheckOutcome::BreakDecode;
                }
            }
        }
    }
    DeviceCheckOutcome::Ok
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
        Ok(AudioCommand::Pause) => {
            let _ = out.stream.pause();
            {
                let mut s = play_state.lock();
                *s = PlayState::Paused;
            }
            let _ = event_tx.send(AudioEvent::Paused);
            // Position still belongs to the finished track while a
            // transition is draining — resume must reopen that song.
            let song_for_resume = match session.transition.as_ref() {
                Some(t) => t.finished_song.clone(),
                None => session.current.song.clone(),
            };
            *paused_req = Some(PlayRequest {
                song: song_for_resume,
                start_nanosec: position.load(Ordering::Relaxed),
            });
            CmdOutcome::BreakDecode
        }
        Ok(AudioCommand::PauseWithFade(dur_ms)) => {
            apply_fade_ramp(fade_gain, 1.0, 0.0, dur_ms);
            let _ = out.stream.pause();
            fade_gain.store(1.0f32.to_bits(), Ordering::Relaxed);
            {
                let mut s = play_state.lock();
                *s = PlayState::Paused;
            }
            let _ = event_tx.send(AudioEvent::Paused);
            let song_for_resume = match session.transition.as_ref() {
                Some(t) => t.finished_song.clone(),
                None => session.current.song.clone(),
            };
            *paused_req = Some(PlayRequest {
                song: song_for_resume,
                start_nanosec: position.load(Ordering::Relaxed),
            });
            CmdOutcome::BreakDecode
        }
        Ok(AudioCommand::Stop) => {
            let _ = out.stream.pause();
            {
                let mut s = play_state.lock();
                *s = PlayState::Stopped;
            }
            let _ = event_tx.send(AudioEvent::Stopped);
            CmdOutcome::BreakDecode
        }
        Ok(AudioCommand::StopWithFade(dur_ms)) => {
            apply_fade_ramp(fade_gain, 1.0, 0.0, dur_ms);
            let _ = out.stream.pause();
            fade_gain.store(1.0f32.to_bits(), Ordering::Relaxed);
            {
                let mut s = play_state.lock();
                *s = PlayState::Stopped;
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

            if let Some(t) = session.transition.take() {
                // Mid-handover seek: the audible position is still in
                // the finished track but its decoder is gone — reopen
                // it fresh at the target. The preload is dropped so
                // AboutToFinish re-arms naturally on the new track.
                session.next = None;
                match ActiveTrack::open(
                    t.finished_song,
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
                preq.song,
                preq.start_nanosec,
                session.target_sample_rate,
                session.target_channels as usize,
            ) {
                Ok(t) => {
                    log::debug!("Preloaded next track {} for gapless", t.song.id);
                    session.next = Some(t);
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
        Ok(AudioCommand::PreloadNextCrossfade(preq, _secs)) => {
            match ActiveTrack::open(
                preq.song,
                preq.start_nanosec,
                session.target_sample_rate,
                session.target_channels as usize,
            ) {
                Ok(t) => {
                    log::debug!("Preloaded next track {} for crossfade", t.song.id);
                    session.next = Some(t);
                }
                Err(e) => {
                    log::warn!("Crossfade preload failed: {e}");
                    session.next = None;
                }
            }
            CmdOutcome::Continue
        }
        Ok(AudioCommand::ClearPreload) => {
            if session.transition.is_none() {
                session.next = None;
                session.current.about_to_finish_sent = false;
            }
            CmdOutcome::Continue
        }
        Err(mpsc::TryRecvError::Empty) => CmdOutcome::Continue,
        Err(mpsc::TryRecvError::Disconnected) => CmdOutcome::BreakDecode,
        Ok(AudioCommand::Resume) | Ok(AudioCommand::ResumeWithFade(_)) => CmdOutcome::Continue, // already playing
        Ok(AudioCommand::Cue(_)) => CmdOutcome::Continue, // already playing
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
    position: &Arc<AtomicU64>,
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
            let finished_song_id = t.finished_song.id;
            let _ = event_tx.send(AudioEvent::TrackTransitioned {
                finished_song_id,
                song_id: session.current.song.id,
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
                    song_id: session.current.song.id,
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
    play_state: &Arc<Mutex<PlayState>>,
    event_tx: &mpsc::Sender<AudioEvent>,
) -> EofOutcome {
    if !session.current.eof {
        return EofOutcome::NotEof;
    }

    if session.transition.is_some() {
        // Waiting for the boundary to be consumed before the next (already
        // fully decoded) track can take over.
        std::thread::sleep(std::time::Duration::from_millis(10));
        return EofOutcome::ContinueDecode;
    }
    if let Some(n) = session.next.take() {
        // Gapless handover: continue decoding the preloaded track into the
        // same ring buffer — no drain, no pause.
        session.transition = Some(PendingTransition {
            boundary_samples: session.pushed_samples,
            finished_song: std::mem::replace(&mut session.current, n).song,
        });
        return EofOutcome::ContinueDecode;
    }

    // No preloaded next — classic drain-then-finish path.
    let is_empty = out.producer.occupied_len() == 0;
    if is_empty {
        let _ = event_tx.send(AudioEvent::TrackFinished {
            song_id: session.current.song.id,
        });
        let _ = out.stream.pause();
        {
            let mut s = play_state.lock();
            *s = PlayState::Stopped;
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

    match session.current.format.next_packet() {
        Ok(Some(packet)) => {
            if packet.track_id != session.current.track_id {
                return DecodeStepOutcome::ContinueDecode;
            }
            match session.current.decoder.decode(&packet) {
                Ok(decoded) => {
                    let mut sample_vec: Vec<f32> = Vec::new();
                    decoded.copy_to_vec_interleaved(&mut sample_vec);
                    let mut samples: &[f32] = &sample_vec;

                    // Enforce the CUE end boundary (`end_nanosec`):
                    // truncate the packet at the cut and treat the
                    // remainder of the file as EOF.
                    let frames = samples.len() / session.current.src_channels;
                    let packet_ns =
                        (frames as f64 * 1_000_000_000.0 / session.current.src_rate as f64) as u64;
                    if let Some(end_ns) = session.current.end_ns {
                        if session.current.decoded_pos_ns >= end_ns {
                            session.current.eof = true;
                            return DecodeStepOutcome::ContinueDecode;
                        }
                        if session.current.decoded_pos_ns + packet_ns > end_ns {
                            let keep_frames = ((end_ns - session.current.decoded_pos_ns) as f64
                                * session.current.src_rate as f64
                                / 1_000_000_000.0)
                                as usize;
                            samples = &samples[..keep_frames * session.current.src_channels];
                            session.current.eof = true;
                        }
                    }
                    session.current.decoded_pos_ns += packet_ns;

                    let channel_converted = convert_channels(
                        samples,
                        session.current.src_channels,
                        session.target_channels as usize,
                    );
                    let resampled = session.current.resampler.resample(&channel_converted);

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
                Err(SymphoniaError::DecodeError(_)) => DecodeStepOutcome::ContinueDecode,
                Err(_) => DecodeStepOutcome::BreakDecode,
            }
        }
        Ok(None) => {
            session.current.eof = true;
            DecodeStepOutcome::ContinueDecode
        }
        Err(SymphoniaError::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            session.current.eof = true;
            DecodeStepOutcome::ContinueDecode
        }
        Err(e) => {
            let _ = event_tx.send(AudioEvent::Error {
                message: format!("Decode error: {e}"),
            });
            DecodeStepOutcome::BreakDecode
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn extract_basic_auth_strips_credentials_and_builds_header() {
        let (clean_url, header) =
            extract_basic_auth("http://test:test@127.0.0.1:8080/Music/song.mp3");
        assert_eq!(clean_url, "http://127.0.0.1:8080/Music/song.mp3");
        // base64("test:test") == "dGVzdDp0ZXN0"
        assert_eq!(header.as_deref(), Some("Basic dGVzdDp0ZXN0"));
    }

    #[test]
    fn extract_basic_auth_no_credentials_is_unchanged() {
        let (clean_url, header) = extract_basic_auth("http://127.0.0.1:8080/Music/song.mp3");
        assert_eq!(clean_url, "http://127.0.0.1:8080/Music/song.mp3");
        assert!(header.is_none());
    }

    #[tokio::test]
    async fn http_range_reader_reads_and_seeks_correctly() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        let test_data = b"0123456789ABCDEFabcdefghijklmnopqrstuvwxyz";

        // Mock HEAD request
        Mock::given(method("HEAD"))
            .and(path("/track.mp3"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-length", test_data.len().to_string())
                    .insert_header("accept-ranges", "bytes"),
            )
            .mount(&mock_server)
            .await;

        // Mock GET request with Range parsing
        Mock::given(method("GET"))
            .and(path("/track.mp3"))
            .respond_with(|req: &wiremock::Request| {
                if let Some(range) = req
                    .headers
                    .get(wiremock::http::HeaderName::from_static("range"))
                {
                    let range_str = range.to_str().unwrap();
                    if let Some(bytes_part) = range_str.strip_prefix("bytes=") {
                        let parts: Vec<&str> = bytes_part.split('-').collect();
                        let start: usize = parts[0].parse().unwrap_or(0);
                        let end: usize = parts
                            .get(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(test_data.len() - 1);
                        let end = end.min(test_data.len() - 1);
                        if start <= end && start < test_data.len() {
                            let slice = &test_data[start..=end];
                            return ResponseTemplate::new(206)
                                .insert_header(
                                    "content-range",
                                    format!("bytes {start}-{end}/{}", test_data.len()),
                                )
                                .insert_header("content-length", slice.len().to_string())
                                .set_body_bytes(slice.to_vec());
                        }
                    }
                }
                ResponseTemplate::new(200).set_body_bytes(test_data.to_vec())
            })
            .mount(&mock_server)
            .await;

        let url = format!("{}/track.mp3", mock_server.uri());
        tokio::task::spawn_blocking(move || {
            let mut reader = HttpRangeReader::new(&url).expect("reader failed to initialize");
            assert_eq!(reader.byte_len(), Some(test_data.len() as u64));
            assert!(reader.is_seekable());

            // Read first 10 bytes
            let mut buf = [0u8; 10];
            reader.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"0123456789");

            // Seek to 16
            let pos = reader.seek(SeekFrom::Start(16)).unwrap();
            assert_eq!(pos, 16);

            // Read next 5 bytes
            let mut buf2 = [0u8; 5];
            reader.read_exact(&mut buf2).unwrap();
            assert_eq!(&buf2, b"abcde");

            // Seek from current (+2)
            let pos = reader.seek(SeekFrom::Current(2)).unwrap();
            assert_eq!(pos, 23); // 16 + 5 + 2

            let mut buf3 = [0u8; 3];
            reader.read_exact(&mut buf3).unwrap();
            assert_eq!(&buf3, b"hij");
        })
        .await
        .unwrap();
    }

    #[test]
    fn test_classify_quality_tier() {
        // Lossless: Standard (<= 48kHz, <= 16-bit) -> HQ
        assert_eq!(
            classify_quality_tier(FileType::Flac, Some(900), Some(44100), Some(16)),
            QualityTier::Hq
        );
        assert_eq!(
            classify_quality_tier(FileType::Wav, None, Some(48000), Some(16)),
            QualityTier::Hq
        );
        assert_eq!(
            classify_quality_tier(FileType::Alac, Some(850), Some(44100), None),
            QualityTier::Hq
        );

        // Lossless: High-Res (> 48kHz or > 16-bit) -> HiRes
        assert_eq!(
            classify_quality_tier(FileType::Flac, Some(1500), Some(96000), Some(24)),
            QualityTier::HiRes
        );
        assert_eq!(
            classify_quality_tier(FileType::Flac, Some(1100), Some(44100), Some(24)),
            QualityTier::HiRes
        );
        assert_eq!(
            classify_quality_tier(FileType::Aiff, None, Some(88200), Some(16)),
            QualityTier::HiRes
        );
        assert_eq!(
            classify_quality_tier(FileType::Dsf, None, Some(2822400), Some(1)),
            QualityTier::HiRes
        );

        // Lossy: >= 256 kbps -> SQ
        assert_eq!(
            classify_quality_tier(FileType::Mp3, Some(320), Some(44100), None),
            QualityTier::Sq
        );
        assert_eq!(
            classify_quality_tier(FileType::Mp3, Some(256), Some(44100), None),
            QualityTier::Sq
        );
        assert_eq!(
            classify_quality_tier(FileType::Aac, Some(256), Some(48000), None),
            QualityTier::Sq
        );
        assert_eq!(
            classify_quality_tier(FileType::OggOpus, Some(320), Some(48000), None),
            QualityTier::Sq
        );

        // Lossy: < 256 kbps -> LQ
        assert_eq!(
            classify_quality_tier(FileType::Mp3, Some(192), Some(44100), None),
            QualityTier::Lq
        );
        assert_eq!(
            classify_quality_tier(FileType::Mp3, Some(128), Some(44100), None),
            QualityTier::Lq
        );
        assert_eq!(
            classify_quality_tier(FileType::Aac, Some(96), Some(44100), None),
            QualityTier::Lq
        );
        assert_eq!(
            classify_quality_tier(FileType::Unknown, None, None, None),
            QualityTier::Lq
        );
    }
}
