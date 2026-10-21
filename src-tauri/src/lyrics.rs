//! Online lyrics lookup — tries LRCLIB (synced `.lrc` lyrics, preferred)
//! and NetEase Cloud Music before falling back to Lyrics.ovh (plain text only).
//! See `LyricsManager::fetch_lyrics` for the fallback chain.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub use luminous_remote::lyrics::{
    clean_featured_title, is_synced_lrc, LyricsClient, LyricsEndpoints, LyricsManager,
    LyricsProvider, LyricsQuery, LyricsResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsSource {
    Embedded,
    Sidecar,
    Lrclib,
    Netease,
    LyricsOvh,
    Manual,
    Unknown,
}

impl LyricsSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::Sidecar => "sidecar",
            Self::Lrclib => "lrclib",
            Self::Netease => "netease",
            Self::LyricsOvh => "lyrics_ovh",
            Self::Manual => "manual",
            Self::Unknown => "unknown",
        }
    }
}

impl std::str::FromStr for LyricsSource {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "embedded" => Self::Embedded,
            "sidecar" => Self::Sidecar,
            "lrclib" => Self::Lrclib,
            "netease" => Self::Netease,
            "lyrics_ovh" => Self::LyricsOvh,
            "manual" => Self::Manual,
            _ => Self::Unknown,
        })
    }
}

impl From<&str> for LyricsSource {
    fn from(s: &str) -> Self {
        match s {
            "embedded" => Self::Embedded,
            "sidecar" => Self::Sidecar,
            "lrclib" => Self::Lrclib,
            "netease" => Self::Netease,
            "lyrics_ovh" => Self::LyricsOvh,
            "manual" => Self::Manual,
            _ => Self::Unknown,
        }
    }
}

