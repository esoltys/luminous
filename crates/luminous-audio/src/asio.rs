//! ASIO audio sink implementation using pure-Rust `azo`.
//!
//! Provides direct, bit-perfect streaming to ASIO hardware devices without
//! requiring Steinberg's C++ SDK or C++ compilers in CI.

#[cfg(windows)]
mod imp {
    use parking_lot::Mutex;
    use std::ffi::{c_long, c_void};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use azo::driver::{Driver, Metadata, Proxy};
    use azo::dto::ChannelId;
    use azo::sys::*;

    use crate::pcm::{self, AsioSampleFormat};
    use crate::sink::{AudioBackendKind, AudioSink};
    use crate::{AudioShared, OutputRenderer};

    struct AsioActiveState {
        renderer: OutputRenderer,
        interleaved_scratch: Vec<f32>,
        left_buf: [*mut u8; 2],
        right_buf: [*mut u8; 2],
        buffer_size: usize,
        sample_format: AsioSampleFormat,
        driver: Arc<Proxy>,
        error_flag: Arc<AtomicBool>,
    }

    // Safety: pointers in AsioActiveState refer to fixed ASIO double buffers
    // allocated by the driver that persist until dispose_buffers is called.
    unsafe impl Send for AsioActiveState {}

    static ACTIVE_ASIO: Mutex<Option<AsioActiveState>> = Mutex::new(None);

    static CALLBACKS: Callbacks = Callbacks {
        buffer_switch,
        sample_rate_did_change,
        asio_message,
        buffer_switch_time_info,
    };

    fn on_buffer_switch(buffer_index: usize) {
        if buffer_index > 1 {
            return;
        }
        if let Some(mut guard) = ACTIVE_ASIO.try_lock() {
            if let Some(state) = guard.as_mut() {
                let frames = state.buffer_size;
                let needed_samples = frames * 2;
                if state.interleaved_scratch.len() < needed_samples {
                    return;
                }
                state
                    .renderer
                    .render(&mut state.interleaved_scratch[..needed_samples]);
                unsafe {
                    pcm::convert_and_deinterleave(
                        &state.interleaved_scratch[..needed_samples],
                        state.sample_format,
                        state.left_buf[buffer_index],
                        state.right_buf[buffer_index],
                        frames,
                    );
                }
                let _ = state.driver.output_ready();
            }
        }
    }

    unsafe extern "system" fn buffer_switch(buffer_index: c_long, _direct_process: Bool) {
        on_buffer_switch(buffer_index as usize);
    }

    unsafe extern "system" fn buffer_switch_time_info(
        time_info: *mut Time,
        buffer_index: c_long,
        _direct_process: Bool,
    ) -> *mut Time {
        on_buffer_switch(buffer_index as usize);
        time_info
    }

    /// Whether a rate the driver reports means the hardware no longer runs at
    /// the stream's `stream_rate`. A report of the rate we set (some drivers
    /// echo our own `set_sample_rate`) or an unusable value is not a change.
    fn is_external_rate_change(reported: f64, stream_rate: u32) -> bool {
        reported.is_finite() && reported > 0.0 && reported.round() as u32 != stream_rate
    }

