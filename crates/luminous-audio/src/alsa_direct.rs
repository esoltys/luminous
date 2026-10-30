//! Direct ALSA hardware output sink (Linux).
//!
//! Opens a card's `hw:` PCM itself, bypassing PipeWire/PulseAudio and every
//! ALSA plugin: no server resampling, no mixing with other apps, no `plug`
//! format conversion. The stream opens at one negotiated rate/format for its
//! lifetime, as close to the requested rate as the hardware allows; the
//! engine reopens it to change rate.
//!
//! The PCM is opened, fed and closed on one render thread the sink owns,
//! which holds the card's reservation (`reserve_device`) around it so the
//! sound server lets go of the card first and takes it back afterwards.

use crate::pcm::PcmFormat;
use crate::sink::OutputFallbackReason;

/// `EBUSY`: another process (usually the sound server) has the PCM open.
const EBUSY: i32 = 16;
/// `EAGAIN`: a non-blocking write found the hardware buffer full.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const EAGAIN: i32 = 11;
/// `ETIMEDOUT`: a running stream stopped taking audio.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const ETIMEDOUT: i32 = 110;

/// Rates to try after the requested rate's own family, in order: what most
/// fixed-rate devices (USB headsets, HDMI) run at.
pub(crate) const FALLBACK_RATES: [u32; 2] = [48_000, 44_100];

/// The formats tried, deepest first (`S32_LE`, `S24_LE`, `S24_3LE`,
/// `S16_LE`): every one holds a 16- or 24-bit source losslessly. ALSA's
/// 24-in-32 layout is right-justified, unlike WASAPI's.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const FORMATS: [PcmFormat; 4] = [
    PcmFormat::I32,
    PcmFormat::I24In32Low,
    PcmFormat::I24Packed,
    PcmFormat::I16,
];

/// Maps a failed ALSA call's errno to the reason shown to the user.
pub(crate) fn classify_failure(errno: i32) -> OutputFallbackReason {
    match errno {
        EBUSY => OutputFallbackReason::DeviceInUse,
        _ => OutputFallbackReason::Unavailable,
    }
}

