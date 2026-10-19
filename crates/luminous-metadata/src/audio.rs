//! Audio tag reading/writing and stream property inspection.
//!
//! Encapsulates all lofty interactions, format quirks (ID3v2, Vorbis Comments,
//! MP4 atoms, APEv2), Windows sharing lock retry backoffs, read-only bit clearance,
//! language sanitization, and multi-value mapping.

use anyhow::{anyhow, Context, Result};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag, TagItem};
use std::collections::HashSet;
use std::path::Path;

// ---------------------------------------------------------------------------
// Data Models
// ---------------------------------------------------------------------------

/// Technical audio stream properties extracted from an audio file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioFileProperties {
    pub duration_nanosec: i64,
    pub bitrate: Option<i32>,
    pub samplerate: Option<i32>,
    pub channels: Option<i32>,
    pub bitdepth: Option<i32>,
    pub is_vbr: Option<bool>,
}

/// Structured track metadata read from on-disk audio tags.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioFileMetadata {
    pub properties: AudioFileProperties,
    pub title: Option<String>,
    pub titlesort: Option<String>,
    pub artist: Option<String>,
    pub artistsort: Option<String>,
    pub album: Option<String>,
    pub albumsort: Option<String>,
    pub album_artist: Option<String>,
    pub album_artist_sort: Option<String>,
    pub composer: Option<String>,
    pub composersort: Option<String>,
    pub genre: Option<String>,
    pub comment: Option<String>,
    pub year: Option<i32>,
    pub originalyear: Option<i32>,
    pub track: Option<i32>,
    pub disc: Option<i32>,
    pub compilation: bool,
    pub grouping: Option<String>,
    pub bpm: Option<f32>,
    pub initial_key: Option<String>,
    pub replaygain_track_gain: Option<f64>,
    pub replaygain_album_gain: Option<f64>,
    pub musicbrainz_artist_id: Option<String>,
    pub musicbrainz_album_artist_id: Option<String>,
    pub musicbrainz_album_id: Option<String>,
    pub musicbrainz_release_group_id: Option<String>,
    pub musicbrainz_recording_id: Option<String>,
    pub musicbrainz_track_id: Option<String>,
    pub musicbrainz_work_id: Option<String>,
    pub musicbrainz_release_type: Option<String>,
    pub musicbrainz_release_country: Option<String>,
    pub barcode: Option<String>,
    pub catalog_number: Option<String>,
    pub lyrics: Option<String>,
    pub art_embedded: bool,
}

/// Every field [`write_tags`] needs to persist a song's tags to disk,
/// grouped into one request instead of a long positional argument list.
/// Borrows rather than owns its string fields — callers already hold the
/// owned data (loaded from the DB or a genre-rewrite result) and just need
/// to name it once per write. Has no `genresort` field: `genresort` has no
/// on-disk tag mapping and was never used by `write_tags`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TagWriteRequest<'a> {
    pub title: &'a str,
    pub titlesort: Option<&'a str>,
    pub artist: &'a str,
    pub artistsort: Option<&'a str>,
    pub album: &'a str,
    pub albumsort: Option<&'a str>,
    pub album_artist: &'a str,
    pub album_artist_sort: Option<&'a str>,
    pub composer: &'a str,
    pub composersort: Option<&'a str>,
    pub genre: &'a str,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub originalyear: Option<u32>,
    pub grouping: &'a str,
    pub bpm: Option<f32>,
    pub initial_key: &'a str,
    pub compilation: bool,
}

// ---------------------------------------------------------------------------
// Tag Reading
// ---------------------------------------------------------------------------

