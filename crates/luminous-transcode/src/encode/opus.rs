//! Ogg Opus with the vendored libopus (RFC 7845 framing).

use super::{interleave, Encoder};
use crate::Error;
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use opusic_sys as ffi;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Opus always runs at 48 kHz; the pipeline resamples to it.
const RATE: u32 = 48_000;
/// 20 ms, libopus's recommended frame for music.
const FRAME: usize = 960;
/// Largest packet libopus can produce for one frame.
const MAX_PACKET: usize = 4000;
const SERIAL: u32 = 0x4C55_4D4E;

pub(crate) struct OpusEncoder {
    encoder: *mut ffi::OpusEncoder,
    writer: PacketWriter<'static, BufWriter<File>>,
    channels: usize,
    /// Encoder delay, which players skip: RFC 7845's pre-skip.
    pre_skip: u64,
    /// Interleaved samples waiting for a whole frame.
    pending: Vec<f32>,
    scratch: Vec<f32>,
    packet: Vec<u8>,
    /// The last encoded packet, held back so the final one can end the
    /// stream, with its granule position.
    held: Option<(Vec<u8>, u64)>,
    /// Frames of real audio received.
    input_frames: u64,
    /// Frames encoded, including padding.
    encoded_frames: u64,
}

impl OpusEncoder {
    pub fn create(bitrate_kbps: u32, channels: u8, out: &Path) -> Result<Self, Error> {
        let mut status = 0;
        // SAFETY: plain constructor; the result is checked before use and
        // owned (destroyed in Drop) by the returned value.
        let encoder = unsafe {
            ffi::opus_encoder_create(
                RATE as i32,
                i32::from(channels),
                ffi::OPUS_APPLICATION_AUDIO,
                &mut status,
            )
        };
        if encoder.is_null() || status != ffi::OPUS_OK {
            return Err(Error::Failed(format!("opus encoder: error {status}")));
        }
        let mut this = Self {
            encoder,
            writer: PacketWriter::new(BufWriter::new(File::create(out)?)),
            channels: usize::from(channels),
            pre_skip: 0,
            pending: Vec::new(),
            scratch: Vec::new(),
            packet: vec![0; MAX_PACKET],
            held: None,
            input_frames: 0,
            encoded_frames: 0,
        };
        let mut lookahead: i32 = 0;
        // SAFETY: `encoder` is valid; these requests take an i32 and an
        // *mut i32 respectively, as libopus's varargs ctl expects.
        unsafe {
            ffi::opus_encoder_ctl(
                encoder,
                ffi::OPUS_SET_BITRATE_REQUEST,
                (bitrate_kbps * 1000) as i32,
            );
            ffi::opus_encoder_ctl(
                encoder,
                ffi::OPUS_GET_LOOKAHEAD_REQUEST,
                &mut lookahead as *mut i32,
            );
        }
        this.pre_skip = lookahead.max(0) as u64;
        this.write_headers()?;
        Ok(this)
    }

    fn write_headers(&mut self) -> Result<(), Error> {
        let mut head = b"OpusHead".to_vec();
        head.push(1); // version
        head.push(self.channels as u8);
        head.extend_from_slice(&(self.pre_skip as u16).to_le_bytes());
        head.extend_from_slice(&RATE.to_le_bytes());
        head.extend_from_slice(&0i16.to_le_bytes()); // output gain
        head.push(0); // channel mapping family: mono or stereo
        self.writer
            .write_packet(head, SERIAL, PacketWriteEndInfo::EndPage, 0)?;

        // Tags are written afterwards with the source's, through lofty.
        let vendor = b"libopus";
        let mut tags = b"OpusTags".to_vec();
        tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        tags.extend_from_slice(vendor);
        tags.extend_from_slice(&0u32.to_le_bytes());
        self.writer
            .write_packet(tags, SERIAL, PacketWriteEndInfo::EndPage, 0)?;
        Ok(())
    }

    /// Encode one frame from the front of `pending`, which holds at least
    /// one frame.
    fn encode_frame(&mut self) -> Result<(), Error> {
        // SAFETY: `pending` holds FRAME interleaved frames of `channels`
        // samples; `packet` has MAX_PACKET bytes, the size passed.
        let len = unsafe {
            ffi::opus_encode_float(
                self.encoder,
                self.pending.as_ptr(),
                FRAME as i32,
                self.packet.as_mut_ptr(),
                MAX_PACKET as i32,
            )
        };
        if len < 0 {
            return Err(Error::Failed(format!("opus encode: error {len}")));
        }
        self.pending.drain(..FRAME * self.channels);
        self.encoded_frames += FRAME as u64;
        let packet = self.packet[..len as usize].to_vec();
        if let Some((previous, granule)) = self.held.replace((packet, self.encoded_frames)) {
            self.writer.write_packet(
                previous,
                SERIAL,
                PacketWriteEndInfo::NormalPacket,
                granule,
            )?;
        }
        Ok(())
    }
}

impl Encoder for OpusEncoder {
    fn write(&mut self, planes: &[&[f32]]) -> Result<(), Error> {
        interleave(planes, &mut self.scratch);
        self.pending.extend_from_slice(&self.scratch);
        self.input_frames += planes.first().map_or(0, |p| p.len()) as u64;
        while self.pending.len() >= FRAME * self.channels {
            self.encode_frame()?;
        }
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), Error> {
        // Pad with silence until the encoder's delay has been flushed past
        // the last real sample; the final granule trims the padding.
        let end = self.pre_skip + self.input_frames;
        while self.encoded_frames < end || !self.pending.is_empty() {
            self.pending.resize(FRAME * self.channels, 0.0);
            self.encode_frame()?;
        }
        if let Some((last, _)) = self.held.take() {
            self.writer
                .write_packet(last, SERIAL, PacketWriteEndInfo::EndStream, end)?;
        }
        std::io::Write::flush(self.writer.inner_mut())?;
        Ok(())
    }
}

impl Drop for OpusEncoder {
    fn drop(&mut self) {
        // SAFETY: created by opus_encoder_create and destroyed only here.
        unsafe { ffi::opus_encoder_destroy(self.encoder) };
    }
}
