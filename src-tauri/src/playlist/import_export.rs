//! Playlist import (`.m3u`/`.pls`/etc.) and export. Split out of `playlist.rs`
//! (#577 item 18) — a second `impl PlaylistManager` block alongside the one
//! in `playlist.rs` that owns CRUD/auto-playlist-sync/dynamic-playlist logic.

use super::PlaylistManager;
use crate::models::Playlist;
use anyhow::{anyhow, Result};
use rusqlite::params;
use uuid::Uuid;

/// Normalize a path to the same form used for `songs.path` in the DB, so an
/// imported playlist's track paths (from `.m3u`/`.pls`, often relative or
/// with `./`/`../` segments) can be matched against already-scanned library
/// songs by exact string comparison. Prefers `canonicalize` (resolves
/// symlinks too, and strips Windows' `\\?\` verbatim prefix so the result
/// matches how `collection.rs` stores paths); if the path doesn't exist on
/// disk yet, falls back to lexically collapsing `.`/`..` components without
/// touching the filesystem.
fn clean_path<P: AsRef<std::path::Path>>(path: P) -> std::path::PathBuf {
    let p = path.as_ref();
    if let Ok(canonical) = std::fs::canonicalize(p) {
        let s = canonical.to_string_lossy();
        #[cfg(windows)]
        let cleaned_s = match s.strip_prefix(r"\\?\") {
            Some(stripped) => stripped.to_string(),
            None => s.to_string(),
        };
        #[cfg(not(windows))]
        let cleaned_s = s.to_string();

        return std::path::PathBuf::from(cleaned_s);
    }

    use std::path::Component;
    let mut components = Vec::new();
    for component in p.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                components.pop();
            }
            c => components.push(c),
        }
    }
    components.iter().collect()
}

