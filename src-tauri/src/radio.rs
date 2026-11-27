//! Local library radio queue generator (#1579).
//!
//! Generates a playback queue of songs from the user's local collection similar
//! to a seed song or artist. This module resolves the seed (and its artist's
//! MusicBrainz id), fetches ListenBrainz similar artists when online, and plays
//! the result; choosing the songs is `song_mix`'s job. The seed song leads the
//! queue and starts playing immediately.

use crate::models::{playable_library_song_where, PlayContext, ShuffleMode};
use crate::song_mix::{self, Candidate, MixOptions, Pool, Profile, RatingBoost, Tuning};
use crate::AppState;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Default target size of a generated radio queue.
pub const DEFAULT_RADIO_LIMIT: usize = 40;

/// Radio's scoring, unchanged from before the shared engine (#2284) until
/// #2286 / #2288 replace it: up to 3 songs per artist (the seed song
/// included), and loved songs boosted like 5-star ones.
const TUNING: Tuning = Tuning {
    max_per_artist: 3,
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

fn mix_options(limit: usize) -> MixOptions<'static> {
    MixOptions {
        limit,
        lead_with_seed: true,
        defer_played_since: None,
        tuning: &TUNING,
    }
}

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

/// The artist's best-loved playable song (loved, then rating, then plays),
/// to seed an artist radio.
fn load_artist_seed_song(conn: &Connection, artist: &str) -> Result<Option<Candidate>> {
    let sql = format!(
        "SELECT id FROM songs
         WHERE (artist = ?1 COLLATE NOCASE OR album_artist = ?1 COLLATE NOCASE)
           AND {lib_where}
         ORDER BY (loved = 1) DESC, rating DESC, playcount DESC, RANDOM() LIMIT 1",
        lib_where = playable_library_song_where("")
    );
    let Some(id) = conn
        .query_row(&sql, params![artist], |r| r.get::<_, i64>(0))
        .optional()?
    else {
        return Ok(None);
    };
    Ok(song_mix::load_by_ids(conn, &[id])?.pop())
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
                        let cand = song_mix::load_by_ids(conn, &[song_id])?
                            .pop()
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

    let result_ids = crate::db::run_blocking(&db, move |conn| {
        let pool = Pool::load(conn)?;
        let mut profile = Profile::from_seeds(seed_song.as_slice(), &pool);
        if seed_song.is_none() {
            profile.add_artist(&artist_for_lookup, &pool);
        }
        profile.set_similar_artists(similar_artists);
        Ok(song_mix::pick(
            &profile,
            &pool,
            &mix_options(limit),
            &mut rand::rng(),
        ))
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
    use crate::song_mix::test_song;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn seed_song_leads_and_caps_each_artist_at_three() {
        let seed = test_song(1, "The Beatles", &["Rock"], Some(1960), None);
        let pool = Pool {
            candidates: (1..=10)
                .map(|i| test_song(i, "The Beatles", &["Rock"], Some(1960), None))
                .chain([test_song(
                    11,
                    "The Rolling Stones",
                    &["Rock"],
                    Some(1960),
                    None,
                )])
                .collect(),
            ..Default::default()
        };
        let profile = Profile::from_seeds(&[seed], &pool);
        let picked = song_mix::pick(
            &profile,
            &pool,
            &mix_options(10),
            &mut StdRng::seed_from_u64(42),
        );
        assert_eq!(picked.first(), Some(&1));
        assert_eq!(picked.iter().filter(|&&id| id <= 10).count(), 3);
        assert!(picked.contains(&11));
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
