//! Song-mix engine (#2284): picks a sequence of library songs that fit a
//! taste profile. Radio (`radio.rs`) and Auto Continue (`continue_mix.rs`)
//! are thin callers — they choose the seeds and decide what to do with the
//! picked ids; everything about *which* songs fit lives here.
//!
//! The core (`Profile`, `pick`) is pure so it can be unit-tested without a
//! database; `Pool::load` and `load_by_ids` are the SQLite edge. This module
//! owns the candidate columns and how they're normalised (lowercased,
//! multi-value fields split, genres canonicalised through `GenreAliases`), so
//! callers never write a song query of their own for mixing.

use crate::genre_aliases::GenreAliases;
use crate::models::{parse_multi_value, playable_library_song_where};
use anyhow::Result;
use rand::RngExt;
use rusqlite::{params_from_iter, Connection};
use std::collections::{HashMap, HashSet};

/// Score points per factor of e in a match's sampling weight: each point
/// makes a song about twice as likely to be drawn next. Scores are additive
/// points, so weighting by the raw score would let a large pool of weak
/// matches outdraw a few strong ones; the exponential keeps strong matches
/// dominant while weaker ones still turn up. Goes when #2286 replaces the
/// additive score with weights of its own.
const WEIGHT_SCALE: f64 = 1.5;
/// Points per shared genre, and how many shared genres count.
const GENRE_POINTS: f64 = 3.0;
const MAX_SHARED_GENRES: usize = 2;
/// Points per shared artist tag (`artist_profiles.tags`), and how many count.
const TAG_POINTS: f64 = 2.0;
const MAX_SHARED_TAGS: usize = 3;

/// The scoring constants a caller tunes. Transitional: they exist so Radio
/// and Auto Continue keep their pre-#2284 behaviour exactly; #2286 and #2288
/// replace the additive scoring and remove most of these.
#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    /// At most this many picked songs per artist (`artist_key`), the lead
    /// song included.
    pub max_per_artist: usize,
    /// Points for a ListenBrainz-similar artist, scaled by its 0–1 match.
    pub similar_artist: f64,
    /// Points when the candidate's artist is a seed artist.
    pub same_artist: f64,
    /// Points when the candidate's decade is a seed decade.
    pub same_decade: f64,
    /// BPM difference at which the tempo bonus drops to zero.
    pub bpm_window: f32,
    /// Tempo bonus at an exact BPM match, falling linearly to the window.
    pub bpm_points: f32,
    /// Boost added on top of a real match (never on its own).
    pub rating: RatingBoost,
}

/// Boost by star rating (0–5), and for loved songs; a loved song gets the
/// larger of `loved` and its rating tier.
#[derive(Debug, Clone, Copy)]
pub struct RatingBoost {
    pub loved: f64,
    pub five: f64,
    pub four: f64,
    pub three: f64,
}

/// A library song with the fields mixing looks at, normalised for comparison.
#[derive(Debug, Clone, Default)]
pub struct Candidate {
    pub id: i64,
    pub title: Option<String>,
    pub artists: Vec<String>,
    /// `album_artist`, falling back to the first artist — the same key
    /// `stats_exclusions` uses for artist exclusions.
    pub artist_key: Option<String>,
    pub album: Option<String>,
    pub genres: Vec<String>,
    pub decade: Option<i32>,
    pub bpm: Option<f32>,
    /// Star rating, or -1.0 when unrated.
    pub rating: f32,
    pub loved: i32,
    pub lastplayed: Option<i64>,
    pub musicbrainz_artist_id: Option<String>,
}

/// Songs `pick` must never return, lowercased by entity type: the stats
/// exclusions (#130) plus any song ids the caller adds.
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

/// Everything `pick` draws from: the playable library, artist tags keyed by
/// lowercased artist, and what is excluded.
#[derive(Debug, Default)]
pub struct Pool {
    pub candidates: Vec<Candidate>,
    pub artist_tags: HashMap<String, HashSet<String>>,
    pub exclusions: Exclusions,
}

impl Pool {
    /// Loads the playable library, artist tags and stats exclusions.
    pub fn load(conn: &Connection) -> Result<Self> {
        let aliases = GenreAliases::load(conn)?;
        let sql = format!(
            "SELECT {CANDIDATE_COLS} FROM songs WHERE {lib_where}",
            lib_where = playable_library_song_where("")
        );
        let mut stmt = conn.prepare(&sql)?;
        let candidates = stmt
            .query_map([], |r| row_to_candidate(r, &aliases))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(Pool {
            candidates,
            artist_tags: load_artist_tags(conn)?,
            exclusions: load_exclusions(conn, &aliases)?,
        })
    }

