//! Turns HLS media segments into one continuous elementary audio stream the
//! audio engine already decodes (#2012):
//!
//! - Packed audio segments (raw ADTS AAC or MPEG audio behind an ID3 tag)
//!   lose their ID3 tags and are concatenated as they are.
//! - MPEG-TS segments are remuxed at the byte level: the audio PES payloads of
//!   the program's first audio stream are extracted, and video, timed ID3
//!   metadata and every other PID are dropped.
//! - Fragmented MP4 (fMP4 / CMAF) segments (`EXT-X-MAP` init + `.m4s`
//!   fragments):
//!   - AAC (`mp4a` + `esds`): raw AAC frames are extracted from `moof`/`mdat`
//!     and wrapped in 7-byte ADTS headers using `AudioSpecificConfig`.
//!   - FLAC (`fLaC` + `dfLa`): the `fLaC` stream marker and metadata blocks from
//!     `dfLa` are emitted once before the first segment's raw FLAC frames.
//!
//! The result is an ADTS, MPEG, or native FLAC byte stream, so no container
//! demuxer is needed downstream.

const TS_PACKET: usize = 188;
const TS_SYNC: u8 = 0x47;

/// The elementary stream a station's segments carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioKind {
    /// AAC in ADTS framing (LC or HE-AAC).
    Adts,
    /// MPEG-1/2 audio (MP3).
    Mpeg,
    /// Native FLAC stream (`fLaC` + metadata blocks + raw frames).
    Flac,
}

impl AudioKind {
    pub fn mime_type(self) -> &'static str {
        match self {
            AudioKind::Adts => "audio/aac",
            AudioKind::Mpeg => "audio/mpeg",
            AudioKind::Flac => "audio/flac",
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
/// every segment, but are remembered across segments in case one omits them;
/// an fMP4 stream's `EXT-X-MAP` init segment provides the AAC or FLAC codec
/// config for all subsequent `.m4s` fragments.
#[derive(Debug, Default)]
pub struct SegmentDemuxer {
    pmt_pid: Option<u16>,
    audio_pid: Option<u16>,
    audio_kind: Option<AudioKind>,
    unsupported_audio: bool,
    fmp4_init: Option<Fmp4Init>,
    flac_header_emitted: bool,
}

impl SegmentDemuxer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses an fMP4 `EXT-X-MAP` initialization segment (`ftyp` + `moov`) so
    /// subsequent `.m4s` media segments can be demuxed into ADTS AAC or native
    /// FLAC frames.
    pub fn set_init_segment(&mut self, data: &[u8]) -> Result<(), String> {
        let init = parse_fmp4_init(data)?;
        self.fmp4_init = Some(init);
        Ok(())
    }

    /// Extracts the audio from one downloaded segment.
    pub fn process(&mut self, data: &[u8]) -> Result<SegmentAudio, String> {
        if is_transport_stream(data) {
            return self.process_ts(data);
        }
        if is_fmp4_segment(data) {
            return self.process_fmp4(data);
        }
        let (title, rest) = strip_id3_tags(data);
        let kind = sniff_frame_kind(rest).ok_or_else(|| {
            "HLS segment is neither MPEG-TS, fMP4, nor ADTS/MPEG audio".to_string()
        })?;
        Ok(SegmentAudio {
            kind: Some(kind),
            bytes: rest.to_vec(),
            title,
        })
    }

    fn process_fmp4(&mut self, data: &[u8]) -> Result<SegmentAudio, String> {
        let init = self.fmp4_init.as_ref().ok_or_else(|| {
            "HLS fragmented MP4 segment arrived without an EXT-X-MAP init segment".to_string()
        })?;
        let mut emit_flac_header = !self.flac_header_emitted;
        let (kind, bytes) = extract_fmp4_audio(data, init, &mut emit_flac_header)?;
        if kind == AudioKind::Flac {
            self.flac_header_emitted = true;
        }
        Ok(SegmentAudio {
            kind: Some(kind),
            bytes,
            title: None,
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

// ---------------------------------------------------------------------------
// Fragmented MP4 (fMP4 / CMAF) audio extraction
// ---------------------------------------------------------------------------

/// Codec configuration extracted from an fMP4 `EXT-X-MAP` init segment.
#[derive(Debug, Clone)]
struct Fmp4Init {
    track_id: Option<u32>,
    default_sample_size: Option<u32>,
    codec: Fmp4Codec,
}

#[derive(Debug, Clone)]
enum Fmp4Codec {
    Aac {
        adts_profile: u8,
        sampling_freq_idx: u8,
        channel_config: u8,
    },
    Flac {
        /// Raw FLAC metadata blocks from `dfLa` (after the 4-byte FullBox header).
        metadata_blocks: Vec<u8>,
    },
}

struct Mp4Box<'a> {
    box_type: [u8; 4],
    payload: &'a [u8],
    offset: usize,
    total_len: usize,
}

fn iter_mp4_boxes(data: &[u8]) -> impl Iterator<Item = Mp4Box<'_>> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if pos + 8 > data.len() {
            return None;
        }
        let size32 = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let box_type = [data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]];
        let (header, total_len) = if size32 == 0 {
            (8usize, data.len() - pos)
        } else if size32 == 1 {
            if pos + 16 > data.len() {
                return None;
            }
            let size64 = u64::from_be_bytes([
                data[pos + 8],
                data[pos + 9],
                data[pos + 10],
                data[pos + 11],
                data[pos + 12],
                data[pos + 13],
                data[pos + 14],
                data[pos + 15],
            ]) as usize;
            (16usize, size64)
        } else {
            (8usize, size32 as usize)
        };
        if total_len < header || pos + total_len > data.len() {
            return None;
        }
        let offset = pos;
        let payload = &data[pos + header..pos + total_len];
        pos += total_len;
        Some(Mp4Box {
            box_type,
            payload,
            offset,
            total_len,
        })
    })
}

