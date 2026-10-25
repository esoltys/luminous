//! Opens the media behind a stored song location for the audio engine and
//! offline analysis. `luminous_audio` only ever sees a `Box<dyn MediaSource>`:
//! everything about where the bytes come from — local files, WebDAV/HTTP
//! Range requests (#682), `subsonic://` paths signed into stream URLs at open
//! time (#1163), saved-server credentials (#1492) — lives here, so signed URLs,
//! credentials and the HTTP client never cross into the engine crate.

use crate::db::Database;
use luminous_audio::{MediaSource, OfflineDecoder};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, LazyLock};

/// Global shared HTTP client for media sources. Reuses connection pools, DNS caching,
/// and TLS 1.3 session tickets across sequential track playback, prefetching, and offline analysis.
static SHARED_CLIENT: LazyLock<reqwest::blocking::Client> = LazyLock::new(|| {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .tcp_keepalive(std::time::Duration::from_secs(60))
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("Failed to build shared HTTP client for media sources")
});

/// The opener handed to `AudioEngine::with_source_opener`: the same lookup
/// as [`open`], owning its database handle.
pub fn opener(
    db: Arc<Database>,
) -> impl Fn(&str) -> Result<Box<dyn MediaSource>, String> + Send + Sync + 'static {
    move |location| open(&db, location)
}

/// Open a stored song location: a local path, an `http(s)://` URL (WebDAV
/// or remote stream), or a `subsonic://` path. Errors name the location the
/// user knows, never a signed URL.
pub fn open(db: &Database, location: &str) -> Result<Box<dyn MediaSource>, String> {
    open_with(
        location,
        |path| {
            let conn = db.pool.get().map_err(|e| e.to_string())?;
            crate::subsonic::resolve_stream_url(&conn, path).map_err(|e| e.to_string())
        },
        |url| {
            let conn = db.pool.get().ok()?;
            crate::webdav::resolve_auth_header(&conn, url)
        },
        |loc| lookup_filesize(db, loc),
    )
}

/// An offline (waveform) decoder for a stored song location, which may be
/// remote — the decoder itself only knows about the opened bytes.
pub fn open_offline(db: &Database, path: &Path) -> anyhow::Result<OfflineDecoder> {
    let location = path.to_string_lossy();
    let source = open(db, &location).map_err(anyhow::Error::msg)?;
    let extension = path.extension().and_then(|e| e.to_str());
    OfflineDecoder::open(source, extension).map_err(anyhow::Error::msg)
}

/// Look up a known file size in bytes for a song location from the database,
/// matching either `path` or `url` with or without embedded credentials,
/// falling back to `webdav_cache.size` if `songs.filesize` is unset.
fn lookup_filesize(db: &Database, location: &str) -> Option<u64> {
    let conn = db.pool.get().ok()?;
    let (clean_url, _) = extract_basic_auth(location);
    let mut stmt = conn
        .prepare_cached(
            "SELECT COALESCE(NULLIF(s.filesize, 0), c.size) \
             FROM songs s \
             LEFT JOIN webdav_cache c ON c.song_id = s.id \
             WHERE (s.path = ?1 OR s.path = ?2 OR s.url = ?1 OR s.url = ?2) \
               AND (s.filesize > 0 OR c.size > 0) \
             LIMIT 1",
        )
        .ok()?;
    let size: Option<i64> = stmt
        .query_row(rusqlite::params![location, clean_url], |r| r.get(0))
        .ok()?;
    size.filter(|&s| s > 0).map(|s| s as u64)
}

