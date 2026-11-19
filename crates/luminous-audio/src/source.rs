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

/// Normalizes an HTTP `Content-Type` header value into a bare MIME type
/// Symphonia's format probe understands (e.g. `"audio/mpeg; charset=utf-8"`
/// -> `"audio/mpeg"`, `"audio/aacp"` -> `"audio/aac"`).
pub fn normalize_probe_mime(raw: &str) -> Option<String> {
    let base = raw.split(';').next()?.trim().to_ascii_lowercase();
    if base.is_empty() {
        return None;
    }
    let mapped = match base.as_str() {
        "audio/aacp" | "audio/x-aac" => "audio/aac",
        "audio/mp3" | "audio/x-mp3" | "audio/x-mpeg" | "audio/mpeg3" => "audio/mpeg",
        "audio/x-flac" => "audio/flac",
        "audio/x-wav" | "audio/wave" => "audio/wav",
        "audio/x-aiff" => "audio/aiff",
        "audio/x-ogg" | "application/x-ogg" => "application/ogg",
        other => other,
    };
    Some(mapped.to_string())
}

/// Decoder and format parameters for a single logical audio bitstream.
pub(crate) struct DefaultTrackDecoder {
    pub(crate) decoder: Box<dyn AudioDecoder>,
    pub(crate) track_id: u32,
    pub(crate) sample_rate: u32,
    pub(crate) channels: usize,
    pub(crate) bits_per_sample: Option<u32>,
}

impl ProbedTrack {
    /// Probe `source`, with `extension` (no dot) and optional `mime_type`
    /// hinting the format.
    pub(crate) fn open(
        source: Box<dyn MediaSource>,
        extension: Option<&str>,
        mime_type: Option<&str>,
    ) -> Result<Self, String> {
        Self::open_with_options(
            source,
            extension,
            mime_type,
            &AudioDecoderOptions::default(),
        )
    }

