//! HE-AAC through the system or runtime `libavcodec` (`aac` decoder, #2203).
//!
//! Loaded dynamically via `dlopen` so Luminous carries no link-time FFmpeg
//! dependency and falls back cleanly to Symphonia's core AAC-LC decoder when
//! `libavcodec` (or its `aac` decoder) is absent.
//!
//! Fed raw AAC blocks from Symphonia's ADTS demuxer with a 2-byte
//! `AudioSpecificConfig` passed via `AVCodecParameters` (`avcodec_parameters_to_context`),
//! FFmpeg's LGPL `aac` decoder decodes SBR and Parametric Stereo to full-rate
//! stereo float without needing `AVCodecContext` struct offsets across FFmpeg
//! 4.4–9.x sonames. Like [`crate::mf_aac::MfAacDecoder`], this decoder declares
//! `2x` core rate stereo up front so the engine's resampler is sized for HE-AAC
//! from packet 0, and follows actual frame metadata if a low-rate stream turns
//! out to be plain AAC-LC.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::ptr::{self, NonNull};
use std::sync::LazyLock;
use symphonia::core::audio::layouts::{CHANNEL_LAYOUT_MONO, CHANNEL_LAYOUT_STEREO};
use symphonia::core::audio::{
    AsGenericAudioBufferRef, Audio, AudioBuffer, AudioMut, AudioSpec, Channels,
    GenericAudioBufferRef,
};
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, FinalizeResult};
use symphonia::core::codecs::CodecInfo;
use symphonia::core::errors::{Error as SymphoniaError, Result};
use symphonia::core::packet::PacketRef;

const SONAMES: &[&CStr] = &[
    c"libavcodec.so.63",
    c"libavcodec.so.62",
    c"libavcodec.so.61",
    c"libavcodec.so.60",
    c"libavcodec.so.59",
    c"libavcodec.so.58",
];

const ADTS_RATES: [u32; 13] = [
    96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000,
    7_350,
];

const AVMEDIA_TYPE_AUDIO: c_int = 1;
const AV_CODEC_ID_AAC: c_int = 0x15002;
const AV_INPUT_BUFFER_PADDING_SIZE: usize = 64;

const AV_SAMPLE_FMT_S16: c_int = 1;
const AV_SAMPLE_FMT_FLT: c_int = 3;
const AV_SAMPLE_FMT_S16P: c_int = 6;
const AV_SAMPLE_FMT_FLTP: c_int = 8;

/// Leading fields of `AVCodecParameters`, unchanged across FFmpeg 4.4–9.x (`.58`–`.63`).
#[repr(C)]
struct AvCodecParametersHead {
    codec_type: c_int,
    codec_id: c_int,
    codec_tag: u32,
    extradata: *mut u8,
    extradata_size: c_int,
}

/// Leading fields of `AVPacket`, unchanged across FFmpeg 4.4–9.x (`.58`–`.63`).
#[repr(C)]
struct AvPacketHead {
    buf: *mut c_void,
    pts: i64,
    dts: i64,
    data: *mut u8,
    size: c_int,
}

/// Leading fields of `AVFrame`, unchanged across FFmpeg 4.4–9.x (`.58`–`.63`).
#[repr(C)]
struct AvFrameHead {
    data: [*mut u8; 8],
    linesize: [c_int; 8],
    extended_data: *mut *mut u8,
    width: c_int,
    height: c_int,
    nb_samples: c_int,
    format: c_int,
}

/// `AVChannelLayout` introduced in FFmpeg 5.1 (`libavutil` 57.24+).
#[repr(C)]
#[derive(Default)]
struct AvChannelLayout {
    order: c_int,
    nb_channels: c_int,
    map_or_mask: u64,
    opaque: *mut c_void,
}

#[repr(C)]
struct AvCodec {
    _private: [u8; 0],
}

#[repr(C)]
struct AvCodecContext {
    _private: [u8; 0],
}

