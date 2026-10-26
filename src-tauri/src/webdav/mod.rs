//! WebDAV integration for Luminous (#1082, #1492, #1767).
//!
//! Protocol mechanics (PROPFIND XML parsing, HTTP range requests, credential stripping)
//! live in [`luminous_remote::webdav`]. This module provides SQLite credential
//! resolution and audio tag probing into [`Song`].

use crate::models::{FileType, Song, SongSource};
use anyhow::Result;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
pub use luminous_remote::webdav::*;
use std::io::Cursor;

/// The `Authorization: Basic ...` value for a credential-free song URL, from the
/// saved server it belongs to (#1492). Prefers the most specific server when
/// several share a host. `None` when no saved server matches or it has no
/// credentials, so plain HTTP streams are untouched.
pub fn resolve_auth_header(conn: &rusqlite::Connection, song_url: &str) -> Option<String> {
    let mut stmt = conn
        .prepare("SELECT url, username, password FROM webdav_servers")
        .ok()?;
    let servers = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .ok()?;
    let (_, user, pass) = servers
        .flatten()
        .filter(|(url, ..)| crate::collection::song_matches_webdav_server(song_url, url))
        .max_by_key(|(url, ..)| {
            reqwest::Url::parse(url).map_or(0, |u| u.path().trim_end_matches('/').len())
        })?;
    let (user, pass) = (user?, pass?);
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
    Some(format!("Basic {encoded}"))
}

/// Extension trait adding tag probing into `Song` for `WebDavClient`.
pub trait WebDavProbeExt {
    /// Fetches the first 256KB for ID3v2/FLAC/Vorbis headers, then the trailing 128KB
    /// (ID3v1/APEv2, an MP4 `moov` atom, an Ogg last page) only if the head left the
    /// duration or a core tag unresolved: most files cost one request, not two.
    fn probe_song_tags(&self, url: &str, content_length: u64) -> Result<Song>;

    /// Like [`WebDavProbeExt::probe_song_tags`], also returning the top-ranked
    /// embedded picture bytes when present in the probed tag buffer (#1823).
    fn probe_song_tags_with_art(
        &self,
        url: &str,
        content_length: u64,
    ) -> Result<(Song, Option<Vec<u8>>)>;
}

impl WebDavProbeExt for WebDavClient {
    fn probe_song_tags(&self, url: &str, content_length: u64) -> Result<Song> {
        self.probe_song_tags_with_art(url, content_length)
            .map(|(song, _)| song)
    }

    fn probe_song_tags_with_art(
        &self,
        url: &str,
        content_length: u64,
    ) -> Result<(Song, Option<Vec<u8>>)> {
        let (head_bytes, metadata_complete) = self.fetch_probe_head(url, content_length)?;
        let (mut song, mut embedded_art) =
            parse_probe_buffer_with_art(&head_bytes, url, content_length);
        let head_len = head_bytes.len() as u64;
        if !metadata_complete
            && content_length > head_len
            && !head_resolves_song(&song, &head_bytes)
        {
            let tail_probe_size = 128 * 1024;
            let tail_start = content_length.saturating_sub(tail_probe_size).max(head_len);
            if let Ok(tail_bytes) =
                self.fetch_range(url, tail_start, content_length.saturating_sub(1))
            {
                let mut probe_buffer = head_bytes;
                probe_buffer.extend_from_slice(&tail_bytes);
                let (tail_song, tail_art) =
                    parse_probe_buffer_with_art(&probe_buffer, url, content_length);
                song = tail_song;
                if tail_art.is_some() {
                    embedded_art = tail_art;
                }
                song.art_embedded = embedded_art.is_some();
            }
        }
        title_from_filename_if_missing(&mut song, url);
        Ok((song, embedded_art))
    }
}
/// Whether the head alone settled everything the tail could still change.
///
/// FLAC keeps its duration (STREAMINFO) and every tag in the header, so any head that
/// yields a duration is final. MP3 carries its tags in an ID3v2 header, but without
/// one (or with core tags missing) ID3v1/APEv2 in the last bytes may still supply
/// them. Every other format falls back to the tail: an MP4 `moov` atom or an Ogg last
/// page decides the duration, and a head-only parse of those fails outright.
fn head_resolves_song(song: &Song, head: &[u8]) -> bool {
    let has_duration = song.length_nanosec.is_some_and(|d| d > 0);
    match song.filetype {
        FileType::Flac => has_duration,
        FileType::Mp3 => {
            head.starts_with(b"ID3")
                && has_duration
                && song.title.is_some()
                && song.artist.is_some()
                && song.album.is_some()
        }
        _ => false,
    }
}

