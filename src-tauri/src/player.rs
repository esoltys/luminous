//! Player — high-level playback controller.
//!
//! Mediates between AudioEngine, PlaylistManager, and the Tauri event system.
//! Owns the shuffle index, repeat mode, queue, and scrobble point tracking.

use crate::{
    audio::AudioEngine,
    db::Database,
    models::{PlaybackState, PlaylistItem, RepeatMode, ShuffleMode, Song},
};
use anyhow::{anyhow, Result};
use rand::seq::SliceRandom;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Player {
    _db: Arc<Database>,
    audio: Arc<Mutex<AudioEngine>>,

    // Current playback context
    pub current_song: Option<Song>,
    pub current_playlist_id: Option<i64>,
    pub current_item_uuid: Option<String>,

    // Playback mode
    pub shuffle_mode: ShuffleMode,
    pub repeat_mode: RepeatMode,
    pub stop_after_current: bool,
    pub volume: f32,

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
        }

        Self {
            _db: db,
            audio,
            current_song: None,
            current_playlist_id: None,
            current_item_uuid: None,
            shuffle_mode,
            repeat_mode,
            stop_after_current: false,
            volume,
            playlist_items: Vec::new(),
            shuffle_order: Vec::new(),
            current_index: None,
            played_indices: Vec::new(),
            queue: std::collections::VecDeque::new(),
            scrobble_point_nanosec: None,
            scrobbled: false,
        }
    }

    /// Load a playlist into the player and start playing the given index.
    pub async fn play_playlist(
        &mut self,
        items: Vec<PlaylistItem>,
        start_index: usize,
        playlist_id: i64,
    ) -> Result<()> {
        self.playlist_items = items;
        self.current_playlist_id = Some(playlist_id);
        self.played_indices.clear();
        self.queue.clear();
        self.scrobbled = false;

        // Build shuffle order if needed
        self.rebuild_shuffle_order();

        let play_index = if self.shuffle_mode != ShuffleMode::Off {
            // Find the position of start_index in the shuffle order
            self.shuffle_order.iter().position(|&i| i == start_index)
                .unwrap_or(0)
        } else {
            start_index
        };

        self.play_at_index(play_index).await
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
            *self.shuffle_order.get(candidate).ok_or(anyhow!("index out of bounds"))?
        } else {
            candidate
        };

        let item = self.playlist_items.get(item_index)
            .ok_or(anyhow!("playlist item not found"))?;

        let song = item.song.clone().ok_or(anyhow!("playlist item has no song"))?;
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

        let audio = self.audio.lock().await;
        audio.play(Box::new(song), start_ns)
    }

    pub async fn pause(&self) -> Result<()> {
        self.audio.lock().await.pause()
    }

    pub async fn resume(&self) -> Result<()> {
        self.audio.lock().await.resume()
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.current_song = None;
        self.current_item_uuid = None;
        self.audio.lock().await.stop()
    }

    pub async fn seek_to(&self, position_nanosec: u64) -> Result<()> {
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
            let song = queued.song.clone().ok_or(anyhow!("queued item has no song"))?;
            let start_ns = song.beginning_nanosec.max(0) as u64;
            self.current_song = Some(song.clone());
            self.current_item_uuid = Some(queued.uuid.clone());
            self.scrobble_point_nanosec = song.length_nanosec.map(|ns| (ns as u64) / 2);
            self.scrobbled = false;
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
                let item_index = self.shuffle_order.get(prev_index).copied().unwrap_or(prev_index);
                if self.playlist_items.get(item_index).map(Self::is_item_playable).unwrap_or(false) {
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
            let mut candidate = if current > 0 { current - 1 } else { len.saturating_sub(1) };
            for _ in 0..len {
                let item_index = if self.shuffle_mode != ShuffleMode::Off {
                    self.shuffle_order.get(candidate).copied().unwrap_or(candidate)
                } else {
                    candidate
                };
                if self.playlist_items.get(item_index).map(Self::is_item_playable).unwrap_or(false) {
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
        }
    }

    /// Update position and check scrobble point.
    pub fn on_position_update(&mut self, position_nanosec: u64) {
        if let Some(scrobble_at) = self.scrobble_point_nanosec {
            if !self.scrobbled && position_nanosec >= scrobble_at {
                self.scrobbled = true;
                // TODO: Phase 3 — trigger scrobbler here
                log::debug!("Scrobble point reached at {}ns", position_nanosec);
            }
        }
    }
}