struct AvCodecApi {
    _handle: NonNull<c_void>,
    aac_codec: NonNull<AvCodec>,
    avcodec_alloc_context3: unsafe extern "C" fn(*const AvCodec) -> *mut AvCodecContext,
    avcodec_open2:
        unsafe extern "C" fn(*mut AvCodecContext, *const AvCodec, *mut *mut c_void) -> c_int,
    avcodec_free_context: unsafe extern "C" fn(*mut *mut AvCodecContext),
    avcodec_flush_buffers: unsafe extern "C" fn(*mut AvCodecContext),
    avcodec_parameters_alloc: unsafe extern "C" fn() -> *mut AvCodecParametersHead,
    avcodec_parameters_free: unsafe extern "C" fn(*mut *mut AvCodecParametersHead),
    avcodec_parameters_to_context:
        unsafe extern "C" fn(*mut AvCodecContext, *const AvCodecParametersHead) -> c_int,
    avcodec_send_packet: unsafe extern "C" fn(*mut AvCodecContext, *const AvPacketHead) -> c_int,
    avcodec_receive_frame: unsafe extern "C" fn(*mut AvCodecContext, *mut AvFrameHead) -> c_int,
    av_mallocz: unsafe extern "C" fn(usize) -> *mut c_void,
    av_packet_alloc: unsafe extern "C" fn() -> *mut AvPacketHead,
    av_packet_free: unsafe extern "C" fn(*mut *mut AvPacketHead),
    av_new_packet: unsafe extern "C" fn(*mut AvPacketHead, c_int) -> c_int,
    av_packet_unref: unsafe extern "C" fn(*mut AvPacketHead),
    av_frame_alloc: unsafe extern "C" fn() -> *mut AvFrameHead,
    av_frame_free: unsafe extern "C" fn(*mut *mut AvFrameHead),
    av_frame_unref: unsafe extern "C" fn(*mut AvFrameHead),
    av_opt_get_int: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut i64) -> c_int,
    av_opt_get_chlayout: Option<
        unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut AvChannelLayout) -> c_int,
    >,
}

// SAFETY: Resolved function pointers and the immutable `AVCodec` descriptor
// live for the process lifetime (the dlopen handle is never closed).
unsafe impl Send for AvCodecApi {}
unsafe impl Sync for AvCodecApi {}

unsafe fn load_sym<T>(handle: *mut c_void, name: &CStr) -> Option<T> {
    let ptr = libc::dlsym(handle, name.as_ptr());
    if ptr.is_null() {
        None
    } else {
        Some(std::mem::transmute_copy(&ptr))
    }
}

