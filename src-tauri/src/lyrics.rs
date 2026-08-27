//! Online lyrics lookup — tries LRCLIB (synced `.lrc` lyrics, preferred)
//! before falling back to Lyrics.ovh (plain text only). See
//! `LyricsManager::fetch_lyrics` for the fallback chain.

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
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
    let (cached_lyrics, is_instrumental): (Option<String>, bool) = conn
        .query_row(
            "SELECT lyrics, COALESCE(is_instrumental, 0) FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or((None, false));

    if is_instrumental {
        return Err("Song is marked as instrumental".to_string());
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

    // 2. Fetch metadata from DB to search online
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
}
