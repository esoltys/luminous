//! HLS (`.m3u8`) internet radio playback (#2012).
//!
//! [`HlsLiveSource`] follows a station's live playlist window, downloads its
//! segments on a background thread, and hands the audio engine one continuous
//! ADTS AAC or MPEG audio byte stream (see [`segment`]), so the engine treats
//! an HLS station like any other non-seekable live stream.
//!
//! Supported: MPEG-TS and packed-audio (ID3 + ADTS/MP3) segments, master
//! playlists (variant and audio-rendition selection) and live window reloads.
//! Not supported, with a clear error at open: encrypted segments (`EXT-X-KEY`)
//! and fragmented MP4 (`EXT-X-MAP`).

pub mod playlist;
pub mod segment;

use luminous_audio::{MediaSource, OpenedSource};
use parking_lot::Mutex;
use playlist::{MediaPlaylist, Playlist};
use reqwest::Url;
use segment::SegmentDemuxer;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, LazyLock};
use std::time::Duration;

/// Playlists and segments are small, finite downloads: unlike the endless
/// Icecast client, these requests get a total timeout.
static HLS_CLIENT: LazyLock<reqwest::blocking::Client> = LazyLock::new(|| {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("Failed to build HLS HTTP client")
});

/// Largest playlist accepted, so a misbehaving server can't make us buffer an
/// endless body.
const MAX_PLAYLIST_BYTES: u64 = 2 * 1024 * 1024;
/// Largest segment accepted (a 10 s video+audio TS segment is a few MB).
const MAX_SEGMENT_BYTES: u64 = 32 * 1024 * 1024;
/// How many segments behind the live edge playback starts. RFC 8216 asks
/// clients to start at least three target durations from the end.
const LIVE_START_OFFSET: usize = 3;
/// Downloaded segments kept ready ahead of the decoder.
const SEGMENT_BUFFER: usize = 3;
/// Delays between playlist retries before the station is reported as lost.
const RELOAD_BACKOFFS: &[Duration] = &[
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];
/// Attempts per segment before it is skipped.
const SEGMENT_ATTEMPTS: usize = 2;
/// Segments skipped in a row before the station is reported as lost, rather
/// than leaving the listener in silence until the read times out.
const MAX_SKIPPED_SEGMENTS: usize = 3;

/// MIME types servers use for HLS playlists.
pub fn is_playlist_mime(mime: &str) -> bool {
    let base = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    matches!(
        base.as_str(),
        "application/vnd.apple.mpegurl"
            | "application/x-mpegurl"
            | "audio/mpegurl"
            | "audio/x-mpegurl"
    )
}

/// True when `url`'s path ends in `.m3u8`.
pub fn has_m3u8_extension(url: &str) -> bool {
    Url::parse(url)
        .map(|u| u.path().to_ascii_lowercase().ends_with(".m3u8"))
        .unwrap_or(false)
}