fn is_fmp4_segment(data: &[u8]) -> bool {
    let Some(first) = iter_mp4_boxes(data).next() else {
        return false;
    };
    matches!(
        &first.box_type,
        b"styp" | b"moof" | b"sidx" | b"emsg" | b"ftyp" | b"free" | b"mdat"
    ) && first.total_len >= 8
}

fn find_box<'a>(data: &'a [u8], target: &[u8; 4]) -> Option<&'a [u8]> {
    iter_mp4_boxes(data)
        .find(|b| &b.box_type == target)
        .map(|b| b.payload)
}

fn parse_fmp4_init(data: &[u8]) -> Result<Fmp4Init, String> {
    let moov = find_box(data, b"moov")
        .ok_or_else(|| "HLS fMP4 init segment has no 'moov' box".to_string())?;

    let mut trex_defaults: Vec<(u32, u32)> = Vec::new();
    if let Some(mvex) = find_box(moov, b"mvex") {
        for b in iter_mp4_boxes(mvex) {
            if &b.box_type == b"trex" && b.payload.len() >= 24 {
                let tid =
                    u32::from_be_bytes([b.payload[4], b.payload[5], b.payload[6], b.payload[7]]);
                let def_sz = u32::from_be_bytes([
                    b.payload[16],
                    b.payload[17],
                    b.payload[18],
                    b.payload[19],
                ]);
                trex_defaults.push((tid, def_sz));
            }
        }
    }

    for trak in iter_mp4_boxes(moov).filter(|b| &b.box_type == b"trak") {
        if let Some(init) = parse_fmp4_audio_track(trak.payload, &trex_defaults)? {
            return Ok(init);
        }
    }

    Err(
        "HLS fMP4 init segment has no supported AAC ('mp4a') or FLAC ('fLaC') audio track"
            .to_string(),
    )
}

