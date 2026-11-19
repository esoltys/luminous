//! Shared Symphonia codec registry.
//!
//! Symphonia has no native Opus decoder (#1121): `symphonia::default::get_codecs()`
//! can probe an Ogg/Opus track but can't decode it. This registry starts from the
//! same set of codecs Symphonia would normally enable, then adds the
//! libopus-backed adapter on top, so it must be used everywhere a decoder is
//! constructed (playback, offline waveform/spectrum decode, R128 analysis).

use std::sync::LazyLock;
use symphonia::core::codecs::registry::CodecRegistry;

pub(crate) static CODEC_REGISTRY: LazyLock<CodecRegistry> = LazyLock::new(|| {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    registry.register_audio_decoder::<symphonia_adapter_libopus::OpusDecoder>();
    registry
});

/// Single authority on whether the audio engine can decode a live stream with
/// the given station directory `codec` string and `hls` flag (#2002).
///
/// Supported today: MP3, Ogg Vorbis, Ogg Opus (via libopus), FLAC, WAV/PCM,
/// AIFF, Matroska/WebM, and AAC-LC (in ADTS or M4A).
/// Unsupported: any HLS stream (`hls == true`) and HE-AAC / AAC+ (`"AAC+"`,
/// `"AACP"`, `"HE-AAC"`), which Symphonia's AAC decoder rejects when SBR is
/// present. Case- and whitespace-insensitive; accepts multi-token Radio Browser
/// codec strings (e.g. `"OGG,VORBIS"` or `"MP3"`).
pub fn can_play_stream(codec: &str, hls: bool) -> bool {
    if hls {
        return false;
    }
    let normalized = codec.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    let mut any_token = false;
    for token in normalized
        .split(|c: char| c == ',' || c == ';' || c == '/' || c.is_ascii_whitespace())
        .filter(|t| !t.is_empty())
    {
        any_token = true;
        let supported = matches!(
            token,
            "mp3"
                | "mpeg"
                | "ogg"
                | "vorbis"
                | "opus"
                | "flac"
                | "wav"
                | "pcm"
                | "aiff"
                | "aif"
                | "mkv"
                | "webm"
                | "matroska"
                | "aac"
                | "aac-lc"
                | "m4a"
                | "mp4"
                | "alac"
        );
        if !supported {
            return false;
        }
    }
    any_token
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_play_stream_accepts_supported_codecs_and_rejects_hls_and_he_aac() {
        for codec in [
            "MP3",
            "mp3",
            "OGG",
            "Vorbis",
            "OGG,VORBIS",
            "Opus",
            "OGG,OPUS",
            "FLAC",
            "WAV",
            "AIFF",
            "MKV",
            "WebM",
            "AAC",
            "AAC-LC",
            "M4A",
        ] {
            assert!(
                can_play_stream(codec, false),
                "expected {codec} (non-HLS) to be playable"
            );
            assert!(
                !can_play_stream(codec, true),
                "expected {codec} with hls=true to be rejected"
            );
        }

        for unsupported in [
            "", "   ", "UNKNOWN", "AAC+", "aac+", "AACP", "aacp", "HE-AAC", "HE-AACv2", "AAC+,MP3",
            "WMA", "FLV", "H.264",
        ] {
            assert!(
                !can_play_stream(unsupported, false),
                "expected {unsupported:?} to be rejected"
            );
        }
    }
}