fn load_api() -> Option<AvCodecApi> {
    for &soname in SONAMES {
        // SAFETY: `soname` is a static NUL-terminated C string.
        let raw = unsafe { libc::dlopen(soname.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        let Some(handle) = NonNull::new(raw) else {
            continue;
        };
        // SAFETY: `handle` is a valid dlopen handle; we check every symbol for non-null.
        unsafe {
            let find_by_name: Option<unsafe extern "C" fn(*const c_char) -> *mut AvCodec> =
                load_sym(handle.as_ptr(), c"avcodec_find_decoder_by_name");
            let Some(find_by_name) = find_by_name else {
                libc::dlclose(handle.as_ptr());
                continue;
            };
            let Some(aac_codec) = NonNull::new(find_by_name(c"aac".as_ptr())) else {
                log::debug!(
                    "HE-AAC: {} loaded, but built-in `aac` decoder is disabled",
                    soname.to_string_lossy()
                );
                libc::dlclose(handle.as_ptr());
                continue;
            };

            let (
                Some(avcodec_alloc_context3),
                Some(avcodec_open2),
                Some(avcodec_free_context),
                Some(avcodec_flush_buffers),
                Some(avcodec_parameters_alloc),
                Some(avcodec_parameters_free),
                Some(avcodec_parameters_to_context),
                Some(avcodec_send_packet),
                Some(avcodec_receive_frame),
                Some(av_mallocz),
                Some(av_packet_alloc),
                Some(av_packet_free),
                Some(av_new_packet),
                Some(av_packet_unref),
                Some(av_frame_alloc),
                Some(av_frame_free),
                Some(av_frame_unref),
                Some(av_opt_get_int),
            ) = (
                load_sym(handle.as_ptr(), c"avcodec_alloc_context3"),
                load_sym(handle.as_ptr(), c"avcodec_open2"),
                load_sym(handle.as_ptr(), c"avcodec_free_context"),
                load_sym(handle.as_ptr(), c"avcodec_flush_buffers"),
                load_sym(handle.as_ptr(), c"avcodec_parameters_alloc"),
                load_sym(handle.as_ptr(), c"avcodec_parameters_free"),
                load_sym(handle.as_ptr(), c"avcodec_parameters_to_context"),
                load_sym(handle.as_ptr(), c"avcodec_send_packet"),
                load_sym(handle.as_ptr(), c"avcodec_receive_frame"),
                load_sym(handle.as_ptr(), c"av_mallocz"),
                load_sym(handle.as_ptr(), c"av_packet_alloc"),
                load_sym(handle.as_ptr(), c"av_packet_free"),
                load_sym(handle.as_ptr(), c"av_new_packet"),
                load_sym(handle.as_ptr(), c"av_packet_unref"),
                load_sym(handle.as_ptr(), c"av_frame_alloc"),
                load_sym(handle.as_ptr(), c"av_frame_free"),
                load_sym(handle.as_ptr(), c"av_frame_unref"),
                load_sym(handle.as_ptr(), c"av_opt_get_int"),
            )
            else {
                libc::dlclose(handle.as_ptr());
                continue;
            };

            let av_opt_get_chlayout = load_sym(handle.as_ptr(), c"av_opt_get_chlayout");
            log::debug!("HE-AAC: using {} (`aac`)", soname.to_string_lossy());
            return Some(AvCodecApi {
                _handle: handle,
                aac_codec,
                avcodec_alloc_context3,
                avcodec_open2,
                avcodec_free_context,
                avcodec_flush_buffers,
                avcodec_parameters_alloc,
                avcodec_parameters_free,
                avcodec_parameters_to_context,
                avcodec_send_packet,
                avcodec_receive_frame,
                av_mallocz,
                av_packet_alloc,
                av_packet_free,
                av_new_packet,
                av_packet_unref,
                av_frame_alloc,
                av_frame_free,
                av_frame_unref,
                av_opt_get_int,
                av_opt_get_chlayout,
            });
        }
    }
    None
}

fn api() -> Option<&'static AvCodecApi> {
    static API: LazyLock<Option<AvCodecApi>> = LazyLock::new(load_api);
    API.as_ref()
}

/// Whether `libavcodec` with the built-in `aac` decoder can be loaded here.
/// Probed once per process.
pub(crate) fn available() -> bool {
    api().is_some()
}

pub(crate) struct LibavcodecAacDecoder {
    api: &'static AvCodecApi,
    avctx: *mut AvCodecContext,
    pkt: *mut AvPacketHead,
    frame: *mut AvFrameHead,
    params: AudioCodecParameters,
    buf: AudioBuffer<f32>,
    pcm: Vec<f32>,
    rate: u32,
    channels: usize,
    warned_mismatch: bool,
}

// SAFETY: `avctx`, `pkt`, and `frame` are exclusively owned by this decoder;
// every mutating FFmpeg call requires `&mut self`, while `&self` methods only
// read `params` and `buf`.
unsafe impl Send for LibavcodecAacDecoder {}
unsafe impl Sync for LibavcodecAacDecoder {}

impl Drop for LibavcodecAacDecoder {
    fn drop(&mut self) {
        // SAFETY: All three pointers were allocated by FFmpeg and are freed once.
        unsafe {
            (self.api.av_frame_free)(&mut self.frame);
            (self.api.av_packet_free)(&mut self.pkt);
            (self.api.avcodec_free_context)(&mut self.avctx);
        }
    }
}

impl LibavcodecAacDecoder {
    /// Opens FFmpeg's `aac` decoder for an implicit HE-AAC ADTS stream.
    /// Fails with a short diagnostic string so the caller can fall back to Symphonia.
    pub(crate) fn try_new(params: &AudioCodecParameters) -> std::result::Result<Self, String> {
        let api = api().ok_or_else(|| "libavcodec with `aac` decoder not found".to_string())?;
        let core_rate = params.sample_rate.ok_or("no sample rate")?;
        let core_channels = params.channels.as_ref().map_or(2, Channels::count);
        if !(1..=2).contains(&core_channels) {
            return Err(format!("{core_channels} channels"));
        }
        let asc = audio_specific_config(core_rate, core_channels as u8)?;

        // SAFETY: `api.aac_codec` is valid for the process lifetime; we free
        // partially allocated handles on any error path.
        unsafe {
            let avctx = (api.avcodec_alloc_context3)(api.aac_codec.as_ptr());
            if avctx.is_null() {
                return Err("avcodec_alloc_context3 failed".to_string());
            }
            let mut par = (api.avcodec_parameters_alloc)();
            if par.is_null() {
                let mut ctx = avctx;
                (api.avcodec_free_context)(&mut ctx);
                return Err("avcodec_parameters_alloc failed".to_string());
            }
            let extradata = (api.av_mallocz)(asc.len() + AV_INPUT_BUFFER_PADDING_SIZE).cast::<u8>();
            if extradata.is_null() {
                (api.avcodec_parameters_free)(&mut par);
                let mut ctx = avctx;
                (api.avcodec_free_context)(&mut ctx);
                return Err("av_mallocz failed".to_string());
            }
            ptr::copy_nonoverlapping(asc.as_ptr(), extradata, asc.len());
            (*par).codec_type = AVMEDIA_TYPE_AUDIO;
            (*par).codec_id = AV_CODEC_ID_AAC;
            (*par).extradata = extradata;
            (*par).extradata_size = asc.len() as c_int;
            let par_rc = (api.avcodec_parameters_to_context)(avctx, par);
            (api.avcodec_parameters_free)(&mut par);
            if par_rc < 0 {
                let mut ctx = avctx;
                (api.avcodec_free_context)(&mut ctx);
                return Err("avcodec_parameters_to_context failed".to_string());
            }
            if (api.avcodec_open2)(avctx, api.aac_codec.as_ptr(), ptr::null_mut()) < 0 {
                let mut ctx = avctx;
                (api.avcodec_free_context)(&mut ctx);
                return Err("avcodec_open2 failed".to_string());
            }
            let pkt = (api.av_packet_alloc)();
            let frame = (api.av_frame_alloc)();
            if pkt.is_null() || frame.is_null() {
                if !frame.is_null() {
                    let mut f = frame;
                    (api.av_frame_free)(&mut f);
                }
                if !pkt.is_null() {
                    let mut p = pkt;
                    (api.av_packet_free)(&mut p);
                }
                let mut ctx = avctx;
                (api.avcodec_free_context)(&mut ctx);
                return Err("av_packet_alloc/av_frame_alloc failed".to_string());
            }

            let rate = core_rate * 2;
            let mut out_params = params.clone();
            out_params
                .with_sample_rate(rate)
                .with_channels(CHANNEL_LAYOUT_STEREO);
            Ok(Self {
                api,
                avctx,
                pkt,
                frame,
                params: out_params,
                buf: AudioBuffer::new(AudioSpec::new(rate, CHANNEL_LAYOUT_STEREO), 2048),
                pcm: Vec::new(),
                rate,
                channels: 2,
                warned_mismatch: false,
            })
        }
    }

    fn query_output_format(&self) -> (Option<u32>, Option<usize>) {
        let mut rate = 0i64;
        // SAFETY: `avctx` is an `AVCodecContext` (`AVClass` at offset 0).
        // FFmpeg's `options_table.h` names the sample-rate option `"ar"`.
        let got_rate = unsafe {
            ((self.api.av_opt_get_int)(self.avctx.cast(), c"ar".as_ptr(), 0, &mut rate) == 0
                || (self.api.av_opt_get_int)(
                    self.avctx.cast(),
                    c"sample_rate".as_ptr(),
                    0,
                    &mut rate,
                ) == 0)
                && rate > 0
        };
        let out_rate = got_rate.then_some(rate as u32);

        let mut ch = 0i64;
        // SAFETY: Try legacy `ac` / `channels` options first (FFmpeg 4.4–6.x), then `ch_layout` (5.1–9.x).
        let out_ch = unsafe {
            if ((self.api.av_opt_get_int)(self.avctx.cast(), c"ac".as_ptr(), 0, &mut ch) == 0
                || (self.api.av_opt_get_int)(self.avctx.cast(), c"channels".as_ptr(), 0, &mut ch)
                    == 0)
                && ch > 0
            {
                Some(ch as usize)
            } else if let Some(get_chlayout) = self.api.av_opt_get_chlayout {
                let mut layout = AvChannelLayout::default();
                if get_chlayout(self.avctx.cast(), c"ch_layout".as_ptr(), 0, &mut layout) == 0
                    && layout.nb_channels > 0
                {
                    Some(layout.nb_channels as usize)
                } else {
                    None
                }
            } else {
                None
            }
        };
        (out_rate, out_ch)
    }

    fn decode_raw_block(&mut self, raw: &[u8]) -> std::result::Result<(), &'static str> {
        let Ok(len) = c_int::try_from(raw.len()) else {
            return Err("aac (libavcodec): packet too large");
        };

        // SAFETY: `pkt`, `avctx`, and `frame` are owned and valid.
        unsafe {
            if (self.api.av_new_packet)(self.pkt, len) < 0 {
                return Err("aac (libavcodec): packet allocation failed");
            }
            ptr::copy_nonoverlapping(raw.as_ptr(), (*self.pkt).data, raw.len());

            let send_rc = (self.api.avcodec_send_packet)(self.avctx, self.pkt);
            (self.api.av_packet_unref)(self.pkt);
            if send_rc < 0 {
                return Err("aac (libavcodec): send_packet failed");
            }

            loop {
                let rc = (self.api.avcodec_receive_frame)(self.avctx, self.frame);
                if rc == -libc::EAGAIN || rc == AVERROR_EOF {
                    break;
                }
                if rc < 0 {
                    (self.api.av_frame_unref)(self.frame);
                    return Err("aac (libavcodec): receive_frame failed");
                }

                let nb_samples = (*self.frame).nb_samples as usize;
                let format = (*self.frame).format;
                if nb_samples > 0 {
                    let (ctx_rate, ctx_ch) = self.query_output_format();
                    if let Some(r) = ctx_rate {
                        self.rate = r;
                    }
                    if let Some(c) = ctx_ch {
                        self.channels = c;
                    }
                    if !self.warned_mismatch
                        && (Some(self.rate) != self.params.sample_rate || self.channels != 2)
                    {
                        self.warned_mismatch = true;
                        log::warn!(
                            "HE-AAC: libavcodec output {} Hz x {} ch, not the declared {:?} Hz stereo",
                            self.rate,
                            self.channels,
                            self.params.sample_rate
                        );
                    }
                    append_frame_samples(
                        &*self.frame,
                        nb_samples,
                        self.channels,
                        format,
                        &mut self.pcm,
                    )?;
                }
                (self.api.av_frame_unref)(self.frame);
            }
        }
        Ok(())
    }
}

