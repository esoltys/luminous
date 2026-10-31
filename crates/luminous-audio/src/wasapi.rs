//! WASAPI exclusive-mode output sink (Windows).
//!
//! Takes sole control of an endpoint and hands it integer PCM directly,
//! bypassing the Windows mixer: no system resampling, no mixing with other
//! apps, no Windows volume or effects. The stream opens at one negotiated
//! rate/format for its lifetime, as close to the requested rate as the device
//! allows; the engine reopens it to change rate.
//!
//! All COM work (activation, initialisation, the event-driven render loop,
//! teardown) happens on one render thread the sink owns, so no other thread's
//! COM apartment history can interfere with it.

use crate::sink::OutputFallbackReason;

// HRESULTs from audioclient.h, kept here so the classifier is testable on any
// platform.
const AUDCLNT_E_UNSUPPORTED_FORMAT: i32 = 0x8889_0008_u32 as i32;
const AUDCLNT_E_DEVICE_IN_USE: i32 = 0x8889_000A_u32 as i32;
const AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED: i32 = 0x8889_000E_u32 as i32;

/// Maps a failed WASAPI call's HRESULT to the reason shown to the user.
pub(crate) fn classify_failure(hresult: i32) -> OutputFallbackReason {
    match hresult {
        AUDCLNT_E_DEVICE_IN_USE => OutputFallbackReason::DeviceInUse,
        AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED => OutputFallbackReason::ExclusiveModeDisabled,
        AUDCLNT_E_UNSUPPORTED_FORMAT => OutputFallbackReason::FormatUnsupported,
        _ => OutputFallbackReason::Unavailable,
    }
}

/// The buffer period, in 100 ns units, that holds exactly `frames` at `rate`:
/// what `Initialize` must be retried with after `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED`.
pub(crate) fn aligned_period_hns(frames: u32, rate: u32) -> i64 {
    (10_000_000.0 * frames as f64 / rate as f64).round() as i64
}

// `IAudioEndpointVolume::QueryHardwareSupport` bits, from endpointvolume.h.
const ENDPOINT_HARDWARE_SUPPORT_VOLUME: u32 = 0x1;
const ENDPOINT_HARDWARE_SUPPORT_MUTE: u32 = 0x2;

/// What an endpoint's volume controls do in device hardware (#1891).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HardwareVolumeSupport {
    /// Whether the endpoint also mutes in hardware.
    pub(crate) mute: bool,
}

impl HardwareVolumeSupport {
    /// Reads `QueryHardwareSupport`'s mask. `None` when the endpoint has no
    /// hardware volume: Windows then applies the endpoint volume in its audio
    /// engine, which an exclusive stream bypasses, so the app must keep its
    /// own software gain.
    pub(crate) fn from_mask(mask: u32) -> Option<Self> {
        (mask & ENDPOINT_HARDWARE_SUPPORT_VOLUME != 0).then_some(Self {
            mute: mask & ENDPOINT_HARDWARE_SUPPORT_MUTE != 0,
        })
    }
}

/// What to write to the endpoint for a master volume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum EndpointSetting {
    /// Mute, leaving the level where it is, so unmuting from Windows
    /// returns to it.
    Mute,
    /// Set the scalar level (Windows' audio taper, the scale its own volume
    /// slider shows), unmuting first when the endpoint mutes in hardware.
    Level { level: f32, unmute: bool },
}

/// The endpoint setting for master volume `vol`. Volume 0 mutes where the
/// endpoint can, since its lowest level is not reliably silent.
pub(crate) fn endpoint_setting(vol: f32, support: HardwareVolumeSupport) -> EndpointSetting {
    if vol <= 0.0 && support.mute {
        EndpointSetting::Mute
    } else {
        EndpointSetting::Level {
            level: vol.clamp(0.0, 1.0),
            unmute: support.mute,
        }
    }
}

