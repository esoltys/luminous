//! What the rule needs to know about a library file, read from its tags and
//! stream headers without decoding audio.

use crate::Error;
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::mp4::{Mp4Codec, Mp4File};
use lofty::probe::Probe;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

/// Identifies one version of a file: a change to either means the file
/// changed, so a probe or cache entry made for the old one no longer applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub size: u64,
    /// Nanoseconds since the Unix epoch; 0 when the filesystem has none.
    pub mtime_ns: u128,
}

impl Stamp {
    pub fn of(path: &Path) -> Result<Self, Error> {
        let meta = std::fs::metadata(path).map_err(Error::at(path))?;
        let mtime_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        Ok(Self {
            size: meta.len(),
            mtime_ns,
        })
    }
}

/// Embedded cover art, as far as sizing goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ArtInfo {
    /// Longest side of the largest picture; 0 when there is none or none
    /// could be read.
    pub largest_px: u32,
    pub count: usize,
    /// Bytes of picture data across every tag.
    pub bytes: u64,
}

/// What the library's own scan recorded about a file: enough for the rule
/// without opening the file again (see [`crate::Transcoder::assess`]).
#[derive(Debug, Clone, PartialEq)]
pub struct KnownSource<'a> {
    pub path: &'a Path,
    pub size: u64,
    /// The codec as `DeviceProfile::supported_formats` names it; `None`
    /// when the scan can't tell (an `.m4a` holds AAC or ALAC), and the file
    /// is read instead.
    pub codec: Option<&'a str>,
    pub sample_rate: u32,
    pub bit_depth: Option<u8>,
    pub channels: u8,
    /// Of the audio stream alone, as the probe reads it.
    pub bitrate_kbps: Option<u32>,
    pub duration: Duration,
    pub has_art: bool,
}

/// Every codec name a probe gives, so a known source's codec is one of them.
const CODECS: [&str; 13] = [
    "alac", "mp3", "flac", "aac", "ogg", "opus", "wav", "aiff", "ape", "mpc", "speex", "wavpack",
    "unknown",
];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceInfo {
    pub path: PathBuf,
    pub stamp: Stamp,
    /// The codec as `DeviceProfile::supported_formats` names it. Formats
    /// outside that vocabulary get their own name (`ape`, `wavpack`...), so
    /// no device lists them and they are always converted.
    pub codec: &'static str,
    pub sample_rate: u32,
    pub bit_depth: Option<u8>,
    pub channels: u8,
    pub bitrate_kbps: Option<u32>,
    pub duration: Duration,
    pub art: ArtInfo,
}

impl SourceInfo {
    pub fn probe(path: &Path, stamp: Stamp) -> Result<Self, Error> {
        let probe = Probe::open(path)
            .map_err(Error::at(path))?
            .options(ParseOptions::new().read_cover_art(true))
            .guess_file_type()
            .map_err(Error::at(path))?;
        let file_type = probe.file_type();

        // The generic properties don't say whether an MP4 holds AAC or ALAC.
        let (tagged, mp4_codec) = if file_type == Some(FileType::Mp4) {
            let mut file = std::fs::File::open(path)?;
            let mp4 = Mp4File::read_from(&mut file, ParseOptions::new().read_cover_art(true))
                .map_err(Error::at(path))?;
            let codec = mp4.properties().codec();
            (lofty::file::TaggedFile::from(mp4), codec)
        } else {
            (probe.read().map_err(Error::at(path))?, None)
        };

        let properties = tagged.properties();
        let codec = match file_type {
            Some(FileType::Mp4) => match mp4_codec {
                Some(Mp4Codec::ALAC) => "alac",
                Some(Mp4Codec::MP3) => "mp3",
                Some(Mp4Codec::FLAC) => "flac",
                _ => "aac",
            },
            Some(FileType::Aac) => "aac",
            Some(FileType::Mpeg) => "mp3",
            Some(FileType::Flac) => "flac",
            Some(FileType::Vorbis) => "ogg",
            Some(FileType::Opus) => "opus",
            Some(FileType::Wav) => "wav",
            Some(FileType::Aiff) => "aiff",
            Some(FileType::Ape) => "ape",
            Some(FileType::Mpc) => "mpc",
            Some(FileType::Speex) => "speex",
            Some(FileType::WavPack) => "wavpack",
            _ => "unknown",
        };

        let mut art = ArtInfo::default();
        for picture in tagged.tags().iter().flat_map(|t| t.pictures()) {
            art.count += 1;
            art.bytes += picture.data().len() as u64;
            if let Some((w, h)) = crate::art::dimensions(picture.data()) {
                art.largest_px = art.largest_px.max(w.max(h));
            }
        }

        Ok(Self {
            path: path.to_path_buf(),
            stamp,
            codec,
            // Opus is always decoded at 48 kHz, whatever the header says.
            sample_rate: if codec == "opus" {
                48_000
            } else {
                properties.sample_rate().unwrap_or(44_100)
            },
            bit_depth: properties.bit_depth(),
            channels: properties.channels().unwrap_or(2),
            bitrate_kbps: properties.audio_bitrate(),
            duration: properties.duration(),
            art,
        })
    }

