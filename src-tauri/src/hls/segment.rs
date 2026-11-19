//! Turns HLS media segments into one continuous elementary audio stream the
//! audio engine already decodes (#2012):
//!
//! - Packed audio segments (raw ADTS AAC or MPEG audio behind an ID3 tag)
//!   lose their ID3 tags and are concatenated as they are.
//! - MPEG-TS segments are remuxed at the byte level: the audio PES payloads of
//!   the program's first audio stream are extracted, and video, timed ID3
//!   metadata and every other PID are dropped.
//!
//! The result is an ADTS or MPEG audio byte stream, so no container demuxer is
//! needed downstream.

const TS_PACKET: usize = 188;
const TS_SYNC: u8 = 0x47;

/// The elementary stream a station's segments carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKind {
    /// AAC in ADTS framing (LC or HE-AAC).
    Adts,
    /// MPEG-1/2 audio (MP3).
    Mpeg,
}

impl AudioKind {
    pub fn mime_type(self) -> &'static str {
        match self {
            AudioKind::Adts => "audio/aac",
            AudioKind::Mpeg => "audio/mpeg",
        }
    }
}

/// Audio extracted from one segment.
#[derive(Debug, Default)]
pub struct SegmentAudio {
    pub kind: Option<AudioKind>,
    pub bytes: Vec<u8>,
    /// "Artist - Title" from the segment's ID3 tag, when it names one.
    pub title: Option<String>,
}

/// Stateful segment processor: an MPEG-TS stream's PIDs normally repeat in
/// every segment, but are remembered across segments in case one omits them.
#[derive(Debug, Default)]
pub struct SegmentDemuxer {
    pmt_pid: Option<u16>,
    audio_pid: Option<u16>,
    audio_kind: Option<AudioKind>,
    unsupported_audio: bool,
}

impl SegmentDemuxer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Extracts the audio from one downloaded segment.
    pub fn process(&mut self, data: &[u8]) -> Result<SegmentAudio, String> {
        if is_transport_stream(data) {
            return self.process_ts(data);
        }
        let (title, rest) = strip_id3_tags(data);
        let kind = sniff_frame_kind(rest).ok_or_else(|| {
            "HLS segment is neither MPEG-TS nor ADTS/MPEG audio (fragmented MP4 isn't supported)"
                .to_string()
        })?;
        Ok(SegmentAudio {
            kind: Some(kind),
            bytes: rest.to_vec(),
            title,
        })
    }

    fn process_ts(&mut self, data: &[u8]) -> Result<SegmentAudio, String> {
        let mut out = Vec::with_capacity(data.len());
        let mut in_audio_pes = false;
        for packet in data.as_chunks::<TS_PACKET>().0 {
            if packet[0] != TS_SYNC || packet[1] & 0x80 != 0 {
                continue; // lost sync or transport error: skip the packet
            }
            let pusi = packet[1] & 0x40 != 0;
            let pid = (u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2]);
            let adaptation = (packet[3] >> 4) & 0x3;
            let payload_start = match adaptation {
                1 => 4,
                3 => 5 + usize::from(packet[4]),
                _ => continue, // adaptation field only, or reserved
            };
            if payload_start >= TS_PACKET {
                continue;
            }
            let payload = &packet[payload_start..];

            if pid == 0 {
                if pusi {
                    self.read_pat(payload);
                }
            } else if Some(pid) == self.pmt_pid {
                if pusi {
                    self.read_pmt(payload);
                }
            } else if Some(pid) == self.audio_pid {
                if pusi {
                    in_audio_pes = false;
                    if let Some(es) = pes_payload(payload) {
                        out.extend_from_slice(es);
                        in_audio_pes = true;
                    }
                } else if in_audio_pes {
                    out.extend_from_slice(payload);
                }
            }
        }

        match self.audio_kind {
            Some(kind) => Ok(SegmentAudio {
                kind: Some(kind),
                bytes: out,
                title: None,
            }),
            None if self.unsupported_audio => {
                Err("HLS stream's audio codec isn't supported (only AAC and MP3 are)".to_string())
            }
            None => Err("HLS segment has no audio stream".to_string()),
        }
    }

    fn read_pat(&mut self, payload: &[u8]) {
        let Some(section) = psi_section(payload, 0x00) else {
            return;
        };
        // Program loop: 4 bytes each, after the 8-byte header, before the CRC.
        for entry in section[8..].as_chunks::<4>().0 {
            let program = u16::from_be_bytes([entry[0], entry[1]]);
            if program != 0 {
                self.pmt_pid = Some((u16::from(entry[2] & 0x1f) << 8) | u16::from(entry[3]));
                return;
            }
        }
    }

    fn read_pmt(&mut self, payload: &[u8]) {
        let Some(section) = psi_section(payload, 0x02) else {
            return;
        };
        if section.len() < 12 {
            return;
        }
        let program_info_len = (usize::from(section[10] & 0x0f) << 8) | usize::from(section[11]);
        let mut i = 12 + program_info_len;
        while i + 5 <= section.len() {
            let stream_type = section[i];
            let pid = (u16::from(section[i + 1] & 0x1f) << 8) | u16::from(section[i + 2]);
            let es_info_len =
                (usize::from(section[i + 3] & 0x0f) << 8) | usize::from(section[i + 4]);
            let kind = match stream_type {
                0x0f => Some(AudioKind::Adts),
                0x03 | 0x04 => Some(AudioKind::Mpeg),
                _ => None,
            };
            if let Some(kind) = kind {
                self.audio_pid = Some(pid);
                self.audio_kind = Some(kind);
                return;
            }
            // LATM AAC, AC-3, E-AC-3: audio we can't decode.
            if matches!(stream_type, 0x11 | 0x81 | 0x87) {
                self.unsupported_audio = true;
            }
            i += 5 + es_info_len;
        }
    }
}