    /// The rate was changed outside Luminous (driver control panel, another
    /// app, the DAC re-locking its clock): samples rendered for the old rate
    /// would now play at the wrong speed and pitch. Flag the stream errored so
    /// the decode loop rebuilds it at the track's rate, as for a reset request.
    unsafe extern "system" fn sample_rate_did_change(rate: SampleRate) {
        log::info!("ASIO driver reported hardware sample rate change: {rate}");
        // Not the buffer callback, so waiting briefly is fine; the timeout
        // keeps a driver that calls this from inside `buffer_switch` from
        // deadlocking on the lock that callback holds.
        let Some(guard) = ACTIVE_ASIO.try_lock_for(std::time::Duration::from_millis(50)) else {
            log::warn!("ASIO rate change not applied: stream state busy");
            return;
        };
        if let Some(state) = guard.as_ref() {
            if is_external_rate_change(rate, state.renderer.sample_rate) {
                log::warn!(
                    "ASIO hardware rate is now {rate} Hz, stream was opened at {} Hz; reopening",
                    state.renderer.sample_rate
                );
                state.error_flag.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Whether this host handles `selector`. Drivers ask before sending a
    /// message (FlexASIO, for one, never sends a reset request otherwise), so
    /// an unanswered query silently drops the driver's reset requests.
    fn handles_message(selector: MessageSelector) -> bool {
        matches!(
            selector,
            MessageSelector::ENGINE_VERSION
                | MessageSelector::RESET_REQUEST
                | MessageSelector::RESYNC_REQUEST
        )
    }

    unsafe extern "system" fn asio_message(
        selector: MessageSelector,
        value: c_long,
        _message: *const c_void,
        _opt: *const f64,
    ) -> c_long {
        match selector {
            MessageSelector::SELECTOR_SUPPORTED => {
                c_long::from(handles_message(MessageSelector(value)))
            }
            MessageSelector::ENGINE_VERSION => 2,
            MessageSelector::RESET_REQUEST => {
                log::warn!("ASIO driver requested subsystem reset");
                if let Some(guard) = ACTIVE_ASIO.try_lock() {
                    if let Some(state) = guard.as_ref() {
                        state.error_flag.store(true, Ordering::Relaxed);
                    }
                }
                1
            }
            MessageSelector::RESYNC_REQUEST => 1,
            _ => 0,
        }
    }

    pub fn list_asio_drivers() -> Vec<(String, String)> {
        match Metadata::enumerate() {
            Ok(drivers) => drivers
                .into_iter()
                .map(|m| {
                    let desc = m.description.to_string_lossy();
                    let name = if desc.is_empty() {
                        "Unknown ASIO Driver".to_string()
                    } else {
                        desc
                    };
                    let clsid = format!("{:?}", m.clsid);
                    (name, clsid)
                })
                .collect(),
            Err(e) => {
                log::warn!("Failed to enumerate ASIO drivers: {e:?}");
                Vec::new()
            }
        }
    }

    pub fn open_asio_control_panel_by_clsid(clsid_str: &str) -> Result<(), String> {
        let all =
            Metadata::enumerate().map_err(|e| format!("Registry enumeration failed: {e:?}"))?;
        let meta = if clsid_str.is_empty() {
            all.into_iter()
                .next()
                .ok_or_else(|| "No ASIO drivers registered on this system".to_string())?
        } else {
            all.into_iter()
                .find(|m| {
                    format!("{:?}", m.clsid) == clsid_str
                        || m.description.to_string_lossy() == clsid_str
                })
                .ok_or_else(|| format!("ASIO driver not found: {clsid_str}"))?
        };

        let host = azo::Host::new();
        let proxy = host
            .create_driver(&meta.clsid)
            .map_err(|e| format!("Failed to create driver instance: {e:?}"))?;
        if !proxy.init(None) {
            return Err("ASIO driver initialization failed".to_string());
        }
        proxy
            .open_control_panel()
            .map_err(|e| format!("Failed to open driver control panel: {e:?}"))
    }

    pub struct AsioSink {
        pub driver: Arc<Proxy>,
        pub sample_rate: u32,
        pub channels: u16,
        pub device_name: String,
        pub sample_format: AsioSampleFormat,
        pub stream_error_flag: Arc<AtomicBool>,
        pub is_playing: AtomicBool,
    }

    impl AudioSink for AsioSink {
        fn play(&self) -> Result<(), String> {
            self.is_playing.store(true, Ordering::Relaxed);
            self.driver
                .start()
                .map_err(|e| format!("ASIO start failed: {e:?}"))
        }

        fn pause(&self) -> Result<(), String> {
            self.is_playing.store(false, Ordering::Relaxed);
            self.driver
                .stop()
                .map_err(|e| format!("ASIO stop failed: {e:?}"))
        }

        fn sample_rate(&self) -> u32 {
            self.sample_rate
        }

        fn channels(&self) -> u16 {
            self.channels
        }

        fn device_name(&self) -> Option<String> {
            Some(self.device_name.clone())
        }

        fn backend(&self) -> AudioBackendKind {
            AudioBackendKind::Asio
        }

        fn output_format(&self) -> &'static str {
            self.sample_format.label()
        }

        fn is_errored(&self) -> bool {
            self.stream_error_flag.load(Ordering::Relaxed)
        }
    }

    impl Drop for AsioSink {
        fn drop(&mut self) {
            let _ = self.driver.stop();
            *ACTIVE_ASIO.lock() = None;
            let _ = self.driver.dispose_buffers();
        }
    }

    pub(crate) fn build_asio_sink(
        device_id_or_name: Option<&str>,
        shared: &Arc<AudioShared>,
        consumer: Arc<Mutex<ringbuf::HeapCons<f32>>>,
        played_samples: Arc<std::sync::atomic::AtomicU64>,
        target_sample_rate: u32,
    ) -> Result<AsioSink, String> {
        let all = Metadata::enumerate()
            .map_err(|e| format!("Failed to enumerate ASIO drivers: {e:?}"))?;
        if all.is_empty() {
            return Err("No ASIO drivers registered on this system".to_string());
        }

        let meta = if let Some(target) = device_id_or_name {
            all.into_iter()
                .find(|m| {
                    format!("{:?}", m.clsid) == target || m.description.to_string_lossy() == target
                })
                .ok_or_else(|| format!("Requested ASIO driver '{target}' not found"))?
        } else {
            all.into_iter()
                .next()
                .ok_or_else(|| "No ASIO drivers found".to_string())?
        };

        let device_name = {
            let d = meta.description.to_string_lossy();
            if d.is_empty() {
                "ASIO Device".to_string()
            } else {
                d
            }
        };

        let host = azo::Host::new();
        let proxy = host
            .create_driver(&meta.clsid)
            .map_err(|e| format!("Failed to instantiate ASIO driver '{device_name}': {e:?}"))?;

        if !proxy.init(None) {
            return Err(format!("ASIO driver '{device_name}' init() returned false"));
        }

        let channel_counts = proxy
            .channel_counts()
            .map_err(|e| format!("Failed to get channel counts: {e:?}"))?;
        if channel_counts.out < 2 {
            return Err(format!(
                "ASIO driver '{device_name}' does not provide at least 2 output channels (found {})",
                channel_counts.out
            ));
        }

        // The requested rate or the closest one the driver takes, else
        // whatever the hardware is running at.
        let active_rate = crate::sink::rate_candidates(target_sample_rate)
            .into_iter()
            .find(|&rate| {
                proxy.can_sample_rate(rate as f64).is_ok()
                    && proxy.set_sample_rate(rate as f64).is_ok()
            })
            .unwrap_or_else(|| {
                proxy
                    .get_sample_rate()
                    .map(|r| r as u32)
                    .unwrap_or(target_sample_rate)
            });

        let ch0 = ChannelId {
            input: false,
            index: 0,
        };
        let ch1 = ChannelId {
            input: false,
            index: 1,
        };

        let sample_info = proxy
            .channel_info(ch0)
            .map_err(|e| format!("Failed to query channel info: {e:?}"))?;
        let sample_format: AsioSampleFormat = sample_info.sample_type.into();
        if !sample_format.is_supported() {
            return Err(format!(
                "ASIO driver '{device_name}' uses unsupported sample format: {sample_format:?}"
            ));
        }

        let buffer_size_info = proxy
            .buffer_size()
            .map_err(|e| format!("Failed to query buffer size: {e:?}"))?;
        let buffer_size = buffer_size_info.preferred as usize;

        let driver = Arc::new(proxy);

        let callbacks_ptr: *const Callbacks = &raw const CALLBACKS;
        let mut created_buffers =
            unsafe { driver.create_buffers([ch0, ch1], buffer_size as c_long, callbacks_ptr) }
                .map_err(|e| format!("Failed to create ASIO buffers: {e:?}"))?;

        let left_pair = created_buffers
            .next()
            .ok_or_else(|| "Missing buffer pair for channel 0".to_string())?;
        let right_pair = created_buffers
            .next()
            .ok_or_else(|| "Missing buffer pair for channel 1".to_string())?;
        drop(created_buffers);

        let left_buf: [*mut u8; 2] = [left_pair[0].cast(), left_pair[1].cast()];
        let right_buf: [*mut u8; 2] = [right_pair[0].cast(), right_pair[1].cast()];

        let stream_error_flag = Arc::new(AtomicBool::new(false));

        let mono_scratch_size = active_rate as usize * 2 * 2;
        let renderer = OutputRenderer {
            shared: Arc::clone(shared),
            consumer,
            played_samples,
            mono_scratch: Vec::with_capacity(mono_scratch_size),
            sample_rate: active_rate,
            channels: 2,
        };

        let scratch_len = buffer_size * 2;
        let interleaved_scratch = vec![0.0f32; scratch_len];

        *ACTIVE_ASIO.lock() = Some(AsioActiveState {
            renderer,
            interleaved_scratch,
            left_buf,
            right_buf,
            buffer_size,
            sample_format,
            driver: Arc::clone(&driver),
            error_flag: Arc::clone(&stream_error_flag),
        });

        Ok(AsioSink {
            driver,
            sample_rate: active_rate,
            channels: 2,
            device_name,
            sample_format,
            stream_error_flag,
            is_playing: AtomicBool::new(false),
        })
    }

    #[cfg(test)]
    mod tests {
        use super::{handles_message, is_external_rate_change, MessageSelector};

        #[test]
        fn reset_and_resync_requests_are_advertised() {
            assert!(handles_message(MessageSelector::RESET_REQUEST));
            assert!(handles_message(MessageSelector::RESYNC_REQUEST));
            assert!(handles_message(MessageSelector::ENGINE_VERSION));
        }

        #[test]
        fn unhandled_messages_are_not_advertised() {
            // Unanswered, the driver falls back to a reset request instead.
            assert!(!handles_message(MessageSelector::BUFFER_SIZE_CHANGE));
            assert!(!handles_message(MessageSelector::LATENCIES_CHANGED));
            assert!(!handles_message(MessageSelector::SELECTOR_SUPPORTED));
        }

        #[test]
        fn a_different_hardware_rate_is_a_change() {
            assert!(is_external_rate_change(48_000.0, 44_100));
            assert!(is_external_rate_change(96_000.0, 192_000));
        }

        #[test]
        fn the_stream_rate_echoed_back_is_not_a_change() {
            assert!(!is_external_rate_change(44_100.0, 44_100));
            assert!(!is_external_rate_change(44_099.999_9, 44_100));
        }

        #[test]
        fn an_unusable_report_is_not_a_change() {
            assert!(!is_external_rate_change(0.0, 44_100));
            assert!(!is_external_rate_change(-1.0, 44_100));
            assert!(!is_external_rate_change(f64::NAN, 44_100));
        }
    }
}

#[cfg(windows)]
pub use imp::*;

#[cfg(not(windows))]
pub fn list_asio_drivers() -> Vec<(String, String)> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn open_asio_control_panel_by_clsid(_clsid_str: &str) -> Result<(), String> {
    Err("ASIO is only supported on Windows".to_string())
}
