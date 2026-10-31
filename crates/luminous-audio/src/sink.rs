//! Audio output sink abstraction and device enumeration.
//!
//! Provides a deep interface decoupling the playback/decode loop from specific
//! audio backends (CPAL shared audio; WASAPI exclusive, ASIO and ALSA
//! hardware bit-perfect output).

use crate::pcm::PcmFormat;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Which output path the engine opens. The serialized names (`cpal`, `asio`,
/// `wasapi_exclusive`, `alsa`) are also how the choice is persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioBackendKind {
    /// Shared system audio through CPAL (on Windows, WASAPI shared mode).
    #[default]
    Cpal,
    Asio,
    /// WASAPI exclusive mode on the same endpoints as `Cpal` (Windows only).
    WasapiExclusive,
    /// A card's `hw:` PCM opened directly, bypassing the sound server (Linux only).
    Alsa,
}

impl AudioBackendKind {
    /// Whether this backend hands the device our samples untouched, with no
    /// system mixer (and so no system volume) in between.
    pub fn bypasses_mixer(self) -> bool {
        self != Self::Cpal
    }
}

impl std::fmt::Display for AudioBackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cpal => write!(f, "CPAL"),
            Self::Asio => write!(f, "ASIO"),
            Self::WasapiExclusive => write!(f, "WASAPI Exclusive"),
            Self::Alsa => write!(f, "ALSA Direct"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub backend: AudioBackendKind,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AudioOutputConfig {
    pub backend: AudioBackendKind,
    pub device_id: Option<String>,
}

impl AudioOutputConfig {
    /// The device shared (CPAL) output should use: the chosen one, which is
    /// the same endpoint when WASAPI exclusive falls back. ASIO's id is a
    /// driver CLSID and ALSA's a `hw:` PCM, which no shared device has, so
    /// their fallback uses the system default.
    pub(crate) fn shared_device_id(&self) -> Option<&str> {
        match self.backend {
            AudioBackendKind::Asio | AudioBackendKind::Alsa => None,
            _ => self.device_id.as_deref(),
        }
    }
}

/// Why a bit-perfect backend couldn't be opened, so the engine plays through
/// shared output instead. Reported to the app in `AudioEvent::OutputFallback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFallbackReason {
    /// Another application holds the device exclusively.
    DeviceInUse,
    /// The device's "Allow applications to take exclusive control" is off.
    ExclusiveModeDisabled,
    /// The device accepts no stereo integer format exclusively.
    FormatUnsupported,
    /// Anything else: the driver failed to load, initialise or start.
    Unavailable,
}

/// Why a bit-perfect sink couldn't be opened: the reason reported to the app,
/// and the detail logged.
#[derive(Debug, Clone)]
pub(crate) struct OpenError {
    pub(crate) reason: OutputFallbackReason,
    pub(crate) message: String,
}

/// Deep abstraction implemented by all output sinks (CPAL, ASIO, WASAPI exclusive, ALSA).
pub trait AudioSink: Send {
    /// Resumes or starts playback on the output device.
    /// Idempotent: the engine calls it on an already-playing sink when a new
    /// track starts.
    fn play(&self) -> Result<(), String>;

    /// Pauses playback on the output device. Idempotent, like `play`.
    fn pause(&self) -> Result<(), String>;

    /// Hardware stream sample rate in Hz.
    fn sample_rate(&self) -> u32;

    /// Number of output channels (typically 2).
    fn channels(&self) -> u16;

    /// Name of the active output device or driver.
    fn device_name(&self) -> Option<String>;

    /// Which backend this sink is; its `Display` is the pipeline's name for it.
    fn backend(&self) -> AudioBackendKind;

    /// The sample format the device receives, for the pipeline display
    /// (e.g. "24-bit integer PCM").
    fn output_format(&self) -> &'static str;

    /// Whether the stream has experienced an unrecoverable device error.
    fn is_errored(&self) -> bool;

    /// Whether volume control occurs in device hardware on this sink.
    fn has_hardware_volume(&self) -> bool {
        false
    }

    /// Sets the volume (0.0 to 1.0) on the sink's hardware mixer, if supported.
    fn set_hardware_volume(&self, _volume: f32) {}
}

/// The rate a bit-perfect device is opened at before any track asks for one:
/// most music is 44.1 kHz.
pub(crate) const DEFAULT_BIT_PERFECT_RATE: u32 = 44_100;