/// True when `data` starts with two consecutive MPEG-TS sync bytes.
fn is_transport_stream(data: &[u8]) -> bool {
    data.len() >= TS_PACKET
        && data[0] == TS_SYNC
        && (data.len() < 2 * TS_PACKET || data[TS_PACKET] == TS_SYNC)
}

/// The PSI section body (from `table_id`, without the CRC) at the start of a
/// PUSI payload, if it's the expected table.
fn psi_section(payload: &[u8], table_id: u8) -> Option<&[u8]> {
    let pointer = usize::from(*payload.first()?);
    let section = payload.get(1 + pointer..)?;
    if *section.first()? != table_id || section.len() < 3 {
        return None;
    }
    let section_len = (usize::from(section[1] & 0x0f) << 8) | usize::from(section[2]);
    let end = (3 + section_len).checked_sub(4)?; // drop the CRC
    section.get(..end.min(section.len()))
}

/// The elementary-stream bytes of a PES packet that starts in `payload`.
fn pes_payload(payload: &[u8]) -> Option<&[u8]> {
    if payload.len() < 9 || payload[..3] != [0, 0, 1] {
        return None;
    }
    let header_len = usize::from(payload[8]);
    payload.get(9 + header_len..)
}

/// ADTS or MPEG audio frame sync at the start of `data`.
fn sniff_frame_kind(data: &[u8]) -> Option<AudioKind> {
    if data.len() < 2 || data[0] != 0xff {
        return None;
    }
    if data[1] & 0xf6 == 0xf0 {
        Some(AudioKind::Adts)
    } else if data[1] & 0xe0 == 0xe0 && data[1] & 0x06 != 0 {
        Some(AudioKind::Mpeg)
    } else {
        None
    }
}

/// Strips every leading ID3v2 tag from `data`, returning the "Artist - Title"
/// the first tag that names one carries.
fn strip_id3_tags(mut data: &[u8]) -> (Option<String>, &[u8]) {
    let mut title = None;
    while data.len() >= 10 && &data[..3] == b"ID3" {
        let size = syncsafe(&data[6..10]);
        let footer = if data[5] & 0x10 != 0 { 10 } else { 0 };
        let end = (10 + size + footer).min(data.len());
        if title.is_none() {
            title = id3_title(data[3], &data[10..(10 + size).min(data.len())]);
        }
        data = &data[end..];
    }
    (title, data)
}

fn syncsafe(b: &[u8]) -> usize {
    b.iter()
        .fold(0usize, |acc, &x| (acc << 7) | usize::from(x & 0x7f))
}

/// Reads `TPE1`/`TIT2` from an ID3v2.3 or v2.4 tag body.
fn id3_title(version: u8, mut body: &[u8]) -> Option<String> {
    if version != 3 && version != 4 {
        return None;
    }
    let mut artist = None;
    let mut title = None;
    while body.len() >= 10 && body[0] != 0 {
        let id = &body[..4];
        let size = if version == 4 {
            syncsafe(&body[4..8])
        } else {
            u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize
        };
        let Some(frame) = body.get(10..10 + size) else {
            break;
        };
        match id {
            b"TIT2" => title = id3_text(frame),
            b"TPE1" => artist = id3_text(frame),
            _ => {}
        }
        body = &body[10 + size..];
    }
    match (artist, title) {
        (Some(a), Some(t)) => Some(format!("{a} - {t}")),
        (None, Some(t)) => Some(t),
        _ => None,
    }
}

