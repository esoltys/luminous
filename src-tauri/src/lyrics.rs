use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize, Debug)]
pub struct LrcLibResponse {
    pub _id: Option<i64>,
    #[serde(rename = "syncedLyrics")]
    pub synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    pub plain_lyrics: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct LyricsOvhResponse {
    pub lyrics: Option<String>,
}

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

    /// Primary search chain: query LRCLIB (for synced/plain), fallback to Lyrics.ovh (for plain).
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
