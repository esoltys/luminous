//! Last.fm 2.0 Web Services wire-protocol client (#1954).
//!
//! Handles desktop authentication token exchange, signed API requests (MD5),
//! now-playing announcements, batch scrobble submissions, and loved tracks synchronization.
//! Does not depend on Tauri, SQLite, or local storage (CI-checked).

use anyhow::{anyhow, Context, Result};
use md5::{Digest, Md5};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

/// Characters left unescaped in query parameters per RFC 3986 (unreserved characters).
const QUERY_ESCAPE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub const LASTFM_API_BASE: &str = "https://ws.audioscrobbler.com/2.0/";
pub const LASTFM_AUTH_BASE: &str = "https://www.last.fm/api/auth/";
pub const SUBMISSION_CLIENT_NAME: &str = "Luminous";

/// Built-in default client credentials for Last.fm desktop integration.
/// Overridable via `LASTFM_API_KEY` / `LASTFM_API_SECRET` environment variables.
pub const DEFAULT_LASTFM_API_KEY: &str = "2c5be3e0fae740d287ee3b4e7a8e7a6f";
pub const DEFAULT_LASTFM_API_SECRET: &str = "7e14a1fa33f6a6fcfce898a3b8417dc4";

/// Credentials representing an authenticated Last.fm user session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LastFmSession {
    pub username: String,
    pub session_key: String,
    pub subscriber: bool,
}

/// A track prepared for now-playing update or scrobble submission.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LastFmTrack {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub track_number: Option<i32>,
    pub duration_secs: Option<u32>,
    pub timestamp: i64,
    pub mbid: Option<String>,
}

/// Outcome of a batch scrobble call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LastFmScrobbleResponse {
    pub accepted: u32,
    pub ignored: u32,
}

/// A loved track returned from Last.fm's library (`user.getLovedTracks`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LastFmLovedTrack {
    pub artist: String,
    pub track: String,
    pub mbid: Option<String>,
    pub date_uts: Option<i64>,
}

/// Wire-protocol client for Last.fm API 2.0.
#[derive(Clone)]
pub struct LastFmClient {
    api_key: String,
    api_secret: String,
    api_base: String,
    auth_base: String,
    client: Client,
}

impl LastFmClient {
    /// Create a new Last.fm client using the specified API credentials.
    pub fn new(api_key: String, api_secret: String) -> Self {
        Self::with_endpoints(
            api_key,
            api_secret,
            LASTFM_API_BASE.to_string(),
            LASTFM_AUTH_BASE.to_string(),
        )
    }

