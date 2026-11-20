//! HE-AAC through Windows' own Media Foundation AAC decoder MFT (#2011).
//! It ships with Windows (N editions need the Media Feature Pack), so
//! Luminous bundles no codec. Fed the raw AAC blocks Symphonia's ADTS reader
//! demuxes, it decodes SBR and Parametric Stereo to full-rate stereo float.
//!
//! The MFT announces its real output format only after the first frames
//! (`MF_E_TRANSFORM_STREAM_CHANGE`): the core rate first, then twice that in
//! stereo once it has seen SBR. Callers fix their resampler from
//! `codec_params()` at open, so this decoder declares the HE-AAC output it
//! will produce (2x the core rate, stereo) from the start. Should the MFT
//! still disagree (a plain LC stream at a low rate), buffers carry the MFT's
//! actual spec and playback follows the buffer's rate.

use std::mem::ManuallyDrop;
use std::sync::LazyLock;
use symphonia::core::audio::layouts::CHANNEL_LAYOUT_STEREO;
use symphonia::core::audio::{
    AsGenericAudioBufferRef, Audio, AudioBuffer, AudioMut, AudioSpec, Channels,
    GenericAudioBufferRef,
};
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, FinalizeResult};
use symphonia::core::codecs::registry::RegisterableAudioDecoder;
use symphonia::core::codecs::CodecInfo;
use symphonia::core::errors::{Error as SymphoniaError, Result};
use symphonia::core::packet::PacketRef;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{
    CoCreateInstance, CoDecrementMTAUsage, CoIncrementMTAUsage, CLSCTX_INPROC_SERVER,
    CO_MTA_USAGE_COOKIE,
};

fn mf_error(context: &str, e: windows::core::Error) -> String {
    format!("{context}: {}", e.message())
}

/// Whether the AAC decoder MFT can be created here. Probed once.
pub(crate) fn available() -> bool {
    static AVAILABLE: LazyLock<bool> = LazyLock::new(|| {
        Session::start()
            .and_then(|_session| {
                // SAFETY: plain COM creation in the MTA the session holds open.
                // The probe MFT is released inside the closure, while the
                // session is still up: releasing it after MFShutdown and the
                // MTA teardown crashes the process.
                unsafe {
                    CoCreateInstance::<_, IMFTransform>(
                        &CLSID_MSAACDecMFT,
                        None,
                        CLSCTX_INPROC_SERVER,
                    )
                }
                .map(drop)
            })
            .is_ok()
    });
    *AVAILABLE
}

/// Keeps the process MTA and Media Foundation up for as long as it lives,
/// whichever thread creates, drives or drops the decoder.
struct Session {
    mta: CO_MTA_USAGE_COOKIE,
}

impl Session {
    fn start() -> windows::core::Result<Self> {
        // SAFETY: balanced by CoDecrementMTAUsage in Drop.
        let mta = unsafe { CoIncrementMTAUsage() }?;
        // SAFETY: balanced by MFShutdown in Drop; MFStartup is refcounted.
        if let Err(e) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) } {
            // SAFETY: balances the increment above.
            let _ = unsafe { CoDecrementMTAUsage(mta) };
            return Err(e);
        }
        Ok(Self { mta })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: balances Session::start.
        unsafe {
            let _ = MFShutdown();
            let _ = CoDecrementMTAUsage(self.mta);
        }
    }
}

pub(crate) struct MfAacDecoder {
    // Declared before `_session` so the MFT is released while MF is still up.
    mft: IMFTransform,
    params: AudioCodecParameters,
    buf: AudioBuffer<f32>,
    /// Interleaved output of the current packet.
    pcm: Vec<f32>,
    /// The MFT's current output format.
    rate: u32,
    channels: usize,
    _session: Session,
}

// SAFETY: the AAC decoder MFT is a free-threaded in-proc object, and the
// Session keeps the process MTA alive, so its interface may be called from
// any thread. Every call needs `&mut self`, so there are never two at once;
// `&self` methods only read `params` and `buf`, never the MFT.
unsafe impl Send for MfAacDecoder {}
unsafe impl Sync for MfAacDecoder {}

