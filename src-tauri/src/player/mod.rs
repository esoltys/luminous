//! Player — high-level playback controller.
//!
//! Mediates between AudioEngine, PlaylistManager, and the Tauri event system.
//! Owns playback state, persistence, and scrobble point tracking.

mod order;

use crate::{
    audio::AudioEngine,
    db::Database,
    models::{
        LoudnessGainSource, PlayContext, PlaybackState, PlaylistItem, RepeatMode, ShuffleMode,
        Song, VolumeSafety,
    },
    stats,
};
use anyhow::{anyhow, Result};
use order::{OrderTarget, OrderTargetKind, PlayOrder};
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

/// Whether a playback failure should flag a song `unavailable = 1` in the DB.
///
/// - Remote server tracks (`WebDav`, `Subsonic`) use `Never`: they have URL/URI
///   paths for which `Path::exists()` is always false, so a transient network or
///   auth failure would otherwise get misread as "confirmed missing" and hide
///   the song from library views (`unavailable = 0`); their availability is owned
///   by server sync (mirroring `find_missing_song_ids` in `collection.rs`).
/// - Live stations (`radio://` / stream sources) use `Always`: when offline they
///   receive the same treatment as missing files, and `materialize_station`
///   resets `unavailable = 0` on directory sync or explicit play.
/// - Local files use `WhenMissingOnDisk`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnavailableOnError {
    /// Always flag (live internet radio stations that fail to open).
    Always,
    /// Flag only when the local file path no longer exists on disk.
    WhenMissingOnDisk,
    /// Never flag (remote server tracks whose availability is owned by server sync).
    Never,
}

/// Per-track playback behaviour rules derived once from a [`Song`], replacing
/// scattered `is_live_song` and `is_remote` branches across [`Player`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlaybackProfile {
    /// Restore the saved playback position on startup.
    pub resumes_position: bool,
    /// Track a 50% scrobble point and record play/skip stats.
    pub scrobbles: bool,
    /// Cue the track into the audio engine on startup restore.
    pub cue_on_restore: bool,
    /// Proactively generate waveform visualizer data in the background.
    pub preloads_waveform: bool,
    /// Apply loudness normalization / ReplayGain.
    pub normalizes_loudness: bool,
    /// Reopen a stopped stream from position 0 on resume instead of unpausing.
    pub restart_on_resume: bool,
    /// Accept ICY `StreamTitle` updates for `station_now_playing`.
    pub accepts_stream_titles: bool,
    /// Record station play history and prune unreferenced stations on start.
    pub tracks_station_play: bool,
    /// Signal the background-read runner to yield network bandwidth while streaming (#1785).
    pub pauses_background_reads: bool,
    /// How playback errors affect the song's `unavailable` flag.
    pub unavailable_on_error: UnavailableOnError,
}

