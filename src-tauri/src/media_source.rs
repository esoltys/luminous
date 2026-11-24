//! Opens the media behind a stored song location for the audio engine and
//! offline analysis. `luminous_audio` only ever sees a `Box<dyn MediaSource>`:
//! everything about where the bytes come from — local files, WebDAV/HTTP
//! Range requests (#682), `subsonic://` paths signed into stream URLs at open
//! time (#1163), saved-server credentials (#1492) — lives here, so signed URLs,
//! credentials and the HTTP client never cross into the engine crate.

use crate::db::Database;
use luminous_audio::{IcyHeaders, MediaSource, OfflineDecoder, OpenedSource};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{mpsc, Arc, LazyLock};
use std::time::Duration;

/// URI scheme for saved/directory internet radio stations (#2002, #2004).
pub const RADIO_URI_SCHEME: &str = "radio://";

/// Global shared HTTP client for finite media sources (WebDAV, Subsonic).
/// Reuses connection pools, DNS caching, and TLS 1.3 session tickets across
/// sequential track playback, prefetching, and offline analysis.
static SHARED_CLIENT: LazyLock<reqwest::blocking::Client> = LazyLock::new(|| {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .tcp_keepalive(Duration::from_secs(60))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .expect("Failed to build shared HTTP client for media sources")
});

/// Dedicated HTTP client for unbounded live streams (#2002): connect timeout
/// with NO total-response timeout, and redirect following enabled.
static LIVE_CLIENT: LazyLock<reqwest::blocking::Client> = LazyLock::new(|| {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .tcp_keepalive(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("Failed to build live stream HTTP client")
});

/// The opener handed to `AudioEngine::with_source_opener`: the same lookup
/// as [`open_for_engine`], owning its database handle.
pub fn opener(
    db: Arc<Database>,
) -> impl Fn(&str) -> Result<OpenedSource, String> + Send + Sync + 'static {
    move |location| open_for_engine(&db, location)
}

/// Open a stored song location for the playback engine, returning the reader
/// together with any transport-reported MIME type and ICY metadata channel.
pub fn open_for_engine(db: &Database, location: &str) -> Result<OpenedSource, String> {
    open_with_options(
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
        |loc| resolve_live_stream_url(db, loc),
    )
}

