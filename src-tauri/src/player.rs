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
    current_loudness_source: LoudnessGainSource,
    current_loudness_gain_db: Option<f32>,

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
}

impl Player {
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
            if let Ok(v_str) = conn.query_row(
                "SELECT value FROM app_state WHERE key = 'volume'",
                [],
                |row| row.get::<_, String>(0),
            ) {
                if let Ok(v) = v_str.parse::<f32>() {
                    volume = v.clamp(0.0, 1.0);
                    // Apply to audio engine
                    if let Ok(engine) = audio.try_lock() {
                        let _ = engine.set_volume(volume);
                    }
                }
            }
            if let Ok(s_str) = conn.query_row(
                "SELECT value FROM app_state WHERE key = 'shuffle_mode'",
                [],
                |row| row.get::<_, String>(0),
            ) {
                shuffle_mode = match s_str.as_str() {
                    "all" => ShuffleMode::All,
                    "inside_album" => ShuffleMode::InsideAlbum,
                    "albums" => ShuffleMode::Albums,
                    "artists" => ShuffleMode::Artists,
                    _ => ShuffleMode::Off,
                };
            }
            if let Ok(r_str) = conn.query_row(
                "SELECT value FROM app_state WHERE key = 'repeat_mode'",
                [],
                |row| row.get::<_, String>(0),
            ) {
                repeat_mode = match r_str.as_str() {
                    "track" => RepeatMode::Track,
                    "album" => RepeatMode::Album,
                    "playlist" => RepeatMode::Playlist,
                    "one_by_one" => RepeatMode::OneByOne,
                    "intro" => RepeatMode::Intro,
                    _ => RepeatMode::Off,
                };
            }

            // Restore last played song & position
            if let Ok(s_str) = conn.query_row(
                "SELECT value FROM app_state WHERE key = 'last_song_id'",
                [],
                |row| row.get::<_, String>(0),
            ) {
                if let Ok(song_id) = s_str.parse::<i64>() {
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
            }

            if let Some(ref song) = restored_song {
                if let Ok(p_str) = conn.query_row(
                    "SELECT value FROM app_state WHERE key = 'last_playlist_id'",
                    [],
                    |row| row.get::<_, String>(0),
                ) {
                    if let Ok(pid) = p_str.parse::<i64>() {
                        restored_playlist_id = Some(pid);
                    }
                }

                if let Ok(uuid) = conn.query_row(
                    "SELECT value FROM app_state WHERE key = 'last_item_uuid'",
                    [],
                    |row| row.get::<_, String>(0),
                ) {
                    restored_item_uuid = Some(uuid);
                }

                if let Ok(pos_str) = conn.query_row(
                    "SELECT value FROM app_state WHERE key = 'last_position_nanosec'",
                    [],
                    |row| row.get::<_, String>(0),
                ) {
                    if let Ok(pos) = pos_str.parse::<u64>() {
                        restored_position_ns = pos;
                    }
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
                        if let Ok(ids_json) = conn.query_row(
                            "SELECT value FROM app_state WHERE key = 'last_adhoc_song_ids'",
                            [],
                            |row| row.get::<_, String>(0),
                        ) {
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
            current_loudness_source: LoudnessGainSource::Disabled,
            current_loudness_gain_db: None,
            playlist_items,
            shuffle_order: Vec::new(),
            current_index,
            played_indices: Vec::new(),
            queue: std::collections::VecDeque::new(),
            scrobble_point_nanosec,
            scrobbled: false,
        };

        player.rebuild_shuffle_order();

        if let Some(song) = restored_song {
            if let Ok(engine) = player.audio.try_lock() {
                let _ = engine.cue(Box::new(song), restored_position_ns);
            }
        }

        player
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
        self.current_play_context =
            context.or_else(|| (playlist_id > 0).then_some(PlayContext::Playlist { playlist_id }));
        self.played_indices.clear();
        self.queue.clear();
        self.scrobbled = false;

        self.persist_adhoc_queue();

        // Build shuffle order if needed
        self.rebuild_shuffle_order();

        let play_index = if self.shuffle_mode != ShuffleMode::Off {
            // Find the position of start_index in the shuffle order
            self.shuffle_order
                .iter()
                .position(|&i| i == start_index)
                .unwrap_or(0)
        } else {
            start_index
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

    /// Play the item at the given index (in virtual/shuffle order).
    /// If the item is unavailable, auto-advances to the next playable track.
    async fn play_at_index(&mut self, index: usize) -> Result<()> {
        let total = if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order.len()
        } else {
            self.playlist_items.len()
        };

        // Walk forward from `index` to find a playable item, guarding against
        // an all-unavailable playlist (cycle limit = total items).
        let mut candidate = index;
        let mut attempts = 0;
        loop {
            if attempts >= total {
                // Every item in the playlist is unavailable — stop.
                log::warn!("Entire playlist contains only unavailable tracks — stopping.");
                return self.stop().await;
            }

            let item_index = if self.shuffle_mode != ShuffleMode::Off {
                match self.shuffle_order.get(candidate) {
                    Some(&i) => i,
                    None => return self.stop().await,
                }
            } else {
                candidate
            };

            let item = match self.playlist_items.get(item_index) {
                Some(i) => i,
                None => return self.stop().await,
            };

            if Self::is_item_playable(item) {
                break;
            }

            log::debug!("Skipping unavailable playlist item at index {candidate}");
            candidate = (candidate + 1) % total;
            attempts += 1;
        }

        let item_index = if self.shuffle_mode != ShuffleMode::Off {
            *self
                .shuffle_order
                .get(candidate)
                .ok_or(anyhow!("index out of bounds"))?
        } else {
            candidate
        };

        let item = self
            .playlist_items
            .get(item_index)
            .ok_or(anyhow!("playlist item not found"))?;

        let song = item
            .song
            .clone()
            .ok_or(anyhow!("playlist item has no song"))?;
        let start_ns = song.beginning_nanosec.max(0) as u64;

        // Set scrobble point at 50% of track length
        self.scrobble_point_nanosec = song.length_nanosec.map(|ns| (ns as u64) / 2);
        self.scrobbled = false;

        self.current_song = Some(song.clone());
        self.current_item_uuid = Some(item.uuid.clone());
        self.current_index = Some(candidate);

        if candidate > 0 && !self.played_indices.contains(&candidate) {
            self.played_indices.push(candidate);
        }

        self.persist_current_song();
        self.persist_position(start_ns);

        self.apply_loudness_gain(&song).await;
        self.preload_upcoming_waveforms();
        let audio = self.audio.lock().await;
        audio.play(Box::new(song), start_ns)
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
                    let _ = conn.execute(
                        "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_adhoc_song_ids', ?1)",
                        rusqlite::params![json],
                    );
                }
            } else {
                let _ = conn.execute(
                    "DELETE FROM app_state WHERE key = 'last_adhoc_song_ids'",
                    [],
                );
            }
        }
    }