/// Event context Luminous passes with every endpoint volume or mute write,
/// so its own changes can be told apart from external ones in
/// `IAudioEndpointVolumeCallback::OnNotify`.
pub(crate) const LUMINOUS_EVENT_CONTEXT: u128 = 0x6c1f4b0e_3a52_4d8e_9b7a_51d2c0e8f314;

/// The master volume a volume notification reports, or `None` when it is
/// the echo of Luminous's own write. Muted reads as 0.
pub(crate) fn external_volume(event_context: u128, level: f32, muted: bool) -> Option<f32> {
    if event_context == LUMINOUS_EVENT_CONTEXT {
        None
    } else if muted {
        Some(0.0)
    } else {
        Some(level.clamp(0.0, 1.0))
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use parking_lot::Mutex;
    use windows::core::{implement, Error, GUID, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, E_FAIL, HANDLE, S_OK, WAIT_OBJECT_0};
    use windows::Win32::Media::Audio::Endpoints::{
        IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
    };
    use windows::Win32::Media::Audio::{
        IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_SHAREMODE_EXCLUSIVE,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDIO_VOLUME_NOTIFICATION_DATA, WAVEFORMATEX,
        WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
    };
    use windows::Win32::Media::KernelStreaming::{
        KSDATAFORMAT_SUBTYPE_PCM, SPEAKER_FRONT_LEFT, SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{
        AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, SetEvent,
        WaitForMultipleObjects, WaitForSingleObject, INFINITE,
    };

    use super::{
        aligned_period_hns, classify_failure, endpoint_setting, external_volume, EndpointSetting,
        HardwareVolumeSupport, LUMINOUS_EVENT_CONTEXT,
    };
    use crate::pcm::{self, PcmFormat};
    use crate::sink::{
        negotiate_format, AudioBackendKind, AudioSink, OpenError, OutputFallbackReason,
    };
    use crate::{AudioShared, OutputRenderer};

    const CHANNELS: u16 = 2;
    /// How long opening may take before the build gives up. With
    /// `ENDPOINT_VOLUME_TIMEOUT` after it, the build stays inside the
    /// engine's 5 s rebuild budget.
    const OPEN_TIMEOUT: Duration = Duration::from_secs(4);
    /// A started stream that signals nothing for this long has stalled.
    const STALL_TIMEOUT_MS: u32 = 2_000;
    /// How long taking over the endpoint volume may take before the sink
    /// settles for software gain.
    const ENDPOINT_VOLUME_TIMEOUT: Duration = Duration::from_millis(500);

    /// A Win32 event handle, closed on drop.
    struct Event(HANDLE);

    // Safety: an event handle is a kernel object reference, usable from any thread.
    unsafe impl Send for Event {}
    unsafe impl Sync for Event {}

    impl Event {
        fn new() -> Result<Self, Error> {
            unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.map(Self)
        }
    }

    impl Drop for Event {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    /// What `play`/`pause`/drop ask of the render thread.
    struct Control {
        playing: AtomicBool,
        quit: AtomicBool,
        wake: Event,
    }

    impl Control {
        fn signal(&self) {
            unsafe {
                let _ = SetEvent(self.wake.0);
            }
        }
    }

    /// Output through a WASAPI endpoint opened in exclusive mode.
    pub struct WasapiExclusiveSink {
        control: Arc<Control>,
        thread: Option<JoinHandle<()>>,
        sample_rate: u32,
        format: PcmFormat,
        device_name: Option<String>,
        stream_error_flag: Arc<AtomicBool>,
        /// Present while the endpoint's hardware volume is the master volume.
        endpoint_volume: Option<EndpointVolume>,
    }

    impl AudioSink for WasapiExclusiveSink {
        fn play(&self) -> Result<(), String> {
            self.control.playing.store(true, Ordering::Release);
            self.control.signal();
            Ok(())
        }

        fn pause(&self) -> Result<(), String> {
            self.control.playing.store(false, Ordering::Release);
            self.control.signal();
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
            AudioBackendKind::WasapiExclusive
        }

        fn output_format(&self) -> &'static str {
            self.format.label()
        }

        fn is_errored(&self) -> bool {
            self.stream_error_flag.load(Ordering::Relaxed)
        }

        fn has_hardware_volume(&self) -> bool {
            self.endpoint_volume.is_some()
        }

        fn set_hardware_volume(&self, volume: f32) {
            if let Some(endpoint_volume) = &self.endpoint_volume {
                endpoint_volume.set_volume(volume);
            }
        }
    }

    impl Drop for WasapiExclusiveSink {
        fn drop(&mut self) {
            self.control.quit.store(true, Ordering::Release);
            self.control.signal();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            // Only once the stream has stopped: restoring the endpoint's
            // earlier (often higher) level any sooner would play the last
            // period at it.
            drop(self.endpoint_volume.take());
        }
    }

    /// What an [`EndpointVolume`], its thread and its change listener share.
    struct VolumeControl {
        /// The master volume to write to the endpoint, as `f32` bits.
        target: AtomicU32,
        quit: AtomicBool,
        wake: Event,
    }

    impl VolumeControl {
        /// Asks the volume thread to write `volume` to the endpoint.
        fn request(&self, volume: f32) {
            self.target.store(volume.to_bits(), Ordering::Release);
            self.signal();
        }

        fn target(&self) -> f32 {
            f32::from_bits(self.target.load(Ordering::Acquire))
        }

        fn signal(&self) {
            unsafe {
                let _ = SetEvent(self.wake.0);
            }
        }
    }

    /// The endpoint's hardware master volume, driven from a thread this owns
    /// (volume calls are cross-process, so they stay off the render thread,
    /// and the engine thread's COM apartment is unknown). While it lives,
    /// the engine's software gain is off; dropping it puts the endpoint's
    /// level and mute back as they were and turns software gain back on.
    struct EndpointVolume {
        control: Arc<VolumeControl>,
        thread: Option<JoinHandle<()>>,
        shared: Arc<AudioShared>,
    }

    impl EndpointVolume {
        /// Takes over `endpoint_id`'s hardware volume at the engine's current
        /// volume, or `None` when the endpoint has no hardware volume (see
        /// [`HardwareVolumeSupport::from_mask`]) or it can't be reached; the
        /// engine then keeps its software gain.
        fn open(endpoint_id: String, shared: &Arc<AudioShared>) -> Option<Self> {
            let control = Arc::new(VolumeControl {
                target: AtomicU32::new(shared.current_volume().to_bits()),
                quit: AtomicBool::new(false),
                wake: Event::new().ok()?,
            });
            let (ready_tx, ready_rx) = mpsc::channel();
            let thread = {
                let (shared, control) = (Arc::clone(shared), Arc::clone(&control));
                std::thread::Builder::new()
                    .name("luminous-wasapi-volume".into())
                    .spawn(move || {
                        endpoint_volume_thread(&endpoint_id, &shared, &control, ready_tx)
                    })
                    .ok()?
            };
            match ready_rx.recv_timeout(ENDPOINT_VOLUME_TIMEOUT) {
                Ok(true) => {
                    shared.hardware_volume_active.store(true, Ordering::Release);
                    shared.output_hardware_volume.store(true, Ordering::Release);
                    Some(Self {
                        control,
                        thread: Some(thread),
                        shared: Arc::clone(shared),
                    })
                }
                Ok(false) => {
                    let _ = thread.join();
                    None
                }
                Err(_) => {
                    log::warn!("Timed out taking over the endpoint volume; using software gain");
                    // The thread puts the endpoint back and exits once its
                    // call returns.
                    control.quit.store(true, Ordering::Release);
                    control.signal();
                    None
                }
            }
        }

        fn set_volume(&self, volume: f32) {
            self.control.request(volume);
        }
    }

    impl Drop for EndpointVolume {
        fn drop(&mut self) {
            self.control.quit.store(true, Ordering::Release);
            self.control.signal();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            self.shared
                .hardware_volume_active
                .store(false, Ordering::Release);
            self.shared
                .output_hardware_volume
                .store(false, Ordering::Release);
        }
    }

    /// Runs an [`EndpointVolume`]: reports on `ready` whether the endpoint
    /// has hardware volume and, if it does, writes `control`'s target to it
    /// on each signal until `quit`, then puts the endpoint back.
    fn endpoint_volume_thread(
        endpoint_id: &str,
        shared: &Arc<AudioShared>,
        control: &Arc<VolumeControl>,
        ready: mpsc::Sender<bool>,
    ) {
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        // Scoped so every COM object is released before CoUninitialize.
        {
            match unsafe { HardwareEndpoint::open(endpoint_id) } {
                None => {
                    let _ = ready.send(false);
                }
                // `open` gave up waiting: leave the endpoint alone.
                Some(_) if control.quit.load(Ordering::Acquire) => {}
                Some(endpoint) => unsafe {
                    // Set before the stream can start, so nothing plays at
                    // the level Windows was left at.
                    endpoint.write(control.target());
                    let listener: IAudioEndpointVolumeCallback = ExternalVolumeListener {
                        shared: Arc::clone(shared),
                        control: Arc::clone(control),
                    }
                    .into();
                    let listening = endpoint
                        .volume
                        .RegisterControlChangeNotify(&listener)
                        .is_ok();
                    if !listening {
                        log::warn!("Endpoint volume change notifications unavailable");
                    }
                    let _ = ready.send(true);
                    while !control.quit.load(Ordering::Acquire) {
                        WaitForSingleObject(control.wake.0, INFINITE);
                        if !control.quit.load(Ordering::Acquire) {
                            endpoint.write(control.target());
                        }
                    }
                    // Stop listening first, so the restore isn't taken for an
                    // external change.
                    if listening {
                        let _ = endpoint.volume.UnregisterControlChangeNotify(&listener);
                    }
                    endpoint.restore();
                },
            }
        }
        if com.is_ok() {
            unsafe { CoUninitialize() };
        }
    }

    /// Passed with every endpoint write, marking it as Luminous's own.
    const OWN_CHANGE: GUID = GUID::from_u128(LUMINOUS_EVENT_CONTEXT);

    /// An endpoint whose volume works in hardware, with the level and mute
    /// it had when opened.
    struct HardwareEndpoint {
        volume: IAudioEndpointVolume,
        support: HardwareVolumeSupport,
        saved_level: Option<f32>,
        saved_mute: Option<bool>,
    }

    impl HardwareEndpoint {
        /// `None` when the endpoint can't be reached or its volume is not in
        /// hardware.
        unsafe fn open(endpoint_id: &str) -> Option<Self> {
            let unusable = |context: &str, e: Error| {
                log::info!("No endpoint hardware volume ({context}: {e}); using software gain");
            };
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| unusable("device enumerator", e))
                    .ok()?;
            let device = enumerator
                .GetDevice(&HSTRING::from(endpoint_id))
                .map_err(|e| unusable("endpoint lookup", e))
                .ok()?;
            let volume: IAudioEndpointVolume = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| unusable("activate", e))
                .ok()?;
            let mask = volume
                .QueryHardwareSupport()
                .map_err(|e| unusable("QueryHardwareSupport", e))
                .ok()?;
            let Some(support) = HardwareVolumeSupport::from_mask(mask) else {
                log::info!(
                    "Endpoint volume is not in hardware (support mask {mask:#x}); using software gain"
                );
                return None;
            };
            let saved_level = volume.GetMasterVolumeLevelScalar().ok();
            let saved_mute = if support.mute {
                volume.GetMute().ok().map(|m| m.as_bool())
            } else {
                None
            };
            Some(Self {
                volume,
                support,
                saved_level,
                saved_mute,
            })
        }

        /// Sets the endpoint for master volume `vol` (see [`endpoint_setting`]).
        unsafe fn write(&self, vol: f32) {
            match endpoint_setting(vol, self.support) {
                EndpointSetting::Mute => {
                    let _ = self.volume.SetMute(true, &OWN_CHANGE);
                }
                EndpointSetting::Level { level, unmute } => {
                    let _ = self.volume.SetMasterVolumeLevelScalar(level, &OWN_CHANGE);
                    if unmute {
                        let _ = self.volume.SetMute(false, &OWN_CHANGE);
                    }
                }
            }
        }

        /// Puts back the level and mute the endpoint had when opened.
        unsafe fn restore(&self) {
            if let Some(level) = self.saved_level {
                let _ = self.volume.SetMasterVolumeLevelScalar(level, &OWN_CHANGE);
            }
            if let Some(muted) = self.saved_mute {
                let _ = self.volume.SetMute(muted, &OWN_CHANGE);
            }
        }
    }

    /// Hands endpoint volume changes made outside the app (a DAC knob, the
    /// Windows volume keys or flyout) to the engine. Windows calls it on its
    /// own thread, so it never calls back into the endpoint: when the safety
    /// ceiling keeps a lower level than the device reports, the volume
    /// thread writes that level back.
    #[implement(IAudioEndpointVolumeCallback)]
    struct ExternalVolumeListener {
        shared: Arc<AudioShared>,
        control: Arc<VolumeControl>,
    }

    impl IAudioEndpointVolumeCallback_Impl for ExternalVolumeListener_Impl {
        fn OnNotify(
            &self,
            notify: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
        ) -> windows::core::Result<()> {
            let Some(data) = (unsafe { notify.as_ref() }) else {
                return Ok(());
            };
            let Some(reading) = external_volume(
                data.guidEventContext.to_u128(),
                data.fMasterVolume,
                data.bMuted.as_bool(),
            ) else {
                return Ok(());
            };
            let kept = self.shared.accept_external_volume(reading);
            if kept < reading {
                self.control.request(kept);
            }
            Ok(())
        }
    }

    /// Opens `endpoint_id` (an `IMMDevice` id) exclusively and starts its
    /// render thread, paused. Fails with the reason to report when the
    /// device is busy, disallows exclusive mode, or takes no stereo integer
    /// format near `requested_rate` (see [`negotiate_format`]).
    pub(crate) fn build_exclusive_sink(
        endpoint_id: String,
        requested_rate: u32,
        device_name: Option<String>,
        shared: &Arc<AudioShared>,
        consumer: Arc<Mutex<ringbuf::HeapCons<f32>>>,
        played_samples: Arc<AtomicU64>,
    ) -> Result<WasapiExclusiveSink, OpenError> {
        let unavailable = |message: String| OpenError {
            reason: OutputFallbackReason::Unavailable,
            message,
        };
        let control = Arc::new(Control {
            playing: AtomicBool::new(false),
            quit: AtomicBool::new(false),
            wake: Event::new().map_err(|e| unavailable(format!("CreateEvent failed: {e}")))?,
        });
        let stream_error_flag = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();

        let thread = {
            let endpoint_id = endpoint_id.clone();
            let shared = Arc::clone(shared);
            let control = Arc::clone(&control);
            let error_flag = Arc::clone(&stream_error_flag);
            std::thread::Builder::new()
                .name("luminous-wasapi-render".into())
                .spawn(move || {
                    render_thread(
                        // Called once the thread has joined the MTA.
                        || unsafe { ExclusiveStream::open(&endpoint_id, requested_rate) },
                        shared,
                        consumer,
                        played_samples,
                        &control,
                        &error_flag,
                        ready_tx,
                    )
                })
                .map_err(|e| unavailable(format!("Failed to spawn render thread: {e}")))?
        };

        match ready_rx.recv_timeout(OPEN_TIMEOUT) {
            // The stream is paused until `play`, so the endpoint volume is
            // in place before anything plays.
            Ok(Ok((sample_rate, format))) => Ok(WasapiExclusiveSink {
                control,
                thread: Some(thread),
                sample_rate,
                format,
                device_name,
                stream_error_flag,
                endpoint_volume: EndpointVolume::open(endpoint_id, shared),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                // Leave the thread to exit on its own once its call returns.
                control.quit.store(true, Ordering::Release);
                control.signal();
                Err(unavailable("Timed out opening exclusive stream".into()))
            }
        }
    }

    fn render_thread(
        open: impl FnOnce() -> Result<ExclusiveStream, OpenError>,
        shared: Arc<AudioShared>,
        consumer: Arc<Mutex<ringbuf::HeapCons<f32>>>,
        played_samples: Arc<AtomicU64>,
        control: &Control,
        error_flag: &AtomicBool,
        ready: mpsc::Sender<Result<(u32, PcmFormat), OpenError>>,
    ) {
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        // Scoped so every COM object is released before CoUninitialize.
        {
            match open() {
                Err(e) => {
                    let _ = ready.send(Err(e));
                }
                Ok(stream) => {
                    let buffer_samples = stream.buffer_frames as usize * CHANNELS as usize;
                    let period = RenderPeriod {
                        renderer: OutputRenderer {
                            shared,
                            consumer,
                            played_samples,
                            mono_scratch: Vec::with_capacity(buffer_samples),
                            sample_rate: stream.rate,
                            channels: CHANNELS,
                        },
                        scratch: vec![0.0; buffer_samples],
                        format: stream.format,
                    };
                    let _ = ready.send(Ok((stream.rate, stream.format)));
                    stream.run(period, control, error_flag);
                }
            }
        }
        if com.is_ok() {
            unsafe { CoUninitialize() };
        }
    }

    fn wave_format(rate: u32, format: PcmFormat) -> WAVEFORMATEXTENSIBLE {
        let block_align = CHANNELS * format.container_bytes() as u16;
        WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
                nChannels: CHANNELS,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * block_align as u32,
                nBlockAlign: block_align,
                wBitsPerSample: 8 * format.container_bytes() as u16,
                cbSize: (std::mem::size_of::<WAVEFORMATEXTENSIBLE>()
                    - std::mem::size_of::<WAVEFORMATEX>()) as u16,
            },
            Samples: WAVEFORMATEXTENSIBLE_0 {
                wValidBitsPerSample: format.valid_bits(),
            },
            dwChannelMask: SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
            SubFormat: KSDATAFORMAT_SUBTYPE_PCM,
        }
    }

    fn open_error(context: &str, e: Error) -> OpenError {
        OpenError {
            reason: classify_failure(e.code().0),
            message: format!("{context}: {e}"),
        }
    }

    /// An initialised, not yet started exclusive stream.
    struct ExclusiveStream {
        client: IAudioClient,
        render: IAudioRenderClient,
        event: Event,
        buffer_frames: u32,
        rate: u32,
        format: PcmFormat,
    }

    impl ExclusiveStream {
        unsafe fn open(endpoint_id: &str, requested_rate: u32) -> Result<Self, OpenError> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| open_error("Device enumerator", e))?;
            let device = enumerator
                .GetDevice(&HSTRING::from(endpoint_id))
                .map_err(|e| open_error("Endpoint lookup", e))?;
            let activate = || -> Result<IAudioClient, OpenError> {
                device
                    .Activate(CLSCTX_ALL, None)
                    .map_err(|e| open_error("Activate", e))
            };
            let client = activate()?;

            let mix = client
                .GetMixFormat()
                .map_err(|e| open_error("GetMixFormat", e))?;
            let mix_rate = (*mix).nSamplesPerSec;
            CoTaskMemFree(Some(mix as *const _));

            // A refusal other than "unsupported format" (device in use,
            // exclusive mode disallowed) is the better reason to report.
            let mut refusal = super::AUDCLNT_E_UNSUPPORTED_FORMAT;
            let (rate, format) = negotiate_format(
                requested_rate,
                &[mix_rate],
                &PcmFormat::PREFERENCE,
                |r, f| {
                    let wf = wave_format(r, f);
                    let hr =
                        client.IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, &wf.Format, None);
                    if hr != S_OK && hr.0 != super::AUDCLNT_E_UNSUPPORTED_FORMAT {
                        refusal = hr.0;
                    }
                    hr == S_OK
                },
            )
            .ok_or_else(|| OpenError {
                reason: classify_failure(refusal),
                message: format!(
                    "No exclusive stereo format near {requested_rate} Hz or at {mix_rate} Hz"
                ),
            })?;
            let wf = wave_format(rate, format);

            let mut period = 0i64;
            client
                .GetDevicePeriod(Some(&mut period), None)
                .map_err(|e| open_error("GetDevicePeriod", e))?;
            let init = |client: &IAudioClient, period: i64| {
                client.Initialize(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    period,
                    period,
                    &wf.Format,
                    None,
                )
            };
            let client = match init(&client, period) {
                Ok(()) => client,
                Err(e) if e.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
                    // The device wants a period that is a whole number of its
                    // own frames: retry on a fresh client with that period.
                    let frames = client
                        .GetBufferSize()
                        .map_err(|e| open_error("GetBufferSize", e))?;
                    drop(client);
                    let client = activate()?;
                    init(&client, aligned_period_hns(frames, rate))
                        .map_err(|e| open_error("Initialize (aligned)", e))?;
                    client
                }
                Err(e) => return Err(open_error("Initialize", e)),
            };

            let event = Event::new().map_err(|e| open_error("CreateEvent", e))?;
            client
                .SetEventHandle(event.0)
                .map_err(|e| open_error("SetEventHandle", e))?;
            let buffer_frames = client
                .GetBufferSize()
                .map_err(|e| open_error("GetBufferSize", e))?;
            let render: IAudioRenderClient = client
                .GetService()
                .map_err(|e| open_error("GetService", e))?;

            log::info!(
                "WASAPI exclusive stream open: {rate} Hz, {}, {buffer_frames}-frame buffer",
                format.label()
            );
            Ok(Self {
                client,
                render,
                event,
                buffer_frames,
                rate,
                format,
            })
        }

        /// Runs until asked to quit or the device fails; a failure sets
        /// `error_flag`, which makes the engine rebuild its output.
        fn run(self, mut period: RenderPeriod, control: &Control, error_flag: &AtomicBool) {
            let mut task_index = 0u32;
            let mmcss = unsafe {
                AvSetMmThreadCharacteristicsW(&HSTRING::from("Pro Audio"), &mut task_index)
            }
            .ok();
            let handles = [self.event.0, control.wake.0];
            let mut started = false;

            let failure = loop {
                if control.quit.load(Ordering::Acquire) {
                    break None;
                }
                let want_playing = control.playing.load(Ordering::Acquire);
                if want_playing && !started {
                    // Exclusive mode plays whatever is in the buffer at Start,
                    // so fill it first.
                    let start = unsafe {
                        period
                            .fill(&self.render, self.buffer_frames)
                            .and_then(|()| self.client.Start())
                    };
                    if let Err(e) = start {
                        break Some(e);
                    }
                    started = true;
                } else if !want_playing && started {
                    unsafe {
                        let _ = self.client.Stop();
                        // Drop the queued period so a later start doesn't
                        // replay audio from before a seek or track change.
                        let _ = self.client.Reset();
                    }
                    started = false;
                }

                let wait = unsafe { WaitForMultipleObjects(&handles, false, STALL_TIMEOUT_MS) };
                if !started {
                    continue;
                }
                if wait == WAIT_OBJECT_0 {
                    if let Err(e) = unsafe { period.fill(&self.render, self.buffer_frames) } {
                        break Some(e);
                    }
                } else if wait.0 != WAIT_OBJECT_0.0 + 1 {
                    break Some(Error::new(E_FAIL, "Exclusive stream stopped signalling"));
                }
            };

            if started {
                unsafe {
                    let _ = self.client.Stop();
                }
            }
            if let Some(e) = failure {
                log::error!("WASAPI exclusive stream failed: {e}");
                error_flag.store(true, Ordering::Relaxed);
            }
            if let Some(handle) = mmcss {
                unsafe {
                    let _ = AvRevertMmThreadCharacteristics(handle);
                }
            }
        }
    }

    /// One device period's worth of output: renders through the engine's
    /// gain chain and converts into the device buffer. Runs on the real-time
    /// render thread: never allocates or blocks.
    struct RenderPeriod {
        renderer: OutputRenderer,
        /// Interleaved float scratch, sized to the device buffer up front.
        scratch: Vec<f32>,
        format: PcmFormat,
    }

    impl RenderPeriod {
        unsafe fn fill(&mut self, render: &IAudioRenderClient, frames: u32) -> Result<(), Error> {
            let data = render.GetBuffer(frames)?;
            let samples = (frames as usize * CHANNELS as usize).min(self.scratch.len());
            let out = &mut self.scratch[..samples];
            self.renderer.render(out);
            let bytes =
                std::slice::from_raw_parts_mut(data, samples * self.format.container_bytes());
            pcm::write_interleaved(out, self.format, bytes);
            render.ReleaseBuffer(frames, 0)
        }
    }
}