/// [`open`] with the database lookups passed in, so the routing and
/// error wording are testable without a database.
fn open_with(
    location: &str,
    sign_subsonic: impl FnOnce(&str) -> Result<String, String>,
    webdav_auth: impl FnOnce(&str) -> Option<String>,
    lookup_filesize: impl FnOnce(&str) -> Option<u64>,
) -> Result<Box<dyn MediaSource>, String> {
    if location.starts_with(crate::subsonic::URI_SCHEME) {
        // Signed fresh on every open; the URL itself never leaves this
        // function. The signer's errors already name the track by its
        // `subsonic://` path, and the UI prefixes the song title.
        let known_length = lookup_filesize(location);
        let url = sign_subsonic(location)?;
        Ok(Box::new(HttpRangeReader::new_with_options(
            &url,
            Some(location),
            None,
            known_length,
        )?))
    } else if location.starts_with("http://") || location.starts_with("https://") {
        let known_length = lookup_filesize(location);
        let (url, embedded) = extract_basic_auth(location);
        let auth = embedded.or_else(|| webdav_auth(&url));
        Ok(Box::new(HttpRangeReader::new_with_options(
            &url,
            None,
            auth,
            known_length,
        )?))
    } else {
        let file = std::fs::File::open(location)
            .map_err(|e| format!("Cannot open file '{location}': {e}"))?;
        Ok(Box::new(file))
    }
}

/// Splits a `user:pass@` prefix out of a URL's authority, if present, returning
/// the credential-free URL and a ready-to-use `Authorization: Basic ...` header
/// value. WebDAV songs synced before #1492 may still carry credentials embedded
/// as userinfo until the migration scrubs them; current rows are credential-free
/// and get their header from the saved server at open time instead.
fn extract_basic_auth(url: &str) -> (String, Option<String>) {
    let Ok(mut parsed) = reqwest::Url::parse(url) else {
        return (url.to_string(), None);
    };
    let username = parsed.username().to_string();
    let password = parsed.password().map(|p| p.to_string());
    if username.is_empty() && password.is_none() {
        return (url.to_string(), None);
    }

    use percent_encoding::percent_decode_str;
    let decoded_user = percent_decode_str(&username)
        .decode_utf8_lossy()
        .to_string();
    let decoded_pass = password
        .as_deref()
        .map(|p| percent_decode_str(p).decode_utf8_lossy().to_string())
        .unwrap_or_default();

    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);

    use base64::Engine;
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(format!("{decoded_user}:{decoded_pass}"));
    (parsed.to_string(), Some(format!("Basic {encoded}")))
}

/// A seekable HTTP media source using HTTP Range requests (`Range: bytes=start-end`).
/// Enables streaming audio from WebDAV and remote HTTP endpoints without full downloads.
pub struct HttpRangeReader {
    url: String,
    /// Names the source in error messages instead of `url`, which for
    /// Subsonic carries a password-equivalent auth token.
    label: String,
    auth_header: Option<String>,
    client: reqwest::blocking::Client,
    content_length: u64,
    position: u64,
    buffer: Vec<u8>,
    buffer_start: u64,
    chunk_size: usize,
}

/// True for replies that are an API error document rather than media —
/// Subsonic servers answer a bad `stream.view` request with HTTP 200 and a
/// JSON (or XML) body.
fn is_error_content_type(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| {
            let ct = ct.to_ascii_lowercase();
            ct.starts_with("application/json")
                || ct.starts_with("application/xml")
                || ct.starts_with("text/xml")
        })
        .unwrap_or(false)
}

/// Parses `Content-Range: bytes {first}-{last}/{total}` into `(first, total)`;
/// `total` is `None` when the server sends `*`.
fn parse_content_range(headers: &reqwest::header::HeaderMap) -> Option<(u64, Option<u64>)> {
    let value = headers.get(reqwest::header::CONTENT_RANGE)?.to_str().ok()?;
    let (range, total) = value.trim().strip_prefix("bytes ")?.split_once('/')?;
    let first = range.split_once('-')?.0.trim().parse().ok()?;
    Some((first, total.trim().parse().ok()))
}

/// Parses total length from `Content-Range: bytes .../{total}`, e.g. on 416 responses
/// (`bytes */{total}`).
fn parse_content_range_total(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let value = headers.get(reqwest::header::CONTENT_RANGE)?.to_str().ok()?;
    let (_, total) = value.trim().strip_prefix("bytes ")?.split_once('/')?;
    total.trim().parse().ok()
}