/// The rates to try, best first, when opening a bit-perfect device for a
/// track at `requested` Hz: the rate itself, then each halving of it down to
/// 44.1 kHz (an integer ratio keeps resampling simple, e.g. 352.8 kHz on a
/// 192 kHz DAC plays at 176.4 kHz).
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))] // bit-perfect backends only
pub(crate) fn rate_candidates(requested: u32) -> Vec<u32> {
    std::iter::successors(Some(requested), |&r| {
        (r % 2 == 0 && r / 2 >= 44_100).then_some(r / 2)
    })
    .collect()
}

/// Picks the first rate/format a bit-perfect device accepts: the
/// [`rate_candidates`] for `requested_rate`, then each of `fallback_rates`
/// (e.g. the device's mix rate), each across `formats` in order. Stereo only.
/// `None` when the device accepts none of them.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
pub(crate) fn negotiate_format(
    requested_rate: u32,
    fallback_rates: &[u32],
    formats: &[PcmFormat],
    mut is_supported: impl FnMut(u32, PcmFormat) -> bool,
) -> Option<(u32, PcmFormat)> {
    let mut rates = rate_candidates(requested_rate);
    for &rate in fallback_rates {
        if !rates.contains(&rate) {
            rates.push(rate);
        }
    }
    rates
        .into_iter()
        .flat_map(|rate| formats.iter().map(move |&format| (rate, format)))
        .find(|&(rate, format)| is_supported(rate, format))
}

/// Standard cross-platform output sink backed by CPAL.
pub struct CpalSink {
    stream: cpal::Stream,
    sample_rate: u32,
    channels: u16,
    device_name: Option<String>,
    stream_error_flag: Arc<AtomicBool>,
}

impl CpalSink {
    pub fn new(
        stream: cpal::Stream,
        sample_rate: u32,
        channels: u16,
        device_name: Option<String>,
        stream_error_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            stream,
            sample_rate,
            channels,
            device_name,
            stream_error_flag,
        }
    }
}

impl AudioSink for CpalSink {
    fn play(&self) -> Result<(), String> {
        use cpal::traits::StreamTrait;
        self.stream
            .play()
            .map_err(|e| format!("CPAL stream play failed: {e}"))
    }

    fn pause(&self) -> Result<(), String> {
        use cpal::traits::StreamTrait;
        self.stream
            .pause()
            .map_err(|e| format!("CPAL stream pause failed: {e}"))
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn device_name(&self) -> Option<String> {
        self.device_name.clone()
    }

    fn backend(&self) -> AudioBackendKind {
        AudioBackendKind::Cpal
    }

    fn output_format(&self) -> &'static str {
        "32-bit float PCM"
    }

    fn is_errored(&self) -> bool {
        self.stream_error_flag.load(Ordering::Relaxed)
    }
}

