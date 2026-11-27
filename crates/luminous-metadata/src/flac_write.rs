//! Tag writes to FLAC files that leave the audio frames where they are (#2426).
//!
//! lofty keeps a FLAC file's `PADDING` block when it saves, but never resizes
//! it to absorb a change in metadata length (upstream lofty-rs#445). So every
//! edit that changes the length of the tags (a longer genre, say) moves the
//! entire audio stream after them, in 64 KiB seek/read/write steps. For a hi-res
//! file on a network share that is tens of megabytes of round trips per edit.
//!
//! [`save`] lets lofty encode the new metadata into an in-memory copy of only
//! the metadata region (the audio is never read), then resizes the padding so
//! the region keeps its exact length and overwrites it in place: one small
//! write. Only when the new metadata no longer fits in the old metadata plus
//! its padding do the frames move, and then they are left
//! [`REWRITE_PADDING`] bytes of room, so the next edits fit.

use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

const BLOCK_TYPE_PADDING: u8 = 1;
const BLOCK_HEADER_LEN: usize = 4;
const MAX_BLOCK_LEN: usize = (1 << 24) - 1;
/// Padding left after a save that had to move the frames anyway.
const REWRITE_PADDING: usize = 8 * 1024;
/// Chunk size for moving the frames: large, because each chunk costs a round
/// trip on a network share.
const MOVE_CHUNK: usize = 4 * 1024 * 1024;

/// Saves the metadata `encode` writes into the FLAC `file`.
///
/// `encode` gets a copy of the file's metadata region only (an optional ID3v2
/// tag, the `fLaC` marker and every metadata block, no audio) and edits it the
/// way it would edit the whole file, e.g. lofty's `save_to`. Returns
/// `Ok(false)`, having written nothing and with `file` rewound to its start,
/// when `file` doesn't begin with a readable FLAC metadata region, so the
/// caller can fall back to a plain save of the same handle.
pub(crate) fn save(
    file: &mut File,
    encode: impl FnOnce(&mut Cursor<Vec<u8>>) -> io::Result<()>,
) -> io::Result<bool> {
    let Some(old_len) = metadata_region_len(file)? else {
        file.rewind()?;
        return Ok(false);
    };
    let mut region = vec![0u8; old_len as usize];
    file.rewind()?;
    file.read_exact(&mut region)?;

    let mut cursor = Cursor::new(region);
    encode(&mut cursor)?;
    let mut region = cursor.into_inner();
    let Some(blocks) = parse_blocks(&region) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "encoded FLAC metadata is not a valid block sequence",
        ));
    };

    if !fit_to_len(&mut region, &blocks, old_len as usize) {
        set_padding(&mut region, &blocks, REWRITE_PADDING);
        move_frames(file, old_len, region.len() as u64)?;
    }
    file.rewind()?;
    file.write_all(&region)?;
    Ok(true)
}

/// Length of the metadata region at the start of `file`: an optional ID3v2
/// tag, `fLaC`, and the metadata blocks up to the one flagged last. `None`
/// when the file doesn't parse as that.
fn metadata_region_len(file: &mut File) -> io::Result<Option<u64>> {
    let file_len = file.seek(SeekFrom::End(0))?;
    file.rewind()?;
    let mut head = [0u8; 10];
    if file.read_exact(&mut head).is_err() {
        return Ok(None);
    }
    let mut pos = id3v2_len(&head) as u64;

    let mut marker = [0u8; 4];
    file.seek(SeekFrom::Start(pos))?;
    if file.read_exact(&mut marker).is_err() || &marker != b"fLaC" {
        return Ok(None);
    }
    pos += 4;

    loop {
        let mut header = [0u8; BLOCK_HEADER_LEN];
        file.seek(SeekFrom::Start(pos))?;
        if file.read_exact(&mut header).is_err() {
            return Ok(None);
        }
        pos += (BLOCK_HEADER_LEN + block_len(&header)) as u64;
        if pos > file_len {
            return Ok(None);
        }
        if header[0] & 0x80 != 0 {
            return Ok(Some(pos));
        }
    }
}