impl HttpRangeReader {
    pub fn new(url: &str) -> Result<Self, String> {
        let (url, auth_header) = extract_basic_auth(url);
        let label = url.clone();
        Self::open(url, label, auth_header, None)
    }

    /// Like `new`, but sends `auth_header` as the `Authorization` value.
    pub fn new_with_auth(url: &str, auth_header: Option<String>) -> Result<Self, String> {
        Self::open(url.to_string(), url.to_string(), auth_header, None)
    }

    /// Like `new`, but error messages name the source as `label` rather than
    /// quoting `url` — for signed URLs that must never reach logs or the UI.
    pub fn new_with_label(url: &str, label: &str) -> Result<Self, String> {
        Self::open(url.to_string(), label.to_string(), None, None)
    }

    /// Opens an HTTP range reader with an explicit label for errors, optional authorization,
    /// and an optional pre-known stream length in bytes.
    pub fn new_with_options(
        url: &str,
        label: Option<&str>,
        auth_header: Option<String>,
        known_length: Option<u64>,
    ) -> Result<Self, String> {
        Self::open(
            url.to_string(),
            label.unwrap_or(url).to_string(),
            auth_header,
            known_length,
        )
    }

    /// Sets or overrides the known content length.
    pub fn with_known_length(mut self, length: u64) -> Self {
        self.content_length = length;
        self
    }

    fn open(
        url: String,
        label: String,
        auth_header: Option<String>,
        known_length: Option<u64>,
    ) -> Result<Self, String> {
        let client = SHARED_CLIENT.clone();

        // If known_length is already provided (e.g. from database filesize or caller),
        // we skip the HEAD request entirely, avoiding a synchronous network
        // round-trip (which can take 2+ seconds on remote servers like Files.fm).
        // Only if known_length is absent do we issue a HEAD request to discover Content-Length.
        let content_length = if let Some(len) = known_length.filter(|&len| len > 0) {
            len
        } else {
            let mut head_req = client.head(&url);
            if let Some(ref h) = auth_header {
                head_req = head_req.header(reqwest::header::AUTHORIZATION, h);
            }
            head_req
                .send()
                .ok()
                .filter(|resp| resp.status().is_success() && !is_error_content_type(resp.headers()))
                .and_then(|resp| {
                    resp.headers()
                        .get(reqwest::header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                })
                .filter(|&len| len > 0)
                .unwrap_or(0)
        };

        let mut reader = Self {
            url,
            label,
            auth_header,
            client,
            content_length,
            position: 0,
            buffer: Vec::new(),
            buffer_start: 0,
            chunk_size: 512 * 1024, // 512 KB buffer chunk for streaming playback
        };

        // Pre-fetch initial chunk so first read is immediate
        reader.fill_buffer(0)?;
        Ok(reader)
    }

    /// When neither HEAD, Content-Range, nor database metadata supplied a content length,
    /// attempt to discover it via a streaming GET without a Range header, reading only the
    /// response headers before dropping the stream.
    fn try_discover_content_length(&mut self) -> Option<u64> {
        if self.content_length > 0 {
            return Some(self.content_length);
        }
        let mut req = self.client.get(&self.url);
        if let Some(ref h) = self.auth_header {
            req = req.header(reqwest::header::AUTHORIZATION, h);
        }
        // Inspecting headers without reading the body drops the response stream,
        // discovering Content-Length without downloading the file.
        if let Ok(resp) = req.send() {
            if resp.status().is_success() && !is_error_content_type(resp.headers()) {
                if let Some(len) = resp
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|&len| len > 0)
                {
                    self.content_length = len;
                    return Some(len);
                }
            }
        }
        None
    }

