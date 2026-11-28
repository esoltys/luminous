//! Local library radio queue generator (#1579).
//!
//! Generates a playback queue of songs from the user's local collection similar
//! to a seed song or artist. This module resolves the seed (and its artist's
//! MusicBrainz id), fetches ListenBrainz similar artists when online, and plays
//! the result; choosing the songs is `song_mix`'s job. The seed song leads the
//! queue and starts playing immediately.

use crate::models::{playable_library_song_where, PlayContext, ShuffleMode};
use crate::similar_artists::{self, ArtistSeed};
use crate::song_mix::{self, Candidate, MixOptions, Pool, Profile, Tuning};
use crate::AppState;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Default target size of a generated radio queue.
pub const DEFAULT_RADIO_LIMIT: usize = 40;

/// Radio's tuning.
const TUNING: Tuning = Tuning { bpm_window: 15.0 };

fn mix_options(limit: usize, now: i64) -> MixOptions<'static> {
    MixOptions {
        limit,
        lead_with_seed: true,
        now,
        after_artist: None,
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

/// Resolves candidate songs from the local library matching `seed`.
pub async fn generate_radio_songs(
    state: &AppState,
    seed: RadioSeed,
    limit: usize,
) -> Result<RadioResult, String> {
    let db = state.db.clone();
    let (seed_song, seed_title, artist_for_lookup, candidate_mbid) =
        crate::db::run_blocking(&db, {
            let seed = seed.clone();
            move |conn| match seed {
                RadioSeed::Song { song_id } => {
                    let cand = song_mix::load_by_ids(conn, &[song_id])?
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("Song not found: {}", song_id))?;
                    let title = cand
                        .title
                        .clone()
                        .unwrap_or_else(|| "Unknown Song".to_string());
                    let artist_name = cand.artist_key.clone().unwrap_or_default();
                    let mbid = cand.musicbrainz_artist_id.clone();
                    Ok((Some(cand), title, artist_name, mbid))
                }
                RadioSeed::Artist { artist } => {
                    let cand = load_artist_seed_song(conn, &artist)?;
                    let title = artist.clone();
                    let mbid = cand.as_ref().and_then(|c| c.musicbrainz_artist_id.clone());
                    Ok((cand, title, artist.to_lowercase(), mbid))
                }
            }
        })
        .await
        .map_err(|e| e.to_string())?;

    let similar_artists = similar_artists::resolve_for_seeds(
        &db,
        &state.similar_artists,
        vec![ArtistSeed {
            artist_key: artist_for_lookup.clone(),
            musicbrainz_artist_id: candidate_mbid,
        }],
    )
    .await;

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
            &mix_options(limit, crate::clock::unix_now()),
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
    fn seed_song_leads_and_its_artist_is_capped_at_a_quarter_of_the_queue() {
        // A 40-song radio: the seed artist's cap is ceil(40 / 4) = 10, the
        // seed song first. 30 single-song bands fill the rest exactly, so the
        // cap is reached and nothing over it is needed.
        let seed = test_song(1, "The Beatles", &["Rock"], Some(1960), None);
        let pool = Pool {
            candidates: (1..=30)
                .map(|i| test_song(i, "The Beatles", &["Rock"], Some(1960), None))
                .chain(
                    (100..130)
                        .map(|i| test_song(i, &format!("Band {i}"), &["Rock"], Some(1960), None)),
                )
                .collect(),
            ..Default::default()
        };
        let profile = Profile::from_seeds(&[seed], &pool);
        let picked = song_mix::pick(
            &profile,
            &pool,
            &mix_options(DEFAULT_RADIO_LIMIT, 1_800_000_000),
            &mut StdRng::seed_from_u64(42),
        );
        assert_eq!(picked.len(), DEFAULT_RADIO_LIMIT);
        assert_eq!(picked.first(), Some(&1));
        assert_eq!(picked.iter().filter(|&&id| id <= 30).count(), 10);
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
