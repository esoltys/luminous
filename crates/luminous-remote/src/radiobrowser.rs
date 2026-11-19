//! Radio Browser community station directory wire-protocol client (#2003).
//!
//! Discovers active Radio Browser API servers via DNS lookup of
//! `all.api.radio-browser.info` (upgraded to HTTPS hostnames via `/json/servers`),
//! randomises the candidate pool, and rotates to the next mirror automatically
//! whenever a request fails.
//!
//! Must never depend on `tauri` or `rusqlite` (#1767, epic #1762).

use parking_lot::Mutex;
use rand::seq::SliceRandom;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use reqwest::{Client, Url};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;

/// DNS hostname whose A/AAAA records enumerate all active Radio Browser API mirrors.
pub const RADIO_BROWSER_DNS_HOST: &str = "all.api.radio-browser.info";

/// Public website URL for Radio Browser directory attribution.
pub const RADIO_BROWSER_ATTRIBUTION_URL: &str = "https://www.radio-browser.info";

/// Default page size when downloading the full station directory.
pub const DEFAULT_STATION_PAGE_SIZE: usize = 10_000;

/// Maximum number of change events fetched in a single incremental sync pass before
/// falling back to a full resync.
pub const DEFAULT_CHANGED_LIMIT: usize = 5_000;

/// Maximum number of station UUIDs queried in one `/json/stations/byuuid` request.
const BY_UUID_CHUNK_SIZE: usize = 200;

/// Percent-encode set for URL path segments and query parameter values, leaving `-`, `_`, `.`, `,` unencoded.
const QUERY_ENCODE_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b',');

/// Normalized station entry returned by the Radio Browser directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RadioBrowserStation {
    pub stationuuid: String,
    pub changeuuid: String,
    pub name: String,
    pub url_resolved: String,
    pub homepage: String,
    pub favicon: String,
    pub tags: String,
    pub country: String,
    pub countrycode: String,
    pub language: String,
    pub codec: String,
    pub bitrate: i32,
    pub hls: bool,
    pub votes: i32,
    pub clickcount: i32,
    pub lastcheckok: bool,
    pub lastchangetime_iso8601: Option<String>,
}

/// Facet count item returned by `/json/tags`, `/json/countries`, `/json/languages`, and `/json/codecs`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RadioBrowserFacetEntry {
    pub name: String,
    pub code: Option<String>,
    pub stationcount: i64,
}

// ---------------------------------------------------------------------------
// Wire deserialization types
// ---------------------------------------------------------------------------

fn deserialize_bool_or_int<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrInt {
        Bool(bool),
        Int(i64),
        Str(String),
    }

    match Option::<BoolOrInt>::deserialize(deserializer)? {
        Some(BoolOrInt::Bool(b)) => Ok(b),
        Some(BoolOrInt::Int(n)) => Ok(n != 0),
        Some(BoolOrInt::Str(s)) => Ok(matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        )),
        None => Ok(false),
    }
}

fn deserialize_i32_default<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<i32>::deserialize(deserializer)?.unwrap_or(0))
}

fn deserialize_string_default<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Deserialize)]
struct WireStation {
    #[serde(default, deserialize_with = "deserialize_string_default")]
    stationuuid: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    changeuuid: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    name: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    url: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    url_resolved: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    homepage: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    favicon: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    tags: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    country: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    countrycode: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    language: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    codec: String,
    #[serde(default, deserialize_with = "deserialize_i32_default")]
    bitrate: i32,
    #[serde(default, deserialize_with = "deserialize_bool_or_int")]
    hls: bool,
    #[serde(default, deserialize_with = "deserialize_i32_default")]
    votes: i32,
    #[serde(default, deserialize_with = "deserialize_i32_default")]
    clickcount: i32,
    #[serde(default, deserialize_with = "deserialize_bool_or_int")]
    lastcheckok: bool,
    #[serde(default)]
    lastchangetime_iso8601: Option<String>,
}

