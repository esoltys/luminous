//! AAC and MP3 through an `ffmpeg` binary on `PATH`, where the platform has
//! no encoder Luminous can use directly (Linux). Not linked: ffmpeg is the
//! user's own install, run as a process fed raw PCM on stdin.

use super::{interleave, Encoder, STANDARD_BITRATES};
use crate::capabilities::FormatCapability;
use crate::Error;
use luminous_devices::TargetFormat;
use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};

const PROGRAM: &str = "ffmpeg";

/// ffmpeg's encoder for `format`, and the muxer for its file.
fn codec(format: TargetFormat) -> (&'static str, &'static str) {
    match format {
        TargetFormat::Aac => ("aac", "ipod"),
        TargetFormat::Mp3 => ("libmp3lame", "mp3"),
        TargetFormat::Opus => ("libopus", "ogg"),
    }
}

/// AAC and MP3 when ffmpeg is installed with their encoders; nothing
/// otherwise (the Flatpak, a system without ffmpeg).
pub(crate) fn formats() -> Vec<FormatCapability> {
    let Ok(output) = Command::new(PROGRAM)
        .args(["-hide_banner", "-encoders"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    let listing = String::from_utf8_lossy(&output.stdout);
    let has = |name: &str| {
        listing
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some(name))
    };
    [TargetFormat::Aac, TargetFormat::Mp3]
        .into_iter()
        .filter(|&format| has(codec(format).0))
        .map(|format| FormatCapability {
            format,
            bitrates_kbps: STANDARD_BITRATES
                .into_iter()
                .filter(|&kbps| kbps >= 128)
                .collect(),
        })
        .collect()
}

pub(crate) struct FfmpegEncoder {
    child: Child,
    stdin: Option<ChildStdin>,
    scratch: Vec<f32>,
    bytes: Vec<u8>,
}

impl FfmpegEncoder {
    pub fn spawn(
        format: TargetFormat,
        bitrate_kbps: u32,
        sample_rate: u32,
        channels: u8,
        out: &Path,
    ) -> Result<Self, Error> {
        let (encoder, muxer) = codec(format);
        let mut child = Command::new(PROGRAM)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "f32le", "-ar", &sample_rate.to_string()])
            .args(["-ac", &channels.to_string(), "-i", "pipe:0"])
            .args(["-vn", "-c:a", encoder, "-b:a", &format!("{bitrate_kbps}k")])
            .args(["-f", muxer])
            .arg(out)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Failed(format!("{PROGRAM}: {e}")))?;
        let stdin = child.stdin.take();
        Ok(Self {
            child,
            stdin,
            scratch: Vec::new(),
            bytes: Vec::new(),
        })
    }

    /// What ffmpeg said about a failure; call once it has exited.
    fn failure(&mut self) -> Error {
        let mut message = String::new();
        if let Some(mut stderr) = self.child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut message);
        }
        Error::Failed(format!("{PROGRAM}: {}", message.trim()))
    }
}

impl Encoder for FfmpegEncoder {
    fn write(&mut self, planes: &[&[f32]]) -> Result<(), Error> {
        interleave(planes, &mut self.scratch);
        self.bytes.clear();
        self.bytes
            .extend(self.scratch.iter().flat_map(|s| s.to_le_bytes()));
        let stdin = self.stdin.as_mut().expect("open until finish");
        if stdin.write_all(&self.bytes).is_err() {
            // ffmpeg exited early; its own message says why.
            self.stdin = None;
            let _ = self.child.wait();
            return Err(self.failure());
        }
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), Error> {
        self.stdin = None; // EOF: ffmpeg finishes the file and exits
        if self.child.wait()?.success() {
            Ok(())
        } else {
            Err(self.failure())
        }
    }
}

impl Drop for FfmpegEncoder {
    fn drop(&mut self) {
        // Cancelled or failed mid-file: don't leave ffmpeg running.
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes a second of stereo through ffmpeg for each format it offers,
    /// then carries tags over the way the pipeline does. Runs wherever
    /// ffmpeg is on `PATH` (Linux CI, or Windows with it added), whichever
    /// backend the platform uses for real.
    #[test]
    fn encodes_and_takes_tags_where_ffmpeg_is_installed() {
        let formats = formats();
        if formats.is_empty() {
            eprintln!("ffmpeg isn't on PATH; skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.wav");
        let tone: Vec<i16> = (0..44_100).map(|i| ((i % 100) * 100) as i16).collect();
        luminous_audio::write_test_pcm16_wav(&source, 44_100, &[tone.clone(), tone]);
        let mut file = lofty::probe::Probe::open(&source).unwrap().read().unwrap();
        let mut tag = lofty::tag::Tag::new(lofty::tag::TagType::Id3v2);
        lofty::tag::Accessor::set_title(&mut tag, "Valediction".into());
        lofty::file::TaggedFileExt::insert_tag(&mut file, tag);
        lofty::file::AudioFile::save_to_path(&file, &source, Default::default()).unwrap();

        for capability in formats {
            let format = capability.format;
            let out = dir
                .path()
                .join(format!("out.partial.{}", format.extension()));
            let left: Vec<f32> = (0..44_100).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
            let mut encoder = Box::new(
                FfmpegEncoder::spawn(format, capability.bitrates_kbps[0], 44_100, 2, &out).unwrap(),
            );
            for (l, r) in left.chunks(4096).zip(left.chunks(4096)) {
                encoder.write(&[l, r]).unwrap();
            }
            encoder.finish().unwrap();
            crate::art::copy_tags(&source, &out, None).unwrap();

            let decoder = luminous_audio::OfflineDecoder::open_file(&out).unwrap();
            let rate = decoder.sample_rate();
            let mut frames = 0;
            decoder.decode_blocks(1, |planes| frames += planes[0].len());
            let seconds = frames as f64 / f64::from(rate);
            assert!((seconds - 1.0).abs() < 0.1, "{format:?}: {seconds} s");
            let tagged = lofty::probe::Probe::open(&out).unwrap().read().unwrap();
            let tag = lofty::file::TaggedFileExt::primary_tag(&tagged).unwrap();
            assert_eq!(
                lofty::tag::Accessor::title(tag).as_deref(),
                Some("Valediction"),
                "{format:?}"
            );
        }
    }
}