/// Bytes taken by an ID3v2 tag at the start of `head` (its 10-byte header,
/// body and optional footer), or 0 when there is none.
fn id3v2_len(head: &[u8]) -> usize {
    if head.len() < 10 || &head[..3] != b"ID3" {
        return 0;
    }
    let body = head[6..10]
        .iter()
        .fold(0usize, |acc, &b| (acc << 7) | (b & 0x7f) as usize);
    let footer = if head[5] & 0x10 != 0 { 10 } else { 0 };
    10 + body + footer
}

fn block_len(header: &[u8]) -> usize {
    u32::from_be_bytes([0, header[1], header[2], header[3]]) as usize
}

/// A metadata block's position inside a metadata region.
struct Block {
    header_at: usize,
    ty: u8,
    len: usize,
}

/// The metadata blocks of a whole metadata `region`, or `None` unless they
/// end, at the block flagged last, exactly where `region` does.
fn parse_blocks(region: &[u8]) -> Option<Vec<Block>> {
    let start = id3v2_len(region);
    if region.get(start..start + 4)? != b"fLaC" {
        return None;
    }
    let mut pos = start + 4;
    let mut blocks = Vec::new();
    loop {
        let header = region.get(pos..pos + BLOCK_HEADER_LEN)?;
        let block = Block {
            header_at: pos,
            ty: header[0] & 0x7f,
            len: block_len(header),
        };
        let last = header[0] & 0x80 != 0;
        pos += BLOCK_HEADER_LEN + block.len;
        blocks.push(block);
        if last {
            return (pos == region.len()).then_some(blocks);
        }
    }
}

/// Resizes or adds padding so `region` is exactly `target` bytes long.
/// Returns false, leaving `region` unchanged, when the blocks other than
/// padding don't fit in `target`.
fn fit_to_len(region: &mut Vec<u8>, blocks: &[Block], target: usize) -> bool {
    let len = region.len();
    if len == target {
        return true;
    }
    match blocks.iter().rev().find(|b| b.ty == BLOCK_TYPE_PADDING) {
        Some(padding) => match (padding.len + target).checked_sub(len) {
            Some(new_len) if new_len <= MAX_BLOCK_LEN => {
                resize_block(region, padding, new_len);
                true
            }
            _ => false,
        },
        None if target >= len + BLOCK_HEADER_LEN => {
            append_padding(region, blocks, target - len - BLOCK_HEADER_LEN);
            true
        }
        None => false,
    }
}

/// Gives `region` a padding block of at least `min_len` bytes.
fn set_padding(region: &mut Vec<u8>, blocks: &[Block], min_len: usize) {
    match blocks.iter().rev().find(|b| b.ty == BLOCK_TYPE_PADDING) {
        Some(padding) if padding.len < min_len => resize_block(region, padding, min_len),
        Some(_) => {}
        None => append_padding(region, blocks, min_len),
    }
}

fn resize_block(region: &mut Vec<u8>, block: &Block, new_len: usize) {
    let content = block.header_at + BLOCK_HEADER_LEN;
    region[block.header_at + 1..content].copy_from_slice(&(new_len as u32).to_be_bytes()[1..]);
    if new_len > block.len {
        let at = content + block.len;
        region.splice(at..at, std::iter::repeat_n(0, new_len - block.len));
    } else {
        region.drain(content + new_len..content + block.len);
    }
}

/// Appends a padding block of `len` bytes, which becomes the last block.
fn append_padding(region: &mut Vec<u8>, blocks: &[Block], len: usize) {
    if let Some(last) = blocks.last() {
        region[last.header_at] &= 0x7f;
    }
    region.push(0x80 | BLOCK_TYPE_PADDING);
    region.extend_from_slice(&(len as u32).to_be_bytes()[1..]);
    region.resize(region.len() + len, 0);
}