#[cfg(windows)]
pub(crate) use imp::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_map_to_the_reason_shown_to_the_user() {
        assert_eq!(
            classify_failure(AUDCLNT_E_DEVICE_IN_USE),
            OutputFallbackReason::DeviceInUse
        );
        assert_eq!(
            classify_failure(AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED),
            OutputFallbackReason::ExclusiveModeDisabled
        );
        assert_eq!(
            classify_failure(AUDCLNT_E_UNSUPPORTED_FORMAT),
            OutputFallbackReason::FormatUnsupported
        );
        // AUDCLNT_E_DEVICE_INVALIDATED
        assert_eq!(
            classify_failure(0x8889_0004_u32 as i32),
            OutputFallbackReason::Unavailable
        );
    }

    #[test]
    fn aligned_period_covers_exactly_the_aligned_frames() {
        // 448 frames at 44.1 kHz ≈ 10.159 ms.
        assert_eq!(aligned_period_hns(448, 44_100), 101_587);
        assert_eq!(aligned_period_hns(480, 48_000), 100_000);
    }

    #[test]
    fn hardware_volume_needs_the_volume_bit() {
        assert_eq!(HardwareVolumeSupport::from_mask(0), None);
        // Mute or a peak meter alone leaves the level to the audio engine,
        // which an exclusive stream bypasses.
        assert_eq!(
            HardwareVolumeSupport::from_mask(ENDPOINT_HARDWARE_SUPPORT_MUTE | 0x4),
            None
        );
        assert_eq!(
            HardwareVolumeSupport::from_mask(ENDPOINT_HARDWARE_SUPPORT_VOLUME),
            Some(HardwareVolumeSupport { mute: false })
        );
        assert_eq!(
            HardwareVolumeSupport::from_mask(0x7),
            Some(HardwareVolumeSupport { mute: true })
        );
    }

    #[test]
    fn zero_volume_mutes_where_the_endpoint_can() {
        let mutes = HardwareVolumeSupport { mute: true };
        let no_mute = HardwareVolumeSupport { mute: false };
        assert_eq!(endpoint_setting(0.0, mutes), EndpointSetting::Mute);
        assert_eq!(
            endpoint_setting(0.0, no_mute),
            EndpointSetting::Level {
                level: 0.0,
                unmute: false
            }
        );
        assert_eq!(
            endpoint_setting(0.4, mutes),
            EndpointSetting::Level {
                level: 0.4,
                unmute: true
            }
        );
        assert_eq!(
            endpoint_setting(1.5, no_mute),
            EndpointSetting::Level {
                level: 1.0,
                unmute: false
            }
        );
    }

    #[test]
    fn own_writes_are_not_external_changes() {
        assert_eq!(external_volume(LUMINOUS_EVENT_CONTEXT, 0.3, false), None);
        assert_eq!(external_volume(LUMINOUS_EVENT_CONTEXT, 0.3, true), None);
        // GUID_NULL: a hardware knob or a driver.
        assert_eq!(external_volume(0, 0.3, false), Some(0.3));
        assert_eq!(external_volume(0, 0.3, true), Some(0.0));
    }
}