    fn fill_buffer(&mut self, start: u64) -> Result<(), String> {
        let end = if self.content_length > 0 {
            (start + self.chunk_size as u64 - 1).min(self.content_length - 1)
        } else {
            start + self.chunk_size as u64 - 1
        };

        let range_header = format!("bytes={start}-{end}");
        let mut req = self
            .client
            .get(&self.url)
            .header(reqwest::header::RANGE, range_header);
        if let Some(ref h) = self.auth_header {
            req = req.header(reqwest::header::AUTHORIZATION, h);
        }
        // `without_url`: reqwest errors otherwise quote the (signed) URL.
        let mut resp = req.send().map_err(|e| {
            format!(
                "Range request failed for '{}': {}",
                self.label,
                e.without_url()
            )
        })?;

        if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            if self.content_length == 0 {
                if let Some(total) = parse_content_range_total(resp.headers()) {
                    self.content_length = total;
                }
            }
            self.buffer_start = start;
            self.buffer.clear();
            return Ok(());
        }

        if !resp.status().is_success() {
            return Err(format!(
                "HTTP error {} when accessing '{}'",
                resp.status(),
                self.label
            ));
        }

        if is_error_content_type(resp.headers()) {
            let body = resp.text().unwrap_or_default();
            let reason = crate::subsonic::describe_error_body(&body)
                .unwrap_or_else(|| "the server sent an error page instead of audio".into());
            return Err(format!("Can't play '{}': {reason}", self.label));
        }

        let partial = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let content_range = if partial {
            parse_content_range(resp.headers())
        } else {
            None
        };

        let mut data = Vec::new();
        resp.copy_to(&mut data)
            .map_err(|e| format!("Failed to read stream chunk: {}", e.without_url()))?;

        if partial {
            if let Some((_, Some(total))) = content_range {
                if self.content_length == 0 {
                    self.content_length = total;
                }
            } else if self.content_length == 0 {
                // If Content-Range is omitted (e.g. Files.fm / SabreDAV), and this response
                // delivered fewer bytes than requested, we have reached the end of the stream.
                let expected_chunk = end.saturating_sub(start) + 1;
                if (data.len() as u64) < expected_chunk {
                    self.content_length = start + data.len() as u64;
                }
            }
            self.buffer_start = content_range.map_or(start, |(first, _)| first);
        } else {
            // A plain 200 ignored the Range header and sent the whole file
            // from byte 0, whatever offset was asked for.
            if self.content_length == 0 {
                self.content_length = data.len() as u64;
            }
            self.buffer_start = 0;
        }
        self.buffer = data;
        Ok(())
    }
}

impl Read for HttpRangeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.content_length > 0 && self.position >= self.content_length {
            return Ok(0); // EOF
        }

        let in_buffer = self.position >= self.buffer_start
            && self.position < self.buffer_start + self.buffer.len() as u64;

        if !in_buffer {
            if let Err(e) = self.fill_buffer(self.position) {
                return Err(std::io::Error::other(e));
            }
            if self.buffer.is_empty() {
                return Ok(0);
            }
            if self.position < self.buffer_start {
                return Err(std::io::Error::other(format!(
                    "Server returned the wrong byte range for '{}'",
                    self.label
                )));
            }
        }

        let offset_in_buffer = (self.position - self.buffer_start) as usize;
        let available = self.buffer.len().saturating_sub(offset_in_buffer);
        if available == 0 {
            return Ok(0);
        }

        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&self.buffer[offset_in_buffer..offset_in_buffer + to_read]);
        self.position += to_read as u64;
        Ok(to_read)
    }
}

impl Seek for HttpRangeReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::Current(offset) => self.position as i64 + offset,
            SeekFrom::End(offset) => {
                if self.content_length == 0 {
                    self.try_discover_content_length();
                }
                if self.content_length > 0 {
                    self.content_length as i64 + offset
                } else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Cannot seek from end of stream with unknown length",
                    ));
                }
            }
        };

        if new_pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot seek to a negative position",
            ));
        }

        self.position = new_pos as u64;
        Ok(self.position)
    }
}

impl MediaSource for HttpRangeReader {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        if self.content_length > 0 {
            Some(self.content_length)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_basic_auth_strips_credentials_and_builds_header() {
        let (clean_url, header) =
            extract_basic_auth("http://test:test@127.0.0.1:8080/Music/song.mp3");
        assert_eq!(clean_url, "http://127.0.0.1:8080/Music/song.mp3");
        // base64("test:test") == "dGVzdDp0ZXN0"
        assert_eq!(header.as_deref(), Some("Basic dGVzdDp0ZXN0"));
    }