/// Reads technical audio properties and tag metadata from an audio file on disk.
pub fn read_tags(path: &Path) -> Result<AudioFileMetadata> {
    let tagged_file = Probe::open(path)
        .context("lofty: cannot open file")?
        .read()
        .context("lofty: cannot read file")?;

    let properties = tagged_file.properties();

    let duration_nanosec = (properties.duration().as_secs_f64() * 1_000_000_000.0) as i64;
    let bitrate = properties.audio_bitrate().map(|b| b as i32);
    let samplerate = properties.sample_rate().map(|r| r as i32);
    let channels = properties.channels().map(|c| c as i32);
    let bitdepth = properties.bit_depth().map(|b| b as i32);

    // Lofty's public API reports only the average bitrate for VBR MP3s, with no way to
    // tell VBR and CBR apart. Sniff the Xing/Info/VBRI header ourselves so callers can
    // label an average as an average instead of implying a constant rate.
    let is_vbr = if tagged_file.file_type() == lofty::file::FileType::Mpeg {
        detect_mp3_vbr(path)
    } else {
        None
    };

    let props = AudioFileProperties {
        duration_nanosec,
        bitrate,
        samplerate,
        channels,
        bitdepth,
        is_vbr,
    };

    let mut meta = AudioFileMetadata {
        properties: props,
        ..Default::default()
    };

    // Prefer the primary tag (ID3v2, VorbisComment, etc.), but fall back to secondary tags
    // (ID3v1, APE, etc.) if the primary tag is missing or lacks specific fields.
    let mut candidate_tags: Vec<&Tag> = Vec::new();
    if let Some(primary) = tagged_file.primary_tag() {
        candidate_tags.push(primary);
    }
    for t in tagged_file.tags() {
        if !candidate_tags
            .iter()
            .any(|existing| std::ptr::eq(*existing, t))
        {
            candidate_tags.push(t);
        }
    }

    for tag in candidate_tags.iter().copied() {
        if meta.title.is_none() {
            meta.title = tag.title().map(|t| t.to_string());
        }
        if meta.artist.is_none() {
            meta.artist = read_multi_value(tag, ItemKey::TrackArtist);
        }
        if meta.album.is_none() {
            meta.album = tag.album().map(|a| a.to_string());
        }
        if meta.genre.is_none() {
            meta.genre = read_multi_value(tag, ItemKey::Genre);
        }
        if meta.comment.is_none() {
            meta.comment = tag.comment().map(|c| c.to_string());
        }
        // `date()` checks ItemKey::RecordingDate (ID3v2 TDRC, MP4, Vorbis DATE)
        // before falling back to ItemKey::Year (ID3v1 only) — using Year alone
        // silently drops the year for every ID3v2-tagged file (#428).
        if meta.year.is_none() {
            meta.year = tag.date().map(|d| d.year as i32).or_else(|| {
                tag.get_string(ItemKey::Year)
                    .and_then(|s| s.trim().parse::<i32>().ok())
            });
        }
        // Original release date (ID3v2 TDOR, MP4/Vorbis equivalents).
        // Parsed the same relaxed way `date()` parses ItemKey::RecordingDate.
        if meta.originalyear.is_none() {
            meta.originalyear = tag
                .get_string(ItemKey::OriginalReleaseDate)
                .and_then(|s| {
                    lofty::tag::items::Timestamp::parse(
                        &mut s.as_bytes(),
                        lofty::config::ParsingMode::Relaxed,
                    )
                    .ok()
                    .flatten()
                })
                .map(|d| d.year as i32);
        }
        if meta.track.is_none() {
            meta.track = tag.track().map(|t| t as i32);
        }
        if meta.disc.is_none() {
            meta.disc = tag.disk().map(|d| d as i32);
        }
        if meta.album_artist.is_none() {
            meta.album_artist = read_multi_value(tag, ItemKey::AlbumArtist);
        }
        if !meta.compilation {
            meta.compilation = tag
                .get_string(ItemKey::FlagCompilation)
                .map(|s| s.trim() == "1")
                .unwrap_or(false);
        }
        if meta.composer.is_none() {
            meta.composer = read_multi_value(tag, ItemKey::Composer);
        }
        if meta.grouping.is_none() {
            meta.grouping = tag.get_string(ItemKey::ContentGroup).map(|s| s.to_string());
        }
        if meta.initial_key.is_none() {
            meta.initial_key = tag.get_string(ItemKey::InitialKey).map(|s| s.to_string());
        }
        if meta.bpm.is_none() {
            meta.bpm = tag
                .get_string(ItemKey::IntegerBpm)
                .or_else(|| tag.get_string(ItemKey::Bpm))
                .and_then(|s| s.trim().parse::<f32>().ok())
                .filter(|&b| b > 0.0);
        }
        if meta.replaygain_track_gain.is_none() {
            meta.replaygain_track_gain = tag
                .get_string(ItemKey::ReplayGainTrackGain)
                .and_then(parse_replaygain_db);
        }
        if meta.replaygain_album_gain.is_none() {
            meta.replaygain_album_gain = tag
                .get_string(ItemKey::ReplayGainAlbumGain)
                .and_then(parse_replaygain_db);
        }
        if meta.titlesort.is_none() {
            meta.titlesort = tag
                .get_string(ItemKey::TrackTitleSortOrder)
                .map(|s| s.to_string());
        }
        if meta.artistsort.is_none() {
            meta.artistsort = tag
                .get_string(ItemKey::TrackArtistSortOrder)
                .map(|s| s.to_string());
        }
        if meta.albumsort.is_none() {
            meta.albumsort = tag
                .get_string(ItemKey::AlbumTitleSortOrder)
                .map(|s| s.to_string());
        }
        if meta.album_artist_sort.is_none() {
            meta.album_artist_sort = tag
                .get_string(ItemKey::AlbumArtistSortOrder)
                .map(|s| s.to_string());
        }
        if meta.composersort.is_none() {
            meta.composersort = tag
                .get_string(ItemKey::ComposerSortOrder)
                .map(|s| s.to_string());
        }
        if meta.musicbrainz_artist_id.is_none() {
            meta.musicbrainz_artist_id = tag
                .get_string(ItemKey::MusicBrainzArtistId)
                .map(|s| s.to_string());
        }
        if meta.musicbrainz_album_artist_id.is_none() {
            meta.musicbrainz_album_artist_id = tag
                .get_string(ItemKey::MusicBrainzReleaseArtistId)
                .map(|s| s.to_string());
        }
        if meta.musicbrainz_album_id.is_none() {
            meta.musicbrainz_album_id = tag
                .get_string(ItemKey::MusicBrainzReleaseId)
                .map(|s| s.to_string());
        }
        if meta.musicbrainz_release_group_id.is_none() {
            meta.musicbrainz_release_group_id = tag
                .get_string(ItemKey::MusicBrainzReleaseGroupId)
                .map(|s| s.to_string());
        }
        if meta.musicbrainz_recording_id.is_none() {
            meta.musicbrainz_recording_id = tag
                .get_string(ItemKey::MusicBrainzRecordingId)
                .map(|s| s.to_string());
        }
        if meta.musicbrainz_track_id.is_none() {
            meta.musicbrainz_track_id = tag
                .get_string(ItemKey::MusicBrainzTrackId)
                .map(|s| s.to_string());
        }
        if meta.musicbrainz_work_id.is_none() {
            meta.musicbrainz_work_id = tag
                .get_string(ItemKey::MusicBrainzWorkId)
                .map(|s| s.to_string());
        }
        if meta.musicbrainz_release_type.is_none() {
            meta.musicbrainz_release_type = tag
                .get_string(ItemKey::MusicBrainzReleaseType)
                .map(|s| s.to_string());
        }
        if meta.musicbrainz_release_country.is_none() {
            meta.musicbrainz_release_country = tag
                .get_string(ItemKey::ReleaseCountry)
                .map(|s| s.to_string());
        }
        if meta.barcode.is_none() {
            meta.barcode = tag.get_string(ItemKey::Barcode).map(|s| s.to_string());
        }
        if meta.catalog_number.is_none() {
            meta.catalog_number = tag
                .get_string(ItemKey::CatalogNumber)
                .map(|s| s.to_string());
        }
    }

    meta.lyrics = read_embedded_lyrics(path, tagged_file.file_type(), &candidate_tags);
    meta.art_embedded = candidate_tags.iter().any(|t| !t.pictures().is_empty());

    Ok(meta)
}