/// Moves everything from `from` to the end of `file` so it starts at `to`.
fn move_frames(file: &mut File, from: u64, to: u64) -> io::Result<()> {
    let file_len = file.seek(SeekFrom::End(0))?;
    let mut buf = vec![0u8; MOVE_CHUNK];
    if to > from {
        // Back to front, so no chunk is overwritten before it has been read.
        let shift = to - from;
        let mut cursor = file_len;
        while cursor > from {
            let chunk = (cursor - from).min(MOVE_CHUNK as u64) as usize;
            let src = cursor - chunk as u64;
            file.seek(SeekFrom::Start(src))?;
            file.read_exact(&mut buf[..chunk])?;
            file.seek(SeekFrom::Start(src + shift))?;
            file.write_all(&buf[..chunk])?;
            cursor = src;
        }
    } else if to < from {
        let shift = from - to;
        let mut cursor = from;
        while cursor < file_len {
            let chunk = (file_len - cursor).min(MOVE_CHUNK as u64) as usize;
            file.seek(SeekFrom::Start(cursor))?;
            file.read_exact(&mut buf[..chunk])?;
            file.seek(SeekFrom::Start(cursor - shift))?;
            file.write_all(&buf[..chunk])?;
            cursor += chunk as u64;
        }
        file.set_len(file_len - shift)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region_with(blocks: &[(u8, usize)]) -> Vec<u8> {
        let mut out = b"fLaC".to_vec();
        for (i, &(ty, len)) in blocks.iter().enumerate() {
            let last = i + 1 == blocks.len();
            out.push(if last { 0x80 | ty } else { ty });
            out.extend_from_slice(&(len as u32).to_be_bytes()[1..]);
            out.resize(out.len() + len, 0xAA);
        }
        out
    }

    fn shape(region: &[u8]) -> Vec<(u8, usize)> {
        parse_blocks(region)
            .expect("valid region")
            .iter()
            .map(|b| (b.ty, b.len))
            .collect()
    }

    #[test]
    fn fit_grows_or_shrinks_padding_in_the_middle() {
        let mut region = region_with(&[(0, 34), (1, 100), (4, 50)]);
        let target = region.len();
        // The comment grew by 30 bytes: padding gives them up.
        let mut grown = region_with(&[(0, 34), (1, 100), (4, 80)]);
        let blocks = parse_blocks(&grown).unwrap();
        assert!(fit_to_len(&mut grown, &blocks, target));
        assert_eq!(shape(&grown), vec![(0, 34), (1, 70), (4, 80)]);

        let blocks = parse_blocks(&region).unwrap();
        assert!(fit_to_len(&mut region, &blocks, target + 10));
        assert_eq!(shape(&region), vec![(0, 34), (1, 110), (4, 50)]);
    }

    #[test]
    fn fit_appends_padding_only_when_a_header_fits() {
        let base = region_with(&[(0, 34), (4, 50)]);
        let mut region = base.clone();
        let blocks = parse_blocks(&region).unwrap();
        assert!(fit_to_len(&mut region, &blocks, base.len() + 20));
        assert_eq!(shape(&region), vec![(0, 34), (4, 50), (1, 16)]);

        let mut region = base.clone();
        let blocks = parse_blocks(&region).unwrap();
        assert!(!fit_to_len(&mut region, &blocks, base.len() + 3));
        assert_eq!(region, base);
    }

    #[test]
    fn fit_refuses_when_padding_is_too_small() {
        let mut region = region_with(&[(0, 34), (4, 80), (1, 10)]);
        let blocks = parse_blocks(&region).unwrap();
        let target = region.len() - 20;
        assert!(!fit_to_len(&mut region, &blocks, target));
    }

    #[test]
    fn parse_skips_a_leading_id3v2_tag() {
        let mut region = b"ID3\x04\x00\x00\x00\x00\x00\x05hello".to_vec();
        region.extend(region_with(&[(0, 34), (1, 8)]));
        assert_eq!(shape(&region), vec![(0, 34), (1, 8)]);
    }

    #[test]
    fn parse_rejects_a_region_with_trailing_bytes() {
        let mut region = region_with(&[(0, 34)]);
        region.push(0);
        assert!(parse_blocks(&region).is_none());
    }

    #[test]
    fn save_returns_false_for_a_file_that_is_not_flac() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not.flac");
        std::fs::write(&path, b"RIFF\0\0\0\0WAVE").unwrap();
        let mut file = File::options().read(true).write(true).open(&path).unwrap();
        assert!(!save(&mut file, |_| Ok(())).unwrap());
        assert_eq!(file.stream_position().unwrap(), 0);
        assert_eq!(std::fs::read(&path).unwrap(), b"RIFF\0\0\0\0WAVE");
    }

    #[test]
    fn move_frames_shifts_both_ways_across_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        let tail = fixture::audio_bytes(MOVE_CHUNK + 1234);
        let mut bytes = vec![7u8; 100];
        bytes.extend_from_slice(&tail);
        std::fs::write(&path, &bytes).unwrap();
        let mut file = File::options().read(true).write(true).open(&path).unwrap();

        move_frames(&mut file, 100, 5000).unwrap();
        assert_eq!(&std::fs::read(&path).unwrap()[5000..], &tail[..]);
        move_frames(&mut file, 5000, 10).unwrap();
        let after = std::fs::read(&path).unwrap();
        assert_eq!(after.len(), 10 + tail.len());
        assert_eq!(&after[10..], &tail[..]);
    }
}