    /// Adds songs `pick` must not return (e.g. those already in the Queue).
    pub fn exclude_songs(&mut self, ids: impl IntoIterator<Item = i64>) {
        self.exclusions.songs.extend(ids);
    }

    fn tags_for<'a>(&'a self, c: &'a Candidate) -> impl Iterator<Item = &'a String> + 'a {
        c.artists
            .iter()
            .chain(c.artist_key.iter())
            .filter_map(|a| self.artist_tags.get(a))
            .flatten()
    }
}

/// What the seeds have in common, scored against.
#[derive(Debug, Default)]
pub struct Profile {
    /// Seed songs, most recent first: id and artist key.
    seeds: Vec<(i64, Option<String>)>,
    genres: HashSet<String>,
    tags: HashSet<String>,
    artist_keys: HashSet<String>,
    decades: HashSet<i32>,
    bpms: Vec<f32>,
    /// ListenBrainz similar artists: lowercased name or MBID -> 0–1 match.
    similar_artists: HashMap<String, f64>,
}

impl Profile {
    /// Builds a profile from seed songs, most recent first. Seeds are never
    /// picked, except the first one when `MixOptions::lead_with_seed` is set.
    pub fn from_seeds(seeds: &[Candidate], pool: &Pool) -> Self {
        let mut p = Profile::default();
        for s in seeds {
            p.seeds.push((s.id, s.artist_key.clone()));
            p.genres.extend(s.genres.iter().cloned());
            p.tags.extend(pool.tags_for(s).cloned());
            p.artist_keys.extend(s.artist_key.iter().cloned());
            p.decades.extend(s.decade);
            p.bpms.extend(s.bpm.filter(|b| *b > 0.0));
        }
        p
    }

    /// Adds a seed artist with no seed song (lowercased key), with its tags.
    pub fn add_artist(&mut self, artist_key: &str, pool: &Pool) {
        self.artist_keys.insert(artist_key.to_string());
        if let Some(tags) = pool.artist_tags.get(artist_key) {
            self.tags.extend(tags.iter().cloned());
        }
    }

    /// Sets the ListenBrainz similar-artist map (lowercased name or MBID ->
    /// 0–1 match).
    pub fn set_similar_artists(&mut self, similar: HashMap<String, f64>) {
        self.similar_artists = similar;
    }

    /// How well `c` fits. Zero means "nothing in common": only positive
    /// scores count as matches; the rest are fallback material.
    fn similarity(&self, c: &Candidate, pool: &Pool, t: &Tuning) -> f64 {
        let mut score = 0.0;

        let lb_match = c
            .artists
            .iter()
            .chain(c.artist_key.iter())
            .chain(c.musicbrainz_artist_id.iter())
            .filter_map(|a| self.similar_artists.get(a))
            .fold(0.0f64, |m, &s| m.max(s));
        if lb_match > 0.0 {
            score += t.similar_artist * lb_match.clamp(0.1, 1.0);
        }

        let shared_genres = c.genres.iter().filter(|g| self.genres.contains(*g)).count();
        score += GENRE_POINTS * shared_genres.min(MAX_SHARED_GENRES) as f64;

        let cand_tags: HashSet<&String> = pool.tags_for(c).collect();
        let shared_tags = cand_tags.iter().filter(|t| self.tags.contains(**t)).count();
        score += TAG_POINTS * shared_tags.min(MAX_SHARED_TAGS) as f64;

        if c.artist_key
            .as_ref()
            .is_some_and(|a| self.artist_keys.contains(a))
        {
            score += t.same_artist;
        }
        if c.decade.is_some_and(|d| self.decades.contains(&d)) {
            score += t.same_decade;
        }
        if let Some(bpm) = c.bpm.filter(|b| *b > 0.0) {
            let closest = self
                .bpms
                .iter()
                .map(|s| (s - bpm).abs())
                .fold(f32::INFINITY, f32::min);
            if closest < t.bpm_window {
                score += ((1.0 - closest / t.bpm_window) * t.bpm_points) as f64;
            }
        }
        score
    }
}

fn rating_boost(c: &Candidate, r: &RatingBoost) -> f64 {
    let by_rating = if c.rating >= 5.0 {
        r.five
    } else if c.rating >= 4.0 {
        r.four
    } else if c.rating >= 3.0 {
        r.three
    } else {
        0.0
    };
    if c.loved == 1 {
        by_rating.max(r.loved)
    } else {
        by_rating
    }
}

