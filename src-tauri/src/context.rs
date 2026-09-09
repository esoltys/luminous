//! Live "context" enrichment for the Details pane — MusicBrainz release-group
//! ratings/genres/tags, an artist's Wikidata-linked Wikipedia bio, and
//! CritiqueBrainz reviews. Each fetch function here is independently
//! fallible; callers (see `commands::context::get_song_context`) treat a
//! failure in one source as "nothing from that source" rather than failing
//! the whole request, so one dead API never blanks out the rest of the panel.
//!
//! TheAudioDB is deliberately not included: its free tier shares a single
//! rate-limited test key across every app that uses it, which doesn't scale
//! to Luminous's whole userbase hitting it at once (see issue #23).

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::watch;

// ---------------------------------------------------------------------------
// In-flight request coalescer (SingleFlight) — ensures that concurrent
// calls for the same artist or release group share a single in-flight operation
// rather than duplicating network requests or racing writes.
// ---------------------------------------------------------------------------

struct FlightGuard<T: Clone> {
    in_flight: Arc<Mutex<HashMap<String, watch::Receiver<Option<T>>>>>,
    key: String,
}

impl<T: Clone> Drop for FlightGuard<T> {
    fn drop(&mut self) {
        let mut map = self.in_flight.lock();
        map.remove(&self.key);
    }
}

#[derive(Clone, Default)]
pub struct FlightGroup<T: Clone> {
    in_flight: Arc<Mutex<HashMap<String, watch::Receiver<Option<T>>>>>,
}