    pub fn persist_current_song(&self) {
        if let Ok(conn) = self._db.pool.get() {
            if let Some(song) = &self.current_song {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_song_id', ?1)",
                    rusqlite::params![song.id.to_string()],
                );
            } else {
                let _ = conn.execute("DELETE FROM app_state WHERE key = 'last_song_id'", []);
            }

            if let Some(pid) = self.current_playlist_id {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_playlist_id', ?1)",
                    rusqlite::params![pid.to_string()],
                );
            } else {
                let _ = conn.execute("DELETE FROM app_state WHERE key = 'last_playlist_id'", []);
            }

            if let Some(uuid) = &self.current_item_uuid {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_item_uuid', ?1)",
                    rusqlite::params![uuid],
                );
            } else {
                let _ = conn.execute("DELETE FROM app_state WHERE key = 'last_item_uuid'", []);
            }
        }
    }

    pub fn persist_position(&self, position_nanosec: u64) {
        if let Ok(conn) = self._db.pool.get() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_position_nanosec', ?1)",
                rusqlite::params![position_nanosec.to_string()],
            );
        }
    }

    /// Recompute and apply the loudness-normalization gain (#77) for a track
    /// that is about to become audible. Called for every non-gapless track
    /// start; for gapless handovers it's applied at the actual audible
    /// boundary (`on_gapless_transition`) instead, since the DSP gain slot is
    /// global and flipping it early would affect the still-draining previous
    /// track's tail.
    async fn apply_loudness_gain(&mut self, song: &Song) {
        let settings = match crate::loudness::get_settings(&self._db) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to load loudness settings: {e}");
                return;
            }
        };
        let (gain, source, gain_db) = if settings.enabled {
            let result = crate::loudness::compute_gain(
                song.ebur128_integrated_loudness_lufs,
                song.replaygain_track_gain,
                song.replaygain_album_gain,
                &settings,
            );
            (result.linear, result.source, Some(result.gain_db))
        } else {
            (1.0, LoudnessGainSource::Disabled, None)
        };
        self.current_loudness_source = source;
        self.current_loudness_gain_db = gain_db;
        self.audio.lock().await.set_loudness_gain(gain);
    }

    /// Re-apply the loudness gain for the currently playing track — called
    /// after a loudness setting changes, so the effect is heard immediately
    /// rather than waiting for the next track change.
    pub async fn refresh_loudness_gain(&mut self) {
        if let Some(song) = self.current_song.clone() {
            self.apply_loudness_gain(&song).await;
        }
    }

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

    pub async fn pause(&self) -> Result<()> {
        let pos = self.audio.lock().await.current_position_nanosec();
        self.persist_position(pos);
        self.audio.lock().await.pause()
    }

    pub async fn resume(&self) -> Result<()> {
        self.audio.lock().await.resume()
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.current_song = None;
        self.current_item_uuid = None;
        self.current_playlist_id = None;
        self.persist_current_song();
        self.persist_position(0);
        self.audio.lock().await.stop()
    }

    pub async fn seek_to(&self, position_nanosec: u64) -> Result<()> {
        self.persist_position(position_nanosec);
        self.audio.lock().await.seek_to(position_nanosec)
    }

    pub async fn set_volume(&mut self, vol: f32) -> Result<()> {
        self.volume = vol.clamp(0.0, 1.0);
        let audio = self.audio.lock().await;
        let _ = audio.set_volume(self.volume);
        if let Ok(conn) = self._db.pool.get() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES ('volume', ?1)",
                rusqlite::params![self.volume.to_string()],
            );
        }
        Ok(())
    }

    pub async fn next_track(&mut self) -> Result<()> {
        // Drain unavailable items from the front of the queue before playing
        while let Some(front) = self.queue.front() {
            if Self::is_item_playable(front) {
                break;
            }
            log::debug!("Skipping unavailable queued item");
            self.queue.pop_front();
        }

        // Check queue first
        if let Some(queued) = self.queue.pop_front() {
            let song = queued
                .song
                .clone()
                .ok_or(anyhow!("queued item has no song"))?;
            let start_ns = song.beginning_nanosec.max(0) as u64;
            self.current_song = Some(song.clone());
            self.current_item_uuid = Some(queued.uuid.clone());
            self.scrobble_point_nanosec = song.length_nanosec.map(|ns| (ns as u64) / 2);
            self.scrobbled = false;
            self.apply_loudness_gain(&song).await;
            self.preload_upcoming_waveforms();
            let audio = self.audio.lock().await;
            return audio.play(Box::new(song), start_ns);
        }

        let next_index = self.get_next_index();
        match next_index {
            Some(idx) => self.play_at_index(idx).await,
            None => {
                // End of playlist — stop
                self.stop().await
            }
        }
    }

    pub async fn previous_track(&mut self) -> Result<()> {
        // In shuffle mode, walk back through history (skip unavailable)
        if self.shuffle_mode != ShuffleMode::Off {
            while let Some(prev_index) = self.played_indices.pop() {
                let item_index = self
                    .shuffle_order
                    .get(prev_index)
                    .copied()
                    .unwrap_or(prev_index);
                if self
                    .playlist_items
                    .get(item_index)
                    .map(Self::is_item_playable)
                    .unwrap_or(false)
                {
                    return self.play_at_index(prev_index).await;
                }
            }
        }

        // Walk backwards from current, skipping unavailable items
        if let Some(current) = self.current_index {
            let len = self.playlist_items.len();
            if len == 0 {
                return Ok(());
            }
            let mut candidate = if current > 0 {
                current - 1
            } else {
                len.saturating_sub(1)
            };
            for _ in 0..len {
                let item_index = if self.shuffle_mode != ShuffleMode::Off {
                    self.shuffle_order
                        .get(candidate)
                        .copied()
                        .unwrap_or(candidate)
                } else {
                    candidate
                };
                if self
                    .playlist_items
                    .get(item_index)
                    .map(Self::is_item_playable)
                    .unwrap_or(false)
                {
                    return self.play_at_index(candidate).await;
                }
                if candidate == 0 {
                    break;
                }
                candidate -= 1;
            }
        }
        Ok(())
    }

    /// Read-only walk from a virtual index to the first playable item,
    /// mirroring `play_at_index`'s skip-unavailable behavior.
    fn peek_playable_index(&self, index: usize) -> Option<usize> {
        let total = if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order.len()
        } else {
            self.playlist_items.len()
        };
        if total == 0 {
            return None;
        }
        let mut candidate = index;
        for _ in 0..total {
            let item_index = if self.shuffle_mode != ShuffleMode::Off {
                *self.shuffle_order.get(candidate)?
            } else {
                candidate
            };
            if self
                .playlist_items
                .get(item_index)
                .map(Self::is_item_playable)
                .unwrap_or(false)
            {
                return Some(candidate);
            }
            candidate = (candidate + 1) % total;
        }
        None
    }

    /// Determine what will play after the current track ends naturally,
    /// without mutating any state. Mirrors `on_track_finished`'s decision
    /// tree — used both to preload the gapless next track and to commit the
    /// transition when the engine reports it happened.
    fn peek_next_natural(&self) -> Option<GaplessTarget> {
        if self.stop_after_current {
            return None;
        }

        match self.repeat_mode {
            RepeatMode::Track => {
                self.current_index?; // only replay when a playlist track is loaded
                let song = self.current_song.clone()?;
                return Some(GaplessTarget {
                    song,
                    uuid: self.current_item_uuid.clone(),
                    kind: GaplessTargetKind::Replay,
                });
            }
            RepeatMode::Playlist => {
                let idx = self.get_next_index().unwrap_or(0);
                let candidate = self.peek_playable_index(idx)?;
                return self.target_at_virtual_index(candidate);
            }
            _ => {}
        }

        // Queue first (peek without popping), then natural playlist order.
        if let Some(item) = self.queue.iter().find(|i| Self::is_item_playable(i)) {
            let song = item.song.clone()?;
            return Some(GaplessTarget {
                song,
                uuid: Some(item.uuid.clone()),
                kind: GaplessTargetKind::Queue,
            });
        }

        let idx = self.get_next_index()?;
        let candidate = self.peek_playable_index(idx)?;
        self.target_at_virtual_index(candidate)
    }

    fn target_at_virtual_index(&self, candidate: usize) -> Option<GaplessTarget> {
        let item_index = if self.shuffle_mode != ShuffleMode::Off {
            *self.shuffle_order.get(candidate)?
        } else {
            candidate
        };
        let item = self.playlist_items.get(item_index)?;
        let song = item.song.clone()?;
        Some(GaplessTarget {
            song,
            uuid: Some(item.uuid.clone()),
            kind: GaplessTargetKind::Index(candidate),
        })
    }

    /// Respond to the engine's `AboutToFinish` signal: prime the next track
    /// for a gapless handover. Does nothing when playback will naturally
    /// stop after the current track.
    pub async fn prepare_gapless_next(&mut self) -> Result<()> {
        self.preload_upcoming_waveforms();
        let Some(target) = self.peek_next_natural() else {
            return Ok(());
        };
        let start_ns = target.song.beginning_nanosec.max(0) as u64;
        self.audio
            .lock()
            .await
            .preload_next(Box::new(target.song), start_ns)
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
                let song = target.song;
                self.scrobble_point_nanosec = song.length_nanosec.map(|ns| (ns as u64) / 2);
                self.scrobbled = false;
                // The engine reports this exactly when the previous track's
                // last sample was consumed and `song` became audible — the
                // correct moment to flip the global loudness-gain slot.
                self.apply_loudness_gain(&song).await;

                match target.kind {
                    GaplessTargetKind::Replay => {
                        // Same track again — nothing else to update.
                    }
                    GaplessTargetKind::Index(candidate) => {
                        self.current_song = Some(song);
                        self.current_item_uuid = target.uuid;
                        self.current_index = Some(candidate);
                        if candidate > 0 && !self.played_indices.contains(&candidate) {
                            self.played_indices.push(candidate);
                        }
                    }
                    GaplessTargetKind::Queue => {
                        // Drop unplayable fronts, then the item that just
                        // started (mirrors next_track's queue handling).
                        while let Some(front) = self.queue.front() {
                            if Self::is_item_playable(front) {
                                break;
                            }
                            self.queue.pop_front();
                        }
                        self.queue.pop_front();
                        self.current_song = Some(song);
                        self.current_item_uuid = target.uuid;
                    }
                }
                self.persist_current_song();
                self.persist_position(0);
                Ok(())
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

    /// Called when the audio engine reports a track has finished.
    pub async fn on_track_finished(&mut self) -> Result<()> {
        if self.stop_after_current {
            self.stop_after_current = false;
            return self.stop().await;
        }

        match self.repeat_mode {
            RepeatMode::Track => {
                // Replay current track
                if let Some(idx) = self.current_index {
                    return self.play_at_index(idx).await;
                }
            }
            RepeatMode::Playlist => {
                let next = self.get_next_index();
                let idx = next.unwrap_or(0); // wrap around
                return self.play_at_index(idx).await;
            }
            _ => {}
        }

        self.next_track().await
    }

    /// Compute the next playback index based on mode.
    fn get_next_index(&self) -> Option<usize> {
        let len = self.playlist_items.len();
        if len == 0 {
            return None;
        }

        let current = self.current_index?;

        match self.shuffle_mode {
            ShuffleMode::Off => {
                let next = current + 1;
                if next < len {
                    Some(next)
                } else {
                    match self.repeat_mode {
                        RepeatMode::Playlist => Some(0),
                        _ => None,
                    }
                }
            }
            _ => {
                // In shuffle mode, current_index tracks position in shuffle_order
                let next = current + 1;
                if next < self.shuffle_order.len() {
                    Some(next)
                } else {
                    match self.repeat_mode {
                        RepeatMode::Playlist => Some(0),
                        _ => None,
                    }
                }
            }
        }
    }

    fn rebuild_shuffle_order(&mut self) {
        let len = self.playlist_items.len();
        let mut order: Vec<usize> = (0..len).collect();
        if self.shuffle_mode != ShuffleMode::Off && len > 0 {
            let mut rng = rand::thread_rng();
            order.shuffle(&mut rng);
        }
        self.shuffle_order = order;
    }

    pub fn set_shuffle_mode(&mut self, mode: ShuffleMode) {
        self.shuffle_mode = mode;
        self.rebuild_shuffle_order();
        let mode_str = match mode {
            ShuffleMode::Off => "off",
            ShuffleMode::All => "all",
            ShuffleMode::InsideAlbum => "inside_album",
            ShuffleMode::Albums => "albums",
            ShuffleMode::Artists => "artists",
        };
        if let Ok(conn) = self._db.pool.get() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES ('shuffle_mode', ?1)",
                rusqlite::params![mode_str],
            );
        }
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.repeat_mode = mode;
        let mode_str = match mode {
            RepeatMode::Off => "off",
            RepeatMode::Track => "track",
            RepeatMode::Album => "album",
            RepeatMode::Playlist => "playlist",
            RepeatMode::OneByOne => "one_by_one",
            RepeatMode::Intro => "intro",
        };
        if let Ok(conn) = self._db.pool.get() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES ('repeat_mode', ?1)",
                rusqlite::params![mode_str],
            );
        }
    }

    /// Get the current playback state snapshot for the frontend.
    pub async fn get_state(&self) -> PlaybackState {
        let audio = self.audio.lock().await;
        PlaybackState {
            state: audio.current_state(),
            current_song: self.current_song.clone(),
            playlist_id: self.current_playlist_id,
            playlist_item_uuid: self.current_item_uuid.clone(),
            position_nanosec: audio.current_position_nanosec() as i64,
            volume: audio.current_volume(),
            shuffle_mode: self.shuffle_mode,
            repeat_mode: self.repeat_mode,
            stop_after_current: self.stop_after_current,
            loudness_source: self.current_loudness_source,
            loudness_gain_db: self.current_loudness_gain_db,
            remaining_playlist_items: self.remaining_playlist_items(),
        }
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
        // TODO: dispatch to online scrobbler services here once scrobbling lands

        let song_id = self.current_song.as_ref()?.id;
        match self._db.pool.get() {
            Ok(conn) => match stats::record_play(&conn, song_id) {
                Ok(()) => {
                    let context = self
                        .current_play_context
                        .clone()
                        .unwrap_or(PlayContext::Song);
                    if let Err(e) = stats::record_play_context(&conn, &context, song_id) {
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
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_player_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        (db, temp_dir)
    }

    #[tokio::test]
    async fn test_player_state_persistence_and_restoration() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = Arc::new(db);

        // Insert a dummy song into DB
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

        // Test updating persistence
        player.current_song = None;
        player.persist_current_song();
        player.persist_position(0);

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
}
