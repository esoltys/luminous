//! Encoders for the target formats, behind one streaming interface.

#[cfg_attr(windows, allow(dead_code))]
mod ffmpeg;
#[cfg(windows)]
mod media_foundation;
mod opus;

use crate::capabilities::FormatCapability;
use crate::Error;
use luminous_devices::TargetFormat;
use std::path::Path;

/// Takes planar f32 audio at the rate and channel count it was opened with
/// and writes a complete file. The file is only valid after `finish`.
pub(crate) trait Encoder {
    /// `planes` holds one slice per channel, all the same length.
    fn write(&mut self, planes: &[&[f32]]) -> Result<(), Error>;
    fn finish(self: Box<Self>) -> Result<(), Error>;
}

/// An encoder writing `format` to `out`. `format` and `bitrate_kbps` come
/// from `Capabilities::resolve`, so they are ones this computer offers.
pub(crate) fn open(
    format: TargetFormat,
    bitrate_kbps: u32,
    sample_rate: u32,
    channels: u8,
    out: &Path,
) -> Result<Box<dyn Encoder>, Error> {
    match format {
        TargetFormat::Opus => Ok(Box::new(opus::OpusEncoder::create(
            bitrate_kbps,
            channels,
            out,
        )?)),
        #[cfg(windows)]
        _ => Ok(Box::new(media_foundation::MfEncoder::create(
            format,
            bitrate_kbps,
            sample_rate,
            channels,
            out,
        )?)),
        #[cfg(not(windows))]
        _ => Ok(Box::new(ffmpeg::FfmpegEncoder::spawn(
            format,
            bitrate_kbps,
            sample_rate,
            channels,
            out,
        )?)),
    }
}

/// AAC and MP3 as far as this computer's encoders go (Opus is built in).
pub(crate) fn system_formats() -> Vec<FormatCapability> {
    #[cfg(windows)]
    return media_foundation::formats();
    #[cfg(not(windows))]
    return ffmpeg::formats();
}

/// Bitrates the Convert picker offers; each backend offers those of these
/// its encoder supports.
pub(crate) const STANDARD_BITRATES: [u32; 6] = [96, 128, 160, 192, 256, 320];

/// Interleaves `planes` into `out` (cleared first).
pub(crate) fn interleave(planes: &[&[f32]], out: &mut Vec<f32>) {
    out.clear();
    let frames = planes.first().map_or(0, |p| p.len());
    out.reserve(frames * planes.len());
    for frame in 0..frames {
        for plane in planes {
            out.push(plane[frame]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleave_alternates_channels() {
        let mut out = vec![9.0];
        interleave(&[&[1.0, 2.0], &[3.0, 4.0]], &mut out);
        assert_eq!(out, vec![1.0, 3.0, 2.0, 4.0]);
    }
}
