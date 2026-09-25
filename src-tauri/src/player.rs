//! Player — high-level playback controller.
//!
//! Mediates between AudioEngine, PlaylistManager, and the Tauri event system.
//! Owns the shuffle index, repeat mode, queue, and scrobble point tracking.

use crate::{
    audio::AudioEngine,
    db::Database,
    models::{
        LoudnessGainSource, PlayContext, PlaybackState, PlaylistItem, RepeatMode, ShuffleMode, Song,
    },
    stats,
};
use anyhow::{anyhow, Result};
use rand::seq::SliceRandom;
use std::sync::Arc;
use tokio::sync::Mutex;

/// After this many consecutive playback failures with no successful
/// `Playing` event in between, the player gives up and stops instead of
/// continuing to advance. Every song under a watched root that's currently
/// unreachable (disconnected drive, sleeping network share) still reads as
/// "available" to the DB — see `find_missing_song_ids` — so without this
/// ceiling, advancing past a failed track (`next_track`) can keep failing
/// forever, and under `RepeatMode::Playlist` specifically has no other
/// terminating condition at all.
pub(crate) const MAX_CONSECUTIVE_PLAYBACK_ERRORS: u32 = 5;

/// Outcome of a single playback failure, returned by `Player::note_playback_error`.
pub struct PlaybackErrorOutcome {
    /// The song that failed to play, for a user-facing toast.
    pub failed_song: Option<Song>,
    /// True once `MAX_CONSECUTIVE_PLAYBACK_ERRORS` consecutive failures have
    /// happened with no successful play in between — the caller should stop
    /// instead of advancing further.
    pub should_stop: bool,
    /// True if the failed song was flagged `unavailable` in the DB as a
    /// result (its file was confirmed gone at the moment of failure) — the
    /// caller should let the frontend know its cached library data is stale.
    pub flagged_unavailable: bool,
}

/// A candidate for the gapless "next track" — resolved by peeking at the
/// playback context without mutating it.
struct GaplessTarget {
    song: Song,
    uuid: Option<String>,
    kind: GaplessTargetKind,
}

enum GaplessTargetKind {
    /// RepeatMode::Track — the current track plays again.
    Replay,
    /// The item at this virtual (shuffle-order) index.
    Index(usize),
    /// The first playable item in the play-next queue.
    Queue,
}

// ---------------------------------------------------------------------------
// Restart-persistence helpers — `Player` owns 8 `app_state` keys (volume,
// shuffle_mode, repeat_mode, last_song_id, last_playlist_id, last_item_uuid,
// last_position_nanosec, last_adhoc_song_ids). These give `Player::new`'s
// restore block and the various persist_*/set_* writers one shared place for
// the raw SQL shape and the enum<->string mapping, mirroring the field↔key
// pattern `commands/settings.rs::UiPreferences::fields()` uses for its own
// app_state keys — the shapes differ enough (`Option<i64>`, a JSON-encoded
// `Vec<i64>`, two non-string enums) that a single literal table doesn't fit,
// so this is the same idea expressed as small typed helpers instead.
// ---------------------------------------------------------------------------

fn app_state_get(conn: &rusqlite::Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM app_state WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get(0),
    )
    .ok()
}

fn app_state_get_parsed<T: std::str::FromStr>(conn: &rusqlite::Connection, key: &str) -> Option<T> {
    app_state_get(conn, key).and_then(|s| s.parse::<T>().ok())
}

fn app_state_set(conn: &rusqlite::Connection, key: &str, value: &str) {
    let _ = conn.execute(
        "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    );
}

fn app_state_clear(conn: &rusqlite::Connection, key: &str) {
    let _ = conn.execute(
        "DELETE FROM app_state WHERE key = ?1",
        rusqlite::params![key],
    );
}

fn shuffle_mode_to_key(mode: ShuffleMode) -> &'static str {
    match mode {
        ShuffleMode::Off => "off",
        ShuffleMode::All => "all",
        ShuffleMode::InsideAlbum => "inside_album",
        ShuffleMode::Albums => "albums",
        ShuffleMode::Artists => "artists",
    }
}

fn shuffle_mode_from_key(key: &str) -> ShuffleMode {
    match key {
        "all" => ShuffleMode::All,
        "inside_album" => ShuffleMode::InsideAlbum,
        "albums" => ShuffleMode::Albums,
        "artists" => ShuffleMode::Artists,
        _ => ShuffleMode::Off,
    }
}

fn repeat_mode_to_key(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "off",
        RepeatMode::Track => "track",
        RepeatMode::Album => "album",
        RepeatMode::Playlist => "playlist",
        RepeatMode::Intro => "intro",
    }
}

fn repeat_mode_from_key(key: &str) -> RepeatMode {
    match key {
        "track" => RepeatMode::Track,
        "album" => RepeatMode::Album,
        "playlist" => RepeatMode::Playlist,
        "intro" => RepeatMode::Intro,
        _ => RepeatMode::Off,
    }
}

pub struct Player {
    _db: Arc<Database>,
    audio: Arc<Mutex<AudioEngine>>,

    // Current playback context
    pub current_song: Option<Song>,
    pub current_playlist_id: Option<i64>,
    pub current_item_uuid: Option<String>,
    /// What the user was inside (album/playlist/standalone) when this
    /// playback queue was started — recorded at the scrobble point for
    /// context-aware "Recently Played".
    current_play_context: Option<PlayContext>,

    // Playback mode
    pub shuffle_mode: ShuffleMode,
    pub repeat_mode: RepeatMode,
    pub stop_after_current: bool,
    pub volume: f32,

    // Loudness normalization (#77) — where the currently applied gain came
    // from, for the player-bar indicator.
    pub current_loudness_source: LoudnessGainSource,
    pub current_loudness_gain_db: Option<f32>,

    // Shuffle state
    /// The playlist items in their current order.
    playlist_items: Vec<PlaylistItem>,
    /// Permuted indices for shuffle playback (virtual index list).
    shuffle_order: Vec<usize>,
    /// Current position in `shuffle_order` (or in `playlist_items` for sequential).
    current_index: Option<usize>,
    /// History stack for Previous navigation in shuffle mode.
    played_indices: Vec<usize>,

    // Queue ("play next") — items prepended before normal order
    queue: std::collections::VecDeque<PlaylistItem>,

    // Scrobble tracking
    /// Position at which we trigger the scrobble (50% of track length).
    scrobble_point_nanosec: Option<u64>,
    scrobbled: bool,
    scrobbler: Option<Arc<crate::scrobbler::ScrobblerManager>>,

    /// Consecutive playback failures since the last successful `Playing`
    /// event — see `MAX_CONSECUTIVE_PLAYBACK_ERRORS`.
    consecutive_playback_errors: u32,
}

/// Runs a synchronous `Player` operation while `player`'s async mutex is
/// held, via `block_in_place` rather than directly — several `Player`
/// methods do rusqlite work, and running that straight on the tokio worker
/// would both stall the runtime and block every other task waiting on the
/// same mutex for the duration (#1097, #1102). Mirrors `playlist::with_playlists`.
pub async fn with_player<F, R>(player: &Mutex<Player>, f: F) -> R
where
    F: FnOnce(&mut Player) -> R,
{
    let mut p = player.lock().await;
    tokio::task::block_in_place(move || f(&mut p))
}

impl Player {
    /// Construct the player and restore state persisted by
    /// `persist_current_song`/`persist_position` from a prior run: volume,
    /// shuffle/repeat mode, and — if the last-played song is still
    /// available — its playlist context and position, cued into the audio
    /// engine paused rather than auto-playing.
    pub fn new(db: Arc<Database>, audio: Arc<Mutex<AudioEngine>>) -> Self {
        let mut volume = 1.0f32;
        let mut shuffle_mode = ShuffleMode::Off;
        let mut repeat_mode = RepeatMode::Off;
        let mut restored_song: Option<Song> = None;
        let mut restored_playlist_id: Option<i64> = None;
        let mut restored_item_uuid: Option<String> = None;
        let mut restored_position_ns: u64 = 0;
        let mut playlist_items: Vec<PlaylistItem> = Vec::new();
        let mut current_index: Option<usize> = None;

        // Query database settings on startup
        if let Ok(conn) = db.pool.get() {
            if let Some(v) = app_state_get_parsed::<f32>(&conn, "volume") {
                volume = v.clamp(0.0, 1.0);
                if let Ok(engine) = audio.try_lock() {
                    let _ = engine.set_volume(volume);
                }
            }
            if let Some(s) = app_state_get(&conn, "shuffle_mode") {
                shuffle_mode = shuffle_mode_from_key(&s);
            }
            if let Some(s) = app_state_get(&conn, "repeat_mode") {
                repeat_mode = repeat_mode_from_key(&s);
            }

            // Restore last played song & position
            if let Some(song_id) = app_state_get_parsed::<i64>(&conn, "last_song_id") {
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1 AND unavailable = 0",
                    crate::collection::SONG_SELECT_COLS
                );
                if let Ok(song) = conn.query_row(
                    &sql,
                    rusqlite::params![song_id],
                    crate::collection::row_to_song,
                ) {
                    restored_song = Some(song);
                }
            }

            if let Some(ref song) = restored_song {
                if let Some(pid) = app_state_get_parsed::<i64>(&conn, "last_playlist_id") {
                    restored_playlist_id = Some(pid);
                }

                if let Some(uuid) = app_state_get(&conn, "last_item_uuid") {
                    restored_item_uuid = Some(uuid);
                }

                if let Some(pos) = app_state_get_parsed::<u64>(&conn, "last_position_nanosec") {
                    restored_position_ns = pos;
                }

                if let Some(pid) = restored_playlist_id {
                    if pid > 0 {
                        if let Ok(items) =
                            crate::playlist::PlaylistManager::get_playlist_tracks_from_conn(
                                &conn, pid,
                            )
                        {
                            if !items.is_empty() {
                                playlist_items = items;
                                if let Some(ref target_uuid) = restored_item_uuid {
                                    current_index =
                                        playlist_items.iter().position(|i| &i.uuid == target_uuid);
                                }
                                if current_index.is_none() {
                                    current_index = playlist_items.iter().position(|i| {
                                        i.song.as_ref().map(|s| s.id) == Some(song.id)
                                    });
                                }
                            }
                        }
                    } else {
                        // Ad-hoc queue (album/artist/search selection played via
                        // play_song/play_songs/open_and_play — not a saved DB
                        // playlist). Its track order only lives in the
                        // `last_adhoc_song_ids` snapshot; without it we'd only
                        // know the single current song and nothing to advance to.
                        if let Some(ids_json) = app_state_get(&conn, "last_adhoc_song_ids") {
                            if let Ok(song_ids) = serde_json::from_str::<Vec<i64>>(&ids_json) {
                                let sql = format!(
                                    "SELECT {} FROM songs WHERE id = ?1 AND unavailable = 0",
                                    crate::collection::SONG_SELECT_COLS
                                );
                                let mut items = Vec::with_capacity(song_ids.len());
                                for (i, sid) in song_ids.iter().enumerate() {
                                    if let Ok(s) = conn.query_row(
                                        &sql,
                                        rusqlite::params![sid],
                                        crate::collection::row_to_song,
                                    ) {
                                        items.push(PlaylistItem::new_song(0, i as i32, s));
                                    }
                                }
                                if !items.is_empty() {
                                    playlist_items = items;
                                    current_index = playlist_items.iter().position(|i| {
                                        i.song.as_ref().map(|s| s.id) == Some(song.id)
                                    });
                                }
                            }
                        }
                    }
                }

                if playlist_items.is_empty() {
                    let uuid = restored_item_uuid
                        .clone()
                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let item = PlaylistItem {
                        id: 0,
                        playlist_id: restored_playlist_id.unwrap_or(0),
                        position: 0,
                        uuid: uuid.clone(),
                        item_type: crate::models::PlaylistItemType::Song,
                        song: Some(song.clone()),
                        url: None,
                        stream_url: None,
                        additional_metadata: None,
                    };
                    restored_item_uuid = Some(uuid);
                    playlist_items = vec![item];
                    current_index = Some(0);
                }
            }
        }

