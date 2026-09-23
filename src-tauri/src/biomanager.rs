//! Artist/album bio sidecar files (`artist.md`, `album.md`).
//!
//! Bios follow the same folder convention as artwork (#98): `album.md` sits
//! in the album directory (a song's parent folder), right where `cover.jpg`
//! lives; `artist.md` sits one level up, right where `artist.jpg` lives. That
//! means a curated bio travels with the library folder itself — copy, sync,
//! or share the folder and another Luminous instance (or a human with a text
//! editor) picks it up the same way it already picks up folder artwork.
//!
//! The `artist_profiles`/`album_profiles` DB tables remain the fast-lookup
//! cache the UI reads from; these files are the portable source callers
//! mirror writes to and adopt from when the DB cache is empty.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub const ARTIST_BIO_FILENAME: &str = "artist.md";
pub const ALBUM_BIO_FILENAME: &str = "album.md";

/// Album directory for a song at `audio_path` — same convention
/// `covermanager` uses for `cover.jpg`.
pub fn album_dir(audio_path: &Path) -> Option<PathBuf> {
    audio_path.parent().map(Path::to_path_buf)
}

/// Artist directory for a song at `audio_path` — the album directory's
/// parent, same convention `covermanager` uses for `artist.jpg`.
pub fn artist_dir(audio_path: &Path) -> Option<PathBuf> {
    album_dir(audio_path)?.parent().map(Path::to_path_buf)
}

/// Read a bio file's contents (trimmed), or `None` if it's missing, empty, or
/// unreadable.
pub fn read_bio(dir: &Path, filename: &str) -> Option<String> {
    let text = fs::read_to_string(dir.join(filename)).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Write `content` to a bio file in `dir`, creating the directory if it
/// somehow doesn't exist yet.
pub fn write_bio(dir: &Path, filename: &str, content: &str) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(filename), content)?;
    Ok(())
}

/// Remove a bio file, if present. Not an error if it's already gone.
pub fn remove_bio(dir: &Path, filename: &str) -> Result<()> {
    let path = dir.join(filename);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_album_dir_and_artist_dir_walk_up_from_song_path() {
        let song = Path::new("/music/Artist Name/Album Name/01 Track.mp3");
        assert_eq!(
            album_dir(song),
            Some(PathBuf::from("/music/Artist Name/Album Name"))
        );
        assert_eq!(artist_dir(song), Some(PathBuf::from("/music/Artist Name")));
    }

    #[test]
    fn test_read_bio_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_bio(dir.path(), ALBUM_BIO_FILENAME), None);
    }

    #[test]
    fn test_read_bio_blank_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(ARTIST_BIO_FILENAME), "   \n\n  ").unwrap();
        assert_eq!(read_bio(dir.path(), ARTIST_BIO_FILENAME), None);
    }

    #[test]
    fn test_write_then_read_bio_round_trips_trimmed_content() {
        let dir = tempfile::tempdir().unwrap();
        write_bio(dir.path(), ARTIST_BIO_FILENAME, "  A short bio.  \n").unwrap();
        assert_eq!(
            read_bio(dir.path(), ARTIST_BIO_FILENAME),
            Some("A short bio.".to_string())
        );
    }

    #[test]
    fn test_remove_bio_deletes_existing_file_and_is_noop_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        write_bio(dir.path(), ALBUM_BIO_FILENAME, "notes").unwrap();
        assert!(dir.path().join(ALBUM_BIO_FILENAME).exists());
        remove_bio(dir.path(), ALBUM_BIO_FILENAME).unwrap();
        assert!(!dir.path().join(ALBUM_BIO_FILENAME).exists());
        remove_bio(dir.path(), ALBUM_BIO_FILENAME).unwrap();
    }
}
