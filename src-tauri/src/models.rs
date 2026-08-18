//! Core data models — Rust structs mirroring the SQLite schema.
//!
//! These types are serialized via serde_json across the Tauri IPC boundary
//! and also used internally by all backend modules.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Song source enum
// ---------------------------------------------------------------------------

/// Where a Song originates. Determines URL resolution, scrobbling eligibility,
/// and display appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SongSource {
    #[default]
    Unknown = 0,
    LocalFile = 1,
    Collection = 2,
    Stream = 3,
    Tidal = 4,
    Subsonic = 5,
    Qobuz = 6,
    SomaFm = 7,
    RadioParadise = 8,
    Spotify = 9,
    RadioBrowser = 10,
}

impl fmt::Display for SongSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<i64> for SongSource {
    fn from(n: i64) -> Self {
        match n {
            1 => Self::LocalFile,
            2 => Self::Collection,
            3 => Self::Stream,
            4 => Self::Tidal,
            5 => Self::Subsonic,
            6 => Self::Qobuz,
            7 => Self::SomaFm,
            8 => Self::RadioParadise,
            9 => Self::Spotify,
            10 => Self::RadioBrowser,
            _ => Self::Unknown,
        }
    }
}

// ---------------------------------------------------------------------------
// File type enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileType {
    #[default]
    Unknown = 0,
    Mp3 = 1,
    Flac = 2,
    OggFlac = 3,
    OggVorbis = 4,
    OggOpus = 5,
    OggSpeex = 6,
    Aac = 7,
    Alac = 8,
    Aiff = 9,
    Wav = 10,
    WavPack = 11,
    Mpc = 12,
    TrueAudio = 13,
    Ape = 14,
    Dsf = 15,
    Dsdiff = 16,
    Asf = 17,
    Stream = 18,
}

impl From<i64> for FileType {
    fn from(n: i64) -> Self {
        match n {
            1 => Self::Mp3,
            2 => Self::Flac,
            3 => Self::OggFlac,
            4 => Self::OggVorbis,
            5 => Self::OggOpus,
            6 => Self::OggSpeex,
            7 => Self::Aac,
            8 => Self::Alac,
            9 => Self::Aiff,
            10 => Self::Wav,
            11 => Self::WavPack,
            12 => Self::Mpc,
            13 => Self::TrueAudio,
            14 => Self::Ape,
            15 => Self::Dsf,
            16 => Self::Dsdiff,
            17 => Self::Asf,
            18 => Self::Stream,
            _ => Self::Unknown,
        }
    }
}

// ---------------------------------------------------------------------------
// Multi-value field helpers (genre, artist, album artist, composer)
// ---------------------------------------------------------------------------

/// Delimiter used to pack multiple values into `songs.genre`/`artist`/
/// `album_artist`/`composer` and the tag editor's corresponding DTO fields,
/// matching the Mp3tag/Winamp `"; "` convention. Shared by every multi-value
/// field so the internal storage format stays consistent regardless of what
/// on-disk tag convention (if any) a given field's legacy fallback uses.
pub const MULTI_VALUE_DELIMITER: &str = "; ";

/// Splits a `songs.genre`/`artist`/`album_artist`/`composer`-style delimited
/// string into a clean, deduped list of individual values, trimming
/// whitespace and dropping empties. This is the single place that owns the
/// internal storage delimiter (`;`) — use it anywhere one of these columns
/// (or a single item read off an on-disk tag, in case some other tool joined
/// multiple values into one string with `;`, e.g. Mp3tag/Winamp's
/// convention) needs to be treated as a list rather than opaque text.
///
/// Deliberately does *not* also split on `/`, even though that's a
/// long-standing convention for legacy-joined multi-value fields (TPE1's own
/// "Lead performer(s)/Soloist(s)" convention, and Picard's default
/// `id3v23_join_with = "/"` for ID3v2.3) — `/` shows up too often inside a
/// single legitimate value (band names like "AC/DC", not to mention genre
/// names) to use as a splitting heuristic without silently corrupting them.
pub fn parse_multi_value(raw: &str) -> Vec<String> {
    split_and_dedup(raw, &[';'])
}

