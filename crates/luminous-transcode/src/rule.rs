//! The "can't play" rule: what sync does with a file for a given device.

use crate::capabilities::{Capabilities, ConversionTarget};
use crate::source::SourceInfo;
use luminous_devices::{ConvertMode, DeviceProfile, TargetFormat};
use serde::Serialize;
use std::path::Path;

/// What happens to a file on its way to the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Decision {
    /// Copied as it is.
    Copy,
    /// Copied with its embedded art downscaled to `max_px`; audio untouched.
    ShrinkArt { max_px: u32 },
    /// Re-encoded. `format` and `bitrate_kbps` are what this computer can
    /// encode, which may differ from what the profile asked for.
    Convert {
        format: TargetFormat,
        bitrate_kbps: u32,
        sample_rate: u32,
        channels: u8,
        /// Art above this is downscaled; `None` keeps it as it is.
        max_art_px: Option<u32>,
    },
}

impl Decision {
    /// Extension the file gets on the device, without the dot.
    pub fn extension(&self, source: &Path) -> String {
        match self {
            Self::Convert { format, .. } => format.extension().to_string(),
            _ => source
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default(),
        }
    }
}

/// A lossy file within this fraction above the target bitrate isn't worth
/// re-encoding in "convert everything" mode: VBR averages wander a little.
const BITRATE_SLACK: f64 = 1.1;

pub(crate) fn decide(info: &SourceInfo, profile: &DeviceProfile, caps: &Capabilities) -> Decision {
    let settings = &profile.convert;
    if settings.mode == ConvertMode::Never {
        return Decision::Copy;
    }
    let ConversionTarget {
        format,
        bitrate_kbps,
        ..
    } = caps.target(profile);

    let playable = profile.supported_formats.iter().any(|f| f == info.codec);
    let within_limits = info.sample_rate <= profile.max_sample_rate
        && info
            .bit_depth
            .is_none_or(|depth| depth <= profile.max_bit_depth);
    let worth_shrinking = settings.mode == ConvertMode::SaveSpace
        && (info.is_lossless()
            || info
                .bitrate_kbps
                .is_some_and(|kbps| f64::from(kbps) > f64::from(bitrate_kbps) * BITRATE_SLACK));

    if !playable || !within_limits || worth_shrinking {
        return Decision::Convert {
            format,
            bitrate_kbps,
            sample_rate: output_rate(format, info.sample_rate, profile.max_sample_rate),
            channels: info.channels.clamp(1, 2),
            max_art_px: settings.max_art_px,
        };
    }
    match settings.max_art_px {
        Some(max_px) if info.art.largest_px > max_px => Decision::ShrinkArt { max_px },
        _ => Decision::Copy,
    }
}

/// The rate to encode at. Opus only runs at 48 kHz. AAC and MP3 encoders
/// take 44.1 or 48 kHz, so hi-res audio drops to the base rate of its own
/// family (88.2 and 176.4 kHz to 44.1; 96 and 192 kHz to 48), which
/// resamples by a whole ratio, unless the device can't take 48 kHz.
fn output_rate(format: TargetFormat, source_rate: u32, device_max: u32) -> u32 {
    if format == TargetFormat::Opus {
        return 48_000;
    }
    let family = if source_rate.is_multiple_of(11_025) {
        44_100
    } else {
        48_000
    };
    if family > device_max {
        44_100
    } else {
        family
    }
}

/// Bytes a downscaled JPEG of `max_px` takes, roughly: about one byte per
/// eight pixels at the quality `art` encodes with.
fn shrunk_art_bytes(max_px: u32) -> u64 {
    u64::from(max_px) * u64::from(max_px) / 8
}

