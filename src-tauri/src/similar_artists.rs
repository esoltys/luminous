//! Multi-seed ListenBrainz similar-artist lookup and session cache (#2289).
//!
//! Resolves up to [`MAX_SEED_ARTISTS`] unique seed artists to MusicBrainz IDs,
//! fetches ListenBrainz Labs similar-artist scores in parallel for cache misses
//! (within a 2.5 s timeout per request), caches per-MBID responses for the
//! session, and merges results by keeping the highest `match` per artist.
//!
//! Callers (`radio.rs`, `continue_mix.rs`) pass seed artists and receive a
//! merged `HashMap<String, f64>` ready for `song_mix::Profile::set_similar_artists`.
//! Offline mode, missing MBIDs, and network/parse failures all resolve to an
//! empty map so callers fall back to local heuristics without error handling.

use crate::db::Database;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

/// Maximum number of distinct seed artists queried per mix generation.
pub const MAX_SEED_ARTISTS: usize = 3;

const LISTENBRAINZ_TIMEOUT: Duration = Duration::from_millis(2500);

/// Seed artist identity extracted from a song or artist name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistSeed {
    pub artist_key: String,
    pub musicbrainz_artist_id: Option<String>,
}

/// Session cache for ListenBrainz similar-artist lookups, keyed by lowercased
/// artist MBID. Empty responses are cached too so artists without ListenBrainz
/// data are not re-queried on every Auto Continue top-up.
#[derive(Debug, Default)]
pub struct SimilarArtistsCache {
    entries: Mutex<HashMap<String, HashMap<String, f64>>>,
}

impl SimilarArtistsCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn get(&self, mbid: &str) -> Option<HashMap<String, f64>> {
        self.entries.lock().get(mbid).cloned()
    }

    fn insert(&self, mbid: String, map: HashMap<String, f64>) {
        self.entries.lock().insert(mbid, map);
    }
}

/// Resolves an artist's MusicBrainz ID from a song tag candidate first, then
/// `artist_profiles.musicbrainz_artist_id` (case-insensitive).
fn resolve_artist_mbid(
    conn: &Connection,
    artist: &str,
    candidate_mbid: Option<&str>,
) -> Option<String> {
    if let Some(mbid) = candidate_mbid {
        let trimmed = mbid.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_lowercase());
        }
    }
    let trimmed_artist = artist.trim();
    if trimmed_artist.is_empty() {
        return None;
    }
    let profile_mbid: Option<String> = conn
        .query_row(
            "SELECT musicbrainz_artist_id FROM artist_profiles WHERE artist_key = ?1 COLLATE NOCASE",
            params![trimmed_artist],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    profile_mbid
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Deserialize)]
struct SimilarArtistResponseItem {
    pub artist_mbid: Option<String>,
    pub name: Option<String>,
    pub score: Option<f64>,
}

/// Fetches similar artists for one MBID from ListenBrainz Labs with a 2.5 s
/// timeout. Fails silently to an empty map so callers proceed with local
/// heuristics.
async fn fetch_listenbrainz_similar_artists(
    client: reqwest::Client,
    artist_mbid: String,
) -> HashMap<String, f64> {
    let url = format!(
        "https://labs.api.listenbrainz.org/similar-artists/json?artist_mbids={}&algorithm=session_based_days_7500_session_300_contribution_5_threshold_10_limit_100_filter_True_skip_30",
        artist_mbid
    );

    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return HashMap::new(),
    };

    let items: Vec<SimilarArtistResponseItem> = match resp.json().await {
        Ok(it) => it,
        Err(_) => return HashMap::new(),
    };

    let mut map = HashMap::new();
    let max_score = items.iter().filter_map(|i| i.score).fold(0.0f64, f64::max);
    for item in items {
        let norm = if max_score > 0.0 {
            (item.score.unwrap_or(0.0).max(0.0) / max_score).clamp(0.0, 1.0)
        } else {
            0.5
        };
        if let Some(name) = item.name {
            let n = name.trim().to_lowercase();
            if !n.is_empty() {
                map.entry(n)
                    .and_modify(|v: &mut f64| *v = v.max(norm))
                    .or_insert(norm);
            }
        }
        if let Some(mbid) = item.artist_mbid {
            let m = mbid.trim().to_lowercase();
            if !m.is_empty() {
                map.entry(m)
                    .and_modify(|v: &mut f64| *v = v.max(norm))
                    .or_insert(norm);
            }
        }
    }
    map
}

/// Merges per-seed similar-artist maps, keeping the highest `match` score for
/// each similar artist key.
fn merge_similar_artists(
    maps: impl IntoIterator<Item = HashMap<String, f64>>,
) -> HashMap<String, f64> {
    let mut merged = HashMap::new();
    for map in maps {
        for (key, score) in map {
            merged
                .entry(key)
                .and_modify(|v: &mut f64| *v = v.max(score))
                .or_insert(score);
        }
    }
    merged
}

