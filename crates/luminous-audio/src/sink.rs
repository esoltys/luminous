//! Audio output sink abstraction and device enumeration.
//!
//! Provides a deep interface decoupling the playback/decode loop from specific
//! audio backends (CPAL shared audio, WASAPI exclusive and ASIO bit-perfect
//! output).

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Which output path the engine opens. The serialized names (`cpal`, `asio`,
/// `wasapi_exclusive`) are also how the choice is persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioBackendKind {
    /// Shared system audio through CPAL (on Windows, WASAPI shared mode).
    #[default]
    Cpal,
    Asio,
    /// WASAPI exclusive mode on the same endpoints as `Cpal` (Windows only).
    WasapiExclusive,
}

impl std::fmt::Display for AudioBackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cpal => write!(f, "CPAL"),
            Self::Asio => write!(f, "ASIO"),
            Self::WasapiExclusive => write!(f, "WASAPI Exclusive"),
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

/// Deep abstraction implemented by all output sinks (CPAL, ASIO, WASAPI exclusive).
pub trait AudioSink: Send {
    /// Resumes or starts playback on the output device.
    fn play(&self) -> Result<(), String>;

    /// Pauses playback on the output device.
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
}

/// The rate a bit-perfect device is opened at before any track asks for one:
/// most music is 44.1 kHz.
pub(crate) const DEFAULT_BIT_PERFECT_RATE: u32 = 44_100;

/// The rates to try, best first, when opening a bit-perfect device for a
/// track at `requested` Hz: the rate itself, then each halving of it down to
/// 44.1 kHz (an integer ratio keeps resampling simple, e.g. 352.8 kHz on a
/// 192 kHz DAC plays at 176.4 kHz).
#[cfg_attr(not(windows), allow(dead_code))] // ASIO and WASAPI only
pub(crate) fn rate_candidates(requested: u32) -> Vec<u32> {
    std::iter::successors(Some(requested), |&r| {
        (r % 2 == 0 && r / 2 >= 44_100).then_some(r / 2)
    })
    .collect()
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
}