/// Reads at most `limit` bytes of a playlist body as text.
pub fn read_playlist_body(reader: impl Read, limit: u64) -> Result<String, String> {
    let mut body = Vec::new();
    reader
        .take(limit)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// A non-seekable live source assembled from an HLS station's segments.
pub struct HlsLiveSource {
    current: Vec<u8>,
    pos: usize,
    rx: Mutex<mpsc::Receiver<Result<Vec<u8>, String>>>,
    stop: Arc<AtomicBool>,
    label: String,
}

impl HlsLiveSource {
    /// Opens an HLS station from an already-downloaded playlist `text` served
    /// at `playlist_url` (after redirects).
    ///
    /// Resolves a master playlist to one media playlist, downloads the first
    /// segment to learn the stream's codec, then keeps following the live
    /// window on a background thread. `label` names the station in errors.
    pub fn open(playlist_url: &str, text: &str, label: &str) -> Result<OpenedSource, String> {
        let base = Url::parse(playlist_url).map_err(|e| e.to_string())?;
        let (media_url, media) = match playlist::parse(text, &base)? {
            Playlist::Media(media) => (base, media),
            Playlist::Master(master) => {
                let prefer_lc = !luminous_audio::decodes_full_he_aac();
                let url = playlist::choose_variant(&master, prefer_lc).ok_or_else(|| {
                    format!("Can't play '{label}': none of its HLS variants uses AAC or MP3 audio")
                })?;
                let media = fetch_media_playlist(&url)?;
                (url, media)
            }
        };
        check_supported(&media, label)?;

        let start = if media.end_list {
            0
        } else {
            media.segments.len().saturating_sub(LIVE_START_OFFSET)
        };
        let Some(first) = media.segments.get(start) else {
            return Err(format!(
                "Can't play '{label}': its HLS playlist has no segments"
            ));
        };

        let mut demux = SegmentDemuxer::new();
        let first_audio = demux.process(&fetch_bytes(&first.uri, MAX_SEGMENT_BYTES)?)?;
        let kind = first_audio
            .kind
            .ok_or_else(|| format!("Can't play '{label}': no audio in its first HLS segment"))?;

        let (tx, rx) = mpsc::sync_channel(SEGMENT_BUFFER);
        let (title_tx, title_rx) = mpsc::channel();
        if let Some(title) = first_audio.title.clone() {
            let _ = title_tx.send(title);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let worker = Worker {
            media_url,
            next_sequence: first.sequence + 1,
            last_media: media,
            demux,
            tx,
            title_tx,
            last_title: first_audio.title,
            skipped: 0,
            stop: stop.clone(),
            label: label.to_string(),
        };
        std::thread::Builder::new()
            .name("hls-live".into())
            .spawn(move || worker.run())
            .map_err(|e| e.to_string())?;

        Ok(OpenedSource {
            reader: Box::new(HlsLiveSource {
                current: first_audio.bytes,
                pos: 0,
                rx: Mutex::new(rx),
                stop,
                label: label.to_string(),
            }),
            mime_type: Some(kind.mime_type().to_string()),
            icy_headers: None,
            metadata_rx: Some(title_rx),
        })
    }
}

impl Drop for HlsLiveSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Read for HlsLiveSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.pos >= self.current.len() {
            // A live window that stops growing for this long is a dead station.
            match self.rx.lock().recv_timeout(Duration::from_secs(60)) {
                Ok(Ok(bytes)) => {
                    self.current = bytes;
                    self.pos = 0;
                }
                Ok(Err(e)) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionAborted,
                        e,
                    ))
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("HLS station '{}' stopped sending audio", self.label),
                    ))
                }
                // The worker finished an `EXT-X-ENDLIST` playlist: a clean end.
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = buf.len().min(self.current.len() - self.pos);
        buf[..n].copy_from_slice(&self.current[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

impl Seek for HlsLiveSource {
    fn seek(&mut self, _pos: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Cannot seek a live stream",
        ))
    }
}

impl MediaSource for HlsLiveSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// Follows the media playlist and feeds segment audio to the reader.
struct Worker {
    media_url: Url,
    next_sequence: u64,
    last_media: MediaPlaylist,
    demux: SegmentDemuxer,
    tx: mpsc::SyncSender<Result<Vec<u8>, String>>,
    title_tx: mpsc::Sender<String>,
    last_title: Option<String>,
    /// Segments skipped in a row (download or demux failures).
    skipped: usize,
    stop: Arc<AtomicBool>,
    label: String,
}

impl Worker {
    fn run(mut self) {
        let mut media = std::mem::take(&mut self.last_media);
        loop {
            if !self.play_new_segments(&media) {
                return; // the reader is gone, or the station was lost
            }
            if media.end_list {
                return; // dropping `tx` ends the stream cleanly
            }

            // Reload after half a target duration when nothing was new
            // (RFC 8216 §6.3.4); new segments only appear once per target.
            let wait = Duration::from_millis(media.target_duration_secs.clamp(1, 30) * 500);
            if !self.sleep(wait) {
                return;
            }
            match self.reload() {
                Some(next) => media = next,
                None => return,
            }
        }
    }

