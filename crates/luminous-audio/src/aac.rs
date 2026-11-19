//! Luminous's AAC decoder: Symphonia's AAC-LC decoder, plus a full HE-AAC
//! path where the platform has one (#2011).
//!
//! Symphonia doesn't implement SBR or Parametric Stereo. An HE-AAC stream
//! whose SBR is signalled implicitly (every ADTS Shoutcast/Icecast "AAC+"
//! station) still decodes, but only its half-rate AAC core: the high band is
//! missing and HE-AACv2 comes out mono. On Windows, those streams go to
//! Media Foundation's AAC decoder instead, which decodes SBR and PS.
//!
//! Routing is on the bitstream, not on a station's codec label (labels are
//! wrong both ways): an ADTS stream (no AudioSpecificConfig) whose core rate
//! is 24 kHz or less is HE-AAC in practice, since plain AAC-LC that low is
//! rare, and Media Foundation decodes plain LC correctly anyway. Files with an
//! AudioSpecificConfig (M4A) always stay on Symphonia, whose demuxer handles
//! their gapless trimming.

use symphonia::core::audio::{
    AsGenericAudioBufferRef, Audio, AudioBuffer, AudioMut, AudioSpec, GenericAudioBufferRef,
};
use symphonia::core::codecs::audio::well_known::CODEC_ID_AAC;
use symphonia::core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult,
};
use symphonia::core::codecs::registry::{RegisterableAudioDecoder, SupportedAudioCodec};
use symphonia::core::codecs::CodecInfo;
use symphonia::core::errors::Result;
use symphonia::core::packet::PacketRef;

/// Highest ADTS core rate routed to the full HE-AAC decoder.
const HE_AAC_MAX_CORE_RATE: u32 = 24_000;

/// True when the stream described by `params` is HE-AAC in practice: ADTS
/// (no AudioSpecificConfig) at a core rate of 24 kHz or less.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_implicit_he_aac(params: &AudioCodecParameters) -> bool {
    params.extra_data.is_none()
        && params
            .sample_rate
            .is_some_and(|rate| rate <= HE_AAC_MAX_CORE_RATE)
}

