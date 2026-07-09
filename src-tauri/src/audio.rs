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
use ringbuf::{
    traits::{Consumer, Observer, Producer, Split},
    HeapRb,
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

        if req.start_nanosec > 0 {
            let target_time = symphonia::core::units::Time::from(
                std::time::Duration::from_nanos(req.start_nanosec),
            );
            match format.seek(
                symphonia::core::formats::SeekMode::Accurate,
                symphonia::core::formats::SeekTo::Time {
                    time: target_time,
                    track_id: Some(track_id),
                },
            ) {
                Ok(_) => decoder.reset(),
                Err(e) => log::warn!("Initial seek to {}ns failed: {:?}", req.start_nanosec, e),
            }
        }

        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track
            .codec_params
            .channels
            .map(|c| c.count() as u16)
            .unwrap_or(2);

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

        let default_config = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                let _ = event_tx.send(AudioEvent::Error {
                    message: format!("Failed to get default output config: {e}"),
                });
                continue;
            }
        };
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

        let vol_ref = Arc::clone(&volume);

        // Update equalizer sample rate and channels format using target device values
        if let Ok(mut eq) = equalizer.lock() {
            eq.update_format(target_sample_rate, target_channels as usize);
        }

        // Buffer capacity based on target device format (approx. 2 seconds of audio)
        let buffer_capacity = target_sample_rate as usize * target_channels as usize * 2;
        let rb = HeapRb::<f32>::new(buffer_capacity);
        let (mut prod, cons) = rb.split();

        // Wrap consumer in a Mutex so that the decode thread can clear it upon Seek/Stop,
        // while the audio callback can perform a non-blocking `try_lock()` on it.
        let shared_consumer = Arc::new(Mutex::new(cons));
        let shared_consumer_reader = Arc::clone(&shared_consumer);

        let played_samples = Arc::new(AtomicU64::new(
            (req.start_nanosec as f64 * target_sample_rate as f64 * target_channels as f64 / 1_000_000_000.0) as u64
        ));
        let played_samples_cpal = Arc::clone(&played_samples);
        let position_cpal = Arc::clone(&position);
        let visualizer_buf_cpal = Arc::clone(&visualizer_buf);
        let eq_cpal = Arc::clone(&equalizer);

        let stream = match device.build_output_stream(
            &config,
            move |output: &mut [f32], _| {
                let vol = vol_ref.lock().map(|v| *v).unwrap_or(1.0);
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

                // Apply equalizer DSP
                if let Ok(mut eq) = eq_cpal.try_lock() {
                    eq.process_interleaved(&mut output[..played]);
                }

                // Apply volume gain
                for sample in output[..played].iter_mut() {
                    *sample *= vol;
                }

                if played > 0 {
                    let channels_u = target_channels as usize;
                    let mut mono_samples = Vec::with_capacity(played / channels_u);
                    for chunk in output[..played].chunks(channels_u) {
                        let sum: f32 = chunk.iter().sum();
                        mono_samples.push(sum / target_channels as f32);
                    }
                    visualizer_buf_cpal.push(&mono_samples);
                }

                let total_played = played_samples_cpal.fetch_add(played as u64, Ordering::Relaxed) + played as u64;
                let pos_ns = (total_played as f64 * 1_000_000_000.0 / (target_sample_rate as f64 * target_channels as f64)) as u64;
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
        let mut resampler = Resampler::new(sample_rate, target_sample_rate, target_channels as usize);

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
                    eprintln!("[Luminous Backend] SeekTo command received. target_ns: {target_ns}");
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
                            eprintln!("[Luminous Backend] Seek successful! seeked_to: {:?}", seeked_to);
                            log::info!("Seek successful: {:?}", seeked_to);
                        }
                        Err(e) => {
                            eprintln!("[Luminous Backend] Seek failed error: {:?}", e);
                            log::error!("Seek failed: {:?}", e);
                        }
                    }

                    // Clear the buffer after seek to avoid stale audio
                    if let Ok(mut consumer) = shared_consumer.lock() {
                        while consumer.try_pop().is_some() {}
                    }
                    let target_samples = (target_ns as f64 * target_sample_rate as f64 * target_channels as f64 / 1_000_000_000.0) as u64;
                    played_samples.store(target_samples, Ordering::Relaxed);
                    position.store(target_ns, Ordering::Relaxed);
                    resampler = Resampler::new(sample_rate, target_sample_rate, target_channels as usize);
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
                let is_empty = prod.occupied_len() == 0;

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
            let is_full = prod.occupied_len() > (target_sample_rate as usize * target_channels as usize * 3 / 2);

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

                            let channel_converted = convert_channels(
                                sample_buf.samples(),
                                channels as usize,
                                target_channels as usize,
                            );
                            let resampled = resampler.resample(&channel_converted);

                            // Push samples into the shared playback buffer
                            let mut pushed = 0;
                            while pushed < resampled.len() {
                                let written = prod.push_slice(&resampled[pushed..]);
                                if written == 0 {
                                    // Ring buffer is full, sleep a bit and try again
                                    std::thread::sleep(std::time::Duration::from_millis(5));
                                } else {
                                    pushed += written;
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
            self.last_frame.copy_from_slice(&input[last_start..last_start + self.channels]);
            self.phase = current_phase - num_input_frames as f64;
        } else {
            self.phase = current_phase;
        }

        output
    }
}
