//! TunePing live now-playing search client (#1998).
//!
//! TunePing (<https://tuneping.com/docs/>) reads the public title line of
//! internet radio streams and answers "which stations are playing this right
//! now". This client wraps the one keyless endpoint Luminous uses,
//! `GET /v1/now-playing/search`, and reports failures as [`TunepingError`] so the
//! app-side service (`src-tauri/src/airplay.rs`) can back off without seeing
//! HTTP details. Matching rows against a library song is the service's job, not
//! this client's.
//!
//! Must never depend on `tauri` or `rusqlite` (#1767, epic #1762).

use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER, USER_AGENT};
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use std::time::Duration;

/// Production API base URL.
pub const TUNEPING_BASE_URL: &str = "https://api.tuneping.com/v1";

/// Attribution link the docs require wherever free or anonymous data is shown.
pub const TUNEPING_ATTRIBUTION_URL: &str = "https://tuneping.com/";

/// Inclusive bounds of the `q` parameter's length, in characters.
const QUERY_MIN_CHARS: usize = 2;
const QUERY_MAX_CHARS: usize = 120;

/// How recently a station must have been polled for its row to count, in seconds
/// (the API accepts 30–900).
const MAX_AGE_SECONDS: u32 = 180;

/// Rows requested per search. Several rows can be the same station on different
/// streams, and rows that don't match the song are dropped app-side, so ask for
/// more than the eight the UI shows.
const SEARCH_LIMIT: u32 = 50;

/// Back-off used when a 429 or 503 carries no usable `Retry-After`.
const DEFAULT_RETRY_AFTER_SECS: u64 = 60;

/// A station in a TunePing row. `id` is the station's Radio Browser UUID.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TunepingStation {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub bitrate: Option<i32>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub stream_url: Option<String>,
}

/// The MusicBrainz link of a row, present when TunePing matched the title to a recording.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TunepingTrack {
    #[serde(default)]
    pub mbid: Option<String>,
}

/// One "station S is playing artist – title" row from a live search.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NowPlayingRow {
    pub station: TunepingStation,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub track: Option<TunepingTrack>,
    /// RFC 3339 time of the station's latest successful poll: "still playing as of".
    #[serde(default)]
    pub confirmed_at: Option<String>,
}

impl NowPlayingRow {
    /// The MusicBrainz recording ID TunePing linked this title to, if any.
    pub fn recording_mbid(&self) -> Option<&str> {
        self.track
            .as_ref()
            .and_then(|t| t.mbid.as_deref())
            .map(str::trim)
            .filter(|m| !m.is_empty())
    }
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<NowPlayingRow>,
}

/// Why a search produced no rows. Every variant means "show the last result, marked stale".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunepingError {
    /// 429 (`rate_limited` or `quota_exceeded`) or 503: don't ask again for `retry_after_secs`.
    Throttled { retry_after_secs: u64 },
    /// Offline, timed out, another status, or an unreadable body.
    Unavailable(String),
}

/// Builds the `q` value for an artist and title: "artist title", trimmed and cut to
/// 120 characters on a character boundary. `None` when it would be under two characters.
pub fn build_query(artist: &str, title: &str) -> Option<String> {
    let joined = format!("{} {}", artist.trim(), title.trim());
    let joined = joined.trim();
    let clamped: String = joined.chars().take(QUERY_MAX_CHARS).collect();
    let clamped = clamped.trim_end().to_string();
    (clamped.chars().count() >= QUERY_MIN_CHARS).then_some(clamped)
}

/// Seconds to wait from a `Retry-After` header given in delta-seconds (the form TunePing sends).
fn retry_after_secs(headers: &HeaderMap) -> u64 {
    headers
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&s| s > 0)
        .unwrap_or(DEFAULT_RETRY_AFTER_SECS)
}

/// Client for the TunePing live search endpoint.
pub struct TunepingClient {
    client: Client,
    base_url: String,
}

impl TunepingClient {
    /// A client for the production API, sending `user_agent` (e.g. `"Luminous/4.1.0"`).
    pub fn new(user_agent: &str) -> Self {
        Self::with_base_url(TUNEPING_BASE_URL, user_agent)
    }