impl WireStation {
    fn into_normalized(self) -> Option<RadioBrowserStation> {
        let stationuuid = self.stationuuid.trim().to_ascii_lowercase();
        if stationuuid.is_empty() {
            return None;
        }
        let url_resolved = {
            let resolved = self.url_resolved.trim();
            if !resolved.is_empty() {
                resolved.to_string()
            } else {
                self.url.trim().to_string()
            }
        };
        if url_resolved.is_empty() {
            return None;
        }

        let name = self
            .name
            .trim_matches(|c: char| c.is_whitespace() || c.is_control())
            .to_string();
        if name.is_empty() {
            return None;
        }

        let tags = normalize_tags(&self.tags);
        let countrycode = self.countrycode.trim().to_ascii_uppercase();
        let country = self.country.trim().to_string();
        let language = normalize_csv_tokens(&self.language);
        let codec = self.codec.trim().to_ascii_uppercase();

        Some(RadioBrowserStation {
            stationuuid,
            changeuuid: self.changeuuid.trim().to_ascii_lowercase(),
            name,
            url_resolved,
            homepage: self.homepage.trim().to_string(),
            favicon: self.favicon.trim().to_string(),
            tags,
            country,
            countrycode,
            language,
            codec,
            bitrate: self.bitrate.max(0),
            hls: self.hls,
            votes: self.votes.max(0),
            clickcount: self.clickcount.max(0),
            lastcheckok: self.lastcheckok,
            lastchangetime_iso8601: self
                .lastchangetime_iso8601
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        })
    }
}

#[derive(Debug, Deserialize)]
struct WireChangeHeader {
    #[serde(default, deserialize_with = "deserialize_string_default")]
    stationuuid: String,
    #[serde(default, deserialize_with = "deserialize_string_default")]
    changeuuid: String,
    #[serde(default)]
    lastchangetime_iso8601: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireServerDescriptor {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct WireFacetEntry {
    #[serde(default, deserialize_with = "deserialize_string_default")]
    name: String,
    #[serde(default)]
    iso_3166_1: Option<String>,
    #[serde(default)]
    iso_639: Option<String>,
    #[serde(default)]
    stationcount: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct WireClickResponse {
    #[serde(default, deserialize_with = "deserialize_bool_or_int")]
    ok: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

/// Normalizes a comma-separated tag list: trims each tag, lowercases, drops empty entries,
/// and deduplicates while preserving order.
pub fn normalize_tags(raw: &str) -> String {
    normalize_csv_tokens(raw)
}

fn normalize_csv_tokens(raw: &str) -> String {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for part in raw.split(',') {
        let cleaned = part
            .trim_matches(|c: char| c.is_whitespace() || c.is_control())
            .to_lowercase();
        if !cleaned.is_empty() && seen.insert(cleaned.clone()) {
            out.push(cleaned);
        }
    }
    out.join(",")
}

fn normalize_base_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = Url::parse(&with_scheme).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    Some(parsed.as_str().trim_end_matches('/').to_string())
}

fn format_ip_base_url(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => format!("http://{v4}"),
        IpAddr::V6(v6) => format!("http://[{v6}]"),
    }
}

fn extract_unique_server_urls(list: Vec<WireServerDescriptor>) -> Vec<String> {
    let mut seen_hosts = HashSet::new();
    let mut host_urls: Vec<String> = list
        .into_iter()
        .filter_map(|d| normalize_base_url(&d.name))
        .filter(|u| seen_hosts.insert(u.clone()))
        .collect();
    if !host_urls.is_empty() {
        let mut rng = rand::rng();
        host_urls.shuffle(&mut rng);
    }
    host_urls
}

/// One page of `/json/stations`, as returned by [`RadioBrowserClient::fetch_stations_page`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationsPage {
    /// The page's stations that passed normalisation (rows with no uuid, URL or name are dropped).
    pub stations: Vec<RadioBrowserStation>,
    /// Number of rows the server returned before normalisation. Paging must use this, not
    /// `stations.len()`: a full page with one dropped row is still a full page (#2199).
    pub raw_count: usize,
}

/// Result of querying `/json/stations/changed` and resolving full station details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedStationsBatch {
    /// Full station records for all unique stations changed since the cursor.
    pub stations: Vec<RadioBrowserStation>,
    /// The newest `changeuuid` observed in the change stream, if any changes were returned.
    pub newest_change_uuid: Option<String>,
    /// ISO 8601 timestamp (`YYYY-MM-DDTHH:MM:SSZ`) of the oldest change event in the batch.
    /// Used by the sync engine to detect when the server ignored an unknown/expired
    /// `lastchangeuuid` and dumped changes older than the local mirror's last sync.
    pub oldest_change_iso8601: Option<String>,
    /// Total number of raw change events returned by `/json/stations/changed`.
    /// When this equals the requested `limit`, the cursor may have fallen behind
    /// the batch window and the caller should fall back to a full resync.
    pub raw_change_count: usize,
    /// True when `raw_change_count < limit` (meaning the batch did not hit the safety cap).
    pub cursor_valid: bool,
}

