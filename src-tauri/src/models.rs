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
    pub mood: Option<String>,
    pub initial_key: Option<String>,

    // Audio properties (durations in nanoseconds)
    pub length_nanosec: Option<i64>,
    pub beginning_nanosec: i64, // CUE sheet start (0 for normal files)
    pub end_nanosec: i64,       // CUE sheet end (0 for normal files)
    pub bitrate: Option<i32>,
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
}

// ---------------------------------------------------------------------------
// Playlist models
// ---------------------------------------------------------------------------

/// Item type within a playlist.
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

/// A named playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub dynamic_enabled: bool,
    pub dynamic_spec: Option<String>, // JSON-serialized smart playlist spec
    pub last_played_row: Option<i32>,
    pub created: i64,
    pub track_count: i32, // joined field, not stored directly
}

// ---------------------------------------------------------------------------
// Playback state models
// ---------------------------------------------------------------------------

/// Shuffle mode.
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

/// Repeat mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    Track,
    Album,
    Playlist,
    OneByOne,
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

// ---------------------------------------------------------------------------
// Collection / library models
// ---------------------------------------------------------------------------

/// A watched music directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicDirectory {
    pub id: i64,
    pub path: String,
    pub subdirs: bool,
}

/// Scan progress event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub phase: ScanPhase,
    pub scanned: u64,
    pub total: u64,
    pub current_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanPhase {
    Discovering,
    ReadingTags,
    Updating,
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
    pub art_embedded: bool,
    pub art_automatic: Option<String>,
    pub art_manual: Option<String>,
}

/// Represents a dynamic item in the Home curation carousels (either a Song or an Album).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HomeItem {
    Song { song: Song },
    Album { album: AlbumItem },
}