/// The size `decision` gives `info` on the device, without encoding.
pub(crate) fn estimate(info: &SourceInfo, decision: &Decision) -> u64 {
    let art_after = |max_px: Option<u32>| match max_px {
        Some(max_px) if info.art.largest_px > max_px => {
            (info.art.count as u64 * shrunk_art_bytes(max_px)).min(info.art.bytes)
        }
        _ => info.art.bytes,
    };
    match *decision {
        Decision::Copy => info.stamp.size,
        Decision::ShrinkArt { max_px } => {
            info.stamp.size - info.art.bytes.min(info.stamp.size) + art_after(Some(max_px))
        }
        Decision::Convert {
            bitrate_kbps,
            max_art_px,
            ..
        } => {
            let audio = info.duration.as_secs_f64() * f64::from(bitrate_kbps) * 1000.0 / 8.0;
            // Container framing and the tags other than art.
            let overhead = audio * 0.01 + 4096.0;
            (audio + overhead) as u64 + art_after(max_art_px)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{ArtInfo, Stamp};
    use luminous_devices::ConvertSettings;
    use std::time::Duration;

    fn flac(sample_rate: u32, bit_depth: u8) -> SourceInfo {
        SourceInfo {
            path: "a.flac".into(),
            stamp: Stamp {
                size: 30_000_000,
                mtime_ns: 1,
            },
            codec: "flac",
            sample_rate,
            bit_depth: Some(bit_depth),
            channels: 2,
            bitrate_kbps: Some(900),
            duration: Duration::from_secs(200),
            art: ArtInfo::default(),
        }
    }

    fn lossy(codec: &'static str, kbps: u32) -> SourceInfo {
        SourceInfo {
            codec,
            bit_depth: None,
            bitrate_kbps: Some(kbps),
            ..flac(44_100, 16)
        }
    }

    fn walkman(mode: ConvertMode) -> DeviceProfile {
        DeviceProfile {
            supported_formats: ["mp3", "aac", "flac"].map(String::from).to_vec(),
            max_sample_rate: 48_000,
            max_bit_depth: 16,
            convert: ConvertSettings {
                mode,
                format: TargetFormat::Aac,
                bitrate_kbps: 192,
                max_art_px: None,
            },
            ..DeviceProfile::default()
        }
    }

    fn convert_to(format: TargetFormat, kbps: u32, rate: u32) -> Decision {
        Decision::Convert {
            format,
            bitrate_kbps: kbps,
            sample_rate: rate,
            channels: 2,
            max_art_px: None,
        }
    }

    #[test]
    fn playable_files_are_copied() {
        let caps = Capabilities::all();
        let profile = walkman(ConvertMode::Unsupported);
        assert_eq!(decide(&flac(44_100, 16), &profile, &caps), Decision::Copy);
        assert_eq!(decide(&lossy("mp3", 320), &profile, &caps), Decision::Copy);
    }

    #[test]
    fn unlisted_codecs_are_converted_and_alac_is_not_aac() {
        let caps = Capabilities::all();
        let profile = walkman(ConvertMode::Unsupported);
        let alac = SourceInfo {
            codec: "alac",
            ..flac(44_100, 16)
        };
        let expected = convert_to(TargetFormat::Aac, 192, 44_100);
        assert_eq!(decide(&alac, &profile, &caps), expected);
        assert_eq!(
            decide(
                &SourceInfo {
                    sample_rate: 48_000,
                    ..lossy("opus", 128)
                },
                &profile,
                &caps
            ),
            convert_to(TargetFormat::Aac, 192, 48_000)
        );
    }

    #[test]
    fn hi_res_above_the_device_maximum_is_converted_at_its_family_rate() {
        let caps = Capabilities::all();
        let profile = walkman(ConvertMode::Unsupported);
        assert_eq!(
            decide(&flac(96_000, 24), &profile, &caps),
            convert_to(TargetFormat::Aac, 192, 48_000)
        );
        assert_eq!(
            decide(&flac(88_200, 24), &profile, &caps),
            convert_to(TargetFormat::Aac, 192, 44_100)
        );
        // 24-bit at a rate the device takes is still over its bit depth.
        assert_eq!(
            decide(&flac(44_100, 24), &profile, &caps),
            convert_to(TargetFormat::Aac, 192, 44_100)
        );
    }

    #[test]
    fn save_space_mode_shrinks_lossless_and_larger_lossy_files_only() {
        let caps = Capabilities::all();
        let profile = walkman(ConvertMode::SaveSpace);
        let to_aac = convert_to(TargetFormat::Aac, 192, 44_100);
        assert_eq!(decide(&flac(44_100, 16), &profile, &caps), to_aac);
        assert_eq!(decide(&lossy("mp3", 320), &profile, &caps), to_aac);
        assert_eq!(decide(&lossy("mp3", 128), &profile, &caps), Decision::Copy);
        assert_eq!(decide(&lossy("aac", 200), &profile, &caps), Decision::Copy);
    }

    #[test]
    fn never_mode_copies_everything() {
        let caps = Capabilities::all();
        let mut profile = walkman(ConvertMode::Never);
        profile.convert.max_art_px = Some(500);
        assert_eq!(decide(&lossy("wma", 128), &profile, &caps), Decision::Copy);
    }

    #[test]
    fn oversized_art_is_shrunk_on_files_otherwise_copied() {
        let caps = Capabilities::all();
        let mut profile = walkman(ConvertMode::Unsupported);
        profile.convert.max_art_px = Some(500);
        let mut info = lossy("mp3", 320);
        info.art = ArtInfo {
            largest_px: 3000,
            count: 1,
            bytes: 2_000_000,
        };
        assert_eq!(
            decide(&info, &profile, &caps),
            Decision::ShrinkArt { max_px: 500 }
        );
        info.art.largest_px = 500;
        assert_eq!(decide(&info, &profile, &caps), Decision::Copy);
    }

    #[test]
    fn an_unavailable_target_falls_back_to_what_this_computer_encodes() {
        let profile = walkman(ConvertMode::Unsupported);
        assert_eq!(
            decide(&lossy("wma", 128), &profile, &Capabilities::opus_only()),
            convert_to(TargetFormat::Opus, 192, 48_000)
        );
    }

    #[test]
    fn estimates_follow_bitrate_and_art() {
        let mut info = flac(44_100, 16);
        info.art = ArtInfo {
            largest_px: 3000,
            count: 1,
            bytes: 2_000_000,
        };
        assert_eq!(estimate(&info, &Decision::Copy), 30_000_000);
        let shrunk = estimate(&info, &Decision::ShrinkArt { max_px: 500 });
        assert_eq!(shrunk, 30_000_000 - 2_000_000 + 31_250);

        // 200 s at 192 kbps is 4.8 MB of audio, plus framing and art.
        let converted = estimate(&info, &convert_to(TargetFormat::Aac, 192, 44_100));
        assert_eq!(converted, 4_800_000 + 48_000 + 4096 + 2_000_000);
    }
}
