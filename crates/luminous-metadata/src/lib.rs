//! Audio file tag reading/writing, sidecar metadata (CUE, DR Meter),
//! and playlist format parsers.
//!
//! Must never depend on Tauri, SQLite, reqwest, or Tokio (#1766, epic #1762).

pub mod audio;
pub mod cue;
pub mod dr;
pub mod playlist;
pub mod values;

pub use audio::{
    clear_embedded_art, detect_mp3_vbr, format_error_chain, is_synced_lrc, parse_replaygain_db,
    read_embedded_lyrics, read_embedded_lyrics_from_path, read_tags, AudioFileMetadata,
    AudioFileProperties, TagWriteRequest,
};
pub use cue::{parse_single_file_cue, CueSheet, CueTrack};
pub use dr::{match_tracks, parse as parse_dr_log, DrLog, DrTrackEntry, SongCandidate};
pub use playlist::{
    clean_track_path, export_playlist, parse_playlist, ExportTrack, ParsedPlaylist, ParsedTrack,
    PlaylistFormat,
};
pub use values::{join_multi_value, parse_multi_value, MULTI_VALUE_DELIMITER};
