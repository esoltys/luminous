//! Online lyrics lookup — tries LRCLIB (synced `.lrc` lyrics, preferred)
//! and NetEase Cloud Music before falling back to Lyrics.ovh (plain text only).
//! See `LyricsManager::fetch_lyrics` for the fallback chain.

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Deserialized LRCLIB `/api/get` response. `_id` is unused but kept so
/// `#[derive(Deserialize)]` doesn't reject the field the API sends.
#[derive(Deserialize, Debug)]
pub struct LrcLibResponse {
    pub _id: Option<i64>,
    #[serde(rename = "syncedLyrics")]
    pub synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    pub plain_lyrics: Option<String>,
}

/// Deserialized Lyrics.ovh response — plain text only, no sync timestamps.
#[derive(Deserialize, Debug)]
pub struct LyricsOvhResponse {
    pub lyrics: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct NetEaseSearchResponse {
    pub result: Option<NetEaseSearchResult>,
}

#[derive(Deserialize, Debug)]
pub struct NetEaseSearchResult {
    pub songs: Option<Vec<NetEaseSong>>,
}

#[derive(Deserialize, Debug)]
pub struct NetEaseSong {
    pub id: i64,
    pub name: Option<String>,
    pub ar: Option<Vec<NetEaseArtist>>,
    pub dt: Option<i64>,
}

#[derive(Deserialize, Debug)]
pub struct NetEaseArtist {
    pub name: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct NetEaseLyricResponse {
    pub lrc: Option<NetEaseLyricData>,
}

#[derive(Deserialize, Debug)]
pub struct NetEaseLyricData {
    pub lyric: Option<String>,
}

/// NetEase search results only count as a match when their duration is within
/// this many milliseconds of the local file's.
const NETEASE_DURATION_TOLERANCE_MS: i64 = 8000;

/// Picks the search result whose duration is closest to `duration_sec`. When
/// the duration is known, a candidate outside the tolerance is never chosen —
/// a same-titled song of a different length (cover, live, remix) would give
/// lyrics timed for the wrong recording. With no known duration, the top
/// result is taken.
fn pick_netease_song(songs: &[NetEaseSong], duration_sec: u32) -> Option<&NetEaseSong> {
    if duration_sec == 0 {
        return songs.first();
    }
    let target_ms = i64::from(duration_sec) * 1000;
    songs
        .iter()
        .filter_map(|s| s.dt.map(|dt| (s, (dt - target_ms).abs())))
        .filter(|(_, diff)| *diff <= NETEASE_DURATION_TOLERANCE_MS)
        .min_by_key(|(_, diff)| *diff)
        .map(|(s, _)| s)
}

/// Holds the shared HTTP client used for every provider request. Cheap to
/// construct (no state beyond the client), so callers can create one
/// per-lookup rather than needing to share an instance.
pub struct LyricsManager {
    client: Client,
}

impl Default for LyricsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsManager {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(6))
                .user_agent(concat!("LuminousMusicPlayer/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Search LRCLIB, NetEase Cloud Music, and Lyrics.ovh in priority order,
    /// returning the first **synced** result found (short-circuits immediately).
    /// If none of the providers return synced lyrics, falls back to whichever
    /// plain-text result was found first rather than failing outright. Errs only
    /// if every provider — including retries with the title's "(feat. ...)"
    /// annotation stripped — comes back empty.
    pub async fn fetch_lyrics(
        &self,
        artist: &str,
        title: &str,
        album: &str,
        duration_sec: u32,
    ) -> Result<String> {
        let mut best_lyrics: Option<String> = None;

        // 1. Try LRCLIB primary (highly specific with track, album, and duration)
        if let Ok(lyrics) = self
            .fetch_lrclib(artist, title, Some(album), duration_sec)
            .await
        {
            if is_synced_lrc(&lyrics) {
                return Ok(lyrics);
            }
            if best_lyrics.is_none() {
                best_lyrics = Some(lyrics);
            }
        }

        // 1b. Try LRCLIB fallback (omitting the album, as album names can differ/remaster/etc.)
        if let Ok(lyrics) = self.fetch_lrclib(artist, title, None, duration_sec).await {
            if is_synced_lrc(&lyrics) {
                return Ok(lyrics);
            }
            if best_lyrics.is_none() {
                best_lyrics = Some(lyrics);
            }
        }

        // 2. Try NetEase Cloud Music (strong coverage for East Asian and international synced lyrics)
        if let Ok(lyrics) = self.fetch_netease(artist, title, duration_sec).await {
            if is_synced_lrc(&lyrics) {
                return Ok(lyrics);
            }
            if best_lyrics.is_none() {
                best_lyrics = Some(lyrics);
            }
        }

        // 3. Try Lyrics.ovh fallback (only needs artist & title, returns plain text)
        if let Ok(lyrics) = self.fetch_lyrics_ovh(artist, title).await {
            if is_synced_lrc(&lyrics) {
                return Ok(lyrics);
            }
            if best_lyrics.is_none() {
                best_lyrics = Some(lyrics);
            }
        }

        // 4. Clean title of featured artist annotations (e.g., "(feat. ...)") and retry online search
        let cleaned_title = clean_featured_title(title);
        if cleaned_title != title {
            if let Ok(lyrics) = self
                .fetch_lrclib(artist, &cleaned_title, None, duration_sec)
                .await
            {
                if is_synced_lrc(&lyrics) {
                    return Ok(lyrics);
                }
                if best_lyrics.is_none() {
                    best_lyrics = Some(lyrics);
                }
            }
            if let Ok(lyrics) = self
                .fetch_netease(artist, &cleaned_title, duration_sec)
                .await
            {
                if is_synced_lrc(&lyrics) {
                    return Ok(lyrics);
                }
                if best_lyrics.is_none() {
                    best_lyrics = Some(lyrics);
                }
            }
            if let Ok(lyrics) = self.fetch_lyrics_ovh(artist, &cleaned_title).await {
                if is_synced_lrc(&lyrics) {
                    return Ok(lyrics);
                }
                if best_lyrics.is_none() {
                    best_lyrics = Some(lyrics);
                }
            }
        }

        if let Some(lyrics) = best_lyrics {
            return Ok(lyrics);
        }

        Err(anyhow!("no lyrics found on any online provider"))
    }

    async fn fetch_lrclib(
        &self,
        artist: &str,
        title: &str,
        album: Option<&str>,
        duration_sec: u32,
    ) -> Result<String> {
        let mut url = format!(
            "https://lrclib.net/api/get?artist_name={}&track_name={}&duration={}",
            percent_encoding::utf8_percent_encode(artist, percent_encoding::NON_ALPHANUMERIC),
            percent_encoding::utf8_percent_encode(title, percent_encoding::NON_ALPHANUMERIC),
            duration_sec
        );

        if let Some(alb) = album {
            if !alb.trim().is_empty() {
                url.push_str(&format!(
                    "&album_name={}",
                    percent_encoding::utf8_percent_encode(alb, percent_encoding::NON_ALPHANUMERIC)
                ));
            }
        }

        let response = self.client.get(&url).send().await?;
        if response.status().is_success() {
            let res: LrcLibResponse = response.json().await?;
            if let Some(synced) = res.synced_lyrics {
                if !synced.trim().is_empty() {
                    return Ok(synced);
                }
            }
            if let Some(plain) = res.plain_lyrics {
                if !plain.trim().is_empty() {
                    return Ok(plain);
                }
            }
        }

        Err(anyhow!("LRCLIB returned no lyrics"))
    }

    async fn fetch_netease(&self, artist: &str, title: &str, duration_sec: u32) -> Result<String> {
        let query = format!("{artist} {title}");
        let search_url = "https://music.163.com/api/cloudsearch/pc";

        let response = self
            .client
            .post(search_url)
            .header("Referer", "https://music.163.com/")
            .form(&[
                ("s", query.as_str()),
                ("type", "1"),
                ("offset", "0"),
                ("limit", "5"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "NetEase search request failed with status: {}",
                response.status()
            ));
        }

        let res: NetEaseSearchResponse = response.json().await?;
        let songs = res.result.and_then(|r| r.songs).unwrap_or_default();

        if songs.is_empty() {
            return Err(anyhow!("NetEase returned no songs for query"));
        }

        let Some(song) = pick_netease_song(&songs, duration_sec) else {
            return Err(anyhow!("No NetEase candidate within duration tolerance"));
        };

        let lyric_url = format!(
            "https://music.163.com/api/song/lyric?id={}&lv=1&kv=1&tv=-1",
            song.id
        );

        let lyric_resp = self
            .client
            .get(&lyric_url)
            .header("Referer", "https://music.163.com/")
            .send()
            .await?;

        if !lyric_resp.status().is_success() {
            return Err(anyhow!("NetEase lyric request failed"));
        }

        let lyric_data: NetEaseLyricResponse = lyric_resp.json().await?;
        if let Some(lrc) = lyric_data.lrc {
            if let Some(lyric) = lrc.lyric {
                if !lyric.trim().is_empty() {
                    return Ok(lyric);
                }
            }
        }

        Err(anyhow!("NetEase returned empty lyrics"))
    }

    async fn fetch_lyrics_ovh(&self, artist: &str, title: &str) -> Result<String> {
        let url = format!(
            "https://api.lyrics.ovh/v1/{}/{}",
            percent_encoding::utf8_percent_encode(artist, percent_encoding::NON_ALPHANUMERIC),
            percent_encoding::utf8_percent_encode(title, percent_encoding::NON_ALPHANUMERIC)
        );

        let response = self.client.get(&url).send().await?;
        if response.status().is_success() {
            let res: LyricsOvhResponse = response.json().await?;
            if let Some(lyrics) = res.lyrics {
                return Ok(lyrics);
            }
        }

        Err(anyhow!("Lyrics.ovh returned no lyrics"))
    }
}

/// Normalize a filename stem or title string for robust matching:
/// lowercases and replaces punctuation/separators with whitespace.
fn normalize_stem(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_disc_prefix(stem: &str) -> Option<&str> {
    let bytes = stem.as_bytes();
    // Check "D-TT " or "D-TT-" or "D.TT " e.g. "1-02 " (len >= 5)
    if bytes.len() >= 5
        && bytes[0].is_ascii_digit()
        && (bytes[1] == b'-' || bytes[1] == b'.')
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
    {
        return Some(&stem[2..]);
    }
    // Check "DD-TT " e.g. "01-02 " (len >= 6)
    if bytes.len() >= 6
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && (bytes[2] == b'-' || bytes[2] == b'.')
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
    {
        return Some(&stem[3..]);
    }
    None
}

fn extract_track_and_title_from_stem(stem: &str) -> (Option<i32>, Option<String>) {
    let effective_stem = strip_disc_prefix(stem).unwrap_or(stem);
    let mut parts = effective_stem.splitn(2, ['-', '_', ' ']);
    if let Some(first) = parts.next() {
        if let Ok(num) = first.trim().parse::<i32>() {
            let title = parts
                .next()
                .map(|t| t.trim_start_matches(['-', '_', ' ']).trim().to_string());
            return (Some(num), title);
        }
    }
    (None, None)
}

/// Sidecar lyric extensions in precedence order. Within each lookup step of
/// `find_sidecar_lyrics`, an earlier extension wins; an earlier step always
/// beats a later one regardless of extension.
const SIDECAR_EXTS: [&str; 3] = ["lrc", "vtt", "srt"];

/// Index of `path`'s extension in `SIDECAR_EXTS` (case-insensitive), or
/// `None` if it isn't a sidecar lyric file.
fn sidecar_ext_rank(path: &Path) -> Option<usize> {
    let ext = path.extension()?.to_str()?;
    SIDECAR_EXTS
        .iter()
        .position(|e| ext.eq_ignore_ascii_case(e))
}

/// Look for a sidecar lyrics file (`.lrc`, `.vtt` or `.srt`) in the same
/// directory as `audio_path`.
///
/// Precedence — steps first, then format (`.lrc` > `.vtt` > `.srt` within a step):
/// 1. Direct match: `<audio_stem>.<ext>`, lower- or uppercase extension.
/// 2. Disc prefix strip / title candidates: e.g. `1-01 Track.flac` -> `01 - Track.lrc`, `01 Track.srt`.
/// 3. Sibling directory scan: matches sidecar files in the parent folder against track
///    number and/or title (e.g. `Artist - Album - 02 Track.lrc`, `Artist_Album_01_Track.vtt`).
pub fn find_sidecar_lyrics(
    audio_path: &Path,
    title: Option<&str>,
    track: Option<i32>,
) -> Option<PathBuf> {
    // 1. Direct match
    for ext in SIDECAR_EXTS {
        for variant in [ext.to_string(), ext.to_ascii_uppercase()] {
            let direct = audio_path.with_extension(variant);
            if direct.is_file() {
                return Some(direct);
            }
        }
    }

    let parent = audio_path.parent()?;
    let stem = audio_path.file_stem()?.to_str()?;

    // 2. Try disc-prefix stripped candidates directly if filename starts with disc number
    // e.g. "1-02 Big Guns" or "1.02 Big Guns" -> track "02", title "Big Guns"
    let mut candidate_stems: Vec<String> = Vec::new();

    if let Some(stripped) = strip_disc_prefix(stem) {
        candidate_stems.push(stripped.to_string());
        let trimmed_leading = stripped.trim_start_matches(['-', '_', ' ']).trim();
        if !trimmed_leading.is_empty() {
            candidate_stems.push(trimmed_leading.to_string());
        }
    }

    if let Some(t) = title.filter(|t| !t.trim().is_empty()) {
        let clean_t = t.trim();
        candidate_stems.push(clean_t.to_string());
        if let Some(trk) = track.filter(|&t| t > 0) {
            candidate_stems.push(format!("{:02} - {}", trk, clean_t));
            candidate_stems.push(format!("{:02} {}", trk, clean_t));
            candidate_stems.push(format!("{} - {}", trk, clean_t));
            candidate_stems.push(format!("{} {}", trk, clean_t));
        }
    }

    for ext in SIDECAR_EXTS {
        for cand_stem in &candidate_stems {
            for variant in [ext.to_string(), ext.to_ascii_uppercase()] {
                let p = parent.join(format!("{cand_stem}.{variant}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    // 3. Scan sibling files in parent directory for tag-prefixed conventions
    // (e.g. "Dorothy - Gifts From the Holy Ghost - 02 Big Guns.lrc")
    let entries = std::fs::read_dir(parent).ok()?;
    let mut sidecar_files: Vec<(usize, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(rank) = sidecar_ext_rank(&path) {
                sidecar_files.push((rank, path));
            }
        }
    }

    if sidecar_files.is_empty() {
        return None;
    }

    // Determine target track number and normalized title
    let (target_track_num, target_title_norm) = match (track, title) {
        (Some(trk), Some(tit)) if trk > 0 && !tit.trim().is_empty() => {
            (Some(trk), Some(normalize_stem(tit)))
        }
        (Some(trk), _) if trk > 0 => (Some(trk), None),
        (_, Some(tit)) if !tit.trim().is_empty() => (None, Some(normalize_stem(tit))),
        _ => {
            let (extracted_trk, extracted_tit) = extract_track_and_title_from_stem(stem);
            (extracted_trk, extracted_tit.map(|t| normalize_stem(&t)))
        }
    };

    // (score, extension rank, path): higher score wins, then the preferred extension.
    let mut best_match: Option<(u8, usize, PathBuf)> = None;

    for (rank, sidecar_path) in sidecar_files {
        let Some(sidecar_stem) = sidecar_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let sidecar_norm = normalize_stem(sidecar_stem);
        let sidecar_tokens: Vec<&str> = sidecar_norm.split_whitespace().collect();

        let track_matches = target_track_num.is_some_and(|trk| {
            let trk_2 = format!("{:02}", trk);
            let trk_1 = format!("{}", trk);
            sidecar_tokens
                .iter()
                .any(|&tok| tok == trk_2 || tok == trk_1)
        });

        let title_matches = target_title_norm.as_ref().is_some_and(|norm_title| {
            if norm_title.is_empty() {
                return false;
            }
            if sidecar_norm.contains(norm_title) {
                return true;
            }
            let title_tokens: Vec<&str> = norm_title.split_whitespace().collect();
            if !title_tokens.is_empty()
                && title_tokens.iter().all(|&tt| sidecar_tokens.contains(&tt))
            {
                return true;
            }
            false
        });

        let is_valid = match (target_track_num.is_some(), target_title_norm.is_some()) {
            (true, true) => title_matches, // When title is known, title must match; score 3 prefers track match too
            (false, true) => title_matches,
            (true, false) => track_matches,
            (false, false) => false,
        };

        if !is_valid {
            continue;
        }

        let score = match (track_matches, title_matches) {
            (true, true) => 3,
            (false, true) => 2,
            (true, false) => 1,
            (false, false) => 0,
        };

        if score > 0 {
            let better = match &best_match {
                Some((best_score, best_rank, _)) => {
                    score > *best_score || (score == *best_score && rank < *best_rank)
                }
                None => true,
            };
            if better {
                best_match = Some((score, rank, sidecar_path));
            }
        }
    }

    best_match.map(|(_, _, path)| path)
}

/// Where an edit to `audio_path`'s lyrics should be written back to disk, or
/// `None` when the song has no sidecar (edits then stay in the database only).
/// A `.lrc` sidecar is overwritten in place; a `.srt`/`.vtt` one is never
/// touched — the edit goes to `<audio_stem>.lrc` instead, which step 1 of
/// `find_sidecar_lyrics` then prefers on the next read.
pub fn sidecar_lyrics_save_path(
    audio_path: &Path,
    title: Option<&str>,
    track: Option<i32>,
) -> Option<PathBuf> {
    let found = find_sidecar_lyrics(audio_path, title, track)?;
    if sidecar_ext_rank(&found) == Some(0) {
        Some(found)
    } else {
        Some(audio_path.with_extension("lrc"))
    }
}

/// Read and return the contents of a sidecar lyrics file if one exists next to `audio_path`.
/// Handles UTF-8 BOM if present, strips trailing/leading whitespace, and ignores empty files.
/// `.srt`/`.vtt` subtitles are converted to synced LRC text; `.lrc` content is returned
/// as-is when synced, and plain text is marked with `[synced:false]\n`.
pub fn read_sidecar_lyrics(
    audio_path: &Path,
    title: Option<&str>,
    track: Option<i32>,
) -> Option<String> {
    let sidecar_path = find_sidecar_lyrics(audio_path, title, track)?;
    let content = std::fs::read_to_string(&sidecar_path).ok()?;
    let trimmed = content.strip_prefix('\u{feff}').unwrap_or(&content).trim();
    if trimmed.is_empty() {
        return None;
    }
    if sidecar_ext_rank(&sidecar_path) != Some(0) {
        let converted = subtitles_to_lrc(trimmed);
        return (!converted.is_empty()).then_some(converted);
    }
    if is_synced_lrc(trimmed) || trimmed.starts_with("[synced:false]") {
        Some(trimmed.to_string())
    } else {
        Some(format!("[synced:false]\n{trimmed}"))
    }
}

/// A cue gap longer than this gets an empty LRC line at the previous cue's
/// end, so the last line doesn't stay highlighted through an instrumental break.
const SUBTITLE_GAP_MS: u64 = 4000;

/// Parse an SRT (`HH:MM:SS,mmm`) or WebVTT (`HH:MM:SS.mmm` / `MM:SS.mmm`)
/// cue timestamp into milliseconds.
fn parse_cue_timestamp(s: &str) -> Option<u64> {
    let (clock, frac) = s.trim().split_once([',', '.'])?;
    let parts: Vec<&str> = clock.split(':').collect();
    let nums: Vec<u64> = parts
        .iter()
        .map(|p| p.trim().parse::<u64>().ok())
        .collect::<Option<_>>()?;
    let secs = match nums.as_slice() {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        _ => return None,
    };
    let frac = frac.trim();
    if frac.is_empty() || frac.len() > 3 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Right-pad to milliseconds: "5" -> 500, "05" -> 50, "005" -> 5.
    let ms = frac.parse::<u64>().ok()? * 10u64.pow(3 - frac.len() as u32);
    Some(secs * 1000 + ms)
}

/// Parse a cue timing line (`start --> end [cue settings]`) into `(start_ms, end_ms)`.
fn parse_cue_timing(line: &str) -> Option<(u64, u64)> {
    let (start, rest) = line.split_once("-->")?;
    let end = rest.split_whitespace().next()?;
    Some((parse_cue_timestamp(start)?, parse_cue_timestamp(end)?))
}

/// Strip markup from a cue text line: `<b>`/`<i>`/`<font …>`/`<v Speaker>`/
/// `<c.class>` tags and the common HTML entities.
fn strip_cue_markup(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_tag = false;
    for ch in line.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn format_lrc_timestamp(ms: u64) -> String {
    format!(
        "[{:02}:{:02}.{:02}]",
        ms / 60_000,
        (ms / 1000) % 60,
        (ms % 1000) / 10
    )
}

/// Convert SRT or WebVTT subtitle text into synced LRC (`[MM:SS.xx] line`).
///
/// Both formats are a series of cues: a timing line (`start --> end`) followed
/// by text lines up to a blank line. Everything outside a cue — SRT indices,
/// VTT cue identifiers, the `WEBVTT` header, `NOTE`/`STYLE`/`REGION` blocks —
/// is dropped. Multi-line cues are joined with a space. Returns an empty
/// string if no cue has text.
pub fn subtitles_to_lrc(content: &str) -> String {
    let lines: Vec<&str> = content.lines().map(str::trim).collect();
    // (start_ms, end_ms, text)
    let mut cues: Vec<(u64, u64, String)> = Vec::new();
    let mut current: Option<(u64, u64, Vec<String>)> = None;

    let flush = |current: &mut Option<(u64, u64, Vec<String>)>, cues: &mut Vec<_>| {
        if let Some((start, end, parts)) = current.take() {
            let text = parts.join(" ");
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !text.is_empty() {
                cues.push((start, end, text));
            }
        }
    };

    for (i, line) in lines.iter().enumerate() {
        if let Some((start, end)) = line
            .contains("-->")
            .then(|| parse_cue_timing(line))
            .flatten()
        {
            flush(&mut current, &mut cues);
            current = Some((start, end, Vec::new()));
        } else if line.is_empty() {
            flush(&mut current, &mut cues);
        } else if let Some((_, _, parts)) = current.as_mut() {
            // A missing blank line before the next cue would otherwise pull
            // its SRT index / VTT identifier into this cue's text.
            let next_is_timing = lines
                .get(i + 1)
                .is_some_and(|next| next.contains("-->") && parse_cue_timing(next).is_some());
            if !next_is_timing {
                parts.push(strip_cue_markup(line));
            }
        }
    }
    flush(&mut current, &mut cues);

    cues.sort_by_key(|(start, _, _)| *start);

    let mut out: Vec<String> = Vec::with_capacity(cues.len() * 2);
    for (idx, (start, _, text)) in cues.iter().enumerate() {
        if idx > 0 {
            let prev_end = cues[idx - 1].1;
            if *start > prev_end && start - prev_end > SUBTITLE_GAP_MS {
                out.push(format_lrc_timestamp(prev_end));
            }
        }
        out.push(format!("{} {}", format_lrc_timestamp(*start), text));
    }
    out.join("\n")
}

/// Resolve lyrics for `song_id`: return cached lyrics immediately when the
/// cache holds synced (or already-checked-unsynced) text, otherwise query
/// online providers via `lyrics_manager` and cache the result. Extracted
/// from the `get_lyrics` Tauri command so it's callable without a Tauri
/// `AppHandle`/`State` — BDD tests exercise this directly (see
/// `tests/lyrics_bdd.rs`) to verify the cache-hit path never reaches
/// `lyrics_manager.fetch_lyrics`.
/// Returned instead of querying online providers while the master toggle is Offline.
pub const OFFLINE_LYRICS_ERROR: &str = "offline: online lyrics lookup is disabled";

pub async fn get_lyrics_for_song(
    db: &crate::db::Database,
    lyrics_manager: &LyricsManager,
    song_id: i64,
    force_refresh: bool,
) -> Result<String, String> {
    // 1. Check database cache and instrumental flag
    let conn = db.pool.get().map_err(|e| e.to_string())?;
    let (path_str, cached_lyrics, is_instrumental, title, track): (
        Option<String>,
        Option<String>,
        bool,
        Option<String>,
        Option<i32>,
    ) = conn
        .query_row(
            "SELECT path, lyrics, COALESCE(is_instrumental, 0), title, track FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap_or((None, None, false, None, None));

    if is_instrumental {
        return Err("Song is marked as instrumental".to_string());
    }

    // 2. Check for a local sidecar .lrc/.vtt/.srt file next to the audio file (#155, #1190)
    if let Some(ref path_str) = path_str {
        let audio_path = Path::new(path_str);
        if let Some(sidecar_lyrics) = read_sidecar_lyrics(audio_path, title.as_deref(), track) {
            // If cached lyrics differ from the sidecar file (or on force refresh), update the database
            if cached_lyrics.as_deref() != Some(&sidecar_lyrics) {
                let _ = conn.execute(
                    "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
                    rusqlite::params![sidecar_lyrics, song_id],
                );
            }
            return Ok(sidecar_lyrics);
        }
    }

    if let Some(ref lyrics) = cached_lyrics {
        if !lyrics.trim().is_empty() {
            let synced = is_synced_lrc(lyrics);
            let has_plain_marker = lyrics.starts_with("[synced:false]");

            // If the cached lyrics are synced LRC, or if we have already checked online and marked it unsynced,
            // return immediately without hitting the network! Skipped entirely on a forced refresh.
            if !force_refresh && (synced || has_plain_marker) {
                return Ok(lyrics.clone());
            }
        }
    }

    // 3. Fetch metadata from DB to search online
    let (artist, title, album, len_ns) = conn
        .query_row(
            "SELECT artist, title, album, length_nanosec FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| {
                let artist: String = row.get(0).unwrap_or_default();
                let title: String = row.get(1).unwrap_or_default();
                let album: String = row.get(2).unwrap_or_default();
                let len_ns: i64 = row.get(3).unwrap_or(0);
                Ok((artist, title, album, len_ns))
            },
        )
        .map_err(|e| e.to_string())?;

    if artist.trim().is_empty() || title.trim().is_empty() {
        if let Some(lyrics) = cached_lyrics {
            if !lyrics.trim().is_empty() {
                return Ok(lyrics);
            }
        }
        return Err("insufficient song metadata (artist/title) to fetch online lyrics".to_string());
    }

    let duration_sec = (len_ns / 1_000_000_000) as u32;

    // Offline master toggle (#1398): sidecar files and cached lyrics were
    // already tried above; never fall through to the online providers, and
    // leave the cache untouched so lookup resumes when back online.
    if !crate::commands::context::is_online_enabled(&conn) {
        return match cached_lyrics {
            Some(lyrics) if !lyrics.trim().is_empty() => Ok(lyrics),
            _ => Err(OFFLINE_LYRICS_ERROR.to_string()),
        };
    }

    // 3. Query online APIs (LRCLIB -> NetEase -> Lyrics.ovh)
    match lyrics_manager
        .fetch_lyrics(&artist, &title, &album, duration_sec)
        .await
    {
        Ok(fetched) => {
            let synced = is_synced_lrc(&fetched);
            let final_lyrics = if synced {
                fetched
            } else {
                format!("[synced:false]\n{fetched}")
            };
            conn.execute(
                "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
                rusqlite::params![final_lyrics, song_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(final_lyrics)
        }
        Err(e) => {
            if let Some(lyrics) = cached_lyrics {
                if !lyrics.trim().is_empty() {
                    // Mark as checked to prevent future online lookup spamming
                    let marked_lyrics = if lyrics.starts_with("[synced:false]") {
                        lyrics.clone()
                    } else {
                        format!("[synced:false]\n{lyrics}")
                    };
                    let _ = conn.execute(
                        "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
                        rusqlite::params![marked_lyrics, song_id],
                    );
                    return Ok(marked_lyrics);
                }
            }
            Err(e.to_string())
        }
    }
}

/// Strip a trailing "(feat. ...)"/"[ft. ...]" annotation from a title.
/// Lyrics providers index by the recording's canonical title, which usually
/// omits featured-artist credits, so searching with the raw tagged title
/// often misses a match that searching with the cleaned title finds.
pub fn clean_featured_title(title: &str) -> String {
    let mut cleaned = title.to_string();
    let lower = cleaned.to_lowercase();
    for marker in &[" (feat.", " [feat.", " (ft.", " [ft.", " feat.", " ft."] {
        if let Some(pos) = lower.find(marker) {
            cleaned.truncate(pos);
            break;
        }
    }
    cleaned.trim().to_string()
}

/// True if `text` contains at least one LRC timestamp tag (`[MM:SS`).
/// Used to distinguish synced lyrics from plain text regardless of which
/// provider they came from, since Lyrics.ovh's plain-only response and
/// LRCLIB's `plain_lyrics` field share the same `String` shape as synced
/// lyrics once unwrapped.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_featured_title() {
        assert_eq!(
            clean_featured_title("Song Title (feat. Artist)"),
            "Song Title"
        );
        assert_eq!(
            clean_featured_title("Song Title [feat. Artist]"),
            "Song Title"
        );
        assert_eq!(clean_featured_title("Song Title ft. Artist"), "Song Title");
        assert_eq!(clean_featured_title("Plain Song Title"), "Plain Song Title");
    }

    #[test]
    fn test_is_synced_lrc() {
        assert!(is_synced_lrc("[00:12.00] Line of lyrics"));
        assert!(!is_synced_lrc("Plain lyrics line without LRC timestamp"));
    }

    #[test]
    fn test_normalize_stem() {
        assert_eq!(
            normalize_stem("Dorothy - Gifts From the Holy Ghost - 02 Big Guns"),
            "dorothy gifts from the holy ghost 02 big guns"
        );
        assert_eq!(normalize_stem("Inhale_Exhale Air"), "inhale exhale air");
        assert_eq!(normalize_stem("Inhale+Exhale Air"), "inhale exhale air");
        assert_eq!(normalize_stem("BEAT UP CHANEL$"), "beat up chanel");
    }

    #[test]
    fn test_strip_disc_prefix() {
        assert_eq!(strip_disc_prefix("1-02 Big Guns"), Some("02 Big Guns"));
        assert_eq!(strip_disc_prefix("01-05 Loving You"), Some("05 Loving You"));
        assert_eq!(strip_disc_prefix("1.02 Big Guns"), Some("02 Big Guns"));
        assert_eq!(strip_disc_prefix("02 Big Guns"), None);
    }

    #[test]
    fn test_find_sidecar_lrc_direct_and_uppercase() {
        let temp_dir = tempfile::tempdir().unwrap();
        let audio_path = temp_dir.path().join("song.flac");
        let lrc_path = temp_dir.path().join("song.lrc");
        std::fs::write(&audio_path, b"dummy audio").unwrap();
        std::fs::write(&lrc_path, b"[00:01.00] test").unwrap();

        assert_eq!(
            find_sidecar_lyrics(&audio_path, None, None),
            Some(lrc_path.clone())
        );

        // Case-insensitive / uppercase .LRC
        std::fs::remove_file(&lrc_path).unwrap();
        let lrc_upper = temp_dir.path().join("song.LRC");
        std::fs::write(&lrc_upper, b"[00:01.00] test upper").unwrap();
        let found = find_sidecar_lyrics(&audio_path, None, None);
        assert!(found.is_some());
        assert_eq!(
            found.unwrap().to_string_lossy().to_lowercase(),
            lrc_upper.to_string_lossy().to_lowercase()
        );
    }

    #[test]
    fn test_find_sidecar_lrc_disc_prefix_and_siblings() {
        let temp_dir = tempfile::tempdir().unwrap();
        let audio_path = temp_dir.path().join("1-02 Big Guns.flac");
        let lrc_path = temp_dir
            .path()
            .join("Dorothy - Gifts From the Holy Ghost - 02 Big Guns.lrc");
        std::fs::write(&audio_path, b"dummy audio").unwrap();
        std::fs::write(&lrc_path, b"[00:07.61] Not gonna play the fool").unwrap();

        // Should find via sibling matching on track (2) and title ("Big Guns")
        let found = find_sidecar_lyrics(&audio_path, Some("Big Guns"), Some(2));
        assert_eq!(found, Some(lrc_path.clone()));

        // Should also find via extracted stem info when metadata is not provided
        let found_from_stem = find_sidecar_lyrics(&audio_path, None, None);
        assert_eq!(found_from_stem, Some(lrc_path));

        // Different track should NOT match
        let other_audio = temp_dir.path().join("1-01 A Beautiful Life.flac");
        std::fs::write(&other_audio, b"dummy audio").unwrap();
        let not_found = find_sidecar_lyrics(&other_audio, Some("A Beautiful Life"), Some(1));
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_read_sidecar_lrc_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let audio_path = temp_dir.path().join("track.mp3");
        let lrc_path = temp_dir.path().join("track.lrc");
        std::fs::write(&audio_path, b"dummy").unwrap();

        // Synced LRC with UTF-8 BOM
        let bom_lrc = "\u{feff}[00:15.00] Synced line\n[00:20.00] Next line".to_string();
        std::fs::write(&lrc_path, bom_lrc.as_bytes()).unwrap();
        let read = read_sidecar_lyrics(&audio_path, None, None);
        assert_eq!(
            read,
            Some("[00:15.00] Synced line\n[00:20.00] Next line".to_string())
        );

        // Plain text LRC (no timestamps) should get [synced:false] prefix
        std::fs::write(&lrc_path, b"Just plain text lyrics").unwrap();
        let read_plain = read_sidecar_lyrics(&audio_path, None, None);
        assert_eq!(
            read_plain,
            Some("[synced:false]\nJust plain text lyrics".to_string())
        );

        // Empty file should return None
        std::fs::write(&lrc_path, b"   \n\t  ").unwrap();
        assert_eq!(read_sidecar_lyrics(&audio_path, None, None), None);
    }

    #[test]
    fn test_subtitles_to_lrc_srt() {
        let srt = "1\r\n00:00:07,610 --> 00:00:10,000\r\n<i>Not gonna</i> <b>play</b>\r\nthe fool\r\n\r\n\
                   2\r\n00:00:10,500 --> 00:00:12,000\r\n<font color=\"#fff\">Next line</font>\r\n\r\n\
                   3\r\n00:01:20,000 --> 00:01:22,000\r\nAfter the break\r\n";
        assert_eq!(
            subtitles_to_lrc(srt),
            "[00:07.61] Not gonna play the fool\n\
             [00:10.50] Next line\n\
             [00:12.00]\n\
             [01:20.00] After the break"
        );
    }

    #[test]
    fn test_subtitles_to_lrc_vtt() {
        let vtt = "WEBVTT - some title\n\n\
                   STYLE\n::cue { color: lime }\n\n\
                   NOTE this is a comment\nspanning --> two lines\n\n\
                   intro\n00:01.000 --> 00:03.500 align:start position:10%\n<v Singer>Hello</v> <c.loud>world</c> &amp; more\n\n\
                   01:02:03.450 --> 01:02:05.000\nLate line\n";
        assert_eq!(
            subtitles_to_lrc(vtt),
            "[00:01.00] Hello world & more\n[00:03.50]\n[62:03.45] Late line"
        );
    }

    #[test]
    fn test_subtitles_to_lrc_missing_blank_line_and_empty() {
        let srt =
            "1\n00:00:01,000 --> 00:00:02,000\nFirst\n2\n00:00:02,000 --> 00:00:03,000\nSecond\n";
        assert_eq!(subtitles_to_lrc(srt), "[00:01.00] First\n[00:02.00] Second");
        assert_eq!(subtitles_to_lrc("WEBVTT\n\nNOTE nothing here\n"), "");
    }

    #[test]
    fn test_find_sidecar_lyrics_format_precedence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let audio_path = temp_dir.path().join("1-02 Big Guns.flac");
        std::fs::write(&audio_path, b"dummy audio").unwrap();

        // Within step 3 (sibling scan), .lrc beats .vtt beats .srt
        let sib_srt = temp_dir.path().join("Dorothy - 02 Big Guns.srt");
        let sib_vtt = temp_dir.path().join("Dorothy - 02 Big Guns.vtt");
        let sib_lrc = temp_dir.path().join("Dorothy - 02 Big Guns.lrc");
        std::fs::write(&sib_srt, b"x").unwrap();
        assert_eq!(
            find_sidecar_lyrics(&audio_path, Some("Big Guns"), Some(2)),
            Some(sib_srt.clone())
        );
        std::fs::write(&sib_vtt, b"x").unwrap();
        assert_eq!(
            find_sidecar_lyrics(&audio_path, Some("Big Guns"), Some(2)),
            Some(sib_vtt.clone())
        );
        std::fs::write(&sib_lrc, b"x").unwrap();
        assert_eq!(
            find_sidecar_lyrics(&audio_path, Some("Big Guns"), Some(2)),
            Some(sib_lrc)
        );

        // An earlier step wins over a better format: a direct .srt beats a fuzzy .lrc
        let direct_srt = audio_path.with_extension("srt");
        std::fs::write(&direct_srt, b"x").unwrap();
        assert_eq!(
            find_sidecar_lyrics(&audio_path, Some("Big Guns"), Some(2)),
            Some(direct_srt)
        );
    }

    #[test]
    fn test_read_sidecar_lyrics_converts_subtitles() {
        let temp_dir = tempfile::tempdir().unwrap();
        let audio_path = temp_dir.path().join("track.mp3");
        std::fs::write(&audio_path, b"dummy").unwrap();

        let vtt_path = temp_dir.path().join("track.VTT");
        std::fs::write(
            &vtt_path,
            "\u{feff}WEBVTT\n\n00:00:05.000 --> 00:00:06.000\nFrom VTT\n",
        )
        .unwrap();
        let read = read_sidecar_lyrics(&audio_path, None, None).unwrap();
        assert_eq!(read, "[00:05.00] From VTT");
        assert!(is_synced_lrc(&read));

        // A subtitle file with no cue text yields nothing
        std::fs::write(&vtt_path, "WEBVTT\n").unwrap();
        assert_eq!(read_sidecar_lyrics(&audio_path, None, None), None);
    }

    #[test]
    fn test_sidecar_lyrics_save_path_never_overwrites_subtitles() {
        let temp_dir = tempfile::tempdir().unwrap();
        let audio_path = temp_dir.path().join("track.flac");
        std::fs::write(&audio_path, b"dummy").unwrap();

        // No sidecar: edits stay in the database only
        assert_eq!(sidecar_lyrics_save_path(&audio_path, None, None), None);

        // .srt sidecar: edits go to <stem>.lrc, leaving the subtitle untouched
        std::fs::write(temp_dir.path().join("track.srt"), b"x").unwrap();
        assert_eq!(
            sidecar_lyrics_save_path(&audio_path, None, None),
            Some(temp_dir.path().join("track.lrc"))
        );

        // A fuzzy-matched .lrc sidecar is overwritten in place
        std::fs::remove_file(temp_dir.path().join("track.srt")).unwrap();
        let fuzzy_lrc = temp_dir.path().join("Artist - track.lrc");
        std::fs::write(&fuzzy_lrc, b"x").unwrap();
        assert_eq!(
            sidecar_lyrics_save_path(&audio_path, Some("track"), None),
            Some(fuzzy_lrc)
        );
    }

    #[tokio::test]
    async fn test_get_lyrics_for_song_sidecar() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(temp_dir.path().to_path_buf()).unwrap();
        let audio_path = temp_dir.path().join("track.flac");
        let lrc_path = temp_dir.path().join("track.lrc");
        std::fs::write(&audio_path, b"dummy audio").unwrap();
        std::fs::write(&lrc_path, b"[00:10.00] Sidecar lyrics line").unwrap();

        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO songs (id, title, artist, path, source, filetype, unavailable)
             VALUES (1, 'Track', 'Artist', ?1, 1, 1, 0)",
            rusqlite::params![audio_path.to_str().unwrap()],
        )
        .unwrap();

        let lyrics_manager = LyricsManager::new();
        let result = get_lyrics_for_song(&db, &lyrics_manager, 1, false).await;
        assert_eq!(result, Ok("[00:10.00] Sidecar lyrics line".to_string()));

        // Check DB was updated with sidecar lyrics
        let cached: Option<String> = conn
            .query_row("SELECT lyrics FROM songs WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cached, Some("[00:10.00] Sidecar lyrics line".to_string()));
    }

    fn netease_song(id: i64, dt: Option<i64>) -> NetEaseSong {
        NetEaseSong {
            id,
            name: None,
            ar: None,
            dt,
        }
    }

    #[test]
    fn test_pick_netease_song_prefers_closest_duration_within_tolerance() {
        let songs = vec![
            netease_song(1, Some(240_000)),
            netease_song(2, Some(201_000)),
            netease_song(3, Some(205_000)),
        ];
        assert_eq!(pick_netease_song(&songs, 200).map(|s| s.id), Some(2));
    }

    #[test]
    fn test_pick_netease_song_rejects_candidates_outside_tolerance() {
        let songs = vec![netease_song(1, Some(300_000)), netease_song(2, None)];
        assert!(pick_netease_song(&songs, 200).is_none());
    }

    #[test]
    fn test_pick_netease_song_takes_first_when_duration_unknown() {
        let songs = vec![
            netease_song(7, Some(300_000)),
            netease_song(8, Some(200_000)),
        ];
        assert_eq!(pick_netease_song(&songs, 0).map(|s| s.id), Some(7));
    }

    #[test]
    fn test_netease_response_deserialization() {
        let search_json = r#"{"result":{"songs":[{"id":123456,"name":"夜に駆ける","ar":[{"name":"YOASOBI"}],"dt":261013}]},"code":200}"#;
        let search_res: NetEaseSearchResponse = serde_json::from_str(search_json).unwrap();
        let songs = search_res.result.unwrap().songs.unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].id, 123456);
        assert_eq!(songs[0].name.as_deref(), Some("夜に駆ける"));
        assert_eq!(songs[0].dt, Some(261013));

        let lyric_json =
            r#"{"lrc":{"version":1,"lyric":"[00:01.00]沈むように溶けてゆくように\n"},"code":200}"#;
        let lyric_res: NetEaseLyricResponse = serde_json::from_str(lyric_json).unwrap();
        assert_eq!(
            lyric_res.lrc.unwrap().lyric.as_deref(),
            Some("[00:01.00]沈むように溶けてゆくように\n")
        );
    }
}