    /// Probe `source` with customized `AudioDecoderOptions`.
    pub(crate) fn open_with_options(
        source: Box<dyn MediaSource>,
        extension: Option<&str>,
        mime_type: Option<&str>,
        decoder_opts: &AudioDecoderOptions,
    ) -> Result<Self, String> {
        let mut hint = Hint::new();
        if let Some(ext) = extension.filter(|e| !e.is_empty()) {
            hint.with_extension(ext);
        }
        if let Some(mime) = mime_type.and_then(normalize_probe_mime) {
            hint.mime_type(&mime);
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

        let DefaultTrackDecoder {
            decoder,
            track_id,
            sample_rate,
            channels,
            bits_per_sample,
        } = Self::build_decoder_for_default_track(&*format, decoder_opts)?;

        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            bits_per_sample,
        })
    }

    /// Re-initializes the audio decoder for the format reader's current default
    /// audio track (used both at initial open and when an Ogg chained stream
    /// signals `SymphoniaError::ResetRequired` at a logical bitstream boundary).
    pub(crate) fn build_decoder_for_default_track(
        format: &dyn FormatReader,
        decoder_opts: &AudioDecoderOptions,
    ) -> Result<DefaultTrackDecoder, String> {
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
        Ok(DefaultTrackDecoder {
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

/// Outcome of structural, frame-level, and checksum verification of an audio stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityCheck {
    /// All decoded frames succeeded and any embedded stream checksum matched.
    Valid,
    /// Audio frames decoded and the track is playable, but a non-fatal integrity
    /// discrepancy was found (e.g., a FLAC `STREAMINFO` MD5 mismatch or trailing
    /// EOF after audio frames were already decoded).
    Warning(String),
    /// The stream or container could not be decoded cleanly (zero decoded
    /// frames, corrupt frame data, or mid-stream packet error).
    Corrupt(String),
}

impl OfflineDecoder {
    /// Probe already-opened bytes, with `extension` (no dot) hinting the
    /// format.
    pub fn open(source: Box<dyn MediaSource>, extension: Option<&str>) -> Result<Self, String> {
        Ok(Self {
            track: ProbedTrack::open(source, extension, None)?,
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
            track: ProbedTrack::open_with_options(source, extension, None, &opts)?,
        })
    }

    /// Open and probe a local file.
    pub fn open_file(path: &std::path::Path) -> Result<Self, String> {
        let source = crate::open_file(&path.to_string_lossy())?.reader;
        Self::open(source, path.extension().and_then(|e| e.to_str()))
    }

    /// Open and probe a local file with checksum verification enabled.
    pub fn open_file_verified(path: &std::path::Path) -> Result<Self, String> {
        let source = crate::open_file(&path.to_string_lossy())?.reader;
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
                Err(SymphoniaError::ResetRequired) => {
                    let DefaultTrackDecoder {
                        decoder,
                        track_id,
                        sample_rate,
                        channels,
                        bits_per_sample,
                    } = ProbedTrack::build_decoder_for_default_track(
                        &*track.format,
                        &AudioDecoderOptions::default(),
                    )?;
                    track.decoder = decoder;
                    track.track_id = track_id;
                    track.sample_rate = sample_rate;
                    track.channels = channels;
                    track.bits_per_sample = bits_per_sample;
                    continue;
                }
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
                Err(SymphoniaError::ResetRequired) => {
                    track.decoder.reset();
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    /// Verifies the structural decodability and stream integrity of this track.
    ///
    /// For FLAC files, decodes all packets with checksum verification active and validates
    /// against the embedded 128-bit STREAMINFO MD5 signature upon stream completion.
    /// Because playback ignores `verify_ok` and stops cleanly at EOF, a FLAC MD5 mismatch
    /// or trailing `UnexpectedEof` after audio frames have been decoded is reported as
    /// [`IntegrityCheck::Warning`] rather than [`IntegrityCheck::Corrupt`].
    ///
    /// For containerized and lossy files (MP3, AAC, ALAC, Opus, Vorbis, etc.), decodes initial
    /// boundary packets to ensure frame sync and header validity, then verifies end boundary
    /// decodability to detect truncated or corrupted files without full stream decoding overhead.
    pub fn verify_integrity(mut self) -> IntegrityCheck {
        if self.codec_id() == symphonia::core::codecs::audio::well_known::CODEC_ID_FLAC {
            self.verify_flac_integrity()
        } else {
            self.verify_stream_integrity()
        }
    }

    fn verify_flac_integrity(&mut self) -> IntegrityCheck {
        let track_id = self.track.track_id;
        let expected_frames = self
            .track
            .format
            .tracks()
            .iter()
            .find(|t| t.id == track_id)
            .and_then(|t| t.duration)
            .map(|d| d.get());
        let mut decoded_frames: u64 = 0;
        let mut any_decoded = false;
        let mut eof_after_audio = false;
        loop {
            match self.track.format.next_packet() {
                Ok(Some(packet)) => {
                    if packet.track_id != track_id {
                        continue;
                    }
                    match self.track.decoder.decode(&packet) {
                        Ok(decoded) => {
                            any_decoded = true;
                            decoded_frames += decoded.frames() as u64;
                        }
                        Err(SymphoniaError::DecodeError(e)) => {
                            return IntegrityCheck::Corrupt(format!(
                                "Corrupt FLAC audio frame: {e}"
                            ));
                        }
                        Err(e) => {
                            return IntegrityCheck::Corrupt(format!(
                                "FLAC frame decode error: {e}"
                            ));
                        }
                    }
                }
                Ok(None) => break,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    let severely_truncated = expected_frames.is_some_and(|exp| {
                        decoded_frames.saturating_mul(100) < exp.saturating_mul(95)
                    });
                    if any_decoded && !severely_truncated {
                        eof_after_audio = true;
                        break;
                    }
                    return IntegrityCheck::Corrupt(
                        "Truncated FLAC audio stream: premature end-of-file".to_string(),
                    );
                }
                Err(e) => {
                    return IntegrityCheck::Corrupt(format!("FLAC stream packet error: {e}"));
                }
            }
        }

        if !any_decoded {
            return IntegrityCheck::Corrupt(
                "Zero audio frames decoded from FLAC stream".to_string(),
            );
        }

        let finalize_res = self.track.decoder.finalize();
        if eof_after_audio {
            return IntegrityCheck::Warning(
                "Truncated FLAC audio stream: premature end-of-file".to_string(),
            );
        }
        if let Some(false) = finalize_res.verify_ok {
            return IntegrityCheck::Warning("FLAC MD5 checksum mismatch".to_string());
        }
        IntegrityCheck::Valid
    }

    fn verify_stream_integrity(&mut self) -> IntegrityCheck {
        let track_id = self.track.track_id;
        // 1. Decode initial packets (head)
        let mut initial_packets = 0;
        let mut any_decoded = false;
        while initial_packets < 10 {
            match self.track.format.next_packet() {
                Ok(Some(packet)) => {
                    if packet.track_id != track_id {
                        continue;
                    }
                    initial_packets += 1;
                    match self.track.decoder.decode(&packet) {
                        Ok(_decoded) => {
                            any_decoded = true;
                        }
                        Err(SymphoniaError::DecodeError(e)) => {
                            return IntegrityCheck::Corrupt(format!("Corrupt audio frame: {e}"));
                        }
                        Err(e) => {
                            return IntegrityCheck::Corrupt(format!("Audio decode error: {e}"));
                        }
                    }
                }
                Ok(None) => break,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    if initial_packets == 0 {
                        return IntegrityCheck::Corrupt(
                            "Truncated audio file: premature end-of-file".to_string(),
                        );
                    }
                    break;
                }
                Err(e) => {
                    return IntegrityCheck::Corrupt(format!("Audio stream packet error: {e}"));
                }
            }
        }

        if !any_decoded {
            return IntegrityCheck::Corrupt("Zero audio frames decoded from stream".to_string());
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
                                    return IntegrityCheck::Corrupt(format!(
                                        "Corrupt trailing audio frame: {e}"
                                    ));
                                }
                            }
                        }
                        return IntegrityCheck::Valid;
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
                        return IntegrityCheck::Corrupt(format!("Corrupt audio frame: {e}"));
                    }
                }
                Ok(None) => break,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return IntegrityCheck::Warning(
                        "Truncated audio file: premature end-of-file".to_string(),
                    );
                }
                Err(e) => {
                    return IntegrityCheck::Corrupt(format!("Audio stream packet error: {e}"));
                }
            }
        }

        IntegrityCheck::Valid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Seek, SeekFrom};

    /// A non-seekable `MediaSource` over in-memory bytes, simulating a live
    /// network stream with no known byte length.
    struct NonSeekableBytes(Cursor<Vec<u8>>);

    impl Read for NonSeekableBytes {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl Seek for NonSeekableBytes {
        fn seek(&mut self, _pos: SeekFrom) -> std::io::Result<u64> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "live stream is not seekable",
            ))
        }
    }

    impl MediaSource for NonSeekableBytes {
        fn is_seekable(&self) -> bool {
            false
        }
        fn byte_len(&self) -> Option<u64> {
            None
        }
    }

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

    /// Ogg Icecast stations chain logical Ogg bitstreams back-to-back at track
    /// boundaries, causing Symphonia's Ogg reader to return `ResetRequired`
    /// (#2002). Verify that a chained stream decodes across the boundary
    /// instead of stopping after the first logical stream.
    #[test]
    fn ogg_chained_stream_decodes_across_logical_bitstream_boundary() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/tests/fixtures/codec/opus.ogg");
        let single_bytes = std::fs::read(&path).expect("read opus.ogg");

        let single_decoder = OfflineDecoder::open(
            Box::new(NonSeekableBytes(Cursor::new(single_bytes.clone()))),
            None,
        )
        .expect("open single ogg stream");
        let mut single_frames = 0;
        single_decoder.decode_blocks(2, |planes| single_frames += planes[0].len());
        assert!(single_frames > 0);

        // Concatenate two complete Ogg bitstreams into a single non-seekable live stream.
        let mut chained_bytes = single_bytes.clone();
        chained_bytes.extend_from_slice(&single_bytes);

        let chained_track = ProbedTrack::open(
            Box::new(NonSeekableBytes(Cursor::new(chained_bytes))),
            None,
            Some("application/ogg; codecs=opus"),
        )
        .expect("probe chained ogg stream with Content-Type hint");

        let chained_decoder = OfflineDecoder {
            track: chained_track,
        };
        let mut chained_frames = 0;
        chained_decoder
            .try_decode_blocks(2, |planes| {
                chained_frames += planes[0].len();
                ControlFlow::Continue(())
            })
            .expect("chained ogg stream must not error at boundary");

        assert_eq!(
            chained_frames,
            single_frames * 2,
            "chained Ogg stream should decode both logical bitstreams seamlessly"
        );
    }

    #[test]
    fn normalize_probe_mime_strips_parameters_and_maps_icecast_aliases() {
        assert_eq!(
            normalize_probe_mime("audio/mpeg; charset=utf-8").as_deref(),
            Some("audio/mpeg")
        );
        assert_eq!(
            normalize_probe_mime("Audio/AACP").as_deref(),
            Some("audio/aac")
        );
        assert_eq!(
            normalize_probe_mime("audio/x-mpeg").as_deref(),
            Some("audio/mpeg")
        );
        assert_eq!(
            normalize_probe_mime("application/x-ogg; codecs=vorbis").as_deref(),
            Some("application/ogg")
        );
        assert_eq!(normalize_probe_mime("   "), None);
    }
}