/// Reads all values under `key` from `tag`, joined into the canonical `; `-delimited
/// storage format (see `crate::values::join_multi_value`).
pub fn read_multi_value(tag: &Tag, key: ItemKey) -> Option<String> {
    let mut seen = HashSet::new();
    let values: Vec<String> = tag
        .get_strings(key)
        .flat_map(crate::values::parse_multi_value)
        .filter(|v| seen.insert(v.to_lowercase()))
        .collect();

    if values.is_empty() {
        None
    } else {
        Some(crate::values::join_multi_value(&values))
    }
}

/// Parse a ReplayGain gain tag value (e.g. "-6.2 dB", "-6.2") into a f64.
pub fn parse_replaygain_db(s: &str) -> Option<f64> {
    s.trim()
        .trim_end_matches(|c: char| c.is_alphabetic() || c.is_whitespace())
        .trim()
        .parse::<f64>()
        .ok()
}

/// Detect whether an MP3 file is VBR-encoded by looking for a Xing/Info/VBRI header
/// in the first audio frame. "Xing" and "VBRI" mark true VBR; "Info" is the Xing
/// encoder's own marker for CBR. A plain CBR file has no such header at all,
/// so `Some(false)` covers both cases. Returns `None` only if the file couldn't be read.
pub fn detect_mp3_vbr(path: &Path) -> Option<bool> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; 8 * 1024];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);

    // Skip a leading ID3v2 tag so its (rare) text content can't be mistaken for a
    // VBR marker — the real header lives in the first MPEG frame, right after it.
    let mut offset = 0;
    if buf.len() >= 10 && &buf[0..3] == b"ID3" {
        let size = ((buf[6] as u32 & 0x7f) << 21)
            | ((buf[7] as u32 & 0x7f) << 14)
            | ((buf[8] as u32 & 0x7f) << 7)
            | (buf[9] as u32 & 0x7f);
        offset = (10 + size as usize).min(buf.len());
    }

    let audio = &buf[offset..];
    let has_marker = |needle: &[u8]| audio.windows(needle.len()).any(|w| w == needle);

    if has_marker(b"Xing") || has_marker(b"VBRI") {
        Some(true)
    } else {
        Some(false)
    }
}

/// Returns true if `text` contains at least one standard LRC timestamp tag (`[MM:SS`).
pub fn is_synced_lrc(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 6 {
        return false;
    }
    for i in 0..(bytes.len() - 5) {
        if bytes[i] == b'['
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3] == b':'
            && bytes[i + 4].is_ascii_digit()
            && bytes[i + 5].is_ascii_digit()
        {
            return true;
        }
    }
    false
}