/// Open a stored song location: a local path, an `http(s)://` URL (WebDAV
/// or remote stream), a `subsonic://` path, or a `radio://` URI. Errors name
/// the location the user knows, never a signed URL.
pub fn open(db: &Database, location: &str) -> Result<Box<dyn MediaSource>, String> {
    open_for_engine(db, location).map(|opened| opened.reader)
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

/// If `location` is a `radio://` URI or a live-stream song row (a source
/// in `LIVE_STREAM_SOURCES` via `LIVE_STREAM_SOURCES_SQL`, or `FileType::Stream`),
/// returns the resolved HTTP(S) stream URL to open via [`LiveHttpSource`].
/// Returns `Ok(None)` when `location` is not a live stream.
fn resolve_live_stream_url(db: &Database, location: &str) -> Result<Option<String>, String> {
    let Ok(conn) = db.pool.get() else {
        if location.starts_with(RADIO_URI_SCHEME) {
            return Err(format!("Cannot resolve '{location}': database unavailable"));
        }
        return Ok(None);
    };

    if location.starts_with(RADIO_URI_SCHEME) {
        let directory_url = crate::radio_directory::parse_station_uuid(location)
            .and_then(|uuid| crate::radio_directory::resolve_station_stream_url(&conn, uuid).ok())
            .flatten();
        if let Some(url) = directory_url {
            return Ok(Some(url));
        }

        let fallback: Option<String> = conn
            .query_row(
                "SELECT COALESCE(NULLIF(stream_url, ''), NULLIF(url, '')) \
                 FROM songs WHERE path = ?1 LIMIT 1",
                rusqlite::params![location],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        return match fallback {
            Some(url) => Ok(Some(url)),
            None => Err(format!(
                "Station offline: no stream URL found for '{location}'"
            )),
        };
    }

    if location.starts_with("http://") || location.starts_with("https://") {
        let sql = format!(
            "SELECT EXISTS( \
                SELECT 1 FROM songs \
                WHERE (path = ?1 OR stream_url = ?1 OR url = ?1) \
                  AND (source IN ({}) OR filetype = ?2) \
            )",
            *crate::models::LIVE_STREAM_SOURCES_SQL,
        );
        let is_live: bool = conn
            .query_row(
                &sql,
                rusqlite::params![location, crate::models::FileType::Stream as i32],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if is_live {
            return Ok(Some(location.to_string()));
        }
    }

    Ok(None)
}

/// [`open`] with the database lookups passed in, so the routing and
/// error wording are testable without a database.
#[cfg(test)]
fn open_with(
    location: &str,
    sign_subsonic: impl FnOnce(&str) -> Result<String, String>,
    webdav_auth: impl FnOnce(&str) -> Option<String>,
    lookup_filesize: impl FnOnce(&str) -> Option<u64>,
) -> Result<Box<dyn MediaSource>, String> {
    open_with_options(
        location,
        sign_subsonic,
        webdav_auth,
        lookup_filesize,
        |_| Ok(None),
    )
    .map(|opened| opened.reader)
}

fn open_with_options(
    location: &str,
    sign_subsonic: impl FnOnce(&str) -> Result<String, String>,
    webdav_auth: impl FnOnce(&str) -> Option<String>,
    lookup_filesize: impl FnOnce(&str) -> Option<u64>,
    resolve_live: impl FnOnce(&str) -> Result<Option<String>, String>,
) -> Result<OpenedSource, String> {
    if let Some(live_url) = resolve_live(location)? {
        let label = if location.starts_with(RADIO_URI_SCHEME) {
            Some(location)
        } else {
            None
        };
        return LiveHttpSource::open_with_label(&live_url, label);
    }

    if location.starts_with(crate::subsonic::URI_SCHEME) {
        // Signed fresh on every open; the URL itself never leaves this
        // function. The signer's errors already name the track by its
        // `subsonic://` path, and the UI prefixes the song title.
        let known_length = lookup_filesize(location);
        let url = sign_subsonic(location)?;
        Ok(OpenedSource::new(Box::new(
            HttpRangeReader::new_with_options(&url, Some(location), None, known_length)?,
        )))
    } else if location.starts_with("http://") || location.starts_with("https://") {
        let known_length = lookup_filesize(location);
        let (url, embedded) = extract_basic_auth(location);
        let auth = embedded.or_else(|| webdav_auth(&url));
        Ok(OpenedSource::new(Box::new(
            HttpRangeReader::new_with_options(&url, None, auth, known_length)?,
        )))
    } else {
        let file = std::fs::File::open(location)
            .map_err(|e| format!("Cannot open file '{location}': {e}"))?;
        Ok(OpenedSource::new(Box::new(file)))
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

struct FetchedChunk {
    data: Vec<u8>,
    buffer_start: u64,
    content_length: Option<u64>,
}

struct PrefetchWorker {
    expected_start: u64,
    rx: std::sync::mpsc::Receiver<Result<FetchedChunk, String>>,
}

impl PrefetchWorker {
    fn contains(&self, pos: u64, chunk_size: usize) -> bool {
        pos >= self.expected_start && pos < self.expected_start + chunk_size as u64
    }
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
    prefetch: Mutex<Option<PrefetchWorker>>,
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
            chunk_size: 1024 * 1024, // 1 MB buffer chunk for streaming playback
            prefetch: Mutex::new(None),
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
        *self.prefetch.lock() = None;
        let chunk = fetch_range(
            &self.client,
            &self.url,
            self.auth_header.as_deref(),
            &self.label,
            start,
            self.chunk_size,
            self.content_length,
        )?;
        self.apply_chunk(chunk);
        Ok(())
    }

    fn apply_chunk(&mut self, chunk: FetchedChunk) {
        if let Some(total) = chunk.content_length {
            if self.content_length == 0 {
                self.content_length = total;
            }
        }
        self.buffer_start = chunk.buffer_start;
        self.buffer = chunk.data;
    }

    fn in_buffer(&self, pos: u64) -> bool {
        pos >= self.buffer_start && pos < self.buffer_start + self.buffer.len() as u64
    }

    fn fill_buffer_or_take_prefetch(&mut self, pos: u64) -> Result<(), String> {
        let prefetched = {
            let mut guard = self.prefetch.lock();
            if let Some(worker) = guard.as_ref() {
                if worker.contains(pos, self.chunk_size) {
                    guard.take()
                } else {
                    *guard = None;
                    None
                }
            } else {
                None
            }
        };

        if let Some(worker) = prefetched {
            match worker.rx.recv() {
                Ok(res) => {
                    let chunk = res?;
                    self.apply_chunk(chunk);
                    return Ok(());
                }
                Err(_) => {
                    // Prefetch worker disconnected prematurely; fall back to synchronous fetch.
                }
            }
        }

        self.fill_buffer(pos)
    }

    fn trigger_prefetch_if_needed(&self) {
        if self.buffer.is_empty() {
            return;
        }
        let halfway = self.buffer_start + (self.buffer.len() as u64 / 2);
        if self.position < halfway {
            return;
        }
        let next_start = self.buffer_start + self.buffer.len() as u64;
        if self.content_length > 0 && next_start >= self.content_length {
            return;
        }

        let mut guard = self.prefetch.lock();
        if guard.is_some() {
            return;
        }

        let client = self.client.clone();
        let url = self.url.clone();
        let auth_header = self.auth_header.clone();
        let label = self.label.clone();
        let chunk_size = self.chunk_size;
        let content_length = self.content_length;

        let (tx, rx) = std::sync::mpsc::channel();
        let spawn_res = std::thread::Builder::new()
            .name("luminous-media-prefetch".to_string())
            .spawn(move || {
                let res = fetch_range(
                    &client,
                    &url,
                    auth_header.as_deref(),
                    &label,
                    next_start,
                    chunk_size,
                    content_length,
                );
                let _ = tx.send(res);
            });

        if spawn_res.is_ok() {
            *guard = Some(PrefetchWorker {
                expected_start: next_start,
                rx,
            });
        }
    }
}

fn fetch_range(
    client: &reqwest::blocking::Client,
    url: &str,
    auth_header: Option<&str>,
    label: &str,
    start: u64,
    chunk_size: usize,
    content_length: u64,
) -> Result<FetchedChunk, String> {
    if content_length > 0 && start >= content_length {
        return Ok(FetchedChunk {
            data: Vec::new(),
            buffer_start: start,
            content_length: Some(content_length),
        });
    }

    let end = if content_length > 0 {
        (start + chunk_size as u64 - 1).min(content_length - 1)
    } else {
        start + chunk_size as u64 - 1
    };

    let range_header = format!("bytes={start}-{end}");
    let mut req = client.get(url).header(reqwest::header::RANGE, range_header);
    if let Some(h) = auth_header {
        req = req.header(reqwest::header::AUTHORIZATION, h);
    }
    // `without_url`: reqwest errors otherwise quote the (signed) URL.
    let mut resp = req
        .send()
        .map_err(|e| format!("Range request failed for '{}': {}", label, e.without_url()))?;

    if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let total = parse_content_range_total(resp.headers());
        return Ok(FetchedChunk {
            data: Vec::new(),
            buffer_start: start,
            content_length: total,
        });
    }

    if !resp.status().is_success() {
        return Err(format!(
            "HTTP error {} when accessing '{}'",
            resp.status(),
            label
        ));
    }

    if is_error_content_type(resp.headers()) {
        let body = resp.text().unwrap_or_default();
        let reason = crate::subsonic::describe_error_body(&body)
            .unwrap_or_else(|| "the server sent an error page instead of audio".into());
        return Err(format!("Can't play '{}': {reason}", label));
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

    let (buffer_start, discovered_total) = if partial {
        let mut total = None;
        if let Some((_, Some(t))) = content_range {
            total = Some(t);
        } else if content_length == 0 {
            // If Content-Range is omitted (e.g. Files.fm / SabreDAV), and this response
            // delivered fewer bytes than requested, we have reached the end of the stream.
            let expected_chunk = end.saturating_sub(start) + 1;
            if (data.len() as u64) < expected_chunk {
                total = Some(start + data.len() as u64);
            }
        }
        let start_pos = content_range.map_or(start, |(first, _)| first);
        (start_pos, total)
    } else {
        // A plain 200 ignored the Range header and sent the whole file
        // from byte 0, whatever offset was asked for.
        let total = if content_length == 0 {
            Some(data.len() as u64)
        } else {
            None
        };
        (0, total)
    };

    Ok(FetchedChunk {
        data,
        buffer_start,
        content_length: discovered_total,
    })
}

impl Read for HttpRangeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.content_length > 0 && self.position >= self.content_length {
            return Ok(0); // EOF
        }

        if !self.in_buffer(self.position) {
            self.fill_buffer_or_take_prefetch(self.position)
                .map_err(std::io::Error::other)?;

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
        self.trigger_prefetch_if_needed();
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

        let in_prefetch_range = {
            let guard = self.prefetch.lock();
            guard
                .as_ref()
                .is_some_and(|w| w.contains(self.position, self.chunk_size))
        };
        if !self.in_buffer(self.position) && !in_prefetch_range {
            *self.prefetch.lock() = None;
        }

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

// ---------------------------------------------------------------------------
// LiveHttpSource — non-seekable Icecast / Shoutcast streaming media source (#2002)
// ---------------------------------------------------------------------------

/// Default pre-roll ring capacity allocated at open time (64 KiB) and initial
/// pre-roll target (16 KiB) so format probing and the first decode frames never
/// underrun on connection startup.
const LIVE_RING_CAPACITY: usize = 64 * 1024;
const LIVE_PREROLL_BYTES: usize = 16 * 1024;

/// Default exponential backoff schedule for mid-stream reconnects.
const DEFAULT_RECONNECT_BACKOFFS: &[Duration] = &[
    Duration::from_millis(200),
    Duration::from_millis(400),
    Duration::from_millis(800),
    Duration::from_millis(1600),
    Duration::from_millis(3200),
];

/// Parses an ICY metadata block (padded with trailing NUL bytes) and extracts
/// the `StreamTitle='...';` value if present and non-empty (#2002).
///
/// Handles embedded single quotes inside titles by matching up to the
/// `';` delimiter (or a trailing `'` at the end of the block).
pub fn parse_icy_stream_title(block: &[u8]) -> Option<String> {
    let last_non_zero = block.iter().rposition(|&b| b != 0)?;
    let trimmed_bytes = &block[..=last_non_zero];
    let text = String::from_utf8_lossy(trimmed_bytes);
    let prefix = "StreamTitle='";
    let start = text.find(prefix)? + prefix.len();
    let rest = &text[start..];
    let end = rest
        .find("';")
        .or_else(|| rest.rfind('\''))
        .unwrap_or(rest.len());
    let title = rest[..end].trim();
    (!title.is_empty()).then(|| title.to_string())
}

struct ConnectedLiveStream {
    reader: Box<dyn Read + Send + Sync>,
    /// The URL that answered, after redirects: HLS resolves segment URIs against it.
    final_url: String,
    mime_type: Option<String>,
    metaint: Option<usize>,
    icy_headers: IcyHeaders,
    used_shoutcast_v1: bool,
}

/// Non-seekable streaming HTTP media source for live Icecast and Shoutcast stations (#2002).
///
/// - Sends `Icy-MetaData: 1`, follows redirects, and tolerates Shoutcast v1's
///   non-standard `ICY 200 OK` status line.
/// - Strips interleaved `icy-metaint` metadata blocks out of the byte stream so
///   audio decoders never see corrupted frames, parses `StreamTitle`, and
///   publishes deduplicated updates on a channel.
/// - Pre-allocates a bounded ring buffer at open time with a small pre-roll.
/// - Reconnects with bounded exponential backoff on mid-stream drops for
///   frame-based streams (MP3, ADTS AAC) and returns a non-EOF I/O error once
///   retries are exhausted so the audio engine emits `AudioEvent::Error`
///   instead of advancing the queue as if a track finished.
pub struct LiveHttpSource {
    url: String,
    label: String,
    stream: Option<Box<dyn Read + Send + Sync>>,
    metaint: Option<usize>,
    bytes_until_meta: usize,
    meta_scratch: Vec<u8>,
    read_scratch: Vec<u8>,
    ring: VecDeque<u8>,
    ring_capacity: usize,
    metadata_tx: Mutex<mpsc::Sender<String>>,
    last_title: Option<String>,
    can_reconnect_in_place: bool,
    used_shoutcast_v1: bool,
    reconnect_backoffs: Vec<Duration>,
}

/// True when a live stream's codec/container is frame-resynchronizable across
/// a fresh HTTP reconnect without re-probing container headers (MP3, ADTS AAC).
/// Containerized streams like Ogg Vorbis/Opus or FLAC require a full track reopen.
fn is_frame_resyncable_mime(mime: Option<&str>, url: &str) -> bool {
    if let Some(normalized) = mime.and_then(luminous_audio::normalize_probe_mime) {
        if matches!(normalized.as_str(), "audio/mpeg" | "audio/aac") {
            return true;
        }
        if normalized.contains("ogg")
            || normalized.contains("flac")
            || normalized.contains("wav")
            || normalized.contains("aiff")
        {
            return false;
        }
    }
    matches!(
        luminous_audio::extension_from_location(url)
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("mp3" | "aac")
    )
}

impl LiveHttpSource {
    /// Opens a live stream at `url` and returns the bundled [`OpenedSource`]
    /// (reader, discovered `Content-Type`, `IcyHeaders`, and `StreamTitle` receiver).
    pub fn open(url: &str) -> Result<OpenedSource, String> {
        Self::open_with_label(url, None)
    }

    /// Opens a live stream at `url`, using `label` in error messages if provided.
    pub fn open_with_label(url: &str, label: Option<&str>) -> Result<OpenedSource, String> {
        Self::open_with_backoffs(url, label, DEFAULT_RECONNECT_BACKOFFS.to_vec())
    }

    pub fn open_with_backoffs(
        url: &str,
        label: Option<&str>,
        reconnect_backoffs: Vec<Duration>,
    ) -> Result<OpenedSource, String> {
        let label_str = label.unwrap_or(url).to_string();
        let mut connected = connect_live_stream(url, &label_str)?;
        if let Some(playlist) = take_hls_playlist(&mut connected)? {
            return crate::hls::HlsLiveSource::open(&connected.final_url, &playlist, &label_str);
        }
        let (metadata_tx, metadata_rx) = mpsc::channel();
        let can_reconnect_in_place = is_frame_resyncable_mime(connected.mime_type.as_deref(), url);
        let mime_type = connected.mime_type;
        let icy_headers = connected.icy_headers;

        let mut source = Self {
            url: url.to_string(),
            label: label_str,
            stream: Some(connected.reader),
            metaint: connected.metaint,
            bytes_until_meta: connected.metaint.unwrap_or(0),
            meta_scratch: Vec::with_capacity(4080),
            read_scratch: vec![0u8; 8192],
            ring: VecDeque::with_capacity(LIVE_RING_CAPACITY),
            ring_capacity: LIVE_RING_CAPACITY,
            metadata_tx: Mutex::new(metadata_tx),
            last_title: None,
            can_reconnect_in_place,
            used_shoutcast_v1: connected.used_shoutcast_v1,
            reconnect_backoffs,
        };

        // Pre-roll initial audio bytes at open time so format probing has an
        // immediate buffer ready.
        source
            .preroll(LIVE_PREROLL_BYTES)
            .map_err(|e| format!("Failed to read live stream '{}': {e}", source.label))?;

        Ok(OpenedSource {
            reader: Box::new(source),
            mime_type,
            icy_headers: Some(icy_headers),
            metadata_rx: Some(metadata_rx),
        })
    }

    fn preroll(&mut self, target_bytes: usize) -> std::io::Result<()> {
        let goal = target_bytes.min(self.ring_capacity);
        while self.ring.len() < goal {
            let n = self.pump_once()?;
            if n == 0 {
                break;
            }
        }
        Ok(())
    }

    /// Reads one chunk of audio bytes from the active connection (stripping any
    /// interleaved ICY metadata block when `bytes_until_meta == 0`) and pushes
    /// it into `self.ring`. Transparently reconnects on drop if supported.
    fn pump_once(&mut self) -> std::io::Result<usize> {
        loop {
            if self.stream.is_none() {
                self.reconnect()?;
            }

            if let Some(metaint) = self.metaint {
                if self.bytes_until_meta == 0 {
                    match self.read_and_strip_icy_block() {
                        Ok(()) => {
                            self.bytes_until_meta = metaint;
                        }
                        Err(e) => {
                            log::warn!(
                                "ICY metadata read failed on '{}': {e}; attempting reconnect",
                                self.label
                            );
                            self.stream = None;
                            self.reconnect()?;
                            continue;
                        }
                    }
                }
            }

            let available_ring = self.ring_capacity.saturating_sub(self.ring.len());
            if available_ring == 0 {
                return Ok(0);
            }

            let max_to_read = match self.metaint {
                Some(_) => self
                    .bytes_until_meta
                    .min(available_ring)
                    .min(self.read_scratch.len()),
                None => available_ring.min(self.read_scratch.len()),
            };

            let read_res = self
                .stream
                .as_mut()
                .expect("stream checked above")
                .read(&mut self.read_scratch[..max_to_read]);

            match read_res {
                Ok(0) => {
                    log::warn!(
                        "Live stream '{}' dropped (EOF); attempting reconnect",
                        self.label
                    );
                    self.stream = None;
                    self.reconnect()?;
                }
                Ok(n) => {
                    self.ring.extend(&self.read_scratch[..n]);
                    if self.metaint.is_some() {
                        self.bytes_until_meta -= n;
                    }
                    return Ok(n);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    log::warn!(
                        "Live stream '{}' read error ({e}); attempting reconnect",
                        self.label
                    );
                    self.stream = None;
                    self.reconnect()?;
                }
            }
        }
    }

    fn read_and_strip_icy_block(&mut self) -> std::io::Result<()> {
        let stream = self.stream.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotConnected, "stream closed")
        })?;
        let mut len_byte = [0u8; 1];
        stream.read_exact(&mut len_byte)?;
        let meta_len = (len_byte[0] as usize) * 16;
        if meta_len == 0 {
            return Ok(());
        }
        self.meta_scratch.resize(meta_len, 0);
        stream.read_exact(&mut self.meta_scratch)?;
        if let Some(title) = parse_icy_stream_title(&self.meta_scratch) {
            if self.last_title.as_deref() != Some(title.as_str()) {
                self.last_title = Some(title.clone());
                let _ = self.metadata_tx.lock().send(title);
            }
        }
        Ok(())
    }

    fn reconnect(&mut self) -> std::io::Result<()> {
        if !self.can_reconnect_in_place || self.reconnect_backoffs.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                format!("Live stream '{}' disconnected", self.label),
            ));
        }

        let mut last_err = String::from("connection lost");
        for delay in &self.reconnect_backoffs {
            if !delay.is_zero() {
                std::thread::sleep(*delay);
            }
            let connect_res = if self.used_shoutcast_v1 {
                let (clean_url, auth_header) = extract_basic_auth(&self.url);
                connect_shoutcast_v1_fallback(&clean_url, auth_header.as_deref(), &self.label)
            } else {
                connect_live_stream(&self.url, &self.label)
            };
            match connect_res {
                Ok(connected) => {
                    self.stream = Some(connected.reader);
                    self.metaint = connected.metaint;
                    self.bytes_until_meta = connected.metaint.unwrap_or(0);
                    self.used_shoutcast_v1 = connected.used_shoutcast_v1;
                    return Ok(());
                }
                Err(e) => {
                    last_err = e;
                }
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            format!(
                "Live stream '{}' disconnected after {} reconnect attempts: {last_err}",
                self.label,
                self.reconnect_backoffs.len()
            ),
        ))
    }
}

