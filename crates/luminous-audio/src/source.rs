//! From opened bytes to decodable packets: format probing and decoder
//! construction, shared by playback and offline analysis.

use crate::codecs::CODEC_REGISTRY;
use std::ops::ControlFlow;
use symphonia::core::{
    codecs::audio::{AudioDecoder, AudioDecoderOptions},
    errors::Error as SymphoniaError,
    formats::{probe::Hint, FormatOptions, FormatReader, TrackType},
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
};

/// A media source probed, with a decoder built for its default audio track.
/// The one place that knows how to get from bytes to decodable packets; playback (`ActiveTrack`) and offline analysis (`OfflineDecoder`)
/// each add their own reading policy on top.
pub(crate) struct ProbedTrack {
    pub(crate) format: Box<dyn FormatReader>,
    pub(crate) decoder: Box<dyn AudioDecoder>,
    pub(crate) track_id: u32,
    /// Falls back to 44.1 kHz when the container doesn't say.
    pub(crate) sample_rate: u32,
    /// Falls back to stereo when the container doesn't say.
    pub(crate) channels: usize,
}

impl ProbedTrack {
    /// Probe `source`, with `extension` (no dot) hinting the format.
    pub(crate) fn open(
        source: Box<dyn MediaSource>,
        extension: Option<&str>,
    ) -> Result<Self, String> {
        let mut hint = Hint::new();
        if let Some(ext) = extension {
            hint.with_extension(ext);
        }

        let mss = MediaSourceStream::new(source, Default::default());
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| format!("Format probe failed: {e}"))?;

        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| "No audio track found".to_string())?;
        let track_id = track.id;
        let audio_params = track
            .codec_params
            .as_ref()
            .and_then(|c| c.audio())
            .ok_or_else(|| "No audio codec parameters".to_string())?;
        let decoder = CODEC_REGISTRY
            .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
            .map_err(|e| format!("Decoder init failed: {e}"))?;
        let sample_rate = audio_params.sample_rate.unwrap_or(44100);
        let channels = audio_params
            .channels
            .as_ref()
            .map(|c| c.count())
            .unwrap_or(2);

        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
        })
    }
}

/// Whole-file decode for offline analysis (waveform peaks, R128 loudness):
/// runs as fast as the CPU allows, with none of playback's pacing, seeking or
/// resampling. Open it, read `sample_rate()`, then drive `decode_blocks`.
pub struct OfflineDecoder {
    track: ProbedTrack,
}

impl OfflineDecoder {
    /// Probe already-opened bytes, with `extension` (no dot) hinting the
    /// format.
    pub fn open(source: Box<dyn MediaSource>, extension: Option<&str>) -> Result<Self, String> {
        Ok(Self {
            track: ProbedTrack::open(source, extension)?,
        })
    }

    /// Open and probe a local file.
    pub fn open_file(path: &std::path::Path) -> Result<Self, String> {
        let source = crate::open_file(&path.to_string_lossy())?;
        Self::open(source, path.extension().and_then(|e| e.to_str()))
    }

    pub fn sample_rate(&self) -> u32 {
        self.track.sample_rate
    }

    /// Channel count of the decoded audio, before any `max_channels` cap.
    pub fn channels(&self) -> usize {
        self.track.channels
    }

    /// Decode to the end, handing each decoded packet to `on_block` as planar
    /// f32 channels: at most `max_channels`, all the same length. The planes
    /// are a scratch buffer reused for the next packet, so callers accumulate
    /// what they need rather than the whole file being held here. Corrupt
    /// packets are skipped; any other error ends the decode early with
    /// whatever was delivered so far.
    pub fn decode_blocks(self, max_channels: usize, mut on_block: impl FnMut(&[Vec<f32>])) {
        let _partial_is_fine = self.try_decode_blocks(max_channels, |planes| {
            on_block(planes);
            ControlFlow::Continue(())
        });
    }

    /// Like `decode_blocks`, for callers that must not mistake a truncated
    /// decode for the whole file (transcoding): an error other than a
    /// corrupt packet is returned instead of ending quietly. A file cut
    /// short mid-packet ends like a whole one, as in playback. `on_block`
    /// can stop the decode early by returning `Break`, which is not an error.
    pub fn try_decode_blocks(
        mut self,
        max_channels: usize,
        mut on_block: impl FnMut(&[Vec<f32>]) -> ControlFlow<()>,
    ) -> Result<(), String> {
        let track = &mut self.track;
        let mut planes: Vec<Vec<f32>> = Vec::new();
        loop {
            let packet = match track.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(()),
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(())
                }
                Err(e) => return Err(e.to_string()),
            };
            if packet.track_id != track.track_id {
                continue;
            }
            match track.decoder.decode(&packet) {
                Ok(decoded) => {
                    decoded.copy_to_vecs_planar(&mut planes);
                    if on_block(&planes[..planes.len().min(max_channels)]).is_break() {
                        return Ok(());
                    }
                }
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opus in a `.ogg` file — the extension Vorbis uses — must still decode,
    /// since the library now imports these files (#1866).
    #[test]
    fn opus_in_ogg_decodes() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/tests/fixtures/codec/opus.ogg");
        let decoder = OfflineDecoder::open_file(&path).expect("open opus .ogg");
        let mut frames = 0;
        decoder.decode_blocks(2, |planes| frames += planes[0].len());
        assert!(frames > 0, "no audio decoded");
    }
}