fn parse_fmp4_audio_track(
    trak: &[u8],
    trex_defaults: &[(u32, u32)],
) -> Result<Option<Fmp4Init>, String> {
    let track_id = find_box(trak, b"tkhd").and_then(|tkhd| {
        let version = *tkhd.first()?;
        let off = if version == 1 { 20 } else { 12 };
        let bytes = tkhd.get(off..off + 4)?;
        Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    });

    let Some(mdia) = find_box(trak, b"mdia") else {
        return Ok(None);
    };
    if let Some(hdlr) = find_box(mdia, b"hdlr") {
        if hdlr.get(8..12) != Some(b"soun".as_slice()) {
            return Ok(None);
        }
    }
    let Some(entries) = find_box(mdia, b"minf")
        .and_then(|m| find_box(m, b"stbl"))
        .and_then(|s| find_box(s, b"stsd"))
        .and_then(|stsd| stsd.get(8..))
    else {
        return Ok(None);
    };
    let default_sample_size = track_id
        .and_then(|tid| {
            trex_defaults
                .iter()
                .find(|(t, _)| *t == tid)
                .map(|(_, s)| *s)
        })
        .filter(|&s| s > 0);

    if let Some(mp4a) = find_box(entries, b"mp4a") {
        if mp4a.len() < 28 {
            return Ok(None);
        }
        let channel_count = u16::from_be_bytes([mp4a[16], mp4a[17]]);
        let esds = find_box(&mp4a[28..], b"esds").ok_or_else(|| {
            "HLS fMP4 audio track has 'mp4a' without 'esds' descriptor".to_string()
        })?;
        let asc = parse_esds_audio_specific_config(esds)?;
        let (adts_profile, sampling_freq_idx, mut channel_config) =
            parse_audio_specific_config(&asc)?;
        if channel_config == 0 && channel_count > 0 {
            channel_config = channel_count.min(7) as u8;
        }
        return Ok(Some(Fmp4Init {
            track_id,
            default_sample_size,
            codec: Fmp4Codec::Aac {
                adts_profile,
                sampling_freq_idx,
                channel_config,
            },
        }));
    }

    if let Some(flac) = find_box(entries, b"fLaC") {
        if flac.len() < 28 {
            return Ok(None);
        }
        let dfla = find_box(&flac[28..], b"dfLa")
            .ok_or_else(|| "HLS fMP4 audio track has 'fLaC' without 'dfLa' box".to_string())?;
        // dfLa is a FullBox (4-byte version/flags) followed by FLAC metadata blocks
        // (at least a 4-byte block header + 34-byte STREAMINFO).
        let metadata_blocks = dfla
            .get(4..)
            .filter(|blocks| blocks.len() >= 38)
            .ok_or_else(|| "HLS fMP4 'dfLa' box is missing FLAC STREAMINFO metadata".to_string())?
            .to_vec();
        return Ok(Some(Fmp4Init {
            track_id,
            default_sample_size,
            codec: Fmp4Codec::Flac { metadata_blocks },
        }));
    }

    Ok(None)
}

fn read_mp4_descr_len(data: &[u8], pos: &mut usize) -> Option<usize> {
    let mut len = 0usize;
    for _ in 0..4 {
        let b = *data.get(*pos)?;
        *pos += 1;
        len = (len << 7) | usize::from(b & 0x7f);
        if b & 0x80 == 0 {
            return Some(len);
        }
    }
    Some(len)
}

fn parse_esds_audio_specific_config(esds: &[u8]) -> Result<Vec<u8>, String> {
    // FullBox header is 4 bytes.
    let mut pos = 4usize;
    while pos < esds.len() {
        let tag = esds[pos];
        pos += 1;
        let len = read_mp4_descr_len(esds, &mut pos)
            .ok_or_else(|| "Truncated ESDS descriptor in fMP4 init segment".to_string())?;
        let end = (pos + len).min(esds.len());
        match tag {
            0x03 => {
                // ES_Descriptor: 2B ES_ID + 1B flags (+ optional fields)
                if pos + 3 > end {
                    break;
                }
                let flags = esds[pos + 2];
                pos += 3;
                if flags & 0x80 != 0 {
                    pos += 2;
                }
                if flags & 0x40 != 0 {
                    let url_len = *esds.get(pos).unwrap_or(&0) as usize;
                    pos += 1 + url_len;
                }
                if flags & 0x20 != 0 {
                    pos += 2;
                }
            }
            0x04 => {
                // DecoderConfigDescriptor: 13B fixed header before child descriptors
                if pos + 13 > end {
                    break;
                }
                pos += 13;
            }
            0x05 => {
                // DecoderSpecificInfo = AudioSpecificConfig
                return Ok(esds[pos..end].to_vec());
            }
            _ => pos = end,
        }
    }
    Err("HLS fMP4 'esds' box missing AudioSpecificConfig".to_string())
}