/// Joins a value list back into the delimited form stored in `songs.genre`/
/// `artist`/`album_artist`/`composer`.
pub fn join_multi_value(values: &[String]) -> String {
    values.join(MULTI_VALUE_DELIMITER)
}

/// Splits `raw` on any of `delimiters`, trims, drops empties, and dedupes
/// case-insensitively while preserving first-seen casing and order.
fn split_and_dedup(raw: &str, delimiters: &[char]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    raw.split(delimiters)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.to_lowercase()))
        .map(|s| s.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Song — central data model
// ---------------------------------------------------------------------------

/// Represents a single audio track. Mirrors the `songs` table in SQLite.
/// Durations are in nanoseconds for precision (CUE sheet support).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Song {
    pub id: i64,
    pub source: SongSource,
    pub filetype: FileType,

    // Paths & URLs
    pub path: Option<String>,
    pub url: Option<String>,        // for streams
    pub stream_url: Option<String>, // resolved at playback time

    // Core metadata
    pub title: Option<String>,
    pub titlesort: Option<String>,
    pub artist: Option<String>,
    pub artistsort: Option<String>,
    pub album: Option<String>,
    pub albumsort: Option<String>,
    pub album_artist: Option<String>,
    pub album_artist_sort: Option<String>,
    pub composer: Option<String>,
    pub composersort: Option<String>,
    pub performer: Option<String>,
    pub performersort: Option<String>,
    pub grouping: Option<String>,
    pub comment: Option<String>,
    pub lyrics: Option<String>,

    // Track info
    pub track: Option<i32>,
    pub disc: Option<i32>,
    pub year: Option<i32>,
    pub originalyear: Option<i32>,
    pub genre: Option<String>,
    pub compilation: bool,

    // Extended tags
    pub bpm: Option<f32>,
    pub initial_key: Option<String>,

    // Audio properties (durations in nanoseconds)
    pub length_nanosec: Option<i64>,
    pub beginning_nanosec: i64, // CUE sheet start (0 for normal files)
    pub end_nanosec: i64,       // CUE sheet end (0 for normal files)
    pub bitrate: Option<i32>,
    pub is_vbr: Option<bool>,
    pub samplerate: Option<i32>,
    pub bitdepth: Option<i32>,
    pub channels: Option<i32>,
    pub filesize: Option<i64>,
    pub mtime: Option<i64>,

    // Play statistics
    pub rating: f32, // 0.0–1.0, -1.0 = unset
    pub playcount: i32,
    pub skipcount: i32,
    pub lastplayed: Option<i64>,
    pub lastseen: Option<i64>,
    pub added: Option<i64>,

    // Album art
    pub art_embedded: bool,
    pub art_automatic: Option<String>, // auto-detected path/URL
    pub art_manual: Option<String>,    // user-set path/URL
    pub art_unset: bool,

    // CUE support
    pub cue_path: Option<String>,

    // AcoustID / fingerprint
    pub acoustid_id: Option<String>,
    pub acoustid_fingerprint: Option<String>,
    pub fingerprint: Option<String>,

    // MusicBrainz IDs
    pub musicbrainz_album_artist_id: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub musicbrainz_original_artist_id: Option<String>,
    pub musicbrainz_album_id: Option<String>,
    pub musicbrainz_original_album_id: Option<String>,
    pub musicbrainz_recording_id: Option<String>,
    pub musicbrainz_track_id: Option<String>,
    pub musicbrainz_disc_id: Option<String>,
    pub musicbrainz_release_group_id: Option<String>,
    pub musicbrainz_work_id: Option<String>,

    // EBU R128 loudness
    pub ebur128_integrated_loudness_lufs: Option<f64>,
    pub ebur128_loudness_range_lu: Option<f64>,

    // ReplayGain 2.0 tag fallback (#77) — dB gain normalized to the -18 LUFS
    // ReplayGain reference level, used when no R128 analysis is available yet.
    pub replaygain_track_gain: Option<f64>,
    pub replaygain_album_gain: Option<f64>,

    // Streaming service IDs
    pub artist_id: Option<String>,
    pub album_id: Option<String>,
    pub song_id: Option<String>,

    /// Set to `true` when the file is missing from disk (soft-delete).
    /// Song metadata is retained so playlists can display last-known info.
    pub unavailable: bool,

    /// Set to `true` when marked as an instrumental track (suppresses online lyrics fetching).
    pub is_instrumental: bool,
}