/// What a caller asks `pick` for.
#[derive(Debug, Clone, Copy)]
pub struct MixOptions<'a> {
    /// Maximum number of ids returned (the lead included).
    pub limit: usize,
    /// Put the profile's first seed song at index 0.
    pub lead_with_seed: bool,
    /// Songs last played at or after this unix time go last: they are never
    /// scored as matches and fill only after every other eligible song.
    pub defer_played_since: Option<i64>,
    pub tuning: &'a Tuning,
}

/// Picks up to `opts.limit` song ids from `pool` to fit `profile`.
///
/// Never returns a seed song (other than the lead) or an excluded song, and
/// never more than `tuning.max_per_artist` songs by one artist, counting the
/// lead. Matching songs come first, drawn at random in proportion to how
/// well they fit, so every call differs and strong matches are likely but
/// not certain to lead. Unmatched eligible songs follow in random order,
/// then deferred recent plays. Returns fewer than `limit` only when no
/// eligible song is left under its artist's cap.
pub fn pick(
    profile: &Profile,
    pool: &Pool,
    opts: &MixOptions,
    rng: &mut impl rand::Rng,
) -> Vec<i64> {
    let t = opts.tuning;
    let seed_ids: HashSet<i64> = profile.seeds.iter().map(|(id, _)| *id).collect();
    let mut picked = Vec::with_capacity(opts.limit);
    let mut per_artist: HashMap<&str, usize> = HashMap::new();

    if opts.lead_with_seed && opts.limit > 0 {
        if let Some((id, artist_key)) = profile.seeds.first() {
            picked.push(*id);
            if let Some(key) = artist_key.as_deref() {
                per_artist.insert(key, 1);
            }
        }
    }

    let entries = pool
        .candidates
        .iter()
        .filter(|c| !seed_ids.contains(&c.id) && !pool.exclusions.excludes(c))
        .map(|c| {
            let deferred = opts
                .defer_played_since
                .is_some_and(|cutoff| c.lastplayed.is_some_and(|p| p >= cutoff));
            if deferred {
                return (Tier::Deferred, 1.0, c);
            }
            let s = profile.similarity(c, pool, t);
            if s > 0.0 {
                let w = match_weight(s + rating_boost(c, &t.rating));
                (Tier::Match, w, c)
            } else {
                (Tier::Unmatched, 1.0, c)
            }
        })
        .collect();

    for c in sample_order(entries, rng) {
        if picked.len() >= opts.limit {
            break;
        }
        if let Some(key) = c.artist_key.as_deref() {
            let n = per_artist.entry(key).or_default();
            if *n >= t.max_per_artist {
                continue;
            }
            *n += 1;
        }
        picked.push(c.id);
    }
    picked
}

/// Which part of the pick order a candidate falls in; every candidate in an
/// earlier tier comes before any in a later one, whatever their weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    /// Shares something with the profile; weighted by `match_weight`.
    Match,
    /// Nothing in common: filler once the matches run out.
    Unmatched,
    /// Played since `MixOptions::defer_played_since`: used last.
    Deferred,
}

/// Sampling weight for a match's score (similarity plus rating boost).
fn match_weight(score: f64) -> f64 {
    (score / WEIGHT_SCALE).exp()
}

/// Orders `entries` by tier, and within a tier by weighted random sampling
/// without replacement (Efraimidis–Spirakis): each entry draws the key
/// `ln(u) / weight` with `u` uniform in (0, 1], and the highest key goes
/// first. Any prefix of a tier is then an exact weighted sample of it: the
/// next entry is chosen with probability proportional to its weight among
/// those left. Equal weights give a uniform shuffle. Weights must be
/// positive and finite.
fn sample_order<T>(entries: Vec<(Tier, f64, T)>, rng: &mut impl rand::Rng) -> Vec<T> {
    let mut keyed: Vec<(Tier, f64, T)> = entries
        .into_iter()
        .map(|(tier, weight, item)| {
            // `random` is in [0, 1), so this is in (0, 1] and `ln` is finite.
            let u = 1.0 - rng.random::<f64>();
            (tier, u.ln() / weight, item)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)));
    keyed.into_iter().map(|(_, _, item)| item).collect()
}

const CANDIDATE_COLS: &str = "id, title, artist, album_artist, album, genre, year, originalyear, \
     bpm, rating, loved, lastplayed, musicbrainz_artist_id";

fn lower_opt(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty())
}

