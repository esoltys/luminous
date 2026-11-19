//! Spike #2011: decode an HE-AAC ADTS capture with Windows' own Media
//! Foundation AAC decoder MFT, to see whether it restores the SBR band (and
//! PS stereo) that Symphonia drops.
//!
//! Usage: `cargo run -p luminous-audio --example heaac_mf -- <file.aac>...`
//! Captures: a few seconds of any `audio/aacp` Shoutcast/Icecast station,
//! e.g. `curl -m 8 -o station.aac <stream url>` (not committed: broadcasts).
//!
//! Prototype only: one ADTS frame per input sample, float output, prints the
//! output format and a spectral rolloff to compare with `heaac_probe`.

#[cfg(not(windows))]
fn main() {
    eprintln!("heaac_mf is Windows-only (Media Foundation)");
}

#[cfg(windows)]
fn main() {
    use std::path::Path;
    // SAFETY: COM/MF are started once for the process and never shut down;
    // fine for a one-shot example.
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_MULTITHREADED,
        );
        windows::Win32::Media::MediaFoundation::MFStartup(
            windows::Win32::Media::MediaFoundation::MF_VERSION,
            windows::Win32::Media::MediaFoundation::MFSTARTUP_NOSOCKET,
        )
        .expect("MFStartup");
    }
    for arg in std::env::args().skip(1) {
        println!("== {arg}");
        let bytes = std::fs::read(Path::new(&arg)).expect("read capture");
        match mf::decode_adts(&bytes) {
            Ok((rate, channels, samples)) => {
                let frames = samples.len() / channels.max(1);
                let left: Vec<f32> = samples.iter().step_by(channels.max(1)).copied().collect();
                let side = if channels == 2 {
                    let (mut l2, mut s2) = (0f64, 0f64);
                    for f in samples.as_chunks::<2>().0 {
                        l2 += f64::from(f[0]).powi(2);
                        s2 += f64::from(f[0] - f[1]).powi(2);
                    }
                    format!(", side/left energy {:.3}", s2 / l2.max(1e-12))
                } else {
                    String::new()
                };
                println!(
                    "  MF decoded: {rate} Hz x {channels} ch, {frames} frames ({:.2} s){side}",
                    frames as f64 / f64::from(rate)
                );
                println!(
                    "  energy above {} Hz (the SBR band): {:.1} dB of total",
                    rate / 4,
                    high_band_db(&left, rate / 4, rate)
                );
            }
            Err(e) => println!("  MF: ERROR {e}"),
        }
    }
}

/// Share of the energy above `cutoff` Hz, in dB. Above half the output rate's
/// Nyquist is exactly what a core-only (Symphonia) decode can't produce.
fn high_band_db(samples: &[f32], cutoff: u32, rate: u32) -> f64 {
    let below = rolloff_power(samples, rate, cutoff);
    10.0 * (1.0 - below).max(1e-12).log10()
}

/// Fraction of the energy below `cutoff` Hz, over Hann-windowed 1024-sample
/// blocks (naive DFT; a diagnostic, not fast).
fn rolloff_power(samples: &[f32], rate: u32, cutoff: u32) -> f64 {
    const N: usize = 1024;
    let mut power = vec![0f64; N / 2];
    for block in samples.as_chunks::<N>().0.iter().skip(4).take(60) {
        for (k, p) in power.iter_mut().enumerate() {
            let (mut re, mut im) = (0f64, 0f64);
            for (n, &s) in block.iter().enumerate() {
                let w = 0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / N as f64).cos();
                let phase = std::f64::consts::TAU * (k * n) as f64 / N as f64;
                re += f64::from(s) * w * phase.cos();
                im -= f64::from(s) * w * phase.sin();
            }
            *p += re * re + im * im;
        }
    }
    let total: f64 = power.iter().sum();
    let edge = (cutoff as usize * N / rate as usize).min(N / 2);
    power[..edge].iter().sum::<f64>() / total.max(1e-30)
}

#[cfg(windows)]
mod mf {
    use std::mem::ManuallyDrop;
    use windows::core::Result;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

    const RATES: [u32; 13] = [
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
    ];