impl Song {
    /// Returns the display title, falling back to the filename.
    pub fn display_title(&self) -> &str {
        self.title
            .as_deref()
            .or(self
                .path
                .as_deref()
                .and_then(|p| std::path::Path::new(p).file_stem().and_then(|s| s.to_str())))
            .unwrap_or("Unknown Title")
    }

    /// Returns the effective album artist (album_artist falling back to artist).
    pub fn effective_album_artist(&self) -> &str {
        self.album_artist
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(self.artist.as_deref())
            .unwrap_or("Unknown Artist")
    }

    /// Duration in seconds (f64 for UI display).
    pub fn duration_secs(&self) -> f64 {
        self.length_nanosec
            .map(|ns| ns as f64 / 1_000_000_000.0)
            .unwrap_or(0.0)
    }

    /// Returns true if this song and `other` belong to the same album or are CUE sheet siblings (#79).
    pub fn is_same_album_or_cue_sibling(&self, other: &Song) -> bool {
        if let (Some(c1), Some(c2)) = (&self.cue_path, &other.cue_path) {
            if !c1.is_empty() && c1 == c2 {
                return true;
            }
        }
        if let (Some(a1), Some(a2)) = (&self.album, &other.album) {
            if !a1.is_empty() && a1.eq_ignore_ascii_case(a2) {
                let artist1 = self.effective_album_artist();
                let artist2 = other.effective_album_artist();
                if !artist1.is_empty() && artist1.eq_ignore_ascii_case(artist2) {
                    return true;
                }
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Playlist models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlaylistItemType {
    #[default]
    Song = 0,
    Stream = 1,
    StreamingService = 2,
}

/// A single item in a playlist. UUID-keyed for stable undo/redo tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: i64,
    pub playlist_id: i64,
    pub position: i32,
    /// Stable UUID — survives reorders. Used by undo/redo stack.
    pub uuid: String,
    pub item_type: PlaylistItemType,
    /// For Song items: the full Song data joined in.
    pub song: Option<Song>,
    /// For stream/service items without a local song_id.
    pub url: Option<String>,
    pub stream_url: Option<String>,
    /// JSON blob for service-specific metadata (streaming services).
    pub additional_metadata: Option<String>,
}

impl PlaylistItem {
    pub fn new_song(playlist_id: i64, position: i32, song: Song) -> Self {
        Self {
            id: 0,
            playlist_id,
            position,
            uuid: Uuid::new_v4().to_string(),
            item_type: PlaylistItemType::Song,
            song: Some(song),
            url: None,
            stream_url: None,
            additional_metadata: None,
        }
    }

    pub fn new_stream(playlist_id: i64, position: i32, url: String) -> Self {
        Self {
            id: 0,
            playlist_id,
            position,
            uuid: Uuid::new_v4().to_string(),
            item_type: PlaylistItemType::Stream,
            song: None,
            url: Some(url),
            stream_url: None,
            additional_metadata: None,
        }
    }
}

/// Where a play was initiated from, recorded alongside each scrobble so
/// "Recently Played" can reflect what the user actually clicked into
/// (an album, a playlist, or an individual song) rather than a heuristic.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlayContext {
    Song,
    Album {
        album: String,
        album_artist: Option<String>,
    },
    Playlist {
        playlist_id: i64,
    },
}

/// The bias used to select tracks when (re)populating a genre/decade
/// auto-playlist or a dynamic Smart Playlist's tracks — see #120. Tab order
/// in the UI, left to right: All, Favourites, Familiar, Discover, Deep Cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum QueuePopulationMode {
    /// Full scope, uniformly randomized (not a deterministic top-N block).
    #[default]
    All,
    /// Biased toward the user's own rating (`rating >= 4`).
    Favourites,
    /// Biased toward higher playcount / more recent lastplayed.
    Familiar,
    /// Biased toward lesser-played (but not never-played) tracks.
    Discover,
    /// Never or almost never played tracks (`playcount = 0 OR lastplayed IS NULL`).
    DeepCuts,
}

impl QueuePopulationMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueuePopulationMode::All => "all",
            QueuePopulationMode::Favourites => "favourites",
            QueuePopulationMode::Familiar => "familiar",
            QueuePopulationMode::Discover => "discover",
            QueuePopulationMode::DeepCuts => "deep_cuts",
        }
    }
}

