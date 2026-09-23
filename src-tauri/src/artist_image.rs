//! Artist profile image fetch (#1127): resolves an artist's MusicBrainz ID to
//! a portrait image via fanart.tv's artist-images API (if an API key is
//! configured) or, lacking a key or a match, Wikidata's `P18` (image)
//! property — the same two-tier "best source, then a no-key-required
//! fallback" shape `context.rs` uses elsewhere. Distinct from the locally-
//! discovered artist artwork in `covermanager.rs` (`ArtistPortrait`/
//! `BandLogo`/`FanartBanner`): those are scanned from files already sitting
//! next to the artist's music, this is fetched from the network on demand.

use crate::context::ContextManager;
use crate::covermanager::detect_image_format_and_clean;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

/// Which source a fetched artist image came from — persisted alongside the
/// cached filename (`artist_profiles.fetched_image_source`) so the UI can
/// label a Wikidata fallback image as such (often lower quality/relevance
/// than a curated fanart.tv pick).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistImageSource {
    Fanart,
    Wikidata,
}

impl ArtistImageSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtistImageSource::Fanart => "fanart",
            ArtistImageSource::Wikidata => "wikidata",
        }
    }
}

#[derive(Deserialize, Debug, Default)]
struct FanartImageEntry {
    url: Option<String>,
    /// Numeric string in fanart.tv's wire format (e.g. `"12"`), not a number.
    #[serde(default)]
    likes: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct FanartArtistResponse {
    #[serde(default)]
    artistthumb: Vec<FanartImageEntry>,
}

/// Picks fanart.tv's highest-liked `artistthumb` entry, if any. An
/// unparseable/missing `likes` value defaults to 0 rather than dropping the
/// entry — fanart.tv's own UI treats a missing like-count the same way.
fn best_fanart_image(entries: Vec<FanartImageEntry>) -> Option<String> {
    entries
        .into_iter()
        .filter_map(|e| {
            let url = e.url?;
            let likes = e.likes.and_then(|l| l.parse::<u32>().ok()).unwrap_or(0);
            Some((url, likes))
        })
        .max_by_key(|(_, likes)| *likes)
        .map(|(url, _)| url)
}

/// Looks up an artist's top fanart.tv `artistthumb` image URL. `Ok(None)` —
/// not an error — both on a 404 (fanart.tv has no entry for this MBID) and
/// when the entry exists but has no `artistthumb` images.
pub async fn fetch_fanart_artist_image_url(
    client: &Client,
    artist_mbid: &str,
    api_key: &str,
) -> Result<Option<String>> {
    let url = format!(
        "https://webservice.fanart.tv/v3/music/{}?api_key={}",
        percent_encoding::utf8_percent_encode(artist_mbid, percent_encoding::NON_ALPHANUMERIC),
        percent_encoding::utf8_percent_encode(api_key, percent_encoding::NON_ALPHANUMERIC)
    );
    let response = client.get(&url).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(anyhow!(
            "fanart.tv artist image lookup failed: HTTP {}",
            response.status()
        ));
    }
    let parsed: FanartArtistResponse = response.json().await?;
    Ok(best_fanart_image(parsed.artistthumb))
}

/// MBID of an artist near-certain to have fanart.tv images, used purely to
/// exercise an API key against a real endpoint — its images are discarded.
const KEY_VALIDATION_PROBE_ARTIST_MBID: &str = "5b11f4ce-a62d-471e-81fc-a69a8278c7da"; // Nirvana

/// Classifies a fanart.tv response status for key validation purposes: any
/// success — even a miss (`404`) on the probe artist — proves the key was
/// accepted, while a `401`/`403` gets a plain "invalid key" message rather
/// than a raw HTTP status. Pure so it's unit-testable without a live request.
fn classify_validation_status(status: reqwest::StatusCode) -> Result<()> {
    match status {
        s if s.is_success() || s == reqwest::StatusCode::NOT_FOUND => Ok(()),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            Err(anyhow!("Invalid fanart.tv API key"))
        }
        s => Err(anyhow!("fanart.tv validation failed: HTTP {s}")),
    }
}

/// Validates a fanart.tv API key for the Settings UI's "Validate & Save"
/// button — same gate the ListenBrainz token field uses.
pub async fn validate_fanart_api_key(client: &Client, api_key: &str) -> Result<()> {
    let url = format!(
        "https://webservice.fanart.tv/v3/music/{}?api_key={}",
        KEY_VALIDATION_PROBE_ARTIST_MBID,
        percent_encoding::utf8_percent_encode(api_key, percent_encoding::NON_ALPHANUMERIC)
    );
    let response = client.get(&url).send().await?;
    classify_validation_status(response.status())
}