/// Byte length of a leading ID3v2 tag (header and footer included), or 0 if there is none.
fn id3v2_len(buffer: &[u8]) -> usize {
    if buffer.len() < 10 || !buffer.starts_with(b"ID3") {
        return 0;
    }
    let size = buffer[6..10]
        .iter()
        .fold(0usize, |acc, b| (acc << 7) | (*b & 0x7f) as usize);
    let footer = if buffer[5] & 0x10 != 0 { 10 } else { 0 };
    10 + size + footer
}

/// Offset of the first FLAC audio frame, found by walking the metadata block headers;
/// 0 if the walk runs off the buffer.
fn flac_audio_start(buffer: &[u8]) -> usize {
    let mut pos = 4;
    while let Some(h) = buffer.get(pos..pos + 4) {
        let end = pos + 4 + ((h[1] as usize) << 16 | (h[2] as usize) << 8 | h[3] as usize);
        if h[0] & 0x80 != 0 {
            return end;
        }
        pos = end;
    }
    0
}

/// Replaces the duration and bitrate lofty derived from a truncated probe buffer.
///
/// Lofty can only see the bytes we fetched, so anything computed from the stream length
/// is wrong: an MP3 without a Xing/Info/VBRI header gets a duration estimated from the
/// buffer, and a FLAC gets an average bitrate over the buffer. `content_length` is the
/// real size, so recompute those two values from it.
fn correct_size_dependent_properties(song: &mut Song, buffer: &[u8], content_length: u64) {
    match song.filetype {
        FileType::Flac => {
            // STREAMINFO makes the duration exact; only the bitrate depends on size.
            let secs = song.length_nanosec.unwrap_or(0) as f64 / 1e9;
            if secs > 0.0 {
                // Lofty's own figure counts audio only, not the tags or embedded art.
                let audio_bytes = content_length.saturating_sub(flac_audio_start(buffer) as u64);
                song.bitrate = Some((audio_bytes as f64 * 8.0 / secs / 1000.0).round() as i32);
            }
        }
        FileType::Mp3 => {
            let audio_start = id3v2_len(buffer);
            let Some(kbps) = song.bitrate.filter(|b| *b > 0) else {
                return;
            };
            let frame_header_end = (audio_start + 4096).min(buffer.len());
            let has_vbr_header = buffer
                .get(audio_start..frame_header_end)
                .is_some_and(|frame| {
                    [&b"Xing"[..], b"Info", b"VBRI"]
                        .iter()
                        .any(|tag| frame.windows(tag.len()).any(|w| w == *tag))
                });
            // Only a head that reaches the first frame can prove the header is absent.
            if has_vbr_header || audio_start >= buffer.len() {
                return;
            }
            let audio_bytes = (content_length as usize).saturating_sub(audio_start) as f64;
            let secs = audio_bytes * 8.0 / (kbps as f64 * 1000.0);
            song.length_nanosec = Some((secs * 1e9) as i64);
        }
        _ => {}
    }
}

/// Parses tags and stream properties out of a probe buffer (the head, or head + tail).
#[cfg(test)]
fn parse_probe_buffer(buffer: &[u8], url: &str, content_length: u64) -> Song {
    parse_probe_buffer_with_art(buffer, url, content_length).0
}