impl From<&str> for QueuePopulationMode {
    fn from(s: &str) -> Self {
        match s {
            "favourites" => QueuePopulationMode::Favourites,
            "familiar" => QueuePopulationMode::Familiar,
            "discover" => QueuePopulationMode::Discover,
            "deep_cuts" => QueuePopulationMode::DeepCuts,
            _ => QueuePopulationMode::All,
        }
    }
}

/// A named playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub dynamic_enabled: bool,
    pub dynamic_spec: Option<String>, // JSON-serialized smart playlist spec
    #[serde(default)]
    pub population_mode: QueuePopulationMode, // Queue population bias (#120)
    pub last_played_row: Option<i32>,
    pub created: i64,
    pub updated: i64,
    pub track_count: i32, // joined field, not stored directly
    /// Whether this row is the app's single built-in Queue playlist. Computed
    /// from the row (not stored) so the "playlist named Queue" convention
    /// lives only in `Playlist::is_queue_row`; everything else — frontend
    /// included — branches on this flag instead of matching the name.
    #[serde(default)]
    pub is_queue: bool,
}

impl Playlist {
    /// The single definition of what makes a row "the Queue".
    pub fn is_queue_row(name: &str, dynamic_enabled: bool) -> bool {
        !dynamic_enabled && name.trim().eq_ignore_ascii_case("queue")
    }

    /// Maps the canonical 9-column playlist SELECT: id, name,
    /// dynamic_enabled, dynamic_spec, population_mode, last_played_row,
    /// created, updated, track_count.
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let name: String = row.get(1)?;
        let dynamic_enabled: bool = row.get(2)?;
        let is_queue = Self::is_queue_row(&name, dynamic_enabled);
        Ok(Playlist {
            id: row.get(0)?,
            name,
            dynamic_enabled,
            dynamic_spec: row.get(3)?,
            population_mode: QueuePopulationMode::from(
                row.get::<_, Option<String>>(4)?
                    .unwrap_or_default()
                    .as_str(),
            ),
            last_played_row: row.get(5)?,
            created: row.get(6)?,
            updated: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
            track_count: row.get(8)?,
            is_queue,
        })
    }
}

// ---------------------------------------------------------------------------
// Playback state models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShuffleMode {
    #[default]
    Off,
    All,
    InsideAlbum,
    Albums,
    Artists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    Track,
    Album,
    Playlist,
    Intro,
}

/// Current playback state snapshot, sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlaybackState {
    pub state: PlayState,
    pub current_song: Option<Song>,
    pub playlist_id: Option<i64>,
    pub playlist_item_uuid: Option<String>,
    pub position_nanosec: i64,
    pub volume: f32, // 0.0–1.0
    pub shuffle_mode: ShuffleMode,
    pub repeat_mode: RepeatMode,
    pub stop_after_current: bool,
    /// Where the currently applied loudness-normalization gain came from
    /// (#77), for a player-bar indicator.
    pub loudness_source: LoudnessGainSource,
    /// The applied gain in dB, when normalization is active for this track.
    pub loudness_gain_db: Option<f32>,
    /// How many playlist items remain after the current track.
    /// Used by the frontend Auto-Play refill logic (#26).
    #[serde(default)]
    pub remaining_playlist_items: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

// ---------------------------------------------------------------------------
// Loudness normalization (#77) — EBU R128 analysis with ReplayGain fallback
// ---------------------------------------------------------------------------

/// Which ReplayGain value to prefer when no R128 analysis is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoudnessMode {
    #[default]
    Track,
    Album,
}

impl LoudnessMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            LoudnessMode::Track => "track",
            LoudnessMode::Album => "album",
        }
    }
}