/// Deduplicates MBIDs (preserving order), caps at [`MAX_SEED_ARTISTS`], serves
/// cache hits from `cache`, fetches misses in parallel via `fetch`, caches the
/// results, and merges all maps by max score.
async fn resolve_for_mbids_with<F, Fut>(
    cache: &SimilarArtistsCache,
    mbids: Vec<String>,
    fetch: F,
) -> HashMap<String, f64>
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = HashMap<String, f64>> + Send + 'static,
{
    let mut seen = HashSet::new();
    let mut unique_mbids = Vec::with_capacity(MAX_SEED_ARTISTS);
    for raw in mbids {
        let norm = raw.trim().to_lowercase();
        if norm.is_empty() || !seen.insert(norm.clone()) {
            continue;
        }
        unique_mbids.push(norm);
        if unique_mbids.len() >= MAX_SEED_ARTISTS {
            break;
        }
    }

    if unique_mbids.is_empty() {
        return HashMap::new();
    }

    let mut maps = Vec::with_capacity(unique_mbids.len());
    let mut misses = Vec::new();
    for mbid in unique_mbids {
        if let Some(cached) = cache.get(&mbid) {
            maps.push(cached);
        } else {
            misses.push(mbid);
        }
    }

    if !misses.is_empty() {
        let fetch = Arc::new(fetch);
        let mut join_set = JoinSet::new();
        for mbid in misses {
            let fetch = Arc::clone(&fetch);
            join_set.spawn(async move {
                let map = fetch(mbid.clone()).await;
                (mbid, map)
            });
        }
        while let Some(res) = join_set.join_next().await {
            if let Ok((mbid, map)) = res {
                cache.insert(mbid, map.clone());
                maps.push(map);
            }
        }
    }

    merge_similar_artists(maps)
}

/// Resolves up to [`MAX_SEED_ARTISTS`] unique seed artists' MBIDs, fetches
/// ListenBrainz similar artists in parallel for cache misses, and merges
/// results by keeping the highest `match` score per similar artist.
///
/// Returns an empty map when offline or when no MBIDs or matches are available.
pub async fn resolve_for_seeds(
    db: &Arc<Database>,
    cache: &SimilarArtistsCache,
    seeds: Vec<ArtistSeed>,
) -> HashMap<String, f64> {
    if seeds.is_empty() {
        return HashMap::new();
    }

    let mbids = match crate::db::run_blocking(db, move |conn| {
        if !crate::commands::context::is_online_enabled(conn) {
            return Ok(Vec::new());
        }
        let mut seen_artists = HashSet::new();
        let mut seen_mbids = HashSet::new();
        let mut resolved = Vec::with_capacity(MAX_SEED_ARTISTS);
        for seed in seeds {
            let artist_norm = seed.artist_key.trim().to_lowercase();
            if !artist_norm.is_empty() && !seen_artists.insert(artist_norm.clone()) {
                continue;
            }
            if let Some(mbid) = resolve_artist_mbid(
                conn,
                &seed.artist_key,
                seed.musicbrainz_artist_id.as_deref(),
            ) {
                if seen_mbids.insert(mbid.clone()) {
                    resolved.push(mbid);
                    if resolved.len() >= MAX_SEED_ARTISTS {
                        break;
                    }
                }
            }
        }
        Ok(resolved)
    })
    .await
    {
        Ok(m) => m,
        Err(_) => return HashMap::new(),
    };

    if mbids.is_empty() {
        return HashMap::new();
    }

    let client = match reqwest::Client::builder()
        .timeout(LISTENBRAINZ_TIMEOUT)
        .user_agent(concat!("LuminousMusicPlayer/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    resolve_for_mbids_with(cache, mbids, move |mbid| {
        fetch_listenbrainz_similar_artists(client.clone(), mbid)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn merge_keeps_highest_match_across_seeds() {
        let map_a = HashMap::from([("shared".to_string(), 0.4), ("only_a".to_string(), 0.7)]);
        let map_b = HashMap::from([("shared".to_string(), 0.9), ("only_b".to_string(), 0.6)]);
        let map_c = HashMap::from([("shared".to_string(), 0.5)]);

        let merged = merge_similar_artists([map_a, map_b, map_c]);
        assert_eq!(merged.get("shared"), Some(&0.9));
        assert_eq!(merged.get("only_a"), Some(&0.7));
        assert_eq!(merged.get("only_b"), Some(&0.6));
    }

    #[tokio::test]
    async fn resolve_for_mbids_caches_and_caps_at_three_unique_seeds() {
        let cache = SimilarArtistsCache::new();
        let fetch_count = Arc::new(AtomicUsize::new(0));

        let count_ref = Arc::clone(&fetch_count);
        let first = resolve_for_mbids_with(
            &cache,
            vec![
                "mbid-1".into(),
                "MBID-1".into(), // duplicate (case-insensitive)
                "mbid-2".into(),
                "mbid-3".into(),
                "mbid-4".into(), // 4th unique seed: must be capped out
            ],
            move |mbid| {
                let count = Arc::clone(&count_ref);
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    match mbid.as_str() {
                        "mbid-1" => HashMap::from([("sim".into(), 0.4)]),
                        "mbid-2" => HashMap::from([("sim".into(), 0.85)]),
                        "mbid-3" => HashMap::from([("other".into(), 0.6)]),
                        "mbid-4" => HashMap::from([("capped".into(), 1.0)]),
                        _ => HashMap::new(),
                    }
                }
            },
        )
        .await;

        assert_eq!(fetch_count.load(Ordering::SeqCst), 3);
        assert_eq!(first.get("sim"), Some(&0.85));
        assert_eq!(first.get("other"), Some(&0.6));
        assert!(!first.contains_key("capped"));

        // Second top-up with overlapping MBIDs must hit the session cache.
        let count_ref2 = Arc::clone(&fetch_count);
        let second = resolve_for_mbids_with(
            &cache,
            vec!["mbid-1".into(), "mbid-2".into()],
            move |_mbid| {
                let count = Arc::clone(&count_ref2);
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    HashMap::new()
                }
            },
        )
        .await;

        assert_eq!(
            fetch_count.load(Ordering::SeqCst),
            3,
            "second lookup must hit cache without fetching"
        );
        assert_eq!(second.get("sim"), Some(&0.85));
    }
}
