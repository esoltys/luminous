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

/// Why the exclusive stream couldn't be opened.
#[derive(Debug, Clone)]
pub(crate) struct OpenError {
    pub(crate) reason: OutputFallbackReason,
    pub(crate) message: String,
}

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

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use parking_lot::Mutex;
    use windows::core::{Error, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, E_FAIL, HANDLE, S_OK, WAIT_OBJECT_0};
    use windows::Win32::Media::Audio::{
        IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_SHAREMODE_EXCLUSIVE,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
        WAVEFORMATEXTENSIBLE_0,
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
        WaitForMultipleObjects,
    };

    use super::{aligned_period_hns, classify_failure, OpenError};
    use crate::pcm::{self, PcmFormat};
    use crate::sink::{negotiate_format, AudioBackendKind, AudioSink, OutputFallbackReason};
    use crate::{AudioShared, OutputRenderer};

    const CHANNELS: u16 = 2;
    /// How long opening may take before the build gives up (inside the
    /// engine's 5 s rebuild budget).
    const OPEN_TIMEOUT: Duration = Duration::from_secs(4);
    /// A started stream that signals nothing for this long has stalled.
    const STALL_TIMEOUT_MS: u32 = 2_000;

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
        pub(crate) stream_error_flag: Arc<AtomicBool>,
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
    }

    impl Drop for WasapiExclusiveSink {
        fn drop(&mut self) {
            self.control.quit.store(true, Ordering::Release);
            self.control.signal();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
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
            Ok(Ok((sample_rate, format))) => Ok(WasapiExclusiveSink {
                control,
                thread: Some(thread),
                sample_rate,
                format,
                device_name,
                stream_error_flag,
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
}