/// Wire-protocol client for the public Radio Browser directory.
///
/// Discovers active servers via DNS lookup of `all.api.radio-browser.info`,
/// randomises the server order, and retries the next mirror on failure.
pub struct RadioBrowserClient {
    client: Client,
    user_agent: String,
    /// Ordered list of active mirror base URLs (e.g. `https://de1.api.radio-browser.info`).
    /// Index 0 is tried first; when index 0 fails, servers are rotated so the first
    /// working mirror becomes the new index 0.
    servers: Mutex<Vec<String>>,
    /// True when `servers` was explicitly provided via [`Self::with_servers`] (disables DNS re-discovery).
    pinned_servers: bool,
}

impl RadioBrowserClient {
    /// Creates a client that discovers mirrors dynamically via DNS lookup of
    /// [`RADIO_BROWSER_DNS_HOST`] and sends `user_agent` (e.g. `"Luminous/4.0.0"`).
    pub fn new(user_agent: impl Into<String>) -> Self {
        let user_agent = Self::sanitize_user_agent(user_agent.into());
        let client = Self::build_http_client(&user_agent);
        Self {
            client,
            user_agent,
            servers: Mutex::new(Vec::new()),
            pinned_servers: false,
        }
    }

    /// Creates a client configured with an explicit list of mirror base URLs (in order,
    /// without random shuffling or DNS lookup). Ideal for deterministic tests against fake servers.
    pub fn with_servers(servers: Vec<String>, user_agent: impl Into<String>) -> Self {
        let user_agent = Self::sanitize_user_agent(user_agent.into());
        let client = Self::build_http_client(&user_agent);
        let normalized: Vec<String> = servers
            .into_iter()
            .filter_map(|s| normalize_base_url(&s))
            .collect();
        Self {
            client,
            user_agent,
            servers: Mutex::new(normalized),
            pinned_servers: true,
        }
    }

    /// Returns the User-Agent string sent with every request.
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// Returns a snapshot of the currently configured/discovered mirror base URLs.
    pub fn servers(&self) -> Vec<String> {
        self.servers.lock().clone()
    }

    fn sanitize_user_agent(ua: String) -> String {
        let trimmed = ua.trim();
        if trimmed.is_empty() {
            concat!("Luminous/", env!("CARGO_PKG_VERSION")).to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn build_http_client(user_agent: &str) -> Client {
        let mut headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(user_agent) {
            headers.insert(USER_AGENT, val);
        }
        Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default()
    }

    /// Discovers Radio Browser mirror base URLs by querying
    /// `https://all.api.radio-browser.info/json/servers` (falling back to DNS
    /// resolution of `all.api.radio-browser.info` if needed), and shuffles the
    /// resulting `https://<name>` mirror list.
    pub async fn discover_servers(&self) -> Result<Vec<String>, String> {
        let primary_servers_url = format!("https://{RADIO_BROWSER_DNS_HOST}/json/servers");
        if let Ok(resp) = self.client.get(&primary_servers_url).send().await {
            if resp.status().is_success() {
                if let Ok(list) = resp.json::<Vec<WireServerDescriptor>>().await {
                    let host_urls = extract_unique_server_urls(list);
                    if !host_urls.is_empty() {
                        return Ok(host_urls);
                    }
                }
            }
        }

        let addrs = tokio::net::lookup_host((RADIO_BROWSER_DNS_HOST, 80))
            .await
            .map_err(|e| format!("DNS lookup for {RADIO_BROWSER_DNS_HOST} failed: {e}"))?;

        let mut seen_ips = HashSet::new();
        let mut ip_urls = Vec::new();
        for sock_addr in addrs {
            let ip = sock_addr.ip();
            if seen_ips.insert(ip) {
                ip_urls.push(format_ip_base_url(ip));
            }
        }

        if ip_urls.is_empty() {
            return Err(format!(
                "DNS lookup for {RADIO_BROWSER_DNS_HOST} returned no addresses"
            ));
        }

        {
            let mut rng = rand::rng();
            ip_urls.shuffle(&mut rng);
        }

        // Try to resolve canonical HTTPS mirror hostnames from `/json/servers` on the first
        // responsive IP, sending the virtual-host `Host` header.
        for ip_base in &ip_urls {
            let url = format!("{ip_base}/json/servers");
            if let Ok(resp) = self
                .client
                .get(&url)
                .header(reqwest::header::HOST, RADIO_BROWSER_DNS_HOST)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(list) = resp.json::<Vec<WireServerDescriptor>>().await {
                        let host_urls = extract_unique_server_urls(list);
                        if !host_urls.is_empty() {
                            return Ok(host_urls);
                        }
                    }
                }
            }
        }

        Ok(vec![format!("https://{RADIO_BROWSER_DNS_HOST}")])
    }