/// Parses tags, stream properties, and any complete embedded picture out of a probe buffer (#1823).
fn parse_probe_buffer_with_art(
    buffer: &[u8],
    url: &str,
    content_length: u64,
) -> (Song, Option<Vec<u8>>) {
    let mut cursor = Cursor::new(buffer);
    let filetype = detect_filetype_from_url(url);

    let mut song = Song {
        source: SongSource::WebDav,
        filetype,
        url: Some(url.to_string()),
        stream_url: Some(url.to_string()),
        filesize: Some(content_length as i64),
        ..Default::default()
    };
    let mut embedded_art = None;

    let parsed = Probe::new(&mut cursor)
        .guess_file_type()
        .map_err(|e| anyhow::anyhow!(e))
        .and_then(|p| p.read().map_err(|e| anyhow::anyhow!(e)));
    if let Err(e) = &parsed {
        log::warn!("Could not read tags from {url}: {e}");
    }
    if let Ok(tagged_file) = parsed {
        let properties = tagged_file.properties();
        let duration_ns = (properties.duration().as_secs_f64() * 1_000_000_000.0) as i64;
        song.length_nanosec = Some(duration_ns);
        song.bitrate = properties.audio_bitrate().map(|b| b as i32);
        if (buffer.len() as u64) < content_length {
            correct_size_dependent_properties(&mut song, buffer, content_length);
        }
        song.samplerate = properties.sample_rate().map(|r| r as i32);
        song.channels = properties.channels().map(|c| c as i32);
        song.bitdepth = properties.bit_depth().map(|b| b as i32);

        embedded_art = crate::covermanager::CoverManager::best_embedded_picture(&tagged_file);
        song.art_embedded = embedded_art.is_some();

        let mut candidate_tags = Vec::new();
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

        for tag in candidate_tags {
            use lofty::tag::{Accessor, ItemKey};
            if song.title.is_none() {
                song.title = tag.title().map(|t| t.to_string());
            }
            if song.artist.is_none() {
                song.artist = tag.artist().map(|a| a.to_string());
            }
            if song.album.is_none() {
                song.album = tag.album().map(|a| a.to_string());
            }
            if song.genre.is_none() {
                song.genre = tag.genre().map(|g| g.to_string());
            }
            if song.track.is_none() {
                song.track = tag.track().map(|t| t as i32);
            }
            if song.disc.is_none() {
                song.disc = tag.disk().map(|d| d as i32);
            }
            if song.year.is_none() {
                song.year = tag.date().map(|d| d.year as i32).or_else(|| {
                    tag.get_string(ItemKey::Year)
                        .and_then(|s| s.trim().parse::<i32>().ok())
                });
            }
        }
    }

    (song, embedded_art)
}

/// Falls back to the file name for a title when the tags hold none.
fn title_from_filename_if_missing(song: &mut Song, url: &str) {
    if song.title.is_none() {
        if let Some(filename) = url.split('/').next_back() {
            let name = filename.split('?').next().unwrap_or(filename);
            // URLs are percent-encoded; the title should read as the file name does.
            let name = percent_encoding::percent_decode_str(name).decode_utf8_lossy();
            let stem = name.rfind('.').map_or(&*name, |idx| &name[..idx]);
            song.title = Some(stem.to_string());
        }
    }
}