impl From<&str> for LoudnessMode {
    fn from(s: &str) -> Self {
        match s {
            "album" => LoudnessMode::Album,
            _ => LoudnessMode::Track,
        }
    }
}

/// Persisted loudness-normalization settings (`loudness_settings` table).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LoudnessSettings {
    pub enabled: bool,
    pub target_lufs: f32,
    pub mode: LoudnessMode,
    /// Gain applied (in dB) when a track has neither R128 analysis nor a
    /// ReplayGain tag. Defaults to a conservative negative value to avoid
    /// clipping unanalyzed, potentially loud tracks.
    pub fallback_gain_db: f32,
}

impl Default for LoudnessSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            target_lufs: -18.0,
            mode: LoudnessMode::Track,
            fallback_gain_db: -6.0,
        }
    }
}

/// Background R128 analysis progress, emitted as `loudness-analysis-progress`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoudnessAnalysisProgress {
    pub analyzed: u64,
    pub remaining: u64,
}

/// Where the currently applied loudness gain came from, for UI display next
/// to the currently playing track (e.g. an "R128"/"RG" badge in the player
/// bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoudnessGainSource {
    /// Normalization is off, or nothing is playing.
    #[default]
    Disabled,
    /// Gain computed from this track's own R128 analysis.
    Analyzed,
    /// Gain derived from a ReplayGain tag (no R128 analysis yet).
    ReplayGain,
    /// Neither analysis nor a tag is available — the fixed fallback gain.
    Fallback,
}

// ---------------------------------------------------------------------------
// Playback Fades & Crossfade models (#79)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FadeSettings {
    pub fade_pause_enabled: bool,
    pub fade_pause_duration_ms: u32,
    pub crossfade_manual_enabled: bool,
    pub crossfade_manual_duration_ms: u32,
    pub crossfade_auto_enabled: bool,
    pub crossfade_auto_duration_secs: f32,
    pub crossfade_suppress_same_album: bool,
}

impl Default for FadeSettings {
    fn default() -> Self {
        Self {
            fade_pause_enabled: true,
            fade_pause_duration_ms: 300,
            crossfade_manual_enabled: true,
            crossfade_manual_duration_ms: 1000,
            crossfade_auto_enabled: false,
            crossfade_auto_duration_secs: 3.0,
            crossfade_suppress_same_album: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Collection / library models
// ---------------------------------------------------------------------------

/// A watched music directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicDirectory {
    pub id: i64,
    pub path: String,
    pub subdirs: bool,
    #[serde(default)]
    pub is_available: bool,
}

/// Result of pruning missing/unavailable songs from the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneResult {
    pub deleted_songs: usize,
    pub removed_folders: usize,
    pub merged_duplicates: usize,
}

/// Scan progress event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub phase: ScanPhase,
    pub scanned: u64,
    pub total: u64,
    pub current_path: Option<String>,
    /// True when this scan was triggered internally by the file watcher as a
    /// catch-up/safety-net rescan (recovering from an overflow, or picking up
    /// a newly-appeared directory) rather than an explicit user action. The
    /// frontend skips the "songs added" completion toast in that case, since
    /// the watcher's own batch-processing events already cover the same
    /// filesystem activity with a per-file-accurate count (#233).
    pub silent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanPhase {
    Discovering,
    ReadingTags,
    Updating,
    Done,
}

/// File-watcher batch lifecycle event payload (#233). The watcher debounces
/// rapid-fire filesystem events into a single batch (see `start_watcher` in
/// `collection.rs`); these events let the frontend collapse that batch into
/// one progress toast instead of one per file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProgress {
    pub batch_id: u64,
    pub current_count: usize,
    pub total_count: usize,
    pub phase: BatchPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchPhase {
    Removing,
    Adding,
    Done,
}

/// Summary stats for the library.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryStats {
    pub total_songs: i64,
    pub total_artists: i64,
    pub total_albums: i64,
    pub total_duration_nanosec: i64,
    pub total_filesize_bytes: i64,
}