/// Wikidata `P18` fallback — reuses the same MBID -> Wikidata QID chain
/// `ContextManager::fetch_wikipedia_bio_for_artist` uses to resolve an
/// artist's bio, but stopping at the QID's image claim instead of its
/// Wikipedia sitelink. Returns `Ok(None)` when the artist has no Wikidata
/// relation, or the Wikidata entity has no `P18` claim.
pub async fn fetch_wikidata_artist_image_url(
    context: &ContextManager,
    artist_mbid: &str,
) -> Result<Option<String>> {
    let Some(wikidata_id) = context
        .fetch_musicbrainz_artist_wikidata_id(artist_mbid)
        .await?
    else {
        return Ok(None);
    };
    let Some(filename) = context.fetch_wikidata_image_filename(&wikidata_id).await? else {
        return Ok(None);
    };
    Ok(Some(format!(
        "https://commons.wikimedia.org/wiki/Special:FilePath/{}?width=800",
        percent_encoding::utf8_percent_encode(&filename, percent_encoding::NON_ALPHANUMERIC)
    )))
}

/// Downloads `image_url` and writes it into `dest_dir` as
/// `{filename_stem}.{ext}` — same format-sniffing convention `CoverManager`
/// uses to cache cover art — returning the cache filename.
pub async fn download_and_cache_artist_image(
    client: &Client,
    image_url: &str,
    dest_dir: &Path,
    filename_stem: &str,
) -> Result<String> {
    let bytes = client.get(image_url).send().await?.bytes().await?;
    let (cleaned, _mime, ext) = detect_image_format_and_clean(&bytes);
    let filename = format!("{filename_stem}.{ext}");
    std::fs::write(dest_dir.join(&filename), cleaned)?;
    Ok(filename)
}

/// Shared HTTP client for fanart.tv/Wikimedia Commons requests — same
/// timeout/user-agent convention as `ContextManager::new()`.
pub fn new_http_client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("LuminousMusicPlayer/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_best_fanart_image_picks_highest_likes() {
        let entries = vec![
            FanartImageEntry {
                url: Some("https://a".to_string()),
                likes: Some("2".to_string()),
            },
            FanartImageEntry {
                url: Some("https://b".to_string()),
                likes: Some("10".to_string()),
            },
            FanartImageEntry {
                url: Some("https://c".to_string()),
                likes: Some("5".to_string()),
            },
        ];
        assert_eq!(best_fanart_image(entries), Some("https://b".to_string()));
    }

    #[test]
    fn test_best_fanart_image_empty_returns_none() {
        assert_eq!(best_fanart_image(vec![]), None);
    }

    #[test]
    fn test_best_fanart_image_skips_entries_with_no_url() {
        let entries = vec![
            FanartImageEntry {
                url: None,
                likes: Some("99".to_string()),
            },
            FanartImageEntry {
                url: Some("https://only-valid".to_string()),
                likes: Some("1".to_string()),
            },
        ];
        assert_eq!(
            best_fanart_image(entries),
            Some("https://only-valid".to_string())
        );
    }

    #[test]
    fn test_best_fanart_image_treats_unparseable_or_missing_likes_as_zero() {
        // Neither entry has a usable like-count, so both fall back to 0 —
        // the important thing is a URL is still returned rather than the
        // whole lookup coming back empty.
        let entries = vec![
            FanartImageEntry {
                url: Some("https://a".to_string()),
                likes: Some("not-a-number".to_string()),
            },
            FanartImageEntry {
                url: Some("https://b".to_string()),
                likes: None,
            },
        ];
        assert!(best_fanart_image(entries).is_some());
    }

    #[test]
    fn test_artist_image_source_as_str() {
        assert_eq!(ArtistImageSource::Fanart.as_str(), "fanart");
        assert_eq!(ArtistImageSource::Wikidata.as_str(), "wikidata");
    }

    #[test]
    fn test_classify_validation_status_accepts_success_and_not_found() {
        assert!(classify_validation_status(reqwest::StatusCode::OK).is_ok());
        // A miss on the probe artist still proves the key itself was accepted.
        assert!(classify_validation_status(reqwest::StatusCode::NOT_FOUND).is_ok());
    }

    #[test]
    fn test_classify_validation_status_rejects_unauthorized_and_forbidden() {
        let unauthorized = classify_validation_status(reqwest::StatusCode::UNAUTHORIZED);
        assert!(unauthorized.is_err());
        assert_eq!(
            unauthorized.unwrap_err().to_string(),
            "Invalid fanart.tv API key"
        );

        let forbidden = classify_validation_status(reqwest::StatusCode::FORBIDDEN);
        assert!(forbidden.is_err());
        assert_eq!(
            forbidden.unwrap_err().to_string(),
            "Invalid fanart.tv API key"
        );
    }

    #[test]
    fn test_classify_validation_status_surfaces_other_errors_as_http_status() {
        let result = classify_validation_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "fanart.tv validation failed: HTTP 500 Internal Server Error"
        );
    }
}
