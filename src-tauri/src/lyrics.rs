//! Online lyrics lookup — tries LRCLIB (synced `.lrc` lyrics, preferred)
//! before falling back to Lyrics.ovh (plain text only). See
//! `LyricsManager::fetch_lyrics` for the fallback chain.

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

    /// Search LRCLIB and Lyrics.ovh in priority order, returning the first
    /// **synced** result found (short-circuits immediately). If none of the
    /// providers return synced lyrics, falls back to whichever plain-text
    /// result was found first rather than failing outright. Errs only if
    /// every provider — including retries with the title's "(feat. ...)"
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

        // 2. Try Lyrics.ovh fallback (only needs artist & title, returns plain text)
        if let Ok(lyrics) = self.fetch_lyrics_ovh(artist, title).await {
            if is_synced_lrc(&lyrics) {
                return Ok(lyrics);
            }
            if best_lyrics.is_none() {
                best_lyrics = Some(lyrics);
            }
        }

        // 3. Clean title of featured artist annotations (e.g., "(feat. ...)") and retry online search
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
    let mut parts = effective_stem.splitn(2, |c: char| c == '-' || c == '_' || c == ' ');
    if let Some(first) = parts.next() {
        if let Ok(num) = first.trim().parse::<i32>() {
            let title = parts.next().map(|t| {
                t.trim_start_matches(|c: char| c == '-' || c == '_' || c == ' ')
                    .trim()
                    .to_string()
            });
            return (Some(num), title);
        }
    }
    (None, None)
}