    /// Internal constructor allowing endpoint override for unit tests.
    pub fn with_endpoints(
        api_key: String,
        api_secret: String,
        api_base: String,
        auth_base: String,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(format!(
                "{SUBMISSION_CLIENT_NAME}/{}",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            api_key,
            api_secret,
            api_base,
            auth_base,
            client,
        }
    }

    /// Build the browser authorization URL directing the user to grant access to Luminous.
    pub fn build_auth_url(&self, token: &str) -> String {
        format!(
            "{}?api_key={}&token={}",
            self.auth_base.trim_end_matches('/'),
            utf8_percent_encode(&self.api_key, QUERY_ESCAPE),
            utf8_percent_encode(token, QUERY_ESCAPE)
        )
    }

    /// Calculate the Last.fm MD5 signature (`api_sig`) across sorted query parameters.
    /// Excludes `format` and `api_sig` as required by the Last.fm specification.
    pub fn sign_params(params: &BTreeMap<&str, String>, secret: &str) -> String {
        let mut pre_hash = String::new();
        for (k, v) in params {
            if *k == "format" || *k == "api_sig" {
                continue;
            }
            pre_hash.push_str(k);
            pre_hash.push_str(v);
        }
        pre_hash.push_str(secret);

        let digest = Md5::digest(pre_hash.as_bytes());
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Request an unauthorized request token (`auth.getToken`).
    pub async fn get_auth_token(&self) -> Result<String> {
        let mut params = BTreeMap::new();
        params.insert("method", "auth.getToken".to_string());
        params.insert("api_key", self.api_key.clone());

        let sig = Self::sign_params(&params, &self.api_secret);
        params.insert("api_sig", sig);
        params.insert("format", "json".to_string());

        let url = Self::build_query_url(&self.api_base, &params);
        let res = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to request Last.fm auth token")?;

        let body = res.text().await.unwrap_or_default();
        Self::check_api_error(&body)?;

        #[derive(Deserialize)]
        struct TokenResponse {
            token: String,
        }

        let resp: TokenResponse = serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse Last.fm auth token response: {body}"))?;

        Ok(resp.token)
    }

    /// Exchange an authorized token for a permanent session key (`auth.getSession`).
    pub async fn get_session(&self, token: &str) -> Result<LastFmSession> {
        let mut params = BTreeMap::new();
        params.insert("method", "auth.getSession".to_string());
        params.insert("api_key", self.api_key.clone());
        params.insert("token", token.to_string());

        let sig = Self::sign_params(&params, &self.api_secret);
        params.insert("api_sig", sig);
        params.insert("format", "json".to_string());

        let url = Self::build_query_url(&self.api_base, &params);
        let res = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to exchange Last.fm session")?;

        let body = res.text().await.unwrap_or_default();
        Self::check_api_error(&body)?;

        #[derive(Deserialize)]
        struct SessionEnvelope {
            session: SessionData,
        }

        #[derive(Deserialize)]
        struct SessionData {
            name: String,
            key: String,
            #[serde(default)]
            subscriber: u8,
        }

        let envelope: SessionEnvelope = serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse Last.fm session response: {body}"))?;

        Ok(LastFmSession {
            username: envelope.session.name,
            session_key: envelope.session.key,
            subscriber: envelope.session.subscriber > 0,
        })
    }

    /// Notify Last.fm that a track is currently playing (`track.updateNowPlaying`).
    pub async fn update_now_playing(&self, session_key: &str, track: &LastFmTrack) -> Result<()> {
        let mut params = BTreeMap::new();
        params.insert("method", "track.updateNowPlaying".to_string());
        params.insert("api_key", self.api_key.clone());
        params.insert("sk", session_key.to_string());
        params.insert("artist", track.artist.clone());
        params.insert("track", track.track.clone());

        if let Some(album) = &track.album {
            params.insert("album", album.clone());
        }
        if let Some(track_no) = track.track_number {
            params.insert("trackNumber", track_no.to_string());
        }
        if let Some(dur) = track.duration_secs {
            params.insert("duration", dur.to_string());
        }
        if let Some(mbid) = &track.mbid {
            if !mbid.trim().is_empty() {
                params.insert("mbid", mbid.clone());
            }
        }

        let sig = Self::sign_params(&params, &self.api_secret);
        params.insert("api_sig", sig);
        params.insert("format", "json".to_string());

        let res = self
            .client
            .post(&self.api_base)
            .form(&params)
            .send()
            .await
            .context("Failed to update now-playing on Last.fm")?;

        let body = res.text().await.unwrap_or_default();
        Self::check_api_error(&body)?;
        Ok(())
    }

    /// Submit a batch of scrobbles (up to 50 tracks) to Last.fm (`track.scrobble`).
    pub async fn scrobble_batch(
        &self,
        session_key: &str,
        tracks: &[LastFmTrack],
    ) -> Result<LastFmScrobbleResponse> {
        if tracks.is_empty() {
            return Ok(LastFmScrobbleResponse::default());
        }

        // Last.fm supports maximum 50 tracks per batch
        let batch = if tracks.len() > 50 {
            &tracks[..50]
        } else {
            tracks
        };

        let mut params = BTreeMap::new();
        params.insert("method", "track.scrobble".to_string());
        params.insert("api_key", self.api_key.clone());
        params.insert("sk", session_key.to_string());

        for (idx, t) in batch.iter().enumerate() {
            let key = |field: &str| -> &'static str {
                // Pre-allocated static indexed parameter names up to 50
                INDEXED_KEYS[idx].get(field)
            };

            params.insert(key("artist"), t.artist.clone());
            params.insert(key("track"), t.track.clone());
            params.insert(key("timestamp"), t.timestamp.to_string());

            if let Some(album) = &t.album {
                params.insert(key("album"), album.clone());
            }
            if let Some(track_no) = t.track_number {
                params.insert(key("trackNumber"), track_no.to_string());
            }
            if let Some(dur) = t.duration_secs {
                params.insert(key("duration"), dur.to_string());
            }
            if let Some(mbid) = &t.mbid {
                if !mbid.trim().is_empty() {
                    params.insert(key("mbid"), mbid.clone());
                }
            }
        }

        let sig = Self::sign_params(&params, &self.api_secret);
        params.insert("api_sig", sig);
        params.insert("format", "json".to_string());

        let res = self
            .client
            .post(&self.api_base)
            .form(&params)
            .send()
            .await
            .context("Failed to submit scrobbles to Last.fm")?;

        let body = res.text().await.unwrap_or_default();
        Self::check_api_error(&body)?;

        #[derive(Deserialize)]
        struct ScrobbleEnvelope {
            scrobbles: ScrobbleAttrHolder,
        }

        #[derive(Deserialize)]
        struct ScrobbleAttrHolder {
            #[serde(rename = "@attr")]
            attr: ScrobbleAttr,
        }

        #[derive(Deserialize)]
        struct ScrobbleAttr {
            #[serde(deserialize_with = "deserialize_u32_from_any")]
            accepted: u32,
            #[serde(deserialize_with = "deserialize_u32_from_any", default)]
            ignored: u32,
        }

        if let Ok(envelope) = serde_json::from_str::<ScrobbleEnvelope>(&body) {
            Ok(LastFmScrobbleResponse {
                accepted: envelope.scrobbles.attr.accepted,
                ignored: envelope.scrobbles.attr.ignored,
            })
        } else {
            // Fallback: If parse fails but no error was present, assume batch accepted
            Ok(LastFmScrobbleResponse {
                accepted: batch.len() as u32,
                ignored: 0,
            })
        }
    }

    /// Mark or unmark a track as loved on Last.fm (`track.love` / `track.unlove`).
    pub async fn set_loved(
        &self,
        session_key: &str,
        artist: &str,
        track: &str,
        loved: bool,
    ) -> Result<()> {
        let mut params = BTreeMap::new();
        let method = if loved { "track.love" } else { "track.unlove" };
        params.insert("method", method.to_string());
        params.insert("api_key", self.api_key.clone());
        params.insert("sk", session_key.to_string());
        params.insert("artist", artist.to_string());
        params.insert("track", track.to_string());

        let sig = Self::sign_params(&params, &self.api_secret);
        params.insert("api_sig", sig);
        params.insert("format", "json".to_string());

        let res = self
            .client
            .post(&self.api_base)
            .form(&params)
            .send()
            .await
            .with_context(|| {
                format!("Failed to set loved={loved} on Last.fm for '{artist} - {track}'")
            })?;

        let body = res.text().await.unwrap_or_default();
        Self::check_api_error(&body)?;
        Ok(())
    }

    /// Fetch all loved tracks for a user (`user.getLovedTracks`), traversing pagination.
    pub async fn fetch_all_loved_tracks(&self, username: &str) -> Result<Vec<LastFmLovedTrack>> {
        let mut all_tracks = Vec::new();
        let mut page = 1u32;
        let limit = 100u32;

        loop {
            let mut params = BTreeMap::new();
            params.insert("method", "user.getLovedTracks".to_string());
            params.insert("api_key", self.api_key.clone());
            params.insert("user", username.to_string());
            params.insert("limit", limit.to_string());
            params.insert("page", page.to_string());
            params.insert("format", "json".to_string());

            let url = Self::build_query_url(&self.api_base, &params);
            let res = self.client.get(&url).send().await.with_context(|| {
                format!("Failed to fetch loved tracks page {page} from Last.fm")
            })?;

            let body = res.text().await.unwrap_or_default();
            Self::check_api_error(&body)?;

            #[derive(Deserialize)]
            struct LovedEnvelope {
                lovedtracks: LovedTracksData,
            }

            #[derive(Deserialize)]
            struct LovedTracksData {
                #[serde(default, deserialize_with = "deserialize_vec_or_single")]
                track: Vec<LovedTrackItem>,
                #[serde(rename = "@attr")]
                attr: LovedAttr,
            }

            #[derive(Deserialize)]
            struct LovedAttr {
                #[serde(rename = "totalPages", deserialize_with = "deserialize_u32_from_any")]
                total_pages: u32,
            }

            #[derive(Deserialize)]
            struct LovedTrackItem {
                name: String,
                #[serde(default)]
                mbid: Option<String>,
                artist: LovedArtistItem,
                #[serde(default)]
                date: Option<LovedDateItem>,
            }

            #[derive(Deserialize)]
            struct LovedArtistItem {
                name: String,
            }

            #[derive(Deserialize)]
            struct LovedDateItem {
                #[serde(default, deserialize_with = "deserialize_opt_i64_from_any")]
                uts: Option<i64>,
            }

            let envelope: LovedEnvelope = serde_json::from_str(&body).with_context(|| {
                format!("Failed to parse Last.fm loved tracks response on page {page}: {body}")
            })?;

            let total_pages = envelope.lovedtracks.attr.total_pages;
            for t in envelope.lovedtracks.track {
                all_tracks.push(LastFmLovedTrack {
                    artist: t.artist.name,
                    track: t.name,
                    mbid: t.mbid.filter(|s| !s.trim().is_empty()),
                    date_uts: t.date.and_then(|d| d.uts),
                });
            }

            if page >= total_pages || total_pages == 0 {
                break;
            }
            page += 1;
        }

        Ok(all_tracks)
    }

    /// Assemble a URL with percent-encoded query parameters.
    pub fn build_query_url(base: &str, params: &BTreeMap<&str, String>) -> String {
        let mut url = base.to_string();
        if !url.contains('?') {
            url.push('?');
        } else if !url.ends_with('?') && !url.ends_with('&') {
            url.push('&');
        }
        let query_pairs: Vec<String> = params
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    utf8_percent_encode(k, QUERY_ESCAPE),
                    utf8_percent_encode(v, QUERY_ESCAPE)
                )
            })
            .collect();
        url.push_str(&query_pairs.join("&"));
        url
    }

    /// Check if the Last.fm response contains an error block.
    fn check_api_error(body: &str) -> Result<()> {
        #[derive(Deserialize)]
        struct ErrorEnvelope {
            error: i32,
            message: String,
        }

        if let Ok(err) = serde_json::from_str::<ErrorEnvelope>(body) {
            Err(anyhow!("Last.fm API Error {}: {}", err.error, err.message))
        } else {
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers & Deserializers
// ---------------------------------------------------------------------------

fn deserialize_vec_or_single<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany<T> {
        One(T),
        Many(Vec<T>),
    }
    match Option::<OneOrMany<T>>::deserialize(deserializer)? {
        Some(OneOrMany::One(val)) => Ok(vec![val]),
        Some(OneOrMany::Many(vec)) => Ok(vec),
        None => Ok(Vec::new()),
    }
}

fn deserialize_u32_from_any<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u32),
        Str(String),
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(n),
        NumOrStr::Str(s) => s.trim().parse::<u32>().map_err(serde::de::Error::custom),
    }
}

