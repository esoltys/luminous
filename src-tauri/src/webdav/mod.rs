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

/// Removes any `user:pass@` userinfo from a URL, re-serialised in normal form.
/// Unparseable input is returned unchanged.
pub fn strip_url_credentials(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(mut parsed) => {
            let _ = parsed.set_username("");
            let _ = parsed.set_password(None);
            parsed.to_string()
        }
        Err(_) => url.to_string(),
    }
}

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

    /// The URL stored as a song's `path`/`url`/`stream_url`: normalised the way
    /// `strip_url_credentials` leaves it, and never carrying credentials. Playback
    /// looks the server's credentials up when it opens the track (#1492).
    pub fn playback_url(&self, path: &str) -> String {
        strip_url_credentials(&self.build_url(path))
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

    /// Fetch a resource's full body — used for small non-audio files (e.g. a
    /// folder-art image found during sync, #1082) where there's no benefit
    /// to `fetch_range`'s partial-probe behavior.
    pub fn fetch_full(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(url)
            .headers(self.auth_headers())
            .send()
            .context("failed to fetch resource")?;

        if !resp.status().is_success() {
            return Err(anyhow!("GET failed with HTTP {}", resp.status()));
        }

        let bytes = resp.bytes().context("failed to read response bytes")?;
        Ok(bytes.to_vec())
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

        let bytes = resp
            .bytes()
            .context("failed to read range response bytes")?;
        Ok(bytes.to_vec())
    }

    /// Fetches `[start, end)` of the file into `buf` at the same offsets (zero-extending
    /// it), skipping whatever the head probe already holds. False if it came up short.
    fn fill_probe_range(
        &self,
        url: &str,
        buf: &mut Vec<u8>,
        head_len: usize,
        start: usize,
        end: usize,
    ) -> bool {
        let from = start.max(head_len);
        if end <= from {
            return true;
        }
        match self.fetch_range(url, from as u64, (end - 1) as u64) {
            Ok(bytes) => {
                if buf.len() < from + bytes.len() {
                    buf.resize(from + bytes.len(), 0);
                }
                buf[from..from + bytes.len()].copy_from_slice(&bytes);
                bytes.len() == end - from
            }
            Err(_) => false,
        }
    }

    /// Reads past the head probe when the tags don't fit in it: a FLAC with a large
    /// padding or picture block, or an MP3 with a large ID3v2 tag (embedded art). The
    /// head is cut mid-tag otherwise and the file's tags are lost. FLAC padding is
    /// skipped, not downloaded. Returns the (possibly longer) buffer, plus whether it
    /// already holds every tag so the tail probe can be skipped.
    fn extend_probe_head(
        &self,
        url: &str,
        mut buf: Vec<u8>,
        content_length: u64,
    ) -> (Vec<u8>, bool) {
        const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
        let head_len = buf.len();
        if content_length as usize <= head_len {
            return (buf, false);
        }

        if buf.starts_with(b"ID3") && buf.len() >= 10 {
            let size = ((buf[6] as usize & 0x7f) << 21)
                | ((buf[7] as usize & 0x7f) << 14)
                | ((buf[8] as usize & 0x7f) << 7)
                | (buf[9] as usize & 0x7f);
            // The tag plus a little audio: the MPEG reader insists on finding a frame.
            let end = (size + 10 + 128 * 1024).min(content_length as usize);
            if end > head_len && size <= MAX_METADATA_BYTES {
                self.fill_probe_range(url, &mut buf, head_len, head_len, end);
            }
            return (buf, false);
        }

        if buf.starts_with(b"fLaC") {
            let mut pos = 4usize;
            let mut extended = false;
            let mut fetched = 0usize;
            loop {
                if !self.fill_probe_range(url, &mut buf, head_len, pos, pos + 4) {
                    return (buf, extended);
                }
                let header = buf[pos];
                let is_last = header & 0x80 != 0;
                let block_type = header & 0x7f;
                let len = ((buf[pos + 1] as usize) << 16)
                    | ((buf[pos + 2] as usize) << 8)
                    | buf[pos + 3] as usize;
                let end = pos + 4 + len;
                if end > head_len {
                    extended = true;
                    if block_type == 1 {
                        // Padding is all zeros: stand in for it instead of downloading it.
                        if buf.len() < end {
                            buf.resize(end, 0);
                        }
                    } else {
                        fetched += len;
                        if fetched > MAX_METADATA_BYTES
                            || !self.fill_probe_range(url, &mut buf, head_len, pos + 4, end)
                        {
                            return (buf, false);
                        }
                    }
                }
                pos = end;
                if is_last {
                    return (buf, extended);
                }
            }
        }

        (buf, false)
    }

    /// Probes remote file metadata using byte ranges.
    /// Fetches the first 256KB for ID3v2/FLAC/Vorbis headers, then the trailing 128KB
    /// (ID3v1/APEv2, an MP4 `moov` atom, an Ogg last page) only if the head left the
    /// duration or a core tag unresolved: most files cost one request, not two.
    pub fn probe_song_tags(&self, url: &str, content_length: u64) -> Result<Song> {
        let initial_probe_size = 256 * 1024;
        let head_size = initial_probe_size.min(content_length);
        let head_bytes = self.fetch_range(url, 0, head_size.saturating_sub(1))?;

        // A tag block bigger than the head probe (FLAC padding or embedded art,
        // a large ID3v2 tag) would be cut off and the tags lost: read the rest.
        let (head_bytes, metadata_complete) =
            self.extend_probe_head(url, head_bytes, content_length);

        let mut song = parse_probe_buffer(&head_bytes, url, content_length);
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
                song = parse_probe_buffer(&probe_buffer, url, content_length);
            }
        }
        title_from_filename_if_missing(&mut song, url);
        Ok(song)
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