        let (loudness_gain, loudness_source, loudness_gain_db) =
            if let Some(ref song) = restored_song {
                let settings = crate::loudness::get_settings(&db).unwrap_or_default();
                Self::compute_loudness_gain(&settings, song)
            } else {
                (1.0, LoudnessGainSource::Disabled, None)
            };

        let scrobble_point_nanosec = restored_song
            .as_ref()
            .and_then(|s| s.length_nanosec.map(|ns| (ns as u64) / 2));

        let mut player = Self {
            _db: db,
            audio,
            current_song: restored_song.clone(),
            current_playlist_id: restored_playlist_id,
            current_item_uuid: restored_item_uuid,
            current_play_context: None,
            shuffle_mode,
            repeat_mode,
            stop_after_current: false,
            volume,
            current_loudness_source: loudness_source,
            current_loudness_gain_db: loudness_gain_db,
            playlist_items,
            shuffle_order: Vec::new(),
            current_index,
            played_indices: Vec::new(),
            queue: std::collections::VecDeque::new(),
            scrobble_point_nanosec,
            scrobbled: false,
            scrobbler: None,
            consecutive_playback_errors: 0,
        };

        player.rebuild_shuffle_order();

        if let Some(song) = restored_song {
            if let Ok(engine) = player.audio.try_lock() {
                engine.set_loudness_gain(loudness_gain);
                let _ = engine.cue(Box::new(song), restored_position_ns);
            }
        }