/// Reads embedded lyrics directly from an audio file on disk, inspecting both native
/// Vorbis comments (for Vorbis-based containers) and candidate ID3/generic tags.
pub fn read_embedded_lyrics_from_path(path: &Path) -> Option<String> {
    let tagged_file = Probe::open(path).ok()?.read().ok()?;
    let mut candidate_tags: Vec<&Tag> = Vec::new();
    if let Some(primary) = tagged_file.primary_tag() {
        candidate_tags.push(primary);
    }
    for t in tagged_file.tags() {
        if !candidate_tags
            .iter()
            .any(|existing| std::ptr::eq(*existing, t))
        {
            candidate_tags.push(t);
        }
    }
    read_embedded_lyrics(path, tagged_file.file_type(), &candidate_tags)
}

/// Extracts embedded lyrics from an audio file, prioritizing synchronized lyrics
/// over unsynchronized lyrics across container formats.
pub fn read_embedded_lyrics(
    path: &Path,
    file_type: lofty::file::FileType,
    candidate_tags: &[&Tag],
) -> Option<String> {
    let mut unsynced_candidate: Option<String> = None;

    // 1. Channel 1: Native Vorbis comments for Vorbis-based containers.
    // lofty's `split_tag()` categorizes unmapped Vorbis comment keys (notably
    // `SYNCEDLYRICS`) as unmapped remainder and drops them from generic `Tag.items`.
    // We read native comments directly to recover them.
    if let Some(vorbis_comments) = read_native_vorbis_comments(path, file_type) {
        // Dedicated synchronized lyrics tags always win as Tier 1.
        for key in ["SYNCEDLYRICS", "SYNCED LYRICS", "SYNCED_LYRICS"] {
            if let Some(val) = vorbis_comments.get(key) {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }

        // General Vorbis lyrics keys: if timestamped, return immediately; otherwise hold as fallback.
        for key in [
            "LYRICS",
            "UNSYNCEDLYRICS",
            "UNSYNCED LYRICS",
            "UNSYNCED_LYRICS",
        ] {
            if let Some(val) = vorbis_comments.get(key) {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    if is_synced_lrc(trimmed) {
                        return Some(trimmed.to_string());
                    } else if unsynced_candidate.is_none() {
                        unsynced_candidate = Some(trimmed.to_string());
                    }
                }
            }
        }
    }

    // 2. Channel 2: Generic and ID3v2 tags from TaggedFile.
    for tag in candidate_tags {
        for key in [ItemKey::Lyrics, ItemKey::UnsyncLyrics] {
            for val in tag.get_strings(key) {
                let trimmed = val.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if is_synced_lrc(trimmed) {
                    return Some(trimmed.to_string());
                } else if unsynced_candidate.is_none() {
                    unsynced_candidate = Some(trimmed.to_string());
                }
            }
        }
    }

    unsynced_candidate
}

