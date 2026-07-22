//! Playlist manager — CRUD, undo/redo, UUID-keyed items.

use crate::{
    collection::CollectionScanner,
    db::Database,
    models::{Playlist, PlaylistItem, PlaylistItemType, Song},
};
use anyhow::{anyhow, Result};
use rusqlite::{params, OptionalExtension};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Undo/Redo stack operations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ClearedItem {
    pub uuid: String,
    pub position: i32,
    pub song_id: Option<i64>,
    pub item_type: i32,
    pub url: Option<String>,
    pub stream_url: Option<String>,
    pub additional_metadata: Option<String>,
}

#[derive(Debug, Clone)]
enum PlaylistOp {
    Insert {
        playlist_id: i64,
        items: Vec<(i32, i64)>, // (position, song_id)
    },
    Remove {
        playlist_id: i64,
        items: Vec<ClearedItem>,
    },
    Clear {
        playlist_id: i64,
        items: Vec<ClearedItem>,
    },
    Move {
        playlist_id: i64,
        from: i32,
        to: i32,
    },
    BatchMove {
        playlist_id: i64,
        moves: Vec<(String, i32, i32)>, // (uuid, old_pos, new_pos)
    },
}

#[derive(Debug)]
pub struct PlaylistManager {
    db: Arc<Database>,
    undo_stack: Vec<PlaylistOp>,
    redo_stack: Vec<PlaylistOp>,
}