fn parse_audio_specific_config(asc: &[u8]) -> Result<(u8, u8, u8), String> {
    if asc.len() < 2 {
        return Err("HLS fMP4 AudioSpecificConfig is too short".to_string());
    }
    let obj_type = (asc[0] >> 3) & 0x1f;
    let mut freq_idx = ((asc[0] & 0x07) << 1) | ((asc[1] >> 7) & 0x01);
    let mut chan_cfg = (asc[1] >> 3) & 0x0f;
    if freq_idx == 15 {
        if asc.len() < 5 {
            return Err("HLS fMP4 AudioSpecificConfig has truncated explicit sample rate".into());
        }
        let hz = (u32::from(asc[1] & 0x7f) << 17)
            | (u32::from(asc[2]) << 9)
            | (u32::from(asc[3]) << 1)
            | u32::from((asc[4] >> 7) & 1);
        freq_idx = match hz {
            96_000 => 0,
            88_200 => 1,
            64_000 => 2,
            48_000 => 3,
            44_100 => 4,
            32_000 => 5,
            24_000 => 6,
            22_050 => 7,
            16_000 => 8,
            12_000 => 9,
            11_025 => 10,
            8_000 => 11,
            7_350 => 12,
            _ => return Err(format!("Unsupported AAC sample rate in fMP4: {hz} Hz")),
        };
        chan_cfg = (asc[4] >> 3) & 0x0f;
    }
    if freq_idx > 12 {
        return Err(format!(
            "Unsupported AAC sample rate index in fMP4: {freq_idx}"
        ));
    }
    // ADTS 2-bit profile: 0=Main, 1=LC, 2=SSR, 3=LTP. Explicit SBR/PS (5/29)
    // wrap an LC core, so ADTS carries profile 1 (LC).
    let adts_profile = match obj_type {
        2 | 5 | 29 => 1,
        1..=4 => obj_type - 1,
        _ => 1,
    };
    Ok((adts_profile, freq_idx, chan_cfg))
}

struct PendingMoof {
    moof_offset: usize,
    data_offset: Option<i32>,
    sample_sizes: Vec<u32>,
}

fn parse_moof_samples(moof: &[u8], moof_offset: usize, init: &Fmp4Init) -> PendingMoof {
    let mut data_offset = None;
    let mut sample_sizes = Vec::new();
    for traf in iter_mp4_boxes(moof).filter(|b| &b.box_type == b"traf") {
        let Some(tfhd) = find_box(traf.payload, b"tfhd") else {
            continue;
        };
        if tfhd.len() < 8 {
            continue;
        }
        let tfhd_flags = u32::from_be_bytes([0, tfhd[1], tfhd[2], tfhd[3]]);
        let track_id = u32::from_be_bytes([tfhd[4], tfhd[5], tfhd[6], tfhd[7]]);
        if init.track_id.is_some_and(|tid| tid != track_id) {
            continue;
        }
        let mut off = 8usize;
        if tfhd_flags & 0x000001 != 0 {
            off += 8;
        }
        if tfhd_flags & 0x000002 != 0 {
            off += 4;
        }
        if tfhd_flags & 0x000008 != 0 {
            off += 4;
        }
        let tfhd_default_size = if tfhd_flags & 0x000010 != 0 {
            let sz = tfhd
                .get(off..off + 4)
                .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
            sz
        } else {
            None
        };
        let default_size = tfhd_default_size.or(init.default_sample_size).unwrap_or(0);

        for trun in iter_mp4_boxes(traf.payload).filter(|b| &b.box_type == b"trun") {
            if trun.payload.len() < 8 {
                continue;
            }
            let flags = u32::from_be_bytes([0, trun.payload[1], trun.payload[2], trun.payload[3]]);
            let sample_count = u32::from_be_bytes([
                trun.payload[4],
                trun.payload[5],
                trun.payload[6],
                trun.payload[7],
            ]) as usize;
            let mut p = 8usize;
            if flags & 0x000001 != 0 {
                if let Some(b) = trun.payload.get(p..p + 4) {
                    if data_offset.is_none() {
                        data_offset = Some(i32::from_be_bytes([b[0], b[1], b[2], b[3]]));
                    }
                }
                p += 4;
            }
            if flags & 0x000004 != 0 {
                p += 4;
            }
            for _ in 0..sample_count {
                if flags & 0x000100 != 0 {
                    p += 4;
                }
                let sz = if flags & 0x000200 != 0 {
                    let Some(b) = trun.payload.get(p..p + 4) else {
                        break;
                    };
                    p += 4;
                    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
                } else {
                    default_size
                };
                if flags & 0x000400 != 0 {
                    p += 4;
                }
                if flags & 0x000800 != 0 {
                    p += 4;
                }
                if sz > 0 {
                    sample_sizes.push(sz);
                }
            }
        }
    }
    PendingMoof {
        moof_offset,
        data_offset,
        sample_sizes,
    }
}