/// Represents an album summary on the Home page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumItem {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<i32>,
    pub track_count: i32,
    pub disc_count: i32,
    pub art_embedded: bool,
    pub art_automatic: Option<String>,
    pub art_manual: Option<String>,
    pub genre: Option<String>,
    pub sample_song_id: Option<i64>,
    /// Independent album rating (-1 = unrated, else 0.5–5.0). Populated by callers
    /// that have a DB connection handy (see `attach_album_ratings`); defaults to
    /// unrated at construction time.
    pub rating: f32,
    /// Sum of every track's `length_nanosec`, used to classify a release as an
    /// EP (under 30 minutes) vs. an Album — the Home-carousel construction sites
    /// don't have this aggregate handy and default it to 0, which is harmless
    /// since those `AlbumItem`s aren't run through release-category logic.
    pub total_duration_nanosec: i64,
}

/// Represents a dynamic item in the Home curation carousels (a Song, an Album, or a Playlist).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HomeItem {
    Song { song: Box<Song> },
    Album { album: AlbumItem },
    Playlist { playlist: Playlist },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_multi_value_splits_trims_and_dedupes() {
        assert_eq!(
            parse_multi_value("Rock; Jazz Fusion; Live"),
            vec!["Rock", "Jazz Fusion", "Live"]
        );
        // Whitespace-only separators still split; empties dropped.
        assert_eq!(parse_multi_value("Rock;;  Jazz "), vec!["Rock", "Jazz"]);
        // Case-insensitive dedup, first-seen casing wins.
        assert_eq!(parse_multi_value("Rock; rock; ROCK"), vec!["Rock"]);
        // A single value with no delimiter round-trips unchanged.
        assert_eq!(parse_multi_value("Metal"), vec!["Metal"]);
        assert_eq!(parse_multi_value(""), Vec::<String>::new());
        assert_eq!(parse_multi_value("   "), Vec::<String>::new());
    }

    #[test]
    fn test_parse_multi_value_does_not_split_on_slash() {
        // '/' is a real-world legacy multi-value join convention (TPE1,
        // Picard's ID3v2.3 fallback) but is deliberately *not* treated as a
        // splitting delimiter here, since it collides with band names that
        // legitimately contain a slash — "AC/DC" must survive intact rather
        // than becoming two artists, "AC" and "DC".
        assert_eq!(parse_multi_value("AC/DC"), vec!["AC/DC"]);
        // A real multi-value string still splits correctly on ';', with the
        // slash-containing value passing through as one of the items.
        assert_eq!(
            parse_multi_value("David Guetta; AC/DC"),
            vec!["David Guetta", "AC/DC"]
        );
    }

    #[test]
    fn test_join_multi_value() {
        assert_eq!(
            join_multi_value(&["Rock".to_string(), "Jazz Fusion".to_string()]),
            "Rock; Jazz Fusion"
        );
        assert_eq!(join_multi_value(&[]), "");
        assert_eq!(join_multi_value(&["Metal".to_string()]), "Metal");
    }

    #[test]
    fn test_multi_value_round_trips_through_join_and_parse() {
        let values = vec!["Rock".to_string(), "Jazz Fusion".to_string()];
        assert_eq!(parse_multi_value(&join_multi_value(&values)), values);
    }

    #[test]
    fn test_is_same_album_or_cue_sibling() {
        let s1 = Song {
            album: Some("The Dark Side of the Moon".to_string()),
            artist: Some("Pink Floyd".to_string()),
            ..Default::default()
        };

        let s2 = Song {
            album: Some("The Dark Side of the Moon".to_string()),
            artist: Some("Pink Floyd".to_string()),
            ..Default::default()
        };

        assert!(s1.is_same_album_or_cue_sibling(&s2));

        let s3 = Song {
            album: Some("Abbey Road".to_string()),
            artist: Some("The Beatles".to_string()),
            ..Default::default()
        };

        assert!(!s1.is_same_album_or_cue_sibling(&s3));
    }

    #[test]
    fn test_cue_sibling_match() {
        let s1 = Song {
            cue_path: Some("/music/album.cue".to_string()),
            ..Default::default()
        };

        let s2 = Song {
            cue_path: Some("/music/album.cue".to_string()),
            ..Default::default()
        };

        assert!(s1.is_same_album_or_cue_sibling(&s2));
    }
}
