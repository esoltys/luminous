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
pub(crate) use ::alsa::mixer::MilliBel;

#[cfg(not(target_os = "linux"))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MilliBel(pub i64);

/// An element in the simple mixer that may control playback volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MixerElementInfo {
    pub(crate) name: String,
    pub(crate) index: u32,
    pub(crate) has_playback_volume: bool,
    pub(crate) has_playback_switch: bool,
}

/// Picks the best playback volume element for a PCM from the card's mixer elements.
/// Priority order:
/// 1) Exact (case-insensitive) match with `pcm_name`
/// 2) "PCM"
/// 3) "Master"
/// 4) "Speaker"
/// 5) "Headphone"
/// 6) "Headset"
/// 7) The first element with playback volume
pub(crate) fn pick_mixer_element(elements: &[MixerElementInfo], pcm_name: &str) -> Option<usize> {
    if !pcm_name.is_empty() {
        if let Some(pos) = elements
            .iter()
            .position(|e| e.has_playback_volume && e.name.eq_ignore_ascii_case(pcm_name))
        {
            return Some(pos);
        }
    }
    const PREFERRED_NAMES: &[&str] = &["PCM", "Master", "Speaker", "Headphone", "Headset"];
    for &name in PREFERRED_NAMES {
        if let Some(pos) = elements
            .iter()
            .position(|e| e.has_playback_volume && e.name.eq_ignore_ascii_case(name))
        {
            return Some(pos);
        }
    }
    elements.iter().position(|e| e.has_playback_volume)
}

/// Maps a linear volume slider position `vol` in `[0.0, 1.0]` to ALSA millibels
/// (`MilliBel`) and mute switch state.
///
/// Logarithmic mapping: `dB = 20 * log10(vol)`, so `mB = 2000 * log10(vol)`.
/// Clamped between `min_mb` and `max_mb.min(0)` so 1.0 never boosts beyond 0 dB.
/// Returns `(min_mb, true)` (muted) when `vol <= 0.0`.
pub(crate) fn linear_to_alsa_db(vol: f32, min_mb: MilliBel, max_mb: MilliBel) -> (MilliBel, bool) {
    if vol <= 0.0 {
        return (min_mb, true);
    }
    let cap_mb = max_mb.0.min(0);
    if vol >= 1.0 || cap_mb <= min_mb.0 {
        return (MilliBel(cap_mb), false);
    }
    let mb = (2000.0 * vol.log10()).round() as i64;
    (MilliBel(mb.clamp(min_mb.0, cap_mb)), false)
}

/// Maps an ALSA millibel value (`MilliBel`) and mute state back to linear volume `[0.0, 1.0]`.
pub(crate) fn alsa_db_to_linear(
    mb: MilliBel,
    min_mb: MilliBel,
    max_mb: MilliBel,
    muted: bool,
) -> f32 {
    if muted || mb <= min_mb {
        return 0.0;
    }
    let cap_mb = max_mb.0.min(0);
    if mb.0 >= cap_mb || cap_mb <= min_mb.0 {
        return 1.0;
    }
    let vol = 10.0f32.powf(mb.0 as f32 / 2000.0);
    vol.clamp(0.0, 1.0)
}

/// Maps linear volume to raw integer volume for elements lacking dB range.
pub(crate) fn linear_to_raw_volume(vol: f32, min_raw: i64, max_raw: i64) -> (i64, bool) {
    if vol <= 0.0 {
        return (min_raw, true);
    }
    if vol >= 1.0 || max_raw <= min_raw {
        return (max_raw, false);
    }
    let raw = min_raw + ((max_raw - min_raw) as f32 * vol).round() as i64;
    (raw.clamp(min_raw, max_raw), false)
}