    /// What a probe would read, from what the library knows; `None` when it
    /// doesn't know enough. Embedded art is taken to be everything in the
    /// file but the audio, at a size the profile may shrink: only its
    /// dimensions are unknown, and they decide nothing but a
    /// [`Decision::ShrinkArt`](crate::Decision::ShrinkArt).
    pub fn from_known(known: &KnownSource) -> Option<Self> {
        let codec = CODECS
            .into_iter()
            .find(|c| Some(*c) == known.codec && *c != "unknown")?;
        if known.sample_rate == 0 || known.channels == 0 || known.duration.is_zero() {
            return None;
        }
        let art = if known.has_art {
            let audio = known.bitrate_kbps.map_or(known.size, |kbps| {
                (known.duration.as_secs_f64() * f64::from(kbps) * 1000.0 / 8.0) as u64
            });
            ArtInfo {
                largest_px: u32::MAX,
                count: 1,
                bytes: known.size.saturating_sub(audio),
            }
        } else {
            ArtInfo::default()
        };
        Some(Self {
            path: known.path.to_path_buf(),
            stamp: Stamp {
                size: known.size,
                mtime_ns: 0,
            },
            codec,
            sample_rate: if codec == "opus" {
                48_000
            } else {
                known.sample_rate
            },
            bit_depth: known.bit_depth,
            channels: known.channels,
            bitrate_kbps: known.bitrate_kbps,
            duration: known.duration,
            art,
        })
    }

    /// Lossless codecs, whose bitrate says nothing about quality.
    pub fn is_lossless(&self) -> bool {
        matches!(
            self.codec,
            "flac" | "alac" | "wav" | "aiff" | "ape" | "wavpack"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_a_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        luminous_audio::write_test_pcm16_wav(&path, 44_100, &[vec![0; 44_100], vec![0; 44_100]]);
        let info = SourceInfo::probe(&path, Stamp::of(&path).unwrap()).unwrap();
        assert_eq!(info.codec, "wav");
        assert_eq!(info.sample_rate, 44_100);
        assert_eq!(info.bit_depth, Some(16));
        assert_eq!(info.channels, 2);
        assert_eq!(info.duration.as_secs(), 1);
        assert!(info.is_lossless());
        assert_eq!(info.art, ArtInfo::default());
    }

    #[test]
    fn a_changed_file_has_a_new_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        std::fs::write(&path, b"one").unwrap();
        let before = Stamp::of(&path).unwrap();
        std::fs::write(&path, b"three").unwrap();
        assert_ne!(Stamp::of(&path).unwrap(), before);
    }
}
