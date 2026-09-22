//! Shared Symphonia codec registry.
//!
//! Symphonia has no native Opus decoder (#1121): `symphonia::default::get_codecs()`
//! can probe an Ogg/Opus track but can't decode it. This registry starts from the
//! same set of codecs Symphonia would normally enable, then adds the
//! libopus-backed adapter on top, so it must be used everywhere a decoder is
//! constructed (playback, offline waveform/spectrum decode, R128 analysis).

use std::sync::LazyLock;
use symphonia::core::codecs::registry::CodecRegistry;

pub static CODEC_REGISTRY: LazyLock<CodecRegistry> = LazyLock::new(|| {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    registry.register_audio_decoder::<symphonia_adapter_libopus::OpusDecoder>();
    registry
});