fn write_adts_header(
    out: &mut Vec<u8>,
    adts_profile: u8,
    sampling_freq_idx: u8,
    channel_config: u8,
    frame_payload_len: usize,
) {
    // 7-byte ADTS header (ISO/IEC 13818-7): sync=0xFFF, ID=0 (MPEG-4),
    // protection_absent=1 (no CRC), buffer_fullness=0x7FF (VBR), 1 raw block.
    let frame_len = 7 + frame_payload_len;
    let profile = adts_profile & 0x03;
    let freq = sampling_freq_idx & 0x0f;
    let chan = channel_config & 0x07;
    out.extend_from_slice(&[
        0xff,
        0xf1,
        (profile << 6) | (freq << 2) | (chan >> 2),
        ((chan & 0x03) << 6) | (((frame_len >> 11) & 0x03) as u8),
        ((frame_len >> 3) & 0xff) as u8,
        (((frame_len & 0x07) << 5) as u8) | 0x1f,
        0xfc,
    ]);
}

fn extract_fmp4_audio(
    data: &[u8],
    init: &Fmp4Init,
    emit_flac_header: &mut bool,
) -> Result<(AudioKind, Vec<u8>), String> {
    let mut out = Vec::new();
    let mut pending: Option<PendingMoof> = None;
    let mut sample_count = 0usize;

    let kind = match init.codec {
        Fmp4Codec::Aac { .. } => AudioKind::Adts,
        Fmp4Codec::Flac { .. } => AudioKind::Flac,
    };

    for b in iter_mp4_boxes(data) {
        match &b.box_type {
            b"moof" => {
                pending = Some(parse_moof_samples(b.payload, b.offset, init));
            }
            b"mdat" => {
                let Some(moof) = pending.take() else {
                    continue;
                };
                let mut cursor = moof
                    .data_offset
                    .and_then(|off| {
                        let abs = moof.moof_offset.checked_add_signed(off as isize)?;
                        let payload_start = b.offset + (b.total_len - b.payload.len());
                        abs.checked_sub(payload_start)
                    })
                    .filter(|&rel| rel <= b.payload.len())
                    .unwrap_or(0);
                for sz in moof.sample_sizes {
                    let sz = sz as usize;
                    let Some(sample) = b.payload.get(cursor..cursor + sz) else {
                        return Err("Truncated audio sample in fMP4 'mdat' box".to_string());
                    };
                    match &init.codec {
                        Fmp4Codec::Aac {
                            adts_profile,
                            sampling_freq_idx,
                            channel_config,
                        } => {
                            write_adts_header(
                                &mut out,
                                *adts_profile,
                                *sampling_freq_idx,
                                *channel_config,
                                sz,
                            );
                            out.extend_from_slice(sample);
                        }
                        Fmp4Codec::Flac { metadata_blocks } => {
                            if *emit_flac_header {
                                out.extend_from_slice(b"fLaC");
                                out.extend_from_slice(metadata_blocks);
                                *emit_flac_header = false;
                            }
                            out.extend_from_slice(sample);
                        }
                    }
                    sample_count += 1;
                    cursor += sz;
                }
            }
            _ => {}
        }
    }

    if sample_count == 0 {
        return Err("HLS fMP4 segment contained no audio samples".to_string());
    }
    Ok((kind, out))
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
    fn fmp4_segments_without_init_are_rejected() {
        let mut data = vec![0, 0, 0, 24];
        data.extend_from_slice(b"styp");
        data.extend_from_slice(&[0; 16]);
        let err = SegmentDemuxer::new().process(&data).unwrap_err();
        assert!(err.contains("init segment"), "{err}");
    }

    fn mp4_box(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(tag);
        out.extend_from_slice(payload);
        out
    }

    pub(crate) fn fmp4_aac_init(track_id: u32, asc: [u8; 2]) -> Vec<u8> {
        let mut tkhd = vec![0u8; 84];
        tkhd[12..16].copy_from_slice(&track_id.to_be_bytes());
        let mut hdlr = vec![0u8; 16];
        hdlr[8..12].copy_from_slice(b"soun");
        // esds: FullBox(4) + tag3(len=20) + es_id(2)+flags(1) + tag4(len=15) + 13B + tag5(len=2) + asc(2)
        let mut esds = vec![0, 0, 0, 0, 0x03, 20, 0, 1, 0, 0x04, 15, 0x40, 0x15];
        esds.extend_from_slice(&[0; 11]);
        esds.extend_from_slice(&[0x05, 2, asc[0], asc[1]]);
        let mut mp4a = vec![0u8; 28];
        mp4a[16..18].copy_from_slice(&2u16.to_be_bytes());
        mp4a.extend(mp4_box(b"esds", &esds));
        let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
        stsd.extend(mp4_box(b"mp4a", &mp4a));
        let stbl = mp4_box(b"stbl", &mp4_box(b"stsd", &stsd));
        let minf = mp4_box(b"minf", &stbl);
        let mdia = mp4_box(b"mdia", &[mp4_box(b"hdlr", &hdlr), minf].concat());
        let trak = mp4_box(b"trak", &[mp4_box(b"tkhd", &tkhd), mdia].concat());
        let moov = mp4_box(b"moov", &trak);
        [mp4_box(b"ftyp", b"isom\0\0\0\0"), moov].concat()
    }

    pub(crate) fn fmp4_aac_segment(track_id: u32, frames: &[&[u8]]) -> Vec<u8> {
        let mut tfhd = vec![0, 0, 0, 0];
        tfhd.extend_from_slice(&track_id.to_be_bytes());
        // trun: flags=0x000201 (data_offset + sample_size)
        let mut trun = vec![0, 0, 0x02, 0x01];
        trun.extend_from_slice(&(frames.len() as u32).to_be_bytes());
        let data_offset_pos = trun.len();
        trun.extend_from_slice(&0i32.to_be_bytes());
        for f in frames {
            trun.extend_from_slice(&(f.len() as u32).to_be_bytes());
        }
        let traf = mp4_box(
            b"traf",
            &[mp4_box(b"tfhd", &tfhd), mp4_box(b"trun", &trun)].concat(),
        );
        let moof_len = 8 + traf.len();
        // Patch data_offset = moof_len + 8 (mdat header)
        let trun_offset_in_moof = 8 + 8 + 8 + tfhd.len() + 8 + data_offset_pos;
        let mut moof = mp4_box(b"moof", &traf);
        moof[trun_offset_in_moof..trun_offset_in_moof + 4]
            .copy_from_slice(&((moof_len + 8) as i32).to_be_bytes());
        let payload: Vec<u8> = frames.iter().flat_map(|f| f.iter().copied()).collect();
        [moof, mp4_box(b"mdat", &payload)].concat()
    }

    #[test]
    fn fmp4_init_and_media_segment_emit_adts_frames() {
        // AAC-LC (2), 44100 Hz (4), stereo (2) -> 0x12, 0x10
        let init = fmp4_aac_init(1, [0x12, 0x10]);
        let f1 = [0xde, 0xad, 0xbe, 0xef];
        let f2 = [0xca, 0xfe];
        let seg = fmp4_aac_segment(1, &[&f1, &f2]);

        let mut demux = SegmentDemuxer::new();
        demux.set_init_segment(&init).unwrap();
        let audio = demux.process(&seg).unwrap();
        assert_eq!(audio.kind, Some(AudioKind::Adts));
        // Two frames, each with 7-byte ADTS header + payload
        assert_eq!(audio.bytes.len(), (7 + f1.len()) + (7 + f2.len()));
        assert_eq!(&audio.bytes[..2], &[0xff, 0xf1]);
        assert_eq!(&audio.bytes[7..7 + f1.len()], &f1);
        assert_eq!(&audio.bytes[7 + f1.len()..7 + f1.len() + 2], &[0xff, 0xf1]);
        assert_eq!(&audio.bytes[7 + f1.len() + 7..], &f2);
    }

    pub(crate) fn fmp4_flac_init(track_id: u32, metadata_blocks: &[u8]) -> Vec<u8> {
        let mut tkhd = vec![0u8; 84];
        tkhd[12..16].copy_from_slice(&track_id.to_be_bytes());
        let mut hdlr = vec![0u8; 16];
        hdlr[8..12].copy_from_slice(b"soun");
        let mut dfla = vec![0, 0, 0, 0]; // FullBox header
        dfla.extend_from_slice(metadata_blocks);
        let mut flac = vec![0u8; 28];
        flac[16..18].copy_from_slice(&2u16.to_be_bytes());
        flac.extend(mp4_box(b"dfLa", &dfla));
        let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
        stsd.extend(mp4_box(b"fLaC", &flac));
        let stbl = mp4_box(b"stbl", &mp4_box(b"stsd", &stsd));
        let minf = mp4_box(b"minf", &stbl);
        let mdia = mp4_box(b"mdia", &[mp4_box(b"hdlr", &hdlr), minf].concat());
        let trak = mp4_box(b"trak", &[mp4_box(b"tkhd", &tkhd), mdia].concat());
        let moov = mp4_box(b"moov", &trak);
        [mp4_box(b"ftyp", b"isom\0\0\0\0"), moov].concat()
    }

    #[test]
    fn fmp4_flac_init_and_media_segments_emit_native_flac_stream() {
        // Minimal 38-byte STREAMINFO metadata block (last-block bit + type 0 + len 34 + payload)
        let mut blocks = vec![0x80, 0x00, 0x00, 34];
        blocks.extend_from_slice(&[0x10; 34]);
        let init = fmp4_flac_init(1, &blocks);
        let f1 = [0xff, 0xf8, 0x69, 0x00];
        let f2 = [0xff, 0xf8, 0x69, 0x01];
        let seg1 = fmp4_aac_segment(1, &[&f1]);
        let seg2 = fmp4_aac_segment(1, &[&f2]);

        let mut demux = SegmentDemuxer::new();
        demux.set_init_segment(&init).unwrap();

        let first = demux.process(&seg1).unwrap();
        assert_eq!(first.kind, Some(AudioKind::Flac));
        assert_eq!(&first.bytes[..4], b"fLaC");
        assert_eq!(&first.bytes[4..4 + blocks.len()], blocks.as_slice());
        assert_eq!(&first.bytes[4 + blocks.len()..], &f1);

        // Even if the init segment is reloaded, the stream header is not repeated mid-stream.
        demux.set_init_segment(&init).unwrap();
        let second = demux.process(&seg2).unwrap();
        assert_eq!(second.kind, Some(AudioKind::Flac));
        assert_eq!(second.bytes, f2);
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