    /// ADTS frames of `bytes`, from the first sync word.
    fn frames(bytes: &[u8]) -> Vec<&[u8]> {
        let mut out = Vec::new();
        let Some(mut i) = bytes
            .windows(2)
            .position(|w| w[0] == 0xFF && w[1] & 0xF6 == 0xF0)
        else {
            return out;
        };
        while i + 7 <= bytes.len() && bytes[i] == 0xFF && bytes[i + 1] & 0xF6 == 0xF0 {
            let len = (usize::from(bytes[i + 3] & 3) << 11)
                | (usize::from(bytes[i + 4]) << 3)
                | usize::from(bytes[i + 5] >> 5);
            if len < 7 || i + len > bytes.len() {
                break;
            }
            out.push(&bytes[i..i + len]);
            i += len;
        }
        out
    }

    /// Decodes every ADTS frame to interleaved f32; returns the decoder's
    /// final output rate and channel count.
    pub fn decode_adts(bytes: &[u8]) -> Result<(u32, usize, Vec<f32>)> {
        let frames = frames(bytes);
        let first = frames
            .first()
            .ok_or_else(|| windows::core::Error::from_hresult(MF_E_INVALIDMEDIATYPE))?;
        let core_rate = RATES[usize::from((first[2] >> 2) & 0x0F)];
        let channel_config = u32::from(((first[2] & 1) << 2) | (first[3] >> 6));

        // SAFETY: plain COM calls on objects this function owns.
        unsafe {
            let mft: IMFTransform =
                CoCreateInstance(&CLSID_MSAACDecMFT, None, CLSCTX_INPROC_SERVER)?;

            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
            input.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, core_rate)?;
            input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channel_config.max(1))?;
            input.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 1)?; // ADTS
                                                          // HEAACWAVEINFO minus its WAVEFORMATEX: payload type 1 (ADTS),
                                                          // profile 0xFE ("no audio profile specified"), no AudioSpecificConfig
                                                          // (ADTS headers carry it).
            input.SetBlob(&MF_MT_USER_DATA, &[1, 0, 0xFE, 0, 0, 0, 0, 0, 0, 0, 0, 0])?;
            mft.SetInputType(0, &input, 0)?;
            set_float_output(&mft)?;

            mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            let mut pcm = Vec::new();
            for frame in &frames {
                let buffer = MFCreateMemoryBuffer(frame.len() as u32)?;
                let mut ptr = std::ptr::null_mut();
                buffer.Lock(&mut ptr, None, None)?;
                std::ptr::copy_nonoverlapping(frame.as_ptr(), ptr, frame.len());
                buffer.Unlock()?;
                buffer.SetCurrentLength(frame.len() as u32)?;
                let sample = MFCreateSample()?;
                sample.AddBuffer(&buffer)?;
                mft.ProcessInput(0, &sample, 0)?;
                drain(&mft, &mut pcm)?;
            }
            mft.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)?;
            drain(&mft, &mut pcm)?;

            let output = mft.GetOutputCurrentType(0)?;
            let rate = output.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?;
            let channels = output.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)? as usize;
            Ok((rate, channels, pcm))
        }
    }

    unsafe fn set_float_output(mft: &IMFTransform) -> Result<()> {
        let mut i = 0;
        loop {
            let candidate = mft.GetOutputAvailableType(0, i)?;
            if candidate.GetGUID(&MF_MT_SUBTYPE)? == MFAudioFormat_Float {
                return mft.SetOutputType(0, &candidate, 0);
            }
            i += 1;
        }
    }

    unsafe fn drain(mft: &IMFTransform, pcm: &mut Vec<f32>) -> Result<()> {
        loop {
            let info = mft.GetOutputStreamInfo(0)?;
            let provides = info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
            let sample = if provides {
                None
            } else {
                let s = MFCreateSample()?;
                s.AddBuffer(&MFCreateMemoryBuffer(info.cbSize.max(64 * 1024))?)?;
                Some(s)
            };
            let mut out = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: ManuallyDrop::new(sample),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            }];
            let mut status = 0;
            let result = mft.ProcessOutput(0, &mut out, &mut status);
            let sample = ManuallyDrop::take(&mut out[0].pSample);
            drop(ManuallyDrop::take(&mut out[0].pEvents));
            match result {
                Ok(()) => {
                    if let Some(sample) = sample {
                        let buffer = sample.ConvertToContiguousBuffer()?;
                        let (mut ptr, mut len) = (std::ptr::null_mut(), 0u32);
                        buffer.Lock(&mut ptr, None, Some(&mut len))?;
                        let floats =
                            std::slice::from_raw_parts(ptr.cast::<f32>(), len as usize / 4);
                        pcm.extend_from_slice(floats);
                        buffer.Unlock()?;
                    }
                }
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => set_float_output(mft)?,
                Err(e) => return Err(e),
            }
        }
    }
}
