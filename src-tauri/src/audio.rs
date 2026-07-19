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
//! → volume → output`. Each gain stage is a single precomputed multiplier
//! read from an atomic — nothing in the output callback allocates or blocks.
//! When every stage is neutral (EQ off, gains at 1.0) samples pass through
//! bit-perfect.

use crate::models::{PlayState, Song};
use anyhow::{anyhow, Result};
use ringbuf::{
    traits::{Consumer, Observer, Producer, Split},
    HeapRb,
};
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};
use symphonia::core::{
    audio::SampleBuffer, codecs::Decoder, codecs::DecoderOptions, errors::Error as SymphoniaError,
    formats::FormatOptions, formats::FormatReader, io::MediaSource, io::MediaSourceStream,
    meta::MetadataOptions, probe::Hint,
};

/// How far before the end boundary the `AboutToFinish` signal fires.
/// Consumers: gapless preload (here), auto-crossfade (#79), CUE
/// sibling-track continuation (#78).
pub const PRELOAD_LEAD_NS: u64 = 8_000_000_000;

// ---------------------------------------------------------------------------
// Control messages sent to the decode thread
// ---------------------------------------------------------------------------

pub enum AudioCommand {
    Play(PlayRequest),
    Cue(PlayRequest),
    Pause,
    Resume,
    Stop,
    SeekTo(u64),    // target position in nanoseconds
    SetVolume(f32), // 0.0–1.0
    /// Prime the next track for a gapless transition after the current one.
    PreloadNext(PlayRequest),
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
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// AudioEngine — public handle
// ---------------------------------------------------------------------------

pub struct AudioEngine {
    cmd_tx: mpsc::SyncSender<AudioCommand>,
    pub event_rx: Arc<Mutex<mpsc::Receiver<AudioEvent>>>,
    pub position_nanosec: Arc<AtomicU64>,
    pub volume: Arc<Mutex<f32>>,
    pub play_state: Arc<Mutex<PlayState>>,
    pub visualizer_buf: Arc<crate::analyzer::AudioVisualizerBuffer>,
    pub spectrum_enabled: Arc<std::sync::atomic::AtomicBool>,
    pub equalizer: Arc<Mutex<crate::equalizer::Equalizer>>,
    /// Per-track loudness-normalization multiplier (#77). f32 bits in an
    /// atomic so the audio callback reads it without locking. 1.0 = neutral.
    pub loudness_gain: Arc<AtomicU32>,
    /// Fade-envelope multiplier slot (#79). 1.0 = neutral.
    pub fade_gain: Arc<AtomicU32>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::sync_channel::<AudioCommand>(64);
        let (event_tx, event_rx) = mpsc::channel::<AudioEvent>();
        let position = Arc::new(AtomicU64::new(0));
        let volume = Arc::new(Mutex::new(1.0f32));
        let play_state = Arc::new(Mutex::new(PlayState::Stopped));
        let visualizer_buf = Arc::new(crate::analyzer::AudioVisualizerBuffer::new(4096));
        let spectrum_enabled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let equalizer = Arc::new(Mutex::new(crate::equalizer::Equalizer::new()));
        let loudness_gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let fade_gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));

        let pos_clone = Arc::clone(&position);
        let vol_clone = Arc::clone(&volume);
        let state_clone = Arc::clone(&play_state);
        let vis_clone = Arc::clone(&visualizer_buf);
        let eq_clone = Arc::clone(&equalizer);
        let loudness_clone = Arc::clone(&loudness_gain);
        let fade_clone = Arc::clone(&fade_gain);

        // Spawn a plain OS thread — no Send requirement on cpal::Stream
        std::thread::Builder::new()
            .name("luminous-audio".to_string())
            .spawn(move || {
                decode_thread(
                    cmd_rx,
                    event_tx,
                    pos_clone,
                    vol_clone,
                    state_clone,
                    vis_clone,
                    eq_clone,
                    loudness_clone,
                    fade_clone,
                );
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
            equalizer,
            loudness_gain,
            fade_gain,
        }
    }

    pub fn play(&self, song: Box<Song>, start_nanosec: u64) -> Result<()> {
        self.cmd_tx
            .send(AudioCommand::Play(PlayRequest {
                song,
                start_nanosec,
            }))
            .map_err(|_| anyhow!("audio thread shut down"))
    }

    pub fn cue(&self, song: Box<Song>, start_nanosec: u64) -> Result<()> {
        if let Ok(mut s) = self.play_state.lock() {
            *s = crate::models::PlayState::Paused;
        }
        self.position_nanosec
            .store(start_nanosec, Ordering::Relaxed);
        self.cmd_tx
            .send(AudioCommand::Cue(PlayRequest {
                song,
                start_nanosec,
            }))
            .map_err(|_| anyhow!("audio thread shut down"))
    }

    pub fn preload_next(&self, song: Box<Song>, start_nanosec: u64) -> Result<()> {
        self.cmd_tx
            .send(AudioCommand::PreloadNext(PlayRequest {
                song,
                start_nanosec,
            }))
            .map_err(|_| anyhow!("audio thread shut down"))
    }

    pub fn clear_preload(&self) -> Result<()> {
        self.cmd_tx
            .send(AudioCommand::ClearPreload)
            .map_err(|_| anyhow!("audio thread shut down"))
    }

    pub fn pause(&self) -> Result<()> {
        self.cmd_tx
            .send(AudioCommand::Pause)
            .map_err(|_| anyhow!("audio thread shut down"))
    }

    pub fn resume(&self) -> Result<()> {
        self.cmd_tx
            .send(AudioCommand::Resume)
            .map_err(|_| anyhow!("audio thread shut down"))
    }

    pub fn stop(&self) -> Result<()> {
        self.cmd_tx
            .send(AudioCommand::Stop)
            .map_err(|_| anyhow!("audio thread shut down"))
    }

    pub fn seek_to(&self, position_nanosec: u64) -> Result<()> {
        self.cmd_tx
            .send(AudioCommand::SeekTo(position_nanosec))
            .map_err(|_| anyhow!("audio thread shut down"))
    }

    pub fn set_volume(&self, vol: f32) -> Result<()> {
        let vol = vol.clamp(0.0, 1.0);
        if let Ok(mut v) = self.volume.lock() {
            *v = vol;
        }
        self.cmd_tx
            .send(AudioCommand::SetVolume(vol))
            .map_err(|_| anyhow!("audio thread shut down"))
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
        self.volume.lock().map(|v| *v).unwrap_or(1.0)
    }

    pub fn current_state(&self) -> PlayState {
        self.play_state
            .lock()
            .map(|s| *s)
            .unwrap_or(PlayState::Stopped)
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Media source & track opening (source-agnostic seam for #82 streaming)
// ---------------------------------------------------------------------------

/// Open a playable media source. Local files today; internet-radio HTTP
/// streams (#82) plug in here by returning a different `MediaSource` impl.
fn open_media_source(path: &str) -> Result<Box<dyn MediaSource>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Cannot open file '{path}': {e}"))?;
    Ok(Box::new(file))
}

/// A fully opened, probed, decode-ready track.
struct ActiveTrack {
    song: Box<Song>,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
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
            .ok_or_else(|| "Song has no local path".to_string())?
            .to_owned();

        let source = open_media_source(&path)?;
        let mss = MediaSourceStream::new(source, Default::default());
        let probed = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| format!("Format probe failed: {e}"))?;

        let mut format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
            .cloned()
            .ok_or_else(|| "No audio track found".to_string())?;

        let track_id = track.id;
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| format!("Decoder init failed: {e}"))?;

        if start_nanosec > 0 {
            let target_time =
                symphonia::core::units::Time::from(std::time::Duration::from_nanos(start_nanosec));
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

        let src_rate = track.codec_params.sample_rate.unwrap_or(44100);
        let src_channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

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
}