/// Detect file type from URL extension.
pub fn detect_filetype_from_url(url: &str) -> FileType {
    let clean = url.split('?').next().unwrap_or(url);
    if let Some(ext) = clean.rsplit('.').next() {
        match ext.to_ascii_lowercase().as_str() {
            "mp3" => FileType::Mp3,
            "flac" => FileType::Flac,
            "ogg" => FileType::OggVorbis,
            "opus" => FileType::OggOpus,
            "m4a" | "aac" => FileType::Aac,
            "alac" => FileType::Alac,
            "wav" => FileType::Wav,
            "aiff" | "aif" => FileType::Aiff,
            "wv" => FileType::WavPack,
            "mpc" => FileType::Mpc,
            "ape" => FileType::Ape,
            "dsf" => FileType::Dsf,
            "dff" => FileType::Dsdiff,
            _ => FileType::Unknown,
        }
    } else {
        FileType::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_resolve_auth_header_uses_the_matching_servers_credentials() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE webdav_servers (url TEXT, username TEXT, password TEXT);
             INSERT INTO webdav_servers VALUES ('http://nas/dav', 'a', 'b');
             INSERT INTO webdav_servers VALUES ('http://nas/dav/kids', 'c', 'd');
             INSERT INTO webdav_servers VALUES ('http://open/dav', NULL, NULL);",
        )
        .unwrap();
        // base64("a:b") / base64("c:d")
        assert_eq!(
            resolve_auth_header(&conn, "http://nas/dav/x.mp3").as_deref(),
            Some("Basic YTpi")
        );
        assert_eq!(
            resolve_auth_header(&conn, "http://nas/dav/kids/x.mp3").as_deref(),
            Some("Basic Yzpk")
        );
        assert_eq!(resolve_auth_header(&conn, "http://open/dav/x.mp3"), None);
        assert_eq!(resolve_auth_header(&conn, "http://other/x.mp3"), None);
    }

    #[test]
    fn test_detect_filetype_from_url() {
        assert_eq!(
            detect_filetype_from_url("https://example.com/music/test.flac?auth=token"),
            FileType::Flac
        );
        assert_eq!(
            detect_filetype_from_url("/files/album/track01.mp3"),
            FileType::Mp3
        );
        assert_eq!(
            detect_filetype_from_url("/files/album/unknown.xyz"),
            FileType::Unknown
        );
    }

    /// rclone encodes `&` in directory names as `&amp;` in the XML response body.
    /// `quick-xml`'s `unescape()` converts `&amp;` → `&`, which is illegal unencoded
    /// in a URL path (it is treated as a query-string separator). The parser must
    /// re-encode it as `%26` so the resulting href is a valid URL path component.
    #[test]
    fn test_parse_propfind_xml_amp_in_href_is_reencoded() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/Music/BandCamp/Astropilot%20&amp;%20Crows%20Labyrinth/</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype><D:collection/></D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/Music/BandCamp/Astropilot%20&amp;%20Crows%20Labyrinth/track.mp3</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype/>
        <D:getcontentlength>1234567</D:getcontentlength>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let items = parse_propfind_response(xml).unwrap();
        assert_eq!(items.len(), 2);

        // The & must be re-encoded as %26 — NOT left as a bare &
        assert_eq!(
            items[0].href,
            "/Music/BandCamp/Astropilot%20%26%20Crows%20Labyrinth/"
        );
        assert!(items[0].is_directory);

        assert_eq!(
            items[1].href,
            "/Music/BandCamp/Astropilot%20%26%20Crows%20Labyrinth/track.mp3"
        );
        assert!(!items[1].is_directory);
        assert_eq!(items[1].content_length, Some(1234567));
    }

    const AUDIO_FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audio/");

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{AUDIO_FIXTURES}{name}")).unwrap()
    }

    /// Parses the first `head_len` bytes the way `probe_song_tags` does, before any tail.
    fn probe_head(name: &str, head_len: usize) -> (super::Song, bool) {
        let bytes = fixture(name);
        let url = format!("https://example.com/{name}");
        let head = &bytes[..head_len.min(bytes.len())];
        let song = super::parse_probe_buffer(head, &url, bytes.len() as u64);
        let resolved = super::head_resolves_song(&song, head);
        (song, resolved)
    }

    #[test]
    fn flac_head_alone_skips_the_tail_even_without_an_album_tag() {
        let (song, resolved) = probe_head("song_gamma.flac", 50 * 1024);
        assert!(resolved);
        assert_eq!(song.length_nanosec, Some(800_000_000));
        assert_eq!(song.title.as_deref(), Some("Song Gamma"));
    }

    #[test]
    fn mp3_with_complete_id3v2_tags_skips_the_tail() {
        let (song, resolved) = probe_head("song_alpha.mp3", 2718);
        assert!(resolved);
        assert_eq!(song.album.as_deref(), Some("Album Gold"));
    }

    #[test]
    fn untagged_mp3_keeps_the_tail_for_id3v1() {
        let (_, resolved) = probe_head("song_short.mp3", 881);
        assert!(!resolved);
    }

    #[test]
    fn formats_that_need_the_file_end_keep_the_tail() {
        for name in ["song_beta.wav", "song_delta.ogg", "song_epsilon.m4a"] {
            let (song, resolved) = probe_head(name, 1024);
            assert!(!resolved, "{name} must fetch the tail");
            assert!(
                song.length_nanosec.is_none(),
                "{name} head parse is partial"
            );
        }
    }

    #[test]
    fn head_plus_tail_recovers_what_a_head_alone_cannot() {
        for (name, head_len, duration) in [
            ("song_delta.ogg", 1024, 500_000_000),
            ("song_epsilon.m4a", 1024, 2_023_000_000),
        ] {
            let bytes = fixture(name);
            let url = format!("https://example.com/{name}");
            let mut probe = bytes[..head_len].to_vec();
            probe.extend_from_slice(&bytes[head_len..]);
            let song = super::parse_probe_buffer(&probe, &url, bytes.len() as u64);
            assert_eq!(song.length_nanosec, Some(duration), "{name}");
            assert!(song.title.is_some(), "{name}");
        }
    }

    #[test]
    fn flac_bitrate_comes_from_the_real_file_size() {
        let bytes = fixture("song_gamma.flac");
        let (song, _) = probe_head("song_gamma.flac", 50 * 1024);
        let audio = bytes.len() - super::flac_audio_start(&bytes);
        let expected = (audio as f64 * 8.0 / 0.8 / 1000.0).round() as i32;
        assert_eq!(song.bitrate, Some(expected));
    }

    #[test]
    fn flac_bitrate_ignores_embedded_art_bytes() {
        let bytes = fixture("song_gamma.flac");
        let (plain, _) = probe_head("song_gamma.flac", 50 * 1024);
        // The same audio claimed to sit in a file 300 KB larger (art stood in as padding).
        let url = "https://example.com/song_gamma.flac";
        let head = &bytes[..50 * 1024];
        let song = super::parse_probe_buffer(head, url, bytes.len() as u64 + 300_000);
        let audio_start = super::flac_audio_start(&bytes);
        assert!(audio_start > 0);
        let expected = ((bytes.len() - audio_start) as f64 * 8.0 / 0.8 / 1000.0).round() as i32;
        assert_eq!(plain.bitrate, Some(expected));
        // With 300 KB extra the figure must rise by exactly those bytes' worth.
        assert!(song.bitrate.unwrap() > plain.bitrate.unwrap());
    }

    #[test]
    fn headerless_mp3_duration_uses_the_real_file_size() {
        let bytes = fixture("song_alpha.mp3");
        let (full, _) = probe_head("song_alpha.mp3", bytes.len());
        let (head, _) = probe_head("song_alpha.mp3", 2718);
        let (full, head) = (full.length_nanosec.unwrap(), head.length_nanosec.unwrap());
        // Tolerate lofty's frame-boundary rounding; the truncated head used to be far off.
        assert!((full - head).abs() < 50_000_000, "full={full} head={head}");
    }

    #[test]
    fn title_falls_back_to_the_decoded_file_name() {
        let mut song = super::Song::default();
        super::title_from_filename_if_missing(&mut song, "https://h/a%20b/Track%20One.mp3?x=1");
        assert_eq!(song.title.as_deref(), Some("Track One"));
    }

    #[test]
    fn parse_probe_buffer_with_art_extracts_embedded_picture() {
        use lofty::config::WriteOptions;
        use lofty::file::TaggedFileExt;
        use lofty::picture::{MimeType, Picture, PictureType};
        use lofty::probe::Probe;
        use lofty::tag::TagExt;

        let temp_dir = tempfile::tempdir().unwrap();
        let mp3_path = temp_dir.path().join("with_art.mp3");
        std::fs::copy(format!("{AUDIO_FIXTURES}song_alpha.mp3"), &mp3_path).unwrap();

        let pic_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F'];
        let mut tagged = Probe::open(&mp3_path).unwrap().read().unwrap();
        let tag = tagged.primary_tag_mut().unwrap();
        tag.push_picture(
            Picture::unchecked(pic_bytes.clone())
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Jpeg)
                .build(),
        );
        tag.save_to_path(&mp3_path, WriteOptions::default())
            .unwrap();

        let bytes = std::fs::read(&mp3_path).unwrap();
        let (song, extracted) = super::parse_probe_buffer_with_art(
            &bytes,
            "https://example.com/with_art.mp3",
            bytes.len() as u64,
        );
        assert!(song.art_embedded);
        assert_eq!(extracted.as_deref(), Some(pic_bytes.as_slice()));
    }
}
