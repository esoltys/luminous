//! OpenSubsonic / Subsonic REST API client (#916).
//!
//! Talks to any Subsonic-compatible server (Navidrome, Nextcloud Music, Gonic,
//! Airsonic, LMS, ...) using the JSON flavour of the API (`f=json`). Sign-in
//! defaults to salted-token authentication (`t = md5(password + salt)`), so the
//! plain password never goes over the wire; legacy password and OpenSubsonic
//! API-key sign-in are per-server opt-ins (see [`AuthMode`], #1167).
//!
//! Two quirks shape the parsing here, both observed against Navidrome 0.63:
//! - API errors come back as **HTTP 200** with `"status": "failed"` in the
//!   body, so success is decided from the envelope, never the HTTP status.
//! - "Unset" values are sent as empty values rather than omitted
//!   (`musicBrainzId: ""`, `replayGain: {}`, `bitDepth: 0`, album
//!   `userRating: 0`), so those are normalized to `None` on the way in.

use anyhow::{anyhow, Context, Result};
use md5::{Digest, Md5};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use std::time::Duration;

pub mod report;
pub mod sync;

/// Subsonic REST API version we speak. 1.16.1 is the last version of the
/// original spec and the baseline every OpenSubsonic server supports.
pub const API_VERSION: &str = "1.16.1";

/// Client name sent as `c=`. Kept constant: Navidrome registers a "player"
/// per client name, so varying it would clutter the user's player list.
pub const CLIENT_NAME: &str = "Luminous";

/// OpenSubsonic extension a server lists when it accepts `apiKey=` sign-in.
pub const API_KEY_EXTENSION: &str = "apiKeyAuthentication";

/// URI scheme stored in `songs.path` for Subsonic tracks. The stored path
/// deliberately carries no credentials — a signed `stream.view` URL holds a
/// password-equivalent token, which must not end up in the library DB,
/// playlist exports, or logs. Playback resolves the URI to a freshly signed
/// URL at open time instead.
pub const URI_SCHEME: &str = "subsonic://";

/// Characters left unescaped in a track id embedded in a `subsonic://` URI.
/// Ids are opaque strings (Navidrome uses base62, Gonic `tr-123`, others
/// plain integers), so escape anything that could be mistaken for URI syntax.
const TRACK_ID_ESCAPE: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.');

/// Builds the library path for a remote track: `subsonic://{server_id}/{track_id}`.
pub fn track_uri(server_id: i64, track_id: &str) -> String {
    format!(
        "{URI_SCHEME}{server_id}/{}",
        utf8_percent_encode(track_id, TRACK_ID_ESCAPE)
    )
}

/// Parses a `subsonic://{server_id}/{track_id}` path back into its parts.
pub fn parse_track_uri(path: &str) -> Option<(i64, String)> {
    let rest = path.strip_prefix(URI_SCHEME)?;
    let (server, track) = rest.split_once('/')?;
    let server_id = server.parse::<i64>().ok()?;
    let track_id = percent_decode_str(track).decode_utf8().ok()?.into_owned();
    if track_id.is_empty() {
        return None;
    }
    Some((server_id, track_id))
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

/// Salted token for `password`: lowercase hex `md5(password + salt)`.
pub fn auth_token(password: &str, salt: &str) -> String {
    let digest = Md5::digest(format!("{password}{salt}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A fresh random salt. The spec asks for at least six characters and a new
/// salt per request, so a signed URL can't simply be replayed forever.
fn new_salt() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// How requests to a server are authenticated (#1167). Stored in
/// `subsonic_servers.auth_mode` as the serde name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthMode {
    /// Salted token, `t = md5(password + salt)`. Works with most servers.
    #[default]
    Token,
    /// Legacy `p=enc:<hex(password)>`, for servers that can't verify a token
    /// (Nextcloud Music, LDAP-backed Subsonic/Airsonic). The password is
    /// effectively cleartext on the wire, so it's opt-in only.
    Password,
    /// OpenSubsonic `apiKeyAuthentication`: `apiKey=<key>`, no username.
    ApiKey,
}

impl AuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthMode::Token => "token",
            AuthMode::Password => "password",
            AuthMode::ApiKey => "apiKey",
        }
    }

    /// Parses a stored `auth_mode`; anything unknown falls back to `Token`.
    pub fn from_db(value: &str) -> Self {
        match value {
            "password" => AuthMode::Password,
            "apiKey" => AuthMode::ApiKey,
            _ => AuthMode::Token,
        }
    }
}

/// A server's credentials. `secret` is the password, or the API key in
/// `ApiKey` mode. `Debug` redacts the secret so it can't reach a log.
#[derive(Clone, PartialEq, Eq)]
pub struct Auth {
    pub mode: AuthMode,
    pub username: String,
    pub secret: String,
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth")
            .field("mode", &self.mode)
            .field("username", &self.username)
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl Auth {
    pub fn new(mode: AuthMode, username: &str, secret: &str) -> Self {
        Self {
            mode,
            username: username.to_string(),
            secret: secret.to_string(),
        }
    }

    /// Token auth with `username` / `password`.
    pub fn token(username: &str, password: &str) -> Self {
        Self::new(AuthMode::Token, username, password)
    }

    /// The query parameters every request carries: the credentials for this
    /// mode, then `v`, `c`, `f`. Token mode gets a fresh salt per call.
    pub fn params(&self) -> Vec<(&'static str, String)> {
        let mut params = match self.mode {
            AuthMode::Token => {
                let salt = new_salt();
                vec![
                    ("u", self.username.clone()),
                    ("t", auth_token(&self.secret, &salt)),
                    ("s", salt),
                ]
            }
            AuthMode::Password => {
                let hex: String = self.secret.bytes().map(|b| format!("{b:02x}")).collect();
                vec![("u", self.username.clone()), ("p", format!("enc:{hex}"))]
            }
            AuthMode::ApiKey => vec![("apiKey", self.secret.clone())],
        };
        params.extend(client_params());
        params
    }
}

/// The non-credential parameters every request carries: `v`, `c`, `f`.
fn client_params() -> [(&'static str, String); 3] {
    [
        ("v", API_VERSION.to_string()),
        ("c", CLIENT_NAME.to_string()),
        ("f", "json".to_string()),
    ]
}

/// Normalizes a user-entered server URL to its root. A trailing `/rest` or
/// `/` is tolerated, since users often paste either.
fn normalize_base_url(base_url: &str) -> Result<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    let trimmed = trimmed.strip_suffix("/rest").unwrap_or(trimmed);
    reqwest::Url::parse(trimmed)
        .context("Enter a full server URL, e.g. https://music.example.com")?;
    Ok(trimmed.to_string())
}

/// Fully signed URL for `endpoint` (e.g. `"stream"`) with `params`. Each call
/// gets a fresh salt.
fn build_signed_url(
    base_url: &str,
    auth: &Auth,
    endpoint: &str,
    params: &[(&str, &str)],
) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(&format!("{base_url}/rest/{endpoint}.view"))
        .context("invalid Subsonic endpoint URL")?;
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in auth.params() {
            q.append_pair(k, &v);
        }
        for (k, v) in params {
            q.append_pair(k, v);
        }
    }
    Ok(url)
}