impl From<LyricsProvider> for LyricsSource {
    fn from(p: LyricsProvider) -> Self {
        match p {
            LyricsProvider::LrcLib => LyricsSource::Lrclib,
            LyricsProvider::NetEase => LyricsSource::Netease,
            LyricsProvider::LyricsOvh => LyricsSource::LyricsOvh,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricsPayload {
    pub text: String,
    pub source: LyricsSource,
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
) -> Result<LyricsPayload, String> {
    // 1. Check database cache and instrumental flag
    let conn = db.pool.get().map_err(|e| e.to_string())?;

    struct SongLyricsRow {
        path: Option<String>,
        cached_lyrics: Option<String>,
        is_instrumental: bool,
        title: Option<String>,
        track: Option<i32>,
        lyrics_source: Option<String>,
    }

    let row = conn
        .query_row(
            "SELECT path, lyrics, COALESCE(is_instrumental, 0), title, track, lyrics_source FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |r| {
                Ok(SongLyricsRow {
                    path: r.get(0)?,
                    cached_lyrics: r.get(1)?,
                    is_instrumental: r.get(2)?,
                    title: r.get(3)?,
                    track: r.get(4)?,
                    lyrics_source: r.get(5)?,
                })
            },
        )
        .unwrap_or(SongLyricsRow {
            path: None,
            cached_lyrics: None,
            is_instrumental: false,
            title: None,
            track: None,
            lyrics_source: None,
        });

    if row.is_instrumental {
        return Err("Song is marked as instrumental".to_string());
    }

    let mut current_source = row.lyrics_source.as_deref().map(LyricsSource::from);

    // 2. Check for a local sidecar .lrc/.vtt/.srt file next to the audio file (#155, #1190)
    if let Some(ref path_str) = row.path {
        let audio_path = Path::new(path_str);
        if let Some(sidecar_lyrics) =
            read_sidecar_lyrics(audio_path, row.title.as_deref(), row.track)
        {
            // If cached lyrics differ from the sidecar file (or on force refresh), update the database
            if row.cached_lyrics.as_deref() != Some(&sidecar_lyrics)
                || current_source != Some(LyricsSource::Sidecar)
            {
                let _ = conn.execute(
                    "UPDATE songs SET lyrics = ?1, lyrics_source = 'sidecar' WHERE id = ?2",
                    rusqlite::params![sidecar_lyrics, song_id],
                );
            }
            return Ok(LyricsPayload {
                text: sidecar_lyrics,
                source: LyricsSource::Sidecar,
            });
        }
    }

    // 2b. If DB has no cached lyrics, check for embedded lyrics in the audio file (#1721).
    // This allows existing libraries scanned prior to embedded lyric support (or external tag edits)
    // to resolve embedded lyrics on demand without requiring a manual album/library rescan.
    let mut cached_lyrics = row.cached_lyrics;
    if cached_lyrics
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        if let Some(ref path_str) = row.path {
            let audio_path = Path::new(path_str);
            if let Some(embedded) = crate::collection::read_embedded_lyrics_from_path(audio_path) {
                let _ = conn.execute(
                    "UPDATE songs SET lyrics = ?1, lyrics_source = 'embedded' WHERE id = ?2",
                    rusqlite::params![embedded, song_id],
                );
                cached_lyrics = Some(embedded);
                current_source = Some(LyricsSource::Embedded);
            }
        }
    }

    if let Some(ref lyrics) = cached_lyrics {
        if !lyrics.trim().is_empty() {
            let synced = is_synced_lrc(lyrics);
            let has_plain_marker = lyrics.starts_with("[synced:false]");

            // If the cached lyrics are synced LRC, or if we have already checked online and marked it unsynced,
            // return immediately without hitting the network! Skipped entirely on a forced refresh.
            if !force_refresh && (synced || has_plain_marker) {
                return Ok(LyricsPayload {
                    text: lyrics.clone(),
                    source: current_source.unwrap_or(LyricsSource::Unknown),
                });
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
                return Ok(LyricsPayload {
                    text: lyrics,
                    source: current_source.unwrap_or(LyricsSource::Unknown),
                });
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
            Some(lyrics) if !lyrics.trim().is_empty() => Ok(LyricsPayload {
                text: lyrics,
                source: current_source.unwrap_or(LyricsSource::Unknown),
            }),
            _ => Err(OFFLINE_LYRICS_ERROR.to_string()),
        };
    }

    // 3. Query online APIs (LRCLIB -> NetEase -> Lyrics.ovh)
    let query = LyricsQuery::new(&artist, &title)
        .with_album(&album)
        .with_duration(duration_sec);

    match lyrics_manager.fetch_with_fallback(&query).await {
        Ok(result) => {
            let final_lyrics = if result.is_synced {
                result.text
            } else {
                format!("[synced:false]\n{}", result.text)
            };
            let source = LyricsSource::from(result.provider);
            conn.execute(
                "UPDATE songs SET lyrics = ?1, lyrics_source = ?2 WHERE id = ?3",
                rusqlite::params![final_lyrics, source.as_str(), song_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(LyricsPayload {
                text: final_lyrics,
                source,
            })
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
                    let source = current_source.unwrap_or(LyricsSource::Unknown);
                    let _ = conn.execute(
                        "UPDATE songs SET lyrics = ?1, lyrics_source = ?2 WHERE id = ?3",
                        rusqlite::params![marked_lyrics, source.as_str(), song_id],
                    );
                    return Ok(LyricsPayload {
                        text: marked_lyrics,
                        source,
                    });
                }
            }
            Err(e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::file::AudioFile;

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
        assert_eq!(
            result,
            Ok(LyricsPayload {
                text: "[00:10.00] Sidecar lyrics line".to_string(),
                source: LyricsSource::Sidecar,
            })
        );

        // Check DB was updated with sidecar lyrics and source
        let cached: (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT lyrics, lyrics_source FROM songs WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(cached.0, Some("[00:10.00] Sidecar lyrics line".to_string()));
        assert_eq!(cached.1, Some("sidecar".to_string()));
    }

    #[tokio::test]
    async fn test_get_lyrics_for_song_embedded_on_demand_when_db_empty() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(temp_dir.path().to_path_buf()).unwrap();
        let audio_path = temp_dir.path().join("track.flac");
        std::fs::copy("tests/fixtures/audio/song_gamma.flac", &audio_path).unwrap();

        // Write VorbisComments containing SYNCEDLYRICS into the FLAC file
        let mut file = std::fs::File::open(&audio_path).unwrap();
        let mut flac_file =
            lofty::flac::FlacFile::read_from(&mut file, lofty::config::ParseOptions::default())
                .unwrap();
        drop(file);

        let mut vorbis = lofty::ogg::tag::VorbisComments::default();
        let synced = "[00:10.00] On-demand embedded synced lyric";
        vorbis.insert(String::from("SYNCEDLYRICS"), synced.to_string());
        flac_file.set_vorbis_comments(vorbis);
        flac_file
            .save_to_path(&audio_path, lofty::config::WriteOptions::default())
            .unwrap();

        let conn = db.pool.get().unwrap();
        // Insert song with lyrics = NULL (as if scanned by an earlier Luminous version)
        conn.execute(
            "INSERT INTO songs (id, title, artist, path, source, filetype, unavailable)
             VALUES (1, 'Track', 'Artist', ?1, 1, 1, 0)",
            rusqlite::params![audio_path.to_str().unwrap()],
        )
        .unwrap();

        let lyrics_manager = LyricsManager::new();
        let result = get_lyrics_for_song(&db, &lyrics_manager, 1, false).await;
        assert_eq!(
            result,
            Ok(LyricsPayload {
                text: synced.to_string(),
                source: LyricsSource::Embedded,
            })
        );

        // Check DB was updated with the embedded lyrics and source
        let cached: (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT lyrics, lyrics_source FROM songs WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(cached.0, Some(synced.to_string()));
        assert_eq!(cached.1, Some("embedded".to_string()));
    }

    #[tokio::test]
    async fn test_get_lyrics_for_song_embedded_plain_text_offline() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(temp_dir.path().to_path_buf()).unwrap();
        let audio_path = temp_dir.path().join("plain.flac");
        std::fs::copy("tests/fixtures/audio/song_gamma.flac", &audio_path).unwrap();

        let mut file = std::fs::File::open(&audio_path).unwrap();
        let mut flac_file =
            lofty::flac::FlacFile::read_from(&mut file, lofty::config::ParseOptions::default())
                .unwrap();
        drop(file);

        let mut vorbis = lofty::ogg::tag::VorbisComments::default();
        let plain_lyrics = "Plain text verse line 1\nPlain text verse line 2";
        vorbis.insert(String::from("LYRICS"), plain_lyrics.to_string());
        flac_file.set_vorbis_comments(vorbis);
        flac_file
            .save_to_path(&audio_path, lofty::config::WriteOptions::default())
            .unwrap();

        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('context_enrichment_enabled', 'false')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO songs (id, title, artist, path, source, filetype, unavailable)
             VALUES (1, 'Track', 'Artist', ?1, 1, 1, 0)",
            rusqlite::params![audio_path.to_str().unwrap()],
        )
        .unwrap();

        let lyrics_manager = LyricsManager::new();
        let result = get_lyrics_for_song(&db, &lyrics_manager, 1, false).await;
        assert_eq!(
            result,
            Ok(LyricsPayload {
                text: plain_lyrics.to_string(),
                source: LyricsSource::Embedded,
            })
        );

        let cached: (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT lyrics, lyrics_source FROM songs WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(cached.0, Some(plain_lyrics.to_string()));
        assert_eq!(cached.1, Some("embedded".to_string()));
    }

    /// Offline master toggle (#1398): sidecar/cached lyrics still resolve; with
    /// neither, the lookup stops before any online provider is queried.
    #[tokio::test]
    async fn test_get_lyrics_for_song_offline_never_queries_online() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(temp_dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('context_enrichment_enabled', 'false')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO songs (id, title, artist, path, source, filetype, unavailable)
             VALUES (1, 'Track', 'Artist', ?1, 1, 1, 0)",
            rusqlite::params![temp_dir.path().join("missing.flac").to_str().unwrap()],
        )
        .unwrap();

        let result = get_lyrics_for_song(&db, &LyricsManager::new(), 1, false).await;
        assert_eq!(result, Err(OFFLINE_LYRICS_ERROR.to_string()));

        // Cached lyrics are still served, and the cache is left unmarked.
        conn.execute("UPDATE songs SET lyrics = 'Cached line' WHERE id = 1", [])
            .unwrap();
        let result = get_lyrics_for_song(&db, &LyricsManager::new(), 1, true).await;
        assert_eq!(
            result,
            Ok(LyricsPayload {
                text: "Cached line".to_string(),
                source: LyricsSource::Unknown,
            })
        );
        let cached: Option<String> = conn
            .query_row("SELECT lyrics FROM songs WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cached, Some("Cached line".to_string()));
    }

    #[test]
    fn test_lyrics_source_conversions_and_serialization() {
        let sources = [
            (LyricsSource::Embedded, "embedded"),
            (LyricsSource::Sidecar, "sidecar"),
            (LyricsSource::Lrclib, "lrclib"),
            (LyricsSource::Netease, "netease"),
            (LyricsSource::LyricsOvh, "lyrics_ovh"),
            (LyricsSource::Manual, "manual"),
            (LyricsSource::Unknown, "unknown"),
        ];

        for (source, string_val) in sources {
            assert_eq!(source.as_str(), string_val);
            assert_eq!(LyricsSource::from(string_val), source);
            assert_eq!(string_val.parse::<LyricsSource>().unwrap(), source);
            let json = serde_json::to_string(&source).unwrap();
            assert_eq!(json, format!("\"{string_val}\""));
            let deserialized: LyricsSource = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, source);
        }

        assert_eq!(LyricsSource::from("other_val"), LyricsSource::Unknown);
        assert_eq!(
            "other_val".parse::<LyricsSource>().unwrap(),
            LyricsSource::Unknown
        );
        assert_eq!(
            LyricsSource::from(LyricsProvider::LrcLib),
            LyricsSource::Lrclib
        );
        assert_eq!(
            LyricsSource::from(LyricsProvider::NetEase),
            LyricsSource::Netease
        );
        assert_eq!(
            LyricsSource::from(LyricsProvider::LyricsOvh),
            LyricsSource::LyricsOvh
        );
    }
}
