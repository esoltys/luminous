//! Audio engine — Symphonia decoder + CPAL output pipeline.
//!
//! The decode loop runs in a plain `std::thread` (not tokio) because
//! `cpal::Stream` is `!Send` and cannot be held across `.await` points.
//! Communication uses `std::sync::mpsc` channels.

use crate::models::{PlayState, Song};
use anyhow::{anyhow, Result};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};

// ---------------------------------------------------------------------------
// Control messages sent to the decode thread
// ---------------------------------------------------------------------------

pub enum AudioCommand {
    Play(PlayRequest),
    Pause,
    Resume,
    Stop,
    SeekTo(u64),    // target position in nanoseconds
    SetVolume(f32), // 0.0–1.0
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
    Playing { song_id: i64 },
    Paused,
    Stopped,
    PositionChanged { position_nanosec: u64 },
    TrackFinished { song_id: i64 },
    Error { message: String },
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

        let pos_clone = Arc::clone(&position);
        let vol_clone = Arc::clone(&volume);
        let state_clone = Arc::clone(&play_state);
        let vis_clone = Arc::clone(&visualizer_buf);
        let eq_clone = Arc::clone(&equalizer);

        // Spawn a plain OS thread — no Send requirement on cpal::Stream
        std::thread::Builder::new()
            .name("luminous-audio".to_string())
            .spawn(move || {
                decode_thread(cmd_rx, event_tx, pos_clone, vol_clone, state_clone, vis_clone, eq_clone);
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
        }
    }

    pub fn play(&self, song: Box<Song>, start_nanosec: u64) -> Result<()> {
        self.cmd_tx
            .send(AudioCommand::Play(PlayRequest { song, start_nanosec }))
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

    pub fn current_position_nanosec(&self) -> u64 {
        self.position_nanosec.load(Ordering::Relaxed)
    }

    pub fn current_volume(&self) -> f32 {
        self.volume.lock().map(|v| *v).unwrap_or(1.0)
    }

    pub fn current_state(&self) -> PlayState {
        self.play_state.lock().map(|s| *s).unwrap_or(PlayState::Stopped)
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Decode thread — runs on a plain OS thread to avoid Send constraints
// ---------------------------------------------------------------------------

fn decode_thread(
    cmd_rx: mpsc::Receiver<AudioCommand>,
    event_tx: mpsc::Sender<AudioEvent>,
    position: Arc<AtomicU64>,
    volume: Arc<Mutex<f32>>,
    play_state: Arc<Mutex<PlayState>>,
    visualizer_buf: Arc<crate::analyzer::AudioVisualizerBuffer>,
    equalizer: Arc<Mutex<crate::equalizer::Equalizer>>,
) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use symphonia::core::{
        audio::SampleBuffer,
        codecs::DecoderOptions,
        errors::Error as SymphoniaError,
        formats::FormatOptions,
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    };

    // Keep current CPAL stream alive. Dropping it stops playback.
    let mut _stream: Option<cpal::Stream> = None;

    let mut current_req = None;
    let mut paused_req: Option<PlayRequest> = None;

    loop {
        let req = match current_req.take() {
            Some(r) => r,
            None => {
                match cmd_rx.recv() {
                    Ok(AudioCommand::Play(r)) => {
                        paused_req = None;
                        r
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
                        _stream = None;
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
                    Err(_) => break, // Channel disconnected
                }
            }
        };

        // Drop any existing stream
        _stream = None;

                let path = match req.song.path.as_deref() {
                    Some(p) => p.to_owned(),
                    None => {
                        let _ = event_tx.send(AudioEvent::Error {
                            message: "Song has no local path".to_string(),
                        });
                        continue;
                    }
                };

                let song_id = req.song.id;

                // Open + probe the file
                let file = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = event_tx.send(AudioEvent::Error {
                            message: format!("Cannot open file '{path}': {e}"),
                        });
                        continue;
                    }
                };

                let mss = MediaSourceStream::new(Box::new(file), Default::default());
                let probed = match symphonia::default::get_probe().format(
                    &Hint::new(),
                    mss,
                    &FormatOptions::default(),
                    &MetadataOptions::default(),
                ) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = event_tx.send(AudioEvent::Error {
                            message: format!("Format probe failed: {e}"),
                        });
                        continue;
                    }
                };