/// Signed `stream.view` URL for `track_id`, requesting the original file
/// (`format=raw`) so it stays seekable with HTTP Range requests. The result
/// holds a password-equivalent token: use it for the request only, never
/// store or log it.
pub fn stream_url(base_url: &str, auth: &Auth, track_id: &str) -> Result<String> {
    let base = normalize_base_url(base_url)?;
    let url = build_signed_url(
        &base,
        auth,
        "stream",
        &[("id", track_id), ("format", "raw")],
    )?;
    Ok(url.to_string())
}

/// A saved server's `(url, credentials)`. `Ok(None)` when no server has that id.
pub fn load_auth(conn: &rusqlite::Connection, server_id: i64) -> Result<Option<(String, Auth)>> {
    use rusqlite::OptionalExtension;
    let row: Option<(String, String, Option<String>, String)> = conn
        .query_row(
            "SELECT url, username, password, auth_mode FROM subsonic_servers WHERE id = ?1",
            rusqlite::params![server_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    Ok(row.map(|(url, username, secret, mode)| {
        let auth = Auth::new(
            AuthMode::from_db(&mode),
            &username,
            &secret.unwrap_or_default(),
        );
        (url, auth)
    }))
}

/// Resolves a stored `subsonic://{server_id}/{track_id}` path to a freshly
/// signed stream URL, using the server's saved credentials. Called by
/// `audio::open_media_source` at open time (#1163).
pub fn resolve_stream_url(conn: &rusqlite::Connection, path: &str) -> Result<String> {
    let (server_id, track_id) =
        parse_track_uri(path).ok_or_else(|| anyhow!("Invalid Subsonic track path '{path}'"))?;
    let (url, auth) = load_auth(conn, server_id)?
        .ok_or_else(|| anyhow!("The Subsonic server for '{path}' has been removed"))?;
    stream_url(&url, &auth, &track_id)
}

// ---------------------------------------------------------------------------
// Response models
// ---------------------------------------------------------------------------

/// `""` (or whitespace) → `None`.
fn empty_string_as_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v: Option<String> = Option::deserialize(d)?;
    Ok(v.filter(|s| !s.trim().is_empty()))
}

/// `0` → `None`.
fn zero_as_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    let v: Option<i64> = Option::deserialize(d)?;
    Ok(v.filter(|n| *n != 0))
}

/// `{}` (every field absent) → `None`.
fn empty_replay_gain_as_none<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<ReplayGain>, D::Error> {
    let v: Option<ReplayGain> = Option::deserialize(d)?;
    Ok(v.filter(|rg| !rg.is_empty()))
}

/// The `error` object of a failed response.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ApiError {
    pub code: i64,
    #[serde(default)]
    pub message: String,
}

/// Server identification carried by every OpenSubsonic response envelope.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    /// Subsonic API version the server implements.
    #[serde(default)]
    pub version: String,
    /// Server software, e.g. `navidrome` (OpenSubsonic only). Read from the
    /// wire's `type`, sent to the frontend as `serverType`.
    #[serde(default, rename(deserialize = "type"))]
    pub server_type: Option<String>,
    #[serde(default)]
    pub server_version: Option<String>,
    #[serde(default)]
    pub open_subsonic: bool,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(rename = "subsonic-response")]
    response: serde_json::Map<String, serde_json::Value>,
}

/// An OpenSubsonic extension advertised by `getOpenSubsonicExtensions`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenSubsonicExtension {
    pub name: String,
    #[serde(default)]
    pub versions: Vec<i64>,
}

/// OpenSubsonic `ReplayGain` object. Values are dB gains / linear peaks.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ReplayGain {
    pub track_gain: Option<f64>,
    pub album_gain: Option<f64>,
    pub track_peak: Option<f64>,
    pub album_peak: Option<f64>,
    pub base_gain: Option<f64>,
    pub fallback_gain: Option<f64>,
}

impl ReplayGain {
    pub fn is_empty(&self) -> bool {
        self.track_gain.is_none()
            && self.album_gain.is_none()
            && self.track_peak.is_none()
            && self.album_peak.is_none()
            && self.base_gain.is_none()
            && self.fallback_gain.is_none()
    }
}

/// `{ id, name }` pair used by OpenSubsonic's `artists`/`albumArtists` arrays.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ArtistRef {
    pub id: String,
    pub name: String,
}

/// `{ name }` entries of OpenSubsonic's `genres` array.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ItemGenre {
    pub name: String,
}