impl PlaylistManager {
    /// Parse an `.m3u`/`.pls`/etc. playlist file (format detected from the
    /// extension) and create a new playlist from it. Each track is matched
    /// against the library first by cleaned path (relative entries resolved
    /// against the playlist file's own directory), then by title+artist(+
    /// duration) tag metadata as a fallback. Tracks that still don't match
    /// any library song are added anyway with `song_id = NULL`, preserving
    /// their title/artist/album/path in `additional_metadata` — nothing
    /// from the source file is silently dropped.
    pub fn import_playlist<P: AsRef<std::path::Path>>(&mut self, file_path: P) -> Result<Playlist> {
        use crate::playlist_parsers;

        let path = file_path.as_ref();
        let parsed = playlist_parsers::parse_playlist(path)?;

        let playlist_name = parsed.title.unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Imported Playlist".to_string())
        });

        let playlist = self.create_playlist(&playlist_name)?;
        let base_dir = path.parent();

        let conn = self.db.pool.get()?;

        for (pos, track) in parsed.tracks.iter().enumerate() {
            let mut resolved_path = std::path::PathBuf::from(&track.path_or_url);
            if resolved_path.is_relative() {
                if let Some(base) = base_dir {
                    resolved_path = base.join(&resolved_path);
                }
            }

            let cleaned_path = clean_path(&resolved_path);
            let path_str = cleaned_path.to_string_lossy().to_string();
            let normalized_path_str = path_str.replace('/', "\\");

            // 1. Try matching by exact path or normalized path in database
            let matched_song_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM songs WHERE path = ?1 OR path = ?2 OR LOWER(REPLACE(path, '/', '\\')) = LOWER(?3) LIMIT 1",
                    params![path_str, track.path_or_url, normalized_path_str],
                    |row| row.get(0),
                )
                .ok();

            // Read metadata tags from file if missing from playlist track entry
            let mut title = track.title.clone();
            let mut artist = track.artist.clone();
            let mut album = track.album.clone();

            if (title.is_none() || artist.is_none()) && cleaned_path.is_file() {
                if let Ok(tagged) = lofty::read_from_path(&cleaned_path) {
                    use lofty::file::TaggedFileExt;
                    use lofty::tag::Accessor;
                    let mut candidate_tags: Vec<&lofty::tag::Tag> = Vec::new();
                    if let Some(primary) = tagged.primary_tag() {
                        candidate_tags.push(primary);
                    }
                    for t in tagged.tags() {
                        if !candidate_tags
                            .iter()
                            .any(|existing| std::ptr::eq(*existing, t))
                        {
                            candidate_tags.push(t);
                        }
                    }
                    for tag in candidate_tags {
                        if title.is_none() {
                            title = tag.title().map(|s| s.to_string());
                        }
                        if artist.is_none() {
                            artist = tag.artist().map(|s| s.to_string());
                        }
                        if album.is_none() {
                            album = tag.album().map(|s| s.to_string());
                        }
                    }
                }
            }

            if title.is_none() && cleaned_path.is_file() {
                title = cleaned_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string());
            }

            // 2. Fallback matching by metadata (title, artist, duration +/- 2s)
            let matched_song_id = if matched_song_id.is_some() {
                matched_song_id
            } else if let Some(ref t) = title {
                if let Some(ref a) = artist {
                    if let Some(dur) = track.duration_sec {
                        conn.query_row(
                            "SELECT id FROM songs WHERE LOWER(title) = LOWER(?1) AND LOWER(artist) = LOWER(?2) AND ABS((length_nanosec / 1000000000) - ?3) <= 2 LIMIT 1",
                            params![t, a, dur],
                            |row| row.get(0),
                        ).ok()
                    } else {
                        conn.query_row(
                            "SELECT id FROM songs WHERE LOWER(title) = LOWER(?1) AND LOWER(artist) = LOWER(?2) LIMIT 1",
                            params![t, a],
                            |row| row.get(0),
                        ).ok()
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let uuid = Uuid::new_v4().to_string();

            if let Some(song_id) = matched_song_id {
                conn.execute(
                    "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type, url)
                     VALUES (?1, ?2, ?3, ?4, 0, ?5)",
                    params![playlist.id, song_id, pos as i32, uuid, path_str],
                )?;
            } else {
                // Save unmatched metadata in additional_metadata so track info isn't lost
                let meta = serde_json::json!({
                    "title": title,
                    "artist": artist,
                    "album": album,
                    "path": path_str,
                    "duration_sec": track.duration_sec,
                });
                conn.execute(
                    "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type, url, additional_metadata)
                     VALUES (?1, NULL, ?2, ?3, 0, ?4, ?5)",
                    params![playlist.id, pos as i32, uuid, path_str, meta.to_string()],
                )?;
            }
        }

        Ok(playlist)
    }

    /// Write the playlist's tracks to `export_path` in the format inferred
    /// from its extension (fails if unsupported). `relative` controls
    /// whether track paths are written relative to `export_path`'s
    /// directory or as absolute paths — matters for moving the exported
    /// file to another machine/directory alongside its music.
    pub fn export_playlist<P: AsRef<std::path::Path>>(
        &self,
        playlist_id: i64,
        export_path: P,
        relative: bool,
    ) -> Result<()> {
        use crate::playlist_parsers::{self, ExportTrack, PlaylistFormat};

        let path = export_path.as_ref();
        let format = PlaylistFormat::from_path(path).ok_or_else(|| {
            anyhow!(
                "Unsupported playlist format for export path: {}",
                path.display()
            )
        })?;

        let conn = self.db.pool.get()?;
        let playlist_name: String = conn.query_row(
            "SELECT name FROM playlists WHERE id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )?;

        let items = self.get_playlist_tracks(playlist_id)?;
        let export_tracks: Vec<ExportTrack> = items
            .iter()
            .filter_map(|item| {
                if let Some(ref song) = item.song {
                    let p = if let Some(ref path) = song.path {
                        std::path::Path::new(path)
                    } else {
                        std::path::Path::new(item.url.as_ref()?)
                    };
                    let dur_sec = song.length_nanosec.map(|ns| ns / 1_000_000_000);
                    Some(ExportTrack {
                        path: p,
                        title: song.title.as_deref(),
                        artist: song.artist.as_deref(),
                        album: song.album.as_deref(),
                        duration_sec: dur_sec,
                    })
                } else {
                    item.url.as_ref().map(|url| ExportTrack {
                        path: std::path::Path::new(url),
                        title: None,
                        artist: None,
                        album: None,
                        duration_sec: None,
                    })
                }
            })
            .collect();

        let content = playlist_parsers::export_playlist(
            &playlist_name,
            &export_tracks,
            format,
            path,
            relative,
        )?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup_test_db() -> (Database, std::path::PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_playlist_import_export_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_import_relative_pls_resolution() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        let music_dir = temp_dir.join("Music");
        std::fs::create_dir_all(&music_dir).unwrap();
        let song_file = music_dir.join("song1.mp3");
        std::fs::write(&song_file, b"dummy audio").unwrap();

        let song_path_str = clean_path(&song_file).to_string_lossy().to_string();

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, path) VALUES ('Song One', 'Artist One', ?1)",
                params![song_path_str],
            )
            .unwrap();
        }

        let downloads_dir = temp_dir.join("Downloads");
        std::fs::create_dir_all(&downloads_dir).unwrap();
        let pls_file = downloads_dir.join("playlist.pls");

        let pls_content = "[playlist]\nNumberOfEntries=1\nFile1=../Music/song1.mp3\n".to_string();
        std::fs::write(&pls_file, pls_content).unwrap();

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let imported = manager.import_playlist(&pls_file).unwrap();

        let tracks = manager.get_playlist_tracks(imported.id).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(
            tracks[0].song.as_ref().unwrap().title.as_deref(),
            Some("Song One")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
