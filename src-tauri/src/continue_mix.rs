//! Auto Continue (#1235): when the Queue is the playing context and nearly
//! finished, append a small batch of library songs that fit what was just
//! playing, so the music doesn't stop.
//!
//! Selection leans on the user's own curation — genres, artist tags, BPM,
//! decade, album artist, ratings — scored against the last few played songs
//! rather than just the final one. Choosing the songs is `song_mix`'s job;
//! this module decides when to top up, what seeds it, and appends the batch.

use crate::song_mix::{self, MixOptions, Pool, Profile, RatingBoost, Tuning};
use crate::AppState;
use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};

/// Songs appended per top-up.
pub const BATCH_SIZE: usize = 5;
/// How many of the most recently played songs seed the mix.
pub const SEED_COUNT: usize = 3;
/// Top up once this many (or fewer) songs remain after the current one —
/// before the last song ends, so the next one is already queued and the
/// engine's gapless preload still has something to prime.
pub const TOP_UP_THRESHOLD: usize = 1;
/// Songs played within this window go last, so the mix favours songs the
/// user hasn't heard lately.
pub const RECENT_PLAY_WINDOW_SECS: i64 = 3 * 24 * 60 * 60;
/// `playlist_items.additional_metadata` marker on appended rows — the Queue
/// view draws its "Auto Continue" divider before the first one.
pub const MARKER_JSON: &str = r#"{"autoContinue":true}"#;

/// Auto Continue's scoring, unchanged from before the shared engine (#2284)
/// until #2286 / #2288 replace it: at most 2 songs per artist per batch, so a
/// strong match on one artist doesn't turn the continuation into their
/// discography.
const TUNING: Tuning = Tuning {
    max_per_artist: 2,
    // Unused for now: Auto Continue sets no similar-artist map (#2289).
    similar_artist: 0.0,
    same_artist: 1.5,
    same_decade: 1.0,
    bpm_window: 12.0,
    bpm_points: 1.0,
    rating: RatingBoost {
        loved: 0.0,
        five: 1.5,
        four: 1.0,
        three: 0.0,
    },
};

fn mix_options(now: i64, count: usize) -> MixOptions<'static> {
    MixOptions {
        limit: count,
        lead_with_seed: false,
        defer_played_since: Some(now - RECENT_PLAY_WINDOW_SECS),
        tuning: &TUNING,
    }
}

/// Picks up to `count` songs to follow `seed_ids` (most recent first), never
/// one in `exclude_ids` (the songs already in the Queue).
pub fn select_songs(
    conn: &Connection,
    seed_ids: &[i64],
    exclude_ids: &HashSet<i64>,
    now: i64,
    count: usize,
    rng: &mut impl rand::Rng,
) -> Result<Vec<i64>> {
    let seeds = song_mix::load_by_ids(conn, seed_ids)?;
    let mut pool = Pool::load(conn)?;
    pool.exclude_songs(exclude_ids.iter().copied());
    let profile = Profile::from_seeds(&seeds, &pool);
    Ok(song_mix::pick(
        &profile,
        &pool,
        &mix_options(now, count),
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
    let now = crate::clock::unix_now();
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
    use crate::song_mix::test_song;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn defers_recent_plays_and_caps_each_artist_at_two() {
        let seed = test_song(1, "A", &["Rock"], Some(1980), Some(120.0));
        let mut recent = test_song(30, "Fresh Pick", &["Rock"], Some(1980), Some(120.0));
        recent.lastplayed = Some(NOW - 60);
        let pool = Pool {
            candidates: (10..16)
                .map(|id| test_song(id, "Same", &["Rock"], Some(1980), Some(120.0)))
                .chain([
                    test_song(20, "Other", &["Jazz"], Some(1950), Some(60.0)),
                    recent,
                ])
                .collect(),
            ..Default::default()
        };
        let profile = Profile::from_seeds(&[seed], &pool);
        let picked = song_mix::pick(
            &profile,
            &pool,
            &mix_options(NOW, 4),
            &mut StdRng::seed_from_u64(7),
        );
        // Two "Same" matches (the cap), then the unrelated song before the
        // recently played match.
        assert_eq!(picked.len(), 4);
        assert_eq!(picked.iter().filter(|id| (10..16).contains(*id)).count(), 2);
        assert_eq!(&picked[2..], &[20, 30]);
    }
}