/// FFmpeg's `AVERROR_EOF` (`-MKTAG('E','O','F',' ')`).
const AVERROR_EOF: c_int =
    -((b'E' as c_int) | ((b'O' as c_int) << 8) | ((b'F' as c_int) << 16) | ((b' ' as c_int) << 24));

unsafe fn append_frame_samples(
    frame: &AvFrameHead,
    nb_samples: usize,
    channels: usize,
    format: c_int,
    out: &mut Vec<f32>,
) -> std::result::Result<(), &'static str> {
    if channels == 0 || channels > 8 {
        return Err("aac (libavcodec): unsupported channel count");
    }
    match format {
        AV_SAMPLE_FMT_FLTP => {
            let planes: Vec<&[f32]> = (0..channels)
                .map(|ch| {
                    let ptr = frame.data[ch].cast::<f32>();
                    std::slice::from_raw_parts(ptr, nb_samples)
                })
                .collect();
            out.reserve(nb_samples * channels);
            for i in 0..nb_samples {
                for plane in &planes {
                    out.push(plane[i]);
                }
            }
        }
        AV_SAMPLE_FMT_FLT => {
            let slice =
                std::slice::from_raw_parts(frame.data[0].cast::<f32>(), nb_samples * channels);
            out.extend_from_slice(slice);
        }
        AV_SAMPLE_FMT_S16P => {
            let planes: Vec<&[i16]> = (0..channels)
                .map(|ch| {
                    let ptr = frame.data[ch].cast::<i16>();
                    std::slice::from_raw_parts(ptr, nb_samples)
                })
                .collect();
            out.reserve(nb_samples * channels);
            for i in 0..nb_samples {
                for plane in &planes {
                    out.push(f32::from(plane[i]) / 32768.0);
                }
            }
        }
        AV_SAMPLE_FMT_S16 => {
            let slice =
                std::slice::from_raw_parts(frame.data[0].cast::<i16>(), nb_samples * channels);
            out.extend(slice.iter().map(|&s| f32::from(s) / 32768.0));
        }
        _ => return Err("aac (libavcodec): unsupported sample format"),
    }
    Ok(())
}

