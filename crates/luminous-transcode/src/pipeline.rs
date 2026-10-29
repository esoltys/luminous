//! Producing a cache entry: decode, resample, encode and tag, or copy and
//! shrink the art.

use crate::rule::Decision;
use crate::source::SourceInfo;
use crate::{art, encode, Error};
use luminous_audio::OfflineDecoder;
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Indexing, Resampler};
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Write the file `decision` calls for to `out`. Never called for
/// `Decision::Copy`.
pub(crate) fn produce(
    info: &SourceInfo,
    decision: &Decision,
    out: &Path,
    cancel: &AtomicBool,
) -> Result<(), Error> {
    match *decision {
        Decision::Copy => unreachable!("copies are not cached"),
        Decision::ShrinkArt { max_px } => {
            std::fs::copy(&info.path, out)?;
            art::shrink_in_place(out, max_px)
        }
        Decision::Convert {
            format,
            bitrate_kbps,
            sample_rate,
            channels,
            max_art_px,
        } => {
            let decoder = OfflineDecoder::open_file(&info.path).map_err(Error::Failed)?;
            let mut resampler =
                Resample::new(decoder.sample_rate(), sample_rate, usize::from(channels))?;
            let mut encoder = encode::open(format, bitrate_kbps, sample_rate, channels, out)?;
            let mut failure = None;
            decoder
                .try_decode_blocks(usize::from(channels), |planes| {
                    if cancel.load(Ordering::Relaxed) {
                        return ControlFlow::Break(());
                    }
                    // A mono stream where the tags promised stereo is
                    // duplicated rather than encoded short of a channel.
                    let planes: Vec<&[f32]> = (0..usize::from(channels))
                        .map(|c| planes.get(c).or(planes.first()).map_or(&[][..], |p| p))
                        .collect();
                    match resampler.push(&planes, encoder.as_mut()) {
                        Ok(()) => ControlFlow::Continue(()),
                        Err(e) => {
                            failure = Some(e);
                            ControlFlow::Break(())
                        }
                    }
                })
                .map_err(|e| Error::Failed(format!("{}: {e}", info.path.display())))?;
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            if let Some(e) = failure {
                return Err(e);
            }
            resampler.finish(encoder.as_mut())?;
            encoder.finish()?;
            art::copy_tags(&info.path, out, max_art_px)
        }
    }
}

/// Streams planar audio from one rate to another, or passes it through when
/// the rates match. Output is trimmed to exactly `input × ratio` frames: the
/// resampler's startup delay and the padding of the last chunk are cut.
struct Resample {
    inner: Option<Fft<f32>>,
    channels: usize,
    ratio: f64,
    pending: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    delay_left: usize,
    frames_in: u64,
    frames_out: u64,
}

const CHUNK: usize = 1024;

impl Resample {
    fn new(from: u32, to: u32, channels: usize) -> Result<Self, Error> {
        let inner = if from == to {
            None
        } else {
            Some(
                Fft::<f32>::new(
                    from as usize,
                    to as usize,
                    CHUNK,
                    channels,
                    FixedSync::Input,
                )
                .map_err(|e| Error::Failed(format!("resampler: {e}")))?,
            )
        };
        let delay_left = inner.as_ref().map_or(0, |r| r.output_delay());
        let output_len = inner.as_ref().map_or(0, |r| r.output_frames_max());
        Ok(Self {
            inner,
            channels,
            ratio: f64::from(to) / f64::from(from),
            pending: vec![Vec::new(); channels],
            output: vec![vec![0.0; output_len]; channels],
            delay_left,
            frames_in: 0,
            frames_out: 0,
        })
    }

    fn push(&mut self, planes: &[&[f32]], sink: &mut dyn encode::Encoder) -> Result<(), Error> {
        let Some(resampler) = &self.inner else {
            return sink.write(planes);
        };
        let needed = resampler.input_frames_next();
        for (pending, plane) in self.pending.iter_mut().zip(planes) {
            pending.extend_from_slice(plane);
        }
        self.frames_in += planes.first().map_or(0, |p| p.len()) as u64;
        while self.pending[0].len() >= needed {
            self.process(None, sink)?;
        }
        Ok(())
    }