    async fn ensure_servers(&self) -> Result<Vec<String>, String> {
        {
            let guard = self.servers.lock();
            if !guard.is_empty() {
                return Ok(guard.clone());
            }
        }
        if self.pinned_servers {
            return Err("No Radio Browser servers configured".to_string());
        }
        let discovered = self.discover_servers().await?;
        let mut guard = self.servers.lock();
        if guard.is_empty() {
            *guard = discovered.clone();
        }
        Ok(guard.clone())
    }

    /// Executes a JSON GET request against `path_and_query` (e.g. `"/json/tags"`),
    /// rotating across discovered servers on transport or HTTP error.
    async fn get_json_with_failover<T>(&self, path_and_query: &str) -> Result<T, String>
    where
        T: serde::de::DeserializeOwned,
    {
        let candidates = self.ensure_servers().await?;
        let mut last_err = String::from("No Radio Browser servers available");

        for (idx, base_url) in candidates.iter().enumerate() {
            let full_url = format!("{base_url}{path_and_query}");
            match self.client.get(&full_url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        last_err = format!("Server {base_url} returned HTTP {status}");
                        log::debug!("Radio Browser mirror failed ({last_err}), trying next mirror");
                        continue;
                    }
                    match resp.json::<T>().await {
                        Ok(parsed) => {
                            if idx > 0 {
                                // Promote the working server to the front of the list
                                // so subsequent calls don't keep hitting dead mirrors first.
                                let mut guard = self.servers.lock();
                                if let Some(pos) = guard.iter().position(|s| s == base_url) {
                                    guard.rotate_left(pos);
                                }
                            }
                            return Ok(parsed);
                        }
                        Err(e) => {
                            last_err = format!(
                                "Failed to parse JSON from {base_url}{path_and_query}: {e}"
                            );
                            log::debug!(
                                "Radio Browser mirror returned invalid JSON ({last_err}), trying next mirror"
                            );
                        }
                    }
                }
                Err(e) => {
                    last_err = format!("Request to {base_url} failed: {e}");
                    log::debug!(
                        "Radio Browser mirror unreachable ({last_err}), trying next mirror"
                    );
                }
            }
        }

        // If all cached DNS mirrors failed and servers are not pinned, clear the cache
        // so the next attempt re-runs DNS discovery.
        if !self.pinned_servers {
            self.servers.lock().clear();
        }

        Err(last_err)
    }

    /// Fetches one page of working stations (`/json/stations?hidebroken=true&offset=...&limit=...`).
    pub async fn fetch_stations_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<StationsPage, String> {
        let path = format!(
            "/json/stations?hidebroken=true&order=stationuuid&offset={offset}&limit={limit}"
        );
        let raw: Vec<WireStation> = self.get_json_with_failover(&path).await?;
        let raw_count = raw.len();
        Ok(StationsPage {
            stations: raw
                .into_iter()
                .filter_map(WireStation::into_normalized)
                .collect(),
            raw_count,
        })
    }

    /// Fetches specific stations by UUID (`/json/stations/byuuid?uuids=...`),
    /// chunking automatically when more than 200 UUIDs are requested.
    pub async fn fetch_stations_by_uuids(
        &self,
        uuids: &[String],
    ) -> Result<Vec<RadioBrowserStation>, String> {
        if uuids.is_empty() {
            return Ok(Vec::new());
        }
        let mut all = Vec::with_capacity(uuids.len());
        for chunk in uuids.chunks(BY_UUID_CHUNK_SIZE) {
            let joined: String = chunk
                .iter()
                .map(|u| u.trim())
                .filter(|u| !u.is_empty())
                .collect::<Vec<_>>()
                .join(",");
            if joined.is_empty() {
                continue;
            }
            let encoded: String =
                percent_encoding::utf8_percent_encode(&joined, QUERY_ENCODE_SET).to_string();
            let path = format!("/json/stations/byuuid?uuids={encoded}");
            let raw: Vec<WireStation> = self.get_json_with_failover(&path).await?;
            all.extend(raw.into_iter().filter_map(WireStation::into_normalized));
        }
        Ok(all)
    }

    /// Fetches incremental station changes since `last_change_uuid` via
    /// `/json/stations/changed?lastchangeuuid=...&limit=...` and resolves full station
    /// details via `/json/stations/byuuid`.
    pub async fn fetch_changed_stations(
        &self,
        last_change_uuid: &str,
        limit: usize,
    ) -> Result<ChangedStationsBatch, String> {
        let clean_cursor = last_change_uuid.trim();
        if clean_cursor.is_empty() {
            return Err("last_change_uuid cannot be empty".to_string());
        }
        let encoded_cursor: String =
            percent_encoding::utf8_percent_encode(clean_cursor, QUERY_ENCODE_SET).to_string();
        let path = format!("/json/stations/changed?lastchangeuuid={encoded_cursor}&limit={limit}");
        let headers: Vec<WireChangeHeader> = self.get_json_with_failover(&path).await?;
        let raw_change_count = headers.len();
        if headers.is_empty() {
            return Ok(ChangedStationsBatch {
                stations: Vec::new(),
                newest_change_uuid: None,
                oldest_change_iso8601: None,
                raw_change_count: 0,
                cursor_valid: true,
            });
        }

        let oldest_change_iso8601 = headers
            .iter()
            .filter_map(|h| h.lastchangetime_iso8601.as_deref())
            .map(str::trim)
            .find(|s| !s.is_empty())
            .map(str::to_string);

        let newest_change_uuid = headers
            .iter()
            .rev()
            .map(|h| h.changeuuid.trim().to_ascii_lowercase())
            .find(|u| !u.is_empty());

        let mut seen = HashSet::new();
        let mut unique_station_uuids = Vec::new();
        for header in headers {
            let suuid = header.stationuuid.trim().to_ascii_lowercase();
            if !suuid.is_empty() && seen.insert(suuid.clone()) {
                unique_station_uuids.push(suuid);
            }
        }

        let stations = self.fetch_stations_by_uuids(&unique_station_uuids).await?;
        let cursor_valid = raw_change_count < limit;

        Ok(ChangedStationsBatch {
            stations,
            newest_change_uuid,
            oldest_change_iso8601,
            raw_change_count,
            cursor_valid,
        })
    }

    async fn fetch_facet_list<F>(
        &self,
        path: &str,
        map_entry: F,
    ) -> Result<Vec<RadioBrowserFacetEntry>, String>
    where
        F: Fn(WireFacetEntry) -> Option<RadioBrowserFacetEntry>,
    {
        let raw: Vec<WireFacetEntry> = self.get_json_with_failover(path).await?;
        Ok(raw.into_iter().filter_map(map_entry).collect())
    }

    /// Fetches tag counts from `/json/tags?hidebroken=true`.
    pub async fn fetch_tags(&self) -> Result<Vec<RadioBrowserFacetEntry>, String> {
        self.fetch_facet_list("/json/tags?hidebroken=true", |e| {
            let name = e.name.trim().to_lowercase();
            (!name.is_empty()).then_some(RadioBrowserFacetEntry {
                name,
                code: None,
                stationcount: e.stationcount.unwrap_or(0),
            })
        })
        .await
    }

    /// Fetches country counts from `/json/countries?hidebroken=true`.
    pub async fn fetch_countries(&self) -> Result<Vec<RadioBrowserFacetEntry>, String> {
        self.fetch_facet_list("/json/countries?hidebroken=true", |e| {
            let name = e.name.trim().to_string();
            let code = e
                .iso_3166_1
                .map(|c| c.trim().to_ascii_uppercase())
                .filter(|c| !c.is_empty());
            (!name.is_empty() || code.is_some()).then_some(RadioBrowserFacetEntry {
                name,
                code,
                stationcount: e.stationcount.unwrap_or(0),
            })
        })
        .await
    }

    /// Fetches language counts from `/json/languages?hidebroken=true`.
    pub async fn fetch_languages(&self) -> Result<Vec<RadioBrowserFacetEntry>, String> {
        self.fetch_facet_list("/json/languages?hidebroken=true", |e| {
            let name = e.name.trim().to_lowercase();
            (!name.is_empty()).then_some(RadioBrowserFacetEntry {
                name,
                code: e
                    .iso_639
                    .map(|c| c.trim().to_ascii_lowercase())
                    .filter(|c| !c.is_empty()),
                stationcount: e.stationcount.unwrap_or(0),
            })
        })
        .await
    }

    /// Fetches codec counts from `/json/codecs?hidebroken=true`.
    pub async fn fetch_codecs(&self) -> Result<Vec<RadioBrowserFacetEntry>, String> {
        self.fetch_facet_list("/json/codecs?hidebroken=true", |e| {
            let name = e.name.trim().to_ascii_uppercase();
            (!name.is_empty()).then_some(RadioBrowserFacetEntry {
                name,
                code: None,
                stationcount: e.stationcount.unwrap_or(0),
            })
        })
        .await
    }

    /// Reports a station play click (`/json/url/{stationuuid}`) and returns the resolved stream URL.
    pub async fn report_station_click(&self, stationuuid: &str) -> Result<String, String> {
        let clean = stationuuid.trim();
        if clean.is_empty() {
            return Err("stationuuid cannot be empty".to_string());
        }
        let encoded: String =
            percent_encoding::utf8_percent_encode(clean, QUERY_ENCODE_SET).to_string();
        let path = format!("/json/url/{encoded}");
        let resp: WireClickResponse = self.get_json_with_failover(&path).await?;
        if !resp.ok {
            return Err(resp
                .message
                .unwrap_or_else(|| "Station click report failed".to_string()));
        }
        Ok(resp.url.unwrap_or_default().trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn rotates_to_next_server_on_failure_and_promotes_working_mirror() {
        let dead_server = MockServer::start().await;
        let working_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/json/tags"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&dead_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/json/tags"))
            .and(query_param("hidebroken", "true"))
            .and(header("user-agent", "Luminous/4.0.0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "Jazz", "stationcount": 420 },
                { "name": "Folk Metal", "stationcount": 37 }
            ])))
            .expect(2)
            .mount(&working_server)
            .await;

        let client = RadioBrowserClient::with_servers(
            vec![dead_server.uri(), working_server.uri()],
            "Luminous/4.0.0",
        );

        let tags = client
            .fetch_tags()
            .await
            .expect("failover to second server");
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "jazz");
        assert_eq!(tags[0].stationcount, 420);

        // The working mirror should now be promoted to index 0 so the second call
        // doesn't hit dead_server again (verified by dead_server's `.expect(1)`).
        assert_eq!(client.servers()[0], working_server.uri());
        let tags_again = client
            .fetch_tags()
            .await
            .expect("second call hits working server first");
        assert_eq!(tags_again.len(), 2);
    }

    #[tokio::test]
    async fn fetches_paged_stations_and_normalizes_wire_fields() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/json/stations"))
            .and(query_param("hidebroken", "true"))
            .and(query_param("offset", "0"))
            .and(query_param("limit", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "stationuuid": "FFC0D701-F43B-4232-8BE6-F7FF72ADD8EF",
                    "changeuuid": "4AF32ABE-0C71-4CCF-94A8-62387CAD17B2",
                    "name": "\tBlasmusikradio mit Bernd\n",
                    "url": "https://stream.laut.fm/blasmusik.pls",
                    "url_resolved": "https://stream.laut.fm/blasmusik",
                    "homepage": " https://laut.fm/blasmusik ",
                    "favicon": "",
                    "tags": "Folk, polka ,folk,,Marsch ",
                    "country": "Germany",
                    "countrycode": "de",
                    "language": "German, DE ",
                    "codec": "mp3",
                    "bitrate": 128,
                    "hls": 0,
                    "votes": 294,
                    "clickcount": 51,
                    "lastcheckok": 1,
                    "lastchangetime_iso8601": "2026-01-15T01:36:03Z"
                },
                {
                    "stationuuid": "9afeaf65-bb57-4d76-892d-c7a39fc6c805",
                    "changeuuid": "74e2f753-ec87-402c-ad46-f4c78aec58c2",
                    "name": "HLS Stream Fallback URL",
                    "url": "https://example.com/live.m3u8",
                    "url_resolved": "",
                    "tags": "rock",
                    "country": "Canada",
                    "countrycode": "CA",
                    "language": "english",
                    "codec": "AAC",
                    "bitrate": 64,
                    "hls": 1,
                    "votes": 10,
                    "clickcount": 5,
                    "lastcheckok": true
                }
            ])))
            .mount(&server)
            .await;

        let client = RadioBrowserClient::with_servers(vec![server.uri()], "Luminous/4.0.0");
        let page = client
            .fetch_stations_page(0, 2)
            .await
            .expect("fetch page 0");
        assert_eq!(page.raw_count, 2);
        let page = page.stations;
        assert_eq!(page.len(), 2);

        let s0 = &page[0];
        assert_eq!(s0.stationuuid, "ffc0d701-f43b-4232-8be6-f7ff72add8ef");
        assert_eq!(s0.changeuuid, "4af32abe-0c71-4ccf-94a8-62387cad17b2");
        assert_eq!(s0.name, "Blasmusikradio mit Bernd");
        assert_eq!(s0.url_resolved, "https://stream.laut.fm/blasmusik");
        assert_eq!(s0.tags, "folk,polka,marsch");
        assert_eq!(s0.countrycode, "DE");
        assert_eq!(s0.language, "german,de");
        assert_eq!(s0.codec, "MP3");
        assert!(!s0.hls);
        assert!(s0.lastcheckok);

        let s1 = &page[1];
        assert_eq!(s1.url_resolved, "https://example.com/live.m3u8");
        assert!(s1.hls);
    }

    #[tokio::test]
    async fn fetch_changed_stations_resolves_full_records_via_byuuid() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/json/stations/changed"))
            .and(query_param("lastchangeuuid", "c0"))
            .and(query_param("limit", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "changeuuid": "c1",
                    "stationuuid": "s1",
                    "name": "Station One Old"
                },
                {
                    "changeuuid": "c2",
                    "stationuuid": "s1",
                    "name": "Station One New"
                },
                {
                    "changeuuid": "c3",
                    "stationuuid": "s2",
                    "name": "Station Two"
                }
            ])))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/json/stations/byuuid"))
            .and(query_param("uuids", "s1,s2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "stationuuid": "s1",
                    "changeuuid": "c2",
                    "name": "Station One New",
                    "url_resolved": "https://one.example/stream",
                    "codec": "MP3",
                    "bitrate": 192,
                    "hls": 0,
                    "lastcheckok": 1
                },
                {
                    "stationuuid": "s2",
                    "changeuuid": "c3",
                    "name": "Station Two",
                    "url_resolved": "https://two.example/stream",
                    "codec": "OGG",
                    "bitrate": 128,
                    "hls": 0,
                    "lastcheckok": 0
                }
            ])))
            .mount(&server)
            .await;

        let client = RadioBrowserClient::with_servers(vec![server.uri()], "Luminous/4.0.0");
        let batch = client
            .fetch_changed_stations("c0", 100)
            .await
            .expect("fetch changed stations");

        assert!(batch.cursor_valid);
        assert_eq!(batch.raw_change_count, 3);
        assert_eq!(batch.newest_change_uuid.as_deref(), Some("c3"));
        assert_eq!(batch.stations.len(), 2);
        assert_eq!(batch.stations[0].url_resolved, "https://one.example/stream");
        assert!(batch.stations[0].lastcheckok);
        assert!(!batch.stations[1].lastcheckok);
    }

    #[tokio::test]
    async fn fetch_facets_and_report_click_work() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/json/countries"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "Canada", "iso_3166_1": "ca", "stationcount": 900 }
            ])))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/json/languages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "English", "iso_639": "EN", "stationcount": 15000 }
            ])))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/json/codecs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "mp3", "stationcount": 32000 }
            ])))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/json/url/station-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "message": "retrieved station url",
                "stationuuid": "station-123",
                "name": "Test FM",
                "url": "https://stream.example.com/live.mp3"
            })))
            .mount(&server)
            .await;

        let client = RadioBrowserClient::with_servers(vec![server.uri()], "Luminous/4.0.0");

        let countries = client.fetch_countries().await.unwrap();
        assert_eq!(countries[0].code.as_deref(), Some("CA"));

        let langs = client.fetch_languages().await.unwrap();
        assert_eq!(langs[0].name, "english");
        assert_eq!(langs[0].code.as_deref(), Some("en"));

        let codecs = client.fetch_codecs().await.unwrap();
        assert_eq!(codecs[0].name, "MP3");

        let url = client.report_station_click("station-123").await.unwrap();
        assert_eq!(url, "https://stream.example.com/live.mp3");
    }
}
