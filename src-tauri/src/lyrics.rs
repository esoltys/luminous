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

impl LyricsManager {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(6))
                .user_agent("LuminousMusicPlayer/0.1.0")
                .build()
                .unwrap_or_default(),
        }
    }

    /// Primary search chain: query LRCLIB (for synced/plain), fallback to Lyrics.ovh (for plain).
    pub async fn fetch_lyrics(&self, artist: &str, title: &str, album: &str, duration_sec: u32) -> Result<String> {
        // 1. Try LRCLIB primary (highly specific with track, album, and duration)
        if let Ok(lyrics) = self.fetch_lrclib(artist, title, album, duration_sec).await {
            if !lyrics.trim().is_empty() {
                return Ok(lyrics);
            }
        }

        // 2. Try Lyrics.ovh fallback (only needs artist & title, returns plain text)
        if let Ok(lyrics) = self.fetch_lyrics_ovh(artist, title).await {
            if !lyrics.trim().is_empty() {
                return Ok(lyrics);
            }
        }

        Err(anyhow!("no lyrics found on any online provider"))
    }

    async fn fetch_lrclib(&self, artist: &str, title: &str, album: &str, duration_sec: u32) -> Result<String> {
        let url = format!(
            "https://lrclib.net/api/get?artist={}&track={}&album={}&duration={}",
            percent_encoding::utf8_percent_encode(artist, percent_encoding::NON_ALPHANUMERIC),
            percent_encoding::utf8_percent_encode(title, percent_encoding::NON_ALPHANUMERIC),
            percent_encoding::utf8_percent_encode(album, percent_encoding::NON_ALPHANUMERIC),
            duration_sec
        );

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
