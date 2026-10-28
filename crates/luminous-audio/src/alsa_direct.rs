//! Direct ALSA hardware output sink (Linux).
//!
//! Opens a card's `hw:` PCM itself, bypassing PipeWire/PulseAudio and every
//! ALSA plugin: no server resampling, no mixing with other apps, no `plug`
//! format conversion. The stream opens at one negotiated rate/format for its
//! lifetime, as close to the requested rate as the hardware allows; the
//! engine reopens it to change rate.
//!
//! The PCM is opened, fed and closed on one render thread the sink owns.

use crate::sink::OutputFallbackReason;

/// `EBUSY`: another process (usually the sound server) has the PCM open.
const EBUSY: i32 = 16;
/// `EAGAIN`: a non-blocking write found the hardware buffer full.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const EAGAIN: i32 = 11;

/// Rates to try after the requested rate's own family, in order: what most
/// fixed-rate devices (USB headsets, HDMI) run at.
pub(crate) const FALLBACK_RATES: [u32; 2] = [48_000, 44_100];

/// Why the hardware stream couldn't be opened.
#[derive(Debug, Clone)]
pub(crate) struct OpenError {
    pub(crate) reason: OutputFallbackReason,
    pub(crate) message: String,
}

/// Maps a failed ALSA call's errno to the reason shown to the user.
pub(crate) fn classify_failure(errno: i32) -> OutputFallbackReason {
    match errno {
        EBUSY => OutputFallbackReason::DeviceInUse,
        _ => OutputFallbackReason::Unavailable,
    }
}

/// Whether a card is a software device rather than hardware that plays sound.
pub(crate) fn is_virtual_driver(driver: &str) -> bool {
    matches!(driver, "Loopback" | "Dummy")
}

/// The id a hardware PCM is chosen and persisted by. It names the card by its
/// id ("PCH", "EarPods"), not its index, so a choice survives a reboot or a
/// USB device coming back on another index.
pub(crate) fn device_id(card_id: &str, device: u32) -> String {
    format!("hw:CARD={card_id},DEV={device}")
}

