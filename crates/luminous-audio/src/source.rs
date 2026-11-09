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
    /// Bit depth reported by the container/stream header, if known.
    pub(crate) bits_per_sample: Option<u32>,
}

impl ProbedTrack {
    /// Probe `source`, with `extension` (no dot) hinting the format.
    pub(crate) fn open(
        source: Box<dyn MediaSource>,
        extension: Option<&str>,
    ) -> Result<Self, String> {
        Self::open_with_options(source, extension, &AudioDecoderOptions::default())
    }

    /// Probe `source` with customized `AudioDecoderOptions`.
    pub(crate) fn open_with_options(
        source: Box<dyn MediaSource>,
        extension: Option<&str>,
        decoder_opts: &AudioDecoderOptions,
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
            .make_audio_decoder(audio_params, decoder_opts)
            .map_err(|e| format!("Decoder init failed: {e}"))?;
        let sample_rate = audio_params.sample_rate.unwrap_or(44100);
        let channels = audio_params
            .channels
            .as_ref()
            .map(|c| c.count())
            .unwrap_or(2);
        let bits_per_sample = audio_params.bits_per_sample;

        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            bits_per_sample,
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

    /// Probe already-opened bytes with checksum verification enabled where supported.
    pub fn open_verified(
        source: Box<dyn MediaSource>,
        extension: Option<&str>,
    ) -> Result<Self, String> {
        let mut opts = AudioDecoderOptions::default();
        opts.verify = true;
        Ok(Self {
            track: ProbedTrack::open_with_options(source, extension, &opts)?,
        })
    }

    /// Open and probe a local file.
    pub fn open_file(path: &std::path::Path) -> Result<Self, String> {
        let source = crate::open_file(&path.to_string_lossy())?;
        Self::open(source, path.extension().and_then(|e| e.to_str()))
    }

    /// Open and probe a local file with checksum verification enabled.
    pub fn open_file_verified(path: &std::path::Path) -> Result<Self, String> {
        let source = crate::open_file(&path.to_string_lossy())?;
        Self::open_verified(source, path.extension().and_then(|e| e.to_str()))
    }

    /// Codec identifier for the audio stream.
    pub fn codec_id(&self) -> symphonia::core::codecs::audio::AudioCodecId {
        self.track.decoder.codec_params().codec
    }

    pub fn sample_rate(&self) -> u32 {
        self.track.sample_rate
    }

    /// Channel count of the decoded audio, before any `max_channels` cap.
    pub fn channels(&self) -> usize {
        self.track.channels
    }

    /// Bits per sample reported by the codec parameters, if known (e.g. 16, 24).
    pub fn bits_per_sample(&self) -> Option<u32> {
        self.track.bits_per_sample
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

    /// Verifies the structural decodability and stream integrity of this track.
    ///
    /// For FLAC files, decodes all packets with checksum verification active and validates
    /// against the embedded 128-bit STREAMINFO MD5 signature upon stream completion.
    ///
    /// For containerized and lossy files (MP3, AAC, ALAC, Opus, Vorbis, etc.), decodes initial
    /// boundary packets to ensure frame sync and header validity, then verifies end boundary
    /// decodability to detect truncated or corrupted files without full stream decoding overhead.
    pub fn verify_integrity(mut self) -> Result<(), String> {
        let track_id = self.track.track_id;
        let is_flac = self.codec_id() == symphonia::core::codecs::audio::well_known::CODEC_ID_FLAC;

        if is_flac {
            let mut any_decoded = false;
            let mut planes: Vec<Vec<f32>> = Vec::new();
            loop {
                match self.track.format.next_packet() {
                    Ok(Some(packet)) => {
                        if packet.track_id != track_id {
                            continue;
                        }
                        match self.track.decoder.decode(&packet) {
                            Ok(decoded) => {
                                any_decoded = true;
                                decoded.copy_to_vecs_planar(&mut planes);
                            }
                            Err(SymphoniaError::DecodeError(e)) => {
                                return Err(format!("Corrupt FLAC audio frame: {e}"));
                            }
                            Err(e) => return Err(format!("FLAC frame decode error: {e}")),
                        }
                    }
                    Ok(None) => break,
                    Err(SymphoniaError::IoError(e))
                        if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                    {
                        return Err(
                            "Truncated FLAC audio stream: premature end-of-file".to_string()
                        );
                    }
                    Err(e) => return Err(format!("FLAC stream packet error: {e}")),
                }
            }

            if !any_decoded {
                return Err("Zero audio frames decoded from FLAC stream".to_string());
            }

            let finalize_res = self.track.decoder.finalize();
            if let Some(false) = finalize_res.verify_ok {
                return Err("FLAC MD5 checksum mismatch".to_string());
            }
            Ok(())
        } else {
            // Containerized / lossy / PCM streams:
            // 1. Decode initial packets (head)
            let mut initial_packets = 0;
            let mut any_decoded = false;
            let mut planes: Vec<Vec<f32>> = Vec::new();
            while initial_packets < 10 {
                match self.track.format.next_packet() {
                    Ok(Some(packet)) => {
                        if packet.track_id != track_id {
                            continue;
                        }
                        initial_packets += 1;
                        match self.track.decoder.decode(&packet) {
                            Ok(decoded) => {
                                any_decoded = true;
                                decoded.copy_to_vecs_planar(&mut planes);
                            }
                            Err(SymphoniaError::DecodeError(e)) => {
                                return Err(format!("Corrupt audio frame: {e}"));
                            }
                            Err(e) => return Err(format!("Audio decode error: {e}")),
                        }
                    }
                    Ok(None) => break,
                    Err(SymphoniaError::IoError(e))
                        if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                    {
                        if initial_packets == 0 {
                            return Err("Truncated audio file: premature end-of-file".to_string());
                        }
                        break;
                    }
                    Err(e) => return Err(format!("Audio stream packet error: {e}")),
                }
            }

            if !any_decoded {
                return Err("Zero audio frames decoded from stream".to_string());
            }

            // 2. Decode trailing packets to detect truncated files
            let track_meta = self.track.format.tracks().iter().find(|t| t.id == track_id);
            let time_base = track_meta.and_then(|t| t.time_base);
            let duration = track_meta.and_then(|t| t.duration);

            if let (Some(tb), Some(dur)) = (time_base, duration) {
                if let Some(total_time) =
                    tb.calc_time(symphonia::core::units::Timestamp::new(dur.get() as i64))
                {
                    if total_time.as_secs() > 4 {
                        let target_secs = total_time.as_secs().saturating_sub(2);
                        let target_time =
                            symphonia::core::units::Time::from_nanos(target_secs * 1_000_000_000);
                        if self
                            .track
                            .format
                            .seek(
                                symphonia::core::formats::SeekMode::Coarse,
                                symphonia::core::formats::SeekTo::Time {
                                    time: target_time,
                                    track_id: Some(track_id),
                                },
                            )
                            .is_ok()
                        {
                            self.track.decoder.reset();
                            while let Ok(Some(packet)) = self.track.format.next_packet() {
                                if packet.track_id == track_id {
                                    if let Err(e) = self.track.decoder.decode(&packet) {
                                        return Err(format!("Corrupt trailing audio frame: {e}"));
                                    }
                                }
                            }
                            return Ok(());
                        }
                    }
                }
            }

            // Fallback: if seek was not available or duration was small, decode remaining packets to the end
            loop {
                match self.track.format.next_packet() {
                    Ok(Some(packet)) => {
                        if packet.track_id != track_id {
                            continue;
                        }
                        if let Err(e) = self.track.decoder.decode(&packet) {
                            return Err(format!("Corrupt audio frame: {e}"));
                        }
                    }
                    Ok(None) => break,
                    Err(SymphoniaError::IoError(e))
                        if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                    {
                        return Err("Truncated audio file: premature end-of-file".to_string());
                    }
                    Err(e) => return Err(format!("Audio stream packet error: {e}")),
                }
            }

            Ok(())
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