impl Read for LiveHttpSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.ring.is_empty() {
            self.pump_once()?;
        }
        let to_copy = buf.len().min(self.ring.len());
        let (first, second) = self.ring.as_slices();
        if to_copy <= first.len() {
            buf[..to_copy].copy_from_slice(&first[..to_copy]);
        } else {
            let first_len = first.len();
            buf[..first_len].copy_from_slice(first);
            buf[first_len..to_copy].copy_from_slice(&second[..to_copy - first_len]);
        }
        self.ring.drain(..to_copy);
        Ok(to_copy)
    }
}

impl Seek for LiveHttpSource {
    fn seek(&mut self, _pos: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Cannot seek a live stream",
        ))
    }
}

impl MediaSource for LiveHttpSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// If the live URL answered with an HLS playlist rather than audio, reads and
/// returns the playlist text (#2012). Detected by content, not only by a
/// `.m3u8` extension, which many stations' URLs don't have: a playlist MIME
/// type, the extension, or any non-audio type is enough to peek at the body
/// for `#EXTM3U`. Peeked bytes go back in front of the stream otherwise, so a
/// mislabelled audio stream still plays.
fn take_hls_playlist(connected: &mut ConnectedLiveStream) -> Result<Option<String>, String> {
    let mime = connected.mime_type.as_deref();
    let declared = mime.is_some_and(crate::hls::is_playlist_mime)
        || crate::hls::has_m3u8_extension(&connected.final_url);
    let audio_typed = mime.is_some_and(|m| {
        let m = m.to_ascii_lowercase();
        m.starts_with("audio/") || m.starts_with("application/ogg") || m.starts_with("video/")
    });
    if !declared && audio_typed {
        return Ok(None);
    }

    let mut reader = std::mem::replace(&mut connected.reader, Box::new(std::io::empty()));
    let mut prefix = Vec::with_capacity(16);
    (&mut reader)
        .take(16)
        .read_to_end(&mut prefix)
        .map_err(|e| e.to_string())?;
    let head = prefix.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&prefix);
    if !head.starts_with(b"#EXTM3U") {
        connected.reader = Box::new(std::io::Cursor::new(prefix).chain(reader));
        return Ok(None);
    }

    let rest = crate::hls::read_playlist_body(reader, 2 * 1024 * 1024)?;
    let text = format!("{}{rest}", String::from_utf8_lossy(&prefix));
    if crate::hls::playlist::looks_like_hls(&text) {
        Ok(Some(text))
    } else {
        Err("The station's URL is a playlist file, not an audio stream".to_string())
    }
}

