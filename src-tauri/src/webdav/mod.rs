//! WebDAV client, PROPFIND XML parser, and remote file probing.
//!
//! Provides directory enumeration via WebDAV `PROPFIND` (RFC 4918), connection testing,
//! and metadata extraction for remote audio files without full file downloads.

use crate::models::{FileType, Song, SongSource};
use anyhow::{anyhow, Context, Result};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, RANGE};
use std::io::Cursor;
use std::time::Duration;

/// An item found during WebDAV directory enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebDavItem {
    pub href: String,
    pub is_directory: bool,
    pub content_length: Option<u64>,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
}

/// WebDAV HTTP client.
#[derive(Clone)]
pub struct WebDavClient {
    base_url: String,
    username: Option<String>,
    password: Option<String>,
    client: Client,
}

impl WebDavClient {
    pub fn new(
        base_url: String,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self> {
        let trimmed_url = base_url.trim_end_matches('/').to_string();
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to create http client")?;

        Ok(Self {
            base_url: trimmed_url,
            username,
            password,
            client,
        })
    }

    /// Construct authorization headers if credentials are configured.
    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            use base64::Engine;
            let creds = format!("{u}:{p}");
            let encoded = base64::engine::general_purpose::STANDARD.encode(creds);
            if let Ok(val) = HeaderValue::from_str(&format!("Basic {encoded}")) {
                headers.insert(AUTHORIZATION, val);
            }
        }
        headers
    }

    /// Resolve an absolute or relative path to a full URL on this server.
    pub fn build_url(&self, path: &str) -> String {
        let clean_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        format!("{}{clean_path}", self.base_url)
    }

    /// Resolve a path to a full URL with credentials embedded as URL userinfo
    /// (`scheme://user:pass@host/path`). The audio engine only ever has the
    /// bare URL string stored on the `Song` to work with — no separate
    /// credential lookup is available at playback time — so this is how
    /// Basic Auth reaches WebDAV streaming requests (see `audio.rs`'s
    /// `HttpRangeReader`, which extracts and strips the userinfo again
    /// before sending the request).
    pub fn build_authenticated_url(&self, path: &str) -> String {
        let base = self.build_url(path);
        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            if let Ok(mut parsed) = reqwest::Url::parse(&base) {
                if parsed.set_username(u).is_ok() && parsed.set_password(Some(p)).is_ok() {
                    return parsed.to_string();
                }
            }
        }
        base
    }

    /// Test server connectivity and authentication using PROPFIND with Depth: 0.
    pub fn test_connection(&self) -> Result<bool> {
        let url = self.build_url("");
        let mut headers = self.auth_headers();
        headers.insert("Depth", HeaderValue::from_static("0"));

        let resp = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .headers(headers)
            .send()
            .context("failed to reach WebDAV server")?;

        if resp.status().is_success() || resp.status().as_u16() == 207 {
            Ok(true)
        } else if resp.status().as_u16() == 401 {
            Err(anyhow!("Authentication failed (HTTP 401 Unauthorized)"))
        } else {
            Err(anyhow!("WebDAV returned HTTP status {}", resp.status()))
        }
    }

    /// List resources under `remote_path` using PROPFIND with Depth: 1.
    pub fn list_directory(&self, remote_path: &str) -> Result<Vec<WebDavItem>> {
        let url = self.build_url(remote_path);
        let mut headers = self.auth_headers();
        headers.insert("Depth", HeaderValue::from_static("1"));
        headers.insert("Content-Type", HeaderValue::from_static("application/xml"));

        let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:resourcetype/>
    <D:getcontentlength/>
    <D:getlastmodified/>
    <D:getetag/>
  </D:prop>
</D:propfind>"#;

        let resp = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .headers(headers)
            .body(propfind_body)
            .send()
            .context("failed to execute PROPFIND request")?;

        if !resp.status().is_success() && resp.status().as_u16() != 207 {
            return Err(anyhow!("PROPFIND failed with status {}", resp.status()));
        }

        let xml_text = resp.text().context("failed to read WebDAV response body")?;
        parse_propfind_response(&xml_text)
    }

    /// Fetch partial bytes via HTTP Range request.
    pub fn fetch_range(&self, url: &str, start: u64, end: u64) -> Result<Vec<u8>> {
        let mut headers = self.auth_headers();
        let range_val = format!("bytes={start}-{end}");
        headers.insert(RANGE, HeaderValue::from_str(&range_val)?);

        let resp = self
            .client
            .get(url)
            .headers(headers)
            .send()
            .context("failed to fetch byte range")?;

        if !resp.status().is_success() && resp.status().as_u16() != 206 {
            return Err(anyhow!("Range request failed with HTTP {}", resp.status()));
        }

        let bytes = resp.bytes().context("failed to read range response bytes")?;
        Ok(bytes.to_vec())
    }

    /// Probes remote file metadata using byte ranges.
    /// Fetches initial 256KB for ID3v2/FLAC/Vorbis headers and trailing 128KB for ID3v1/APEv2.
    pub fn probe_song_tags(&self, url: &str, content_length: u64) -> Result<Song> {
        let initial_probe_size = 256 * 1024;
        let head_size = initial_probe_size.min(content_length);
        let head_bytes = self.fetch_range(url, 0, head_size.saturating_sub(1))?;

        let tail_bytes = if content_length > head_size {
            let tail_probe_size = 128 * 1024;
            let tail_start = content_length.saturating_sub(tail_probe_size).max(head_size);
            self.fetch_range(url, tail_start, content_length.saturating_sub(1))
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut probe_buffer = Vec::with_capacity(head_bytes.len() + tail_bytes.len());
        probe_buffer.extend_from_slice(&head_bytes);
        if !tail_bytes.is_empty() {
            probe_buffer.extend_from_slice(&tail_bytes);
        }

        let mut cursor = Cursor::new(probe_buffer);
        let filetype = detect_filetype_from_url(url);

        let mut song = Song {
            source: SongSource::WebDav,
            filetype,
            url: Some(url.to_string()),
            stream_url: Some(url.to_string()),
            filesize: Some(content_length as i64),
            ..Default::default()
        };

        if let Ok(tagged_file) = Probe::new(&mut cursor)
            .guess_file_type()
            .map_err(|e| anyhow::anyhow!(e))
            .and_then(|p| p.read().map_err(|e| anyhow::anyhow!(e)))
        {
            let properties = tagged_file.properties();
            let duration_ns = (properties.duration().as_secs_f64() * 1_000_000_000.0) as i64;
            song.length_nanosec = Some(duration_ns);
            song.bitrate = properties.audio_bitrate().map(|b| b as i32);
            song.samplerate = properties.sample_rate().map(|r| r as i32);
            song.channels = properties.channels().map(|c| c as i32);
            song.bitdepth = properties.bit_depth().map(|b| b as i32);

            let mut candidate_tags = Vec::new();
            if let Some(primary) = tagged_file.primary_tag() {
                candidate_tags.push(primary);
            }
            for t in tagged_file.tags() {
                if !candidate_tags.iter().any(|existing| std::ptr::eq(*existing, t)) {
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

        // Fallback: if title is missing, infer from filename
        if song.title.is_none() {
            if let Some(filename) = url.split('/').last() {
                let name = filename.split('?').next().unwrap_or(filename);
                if let Some(idx) = name.rfind('.') {
                    song.title = Some(name[..idx].to_string());
                } else {
                    song.title = Some(name.to_string());
                }
            }
        }

        Ok(song)
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

/// Parse WebDAV XML PROPFIND multistatus response.
pub fn parse_propfind_response(xml: &str) -> Result<Vec<WebDavItem>> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut items = Vec::new();
    let mut current_href = String::new();
    let mut current_is_dir = false;
    let mut current_length = None;
    let mut current_mtime = None;
    let mut current_etag = None;

    let mut inside_response = false;
    let mut inside_resourcetype = false;
    let mut current_tag = String::new();
    // Accumulates a leaf element's text across however many events it arrives
    // in — quick-xml 0.41 delivers an entity reference (e.g. `&amp;`) as its
    // own `GeneralRef` event, splitting what used to be one `Text` event into
    // `Text` + `GeneralRef` + `Text`. Committed into the matching current_*
    // field only once the leaf element's `End` event confirms it's complete.
    let mut text_buf = String::new();

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                current_tag = name.to_ascii_lowercase();
                text_buf.clear();

                if current_tag == "response" {
                    inside_response = true;
                    current_href.clear();
                    current_is_dir = false;
                    current_length = None;
                    current_mtime = None;
                    current_etag = None;
                } else if current_tag == "resourcetype" {
                    inside_resourcetype = true;
                } else if inside_resourcetype && current_tag == "collection" {
                    current_is_dir = true;
                }
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                let tag_lower = name.to_ascii_lowercase();
                if inside_resourcetype && tag_lower == "collection" {
                    current_is_dir = true;
                }
            }
            Ok(Event::Text(e)) => {
                if inside_response {
                    // `decode()` handles the document's byte encoding only — entity
                    // references arrive separately as `GeneralRef` events (below).
                    if let Ok(decoded) = e.decode() {
                        text_buf.push_str(&decoded);
                    }
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if inside_response {
                    if let Ok(Some(ch)) = e.resolve_char_ref() {
                        text_buf.push(ch);
                    } else if let Ok(name) = e.decode() {
                        // The five predefined XML entities — a DTD-less WebDAV
                        // PROPFIND response can't define any others.
                        match name.as_ref() {
                            "amp" => text_buf.push('&'),
                            "lt" => text_buf.push('<'),
                            "gt" => text_buf.push('>'),
                            "apos" => text_buf.push('\''),
                            "quot" => text_buf.push('"'),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                let tag_lower = name.to_ascii_lowercase();

                if inside_response {
                    match tag_lower.as_str() {
                        "href" => {
                            // & is illegal unencoded in a URL path — it's a query-separator.
                            // Re-encode it (and bare spaces) so the href is a valid URL path.
                            current_href = text_buf.replace('&', "%26").replace(' ', "%20");
                        }
                        "getcontentlength" => {
                            current_length = text_buf.trim().parse::<u64>().ok()
                        }
                        "getlastmodified" => current_mtime = Some(text_buf.trim().to_string()),
                        "getetag" => current_etag = Some(text_buf.trim().to_string()),
                        _ => {}
                    }
                }
                text_buf.clear();

                if tag_lower == "resourcetype" {
                    inside_resourcetype = false;
                } else if tag_lower == "response" {
                    inside_response = false;
                    if !current_href.is_empty() {
                        items.push(WebDavItem {
                            href: current_href.clone(),
                            is_directory: current_is_dir || current_href.ends_with('/'),
                            content_length: current_length,
                            last_modified: current_mtime.clone(),
                            etag: current_etag.clone(),
                        });
                    }
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(anyhow!("Error parsing PROPFIND XML: {e}")),
        }
        buf.clear();
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_propfind_xml() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/remote.php/webdav/Music/</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype><D:collection/></D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/remote.php/webdav/Music/song.mp3</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype/>
        <D:getcontentlength>5242880</D:getcontentlength>
        <D:getlastmodified>Wed, 21 Oct 2025 07:28:00 GMT</D:getlastmodified>
        <D:getetag>"abcd1234efgh"</D:getetag>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let items = parse_propfind_response(xml).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].href, "/remote.php/webdav/Music/");
        assert!(items[0].is_directory);

        assert_eq!(items[1].href, "/remote.php/webdav/Music/song.mp3");
        assert!(!items[1].is_directory);
        assert_eq!(items[1].content_length, Some(5242880));
        assert_eq!(
            items[1].last_modified.as_deref(),
            Some("Wed, 21 Oct 2025 07:28:00 GMT")
        );
        assert_eq!(items[1].etag.as_deref(), Some("\"abcd1234efgh\""));
    }

    #[test]
    fn test_build_authenticated_url_embeds_credentials() {
        let client = WebDavClient::new(
            "http://127.0.0.1:8080".to_string(),
            Some("test".to_string()),
            Some("test".to_string()),
        )
        .unwrap();

        assert_eq!(
            client.build_authenticated_url("/Music/song.mp3"),
            "http://test:test@127.0.0.1:8080/Music/song.mp3"
        );
    }

    #[test]
    fn test_build_authenticated_url_without_credentials_is_unchanged() {
        let client = WebDavClient::new("http://127.0.0.1:8080".to_string(), None, None).unwrap();

        assert_eq!(
            client.build_authenticated_url("/Music/song.mp3"),
            "http://127.0.0.1:8080/Music/song.mp3"
        );
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
}