/// A song (`Child` in the spec). Only the fields Luminous maps are modelled.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Child {
    pub id: String,
    pub parent: Option<String>,
    pub is_dir: bool,
    pub title: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub album: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub artist: Option<String>,
    #[serde(deserialize_with = "zero_as_none")]
    pub track: Option<i64>,
    #[serde(deserialize_with = "zero_as_none")]
    pub disc_number: Option<i64>,
    #[serde(deserialize_with = "zero_as_none")]
    pub year: Option<i64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub genre: Option<String>,
    pub genres: Vec<ItemGenre>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub cover_art: Option<String>,
    #[serde(deserialize_with = "zero_as_none")]
    pub size: Option<i64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub content_type: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub suffix: Option<String>,
    /// Seconds.
    #[serde(deserialize_with = "zero_as_none")]
    pub duration: Option<i64>,
    /// kbps.
    #[serde(deserialize_with = "zero_as_none")]
    pub bit_rate: Option<i64>,
    /// Server-side file path, relative to the server's music folder.
    #[serde(deserialize_with = "empty_string_as_none")]
    pub path: Option<String>,
    /// RFC 3339 timestamp the server first indexed the file.
    #[serde(deserialize_with = "empty_string_as_none")]
    pub created: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub album_id: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub artist_id: Option<String>,
    /// 1–5; absent (songs) or `0` (albums) when unrated.
    #[serde(deserialize_with = "zero_as_none")]
    pub user_rating: Option<i64>,
    /// RFC 3339 timestamp; present iff the item is starred (favourited).
    #[serde(deserialize_with = "empty_string_as_none")]
    pub starred: Option<String>,
    #[serde(deserialize_with = "zero_as_none")]
    pub play_count: Option<i64>,
    #[serde(deserialize_with = "zero_as_none")]
    pub bpm: Option<i64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub comment: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub music_brainz_id: Option<String>,
    #[serde(deserialize_with = "empty_replay_gain_as_none")]
    pub replay_gain: Option<ReplayGain>,
    #[serde(deserialize_with = "zero_as_none")]
    pub channel_count: Option<i64>,
    #[serde(deserialize_with = "zero_as_none")]
    pub sampling_rate: Option<i64>,
    /// `0` for lossy formats.
    #[serde(deserialize_with = "zero_as_none")]
    pub bit_depth: Option<i64>,
    pub artists: Vec<ArtistRef>,
    pub album_artists: Vec<ArtistRef>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub display_artist: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub display_album_artist: Option<String>,
}

/// An album (`AlbumID3`), as returned by `getAlbumList2`/`getAlbum`/`getStarred2`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AlbumId3 {
    pub id: String,
    pub name: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub artist: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub artist_id: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub display_artist: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub cover_art: Option<String>,
    pub song_count: i64,
    #[serde(deserialize_with = "zero_as_none")]
    pub year: Option<i64>,
    #[serde(deserialize_with = "zero_as_none")]
    pub user_rating: Option<i64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub starred: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub music_brainz_id: Option<String>,
    /// Present on `getAlbum` responses only.
    pub song: Vec<Child>,
}

/// An artist (`ArtistID3`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ArtistId3 {
    pub id: String,
    pub name: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub cover_art: Option<String>,
    pub album_count: i64,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub starred: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub music_brainz_id: Option<String>,
}

/// `search3` result.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct SearchResult3 {
    pub artist: Vec<ArtistId3>,
    pub album: Vec<AlbumId3>,
    pub song: Vec<Child>,
}

/// What `SubsonicClient::probe` learns about a server.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerProbe {
    #[serde(flatten)]
    pub info: ServerInfo,
    pub extensions: Vec<OpenSubsonicExtension>,
}

impl ServerProbe {
    pub fn extension_names(&self) -> Vec<String> {
        self.extensions.iter().map(|e| e.name.clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// User-facing text for a Subsonic API error code, falling back to the
/// server's own message for codes without a clearer explanation.
pub fn describe_api_error(err: &ApiError) -> String {
    let msg = err.message.trim();
    match err.code {
        10 => format!("The server rejected the request (missing parameter: {msg})"),
        20 => "The server needs a newer Subsonic client version than Luminous supports".into(),
        30 => "The server's Subsonic API version is too old for Luminous".into(),
        40 => "Wrong username or password".into(),
        41 => "The server can't check a token sign-in (common with LDAP logins) — try the Password sign-in method".into(),
        42 => "The server doesn't support this sign-in method".into(),
        43 => "The server received more than one kind of sign-in details".into(),
        44 => "The server rejected the API key".into(),
        50 => "This account isn't allowed to do that on the server".into(),
        60 => "The server's trial period has ended".into(),
        70 => "Not found on the server".into(),
        _ if !msg.is_empty() => format!("Server error: {msg}"),
        code => format!("Server error (code {code})"),
    }
}

/// A failed envelope (`"status": "failed"`), kept structured so callers can
/// branch on the code (e.g. treat "not found" differently from bad auth).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubsonicApiError(pub ApiError);

impl std::fmt::Display for SubsonicApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&describe_api_error(&self.0))
    }
}

impl std::error::Error for SubsonicApiError {}

/// Splits a raw response body into server info + the full envelope map,
/// failing on a `"status": "failed"` envelope.
fn parse_envelope(body: &str) -> Result<(ServerInfo, serde_json::Map<String, serde_json::Value>)> {
    let envelope: Envelope = serde_json::from_str(body)
        .context("The server's reply isn't a Subsonic API response — check the server URL")?;
    let map = envelope.response;
    let info: ServerInfo = serde_json::from_value(serde_json::Value::Object(map.clone()))
        .context("Malformed Subsonic response envelope")?;

    let status = map.get("status").and_then(|s| s.as_str()).unwrap_or("");
    if status != "ok" {
        let err = map
            .get("error")
            .and_then(|e| serde_json::from_value::<ApiError>(e.clone()).ok())
            .unwrap_or(ApiError {
                code: 0,
                message: format!("unexpected status '{status}'"),
            });
        return Err(SubsonicApiError(err).into());
    }
    Ok((info, map))
}

/// User-facing message for a failed envelope in a body that was expected to
/// be media. `stream.view` reports errors (bad auth, unknown id) as HTTP 200
/// with a JSON body, so the playback reader checks non-audio replies here.
pub fn describe_error_body(body: &str) -> Option<String> {
    parse_envelope(body)
        .err()?
        .downcast_ref::<SubsonicApiError>()
        .map(|e| e.to_string())
}