/// Where the frames of the FLAC `file` start, for tests in other modules.
#[cfg(test)]
pub(crate) fn frames_start(file: &mut File) -> u64 {
    metadata_region_len(file).unwrap().expect("a FLAC file")
}

#[cfg(test)]
pub(crate) mod fixture {
    use std::path::Path;

    /// Bytes standing in for the audio frames of a fixture FLAC.
    pub(crate) fn audio_bytes(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    /// Writes a minimal FLAC to `path`: `fLaC`, a STREAMINFO block, an empty
    /// Vorbis comment, a PADDING block of `padding` bytes when given, then
    /// [`audio_bytes`]`(audio_len)` as the frames. Returns where the frames start.
    pub(crate) fn write_flac(path: &Path, padding: Option<u32>, audio_len: usize) -> u64 {
        let mut out = b"fLaC".to_vec();

        let mut streaminfo = vec![0x10, 0x00, 0x10, 0x00, 0, 0, 0, 0, 0, 0];
        let total_samples: u64 = 44_100 * 10;
        let packed: u64 = (44_100u64 << 44) | (1u64 << 41) | (15u64 << 36) | total_samples;
        streaminfo.extend_from_slice(&packed.to_be_bytes());
        streaminfo.extend_from_slice(&[0u8; 16]);
        push_block(&mut out, 0, false, &streaminfo);

        let vendor = b"fixture";
        let mut comment = (vendor.len() as u32).to_le_bytes().to_vec();
        comment.extend_from_slice(vendor);
        comment.extend_from_slice(&0u32.to_le_bytes());
        push_block(&mut out, 4, padding.is_none(), &comment);

        if let Some(len) = padding {
            push_block(&mut out, 1, true, &vec![0u8; len as usize]);
        }

        let audio_start = out.len() as u64;
        out.extend_from_slice(&audio_bytes(audio_len));
        std::fs::write(path, out).expect("write fixture flac");
        audio_start
    }

    fn push_block(out: &mut Vec<u8>, ty: u8, last: bool, content: &[u8]) {
        out.push(if last { 0x80 | ty } else { ty });
        out.extend_from_slice(&(content.len() as u32).to_be_bytes()[1..]);
        out.extend_from_slice(content);
    }
}
