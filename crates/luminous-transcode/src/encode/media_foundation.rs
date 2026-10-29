//! AAC (`.m4a`) and MP3 through Windows' own Media Foundation encoders and
//! sink writer. No codec ships with Luminous: Windows N editions without the
//! Media Feature Pack simply report no formats here.

use super::{interleave, Encoder, STANDARD_BITRATES};
use crate::capabilities::FormatCapability;
use crate::Error;
use luminous_devices::TargetFormat;
use std::path::Path;
use windows::core::{Interface, GUID, HSTRING};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

impl From<windows::core::Error> for Error {
    fn from(e: windows::core::Error) -> Self {
        Self::Failed(format!("Media Foundation: {}", e.message()))
    }
}

fn subtype(format: TargetFormat) -> GUID {
    match format {
        TargetFormat::Mp3 => MFAudioFormat_MP3,
        _ => MFAudioFormat_AAC,
    }
}

/// COM and Media Foundation started on this thread for as long as it lives.
struct Session {
    uninit_com: bool,
}

impl Session {
    fn start() -> Result<Self, Error> {
        // SAFETY: balanced in Drop; S_FALSE / RPC_E_CHANGED_MODE mean COM was
        // already initialized here, which serves just as well.
        let uninit_com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        // SAFETY: balanced by MFShutdown in Drop.
        if let Err(e) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) } {
            if uninit_com {
                // SAFETY: balances the successful CoInitializeEx above.
                unsafe { CoUninitialize() };
            }
            return Err(e.into());
        }
        Ok(Self { uninit_com })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: balances `start`.
        unsafe {
            let _ = MFShutdown();
            if self.uninit_com {
                CoUninitialize();
            }
        }
    }
}

/// The output types Windows' encoder for `format` offers.
fn output_types(format: TargetFormat) -> Result<Vec<IMFMediaType>, Error> {
    // SAFETY: plain MF calls on a started session; elements of the returned
    // collection are media types.
    unsafe {
        let collection = MFTranscodeGetAudioOutputAvailableTypes(
            &subtype(format),
            MFT_ENUM_FLAG_ALL.0 as u32 | MFT_ENUM_FLAG_SORTANDFILTER.0 as u32,
            None,
        )?;
        let mut types = Vec::new();
        for i in 0..collection.GetElementCount()? {
            if let Ok(t) = collection.GetElement(i)?.cast::<IMFMediaType>() {
                types.push(t);
            }
        }
        Ok(types)
    }
}

/// `(sample rate, channels, kbps)` of an output type.
fn describe(t: &IMFMediaType) -> Option<(u32, u8, u32)> {
    // SAFETY: attribute reads on a valid media type.
    unsafe {
        let rate = t.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).ok()?;
        let channels = t.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).ok()?;
        let bytes_per_second = t.GetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND).ok()?;
        Some((rate, channels as u8, bytes_per_second * 8 / 1000))
    }
}

pub(crate) fn formats() -> Vec<FormatCapability> {
    let Ok(_session) = Session::start() else {
        return Vec::new();
    };
    [TargetFormat::Aac, TargetFormat::Mp3]
        .into_iter()
        .filter_map(|format| {
            let offered: Vec<u32> = output_types(format)
                .ok()?
                .iter()
                .filter_map(describe)
                .filter(|&(rate, channels, _)| rate == 44_100 && channels == 2)
                .map(|(_, _, kbps)| kbps)
                .collect();
            let bitrates_kbps: Vec<u32> = STANDARD_BITRATES
                .into_iter()
                .filter(|kbps| offered.contains(kbps))
                .collect();
            (!bitrates_kbps.is_empty()).then_some(FormatCapability {
                format,
                bitrates_kbps,
            })
        })
        .collect()
}

pub(crate) struct MfEncoder {
    writer: IMFSinkWriter,
    stream: u32,
    sample_rate: u32,
    channels: usize,
    frames_written: u64,
    scratch: Vec<f32>,
    // Last, so the writer is released before Media Foundation shuts down.
    _session: Session,
}