fn id3_text(frame: &[u8]) -> Option<String> {
    let (&encoding, text) = frame.split_first()?;
    let decoded = match encoding {
        0 => text.iter().map(|&b| char::from(b)).collect(),
        3 => String::from_utf8_lossy(text).into_owned(),
        1 | 2 => {
            let (units, big_endian) = match text {
                [0xff, 0xfe, rest @ ..] => (rest, false),
                [0xfe, 0xff, rest @ ..] => (rest, true),
                rest => (rest, encoding == 2),
            };
            let words: Vec<u16> = units
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| {
                    if big_endian {
                        u16::from_be_bytes([c[0], c[1]])
                    } else {
                        u16::from_le_bytes([c[0], c[1]])
                    }
                })
                .collect();
            String::from_utf16_lossy(&words)
        }
        _ => return None,
    };
    let cleaned = decoded.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fake ADTS frame: sync word, then deterministic filler.
    pub(crate) fn adts_frame(seed: u8) -> Vec<u8> {
        let mut f = vec![0xff, 0xf1, 0x50, 0x80];
        f.extend((0..60u8).map(|i| i.wrapping_mul(7).wrapping_add(seed)));
        f
    }

    fn ts_packet(pid: u16, pusi: bool, cc: u8, payload: &[u8], adaptation_pad: bool) -> Vec<u8> {
        let mut p = vec![
            TS_SYNC,
            (if pusi { 0x40 } else { 0 }) | ((pid >> 8) as u8 & 0x1f),
            pid as u8,
            0,
        ];
        let room = TS_PACKET - 4;
        if adaptation_pad || payload.len() < room {
            // Stuff with an adaptation field so the payload fills the packet's tail.
            let af_len = room - 1 - payload.len();
            p[3] = 0x30 | (cc & 0x0f);
            p.push(af_len as u8);
            if af_len > 0 {
                p.push(0);
                p.extend(std::iter::repeat_n(0xff, af_len - 1));
            }
        } else {
            p[3] = 0x10 | (cc & 0x0f);
        }
        p.extend_from_slice(payload);
        assert_eq!(p.len(), TS_PACKET);
        p
    }

    fn psi(table_id: u8, body: &[u8]) -> Vec<u8> {
        let section_len = body.len() + 5 + 4; // header after length + CRC
        let mut s = vec![
            0u8,
            table_id,
            0xb0 | ((section_len >> 8) as u8),
            section_len as u8,
        ];
        s.extend_from_slice(&[0, 1, 0xc1, 0, 0]);
        s.extend_from_slice(body);
        s.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        s
    }

    /// One MPEG-TS segment: PAT, PMT (video PID 0x100, audio PID 0x101 with
    /// `audio_stream_type`), a video packet, then `audio` split into PES
    /// packets of one or more TS packets each.
    pub(crate) fn ts_segment(audio_stream_type: u8, audio_pes: &[Vec<u8>]) -> Vec<u8> {
        let mut seg = Vec::new();
        seg.extend(ts_packet(
            0,
            true,
            0,
            &psi(0x00, &[0, 1, 0xe0 | 0x10, 0x00]),
            true,
        ));
        let pmt_body = [
            0xe1,
            0x00,
            0xf0,
            0x00, // PCR PID 0x100, no program info
            0x1b,
            0xe1,
            0x00,
            0xf0,
            0x00, // H.264 on 0x100
            audio_stream_type,
            0xe1,
            0x01,
            0xf0,
            0x00, // audio on 0x101
        ];
        seg.extend(ts_packet(0x1000, true, 0, &psi(0x02, &pmt_body), true));
        seg.extend(ts_packet(
            0x100,
            true,
            0,
            &[0, 0, 1, 0xe0, 0, 0, 0x80, 0, 0],
            true,
        ));
        let mut cc = 0u8;
        for es in audio_pes {
            let mut pes = vec![0, 0, 1, 0xc0, 0, 0, 0x80, 0x80, 5, 0x21, 0, 1, 0, 1];
            pes.extend_from_slice(es);
            let mut first = true;
            for chunk in pes.chunks(TS_PACKET - 4) {
                let pad = chunk.len() < TS_PACKET - 4;
                seg.extend(ts_packet(0x101, first, cc, chunk, pad));
                first = false;
                cc = cc.wrapping_add(1);
            }
        }
        seg
    }

    pub(crate) fn id3_tag(artist: &str, title: &str) -> Vec<u8> {
        let mut frames = Vec::new();
        for (id, text) in [(b"TPE1", artist), (b"TIT2", title)] {
            let mut data = vec![3u8];
            data.extend_from_slice(text.as_bytes());
            frames.extend_from_slice(id);
            frames.extend_from_slice(&(data.len() as u32).to_be_bytes());
            frames.extend_from_slice(&[0, 0]);
            frames.extend(data);
        }
        let mut tag = b"ID3\x03\x00\x00".to_vec();
        let n = frames.len();
        tag.extend_from_slice(&[
            ((n >> 21) & 0x7f) as u8,
            ((n >> 14) & 0x7f) as u8,
            ((n >> 7) & 0x7f) as u8,
            (n & 0x7f) as u8,
        ]);
        tag.extend(frames);
        tag
    }

    #[test]
    fn remuxes_ts_audio_pes_across_packets_and_drops_video() {
        let frames: Vec<Vec<u8>> = (0..5).map(adts_frame).collect();
        // One small PES, then one spanning several TS packets.
        let big: Vec<u8> = frames[1..].concat();
        let big = [big.clone(), big.clone(), big].concat();
        let pes = vec![frames[0].clone(), big.clone()];
        let seg = ts_segment(0x0f, &pes);

        let mut demux = SegmentDemuxer::new();
        let audio = demux.process(&seg).unwrap();
        assert_eq!(audio.kind, Some(AudioKind::Adts));
        assert_eq!(audio.bytes, [frames[0].clone(), big].concat());
    }

    #[test]
    fn ts_with_mp3_stream_type_is_mpeg_audio() {
        let seg = ts_segment(0x03, &[vec![0xff, 0xfb, 0x90, 0x64, 1, 2, 3]]);
        let audio = SegmentDemuxer::new().process(&seg).unwrap();
        assert_eq!(audio.kind, Some(AudioKind::Mpeg));
        assert_eq!(audio.bytes, vec![0xff, 0xfb, 0x90, 0x64, 1, 2, 3]);
    }

    #[test]
    fn ts_with_only_unsupported_audio_is_an_error() {
        let seg = ts_segment(0x81, &[vec![0x0b, 0x77, 1, 2]]);
        let err = SegmentDemuxer::new().process(&seg).unwrap_err();
        assert!(err.contains("codec isn't supported"), "{err}");
    }

    #[test]
    fn packed_audio_loses_id3_tags_and_reports_their_title() {
        let frame = adts_frame(3);
        let data = [
            id3_tag("Nightwish", "Ghost Love Score"),
            id3_tag("ignored", "second tag"),
            frame.clone(),
        ]
        .concat();
        let audio = SegmentDemuxer::new().process(&data).unwrap();
        assert_eq!(audio.kind, Some(AudioKind::Adts));
        assert_eq!(audio.bytes, frame);
        assert_eq!(audio.title.as_deref(), Some("Nightwish - Ghost Love Score"));
    }

    #[test]
    fn packed_mp3_without_id3_passes_through() {
        let data = vec![0xff, 0xfb, 0x90, 0x64, 9, 9, 9];
        let audio = SegmentDemuxer::new().process(&data).unwrap();
        assert_eq!(audio.kind, Some(AudioKind::Mpeg));
        assert_eq!(audio.bytes, data);
        assert_eq!(audio.title, None);
    }

    #[test]
    fn fragmented_mp4_segments_are_rejected() {
        let mut data = vec![0, 0, 0, 24];
        data.extend_from_slice(b"styp");
        data.extend_from_slice(&[0; 16]);
        assert!(SegmentDemuxer::new().process(&data).is_err());
    }

    #[test]
    fn id3_text_decodes_utf16_with_bom() {
        let mut frame = vec![1u8, 0xff, 0xfe];
        for u in "Épica".encode_utf16() {
            frame.extend_from_slice(&u.to_le_bytes());
        }
        frame.extend_from_slice(&[0, 0]);
        assert_eq!(id3_text(&frame).as_deref(), Some("Épica"));
    }
}
