//! Local library radio queue generator (#1579).
//!
//! Generates a playback queue of songs from the user's local collection similar
//! to a seed song or artist, using ListenBrainz similarity and local metadata heuristics.
//!
//! The seed song is always index 0 and starts playing immediately; similar songs
//! are selected using a hybrid scoring algorithm that combines ListenBrainz collaborative
//! filtering (when online and available) with local metadata (genres, artist tags from
//! `artist_profiles`, era/decade proximity, BPM/tempo matching, and loved/rating boosts).

use crate::models::{parse_multi_value, PlayContext, ShuffleMode, LIBRARY_SOURCES_SQL};
use crate::AppState;
use anyhow::Result;
use rand::seq::SliceRandom;
use rand::RngExt;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Default target size of a generated radio queue.
pub const DEFAULT_RADIO_LIMIT: usize = 40;

/// At most this many songs by one artist per radio queue (excluding the seed song
/// if it is by that artist), ensuring musical diversity.
pub const MAX_PER_ARTIST: usize = 3;

/// Random jitter added to scores to provide variety across radio runs.
const JITTER: f64 = 1.0;

/// BPM difference at which the tempo bonus drops to zero.
const BPM_WINDOW: f32 = 15.0;

/// Seed parameter for generating a radio queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RadioSeed {
    /// Radio centered around a specific song.
    #[serde(rename_all = "camelCase")]
    Song {
        #[serde(alias = "song_id")]
        song_id: i64,
    },
    /// Radio centered around an artist.
    #[serde(rename_all = "camelCase")]
    Artist { artist: String },
}

/// Metadata returned upon starting a radio session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadioResult {
    /// Song IDs placed into the Queue in playback order.
    pub song_ids: Vec<i64>,
    /// Context name for display and completion toasts (e.g. "Bohemian Rhapsody Radio").
    pub context_name: String,
    /// Display title of the seed (song title or artist name).
    pub seed_title: String,
    /// Total number of songs populated in the radio queue.
    pub song_count: usize,
}

/// Library song candidate with fields normalized for scoring.
#[derive(Debug, Clone, Default)]
pub struct Candidate {
    pub id: i64,
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub artist_key: Option<String>,
    pub album: Option<String>,
    pub genres: Vec<String>,
    pub decade: Option<i32>,
    pub bpm: Option<f32>,
    pub rating: f32,
    pub loved: i32,
    pub playcount: i64,
    pub lastplayed: Option<i64>,
    pub musicbrainz_artist_id: Option<String>,
}

/// Profile constructed from the seed song/artist to score candidate songs against.
#[derive(Debug, Default)]
pub struct RadioProfile {
    pub genres: HashSet<String>,
    pub tags: HashSet<String>,
    pub artist_keys: HashSet<String>,
    pub artist_mbids: HashSet<String>,
    pub decades: HashSet<i32>,
    pub bpms: Vec<f32>,
    /// ListenBrainz similar artists lookup: artist name / MBID -> normalized similarity score (0.0 to 1.0)
    pub similar_artists: HashMap<String, f64>,
}

/// Stats exclusions (#130), lowercased, by entity type.
#[derive(Debug, Default)]
pub struct Exclusions {
    pub songs: HashSet<i64>,
    pub albums: HashSet<String>,
    pub artists: HashSet<String>,
    pub genres: HashSet<String>,
}

impl Exclusions {
    pub fn excludes(&self, c: &Candidate) -> bool {
        self.songs.contains(&c.id)
            || c.album.as_ref().is_some_and(|a| self.albums.contains(a))
            || c.artist_key
                .as_ref()
                .is_some_and(|a| self.artists.contains(a))
            || c.genres.iter().any(|g| self.genres.contains(g))
    }
}

fn lower_opt(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty())
}

fn tags_for<'a>(
    c: &'a Candidate,
    artist_tags: &'a HashMap<String, HashSet<String>>,
) -> impl Iterator<Item = String> + 'a {
    c.artists
        .iter()
        .chain(c.artist_key.iter())
        .filter_map(|a| artist_tags.get(a))
        .flatten()
        .cloned()
}