fn row_to_candidate(row: &rusqlite::Row, aliases: &GenreAliases) -> rusqlite::Result<Candidate> {
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
        genres: aliases
            .canonical_genres(genre.as_deref().unwrap_or(""))
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
        lastplayed: row.get(11)?,
        musicbrainz_artist_id: lower_opt(row.get(12)?),
    })
}

/// Loads songs by id, in the order of `ids`; ids with no song are skipped.
/// Unlike `Pool::load` this doesn't filter to playable library songs, so a
/// seed can be any song the user started from.
pub fn load_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<Candidate>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let aliases = GenreAliases::load(conn)?;
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("SELECT {CANDIDATE_COLS} FROM songs WHERE id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let mut by_id: HashMap<i64, Candidate> = stmt
        .query_map(params_from_iter(ids.iter()), |r| {
            row_to_candidate(r, &aliases)
        })?
        .filter_map(|r| r.ok())
        .map(|c| (c.id, c))
        .collect();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
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

fn load_exclusions(conn: &Connection, aliases: &GenreAliases) -> Result<Exclusions> {
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
    ex.genres = aliases.expand(&ex.genres);
    Ok(ex)
}

/// A normalised test song: lowercased artist as its key, its own album.
#[cfg(test)]
pub(crate) fn test_song(
    id: i64,
    artist: &str,
    genres: &[&str],
    decade: Option<i32>,
    bpm: Option<f32>,
) -> Candidate {
    Candidate {
        id,
        title: Some(format!("Song {id}")),
        artists: vec![artist.to_lowercase()],
        artist_key: Some(artist.to_lowercase()),
        album: Some(format!("album {id}")),
        genres: genres.iter().map(|g| g.to_lowercase()).collect(),
        decade,
        bpm,
        rating: -1.0,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    const NOW: i64 = 1_800_000_000;

    const TUNING: Tuning = Tuning {
        max_per_artist: 2,
        similar_artist: 5.0,
        same_artist: 2.0,
        same_decade: 1.5,
        bpm_window: 15.0,
        bpm_points: 1.5,
        rating: RatingBoost {
            loved: 1.5,
            five: 1.5,
            four: 1.0,
            three: 0.5,
        },
    };

    fn song(id: i64, artist: &str, genres: &[&str], decade: i32, bpm: f32) -> Candidate {
        test_song(id, artist, genres, Some(decade), Some(bpm))
    }

    fn pool(candidates: Vec<Candidate>) -> Pool {
        Pool {
            candidates,
            ..Default::default()
        }
    }

    fn opts(limit: usize) -> MixOptions<'static> {
        MixOptions {
            limit,
            lead_with_seed: false,
            defer_played_since: None,
            tuning: &TUNING,
        }
    }

    fn pick_with(seeds: &[Candidate], pool: &Pool, opts: &MixOptions, seed: u64) -> Vec<i64> {
        let profile = Profile::from_seeds(seeds, pool);
        pick(&profile, pool, opts, &mut StdRng::seed_from_u64(seed))
    }

    fn pick_default(seeds: &[Candidate], lib: Vec<Candidate>, limit: usize) -> Vec<i64> {
        pick_with(seeds, &pool(lib), &opts(limit), 7)
    }

    #[test]
    fn matching_genre_beats_unrelated_songs() {
        let seeds = [song(1, "A", &["Folk Metal"], 2000, 140.0)];
        let lib = vec![
            song(10, "X", &["Jazz"], 1960, 90.0),
            song(11, "Y", &["Folk Metal"], 2010, 150.0),
            song(12, "Z", &["Ambient"], 1990, 70.0),
        ];
        assert_eq!(pick_default(&seeds, lib, 1), vec![11]);
    }

    #[test]
    fn artist_tags_link_songs_without_shared_genres() {
        let seeds = [song(1, "A", &[], 2000, 0.0)];
        let p = Pool {
            candidates: vec![song(10, "X", &[], 1970, 0.0), song(11, "Y", &[], 1970, 0.0)],
            artist_tags: HashMap::from([
                ("a".to_string(), HashSet::from(["symphonic".to_string()])),
                ("y".to_string(), HashSet::from(["symphonic".to_string()])),
            ]),
            ..Default::default()
        };
        assert_eq!(pick_with(&seeds, &p, &opts(1), 7), vec![11]);
    }

    #[test]
    fn similar_artists_score_well_above_genre_only_matches() {
        let p = pool(vec![]);
        let mut profile = Profile::default();
        profile.genres.insert("rock".to_string());
        profile.set_similar_artists(HashMap::from([(
            "electric light orchestra".to_string(),
            0.9,
        )]));
        let similar = test_song(2, "Electric Light Orchestra", &["Rock"], Some(1970), None);
        let other = test_song(3, "Unknown Band", &["Rock"], Some(1970), None);
        assert!(
            profile.similarity(&similar, &p, &TUNING)
                > profile.similarity(&other, &p, &TUNING) + 3.0
        );
    }

    #[test]
    fn similarity_counts_genres_tags_decades_and_bpm() {
        let p = Pool {
            artist_tags: HashMap::from([(
                "rush".to_string(),
                HashSet::from(["virtuoso".to_string()]),
            )]),
            ..Default::default()
        };
        let mut profile = Profile::default();
        profile.genres.insert("prog-rock".to_string());
        profile.tags.insert("virtuoso".to_string());
        profile.decades.insert(1980);
        profile.bpms.push(120.0);

        let matching = test_song(1, "Rush", &["Prog-Rock"], Some(1980), Some(121.0));
        let unrelated = test_song(2, "Other", &["Classical"], Some(1920), Some(60.0));
        assert!(profile.similarity(&matching, &p, &TUNING) > 7.0);
        assert_eq!(profile.similarity(&unrelated, &p, &TUNING), 0.0);
    }

    /// How often each id is picked first, over `runs` seeds.
    fn first_pick_counts(seeds: &[Candidate], lib: Vec<Candidate>, runs: u64) -> HashMap<i64, u64> {
        let p = pool(lib);
        let mut counts = HashMap::new();
        for seed in 0..runs {
            if let Some(&id) = pick_with(seeds, &p, &opts(1), seed).first() {
                *counts.entry(id).or_default() += 1;
            }
        }
        counts
    }

    /// Share of 10k draws in which entry 0 comes first, for weights `w0, w1`.
    fn first_share(w0: f64, w1: f64) -> f64 {
        let mut rng = StdRng::seed_from_u64(11);
        let draws = 10_000;
        let wins = (0..draws)
            .filter(|_| {
                let order =
                    sample_order(vec![(Tier::Match, w0, 0), (Tier::Match, w1, 1)], &mut rng);
                order[0] == 0
            })
            .count();
        wins as f64 / draws as f64
    }

    #[test]
    fn sampling_frequency_tracks_weight_ratio() {
        assert!((first_share(2.0, 1.0) - 2.0 / 3.0).abs() < 0.02);
        assert!((first_share(3.0, 1.0) - 0.75).abs() < 0.02);
        assert!((first_share(1.0, 1.0) - 0.5).abs() < 0.02);
    }

    #[test]
    fn earlier_tier_always_comes_first_whatever_the_weight() {
        let mut rng = StdRng::seed_from_u64(3);
        for _ in 0..100 {
            let order = sample_order(
                vec![
                    (Tier::Deferred, 1e9, 'd'),
                    (Tier::Unmatched, 1e9, 'u'),
                    (Tier::Match, 1e-9, 'm'),
                ],
                &mut rng,
            );
            assert_eq!(order, vec!['m', 'u', 'd']);
        }
    }

    #[test]
    fn each_score_point_roughly_doubles_the_weight() {
        let ratio = match_weight(7.0) / match_weight(6.0);
        assert!((1.8..2.1).contains(&ratio), "ratio {ratio}");
    }

    #[test]
    fn favourites_are_picked_more_often_but_not_always() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let mut fav = song(11, "Y", &["Rock"], 1980, 120.0);
        fav.rating = 5.0;
        let mut loved = song(12, "Z", &["Rock"], 1980, 120.0);
        loved.loved = 1;
        for boosted in [fav, loved] {
            let id = boosted.id;
            let counts = first_pick_counts(
                &seeds,
                vec![song(10, "X", &["Rock"], 1980, 120.0), boosted],
                400,
            );
            assert!(counts[&id] > 240, "{counts:?}");
            assert!(counts[&10] > 40, "{counts:?}");
        }
    }

    #[test]
    fn nearby_bpm_is_picked_more_often_than_distant_bpm() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let lib = vec![
            song(10, "X", &["Rock"], 1980, 60.0),
            song(11, "Y", &["Rock"], 1980, 122.0),
        ];
        let counts = first_pick_counts(&seeds, lib, 400);
        assert!(counts[&11] > 230, "{counts:?}");
        assert!(counts[&10] > 40, "{counts:?}");
    }

    #[test]
    fn never_picks_seeds_or_excluded_songs() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let mut p = pool(vec![
            song(1, "A", &["Rock"], 1980, 120.0),         // the seed itself
            song(10, "X", &["Rock"], 1980, 120.0),        // caller-excluded
            song(11, "Y", &["Rock"], 1980, 120.0),        // excluded song
            song(12, "Z", &["Rock"], 1980, 120.0),        // excluded artist
            song(13, "W", &["Rock", "Emo"], 1980, 120.0), // excluded genre
            song(14, "V", &["Rock"], 1980, 120.0),
        ]);
        p.exclusions = Exclusions {
            songs: HashSet::from([11]),
            artists: HashSet::from(["z".to_string()]),
            genres: HashSet::from(["emo".to_string()]),
            ..Default::default()
        };
        p.exclude_songs([10]);
        assert_eq!(pick_with(&seeds, &p, &opts(5), 7), vec![14]);
    }

    #[test]
    fn lead_seed_comes_first_even_outside_the_pool() {
        let seeds = [song(99, "A", &["Rock"], 1980, 120.0)];
        let lead = MixOptions {
            lead_with_seed: true,
            ..opts(40)
        };
        assert_eq!(pick_with(&seeds, &pool(vec![]), &lead, 7), vec![99]);
        assert!(pick_with(&[], &pool(vec![]), &lead, 7).is_empty());
    }

    #[test]
    fn lead_counts_toward_its_artists_cap() {
        let seeds = [song(1, "Same", &["Rock"], 1980, 120.0)];
        let lib: Vec<Candidate> = (10..16)
            .map(|id| song(id, "Same", &["Rock"], 1980, 120.0))
            .collect();
        let lead = MixOptions {
            lead_with_seed: true,
            ..opts(10)
        };
        let picked = pick_with(&seeds, &pool(lib), &lead, 7);
        assert_eq!(picked[0], 1);
        assert_eq!(picked.len(), TUNING.max_per_artist);
    }

    #[test]
    fn deferred_recent_plays_come_after_everything_else() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let cutoff = NOW - 3600;
        let mut recent = song(10, "X", &["Rock"], 1980, 120.0);
        recent.lastplayed = Some(NOW - 60);
        let mut stale = song(11, "Y", &["Jazz"], 1950, 60.0);
        stale.lastplayed = Some(cutoff - 1);
        let defer = MixOptions {
            defer_played_since: Some(cutoff),
            ..opts(1)
        };
        // The recent song is the only match, but an unplayed-lately song
        // (even unrelated) fills the slot first.
        let p = pool(vec![recent.clone(), stale]);
        assert_eq!(pick_with(&seeds, &p, &defer, 7), vec![11]);
        // With nothing else left, the recent song is still used.
        assert_eq!(pick_with(&seeds, &pool(vec![recent]), &defer, 7), vec![10]);
    }

    #[test]
    fn falls_back_to_random_library_songs_when_nothing_matches() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let lib: Vec<Candidate> = (10..20)
            .map(|id| song(id, &format!("artist {id}"), &["Jazz"], 1950, 60.0))
            .collect();
        let picked = pick_default(&seeds, lib, 5);
        assert_eq!(picked.len(), 5);
        assert!(picked.iter().all(|id| (10..20).contains(id)));
    }

    #[test]
    fn caps_songs_per_artist() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let lib: Vec<Candidate> = (10..16)
            .map(|id| song(id, "Same", &["Rock"], 1980, 120.0))
            .chain([song(20, "Other", &["Jazz"], 1950, 60.0)])
            .collect();
        let picked = pick_default(&seeds, lib, 3);
        assert_eq!(picked.len(), 3);
        assert_eq!(
            picked.iter().filter(|id| **id < 20).count(),
            TUNING.max_per_artist
        );
        assert!(picked.contains(&20));
    }

    #[test]
    fn empty_pool_picks_nothing() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        assert!(pick_default(&seeds, vec![], 5).is_empty());
        assert!(pick_default(&[], vec![], 5).is_empty());
    }

    #[test]
    fn different_seeds_give_different_queues() {
        let seeds = [song(1, "A", &["Rock"], 1980, 120.0)];
        let p = pool(
            (10..30)
                .map(|id| song(id, &format!("artist {id}"), &["Rock"], 1980, 120.0))
                .collect(),
        );
        let runs: HashSet<Vec<i64>> = (0..8)
            .map(|seed| pick_with(&seeds, &p, &opts(3), seed))
            .collect();
        assert!(runs.len() > 1);
    }
}
