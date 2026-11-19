//! Shared Symphonia codec registry.
//!
//! Symphonia has no native Opus decoder (#1121): `symphonia::default::get_codecs()`
//! can probe an Ogg/Opus track but can't decode it. This registry starts from the
//! same set of codecs Symphonia would normally enable, then adds the
//! libopus-backed adapter on top, and Luminous's own AAC decoder in place of
//! Symphonia's (`aac.rs`), so it must be used everywhere a decoder is
//! constructed (playback, offline waveform/spectrum decode, R128 analysis).

use std::sync::LazyLock;
use symphonia::core::codecs::registry::CodecRegistry;

pub(crate) static CODEC_REGISTRY: LazyLock<CodecRegistry> = LazyLock::new(|| {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    registry.register_audio_decoder::<symphonia_adapter_libopus::OpusDecoder>();
    // Replaces Symphonia's AAC decoder: it wraps it, and adds full HE-AAC
    // where the platform can decode it (#2011).
    registry.register_audio_decoder::<crate::aac::AacDecoder>();
    registry
});

/// Single authority on whether the audio engine can decode a live stream with
/// the given station directory `codec` string and `hls` flag (#2002).
///
/// Supported today: MP3, Ogg Vorbis, Ogg Opus (via libopus), FLAC, WAV/PCM,
/// AIFF, Matroska/WebM, AAC-LC (in ADTS or M4A) and HE-AAC / AAC+ (`"AAC+"`,
/// `"AACP"`, `"HE-AAC"`), at reduced quality where the platform has no full
/// HE-AAC decoder (see [`plays_at_reduced_quality`]).
/// Unsupported: any HLS stream (`hls == true`). Case- and
/// whitespace-insensitive; accepts multi-token Radio Browser codec strings
/// (e.g. `"OGG,VORBIS"` or `"MP3"`).
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
                | "aac+"
                | "aacp"
                | "he-aac"
                | "heaac"
                | "he-aacv2"
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

/// True when a playable stream labelled `codec` is HE-AAC and this platform
/// decodes only its AAC core: no high band above about 11 kHz, and HE-AACv2
/// stations in mono (#2011). Label-based, so a station mislabelled plain
/// "AAC" isn't flagged; playback still decodes whatever the stream really is.
pub fn plays_at_reduced_quality(codec: &str, hls: bool) -> bool {
    can_play_stream(codec, hls) && !crate::aac::decodes_full_he_aac() && is_he_aac_label(codec)
}

fn is_he_aac_label(codec: &str) -> bool {
    codec
        .to_ascii_lowercase()
        .split(|c: char| c == ',' || c == ';' || c == '/' || c.is_ascii_whitespace())
        .any(|t| matches!(t, "aac+" | "aacp" | "he-aac" | "heaac" | "he-aacv2"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_play_stream_accepts_supported_codecs_and_rejects_hls() {
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
            "AAC+",
            "aac+",
            "AACP",
            "aacp",
            "HE-AAC",
            "HE-AACv2",
            "AAC+,MP3",
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

        for unsupported in ["", "   ", "UNKNOWN", "WMA", "FLV", "H.264", "AAC+,WMA"] {
            assert!(
                !can_play_stream(unsupported, false),
                "expected {unsupported:?} to be rejected"
            );
        }
    }

    #[test]
    fn only_he_aac_labels_play_at_reduced_quality_and_only_without_a_full_decoder() {
        let reduced = !crate::aac::decodes_full_he_aac();
        for codec in ["AAC+", "aacp", "HE-AAC", "HE-AACv2"] {
            assert_eq!(plays_at_reduced_quality(codec, false), reduced, "{codec}");
            assert!(!plays_at_reduced_quality(codec, true), "{codec} over HLS");
        }
        for codec in ["AAC", "MP3", "OGG,OPUS", ""] {
            assert!(!plays_at_reduced_quality(codec, false), "{codec}");
        }
    }
}