pub fn clean_path<P: AsRef<std::path::Path>>(path: P) -> std::path::PathBuf {
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
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            db,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        })
    }

    // -----------------------------------------------------------------------
    // Playlist CRUD
    // -----------------------------------------------------------------------

    pub fn create_playlist(&self, name: &str) -> Result<Playlist> {
        let conn = self.db.pool.get()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO playlists (name, updated) VALUES (?1, ?2)",
            params![name, now],
        )?;
        let id = conn.last_insert_rowid();
        Ok(Playlist {
            id,
            name: name.to_string(),
            dynamic_enabled: false,
            dynamic_spec: None,
            auto_play: false,
            last_played_row: None,
            created: now,
            updated: now,
            track_count: 0,
        })
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE playlists SET name = ?1, updated = ?2 WHERE id = ?3",
            params![name, chrono::Utc::now().timestamp(), id],
        )?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: i64) -> Result<()> {
        let conn = self.db.pool.get()?;
        // Cascade deletes playlist_items too (via FK)
        conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_playlists(&self) -> Result<Vec<Playlist>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.dynamic_enabled, p.dynamic_spec, p.auto_play,
                    p.last_played_row, p.created, p.updated,
                    COUNT(pi.id) as track_count
             FROM playlists p
             LEFT JOIN playlist_items pi ON pi.playlist_id = p.id
             GROUP BY p.id
             ORDER BY p.created",
        )?;
        let playlists = stmt
            .query_map([], |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    dynamic_enabled: row.get(2)?,
                    dynamic_spec: row.get(3)?,
                    auto_play: row.get::<_, Option<bool>>(4)?.unwrap_or(false),
                    last_played_row: row.get(5)?,
                    created: row.get(6)?,
                    updated: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
                    track_count: row.get(8)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(playlists)
    }

    pub fn get_playlists_by_artist(&self, artist: &str) -> Result<Vec<Playlist>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.dynamic_enabled, p.dynamic_spec, p.auto_play,
                    p.last_played_row, p.created, p.updated,
                    (SELECT COUNT(*) FROM playlist_items pi2 WHERE pi2.playlist_id = p.id) as track_count
             FROM playlists p
             WHERE EXISTS (
                 SELECT 1 FROM playlist_items pi
                 JOIN songs s ON s.id = pi.song_id
                 WHERE pi.playlist_id = p.id
                   AND COALESCE(NULLIF(s.album_artist, ''), s.artist) = ?1
                   AND s.unavailable = 0
             )
             ORDER BY p.created",
        )?;
        let playlists = stmt
            .query_map(params![artist], |row| {
                Ok(Playlist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    dynamic_enabled: row.get(2)?,
                    dynamic_spec: row.get(3)?,
                    auto_play: row.get::<_, Option<bool>>(4)?.unwrap_or(false),
                    last_played_row: row.get(5)?,
                    created: row.get(6)?,
                    updated: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
                    track_count: row.get(8)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(playlists)
    }

    /// Regenerates each genre "auto-playlist" — a system-managed `playlists` row
    /// with `dynamic_enabled = 1` and `dynamic_spec` set to the genre name — if
    /// it's missing or its `updated` timestamp is more than 24h old, and prunes
    /// rows for genres no longer present in the library. `updated` doubles as
    /// the "last (re)generated at" timestamp shown in the UI.
    pub fn sync_genre_auto_playlists(&self) -> Result<()> {
        const STALE_AFTER_SECS: i64 = 24 * 60 * 60;

        let scanner = CollectionScanner::new(self.db.clone());
        let genres = scanner.get_library_genres()?;
        let conn = self.db.pool.get()?;
        let now = chrono::Utc::now().timestamp();

        // Prune genre auto-playlists for genres no longer in the library. Only
        // touch rows using the bare-genre-name convention (e.g. "Rock") —
        // anything containing ':' is either a decade auto-playlist (excluded
        // above) or a user-created Smart Playlist rule spec (e.g. "genre:rock"),
        // and must not be swept up here.
        let mut stmt =
            conn.prepare("SELECT id, dynamic_spec FROM playlists WHERE dynamic_enabled = 1 AND dynamic_spec NOT LIKE '%:%'")?;
        let existing: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        for (id, spec) in &existing {
            if !genres.contains(spec) {
                conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
            }
        }

        for genre in &genres {
            let existing_row: Option<(i64, i64, i64)> = conn
                .query_row(
                    "SELECT p.id, COALESCE(p.updated, 0), COUNT(pi.id) FROM playlists p LEFT JOIN playlist_items pi ON pi.playlist_id = p.id WHERE p.dynamic_enabled = 1 AND p.dynamic_spec = ?1 GROUP BY p.id",
                    params![genre],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok();

            // Always check song count first — prune the playlist if it falls below threshold.
            let songs = scanner.get_songs_by_genre(genre, 25)?;
            if songs.len() < 25 {
                if let Some((id, _, _)) = existing_row {
                    conn.execute(
                        "DELETE FROM playlist_items WHERE playlist_id = ?1",
                        params![id],
                    )?;
                    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
                }
                continue;
            }

            let needs_generation = match existing_row {
                None => true,
                Some((_, updated, track_count)) => {
                    now - updated > STALE_AFTER_SECS || track_count != 25
                }
            };
            if !needs_generation {
                continue;
            }

            let playlist_id = match existing_row {
                Some((id, _, _)) => {
                    conn.execute(
                        "UPDATE playlists SET updated = ?1 WHERE id = ?2",
                        params![now, id],
                    )?;
                    conn.execute(
                        "DELETE FROM playlist_items WHERE playlist_id = ?1",
                        params![id],
                    )?;
                    id
                }
                None => {
                    conn.execute(
                        "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec, created, updated, auto_play) VALUES (?1, 1, ?1, ?2, ?2, 1)",
                        params![genre, now],
                    )?;
                    conn.last_insert_rowid()
                }
            };

            for (position, song) in songs.iter().enumerate() {
                conn.execute(
                    "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type) VALUES (?1, ?2, ?3, ?4, 0)",
                    params![playlist_id, song.id, position as i32, Uuid::new_v4().to_string()],
                )?;
            }
        }

        Ok(())
    }

    /// Regenerates each decade "auto-playlist" — a system-managed `playlists` row
    /// with `dynamic_enabled = 1` and `dynamic_spec` set to `decade:<decade>` (e.g. `decade:1980s`) — if
    /// it's missing or its `updated` timestamp is more than 24h old, and prunes
    /// rows for decades no longer present in the library.
    pub fn sync_decade_auto_playlists(&self) -> Result<()> {
        const STALE_AFTER_SECS: i64 = 24 * 60 * 60;

        let scanner = CollectionScanner::new(self.db.clone());
        let decades = scanner.get_library_decades()?;
        let conn = self.db.pool.get()?;
        let now = chrono::Utc::now().timestamp();

        // Prune decade auto-playlists for decades no longer in the library.
        let mut stmt =
            conn.prepare("SELECT id, dynamic_spec FROM playlists WHERE dynamic_enabled = 1 AND dynamic_spec LIKE 'decade:%'")?;
        let existing: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        for (id, spec) in &existing {
            let decade = spec.strip_prefix("decade:").unwrap_or(spec);
            if !decades.contains(&decade.to_string()) {
                conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
            }
        }

        for decade in &decades {
            let spec = format!("decade:{}", decade);
            let existing_row: Option<(i64, i64, i64)> = conn
                .query_row(
                    "SELECT p.id, COALESCE(p.updated, 0), COUNT(pi.id) FROM playlists p LEFT JOIN playlist_items pi ON pi.playlist_id = p.id WHERE p.dynamic_enabled = 1 AND p.dynamic_spec = ?1 GROUP BY p.id",
                    params![spec],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok();

            // Always check song count first — prune the playlist if it falls below threshold.
            let songs = scanner.get_songs_by_decade(decade, 25)?;
            if songs.len() < 25 {
                if let Some((id, _, _)) = existing_row {
                    conn.execute(
                        "DELETE FROM playlist_items WHERE playlist_id = ?1",
                        params![id],
                    )?;
                    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
                }
                continue;
            }

            let needs_generation = match existing_row {
                None => true,
                Some((_, updated, track_count)) => {
                    now - updated > STALE_AFTER_SECS || track_count != 25
                }
            };
            if !needs_generation {
                continue;
            }

            let playlist_id = match existing_row {
                Some((id, _, _)) => {
                    conn.execute(
                        "UPDATE playlists SET updated = ?1 WHERE id = ?2",
                        params![now, id],
                    )?;
                    conn.execute(
                        "DELETE FROM playlist_items WHERE playlist_id = ?1",
                        params![id],
                    )?;
                    id
                }
                None => {
                    conn.execute(
                        "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec, created, updated, auto_play) VALUES (?1, 1, ?2, ?3, ?3, 1)",
                        params![decade, spec, now],
                    )?;
                    conn.last_insert_rowid()
                }
            };

            for (position, song) in songs.iter().enumerate() {
                conn.execute(
                    "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type) VALUES (?1, ?2, ?3, ?4, 0)",
                    params![playlist_id, song.id, position as i32, Uuid::new_v4().to_string()],
                )?;
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Auto-Play (dynamic refill) — #26
    // -----------------------------------------------------------------------

    /// Persist the `auto_play` flag for a playlist row.
    pub fn set_playlist_auto_play(&self, id: i64, auto_play: bool) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE playlists SET auto_play = ?1 WHERE id = ?2",
            params![auto_play, id],
        )?;
        Ok(())
    }

    /// Populate/refresh tracks for any dynamic playlist based on its `dynamic_spec`.
    pub fn populate_dynamic_playlist(&mut self, playlist_id: i64) -> Result<()> {
        let conn = self.db.pool.get()?;
        let spec: Option<String> = conn
            .query_row(
                "SELECT dynamic_spec FROM playlists WHERE id = ?1 AND dynamic_enabled = 1",
                params![playlist_id],
                |row| row.get(0),
            )
            .optional()?;

        let spec = match spec {
            Some(s) if !s.trim().is_empty() => s,
            _ => return Ok(()),
        };

        let scanner = CollectionScanner::new(self.db.clone());
        let songs = if let Some(decade) = spec.strip_prefix("decade:") {
            scanner.get_songs_by_decade(decade, 100)?
        } else if !spec.contains(':') {
            // Bare-name convention: a system genre auto-playlist, not a Smart
            // Playlist rule spec (which always contains a "field:" rule).
            scanner.get_songs_by_genre(&spec, 100)?
        } else {
            let query = spec.replace(';', " ");
            scanner.search_songs(&query, 100)?
        };

        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE playlists SET updated = ?1 WHERE id = ?2",
            params![now, playlist_id],
        )?;

        conn.execute(
            "DELETE FROM playlist_items WHERE playlist_id = ?1",
            params![playlist_id],
        )?;

        for (position, song) in songs.iter().enumerate() {
            conn.execute(
                "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type) VALUES (?1, ?2, ?3, ?4, 0)",
                params![playlist_id, song.id, position as i32, Uuid::new_v4().to_string()],
            )?;
        }

        Ok(())
    }

    /// Update the `dynamic_spec` and `dynamic_enabled` fields for a playlist row, and populate its matching songs.
    pub fn set_playlist_dynamic_spec(&mut self, id: i64, spec: &str) -> Result<()> {
        let conn = self.db.pool.get()?;
        let enabled = !spec.trim().is_empty();
        conn.execute(
            "UPDATE playlists SET dynamic_spec = ?1, dynamic_enabled = ?2 WHERE id = ?3",
            params![spec, enabled, id],
        )?;
        if enabled {
            self.populate_dynamic_playlist(id)?;
        }
        Ok(())
    }

    /// Returns songs that match the dynamic spec of playlist `id` but are NOT
    /// yet present in its `playlist_items`.  Used for the Auto-Play refill
    /// path — up to `limit` new songs are returned in random order so each
    /// batch feels fresh.
    pub fn get_auto_playlist_refill_songs(
        &self,
        playlist_id: i64,
        dynamic_spec: &str,
        limit: usize,
    ) -> Result<Vec<Song>> {
        let scanner = CollectionScanner::new(self.db.clone());

        // Parse spec prefix to determine filter type
        let songs: Vec<Song> = if let Some(decade) = dynamic_spec.strip_prefix("decade:") {
            scanner.get_songs_by_decade(decade, (limit * 4) as i64)?
        } else if !dynamic_spec.contains(':') {
            // Bare-name convention: a system genre auto-playlist, not a Smart
            // Playlist rule spec (which always contains a "field:" rule).
            scanner.get_songs_by_genre(dynamic_spec, (limit * 4) as i64)?
        } else {
            let query = dynamic_spec.replace(';', " ");
            scanner.search_songs(&query, 100)?
        };

        // Collect song IDs already in the playlist so we can exclude them
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT song_id FROM playlist_items WHERE playlist_id = ?1 AND song_id IS NOT NULL",
        )?;
        let existing_ids: std::collections::HashSet<i64> = stmt
            .query_map(params![playlist_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let new_songs: Vec<Song> = songs
            .into_iter()
            .filter(|s| !existing_ids.contains(&s.id))
            .take(limit)
            .collect();

        Ok(new_songs)
    }

    /// Force-regenerates a dynamic/auto playlist's tracks (e.g. when user clicks
    /// the "Refresh" button in the auto-playlist header), replacing its contents
    /// with a fresh selection of matching songs from the library.
    pub fn refresh_auto_playlist(&mut self, playlist_id: i64) -> Result<()> {
        self.populate_dynamic_playlist(playlist_id)
    }

    // -----------------------------------------------------------------------
    // Import & Export
    // -----------------------------------------------------------------------

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
                    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
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
                    } else if let Some(ref url) = item.url {
                        std::path::Path::new(url)
                    } else {
                        return None;
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

    // -----------------------------------------------------------------------
    // Playlist item operations
    // -----------------------------------------------------------------------

    pub fn get_playlist_tracks(&self, playlist_id: i64) -> Result<Vec<PlaylistItem>> {
        let conn = self.db.pool.get()?;
        Self::get_playlist_tracks_from_conn(&conn, playlist_id)
    }

    pub fn get_playlist_tracks_from_conn(
        conn: &rusqlite::Connection,
        playlist_id: i64,
    ) -> Result<Vec<PlaylistItem>> {
        let mut stmt = conn.prepare(
            "SELECT pi.id, pi.playlist_id, pi.song_id, pi.position,
                         pi.uuid, pi.type, pi.url, pi.stream_url,
                         pi.additional_metadata,
                         -- song fields
                         s.id, s.source, s.filetype, s.path, s.url, s.stream_url,
                         s.title, s.titlesort, s.artist, s.artistsort,
                         s.album, s.albumsort, s.album_artist, s.album_artist_sort,
                         s.composer, s.composersort, s.performer, s.performersort,
                         s.grouping, s.comment, s.lyrics,
                         s.track, s.disc, s.year, s.originalyear, s.genre, s.compilation,
                         s.bpm, s.mood, s.initial_key,
                         s.length_nanosec, s.beginning_nanosec, s.end_nanosec,
                         s.bitrate, s.samplerate, s.bitdepth, s.channels,
                         s.filesize, s.mtime, s.rating, s.playcount, s.skipcount,
                         s.lastplayed, s.lastseen, s.art_embedded,
                         s.art_automatic, s.art_manual, s.art_unset,
                         s.unavailable, s.replaygain_track_gain,
                         s.replaygain_album_gain, s.is_vbr, s.is_instrumental
                  FROM playlist_items pi
                  LEFT JOIN songs s ON s.id = pi.song_id
                  WHERE pi.playlist_id = ?1
                  ORDER BY pi.position",
        )?;

        let items = stmt
            .query_map(params![playlist_id], |row| {
                let additional_meta_str: Option<String> = row.get(8)?;

                let song = if row.get::<_, Option<i64>>(2)?.is_some() {
                    Some(Song {
                        id: row.get::<_, Option<i64>>(9)?.unwrap_or(0),
                        source: row.get::<_, i64>(10).map(crate::models::SongSource::from)?,
                        filetype: row.get::<_, i64>(11).map(crate::models::FileType::from)?,
                        path: row.get(12)?,
                        url: row.get(13)?,
                        stream_url: row.get(14)?,
                        title: row.get(15)?,
                        titlesort: row.get(16)?,
                        artist: row.get(17)?,
                        artistsort: row.get(18)?,
                        album: row.get(19)?,
                        albumsort: row.get(20)?,
                        album_artist: row.get(21)?,
                        album_artist_sort: row.get(22)?,
                        composer: row.get(23)?,
                        composersort: row.get(24)?,
                        performer: row.get(25)?,
                        performersort: row.get(26)?,
                        grouping: row.get(27)?,
                        comment: row.get(28)?,
                        lyrics: row.get(29)?,
                        track: row.get(30)?,
                        disc: row.get(31)?,
                        year: row.get(32)?,
                        originalyear: row.get(33)?,
                        genre: row.get(34)?,
                        compilation: row.get(35)?,
                        bpm: row.get(36)?,
                        mood: row.get(37)?,
                        initial_key: row.get(38)?,
                        length_nanosec: row.get(39)?,
                        beginning_nanosec: row.get::<_, Option<i64>>(40)?.unwrap_or(0),
                        end_nanosec: row.get::<_, Option<i64>>(41)?.unwrap_or(0),
                        bitrate: row.get(42)?,
                        samplerate: row.get(43)?,
                        bitdepth: row.get(44)?,
                        channels: row.get(45)?,
                        filesize: row.get(46)?,
                        mtime: row.get(47)?,
                        rating: row.get::<_, Option<f32>>(48)?.unwrap_or(-1.0),
                        playcount: row.get::<_, Option<i32>>(49)?.unwrap_or(0),
                        skipcount: row.get::<_, Option<i32>>(50)?.unwrap_or(0),
                        lastplayed: row.get(51)?,
                        lastseen: row.get(52)?,
                        art_embedded: row.get(53)?,
                        art_automatic: row.get(54)?,
                        art_manual: row.get(55)?,
                        art_unset: row.get(56)?,
                        unavailable: row.get::<_, Option<bool>>(57)?.unwrap_or(false),
                        replaygain_track_gain: row.get(58)?,
                        replaygain_album_gain: row.get(59)?,
                        is_vbr: row.get(60)?,
                        is_instrumental: row.get::<_, Option<bool>>(61)?.unwrap_or(false),
                        ..Default::default()
                    })
                } else if let Some(ref meta_json) = additional_meta_str {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(meta_json) {
                        let title = val
                            .get("title")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let artist = val
                            .get("artist")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let album = val
                            .get("album")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let path = val
                            .get("path")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let dur_sec = val.get("duration_sec").and_then(|v| v.as_i64());
                        Some(Song {
                            title,
                            artist,
                            album,
                            path,
                            length_nanosec: dur_sec.map(|s| s * 1_000_000_000),
                            unavailable: true,
                            ..Default::default()
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok(PlaylistItem {
                    id: row.get(0)?,
                    playlist_id: row.get(1)?,
                    position: row.get(3)?,
                    uuid: row.get(4)?,
                    item_type: PlaylistItemType::Song,
                    song,
                    url: row.get(6)?,
                    stream_url: row.get(7)?,
                    additional_metadata: additional_meta_str,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    pub fn add_songs_to_playlist(&mut self, playlist_id: i64, song_ids: &[i64]) -> Result<()> {
        let conn = self.db.pool.get()?;

        // Get current max position
        let max_pos: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) FROM playlist_items WHERE playlist_id = ?1",
                params![playlist_id],
                |row| row.get(0),
            )
            .unwrap_or(-1);

        let mut positions = Vec::new();
        for (i, &song_id) in song_ids.iter().enumerate() {
            let pos = max_pos + 1 + i as i32;
            let uuid = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type)
                 VALUES (?1, ?2, ?3, ?4, 0)",
                params![playlist_id, song_id, pos, uuid],
            )?;
            positions.push((pos, song_id));
        }
        self.touch_updated(&conn, playlist_id)?;

        self.undo_stack.push(PlaylistOp::Insert {
            playlist_id,
            items: positions,
        });
        self.redo_stack.clear();

        Ok(())
    }

    pub fn remove_from_playlist(&mut self, playlist_id: i64, uuids: &[String]) -> Result<()> {
        let conn = self.db.pool.get()?;
        let mut removed = Vec::new();

        for uuid in uuids {
            let result: Result<ClearedItem, _> = conn.query_row(
                "SELECT uuid, position, song_id, type, url, stream_url, additional_metadata
                 FROM playlist_items WHERE uuid = ?1",
                params![uuid],
                |row| {
                    Ok(ClearedItem {
                        uuid: row.get(0)?,
                        position: row.get(1)?,
                        song_id: row.get(2)?,
                        item_type: row.get(3)?,
                        url: row.get(4)?,
                        stream_url: row.get(5)?,
                        additional_metadata: row.get(6)?,
                    })
                },
            );
            if let Ok(item) = result {
                conn.execute("DELETE FROM playlist_items WHERE uuid = ?1", params![uuid])?;
                removed.push(item);
            }
        }

        // Re-number positions to be contiguous
        self.renumber_positions(&conn, playlist_id)?;
        self.touch_updated(&conn, playlist_id)?;

        self.undo_stack.push(PlaylistOp::Remove {
            playlist_id,
            items: removed,
        });
        self.redo_stack.clear();

        Ok(())
    }

    /// Bumps a playlist's `updated` timestamp to now — called whenever its
    /// contents or name change, so "Updated" sort/display stays accurate.
    fn touch_updated(&self, conn: &rusqlite::Connection, playlist_id: i64) -> Result<()> {
        conn.execute(
            "UPDATE playlists SET updated = ?1 WHERE id = ?2",
            params![chrono::Utc::now().timestamp(), playlist_id],
        )?;
        Ok(())
    }

    fn reorder_item_internal(&self, playlist_id: i64, from: i32, to: i32) -> Result<()> {
        let conn = self.db.pool.get()?;
        if from == to {
            return Ok(());
        }

        let uuid: String = conn.query_row(
            "SELECT uuid FROM playlist_items WHERE playlist_id = ?1 AND position = ?2",
            params![playlist_id, from],
            |row| row.get(0),
        )?;

        if from < to {
            conn.execute(
                "UPDATE playlist_items SET position = position - 1
                 WHERE playlist_id = ?1 AND position > ?2 AND position <= ?3",
                params![playlist_id, from, to],
            )?;
        } else {
            conn.execute(
                "UPDATE playlist_items SET position = position + 1
                 WHERE playlist_id = ?1 AND position >= ?2 AND position < ?3",
                params![playlist_id, to, from],
            )?;
        }

        conn.execute(
            "UPDATE playlist_items SET position = ?1 WHERE uuid = ?2",
            params![to, uuid],
        )?;

        Ok(())
    }

    pub fn reorder_playlist_item(&mut self, playlist_id: i64, from: i32, to: i32) -> Result<()> {
        if from == to {
            return Ok(());
        }

        self.reorder_item_internal(playlist_id, from, to)?;
        let conn = self.db.pool.get()?;
        self.touch_updated(&conn, playlist_id)?;

        self.undo_stack.push(PlaylistOp::Move {
            playlist_id,
            from,
            to,
        });
        self.redo_stack.clear();

        Ok(())
    }

    pub fn reorder_playlist_items_batch(
        &mut self,
        playlist_id: i64,
        from_indices: &[i32],
        to_index: i32,
    ) -> Result<()> {
        if from_indices.is_empty() {
            return Ok(());
        }
        if from_indices.len() == 1 {
            return self.reorder_playlist_item(playlist_id, from_indices[0], to_index);
        }

        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT uuid, position FROM playlist_items WHERE playlist_id = ?1 ORDER BY position",
        )?;
        let items: Vec<(String, i32)> = stmt
            .query_map(params![playlist_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        if items.is_empty() {
            return Ok(());
        }

        let mut valid_from: Vec<usize> = from_indices
            .iter()
            .filter_map(|&idx| {
                if idx >= 0 && (idx as usize) < items.len() {
                    Some(idx as usize)
                } else {
                    None
                }
            })
            .collect();

        valid_from.sort_unstable();
        valid_from.dedup();

        if valid_from.is_empty() {
            return Ok(());
        }

        let from_set: std::collections::HashSet<usize> = valid_from.iter().cloned().collect();

        let moving_items: Vec<(String, i32)> = items
            .iter()
            .enumerate()
            .filter(|(idx, _)| from_set.contains(idx))
            .map(|(_, item)| item.clone())
            .collect();

        let remaining_items: Vec<(String, i32)> = items
            .iter()
            .enumerate()
            .filter(|(idx, _)| !from_set.contains(idx))
            .map(|(_, item)| item.clone())
            .collect();

        let first_from = valid_from[0];
        let target_idx = (to_index as usize).min(items.len().saturating_sub(1));
        let insert_pos = if first_from < target_idx {
            remaining_items
                .iter()
                .filter(|(_, pos)| (*pos as usize) <= target_idx)
                .count()
        } else {
            remaining_items
                .iter()
                .filter(|(_, pos)| (*pos as usize) < target_idx)
                .count()
        };

        let mut new_order = Vec::with_capacity(items.len());
        new_order.extend(remaining_items[..insert_pos].iter().cloned());
        new_order.extend(moving_items);
        new_order.extend(remaining_items[insert_pos..].iter().cloned());

        let mut moves = Vec::new();
        for (new_pos, (uuid, _)) in new_order.iter().enumerate() {
            let new_pos = new_pos as i32;
            if let Some((_, old_pos)) = items.iter().find(|(u, _)| u == uuid) {
                if *old_pos != new_pos {
                    moves.push((uuid.clone(), *old_pos, new_pos));
                }
            }
        }

        if moves.is_empty() {
            return Ok(());
        }

        for (uuid, _, new_pos) in &moves {
            conn.execute(
                "UPDATE playlist_items SET position = ?1 WHERE uuid = ?2 AND playlist_id = ?3",
                params![new_pos, uuid, playlist_id],
            )?;
        }

        self.touch_updated(&conn, playlist_id)?;
        self.undo_stack
            .push(PlaylistOp::BatchMove { playlist_id, moves });
        self.redo_stack.clear();

        Ok(())
    }

    pub fn clear_playlist(&mut self, playlist_id: i64) -> Result<()> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT uuid, position, song_id, type, url, stream_url, additional_metadata
             FROM playlist_items WHERE playlist_id = ?1 ORDER BY position",
        )?;
        let items: Vec<ClearedItem> = stmt
            .query_map(params![playlist_id], |row| {
                Ok(ClearedItem {
                    uuid: row.get(0)?,
                    position: row.get(1)?,
                    song_id: row.get(2)?,
                    item_type: row.get(3)?,
                    url: row.get(4)?,
                    stream_url: row.get(5)?,
                    additional_metadata: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        if items.is_empty() {
            return Ok(());
        }

        conn.execute(
            "DELETE FROM playlist_items WHERE playlist_id = ?1",
            params![playlist_id],
        )?;
        self.touch_updated(&conn, playlist_id)?;

        self.undo_stack
            .push(PlaylistOp::Clear { playlist_id, items });
        self.redo_stack.clear();

        Ok(())
    }

    pub fn undo(&mut self) -> Result<()> {
        let op = self.undo_stack.pop().ok_or(anyhow!("nothing to undo"))?;
        match &op {
            PlaylistOp::Move {
                playlist_id,
                from,
                to,
            } => {
                self.reorder_item_internal(*playlist_id, *to, *from)?;
            }
            PlaylistOp::Insert { playlist_id, items } => {
                let conn = self.db.pool.get()?;
                for (pos, _song_id) in items {
                    conn.execute(
                        "DELETE FROM playlist_items WHERE playlist_id = ?1 AND position = ?2",
                        params![playlist_id, pos],
                    )?;
                }
                self.renumber_positions(&conn, *playlist_id)?;
            }
            PlaylistOp::Remove { playlist_id, items } => {
                let conn = self.db.pool.get()?;
                for item in items {
                    conn.execute(
                        "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type, url, stream_url, additional_metadata)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            playlist_id,
                            item.song_id,
                            item.position,
                            item.uuid,
                            item.item_type,
                            item.url,
                            item.stream_url,
                            item.additional_metadata,
                        ],
                    )?;
                }
                self.renumber_positions(&conn, *playlist_id)?;
            }
            PlaylistOp::Clear { playlist_id, items } => {
                let conn = self.db.pool.get()?;
                for item in items {
                    conn.execute(
                        "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type, url, stream_url, additional_metadata)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            playlist_id,
                            item.song_id,
                            item.position,
                            item.uuid,
                            item.item_type,
                            item.url,
                            item.stream_url,
                            item.additional_metadata,
                        ],
                    )?;
                }
            }
            PlaylistOp::BatchMove { playlist_id, moves } => {
                let conn = self.db.pool.get()?;
                for (uuid, old_pos, _) in moves {
                    conn.execute(
                        "UPDATE playlist_items SET position = ?1 WHERE uuid = ?2 AND playlist_id = ?3",
                        params![old_pos, uuid, playlist_id],
                    )?;
                }
            }
        }
        self.redo_stack.push(op);
        Ok(())
    }

    pub fn redo(&mut self) -> Result<()> {
        let op = self.redo_stack.pop().ok_or(anyhow!("nothing to redo"))?;
        match &op {
            PlaylistOp::Move {
                playlist_id,
                from,
                to,
            } => {
                self.reorder_item_internal(*playlist_id, *from, *to)?;
            }
            PlaylistOp::Insert { playlist_id, items } => {
                let conn = self.db.pool.get()?;
                for (pos, song_id) in items {
                    let uuid = Uuid::new_v4().to_string();
                    conn.execute(
                        "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type)
                         VALUES (?1, ?2, ?3, ?4, 0)",
                        params![playlist_id, song_id, pos, uuid],
                    )?;
                }
                self.renumber_positions(&conn, *playlist_id)?;
            }
            PlaylistOp::Remove { playlist_id, items } => {
                let conn = self.db.pool.get()?;
                for item in items {
                    conn.execute(
                        "DELETE FROM playlist_items WHERE uuid = ?1",
                        params![&item.uuid],
                    )?;
                }
                self.renumber_positions(&conn, *playlist_id)?;
            }
            PlaylistOp::Clear { playlist_id, .. } => {
                let conn = self.db.pool.get()?;
                conn.execute(
                    "DELETE FROM playlist_items WHERE playlist_id = ?1",
                    params![playlist_id],
                )?;
            }
            PlaylistOp::BatchMove { playlist_id, moves } => {
                let conn = self.db.pool.get()?;
                for (uuid, _, new_pos) in moves {
                    conn.execute(
                        "UPDATE playlist_items SET position = ?1 WHERE uuid = ?2 AND playlist_id = ?3",
                        params![new_pos, uuid, playlist_id],
                    )?;
                }
            }
        }
        self.undo_stack.push(op);
        Ok(())
    }

    fn renumber_positions(&self, conn: &rusqlite::Connection, playlist_id: i64) -> Result<()> {
        conn.execute_batch(&format!(
            "WITH ranked AS (
                SELECT id, ROW_NUMBER() OVER (ORDER BY position) - 1 AS new_pos
                FROM playlist_items WHERE playlist_id = {playlist_id}
             )
             UPDATE playlist_items SET position = (SELECT new_pos FROM ranked WHERE ranked.id = playlist_items.id)
             WHERE playlist_id = {playlist_id}"
        ))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup_test_db() -> (Database, std::path::PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_playlist_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_playlist_crud() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        let manager = PlaylistManager::new(db_arc.clone()).unwrap();

        // Create playlist
        let pl = manager.create_playlist("Chill Mix").unwrap();
        let pl_id = pl.id;
        assert!(pl_id > 0);

        // Get playlists
        let playlists = manager.get_playlists().unwrap();
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].name, "Chill Mix");

        // Rename playlist
        manager.rename_playlist(pl_id, "Chill Beats").unwrap();
        let playlists = manager.get_playlists().unwrap();
        assert_eq!(playlists[0].name, "Chill Beats");

        // Delete playlist
        manager.delete_playlist(pl_id).unwrap();
        let playlists = manager.get_playlists().unwrap();
        assert_eq!(playlists.len(), 0);

        // Cleanup
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_clear_playlist_undo_redo() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        // Insert dummy song into DB
        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, path) VALUES ('Test Song', 'Test Artist', '/test.mp3')",
                [],
            ).unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Clear Test").unwrap();
        manager.add_songs_to_playlist(pl.id, &[1]).unwrap();

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks.len(), 1);

        // Clear playlist
        manager.clear_playlist(pl.id).unwrap();
        let tracks_after_clear = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks_after_clear.len(), 0);

        // Undo clear
        manager.undo().unwrap();
        let tracks_after_undo = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks_after_undo.len(), 1);
        assert_eq!(
            tracks_after_undo[0].song.as_ref().unwrap().title.as_deref(),
            Some("Test Song")
        );

        // Redo clear
        manager.redo().unwrap();
        let tracks_after_redo = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks_after_redo.len(), 0);

        let _ = std::fs::remove_dir_all(temp_dir);
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

    #[test]
    fn test_reorder_playlist_items_batch() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute("INSERT INTO songs (id, title) VALUES (1, 'Song 1')", [])
                .unwrap();
            conn.execute("INSERT INTO songs (id, title) VALUES (2, 'Song 2')", [])
                .unwrap();
            conn.execute("INSERT INTO songs (id, title) VALUES (3, 'Song 3')", [])
                .unwrap();
            conn.execute("INSERT INTO songs (id, title) VALUES (4, 'Song 4')", [])
                .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Batch Test").unwrap();
        manager.add_songs_to_playlist(pl.id, &[1, 2, 3, 4]).unwrap();

        // Drag items [0, 1] (Song 1, Song 2) to the end (index 3, Song 4)
        manager
            .reorder_playlist_items_batch(pl.id, &[0, 1], 3)
            .unwrap();

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        let titles: Vec<&str> = tracks
            .iter()
            .map(|t| t.song.as_ref().unwrap().title.as_deref().unwrap())
            .collect();
        assert_eq!(titles, vec!["Song 3", "Song 4", "Song 1", "Song 2"]);

        // Test Undo
        manager.undo().unwrap();
        let tracks_undo = manager.get_playlist_tracks(pl.id).unwrap();
        let titles_undo: Vec<&str> = tracks_undo
            .iter()
            .map(|t| t.song.as_ref().unwrap().title.as_deref().unwrap())
            .collect();
        assert_eq!(titles_undo, vec!["Song 1", "Song 2", "Song 3", "Song 4"]);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_decade_auto_playlists() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();

            // Insert 1 song in the 80s — below the 25-song threshold, should be skipped.
            conn.execute(
                "INSERT INTO songs (title, year, source, unavailable) VALUES ('Track 80s', 1982, 1, 0)",
                [],
            )
            .unwrap();

            // Insert 25 songs in the 90s — meets the threshold, should create a playlist.
            for i in 0..25 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, originalyear, source, unavailable) VALUES ('Track 90s {}', 1995, 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_decade_auto_playlists().unwrap();

        let playlists = manager.get_playlists().unwrap();
        let decade_playlists: Vec<_> = playlists
            .iter()
            .filter(|p| {
                p.dynamic_enabled
                    && p.dynamic_spec
                        .as_deref()
                        .unwrap_or("")
                        .starts_with("decade:")
            })
            .collect();

        // 80s had only 1 song — below minimum, so no playlist created.
        assert!(
            !decade_playlists.iter().any(|p| p.name == "1980s"),
            "expected 80s playlist to be skipped (< 25 songs)"
        );

        // 90s had 25 songs — should have a playlist.
        assert_eq!(decade_playlists.len(), 1);
        assert_eq!(decade_playlists[0].name, "1990s");

        let tracks = manager.get_playlist_tracks(decade_playlists[0].id).unwrap();
        assert_eq!(tracks.len(), 25);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_smart_playlist_genre_rule_populates_via_filter_not_exact_genre_match() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Rock Song', 'Classic Rock', 1, 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Jazz Song', 'Jazz', 1, 0)",
                [],
            )
            .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Rock Mix").unwrap();

        // Mirrors the spec the Smart Playlist builder serialises for a single
        // "genre contains rock" rule — must NOT be routed to the exact-match
        // get_songs_by_genre() path, which would never match "Classic Rock".
        manager
            .set_playlist_dynamic_spec(pl.id, "genre:rock")
            .unwrap();

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(
            tracks.len(),
            1,
            "expected the contains-style genre rule to match 'Classic Rock' via LIKE, not require an exact 'rock' genre"
        );
        assert_eq!(
            tracks[0].song.as_ref().unwrap().title.as_deref(),
            Some("Rock Song")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_genre_auto_playlists_does_not_prune_smart_playlists() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, genre, artist, source, unavailable) VALUES ('Song A', 'Jazz', 'Miles Davis', 1, 0)",
                [],
            )
            .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();

        // A user-created Smart Playlist whose spec has nothing to do with any
        // real library genre — the genre-auto-playlist sync/prune pass must
        // leave it alone since its spec contains a "field:" rule.
        let pl = manager.create_playlist("Miles Mix").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "artist:Miles Davis")
            .unwrap();

        manager.sync_genre_auto_playlists().unwrap();

        let playlists = manager.get_playlists().unwrap();
        assert!(
            playlists.iter().any(|p| p.id == pl.id),
            "Smart Playlist should survive sync_genre_auto_playlists, but it was deleted"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