impl MfEncoder {
    pub fn create(
        format: TargetFormat,
        bitrate_kbps: u32,
        sample_rate: u32,
        channels: u8,
        out: &Path,
    ) -> Result<Self, Error> {
        let session = Session::start()?;
        // The offered bitrates are the stereo 44.1 kHz ones; other layouts
        // may top out lower (mono AAC), so take the closest at or below the
        // request, else the lowest. Several types can share a bitrate, and
        // only the first listed is sure to suit the file sinks (a later one
        // makes the MP4 sink reject the type), so keep enumeration order.
        let candidates: Vec<(u32, IMFMediaType)> = output_types(format)?
            .into_iter()
            .filter_map(|t| match describe(&t) {
                Some((rate, ch, kbps)) if rate == sample_rate && ch == channels => Some((kbps, t)),
                _ => None,
            })
            .collect();
        let chosen = candidates
            .iter()
            .map(|(kbps, _)| *kbps)
            .filter(|&kbps| kbps <= bitrate_kbps)
            .max()
            .or_else(|| candidates.iter().map(|(kbps, _)| *kbps).min());
        let output = candidates
            .into_iter()
            .find(|(kbps, _)| Some(*kbps) == chosen)
            .map(|(_, t)| t)
            .ok_or_else(|| {
                Error::Failed(format!(
                    "no {} encoder for {sample_rate} Hz, {channels} ch, {bitrate_kbps} kbps",
                    format.format_name()
                ))
            })?;
        // SAFETY: MF calls on a started session with valid arguments; the
        // sink writer picks its container from the path's extension.
        unsafe {
            let writer = MFCreateSinkWriterFromURL(&HSTRING::from(out.as_os_str()), None, None)?;
            let stream = writer.AddStream(&output)?;

            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            input.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, sample_rate)?;
            input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, u32::from(channels))?;
            input.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            input.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, u32::from(channels) * 2)?;
            input.SetUINT32(
                &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                sample_rate * u32::from(channels) * 2,
            )?;
            input.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
            writer.SetInputMediaType(stream, &input, None)?;
            writer.BeginWriting()?;
            Ok(Self {
                writer,
                stream,
                sample_rate,
                channels: usize::from(channels),
                frames_written: 0,
                scratch: Vec::new(),
                _session: session,
            })
        }
    }

    /// 100 ns units, MF's clock.
    fn time(&self, frames: u64) -> i64 {
        (frames * 10_000_000 / u64::from(self.sample_rate)) as i64
    }
}

impl Encoder for MfEncoder {
    fn write(&mut self, planes: &[&[f32]]) -> Result<(), Error> {
        let frames = planes.first().map_or(0, |p| p.len());
        if frames == 0 {
            return Ok(());
        }
        interleave(planes, &mut self.scratch);
        let len = (self.scratch.len() * 2) as u32;
        // SAFETY: the buffer is locked for exactly `len` bytes, the size it
        // was created with, and unlocked before it is handed on.
        unsafe {
            let buffer = MFCreateMemoryBuffer(len)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            let out = std::slice::from_raw_parts_mut(data.cast::<i16>(), self.scratch.len());
            for (o, s) in out.iter_mut().zip(&self.scratch) {
                *o = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            }
            buffer.Unlock()?;
            buffer.SetCurrentLength(len)?;

            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            let start = self.time(self.frames_written);
            self.frames_written += frames as u64;
            sample.SetSampleTime(start)?;
            sample.SetSampleDuration(self.time(self.frames_written) - start)?;
            self.writer.WriteSample(self.stream, &sample)?;
        }
        debug_assert_eq!(self.scratch.len(), frames * self.channels);
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<(), Error> {
        // SAFETY: completes the file; the writer is not used afterwards.
        unsafe { self.writer.Finalize()? };
        Ok(())
    }
}
