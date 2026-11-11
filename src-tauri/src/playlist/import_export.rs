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
/// disk yet, or lives on a mapped network drive, falls back to lexically
/// collapsing `.`/`..` components without touching the filesystem.
fn clean_path<P: AsRef<std::path::Path>>(path: P) -> std::path::PathBuf {
    let p = path.as_ref();
    if let Ok(canonical) = std::fs::canonicalize(p) {
        let s = canonical.to_string_lossy();
        #[cfg(windows)]
        let cleaned_s = match s.strip_prefix(r"\\?\UNC\") {
            // A mapped network drive (`Z:\...`) canonicalizes to its UNC
            // target, which never matches the drive-letter form the scanner
            // stored — fall through to the lexical cleanup instead. A path
            // that was already UNC gets its `\\` prefix back.
            Some(unc) if p.to_string_lossy().starts_with(r"\\") => Some(format!(r"\\{unc}")),
            Some(_) => None,
            None => Some(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()),
        };
        #[cfg(not(windows))]
        let cleaned_s = Some(s.to_string());

        if let Some(cleaned_s) = cleaned_s {
            return std::path::PathBuf::from(cleaned_s);
        }
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

/// The library song a playlist track refers to, or `None`. Tries each of
/// `paths` exactly and then separator/case-insensitively, then falls back to
/// title + artist (case-insensitive), requiring the length to be within 2 s
/// of `duration_sec` when that is known. Shared by playlist import and
/// "Find missing tracks", so both resolve a track the same way.
fn match_library_song(
    conn: &rusqlite::Connection,
    paths: &[&str],
    title: Option<&str>,
    artist: Option<&str>,
    duration_sec: Option<i64>,
) -> Option<i64> {
    for path in paths.iter().filter(|p| !p.is_empty()) {
        let by_path = conn
            .query_row(
                "SELECT id FROM songs WHERE path = ?1 OR LOWER(REPLACE(path, '/', '\\')) = LOWER(?2) LIMIT 1",
                params![path, path.replace('/', "\\")],
                |row| row.get(0),
            )
            .ok();
        if by_path.is_some() {
            return by_path;
        }
    }
    let (title, artist) = (title?, artist?);
    match duration_sec {
        Some(dur) => conn.query_row(
            "SELECT id FROM songs WHERE LOWER(title) = LOWER(?1) AND LOWER(artist) = LOWER(?2) AND ABS((length_nanosec / 1000000000) - ?3) <= 2 LIMIT 1",
            params![title, artist, dur],
            |row| row.get(0),
        ),
        None => conn.query_row(
            "SELECT id FROM songs WHERE LOWER(title) = LOWER(?1) AND LOWER(artist) = LOWER(?2) LIMIT 1",
            params![title, artist],
            |row| row.get(0),
        ),
    }
    .ok()
}

/// Keys a missing track's snapshot adds to `additional_metadata` (written by
/// import for unmatched tracks and by the `songs_bd_playlist_items` trigger,
/// see `MIGRATION_82` in db.rs). Removed again when the item is relinked.
const SNAPSHOT_KEYS: &[&str] = &[
    "title",
    "artist",
    "album",
    "path",
    "duration_sec",
    "beginning_nanosec",
];

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
        use luminous_metadata::playlist;

        let path = file_path.as_ref();
        let parsed = playlist::parse_playlist(path)?;

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

            // Read metadata tags from file if missing from playlist track entry
            let mut title = track.title.clone();
            let mut artist = track.artist.clone();
            let mut album = track.album.clone();

            if (title.is_none() || artist.is_none()) && cleaned_path.is_file() {
                if let Ok(meta) = luminous_metadata::audio::read_tags(&cleaned_path) {
                    if title.is_none() {
                        title = meta.title;
                    }
                    if artist.is_none() {
                        artist = meta.artist;
                    }
                    if album.is_none() {
                        album = meta.album;
                    }
                }
            }

            if title.is_none() && cleaned_path.is_file() {
                title = cleaned_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string());
            }

            let matched_song_id = match_library_song(
                &conn,
                &[&path_str, &track.path_or_url],
                title.as_deref(),
                artist.as_deref(),
                track.duration_sec,
            );

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

    /// Relinks `playlist_id`'s missing tracks (items whose library song is
    /// gone, or that import couldn't match) to library songs found by
    /// `match_library_song` from their stored path and tags. Each relinked
    /// item keeps its uuid and position and loses its snapshot keys. Returns
    /// how many were relinked. Items whose path reappears in the library are
    /// relinked automatically on insert; this covers moved or retagged files.
    pub fn relink_missing_items(&self, playlist_id: i64) -> Result<usize> {
        let conn = self.db.pool.get()?;
        let missing: Vec<(i64, Option<String>, Option<String>)> = conn
            .prepare(
                "SELECT id, url, additional_metadata FROM playlist_items
                 WHERE playlist_id = ?1 AND song_id IS NULL AND type = 0",
            )?
            .query_map(params![playlist_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<rusqlite::Result<_>>()?;

        let mut relinked = 0;
        for (item_id, url, meta) in missing {
            let mut meta: serde_json::Map<String, serde_json::Value> = meta
                .and_then(|m| serde_json::from_str(&m).ok())
                .unwrap_or_default();
            let text = |key: &str| meta.get(key).and_then(|v| v.as_str()).map(str::to_owned);
            let (title, artist, snapshot_path) = (text("title"), text("artist"), text("path"));
            let paths: Vec<&str> = [url.as_deref(), snapshot_path.as_deref()]
                .into_iter()
                .flatten()
                .collect();
            let duration = meta.get("duration_sec").and_then(|v| v.as_i64());

            let Some(song_id) =
                match_library_song(&conn, &paths, title.as_deref(), artist.as_deref(), duration)
            else {
                continue;
            };
            for key in SNAPSHOT_KEYS {
                meta.remove(*key);
            }
            let meta = (!meta.is_empty()).then(|| serde_json::Value::Object(meta).to_string());
            conn.execute(
                "UPDATE playlist_items SET song_id = ?1, additional_metadata = ?2 WHERE id = ?3",
                params![song_id, meta, item_id],
            )?;
            relinked += 1;
        }
        if relinked > 0 {
            self.touch_updated(&conn, playlist_id)?;
        }
        Ok(relinked)
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
        use luminous_metadata::playlist::{self, ExportTrack, PlaylistFormat};

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
        // `get_playlist_tracks` takes its own connection.
        drop(conn);

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

        let content =
            playlist::export_playlist(&playlist_name, &export_tracks, format, path, relative)?;

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

    fn setup_test_db() -> (tempfile::TempDir, Database) {
        let temp_dir = tempfile::Builder::new()
            .prefix("luminous_playlist_import_export_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        (temp_dir, db)
    }

    #[test]
    fn test_import_relative_pls_resolution() {
        let (temp_dir, db) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        let music_dir = temp_dir.path().join("Music");
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

        let downloads_dir = temp_dir.path().join("Downloads");
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
    }

    #[test]
    fn test_match_library_song_prefers_path_then_tags_within_two_seconds() {
        let (_dir, db) = setup_test_db();
        let conn = db.pool.get().unwrap();
        conn.execute_batch(
            "INSERT INTO songs (id, title, artist, path, length_nanosec) VALUES
                 (1, 'Song', 'Band', 'C:/Music/a.flac', 200000000000),
                 (2, 'Other', 'Band', 'C:/Music/b.flac', 100000000000);",
        )
        .unwrap();
        // Separator/case-insensitive path wins over contradicting tags.
        assert_eq!(
            match_library_song(
                &conn,
                &["c:\\music\\B.FLAC"],
                Some("Song"),
                Some("Band"),
                None
            ),
            Some(2)
        );
        assert_eq!(
            match_library_song(
                &conn,
                &["/gone.flac"],
                Some("song"),
                Some("BAND"),
                Some(201)
            ),
            Some(1)
        );
        assert_eq!(
            match_library_song(
                &conn,
                &["/gone.flac"],
                Some("Song"),
                Some("Band"),
                Some(210)
            ),
            None
        );
        assert_eq!(
            match_library_song(&conn, &["/gone.flac"], Some("Song"), None, None),
            None
        );
    }

    #[test]
    fn test_relink_missing_items_matches_moved_song_by_tags_and_keeps_other_keys() {
        let (_dir, db) = setup_test_db();
        let db = std::sync::Arc::new(db);
        let manager = PlaylistManager::new(db.clone()).unwrap();
        let conn = db.pool.get().unwrap();
        conn.execute_batch(
            "INSERT INTO playlists (id, name, dynamic_enabled) VALUES (50, 'Mine', 0);
             INSERT INTO songs (id, title, artist, path, length_nanosec) VALUES
                 (1, 'Song', 'Band', '/old/a.flac', 200000000000);
             INSERT INTO playlist_items (playlist_id, song_id, position, uuid) VALUES
                 (50, 1, 0, 'moved'),
                 (50, NULL, 1, 'unknown');
             DELETE FROM songs WHERE id = 1;
             INSERT INTO songs (id, title, artist, path, length_nanosec) VALUES
                 (2, 'Song', 'Band', '/new/a.flac', 200000000000);",
        )
        .unwrap();
        conn.execute(
            "UPDATE playlist_items SET additional_metadata = json_set(additional_metadata, '$.autoContinue', json('true'))
             WHERE uuid = 'moved'",
            [],
        )
        .unwrap();

        assert_eq!(manager.relink_missing_items(50).unwrap(), 1);

        let (song_id, meta, pos): (Option<i64>, Option<String>, i32) = conn
            .query_row(
                "SELECT song_id, additional_metadata, position FROM playlist_items WHERE uuid = 'moved'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(song_id, Some(2));
        assert_eq!(meta.as_deref(), Some("{\"autoContinue\":true}"));
        assert_eq!(pos, 0);
        assert_eq!(
            manager.relink_missing_items(50).unwrap(),
            0,
            "nothing left that can match"
        );
    }
}
