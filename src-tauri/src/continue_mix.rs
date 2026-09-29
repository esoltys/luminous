//! Auto Continue (#1235): when the Queue is the playing context and nearly
//! finished, append a small batch of library songs that fit what was just
//! playing, so the music doesn't stop.
//!
//! Selection leans on the user's own curation — genres, artist tags, BPM,
//! decade, album artist, ratings — scored against the last few played songs
//! rather than just the final one, so one outlier doesn't steer the mix.
//! The pure scoring/picking half (`pick`) is separate from the SQL loading
//! half (`select_songs`) and the async orchestration (`maybe_extend`) so it
//! can be unit-tested without a database or an app handle.

use crate::models::{parse_multi_value, LIBRARY_SOURCES_SQL};
use crate::AppState;
use anyhow::Result;
use rand::seq::SliceRandom;
use rand::RngExt;
use rusqlite::{params_from_iter, Connection};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};

/// Songs appended per top-up.
pub const BATCH_SIZE: usize = 5;
/// How many of the most recently played songs seed the mix.
pub const SEED_COUNT: usize = 3;
/// Top up once this many (or fewer) songs remain after the current one —
/// before the last song ends, so the next one is already queued and the
/// gapless preload in `audio.rs` still has something to prime.
pub const TOP_UP_THRESHOLD: usize = 1;
/// Songs played within this window are skipped so the mix favours songs the
/// user hasn't heard lately.
pub const RECENT_PLAY_WINDOW_SECS: i64 = 3 * 24 * 60 * 60;
/// `playlist_items.additional_metadata` marker on appended rows — the Queue
/// view draws its "Auto Continue" divider before the first one.
pub const MARKER_JSON: &str = r#"{"autoContinue":true}"#;

/// At most this many songs by one artist per batch, so a strong match on a
/// single artist doesn't turn the continuation into their discography.
const MAX_PER_ARTIST: usize = 2;
/// Upper bound of the random jitter added to each score — enough to vary
/// the pick between equally good matches, not enough to beat a real match.
const JITTER: f64 = 1.0;
/// BPM difference at which the tempo bonus drops to zero.
const BPM_WINDOW: f32 = 12.0;

/// The fields of a song that scoring looks at, normalised for comparison
/// (lowercased, multi-value fields split).
#[derive(Debug, Clone, Default)]
pub struct Candidate {
    pub id: i64,
    pub artists: Vec<String>,
    /// `album_artist`, falling back to the first artist — the same key
    /// `stats_exclusions` uses for artist exclusions.
    pub artist_key: Option<String>,
    pub album: Option<String>,
    pub genres: Vec<String>,
    pub decade: Option<i32>,
    pub bpm: Option<f32>,
    pub rating: f32,
    pub lastplayed: Option<i64>,
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
    fn excludes(&self, c: &Candidate) -> bool {
        self.songs.contains(&c.id)
            || c.album.as_ref().is_some_and(|a| self.albums.contains(a))
            || c.artist_key
                .as_ref()
                .is_some_and(|a| self.artists.contains(a))
            || c.genres.iter().any(|g| self.genres.contains(g))
    }
}

/// What the seed songs have in common, built once and scored against.
#[derive(Debug, Default)]
struct Profile {
    genres: HashSet<String>,
    tags: HashSet<String>,
    artist_keys: HashSet<String>,
    decades: HashSet<i32>,
    bpms: Vec<f32>,
}