fn parse_icy_header_values<'a>(
    mut get_str: impl FnMut(&str) -> Option<&'a str>,
) -> (Option<usize>, IcyHeaders) {
    let non_empty = |s: &'a str| {
        let trimmed = s.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    };
    let metaint = get_str("icy-metaint")
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|&n| n > 0);
    let name = get_str("icy-name").and_then(non_empty);
    let genre = get_str("icy-genre").and_then(non_empty);
    let bitrate_kbps = get_str("icy-br")
        .and_then(|s| {
            s.split(',')
                .next()
                .and_then(|b| b.trim().parse::<u32>().ok())
        })
        .filter(|&b| b > 0);
    (
        metaint,
        IcyHeaders {
            name,
            genre,
            bitrate_kbps,
        },
    )
}

fn parse_icy_headers_from_map(headers: &reqwest::header::HeaderMap) -> (Option<usize>, IcyHeaders) {
    parse_icy_header_values(|name| headers.get(name).and_then(|v| v.to_str().ok()))
}

fn connect_live_stream(url: &str, label: &str) -> Result<ConnectedLiveStream, String> {
    let (clean_url, auth_header) = extract_basic_auth(url);
    let mut req = LIVE_CLIENT.get(&clean_url).header("Icy-MetaData", "1");
    if let Some(ref auth) = auth_header {
        req = req.header(reqwest::header::AUTHORIZATION, auth);
    }

    match req.send() {
        Ok(resp) => {
            if !resp.status().is_success() {
                return Err(format!(
                    "HTTP error {} when connecting to live stream '{}'",
                    resp.status(),
                    label
                ));
            }
            if is_error_content_type(resp.headers()) {
                return Err(format!(
                    "Can't play '{}': the server sent an error page instead of audio",
                    label
                ));
            }
            let mime_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let (metaint, icy_headers) = parse_icy_headers_from_map(resp.headers());
            let final_url = resp.url().to_string();
            Ok(ConnectedLiveStream {
                reader: Box::new(resp),
                final_url,
                mime_type,
                metaint,
                icy_headers,
                used_shoutcast_v1: false,
            })
        }
        Err(err) => {
            // Shoutcast v1 servers respond with `ICY 200 OK\r\n` instead of
            // `HTTP/1.0 200 OK\r\n`, which hyper/reqwest rejects as an invalid
            // HTTP version. Fall back to a raw HTTP/1.0 TCP stream for plain
            // `http://` URLs when reqwest fails to parse the response.
            if clean_url.starts_with("http://") {
                if let Ok(shoutcast) =
                    connect_shoutcast_v1_fallback(&clean_url, auth_header.as_deref(), label)
                {
                    return Ok(shoutcast);
                }
            }
            Err(format!(
                "Failed to connect to live stream '{}': {}",
                label,
                err.without_url()
            ))
        }
    }
}