/// Whether this platform decodes HE-AAC at full quality (SBR and PS), rather
/// than the half-rate core only. Checked once per process.
pub fn decodes_full_he_aac() -> bool {
    #[cfg(windows)]
    {
        crate::mf_aac::available()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// The registered AAC decoder: Symphonia's core decoder (below), unless
/// routing hands the stream to Media Foundation (see the module docs).
impl RegisterableAudioDecoder for AacDecoder {
    fn try_registry_new(
        params: &AudioCodecParameters,
        opts: &AudioDecoderOptions,
    ) -> Result<Box<dyn AudioDecoder>> {
        #[cfg(windows)]
        if is_implicit_he_aac(params) {
            match crate::mf_aac::MfAacDecoder::try_new(params) {
                Ok(decoder) => return Ok(Box::new(decoder)),
                Err(e) => log::warn!("HE-AAC: Media Foundation unavailable, core only: {e}"),
            }
        }
        Ok(Box::new(Self::try_new(params, opts)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        // Spelled out: `support_audio_codec!` names `symphonia_core`, which
        // this crate only reaches through `symphonia`.
        const SUPPORTED: &[SupportedAudioCodec] = &[SupportedAudioCodec {
            id: CODEC_ID_AAC,
            info: CodecInfo {
                short_name: "aac",
                long_name: "Advanced Audio Coding",
                profiles: &[],
            },
        }];
        SUPPORTED
    }
}

/// Symphonia's AAC decoder (the AAC core only), with the one HE-AAC failure
/// it would otherwise make audible fixed: an HE-AACv2 stream whose ADTS header says stereo but
/// whose frames carry one channel plus Parametric Stereo decodes with a
/// silent right channel. The core is mono then, so it's played on both sides.
pub(crate) struct AacDecoder {
    inner: symphonia::default::codecs::AacDecoder,
    buf: AudioBuffer<f32>,
    scratch: Vec<f32>,
    /// Latched once a packet shows the silent-right pattern, so the image
    /// doesn't flap between packets.
    mono_core: bool,
}

impl AacDecoder {
    fn try_new(params: &AudioCodecParameters, opts: &AudioDecoderOptions) -> Result<Self> {
        let inner = symphonia::default::codecs::AacDecoder::try_new(params, opts)?;
        let spec = AudioSpec::new(
            inner.codec_params().sample_rate.unwrap_or(44_100),
            inner
                .codec_params()
                .channels
                .clone()
                .unwrap_or(symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO),
        );
        Ok(Self {
            inner,
            buf: AudioBuffer::new(spec, 1024),
            scratch: Vec::new(),
            mono_core: false,
        })
    }
}

impl AudioDecoder for AacDecoder {
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn codec_info(&self) -> &CodecInfo {
        self.inner.codec_info()
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        self.inner.codec_params()
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        let decoded = self.inner.decode_ref(packet)?;
        if decoded.spec() != self.buf.spec() {
            self.buf = AudioBuffer::new(decoded.spec().clone(), decoded.frames().max(1024));
        }
        let frames = decoded.frames();
        decoded.copy_to_vec_interleaved(&mut self.scratch);
        if self.buf.capacity() < frames {
            self.buf.grow_capacity(frames);
        }
        self.buf.clear();
        self.buf.render_uninit(Some(frames));
        self.buf.copy_from_slice_interleaved(&self.scratch);
        if let Some((left, right)) = self.buf.plane_pair_mut(0, 1) {
            fill_silent_right(left, right, &mut self.mono_core);
        }
        Ok(self.buf.as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        self.inner.finalize()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buf.as_generic_audio_buffer_ref()
    }
}

/// Copies `left` into `right` when the stream is a mono core in a stereo
/// header: `right` is exact digital silence while `left` isn't. Real stereo
/// never decodes to a bit-exact zero channel under a non-silent one, so the
/// first such packet latches `mono_core` for the rest of the stream.
fn fill_silent_right(left: &[f32], right: &mut [f32], mono_core: &mut bool) {
    if !*mono_core {
        *mono_core = right.iter().all(|&s| s == 0.0) && left.iter().any(|&s| s != 0.0);
    }
    if *mono_core {
        right.copy_from_slice(left);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO;

    #[test]
    fn silent_right_channel_takes_the_mono_core_and_stays_latched() {
        let mut mono_core = false;
        let left = [0.1, -0.2, 0.3];
        let mut right = [0.0; 3];
        fill_silent_right(&left, &mut right, &mut mono_core);
        assert!(mono_core);
        assert_eq!(right, left);

        // A later packet that happens to have a quiet left still copies.
        let quiet = [0.0; 3];
        let mut right = [0.5; 3];
        fill_silent_right(&quiet, &mut right, &mut mono_core);
        assert_eq!(right, quiet);
    }

    #[test]
    fn real_stereo_and_digital_silence_are_left_alone() {
        let mut mono_core = false;
        let left = [0.1, -0.2, 0.3];
        let mut right = [0.0, 0.0, 0.01];
        fill_silent_right(&left, &mut right, &mut mono_core);
        assert!(!mono_core);
        assert_eq!(right, [0.0, 0.0, 0.01]);

        let mut right = [0.0; 3];
        fill_silent_right(&[0.0; 3], &mut right, &mut mono_core);
        assert!(!mono_core, "silence on both sides isn't a mono core");
    }

    fn adts_params(rate: u32) -> AudioCodecParameters {
        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_ID_AAC)
            .with_sample_rate(rate)
            .with_channels(CHANNEL_LAYOUT_STEREO);
        params
    }

    #[test]
    fn routes_low_rate_adts_as_he_aac_but_not_full_rate_or_m4a() {
        assert!(is_implicit_he_aac(&adts_params(22_050)));
        assert!(is_implicit_he_aac(&adts_params(24_000)));
        assert!(!is_implicit_he_aac(&adts_params(44_100)));
        let mut m4a = adts_params(22_050);
        m4a.with_extra_data(vec![0x13, 0x10].into_boxed_slice());
        assert!(!is_implicit_he_aac(&m4a));
    }

    /// The decoder reports the rate its buffers carry, so playback's
    /// resampler is built for the real output rate from the first packet.
    #[test]
    fn he_aac_stream_reports_its_output_rate_up_front() {
        let decoder = AacDecoder::try_registry_new(&adts_params(22_050), &Default::default())
            .expect("decoder for an ADTS core");
        let expected = if decodes_full_he_aac() {
            44_100
        } else {
            22_050
        };
        assert_eq!(decoder.codec_params().sample_rate, Some(expected));
        assert_eq!(
            decoder.codec_params().channels.as_ref().map(|c| c.count()),
            Some(2)
        );
    }
}