/// Whether a CPAL ALSA device is one the shared list offers: a system-level
/// PCM (`default`, `pipewire`, `pulse`) rather than a per-card plugin
/// (`front:CARD=…`, `sysdefault:CARD=…`, `hw:…`), which lists each card many
/// times over and which Direct Hardware (ALSA) covers properly. `null`
/// discards everything, so it's no output at all.
pub(crate) fn is_system_level_pcm(pcm_id: &str) -> bool {
    !pcm_id.contains(':') && pcm_id != "null"
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
    use parking_lot::{Condvar, Mutex};

    use super::{
        classify_failure, device_id, device_label, is_virtual_driver, EAGAIN, EBUSY, ETIMEDOUT,
        FALLBACK_RATES, FORMATS,
    };
    use crate::pcm::{self, PcmFormat};
    use crate::reserve_device;
    use crate::sink::{
        negotiate_format, AudioBackendKind, AudioSink, OpenError, OutputFallbackReason,
    };
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
    /// How long a card just handed over by the sound server may still be
    /// busy while it finishes closing it.
    const HANDOVER_BUSY_TIMEOUT: Duration = Duration::from_secs(1);
    /// How long a release request waits for the render thread to close the
    /// PCM before answering anyway.
    const RELEASE_TIMEOUT: Duration = Duration::from_secs(1);

    /// A playback PCM on a hardware card.
    pub(crate) struct HwDevice {
        pub(crate) id: String,
        pub(crate) name: String,
        /// The card's ALSA index, which names its reservation.
        pub(crate) card_index: u32,
    }

    /// Lists every playback PCM on every hardware card, skipping software
    /// cards (`snd-aloop`, `snd-dummy`). Cards or devices that can't be
    /// queried are left out rather than failing the whole list.
    pub(crate) fn list_hw_devices() -> Vec<HwDevice> {
        let mut devices = Vec::new();
        for card in ::alsa::card::Iter::new().filter_map(Result::ok) {
            let Ok(card_index) = u32::try_from(card.get_index()) else {
                continue;
            };
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
                    card_index,
                });
            }
        }
        devices
    }

    /// What `play`/`pause`/drop and a release request ask of the render
    /// thread, and its answer that the PCM is closed.
    struct Control {
        playing: AtomicBool,
        quit: AtomicBool,
        /// A higher-priority app asked for the card.
        released: AtomicBool,
        closed: Mutex<bool>,
        closed_cv: Condvar,
    }

    impl Control {
        fn stopping(&self) -> bool {
            self.quit.load(Ordering::Acquire) || self.released.load(Ordering::Acquire)
        }

        fn mark_closed(&self) {
            *self.closed.lock() = true;
            self.closed_cv.notify_all();
        }

        /// The hook the card's reservation runs when another app takes the
        /// card: stops `render_thread` and waits for it to close the PCM.
        fn release_hook(self: &Arc<Self>, render_thread: Thread) -> reserve_device::ReleaseHook {
            let control = Arc::clone(self);
            Box::new(move || {
                control.released.store(true, Ordering::Release);
                render_thread.unpark();
                let mut closed = control.closed.lock();
                if !*closed {
                    let _ = control.closed_cv.wait_for(&mut closed, RELEASE_TIMEOUT);
                }
            })
        }
    }

    /// Tells a release request the PCM is closed, however the render thread ends.
    struct ClosedOnDrop(Arc<Control>);

    impl Drop for ClosedOnDrop {
        fn drop(&mut self) {
            self.0.mark_closed();
        }
    }

    /// Output straight to a hardware PCM.
    pub struct AlsaSink {
        control: Arc<Control>,
        render_thread: Thread,
        thread: Option<JoinHandle<()>>,
        sample_rate: u32,
        format: PcmFormat,
        device_name: Option<String>,
        stream_error_flag: Arc<AtomicBool>,
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
            released: AtomicBool::new(false),
            closed: Mutex::new(false),
            closed_cv: Condvar::new(),
        });
        let stream_error_flag = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();

        let thread = {
            let shared = Arc::clone(shared);
            let control = Arc::clone(&control);
            let error_flag = Arc::clone(&stream_error_flag);
            let id = device.id.clone();
            let card_index = device.card_index;
            std::thread::Builder::new()
                .name("luminous-alsa-render".into())
                .spawn(move || {
                    let _closed = ClosedOnDrop(Arc::clone(&control));
                    // Declared before the stream, so it's released after the
                    // PCM closes.
                    let reservation = match reserve_device::reserve_card(
                        card_index,
                        &id,
                        control.release_hook(std::thread::current()),
                    ) {
                        Ok(reservation) => reservation,
                        Err(e) => {
                            let _ = ready_tx.send(Err(e));
                            return;
                        }
                    };
                    match HwStream::open(&id, requested_rate, reservation.handed_over()) {
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
        // `set_rate` takes the nearest rate the device has; test first so a
        // rate it lacks fails instead of quietly becoming another.
        hwp.test_rate(rate)?;
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
        /// Opens `device_id` near `requested_rate`. `just_handed_over`: the
        /// sound server released the card a moment ago and may still be
        /// closing it, so a busy device is retried briefly.
        fn open(
            device_id: &str,
            requested_rate: u32,
            just_handed_over: bool,
        ) -> Result<Self, OpenError> {
            let busy_deadline = std::time::Instant::now() + HANDOVER_BUSY_TIMEOUT;
            let pcm = loop {
                // Non-blocking, so a device the sound server holds fails at
                // once with EBUSY instead of waiting for it.
                match PCM::new(device_id, Direction::Playback, true) {
                    Ok(pcm) => break pcm,
                    Err(e)
                        if just_handed_over
                            && e.errno() == EBUSY
                            && std::time::Instant::now() < busy_deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(e) => return Err(open_error("Open", e)),
                }
            };

            let (rate, format) =
                negotiate_format(requested_rate, &FALLBACK_RATES, &FORMATS, |rate, format| {
                    alsa_format(format).is_some_and(|f| constrain(&pcm, rate, f).is_ok())
                })
                .ok_or_else(|| OpenError {
                    reason: OutputFallbackReason::FormatUnsupported,
                    message: format!("No stereo integer format near {requested_rate} Hz"),
                })?;

            let (rate, period_frames) = {
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
                // Report the rate the hardware runs at, whatever was asked.
                let rate = current.get_rate().map_err(|e| open_error("Read rate", e))?;
                let period_frames = current
                    .get_period_size()
                    .map_err(|e| open_error("Period size", e))?;
                (rate, period_frames as usize)
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
                if control.released.load(Ordering::Acquire) {
                    break Some(::alsa::Error::new("card released to another app", EBUSY));
                }
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
        /// including the device disappearing or a stall (`ETIMEDOUT`), is
        /// returned.
        fn write(&mut self, pcm: &PCM, control: &Control) -> ::alsa::Result<()> {
            self.renderer.render(&mut self.scratch);
            let len = pcm::write_interleaved(&self.scratch, self.format, &mut self.bytes)
                * self.format.container_bytes();
            let frame_bytes = CHANNELS as usize * self.format.container_bytes();
            let io = pcm.io_bytes();
            let mut offset = 0;
            while offset < len {
                if control.stopping() || !control.playing.load(Ordering::Acquire) {
                    return Ok(());
                }
                match io.writei(&self.bytes[offset..len]) {
                    Ok(frames) => offset += frames * frame_bytes,
                    Err(e) if e.errno() == EAGAIN => match pcm.wait(Some(STALL_TIMEOUT_MS)) {
                        Ok(true) => {}
                        Ok(false) => return Err(::alsa::Error::new("snd_pcm_wait", ETIMEDOUT)),
                        Err(e) => pcm.try_recover(e, true)?,
                    },
                    Err(e) => pcm.try_recover(e, true)?,
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
    fn the_shared_list_keeps_only_system_level_pcms() {
        for id in ["default", "pipewire", "pulse"] {
            assert!(is_system_level_pcm(id), "{id}");
        }
        for id in [
            "null",
            "sysdefault:CARD=PCH",
            "front:CARD=PCH,DEV=0",
            "surround51:CARD=PCH,DEV=0",
            "hdmi:CARD=PCH,DEV=0",
            "hw:CARD=EarPods,DEV=0",
            "plughw:CARD=EarPods,DEV=0",
        ] {
            assert!(!is_system_level_pcm(id), "{id}");
        }
    }

    #[test]
    fn software_cards_are_not_hardware_outputs() {
        assert!(is_virtual_driver("Loopback"));
        assert!(is_virtual_driver("Dummy"));
        assert!(!is_virtual_driver("USB-Audio"));
        assert!(!is_virtual_driver("HDA-Intel"));
    }
}