/// Attempts to read native Vorbis comments directly from Vorbis-based containers
/// (FLAC, Ogg Vorbis, Opus, Speex) before lofty's `split_tag()` converts them to
/// generic `Tag` and drops unmapped comment keys such as `SYNCEDLYRICS`.
fn read_native_vorbis_comments(
    path: &Path,
    file_type: lofty::file::FileType,
) -> Option<lofty::ogg::tag::VorbisComments> {
    if !matches!(
        file_type,
        lofty::file::FileType::Flac
            | lofty::file::FileType::Opus
            | lofty::file::FileType::Vorbis
            | lofty::file::FileType::Speex
    ) {
        return None;
    }

    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let parse_options = lofty::config::ParseOptions::new()
        .read_cover_art(false)
        .read_properties(false);

    match file_type {
        lofty::file::FileType::Flac => {
            use lofty::flac::FlacFile;
            FlacFile::read_from(&mut reader, parse_options)
                .ok()?
                .vorbis_comments()
                .cloned()
        }
        lofty::file::FileType::Opus => {
            use lofty::ogg::OpusFile;
            let opus = OpusFile::read_from(&mut reader, parse_options).ok()?;
            Some(opus.vorbis_comments().clone())
        }
        lofty::file::FileType::Vorbis => {
            use lofty::ogg::VorbisFile;
            let vorbis = VorbisFile::read_from(&mut reader, parse_options).ok()?;
            Some(vorbis.vorbis_comments().clone())
        }
        lofty::file::FileType::Speex => {
            use lofty::ogg::SpeexFile;
            let speex = SpeexFile::read_from(&mut reader, parse_options).ok()?;
            Some(speex.vorbis_comments().clone())
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tag Writing
// ---------------------------------------------------------------------------

/// Write the given tag fields to `path`'s primary tag, creating one if the
/// file has none yet. Every field is written unconditionally — `Option`
/// fields (`track`, `disc`, `year`, `originalyear`, `bpm`) are cleared from the file when
/// `None` rather than left untouched, so callers must pass the full desired
/// state, not a partial patch. Preserves any existing embedded cover art,
/// which lofty's tag-mutation calls would otherwise silently drop (#106).
/// Retries the on-disk save up to 5 times with a short delay, since it can
/// transiently fail if another process (e.g. an antivirus scanner) has the
/// file open.
pub fn write_tags(path: &Path, req: &TagWriteRequest) -> Result<()> {
    let TagWriteRequest {
        title,
        titlesort,
        artist,
        artistsort,
        album,
        albumsort,
        album_artist,
        album_artist_sort,
        composer,
        composersort,
        genre,
        track,
        disc,
        year,
        originalyear,
        grouping,
        bpm,
        initial_key,
        compilation,
    } = *req;

    let mut tagged_file = Probe::open(path)
        .context("failed to open audio file for tag writing")?
        .read()
        .context("failed to parse audio tags")?;

    let tag = match tagged_file.primary_tag_mut() {
        Some(t) => t,
        None => {
            let tag_type = tagged_file.primary_tag_type();
            tagged_file.insert_tag(Tag::new(tag_type));
            tagged_file
                .primary_tag_mut()
                .ok_or_else(|| anyhow!("could not create a tag frame"))?
        }
    };

    // Guard against embedded cover art being dropped by text edits (#106).
    let original_pictures = tag.pictures().to_vec();

    tag.set_title(title.to_string());
    tag.set_album(album.to_string());

    let mut set_or_remove_sort = |key: lofty::tag::ItemKey, val: Option<&str>| {
        tag.remove_key(key);
        if let Some(v) = val.map(|s| s.trim()).filter(|s| !s.is_empty()) {
            tag.insert_text(key, v.to_string());
        }
    };

    set_or_remove_sort(lofty::tag::ItemKey::TrackTitleSortOrder, titlesort);
    set_or_remove_sort(lofty::tag::ItemKey::TrackArtistSortOrder, artistsort);
    set_or_remove_sort(lofty::tag::ItemKey::AlbumTitleSortOrder, albumsort);
    set_or_remove_sort(lofty::tag::ItemKey::AlbumArtistSortOrder, album_artist_sort);
    set_or_remove_sort(lofty::tag::ItemKey::ComposerSortOrder, composersort);

    tag.remove_key(lofty::tag::ItemKey::TrackArtist);
    for a in crate::values::parse_multi_value(artist) {
        tag.push(lofty::tag::TagItem::new(
            lofty::tag::ItemKey::TrackArtist,
            lofty::tag::ItemValue::Text(a),
        ));
    }

    tag.remove_key(lofty::tag::ItemKey::Genre);
    for g in crate::values::parse_multi_value(genre) {
        tag.push(lofty::tag::TagItem::new(
            lofty::tag::ItemKey::Genre,
            lofty::tag::ItemValue::Text(g),
        ));
    }

    tag.remove_key(lofty::tag::ItemKey::AlbumArtist);
    for aa in crate::values::parse_multi_value(album_artist) {
        tag.push(lofty::tag::TagItem::new(
            lofty::tag::ItemKey::AlbumArtist,
            lofty::tag::ItemValue::Text(aa),
        ));
    }

    tag.remove_key(lofty::tag::ItemKey::Composer);
    for c in crate::values::parse_multi_value(composer) {
        tag.push(lofty::tag::TagItem::new(
            lofty::tag::ItemKey::Composer,
            lofty::tag::ItemValue::Text(c),
        ));
    }

    tag.insert_text(lofty::tag::ItemKey::ContentGroup, grouping.to_string());
    tag.insert_text(lofty::tag::ItemKey::InitialKey, initial_key.to_string());
    tag.insert_text(
        lofty::tag::ItemKey::FlagCompilation,
        if compilation { "1" } else { "0" }.to_string(),
    );

    if let Some(b) = bpm {
        let bpm_str = if b.fract() == 0.0 {
            (b as i64).to_string()
        } else {
            b.to_string()
        };
        tag.insert_text(lofty::tag::ItemKey::IntegerBpm, bpm_str.clone());
        tag.insert_text(lofty::tag::ItemKey::Bpm, bpm_str);
    } else {
        tag.remove_key(lofty::tag::ItemKey::IntegerBpm);
        tag.remove_key(lofty::tag::ItemKey::Bpm);
    }

    if let Some(t) = track {
        tag.set_track(t);
    } else {
        tag.remove_key(lofty::tag::ItemKey::TrackNumber);
    }

    if let Some(d) = disc {
        tag.set_disk(d);
    } else {
        tag.remove_key(lofty::tag::ItemKey::DiscNumber);
    }

    if let Some(y) = year {
        tag.set_date(lofty::tag::items::Timestamp {
            year: y as u16,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        });
    } else {
        tag.remove_date();
        tag.remove_key(lofty::tag::ItemKey::Year);
    }

    if let Some(y) = originalyear {
        tag.insert_text(lofty::tag::ItemKey::OriginalReleaseDate, y.to_string());
    } else {
        tag.remove_key(lofty::tag::ItemKey::OriginalReleaseDate);
    }

    if tag.pictures().is_empty() && !original_pictures.is_empty() {
        for picture in original_pictures {
            tag.push_picture(picture);
        }
    }

    sanitize_tag_languages(tag);

    save_tagged_file_with_retry(&tagged_file, path, "write tags")
}

/// Sanitizes all language-bearing tag items in `tag` before saving to disk.
fn sanitize_tag_languages(tag: &mut Tag) {
    for key in [ItemKey::UnsyncLyrics, ItemKey::Lyrics, ItemKey::Comment] {
        let items: Vec<TagItem> = tag.take(key).collect();
        for mut item in items {
            if item.lang().iter().any(|c| !c.is_ascii_alphabetic()) {
                item.set_lang(*b"XXX");
            }
            tag.push_unchecked(item);
        }
    }
}

/// If `path` has a read-only filesystem attribute, attempt to clear it so tag writing can proceed (#726).
fn ensure_writable(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                perms.set_mode(perms.mode() | 0o200);
            }
            #[cfg(not(unix))]
            {
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
            }
            let _ = std::fs::set_permissions(path, perms);
        }
    }
}