impl MfAacDecoder {
    /// Fails with why Media Foundation can't take this stream; the caller
    /// falls back to the core decoder.
    pub(crate) fn try_new(params: &AudioCodecParameters) -> std::result::Result<Self, String> {
        let core_rate = params.sample_rate.ok_or("no sample rate")?;
        let core_channels = params.channels.as_ref().map_or(2, Channels::count);
        if !(1..=2).contains(&core_channels) {
            return Err(format!("{core_channels} channels"));
        }
        let user_data = user_data(core_rate, core_channels as u8)?;
        let session = Session::start().map_err(|e| mf_error("startup", e))?;
        // SAFETY: plain COM calls on objects this function owns, in the MTA
        // the session holds open.
        let mft = unsafe {
            let mft: IMFTransform =
                CoCreateInstance(&CLSID_MSAACDecMFT, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| mf_error("create decoder", e))?;
            let input = MFCreateMediaType().map_err(|e| mf_error("media type", e))?;
            let configure = || -> windows::core::Result<()> {
                input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
                input.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, core_rate)?;
                input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, core_channels as u32)?;
                input.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?; // raw blocks
                input.SetBlob(&MF_MT_USER_DATA, &user_data)?;
                mft.SetInputType(0, &input, 0)?;
                set_float_output(&mft)?;
                mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
            };
            configure().map_err(|e| mf_error("configure", e))?;
            mft
        };

        let rate = core_rate * 2;
        let mut params = params.clone();
        params
            .with_sample_rate(rate)
            .with_channels(CHANNEL_LAYOUT_STEREO);
        Ok(Self {
            mft,
            params,
            buf: AudioBuffer::new(AudioSpec::new(rate, CHANNEL_LAYOUT_STEREO), 2048),
            pcm: Vec::new(),
            rate,
            channels: 2,
            _session: session,
        })
    }

    /// Pulls every output sample the MFT has ready into `self.pcm`.
    fn drain(&mut self) -> windows::core::Result<()> {
        loop {
            // SAFETY: COM calls on the owned MFT; output buffers are owned
            // here and released through ManuallyDrop::take.
            unsafe {
                let info = self.mft.GetOutputStreamInfo(0)?;
                let provides = info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
                let sample = if provides {
                    None
                } else {
                    let sample = MFCreateSample()?;
                    sample.AddBuffer(&MFCreateMemoryBuffer(info.cbSize.max(32 * 1024))?)?;
                    Some(sample)
                };
                let mut out = [MFT_OUTPUT_DATA_BUFFER {
                    dwStreamID: 0,
                    pSample: ManuallyDrop::new(sample),
                    dwStatus: 0,
                    pEvents: ManuallyDrop::new(None),
                }];
                let mut status = 0;
                let result = self.mft.ProcessOutput(0, &mut out, &mut status);
                let sample = ManuallyDrop::take(&mut out[0].pSample);
                drop(ManuallyDrop::take(&mut out[0].pEvents));
                match result {
                    Ok(()) => {
                        if let Some(sample) = sample {
                            let buffer = sample.ConvertToContiguousBuffer()?;
                            let (mut ptr, mut len) = (std::ptr::null_mut(), 0u32);
                            buffer.Lock(&mut ptr, None, Some(&mut len))?;
                            self.pcm.extend_from_slice(std::slice::from_raw_parts(
                                ptr.cast::<f32>(),
                                len as usize / size_of::<f32>(),
                            ));
                            buffer.Unlock()?;
                        }
                    }
                    Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                    Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                        let output = set_float_output(&self.mft)?;
                        self.rate = output.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?;
                        self.channels = output.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)? as usize;
                        if Some(self.rate) != self.params.sample_rate || self.channels != 2 {
                            log::warn!(
                                "HE-AAC: decoder output {} Hz x {} ch, not the declared {:?} Hz stereo",
                                self.rate,
                                self.channels,
                                self.params.sample_rate
                            );
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }

    fn feed(&mut self, data: &[u8]) -> windows::core::Result<()> {
        // SAFETY: COM calls on the owned MFT and a buffer created here;
        // the copy stays within the buffer's `data.len()` bytes.
        unsafe {
            let buffer = MFCreateMemoryBuffer(data.len() as u32)?;
            let mut ptr = std::ptr::null_mut();
            buffer.Lock(&mut ptr, None, None)?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(data.len() as u32)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            match self.mft.ProcessInput(0, &sample, 0) {
                Err(e) if e.code() == MF_E_NOTACCEPTING => {
                    self.drain()?;
                    self.mft.ProcessInput(0, &sample, 0)
                }
                other => other,
            }
        }
    }
}

/// The MFT's `MF_MT_USER_DATA` for raw AAC: the `HEAACWAVEINFO` fields after
/// its `WAVEFORMATEX` (payload type 0 = raw, profile 0xFE = unspecified),
/// then a two-byte AudioSpecificConfig for the AAC-LC core.
fn user_data(core_rate: u32, channels: u8) -> std::result::Result<[u8; 14], String> {
    let [hi, lo] = crate::aac::audio_specific_config(core_rate, channels)?;
    Ok([0, 0, 0xFE, 0, 0, 0, 0, 0, 0, 0, 0, 0, hi, lo])
}

/// Selects the MFT's float PCM output and returns it.
unsafe fn set_float_output(mft: &IMFTransform) -> windows::core::Result<IMFMediaType> {
    let mut index = 0;
    loop {
        // Runs out with MF_E_NO_MORE_TYPES if there's no float output.
        let candidate = mft.GetOutputAvailableType(0, index)?;
        if candidate.GetGUID(&MF_MT_SUBTYPE)? == MFAudioFormat_Float {
            mft.SetOutputType(0, &candidate, 0)?;
            return Ok(candidate);
        }
        index += 1;
    }
}

fn channels_for(count: usize) -> Channels {
    match count {
        1 => symphonia::core::audio::layouts::CHANNEL_LAYOUT_MONO,
        _ => CHANNEL_LAYOUT_STEREO,
    }
}

const MF_AAC_CODEC_INFO: symphonia::core::codecs::CodecInfo = symphonia::core::codecs::CodecInfo {
    short_name: "aac",
    long_name: crate::aac::MF_AAC_LONG_NAME,
    profiles: &[],
};

impl AudioDecoder for MfAacDecoder {
    fn reset(&mut self) {
        // SAFETY: COM call on the owned MFT.
        let _ = unsafe { self.mft.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0) };
        self.pcm.clear();
    }

    fn codec_info(&self) -> &CodecInfo {
        &MF_AAC_CODEC_INFO
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        self.pcm.clear();
        if let Err(e) = self.feed(packet.data).and_then(|()| self.drain()) {
            log::debug!("HE-AAC: {}", mf_error("decode", e));
            return Err(SymphoniaError::DecodeError(
                "aac (media foundation): decode failed",
            ));
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
    #[test]
    fn audio_specific_config_describes_the_lc_core() {
        // 22.05 kHz stereo LC: 00010 0111 0010 000 = 0x13 0x90.
        assert_eq!(super::user_data(22_050, 2).unwrap()[12..], [0x13, 0x90]);
        // 24 kHz mono: 00010 0110 0001 000 = 0x13 0x08.
        assert_eq!(super::user_data(24_000, 1).unwrap()[12..], [0x13, 0x08]);
        assert!(super::user_data(22_000, 2).is_err());
    }
}