impl Profile {
    fn from_seeds(seeds: &[Candidate], artist_tags: &HashMap<String, HashSet<String>>) -> Self {
        let mut p = Profile::default();
        for s in seeds {
            p.genres.extend(s.genres.iter().cloned());
            p.tags.extend(tags_for(s, artist_tags));
            p.artist_keys.extend(s.artist_key.iter().cloned());
            p.decades.extend(s.decade);
            p.bpms.extend(s.bpm.filter(|b| *b > 0.0));
        }
        p
    }
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

/// How well `c` fits the seed profile. Zero means "nothing in common" —
/// only positive scores count as matches; the rest are fallback material.
fn similarity(p: &Profile, c: &Candidate, artist_tags: &HashMap<String, HashSet<String>>) -> f64 {
    let mut score = 0.0;

    let shared_genres = c.genres.iter().filter(|g| p.genres.contains(*g)).count();
    score += 3.0 * shared_genres.min(2) as f64;

    let cand_tags: HashSet<String> = tags_for(c, artist_tags).collect();
    let shared_tags = cand_tags.iter().filter(|t| p.tags.contains(*t)).count();
    score += 2.0 * shared_tags.min(3) as f64;

    if c.artist_key
        .as_ref()
        .is_some_and(|a| p.artist_keys.contains(a))
    {
        score += 1.5;
    }
    if c.decade.is_some_and(|d| p.decades.contains(&d)) {
        score += 1.0;
    }
    if let Some(bpm) = c.bpm.filter(|b| *b > 0.0) {
        let closest = p
            .bpms
            .iter()
            .map(|s| (s - bpm).abs())
            .fold(f32::INFINITY, f32::min);
        if closest < BPM_WINDOW {
            score += (1.0 - closest / BPM_WINDOW) as f64;
        }
    }
    score
}

/// Favourites and higher-rated songs get a nudge, but only on top of a real
/// match — a favourite with nothing in common shouldn't beat a fitting song.
fn rating_boost(rating: f32) -> f64 {
    if rating >= 5.0 {
        1.5
    } else if rating >= 4.0 {
        1.0
    } else {
        0.0
    }
}

/// Picks up to `count` song ids from `candidates` to follow `seeds`.
///
/// Never picks a seed or a song matching `exclusions` (stats exclusions plus
/// the songs already in the Queue). Matching songs not played within
/// `RECENT_PLAY_WINDOW_SECS` of `now` come first; if there aren't enough,
/// random eligible songs fill the batch — unplayed-lately ones before
/// recently played ones.
pub fn pick(
    seeds: &[Candidate],
    candidates: &[Candidate],
    artist_tags: &HashMap<String, HashSet<String>>,
    exclusions: &Exclusions,
    now: i64,
    count: usize,
    rng: &mut impl rand::Rng,
) -> Vec<i64> {
    let profile = Profile::from_seeds(seeds, artist_tags);
    let seed_ids: HashSet<i64> = seeds.iter().map(|s| s.id).collect();
    let recent_cutoff = now - RECENT_PLAY_WINDOW_SECS;

    let eligible: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| !seed_ids.contains(&c.id) && !exclusions.excludes(c))
        .collect();
    let (fresh, recent): (Vec<&Candidate>, Vec<&Candidate>) = eligible
        .into_iter()
        .partition(|c| c.lastplayed.is_none_or(|t| t < recent_cutoff));

    let mut scored: Vec<(f64, &Candidate)> = fresh
        .iter()
        .filter_map(|c| {
            let s = similarity(&profile, c, artist_tags);
            (s > 0.0).then(|| {
                (
                    s + rating_boost(c.rating) + rng.random::<f64>() * JITTER,
                    *c,
                )
            })
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut fallback_fresh = fresh.clone();
    fallback_fresh.shuffle(rng);
    let mut fallback_recent = recent;
    fallback_recent.shuffle(rng);

    let mut picked = Vec::with_capacity(count);
    let mut picked_ids = HashSet::new();
    let mut per_artist: HashMap<&str, usize> = HashMap::new();
    let ordered = scored
        .into_iter()
        .map(|(_, c)| c)
        .chain(fallback_fresh)
        .chain(fallback_recent);
    for c in ordered {
        if picked.len() >= count {
            break;
        }
        if !picked_ids.insert(c.id) {
            continue;
        }
        if let Some(key) = c.artist_key.as_deref() {
            let n = per_artist.entry(key).or_default();
            if *n >= MAX_PER_ARTIST {
                picked_ids.remove(&c.id);
                continue;
            }
            *n += 1;
        }
        picked.push(c.id);
    }
    picked
}

fn lower_opt(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty())
}

fn row_to_candidate(row: &rusqlite::Row) -> rusqlite::Result<Candidate> {
    let artist: Option<String> = row.get(1)?;
    let album_artist: Option<String> = row.get(2)?;
    let genre: Option<String> = row.get(4)?;
    let year: Option<i32> = row.get(5)?;
    let originalyear: Option<i32> = row.get(6)?;
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
        artist_key,
        artists,
        album: lower_opt(row.get(3)?),
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
        bpm: row.get(7)?,
        rating: row.get::<_, Option<f32>>(8)?.unwrap_or(-1.0),
        lastplayed: row.get(9)?,
    })
}

const CANDIDATE_COLS: &str =
    "id, artist, album_artist, album, genre, year, originalyear, bpm, rating, lastplayed";

fn load_library(conn: &Connection) -> Result<Vec<Candidate>> {
    let sql = format!(
        "SELECT {CANDIDATE_COLS} FROM songs
         WHERE source IN ({lib}) AND unavailable = 0 AND not_included = 0",
        lib = *LIBRARY_SOURCES_SQL
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], row_to_candidate)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