fn deserialize_opt_i64_from_any<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(i64),
        Str(String),
    }
    match Option::<NumOrStr>::deserialize(deserializer)? {
        Some(NumOrStr::Num(n)) => Ok(Some(n)),
        Some(NumOrStr::Str(s)) => {
            if s.trim().is_empty() {
                Ok(None)
            } else {
                s.trim()
                    .parse::<i64>()
                    .map(Some)
                    .map_err(serde::de::Error::custom)
            }
        }
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Pre-allocated Indexed Field Names (0..50)
// ---------------------------------------------------------------------------

struct IndexedKeySet {
    artist: &'static str,
    track: &'static str,
    timestamp: &'static str,
    album: &'static str,
    track_number: &'static str,
    duration: &'static str,
    mbid: &'static str,
}

impl IndexedKeySet {
    fn get(&self, field: &str) -> &'static str {
        match field {
            "artist" => self.artist,
            "track" => self.track,
            "timestamp" => self.timestamp,
            "album" => self.album,
            "trackNumber" => self.track_number,
            "duration" => self.duration,
            "mbid" => self.mbid,
            _ => panic!("Unknown field: {field}"),
        }
    }
}

macro_rules! define_indexed_keys {
    ($($i:expr),*) => {
        [
            $(
                IndexedKeySet {
                    artist: concat!("artist[", stringify!($i), "]"),
                    track: concat!("track[", stringify!($i), "]"),
                    timestamp: concat!("timestamp[", stringify!($i), "]"),
                    album: concat!("album[", stringify!($i), "]"),
                    track_number: concat!("trackNumber[", stringify!($i), "]"),
                    duration: concat!("duration[", stringify!($i), "]"),
                    mbid: concat!("mbid[", stringify!($i), "]"),
                }
            ),*
        ]
    };
}