    #[test]
    fn extract_basic_auth_no_credentials_is_unchanged() {
        let (clean_url, header) = extract_basic_auth("http://127.0.0.1:8080/Music/song.mp3");
        assert_eq!(clean_url, "http://127.0.0.1:8080/Music/song.mp3");
        assert!(header.is_none());
    }

    #[tokio::test]
    async fn http_range_reader_reads_and_seeks_correctly() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        let test_data = b"0123456789ABCDEFabcdefghijklmnopqrstuvwxyz";

        // Mock HEAD request
        Mock::given(method("HEAD"))
            .and(path("/track.mp3"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-length", test_data.len().to_string())
                    .insert_header("accept-ranges", "bytes"),
            )
            .mount(&mock_server)
            .await;

        // Mock GET request with Range parsing
        Mock::given(method("GET"))
            .and(path("/track.mp3"))
            .respond_with(|req: &wiremock::Request| {
                if let Some(range) = req
                    .headers
                    .get(wiremock::http::HeaderName::from_static("range"))
                {
                    let range_str = range.to_str().unwrap();
                    if let Some(bytes_part) = range_str.strip_prefix("bytes=") {
                        let parts: Vec<&str> = bytes_part.split('-').collect();
                        let start: usize = parts[0].parse().unwrap_or(0);
                        let end: usize = parts
                            .get(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(test_data.len() - 1);
                        let end = end.min(test_data.len() - 1);
                        if start <= end && start < test_data.len() {
                            let slice = &test_data[start..=end];
                            return ResponseTemplate::new(206)
                                .insert_header(
                                    "content-range",
                                    format!("bytes {start}-{end}/{}", test_data.len()),
                                )
                                .insert_header("content-length", slice.len().to_string())
                                .set_body_bytes(slice.to_vec());
                        }
                    }
                }
                ResponseTemplate::new(200).set_body_bytes(test_data.to_vec())
            })
            .mount(&mock_server)
            .await;

        let url = format!("{}/track.mp3", mock_server.uri());
        tokio::task::spawn_blocking(move || {
            let mut reader = HttpRangeReader::new(&url).expect("reader failed to initialize");
            assert_eq!(reader.byte_len(), Some(test_data.len() as u64));
            assert!(reader.is_seekable());

            // Read first 10 bytes
            let mut buf = [0u8; 10];
            reader.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"0123456789");

            // Seek to 16
            let pos = reader.seek(SeekFrom::Start(16)).unwrap();
            assert_eq!(pos, 16);

            // Read next 5 bytes
            let mut buf2 = [0u8; 5];
            reader.read_exact(&mut buf2).unwrap();
            assert_eq!(&buf2, b"abcde");

            // Seek from current (+2)
            let pos = reader.seek(SeekFrom::Current(2)).unwrap();
            assert_eq!(pos, 23); // 16 + 5 + 2

            let mut buf3 = [0u8; 3];
            reader.read_exact(&mut buf3).unwrap();
            assert_eq!(&buf3, b"hij");
        })
        .await
        .unwrap();
    }

    const RANGE_TEST_DATA: &[u8] = b"0123456789ABCDEFabcdefghijklmnopqrstuvwxyz";

    /// Answers `Range` GETs with 206 + `Content-Range`, like a well-behaved server.
    fn ranged_response(req: &wiremock::Request) -> wiremock::ResponseTemplate {
        let data = RANGE_TEST_DATA;
        let (start, end) = req
            .headers
            .get(wiremock::http::HeaderName::from_static("range"))
            .and_then(|r| r.to_str().ok())
            .and_then(|r| r.strip_prefix("bytes="))
            .and_then(|r| r.split_once('-'))
            .map(|(s, e)| {
                let start: usize = s.parse().unwrap();
                let end: usize = e.parse().unwrap_or(data.len() - 1);
                (start, end.min(data.len() - 1))
            })
            .expect("test server expects a Range header");
        wiremock::ResponseTemplate::new(206)
            .insert_header(
                "content-range",
                format!("bytes {start}-{end}/{}", data.len()),
            )
            .set_body_bytes(data[start..=end].to_vec())
    }