fn load_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<Candidate>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("SELECT {CANDIDATE_COLS} FROM songs WHERE id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(ids.iter()), row_to_candidate)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
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
                // A raw genre string may itself hold several values.
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

/// Loads the library, seeds, artist tags and exclusions, then `pick`s.
pub fn select_songs(
    conn: &Connection,
    seed_ids: &[i64],
    exclude_ids: &HashSet<i64>,
    now: i64,
    count: usize,
    rng: &mut impl rand::Rng,
) -> Result<Vec<i64>> {
    let seeds = load_by_ids(conn, seed_ids)?;
    let library = load_library(conn)?;
    let artist_tags = load_artist_tags(conn)?;
    let mut exclusions = load_exclusions(conn)?;
    exclusions.songs.extend(exclude_ids);
    Ok(pick(
        &seeds,
        &library,
        &artist_tags,
        &exclusions,
        now,
        count,
        rng,
    ))
}

/// Guards against two overlapping top-ups (e.g. a `Playing` event and the
/// toggle command landing together) both appending a batch.
static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

struct InFlightGuard;
impl Drop for InFlightGuard {
    fn drop(&mut self) {
        IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

/// Tops up the Queue if Auto Continue applies right now — see
/// `Player::auto_continue_seed` for the conditions. Spawned (never awaited
/// under a lock) from the audio event loop after a track starts, and from
/// `set_auto_continue` when the toggle is switched on; takes the player and
/// playlist locks one at a time, never nested.
pub async fn maybe_extend(app: tauri::AppHandle) {
    if IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }
    let _guard = InFlightGuard;
    if let Err(e) = extend(&app).await {
        log::warn!("Auto Continue top-up failed: {e}");
    }
}

async fn extend(app: &tauri::AppHandle) -> Result<()> {
    let Some(state) = app.try_state::<AppState>() else {
        return Ok(());
    };

    let Some((playlist_id, seed_ids)) = state.player.lock().await.auto_continue_seed() else {
        return Ok(());
    };

    let queue_items = crate::playlist::with_playlists(&state.playlists, |pm| {
        let queue = pm.queue()?;
        if queue.id != playlist_id {
            return Ok(None);
        }
        Ok(Some((queue.id, pm.get_playlist_tracks(queue.id)?)))
    })
    .await?;
    let Some((queue_id, queue_items)) = queue_items else {
        return Ok(());
    };
    let in_queue: HashSet<i64> = queue_items
        .iter()
        .filter_map(|i| i.song.as_ref().map(|s| s.id))
        .collect();

    let db = state.db.clone();
    let now = chrono::Utc::now().timestamp();
    let ids = tokio::task::spawn_blocking(move || {
        let conn = db.pool.get()?;
        select_songs(
            &conn,
            &seed_ids,
            &in_queue,
            now,
            BATCH_SIZE,
            &mut rand::rng(),
        )
    })
    .await??;
    if ids.is_empty() {
        log::info!("Auto Continue: no eligible songs to append");
        return Ok(());
    }

    let added = crate::playlist::with_playlists(&state.playlists, |pm| {
        pm.append_auto_continue_songs(queue_id, &ids)
    })
    .await?;
    log::info!(
        "Auto Continue: appended {} song(s) to the Queue",
        added.len()
    );

    {
        let mut player = state.player.lock().await;
        // The Queue may have stopped being the context while we selected;
        // the rows are in the DB either way, the live order only follows
        // if it's still playing.
        if player.current_playlist_id == Some(queue_id) {
            player.append_songs_to_playlist_items(added);
        }
        let _ = app.emit("playback-state", player.get_state().await);
    }
    let _ = app.emit("playlists-changed", vec![queue_id]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    const NOW: i64 = 1_800_000_000;

    fn song(id: i64, artist: &str, genres: &[&str], decade: i32, bpm: f32) -> Candidate {
        Candidate {
            id,
            artists: vec![artist.to_lowercase()],
            artist_key: Some(artist.to_lowercase()),
            album: Some(format!("album {id}")),
            genres: genres.iter().map(|g| g.to_lowercase()).collect(),
            decade: Some(decade),
            bpm: Some(bpm),
            rating: -1.0,
            lastplayed: None,
        }
    }

    fn rng() -> StdRng {
        StdRng::seed_from_u64(7)
    }

    fn pick_default(seeds: &[Candidate], lib: &[Candidate], count: usize) -> Vec<i64> {
        pick(
            seeds,
            lib,
            &HashMap::new(),
            &Exclusions::default(),
            NOW,
            count,
            &mut rng(),
        )
    }

    #[test]
    fn matching_genre_beats_unrelated_songs() {
        let seeds = [song(1, "A", &["Folk Metal"], 2000, 140.0)];
        let lib = [
            song(10, "X", &["Jazz"], 1960, 90.0),
            song(11, "Y", &["Folk Metal"], 2010, 150.0),
            song(12, "Z", &["Ambient"], 1990, 70.0),
        ];
        assert_eq!(pick_default(&seeds, &lib, 1), vec![11]);
    }

    #[test]
    fn artist_tags_link_songs_without_shared_genres() {
        let seeds = [song(1, "A", &[], 2000, 0.0)];
        let lib = [song(10, "X", &[], 1970, 0.0), song(11, "Y", &[], 1970, 0.0)];
        let tags = HashMap::from([
            ("a".to_string(), HashSet::from(["symphonic".to_string()])),
            ("y".to_string(), HashSet::from(["symphonic".to_string()])),
        ]);
        let picked = pick(
            &seeds,
            &lib,
            &tags,
            &Exclusions::default(),
            NOW,
            1,
            &mut rng(),
        );
        assert_eq!(picked, vec![11]);
    }

    #[test]
    fn favourite_breaks_tie_between_equal_matches() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let mut fav = song(11, "Y", &["Rock"], 1980, 120.0);
        fav.rating = 5.0;
        let lib = [song(10, "X", &["Rock"], 1980, 120.0), fav];
        assert_eq!(pick_default(&seeds, &lib, 1), vec![11]);
    }

    #[test]
    fn nearby_bpm_scores_higher_than_distant_bpm() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let lib = [
            song(10, "X", &["Rock"], 1980, 60.0),
            song(11, "Y", &["Rock"], 1980, 122.0),
        ];
        assert_eq!(pick_default(&seeds, &lib, 1), vec![11]);
    }

    #[test]
    fn excludes_queue_seeds_and_stats_exclusions() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let lib = [
            song(1, "A", &["Rock"], 1980, 120.0),         // the seed itself
            song(10, "X", &["Rock"], 1980, 120.0),        // already in queue
            song(11, "Y", &["Rock"], 1980, 120.0),        // excluded song
            song(12, "Z", &["Rock"], 1980, 120.0),        // excluded artist
            song(13, "W", &["Rock", "Emo"], 1980, 120.0), // excluded genre
            song(14, "V", &["Rock"], 1980, 120.0),
        ];
        let ex = Exclusions {
            songs: HashSet::from([10, 11]),
            artists: HashSet::from(["z".to_string()]),
            genres: HashSet::from(["emo".to_string()]),
            ..Default::default()
        };
        let picked = pick(&seeds, &lib, &HashMap::new(), &ex, NOW, 5, &mut rng());
        assert_eq!(picked, vec![14]);
    }

    #[test]
    fn recently_played_matches_come_after_unplayed_ones() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let mut recent = song(10, "X", &["Rock"], 1980, 120.0);
        recent.lastplayed = Some(NOW - 60);
        let stale = {
            let mut s = song(11, "Y", &["Jazz"], 1950, 60.0);
            s.lastplayed = Some(NOW - RECENT_PLAY_WINDOW_SECS - 1);
            s
        };
        // The recent song is the only match, but an unplayed-lately song
        // (even unrelated) fills the slot first.
        assert_eq!(pick_default(&seeds, &[recent.clone(), stale], 1), vec![11]);
        // With nothing else left, the recent song is still used rather
        // than stopping the music.
        assert_eq!(pick_default(&seeds, &[recent], 1), vec![10]);
    }

    #[test]
    fn falls_back_to_random_library_songs_when_nothing_matches() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let lib: Vec<Candidate> = (10..20)
            .map(|id| song(id, &format!("artist {id}"), &["Jazz"], 1950, 60.0))
            .collect();
        let picked = pick_default(&seeds, &lib, BATCH_SIZE);
        assert_eq!(picked.len(), BATCH_SIZE);
        assert!(picked.iter().all(|id| (10..20).contains(id)));
    }

    #[test]
    fn caps_songs_per_artist_in_one_batch() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let lib: Vec<Candidate> = (10..16)
            .map(|id| song(id, "Same", &["Rock"], 1980, 120.0))
            .chain([song(20, "Other", &["Jazz"], 1950, 60.0)])
            .collect();
        let picked = pick_default(&seeds, &lib, 3);
        assert_eq!(picked.len(), 3);
        assert_eq!(picked.iter().filter(|id| **id < 20).count(), MAX_PER_ARTIST);
        assert!(picked.contains(&20));
    }

    #[test]
    fn empty_library_picks_nothing() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        assert!(pick_default(&seeds, &[], BATCH_SIZE).is_empty());
    }

    #[test]
    fn jitter_varies_picks_between_equal_matches() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let lib: Vec<Candidate> = (10..30)
            .map(|id| song(id, &format!("artist {id}"), &["Rock"], 1980, 120.0))
            .collect();
        let runs: HashSet<Vec<i64>> = (0..8)
            .map(|seed| {
                pick(
                    &seeds,
                    &lib,
                    &HashMap::new(),
                    &Exclusions::default(),
                    NOW,
                    3,
                    &mut StdRng::seed_from_u64(seed),
                )
            })
            .collect();
        assert!(runs.len() > 1);
    }
}