/// Deserializes `map[key]` into `T`, treating a missing key as `T::default()`
/// (servers omit empty containers, e.g. `searchResult3` with no hits).
fn payload<T: DeserializeOwned + Default>(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<T> {
    match map.get(key) {
        Some(v) => serde_json::from_value(v.clone())
            .with_context(|| format!("Malformed '{key}' in Subsonic response")),
        None => Ok(T::default()),
    }
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// What a `star` / `unstar` call targets.
#[derive(Debug, Clone, Copy)]
pub enum StarTarget<'a> {
    Song(&'a str),
    Album(&'a str),
}

/// Blocking OpenSubsonic client — callers run it on a blocking thread
/// (`tokio::task::spawn_blocking`), same as `WebDavClient`.
#[derive(Clone)]
pub struct SubsonicClient {
    base_url: String,
    auth: Auth,
    client: Client,
}

impl SubsonicClient {
    /// `base_url` is the server root (e.g. `https://music.example.com`); a
    /// trailing `/rest` or `/` is tolerated, since users often paste either.
    pub fn new(base_url: &str, auth: Auth) -> Result<Self> {
        let base_url = normalize_base_url(base_url)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to create http client")?;
        Ok(Self {
            base_url,
            auth,
            client,
        })
    }

    /// Fully signed URL for `endpoint` (e.g. `"stream"`) with `params`.
    /// Each call gets a fresh salt.
    pub fn signed_url(&self, endpoint: &str, params: &[(&str, &str)]) -> Result<reqwest::Url> {
        build_signed_url(&self.base_url, &self.auth, endpoint, params)
    }

    /// Calls `endpoint` and returns the parsed, status-checked envelope.
    fn call(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> Result<(ServerInfo, serde_json::Map<String, serde_json::Value>)> {
        self.fetch_envelope(self.signed_url(endpoint, params)?)
    }

    /// GETs `url` and returns the parsed, status-checked envelope.
    fn fetch_envelope(
        &self,
        url: reqwest::Url,
    ) -> Result<(ServerInfo, serde_json::Map<String, serde_json::Value>)> {
        let resp = self
            .client
            .get(url)
            .send()
            .context("Couldn't reach the server")?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(anyhow!("The server refused access (HTTP {status})"));
        }
        if !status.is_success() {
            return Err(anyhow!(
                "The server returned HTTP {status} — check the server URL"
            ));
        }
        let body = resp.text().context("failed to read server response")?;
        parse_envelope(&body)
    }

    /// `ping`: checks reachability and credentials.
    pub fn ping(&self) -> Result<ServerInfo> {
        self.call("ping", &[]).map(|(info, _)| info)
    }

    /// `getOpenSubsonicExtensions`. A legacy server without it yields an
    /// empty list rather than an error.
    pub fn get_open_subsonic_extensions(&self) -> Result<Vec<OpenSubsonicExtension>> {
        match self.call("getOpenSubsonicExtensions", &[]) {
            Ok((_, map)) => payload(&map, "openSubsonicExtensions"),
            Err(e) if e.downcast_ref::<SubsonicApiError>().is_some() => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    /// One page of `search3` with an empty query — how Navidrome (and most
    /// OpenSubsonic servers) enumerate the whole song library.
    pub fn search3_songs_page(&self, offset: usize, count: usize) -> Result<Vec<Child>> {
        let (offset, count) = (offset.to_string(), count.to_string());
        let (_, map) = self.call(
            "search3",
            &[
                ("query", ""),
                ("songCount", &count),
                ("songOffset", &offset),
                ("albumCount", "0"),
                ("artistCount", "0"),
            ],
        )?;
        Ok(payload::<SearchResult3>(&map, "searchResult3")?.song)
    }

    /// One page of `getAlbumList2?type=alphabeticalByName`.
    pub fn album_list2_page(&self, offset: usize, size: usize) -> Result<Vec<AlbumId3>> {
        #[derive(Deserialize, Default)]
        struct AlbumList2 {
            #[serde(default)]
            album: Vec<AlbumId3>,
        }
        let (offset, size) = (offset.to_string(), size.to_string());
        let (_, map) = self.call(
            "getAlbumList2",
            &[
                ("type", "alphabeticalByName"),
                ("size", &size),
                ("offset", &offset),
            ],
        )?;
        Ok(payload::<AlbumList2>(&map, "albumList2")?.album)
    }

    /// `getAlbum`: the album with its songs.
    pub fn get_album(&self, id: &str) -> Result<AlbumId3> {
        let (_, map) = self.call("getAlbum", &[("id", id)])?;
        payload(&map, "album")
    }

    /// `getCoverArt`: raw image bytes. A missing image comes back as an
    /// HTTP-200 JSON/XML error envelope, not an image, so the content type
    /// is checked before the body is trusted.
    pub fn get_cover_art(&self, id: &str, size: u32) -> Result<Vec<u8>> {
        let size = size.to_string();
        let url = self.signed_url("getCoverArt", &[("id", id), ("size", &size)])?;
        let resp = self
            .client
            .get(url)
            .send()
            .context("Couldn't reach the server")?;
        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow!("Cover art request returned HTTP {status}"));
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if content_type.contains("json") || content_type.contains("xml") {
            let body = resp.text().unwrap_or_default();
            parse_envelope(&body)?;
            return Err(anyhow!("The server returned no image"));
        }
        Ok(resp.bytes().context("failed to read cover art")?.to_vec())
    }

    /// `scrobble`: `submission = false` reports "now playing"; `true`
    /// records a play at `time` (Unix seconds).
    pub fn scrobble(&self, id: &str, submission: bool, time: Option<i64>) -> Result<()> {
        let time_ms = time.map(|t| (t * 1000).to_string());
        let mut params = vec![
            ("id", id),
            ("submission", if submission { "true" } else { "false" }),
        ];
        if let Some(ms) = time_ms.as_deref() {
            params.push(("time", ms));
        }
        self.call("scrobble", &params).map(|_| ())
    }

    /// `star` / `unstar` a song or an album (ID3 `albumId`).
    pub fn set_starred(&self, target: StarTarget<'_>, starred: bool) -> Result<()> {
        let endpoint = if starred { "star" } else { "unstar" };
        let param = match target {
            StarTarget::Song(id) => ("id", id),
            StarTarget::Album(id) => ("albumId", id),
        };
        self.call(endpoint, &[param]).map(|_| ())
    }

    /// `setRating`: 1–5 stars, or 0 to clear. Works for song and album ids.
    pub fn set_rating(&self, id: &str, rating: u8) -> Result<()> {
        let rating = rating.min(5).to_string();
        self.call("setRating", &[("id", id), ("rating", &rating)])
            .map(|_| ())
    }

    /// `getOpenSubsonicExtensions` sent without credentials. The OpenSubsonic
    /// spec makes this endpoint public so a client can tell which sign-in
    /// methods a server supports before asking for any. A server that
    /// refuses it (a legacy Subsonic server) yields an empty list.
    pub fn public_extensions(&self) -> Result<Vec<OpenSubsonicExtension>> {
        let mut url = reqwest::Url::parse(&format!(
            "{}/rest/getOpenSubsonicExtensions.view",
            self.base_url
        ))
        .context("invalid Subsonic endpoint URL")?;
        url.query_pairs_mut().extend_pairs(client_params());
        match self.fetch_envelope(url) {
            Ok((_, map)) => payload(&map, "openSubsonicExtensions"),
            Err(e) if e.downcast_ref::<SubsonicApiError>().is_some() => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    /// Whether the server offers API-key sign-in (`apiKeyAuthentication`).
    pub fn supports_api_key(&self) -> Result<bool> {
        Ok(self
            .public_extensions()?
            .iter()
            .any(|e| e.name == API_KEY_EXTENSION))
    }

    /// Connection test + capability discovery: `ping`, then (on an
    /// OpenSubsonic server) the extension list. In API-key mode the server
    /// is first checked for `apiKeyAuthentication`, so an unsupported server
    /// gets a clear message rather than a generic auth error.
    pub fn probe(&self) -> Result<ServerProbe> {
        if self.auth.mode == AuthMode::ApiKey && !self.supports_api_key()? {
            return Err(anyhow!("This server doesn't support API key sign-in"));
        }
        let info = self.ping()?;
        let extensions = if info.open_subsonic {
            self.get_open_subsonic_extensions().unwrap_or_else(|e| {
                log::warn!("Failed to list OpenSubsonic extensions: {e:#}");
                Vec::new()
            })
        } else {
            Vec::new()
        };
        Ok(ServerProbe { info, extensions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed-down real `search3` reply from Navidrome 0.63.2, including its
    /// empty-value quirks (`musicBrainzId: ""`, `replayGain: {}`,
    /// `bitDepth: 0`, no `userRating` on an unrated song).
    const NAVIDROME_SEARCH3: &str = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","type":"navidrome","serverVersion":"0.63.2 (be10f89c)","openSubsonic":true,"searchResult3":{"song":[
      {"id":"6zNzY0KWdlrffKVhMqzQv6","parent":"4VllSh3CCoJi4wPk1Y5upl","isDir":false,"title":"Dead Engine","album":"Lonnie & Donny","artist":"Combover Beethoven","track":3,"year":2024,"coverArt":"mf-6zNzY0KWdlrffKVhMqzQv6_6a6138c6","size":9415606,"contentType":"audio/mpeg","suffix":"mp3","duration":234,"bitRate":320,"path":"Combover Beethoven/Lonnie & Donny/03 - Dead Engine.mp3","created":"2026-09-23T10:09:12.9315509-07:00","albumId":"4VllSh3CCoJi4wPk1Y5upl","artistId":"0mlgplP8cDmVMoLA5PNcYt","type":"music","bpm":0,"comment":"Visit https://comboverbeethoven.bandcamp.com","sortName":"dead engine","mediaType":"song","musicBrainzId":"","isrc":[],"genres":[],"replayGain":{},"channelCount":2,"samplingRate":44100,"bitDepth":0,"moods":[],"artists":[{"id":"0mlgplP8cDmVMoLA5PNcYt","name":"Combover Beethoven"}],"displayArtist":"Combover Beethoven","albumArtists":[{"id":"0mlgplP8cDmVMoLA5PNcYt","name":"Combover Beethoven"}],"displayAlbumArtist":"Combover Beethoven","contributors":[],"displayComposer":"","explicitStatus":"","groupings":[],"works":[],"movements":[]},
      {"id":"I24Q4padeoXCGbaGhZMhPG","isDir":false,"title":"Knowing","album":"Bloom","starred":"2026-09-23T15:19:06.1686105-07:00","userRating":5,"musicBrainzId":"0b1e8f6c-1111-4222-8333-944455556666","replayGain":{"trackGain":-7.5,"trackPeak":0.98},"bitDepth":24,"samplingRate":96000,"discNumber":2}
    ]}}}"#;

    const NAVIDROME_ALBUM_LIST2: &str = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","type":"navidrome","serverVersion":"0.63.2 (be10f89c)","openSubsonic":true,"albumList2":{"album":[
      {"id":"6vDhuNdz6CZyUtE0N7CwdH","name":"Bloom","artist":"Crows Labyrinth","artistId":"3FEmlbtcUGwlbW99vl6e17","coverArt":"al-6vDhuNdz6CZyUtE0N7CwdH_6a75f0ea","songCount":10,"duration":5330,"year":2019,"userRating":0,"starred":"2026-09-23T15:16:26.2871979-07:00","musicBrainzId":"","releaseDate":{}}
    ]}}}"#;

    const NAVIDROME_ERROR: &str = r#"{"subsonic-response":{"status":"failed","version":"1.16.1","type":"navidrome","serverVersion":"0.63.2 (be10f89c)","openSubsonic":true,"error":{"code":70,"message":"data not found"}}}"#;

    const NAVIDROME_EXTENSIONS: &str = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","type":"navidrome","serverVersion":"0.63.2 (be10f89c)","openSubsonic":true,"openSubsonicExtensions":[{"name":"transcodeOffset","versions":[1]},{"name":"formPost","versions":[1]},{"name":"songLyrics","versions":[1,2]}]}}"#;

    #[test]
    fn auth_token_matches_spec_example() {
        // Example from the Subsonic API docs: password "sesame", salt "c19b2d".
        assert_eq!(
            auth_token("sesame", "c19b2d"),
            "26719a1196d2a940705a59634eb18eab"
        );
    }

    #[test]
    fn auth_params_use_fresh_salt_and_never_include_password() {
        let a = Auth::token("alice", "hunter2").params();
        let b = Auth::token("alice", "hunter2").params();
        let salt = |p: &[(&str, String)]| p.iter().find(|(k, _)| *k == "s").unwrap().1.clone();
        assert_ne!(salt(&a), salt(&b));
        assert!(salt(&a).len() >= 6);
        assert!(a.iter().all(|(_, v)| !v.contains("hunter2")));
        let get = |k: &str| a.iter().find(|(kk, _)| *kk == k).unwrap().1.clone();
        assert_eq!(get("t"), auth_token("hunter2", &salt(&a)));
        assert_eq!(get("v"), API_VERSION);
        assert_eq!(get("c"), CLIENT_NAME);
        assert_eq!(get("f"), "json");
    }

    #[test]
    fn stream_url_is_signed_without_the_password() {
        let auth = Auth::token("alice", "hunter2");
        let a = stream_url("https://music.example.com/rest/", &auth, "tr/1").unwrap();
        let b = stream_url("https://music.example.com", &auth, "tr/1").unwrap();
        assert!(a.starts_with("https://music.example.com/rest/stream.view?"));
        assert!(!a.contains("hunter2") && !a.contains("p="));
        let parsed = reqwest::Url::parse(&a).unwrap();
        let q: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(q["id"], "tr/1");
        assert_eq!(q["format"], "raw");
        assert_eq!(q["u"], "alice");
        assert_eq!(q["t"], auth_token("hunter2", &q["s"]));
        assert_ne!(a, b, "each URL gets a fresh salt");
    }

    fn query(url: &str) -> std::collections::HashMap<String, String> {
        reqwest::Url::parse(url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect()
    }

    #[test]
    fn password_mode_sends_hex_encoded_password() {
        let auth = Auth::new(AuthMode::Password, "alice", "sesame");
        let q = query(&stream_url("https://music.example.com", &auth, "1").unwrap());
        assert_eq!(q["u"], "alice");
        assert_eq!(q["p"], "enc:736573616d65");
        assert!(!q.contains_key("t") && !q.contains_key("s") && !q.contains_key("apiKey"));
        assert_eq!(q["v"], API_VERSION);
    }

    #[test]
    fn api_key_mode_sends_only_the_key() {
        let auth = Auth::new(AuthMode::ApiKey, "", "k3y");
        let q = query(&stream_url("https://music.example.com", &auth, "1").unwrap());
        assert_eq!(q["apiKey"], "k3y");
        for k in ["u", "t", "s", "p"] {
            assert!(!q.contains_key(k), "{k} should not be sent");
        }
        assert_eq!(q["c"], CLIENT_NAME);
        assert_eq!(q["f"], "json");
    }

    #[test]
    fn auth_debug_never_shows_the_secret() {
        let dbg = format!("{:?}", Auth::new(AuthMode::ApiKey, "", "k3y-secret"));
        assert!(!dbg.contains("k3y-secret"), "{dbg}");
    }

    #[test]
    fn auth_mode_round_trips_through_the_db_value() {
        for mode in [AuthMode::Token, AuthMode::Password, AuthMode::ApiKey] {
            assert_eq!(AuthMode::from_db(mode.as_str()), mode);
            assert_eq!(
                serde_json::to_string(&mode).unwrap(),
                format!("\"{}\"", mode.as_str())
            );
        }
        assert_eq!(AuthMode::from_db("bogus"), AuthMode::Token);
    }

    fn servers_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE subsonic_servers (id INTEGER PRIMARY KEY, url TEXT, username TEXT, password TEXT,
                auth_mode TEXT NOT NULL DEFAULT 'token');
             INSERT INTO subsonic_servers (id, url, username, password) VALUES (3, 'https://music.example.com', 'alice', 'hunter2');
             INSERT INTO subsonic_servers VALUES (4, 'https://keys.example.com', '', 'k3y', 'apiKey');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn resolve_stream_url_signs_with_the_saved_server() {
        let conn = servers_db();
        let url = resolve_stream_url(&conn, &track_uri(3, "a b")).unwrap();
        assert!(url.starts_with("https://music.example.com/rest/stream.view?"));
        assert!(url.contains("id=a+b") || url.contains("id=a%20b"));
        assert!(url.contains("u=alice"));
    }

    #[test]
    fn resolve_stream_url_uses_the_saved_auth_mode() {
        let conn = servers_db();
        let q = query(&resolve_stream_url(&conn, &track_uri(4, "x")).unwrap());
        assert_eq!(q["apiKey"], "k3y");
        assert!(!q.contains_key("u"));
    }

    #[test]
    fn resolve_stream_url_reports_a_removed_server() {
        let conn = servers_db();
        let err = resolve_stream_url(&conn, "subsonic://9/abc").unwrap_err();
        assert!(err.to_string().contains("has been removed"), "{err}");
        assert!(resolve_stream_url(&conn, "/music/a.flac").is_err());
    }

    #[test]
    fn describe_error_body_only_matches_failed_envelopes() {
        assert_eq!(
            describe_error_body(NAVIDROME_ERROR).as_deref(),
            Some("Not found on the server")
        );
        assert_eq!(describe_error_body(NAVIDROME_EXTENSIONS), None);
        assert_eq!(describe_error_body("<html>nope</html>"), None);
    }

    #[test]
    fn track_uri_round_trips_including_awkward_ids() {
        for id in [
            "6zNzY0KWdlrffKVhMqzQv6",
            "tr-123",
            "42",
            "a/b?c=d&e#f",
            "ünï code",
        ] {
            let uri = track_uri(7, id);
            assert!(uri.starts_with("subsonic://7/"));
            assert_eq!(uri.matches('/').count(), 3, "id must be escaped: {uri}");
            assert_eq!(parse_track_uri(&uri), Some((7, id.to_string())));
        }
    }

    #[test]
    fn parse_track_uri_rejects_other_paths() {
        assert_eq!(parse_track_uri("/music/song.flac"), None);
        assert_eq!(parse_track_uri("https://host/rest/stream.view?id=1"), None);
        assert_eq!(parse_track_uri("subsonic://notanumber/abc"), None);
        assert_eq!(parse_track_uri("subsonic://3/"), None);
        assert_eq!(parse_track_uri("subsonic://3"), None);
    }

    #[test]
    fn search3_parses_and_normalizes_empty_values() {
        let (info, map) = parse_envelope(NAVIDROME_SEARCH3).unwrap();
        assert_eq!(info.server_type.as_deref(), Some("navidrome"));
        assert_eq!(info.server_version.as_deref(), Some("0.63.2 (be10f89c)"));
        assert!(info.open_subsonic);

        let result: SearchResult3 = payload(&map, "searchResult3").unwrap();
        assert_eq!(result.song.len(), 2);

        let s = &result.song[0];
        assert_eq!(s.title, "Dead Engine");
        assert_eq!(s.album.as_deref(), Some("Lonnie & Donny"));
        assert_eq!(s.track, Some(3));
        assert_eq!(s.duration, Some(234));
        assert_eq!(s.size, Some(9415606));
        assert_eq!(s.suffix.as_deref(), Some("mp3"));
        assert_eq!(
            s.display_album_artist.as_deref(),
            Some("Combover Beethoven")
        );
        assert_eq!(s.album_artists.len(), 1);
        // Empty-value quirks → None.
        assert_eq!(s.music_brainz_id, None);
        assert_eq!(s.replay_gain, None);
        assert_eq!(s.bit_depth, None);
        assert_eq!(s.bpm, None);
        assert_eq!(s.user_rating, None);
        assert_eq!(s.starred, None);
        assert_eq!(s.sampling_rate, Some(44100));
        assert_eq!(s.channel_count, Some(2));

        let k = &result.song[1];
        assert_eq!(k.user_rating, Some(5));
        assert!(k.starred.is_some());
        assert_eq!(k.bit_depth, Some(24));
        assert_eq!(k.disc_number, Some(2));
        assert_eq!(
            k.music_brainz_id.as_deref(),
            Some("0b1e8f6c-1111-4222-8333-944455556666")
        );
        let rg = k.replay_gain.as_ref().unwrap();
        assert_eq!(rg.track_gain, Some(-7.5));
        assert_eq!(rg.album_gain, None);
    }

    #[test]
    fn album_list2_treats_zero_rating_as_unrated() {
        let (_, map) = parse_envelope(NAVIDROME_ALBUM_LIST2).unwrap();
        #[derive(Deserialize, Default)]
        struct AlbumList2 {
            #[serde(default)]
            album: Vec<AlbumId3>,
        }
        let list: AlbumList2 = payload(&map, "albumList2").unwrap();
        let a = &list.album[0];
        assert_eq!(a.name, "Bloom");
        assert_eq!(a.user_rating, None);
        assert!(a.starred.is_some());
        assert_eq!(a.music_brainz_id, None);
        assert_eq!(
            a.cover_art.as_deref(),
            Some("al-6vDhuNdz6CZyUtE0N7CwdH_6a75f0ea")
        );
    }

    #[test]
    fn missing_payload_key_is_default() {
        let body = r#"{"subsonic-response":{"status":"ok","version":"1.16.1"}}"#;
        let (info, map) = parse_envelope(body).unwrap();
        assert!(!info.open_subsonic);
        assert_eq!(info.server_type, None);
        let result: SearchResult3 = payload(&map, "searchResult3").unwrap();
        assert!(result.song.is_empty());
    }

    #[test]
    fn failed_envelope_is_a_structured_error() {
        let err = parse_envelope(NAVIDROME_ERROR).unwrap_err();
        let api = err.downcast_ref::<SubsonicApiError>().unwrap();
        assert_eq!(api.0.code, 70);
        assert_eq!(err.to_string(), "Not found on the server");
    }

    #[test]
    fn non_subsonic_body_is_a_clear_error() {
        let err = parse_envelope("<html>login page</html>").unwrap_err();
        assert!(err.to_string().contains("check the server URL"));
    }

    #[test]
    fn describe_api_error_covers_auth_codes() {
        let e = |code| ApiError {
            code,
            message: "x".into(),
        };
        assert_eq!(describe_api_error(&e(40)), "Wrong username or password");
        assert!(describe_api_error(&e(41)).contains("Password sign-in method"));
        assert_eq!(describe_api_error(&e(44)), "The server rejected the API key");
        assert_eq!(describe_api_error(&e(999)), "Server error: x");
    }

    #[test]
    fn extensions_parse() {
        let (_, map) = parse_envelope(NAVIDROME_EXTENSIONS).unwrap();
        let ext: Vec<OpenSubsonicExtension> = payload(&map, "openSubsonicExtensions").unwrap();
        assert_eq!(ext.len(), 3);
        assert_eq!(ext[2].name, "songLyrics");
        assert_eq!(ext[2].versions, vec![1, 2]);
    }

    #[test]
    fn client_normalizes_base_url() {
        for input in [
            "https://music.example.com",
            "https://music.example.com/",
            "https://music.example.com/rest",
            "https://music.example.com/rest/",
        ] {
            let c = SubsonicClient::new(input, Auth::token("u", "p")).unwrap();
            let url = c.signed_url("ping", &[]).unwrap();
            assert_eq!(url.path(), "/rest/ping.view", "input: {input}");
        }
        // Sub-path installs keep their prefix.
        let c = SubsonicClient::new("https://example.com/navidrome/", Auth::token("u", "p")).unwrap();
        assert_eq!(
            c.signed_url("ping", &[]).unwrap().path(),
            "/navidrome/rest/ping.view"
        );
        assert!(SubsonicClient::new("music.example.com", Auth::token("u", "p")).is_err());
    }

    #[test]
    fn signed_url_carries_extra_params_and_no_password() {
        let c = SubsonicClient::new("https://music.example.com", Auth::token("alice", "hunter2")).unwrap();
        let url = c
            .signed_url("stream", &[("id", "abc"), ("format", "raw")])
            .unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q.get("id").map(String::as_str), Some("abc"));
        assert_eq!(q.get("format").map(String::as_str), Some("raw"));
        assert_eq!(q.get("u").map(String::as_str), Some("alice"));
        assert!(!url.as_str().contains("hunter2"));
    }

    mod http {
        use super::super::*;
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        fn json(body: &str) -> ResponseTemplate {
            ResponseTemplate::new(200).set_body_raw(body.to_string(), "application/json")
        }

        #[tokio::test]
        async fn probe_pings_then_lists_extensions() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/rest/ping.view"))
                .and(query_param("u", "alice"))
                .and(query_param("f", "json"))
                .respond_with(json(
                    r#"{"subsonic-response":{"status":"ok","version":"1.16.1","type":"navidrome","serverVersion":"0.63.2","openSubsonic":true}}"#,
                ))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/rest/getOpenSubsonicExtensions.view"))
                .respond_with(json(super::NAVIDROME_EXTENSIONS))
                .expect(1)
                .mount(&server)
                .await;

            let uri = server.uri();
            let probe = tokio::task::spawn_blocking(move || {
                SubsonicClient::new(&uri, Auth::token("alice", "pw")).unwrap().probe()
            })
            .await
            .unwrap()
            .unwrap();
            assert_eq!(probe.info.server_type.as_deref(), Some("navidrome"));
            assert_eq!(
                probe.extension_names(),
                vec!["transcodeOffset", "formPost", "songLyrics"]
            );
        }

        #[tokio::test]
        async fn probe_skips_extensions_on_legacy_server() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/rest/ping.view"))
                .respond_with(json(
                    r#"{"subsonic-response":{"status":"ok","version":"1.15.0"}}"#,
                ))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/rest/getOpenSubsonicExtensions.view"))
                .respond_with(json("{}"))
                .expect(0)
                .mount(&server)
                .await;

            let uri = server.uri();
            let probe = tokio::task::spawn_blocking(move || {
                SubsonicClient::new(&uri, Auth::token("alice", "pw")).unwrap().probe()
            })
            .await
            .unwrap()
            .unwrap();
            assert!(!probe.info.open_subsonic);
            assert!(probe.extensions.is_empty());
        }

        #[tokio::test]
        async fn ping_reports_bad_credentials_sent_as_http_200() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/rest/ping.view"))
                .respond_with(json(
                    r#"{"subsonic-response":{"status":"failed","version":"1.16.1","openSubsonic":true,"error":{"code":40,"message":"Wrong username or password"}}}"#,
                ))
                .mount(&server)
                .await;

            let uri = server.uri();
            let err = tokio::task::spawn_blocking(move || {
                SubsonicClient::new(&uri, Auth::token("alice", "wrong")).unwrap().ping()
            })
            .await
            .unwrap()
            .unwrap_err();
            assert_eq!(err.to_string(), "Wrong username or password");
        }

        #[tokio::test]
        async fn ping_reports_wrong_url() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(404))
                .mount(&server)
                .await;

            let uri = server.uri();
            let err = tokio::task::spawn_blocking(move || {
                SubsonicClient::new(&uri, Auth::token("alice", "pw")).unwrap().ping()
            })
            .await
            .unwrap()
            .unwrap_err();
            assert!(err.to_string().contains("HTTP 404"));
        }

        const PING_OK: &str =
            r#"{"subsonic-response":{"status":"ok","version":"1.16.1","openSubsonic":true}}"#;
        const API_KEY_EXTENSIONS: &str = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","openSubsonic":true,"openSubsonicExtensions":[{"name":"apiKeyAuthentication","versions":[1]}]}}"#;

        #[tokio::test]
        async fn password_mode_sends_hex_password() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/rest/ping.view"))
                .and(query_param("u", "alice"))
                .and(query_param("p", "enc:736573616d65"))
                .respond_with(json(PING_OK))
                .expect(1)
                .mount(&server)
                .await;

            let uri = server.uri();
            tokio::task::spawn_blocking(move || {
                SubsonicClient::new(&uri, Auth::new(AuthMode::Password, "alice", "sesame"))
                    .unwrap()
                    .ping()
            })
            .await
            .unwrap()
            .unwrap();
        }

        #[tokio::test]
        async fn api_key_probe_checks_public_extensions_then_pings_with_key() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/rest/ping.view"))
                .and(query_param("apiKey", "k3y"))
                .respond_with(json(PING_OK))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/rest/getOpenSubsonicExtensions.view"))
                .respond_with(json(API_KEY_EXTENSIONS))
                .mount(&server)
                .await;

            let uri = server.uri();
            let probe = tokio::task::spawn_blocking(move || {
                SubsonicClient::new(&uri, Auth::new(AuthMode::ApiKey, "", "k3y"))
                    .unwrap()
                    .probe()
            })
            .await
            .unwrap()
            .unwrap();
            assert_eq!(probe.extension_names(), vec!["apiKeyAuthentication"]);

            // The capability check goes out without any credentials.
            let requests = server.received_requests().await.unwrap();
            let public = requests
                .iter()
                .find(|r| r.url.path().ends_with("getOpenSubsonicExtensions.view"))
                .unwrap();
            let q: Vec<_> = public.url.query_pairs().map(|(k, _)| k.into_owned()).collect();
            for k in ["u", "t", "s", "p", "apiKey"] {
                assert!(!q.contains(&k.to_string()), "{k} sent to public endpoint");
            }
        }

        #[tokio::test]
        async fn api_key_probe_is_refused_without_the_extension() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/rest/getOpenSubsonicExtensions.view"))
                .respond_with(json(super::NAVIDROME_EXTENSIONS))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/rest/ping.view"))
                .respond_with(json(PING_OK))
                .expect(0)
                .mount(&server)
                .await;

            let uri = server.uri();
            let err = tokio::task::spawn_blocking(move || {
                SubsonicClient::new(&uri, Auth::new(AuthMode::ApiKey, "", "k3y"))
                    .unwrap()
                    .probe()
            })
            .await
            .unwrap()
            .unwrap_err();
            assert_eq!(err.to_string(), "This server doesn't support API key sign-in");
        }

        #[tokio::test]
        async fn supports_api_key_is_false_on_a_legacy_server() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/rest/getOpenSubsonicExtensions.view"))
                .respond_with(json(
                    r#"{"subsonic-response":{"status":"failed","version":"1.15.0","error":{"code":0,"message":"Unknown endpoint"}}}"#,
                ))
                .mount(&server)
                .await;

            let uri = server.uri();
            let supported = tokio::task::spawn_blocking(move || {
                SubsonicClient::new(&uri, Auth::token("", ""))
                    .unwrap()
                    .supports_api_key()
            })
            .await
            .unwrap()
            .unwrap();
            assert!(!supported);
        }

        #[tokio::test]
        async fn token_error_41_suggests_password_sign_in() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/rest/ping.view"))
                .respond_with(json(
                    r#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":41,"message":"Token authentication not supported for LDAP users."}}}"#,
                ))
                .mount(&server)
                .await;

            let uri = server.uri();
            let err = tokio::task::spawn_blocking(move || {
                SubsonicClient::new(&uri, Auth::token("alice", "pw"))
                    .unwrap()
                    .probe()
            })
            .await
            .unwrap()
            .unwrap_err();
            let api = err.downcast_ref::<SubsonicApiError>().unwrap();
            assert_eq!(api.0.code, 41);
            assert!(err.to_string().contains("Password sign-in method"));
        }
    }
}
