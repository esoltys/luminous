//! Wire-protocol clients and response parsers for remote services.
//!
//! Owns the HTTP/XML/JSON protocol clients (Subsonic/OpenSubsonic, WebDAV,
//! ListenBrainz/CritiqueBrainz, multi-provider Lyrics, and MusicBrainz OAuth),
//! authentication token and signature math, and wire response parsing.
//!
//! Must never depend on `tauri` or `rusqlite` (#1767, epic #1762). App-side
//! SQLite persistence, sync reconciliation, and cover art caching remain in
//! `src-tauri`.

pub mod lyrics;
pub mod musicbrainz;
pub mod scrobbler;
pub mod subsonic;
pub mod webdav;