/// Formats the nested source chain of an error into a single string.
pub fn format_error_chain(err: &dyn std::error::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut current = err.source();
    while let Some(cause) = current {
        let msg = cause.to_string();
        if !parts.contains(&msg) {
            parts.push(msg);
        }
        current = cause.source();
    }
    parts.join(": ")
}

fn save_tagged_file_with_retry(
    tagged_file: &lofty::file::TaggedFile,
    path: &Path,
    action: &str,
) -> Result<()> {
    ensure_writable(path);

    let mut attempts = 0;
    loop {
        match tagged_file.save_to_path(path, WriteOptions::default()) {
            Ok(_) => break,
            Err(e) => {
                attempts += 1;
                if attempts >= 5 {
                    return Err(anyhow!(
                        "failed to {action} back to file after multiple attempts: {}",
                        format_error_chain(&e)
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
    Ok(())
}

/// Remove every embedded cover art picture from `path`'s primary tag,
/// leaving all other tag fields untouched (#386). A no-op if the file has no
/// tag, or its tag has no pictures.
pub fn clear_embedded_art(path: &Path) -> Result<()> {
    let mut tagged_file = Probe::open(path)
        .context("failed to open audio file for cover art clearing")?
        .read()
        .context("failed to parse audio tags")?;

    let tag = match tagged_file.primary_tag_mut() {
        Some(t) => t,
        None => return Ok(()),
    };

    if tag.pictures().is_empty() {
        return Ok(());
    }

    while !tag.pictures().is_empty() {
        tag.remove_picture(0);
    }

    sanitize_tag_languages(tag);

    save_tagged_file_with_retry(&tagged_file, path, "clear embedded art")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::tag::ItemValue;

    fn write_test_wav(path: &Path) {
        let sample_rate = 8_000u32;
        let channels = 1u16;
        let bits_per_sample = 16u16;
        let data = vec![0u8; (sample_rate / 10) as usize * 2]; // 100ms of silence

        let byte_rate = sample_rate * channels as u32 * (bits_per_sample / 8) as u32;
        let block_align = channels * (bits_per_sample / 8);

        let mut wav = Vec::with_capacity(44 + data.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);

        std::fs::write(path, wav).expect("failed to write test wav fixture");
    }

    #[test]
    fn test_clear_embedded_art_removes_picture_and_keeps_other_tags() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        let mut tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag_type = tagged_file.primary_tag_type();
        let mut tag = Tag::new(tag_type);
        tag.set_title("Original Title".to_string());
        let picture = Picture::unchecked(vec![0xFF, 0xD8, 0xFF, 0xE0])
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Jpeg)
            .build();
        tag.push_picture(picture);
        tagged_file.insert_tag(tag);
        tagged_file
            .save_to_path(&path, WriteOptions::default())
            .expect("failed to write fixture tag");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        assert_eq!(tagged_file.primary_tag().unwrap().pictures().len(), 1);
        drop(tagged_file);

        clear_embedded_art(&path).expect("clear_embedded_art should succeed");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert!(
            tag.pictures().is_empty(),
            "embedded picture should be removed"
        );
        assert_eq!(
            tag.title().as_deref(),
            Some("Original Title"),
            "clearing artwork must not touch other tag fields"
        );
    }

    #[test]
    fn test_clear_embedded_art_is_noop_without_a_tag() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("untagged.wav");
        write_test_wav(&path);

        clear_embedded_art(&path).expect("clearing an untagged file should not error");
    }

    #[test]
    fn test_write_tags_round_trips_multiple_genres_as_id3v24_multivalue() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                album_artist: "Album Artist",
                composer: "Composer",
                genre: "Rock; Jazz Fusion; Live",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        let genres: Vec<&str> = tag.get_strings(lofty::tag::ItemKey::Genre).collect();
        assert_eq!(genres, vec!["Rock", "Jazz Fusion", "Live"]);
    }

    #[test]
    fn test_write_tags_single_genre_round_trips() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                genre: "Metal",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        let genres: Vec<&str> = tag.get_strings(lofty::tag::ItemKey::Genre).collect();
        assert_eq!(genres, vec!["Metal"]);
    }

    #[test]
    fn test_write_tags_empty_genre_clears_field() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                genre: "Metal",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");
        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                ..Default::default()
            },
        )
        .expect("write_tags with empty genre should succeed");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(tag.get_strings(lofty::tag::ItemKey::Genre).count(), 0);
    }

    #[test]
    fn test_write_tags_round_trips_multiple_artists_album_artists_and_composers() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist A; Artist B",
                album: "Album",
                album_artist: "Album Artist A; Album Artist B",
                composer: "Composer A; Composer B",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(
            tag.get_strings(lofty::tag::ItemKey::TrackArtist)
                .collect::<Vec<_>>(),
            vec!["Artist A", "Artist B"]
        );
        assert_eq!(
            tag.get_strings(lofty::tag::ItemKey::AlbumArtist)
                .collect::<Vec<_>>(),
            vec!["Album Artist A", "Album Artist B"]
        );
        assert_eq!(
            tag.get_strings(lofty::tag::ItemKey::Composer)
                .collect::<Vec<_>>(),
            vec!["Composer A", "Composer B"]
        );
    }

    #[test]
    fn test_write_tags_empty_artist_album_artist_composer_clears_fields() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                album_artist: "Album Artist",
                composer: "Composer",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");
        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                album: "Album",
                ..Default::default()
            },
        )
        .expect("write_tags with empty artist/album_artist/composer should succeed");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(tag.get_strings(lofty::tag::ItemKey::TrackArtist).count(), 0);
        assert_eq!(tag.get_strings(lofty::tag::ItemKey::AlbumArtist).count(), 0);
        assert_eq!(tag.get_strings(lofty::tag::ItemKey::Composer).count(), 0);
    }

    #[test]
    fn test_write_and_read_sort_tags() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "The Beatles",
                titlesort: Some("Beatles, The"),
                artist: "The Beatles",
                artistsort: Some("Beatles, The"),
                album: "Abbey Road",
                albumsort: Some("Abbey Road Sort"),
                album_artist: "The Beatles",
                album_artist_sort: Some("Beatles, The"),
                composer: "McCartney",
                composersort: Some("McCartney, Paul"),
                genre: "Rock",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let meta = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(meta.title.as_deref(), Some("The Beatles"));
        assert_eq!(meta.titlesort.as_deref(), Some("Beatles, The"));
        assert_eq!(meta.artist.as_deref(), Some("The Beatles"));
        assert_eq!(meta.artistsort.as_deref(), Some("Beatles, The"));
        assert_eq!(meta.album.as_deref(), Some("Abbey Road"));
        assert_eq!(meta.albumsort.as_deref(), Some("Abbey Road Sort"));
        assert_eq!(meta.album_artist.as_deref(), Some("The Beatles"));
        assert_eq!(meta.album_artist_sort.as_deref(), Some("Beatles, The"));
        assert_eq!(meta.composer.as_deref(), Some("McCartney"));
        assert_eq!(meta.composersort.as_deref(), Some("McCartney, Paul"));
        assert_eq!(meta.genre.as_deref(), Some("Rock"));
    }

    #[test]
    fn test_write_tags_round_trips_originalyear() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                genre: "Rock",
                year: Some(2009),
                originalyear: Some(1969),
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let meta = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(meta.year, Some(2009));
        assert_eq!(meta.originalyear, Some(1969));

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                genre: "Rock",
                year: Some(2009),
                ..Default::default()
            },
        )
        .expect("write_tags with cleared originalyear should succeed");

        let meta = read_tags(&path).expect("read_tags should succeed");
        assert_eq!(meta.year, Some(2009));
        assert_eq!(meta.originalyear, None);
    }

    #[test]
    fn test_write_tags_clears_readonly_attribute_and_succeeds() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("readonly_song.wav");
        write_test_wav(&path);

        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms).unwrap();
        assert!(
            std::fs::metadata(&path).unwrap().permissions().readonly(),
            "fixture must be marked read-only"
        );

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Updated Title",
                artist: "Artist",
                album: "Album",
                genre: "Rock; Instrumental Rock",
                ..Default::default()
            },
        )
        .expect("write_tags must succeed on read-only file by clearing readonly attribute");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Updated Title"));
    }

    #[test]
    fn test_write_tags_mp3_multiple_genres() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("test.mp3");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/tests/fixtures/audio/song_alpha.mp3");
        if !fixture.exists() {
            return;
        }
        std::fs::copy(&fixture, &path).expect("failed to copy mp3 fixture");

        write_tags(
            &path,
            &TagWriteRequest {
                title: "MP3 Title",
                artist: "MP3 Artist",
                album: "MP3 Album",
                genre: "Rock; Instrumental Rock",
                ..Default::default()
            },
        )
        .expect("write_tags must succeed on mp3 file");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("MP3 Title"));
        let genres: Vec<&str> = tag.get_strings(lofty::tag::ItemKey::Genre).collect();
        assert_eq!(genres, vec!["Rock", "Instrumental Rock"]);
    }

    #[test]
    fn test_format_error_chain_unpacks_nested_sources() {
        #[derive(Debug)]
        struct Level2;
        impl std::fmt::Display for Level2 {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "disk full (os error 28)")
            }
        }
        impl std::error::Error for Level2 {}

        #[derive(Debug)]
        struct Level1(Level2);
        impl std::fmt::Display for Level1 {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "failed to write ID3v2 tag")
            }
        }
        impl std::error::Error for Level1 {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        #[derive(Debug)]
        struct Top(Level1);
        impl std::fmt::Display for Top {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "failed to write Mpeg file")
            }
        }
        impl std::error::Error for Top {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let top = Top(Level1(Level2));
        assert_eq!(
            format_error_chain(&top),
            "failed to write Mpeg file: failed to write ID3v2 tag: disk full (os error 28)"
        );
    }

    #[test]
    fn test_write_tags_sanitizes_null_language_and_succeeds() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song_with_null_lang.mp3");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/tests/fixtures/audio/song_alpha.mp3");
        if !fixture.exists() {
            return;
        }
        std::fs::copy(&fixture, &path).expect("failed to copy mp3 fixture");

        let mut tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag_mut().unwrap();
        let mut invalid_lyrics = TagItem::new(
            ItemKey::UnsyncLyrics,
            ItemValue::Text("Lyrics with null language".to_string()),
        );
        invalid_lyrics.set_lang([0, 0, 0]);
        tag.push_unchecked(invalid_lyrics);
        sanitize_tag_languages(tag);
        tagged_file
            .save_to_path(&path, WriteOptions::default())
            .expect("should save with sanitized language");

        let mut tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag_mut().unwrap();
        let mut invalid_comment = TagItem::new(
            ItemKey::Comment,
            ItemValue::Text("Comment with null language".to_string()),
        );
        invalid_comment.set_lang([0, 0, 0]);
        tag.push_unchecked(invalid_comment);

        sanitize_tag_languages(tag);
        let comment_item = tag.get(ItemKey::Comment).unwrap();
        assert_eq!(comment_item.lang(), b"XXX");

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Healed Title",
                artist: "Artist",
                album: "Album",
                genre: "Rock; Instrumental Rock",
                ..Default::default()
            },
        )
        .expect("write_tags must succeed on file with previously null language in frames");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Healed Title"));
        let lyrics_item = tag.get(ItemKey::UnsyncLyrics).unwrap();
        assert_eq!(lyrics_item.lang(), b"XXX");
    }

    #[test]
    fn test_is_synced_lrc() {
        assert!(is_synced_lrc("[00:12.00] Line of lyrics"));
        assert!(!is_synced_lrc("Plain lyrics line without LRC timestamp"));
        assert!(!is_synced_lrc(""));
    }

    #[test]
    fn test_parse_replaygain_db() {
        assert_eq!(parse_replaygain_db("-6.2 dB"), Some(-6.2));
        assert_eq!(parse_replaygain_db("+1.50 dB"), Some(1.5));
        assert_eq!(parse_replaygain_db("-0.5"), Some(-0.5));
        assert_eq!(parse_replaygain_db("invalid"), None);
    }

    #[test]
    fn test_detect_mp3_vbr_finds_xing_header() {
        let path = write_temp_file("vbr_xing", &[b"\xFF\xFB\x90\x00", b"Xing", &[0u8; 100]]);
        assert_eq!(detect_mp3_vbr(&path), Some(true));
    }

    #[test]
    fn test_detect_mp3_vbr_finds_vbri_header() {
        let path = write_temp_file("vbr_vbri", &[b"\xFF\xFB\x90\x00", b"VBRI", &[0u8; 100]]);
        assert_eq!(detect_mp3_vbr(&path), Some(true));
    }

    #[test]
    fn test_detect_mp3_vbr_no_header_is_cbr() {
        let path = write_temp_file("cbr_plain", &[b"\xFF\xFB\x90\x00", &[0u8; 200]]);
        assert_eq!(detect_mp3_vbr(&path), Some(false));
    }

    #[test]
    fn test_detect_mp3_vbr_info_marker_is_cbr() {
        let path = write_temp_file("cbr_info", &[b"\xFF\xFB\x90\x00", b"Info", &[0u8; 100]]);
        assert_eq!(detect_mp3_vbr(&path), Some(false));
    }

    #[test]
    fn test_detect_mp3_vbr_skips_id3v2_tag_content() {
        let id3_header: &[u8] = &[b'I', b'D', b'3', 3, 0, 0, 0, 0, 0, 4];
        let path = write_temp_file(
            "cbr_id3_xing_text",
            &[id3_header, b"Xing", b"\xFF\xFB\x90\x00", &[0u8; 100]],
        );
        assert_eq!(detect_mp3_vbr(&path), Some(false));
    }

    fn write_temp_file(name: &str, chunks: &[&[u8]]) -> tempfile::TempPath {
        let path = tempfile::Builder::new()
            .prefix(&format!("luminous_vbr_test_{name}_"))
            .tempfile()
            .unwrap()
            .into_temp_path();
        let mut data = Vec::new();
        for chunk in chunks {
            data.extend_from_slice(chunk);
        }
        std::fs::write(&path, data).unwrap();
        path
    }
}