/// Look for a sidecar `.lrc` file in the same directory as `audio_path`.
///
/// Precedence:
/// 1. Direct match: `<audio_stem>.lrc` and `<audio_stem>.LRC`.
/// 2. Disc prefix strip: e.g. `1-01 Track.flac` -> `01 - Track.lrc`, `01 Track.lrc`.
/// 3. Sibling directory scan: matches files in the parent folder ending with `.lrc`/`.LRC`
///    against track number and/or title (e.g. `Artist - Album - 02 Track.lrc`, `Artist_Album_01_Track.lrc`).
pub fn find_sidecar_lrc(
    audio_path: &Path,
    title: Option<&str>,
    track: Option<i32>,
) -> Option<PathBuf> {
    // 1. Direct match with .lrc or .LRC
    let direct_lrc = audio_path.with_extension("lrc");
    if direct_lrc.is_file() {
        return Some(direct_lrc);
    }
    let direct_upper = audio_path.with_extension("LRC");
    if direct_upper.is_file() {
        return Some(direct_upper);
    }

    let parent = audio_path.parent()?;
    let stem = audio_path.file_stem()?.to_str()?;

    // 2. Try disc-prefix stripped candidates directly if filename starts with disc number
    // e.g. "1-02 Big Guns" or "1.02 Big Guns" -> track "02", title "Big Guns"
    let mut candidate_stems: Vec<String> = Vec::new();

    if let Some(stripped) = strip_disc_prefix(stem) {
        candidate_stems.push(stripped.to_string());
        let trimmed_leading = stripped
            .trim_start_matches(|c: char| c == '-' || c == '_' || c == ' ')
            .trim();
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

    for cand_stem in &candidate_stems {
        let p_lrc = parent.join(format!("{cand_stem}.lrc"));
        if p_lrc.is_file() {
            return Some(p_lrc);
        }
        let p_upper = parent.join(format!("{cand_stem}.LRC"));
        if p_upper.is_file() {
            return Some(p_upper);
        }
    }

    // 3. Scan sibling files in parent directory for tag-prefixed conventions
    // (e.g. "Dorothy - Gifts From the Holy Ghost - 02 Big Guns.lrc")
    let entries = std::fs::read_dir(parent).ok()?;
    let mut lrc_files: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("lrc") {
                    lrc_files.push(path);
                }
            }
        }
    }

    if lrc_files.is_empty() {
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

    let mut best_match: Option<(u8, PathBuf)> = None;

    for lrc_path in lrc_files {
        let Some(lrc_stem) = lrc_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let lrc_norm = normalize_stem(lrc_stem);
        let lrc_tokens: Vec<&str> = lrc_norm.split_whitespace().collect();

        let track_matches = target_track_num.map_or(false, |trk| {
            let trk_2 = format!("{:02}", trk);
            let trk_1 = format!("{}", trk);
            lrc_tokens.iter().any(|&tok| tok == trk_2 || tok == trk_1)
        });

        let title_matches = target_title_norm.as_ref().map_or(false, |norm_title| {
            if norm_title.is_empty() {
                return false;
            }
            if lrc_norm.contains(norm_title) {
                return true;
            }
            let title_tokens: Vec<&str> = norm_title.split_whitespace().collect();
            if !title_tokens.is_empty() && title_tokens.iter().all(|&tt| lrc_tokens.contains(&tt)) {
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
            if let Some((best_score, _)) = best_match {
                if score > best_score {
                    best_match = Some((score, lrc_path));
                }
            } else {
                best_match = Some((score, lrc_path));
            }
        }
    }

    best_match.map(|(_, path)| path)
}

/// Read and return the contents of a sidecar `.lrc` file if one exists next to `audio_path`.
/// Handles UTF-8 BOM if present, strips trailing/leading whitespace, and ignores empty files.
/// Synced LRC content is returned as-is; plain text is marked with `[synced:false]\n`.
pub fn read_sidecar_lrc(
    audio_path: &Path,
    title: Option<&str>,
    track: Option<i32>,
) -> Option<String> {
    let lrc_path = find_sidecar_lrc(audio_path, title, track)?;
    let content = std::fs::read_to_string(&lrc_path).ok()?;
    let trimmed = content.strip_prefix('\u{feff}').unwrap_or(&content).trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_synced_lrc(trimmed) || trimmed.starts_with("[synced:false]") {
        Some(trimmed.to_string())
    } else {
        Some(format!("[synced:false]\n{trimmed}"))
    }
}

/// Resolve lyrics for `song_id`: return cached lyrics immediately when the
/// cache holds synced (or already-checked-unsynced) text, otherwise query
/// online providers via `lyrics_manager` and cache the result. Extracted
/// from the `get_lyrics` Tauri command so it's callable without a Tauri
/// `AppHandle`/`State` — BDD tests exercise this directly (see
/// `tests/lyrics_bdd.rs`) to verify the cache-hit path never reaches
/// `lyrics_manager.fetch_lyrics`.
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

    // 2. Check for local sidecar .lrc file next to the audio file (#155)
    if let Some(ref path_str) = path_str {
        let audio_path = Path::new(path_str);
        if let Some(sidecar_lyrics) = read_sidecar_lrc(audio_path, title.as_deref(), track) {
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

    // 3. Query online APIs (LRCLIB -> Lyrics.ovh)
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
            find_sidecar_lrc(&audio_path, None, None),
            Some(lrc_path.clone())
        );

        // Case-insensitive / uppercase .LRC
        std::fs::remove_file(&lrc_path).unwrap();
        let lrc_upper = temp_dir.path().join("song.LRC");
        std::fs::write(&lrc_upper, b"[00:01.00] test upper").unwrap();
        let found = find_sidecar_lrc(&audio_path, None, None);
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
        let found = find_sidecar_lrc(&audio_path, Some("Big Guns"), Some(2));
        assert_eq!(found, Some(lrc_path.clone()));

        // Should also find via extracted stem info when metadata is not provided
        let found_from_stem = find_sidecar_lrc(&audio_path, None, None);
        assert_eq!(found_from_stem, Some(lrc_path));

        // Different track should NOT match
        let other_audio = temp_dir.path().join("1-01 A Beautiful Life.flac");
        std::fs::write(&other_audio, b"dummy audio").unwrap();
        let not_found = find_sidecar_lrc(&other_audio, Some("A Beautiful Life"), Some(1));
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_read_sidecar_lrc_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let audio_path = temp_dir.path().join("track.mp3");
        let lrc_path = temp_dir.path().join("track.lrc");
        std::fs::write(&audio_path, b"dummy").unwrap();

        // Synced LRC with UTF-8 BOM
        let bom_lrc = format!("\u{feff}[00:15.00] Synced line\n[00:20.00] Next line");
        std::fs::write(&lrc_path, bom_lrc.as_bytes()).unwrap();
        let read = read_sidecar_lrc(&audio_path, None, None);
        assert_eq!(
            read,
            Some("[00:15.00] Synced line\n[00:20.00] Next line".to_string())
        );

        // Plain text LRC (no timestamps) should get [synced:false] prefix
        std::fs::write(&lrc_path, b"Just plain text lyrics").unwrap();
        let read_plain = read_sidecar_lrc(&audio_path, None, None);
        assert_eq!(
            read_plain,
            Some("[synced:false]\nJust plain text lyrics".to_string())
        );

        // Empty file should return None
        std::fs::write(&lrc_path, b"   \n\t  ").unwrap();
        assert_eq!(read_sidecar_lrc(&audio_path, None, None), None);
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
}