impl<T: Clone + Send + 'static> FlightGroup<T> {
    pub fn new() -> Self {
        Self {
            in_flight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn work<F, Fut>(&self, key: &str, f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        enum FlightAction<T> {
            Wait(watch::Receiver<Option<T>>),
            Leader(watch::Sender<Option<T>>),
        }

        let action = {
            let mut map = self.in_flight.lock();
            if let Some(rx) = map.get(key) {
                FlightAction::Wait(rx.clone())
            } else {
                let (tx, rx) = watch::channel(None);
                map.insert(key.to_string(), rx);
                FlightAction::Leader(tx)
            }
        };

        match action {
            FlightAction::Leader(tx) => {
                let _guard = FlightGuard {
                    in_flight: self.in_flight.clone(),
                    key: key.to_string(),
                };
                let result = f().await;
                let _ = tx.send(Some(result.clone()));
                result
            }
            FlightAction::Wait(mut rx) => {
                while rx.borrow().is_none() {
                    if rx.changed().await.is_err() {
                        break;
                    }
                }
                if let Some(val) = rx.borrow().clone() {
                    return val;
                }
                f().await
            }
        }
    }
}

pub static ARTIST_FLIGHT: LazyLock<FlightGroup<Result<Option<WikipediaSummary>, String>>> =
    LazyLock::new(FlightGroup::new);

pub static RELEASE_GROUP_FLIGHT: LazyLock<
    FlightGroup<(
        Result<MusicBrainzReleaseGroupData, String>,
        Result<CritiqueBrainzData, String>,
    )>,
> = LazyLock::new(FlightGroup::new);

// ---------------------------------------------------------------------------
// MusicBrainz rate limiting — MetaBrainz asks for roughly one request per
// second per client. This is a process-global constraint (their servers
// don't care which `ContextManager` instance made the call), so the limiter
// is a module-level static rather than per-instance state.
// ---------------------------------------------------------------------------

const MB_MIN_INTERVAL: Duration = Duration::from_millis(1100);

static MB_NEXT_SLOT: LazyLock<Mutex<Instant>> = LazyLock::new(|| Mutex::new(Instant::now()));

/// Reserves the next available MusicBrainz request slot and returns how long
/// the caller should wait before sending. Pure w.r.t. `last`/`now`/`min_interval`
/// so the serialization behavior is unit-testable without real sleeping —
/// see `test_reserve_next_slot_serializes_calls`.
fn reserve_next_slot(last: &mut Instant, now: Instant, min_interval: Duration) -> Duration {
    let next_available = if *last > now { *last } else { now };
    let wait = next_available.saturating_duration_since(now);
    *last = next_available + min_interval;
    wait
}

async fn throttle_musicbrainz() {
    let wait = {
        let mut slot = MB_NEXT_SLOT.lock();
        reserve_next_slot(&mut slot, Instant::now(), MB_MIN_INTERVAL)
    };
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
}

// ---------------------------------------------------------------------------
// Public result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MusicBrainzReleaseGroupData {
    pub rating: Option<f32>,
    pub rating_votes: Option<u32>,
    /// Merged, de-duplicated genres + tags, most-tagged first, capped to a
    /// UI-friendly count.
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CritiqueBrainzData {
    pub average_rating: Option<f32>,
    pub review_count: u32,
    pub review_links: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WikipediaSummary {
    pub extract: String,
    pub page_url: Option<String>,
    pub thumbnail_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Wire-format structs (kept `Option`/`#[serde(default)]`-heavy so an
// unexpected/missing field degrades to "no data" instead of a parse error).
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug, Default)]
struct MbRating {
    value: Option<f32>,
    #[serde(rename = "votes-count")]
    votes_count: Option<u32>,
}

#[derive(Deserialize, Debug, Default)]
struct MbTagOrGenre {
    name: String,
    #[serde(default)]
    count: i64,
}

#[derive(Deserialize, Debug, Default)]
struct MbReleaseGroupResponse {
    #[serde(default)]
    rating: Option<MbRating>,
    #[serde(default)]
    tags: Vec<MbTagOrGenre>,
    #[serde(default)]
    genres: Vec<MbTagOrGenre>,
}

#[derive(Deserialize, Debug, Default)]
struct MbUrlRef {
    resource: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct MbRelation {
    #[serde(rename = "type")]
    rel_type: Option<String>,
    #[serde(default)]
    url: Option<MbUrlRef>,
}

#[derive(Deserialize, Debug, Default)]
struct MbArtistResponse {
    #[serde(default)]
    relations: Vec<MbRelation>,
}

#[derive(Deserialize, Debug, Default)]
struct WikidataSitelink {
    title: String,
}

#[derive(Deserialize, Debug, Default)]
struct WikidataEntity {
    #[serde(default)]
    sitelinks: HashMap<String, WikidataSitelink>,
}

#[derive(Deserialize, Debug, Default)]
struct WikidataEntityData {
    #[serde(default)]
    entities: HashMap<String, WikidataEntity>,
}

#[derive(Deserialize, Debug, Default)]
struct WikipediaDesktopUrls {
    page: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct WikipediaContentUrls {
    desktop: Option<WikipediaDesktopUrls>,
}

#[derive(Deserialize, Debug, Default)]
struct WikipediaThumbnail {
    source: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct WikipediaSummaryResponse {
    extract: Option<String>,
    #[serde(default)]
    content_urls: Option<WikipediaContentUrls>,
    #[serde(default)]
    thumbnail: Option<WikipediaThumbnail>,
}

#[derive(Deserialize, Debug, Default)]
struct CritiqueBrainzAverageRating {
    rating: Option<f32>,
}

#[derive(Deserialize, Debug, Default)]
struct CritiqueBrainzReview {
    id: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct CritiqueBrainzResponse {
    #[serde(default)]
    count: u32,
    #[serde(default)]
    average_rating: Option<CritiqueBrainzAverageRating>,
    #[serde(default)]
    reviews: Vec<CritiqueBrainzReview>,
}

/// Extracts a Wikidata QID (e.g. `"Q11649"`) from a `wikidata` relation's
/// resource URL (`https://www.wikidata.org/wiki/Q11649`).
fn extract_wikidata_qid(resource_url: &str) -> Option<String> {
    let candidate = resource_url.rsplit('/').next()?;
    let candidate = candidate.trim();
    if candidate.len() > 1
        && candidate.starts_with('Q')
        && candidate[1..].chars().all(|c| c.is_ascii_digit())
    {
        Some(candidate.to_string())
    } else {
        None
    }
}

/// Merges MusicBrainz genres + tags into one de-duplicated list, most-used
/// first, capped so the UI never has to render an unbounded chip list.
fn merge_tags(genres: Vec<MbTagOrGenre>, tags: Vec<MbTagOrGenre>, cap: usize) -> Vec<String> {
    let mut merged: Vec<MbTagOrGenre> = genres;
    for tag in tags {
        if !merged.iter().any(|g| g.name.eq_ignore_ascii_case(&tag.name)) {
            merged.push(tag);
        }
    }
    merged.sort_by(|a, b| b.count.cmp(&a.count));
    merged.into_iter().take(cap).map(|t| t.name).collect()
}

/// True when a cached row's `fetched_at` (unix seconds) is still within the
/// 30-day TTL relative to `now` (unix seconds). Pure so it's testable
/// without touching the database or wall-clock time.
pub fn is_cache_fresh(fetched_at: i64, now: i64) -> bool {
    const TTL_SECONDS: i64 = 30 * 24 * 3600;
    now.saturating_sub(fetched_at) < TTL_SECONDS
}

/// Holds the shared HTTP client used for every source. Cheap to construct
/// (no state beyond the client), so callers can create one per-lookup rather
/// than needing to share an instance — matches `LyricsManager`.
#[derive(Clone)]
pub struct ContextManager {
    client: Client,
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextManager {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(6))
                .user_agent(concat!("LuminousMusicPlayer/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Release-group ratings + merged genres/tags. `inc=ratings+tags+genres`
    /// needs no auth beyond the User-Agent header; throttled to MusicBrainz's
    /// ~1 req/sec limit.
    pub async fn fetch_musicbrainz_release_group(
        &self,
        release_group_id: &str,
    ) -> Result<MusicBrainzReleaseGroupData> {
        throttle_musicbrainz().await;
        let url = format!(
            "https://musicbrainz.org/ws/2/release-group/{}?inc=ratings+tags+genres&fmt=json",
            percent_encoding::utf8_percent_encode(
                release_group_id,
                percent_encoding::NON_ALPHANUMERIC
            )
        );
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "MusicBrainz release-group lookup failed: HTTP {}",
                response.status()
            ));
        }
        let parsed: MbReleaseGroupResponse = response.json().await?;
        Ok(MusicBrainzReleaseGroupData {
            rating: parsed.rating.as_ref().and_then(|r| r.value),
            rating_votes: parsed.rating.as_ref().and_then(|r| r.votes_count),
            tags: merge_tags(parsed.genres, parsed.tags, 12),
        })
    }

    /// Looks up the artist's `wikidata` URL relation, if any. Returns `None`
    /// (not an error) when the artist simply has no such relation in
    /// MusicBrainz — that's the common case, not a failure.
    pub async fn fetch_musicbrainz_artist_wikidata_id(
        &self,
        artist_id: &str,
    ) -> Result<Option<String>> {
        throttle_musicbrainz().await;
        let url = format!(
            "https://musicbrainz.org/ws/2/artist/{}?inc=url-rels&fmt=json",
            percent_encoding::utf8_percent_encode(artist_id, percent_encoding::NON_ALPHANUMERIC)
        );
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "MusicBrainz artist lookup failed: HTTP {}",
                response.status()
            ));
        }
        let parsed: MbArtistResponse = response.json().await?;
        let qid = parsed
            .relations
            .into_iter()
            .find(|r| r.rel_type.as_deref() == Some("wikidata"))
            .and_then(|r| r.url)
            .and_then(|u| u.resource)
            .and_then(|resource| extract_wikidata_qid(&resource));
        Ok(qid)
    }

    /// Resolves a Wikidata QID to its English Wikipedia article title via
    /// `sitelinks.enwiki.title`. Returns `None` when the entity has no
    /// English Wikipedia article.
    async fn resolve_wikidata_to_wikipedia_title(&self, wikidata_id: &str) -> Result<Option<String>> {
        let url = format!(
            "https://www.wikidata.org/wiki/Special:EntityData/{}.json",
            percent_encoding::utf8_percent_encode(wikidata_id, percent_encoding::NON_ALPHANUMERIC)
        );
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "Wikidata entity lookup failed: HTTP {}",
                response.status()
            ));
        }
        let parsed: WikidataEntityData = response.json().await?;
        Ok(parsed
            .entities
            .get(wikidata_id)
            .and_then(|e| e.sitelinks.get("enwiki"))
            .map(|s| s.title.clone()))
    }

    /// Full chain: artist MusicBrainz ID -> Wikidata QID -> Wikipedia title
    /// -> summary. Any missing link along the way yields `Ok(None)`, not an
    /// error — most artists simply won't have a Wikidata/Wikipedia entry.
    pub async fn fetch_wikipedia_bio_for_artist(
        &self,
        artist_id: &str,
    ) -> Result<Option<WikipediaSummary>> {
        let Some(wikidata_id) = self.fetch_musicbrainz_artist_wikidata_id(artist_id).await? else {
            return Ok(None);
        };
        let Some(title) = self
            .resolve_wikidata_to_wikipedia_title(&wikidata_id)
            .await?
        else {
            return Ok(None);
        };
        self.fetch_wikipedia_summary(&title).await.map(Some)
    }

    async fn fetch_wikipedia_summary(&self, title: &str) -> Result<WikipediaSummary> {
        let url = format!(
            "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
            percent_encoding::utf8_percent_encode(title, percent_encoding::NON_ALPHANUMERIC)
        );
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "Wikipedia summary lookup failed: HTTP {}",
                response.status()
            ));
        }
        let parsed: WikipediaSummaryResponse = response.json().await?;
        let extract = parsed.extract.unwrap_or_default();
        if extract.trim().is_empty() {
            return Err(anyhow!("Wikipedia summary had no extract text"));
        }
        Ok(WikipediaSummary {
            extract,
            page_url: parsed.content_urls.and_then(|c| c.desktop).and_then(|d| d.page),
            thumbnail_url: parsed.thumbnail.and_then(|t| t.source),
        })
    }

    /// Aggregate rating + a handful of review links for a release-group.
    /// CritiqueBrainz has no MusicBrainz-style rate limit; a single lookup
    /// per song view doesn't need throttling.
    pub async fn fetch_critiquebrainz_reviews(
        &self,
        release_group_id: &str,
    ) -> Result<CritiqueBrainzData> {
        let url = format!(
            "https://critiquebrainz.org/ws/1/review/?entity_id={}&entity_type=release_group&limit=5",
            percent_encoding::utf8_percent_encode(
                release_group_id,
                percent_encoding::NON_ALPHANUMERIC
            )
        );
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "CritiqueBrainz review lookup failed: HTTP {}",
                response.status()
            ));
        }
        let parsed: CritiqueBrainzResponse = response.json().await?;
        let review_links = parsed
            .reviews
            .iter()
            .filter_map(|r| r.id.as_ref())
            .map(|id| format!("https://critiquebrainz.org/review/{id}"))
            .collect();
        Ok(CritiqueBrainzData {
            average_rating: parsed.average_rating.and_then(|a| a.rating),
            review_count: parsed.count,
            review_links,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_wikidata_qid() {
        assert_eq!(
            extract_wikidata_qid("https://www.wikidata.org/wiki/Q11649"),
            Some("Q11649".to_string())
        );
        assert_eq!(extract_wikidata_qid("https://www.wikidata.org/wiki/"), None);
        assert_eq!(
            extract_wikidata_qid("https://www.wikipedia.org/wiki/Nirvana"),
            None
        );
    }

    #[test]
    fn test_merge_tags_dedupes_prefers_genres_and_sorts_by_count() {
        let genres = vec![
            MbTagOrGenre { name: "progressive rock".to_string(), count: 42 },
            MbTagOrGenre { name: "art rock".to_string(), count: 17 },
        ];
        let tags = vec![
            MbTagOrGenre { name: "Progressive Rock".to_string(), count: 99 }, // dup, case-insensitive
            MbTagOrGenre { name: "concept album".to_string(), count: 5 },
        ];
        let merged = merge_tags(genres, tags, 12);
        assert_eq!(merged, vec!["progressive rock", "art rock", "concept album"]);
    }

    #[test]
    fn test_merge_tags_respects_cap() {
        let genres = vec![
            MbTagOrGenre { name: "a".to_string(), count: 3 },
            MbTagOrGenre { name: "b".to_string(), count: 2 },
            MbTagOrGenre { name: "c".to_string(), count: 1 },
        ];
        let merged = merge_tags(genres, vec![], 2);
        assert_eq!(merged, vec!["a", "b"]);
    }

    #[test]
    fn test_is_cache_fresh() {
        let now = 1_700_000_000_i64;
        assert!(is_cache_fresh(now - 60, now)); // 1 minute old
        assert!(is_cache_fresh(now - 29 * 24 * 3600, now)); // 29 days old
        assert!(!is_cache_fresh(now - 31 * 24 * 3600, now)); // 31 days old, stale
    }

    #[test]
    fn test_reserve_next_slot_serializes_calls() {
        let far_past = Instant::now() - Duration::from_secs(10);
        let mut last = far_past;
        let now = Instant::now();

        // A slot far enough in the past needs no wait.
        let wait1 = reserve_next_slot(&mut last, now, MB_MIN_INTERVAL);
        assert_eq!(wait1, Duration::ZERO);

        // An immediate second call must wait out the remainder of the window.
        let wait2 = reserve_next_slot(&mut last, now, MB_MIN_INTERVAL);
        assert_eq!(wait2, MB_MIN_INTERVAL);

        // A third call right after must wait for two full windows total.
        let wait3 = reserve_next_slot(&mut last, now, MB_MIN_INTERVAL);
        assert_eq!(wait3, MB_MIN_INTERVAL * 2);
    }

    #[test]
    fn test_musicbrainz_release_group_deserializes_captured_fixture() {
        // Captured from a live `inc=ratings+tags+genres` response shape.
        let json = r#"{
            "rating": {"votes-count": 105, "value": 4.75},
            "genres": [
                {"id": "608b0471-7531-4854-a348-e698c69cb699", "name": "ambient", "count": 3},
                {"id": "ae9b8279-3959-48d8-8a88-741a7f6d4a48", "name": "progressive rock", "count": 42}
            ],
            "tags": [
                {"name": "1973", "count": 1},
                {"name": "progressive rock", "count": 42}
            ]
        }"#;
        let parsed: MbReleaseGroupResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.rating.as_ref().unwrap().value, Some(4.75));
        assert_eq!(parsed.rating.as_ref().unwrap().votes_count, Some(105));
        let merged = merge_tags(parsed.genres, parsed.tags, 12);
        assert_eq!(merged, vec!["progressive rock", "ambient", "1973"]);
    }

    #[test]
    fn test_musicbrainz_artist_relations_deserializes_wikidata_relation() {
        let json = r#"{
            "relations": [
                {
                    "type": "wikidata",
                    "target-type": "url",
                    "url": {"resource": "https://www.wikidata.org/wiki/Q11649", "id": "1221730c-3a48-49fa-8001-beaa6e93c892"}
                }
            ]
        }"#;
        let parsed: MbArtistResponse = serde_json::from_str(json).unwrap();
        let qid = parsed
            .relations
            .into_iter()
            .find(|r| r.rel_type.as_deref() == Some("wikidata"))
            .and_then(|r| r.url)
            .and_then(|u| u.resource)
            .and_then(|resource| extract_wikidata_qid(&resource));
        assert_eq!(qid, Some("Q11649".to_string()));
    }

    #[test]
    fn test_wikidata_entity_data_resolves_enwiki_title() {
        let json = r#"{
            "entities": {
                "Q11649": {
                    "sitelinks": {
                        "enwiki": {"title": "Nirvana (band)"},
                        "frwiki": {"title": "Nirvana (groupe)"}
                    }
                }
            }
        }"#;
        let parsed: WikidataEntityData = serde_json::from_str(json).unwrap();
        let title = parsed
            .entities
            .get("Q11649")
            .and_then(|e| e.sitelinks.get("enwiki"))
            .map(|s| s.title.clone());
        assert_eq!(title, Some("Nirvana (band)".to_string()));
    }

    #[test]
    fn test_wikipedia_summary_deserializes_captured_fixture() {
        let json = r#"{
            "type": "standard",
            "title": "Nirvana (band)",
            "extract": "Nirvana was an American rock band formed in Aberdeen, Washington, in 1987.",
            "content_urls": {"desktop": {"page": "https://en.wikipedia.org/wiki/Nirvana_(band)"}},
            "thumbnail": {"source": "https://upload.wikimedia.org/thumb.jpg", "width": 330, "height": 311}
        }"#;
        let parsed: WikipediaSummaryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            parsed.extract.as_deref(),
            Some("Nirvana was an American rock band formed in Aberdeen, Washington, in 1987.")
        );
        assert_eq!(
            parsed.content_urls.unwrap().desktop.unwrap().page,
            Some("https://en.wikipedia.org/wiki/Nirvana_(band)".to_string())
        );
        assert_eq!(
            parsed.thumbnail.unwrap().source,
            Some("https://upload.wikimedia.org/thumb.jpg".to_string())
        );
    }

    #[test]
    fn test_critiquebrainz_response_deserializes_captured_fixture() {
        let json = r#"{
            "count": 2,
            "limit": 5,
            "offset": 0,
            "average_rating": {"rating": 3.8, "count": 2},
            "reviews": [
                {"id": "11111111-1111-1111-1111-111111111111", "entity_id": "x", "entity_type": "release_group", "rating": 4, "text": "great"},
                {"id": "22222222-2222-2222-2222-222222222222", "entity_id": "x", "entity_type": "release_group", "rating": 3, "text": "good"}
            ]
        }"#;
        let parsed: CritiqueBrainzResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.count, 2);
        assert_eq!(parsed.average_rating.unwrap().rating, Some(3.8));
        let links: Vec<String> = parsed
            .reviews
            .iter()
            .filter_map(|r| r.id.as_ref())
            .map(|id| format!("https://critiquebrainz.org/review/{id}"))
            .collect();
        assert_eq!(
            links,
            vec![
                "https://critiquebrainz.org/review/11111111-1111-1111-1111-111111111111",
                "https://critiquebrainz.org/review/22222222-2222-2222-2222-222222222222",
            ]
        );
    }

    #[tokio::test]
    async fn test_flight_group_deduplicates_concurrent_calls() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let group: FlightGroup<String> = FlightGroup::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let g1 = group.clone();
        let c1 = counter.clone();
        let t1 = tokio::spawn(async move {
            g1.work("key1", move || async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                c1.fetch_add(1, Ordering::SeqCst);
                "result_val".to_string()
            })
            .await
        });

        let g2 = group.clone();
        let c2 = counter.clone();
        let t2 = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            g2.work("key1", move || async move {
                c2.fetch_add(1, Ordering::SeqCst);
                "result_val".to_string()
            })
            .await
        });

        let (r1, r2) = tokio::join!(t1, t2);
        assert_eq!(r1.unwrap(), "result_val");
        assert_eq!(r2.unwrap(), "result_val");
        // Only one worker should have actually executed the inner work future
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