/// Enumerates all available audio output devices across supported backends.
/// Safe to call on any thread; never activates COM audio streams.
pub fn enumerate_output_devices() -> Vec<AudioDeviceInfo> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let mut devices = Vec::new();

    let host = cpal::default_host();
    let default_id = host
        .default_output_device()
        .and_then(|d| d.id().ok().map(|id| id.to_string()));

    if let Ok(cpal_devs) = host.output_devices() {
        for d in cpal_devs {
            #[cfg(target_os = "linux")]
            if !d
                .id()
                .is_ok_and(|id| crate::alsa_direct::is_system_level_pcm(id.id()))
            {
                continue;
            }
            let name = d
                .description()
                .map(|desc| desc.name().to_string())
                .unwrap_or_else(|_| d.to_string());
            let id = d
                .id()
                .ok()
                .map(|i| i.to_string())
                .unwrap_or_else(|| name.clone());
            let is_default = default_id.as_ref().is_some_and(|def| *def == id);
            // Exclusive mode opens the same WASAPI endpoints, under the same
            // ids, so a selection survives switching between the two modes.
            #[cfg(windows)]
            devices.push(AudioDeviceInfo {
                id: id.clone(),
                name: name.clone(),
                backend: AudioBackendKind::WasapiExclusive,
                is_default,
            });
            devices.push(AudioDeviceInfo {
                id,
                name,
                backend: AudioBackendKind::Cpal,
                is_default,
            });
        }
    }

    #[cfg(target_os = "linux")]
    for d in crate::alsa_direct::list_hw_devices() {
        devices.push(AudioDeviceInfo {
            id: d.id,
            name: d.name,
            backend: AudioBackendKind::Alsa,
            is_default: false,
        });
    }

    #[cfg(windows)]
    {
        for (name, clsid) in crate::asio::list_asio_drivers() {
            devices.push(AudioDeviceInfo {
                id: clsid,
                name,
                backend: AudioBackendKind::Asio,
                is_default: false,
            });
        }
    }

    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(backend: AudioBackendKind, device_id: &str) -> AudioOutputConfig {
        AudioOutputConfig {
            backend,
            device_id: Some(device_id.to_string()),
        }
    }

    #[test]
    fn asio_and_alsa_fallbacks_use_the_system_default_device() {
        let asio = config(
            AudioBackendKind::Asio,
            "462F2ABF-5278-436A-95B6-72CBF65482AE",
        );
        assert_eq!(asio.shared_device_id(), None);
        let alsa = config(AudioBackendKind::Alsa, "hw:CARD=EarPods,DEV=0");
        assert_eq!(alsa.shared_device_id(), None);
    }

    #[test]
    fn wasapi_endpoints_fall_back_on_the_chosen_endpoint() {
        for backend in [AudioBackendKind::Cpal, AudioBackendKind::WasapiExclusive] {
            assert_eq!(config(backend, "dev").shared_device_id(), Some("dev"));
        }
    }

    #[test]
    fn rate_candidates_halve_down_to_cd_rate() {
        assert_eq!(rate_candidates(44_100), vec![44_100]);
        assert_eq!(rate_candidates(48_000), vec![48_000]);
        assert_eq!(rate_candidates(96_000), vec![96_000, 48_000]);
        assert_eq!(
            rate_candidates(352_800),
            vec![352_800, 176_400, 88_200, 44_100]
        );
        // Odd rates have no exact halving.
        assert_eq!(rate_candidates(22_050), vec![22_050]);
        assert_eq!(rate_candidates(11_025), vec![11_025]);
    }

    const WASAPI: [PcmFormat; 4] = PcmFormat::PREFERENCE;

    #[test]
    fn negotiation_prefers_the_preferred_rate_at_the_deepest_format() {
        let picked = negotiate_format(44_100, &[48_000], &WASAPI, |_, _| true);
        assert_eq!(picked, Some((44_100, PcmFormat::I32)));
    }

    #[test]
    fn negotiation_walks_formats_before_falling_back_to_the_mix_rate() {
        let picked = negotiate_format(44_100, &[48_000], &WASAPI, |rate, format| {
            (rate == 44_100 && format == PcmFormat::I16) || rate == 48_000
        });
        assert_eq!(picked, Some((44_100, PcmFormat::I16)));

        let picked = negotiate_format(44_100, &[48_000], &WASAPI, |rate, format| {
            rate == 48_000 && format == PcmFormat::I24Packed
        });
        assert_eq!(picked, Some((48_000, PcmFormat::I24Packed)));
    }

    #[test]
    fn negotiation_asks_each_candidate_once_when_the_rates_coincide() {
        let mut asked = Vec::new();
        let picked = negotiate_format(48_000, &[48_000], &WASAPI, |rate, format| {
            asked.push((rate, format));
            false
        });
        assert_eq!(picked, None);
        assert_eq!(asked.len(), WASAPI.len());
    }

    #[test]
    fn negotiation_falls_back_to_the_closest_rate_of_the_same_family() {
        // A 192 kHz DAC asked for 352.8 kHz opens at 176.4 kHz, not its mix rate.
        let picked = negotiate_format(352_800, &[48_000], &WASAPI, |rate, _| rate <= 192_000);
        assert_eq!(picked, Some((176_400, PcmFormat::I32)));
    }

    #[test]
    fn negotiation_tries_fallback_rates_in_order() {
        // A 48 kHz-only USB device (packed 24-bit) asked for 44.1 kHz.
        let picked = negotiate_format(
            44_100,
            &[48_000, 44_100],
            &[
                PcmFormat::I32,
                PcmFormat::I24In32Low,
                PcmFormat::I24Packed,
                PcmFormat::I16,
            ],
            |rate, format| rate == 48_000 && format == PcmFormat::I24Packed,
        );
        assert_eq!(picked, Some((48_000, PcmFormat::I24Packed)));
    }
}