    fn finish(&mut self, sink: &mut dyn encode::Encoder) -> Result<(), Error> {
        if self.inner.is_none() {
            return Ok(());
        }
        let expected = (self.frames_in as f64 * self.ratio).ceil() as u64;
        if !self.pending[0].is_empty() {
            self.process(Some(self.pending[0].len()), sink)?;
        }
        // Feed silence until the delayed tail of the audio has come out.
        while self.frames_out < expected {
            let before = self.frames_out;
            self.process(Some(0), sink)?;
            if self.frames_out == before && self.delay_left == 0 {
                break;
            }
        }
        Ok(())
    }

    /// Resample one chunk from the front of `pending`; `partial` is how many
    /// real frames it has when fewer than a whole chunk remain.
    fn process(
        &mut self,
        partial: Option<usize>,
        sink: &mut dyn encode::Encoder,
    ) -> Result<(), Error> {
        let resampler = self.inner.as_mut().expect("only called when resampling");
        let available = partial.unwrap_or_else(|| resampler.input_frames_next());
        let error = |e: &dyn std::fmt::Display| Error::Failed(format!("resampler: {e}"));
        let input = SequentialSliceOfVecs::new(&self.pending, self.channels, available)
            .map_err(|e| error(&e))?;
        let capacity = self.output[0].len();
        let mut output = SequentialSliceOfVecs::new_mut(&mut self.output, self.channels, capacity)
            .map_err(|e| error(&e))?;
        let indexing = Indexing {
            partial_len: partial,
            ..Indexing::default()
        };
        let (_, written) = resampler
            .process_into_buffer(&input, &mut output, Some(&indexing))
            .map_err(|e| error(&e))?;
        for pending in &mut self.pending {
            pending.drain(..available);
        }

        let skip = self.delay_left.min(written);
        self.delay_left -= skip;
        let allowed = ((self.frames_in as f64 * self.ratio).ceil() as u64)
            .saturating_sub(self.frames_out) as usize;
        let count = (written - skip).min(allowed);
        if count == 0 {
            return Ok(());
        }
        self.frames_out += count as u64;
        let planes: Vec<&[f32]> = self.output.iter().map(|o| &o[skip..skip + count]).collect();
        sink.write(&planes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Collect(Vec<Vec<f32>>);

    impl encode::Encoder for Collect {
        fn write(&mut self, planes: &[&[f32]]) -> Result<(), Error> {
            self.0.resize(planes.len(), Vec::new());
            for (all, plane) in self.0.iter_mut().zip(planes) {
                all.extend_from_slice(plane);
            }
            Ok(())
        }

        fn finish(self: Box<Self>) -> Result<(), Error> {
            Ok(())
        }
    }

    fn sine(rate: u32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| (i as f32 * 1000.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5)
            .collect()
    }

    #[test]
    fn resampling_yields_exactly_the_scaled_length_without_leading_silence() {
        let input = sine(44_100, 44_100 + 777);
        let mut resample = Resample::new(44_100, 48_000, 1).unwrap();
        let mut out = Collect::default();
        for block in input.chunks(1152) {
            resample.push(&[block], &mut out).unwrap();
        }
        resample.finish(&mut out).unwrap();
        let expected = ((44_100 + 777) as f64 * 48_000.0 / 44_100.0).ceil() as usize;
        assert_eq!(out.0[0].len(), expected);
        // The delay is trimmed: the sine starts at once, not after silence.
        let early_peak = out.0[0][..48].iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(early_peak > 0.3, "peak in the first ms was {early_peak}");
    }

    #[test]
    fn matching_rates_pass_through() {
        let input = sine(48_000, 1000);
        let mut resample = Resample::new(48_000, 48_000, 1).unwrap();
        let mut out = Collect::default();
        resample.push(&[&input], &mut out).unwrap();
        resample.finish(&mut out).unwrap();
        assert_eq!(out.0[0], input);
    }
}