/// Two-byte `AudioSpecificConfig` for an AAC-LC core; SBR stays implicit and
/// `libavcodec` picks it up from the raw blocks.
fn audio_specific_config(core_rate: u32, channels: u8) -> std::result::Result<[u8; 2], String> {
    let index = ADTS_RATES
        .iter()
        .position(|&r| r == core_rate)
        .ok_or_else(|| format!("no AAC rate index for {core_rate} Hz"))? as u16;
    let asc = (2u16 << 11) | (index << 7) | (u16::from(channels) << 3);
    Ok(asc.to_be_bytes())
}

fn channels_for(count: usize) -> Channels {
    match count {
        1 => CHANNEL_LAYOUT_MONO,
        _ => CHANNEL_LAYOUT_STEREO,
    }
}

const LIBAVCODEC_AAC_CODEC_INFO: CodecInfo = CodecInfo {
    short_name: "aac",
    long_name: crate::aac::LIBAVCODEC_AAC_LONG_NAME,
    profiles: &[],
};

impl AudioDecoder for LibavcodecAacDecoder {
    fn reset(&mut self) {
        // SAFETY: `avctx` is owned and valid.
        unsafe { (self.api.avcodec_flush_buffers)(self.avctx) };
        self.pcm.clear();
    }

    fn codec_info(&self) -> &CodecInfo {
        &LIBAVCODEC_AAC_CODEC_INFO
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        self.pcm.clear();
        if let Err(msg) = self.decode_raw_block(packet.data) {
            log::debug!("HE-AAC: {msg}");
            return Err(SymphoniaError::DecodeError(msg));
        }

        let spec = AudioSpec::new(self.rate, channels_for(self.channels));
        let frames = self.pcm.len() / self.channels.max(1);
        if *self.buf.spec() != spec {
            self.buf = AudioBuffer::new(spec, frames.max(2048));
        } else if self.buf.capacity() < frames {
            self.buf.grow_capacity(frames);
        }
        self.buf.clear();
        self.buf.render_uninit(Some(frames));
        self.buf
            .copy_from_slice_interleaved(&&self.pcm[..frames * self.channels]);
        Ok(self.buf.as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buf.as_generic_audio_buffer_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::codecs::audio::well_known::CODEC_ID_AAC;

    #[test]
    fn audio_specific_config_describes_the_lc_core() {
        assert_eq!(audio_specific_config(22_050, 2).unwrap(), [0x13, 0x90]);
        assert_eq!(audio_specific_config(24_000, 1).unwrap(), [0x13, 0x08]);
        assert!(audio_specific_config(22_000, 2).is_err());
    }

    #[test]
    fn opens_and_resets_when_libavcodec_is_available() {
        if !available() {
            return;
        }
        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_ID_AAC)
            .with_sample_rate(22_050)
            .with_channels(CHANNEL_LAYOUT_STEREO);
        let mut decoder = LibavcodecAacDecoder::try_new(&params).expect("open libavcodec aac");
        assert_eq!(decoder.codec_params().sample_rate, Some(44_100));
        assert_eq!(
            decoder.codec_info().long_name,
            crate::aac::LIBAVCODEC_AAC_LONG_NAME
        );
        decoder.reset();
    }

    #[test]
    fn decodes_raw_aac_blocks_via_libavcodec_when_available() {
        if !available() {
            return;
        }
        use symphonia::core::packet::PacketRef;
        use symphonia::core::units::{Duration, Timestamp};

        // Two raw AAC-LC blocks from a 22.05 kHz stereo 1 kHz sine wave.
        const FRAME_0: &[u8] = &[
            0xdc, 0x00, 0x4c, 0x61, 0x76, 0x63, 0x36, 0x33, 0x2e, 0x31, 0x2e, 0x31, 0x30, 0x32,
            0x00, 0x42, 0x50, 0x29, 0x86, 0x7d, 0x91, 0x1a, 0xe7, 0x8e, 0x4a, 0x14, 0xd3, 0x26,
            0x6f, 0x89, 0x9c, 0x54, 0x92, 0x49, 0x68, 0x19, 0xd9, 0xd9, 0xd9, 0xd9, 0xd9, 0xd9,
            0xd9, 0xd9, 0xd9, 0xd9, 0xd9, 0xd9, 0xd9, 0xda, 0x4a, 0x64, 0xc7, 0x06, 0x79, 0xab,
            0x56, 0x61, 0x5a, 0x69, 0x5a, 0x69, 0x8c, 0xb3, 0x4f, 0xfa, 0xf5, 0x33, 0x96, 0x6a,
            0xdd, 0x9a, 0xa8, 0xab, 0x7c, 0x1b, 0xe4, 0xc0, 0xc8, 0xb4, 0x30, 0x30, 0x30, 0x30,
            0x30, 0x30, 0x32, 0x24, 0x40, 0xc6, 0xcd, 0x9b, 0x96, 0x59, 0x65, 0x96, 0x59, 0x65,
            0x96, 0x59, 0x65, 0x96, 0x59, 0x65, 0x96, 0x59, 0x65, 0x96, 0x59, 0x65, 0x96, 0x59,
            0x65, 0x96, 0x59, 0x65, 0x96, 0x60, 0x7e, 0x17, 0x3c, 0x72, 0x50, 0xa6, 0x99, 0x33,
            0x7c, 0x4c, 0xe2, 0xa4, 0x92, 0x4b, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x0e,
        ];
        const FRAME_1: &[u8] = &[
            0x21, 0x49, 0x6c, 0xff, 0xff, 0xff, 0xff, 0xfc, 0x61, 0x75, 0x64, 0xb9, 0xb2, 0x5c,
            0xd9, 0x2e, 0x5f, 0x3f, 0xeb, 0xbf, 0xfb, 0xfb, 0x6b, 0x5c, 0x7e, 0x3f, 0xfe, 0x2f,
            0xfd, 0x3c, 0xff, 0xdf, 0xdb, 0x8d, 0x71, 0xff, 0x9f, 0xff, 0xe8, 0xff, 0xe3, 0x8f,
            0xfb, 0xfb, 0x71, 0xae, 0xbf, 0xfb, 0x7f, 0xfe, 0xaf, 0xfd, 0x35, 0xff, 0x7f, 0x8e,
            0x35, 0xd0, 0xfa, 0x2f, 0x03, 0x1a, 0x8a, 0x28, 0x35, 0x06, 0xfb, 0x75, 0xd2, 0x2d,
            0x03, 0x6f, 0xfe, 0xb7, 0xff, 0xe4, 0xa1, 0xf3, 0x84, 0x82, 0x26, 0x14, 0x2f, 0x45,
            0xc5, 0xe8, 0xbe, 0x4f, 0x3d, 0x11, 0x7c, 0xa2, 0x79, 0xe8, 0x8b, 0xe5, 0xc4, 0x8c,
            0xfc, 0x7e, 0x0d, 0xb9, 0x25, 0x96, 0xc1, 0x91, 0xda, 0x56, 0x25, 0x9e, 0xd6, 0x91,
            0xcd, 0xf0, 0xd3, 0xad, 0x29, 0x07, 0x06, 0x0a, 0x33, 0x15, 0x43, 0xd3, 0xd0, 0xf4,
            0xe2, 0x83, 0x3c, 0xc9, 0xd5, 0x45, 0x1c, 0xef, 0x10, 0xaf, 0x50, 0x9f, 0x06, 0xe1,
            0x12, 0x1c, 0x8b, 0x97, 0x2e, 0x45, 0xc8, 0xa3, 0x07, 0xa1, 0xe8, 0x7a, 0x1e, 0x9f,
            0x3f, 0xeb, 0xbf, 0xfb, 0xfb, 0x6b, 0x5c, 0x7e, 0x3f, 0xfe, 0x2f, 0xfd, 0x3c, 0xff,
            0xdf, 0xdb, 0x8d, 0x71, 0xff, 0x9f, 0xff, 0xe8, 0xff, 0xe3, 0x8f, 0xfb, 0xfb, 0x71,
            0xae, 0xbf, 0xfb, 0x7f, 0xfe, 0xaf, 0xfd, 0x35, 0xff, 0x7f, 0x8e, 0x35, 0xd0, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe0,
        ];

        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_ID_AAC)
            .with_sample_rate(22_050)
            .with_channels(CHANNEL_LAYOUT_STEREO);
        let mut decoder = LibavcodecAacDecoder::try_new(&params).expect("open libavcodec aac");

        let mut total_frames = 0usize;
        for block in [FRAME_0, FRAME_1] {
            let pkt = PacketRef::new(0, Timestamp::ZERO, Duration::new(1024), block);
            let buf = decoder.decode_ref(&pkt).expect("decode raw AAC block");
            total_frames += buf.frames();
            if buf.frames() > 0 {
                assert_eq!(buf.spec().rate(), 22_050);
                assert_eq!(buf.spec().channels().count(), 2);
            }
        }
        assert!(total_frames > 0, "expected decoded PCM frames");
    }
}
