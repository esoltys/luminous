//! Spike #2011: what does Symphonia do with an HE-AAC (AAC+) ADTS capture?
//!
//! Usage: `cargo run -p luminous-audio --example heaac_probe -- <file.aac>...`
//! Captures: a few seconds of any `audio/aacp` Shoutcast/Icecast station,
//! e.g. `curl -m 8 -o station.aac <stream url>` (not committed: broadcasts).
//!
//! Prints the ADTS header (core rate and channel config), whether SBR / PS
//! extension payloads are present in the raw frames, and what the decoder
//! actually produced (rate, channels, duration, per-channel RMS).

use luminous_audio::OfflineDecoder;
use std::path::Path;

const RATES: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

struct Adts {
    rate: u32,
    channel_config: u8,
    frames: usize,
    /// Frames whose raw data block ends in an `ID_FIL` carrying `EXT_SBR_DATA`.
    sbr_frames: usize,
}

/// Walks ADTS frames and looks for the SBR extension (`ID_FIL` with
/// `EXT_SBR_DATA` / `EXT_SBR_DATA_CRC`), the implicit signalling HE-AAC uses
/// in ADTS. A heuristic byte scan, good enough to label a capture.
fn scan_adts(bytes: &[u8]) -> Option<Adts> {
    let mut i = bytes
        .windows(2)
        .position(|w| w[0] == 0xFF && w[1] & 0xF6 == 0xF0)?;
    let rate = RATES[usize::from((bytes[i + 2] >> 2) & 0x0F)];
    let channel_config = ((bytes[i + 2] & 1) << 2) | (bytes[i + 3] >> 6);
    let (mut frames, mut sbr_frames) = (0, 0);
    while i + 7 <= bytes.len() && bytes[i] == 0xFF && bytes[i + 1] & 0xF6 == 0xF0 {
        let len = (usize::from(bytes[i + 3] & 3) << 11)
            | (usize::from(bytes[i + 4]) << 3)
            | usize::from(bytes[i + 5] >> 5);
        if len < 7 || i + len > bytes.len() {
            break;
        }
        let header = if bytes[i + 1] & 1 == 0 { 9 } else { 7 };
        let payload = &bytes[i + header..i + len];
        // ID_FIL (3 bits = 6), count (4 bits), then ext type 0xD/0xE (4 bits):
        // look for the bit pattern anywhere byte-aligned or not.
        if has_sbr_fil(payload) {
            sbr_frames += 1;
        }
        frames += 1;
        i += len;
    }
    Some(Adts {
        rate,
        channel_config,
        frames,
        sbr_frames,
    })
}

fn has_sbr_fil(payload: &[u8]) -> bool {
    let bits = payload.len() * 8;
    let bit = |n: usize| (payload[n / 8] >> (7 - n % 8)) & 1;
    let read = |at: usize, n: usize| (0..n).fold(0u32, |v, k| (v << 1) | u32::from(bit(at + k)));
    // The SBR fill element is the last one in the raw data block: ID_FIL,
    // count (escaped past 14), `count` bytes starting with the extension
    // type, then ID_TERM and at most 7 bits of byte-alignment padding. Only
    // accept a candidate whose declared length lands exactly on that ID_TERM.
    (0..bits.saturating_sub(18)).any(|at| {
        if read(at, 3) != 6 {
            return false;
        }
        let mut count = read(at + 3, 4) as usize;
        let mut body = at + 7;
        if count == 15 {
            count = 14 + read(body, 8) as usize;
            body += 8;
        }
        let term = body + count * 8;
        count > 0
            && term + 3 <= bits
            && bits - (term + 3) < 8
            && matches!(read(body, 4), 0xD | 0xE)
            && read(term, 3) == 7
    })
}

fn main() {
    for arg in std::env::args().skip(1) {
        let path = Path::new(&arg);
        println!("== {}", path.display());
        let bytes = std::fs::read(path).expect("read capture");
        match scan_adts(&bytes) {
            Some(a) => println!(
                "  ADTS: core {} Hz, channel_config {}, {} frames, {} with SBR fill",
                a.rate, a.channel_config, a.frames, a.sbr_frames
            ),
            None => println!("  ADTS: no sync word"),
        }
        let decoder = match OfflineDecoder::open_file(path) {
            Ok(d) => d,
            Err(e) => {
                println!("  open: ERROR {e}");
                continue;
            }
        };
        let (rate, channels) = (decoder.sample_rate(), decoder.channels());
        let mut frames = 0usize;
        let mut sum_sq = vec![0f64; channels];
        let mut mono: Vec<f32> = Vec::new();
        decoder.decode_blocks(channels, |planes| {
            frames += planes[0].len();
            if mono.len() < 64 * 1024 {
                mono.extend_from_slice(&planes[0]);
            }
            for (c, plane) in planes.iter().enumerate() {
                sum_sq[c] += plane
                    .iter()
                    .map(|&s| f64::from(s) * f64::from(s))
                    .sum::<f64>();
            }
        });
        let rms: Vec<String> = sum_sq
            .iter()
            .map(|s| format!("{:.4}", (s / frames.max(1) as f64).sqrt()))
            .collect();
        println!(
            "  decoded: {rate} Hz x {channels} ch, {frames} frames ({:.2} s), RMS per channel [{}]",
            frames as f64 / f64::from(rate),
            rms.join(", ")
        );
        println!(
            "  99% spectral rolloff: {:.0} Hz (Nyquist {} Hz)",
            rolloff(&mono, rate, 0.99),
            rate / 2
        );
    }
}

/// Frequency below which `share` of the energy lies, averaged over
/// Hann-windowed 1024-sample blocks (naive DFT; a diagnostic, not fast).
fn rolloff(samples: &[f32], rate: u32, share: f64) -> f64 {
    const N: usize = 1024;
    let mut power = vec![0f64; N / 2];
    for block in samples.as_chunks::<N>().0.iter().skip(4) {
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
    let mut acc = 0.0;
    for (k, p) in power.iter().enumerate() {
        acc += p;
        if acc >= total * share {
            return k as f64 * f64::from(rate) / N as f64;
        }
    }
    f64::from(rate) / 2.0
}