    #[tokio::test]
    async fn http_range_reader_takes_length_from_content_range_when_head_fails() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(405))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ranged_response)
            .mount(&server)
            .await;

        let url = format!("{}/rest/stream.view?id=1", server.uri());
        tokio::task::spawn_blocking(move || {
            let mut reader = HttpRangeReader::new_with_label(&url, "subsonic://1/1").unwrap();
            assert_eq!(reader.byte_len(), Some(RANGE_TEST_DATA.len() as u64));
            reader.chunk_size = 8;
            reader.seek(SeekFrom::End(-4)).unwrap();
            let mut tail = Vec::new();
            reader.read_to_end(&mut tail).unwrap();
            assert_eq!(tail, b"wxyz");
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn http_range_reader_handles_a_server_that_ignores_range() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("HEAD"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-length", RANGE_TEST_DATA.len().to_string()),
            )
            .mount(&server)
            .await;
        // Always the whole file with a plain 200, whatever range was asked for.
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(RANGE_TEST_DATA.to_vec()))
            .mount(&server)
            .await;

        let url = format!("{}/track.mp3", server.uri());
        tokio::task::spawn_blocking(move || {
            let mut reader = HttpRangeReader::new(&url).unwrap();
            reader.chunk_size = 8;
            // Force a refill at a non-zero offset: drop the buffered prefix.
            reader.buffer.clear();
            reader.seek(SeekFrom::Start(16)).unwrap();
            let mut buf = [0u8; 5];
            reader.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"abcde");
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn http_range_reader_rejects_a_subsonic_error_sent_as_200() {
        use wiremock::matchers::any;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":70,"message":"data not found"}}}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let url = format!("{}/rest/stream.view?id=1&t=SECRETTOKEN", server.uri());
        let err = tokio::task::spawn_blocking(move || {
            HttpRangeReader::new_with_label(&url, "subsonic://1/1")
                .err()
                .expect("an error body must not be decoded as audio")
        })
        .await
        .unwrap();
        assert!(err.contains("Not found on the server"), "{err}");
        assert!(err.contains("subsonic://1/1"), "{err}");
        assert!(!err.contains("SECRETTOKEN"), "{err}");
    }

    #[test]
    fn http_range_reader_errors_never_quote_a_signed_url() {
        // Nothing listens on port 9 (discard) locally, so the request fails
        // at connect time — reqwest's own message would include the URL.
        let url = "http://127.0.0.1:9/rest/stream.view?t=SECRETTOKEN";
        let err = HttpRangeReader::new_with_label(url, "subsonic://1/1")
            .err()
            .expect("connection should fail");
        assert!(err.contains("subsonic://1/1"), "{err}");
        assert!(!err.contains("SECRETTOKEN"), "{err}");
    }

    #[test]
    /// The signer already names the track; wrapping its error again showed
    /// the `subsonic://` path twice in the skipped-track toast.
    fn subsonic_signing_errors_pass_through_unwrapped() {
        let err = open_with(
            "subsonic://4/abc",
            |path| Err(format!("The Subsonic server for '{path}' has been removed")),
            |_| None,
            |_| None,
        )
        .err()
        .expect("resolution failure must surface");
        assert_eq!(
            err,
            "The Subsonic server for 'subsonic://4/abc' has been removed"
        );
    }

    /// Answers `Range` GETs with 206 but NO `Content-Range` header (like Files.fm / SabreDAV).
    fn ranged_response_without_content_range(
        req: &wiremock::Request,
    ) -> wiremock::ResponseTemplate {
        let data = RANGE_TEST_DATA;
        let (start, end) = req
            .headers
            .get(wiremock::http::HeaderName::from_static("range"))
            .and_then(|r| r.to_str().ok())
            .and_then(|r| r.strip_prefix("bytes="))
            .and_then(|r| r.split_once('-'))
            .map(|(s, e)| {
                let start: usize = s.parse().unwrap();
                let end: usize = e.parse().unwrap_or(data.len() - 1);
                (start, end.min(data.len() - 1))
            })
            .expect("test server expects a Range header");
        let slice = &data[start..=end];
        wiremock::ResponseTemplate::new(206)
            .insert_header("content-length", slice.len().to_string())
            .set_body_bytes(slice.to_vec())
    }

    #[tokio::test]
    async fn http_range_reader_handles_206_omitting_content_range_with_known_length() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // SabreDAV/Files.fm may answer HEAD with 405 Method Not Allowed
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(405))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ranged_response_without_content_range)
            .mount(&server)
            .await;

        let url = format!("{}/track.mp3", server.uri());
        tokio::task::spawn_blocking(move || {
            let mut reader = HttpRangeReader::new_with_options(
                &url,
                None,
                None,
                Some(RANGE_TEST_DATA.len() as u64),
            )
            .unwrap();
            assert_eq!(reader.byte_len(), Some(RANGE_TEST_DATA.len() as u64));
            reader.chunk_size = 8;
            // Seeking from End must succeed using the injected known_length
            reader.seek(SeekFrom::End(-4)).unwrap();
            let mut tail = Vec::new();
            reader.read_to_end(&mut tail).unwrap();
            assert_eq!(tail, b"wxyz");
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn http_range_reader_falls_back_to_get_headers_when_length_unknown_and_head_fails() {
        use wiremock::matchers::{header_exists, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(405))
            .mount(&server)
            .await;
        // Ranged GET returns 206 omitting Content-Range
        Mock::given(method("GET"))
            .and(header_exists("range"))
            .respond_with(ranged_response_without_content_range)
            .mount(&server)
            .await;
        // Unranged GET returns 200 with Content-Length (header probe)
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-length", RANGE_TEST_DATA.len().to_string())
                    .set_body_bytes(RANGE_TEST_DATA.to_vec()),
            )
            .mount(&server)
            .await;

        let url = format!("{}/track.mp3", server.uri());
        tokio::task::spawn_blocking(move || {
            // No known_length passed upfront
            let mut reader = HttpRangeReader::new(&url).unwrap();
            reader.chunk_size = 8;
            // SeekFrom::End should trigger the lazy GET header check and succeed
            reader.seek(SeekFrom::End(-4)).unwrap();
            assert_eq!(reader.byte_len(), Some(RANGE_TEST_DATA.len() as u64));
            let mut tail = Vec::new();
            reader.read_to_end(&mut tail).unwrap();
            assert_eq!(tail, b"wxyz");
        })
        .await
        .unwrap();
    }

    #[test]
    fn open_with_queries_filesize_for_webdav_and_subsonic() {
        let mut looked_up_webdav = false;
        let _ = open_with(
            "http://127.0.0.1:9/track.mp3",
            |_| Ok("".into()),
            |_| None,
            |loc| {
                assert_eq!(loc, "http://127.0.0.1:9/track.mp3");
                looked_up_webdav = true;
                Some(5000)
            },
        );
        assert!(looked_up_webdav);

        let mut looked_up_subsonic = false;
        let _ = open_with(
            "subsonic://1/42",
            |_| Err("err".into()),
            |_| None,
            |loc| {
                assert_eq!(loc, "subsonic://1/42");
                looked_up_subsonic = true;
                Some(12345)
            },
        );
        assert!(looked_up_subsonic);
    }

    #[tokio::test]
    async fn http_range_reader_skips_head_when_known_length_provided() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // HEAD must NEVER be called when known_length is provided upfront
        Mock::given(method("HEAD"))
            .and(path("/track.mp3"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/track.mp3"))
            .respond_with(ranged_response)
            .expect(1..)
            .mount(&server)
            .await;

        let url = format!("{}/track.mp3", server.uri());
        tokio::task::spawn_blocking(move || {
            let mut reader = HttpRangeReader::new_with_options(
                &url,
                None,
                None,
                Some(RANGE_TEST_DATA.len() as u64),
            )
            .unwrap();
            assert_eq!(reader.byte_len(), Some(RANGE_TEST_DATA.len() as u64));
            let mut buf = [0u8; 10];
            reader.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, &RANGE_TEST_DATA[..10]);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn http_range_reader_issues_head_when_known_length_absent() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // HEAD MUST be called to discover Content-Length when known_length is absent
        Mock::given(method("HEAD"))
            .and(path("/track.mp3"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-length", RANGE_TEST_DATA.len().to_string()),
            )
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/track.mp3"))
            .respond_with(ranged_response)
            .expect(1..)
            .mount(&server)
            .await;

        let url = format!("{}/track.mp3", server.uri());
        tokio::task::spawn_blocking(move || {
            let mut reader = HttpRangeReader::new(&url).unwrap();
            assert_eq!(reader.byte_len(), Some(RANGE_TEST_DATA.len() as u64));
            let mut buf = [0u8; 10];
            reader.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, &RANGE_TEST_DATA[..10]);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn http_range_reader_reuses_connection_pool_across_instances() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/track1.mp3"))
            .respond_with(ranged_response)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/track2.mp3"))
            .respond_with(ranged_response)
            .mount(&server)
            .await;

        let url1 = format!("{}/track1.mp3", server.uri());
        let url2 = format!("{}/track2.mp3", server.uri());

        tokio::task::spawn_blocking(move || {
            // First track
            let mut reader1 = HttpRangeReader::new_with_options(
                &url1,
                None,
                None,
                Some(RANGE_TEST_DATA.len() as u64),
            )
            .unwrap();
            let mut buf1 = [0u8; 10];
            reader1.read_exact(&mut buf1).unwrap();
            assert_eq!(&buf1, &RANGE_TEST_DATA[..10]);

            // Second track from same server (sequential playback)
            let mut reader2 = HttpRangeReader::new_with_options(
                &url2,
                None,
                None,
                Some(RANGE_TEST_DATA.len() as u64),
            )
            .unwrap();
            let mut buf2 = [0u8; 10];
            reader2.read_exact(&mut buf2).unwrap();
            assert_eq!(&buf2, &RANGE_TEST_DATA[..10]);
        })
        .await
        .unwrap();
    }

    #[test]
    fn lookup_filesize_resolves_from_songs_and_falls_back_to_webdav_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        // Insert song with filesize in songs table
        conn.execute(
            "INSERT INTO songs (id, path, url, filesize) VALUES (1, 'https://example.com/song1.mp3', 'https://example.com/song1.mp3', 1048576)",
            [],
        )
        .unwrap();

        // Insert server and song with 0 filesize in songs table, but size in webdav_cache
        conn.execute(
            "INSERT INTO webdav_servers (id, name, url) VALUES (1, 'Test Server', 'https://example.com')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO songs (id, path, url, filesize) VALUES (2, 'https://example.com/song2.mp3', 'https://example.com/song2.mp3', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO webdav_cache (server_id, remote_path, size, song_id) VALUES (1, '/song2.mp3', 2097152, 2)",
            [],
        )
        .unwrap();

        // Song 1 gets size directly from songs.filesize
        assert_eq!(
            lookup_filesize(&db, "https://example.com/song1.mp3"),
            Some(1048576)
        );
        // Song 2 falls back to webdav_cache.size
        assert_eq!(
            lookup_filesize(&db, "https://example.com/song2.mp3"),
            Some(2097152)
        );
        // Also works when location has embedded basic auth
        assert_eq!(
            lookup_filesize(&db, "https://user:pass@example.com/song2.mp3"),
            Some(2097152)
        );
        // Unknown song returns None
        assert_eq!(
            lookup_filesize(&db, "https://example.com/unknown.mp3"),
            None
        );
    }
}
