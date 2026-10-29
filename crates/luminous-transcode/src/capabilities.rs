//! Which target formats and bitrates this computer can encode.

use luminous_devices::{DeviceProfile, TargetFormat};
use serde::Serialize;

/// One format conversions can produce here, and the bitrates it offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCapability {
    pub format: TargetFormat,
    /// Ascending, never empty.
    pub bitrates_kbps: Vec<u32>,
}

/// The formats conversions can produce on this computer, in the order the
/// Convert picker lists them. Opus is always among them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub formats: Vec<FormatCapability>,
}

/// The format and bitrate conversions for a device actually use here. Sync
/// shows this before it converts anything, so a substitute the device can't
/// play is the user's call, never a surprise on the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionTarget {
    pub format: TargetFormat,
    pub bitrate_kbps: u32,
    /// The profile asked for a format or bitrate this computer can't
    /// encode, and this one stands in for it.
    pub substituted: bool,
    /// `format` is in the device's plays list. When false, converted files
    /// may not play on the device.
    pub device_plays: bool,
}

/// Bitrates offered for Opus; libopus encodes all of them.
pub(crate) const OPUS_BITRATES: [u32; 5] = [96, 128, 160, 192, 256];

impl Capabilities {
    /// Probe this computer's encoders. Slow (it may start a process), so
    /// done once per `Transcoder`.
    pub(crate) fn detect() -> Self {
        let mut formats = crate::encode::system_formats();
        formats.push(FormatCapability {
            format: TargetFormat::Opus,
            bitrates_kbps: OPUS_BITRATES.to_vec(),
        });
        Self { formats }
    }

    /// The format and bitrate actually used for a request: the requested
    /// format when it is available, otherwise Opus; the highest offered
    /// bitrate not above the requested one, otherwise the lowest offered.
    pub(crate) fn resolve(&self, format: TargetFormat, bitrate_kbps: u32) -> (TargetFormat, u32) {
        let capability = self
            .formats
            .iter()
            .find(|c| c.format == format)
            .or_else(|| self.formats.iter().find(|c| c.format == TargetFormat::Opus))
            .expect("Opus is always available");
        let bitrate = capability
            .bitrates_kbps
            .iter()
            .rev()
            .find(|&&b| b <= bitrate_kbps)
            .or(capability.bitrates_kbps.first())
            .copied()
            .expect("bitrates are never empty");
        (capability.format, bitrate)
    }

    /// What files converted for `profile` will be on this computer, and
    /// whether that is what the profile asked for and what the device plays.
    pub fn target(&self, profile: &DeviceProfile) -> ConversionTarget {
        let requested = (profile.convert.format, profile.convert.bitrate_kbps);
        let (format, bitrate_kbps) = self.resolve(requested.0, requested.1);
        ConversionTarget {
            format,
            bitrate_kbps,
            substituted: (format, bitrate_kbps) != requested,
            device_plays: profile
                .supported_formats
                .iter()
                .any(|f| f == format.format_name()),
        }
    }

    #[cfg(test)]
    pub(crate) fn opus_only() -> Self {
        Self {
            formats: vec![FormatCapability {
                format: TargetFormat::Opus,
                bitrates_kbps: OPUS_BITRATES.to_vec(),
            }],
        }
    }

    #[cfg(test)]
    pub(crate) fn all() -> Self {
        let mut caps = Self::opus_only();
        for format in [TargetFormat::Aac, TargetFormat::Mp3] {
            caps.formats.push(FormatCapability {
                format,
                bitrates_kbps: vec![128, 192, 256, 320],
            });
        }
        caps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_keeps_an_available_request() {
        assert_eq!(
            Capabilities::all().resolve(TargetFormat::Mp3, 256),
            (TargetFormat::Mp3, 256)
        );
    }

    #[test]
    fn resolve_rounds_the_bitrate_down_then_up() {
        let caps = Capabilities::all();
        assert_eq!(
            caps.resolve(TargetFormat::Aac, 300),
            (TargetFormat::Aac, 256)
        );
        assert_eq!(
            caps.resolve(TargetFormat::Aac, 64),
            (TargetFormat::Aac, 128)
        );
    }

    #[test]
    fn resolve_falls_back_to_opus() {
        assert_eq!(
            Capabilities::opus_only().resolve(TargetFormat::Aac, 320),
            (TargetFormat::Opus, 256)
        );
    }

    #[test]
    fn target_reports_a_substitute_the_device_cannot_play() {
        let profile = DeviceProfile::default(); // plays MP3 and AAC; asks AAC 192
        assert_eq!(
            Capabilities::all().target(&profile),
            ConversionTarget {
                format: TargetFormat::Aac,
                bitrate_kbps: 192,
                substituted: false,
                device_plays: true,
            }
        );
        assert_eq!(
            Capabilities::opus_only().target(&profile),
            ConversionTarget {
                format: TargetFormat::Opus,
                bitrate_kbps: 192,
                substituted: true,
                device_plays: false,
            }
        );
    }

    #[test]
    fn detect_always_offers_opus() {
        let caps = Capabilities::detect();
        assert!(caps.formats.iter().any(|c| c.format == TargetFormat::Opus));
        assert!(caps.formats.iter().all(|c| !c.bitrates_kbps.is_empty()));
    }
}