#[allow(clippy::too_many_arguments)]
fn build_output(
    event_tx: &mpsc::Sender<AudioEvent>,
    position: &Arc<AtomicU64>,
    volume: &Arc<Mutex<f32>>,
    visualizer_buf: &Arc<crate::analyzer::AudioVisualizerBuffer>,
    equalizer: &Arc<Mutex<crate::equalizer::Equalizer>>,
    loudness_gain: &Arc<AtomicU32>,
    fade_gain: &Arc<AtomicU32>,
) -> Result<AudioOutput, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "No audio output device".to_string())?;
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

    let target_sample_rate = config.sample_rate.0;
    let target_channels = config.channels;

    // Update equalizer sample rate and channels format using target device values
    if let Ok(mut eq) = equalizer.lock() {
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
    let vol_ref = Arc::clone(volume);
    let position_cpal = Arc::clone(position);
    let visualizer_buf_cpal = Arc::clone(visualizer_buf);
    let eq_cpal = Arc::clone(equalizer);
    let loudness_cpal = Arc::clone(loudness_gain);
    let fade_cpal = Arc::clone(fade_gain);

    // Pre-allocated scratch for the visualizer's mono downmix — the output
    // callback must never allocate. Sized for the whole ring buffer, far
    // larger than any single callback burst.
    let mut mono_scratch: Vec<f32> = Vec::with_capacity(buffer_capacity);

    let stream = device
        .build_output_stream(
            &config,
            move |output: &mut [f32], _| {
                let vol = vol_ref.lock().map(|v| *v).unwrap_or(1.0);
                let loudness = f32::from_bits(loudness_cpal.load(Ordering::Relaxed));
                let fade = f32::from_bits(fade_cpal.load(Ordering::Relaxed));
                let mut played = 0;

                // Non-blocking try_lock ensures CPAL callback never stalls
                if let Ok(mut consumer) = shared_consumer_reader.try_lock() {
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
                if let Ok(mut eq) = eq_cpal.try_lock() {
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
            |err| log::error!("CPAL stream error: {err}"),
            None,
        )
        .map_err(|e| format!("CPAL stream build failed: {e}"))?;

    // Start paused — nothing decoded yet. The caller starts it once a track
    // is actually ready to play.
    if let Err(e) = stream.pause() {
        let _ = event_tx.send(AudioEvent::Error {
            message: format!("CPAL stream pause failed: {e}"),
        });
    }

    Ok(AudioOutput {
        stream,
        producer: prod,
        consumer: shared_consumer,
        played_samples,
        sample_rate: target_sample_rate,
        channels: target_channels,
    })
}

#[allow(clippy::too_many_arguments)]
fn decode_thread(
    cmd_rx: mpsc::Receiver<AudioCommand>,
    event_tx: mpsc::Sender<AudioEvent>,
    position: Arc<AtomicU64>,
    volume: Arc<Mutex<f32>>,
    play_state: Arc<Mutex<PlayState>>,
    visualizer_buf: Arc<crate::analyzer::AudioVisualizerBuffer>,
    equalizer: Arc<Mutex<crate::equalizer::Equalizer>>,
    loudness_gain: Arc<AtomicU32>,
    fade_gain: Arc<AtomicU32>,
) {
    use cpal::traits::StreamTrait;

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
                            match build_output(
                                &event_tx,
                                &position,
                                &volume,
                                &visualizer_buf,
                                &equalizer,
                                &loudness_gain,
                                &fade_gain,
                            ) {
                                Ok(o) => output = Some(o),
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
                                if let Ok(mut consumer) = out.consumer.lock() {
                                    while consumer.try_pop().is_some() {}
                                }
                                let start_samples = samples_for_ns(
                                    r.start_nanosec,
                                    target_sample_rate,
                                    target_channels,
                                );
                                out.played_samples.store(start_samples, Ordering::Relaxed);
                                let _ = out.stream.pause();
                                if let Ok(mut s) = play_state.lock() {
                                    *s = PlayState::Paused;
                                }
                                position.store(r.start_nanosec, Ordering::Relaxed);
                                paused_req = Some(r);
                                let _ = event_tx.send(AudioEvent::Paused);
                            }
                            Err(message) => {
                                let _ = event_tx.send(AudioEvent::Error { message });
                            }
                        }
                        continue;
                    }
                    Ok(AudioCommand::Resume) => {
                        if let Some(r) = paused_req.take() {
                            let cur_pos = position.load(Ordering::Relaxed);
                            PlayRequest {
                                song: r.song,
                                start_nanosec: cur_pos,
                            }
                        } else {
                            continue;
                        }
                    }
                    Ok(AudioCommand::Stop) => {
                        if let Some(out) = output.as_ref() {
                            let _ = out.stream.pause();
                        }
                        paused_req = None;
                        position.store(0, Ordering::Relaxed);
                        if let Ok(mut s) = play_state.lock() {
                            *s = PlayState::Stopped;
                        }
                        let _ = event_tx.send(AudioEvent::Stopped);
                        continue;
                    }
                    Ok(AudioCommand::SetVolume(v)) => {
                        if let Ok(mut vol) = volume.lock() {
                            *vol = v.clamp(0.0, 1.0);
                        }
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
                Ok(AudioCommand::SetVolume(v)) => {
                    if let Ok(mut vol) = volume.lock() {
                        *vol = v.clamp(0.0, 1.0);
                    }
                }
                Ok(AudioCommand::Stop) => {
                    if let Some(out) = output.as_ref() {
                        let _ = out.stream.pause();
                    }
                    paused_req = None;
                    position.store(0, Ordering::Relaxed);
                    if let Ok(mut s) = play_state.lock() {
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
        if output.is_none() {
            match build_output(
                &event_tx,
                &position,
                &volume,
                &visualizer_buf,
                &equalizer,
                &loudness_gain,
                &fade_gain,
            ) {
                Ok(o) => output = Some(o),
                Err(message) => {
                    let _ = event_tx.send(AudioEvent::Error { message });
                    continue;
                }
            }
        }
        let out = output.as_mut().unwrap();
        let target_sample_rate = out.sample_rate;
        let target_channels = out.channels;

        let mut current = match ActiveTrack::open(
            req.song,
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
        let song_id = current.song.id;

        // Clear whatever was left in the buffer from the previous track and
        // reset the played-sample counter for this track's start offset.
        if let Ok(mut consumer) = out.consumer.lock() {
            while consumer.try_pop().is_some() {}
        }
        let start_samples = samples_for_ns(current.start_ns, target_sample_rate, target_channels);
        out.played_samples.store(start_samples, Ordering::Relaxed);
        // Absolute count of samples pushed to the ring buffer, in the same
        // "space" as `played_samples` (kept in sync at every buffer clear).
        let mut pushed_samples: u64 = start_samples;

        if let Err(e) = out.stream.play() {
            let _ = event_tx.send(AudioEvent::Error {
                message: format!("CPAL stream play failed: {e}"),
            });
            continue;
        }

        if let Ok(mut s) = play_state.lock() {
            *s = PlayState::Playing;
        }
        position.store(current.start_ns, Ordering::Relaxed);
        let _ = event_tx.send(AudioEvent::Playing { song_id });

        // Gapless state: a preloaded next track, and (once the current file
        // is exhausted) the pending handover to it.
        let mut next: Option<ActiveTrack> = None;
        let mut transition: Option<PendingTransition> = None;

        // Decode loop
        'decode: loop {
            let out = output.as_mut().unwrap();

            // Non-blocking command check
            match cmd_rx.try_recv() {
                Ok(AudioCommand::Pause) => {
                    let _ = out.stream.pause();
                    if let Ok(mut s) = play_state.lock() {
                        *s = PlayState::Paused;
                    }
                    let _ = event_tx.send(AudioEvent::Paused);
                    // Position still belongs to the finished track while a
                    // transition is draining — resume must reopen that song.
                    let song_for_resume = match transition.as_ref() {
                        Some(t) => t.finished_song.clone(),
                        None => current.song.clone(),
                    };
                    paused_req = Some(PlayRequest {
                        song: song_for_resume,
                        start_nanosec: position.load(Ordering::Relaxed),
                    });
                    break 'decode;
                }
                Ok(AudioCommand::Stop) => {
                    let _ = out.stream.pause();
                    if let Ok(mut s) = play_state.lock() {
                        *s = PlayState::Stopped;
                    }
                    let _ = event_tx.send(AudioEvent::Stopped);
                    break 'decode;
                }
                Ok(AudioCommand::Play(new_req)) => {
                    current_req = Some(new_req);
                    let _ = event_tx.send(AudioEvent::Stopped);
                    break 'decode;
                }
                Ok(AudioCommand::SeekTo(target_ns)) => {
                    log::debug!("SeekTo command received. target_ns: {target_ns}");

                    if let Some(t) = transition.take() {
                        // Mid-handover seek: the audible position is still in
                        // the finished track but its decoder is gone — reopen
                        // it fresh at the target. The preload is dropped so
                        // AboutToFinish re-arms naturally on the new track.
                        next = None;
                        match ActiveTrack::open(
                            t.finished_song,
                            target_ns,
                            target_sample_rate,
                            target_channels as usize,
                        ) {
                            Ok(t) => current = t,
                            Err(message) => {
                                let _ = event_tx.send(AudioEvent::Error { message });
                                break 'decode;
                            }
                        }
                    } else {
                        let target_time = symphonia::core::units::Time::from(
                            std::time::Duration::from_nanos(target_ns),
                        );
                        let seek_res = current.format.seek(
                            symphonia::core::formats::SeekMode::Accurate,
                            symphonia::core::formats::SeekTo::Time {
                                time: target_time,
                                track_id: Some(current.track_id),
                            },
                        );
                        match seek_res {
                            Ok(seeked_to) => {
                                current.decoder.reset();
                                log::info!("Seek successful: {seeked_to:?}");
                            }
                            Err(e) => {
                                log::error!("Seek failed: {e:?}");
                            }
                        }
                        current.decoded_pos_ns = target_ns;
                        current.eof = false;
                        current.resampler = Resampler::new(
                            current.src_rate,
                            target_sample_rate,
                            target_channels as usize,
                        );
                    }

                    // Clear the buffer after seek to avoid stale audio
                    if let Ok(mut consumer) = out.consumer.lock() {
                        while consumer.try_pop().is_some() {}
                    }
                    let target_samples =
                        samples_for_ns(target_ns, target_sample_rate, target_channels);
                    out.played_samples.store(target_samples, Ordering::Relaxed);
                    pushed_samples = target_samples;
                    position.store(target_ns, Ordering::Relaxed);
                }
                Ok(AudioCommand::SetVolume(v)) => {
                    if let Ok(mut vol) = volume.lock() {
                        *vol = v.clamp(0.0, 1.0);
                    }
                }
                Ok(AudioCommand::PreloadNext(preq)) => {
                    match ActiveTrack::open(
                        preq.song,
                        preq.start_nanosec,
                        target_sample_rate,
                        target_channels as usize,
                    ) {
                        Ok(t) => {
                            log::debug!("Preloaded next track {} for gapless", t.song.id);
                            next = Some(t);
                        }
                        Err(e) => {
                            // Fall back to the drain + TrackFinished path;
                            // the player will issue a normal Play.
                            log::warn!("Gapless preload failed: {e}");
                            next = None;
                        }
                    }
                }
                Ok(AudioCommand::ClearPreload) => {
                    if transition.is_none() {
                        next = None;
                        current.about_to_finish_sent = false;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {} // no pending command
                Err(mpsc::TryRecvError::Disconnected) => break 'decode,
                Ok(AudioCommand::Resume) => {} // already playing
                Ok(AudioCommand::Cue(_)) => {} // already playing
            }

            // Complete a pending gapless handover once the output callback
            // has actually consumed the finished track's last sample.
            if let Some(t) = transition.as_ref() {
                let played = out.played_samples.load(Ordering::Relaxed);
                if played >= t.boundary_samples {
                    let next_start_samples =
                        samples_for_ns(current.start_ns, target_sample_rate, target_channels);
                    // Rebase the sample counter onto the new track's
                    // timeline. fetch_sub composes safely with the callback's
                    // concurrent fetch_add.
                    if t.boundary_samples >= next_start_samples {
                        let delta = t.boundary_samples - next_start_samples;
                        out.played_samples.fetch_sub(delta, Ordering::Relaxed);
                        pushed_samples -= delta;
                    } else {
                        let delta = next_start_samples - t.boundary_samples;
                        out.played_samples.fetch_add(delta, Ordering::Relaxed);
                        pushed_samples += delta;
                    }
                    position.store(current.start_ns, Ordering::Relaxed);
                    let finished_song_id = t.finished_song.id;
                    let _ = event_tx.send(AudioEvent::TrackTransitioned {
                        finished_song_id,
                        song_id: current.song.id,
                    });
                    transition = None;
                }
            }

            // "About to finish" signal: fires once per track when the
            // audible position enters the preload window before the end
            // boundary. Suppressed while a handover is draining (the
            // position still belongs to the previous track then). Checked
            // before the EOF branch so short, already-fully-decoded tracks
            // still request their preload.
            if !current.about_to_finish_sent && transition.is_none() {
                if let Some(about_end) = current.about_end_ns {
                    let pos = position.load(Ordering::Relaxed);
                    if pos + PRELOAD_LEAD_NS >= about_end {
                        current.about_to_finish_sent = true;
                        let _ = event_tx.send(AudioEvent::AboutToFinish {
                            song_id: current.song.id,
                        });
                    }
                }
            }

            // If the decoder exhausted the current file:
            if current.eof {
                if transition.is_some() {
                    // Waiting for the boundary to be consumed before the
                    // next (already fully decoded) track can take over.
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue 'decode;
                }
                if let Some(n) = next.take() {
                    // Gapless handover: continue decoding the preloaded
                    // track into the same ring buffer — no drain, no pause.
                    transition = Some(PendingTransition {
                        boundary_samples: pushed_samples,
                        finished_song: std::mem::replace(&mut current, n).song,
                    });
                    continue 'decode;
                }

                // No preloaded next — classic drain-then-finish path.
                let is_empty = out.producer.occupied_len() == 0;
                if is_empty {
                    let _ = event_tx.send(AudioEvent::TrackFinished {
                        song_id: current.song.id,
                    });
                    let _ = out.stream.pause();
                    if let Ok(mut s) = play_state.lock() {
                        *s = PlayState::Stopped;
                    }
                    break 'decode;
                } else {
                    // Buffer still has remaining audio, wait for it to be played
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    continue 'decode;
                }
            }

            // Rate limit: if the buffer is full (more than 1.5 seconds of audio), sleep
            let is_full = out.producer.occupied_len()
                > (target_sample_rate as usize * target_channels as usize * 3 / 2);

            if is_full {
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue 'decode;
            }

            // Decode one packet
            match current.format.next_packet() {
                Ok(packet) => {
                    if packet.track_id() != current.track_id {
                        continue;
                    }
                    match current.decoder.decode(&packet) {
                        Ok(decoded) => {
                            let spec = *decoded.spec();
                            let mut sample_buf =
                                SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                            sample_buf.copy_interleaved_ref(decoded);
                            let mut samples = sample_buf.samples();

                            // Enforce the CUE end boundary (`end_nanosec`):
                            // truncate the packet at the cut and treat the
                            // remainder of the file as EOF.
                            let frames = samples.len() / current.src_channels;
                            let packet_ns =
                                (frames as f64 * 1_000_000_000.0 / current.src_rate as f64) as u64;
                            if let Some(end_ns) = current.end_ns {
                                if current.decoded_pos_ns >= end_ns {
                                    current.eof = true;
                                    continue 'decode;
                                }
                                if current.decoded_pos_ns + packet_ns > end_ns {
                                    let keep_frames = ((end_ns - current.decoded_pos_ns) as f64
                                        * current.src_rate as f64
                                        / 1_000_000_000.0)
                                        as usize;
                                    samples = &samples[..keep_frames * current.src_channels];
                                    current.eof = true;
                                }
                            }
                            current.decoded_pos_ns += packet_ns;

                            let channel_converted = convert_channels(
                                samples,
                                current.src_channels,
                                target_channels as usize,
                            );
                            let resampled = current.resampler.resample(&channel_converted);

                            // Push samples into the shared playback buffer
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
                            pushed_samples += resampled.len() as u64;
                        }
                        Err(SymphoniaError::DecodeError(_)) => continue,
                        Err(_) => break 'decode,
                    }
                }
                Err(SymphoniaError::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    current.eof = true;
                    continue 'decode;
                }
                Err(e) => {
                    let _ = event_tx.send(AudioEvent::Error {
                        message: format!("Decode error: {e}"),
                    });
                    break 'decode;
                }
            }
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
            (m, n) => {
                for i in 0..n {
                    if i < m {
                        output.push(frame[i]);
                    } else {
                        output.push(0.0);
                    }
                }
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
}