/// Computes how well `c` fits the radio `profile`.
pub fn similarity(
    p: &RadioProfile,
    c: &Candidate,
    artist_tags: &HashMap<String, HashSet<String>>,
) -> f64 {
    let mut score = 0.0;

    // 1. ListenBrainz similarity match
    let mut lb_match = 0.0f64;
    for a in &c.artists {
        if let Some(&s) = p.similar_artists.get(a) {
            lb_match = lb_match.max(s);
        }
    }
    if let Some(ref k) = c.artist_key {
        if let Some(&s) = p.similar_artists.get(k) {
            lb_match = lb_match.max(s);
        }
    }
    if let Some(ref mbid) = c.musicbrainz_artist_id {
        if let Some(&s) = p.similar_artists.get(mbid) {
            lb_match = lb_match.max(s);
        }
    }
    if lb_match > 0.0 {
        // Boost up to 5.0 points based on ListenBrainz similarity score
        score += 5.0 * lb_match.clamp(0.1, 1.0);
    }

    // 2. Shared genres (up to 2 genres count)
    let shared_genres = c.genres.iter().filter(|g| p.genres.contains(*g)).count();
    score += 3.0 * shared_genres.min(2) as f64;

    // 3. Shared artist tags (up to 3 tags count)
    let cand_tags: HashSet<String> = tags_for(c, artist_tags).collect();
    let shared_tags = cand_tags.iter().filter(|t| p.tags.contains(*t)).count();
    score += 2.0 * shared_tags.min(3) as f64;

    // 4. Same artist
    if c.artist_key
        .as_ref()
        .is_some_and(|a| p.artist_keys.contains(a))
    {
        score += 2.0;
    }

    // 5. Same decade
    if c.decade.is_some_and(|d| p.decades.contains(&d)) {
        score += 1.5;
    }

    // 6. BPM proximity
    if let Some(bpm) = c.bpm.filter(|b| *b > 0.0) {
        let closest = p
            .bpms
            .iter()
            .map(|s| (s - bpm).abs())
            .fold(f32::INFINITY, f32::min);
        if closest < BPM_WINDOW {
            score += ((1.0 - closest / BPM_WINDOW) * 1.5) as f64;
        }
    }

    score
}

/// Nudge higher-rated and loved songs on top of similarity matches.
fn rating_boost(c: &Candidate) -> f64 {
    if c.loved == 1 || c.rating >= 5.0 {
        1.5
    } else if c.rating >= 4.0 {
        1.0
    } else if c.rating >= 3.0 {
        0.5
    } else {
        0.0
    }
}

