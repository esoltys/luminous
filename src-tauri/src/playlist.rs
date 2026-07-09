//! Playlist manager — CRUD, undo/redo, UUID-keyed items.

use crate::{
    db::Database,
    models::{Playlist, PlaylistItem, PlaylistItemType, Song},
};
use anyhow::{anyhow, Result};
use rusqlite::params;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Undo/Redo stack operations
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum PlaylistOp {
    Insert {
        playlist_id: i64,
        items: Vec<(i32, i64)>, // (position, song_id)
    },
    Remove {
        playlist_id: i64,
        items: Vec<(String, i32, i64)>, // (uuid, position, song_id)
    },
    Move {
        playlist_id: i64,
        from: i32,
        to: i32,
    },
}

pub struct PlaylistManager {
    db: Arc<Database>,
    undo_stack: Vec<PlaylistOp>,
    redo_stack: Vec<PlaylistOp>,
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
        conn.execute(
            "INSERT INTO playlists (name) VALUES (?1)",
            params![name],
        )?;
        let id = conn.last_insert_rowid();
        Ok(Playlist {
            id,
            name: name.to_string(),
            dynamic_enabled: false,
            dynamic_spec: None,
            last_played_row: None,
            created: chrono::Utc::now().timestamp(),
            track_count: 0,
        })
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE playlists SET name = ?1 WHERE id = ?2",
            params![name, id],
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
            "SELECT p.id, p.name, p.dynamic_enabled, p.dynamic_spec,
                    p.last_played_row, p.created,
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
                    last_played_row: row.get(4)?,
                    created: row.get(5)?,
                    track_count: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(playlists)
    }

    // -----------------------------------------------------------------------
    // Playlist item operations
    // -----------------------------------------------------------------------

    pub fn get_playlist_tracks(&self, playlist_id: i64) -> Result<Vec<PlaylistItem>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            &format!("SELECT pi.id, pi.playlist_id, pi.song_id, pi.position,
                             pi.uuid, pi.type, pi.url, pi.stream_url,
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
                             s.unavailable
                      FROM playlist_items pi
                      LEFT JOIN songs s ON s.id = pi.song_id
                      WHERE pi.playlist_id = ?1
                      ORDER BY pi.position"),
        )?;

        let items = stmt
            .query_map(params![playlist_id], |row| {
                let song = if row.get::<_, Option<i64>>(2)?.is_some() {
                    Some(Song {
                        id: row.get::<_, Option<i64>>(8)?.unwrap_or(0),
                        source: row.get::<_, i64>(9).map(crate::models::SongSource::from)?,
                        filetype: row.get::<_, i64>(10).map(crate::models::FileType::from)?,
                        path: row.get(11)?,
                        url: row.get(12)?,
                        stream_url: row.get(13)?,
                        title: row.get(14)?,
                        titlesort: row.get(15)?,
                        artist: row.get(16)?,
                        artistsort: row.get(17)?,
                        album: row.get(18)?,
                        albumsort: row.get(19)?,
                        album_artist: row.get(20)?,
                        album_artist_sort: row.get(21)?,
                        composer: row.get(22)?,
                        composersort: row.get(23)?,
                        performer: row.get(24)?,
                        performersort: row.get(25)?,
                        grouping: row.get(26)?,
                        comment: row.get(27)?,
                        lyrics: row.get(28)?,
                        track: row.get(29)?,
                        disc: row.get(30)?,
                        year: row.get(31)?,
                        originalyear: row.get(32)?,
                        genre: row.get(33)?,
                        compilation: row.get(34)?,
                        bpm: row.get(35)?,
                        mood: row.get(36)?,
                        initial_key: row.get(37)?,
                        length_nanosec: row.get(38)?,
                        beginning_nanosec: row.get::<_, Option<i64>>(39)?.unwrap_or(0),
                        end_nanosec: row.get::<_, Option<i64>>(40)?.unwrap_or(0),
                        bitrate: row.get(41)?,
                        samplerate: row.get(42)?,
                        bitdepth: row.get(43)?,
                        channels: row.get(44)?,
                        filesize: row.get(45)?,
                        mtime: row.get(46)?,
                        rating: row.get::<_, Option<f32>>(47)?.unwrap_or(-1.0),
                        playcount: row.get::<_, Option<i32>>(48)?.unwrap_or(0),
                        skipcount: row.get::<_, Option<i32>>(49)?.unwrap_or(0),
                        lastplayed: row.get(50)?,
                        lastseen: row.get(51)?,
                        art_embedded: row.get(52)?,
                        art_automatic: row.get(53)?,
                        art_manual: row.get(54)?,
                        art_unset: row.get(55)?,
                        unavailable: row.get::<_, Option<bool>>(56)?.unwrap_or(false),
                        ..Default::default()
                    })
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
                    additional_metadata: None,
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
            let result: Result<(i32, Option<i64>), _> = conn.query_row(
                "SELECT position, song_id FROM playlist_items WHERE uuid = ?1",
                params![uuid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );
            if let Ok((pos, song_id)) = result {
                conn.execute(
                    "DELETE FROM playlist_items WHERE uuid = ?1",
                    params![uuid],
                )?;
                if let Some(sid) = song_id {
                    removed.push((uuid.clone(), pos, sid));
                }
            }
        }

        // Re-number positions to be contiguous
        self.renumber_positions(&conn, playlist_id)?;

        self.undo_stack.push(PlaylistOp::Remove {
            playlist_id,
            items: removed,
        });
        self.redo_stack.clear();

        Ok(())
    }

    pub fn reorder_playlist_item(&mut self, playlist_id: i64, from: i32, to: i32) -> Result<()> {
        let conn = self.db.pool.get()?;

        if from == to {
            return Ok(());
        }

        // Move item at `from` to `to`, shifting others
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

        self.undo_stack.push(PlaylistOp::Move {
            playlist_id,
            from,
            to,
        });
        self.redo_stack.clear();

        Ok(())
    }

    pub fn clear_playlist(&self, playlist_id: i64) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "DELETE FROM playlist_items WHERE playlist_id = ?1",
            params![playlist_id],
        )?;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<()> {
        let op = self.undo_stack.pop().ok_or(anyhow!("nothing to undo"))?;
        // TODO: reverse the operation
        self.redo_stack.push(op);
        Ok(())
    }

    pub fn redo(&mut self) -> Result<()> {
        let op = self.redo_stack.pop().ok_or(anyhow!("nothing to redo"))?;
        // TODO: re-apply the operation
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
        let temp_dir = std::env::temp_dir().join(format!("luminous_playlist_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
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
}