                let mut format = probed.format;
                let track = match format
                    .tracks()
                    .iter()
                    .find(|t| {
                        t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL
                    })
                    .cloned()
                {
                    Some(t) => t,
                    None => {
                        let _ = event_tx.send(AudioEvent::Error {
                            message: "No audio track found".to_string(),
                        });
                        continue;
                    }
                };

                let track_id = track.id;
                let mut decoder = match symphonia::default::get_codecs()
                    .make(&track.codec_params, &DecoderOptions::default())
                {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = event_tx.send(AudioEvent::Error {
                            message: format!("Decoder init failed: {e}"),
                        });
                        continue;
                    }
                };

                // Set up CPAL
                let host = cpal::default_host();
                let device = match host.default_output_device() {
                    Some(d) => d,
                    None => {
                        let _ = event_tx.send(AudioEvent::Error {
                            message: "No audio output device".to_string(),
                        });
                        continue;
                    }
                };

                let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
                let channels = track
                    .codec_params
                    .channels
                    .map(|c| c.count() as u16)
                    .unwrap_or(2);

                let config = cpal::StreamConfig {
                    channels,
                    sample_rate: cpal::SampleRate(sample_rate),
                    buffer_size: cpal::BufferSize::Default,
                };

                let vol_ref = Arc::clone(&volume);

                // Update equalizer sample rate and channels format
                if let Ok(mut eq) = equalizer.lock() {
                    eq.update_format(sample_rate, channels as usize);
                }

                // Simpler approach: use a shared VecDeque protected by Mutex
                let shared_buf: Arc<Mutex<std::collections::VecDeque<f32>>> =
                    Arc::new(Mutex::new(std::collections::VecDeque::with_capacity(
                        sample_rate as usize * channels as usize * 2,
                    )));
                let shared_buf_writer = Arc::clone(&shared_buf);
                let shared_buf_reader = Arc::clone(&shared_buf);

                let played_samples = Arc::new(AtomicU64::new(
                    (req.start_nanosec as f64 * sample_rate as f64 * channels as f64 / 1_000_000_000.0) as u64
                ));
                let played_samples_cpal = Arc::clone(&played_samples);
                let position_cpal = Arc::clone(&position);
                let visualizer_buf_cpal = Arc::clone(&visualizer_buf);
                let eq_cpal = Arc::clone(&equalizer);

                let stream = match device.build_output_stream(
                    &config,
                    move |output: &mut [f32], _| {
                        let vol = vol_ref.lock().map(|v| *v).unwrap_or(1.0);
                        let mut buf = shared_buf_reader.lock().unwrap();
                        let mut played = 0;
                        for sample in output.iter_mut() {
                            if let Some(s) = buf.pop_front() {
                                *sample = s;
                                played += 1;
                            } else {
                                *sample = 0.0;
                            }
                        }

                        // Apply equalizer DSP
                        if let Ok(mut eq) = eq_cpal.try_lock() {
                            eq.process_interleaved(&mut output[..played]);
                        }

                        // Apply volume gain
                        for sample in output[..played].iter_mut() {
                            *sample *= vol;
                        }

                        if played > 0 {
                            let channels_u = channels as usize;
                            let mut mono_samples = Vec::with_capacity(played / channels_u);
                            for chunk in output[..played].chunks(channels_u) {
                                let sum: f32 = chunk.iter().sum();
                                mono_samples.push(sum / channels as f32);
                            }
                            visualizer_buf_cpal.push(&mono_samples);
                        }

                        let total_played = played_samples_cpal.fetch_add(played as u64, Ordering::Relaxed) + played as u64;
                        let pos_ns = (total_played as f64 * 1_000_000_000.0 / (sample_rate as f64 * channels as f64)) as u64;
                        position_cpal.store(pos_ns, Ordering::Relaxed);
                    },
                    |err| log::error!("CPAL stream error: {err}"),
                    None,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = event_tx.send(AudioEvent::Error {
                            message: format!("CPAL stream build failed: {e}"),
                        });
                        continue;
                    }
                };

                if let Err(e) = stream.play() {
                    let _ = event_tx.send(AudioEvent::Error {
                        message: format!("CPAL stream play failed: {e}"),
                    });
                    continue;
                }

                _stream = Some(stream);

                if let Ok(mut s) = play_state.lock() {
                    *s = PlayState::Playing;
                }
                position.store(req.start_nanosec, Ordering::Relaxed);
                let _ = event_tx.send(AudioEvent::Playing { song_id });

                let mut eof_reached = false;

                // Decode loop
                'decode: loop {
                    // Non-blocking command check
                    match cmd_rx.try_recv() {
                        Ok(AudioCommand::Pause) => {
                            _stream = None;
                            if let Ok(mut s) = play_state.lock() {
                                *s = PlayState::Paused;
                            }
                            let _ = event_tx.send(AudioEvent::Paused);
                            paused_req = Some(PlayRequest {
                                song: req.song,
                                start_nanosec: position.load(Ordering::Relaxed),
                            });
                            break 'decode;
                        }
                        Ok(AudioCommand::Stop) => {
                            _stream = None;
                            if let Ok(mut s) = play_state.lock() {
                                *s = PlayState::Stopped;
                            }
                            let _ = event_tx.send(AudioEvent::Stopped);
                            break 'decode;
                        }
                        Ok(AudioCommand::Play(new_req)) => {
                            current_req = Some(new_req);
                            _stream = None;
                            let _ = event_tx.send(AudioEvent::Stopped);
                            break 'decode;
                        }
                        Ok(AudioCommand::SeekTo(target_ns)) => {
                            let target_time = symphonia::core::units::Time::from(
                                std::time::Duration::from_nanos(target_ns),
                            );
                            
                            // Use SeekTo::Time instead of TimeStamp for robust format reader handling
                            let seek_res = format.seek(
                                symphonia::core::formats::SeekMode::Accurate,
                                symphonia::core::formats::SeekTo::Time {
                                    time: target_time,
                                    track_id: Some(track_id),
                                },
                            );

                            match seek_res {
                                Ok(seeked_to) => {
                                    decoder.reset();
                                    log::info!("Seek successful: {:?}", seeked_to);
                                }
                                Err(e) => {
                                    log::error!("Seek failed: {:?}", e);
                                }
                            }

                            // Clear the buffer after seek to avoid stale audio
                            if let Ok(mut buf) = shared_buf_writer.lock() {
                                buf.clear();
                            }
                            let target_samples = (target_ns as f64 * sample_rate as f64 * channels as f64 / 1_000_000_000.0) as u64;
                            played_samples.store(target_samples, Ordering::Relaxed);
                            position.store(target_ns, Ordering::Relaxed);
                            eof_reached = false; // Reset EOF so we resume decoding
                        }
                        Ok(AudioCommand::SetVolume(v)) => {
                            if let Ok(mut vol) = volume.lock() {
                                *vol = v.clamp(0.0, 1.0);
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => {} // no pending command
                        Err(mpsc::TryRecvError::Disconnected) => break 'decode,
                        Ok(AudioCommand::Resume) => {} // already playing
                    }

                    // If decoder has reached the end of the file, wait until output buffer drains completely
                    if eof_reached {
                        let is_empty = if let Ok(buf) = shared_buf_writer.lock() {
                            buf.is_empty()
                        } else {
                            true
                        };

                        if is_empty {
                            let _ = event_tx.send(AudioEvent::TrackFinished { song_id });
                            _stream = None;
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
                    let is_full = if let Ok(buf) = shared_buf_writer.lock() {
                        buf.len() > (sample_rate as usize * channels as usize * 3 / 2)
                    } else {
                        false
                    };

                    if is_full {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        continue 'decode;
                    }

                    // Decode one packet
                    match format.next_packet() {
                        Ok(packet) => {
                            if packet.track_id() != track_id {
                                continue;
                            }
                            match decoder.decode(&packet) {
                                Ok(decoded) => {
                                    let spec = *decoded.spec();
                                    let mut sample_buf = SampleBuffer::<f32>::new(
                                        decoded.capacity() as u64,
                                        spec,
                                    );
                                    sample_buf.copy_interleaved_ref(decoded);

                                    // Push samples into the shared playback buffer
                                    if let Ok(mut buf) = shared_buf_writer.lock() {
                                        for &s in sample_buf.samples() {
                                            buf.push_back(s);
                                        }
                                    }
                                }
                                Err(SymphoniaError::DecodeError(_)) => continue,
                                Err(_) => break 'decode,
                            }
                        }
                        Err(SymphoniaError::IoError(ref e))
                            if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                        {
                            eof_reached = true;
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