    /// A client for another base URL (tests point this at a fake server).
    pub fn with_base_url(base_url: &str, user_agent: &str) -> Self {
        let mut headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(user_agent.trim()) {
            headers.insert(USER_AGENT, val);
        }
        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(8))
            .build()
            .unwrap_or_default();
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Stations whose current title matches `artist` and `title`, as TunePing ranks
    /// them (fuzzy; the caller verifies each row). Sends only the documented
    /// parameters, since TunePing rejects unknown ones. An artist and title too short
    /// to search for answer an empty list without a request.
    pub async fn search_now_playing(
        &self,
        artist: &str,
        title: &str,
    ) -> Result<Vec<NowPlayingRow>, TunepingError> {
        let Some(q) = build_query(artist, title) else {
            return Ok(Vec::new());
        };
        let url = Url::parse_with_params(
            &format!("{}/now-playing/search", self.base_url),
            &[
                ("q", q.as_str()),
                ("max_age_seconds", &MAX_AGE_SECONDS.to_string()),
                ("limit", &SEARCH_LIMIT.to_string()),
            ],
        )
        .map_err(|e| TunepingError::Unavailable(e.to_string()))?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TunepingError::Unavailable(e.to_string()))?;

        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE {
            return Err(TunepingError::Throttled {
                retry_after_secs: retry_after_secs(response.headers()),
            });
        }
        if !status.is_success() {
            return Err(TunepingError::Unavailable(format!("HTTP {status}")));
        }
        response
            .json::<SearchResponse>()
            .await
            .map(|r| r.data)
            .map_err(|e| TunepingError::Unavailable(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const ROW: &str = r#"{
      "station": { "id": "159824f0-5475-4263-bcc8-59a48a213896", "name": "Radio 10 80's Hits", "country": "NL",
                   "codec": "MP3", "bitrate": 128, "is_hls": false,
                   "stream_url": "http://example.test/s.mp3", "homepage": "http://www.radio10.nl/", "recordability": "recordable" },
      "raw_title": "Madonna - Holiday",
      "artist": "Madonna", "title": "Holiday",
      "track": { "id": "trk_c6f", "mbid": "d4708b79-62e7-4ce4-a6ca-247237d4ef4e", "artist_id": "art_fn" },
      "classification": "song", "match_grade": "high", "confidence": "high",
      "started_at": "2026-10-29T15:08:44.883Z", "start_uncertainty_seconds": 32,
      "observed_at": "2026-10-29T15:09:26.631Z", "confirmed_at": "2026-10-29T15:10:30.679Z",
      "method": "icy", "poll_interval_seconds": 60, "similarity": 0.5
    }"#;

    #[test]
    fn query_joins_artist_and_title() {
        assert_eq!(
            build_query(" Madonna ", "Holiday "),
            Some("Madonna Holiday".into())
        );
    }

    #[test]
    fn query_is_cut_to_120_characters_on_a_char_boundary() {
        let title = "é".repeat(200);
        let q = build_query("Björk", &title).unwrap();
        assert_eq!(q.chars().count(), 120);
        assert!(q.starts_with("Björk é"));
    }

    #[test]
    fn query_too_short_is_none() {
        assert_eq!(build_query("", "a"), None);
        assert_eq!(build_query(" ", " "), None);
        assert_eq!(build_query("", "ab"), Some("ab".into()));
    }

    #[tokio::test]
    async fn search_sends_only_documented_params_and_parses_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/now-playing/search"))
            .and(query_param("q", "Madonna Holiday"))
            .and(query_param("max_age_seconds", "180"))
            .and(query_param("limit", "50"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                format!(r#"{{"data":[{ROW}],"truncated":false}}"#),
                "application/json",
            ))
            .expect(1)
            .mount(&server)
            .await;

        let client = TunepingClient::with_base_url(&server.uri(), "Luminous/test");
        let rows = client
            .search_now_playing("Madonna", "Holiday")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.station.id, "159824f0-5475-4263-bcc8-59a48a213896");
        assert_eq!(row.station.bitrate, Some(128));
        assert_eq!(row.artist.as_deref(), Some("Madonna"));
        assert_eq!(
            row.recording_mbid(),
            Some("d4708b79-62e7-4ce4-a6ca-247237d4ef4e")
        );
        assert_eq!(
            row.confirmed_at.as_deref(),
            Some("2026-10-29T15:10:30.679Z")
        );

        let requests = server.received_requests().await.unwrap();
        let pairs: Vec<String> = requests[0]
            .url
            .query_pairs()
            .map(|(k, _)| k.into_owned())
            .collect();
        assert_eq!(pairs, vec!["q", "max_age_seconds", "limit"]);
    }

    #[tokio::test]
    async fn quota_exceeded_reports_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/now-playing/search"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "7200")
                    .set_body_raw(
                        r#"{"status":429,"code":"quota_exceeded","title":"Quota exceeded"}"#,
                        "application/problem+json",
                    ),
            )
            .mount(&server)
            .await;

        let client = TunepingClient::with_base_url(&server.uri(), "Luminous/test");
        assert_eq!(
            client.search_now_playing("Madonna", "Holiday").await,
            Err(TunepingError::Throttled {
                retry_after_secs: 7200
            })
        );
    }

    #[tokio::test]
    async fn throttle_without_retry_after_uses_the_default() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = TunepingClient::with_base_url(&server.uri(), "Luminous/test");
        assert_eq!(
            client.search_now_playing("Madonna", "Holiday").await,
            Err(TunepingError::Throttled {
                retry_after_secs: DEFAULT_RETRY_AFTER_SECS
            })
        );
    }

    #[tokio::test]
    async fn other_errors_are_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;

        let client = TunepingClient::with_base_url(&server.uri(), "Luminous/test");
        assert!(matches!(
            client.search_now_playing("Madonna", "Holiday").await,
            Err(TunepingError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn too_short_query_makes_no_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let client = TunepingClient::with_base_url(&server.uri(), "Luminous/test");
        assert_eq!(client.search_now_playing("", "x").await, Ok(Vec::new()));
    }
}