/// The name shown for a hardware PCM, e.g. "EarPods — USB Audio".
pub(crate) fn device_label(card_name: &str, pcm_name: &str) -> String {
    if pcm_name.is_empty() || pcm_name == card_name {
        card_name.to_string()
    } else {
        format!("{card_name} — {pcm_name}")
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread::{JoinHandle, Thread};
    use std::time::Duration;

    use ::alsa::ctl::{CardInfo, Ctl, DeviceIter};
    use ::alsa::pcm::{Access, Format, HwParams, PCM};
    use ::alsa::{Direction, ValueOr};
    use parking_lot::Mutex;

    use super::{
        classify_failure, device_id, device_label, is_virtual_driver, OpenError, EAGAIN,
        FALLBACK_RATES,
    };
    use crate::pcm::{self, PcmFormat};
    use crate::sink::{negotiate_format, AudioBackendKind, AudioSink, OutputFallbackReason};
    use crate::{AudioShared, OutputRenderer};

    const CHANNELS: u16 = 2;
    /// How long opening may take before the build gives up (inside the
    /// engine's 5 s rebuild budget).
    const OPEN_TIMEOUT: Duration = Duration::from_secs(4);
    /// A running stream that takes no audio for this long has stalled.
    const STALL_TIMEOUT_MS: u32 = 2_000;
    /// Hardware buffer and period lengths, in µs: short enough that a pause
    /// or seek discards little, long enough to ride out scheduling hiccups
    /// on a thread without real-time priority.
    const BUFFER_TIME_US: u32 = 100_000;
    const PERIOD_TIME_US: u32 = 25_000;

    /// A playback PCM on a hardware card.
    pub(crate) struct HwDevice {
        pub(crate) id: String,
        pub(crate) name: String,
    }

    /// Lists every playback PCM on every hardware card, skipping software
    /// cards (`snd-aloop`, `snd-dummy`). Cards or devices that can't be
    /// queried are left out rather than failing the whole list.
    pub(crate) fn list_hw_devices() -> Vec<HwDevice> {
        let mut devices = Vec::new();
        for card in ::alsa::card::Iter::new().filter_map(Result::ok) {
            let Ok(ctl) = Ctl::from_card(&card, false) else {
                continue;
            };
            let Ok(info): Result<CardInfo, _> = ctl.card_info() else {
                continue;
            };
            let (Ok(card_id), Ok(card_name), Ok(driver)) =
                (info.get_id(), info.get_name(), info.get_driver())
            else {
                continue;
            };
            if is_virtual_driver(driver) {
                continue;
            }
            for dev in DeviceIter::new(&ctl) {
                let Ok(dev) = u32::try_from(dev) else {
                    continue;
                };
                // Fails for capture-only devices.
                let Ok(pcm_info) = ctl.pcm_info(dev, 0, Direction::Playback) else {
                    continue;
                };
                devices.push(HwDevice {
                    id: device_id(card_id, dev),
                    name: device_label(card_name, pcm_info.get_name().unwrap_or("")),
                });
            }
        }
        devices
    }

    /// What `play`/`pause`/drop ask of the render thread.
    struct Control {
        playing: AtomicBool,
        quit: AtomicBool,
    }

    /// Output straight to a hardware PCM.
    pub struct AlsaSink {
        control: Arc<Control>,
        render_thread: Thread,
        thread: Option<JoinHandle<()>>,
        sample_rate: u32,
        format: PcmFormat,
        device_name: Option<String>,
        pub(crate) stream_error_flag: Arc<AtomicBool>,
    }

    impl AudioSink for AlsaSink {
        fn play(&self) -> Result<(), String> {
            self.control.playing.store(true, Ordering::Release);
            self.render_thread.unpark();
            Ok(())
        }

        fn pause(&self) -> Result<(), String> {
            self.control.playing.store(false, Ordering::Release);
            self.render_thread.unpark();
            Ok(())
        }

        fn sample_rate(&self) -> u32 {
            self.sample_rate
        }

        fn channels(&self) -> u16 {
            CHANNELS
        }

        fn device_name(&self) -> Option<String> {
            self.device_name.clone()
        }

        fn backend(&self) -> AudioBackendKind {
            AudioBackendKind::Alsa
        }

        fn output_format(&self) -> &'static str {
            self.format.label()
        }

        fn is_errored(&self) -> bool {
            self.stream_error_flag.load(Ordering::Relaxed)
        }
    }

    impl Drop for AlsaSink {
        fn drop(&mut self) {
            self.control.quit.store(true, Ordering::Release);
            self.render_thread.unpark();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    /// Opens the hardware PCM `device_id` (the first hardware device when
    /// `None`) and starts its render thread, paused. Fails with the reason to
    /// report when the device is gone, held by another process (usually the
    /// sound server), or takes no stereo integer format near `requested_rate`.
    pub(crate) fn build_alsa_sink(
        device_id: Option<&str>,
        requested_rate: u32,
        shared: &Arc<AudioShared>,
        consumer: Arc<Mutex<ringbuf::HeapCons<f32>>>,
        played_samples: Arc<AtomicU64>,
    ) -> Result<AlsaSink, OpenError> {
        let unavailable = |message: String| OpenError {
            reason: OutputFallbackReason::Unavailable,
            message,
        };
        let devices = list_hw_devices();
        let device = match device_id {
            Some(id) => devices.into_iter().find(|d| d.id == id),
            None => devices.into_iter().next(),
        }
        .ok_or_else(|| unavailable("Hardware device not found".into()))?;

        let control = Arc::new(Control {
            playing: AtomicBool::new(false),
            quit: AtomicBool::new(false),
        });
        let stream_error_flag = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();

        let thread = {
            let shared = Arc::clone(shared);
            let control = Arc::clone(&control);
            let error_flag = Arc::clone(&stream_error_flag);
            let id = device.id.clone();
            std::thread::Builder::new()
                .name("luminous-alsa-render".into())
                .spawn(move || match HwStream::open(&id, requested_rate) {
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                    Ok(stream) => {
                        let samples = stream.period_frames * CHANNELS as usize;
                        let period = RenderPeriod {
                            renderer: OutputRenderer {
                                shared,
                                consumer,
                                played_samples,
                                mono_scratch: Vec::with_capacity(samples),
                                sample_rate: stream.rate,
                                channels: CHANNELS,
                            },
                            scratch: vec![0.0; samples],
                            bytes: vec![0; samples * stream.format.container_bytes()],
                            format: stream.format,
                        };
                        let _ = ready_tx.send(Ok((stream.rate, stream.format)));
                        stream.run(period, &control, &error_flag);
                    }
                })
                .map_err(|e| unavailable(format!("Failed to spawn render thread: {e}")))?
        };

        match ready_rx.recv_timeout(OPEN_TIMEOUT) {
            Ok(Ok((sample_rate, format))) => Ok(AlsaSink {
                control,
                render_thread: thread.thread().clone(),
                thread: Some(thread),
                sample_rate,
                format,
                device_name: Some(device.name),
                stream_error_flag,
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                // Leave the thread to exit on its own once its call returns.
                control.quit.store(true, Ordering::Release);
                thread.thread().unpark();
                Err(unavailable("Timed out opening hardware stream".into()))
            }
        }
    }

    fn alsa_format(format: PcmFormat) -> Option<Format> {
        match format {
            PcmFormat::I32 => Some(Format::S32LE),
            PcmFormat::I24In32Low => Some(Format::S24LE),
            PcmFormat::I24Packed => Some(Format::S243LE),
            PcmFormat::I16 => Some(Format::S16LE),
            PcmFormat::I24In32 => None,
        }
    }

    fn open_error(context: &str, e: ::alsa::Error) -> OpenError {
        OpenError {
            reason: classify_failure(e.errno()),
            message: format!("{context}: {e}"),
        }
    }

    /// Hardware parameters for interleaved stereo at exactly `rate` in
    /// `format`, with ALSA's own resampling off. `Err` when the device can't
    /// do that combination.
    fn constrain(pcm: &PCM, rate: u32, format: Format) -> ::alsa::Result<HwParams<'_>> {
        let hwp = HwParams::any(pcm)?;
        hwp.set_access(Access::RWInterleaved)?;
        hwp.set_rate_resample(false)?;
        hwp.set_channels(CHANNELS as u32)?;
        hwp.set_format(format)?;
        hwp.set_rate(rate, ValueOr::Nearest)?;
        Ok(hwp)
    }

    /// A configured, not yet started hardware stream.
    struct HwStream {
        pcm: PCM,
        period_frames: usize,
        rate: u32,
        format: PcmFormat,
    }

    impl HwStream {
        fn open(device_id: &str, requested_rate: u32) -> Result<Self, OpenError> {
            // Non-blocking, so a device the sound server holds fails at once
            // with EBUSY instead of waiting for it.
            let pcm = PCM::new(device_id, Direction::Playback, true)
                .map_err(|e| open_error("Open", e))?;

            let (rate, format) = negotiate_format(
                requested_rate,
                &FALLBACK_RATES,
                &PcmFormat::ALSA_PREFERENCE,
                |rate, format| {
                    alsa_format(format).is_some_and(|f| constrain(&pcm, rate, f).is_ok())
                },
            )
            .ok_or_else(|| OpenError {
                reason: OutputFallbackReason::FormatUnsupported,
                message: format!("No stereo integer format near {requested_rate} Hz"),
            })?;

            let period_frames = {
                // `negotiate_format` only returns ALSA formats.
                let hwp = constrain(&pcm, rate, alsa_format(format).unwrap_or(Format::S16LE))
                    .map_err(|e| open_error("Hardware parameters", e))?;
                hwp.set_buffer_time_near(BUFFER_TIME_US, ValueOr::Nearest)
                    .map_err(|e| open_error("Buffer time", e))?;
                hwp.set_period_time_near(PERIOD_TIME_US, ValueOr::Nearest)
                    .map_err(|e| open_error("Period time", e))?;
                pcm.hw_params(&hwp)
                    .map_err(|e| open_error("Apply hardware parameters", e))?;
                let current = pcm
                    .hw_params_current()
                    .map_err(|e| open_error("Read hardware parameters", e))?;
                current
                    .get_period_size()
                    .map_err(|e| open_error("Period size", e))? as usize
            };

            log::info!(
                "ALSA hardware stream open on {device_id}: {rate} Hz, {}, {period_frames}-frame period",
                format.label()
            );
            Ok(Self {
                pcm,
                period_frames,
                rate,
                format,
            })
        }

        /// Runs until asked to quit or the device fails; a failure sets
        /// `error_flag`, which makes the engine rebuild its output.
        fn run(self, mut period: RenderPeriod, control: &Control, error_flag: &AtomicBool) {
            let mut started = false;
            let failure = loop {
                if control.quit.load(Ordering::Acquire) {
                    break None;
                }
                let want_playing = control.playing.load(Ordering::Acquire);
                if started && !want_playing {
                    // Drop the queued audio so a later start doesn't replay
                    // what was buffered before a seek or track change.
                    let _ = self.pcm.drop();
                    let _ = self.pcm.prepare();
                }
                started = want_playing;
                if !started {
                    std::thread::park_timeout(Duration::from_millis(250));
                    continue;
                }
                if let Err(e) = period.write(&self.pcm, control) {
                    break Some(e);
                }
            };

            let _ = self.pcm.drop();
            if let Some(e) = failure {
                log::error!("ALSA hardware stream failed: {e}");
                error_flag.store(true, Ordering::Relaxed);
            }
        }
    }

    /// One period's worth of output: renders through the engine's gain chain,
    /// converts to the device format and writes it. Runs on the render
    /// thread: never allocates.
    struct RenderPeriod {
        renderer: OutputRenderer,
        /// Interleaved float scratch, one period long.
        scratch: Vec<f32>,
        /// The same period in the device format.
        bytes: Vec<u8>,
        format: PcmFormat,
    }

    impl RenderPeriod {
        /// Renders one period and writes all of it, waiting for room in the
        /// hardware buffer as needed. Stops early (dropping the rest) when
        /// asked to pause or quit. Recovers from underruns; anything else,
        /// including the device disappearing, is returned.
        fn write(&mut self, pcm: &PCM, control: &Control) -> Result<(), String> {
            self.renderer.render(&mut self.scratch);
            let len = pcm::write_interleaved(&self.scratch, self.format, &mut self.bytes)
                * self.format.container_bytes();
            let frame_bytes = CHANNELS as usize * self.format.container_bytes();
            let io = pcm.io_bytes();
            let mut offset = 0;
            while offset < len {
                if control.quit.load(Ordering::Acquire) || !control.playing.load(Ordering::Acquire)
                {
                    return Ok(());
                }
                match io.writei(&self.bytes[offset..len]) {
                    Ok(frames) => offset += frames * frame_bytes,
                    Err(e) if e.errno() == EAGAIN => match pcm.wait(Some(STALL_TIMEOUT_MS)) {
                        Ok(true) => {}
                        Ok(false) => return Err("Hardware stream stopped taking audio".into()),
                        Err(e) => pcm.try_recover(e, true).map_err(|e| e.to_string())?,
                    },
                    Err(e) => pcm.try_recover(e, true).map_err(|e| e.to_string())?,
                }
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) use imp::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_device_is_reported_as_in_use() {
        assert_eq!(classify_failure(EBUSY), OutputFallbackReason::DeviceInUse);
        // ENOENT: the device isn't there (unplugged).
        assert_eq!(classify_failure(2), OutputFallbackReason::Unavailable);
    }

    #[test]
    fn devices_are_identified_by_card_id_not_index() {
        assert_eq!(device_id("EarPods", 0), "hw:CARD=EarPods,DEV=0");
    }

    #[test]
    fn labels_join_card_and_pcm_names_without_repeating_them() {
        assert_eq!(device_label("EarPods", "USB Audio"), "EarPods — USB Audio");
        assert_eq!(
            device_label("HDA Intel PCH", "HDMI 0"),
            "HDA Intel PCH — HDMI 0"
        );
        assert_eq!(device_label("Scarlett 2i2", ""), "Scarlett 2i2");
        assert_eq!(device_label("Scarlett 2i2", "Scarlett 2i2"), "Scarlett 2i2");
    }

    #[test]
    fn software_cards_are_not_hardware_outputs() {
        assert!(is_virtual_driver("Loopback"));
        assert!(is_virtual_driver("Dummy"));
        assert!(!is_virtual_driver("USB-Audio"));
        assert!(!is_virtual_driver("HDA-Intel"));
    }
}