    /// Downloads and forwards every segment at or after `next_sequence`.
    /// Returns false once the reader has been dropped, or after too many
    /// segments failed in a row (the error has then been sent to the reader).
    fn play_new_segments(&mut self, media: &MediaPlaylist) -> bool {
        let Some(last) = media.segments.last() else {
            return true;
        };
        let first = media.segments[0].sequence;
        if self.next_sequence < first || self.next_sequence > last.sequence + 1 {
            // Fell out of the window (a long pause) or the stream restarted
            // its numbering: rejoin near the live edge.
            let rejoin = media.segments.len().saturating_sub(LIVE_START_OFFSET);
            log::warn!(
                "HLS station '{}' rejoining the live window at sequence {}",
                self.label,
                media.segments[rejoin].sequence
            );
            self.next_sequence = media.segments[rejoin].sequence;
        }

        let from = self.next_sequence;
        for seg in media.segments.iter().filter(|s| s.sequence >= from) {
            if self.stop.load(Ordering::Relaxed) {
                return false;
            }
            self.next_sequence = seg.sequence + 1;
            let processed = self
                .fetch_segment(&seg.uri)
                .and_then(|data| self.demux.process(&data));
            match processed {
                Ok(audio) => {
                    self.skipped = 0;
                    if let Some(title) = audio.title {
                        if self.last_title.as_deref() != Some(title.as_str()) {
                            let _ = self.title_tx.send(title.clone());
                            self.last_title = Some(title);
                        }
                    }
                    if self.tx.send(Ok(audio.bytes)).is_err() {
                        return false;
                    }
                }
                Err(e) => {
                    log::warn!("HLS station '{}': skipping a segment: {e}", self.label);
                    self.skipped += 1;
                    if self.skipped >= MAX_SKIPPED_SEGMENTS {
                        let _ = self.tx.send(Err(format!(
                            "HLS station '{}' stopped delivering audio: {e}",
                            self.label
                        )));
                        return false;
                    }
                }
            }
        }
        true
    }

    fn fetch_segment(&self, uri: &Url) -> Result<Vec<u8>, String> {
        let mut last_err = String::new();
        for attempt in 1..=SEGMENT_ATTEMPTS {
            match fetch_bytes(uri, MAX_SEGMENT_BYTES) {
                Ok(data) => return Ok(data),
                Err(e) => {
                    log::warn!(
                        "HLS station '{}': segment download failed (attempt {attempt}): {e}",
                        self.label
                    );
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    /// Re-downloads the media playlist, retrying with backoff. On a final
    /// failure the error goes to the reader so the engine reports it.
    fn reload(&mut self) -> Option<MediaPlaylist> {
        let mut last_err = String::new();
        for delay in std::iter::once(&Duration::ZERO).chain(RELOAD_BACKOFFS) {
            if !self.sleep(*delay) {
                return None;
            }
            match fetch_media_playlist(&self.media_url) {
                Ok(media) => match check_supported(&media, &self.label) {
                    Ok(()) => return Some(media),
                    Err(e) => {
                        last_err = e;
                        break;
                    }
                },
                Err(e) => last_err = e,
            }
        }
        let _ = self.tx.send(Err(format!(
            "HLS station '{}' disconnected: {last_err}",
            self.label
        )));
        None
    }

    /// Sleeps in short steps so a dropped reader stops the thread promptly.
    /// Returns false when asked to stop.
    fn sleep(&self, total: Duration) -> bool {
        let step = Duration::from_millis(200);
        let mut left = total;
        while !left.is_zero() {
            if self.stop.load(Ordering::Relaxed) {
                return false;
            }
            let nap = left.min(step);
            std::thread::sleep(nap);
            left -= nap;
        }
        !self.stop.load(Ordering::Relaxed)
    }
}

fn check_supported(media: &MediaPlaylist, label: &str) -> Result<(), String> {
    if media.encrypted {
        return Err(format!("Can't play '{label}': its HLS stream is encrypted"));
    }
    if media.has_init_map {
        return Err(format!(
            "Can't play '{label}': its HLS stream uses fragmented MP4, which isn't supported"
        ));
    }
    Ok(())
}

fn fetch_media_playlist(url: &Url) -> Result<MediaPlaylist, String> {
    let resp = HLS_CLIENT
        .get(url.clone())
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.without_url().to_string())?;
    let final_url = resp.url().clone();
    let text = read_playlist_body(resp, MAX_PLAYLIST_BYTES)?;
    match playlist::parse(&text, &final_url)? {
        Playlist::Media(media) => Ok(media),
        Playlist::Master(_) => Err("expected a media playlist, got a master playlist".into()),
    }
}

fn fetch_bytes(url: &Url, limit: u64) -> Result<Vec<u8>, String> {
    let resp = HLS_CLIENT
        .get(url.clone())
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.without_url().to_string())?;
    let mut data = Vec::new();
    resp.take(limit)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    Ok(data)
}

#[cfg(test)]
mod tests;