        player
    }

    /// Whether `playlist_id` is the app's built-in Queue playlist rather than
    /// a regular/auto playlist — playing from the Queue doesn't set a
    /// "playing from playlist X" context (see `current_play_context` below),
    /// since the Queue *is* the current play session, not something you're
    /// playing "from".
    pub fn is_queue_playlist(&self, playlist_id: i64) -> bool {
        if playlist_id <= 0 {
            return false;
        }
        if let Ok(conn) = self._db.pool.get() {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM playlists WHERE id = ?1 AND dynamic_enabled = 0 AND LOWER(TRIM(name)) = 'queue')",
                rusqlite::params![playlist_id],
                |row| row.get(0),
            )
            .unwrap_or(false)
        } else {
            false
        }
    }

    /// Attach the scrobbler service manager to receive scrobble notifications.
    pub fn set_scrobbler(&mut self, scrobbler: Arc<crate::scrobbler::ScrobblerManager>) {
        self.scrobbler = Some(scrobbler);
    }

    /// Load a playlist into the player and start playing the given index.
    pub async fn play_playlist(
        &mut self,
        items: Vec<PlaylistItem>,
        start_index: usize,
        playlist_id: i64,
        context: Option<PlayContext>,
    ) -> Result<()> {
        self.playlist_items = items;
        self.current_playlist_id = Some(playlist_id);
        self.current_play_context = context.or_else(|| {
            if playlist_id > 0 && !self.is_queue_playlist(playlist_id) {
                Some(PlayContext::Playlist { playlist_id })
            } else {
                None
            }
        });
        self.played_indices.clear();
        self.queue.clear();
        self.scrobbled = false;
        self.current_index = if start_index < self.playlist_items.len() {
            Some(start_index)
        } else {
            None
        };
        self.shuffle_order.clear();

        self.persist_adhoc_queue();

        self.rebuild_shuffle_order();

        let play_index = if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order
                .iter()
                .position(|&i| i == start_index)
                .unwrap_or(0)
        } else {
            start_index
        };

        self.play_at_index(play_index).await
    }

    /// Play a playlist item identified by its unique item UUID.
    pub async fn play_item_by_uuid(
        &mut self,
        playlist_id: i64,
        uuid: &str,
        db_playlists: &crate::playlist::PlaylistManager,
    ) -> Result<()> {
        let is_same_playlist = self.current_playlist_id == Some(playlist_id);
        let has_uuid = is_same_playlist && self.playlist_items.iter().any(|i| i.uuid == uuid);

        if !has_uuid {
            let items = db_playlists.get_playlist_tracks(playlist_id)?;
            self.playlist_items = items;
            self.current_playlist_id = Some(playlist_id);
            self.current_play_context = if playlist_id > 0 && !self.is_queue_playlist(playlist_id) {
                Some(PlayContext::Playlist { playlist_id })
            } else {
                self.current_play_context.clone()
            };
            self.played_indices.clear();
            self.queue.clear();
            self.scrobbled = false;
            self.current_index = None;
            self.shuffle_order.clear();
            self.persist_adhoc_queue();
            self.rebuild_shuffle_order();
        }

        let real_idx = self
            .playlist_items
            .iter()
            .position(|i| i.uuid == uuid)
            .ok_or_else(|| anyhow!("Playlist item with uuid '{uuid}' not found"))?;

        let play_index = if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order
                .iter()
                .position(|&i| i == real_idx)
                .unwrap_or(0)
        } else {
            real_idx
        };

        self.play_at_index(play_index).await
    }

    /// Append songs directly to the in-memory `playlist_items` so the player
    /// can continue playing them seamlessly.  Called by the Auto-Play refill
    /// path after the backend has already persisted the new items to the DB.
    pub fn append_songs_to_playlist_items(&mut self, items: Vec<PlaylistItem>) {
        self.playlist_items.extend(items);
        // Keep the shuffle order in sync (append new indices at the end
        // in sequential order; they'll be reached naturally)
        let new_start = if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order.len()
        } else {
            0 // irrelevant in non-shuffle, order == position
        };
        let new_len = self.playlist_items.len();
        let existing_ordered: std::collections::HashSet<usize> =
            self.shuffle_order.iter().copied().collect();
        for i in new_start..new_len {
            if !existing_ordered.contains(&i) {
                self.shuffle_order.push(i);
            }
        }
    }

    /// Remove items from the in-memory `playlist_items` by uuid, so the live
    /// player stops treating them as up-next after they're deleted from the
    /// backing DB playlist (mirrors `append_songs_to_playlist_items` for the
    /// removal direction — see #262). The currently-playing item is never
    /// dropped this way; it keeps playing to completion even if its DB row
    /// was removed, since yanking it out from under an in-flight decode
    /// would require a mid-playback jump this method doesn't attempt.
    pub fn remove_songs_from_playlist_items(&mut self, uuids: &[String]) {
        if self.playlist_items.is_empty() {
            return;
        }

        let uuid_set: std::collections::HashSet<&str> = uuids.iter().map(String::as_str).collect();
        let current_uuid = self.current_item_uuid.clone();
        let old_uuids: Vec<String> = self.playlist_items.iter().map(|i| i.uuid.clone()).collect();

        self.playlist_items.retain(|item| {
            !uuid_set.contains(item.uuid.as_str())
                || current_uuid.as_deref() == Some(item.uuid.as_str())
        });

        let uuid_to_new_idx: std::collections::HashMap<&str, usize> = self
            .playlist_items
            .iter()
            .enumerate()
            .map(|(idx, item)| (item.uuid.as_str(), idx))
            .collect();

        let new_real_idx = current_uuid
            .as_ref()
            .and_then(|u| uuid_to_new_idx.get(u.as_str()).copied());

        self.played_indices.clear();

        if self.shuffle_mode == ShuffleMode::Off {
            self.current_index = new_real_idx;
            self.shuffle_order = (0..self.playlist_items.len()).collect();
        } else {
            let mut new_shuffle_order = Vec::with_capacity(self.playlist_items.len());
            for &old_real_idx in &self.shuffle_order {
                if let Some(old_uuid) = old_uuids.get(old_real_idx) {
                    if let Some(&new_idx) = uuid_to_new_idx.get(old_uuid.as_str()) {
                        new_shuffle_order.push(new_idx);
                    }
                }
            }
            let mut seen = std::collections::HashSet::new();
            for &idx in &new_shuffle_order {
                seen.insert(idx);
            }
            for i in 0..self.playlist_items.len() {
                if !seen.contains(&i) {
                    new_shuffle_order.push(i);
                }
            }
            self.shuffle_order = new_shuffle_order;
            self.current_index = new_real_idx
                .and_then(|real_idx| self.shuffle_order.iter().position(|&idx| idx == real_idx));
        }

        if let Some(idx) = self.current_index {
            self.played_indices.push(idx);
        }
    }

    /// Reorder items in the in-memory `playlist_items` so the live player
    /// respects track reordering performed in the Queue UI.
    pub fn reorder_playlist_items(&mut self, from: usize, to: usize) {
        if from >= self.playlist_items.len() || to >= self.playlist_items.len() || from == to {
            return;
        }

        let moved = self.playlist_items.remove(from);
        self.playlist_items.insert(to, moved);

        // Update current_index if the playing track was affected by reordering
        let current_uuid = self.current_item_uuid.clone();
        let new_real_idx = current_uuid
            .as_ref()
            .and_then(|u| self.playlist_items.iter().position(|i| &i.uuid == u));

        if self.shuffle_mode == ShuffleMode::Off {
            self.current_index = new_real_idx;
            self.shuffle_order = (0..self.playlist_items.len()).collect();
        } else {
            for idx in &mut self.shuffle_order {
                if *idx == from {
                    *idx = to;
                } else if from < to && *idx > from && *idx <= to {
                    *idx -= 1;
                } else if from > to && *idx >= to && *idx < from {
                    *idx += 1;
                }
            }
            self.current_index = new_real_idx
                .and_then(|real_idx| self.shuffle_order.iter().position(|&i| i == real_idx));
        }
    }

    /// Reorder item in in-memory playlist_items targeted by item UUIDs.
    pub fn reorder_playlist_item_by_uuid(&mut self, source_uuid: &str, target_uuid: &str) {
        if source_uuid == target_uuid {
            return;
        }

        let from_idx = self
            .playlist_items
            .iter()
            .position(|i| i.uuid == source_uuid);
        let to_idx = self
            .playlist_items
            .iter()
            .position(|i| i.uuid == target_uuid);

        if let (Some(from), Some(to)) = (from_idx, to_idx) {
            self.reorder_playlist_items(from, to);
        }
    }

    /// Clear all in-memory playlist items and queue state when the Queue is cleared.
    pub fn clear_playlist_items(&mut self) {
        self.playlist_items.clear();
        self.shuffle_order.clear();
        self.queue.clear();
        self.current_index = None;
        self.played_indices.clear();
    }

    /// Number of playlist items that have not yet been played (ahead of current index).
    pub fn remaining_playlist_items(&self) -> usize {
        let total = self.playlist_items.len();
        match self.current_index {
            Some(idx) => total.saturating_sub(idx + 1),
            None => total,
        }
    }

    /// Returns true if the playlist item has a playable (present + available) song.
    fn is_item_playable(item: &PlaylistItem) -> bool {
        match &item.song {
            Some(song) => !song.unavailable,
            None => false, // legacy ghost row (song_id = NULL)
        }
    }

    /// The single "start a track" sequence shared by `play_at_index`,
    /// `next_track`'s queue branch, and `on_gapless_transition`'s commit —
    /// scrobble point, current song/uuid/(index or queue-pop), loudness
    /// gain, waveform preload, and restart persistence. `kind` mirrors
    /// `peek_next_natural`'s already-decided target (#1072); `real_play`
    /// issues an actual `AudioEngine::play` call, skipped for gapless
    /// commits where the audio is already playing (position is persisted as
    /// 0 in that case, matching a track that just started).
    async fn start_track(
        &mut self,
        song: Song,
        uuid: Option<String>,
        kind: GaplessTargetKind,
        real_play: bool,
    ) -> Result<()> {
        let start_ns = song.beginning_nanosec.max(0) as u64;

        self.scrobble_point_nanosec = song.length_nanosec.map(|ns| (ns as u64) / 2);
        self.scrobbled = false;

        match kind {
            GaplessTargetKind::Replay => {
                // current_song/uuid/index are already correct.
            }
            GaplessTargetKind::Index(candidate) => {
                self.current_song = Some(song.clone());
                self.current_item_uuid = uuid;
                self.current_index = Some(candidate);
                if !self.played_indices.contains(&candidate) {
                    self.played_indices.push(candidate);
                }
            }
            GaplessTargetKind::Queue => {
                // Drop unplayable fronts, then the item that's starting.
                while let Some(front) = self.queue.front() {
                    if Self::is_item_playable(front) {
                        break;
                    }
                    self.queue.pop_front();
                }
                self.queue.pop_front();
                self.current_song = Some(song.clone());
                self.current_item_uuid = uuid;
            }
        }

        self.apply_loudness_gain(&song).await;
        self.preload_upcoming_waveforms();
        self.persist_current_song().await;
        self.persist_position(if real_play { start_ns } else { 0 })
            .await;

        if real_play {
            let audio = self.audio.lock().await;
            audio.play(Box::new(song), start_ns)
        } else {
            Ok(())
        }
    }

    /// Play the item at the given index (in virtual/shuffle order).
    /// If the item is unavailable, auto-advances to the next playable track.
    async fn play_at_index(&mut self, index: usize) -> Result<()> {
        let Some(candidate) = self.find_playable_from(index) else {
            log::warn!("Entire playlist contains only unavailable tracks — stopping.");
            return self.stop().await;
        };

        let item_index = self
            .resolve_item_index(candidate)
            .ok_or(anyhow!("index out of bounds"))?;

        let item = self
            .playlist_items
            .get(item_index)
            .ok_or(anyhow!("playlist item not found"))?;

        let song = item
            .song
            .clone()
            .ok_or(anyhow!("playlist item has no song"))?;
        let uuid = Some(item.uuid.clone());

        self.start_track(song, uuid, GaplessTargetKind::Index(candidate), true)
            .await
    }

    /// Proactively pre-generates waveform visualizer data for the current song
    /// and the upcoming next track in the queue/playlist on a background thread.
    pub fn preload_upcoming_waveforms(&self) {
        let db = Arc::clone(&self._db);
        let mut songs_to_preload = Vec::new();

        if let Some(ref song) = self.current_song {
            if let Some(ref path_str) = song.path {
                songs_to_preload.push((song.id, std::path::PathBuf::from(path_str)));
            }
        }

        if let Some(target) = self.peek_next_natural() {
            if let Some(ref path_str) = target.song.path {
                songs_to_preload.push((target.song.id, std::path::PathBuf::from(path_str)));
            }
        }

        if songs_to_preload.is_empty() {
            return;
        }

        tauri::async_runtime::spawn_blocking(move || {
            for (song_id, path) in songs_to_preload {
                if let Err(e) = crate::waveform::generate_visualizer_data(&db, song_id, &path) {
                    log::debug!(
                        "Proactive waveform pre-generation skipped for song {song_id}: {e}"
                    );
                }
            }
        });
    }

    /// Persist the ordered song-id list for ad-hoc queues (`playlist_id ==
    /// 0` — album/artist/search selections, not a saved DB playlist) so a
    /// restart can rebuild the full playback context instead of just the
    /// current song. Saved playlists don't need this: they're reloaded from
    /// the `playlists`/`playlist_items` tables via `last_playlist_id`.
    fn persist_adhoc_queue(&self) {
        if let Ok(conn) = self._db.pool.get() {
            if self.current_playlist_id == Some(0) {
                let song_ids: Vec<i64> = self
                    .playlist_items
                    .iter()
                    .filter_map(|i| i.song.as_ref().map(|s| s.id))
                    .collect();
                if let Ok(json) = serde_json::to_string(&song_ids) {
                    app_state_set(&conn, "last_adhoc_song_ids", &json);
                }
            } else {
                app_state_clear(&conn, "last_adhoc_song_ids");
            }
        }
    }

    /// Write the current song/playlist/queue-item identity to `app_state` so
    /// playback can resume across an app restart (see `Player::new`'s
    /// startup restore). `None` fields clear their key rather than leaving a
    /// stale value behind. Called on every track change; separate from
    /// `persist_position`, which `lib.rs` also calls periodically on its own
    /// while a track just keeps playing.
    ///
    /// Runs the rusqlite write on a blocking thread (#1097) — every caller
    /// already holds `AppState.player`'s async mutex, so doing this
    /// synchronously on the tokio worker would stall every other IPC command
    /// waiting on that lock for as long as the r2d2 pool takes.
    pub async fn persist_current_song(&self) {
        let db = self._db.clone();
        let song_id = self.current_song.as_ref().map(|s| s.id);
        let playlist_id = self.current_playlist_id;
        let item_uuid = self.current_item_uuid.clone();
        let result = tokio::task::spawn_blocking(move || {
            if let Ok(conn) = db.pool.get() {
                match song_id {
                    Some(id) => app_state_set(&conn, "last_song_id", &id.to_string()),
                    None => app_state_clear(&conn, "last_song_id"),
                }

                match playlist_id {
                    Some(pid) => app_state_set(&conn, "last_playlist_id", &pid.to_string()),
                    None => app_state_clear(&conn, "last_playlist_id"),
                }

                match &item_uuid {
                    Some(uuid) => app_state_set(&conn, "last_item_uuid", uuid),
                    None => app_state_clear(&conn, "last_item_uuid"),
                }
            }
        })
        .await;
        if let Err(e) = result {
            log::warn!("persist_current_song task panicked: {e}");
        }
    }

    /// Write the current playback position to `app_state` for restart
    /// restore. Called on every seek/pause/track-change and, while a track
    /// keeps playing, periodically by `lib.rs`'s position-tick loop —
    /// intentionally cheap (one `INSERT OR REPLACE`) since it runs often.
    ///
    /// Runs on a blocking thread for the same reason as
    /// `persist_current_song` above (#1097).
    pub async fn persist_position(&self, position_nanosec: u64) {
        let db = self._db.clone();
        let result = tokio::task::spawn_blocking(move || {
            if let Ok(conn) = db.pool.get() {
                app_state_set(
                    &conn,
                    "last_position_nanosec",
                    &position_nanosec.to_string(),
                );
            }
        })
        .await;
        if let Err(e) = result {
            log::warn!("persist_position task panicked: {e}");
        }
    }

    /// How long a live loudness-gain change ramps over when a track is
    /// already audible (`refresh_loudness_gain`), to avoid an audible step
    /// in level. Track-start application (`apply_loudness_gain`) applies
    /// instantly instead — there's no continuous waveform across a track
    /// boundary for a step to be audible against.
    const LOUDNESS_REFRESH_RAMP_MS: u32 = 150;

    /// Runs the (synchronous, rusqlite) loudness-settings read on a blocking
    /// thread rather than the tokio worker calling this — both call sites run
    /// while a caller holds `AppState.player`'s async mutex, so blocking the
    /// worker here would stall every other IPC command waiting on that lock
    /// for as long as the r2d2 pool takes to hand back a connection.
    async fn load_loudness_settings(db: &Arc<Database>) -> Result<crate::models::LoudnessSettings> {
        let db = db.clone();
        tokio::task::spawn_blocking(move || crate::loudness::get_settings(&db))
            .await
            .map_err(|e| anyhow!("loudness settings task panicked: {e}"))?
    }

    fn compute_loudness_gain(
        settings: &crate::models::LoudnessSettings,
        song: &Song,
    ) -> (f32, LoudnessGainSource, Option<f32>) {
        if settings.enabled {
            let result = crate::loudness::compute_gain(
                song.ebur128_integrated_loudness_lufs,
                song.replaygain_track_gain,
                song.replaygain_album_gain,
                song.dynamic_range_rms,
                song.dynamic_range_peak,
                settings,
            );
            (result.linear, result.source, Some(result.gain_db))
        } else {
            (1.0, LoudnessGainSource::Disabled, None)
        }
    }

    /// Recompute and apply the loudness-normalization gain (#77) for a track
    /// that is about to become audible. Called for every non-gapless track
    /// start; for gapless handovers it's applied at the actual audible
    /// boundary (`on_gapless_transition`) instead, since the DSP gain slot is
    /// global and flipping it early would affect the still-draining previous
    /// track's tail.
    async fn apply_loudness_gain(&mut self, song: &Song) {
        let settings = match Self::load_loudness_settings(&self._db).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to load loudness settings: {e}");
                return;
            }
        };
        let (gain, source, gain_db) = Self::compute_loudness_gain(&settings, song);
        self.current_loudness_source = source;
        self.current_loudness_gain_db = gain_db;
        self.audio.lock().await.set_loudness_gain(gain);
    }

    /// Re-apply the loudness gain for the currently playing track — called
    /// after a loudness setting changes, so the effect is heard immediately
    /// rather than waiting for the next track change. Ramps to the new gain
    /// instead of stepping it, since (unlike a track boundary) this changes
    /// the level in the middle of the same continuous waveform and a hard
    /// step would be an audible click/zipper.
    pub async fn refresh_loudness_gain(&mut self) {
        let Some(song) = self.current_song.clone() else {
            return;
        };
        let settings = match Self::load_loudness_settings(&self._db).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to load loudness settings: {e}");
                return;
            }
        };
        let (target_gain, source, gain_db) = Self::compute_loudness_gain(&settings, &song);
        self.current_loudness_source = source;
        self.current_loudness_gain_db = gain_db;

        let handle = self.audio.lock().await.loudness_gain_handle();
        crate::audio::ramp_gain(&handle, target_gain, Self::LOUDNESS_REFRESH_RAMP_MS).await;
    }

    /// Sync `is_instrumental` into this song's in-memory copies (current
    /// song, cached playlist items) after the DB row has already been
    /// updated elsewhere (`commands::lyrics::set_instrumental`) — the
    /// Player holds its own `Song` copies rather than re-querying the DB on
    /// every read, so a direct DB write alone wouldn't be reflected here.
    pub fn update_song_instrumental(&mut self, song_id: i64, is_instrumental: bool) {
        if let Some(ref mut song) = self.current_song {
            if song.id == song_id {
                song.is_instrumental = is_instrumental;
            }
        }
        for item in &mut self.playlist_items {
            if let Some(ref mut song) = item.song {
                if song.id == song_id {
                    song.is_instrumental = is_instrumental;
                }
            }
        }
        for item in &mut self.queue {
            if let Some(ref mut song) = item.song {
                if song.id == song_id {
                    song.is_instrumental = is_instrumental;
                }
            }
        }
    }

    /// `Some(ms)` when pause/resume/stop transitions should fade over `ms`
    /// milliseconds, `None` when they should apply instantly — the one
    /// decision `pause`, `resume`, and `stop` each otherwise re-derived from
    /// the same DB settings independently.
    ///
    /// Runs the rusqlite read on a blocking thread (#1097) — every caller is
    /// itself called while `AppState.player`'s async mutex is held, so a
    /// synchronous read here would stall every other IPC command waiting on
    /// that lock for as long as the r2d2 pool takes.
    async fn fade_duration_ms(&self) -> Option<u32> {
        let db = self._db.clone();
        let settings = tokio::task::spawn_blocking(move || {
            crate::fade::get_fade_settings_from_db(&db).unwrap_or_default()
        })
        .await
        .unwrap_or_default();
        (settings.fade_pause_enabled && settings.fade_pause_duration_ms > 0)
            .then_some(settings.fade_pause_duration_ms)
    }

    pub async fn pause(&self) -> Result<()> {
        let pos = self.audio.lock().await.current_position_nanosec();
        self.persist_position(pos).await;
        match self.fade_duration_ms().await {
            Some(ms) => self.audio.lock().await.pause_with_fade(ms),
            None => self.audio.lock().await.pause(),
        }
    }

    pub async fn resume(&self) -> Result<()> {
        match self.fade_duration_ms().await {
            Some(ms) => self.audio.lock().await.resume_with_fade(ms),
            None => self.audio.lock().await.resume(),
        }
    }

    /// Called by the audio-event loop before treating a playback failure as a
    /// real skip. If the current song's path is only stale-cased (see
    /// `collection::resolve_case_insensitive_path`), repoints it to the real
    /// on-disk path in the DB and retries the same track in place, so the
    /// user never sees a toast for what's really just a Linux/case-sensitive-
    /// filesystem quirk. Returns `true` if the caller should treat the error
    /// as handled and not surface it — either because a heal+retry was just
    /// attempted, or because the file already exists under the path we have,
    /// meaning this `Error` event is stale: the audio engine can report more
    /// than one `Error` for the same failed open (e.g. a benign device-level
    /// error alongside the real decode error), and by the time a later one
    /// arrives an earlier heal may have already fixed and retried the track.
    /// Without this check that stale event would fall through to the normal
    /// failure path and wrongly skip a track that's already playing fine.
    pub async fn try_heal_and_retry_current_track(&mut self) -> bool {
        let Some(path) = self.current_song.as_ref().and_then(|s| s.path.clone()) else {
            return false;
        };
        if std::path::Path::new(&path).exists() {
            return true;
        }
        let Some(healed) =
            crate::collection::resolve_case_insensitive_path(std::path::Path::new(&path))
        else {
            return false;
        };
        let healed_str = healed.to_string_lossy().to_string();
        if healed_str == path {
            return false;
        }

        let Some(song_id) = self.current_song.as_ref().map(|s| s.id) else {
            return false;
        };
        let mtime = crate::collection::get_mtime(&healed).unwrap_or(0);
        let updated = match self._db.pool.get() {
            Ok(conn) => conn
                .execute(
                    "UPDATE songs SET path = ?1, mtime = ?2 WHERE id = ?3",
                    rusqlite::params![healed_str, mtime, song_id],
                )
                .map(|n| n > 0)
                .unwrap_or(false),
            Err(_) => false,
        };
        if !updated {
            return false;
        }

        if let Some(song) = self.current_song.as_mut() {
            song.path = Some(healed_str.clone());
        }
        if let Some(uuid) = self.current_item_uuid.clone() {
            if let Some(item) = self.playlist_items.iter_mut().find(|i| i.uuid == uuid) {
                if let Some(song) = item.song.as_mut() {
                    song.path = Some(healed_str.clone());
                }
            }
        }

        if let Some(idx) = self.current_index {
            let _ = self.play_at_index(idx).await;
        }
        true
    }

    /// Called by the audio-event loop when the engine reports it couldn't
    /// open/decode the current track. Flags the failed song `unavailable` if
    /// its file is confirmed gone right now (a live single-file check, not
    /// the directory-unreachable heuristic `find_missing_song_ids` uses —
    /// so it's safe even when the drive as a whole is still being treated as
    /// "can't currently verify" rather than "confirmed missing").
    pub fn note_playback_error(&mut self) -> PlaybackErrorOutcome {
        self.consecutive_playback_errors += 1;
        let should_stop = self.consecutive_playback_errors >= MAX_CONSECUTIVE_PLAYBACK_ERRORS;

        let mut flagged_unavailable = false;
        if let Some(song) = &self.current_song {
            // Remote songs (WebDAV, OpenSubsonic) have URL/URI paths — `Path::exists()`
            // always returns false for them, so a transient network/auth failure would
            // otherwise get misread as "confirmed missing" and hide the song from every
            // library view (which all filter on `unavailable = 0`). They're managed by
            // their server sync instead, mirroring the same exemption in
            // `find_missing_song_ids` (see collection.rs).
            let missing_on_disk = !song.source.is_remote()
                && song
                    .path
                    .as_deref()
                    .map(|p| !std::path::Path::new(p).exists())
                    .unwrap_or(false);
            if missing_on_disk {
                if let Ok(conn) = self._db.pool.get() {
                    flagged_unavailable = conn
                        .execute(
                            "UPDATE songs SET unavailable = 1 WHERE id = ?1 AND unavailable = 0",
                            rusqlite::params![song.id],
                        )
                        .map(|n| n > 0)
                        .unwrap_or(false);
                }
            }
        }

        PlaybackErrorOutcome {
            failed_song: self.current_song.clone(),
            should_stop,
            flagged_unavailable,
        }
    }

    /// Clears the consecutive-failure counter — called whenever the engine
    /// confirms a track actually started playing.
    pub fn reset_playback_errors(&mut self) {
        self.consecutive_playback_errors = 0;
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.current_song = None;
        self.current_item_uuid = None;
        self.current_playlist_id = None;
        self.persist_current_song().await;
        self.persist_position(0).await;
        match self.fade_duration_ms().await {
            Some(ms) => self.audio.lock().await.stop_with_fade(ms),
            None => self.audio.lock().await.stop(),
        }
    }

    /// `position_nanosec` is track-relative (0 at the start of the current
    /// song, matching what the UI displays and what `Song::duration_secs()`
    /// covers) — converted here to the audio engine's absolute-file-offset
    /// convention by adding the current song's `beginning_nanosec` (0 for a
    /// plain, non-CUE song, so this is a no-op for the common case).
    pub async fn seek_to(&self, position_nanosec: u64) -> Result<()> {
        let absolute_ns = position_nanosec + self.current_song_beginning_nanosec();
        self.persist_position(absolute_ns).await;
        self.audio.lock().await.seek_to(absolute_ns)
    }

    /// The current song's CUE start offset within its physical file (0 for a
    /// plain, non-CUE song) — the audio engine's `position_nanosec` is
    /// absolute within that file, but everything the UI and play-stats logic
    /// deal in is relative to the track's own start (#78).
    pub fn current_song_beginning_nanosec(&self) -> u64 {
        self.current_song
            .as_ref()
            .map(|s| s.beginning_nanosec.max(0) as u64)
            .unwrap_or(0)
    }

    pub async fn set_volume(&mut self, vol: f32) -> Result<()> {
        self.volume = vol.clamp(0.0, 1.0);
        let audio = self.audio.lock().await;
        let _ = audio.set_volume(self.volume);
        if let Ok(conn) = self._db.pool.get() {
            app_state_set(&conn, "volume", &self.volume.to_string());
        }
        Ok(())
    }

    /// Advance playback: first drains any "play next" `queue` entries
    /// (skipping ones that became unplayable), and only once that's empty
    /// falls back to the normal shuffle/repeat-aware playlist advance. This
    /// `queue` is the ad-hoc "play next" list, unrelated to the persistent
    /// Queue *playlist* (`is_queue_playlist`).
    pub async fn next_track(&mut self) -> Result<()> {
        while let Some(front) = self.queue.front() {
            if Self::is_item_playable(front) {
                break;
            }
            log::debug!("Skipping unavailable queued item");
            self.queue.pop_front();
        }

        if let Some(queued) = self.queue.front() {
            let song = queued
                .song
                .clone()
                .ok_or(anyhow!("queued item has no song"))?;
            let uuid = Some(queued.uuid.clone());
            return self
                .start_track(song, uuid, GaplessTargetKind::Queue, true)
                .await;
        }

        let next_index = self.get_next_index();
        match next_index {
            Some(idx) => self.play_at_index(idx).await,
            None => self.stop().await,
        }
    }

    /// Go back a track: in shuffle mode, walks back through actual play
    /// history (see the comment below on `played_indices`); otherwise steps
    /// to `current_index - 1` in playlist order.
    pub async fn previous_track(&mut self) -> Result<()> {
        // In shuffle mode, walk back through history (skip unavailable).
        // `play_at_index` pushes every track it plays onto `played_indices`,
        // including the current one — so the entry on top of the stack right
        // now is the current track's own history entry, not the prior track.
        // Discard it (and any other stale entries matching current_index)
        // before treating a popped entry as the destination, or Previous
        // just replays the current song instead of moving back (#105).
        if self.shuffle_mode != ShuffleMode::Off {
            while let Some(prev_index) = self.played_indices.pop() {
                if Some(prev_index) == self.current_index {
                    continue;
                }
                if self.is_playable_at(prev_index) {
                    return self.play_at_index(prev_index).await;
                }
            }
        }

        // Walk backwards from current, skipping unavailable items. Mirrors
        // `get_next_index`'s boundary behavior: only wrap past the start
        // under `RepeatMode::Playlist` — otherwise Previous at the first
        // track is a no-op instead of jumping to the last track. The wrap
        // applies wherever the walk crosses the start, not only when it
        // begins there, so unavailable tracks before the current one can't
        // strand Previous (#1223).
        if let Some(current) = self.current_index {
            let len = self.playlist_items.len();
            if len == 0 {
                return Ok(());
            }
            let wrap = self.repeat_mode == RepeatMode::Playlist;
            let mut candidate = if current > 0 {
                current - 1
            } else if wrap {
                len - 1
            } else {
                return Ok(());
            };
            for _ in 0..len {
                if self.is_playable_at(candidate) {
                    return self.play_at_index(candidate).await;
                }
                candidate = match candidate {
                    0 if wrap => len - 1,
                    0 => break,
                    c => c - 1,
                };
            }
        }
        Ok(())
    }

    /// Number of virtual indices in whichever order is currently active:
    /// `shuffle_order` while shuffling, `playlist_items` otherwise.
    /// `rebuild_shuffle_order` keeps `shuffle_order` sized to match
    /// `playlist_items` even when shuffle is off (as the identity mapping),
    /// so every index-walking helper below can treat "virtual index" as the
    /// single space to reason about instead of re-deriving it per call site.
    fn virtual_len(&self) -> usize {
        if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order.len()
        } else {
            self.playlist_items.len()
        }
    }

    /// Resolves a virtual index (position in the active play order) to its
    /// underlying `playlist_items` index — identity when shuffle is off,
    /// indirected through `shuffle_order` when shuffle is on. `None` only if
    /// `virtual_index` is out of range for whichever indexing is active.
    fn resolve_item_index(&self, virtual_index: usize) -> Option<usize> {
        if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order.get(virtual_index).copied()
        } else {
            (virtual_index < self.playlist_items.len()).then_some(virtual_index)
        }
    }

    /// True if the playlist item at virtual index `candidate` exists and is
    /// currently playable (see `is_item_playable`).
    fn is_playable_at(&self, candidate: usize) -> bool {
        self.resolve_item_index(candidate)
            .and_then(|i| self.playlist_items.get(i))
            .map(Self::is_item_playable)
            .unwrap_or(false)
    }

    /// Read-only walk forward from a virtual index (wrapping through the
    /// active play order) to the first playable item, mirroring
    /// `play_at_index`'s skip-unavailable behavior. `None` only if every
    /// item was checked without finding one (an all-unavailable playlist).
    fn find_playable_from(&self, start: usize) -> Option<usize> {
        let total = self.virtual_len();
        if total == 0 {
            return None;
        }
        let mut candidate = start % total;
        for _ in 0..total {
            if self.is_playable_at(candidate) {
                return Some(candidate);
            }
            candidate = (candidate + 1) % total;
        }
        None
    }

    /// Read-only walk from a virtual index to the first playable item,
    /// mirroring `play_at_index`'s skip-unavailable behavior.
    fn peek_playable_index(&self, index: usize) -> Option<usize> {
        self.find_playable_from(index)
    }

    /// Determine what will play after the current track ends naturally,
    /// without mutating any state. The single source of truth for "what
    /// plays next" (#1072) — `prepare_gapless_next` preloads this,
    /// `on_gapless_transition` commits it silently, and `on_track_finished`
    /// commits it with a real `Play` call.
    fn peek_next_natural(&self) -> Option<GaplessTarget> {
        if self.stop_after_current {
            return None;
        }

        if self.repeat_mode == RepeatMode::Track {
            self.current_index?; // only replay when a playlist track is loaded
            let song = self.current_song.clone()?;
            return Some(GaplessTarget {
                song,
                uuid: self.current_item_uuid.clone(),
                kind: GaplessTargetKind::Replay,
            });
        }

        // Queue first (peek without popping) — every other repeat mode,
        // including Playlist (#1073), lets a queued "play next" track take
        // priority over the natural playlist advance, matching manual skip.
        if let Some(item) = self.queue.iter().find(|i| Self::is_item_playable(i)) {
            let song = item.song.clone()?;
            return Some(GaplessTarget {
                song,
                uuid: Some(item.uuid.clone()),
                kind: GaplessTargetKind::Queue,
            });
        }

        // RepeatMode::Album scopes the *natural* advance to the current
        // album's own tracks (wrapping within it) — but this must not reach
        // manual skip (get_next_index, also used by next_track): a track
        // with no/blank album tag is its own group of one, so scoping
        // manual Next the same way would make it appear to do nothing.
        // RepeatMode::Playlist wraps to the first track when nothing else is
        // playing yet (current_index is None); every other mode simply has
        // nothing left to advance to at that point.
        let idx = if self.repeat_mode == RepeatMode::Album {
            self.next_album_index()?
        } else if self.repeat_mode == RepeatMode::Playlist {
            self.get_next_index().unwrap_or(0)
        } else {
            self.get_next_index()?
        };
        let candidate = self.peek_playable_index(idx)?;
        self.target_at_virtual_index(candidate)
    }

    fn target_at_virtual_index(&self, candidate: usize) -> Option<GaplessTarget> {
        let item_index = self.resolve_item_index(candidate)?;
        let item = self.playlist_items.get(item_index)?;
        let song = item.song.clone()?;
        Some(GaplessTarget {
            song,
            uuid: Some(item.uuid.clone()),
            kind: GaplessTargetKind::Index(candidate),
        })
    }

    /// Respond to the engine's `AboutToFinish` signal: prime the next track
    /// for a gapless or crossfade handover (#79). Does nothing when playback will
    /// naturally stop after the current track.
    pub async fn prepare_gapless_next(&mut self) -> Result<()> {
        self.preload_upcoming_waveforms();
        let Some(target) = self.peek_next_natural() else {
            return Ok(());
        };
        let start_ns = target.song.beginning_nanosec.max(0) as u64;

        let fade_settings = crate::fade::get_fade_settings_from_db(&self._db).unwrap_or_default();

        let is_same_album = if let Some(current) = &self.current_song {
            current.is_same_album_or_cue_sibling(&target.song)
        } else {
            false
        };

        if fade_settings.crossfade_auto_enabled
            && (!fade_settings.crossfade_suppress_same_album || !is_same_album)
        {
            log::info!(
                "Auto-crossfade armed for track transition to song {}",
                target.song.id
            );
            self.audio.lock().await.preload_next_with_crossfade(
                Box::new(target.song),
                start_ns,
                fade_settings.crossfade_auto_duration_secs,
            )
        } else {
            self.audio
                .lock()
                .await
                .preload_next(Box::new(target.song), start_ns)
        }
    }

    /// Commit a completed gapless handover reported by the engine. Advances
    /// queue/index/scrobble bookkeeping exactly as `on_track_finished` would,
    /// but without issuing a new `Play` (the audio never stopped). If the
    /// playback context changed since the preload (mode/queue edits), falls
    /// back to the normal advance logic to self-heal.
    pub async fn on_gapless_transition(&mut self, started_song_id: i64) -> Result<()> {
        if self.stop_after_current {
            self.stop_after_current = false;
            return self.stop().await;
        }

        match self.peek_next_natural() {
            Some(target) if target.song.id == started_song_id => {
                self.start_track(target.song, target.uuid, target.kind, false)
                    .await
            }
            _ => {
                // The preloaded track no longer matches what should play —
                // correct by running the normal advance (issues a real Play).
                log::warn!(
                    "Gapless transition to song {started_song_id} no longer matches playback context; correcting"
                );
                self.on_track_finished().await
            }
        }
    }

    /// Called when the audio engine reports a track has finished. Commits
    /// `peek_next_natural`'s decision with a real `Play` call through the
    /// same `start_track` sequence `on_gapless_transition` commits silently
    /// and `prepare_gapless_next` preloads — so there is exactly one place
    /// that decides what plays next and exactly one place that starts one.
    pub async fn on_track_finished(&mut self) -> Result<()> {
        if self.stop_after_current {
            self.stop_after_current = false;
            return self.stop().await;
        }

        if let Some(target) = self.peek_next_natural() {
            return self
                .start_track(target.song, target.uuid, target.kind, true)
                .await;
        }

        self.next_track().await
    }

    /// The key `ShuffleMode::Albums`/`InsideAlbum` group tracks under, also
    /// reused by `RepeatMode::Album` to find "the current album"'s tracks —
    /// a single source of truth so both features agree on what counts as
    /// the same album. Falls back to the item's own uuid when the song has
    /// no (or blank) album tag, so an ungrouped track forms a group of one
    /// rather than colliding with other untagged tracks.
    fn album_key(item: &PlaylistItem) -> String {
        if let Some(ref song) = item.song {
            if let Some(ref album) = song.album {
                if !album.trim().is_empty() {
                    return album.to_lowercase();
                }
            }
        }
        item.uuid.clone()
    }

    /// The next virtual index within the current track's album, wrapping
    /// back to the album's first (virtual-order) track after its last —
    /// `RepeatMode::Album`'s "loop the current album indefinitely". A
    /// single-track album (or an ungrouped track, its own group of one)
    /// loops back to itself, mirroring `RepeatMode::Track`'s replay.
    fn next_album_index(&self) -> Option<usize> {
        let total = self.virtual_len();
        if total == 0 {
            return None;
        }
        let current_virtual = self.current_index?;
        let current_real = self.resolve_item_index(current_virtual)?;
        let key = Self::album_key(self.playlist_items.get(current_real)?);

        let album_virtual_indices: Vec<usize> = (0..total)
            .filter(|&v| {
                self.resolve_item_index(v)
                    .and_then(|r| self.playlist_items.get(r))
                    .map(|item| Self::album_key(item) == key)
                    .unwrap_or(false)
            })
            .collect();

        let pos = album_virtual_indices
            .iter()
            .position(|&v| v == current_virtual)?;
        let next_pos = (pos + 1) % album_virtual_indices.len();
        Some(album_virtual_indices[next_pos])
    }

    /// Compute the next playback index based on mode.
    fn get_next_index(&self) -> Option<usize> {
        let total = self.virtual_len();
        if total == 0 {
            return None;
        }

        let current = self.current_index?;

        let next = current + 1;
        if next < total {
            Some(next)
        } else {
            match self.repeat_mode {
                RepeatMode::Playlist => Some(0),
                _ => None,
            }
        }
    }

    /// Shared by `ShuffleMode::Albums`/`Artists`/`InsideAlbum`, which are
    /// structurally identical modulo the key a track groups under and which
    /// half of "group order" vs. "order within a group" gets shuffled:
    /// - `Albums`/`Artists`: keep each group's original track order, but
    ///   shuffle which group comes next (`shuffle_groups`).
    /// - `InsideAlbum`: keep every group in its original playlist position,
    ///   but shuffle the track order within each group
    ///   (`shuffle_within_groups`).
    ///
    /// Returns the current item's own group's remaining tracks first,
    /// followed by every other group — matching every mode's "don't jump
    /// away from what's already playing" behavior. Does NOT include the
    /// current item's own index; callers seed their own order with it, since
    /// `current_real_idx` is excluded from grouping here. Note: unlike the
    /// original per-mode code, this always tracks the current group's key in
    /// `other_keys` too — harmless, since `groups.remove` on an
    /// already-removed key is a no-op when that key is walked a second time
    /// below.
    fn shuffle_grouped(
        &self,
        len: usize,
        current_real_idx: Option<usize>,
        rng: &mut impl rand::Rng,
        key_fn: impl Fn(&PlaylistItem) -> String,
        shuffle_groups: bool,
        shuffle_within_groups: bool,
    ) -> Vec<usize> {
        let current_key = current_real_idx.map(|idx| key_fn(&self.playlist_items[idx]));
        let mut groups: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        let mut other_keys = Vec::new();

        for i in 0..len {
            if current_real_idx == Some(i) {
                continue;
            }
            let key = key_fn(&self.playlist_items[i]);
            groups
                .entry(key.clone())
                .or_insert_with(|| {
                    other_keys.push(key.clone());
                    Vec::new()
                })
                .push(i);
        }

        if shuffle_within_groups {
            for group_tracks in groups.values_mut() {
                group_tracks.shuffle(rng);
            }
        }
        if shuffle_groups {
            other_keys.shuffle(rng);
        }

        // Note: the current item itself (excluded from `groups` above) is
        // NOT included here — callers already seed their own order with it.
        let mut order = Vec::with_capacity(len);

        if let Some(ref key) = current_key {
            if let Some(mut current_group) = groups.remove(key) {
                order.append(&mut current_group);
            }
        }
        for key in &other_keys {
            if let Some(mut group) = groups.remove(key) {
                order.append(&mut group);
            }
        }

        order
    }

    /// Replaces `shuffle_order` for the current `shuffle_mode`, keeping the
    /// virtual indices that point into it (`current_index`, `played_indices`)
    /// on the same items: each is resolved to its real index through the
    /// outgoing order, then re-found in the new one — including when shuffle
    /// is turned *off*, where the new order is the identity (#1221, #1222).
    /// Proven for every permutation in `verification/lean/Luminous/Player.lean`
    /// (`rebuildFixed_ok`).
    fn rebuild_shuffle_order(&mut self) {
        let len = self.playlist_items.len();
        if len == 0 {
            self.shuffle_order = Vec::new();
            return;
        }

        // An empty outgoing order means callers seeded `current_index` with
        // a real index (`play_playlist`, `Player::new`).
        let old_order = std::mem::take(&mut self.shuffle_order);
        let to_real = |pos: usize| -> Option<usize> {
            let idx = if old_order.is_empty() {
                pos
            } else {
                old_order.get(pos).copied().unwrap_or(pos)
            };
            (idx < len).then_some(idx)
        };
        let current_real_idx = self.current_index.and_then(to_real);
        let played_real: Vec<usize> = self
            .played_indices
            .iter()
            .filter_map(|&v| to_real(v))
            .collect();

        let order = self.build_play_order(len, current_real_idx);

        let mut virtual_of_real = vec![None; len];
        for (virtual_idx, &real_idx) in order.iter().enumerate() {
            virtual_of_real[real_idx] = Some(virtual_idx);
        }
        self.current_index = current_real_idx.and_then(|r| virtual_of_real[r]);
        self.played_indices = played_real
            .into_iter()
            .filter_map(|r| virtual_of_real[r])
            .collect();
        self.shuffle_order = order;
    }

    /// A fresh play order (a permutation of `0..len`) for the current
    /// `shuffle_mode`: the identity when shuffle is off, otherwise a shuffle
    /// that starts with `current_real_idx` so the playing track stays put.
    fn build_play_order(&self, len: usize, current_real_idx: Option<usize>) -> Vec<usize> {
        let mut rng = rand::rng();

        let get_artist_key = |item: &PlaylistItem| -> String {
            if let Some(ref song) = item.song {
                if let Some(ref artist) = song.artist {
                    if !artist.trim().is_empty() {
                        return artist.to_lowercase();
                    }
                }
            }
            item.uuid.clone()
        };

        let mut order = if let Some(idx) = current_real_idx {
            vec![idx]
        } else {
            Vec::new()
        };

        match self.shuffle_mode {
            ShuffleMode::Off => return (0..len).collect(),
            ShuffleMode::All => {
                let mut remaining_indices: Vec<usize> =
                    (0..len).filter(|&i| current_real_idx != Some(i)).collect();
                remaining_indices.shuffle(&mut rng);
                order.extend(remaining_indices);
            }
            ShuffleMode::InsideAlbum => {
                order.extend(self.shuffle_grouped(
                    len,
                    current_real_idx,
                    &mut rng,
                    Self::album_key,
                    false, // keep album order as-is
                    true,  // shuffle each album's own track order
                ));
            }
            ShuffleMode::Albums => {
                order.extend(self.shuffle_grouped(
                    len,
                    current_real_idx,
                    &mut rng,
                    Self::album_key,
                    true,  // shuffle which album comes next
                    false, // keep each album's own track order
                ));
            }
            ShuffleMode::Artists => {
                order.extend(self.shuffle_grouped(
                    len,
                    current_real_idx,
                    &mut rng,
                    get_artist_key,
                    true,  // shuffle which artist comes next
                    false, // keep each artist's own track order
                ));
            }
        }

        order
    }

    pub fn set_shuffle_mode(&mut self, mode: ShuffleMode) {
        self.shuffle_mode = mode;
        self.rebuild_shuffle_order();
        if let Ok(conn) = self._db.pool.get() {
            app_state_set(&conn, "shuffle_mode", shuffle_mode_to_key(mode));
        }
    }

    /// The current playlist's items in actual playback order: `shuffle_order`
    /// holds identity order `[0, 1, 2, ...]` when shuffle is off, so this
    /// works the same way regardless of shuffle mode. Includes every item —
    /// already-played and upcoming — since `current_index` only marks a
    /// position within this order, not a cutoff; a Queue row must stay
    /// visible after it plays (#888/#902).
    pub fn get_playlist_tracks_in_playback_order(&self) -> Vec<PlaylistItem> {
        self.shuffle_order
            .iter()
            .filter_map(|&real_idx| self.playlist_items.get(real_idx).cloned())
            .collect()
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.repeat_mode = mode;
        if let Ok(conn) = self._db.pool.get() {
            app_state_set(&conn, "repeat_mode", repeat_mode_to_key(mode));
        }
    }

    /// Re-fetch every queued item's `Song` (and the current song) from the DB
    /// by id. `playlist_items`/`current_song` are snapshots taken when the
    /// queue was built, so a library rescan that repoints a moved file's path
    /// (or hard-deletes a genuinely missing one) fixes the database but not
    /// an already-loaded queue — this brings it back in sync without
    /// requiring the queue to be rebuilt or the app restarted. A song id that
    /// no longer exists (hard-deleted) is left untouched; `is_item_playable`
    /// already treats a stale/unavailable row as unplayable and skips it.
    pub fn resync_queue_with_db(&mut self) -> Result<()> {
        let conn = self._db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );

        for item in &mut self.playlist_items {
            let Some(id) = item.song.as_ref().map(|s| s.id) else {
                continue;
            };
            if let Ok(fresh) =
                conn.query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
            {
                item.song = Some(fresh);
            }
        }

        if let Some(id) = self.current_song.as_ref().map(|s| s.id) {
            if let Ok(fresh) =
                conn.query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
            {
                self.current_song = Some(fresh);
            }
        }

        Ok(())
    }

    /// Get the current playback state snapshot for the frontend.
    pub async fn get_state(&self) -> PlaybackState {
        let audio = self.audio.lock().await;
        PlaybackState {
            state: audio.current_state(),
            current_song: self.current_song.clone(),
            playlist_id: self.current_playlist_id,
            playlist_item_uuid: self.current_item_uuid.clone(),
            position_nanosec: audio
                .current_position_nanosec()
                .saturating_sub(self.current_song_beginning_nanosec())
                as i64,
            volume: audio.current_volume(),
            shuffle_mode: self.shuffle_mode,
            repeat_mode: self.repeat_mode,
            stop_after_current: self.stop_after_current,
            loudness_source: self.current_loudness_source,
            loudness_gain_db: self.current_loudness_gain_db,
            remaining_playlist_items: self.remaining_playlist_items(),
        }
    }

    /// Snapshots the current audio pipeline configuration for the active track (#1041).
    pub fn get_pipeline_info(
        &self,
        audio: &AudioEngine,
    ) -> Option<crate::models::AudioPipelineInfo> {
        audio.get_pipeline_info(
            self.current_song.as_ref(),
            self.current_loudness_source,
            self.current_loudness_gain_db,
        )
    }

    /// Update position and check scrobble point. When the scrobble point is
    /// crossed, the listen is recorded (playcount/lastplayed) and the
    /// `song-stats-changed` payload is returned for the caller to emit.
    pub fn on_position_update(&mut self, position_nanosec: u64) -> Option<serde_json::Value> {
        let scrobble_at = self.scrobble_point_nanosec?;
        if self.scrobbled || position_nanosec < scrobble_at {
            return None;
        }
        self.scrobbled = true;
        log::debug!("Scrobble point reached at {}ns", position_nanosec);

        if let (Some(scrobbler), Some(song)) = (&self.scrobbler, &self.current_song) {
            let scrobbler = Arc::clone(scrobbler);
            let song = song.clone();
            let listened_at = chrono::Utc::now().timestamp();
            tokio::spawn(async move {
                scrobbler.on_scrobble_point(&song, listened_at).await;
            });
        }

        let song_id = self.current_song.as_ref()?.id;
        let duration_secs = self.current_song.as_ref()?.duration_secs().round() as i64;
        match self._db.pool.get() {
            Ok(conn) => match stats::record_play(&conn, song_id) {
                Ok(()) => {
                    let context = self
                        .current_play_context
                        .clone()
                        .unwrap_or(PlayContext::Song);
                    if let Err(e) =
                        stats::record_play_context(&conn, &context, song_id, duration_secs)
                    {
                        log::warn!("Failed to record play context for song {song_id}: {e}");
                    }
                    Some(stats::stats_payload(&conn, song_id))
                }
                Err(e) => {
                    log::warn!("Failed to record play for song {song_id}: {e}");
                    None
                }
            },
            Err(e) => {
                log::warn!("Failed to get db connection for play stats: {e}");
                None
            }
        }
    }

    /// Record a skip for the current track if it has not reached its scrobble
    /// point. Call before a user-initiated track change (never on natural
    /// completion). Returns the `song-stats-changed` payload for emission.
    pub fn note_manual_skip(&mut self) -> Option<serde_json::Value> {
        if self.scrobbled {
            return None;
        }
        let song_id = self.current_song.as_ref()?.id;
        match self._db.pool.get() {
            Ok(conn) => match stats::record_skip(&conn, song_id) {
                Ok(()) => Some(stats::stats_payload(&conn, song_id)),
                Err(e) => {
                    log::warn!("Failed to record skip for song {song_id}: {e}");
                    None
                }
            },
            Err(e) => {
                log::warn!("Failed to get db connection for skip stats: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use std::sync::Arc;

    fn setup_test_db() -> (Database, std::path::PathBuf) {
        let temp_dir =
            std::env::temp_dir().join(format!("luminous_player_test_{}", uuid::Uuid::new_v4()));
        let db = Database::new(temp_dir.clone()).unwrap();
        (db, temp_dir)
    }

    #[tokio::test]
    async fn note_playback_error_never_flags_webdav_songs_unavailable() {
        let (db, _temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (id, source, path, title) VALUES (99, 11, 'http://127.0.0.1:8080/song.mp3', 'Remote Song')",
                [],
            )
            .unwrap();
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());
        player.current_song = Some(crate::models::Song {
            id: 99,
            source: crate::models::SongSource::WebDav,
            path: Some("http://127.0.0.1:8080/song.mp3".to_string()),
            ..Default::default()
        });

        let outcome = player.note_playback_error();
        assert!(!outcome.flagged_unavailable);

        let conn = db_arc.pool.get().unwrap();
        let unavailable: i64 = conn
            .query_row("SELECT unavailable FROM songs WHERE id = 99", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(unavailable, 0);
    }

    #[tokio::test]
    async fn test_player_state_persistence_and_restoration() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES (42, '/fake/path.mp3', 'Test Title', 'Test Artist', 'Test Album', 180000000000)",
                [],
            ).unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_song_id', '42')",
                [],
            )
            .unwrap();
            conn.execute("INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_position_nanosec', '45000000000')", []).unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_playlist_id', '0')",
                [],
            )
            .unwrap();
            conn.execute("INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_item_uuid', 'test-uuid-123')", []).unwrap();
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        assert!(player.current_song.is_some());
        let restored = player.current_song.as_ref().unwrap();
        assert_eq!(restored.id, 42);
        assert_eq!(restored.title.as_deref(), Some("Test Title"));
        assert_eq!(player.current_item_uuid.as_deref(), Some("test-uuid-123"));

        let state = player.get_state().await;
        assert_eq!(state.state, crate::models::PlayState::Paused);
        assert_eq!(state.position_nanosec, 45_000_000_000);

        player.current_song = None;
        player.persist_current_song().await;
        player.persist_position(0).await;

        let conn = db_arc.pool.get().unwrap();
        let song_id_exists: Result<String, _> = conn.query_row(
            "SELECT value FROM app_state WHERE key = 'last_song_id'",
            [],
            |r| r.get(0),
        );
        assert!(song_id_exists.is_err());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn test_adhoc_queue_survives_restart() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=3i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let items = (1..=3i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        // Simulate an ad-hoc selection (album/artist/search — playlist_id 0)
        // starting on the middle track, then "quitting" mid-playback.
        player.play_playlist(items, 1, 0, None).await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 2);

        // Reopening the app re-runs Player::new against the same DB.
        let restarted = Player::new(db_arc.clone(), audio.clone());
        assert_eq!(restarted.current_song.as_ref().unwrap().id, 2);
        assert_eq!(restarted.playlist_items.len(), 3);
        assert_eq!(restarted.current_index, Some(1));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression test for the "album skips first track, file not found"
    /// bug: the audio engine can report more than one `AudioEvent::Error`
    /// for a single failed open (e.g. a benign device-level error alongside
    /// the real decode error). The first `Error` triggers a successful
    /// case-heal-and-retry; a second, stale `Error` for that same already-
    /// resolved attempt must not be misread as a fresh, unrecoverable
    /// failure — that misread is what skipped the track and showed a
    /// misleading "file not found" toast even though the track was already
    /// healed and playing.
    #[tokio::test]
    async fn test_duplicate_error_after_successful_heal_is_not_treated_as_failure() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);
        let album_dir = temp_dir.join("Album");
        std::fs::create_dir_all(&album_dir).unwrap();
        std::fs::write(album_dir.join("Track.mp3"), b"fake audio bytes").unwrap();

        // DB row stores a stale-cased path that only resolves case-insensitively.
        let stale_path = album_dir.join("track.mp3");
        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                &format!(
                    "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES (1, '{}', 'Track', 'Artist', 'Album', 3000000000)",
                    stale_path.to_string_lossy()
                ),
                [],
            )
            .unwrap();
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());
        let song = {
            let conn = db_arc.pool.get().unwrap();
            let sql = format!(
                "SELECT {} FROM songs WHERE id = ?1",
                crate::collection::SONG_SELECT_COLS
            );
            conn.query_row(
                &sql,
                rusqlite::params![1i64],
                crate::collection::row_to_song,
            )
            .unwrap()
        };
        player
            .play_playlist(vec![PlaylistItem::new_song(0, 0, song)], 0, 0, None)
            .await
            .unwrap();

        let healed = player.try_heal_and_retry_current_track().await;
        assert!(
            healed,
            "a case-only path mismatch should heal on the first error"
        );
        let healed_path = player.current_song.as_ref().unwrap().path.clone().unwrap();
        assert!(
            std::path::Path::new(&healed_path).exists(),
            "heal should repoint to the real on-disk path"
        );

        let stale_duplicate = player.try_heal_and_retry_current_track().await;
        assert!(
            stale_duplicate,
            "a stale duplicate Error for an already-healed track must be treated as handled, \
             not fall through to the skip-and-toast failure path"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn test_previous_track_walks_back_through_playlist() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=3i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let items = (1..=3i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        // Start on the last track (index 2, song id 3).
        player.play_playlist(items, 2, 0, None).await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 3);
        assert_eq!(player.current_index, Some(2));

        player.previous_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            2,
            "previous should move to the prior track, not replay the current one"
        );
        assert_eq!(player.current_index, Some(1));

        player.previous_track().await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 1);
        assert_eq!(player.current_index, Some(0));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Pressing Previous at the very first track must not wrap around to the
    /// last track — it should stay put, mirroring `get_next_index`'s
    /// boundary behavior for Next. Wrapping unconditionally here silently
    /// jumped playback to the end of the queue, which then triggered the
    /// frontend's natural-completion handling (clearing the Queue) far
    /// earlier than the user expected.
    #[tokio::test]
    async fn test_previous_track_does_not_wrap_at_start_without_repeat() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=3i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let items = (1..=3i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        // Start on the first track (index 0).
        player.play_playlist(items, 0, 0, None).await.unwrap();
        assert_eq!(player.current_index, Some(0));

        player.previous_track().await.unwrap();
        assert_eq!(
            player.current_index,
            Some(0),
            "previous at the start of the queue must not wrap to the last track"
        );
        assert_eq!(player.current_song.as_ref().unwrap().id, 1);

        // With RepeatMode::Playlist, wrapping to the last track is intended.
        player.set_repeat_mode(RepeatMode::Playlist);
        player.previous_track().await.unwrap();
        assert_eq!(player.current_index, Some(2));
        assert_eq!(player.current_song.as_ref().unwrap().id, 3);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Same scenario as `test_previous_track_walks_back_through_playlist`,
    /// but exercises the real saved-playlist path (`PlaylistManager` +
    /// `get_playlist_tracks`) instead of the ad-hoc `PlaylistItem::new_song`
    /// helper, to check for divergence between Album and Playlist playback
    /// reported in #105 ("Previous song works on albums, but not playlists").
    #[tokio::test]
    async fn test_previous_track_walks_back_through_saved_playlist() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=3i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let mut manager = crate::playlist::PlaylistManager::new(db_arc.clone()).unwrap();
        let playlist = manager.create_playlist("Test Playlist").unwrap();
        manager
            .add_songs_to_playlist(playlist.id, &[1, 2, 3])
            .unwrap();

        let items = manager.get_playlist_tracks(playlist.id).unwrap();
        assert_eq!(items.len(), 3);

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        // Start on the last track (index 2).
        player
            .play_playlist(items, 2, playlist.id, None)
            .await
            .unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 3);
        assert_eq!(player.current_index, Some(2));

        player.previous_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            2,
            "previous should move to the prior playlist track, not replay the current one"
        );
        assert_eq!(player.current_index, Some(1));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Root-caused #105's "Previous restarts the current song instead of
    /// moving to the prior one" report: it only reproduces with Shuffle on.
    /// `play_at_index` pushes every played (virtual) index onto
    /// `played_indices`, including the current track's own entry, so the
    /// naive top-of-stack pop in `previous_track` just replayed it.
    #[tokio::test]
    async fn test_previous_track_in_shuffle_mode_does_not_replay_current() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=4i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let items = (1..=4i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        player.set_shuffle_mode(ShuffleMode::All);
        player.set_repeat_mode(RepeatMode::Playlist);
        player.play_playlist(items, 0, 0, None).await.unwrap();
        let first_song_id = player.current_song.as_ref().unwrap().id;

        // Advance forward twice so there's real history to walk back through.
        player.next_track().await.unwrap();
        let second_song_id = player.current_song.as_ref().unwrap().id;
        player.next_track().await.unwrap();
        let third_song_id = player.current_song.as_ref().unwrap().id;
        assert_ne!(second_song_id, first_song_id);
        assert_ne!(third_song_id, second_song_id);

        player.previous_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            second_song_id,
            "previous should move to the prior shuffled track, not replay the current one"
        );

        player.previous_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            first_song_id,
            "a second previous press should keep walking back, not stay put"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn test_play_playlist_with_shuffle_mode_plays_correct_requested_track() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=10i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let build_items = |ids: Vec<i64>| -> Vec<PlaylistItem> {
            ids.into_iter()
                .map(|id| {
                    let conn = db_arc.pool.get().unwrap();
                    let sql = format!(
                        "SELECT {} FROM songs WHERE id = ?1",
                        crate::collection::SONG_SELECT_COLS
                    );
                    let song = conn
                        .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                        .unwrap();
                    PlaylistItem::new_song(0, 0, song)
                })
                .collect()
        };

        player.set_shuffle_mode(ShuffleMode::All);
        player
            .play_playlist(build_items((1..=10).collect()), 5, 0, None)
            .await
            .unwrap();

        // Now load a new smaller playlist with shuffle mode active and request track 4 (index 3).
        let new_items = build_items(vec![1, 2, 3, 4, 5]);
        player.play_playlist(new_items, 3, 0, None).await.unwrap();

        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            4,
            "play_playlist should play requested start_index track even when shuffle mode is active"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression coverage for the get_next_index/find_playable_from/
    /// resolve_item_index consolidation (#577 item 13): non-shuffle forward
    /// advance must still stop after the last track with repeat off, and
    /// still wrap back to the first track with RepeatMode::Playlist.
    #[tokio::test]
    async fn test_next_track_stops_at_end_then_wraps_with_repeat_playlist() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=3i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let items = (1..=3i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        player.set_repeat_mode(RepeatMode::Off);
        player
            .play_playlist(items.clone(), 0, 0, None)
            .await
            .unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 1);

        player.next_track().await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 2);

        player.next_track().await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 3);

        player.next_track().await.unwrap();
        assert!(
            player.current_song.is_none(),
            "advancing past the last track with repeat off must stop, not wrap"
        );

        player.set_repeat_mode(RepeatMode::Playlist);
        player.play_playlist(items, 2, 0, None).await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 3);

        player.next_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            1,
            "advancing past the last track with RepeatMode::Playlist must wrap to the first"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression coverage for #1070: `RepeatMode::Album` used to fall
    /// through the `_ => {}`/`_ => None` catch-all in every repeat-mode
    /// match, behaving identically to `Off` despite being reachable and
    /// advertised in the UI ("Loop the current album indefinitely"). Songs
    /// 1/3 are "Album A" and 2/4 are "Album B", interleaved in playlist
    /// order — this proves *natural* advance (on_track_finished) stays
    /// scoped to the current album's own tracks (not just "the next track"
    /// or "the whole playlist"), and wraps back to the album's first track
    /// rather than stopping or spilling into the other album. Manual skip
    /// is intentionally NOT scoped this way — see the follow-up regression
    /// test below (a real bug this exact confusion caused).
    #[tokio::test]
    async fn test_repeat_album_stays_within_album_and_wraps() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=4i64 {
                let album = if id % 2 == 1 { "Album A" } else { "Album B" };
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', '{album}', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let items = (1..=4i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        player.set_repeat_mode(RepeatMode::Album);
        player
            .play_playlist(items.clone(), 0, 0, None)
            .await
            .unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 1);

        // Natural track-end must stay within Album A, skipping over song 2
        // (Album B) to reach song 3.
        player.on_track_finished().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            3,
            "RepeatMode::Album must advance to the next track within the same album, \
             skipping over tracks that belong to a different album"
        );

        // Past Album A's last track, must wrap back to its first rather
        // than stopping or continuing into Album B.
        player.on_track_finished().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            1,
            "RepeatMode::Album must wrap back to the album's first track, not stop or \
             spill into the next album"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression coverage for a real bug caught in manual testing: an
    /// earlier version of the #1070 fix scoped `get_next_index` itself to
    /// the current album, which meant manual skip (`next_track`) inherited
    /// that scoping too. For a track with no/blank album tag (its own
    /// group of one via `album_key`'s uuid fallback) — or a genuine
    /// single-track album — that made pressing Next appear to do nothing:
    /// it kept "advancing" to the same track. Manual skip must always
    /// advance linearly through the whole playlist regardless of repeat
    /// mode, exactly like `RepeatMode::Track` already does (repeat only
    /// governs natural track-end, never user-initiated navigation).
    #[tokio::test]
    async fn test_repeat_album_does_not_confine_manual_skip() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=3i64 {
                // No album tag at all — each track is its own group of one
                // under `album_key`'s uuid fallback, the exact case that
                // triggered the bug.
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let items = (1..=3i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        player.set_repeat_mode(RepeatMode::Album);
        player.play_playlist(items, 0, 0, None).await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 1);

        player.next_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            2,
            "manual skip must always advance to the next track, even when the current \
             track's \"album\" is just itself under RepeatMode::Album"
        );

        player.next_track().await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 3);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression coverage for #1073: `next_track`'s ad-hoc queue branch
    /// used to update `current_song`/`current_item_uuid` in memory but skip
    /// `persist_current_song`/`persist_position` entirely — so quitting the
    /// app right after skipping into a queued "play next" track would
    /// restore the *previous* song on restart instead of the one that was
    /// actually audible when the app closed. Now routed through the shared
    /// `start_track` helper every "start a track" path uses, so it can no
    /// longer skip a step.
    #[tokio::test]
    async fn test_skipping_into_queued_track_persists_for_restart() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=4i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );

        let items = (1..=3i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        player.play_playlist(items, 0, 0, None).await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 1);

        // Queue song 4 to play next.
        let queued_song = {
            let conn = db_arc.pool.get().unwrap();
            conn.query_row(
                &sql,
                rusqlite::params![4i64],
                crate::collection::row_to_song,
            )
            .unwrap()
        };
        let expected_start_ns = queued_song.beginning_nanosec.max(0) as u64;
        let queued_item = PlaylistItem::new_song(0, 0, queued_song);
        let queued_uuid = queued_item.uuid.clone();
        player.queue.push_back(queued_item);

        player.next_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            4,
            "manual skip must drain the play-next queue before continuing the playlist"
        );

        // The concrete regression: restart persistence must reflect the
        // queued track that's actually playing now, not the previous one.
        let conn = db_arc.pool.get().unwrap();
        let persisted_song_id: String = conn
            .query_row(
                "SELECT value FROM app_state WHERE key = 'last_song_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted_song_id, "4");

        let persisted_uuid: String = conn
            .query_row(
                "SELECT value FROM app_state WHERE key = 'last_item_uuid'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted_uuid, queued_uuid);

        let persisted_position: String = conn
            .query_row(
                "SELECT value FROM app_state WHERE key = 'last_position_nanosec'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted_position, expected_start_ns.to_string());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Coverage for the #1073 design decision: under `RepeatMode::Playlist`,
    /// a track finishing naturally must drain the ad-hoc "play next" queue
    /// first, the same as manual skip — previously `on_track_finished`'s
    /// `Playlist` arm called `play_at_index` directly and never looked at
    /// `self.queue` at all, so a queued track would be silently skipped
    /// while repeat-playlist was on.
    #[tokio::test]
    async fn test_repeat_playlist_drains_queue_before_wrapping() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=4i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );

        let items = (1..=3i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        player.set_repeat_mode(RepeatMode::Playlist);
        // Start on the last track, so "finishing naturally" would otherwise
        // wrap straight back to track 1.
        player.play_playlist(items, 2, 0, None).await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 3);

        let queued_song = {
            let conn = db_arc.pool.get().unwrap();
            conn.query_row(
                &sql,
                rusqlite::params![4i64],
                crate::collection::row_to_song,
            )
            .unwrap()
        };
        player
            .queue
            .push_back(PlaylistItem::new_song(0, 0, queued_song));

        player.on_track_finished().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            4,
            "RepeatMode::Playlist must drain a queued \"play next\" track before wrapping \
             back to the start of the playlist"
        );

        // The queue is now empty, so the *next* natural finish wraps as usual.
        player.on_track_finished().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            1,
            "once the queue is drained, RepeatMode::Playlist still wraps to the first track"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Coverage for #1077: none of the file's tests previously touched
    /// gapless handover at all. Exercises the happy path —
    /// `prepare_gapless_next` preloading, then `on_gapless_transition`
    /// committing it — confirming index/scrobble/persistence bookkeeping
    /// lands correctly without a real `Play` call (the audio never stops
    /// for a gapless commit).
    #[tokio::test]
    async fn test_gapless_transition_commits_index_and_scrobble_bookkeeping() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=3i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );
        let items = (1..=3i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        player.set_repeat_mode(RepeatMode::Off);
        player.play_playlist(items, 0, 0, None).await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 1);

        player.prepare_gapless_next().await.unwrap();

        // Song 2 is what peek_next_natural should have preloaded.
        player.on_gapless_transition(2).await.unwrap();

        assert_eq!(player.current_song.as_ref().unwrap().id, 2);
        assert_eq!(player.current_index, Some(1));
        assert!(
            player.played_indices.contains(&1),
            "on_gapless_transition must record the new index in played_indices, matching \
             play_at_index's bookkeeping"
        );
        assert!(
            !player.scrobbled,
            "a freshly-started track must not already be scrobbled"
        );
        assert_eq!(
            player.scrobble_point_nanosec,
            Some(90_000_000_000),
            "scrobble point must be recomputed for the new song (50% of its 180s length)"
        );

        let conn = db_arc.pool.get().unwrap();
        let persisted_song_id: String = conn
            .query_row(
                "SELECT value FROM app_state WHERE key = 'last_song_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted_song_id, "2");
        let persisted_position: String = conn
            .query_row(
                "SELECT value FROM app_state WHERE key = 'last_position_nanosec'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            persisted_position, "0",
            "a gapless commit persists position 0 — the new track just started"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Coverage for #1077: `on_gapless_transition`'s doc comment promises a
    /// self-healing fallback to `on_track_finished` when the preloaded
    /// track no longer matches what should play (e.g. mode/queue changed
    /// after the preload was armed) — previously untested.
    #[tokio::test]
    async fn test_gapless_transition_mismatch_falls_back_to_on_track_finished() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=3i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );
        let items = (1..=3i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        player.set_repeat_mode(RepeatMode::Off);
        player.play_playlist(items, 0, 0, None).await.unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 1);

        // A song id that doesn't match what peek_next_natural would return
        // (song 2) — the preload no longer matches playback context.
        player.on_gapless_transition(999).await.unwrap();

        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            2,
            "a song-id mismatch must self-heal via on_track_finished's normal advance, not \
             leave playback stuck or desynced"
        );
        assert_eq!(player.current_index, Some(1));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Coverage for #1077: scrobble-point computation and the `scrobbled`
    /// flag were previously untested. `on_position_update` must record the
    /// listen exactly once when the position first crosses the 50%-length
    /// scrobble point, and do nothing before or after that first crossing.
    #[tokio::test]
    async fn test_scrobble_point_reached_records_once() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES (1, '/fake/path1.mp3', 'Track 1', 'Artist', 'Album', 180000000000)",
                [],
            )
            .unwrap();
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );
        let song = {
            let conn = db_arc.pool.get().unwrap();
            conn.query_row(
                &sql,
                rusqlite::params![1i64],
                crate::collection::row_to_song,
            )
            .unwrap()
        };
        player
            .play_playlist(vec![PlaylistItem::new_song(0, 0, song)], 0, 0, None)
            .await
            .unwrap();

        assert_eq!(player.scrobble_point_nanosec, Some(90_000_000_000));
        assert!(!player.scrobbled);

        // Below the scrobble point: no-op, not yet scrobbled.
        assert!(player.on_position_update(50_000_000_000).is_none());
        assert!(!player.scrobbled);

        // At the scrobble point: records once.
        assert!(player.on_position_update(90_000_000_000).is_some());
        assert!(player.scrobbled);

        // Already scrobbled: must not double-record even well past the point.
        assert!(player.on_position_update(120_000_000_000).is_none());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression coverage for the shuffle_grouped extraction (#577 item
    /// 14): `Albums` must keep each album's own track order intact and only
    /// reorder which album comes next, while `InsideAlbum` must do the
    /// opposite (keep album order, reorder tracks within each album).
    /// Songs 1/3/5 are "Album A" and 2/4/6 are "Album B", interleaved in
    /// playlist order — with only two groups, an "other" group-order
    /// shuffle is a no-op (nothing to permute among one element), which
    /// makes `Albums`'s exact resulting order deterministic and assertable.
    #[tokio::test]
    async fn test_shuffle_grouped_albums_keeps_track_order_inside_each_group() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=6i64 {
                let album = if id % 2 == 1 { "Album A" } else { "Album B" };
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', '{album}', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let items = (1..=6i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        // current_index = 0 -> song 1 (Album A) is "now playing".
        player.set_shuffle_mode(ShuffleMode::Albums);
        player
            .play_playlist(items.clone(), 0, 0, None)
            .await
            .unwrap();

        let order_ids: Vec<i64> = player
            .shuffle_order
            .iter()
            .map(|&idx| player.playlist_items[idx].song.as_ref().unwrap().id)
            .collect();
        assert_eq!(
            order_ids,
            vec![1, 3, 5, 2, 4, 6],
            "Albums mode must keep each album's own track order (1,3,5 then 2,4,6), only \
             reordering which album comes next — with just 2 albums here that reorder is a \
             no-op, so the exact order is deterministic"
        );

        // InsideAlbum: album group order must stay as encountered in the
        // playlist (A's group before B's group), but track order *within*
        // each album is free to shuffle — assert group membership/position,
        // not exact per-track order.
        player.set_shuffle_mode(ShuffleMode::InsideAlbum);
        player.play_playlist(items, 0, 0, None).await.unwrap();

        let order_ids: Vec<i64> = player
            .shuffle_order
            .iter()
            .map(|&idx| player.playlist_items[idx].song.as_ref().unwrap().id)
            .collect();
        assert_eq!(order_ids[0], 1, "current track must stay first");
        let mut album_a_tail: Vec<i64> = order_ids[1..3].to_vec();
        album_a_tail.sort();
        assert_eq!(
            album_a_tail,
            vec![3, 5],
            "InsideAlbum must keep Album A's remaining tracks immediately after the current \
             track, before any Album B track"
        );
        let mut album_b: Vec<i64> = order_ids[3..6].to_vec();
        album_b.sort();
        assert_eq!(
            album_b,
            vec![2, 4, 6],
            "InsideAlbum must keep Album B as a contiguous group after Album A"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn test_remove_songs_preserves_shuffle_order() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=5i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let items = (1..=5i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        player.set_shuffle_mode(ShuffleMode::All);
        player
            .play_playlist(items.clone(), 0, 0, None)
            .await
            .unwrap();

        // Fix shuffle order manually to test deterministic preservation
        // Order: [Song 1 (idx 0), Song 4 (idx 3), Song 2 (idx 1), Song 5 (idx 4), Song 3 (idx 2)]
        player.shuffle_order = vec![0, 3, 1, 4, 2];
        player.current_index = Some(0);

        // Remove Song 2 (uuid of items[1])
        let removed_uuid = items[1].uuid.clone();
        player.remove_songs_from_playlist_items(&[removed_uuid]);

        // After removing Song 2 (items[1]):
        // Remaining items: Song 1 (idx 0), Song 3 (idx 1), Song 4 (idx 2), Song 5 (idx 3)
        // Shuffle order should be: Song 1 (0), Song 4 (2), Song 5 (3), Song 3 (1) -> vec![0, 2, 3, 1]
        assert_eq!(player.shuffle_order, vec![0, 2, 3, 1]);
        assert_eq!(player.current_index, Some(0));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn test_get_playlist_tracks_in_playback_order_includes_already_played_items() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for id in 1..=3i64 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES ({id}, '/fake/path{id}.mp3', 'Track {id}', 'Artist', 'Album', 180000000000)"
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        let items = (1..=3i64)
            .map(|id| {
                let conn = db_arc.pool.get().unwrap();
                let sql = format!(
                    "SELECT {} FROM songs WHERE id = ?1",
                    crate::collection::SONG_SELECT_COLS
                );
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect::<Vec<_>>();

        player
            .play_playlist(items.clone(), 0, 0, None)
            .await
            .unwrap();

        // Simulate having advanced past the first two tracks (#902): they must
        // still be returned, not sliced away just because current_index moved.
        player.current_index = Some(2);

        let result = player.get_playlist_tracks_in_playback_order();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].uuid, items[0].uuid);
        assert_eq!(result[1].uuid, items[1].uuid);
        assert_eq!(result[2].uuid, items[2].uuid);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// A `Player` over songs `1..=count` (all on one album/artist), with the
    /// given ids flagged `unavailable`, plus the matching playlist items.
    fn player_with_songs(
        count: i64,
        unavailable: &[i64],
    ) -> (Player, Vec<PlaylistItem>, std::path::PathBuf) {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);
        let conn = db_arc.pool.get().unwrap();
        for id in 1..=count {
            conn.execute(
                "INSERT INTO songs (id, path, title, artist, album, length_nanosec, unavailable) VALUES (?1, ?2, ?3, 'Artist', 'Album', 180000000000, ?4)",
                rusqlite::params![
                    id,
                    format!("/fake/path{id}.mp3"),
                    format!("Track {id}"),
                    unavailable.contains(&id)
                ],
            )
            .unwrap();
        }
        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );
        let items = (1..=count)
            .map(|id| {
                let song = conn
                    .query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                    .unwrap();
                PlaylistItem::new_song(0, 0, song)
            })
            .collect();
        drop(conn);
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        (Player::new(db_arc, audio), items, temp_dir)
    }

    /// #1221: turning shuffle off must leave `current_index` on the playing
    /// track, so Next continues from it in playlist order.
    #[tokio::test]
    async fn test_shuffle_off_keeps_next_relative_to_current_track() {
        let (mut player, items, temp_dir) = player_with_songs(6, &[]);

        for _ in 0..10 {
            player.set_shuffle_mode(ShuffleMode::All);
            player
                .play_playlist(items.clone(), 2, 0, None)
                .await
                .unwrap();
            assert_eq!(player.current_song.as_ref().unwrap().id, 3);
            // Wander through the shuffle so current_index is not 0.
            player.next_track().await.unwrap();
            player.next_track().await.unwrap();
            let playing = player.current_song.as_ref().unwrap().id;

            player.set_shuffle_mode(ShuffleMode::Off);
            player.next_track().await.unwrap();
            let expected = if playing == 6 {
                None
            } else {
                Some(playing + 1)
            };
            assert_eq!(
                player.current_song.as_ref().map(|s| s.id),
                expected,
                "Next after turning shuffle off must play the track after {playing}"
            );
        }

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// #1222: re-shuffling (switching shuffle modes) must keep Previous
    /// walking back through the songs that actually played.
    #[tokio::test]
    async fn test_reshuffle_keeps_previous_history() {
        let (mut player, items, temp_dir) = player_with_songs(8, &[]);

        for mode in [ShuffleMode::Artists, ShuffleMode::Albums, ShuffleMode::All] {
            for _ in 0..10 {
                player.set_shuffle_mode(ShuffleMode::All);
                player
                    .play_playlist(items.clone(), 0, 0, None)
                    .await
                    .unwrap();
                let first = player.current_song.as_ref().unwrap().id;
                player.next_track().await.unwrap();
                let second = player.current_song.as_ref().unwrap().id;
                player.next_track().await.unwrap();
                let third = player.current_song.as_ref().unwrap().id;

                player.set_shuffle_mode(mode);
                assert_eq!(
                    player.current_song.as_ref().unwrap().id,
                    third,
                    "re-shuffling must not change the playing track"
                );
                player.previous_track().await.unwrap();
                assert_eq!(player.current_song.as_ref().unwrap().id, second);
                player.previous_track().await.unwrap();
                assert_eq!(player.current_song.as_ref().unwrap().id, first);
            }
        }

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// #1223: under RepeatMode::Playlist, Previous wraps past the start even
    /// when the walk reaches it through unavailable tracks; without repeat
    /// it still stays put.
    #[tokio::test]
    async fn test_previous_wraps_past_unavailable_tracks_with_repeat_playlist() {
        let (mut player, items, temp_dir) = player_with_songs(3, &[1]);

        player.set_repeat_mode(RepeatMode::Playlist);
        player
            .play_playlist(items.clone(), 1, 0, None)
            .await
            .unwrap();
        assert_eq!(player.current_song.as_ref().unwrap().id, 2);
        player.previous_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            3,
            "Previous must skip the unavailable first track and wrap to the last"
        );

        player.set_repeat_mode(RepeatMode::Off);
        player.play_playlist(items, 1, 0, None).await.unwrap();
        player.previous_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            2,
            "without repeat, Previous must not wrap past the start"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