/// Maps raw integer volume back to linear volume `[0.0, 1.0]`.
pub(crate) fn raw_volume_to_linear(raw: i64, min_raw: i64, max_raw: i64, muted: bool) -> f32 {
    if muted || raw <= min_raw || max_raw <= min_raw {
        return 0.0;
    }
    if raw >= max_raw {
        return 1.0;
    }
    let vol = (raw - min_raw) as f32 / (max_raw - min_raw) as f32;
    vol.clamp(0.0, 1.0)
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
        alsa_db_to_linear, classify_failure, device_id, device_label, is_virtual_driver,
        linear_to_alsa_db, linear_to_raw_volume, pick_mixer_element, raw_volume_to_linear,
        MilliBel, MixerElementInfo, EAGAIN, EBUSY, ETIMEDOUT, FALLBACK_RATES, FORMATS,
    };
    use crate::pcm::{self, PcmFormat};
    use crate::reserve_device;
    use crate::sink::{
        negotiate_format, AudioBackendKind, AudioSink, OpenError, OutputFallbackReason,
    };
    use crate::{AudioShared, OutputRenderer};
    use ::alsa::mixer::{Mixer, Selem, SelemChannelId, SelemId};
    use std::sync::atomic::AtomicU32;

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
        pub(crate) card_id: String,
        pub(crate) pcm_name: String,
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
                    card_id: card_id.to_string(),
                    pcm_name: pcm_info.get_name().unwrap_or("").to_string(),
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
        mixer_controller: Option<AlsaMixerController>,
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

        fn has_hardware_volume(&self) -> bool {
            self.mixer_controller.is_some()
        }

        fn set_hardware_volume(&self, volume: f32) {
            if let Some(ref mixer) = self.mixer_controller {
                mixer.set_volume(volume);
            }
        }
    }

    impl Drop for AlsaSink {
        fn drop(&mut self) {
            drop(self.mixer_controller.take());
            self.control.quit.store(true, Ordering::Release);
            self.render_thread.unpark();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    /// Dedicated controller thread managing hardware volume via ALSA simple mixer.
    struct AlsaMixerController {
        target_volume: Arc<AtomicU32>,
        quit: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl AlsaMixerController {
        fn open(card_id: &str, pcm_name: &str, shared: Arc<AudioShared>) -> Option<Self> {
            let mixer_card_name = format!("hw:CARD={card_id}");
            let mixer = match Mixer::new(&mixer_card_name, false) {
                Ok(m) => m,
                Err(e) => {
                    log::info!("No ALSA mixer for {mixer_card_name}: {e}");
                    return None;
                }
            };

            let mut elements = Vec::new();
            for elem in mixer.iter() {
                if let Some(selem) = Selem::new(elem) {
                    let id = selem.get_id();
                    let name = id.get_name().unwrap_or("").to_string();
                    let index = id.get_index();
                    let has_playback_volume = selem.has_playback_volume();
                    let has_playback_switch = selem.has_playback_switch();
                    elements.push((
                        MixerElementInfo {
                            name,
                            index,
                            has_playback_volume,
                            has_playback_switch,
                        },
                        (id.get_name().unwrap_or("").to_string(), index),
                    ));
                }
            }

            let infos: Vec<_> = elements.iter().map(|(info, _)| info.clone()).collect();
            let chosen_idx = pick_mixer_element(&infos, pcm_name)?;
            let (_, (chosen_name, chosen_index)) = elements.remove(chosen_idx);
            let sid = SelemId::new(&chosen_name, chosen_index);
            let selem = mixer.find_selem(&sid)?;

            let (min_mb, max_mb) = selem.get_playback_db_range();
            let has_db_range = max_mb > min_mb;
            let (min_raw, max_raw) = selem.get_playback_volume_range();
            let has_playback_switch = selem.has_playback_switch();

            let saved_db = if has_db_range {
                selem.get_playback_vol_db(SelemChannelId::mono()).ok()
            } else {
                None
            };
            let saved_raw = selem
                .get_playback_volume(SelemChannelId::mono())
                .ok()
                .unwrap_or(min_raw);
            let saved_switch = if has_playback_switch {
                selem.get_playback_switch(SelemChannelId::mono()).ok()
            } else {
                None
            };

            let initial_vol = f32::from_bits(shared.volume.load(Ordering::Acquire));
            apply_to_selem(
                &selem,
                initial_vol,
                has_db_range,
                min_mb,
                max_mb,
                min_raw,
                max_raw,
                has_playback_switch,
            );

            shared.hardware_volume_active.store(true, Ordering::Release);
            shared.output_hardware_volume.store(true, Ordering::Release);

            let quit = Arc::new(AtomicBool::new(false));
            let target_volume = Arc::new(AtomicU32::new(initial_vol.to_bits()));

            let thread = {
                let quit = Arc::clone(&quit);
                let target_volume = Arc::clone(&target_volume);
                let shared = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name("luminous-alsa-mixer".into())
                    .spawn(move || {
                        run_mixer_loop(
                            mixer,
                            chosen_name,
                            chosen_index,
                            has_db_range,
                            min_mb,
                            max_mb,
                            min_raw,
                            max_raw,
                            has_playback_switch,
                            saved_db,
                            saved_raw,
                            saved_switch,
                            initial_vol,
                            target_volume,
                            quit,
                            shared,
                        );
                    })
                    .ok()?
            };

            Some(Self {
                target_volume,
                quit,
                thread: Some(thread),
            })
        }

        fn set_volume(&self, volume: f32) {
            self.target_volume
                .store(volume.to_bits(), Ordering::Release);
        }
    }

    impl Drop for AlsaMixerController {
        fn drop(&mut self) {
            self.quit.store(true, Ordering::Release);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn apply_to_selem(
        selem: &Selem<'_>,
        vol: f32,
        has_db_range: bool,
        min_mb: MilliBel,
        max_mb: MilliBel,
        min_raw: i64,
        max_raw: i64,
        has_playback_switch: bool,
    ) {
        if has_db_range {
            let (mb, muted) = linear_to_alsa_db(vol, min_mb, max_mb);
            let _ = selem.set_playback_db_all(mb, ::alsa::Round::Floor);
            if has_playback_switch {
                let _ = selem.set_playback_switch_all(if muted { 0 } else { 1 });
            }
        } else {
            let (raw, muted) = linear_to_raw_volume(vol, min_raw, max_raw);
            let _ = selem.set_playback_volume_all(raw);
            if has_playback_switch {
                let _ = selem.set_playback_switch_all(if muted { 0 } else { 1 });
            }
        }
    }

    fn read_from_selem(
        selem: &Selem<'_>,
        has_db_range: bool,
        min_mb: MilliBel,
        max_mb: MilliBel,
        min_raw: i64,
        max_raw: i64,
        has_playback_switch: bool,
    ) -> Option<f32> {
        let muted = if has_playback_switch {
            selem
                .get_playback_switch(SelemChannelId::mono())
                .ok()
                .map(|s| s == 0)
                .unwrap_or(false)
        } else {
            false
        };
        if has_db_range {
            let mb = selem.get_playback_vol_db(SelemChannelId::mono()).ok()?;
            Some(alsa_db_to_linear(mb, min_mb, max_mb, muted))
        } else {
            let raw = selem.get_playback_volume(SelemChannelId::mono()).ok()?;
            Some(raw_volume_to_linear(raw, min_raw, max_raw, muted))
        }
    }

    fn run_mixer_loop(
        mixer: Mixer,
        element_name: String,
        element_index: u32,
        has_db_range: bool,
        min_mb: MilliBel,
        max_mb: MilliBel,
        min_raw: i64,
        max_raw: i64,
        has_playback_switch: bool,
        saved_db: Option<MilliBel>,
        saved_raw: i64,
        saved_switch: Option<i32>,
        initial_vol: f32,
        target_volume: Arc<AtomicU32>,
        quit: Arc<AtomicBool>,
        shared: Arc<AudioShared>,
    ) {
        let sid = SelemId::new(&element_name, element_index);
        let mut last_applied_vol = initial_vol;
        while !quit.load(Ordering::Acquire) {
            let target_vol = f32::from_bits(target_volume.load(Ordering::Acquire));
            if (target_vol - last_applied_vol).abs() > 0.001 {
                if let Some(selem) = mixer.find_selem(&sid) {
                    apply_to_selem(
                        &selem,
                        target_vol,
                        has_db_range,
                        min_mb,
                        max_mb,
                        min_raw,
                        max_raw,
                        has_playback_switch,
                    );
                    last_applied_vol = target_vol;
                }
            }

            let _ = mixer.wait(Some(50));
            if let Ok(count) = mixer.handle_events() {
                if count > 0 {
                    if let Some(selem) = mixer.find_selem(&sid) {
                        if let Some(read_vol) = read_from_selem(
                            &selem,
                            has_db_range,
                            min_mb,
                            max_mb,
                            min_raw,
                            max_raw,
                            has_playback_switch,
                        ) {
                            if (read_vol - last_applied_vol).abs() >= 0.005 {
                                last_applied_vol = read_vol;
                                target_volume.store(read_vol.to_bits(), Ordering::Release);
                                shared.volume.store(read_vol.to_bits(), Ordering::Release);
                                let _ = shared
                                    .event_tx
                                    .send(crate::AudioEvent::VolumeChanged(read_vol));
                            }
                        }
                    }
                }
            }
        }

        // Teardown: restore original level on the element
        if let Some(selem) = mixer.find_selem(&sid) {
            if has_db_range {
                if let Some(mb) = saved_db {
                    let _ = selem.set_playback_db_all(mb, ::alsa::Round::Floor);
                }
            } else {
                let _ = selem.set_playback_volume_all(saved_raw);
            }
            if let Some(sw) = saved_switch {
                let _ = selem.set_playback_switch_all(sw);
            }
        }
        shared
            .hardware_volume_active
            .store(false, Ordering::Release);
        shared
            .output_hardware_volume
            .store(false, Ordering::Release);
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
            Ok(Ok((sample_rate, format))) => {
                let mixer_controller = AlsaMixerController::open(
                    &device.card_id,
                    &device.pcm_name,
                    Arc::clone(shared),
                );
                Ok(AlsaSink {
                    control,
                    render_thread: thread.thread().clone(),
                    thread: Some(thread),
                    sample_rate,
                    format,
                    device_name: Some(device.name),
                    stream_error_flag,
                    mixer_controller,
                })
            }
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

        /// Runs until asked to quit, the card is released to another app, or
        /// the device fails; the last two set `error_flag`, which makes the
        /// engine rebuild its output.
        fn run(self, mut period: RenderPeriod, control: &Control, error_flag: &AtomicBool) {
            let mut started = false;
            let failure = loop {
                if control.stopping() {
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
            // Giving the card away ends the stream like a failure would, so
            // the engine rebuilds its output (on shared output, since the
            // app that took the card outranks us).
            let released = control.released.load(Ordering::Acquire);
            match &failure {
                Some(e) => log::error!("ALSA hardware stream failed: {e}"),
                None if released => {
                    log::info!("ALSA hardware stream closed: card released to another app")
                }
                None => {}
            }
            if failure.is_some() || released {
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

    #[test]
    fn mixer_element_priority_selection() {
        let elements = vec![
            MixerElementInfo {
                name: "Capture".into(),
                index: 0,
                has_playback_volume: false,
                has_playback_switch: false,
            },
            MixerElementInfo {
                name: "Line In".into(),
                index: 0,
                has_playback_volume: true,
                has_playback_switch: true,
            },
            MixerElementInfo {
                name: "Headphone".into(),
                index: 0,
                has_playback_volume: true,
                has_playback_switch: true,
            },
            MixerElementInfo {
                name: "Speaker".into(),
                index: 0,
                has_playback_volume: true,
                has_playback_switch: true,
            },
            MixerElementInfo {
                name: "Master".into(),
                index: 0,
                has_playback_volume: true,
                has_playback_switch: true,
            },
            MixerElementInfo {
                name: "PCM".into(),
                index: 0,
                has_playback_volume: true,
                has_playback_switch: true,
            },
        ];

        // 1) PCM name match takes highest priority
        assert_eq!(pick_mixer_element(&elements, "Speaker"), Some(3));
        assert_eq!(pick_mixer_element(&elements, "headphone"), Some(2));

        // 2) With empty/non-matching pcm_name, preferred candidate order applies:
        // PCM (idx 5) > Master (idx 4) > Speaker (idx 3) > Headphone (idx 2)
        assert_eq!(pick_mixer_element(&elements, "Unknown"), Some(5));
        assert_eq!(pick_mixer_element(&elements, ""), Some(5));

        // Drop PCM -> picks Master
        assert_eq!(pick_mixer_element(&elements[..5], ""), Some(4));
        // Drop Master -> picks Speaker
        assert_eq!(pick_mixer_element(&elements[..4], ""), Some(3));
        // Drop Speaker -> picks Headphone
        assert_eq!(pick_mixer_element(&elements[..3], ""), Some(2));
        // Drop Headphone -> picks first playback element ("Line In", idx 1)
        assert_eq!(pick_mixer_element(&elements[..2], ""), Some(1));
        // Only elements without playback volume -> None
        assert_eq!(pick_mixer_element(&elements[..1], ""), None);
    }

    #[test]
    fn linear_and_alsa_db_roundtrip() {
        let min_mb = MilliBel(-6000); // -60 dB
        let max_mb = MilliBel(0); // 0 dB

        // Zero volume mutes and sets minimum dB
        let (mb, muted) = linear_to_alsa_db(0.0, min_mb, max_mb);
        assert!(muted);
        assert_eq!(mb, min_mb);
        assert_eq!(alsa_db_to_linear(mb, min_mb, max_mb, muted), 0.0);

        // Unity volume (1.0) maps to max (0 dB) and unmuted
        let (mb, muted) = linear_to_alsa_db(1.0, min_mb, max_mb);
        assert!(!muted);
        assert_eq!(mb, MilliBel(0));
        assert!((alsa_db_to_linear(mb, min_mb, max_mb, muted) - 1.0).abs() < 1e-4);

        // 0.5 linear amplitude -> -6.02 dB (-602 mB)
        let (mb, muted) = linear_to_alsa_db(0.5, min_mb, max_mb);
        assert!(!muted);
        assert_eq!(mb.0, -602);
        let vol = alsa_db_to_linear(mb, min_mb, max_mb, muted);
        assert!((vol - 0.5).abs() < 0.01);

        // 0.1 linear amplitude -> -20 dB (-2000 mB)
        let (mb, muted) = linear_to_alsa_db(0.1, min_mb, max_mb);
        assert!(!muted);
        assert_eq!(mb.0, -2000);
        let vol = alsa_db_to_linear(mb, min_mb, max_mb, muted);
        assert!((vol - 0.1).abs() < 0.01);
    }

    #[test]
    fn linear_and_raw_volume_roundtrip() {
        let min_raw = 0;
        let max_raw = 100;

        let (raw, muted) = linear_to_raw_volume(0.0, min_raw, max_raw);
        assert!(muted);
        assert_eq!(raw, 0);
        assert_eq!(raw_volume_to_linear(raw, min_raw, max_raw, muted), 0.0);

        let (raw, muted) = linear_to_raw_volume(1.0, min_raw, max_raw);
        assert!(!muted);
        assert_eq!(raw, 100);
        assert_eq!(raw_volume_to_linear(raw, min_raw, max_raw, muted), 1.0);

        let (raw, muted) = linear_to_raw_volume(0.5, min_raw, max_raw);
        assert!(!muted);
        assert_eq!(raw, 50);
        assert_eq!(raw_volume_to_linear(raw, min_raw, max_raw, muted), 0.5);
    }
}