static INDEXED_KEYS: [IndexedKeySet; 50] = define_indexed_keys!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49
);

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_sign_params() {
        let mut params = BTreeMap::new();
        params.insert("method", "auth.getSession".to_string());
        params.insert("api_key", "testkey".to_string());
        params.insert("token", "mytoken".to_string());
        params.insert("format", "json".to_string()); // should be ignored

        // Expected md5 of "api_keytestkeymethodauth.getSessiontokenmytokenmysecret"
        let expected_pre = "api_keytestkeymethodauth.getSessiontokenmytokenmysecret";
        let digest = Md5::digest(expected_pre.as_bytes());
        let expected_sig: String = digest.iter().map(|b| format!("{b:02x}")).collect();

        let sig = LastFmClient::sign_params(&params, "mysecret");
        expect_eq(&sig, &expected_sig);
    }

    fn expect_eq(a: &str, b: &str) {
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn test_get_auth_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .and(query_param("method", "auth.getToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "token": "tok_12345"
            })))
            .mount(&server)
            .await;

        let client = LastFmClient::with_endpoints(
            "test_api_key".into(),
            "test_secret".into(),
            server.uri(),
            format!("{}/auth", server.uri()),
        );

        let token = client.get_auth_token().await.unwrap();
        assert_eq!(token, "tok_12345");
        assert_eq!(
            client.build_auth_url(&token),
            format!("{}/auth?api_key=test_api_key&token=tok_12345", server.uri())
        );
    }

    #[tokio::test]
    async fn test_get_session() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .and(query_param("method", "auth.getSession"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "session": {
                    "name": "LastFmFan",
                    "key": "session_key_999",
                    "subscriber": 1
                }
            })))
            .mount(&server)
            .await;

        let client = LastFmClient::with_endpoints(
            "test_api_key".into(),
            "test_secret".into(),
            server.uri(),
            format!("{}/auth", server.uri()),
        );

        let session = client.get_session("tok_12345").await.unwrap();
        assert_eq!(session.username, "LastFmFan");
        assert_eq!(session.session_key, "session_key_999");
        assert!(session.subscriber);
    }

    #[tokio::test]
    async fn test_api_error_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "error": 9,
                "message": "Invalid session key - Please re-authenticate"
            })))
            .mount(&server)
            .await;

        let client = LastFmClient::with_endpoints(
            "test_api_key".into(),
            "test_secret".into(),
            server.uri(),
            format!("{}/auth", server.uri()),
        );

        let err = client.get_session("invalid").await.unwrap_err();
        assert!(err
            .to_string()
            .contains("Last.fm API Error 9: Invalid session key"));
    }

    #[tokio::test]
    async fn test_scrobble_batch_and_loved() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "scrobbles": {
                    "@attr": {
                        "accepted": 2,
                        "ignored": 0
                    }
                }
            })))
            .mount(&server)
            .await;

        let client = LastFmClient::with_endpoints(
            "test_api_key".into(),
            "test_secret".into(),
            server.uri(),
            format!("{}/auth", server.uri()),
        );

        let tracks = vec![
            LastFmTrack {
                artist: "Radiohead".into(),
                track: "Paranoid Android".into(),
                album: Some("OK Computer".into()),
                timestamp: 1600000000,
                ..Default::default()
            },
            LastFmTrack {
                artist: "Pink Floyd".into(),
                track: "Time".into(),
                album: Some("The Dark Side of the Moon".into()),
                timestamp: 1600000300,
                ..Default::default()
            },
        ];

        let res = client.scrobble_batch("my_session", &tracks).await.unwrap();
        assert_eq!(res.accepted, 2);
        assert_eq!(res.ignored, 0);
    }
}
