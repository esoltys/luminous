//! Online lyrics lookup client (#1767).
//!
//! Tries LRCLIB (synced `.lrc` lyrics, preferred) and NetEase Cloud Music
//! before falling back to Lyrics.ovh (plain text only).

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize, Debug)]
pub(crate) struct LrcLibResponse {
    pub _id: Option<i64>,
    #[serde(rename = "syncedLyrics")]
    pub synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    pub plain_lyrics: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct LyricsOvhResponse {
    pub lyrics: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct NetEaseSearchResponse {
    pub result: Option<NetEaseSearchResult>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct NetEaseSearchResult {
    pub songs: Option<Vec<NetEaseSong>>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub(crate) struct NetEaseSong {
    pub id: i64,
    pub name: Option<String>,
    pub ar: Option<Vec<NetEaseArtist>>,
    pub dt: Option<i64>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub(crate) struct NetEaseArtist {
    pub name: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct NetEaseLyricResponse {
    pub lrc: Option<NetEaseLyricData>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct NetEaseLyricData {
    pub lyric: Option<String>,
}

/// NetEase search results only count as a match when their duration is within
/// this many milliseconds of the local file's.
const NETEASE_DURATION_TOLERANCE_MS: i64 = 8000;

/// Picks the search result whose duration is closest to `duration_sec`.
pub(crate) fn pick_netease_song(songs: &[NetEaseSong], duration_sec: u32) -> Option<&NetEaseSong> {
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

/// Returns true if `text` contains at least one standard LRC timestamp tag (`[MM:SS`).
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

/// Clean featured artist annotations (e.g., "(feat. ...)", "[ft. ...]") from title.
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

/// Holds the HTTP client used for provider requests.
#[derive(Clone)]
pub struct LyricsClient {
    client: Client,
}

pub type LyricsManager = LyricsClient;

impl Default for LyricsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(6))
                .user_agent(concat!("LuminousMusicPlayer/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// Search LRCLIB, NetEase Cloud Music, and Lyrics.ovh in priority order,
    /// returning the first **synced** result found (short-circuits immediately).
    /// If none of the providers return synced lyrics, falls back to whichever
    /// plain-text result was found first rather than failing outright.
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

        // 1b. Try LRCLIB fallback (omitting the album)
        if let Ok(lyrics) = self.fetch_lrclib(artist, title, None, duration_sec).await {
            if is_synced_lrc(&lyrics) {
                return Ok(lyrics);
            }
            if best_lyrics.is_none() {
                best_lyrics = Some(lyrics);
            }
        }

        // 2. Try NetEase Cloud Music
        if let Ok(lyrics) = self.fetch_netease(artist, title, duration_sec).await {
            if is_synced_lrc(&lyrics) {
                return Ok(lyrics);
            }
            if best_lyrics.is_none() {
                best_lyrics = Some(lyrics);
            }
        }

        // 3. Try Lyrics.ovh fallback (plain text)
        if let Ok(lyrics) = self.fetch_lyrics_ovh(artist, title).await {
            if is_synced_lrc(&lyrics) {
                return Ok(lyrics);
            }
            if best_lyrics.is_none() {
                best_lyrics = Some(lyrics);
            }
        }

        // 4. Clean title of featured artist annotations and retry
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

    pub async fn fetch_lrclib(
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

    pub async fn fetch_netease(
        &self,
        artist: &str,
        title: &str,
        duration_sec: u32,
    ) -> Result<String> {
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

    pub async fn fetch_lyrics_ovh(&self, artist: &str, title: &str) -> Result<String> {
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
        assert!(is_synced_lrc("[01:23:45] Line of lyrics"));
        assert!(!is_synced_lrc("Plain lyrics line without LRC timestamp"));
        assert!(!is_synced_lrc(""));
        assert!(!is_synced_lrc("[abc]"));
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

    #[test]
    fn test_deserialize_lrclib_response() {
        let json = r#"{"_id": 123, "syncedLyrics": "[00:01.00] Hello", "plainLyrics": "Hello"}"#;
        let res: LrcLibResponse = serde_json::from_str(json).unwrap();
        assert_eq!(res._id, Some(123));
        assert_eq!(res.synced_lyrics.as_deref(), Some("[00:01.00] Hello"));
        assert_eq!(res.plain_lyrics.as_deref(), Some("Hello"));
    }
}