/// Parses tags and stream properties out of a probe buffer (the head, or head + tail).
fn parse_probe_buffer(buffer: &[u8], url: &str, content_length: u64) -> Song {
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
        song.samplerate = properties.sample_rate().map(|r| r as i32);
        song.channels = properties.channels().map(|c| c as i32);
        song.bitdepth = properties.bit_depth().map(|b| b as i32);

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

    song
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

/// Whether a PROPFIND entry's `href` names the collection that was listed. A
/// `Depth: 1` listing includes the collection itself, which must not be walked
/// again as a child. Servers percent-encode hrefs (`/Alice%20in%20Chains/`) while a
/// saved remote path is typed as-is (`/Alice in Chains`), so compare decoded paths.
pub fn is_listed_collection(item_href: &str, listed_path: &str) -> bool {
    use percent_encoding::percent_decode_str;
    let decode = |p: &str| {
        percent_decode_str(p.trim_end_matches('/'))
            .decode_utf8_lossy()
            .into_owned()
    };
    decode(item_href) == decode(listed_path)
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
                let name = e.local_name().as_ref().to_string();
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
                let name = e.local_name().as_ref().to_string();
                let tag_lower = name.to_ascii_lowercase();
                if inside_resourcetype && tag_lower == "collection" {
                    current_is_dir = true;
                }
            }
            Ok(Event::Text(e)) => {
                if inside_response {
                    // Text content arrives pre-decoded as `&str` — entity
                    // references arrive separately as `GeneralRef` events (below).
                    text_buf.push_str(&e);
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if inside_response {
                    if let Ok(Some(ch)) = e.resolve_char_ref() {
                        text_buf.push(ch);
                    } else {
                        // The five predefined XML entities — a DTD-less WebDAV
                        // PROPFIND response can't define any others.
                        match e.as_ref() {
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
                let name = e.local_name().as_ref().to_string();
                let tag_lower = name.to_ascii_lowercase();

                if inside_response {
                    match tag_lower.as_str() {
                        "href" => {
                            // & is illegal unencoded in a URL path — it's a query-separator.
                            // Re-encode it (and bare spaces) so the href is a valid URL path.
                            current_href = text_buf.replace('&', "%26").replace(' ', "%20");
                        }
                        "getcontentlength" => current_length = text_buf.trim().parse::<u64>().ok(),
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
    #[test]
    fn listed_collection_matches_across_percent_encoding_and_trailing_slash() {
        use super::is_listed_collection;
        assert!(is_listed_collection(
            "/Alice%20in%20Chains/",
            "/Alice in Chains"
        ));
        assert!(is_listed_collection(
            "/Alice%20in%20Chains/",
            "/Alice%20in%20Chains"
        ));
        assert!(is_listed_collection("/music/", "/music"));
        assert!(!is_listed_collection(
            "/Alice%20in%20Chains/Jar%20of%20Flies/",
            "/Alice in Chains"
        ));
    }

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

    /// A FLAC whose tags sit behind more than the head probe's worth of data used
    /// to lose them entirely (#1493): the probe reads on past a big picture block
    /// and stands in for the padding that follows it instead of downloading it.
    #[tokio::test]
    async fn extend_probe_head_reads_past_the_head_and_skips_padding() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer};

        fn block(kind: u8, last: bool, body: Vec<u8>) -> Vec<u8> {
            let mut out = vec![kind | if last { 0x80 } else { 0 }];
            out.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
            out.extend(body);
            out
        }
        let picture = vec![7u8; 300_000];
        let mut file = b"fLaC".to_vec();
        file.extend(block(0, false, vec![1; 34]));
        file.extend(block(6, false, picture.clone()));
        file.extend(block(1, true, vec![0; 400_000]));
        let metadata_len = file.len();
        file.extend(vec![9u8; 1_000]); // audio frames
        let served = file.clone();

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(move |req: &wiremock::Request| {
                let (start, end) = req
                    .headers
                    .get(wiremock::http::HeaderName::from_static("range"))
                    .and_then(|r| r.to_str().ok())
                    .and_then(|r| r.strip_prefix("bytes="))
                    .and_then(|r| r.split_once('-'))
                    .map(|(s, e)| (s.parse::<usize>().unwrap(), e.parse::<usize>().unwrap()))
                    .unwrap();
                let end = end.min(served.len() - 1);
                wiremock::ResponseTemplate::new(206).set_body_bytes(served[start..=end].to_vec())
            })
            .mount(&server)
            .await;

        let base = server.uri();
        let url = format!("{base}/song.flac");
        let total = file.len() as u64;
        let head = file[..256 * 1024].to_vec();
        let (buf, complete) = tokio::task::spawn_blocking(move || {
            let client = WebDavClient::new(base, None, None).unwrap();
            client.extend_probe_head(&url, head, total)
        })
        .await
        .unwrap();

        assert!(
            complete,
            "every tag block was read, so the tail probe is skipped"
        );
        assert_eq!(buf.len(), metadata_len);
        assert_eq!(
            &buf[..],
            &file[..metadata_len - 400_000]
                .iter()
                .copied()
                .chain(std::iter::repeat_n(0, 400_000))
                .collect::<Vec<u8>>()[..]
        );
    }

    #[test]
    fn test_playback_url_never_embeds_credentials() {
        let client = WebDavClient::new(
            "http://127.0.0.1:8080".to_string(),
            Some("test".to_string()),
            Some("test".to_string()),
        )
        .unwrap();

        assert_eq!(
            client.playback_url("/Music/song.mp3"),
            "http://127.0.0.1:8080/Music/song.mp3"
        );
    }

    #[test]
    fn test_strip_url_credentials() {
        assert_eq!(
            strip_url_credentials("http://u:p%40ss@host:8080/a b/c.mp3"),
            "http://host:8080/a%20b/c.mp3"
        );
        assert_eq!(
            strip_url_credentials("https://host/Music/c.mp3"),
            "https://host/Music/c.mp3"
        );
        assert_eq!(strip_url_credentials("not a url"), "not a url");
    }

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
    fn title_falls_back_to_the_decoded_file_name() {
        let mut song = super::Song::default();
        super::title_from_filename_if_missing(&mut song, "https://h/a%20b/Track%20One.mp3?x=1");
        assert_eq!(song.title.as_deref(), Some("Track One"));
    }
}