/// Raw TCP fallback for Shoutcast v1 servers that emit `ICY 200 OK\r\n` instead
/// of a valid `HTTP/1.x` status line (#2002).
fn connect_shoutcast_v1_fallback(
    url: &str,
    auth_header: Option<&str>,
    label: &str,
) -> Result<ConnectedLiveStream, String> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpStream, ToSocketAddrs};

    let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "http" {
        return Err("Shoutcast v1 fallback only applies to http://".to_string());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "Missing host in URL".to_string())?;
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or_else(|| "Could not resolve host".to_string())?;

    let stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

    let mut path_and_query = parsed.path().to_string();
    if path_and_query.is_empty() {
        path_and_query.push('/');
    }
    if let Some(q) = parsed.query() {
        path_and_query.push('?');
        path_and_query.push_str(q);
    }

    let host_header = if port == 80 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    };
    let mut request = format!(
        "GET {path_and_query} HTTP/1.0\r\nHost: {host_header}\r\nIcy-MetaData: 1\r\nConnection: close\r\n"
    );
    if let Some(auth) = auth_header {
        request.push_str(&format!("Authorization: {auth}\r\n"));
    }
    request.push_str("\r\n");

    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    writer
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|e| e.to_string())?;
    let status_trimmed = status_line.trim();
    let is_ok = status_trimmed.starts_with("ICY 200")
        || status_trimmed.starts_with("HTTP/1.0 200")
        || status_trimmed.starts_with("HTTP/1.1 200");
    if !is_ok {
        return Err(format!("Non-200 status '{status_trimmed}' from '{label}'"));
    }

    let mut raw_headers: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            if !val.is_empty() {
                raw_headers.insert(key, val.to_string());
            }
        }
    }

    let mime_type = raw_headers.get("content-type").cloned();
    let (metaint, icy_headers) =
        parse_icy_header_values(|name| raw_headers.get(name).map(String::as_str));

    Ok(ConnectedLiveStream {
        reader: Box::new(reader),
        final_url: url.to_string(),
        mime_type,
        metaint,
        icy_headers,
        used_shoutcast_v1: true,
    })
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

    #[tokio::test]
    async fn http_range_reader_prefetches_next_chunk_when_crossing_halfway_mark() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ranged_response)
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
            // Test data is 43 bytes long. Set chunk size to 16 and re-prime buffer.
            reader.chunk_size = 16;
            reader.fill_buffer(0).unwrap();
            reader.seek(SeekFrom::Start(0)).unwrap();
            // Initially, chunk 0..16 is loaded and position is 0. Prefetch has not triggered.
            assert!(reader.prefetch.lock().is_none());

            // Read 7 bytes (position becomes 7, which is < 8 halfway mark of 16-byte chunk)
            let mut buf = [0u8; 7];
            reader.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, &RANGE_TEST_DATA[..7]);
            assert!(reader.prefetch.lock().is_none());

            // Read 2 bytes (position becomes 9, crossing the halfway mark of 8)
            let mut buf2 = [0u8; 2];
            reader.read_exact(&mut buf2).unwrap();
            assert_eq!(&buf2, &RANGE_TEST_DATA[7..9]);

            // Prefetch must now be active for expected_start = 16
            let is_prefetched = {
                let guard = reader.prefetch.lock();
                guard.as_ref().is_some_and(|w| w.expected_start == 16)
            };
            assert!(
                is_prefetched,
                "prefetch should be triggered at halfway mark"
            );

            // Read the remaining 7 bytes of the first chunk (up to position 16)
            let mut buf3 = [0u8; 7];
            reader.read_exact(&mut buf3).unwrap();
            assert_eq!(&buf3, &RANGE_TEST_DATA[9..16]);

            // Next read crosses the chunk boundary seamlessly without blocking
            let mut buf4 = [0u8; 5];
            reader.read_exact(&mut buf4).unwrap();
            assert_eq!(&buf4, &RANGE_TEST_DATA[16..21]);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn http_range_reader_seek_invalidates_prefetch_on_distant_seek() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ranged_response)
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
            reader.chunk_size = 16;
            reader.fill_buffer(0).unwrap();
            reader.seek(SeekFrom::Start(0)).unwrap();

            // Advance past halfway mark (9 bytes) to trigger prefetch for bytes 16..32
            let mut buf = [0u8; 9];
            reader.read_exact(&mut buf).unwrap();
            assert!(reader.prefetch.lock().is_some());

            // Seek to a distant offset (35), outside both current buffer (0..16) and prefetch (16..32)
            reader.seek(SeekFrom::Start(35)).unwrap();
            assert!(
                reader.prefetch.lock().is_none(),
                "distant seek must invalidate obsolete in-flight prefetch"
            );

            // Reading from 35 succeeds with newly fetched range
            let mut buf2 = [0u8; 5];
            reader.read_exact(&mut buf2).unwrap();
            assert_eq!(&buf2, &RANGE_TEST_DATA[35..40]);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn http_range_reader_seek_preserves_prefetch_when_seeking_into_prefetch_range() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ranged_response)
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
            reader.chunk_size = 16;
            reader.fill_buffer(0).unwrap();
            reader.seek(SeekFrom::Start(0)).unwrap();

            // Trigger prefetch for chunk starting at 16
            let mut buf = [0u8; 9];
            reader.read_exact(&mut buf).unwrap();
            assert!(reader.prefetch.lock().is_some());

            // Seek directly into the prefetched chunk (offset 20)
            reader.seek(SeekFrom::Start(20)).unwrap();
            assert!(
                reader.prefetch.lock().is_some(),
                "prefetch should be preserved when seeking into its range"
            );

            // Read 5 bytes at offset 20
            let mut buf2 = [0u8; 5];
            reader.read_exact(&mut buf2).unwrap();
            assert_eq!(&buf2, &RANGE_TEST_DATA[20..25]);
        })
        .await
        .unwrap();
    }

    /// Encodes a metadata string (`StreamTitle='...';`) as an ICY metadata block:
    /// 1 length byte (`len / 16`) followed by NUL-padded payload of `len_byte * 16` bytes.
    fn encode_icy_block(text: &str) -> Vec<u8> {
        if text.is_empty() {
            return vec![0u8];
        }
        let bytes = text.as_bytes();
        let blocks = bytes.len().div_ceil(16);
        assert!(blocks <= 255, "ICY metadata block too long");
        let mut out = vec![0u8; 1 + blocks * 16];
        out[0] = blocks as u8;
        out[1..1 + bytes.len()].copy_from_slice(bytes);
        out
    }

    /// Interleaves raw `audio` bytes with ICY metadata blocks every `metaint` bytes.
    fn build_icy_interleaved_stream(audio: &[u8], metaint: usize, titles: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut offset = 0;
        let mut block_idx = 0;
        while offset < audio.len() {
            let remaining = audio.len() - offset;
            if remaining >= metaint {
                out.extend_from_slice(&audio[offset..offset + metaint]);
                offset += metaint;
                let meta_str = titles
                    .get(block_idx)
                    .copied()
                    .filter(|s| !s.is_empty())
                    .map(|t| format!("StreamTitle='{t}';StreamUrl='';"))
                    .unwrap_or_default();
                out.extend_from_slice(&encode_icy_block(&meta_str));
                block_idx += 1;
            } else {
                out.extend_from_slice(&audio[offset..]);
                break;
            }
        }
        out
    }

    #[test]
    fn icy_stream_title_parser_handles_padding_quotes_and_empty_blocks() {
        let block = encode_icy_block("StreamTitle='Boards of Canada - Roygbiv';StreamUrl='';");
        assert_eq!(
            parse_icy_stream_title(&block[1..]).as_deref(),
            Some("Boards of Canada - Roygbiv")
        );

        // Single quote inside the track title before the `';` delimiter
        let with_quote =
            encode_icy_block("StreamTitle='Guns N' Roses - Sweet Child O' Mine';StreamUrl='';");
        assert_eq!(
            parse_icy_stream_title(&with_quote[1..]).as_deref(),
            Some("Guns N' Roses - Sweet Child O' Mine")
        );

        // Empty title or all-zero block produces None
        let empty_title = encode_icy_block("StreamTitle='   ';StreamUrl='';");
        assert_eq!(parse_icy_stream_title(&empty_title[1..]), None);
        assert_eq!(parse_icy_stream_title(&[0u8; 16]), None);
    }

    #[tokio::test]
    async fn live_http_source_strips_icy_metadata_exposes_headers_and_publishes_titles() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let metaint = 1024usize;
        // 40 KiB of deterministic pseudo-audio payload (> 16 KiB preroll, crossing ~40 ICY blocks)
        let original_audio: Vec<u8> = (0..40_960).map(|i| ((i * 31 + 7) % 251) as u8).collect();
        let interleaved = build_icy_interleaved_stream(
            &original_audio,
            metaint,
            &[
                "Nightwish - Ghost Love Score",
                "Nightwish - Ghost Love Score", // duplicate: must be deduplicated
                "",                             // empty block (len = 0)
                "Epica - Consign to Oblivion",
            ],
        );

        Mock::given(method("GET"))
            .and(path("/live"))
            .and(header("Icy-MetaData", "1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "audio/mpeg")
                    .insert_header("icy-metaint", metaint.to_string())
                    .insert_header("icy-name", "Luminous Metal Radio")
                    .insert_header("icy-genre", "Symphonic Metal")
                    .insert_header("icy-br", "192")
                    .set_body_bytes(interleaved),
            )
            .expect(1)
            .mount(&server)
            .await;

        let url = format!("{}/live", server.uri());
        tokio::task::spawn_blocking(move || {
            // No reconnects so we can read cleanly up to the end of the 40 KiB fixture
            let opened = LiveHttpSource::open_with_backoffs(&url, None, Vec::new())
                .expect("open live stream");
            assert!(!opened.reader.is_seekable());
            assert_eq!(opened.reader.byte_len(), None);
            assert_eq!(opened.mime_type.as_deref(), Some("audio/mpeg"));
            assert_eq!(
                opened.icy_headers,
                Some(IcyHeaders {
                    name: Some("Luminous Metal Radio".into()),
                    genre: Some("Symphonic Metal".into()),
                    bitrate_kbps: Some(192),
                })
            );

            let mut reader = opened.reader;
            let mut decoded_audio = vec![0u8; original_audio.len()];
            reader
                .read_exact(&mut decoded_audio)
                .expect("read full stripped audio payload");
            assert_eq!(
                decoded_audio, original_audio,
                "ICY metadata blocks must be completely stripped without corrupting audio bytes"
            );

            let rx = opened.metadata_rx.expect("metadata channel present");
            let titles: Vec<String> = rx.try_iter().collect();
            assert_eq!(
                titles,
                vec![
                    "Nightwish - Ghost Love Score".to_string(),
                    "Epica - Consign to Oblivion".to_string(),
                ]
            );
        })
        .await
        .unwrap();
    }

    #[test]
    fn live_http_source_tolerates_shoutcast_v1_icy_200_ok_and_reconnects_on_drop() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake shoutcast server");
        let addr = listener.local_addr().unwrap();

        let part1: Vec<u8> = (0..20_000).map(|i| (i % 256) as u8).collect();
        let part2: Vec<u8> = (20_000..40_000).map(|i| (i % 256) as u8).collect();
        let part1_srv = part1.clone();
        let part2_srv = part2.clone();
        let metaint = 4096usize;
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_srv = Arc::clone(&hits);

        let server_thread = std::thread::spawn(move || {
            // Accept 4 connections:
            // 0) Initial reqwest probe: responds with non-HTTP `ICY 200 OK`, causing reqwest
            //    to trigger the Shoutcast v1 raw TCP fallback.
            // 1) Initial Shoutcast v1 fallback open: responds with `ICY 200 OK` + part1, then drops.
            // 2) First reconnect (goes directly to Shoutcast v1 fallback): sends part2, then drops.
            // 3) Second reconnect: refuses with 503 so retry exhaustion surfaces an error.
            for conn_idx in 0..4 {
                let Ok((mut socket, _)) = listener.accept() else {
                    break;
                };
                hits_srv.fetch_add(1, Ordering::SeqCst);
                let mut req_buf = [0u8; 1024];
                let _ = socket.read(&mut req_buf);
                let req_text = String::from_utf8_lossy(&req_buf);
                assert!(
                    req_text.to_ascii_lowercase().contains("icy-metadata: 1"),
                    "client must send Icy-MetaData: 1"
                );

                match conn_idx {
                    0 | 1 => {
                        let body = build_icy_interleaved_stream(
                            &part1_srv,
                            metaint,
                            &["Track One - Shoutcast v1"],
                        );
                        let headers = format!(
                            "ICY 200 OK\r\nicy-name:Classic Shoutcast\r\nicy-genre:Rock\r\nicy-br:128\r\nicy-metaint:{metaint}\r\ncontent-type:audio/mpeg\r\n\r\n"
                        );
                        let _ = socket.write_all(headers.as_bytes());
                        let _ = socket.write_all(&body);
                    }
                    2 => {
                        let body = build_icy_interleaved_stream(
                            &part2_srv,
                            metaint,
                            &["Track Two - Reconnected"],
                        );
                        let headers = format!(
                            "ICY 200 OK\r\nicy-name:Classic Shoutcast\r\nicy-metaint:{metaint}\r\ncontent-type:audio/mpeg\r\n\r\n"
                        );
                        let _ = socket.write_all(headers.as_bytes());
                        let _ = socket.write_all(&body);
                    }
                    _ => {
                        let _ = socket.write_all(b"HTTP/1.0 503 Service Unavailable\r\n\r\n");
                    }
                }
            }
        });

        let url = format!("http://{addr}/stream");
        let opened = LiveHttpSource::open_with_backoffs(
            &url,
            Some("radio://shoutcast-test"),
            vec![Duration::from_millis(10)],
        )
        .expect("connect to Shoutcast v1 ICY 200 OK server");

        assert_eq!(opened.mime_type.as_deref(), Some("audio/mpeg"));
        assert_eq!(
            opened.icy_headers.as_ref().and_then(|h| h.name.as_deref()),
            Some("Classic Shoutcast")
        );

        let mut reader = opened.reader;
        let mut combined = vec![0u8; part1.len() + part2.len()];
        reader
            .read_exact(&mut combined)
            .expect("seamlessly read across dropped connection and reconnect");
        assert_eq!(&combined[..part1.len()], &part1[..]);
        assert_eq!(&combined[part1.len()..], &part2[..]);

        let rx = opened.metadata_rx.unwrap();
        let titles: Vec<String> = rx.try_iter().collect();
        assert_eq!(
            titles,
            vec![
                "Track One - Shoutcast v1".to_string(),
                "Track Two - Reconnected".to_string()
            ]
        );

        // Next read after part2 drops and exhausts the 1 reconnect attempt against the 503 response.
        // It must return an Err (not Ok(0) EOF) so the audio engine emits AudioEvent::Error.
        let mut extra = [0u8; 64];
        let err = reader
            .read(&mut extra)
            .expect_err("exhausted reconnects on live stream must return an error, never Ok(0)");
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionAborted);
        assert!(
            err.to_string().contains("radio://shoutcast-test"),
            "error should name the label: {err}"
        );
        assert!(hits.load(Ordering::SeqCst) >= 3);
        let _ = server_thread.join();
    }

    #[test]
    fn radio_uri_resolves_against_directory_before_songs_fallback() {
        let dir = tempfile::Builder::new()
            .prefix("luminous_media_source_radio_")
            .tempdir()
            .unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO radio_directory (
                stationuuid, name, url_resolved, homepage, favicon, tags,
                country, countrycode, language, codec, bitrate, hls,
                votes, clickcount, lastcheckok, changeuuid, updated_at
             ) VALUES (
                'u-1', 'Station One', 'https://fresh.example/live.mp3', '', '', '',
                '', '', '', 'MP3', 128, 0, 1, 1, 1, 'c-1', 100
             )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO songs (source, filetype, path, stream_url, title) VALUES (?1, ?2, 'radio://u-1', 'https://stale.example/old.mp3', 'Station One')",
            rusqlite::params![
                crate::models::SongSource::RADIO_BROWSER_ID,
                crate::models::FileType::Stream as i32
            ],
        )
        .unwrap();

        let resolved = resolve_live_stream_url(&db, "radio://u-1").unwrap();
        assert_eq!(resolved.as_deref(), Some("https://fresh.example/live.mp3"));

        // Falls back to songs.stream_url when absent from radio_directory.
        conn.execute(
            "INSERT INTO songs (source, filetype, path, stream_url, title) VALUES (?1, ?2, 'radio://u-fallback', 'https://fallback.example/stream', 'Fallback')",
            rusqlite::params![
                crate::models::SongSource::RADIO_BROWSER_ID,
                crate::models::FileType::Stream as i32
            ],
        )
        .unwrap();
        let fallback = resolve_live_stream_url(&db, "radio://u-fallback").unwrap();
        assert_eq!(fallback.as_deref(), Some("https://fallback.example/stream"));

        let err = resolve_live_stream_url(&db, "radio://missing").unwrap_err();
        assert!(err.contains("Station offline"));
    }
}