/// Picks up to `limit` songs to form the radio queue.
///
/// If `seed_id` is supplied, it is always placed at index 0. Remaining songs
/// are chosen by similarity score, respecting `exclusions` and `MAX_PER_ARTIST`.
/// If there are not enough similar songs to reach `limit`, other eligible library
/// songs pad the queue.
pub fn pick_radio_songs(
    seed_id: Option<i64>,
    profile: &RadioProfile,
    candidates: &[Candidate],
    artist_tags: &HashMap<String, HashSet<String>>,
    exclusions: &Exclusions,
    limit: usize,
    rng: &mut impl rand::Rng,
) -> Vec<i64> {
    let mut picked = Vec::with_capacity(limit);
    let mut picked_ids = HashSet::new();

    // 1. Seed song is always index 0
    if let Some(sid) = seed_id {
        picked.push(sid);
        picked_ids.insert(sid);
    }

    let eligible: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| !picked_ids.contains(&c.id) && !exclusions.excludes(c))
        .collect();

    // 2. Score eligible candidates
    let mut scored: Vec<(f64, &Candidate)> = eligible
        .iter()
        .filter_map(|c| {
            let s = similarity(profile, c, artist_tags);
            (s > 0.0).then(|| (s + rating_boost(c) + rng.random::<f64>() * JITTER, *c))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut fallback = eligible;
    fallback.shuffle(rng);

    let mut per_artist: HashMap<&str, usize> = HashMap::new();
    if let Some(sid) = seed_id {
        if let Some(seed_cand) = candidates.iter().find(|c| c.id == sid) {
            if let Some(key) = seed_cand.artist_key.as_deref() {
                per_artist.insert(key, 1);
            }
        }
    }

    let ordered = scored.into_iter().map(|(_, c)| c).chain(fallback);
    for c in ordered {
        if picked.len() >= limit {
            break;
        }
        if !picked_ids.insert(c.id) {
            continue;
        }
        if let Some(key) = c.artist_key.as_deref() {
            let count = per_artist.entry(key).or_default();
            if *count >= MAX_PER_ARTIST {
                picked_ids.remove(&c.id);
                continue;
            }
            *count += 1;
        }
        picked.push(c.id);
    }

    picked
}

fn row_to_candidate(row: &rusqlite::Row) -> rusqlite::Result<Candidate> {
    let artist: Option<String> = row.get(2)?;
    let album_artist: Option<String> = row.get(3)?;
    let genre: Option<String> = row.get(5)?;
    let year: Option<i32> = row.get(6)?;
    let originalyear: Option<i32> = row.get(7)?;
    let artists: Vec<String> = artist
        .as_deref()
        .map(parse_multi_value)
        .unwrap_or_default()
        .into_iter()
        .map(|a| a.to_lowercase())
        .collect();
    let artist_key = lower_opt(album_artist).or_else(|| artists.first().cloned());
    Ok(Candidate {
        id: row.get(0)?,
        title: row.get(1)?,
        artist_key,
        artists,
        album: lower_opt(row.get(4)?),
        genres: genre
            .as_deref()
            .map(parse_multi_value)
            .unwrap_or_default()
            .into_iter()
            .map(|g| g.to_lowercase())
            .collect(),
        decade: originalyear
            .filter(|y| *y > 0)
            .or(year.filter(|y| *y > 0))
            .map(|y| y / 10 * 10),
        bpm: row.get(8)?,
        rating: row.get::<_, Option<f32>>(9)?.unwrap_or(-1.0),
        loved: row.get::<_, Option<i32>>(10)?.unwrap_or(0),
        playcount: row.get::<_, Option<i64>>(11)?.unwrap_or(0),
        lastplayed: row.get(12)?,
        musicbrainz_artist_id: lower_opt(row.get(13)?),
    })
}

const CANDIDATE_SELECT: &str =
    "id, title, artist, album_artist, album, genre, year, originalyear, bpm, rating, loved, playcount, lastplayed, musicbrainz_artist_id";

fn load_library(conn: &Connection) -> Result<Vec<Candidate>> {
    let sql = format!(
        "SELECT {CANDIDATE_SELECT} FROM songs
         WHERE source IN ({lib}) AND unavailable = 0 AND not_included = 0 AND loved != -1",
        lib = *LIBRARY_SOURCES_SQL
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], row_to_candidate)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

fn load_by_id(conn: &Connection, id: i64) -> Result<Option<Candidate>> {
    let sql = format!("SELECT {CANDIDATE_SELECT} FROM songs WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], row_to_candidate)?;
    if let Some(r) = rows.next() {
        Ok(r.ok())
    } else {
        Ok(None)
    }
}

fn load_artist_seed_song(conn: &Connection, artist: &str) -> Result<Option<Candidate>> {
    let sql = format!(
        "SELECT {CANDIDATE_SELECT} FROM songs
         WHERE (artist = ?1 COLLATE NOCASE OR album_artist = ?1 COLLATE NOCASE)
           AND source IN ({lib}) AND unavailable = 0 AND not_included = 0 AND loved != -1
         ORDER BY (loved = 1) DESC, rating DESC, playcount DESC, RANDOM() LIMIT 1",
        lib = *LIBRARY_SOURCES_SQL
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![artist], row_to_candidate)?;
    if let Some(r) = rows.next() {
        Ok(r.ok())
    } else {
        Ok(None)
    }
}

fn resolve_artist_mbid(
    conn: &Connection,
    artist: &str,
    candidate_mbid: Option<&str>,
) -> Option<String> {
    if let Some(mbid) = candidate_mbid {
        if !mbid.trim().is_empty() {
            return Some(mbid.trim().to_lowercase());
        }
    }
    let profile_mbid: Option<String> = conn
        .query_row(
            "SELECT musicbrainz_artist_id FROM artist_profiles WHERE artist_key = ?1 COLLATE NOCASE",
            params![artist],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    profile_mbid
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

fn load_artist_tags(conn: &Connection) -> Result<HashMap<String, HashSet<String>>> {
    let mut stmt =
        conn.prepare("SELECT artist_key, tags FROM artist_profiles WHERE tags != '[]'")?;
    let map = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(artist, tags)| {
            let tags: Vec<String> = serde_json::from_str(&tags).ok()?;
            let set: HashSet<String> = tags
                .into_iter()
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            (!set.is_empty()).then(|| (artist.to_lowercase(), set))
        })
        .collect();
    Ok(map)
}

fn load_exclusions(conn: &Connection) -> Result<Exclusions> {
    let mut ex = Exclusions::default();
    for (kind, key) in crate::stats::get_stats_exclusions(conn)? {
        let key_lc = key.trim().to_lowercase();
        match kind.as_str() {
            "song" => ex.songs.extend(key.parse::<i64>().ok()),
            "album" => {
                ex.albums.insert(key_lc);
            }
            "artist" => {
                ex.artists.insert(key_lc);
            }
            "genre" => {
                ex.genres.extend(
                    parse_multi_value(&key)
                        .into_iter()
                        .map(|g| g.to_lowercase()),
                );
            }
            _ => {}
        }
    }
    Ok(ex)
}

#[derive(Debug, Deserialize)]
struct SimilarArtistResponseItem {
    pub artist_mbid: Option<String>,
    pub name: Option<String>,
    pub score: Option<f64>,
}

/// Fetches similar artists from ListenBrainz Labs API with a 2.5s timeout.
/// Fails silently to an empty map so callers proceed with local heuristics.
async fn fetch_listenbrainz_similar_artists(artist_mbid: &str) -> HashMap<String, f64> {
    let url = format!(
        "https://labs.api.listenbrainz.org/similar-artists/json?artist_mbids={}&algorithm=session_based_days_7500_session_300_contribution_5_threshold_10_limit_100_filter_True_skip_30",
        artist_mbid
    );

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(2500))
        .user_agent(concat!("LuminousMusicPlayer/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

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
            item.score.unwrap_or(0.0) / max_score
        } else {
            0.5
        };
        if let Some(name) = item.name {
            let n = name.trim().to_lowercase();
            if !n.is_empty() {
                map.insert(n, norm);
            }
        }
        if let Some(mbid) = item.artist_mbid {
            let m = mbid.trim().to_lowercase();
            if !m.is_empty() {
                map.insert(m, norm);
            }
        }
    }
    map
}

/// Resolves candidate songs from the local library matching `seed`.
pub async fn generate_radio_songs(
    state: &AppState,
    seed: RadioSeed,
    limit: usize,
) -> Result<RadioResult, String> {
    let db = state.db.clone();
    let (seed_song, seed_title, artist_for_lookup, artist_mbid_opt, is_online) =
        crate::db::run_blocking(&db, {
            let seed = seed.clone();
            move |conn| {
                let online = crate::commands::context::is_online_enabled(conn);
                match seed {
                    RadioSeed::Song { song_id } => {
                        let cand = load_by_id(conn, song_id)?
                            .ok_or_else(|| anyhow::anyhow!("Song not found: {}", song_id))?;
                        let title = cand
                            .title
                            .clone()
                            .unwrap_or_else(|| "Unknown Song".to_string());
                        let artist_name = cand.artist_key.clone().unwrap_or_default();
                        let mbid = resolve_artist_mbid(
                            conn,
                            &artist_name,
                            cand.musicbrainz_artist_id.as_deref(),
                        );
                        Ok((Some(cand), title, artist_name, mbid, online))
                    }
                    RadioSeed::Artist { artist } => {
                        let cand = load_artist_seed_song(conn, &artist)?;
                        let title = artist.clone();
                        let mbid = resolve_artist_mbid(
                            conn,
                            &artist,
                            cand.as_ref()
                                .and_then(|c| c.musicbrainz_artist_id.as_deref()),
                        );
                        Ok((cand, title, artist.to_lowercase(), mbid, online))
                    }
                }
            }
        })
        .await
        .map_err(|e| e.to_string())?;

    let similar_artists = if is_online {
        if let Some(ref mbid) = artist_mbid_opt {
            fetch_listenbrainz_similar_artists(mbid).await
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    let seed_id = seed_song.as_ref().map(|s| s.id);
    let result_ids = crate::db::run_blocking(&db, {
        let similar_artists = similar_artists.clone();
        let artist_for_lookup = artist_for_lookup.clone();
        move |conn| {
            let library = load_library(conn)?;
            let artist_tags = load_artist_tags(conn)?;
            let exclusions = load_exclusions(conn)?;

            let mut profile = RadioProfile::default();
            if let Some(ref s) = seed_song {
                profile.genres.extend(s.genres.iter().cloned());
                profile.tags.extend(tags_for(s, &artist_tags));
                profile.artist_keys.extend(s.artist_key.iter().cloned());
                profile
                    .artist_mbids
                    .extend(s.musicbrainz_artist_id.iter().cloned());
                profile.decades.extend(s.decade);
                profile.bpms.extend(s.bpm.filter(|b| *b > 0.0));
            } else {
                profile.artist_keys.insert(artist_for_lookup.clone());
                if let Some(tags) = artist_tags.get(&artist_for_lookup) {
                    profile.tags.extend(tags.iter().cloned());
                }
            }
            profile.similar_artists = similar_artists;

            let mut rng = rand::rng();
            Ok(pick_radio_songs(
                seed_id,
                &profile,
                &library,
                &artist_tags,
                &exclusions,
                limit,
                &mut rng,
            ))
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    let context_name = format!("{seed_title} Radio");
    let song_count = result_ids.len();

    Ok(RadioResult {
        song_ids: result_ids,
        context_name,
        seed_title,
        song_count,
    })
}

/// Starts playback of a generated radio queue.
pub async fn start_radio_impl(
    state: &AppState,
    seed: RadioSeed,
    limit: usize,
) -> Result<RadioResult, String> {
    let result = generate_radio_songs(state, seed, limit).await?;

    if result.song_ids.is_empty() {
        return Err("No eligible songs in library to start radio".to_string());
    }

    let (queue_id, items) = crate::playlist::with_playlists(&state.playlists, {
        let ids = result.song_ids.clone();
        move |pm| pm.replace_queue(&ids)
    })
    .await
    .map_err(|e| e.to_string())?;

    let mut player = state.player.lock().await;
    player.set_shuffle_mode(ShuffleMode::Off);
    player
        .play_playlist(items, 0, queue_id, Some(PlayContext::Song))
        .await
        .map_err(|e| e.to_string())?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn make_candidate(
        id: i64,
        artist: &str,
        genres: &[&str],
        decade: Option<i32>,
        bpm: Option<f32>,
        rating: f32,
    ) -> Candidate {
        Candidate {
            id,
            title: Some(format!("Song {id}")),
            artist_key: Some(artist.to_lowercase()),
            artists: vec![artist.to_lowercase()],
            album: Some("Album".to_string()),
            genres: genres.iter().map(|g| g.to_lowercase()).collect(),
            decade,
            bpm,
            rating,
            loved: 0,
            playcount: 0,
            lastplayed: None,
            musicbrainz_artist_id: None,
        }
    }

    #[test]
    fn test_pick_radio_songs_puts_seed_at_index_0() {
        let mut rng = StdRng::seed_from_u64(42);
        let c1 = make_candidate(1, "Queen", &["Rock"], Some(1970), Some(120.0), 5.0);
        let c2 = make_candidate(2, "Queen", &["Rock"], Some(1970), Some(122.0), 4.0);
        let c3 = make_candidate(3, "Led Zeppelin", &["Rock"], Some(1970), Some(118.0), 5.0);

        let candidates = vec![c1.clone(), c2, c3];
        let mut profile = RadioProfile::default();
        profile.genres.insert("rock".to_string());
        profile.artist_keys.insert("queen".to_string());
        profile.decades.insert(1970);

        let artist_tags = HashMap::new();
        let exclusions = Exclusions::default();

        let picked = pick_radio_songs(
            Some(1),
            &profile,
            &candidates,
            &artist_tags,
            &exclusions,
            10,
            &mut rng,
        );

        assert_eq!(picked.first(), Some(&1));
        assert!(picked.contains(&2));
        assert!(picked.contains(&3));
    }

    #[test]
    fn test_pick_radio_songs_respects_max_per_artist() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut candidates = Vec::new();
        for i in 1..=10 {
            candidates.push(make_candidate(
                i,
                "The Beatles",
                &["Rock"],
                Some(1960),
                None,
                4.0,
            ));
        }
        candidates.push(make_candidate(
            11,
            "The Rolling Stones",
            &["Rock"],
            Some(1960),
            None,
            4.0,
        ));

        let mut profile = RadioProfile::default();
        profile.genres.insert("rock".to_string());
        profile.artist_keys.insert("the beatles".to_string());

        let artist_tags = HashMap::new();
        let exclusions = Exclusions::default();

        let picked = pick_radio_songs(
            Some(1),
            &profile,
            &candidates,
            &artist_tags,
            &exclusions,
            10,
            &mut rng,
        );

        // Seed Beatles song is index 0. Beatles should not exceed MAX_PER_ARTIST (3).
        let beatles_count = picked.iter().filter(|&&id| id <= 10).count();
        assert!(beatles_count <= MAX_PER_ARTIST);
        assert!(picked.contains(&11));
    }

    #[test]
    fn test_similarity_boosts_listenbrainz_similar_artists() {
        let mut profile = RadioProfile::default();
        profile.genres.insert("rock".to_string());
        profile
            .similar_artists
            .insert("electric light orchestra".to_string(), 0.9);

        let cand_similar = make_candidate(
            2,
            "Electric Light Orchestra",
            &["Rock"],
            Some(1970),
            None,
            3.0,
        );
        let cand_other = make_candidate(3, "Unknown Band", &["Rock"], Some(1970), None, 3.0);

        let artist_tags = HashMap::new();
        let score_similar = similarity(&profile, &cand_similar, &artist_tags);
        let score_other = similarity(&profile, &cand_other, &artist_tags);

        assert!(
            score_similar > score_other + 3.0,
            "Similar artist should score significantly higher due to ListenBrainz boost"
        );
    }

    #[test]
    fn test_exclusions_filters_excluded_songs_and_artists() {
        let mut rng = StdRng::seed_from_u64(42);
        let c1 = make_candidate(1, "Artist A", &["Pop"], None, None, 4.0);
        let c2 = make_candidate(2, "Blocked Artist", &["Pop"], None, None, 4.0);
        let c3 = make_candidate(3, "Artist C", &["Pop"], None, None, 4.0);

        let candidates = vec![c1, c2, c3];
        let profile = RadioProfile::default();
        let artist_tags = HashMap::new();

        let mut exclusions = Exclusions::default();
        exclusions.artists.insert("blocked artist".to_string());
        exclusions.songs.insert(3);

        let picked = pick_radio_songs(
            None,
            &profile,
            &candidates,
            &artist_tags,
            &exclusions,
            10,
            &mut rng,
        );

        assert_eq!(picked, vec![1]);
    }

    #[test]
    fn test_pick_radio_songs_handles_empty_candidates_gracefully() {
        let mut rng = StdRng::seed_from_u64(42);
        let profile = RadioProfile::default();
        let artist_tags = HashMap::new();
        let exclusions = Exclusions::default();

        let picked = pick_radio_songs(None, &profile, &[], &artist_tags, &exclusions, 40, &mut rng);
        assert!(picked.is_empty());

        let picked_with_seed = pick_radio_songs(
            Some(99),
            &profile,
            &[],
            &artist_tags,
            &exclusions,
            40,
            &mut rng,
        );
        assert_eq!(picked_with_seed, vec![99]);
    }

    #[test]
    fn test_similarity_considers_genres_tags_decades_and_bpm() {
        let mut profile = RadioProfile::default();
        profile.genres.insert("prog-rock".to_string());
        profile.tags.insert("virtuoso".to_string());
        profile.decades.insert(1980);
        profile.bpms.push(120.0);

        let mut artist_tags = HashMap::new();
        let mut rush_tags = HashSet::new();
        rush_tags.insert("virtuoso".to_string());
        artist_tags.insert("rush".to_string(), rush_tags);

        let matching = make_candidate(1, "Rush", &["Prog-Rock"], Some(1980), Some(121.0), 5.0);
        let non_matching = make_candidate(2, "Other", &["Classical"], Some(1920), Some(60.0), 1.0);

        let score_m = similarity(&profile, &matching, &artist_tags);
        let score_nm = similarity(&profile, &non_matching, &artist_tags);

        assert!(score_m > 7.0);
        assert_eq!(score_nm, 0.0);
    }

    #[test]
    fn test_deserialize_radio_seed_camel_case_and_snake_case() {
        let json_camel = r#"{"type":"song","songId":42}"#;
        let seed1: RadioSeed = serde_json::from_str(json_camel).unwrap();
        match seed1 {
            RadioSeed::Song { song_id } => assert_eq!(song_id, 42),
            _ => panic!("Expected Song variant"),
        }

        let json_snake = r#"{"type":"song","song_id":42}"#;
        let seed2: RadioSeed = serde_json::from_str(json_snake).unwrap();
        match seed2 {
            RadioSeed::Song { song_id } => assert_eq!(song_id, 42),
            _ => panic!("Expected Song variant"),
        }

        let json_artist = r#"{"type":"artist","artist":"Queen"}"#;
        let seed3: RadioSeed = serde_json::from_str(json_artist).unwrap();
        match seed3 {
            RadioSeed::Artist { artist } => assert_eq!(artist, "Queen"),
            _ => panic!("Expected Artist variant"),
        }
    }
}