impl PlaybackProfile {
    /// Derives the playback profile for `song` from its source, file type, and URI scheme.
    pub(crate) fn for_song(song: &Song) -> Self {
        let is_live = crate::audio::is_live_song(song);
        let is_remote = song.source.is_remote();
        Self {
            resumes_position: !is_live,
            scrobbles: !is_live,
            cue_on_restore: !is_live,
            preloads_waveform: !is_live && !is_remote,
            normalizes_loudness: !is_live,
            restart_on_resume: is_live,
            accepts_stream_titles: is_live,
            tracks_station_play: is_live,
            pauses_background_reads: is_remote,
            unavailable_on_error: if is_live {
                UnavailableOnError::Always
            } else if is_remote {
                UnavailableOnError::Never
            } else {
                UnavailableOnError::WhenMissingOnDisk
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Restart-persistence helpers — `Player` owns 10 `app_state` keys (volume,
// volume_safety_enabled, volume_safety_ceiling, shuffle_mode, repeat_mode,
// last_song_id, last_playlist_id, last_item_uuid, last_position_nanosec,
// last_adhoc_song_ids). These give `Player::new`'s
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
    db: Arc<Database>,
    audio: Arc<Mutex<AudioEngine>>,

    // Current playback context
    pub current_song: Option<Song>,
    pub current_playlist_id: Option<i64>,
    pub current_item_uuid: Option<String>,
    /// What the user was inside (album/playlist/standalone) when this
    /// playback queue was started — recorded at the scrobble point for
    /// context-aware "Recently Played".
    current_play_context: Option<PlayContext>,

    // Playback mode & order
    pub stop_after_current: bool,
    /// Auto Continue (#1235): top the Queue up with fitting library songs
    /// as it nears its end. Persisted as `auto_continue`.
    pub auto_continue: bool,
    pub volume: f32,
    /// The volume safety ceiling (#1890). Persisted as
    /// `volume_safety_enabled` / `volume_safety_ceiling`.
    pub volume_safety: VolumeSafety,
    /// Playback speed (#2142); 1.0 is normal. Persisted as `playback_speed`.
    pub playback_speed: f32,

    // Loudness normalization (#77) — where the currently applied gain came
    // from, for the player-bar indicator.
    pub current_loudness_source: LoudnessGainSource,
    pub current_loudness_gain_db: Option<f32>,

    pub(super) order: PlayOrder,

    // Scrobble tracking
    /// Position at which we trigger the scrobble (50% of track length).
    scrobble_point_nanosec: Option<u64>,
    scrobbled: bool,
    scrobbler: Option<Arc<crate::scrobbler::ScrobblerManager>>,
    /// True once the Radio Browser click report has been dispatched for the
    /// current live station play (#2004).
    click_reported_for_play: bool,
    /// The track the playing station announces in its stream metadata
    /// (#2008). Lives here, never on the song row, and is cleared whenever a
    /// track starts or playback stops, so it never outlives its station play.
    station_now_playing: Option<crate::now_playing::StationNowPlaying>,

    /// Consecutive playback failures since the last successful `Playing`
    /// event — see `MAX_CONSECUTIVE_PLAYBACK_ERRORS`.
    consecutive_playback_errors: u32,

    /// Indicator shared across subsystems flagging when a remote audio stream
    /// (WebDAV/Subsonic) is active or buffering (#1785).
    pub remote_streaming_active: Option<Arc<std::sync::atomic::AtomicBool>>,
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
        let mut volume_safety = VolumeSafety::default();
        let mut playback_speed = 1.0f32;
        let mut shuffle_mode = ShuffleMode::Off;
        let mut repeat_mode = RepeatMode::Off;
        let mut auto_continue = false;
        let mut restored_song: Option<Song> = None;
        let mut restored_playlist_id: Option<i64> = None;
        let mut restored_item_uuid: Option<String> = None;
        let mut restored_position_ns: u64 = 0;
        let mut playlist_items: Vec<PlaylistItem> = Vec::new();
        let mut current_index: Option<usize> = None;

        // Query database settings on startup
        if let Ok(conn) = db.pool.get() {
            volume_safety = VolumeSafety::new(
                app_state_get(&conn, "volume_safety_enabled").as_deref() == Some("true"),
                app_state_get_parsed::<f32>(&conn, "volume_safety_ceiling")
                    .unwrap_or(VolumeSafety::DEFAULT_CEILING),
            );
            let saved_volume = app_state_get_parsed::<f32>(&conn, "volume");
            if let Ok(engine) = audio.try_lock() {
                // The cap goes in before the restored volume, so playback
                // can't start above it.
                engine.set_volume_ceiling(volume_safety.active_ceiling());
                volume = engine.set_volume(saved_volume.unwrap_or(volume));
            } else {
                // Unreachable at construction (nothing else holds the fresh
                // engine yet), but a skipped safety cap must not be silent.
                log::error!("Audio engine busy at startup: volume and safety ceiling not restored");
            }
            // Restored before anything plays, so the first track starts at
            // the speed the user left. The engine reports what it applies.
            if let Some(saved) = app_state_get_parsed::<f32>(&conn, "playback_speed") {
                playback_speed = match audio.try_lock() {
                    Ok(engine) => engine.set_playback_speed(saved).unwrap_or(1.0),
                    Err(_) => {
                        log::error!("Audio engine busy at startup: playback speed not restored");
                        1.0
                    }
                };
            }
            if let Some(s) = app_state_get(&conn, "shuffle_mode") {
                shuffle_mode = shuffle_mode_from_key(&s);
            }
            if let Some(s) = app_state_get(&conn, "repeat_mode") {
                repeat_mode = repeat_mode_from_key(&s);
            }
            auto_continue = app_state_get(&conn, "auto_continue").as_deref() == Some("true");

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

                if PlaybackProfile::for_song(song).resumes_position {
                    if let Some(pos) = app_state_get_parsed::<u64>(&conn, "last_position_nanosec") {
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

        let scrobble_point_nanosec = restored_song.as_ref().and_then(Self::scrobble_point_for);

        let player = Self {
            db,
            audio,
            current_song: restored_song.clone(),
            current_playlist_id: restored_playlist_id,
            current_item_uuid: restored_item_uuid,
            current_play_context: None,
            stop_after_current: false,
            auto_continue,
            volume,
            volume_safety,
            playback_speed,
            current_loudness_source: loudness_source,
            current_loudness_gain_db: loudness_gain_db,
            order: PlayOrder::new(playlist_items, current_index, shuffle_mode, repeat_mode),
            scrobble_point_nanosec,
            scrobbled: false,
            scrobbler: None,
            click_reported_for_play: false,
            station_now_playing: None,
            consecutive_playback_errors: 0,
            remote_streaming_active: None,
        };

        if let Some(song) = restored_song {
            if let Ok(engine) = player.audio.try_lock() {
                engine.set_loudness_gain(loudness_gain);
                // Live radio stations are restored without opening the stream
                // until the user explicitly presses play (#2004).
                if PlaybackProfile::for_song(&song).cue_on_restore {
                    let _ = engine.cue(crate::audio::track_for(&song), restored_position_ns);
                }
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
        if let Ok(conn) = self.db.pool.get() {
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

    /// Whether the player currently has the specified item loaded in its active playlist.
    pub fn has_playlist_item(&self, playlist_id: i64, uuid: &str) -> bool {
        self.current_playlist_id == Some(playlist_id) && self.order.contains_uuid(uuid)
    }

    /// Attach the scrobbler service manager to receive scrobble notifications.
    pub fn set_scrobbler(&mut self, scrobbler: Arc<crate::scrobbler::ScrobblerManager>) {
        self.scrobbler = Some(scrobbler);
    }

    /// Set an atomic boolean indicator to track when remote streaming is active (#1785).
    pub fn set_remote_streaming_indicator(
        &mut self,
        indicator: Arc<std::sync::atomic::AtomicBool>,
    ) {
        self.remote_streaming_active = Some(indicator);
    }

    /// Load a playlist into the player and start playing the given index.
    pub async fn play_playlist(
        &mut self,
        items: Vec<PlaylistItem>,
        start_index: usize,
        playlist_id: i64,
        context: Option<PlayContext>,
    ) -> Result<()> {
        self.current_playlist_id = Some(playlist_id);
        self.current_play_context = context.or_else(|| {
            if playlist_id > 0 && !self.is_queue_playlist(playlist_id) {
                Some(PlayContext::Playlist { playlist_id })
            } else {
                None
            }
        });
        self.scrobbled = false;

        let play_index = self.order.load(items, Some(start_index)).unwrap_or(0);

        self.persist_adhoc_playlist();

        self.play_at_index(play_index).await
    }

    /// Play a playlist item identified by its unique item UUID.
    pub async fn play_item_by_uuid(
        &mut self,
        playlist_id: i64,
        uuid: &str,
        fresh_items: Option<Vec<PlaylistItem>>,
    ) -> Result<()> {
        let is_same_playlist = self.current_playlist_id == Some(playlist_id);
        let has_uuid = is_same_playlist && self.order.contains_uuid(uuid);

        if !has_uuid {
            let items = fresh_items.ok_or_else(|| {
                anyhow!("Playlist items needed to switch to playlist {playlist_id}")
            })?;
            self.current_playlist_id = Some(playlist_id);
            self.current_play_context = if playlist_id > 0 && !self.is_queue_playlist(playlist_id) {
                Some(PlayContext::Playlist { playlist_id })
            } else {
                self.current_play_context.clone()
            };
            self.scrobbled = false;
            self.order.load(items, None);
            self.persist_adhoc_playlist();
        }

        let play_index = self
            .order
            .virtual_index_for_uuid(uuid)
            .ok_or_else(|| anyhow!("Playlist item with uuid '{uuid}' not found"))?;

        self.order.clear_live_unavailable_for_uuid(uuid);
        self.play_at_index(play_index).await
    }

    /// Append songs directly to the in-memory `PlayOrder` so the player
    /// can continue playing them seamlessly.  Called by Auto Continue (#1235,
    /// `continue_mix::extend`) after it has already persisted the new items
    /// to the DB.
    pub fn append_songs_to_playlist_items(&mut self, items: Vec<PlaylistItem>) {
        self.order.append(items);
    }

    /// Remove items from the in-memory `PlayOrder` by uuid, so the live
    /// player stops treating them as up-next after they're deleted from the
    /// backing DB playlist (mirrors `append_songs_to_playlist_items` for the
    /// removal direction — see #262). The currently-playing item is never
    /// dropped this way; it keeps playing to completion even if its DB row
    /// was removed, since yanking it out from under an in-flight decode
    /// would require a mid-playback jump this method doesn't attempt.
    pub fn remove_songs_from_playlist_items(&mut self, uuids: &[String]) {
        self.order
            .remove_by_uuids(uuids, self.current_item_uuid.as_deref());
    }

    /// Reorder items in the in-memory `PlayOrder` so the live player
    /// respects track reordering performed in the Queue UI.
    pub fn reorder_playlist_items(&mut self, from: usize, to: usize) {
        self.order
            .reorder(from, to, self.current_item_uuid.as_deref());
    }

    /// Rearranges the loaded playlist items into `order` (item uuids) after
    /// a sort or move — the general permutation behind
    /// `reorder_playlist_items`, with the same bookkeeping: the playing item
    /// stays current, and history follows its items (shuffle off) or keeps
    /// its play-order slots (shuffle on). Items missing from `order` keep
    /// their relative order after the listed ones; unknown uuids are ignored.
    pub fn apply_playlist_order(&mut self, order: &[String]) {
        self.order
            .apply_order(order, self.current_item_uuid.as_deref());
    }

    /// Clear all in-memory playlist items and play-next state when the Queue is cleared.
    pub fn clear_playlist_items(&mut self) {
        self.order.clear();
    }

    /// Number of playlist items that have not yet been played (ahead of current index).
    pub fn remaining_playlist_items(&self) -> usize {
        self.order.remaining_count()
    }

    /// Half-track scrobble threshold in nanoseconds, or `None` when the track
    /// does not scrobble or has no known duration.
    fn scrobble_point_for(song: &Song) -> Option<u64> {
        if PlaybackProfile::for_song(song).scrobbles {
            song.length_nanosec.map(|ns| (ns as u64) / 2)
        } else {
            None
        }
    }

    /// Clears per-play ephemeral state when transitioning to a new track.
    fn reset_track_ephemeral_state(&mut self) {
        self.scrobbled = false;
        self.click_reported_for_play = false;
        self.station_now_playing = None;
    }

    fn resolve_order_target(
        &self,
        target: OrderTarget,
    ) -> Result<(Song, Option<String>, OrderTargetKind)> {
        match target {
            OrderTarget::Replay => {
                let song = self
                    .current_song
                    .clone()
                    .ok_or_else(|| anyhow!("no current song to replay"))?;
                Ok((
                    song,
                    self.current_item_uuid.clone(),
                    OrderTargetKind::Replay,
                ))
            }
            OrderTarget::Index {
                virtual_index,
                song,
                uuid,
            } => Ok((song, uuid, OrderTargetKind::Index(virtual_index))),
            OrderTarget::PlayNext { song, uuid } => Ok((song, uuid, OrderTargetKind::PlayNext)),
        }
    }

    /// The single "start a track" sequence shared by `play_at_index`,
    /// `next_track`, and `on_gapless_transition`'s commit —
    /// scrobble point, current song/uuid/(index or `play_next` pop), loudness
    /// gain, waveform preload, and restart persistence. `kind` mirrors
    /// `PlayOrder::peek_next_natural`'s already-decided target (#1072); `real_play`
    /// issues an actual `AudioEngine::play` call, skipped for gapless
    /// commits where the audio is already playing (position is persisted as
    /// 0 in that case, matching a track that just started).
    async fn start_track(
        &mut self,
        song: Song,
        uuid: Option<String>,
        kind: OrderTargetKind,
        real_play: bool,
    ) -> Result<()> {
        let start_ns = song.beginning_nanosec.max(0) as u64;
        let profile = PlaybackProfile::for_song(&song);

        self.scrobble_point_nanosec = Self::scrobble_point_for(&song);
        self.reset_track_ephemeral_state();

        self.order.commit(kind);
        match kind {
            OrderTargetKind::Replay => {}
            OrderTargetKind::Index(_) | OrderTargetKind::PlayNext => {
                self.current_song = Some(song.clone());
                self.current_item_uuid = uuid;
            }
        }

        self.apply_loudness_gain(&song).await;
        if let Some(ref ind) = self.remote_streaming_active {
            ind.store(
                profile.pauses_background_reads,
                std::sync::atomic::Ordering::Relaxed,
            );
        }
        self.preload_upcoming_waveforms();
        if profile.tracks_station_play {
            if let Ok(conn) = self.db.pool.get() {
                crate::radio_directory::note_station_started(&conn, song.id, real_play);
            }
        }
        self.persist_current_song().await;
        self.persist_position(if real_play { start_ns } else { 0 })
            .await;

        if real_play {
            let audio = self.audio.lock().await;
            audio.play(crate::audio::track_for(&song), start_ns)
        } else {
            Ok(())
        }
    }

    /// Play the item at the given index (in virtual/shuffle order).
    /// If the item is unavailable, auto-advances to the next playable track.
    async fn play_at_index(&mut self, index: usize) -> Result<()> {
        let Some(target) = self.order.resolve_playable_target(index) else {
            log::warn!("Entire playlist contains only unavailable tracks — stopping.");
            return self.stop().await;
        };
        let (song, uuid, kind) = self.resolve_order_target(target)?;
        self.start_track(song, uuid, kind, true).await
    }

    /// Proactively pre-generates waveform visualizer data for the current song
    /// and the upcoming next track in `play_next`/playlist on a background thread.
    ///
    /// Remote sources (WebDAV, Subsonic) are excluded: full-file background decodes
    /// over HTTP saturate network connections and server worker pools, starving
    /// real-time audio playback (#1785).
    pub fn preload_upcoming_waveforms(&self) {
        let db = Arc::clone(&self.db);
        let mut songs_to_preload = Vec::new();

        let next_target = self.order.peek_next_natural(self.stop_after_current);
        let next_song = next_target
            .as_ref()
            .and_then(|t| t.song(self.current_song.as_ref()));
        for song in [self.current_song.as_ref(), next_song]
            .into_iter()
            .flatten()
        {
            if PlaybackProfile::for_song(song).preloads_waveform {
                if let Some(ref path_str) = song.path {
                    songs_to_preload.push((song.id, std::path::PathBuf::from(path_str)));
                }
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

    /// Persist the ordered song-id list for ad-hoc playlists (`playlist_id ==
    /// 0` — album/artist/search selections, not a saved DB playlist) so a
    /// restart can rebuild the full playback context instead of just the
    /// current song. Saved playlists don't need this: they're reloaded from
    /// the `playlists`/`playlist_items` tables via `last_playlist_id`.
    fn persist_adhoc_playlist(&self) {
        if let Ok(conn) = self.db.pool.get() {
            if self.current_playlist_id == Some(0) {
                let song_ids: Vec<i64> = self
                    .order
                    .items()
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
        let db = self.db.clone();
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
        let db = self.db.clone();
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
        // Loudness normalization and ReplayGain are never applied to live streams (#2002).
        if settings.enabled && PlaybackProfile::for_song(song).normalizes_loudness {
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
        let settings = match Self::load_loudness_settings(&self.db).await {
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
        let settings = match Self::load_loudness_settings(&self.db).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to load loudness settings: {e}");
                return;
            }
        };
        let (target_gain, source, gain_db) = Self::compute_loudness_gain(&settings, &song);
        self.current_loudness_source = source;
        self.current_loudness_gain_db = gain_db;

        self.audio
            .lock()
            .await
            .ramp_loudness_gain(target_gain, Self::LOUDNESS_REFRESH_RAMP_MS);
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
        self.order
            .update_song_instrumental(song_id, is_instrumental);
    }

    /// How long pause/resume/stop transitions should fade, in ms (0 when
    /// fading is off, so they apply instantly) — the one
    /// decision `pause`, `resume`, and `stop` each otherwise re-derived from
    /// the same DB settings independently.
    ///
    /// Runs the rusqlite read on a blocking thread (#1097) — every caller is
    /// itself called while `AppState.player`'s async mutex is held, so a
    /// synchronous read here would stall every other IPC command waiting on
    /// that lock for as long as the r2d2 pool takes.
    async fn fade_duration_ms(&self) -> u32 {
        let db = self.db.clone();
        let settings = tokio::task::spawn_blocking(move || {
            crate::fade::get_fade_settings_from_db(&db).unwrap_or_default()
        })
        .await
        .unwrap_or_default();
        if settings.fade_pause_enabled {
            settings.fade_pause_duration_ms
        } else {
            0
        }
    }

    pub async fn pause(&self) -> Result<()> {
        let pos = self.audio.lock().await.current_position_nanosec();
        self.persist_position(pos).await;
        let fade_ms = self.fade_duration_ms().await;
        self.audio.lock().await.pause(fade_ms)
    }

    pub async fn resume(&self) -> Result<()> {
        if let Some(ref song) = self.current_song {
            if PlaybackProfile::for_song(song).restart_on_resume {
                let audio = self.audio.lock().await;
                if audio.current_state() == luminous_audio::EngineState::Stopped {
                    return audio.play(crate::audio::track_for(song), 0);
                }
            }
        }
        let fade_ms = self.fade_duration_ms().await;
        self.audio.lock().await.resume(fade_ms)
    }

    /// Records the `StreamTitle` the engine reported for `track_id` (#2008).
    /// Ignored unless that track is the live station playing now, so a late
    /// title from the previous station never lands on the next one. Returns
    /// whether the announced track changed (an unparseable title clears it).
    pub fn note_stream_title(&mut self, track_id: i64, raw: &str) -> bool {
        let Some(song) = self
            .current_song
            .as_ref()
            .filter(|s| s.id == track_id && PlaybackProfile::for_song(s).accepts_stream_titles)
        else {
            return false;
        };
        let parsed = crate::now_playing::parse_stream_title(raw, song.title.as_deref());
        if parsed == self.station_now_playing {
            return false;
        }
        self.station_now_playing = parsed;
        true
    }

    /// Forgets the station's announced track, e.g. when its stream drops.
    /// Returns whether there was one.
    pub fn clear_station_now_playing(&mut self) -> bool {
        self.station_now_playing.take().is_some()
    }

    /// Returns the Radio Browser `stationuuid` once per play when a `radio://`
    /// station begins playing, so the caller can dispatch the click report (#2004).
    pub fn take_pending_station_click_uuid(&mut self) -> Option<String> {
        if self.click_reported_for_play {
            return None;
        }
        let song = self.current_song.as_ref()?;
        if song.source != crate::models::SongSource::RadioBrowser {
            return None;
        }
        let path = song.path.as_deref()?;
        let uuid = crate::radio_directory::parse_station_uuid(path)?;
        self.click_reported_for_play = true;
        Some(uuid.to_string())
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
        // Preserve `mtime` so the next scan still detects any tag edits made
        // alongside the external case rename (see `repath_song_preserving_mtime`).
        let updated = match self.db.pool.get() {
            Ok(conn) => conn
                .execute(
                    "UPDATE songs SET path = ?1 WHERE id = ?2",
                    rusqlite::params![healed_str, song_id],
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
            if let Some(item) = self.order.items_mut().iter_mut().find(|i| i.uuid == uuid) {
                if let Some(song) = item.song.as_mut() {
                    song.path = Some(healed_str.clone());
                }
            }
        }

        if let Some(idx) = self.order.current_index() {
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
            // See `UnavailableOnError` / `PlaybackProfile::for_song` for source-specific policy.
            let should_flag_unavailable = match PlaybackProfile::for_song(song).unavailable_on_error
            {
                UnavailableOnError::Always => true,
                UnavailableOnError::Never => false,
                UnavailableOnError::WhenMissingOnDisk => song
                    .path
                    .as_deref()
                    .map(|p| !std::path::Path::new(p).exists())
                    .unwrap_or(false),
            };
            if should_flag_unavailable {
                if let Ok(conn) = self.db.pool.get() {
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
        if let Some(song) = &mut self.current_song {
            if song.unavailable {
                if let Ok(conn) = self.db.pool.get() {
                    let _ = conn.execute(
                        "UPDATE songs SET unavailable = 0 WHERE id = ?1 AND unavailable = 1",
                        rusqlite::params![song.id],
                    );
                }
                song.unavailable = false;
            }
        }
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(ref ind) = self.remote_streaming_active {
            ind.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        self.current_song = None;
        self.current_item_uuid = None;
        self.current_playlist_id = None;
        self.station_now_playing = None;
        self.persist_current_song().await;
        self.persist_position(0).await;
        let fade_ms = self.fade_duration_ms().await;
        self.audio.lock().await.stop(fade_ms)
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

    /// Set the volume, capped by the safety ceiling; `self.volume` holds
    /// the level actually applied.
    pub async fn set_volume(&mut self, vol: f32) -> Result<()> {
        self.volume = self.audio.lock().await.set_volume(vol);
        if let Ok(conn) = self.db.pool.get() {
            app_state_set(&conn, "volume", &self.volume.to_string());
        }
        Ok(())
    }

    /// Play at `speed` (#2142). The engine clamps it to 0.25-3.0 in 0.05
    /// steps; `self.playback_speed` holds what it applies, and that is what
    /// gets persisted.
    pub async fn set_playback_speed(&mut self, speed: f32) -> Result<()> {
        self.playback_speed = self.audio.lock().await.set_playback_speed(speed)?;
        if let Ok(conn) = self.db.pool.get() {
            app_state_set(&conn, "playback_speed", &self.playback_speed.to_string());
        }
        Ok(())
    }

    /// Turn the volume safety ceiling (#1890) on or off and set its level
    /// (kept within `VolumeSafety`'s bounds). Turning it on, or lowering
    /// it, brings a louder volume down to the ceiling at once.
    pub async fn set_volume_safety(&mut self, enabled: bool, ceiling: f32) {
        self.volume_safety = VolumeSafety::new(enabled, ceiling);
        self.volume = self
            .audio
            .lock()
            .await
            .set_volume_ceiling(self.volume_safety.active_ceiling());
        if let Ok(conn) = self.db.pool.get() {
            app_state_set(&conn, "volume_safety_enabled", &enabled.to_string());
            app_state_set(
                &conn,
                "volume_safety_ceiling",
                &self.volume_safety.ceiling.to_string(),
            );
            app_state_set(&conn, "volume", &self.volume.to_string());
        }
    }

    /// Records an external volume change (e.g. from alsamixer or a physical DAC knob)
    /// without re-issuing a volume command to the audio engine.
    pub fn update_external_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 1.0);
        if let Ok(conn) = self.db.pool.get() {
            app_state_set(&conn, "volume", &self.volume.to_string());
        }
    }

    /// Advance playback: first drains any `play_next` entries
    /// (skipping ones that became unplayable), and only once that's empty
    /// falls back to the normal shuffle/repeat-aware playlist advance.
    pub async fn next_track(&mut self) -> Result<()> {
        match self.order.next_manual() {
            Some(target) => {
                let (song, uuid, kind) = self.resolve_order_target(target)?;
                self.start_track(song, uuid, kind, true).await
            }
            None => self.stop().await,
        }
    }

    /// Go back a track: in shuffle mode, walks back through actual play
    /// history; otherwise steps to `current_index - 1` in playlist order.
    pub async fn previous_track(&mut self) -> Result<()> {
        if let Some(prev_index) = self.order.previous() {
            self.play_at_index(prev_index).await
        } else {
            Ok(())
        }
    }

    /// Respond to the engine's `AboutToFinish` signal: prime the next track
    /// for a gapless or crossfade handover (#79). Does nothing when playback will
    /// naturally stop after the current track.
    pub async fn prepare_gapless_next(&mut self) -> Result<()> {
        self.preload_upcoming_waveforms();
        let Some(target) = self.order.peek_next_natural(self.stop_after_current) else {
            return Ok(());
        };
        let (song, _, _) = self.resolve_order_target(target)?;
        let start_ns = song.beginning_nanosec.max(0) as u64;

        let fade_settings = crate::fade::get_fade_settings_from_db(&self.db).unwrap_or_default();

        let is_same_album = if let Some(current) = &self.current_song {
            current.is_same_album_or_cue_sibling(&song)
        } else {
            false
        };

        if fade_settings.crossfade_auto_enabled
            && (!fade_settings.crossfade_suppress_same_album || !is_same_album)
        {
            log::info!(
                "Auto-crossfade armed for track transition to song {}",
                song.id
            );
            // The engine switches to the incoming track's loudness gain at
            // the overlap's first sample, so it needs that gain up front.
            let incoming_gain = match Self::load_loudness_settings(&self.db).await {
                Ok(settings) => Self::compute_loudness_gain(&settings, &song).0,
                Err(e) => {
                    log::warn!("Failed to load loudness settings: {e}");
                    1.0
                }
            };
            self.audio.lock().await.preload_next_with_crossfade(
                crate::audio::track_for(&song),
                start_ns,
                fade_settings.crossfade_auto_duration_secs,
                incoming_gain,
            )
        } else {
            self.audio
                .lock()
                .await
                .preload_next(crate::audio::track_for(&song), start_ns)
        }
    }

    /// Commit a completed gapless handover reported by the engine. Advances
    /// `play_next`/index/scrobble bookkeeping exactly as `on_track_finished` would,
    /// but without issuing a new `Play` (the audio never stopped). If the
    /// playback context changed since the preload (mode/playlist/`play_next` edits), falls
    /// back to the normal advance logic to self-heal.
    pub async fn on_gapless_transition(&mut self, started_song_id: i64) -> Result<()> {
        if self.stop_after_current {
            self.stop_after_current = false;
            return self.stop().await;
        }

        if let Some(target) = self.order.peek_next_natural(false) {
            let (song, uuid, kind) = self.resolve_order_target(target)?;
            if song.id == started_song_id {
                return self.start_track(song, uuid, kind, false).await;
            }
        }

        // The preloaded track no longer matches what should play —
        // correct by running the normal advance (issues a real Play).
        log::warn!(
            "Gapless transition to song {started_song_id} no longer matches playback context; correcting"
        );
        self.on_track_finished().await
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

        if let Some(target) = self.order.peek_next_natural(false) {
            let (song, uuid, kind) = self.resolve_order_target(target)?;
            return self.start_track(song, uuid, kind, true).await;
        }

        self.next_track().await
    }

    pub fn set_shuffle_mode(&mut self, mode: ShuffleMode) {
        self.order.set_shuffle_mode(mode);
        if let Ok(conn) = self.db.pool.get() {
            app_state_set(&conn, "shuffle_mode", shuffle_mode_to_key(mode));
        }
    }

    /// The current playlist's items in actual playback order.
    pub fn get_playlist_tracks_in_playback_order(&self) -> Vec<PlaylistItem> {
        self.order.tracks_in_playback_order()
    }

    pub fn set_auto_continue(&mut self, enabled: bool) {
        self.auto_continue = enabled;
        if let Ok(conn) = self.db.pool.get() {
            app_state_set(
                &conn,
                "auto_continue",
                if enabled { "true" } else { "false" },
            );
        }
    }

    /// When Auto Continue should top up the playing list right now, returns
    /// its playlist id and the seed songs: the current song and the ones
    /// before it in playback order, most recent first, up to
    /// `continue_mix::SEED_COUNT`. Requires Auto Continue on, Repeat off, and
    /// at most `continue_mix::TOP_UP_THRESHOLD` songs left (play-next items
    /// included). The caller still checks the playlist is the Queue — that
    /// needs the playlists lock, which must not nest inside this one.
    pub fn auto_continue_seed(&self) -> Option<(i64, Vec<i64>)> {
        if !self.auto_continue {
            return None;
        }
        let playlist_id = self.current_playlist_id?;
        let song = self.current_song.as_ref()?;
        let seeds = self.order.auto_continue_seeds(song.id)?;
        Some((playlist_id, seeds))
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.order.set_repeat_mode(mode);
        if let Ok(conn) = self.db.pool.get() {
            app_state_set(&conn, "repeat_mode", repeat_mode_to_key(mode));
        }
    }

    /// Re-fetch every queued item's `Song` (and the current song) from the DB
    /// by id.
    pub fn resync_queue_with_db(&mut self) -> Result<()> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );

        for item in self.order.items_mut() {
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
        let (playlist_index, playlist_total) = self.order.position_info();
        let state: crate::models::PlayState = audio.current_state().into();
        let is_live = self
            .current_song
            .as_ref()
            .is_some_and(crate::audio::is_live_song);
        let can_seek = self.current_song.is_some() && !is_live;
        let position_nanosec = if is_live && state == crate::models::PlayState::Stopped {
            0
        } else {
            audio
                .current_position_nanosec()
                .saturating_sub(self.current_song_beginning_nanosec()) as i64
        };
        PlaybackState {
            state,
            current_song: self.current_song.clone(),
            playlist_id: self.current_playlist_id,
            playlist_item_uuid: self.current_item_uuid.clone(),
            position_nanosec,
            volume: audio.current_volume(),
            shuffle_mode: self.order.shuffle_mode(),
            repeat_mode: self.order.repeat_mode(),
            stop_after_current: self.stop_after_current,
            loudness_source: self.current_loudness_source,
            loudness_gain_db: self.current_loudness_gain_db,
            remaining_playlist_items: self.remaining_playlist_items(),
            auto_continue: self.auto_continue,
            playlist_index,
            playlist_total,
            volume_safety: self.volume_safety,
            playback_speed: self.playback_speed,
            station_now_playing: self.station_now_playing.clone(),
            can_seek,
        }
    }

    /// Snapshots the current audio pipeline configuration for the active track (#1041).
    pub fn get_pipeline_info(
        &self,
        audio: &AudioEngine,
    ) -> Option<crate::models::AudioPipelineInfo> {
        crate::audio::pipeline_info(
            audio,
            self.current_song.as_ref(),
            self.current_loudness_source,
            self.current_loudness_gain_db,
        )
    }

    /// Update position and check scrobble point. When the scrobble point is
    /// crossed, the listen is recorded (playcount/lastplayed) and the
    /// `song-stats-changed` payload is returned for the caller to emit.
    pub fn on_position_update(&mut self, position_nanosec: u64) -> Option<serde_json::Value> {
        if self
            .current_song
            .as_ref()
            .is_some_and(|s| !PlaybackProfile::for_song(s).scrobbles)
        {
            return None;
        }
        let scrobble_at = self.scrobble_point_nanosec?;
        if self.scrobbled || position_nanosec < scrobble_at {
            return None;
        }
        self.scrobbled = true;
        log::debug!("Scrobble point reached at {}ns", position_nanosec);

        if let (Some(scrobbler), Some(song)) = (&self.scrobbler, &self.current_song) {
            let scrobbler = Arc::clone(scrobbler);
            let song = song.clone();
            let listened_at = crate::clock::unix_now();
            tokio::spawn(async move {
                scrobbler.on_scrobble_point(&song, listened_at).await;
            });
        }

        let song_id = self.current_song.as_ref()?.id;
        let duration_secs = self.current_song.as_ref()?.duration_secs().round() as i64;
        match self.db.pool.get() {
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
        if self.scrobbled
            || self
                .current_song
                .as_ref()
                .is_some_and(|s| !PlaybackProfile::for_song(s).scrobbles)
        {
            return None;
        }
        let song_id = self.current_song.as_ref()?.id;
        let db = self.db.clone();
        tokio::task::block_in_place(move || match db.pool.get() {
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
        })
    }
}

#[cfg(test)]
mod invariant_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use std::sync::Arc;

    /// Test database directory, removed on drop. `Player` hands a database
    /// clone to a detached waveform-preload task that can outlive the test,
    /// and Windows can't delete a directory with open SQLite files, so drop
    /// retries until that task has released the database.
    pub(super) struct TestDir(Option<tempfile::TempDir>);

    impl TestDir {
        pub(super) fn path(&self) -> &std::path::Path {
            self.0.as_ref().unwrap().path()
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let path = self.0.take().unwrap().keep();
            for _ in 0..100 {
                if std::fs::remove_dir_all(&path).is_ok() || !path.exists() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    pub(super) fn setup_test_db() -> (TestDir, Database) {
        let temp_dir = tempfile::Builder::new()
            .prefix("luminous_player_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        (TestDir(Some(temp_dir)), db)
    }

    #[tokio::test]
    async fn note_playback_error_never_flags_webdav_songs_unavailable() {
        let (_temp_dir, db) = setup_test_db();
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

    /// Auto Continue (#1235) tops up only when it's on, Repeat is off and
    /// at most one song is left; its seeds are the current song followed by
    /// the songs played just before it, most recent first.
    #[tokio::test]
    async fn auto_continue_seed_requires_toggle_repeat_off_and_near_end() {
        let (_temp_dir, db) = setup_test_db();
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(Arc::new(db), audio);

        let song = |id: i64| crate::models::Song {
            id,
            ..Default::default()
        };
        let items = (1..=5)
            .map(|id| PlaylistItem::new_song(7, id as i32, song(id)))
            .collect();
        player.order.load(items, Some(2));
        player.current_playlist_id = Some(7);
        player.current_song = Some(song(3));

        assert_eq!(player.auto_continue_seed(), None, "off by default");
        player.auto_continue = true;
        assert_eq!(player.auto_continue_seed(), None, "two songs still left");

        player.order.set_current_index(Some(3));
        player.current_song = Some(song(4));
        assert_eq!(player.auto_continue_seed(), Some((7, vec![4, 3, 2])));

        player.set_repeat_mode(RepeatMode::Playlist);
        assert_eq!(
            player.auto_continue_seed(),
            None,
            "repeat keeps its meaning"
        );
    }

    #[tokio::test]
    async fn test_player_state_persistence_and_restoration() {
        let (_temp_dir, db) = setup_test_db();
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
    }

    #[tokio::test]
    async fn test_adhoc_queue_survives_restart() {
        let (_temp_dir, db) = setup_test_db();
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
        assert_eq!(restarted.order.items().len(), 3);
        assert_eq!(restarted.order.current_index(), Some(1));
    }

    #[tokio::test]
    async fn volume_safety_caps_volume_and_is_restored_before_it() {
        let (_dir, db) = setup_test_db();
        let db_arc = Arc::new(db);
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());

        player.set_volume(0.9).await.unwrap();
        player.set_volume_safety(true, 0.6).await;
        assert_eq!(player.volume, 0.6, "turning the limit on lowers the volume");
        player.set_volume(1.0).await.unwrap();
        assert_eq!(player.volume, 0.6, "requests above the ceiling stop at it");

        player.set_volume_safety(true, 2.0).await;
        assert_eq!(player.volume_safety.ceiling, VolumeSafety::MAX_CEILING);
        player.set_volume_safety(true, 0.6).await;

        // A saved volume above the ceiling (say, from before the limit was
        // turned on) must not reach a freshly opened engine.
        app_state_set(&db_arc.pool.get().unwrap(), "volume", "0.9");
        let fresh_audio = Arc::new(Mutex::new(AudioEngine::new()));
        let restarted = Player::new(db_arc.clone(), fresh_audio.clone());
        assert_eq!(restarted.volume_safety, VolumeSafety::new(true, 0.6));
        assert_eq!(restarted.volume, 0.6);
        assert_eq!(fresh_audio.lock().await.current_volume(), 0.6);
    }

    #[tokio::test]
    async fn playback_speed_is_snapped_persisted_and_restored() {
        let (_dir, db) = setup_test_db();
        let db_arc = Arc::new(db);
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db_arc.clone(), audio.clone());
        assert_eq!(player.playback_speed, 1.0);
        assert_eq!(player.get_state().await.playback_speed, 1.0);

        // The engine's answer, not the request, is what is kept and shown.
        player.set_playback_speed(1.12).await.unwrap();
        assert_eq!(player.playback_speed, 1.10);
        assert_eq!(player.get_state().await.playback_speed, 1.10);
        player.set_playback_speed(9.0).await.unwrap();
        assert_eq!(player.playback_speed, 3.0);
        player.set_playback_speed(0.1).await.unwrap();
        assert_eq!(player.playback_speed, 0.25);

        // A restart comes back at the speed the user left.
        player.set_playback_speed(1.5).await.unwrap();
        let restarted = Player::new(db_arc.clone(), Arc::new(Mutex::new(AudioEngine::new())));
        assert_eq!(restarted.playback_speed, 1.5);
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
        let (temp_dir_guard, db) = setup_test_db();
        let temp_dir = temp_dir_guard.path();
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
    }

    /// Same scenario as `PlayOrder::previous_walks_back_through_playlist`,
    /// but exercises the real saved-playlist path (`PlaylistManager` +
    /// `get_playlist_tracks`) instead of the ad-hoc `PlaylistItem::new_song`
    /// helper, to check for divergence between Album and Playlist playback
    /// reported in #105 ("Previous song works on albums, but not playlists").
    #[tokio::test]
    async fn test_previous_track_walks_back_through_saved_playlist() {
        let (_temp_dir, db) = setup_test_db();
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
        assert_eq!(player.order.current_index(), Some(2));

        player.previous_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            2,
            "previous should move to the prior playlist track, not replay the current one"
        );
        assert_eq!(player.order.current_index(), Some(1));
    }

    #[tokio::test]
    async fn test_play_playlist_with_shuffle_mode_plays_correct_requested_track() {
        let (_temp_dir, db) = setup_test_db();
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
    }

    /// Regression coverage for the get_next_index/find_playable_from/
    /// resolve_item_index consolidation (#577 item 13): non-shuffle forward
    /// advance must still stop after the last track with repeat off, and
    /// still wrap back to the first track with RepeatMode::Playlist.
    #[tokio::test]
    async fn test_next_track_stops_at_end_then_wraps_with_repeat_playlist() {
        let (_temp_dir, db) = setup_test_db();
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
    /// Regression coverage for #1073: `next_track`'s `play_next` branch
    /// used to update `current_song`/`current_item_uuid` in memory but skip
    /// `persist_current_song`/`persist_position` entirely — so quitting the
    /// app right after skipping into a queued "play next" track would
    /// restore the *previous* song on restart instead of the one that was
    /// actually audible when the app closed. Now routed through the shared
    /// `start_track` helper every "start a track" path uses, so it can no
    /// longer skip a step.
    #[tokio::test]
    async fn test_skipping_into_queued_track_persists_for_restart() {
        let (_temp_dir, db) = setup_test_db();
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
        player.order.play_next_mut().push_back(queued_item);

        player.next_track().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            4,
            "manual skip must drain play_next before continuing the playlist"
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
    }

    /// Coverage for the #1073 design decision: under `RepeatMode::Playlist`,
    /// a track finishing naturally must drain `play_next` first, the same as
    /// manual skip — previously `on_track_finished`'s `Playlist` arm called
    /// `play_at_index` directly and never looked at `play_next` at all,
    /// so a queued track would be silently skipped while repeat-playlist was on.
    #[tokio::test]
    async fn test_repeat_playlist_drains_play_next_before_wrapping() {
        let (_temp_dir, db) = setup_test_db();
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
            .order
            .play_next_mut()
            .push_back(PlaylistItem::new_song(0, 0, queued_song));

        player.on_track_finished().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            4,
            "RepeatMode::Playlist must drain a queued \"play next\" track before wrapping \
             back to the start of the playlist"
        );

        // `play_next` is now empty, so the *next* natural finish wraps as usual.
        player.on_track_finished().await.unwrap();
        assert_eq!(
            player.current_song.as_ref().unwrap().id,
            1,
            "once play_next is drained, RepeatMode::Playlist still wraps to the first track"
        );
    }

    /// Coverage for #1077: none of the file's tests previously touched
    /// gapless handover at all. Exercises the happy path —
    /// `prepare_gapless_next` preloading, then `on_gapless_transition`
    /// committing it — confirming index/scrobble/persistence bookkeeping
    /// lands correctly without a real `Play` call (the audio never stops
    /// for a gapless commit).
    #[tokio::test]
    async fn test_gapless_transition_commits_index_and_scrobble_bookkeeping() {
        let (_temp_dir, db) = setup_test_db();
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
        assert_eq!(player.order.current_index(), Some(1));
        assert!(
            player.order.played_indices().contains(&1),
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
    }

    /// Coverage for #1077: `on_gapless_transition`'s doc comment promises a
    /// self-healing fallback to `on_track_finished` when the preloaded
    /// track no longer matches what should play (e.g. mode/playlist/play_next changed
    /// after the preload was armed) — previously untested.
    #[tokio::test]
    async fn test_gapless_transition_mismatch_falls_back_to_on_track_finished() {
        let (_temp_dir, db) = setup_test_db();
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
        assert_eq!(player.order.current_index(), Some(1));
    }

    /// Coverage for #1077: scrobble-point computation and the `scrobbled`
    /// flag were previously untested. `on_position_update` must record the
    /// listen exactly once when the position first crosses the 50%-length
    /// scrobble point, and do nothing before or after that first crossing.
    #[tokio::test]
    async fn test_scrobble_point_reached_records_once() {
        let (_temp_dir, db) = setup_test_db();
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
    async fn test_remove_songs_preserves_shuffle_order() {
        let (_temp_dir, db) = setup_test_db();
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
        player.order.set_shuffle_order(vec![0, 3, 1, 4, 2]);
        player.order.set_current_index(Some(0));

        // Remove Song 2 (uuid of items[1])
        let removed_uuid = items[1].uuid.clone();
        player.remove_songs_from_playlist_items(&[removed_uuid]);

        // After removing Song 2 (items[1]):
        // Remaining items: Song 1 (idx 0), Song 3 (idx 1), Song 4 (idx 2), Song 5 (idx 3)
        // Shuffle order should be: Song 1 (0), Song 4 (2), Song 5 (3), Song 3 (1) -> vec![0, 2, 3, 1]
        assert_eq!(player.order.shuffle_order(), &[0, 2, 3, 1]);
        assert_eq!(player.order.current_index(), Some(0));
    }

    #[tokio::test]
    async fn test_get_playlist_tracks_in_playback_order_includes_already_played_items() {
        let (_temp_dir, db) = setup_test_db();
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
        player.order.set_current_index(Some(2));

        let result = player.get_playlist_tracks_in_playback_order();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].uuid, items[0].uuid);
        assert_eq!(result[1].uuid, items[1].uuid);
        assert_eq!(result[2].uuid, items[2].uuid);
    }

    /// A `Player` over songs `1..=count` (all on one album/artist), with the
    /// given ids flagged `unavailable`, plus the matching playlist items.
    fn player_with_songs(count: i64, unavailable: &[i64]) -> (TestDir, Player, Vec<PlaylistItem>) {
        let (temp_dir, db) = setup_test_db();
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
        (temp_dir, Player::new(db_arc, audio), items)
    }

    /// #1221: turning shuffle off must leave `current_index` on the playing
    /// track, so Next continues from it in playlist order.
    #[tokio::test]
    async fn test_shuffle_off_keeps_next_relative_to_current_track() {
        let (_temp_dir, mut player, items) = player_with_songs(6, &[]);

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
    }

    /// #1222: re-shuffling (switching shuffle modes) must keep Previous
    /// walking back through the songs that actually played.
    #[tokio::test]
    async fn test_reshuffle_keeps_previous_history() {
        let (_temp_dir, mut player, items) = player_with_songs(8, &[]);

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
    }

    /// #1223: under RepeatMode::Playlist, Previous wraps past the start even
    /// when the walk reaches it through unavailable tracks; without repeat
    /// it still stays put.
    #[tokio::test]
    async fn test_previous_wraps_past_unavailable_tracks_with_repeat_playlist() {
        let (_temp_dir, mut player, items) = player_with_songs(3, &[1]);

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
    }

    /// #1605: PlaybackState exposes 1-based playlist_index and total active tracks.
    #[tokio::test]
    async fn test_playback_state_playlist_index_and_total() {
        let (_temp_dir, mut player, items) = player_with_songs(5, &[]);

        // When stopped / idle before play
        let idle_state = player.get_state().await;
        assert_eq!(idle_state.playlist_index, None);
        assert_eq!(idle_state.playlist_total, 0);

        // Playing track at index 2 (the 3rd track, 1-based index 3)
        player
            .play_playlist(items.clone(), 2, 1, None)
            .await
            .unwrap();

        let state = player.get_state().await;
        assert_eq!(state.playlist_index, Some(3));
        assert_eq!(state.playlist_total, 5);

        // Advancing to next track
        player.next_track().await.unwrap();
        let next_state = player.get_state().await;
        assert_eq!(next_state.playlist_index, Some(4));
        assert_eq!(next_state.playlist_total, 5);
    }

    #[tokio::test]
    async fn test_remote_streaming_indicator_tracks_source() {
        let (_temp_dir, mut player, items) = player_with_songs(2, &[]);
        let indicator = Arc::new(std::sync::atomic::AtomicBool::new(false));
        player.set_remote_streaming_indicator(indicator.clone());

        // Playing a local track -> indicator stays false
        player
            .play_playlist(items.clone(), 0, 1, None)
            .await
            .unwrap();
        assert!(!indicator.load(std::sync::atomic::Ordering::Relaxed));

        // Now play a remote WebDAV track
        let mut remote_song = items[1].song.clone().unwrap();
        remote_song.source = crate::models::SongSource::WebDav;
        player
            .start_track(remote_song, None, OrderTargetKind::Index(1), false)
            .await
            .unwrap();
        assert!(indicator.load(std::sync::atomic::Ordering::Relaxed));

        // Stopping playback clears the indicator (#1785)
        player.stop().await.unwrap();
        assert!(!indicator.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn compute_loudness_gain_is_disabled_for_live_streams() {
        let settings = crate::models::LoudnessSettings {
            enabled: true,
            ..Default::default()
        };
        let mut song = Song {
            id: 1,
            ebur128_integrated_loudness_lufs: Some(-9.0),
            replaygain_track_gain: Some(-6.0),
            ..Song::default()
        };
        let (local_gain, local_src, _) = Player::compute_loudness_gain(&settings, &song);
        assert_ne!(local_src, LoudnessGainSource::Disabled);
        assert!((local_gain - 1.0).abs() > 1e-3);

        song.source = crate::models::SongSource::RadioBrowser;
        let (live_gain, live_src, live_db) = Player::compute_loudness_gain(&settings, &song);
        assert_eq!(live_gain, 1.0);
        assert_eq!(live_src, LoudnessGainSource::Disabled);
        assert_eq!(live_db, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn live_station_skips_stats_unavailable_flag_and_startup_stream_open() {
        let (_dir, db) = setup_test_db();
        let db = Arc::new(db);

        let station_id: i64 = {
            let conn = db.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (source, filetype, path, stream_url, title, length_nanosec)
                 VALUES (?1, ?2, 'radio://station-test-1', 'http://127.0.0.1:9/stream', 'Test Station', 120000000000)",
                rusqlite::params![
                    crate::models::SongSource::RADIO_BROWSER_ID,
                    crate::models::FileType::Stream as i32
                ],
            )
            .unwrap();
            let id = conn.last_insert_rowid();
            conn.execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_song_id', ?1), ('last_position_nanosec', '45000000000')",
                rusqlite::params![id.to_string()],
            )
            .unwrap();
            id
        };

        let audio = Arc::new(tokio::sync::Mutex::new(crate::audio::AudioEngine::new()));
        let mut player = Player::new(db.clone(), audio.clone());

        // Restored station is selected, position is 0, scrobble point is None,
        // and the audio engine stays Stopped (stream not opened at startup).
        assert_eq!(player.current_song.as_ref().map(|s| s.id), Some(station_id));
        assert_eq!(player.scrobble_point_nanosec, None);
        assert_eq!(
            audio.lock().await.current_state(),
            luminous_audio::EngineState::Stopped
        );

        // Position update and manual skip do not record stats.
        assert!(player.on_position_update(100_000_000_000).is_none());
        assert!(player.note_manual_skip().is_none());

        // Playback error flags an offline station unavailable = 1 (same as a missing file).
        let outcome = player.note_playback_error();
        assert!(outcome.flagged_unavailable);
        let unavailable: i32 = db
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT unavailable FROM songs WHERE id = ?1",
                rusqlite::params![station_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(unavailable, 1);

        // Click UUID is emitted once per play.
        assert_eq!(
            player.take_pending_station_click_uuid().as_deref(),
            Some("station-test-1")
        );
        assert_eq!(player.take_pending_station_click_uuid(), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn station_now_playing_follows_its_station_and_clears_on_change_and_stop() {
        let (_dir, db) = setup_test_db();
        let db = Arc::new(db);
        let audio = Arc::new(tokio::sync::Mutex::new(crate::audio::AudioEngine::new()));
        let mut player = Player::new(db, audio);
        let station = |id: i64, name: &str| Song {
            id,
            title: Some(name.to_string()),
            source: crate::models::SongSource::RadioBrowser,
            path: Some(format!("radio://station-{id}")),
            ..Song::default()
        };
        let announced = |p: &Player| {
            p.station_now_playing
                .as_ref()
                .map(|np| (np.artist.clone(), np.title.clone()))
        };

        player
            .start_track(
                station(1, "Groove Salad"),
                None,
                OrderTargetKind::PlayNext,
                false,
            )
            .await
            .unwrap();
        assert!(player.note_stream_title(1, "Air - Alone in Kyoto"));
        assert_eq!(
            announced(&player),
            Some((Some("Air".to_string()), "Alone in Kyoto".to_string()))
        );
        assert!(
            !player.note_stream_title(1, "Air - Alone in Kyoto"),
            "unchanged title: nothing to emit"
        );
        let state = player.get_state().await;
        assert_eq!(
            state.station_now_playing.map(|np| np.title).as_deref(),
            Some("Alone in Kyoto")
        );
        assert_eq!(
            state.current_song.and_then(|s| s.title).as_deref(),
            Some("Groove Salad"),
            "the song row keeps the station name"
        );

        // The station repeating its own name clears the announced track.
        assert!(player.note_stream_title(1, "Groove Salad"));
        assert_eq!(announced(&player), None);
        assert!(player.note_stream_title(1, "Air - Alone in Kyoto"));

        // A new station starts with nothing announced, and a late title from
        // the old station is ignored.
        player
            .start_track(
                station(2, "Drone Zone"),
                None,
                OrderTargetKind::PlayNext,
                false,
            )
            .await
            .unwrap();
        assert_eq!(announced(&player), None);
        assert!(!player.note_stream_title(1, "Late - Title"));
        assert_eq!(announced(&player), None);

        // The stream dropping, and stop, both clear it.
        assert!(player.note_stream_title(2, "Stars of the Lid - Requiem"));
        assert!(player.clear_station_now_playing());
        assert!(!player.clear_station_now_playing());
        assert!(player.note_stream_title(2, "Stars of the Lid - Requiem"));
        player.stop().await.unwrap();
        assert_eq!(announced(&player), None);

        // A library song never takes a station title.
        let local = Song {
            id: 3,
            title: Some("Roygbiv".to_string()),
            ..Song::default()
        };
        player
            .start_track(local, None, OrderTargetKind::PlayNext, false)
            .await
            .unwrap();
        assert!(!player.note_stream_title(3, "Some - Thing"));
        assert_eq!(announced(&player), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn live_station_list_failure_hands_over_to_next_station() {
        let (_dir, db) = setup_test_db();
        let db = Arc::new(db);

        let (id1, id2) = {
            let conn = db.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (source, filetype, path, stream_url, title)
                 VALUES (?1, ?2, 'radio://station-1', 'http://127.0.0.1:9/dead', 'Dead Station')",
                rusqlite::params![
                    crate::models::SongSource::RADIO_BROWSER_ID,
                    crate::models::FileType::Stream as i32
                ],
            )
            .unwrap();
            let id1 = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO songs (source, filetype, path, stream_url, title)
                 VALUES (?1, ?2, 'radio://station-2', 'http://127.0.0.1:9/backup', 'Backup Station')",
                rusqlite::params![
                    crate::models::SongSource::RADIO_BROWSER_ID,
                    crate::models::FileType::Stream as i32
                ],
            )
            .unwrap();
            let id2 = conn.last_insert_rowid();
            (id1, id2)
        };

        let s1 = Song {
            id: id1,
            source: crate::models::SongSource::RadioBrowser,
            filetype: crate::models::FileType::Stream,
            path: Some("radio://station-1".to_string()),
            title: Some("Dead Station".to_string()),
            ..Song::default()
        };
        let s2 = Song {
            id: id2,
            source: crate::models::SongSource::RadioBrowser,
            filetype: crate::models::FileType::Stream,
            path: Some("radio://station-2".to_string()),
            title: Some("Backup Station".to_string()),
            ..Song::default()
        };

        let audio = Arc::new(tokio::sync::Mutex::new(crate::audio::AudioEngine::new()));
        let mut player = Player::new(db.clone(), audio.clone());
        player.order.load(
            vec![
                crate::models::PlaylistItem::new_song(42, 0, s1.clone()),
                crate::models::PlaylistItem::new_song(42, 1, s2),
            ],
            Some(0),
        );
        player.current_playlist_id = Some(42);
        player.current_song = Some(s1);

        let outcome = player.note_playback_error();
        assert!(!outcome.should_stop);
        assert!(outcome.flagged_unavailable);
        assert_eq!(outcome.failed_song.as_ref().map(|s| s.id), Some(id1));

        // Handover advances to the next station in the list (even if stream open fails offline, index advances).
        let _ = player.next_track().await;
        assert_eq!(player.order.current_index(), Some(1));
        assert_eq!(player.current_song.as_ref().map(|s| s.id), Some(id2));
    }

    #[test]
    fn playback_profile_covers_local_remote_and_live_sources() {
        use crate::models::{FileType, SongSource};

        let local_expected = PlaybackProfile {
            resumes_position: true,
            scrobbles: true,
            cue_on_restore: true,
            preloads_waveform: true,
            normalizes_loudness: true,
            restart_on_resume: false,
            accepts_stream_titles: false,
            tracks_station_play: false,
            pauses_background_reads: false,
            unavailable_on_error: UnavailableOnError::WhenMissingOnDisk,
        };
        let remote_expected = PlaybackProfile {
            preloads_waveform: false,
            pauses_background_reads: true,
            unavailable_on_error: UnavailableOnError::Never,
            ..local_expected
        };
        let live_expected = PlaybackProfile {
            resumes_position: false,
            scrobbles: false,
            cue_on_restore: false,
            preloads_waveform: false,
            normalizes_loudness: false,
            restart_on_resume: true,
            accepts_stream_titles: true,
            tracks_station_play: true,
            pauses_background_reads: false,
            unavailable_on_error: UnavailableOnError::Always,
        };

        let cases = [
            (
                "local file",
                Song {
                    source: SongSource::LocalFile,
                    filetype: FileType::Flac,
                    path: Some("/music/track.flac".to_string()),
                    length_nanosec: Some(180_000_000_000),
                    ..Song::default()
                },
                local_expected,
            ),
            (
                "webdav remote",
                Song {
                    source: SongSource::WebDav,
                    filetype: FileType::Mp3,
                    path: Some("http://webdav.example/track.mp3".to_string()),
                    length_nanosec: Some(180_000_000_000),
                    ..Song::default()
                },
                remote_expected,
            ),
            (
                "subsonic remote",
                Song {
                    source: SongSource::Subsonic,
                    filetype: FileType::Flac,
                    path: Some("subsonic://123".to_string()),
                    length_nanosec: Some(180_000_000_000),
                    ..Song::default()
                },
                remote_expected,
            ),
            (
                "radio browser",
                Song {
                    source: SongSource::RadioBrowser,
                    filetype: FileType::Stream,
                    path: Some("radio://station-1".to_string()),
                    length_nanosec: Some(180_000_000_000),
                    ..Song::default()
                },
                live_expected,
            ),
            (
                "stream source with http path",
                Song {
                    source: SongSource::Stream,
                    filetype: FileType::Mp3,
                    path: Some("http://icecast.example/live.mp3".to_string()),
                    length_nanosec: Some(180_000_000_000),
                    ..Song::default()
                },
                live_expected,
            ),
            (
                "stream filetype alone",
                Song {
                    source: SongSource::Unknown,
                    filetype: FileType::Stream,
                    path: Some("http://example.com/stream".to_string()),
                    length_nanosec: Some(180_000_000_000),
                    ..Song::default()
                },
                live_expected,
            ),
            (
                "radio uri path alone",
                Song {
                    source: SongSource::Unknown,
                    filetype: FileType::Unknown,
                    path: Some("radio://station-2".to_string()),
                    length_nanosec: Some(180_000_000_000),
                    ..Song::default()
                },
                live_expected,
            ),
        ];

        for (label, song, expected) in &cases {
            let profile = PlaybackProfile::for_song(song);
            assert_eq!(profile, *expected, "case: {label}");
            let expected_scrobble = if expected.scrobbles {
                Some(90_000_000_000)
            } else {
                None
            };
            assert_eq!(
                Player::scrobble_point_for(song),
                expected_scrobble,
                "scrobble point case: {label}"
            );
        }
    }

    #[tokio::test]
    async fn get_state_reports_can_seek_and_zero_stopped_position_for_live_stream() {
        let (_dir, db) = setup_test_db();
        let db = Arc::new(db);
        let audio = Arc::new(tokio::sync::Mutex::new(crate::audio::AudioEngine::new()));
        let mut player = Player::new(db, audio);

        // No current song -> can_seek is false.
        let empty = player.get_state().await;
        assert!(!empty.can_seek);

        // Stopped live stream -> can_seek is false, position_nanosec is 0.
        player.current_song = Some(Song {
            id: 1,
            source: crate::models::SongSource::Stream,
            path: Some("https://icecast.example.com/live.mp3".to_string()),
            beginning_nanosec: 500,
            ..Song::default()
        });
        let live_state = player.get_state().await;
        assert!(!live_state.can_seek);
        assert_eq!(live_state.position_nanosec, 0);

        // Local track -> can_seek is true.
        player.current_song = Some(Song {
            id: 2,
            source: crate::models::SongSource::LocalFile,
            filetype: crate::models::FileType::Flac,
            path: Some("/music/track.flac".to_string()),
            ..Song::default()
        });
        let local_state = player.get_state().await;
        assert!(local_state.can_seek);
    }
}
