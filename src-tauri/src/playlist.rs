//! Playlist manager — CRUD, undo/redo, UUID-keyed items.

use crate::{
    db::Database,
    models::{Playlist, PlaylistItem, QueuePopulationMode},
};
use anyhow::{anyhow, Result};
use rusqlite::params;
use std::sync::Arc;

mod auto_sync;
mod dynamic;
mod import_export;
mod mutation_undo;

pub use dynamic::{reconcile_and_sync, DynamicPlaylistDelta};

/// SQLite treats a negative `LIMIT` as "no limit" — used when populating an
/// auto-playlist so it includes every matching song, however large the
/// library (e.g. 500+ songs in a single decade).
const NO_SONG_LIMIT: i64 = -1;

/// Names reserved for the app's single built-in Queue playlist: the literal
/// DB name ("Queue" — see `queuePlaylist` in playlists.svelte.ts, which
/// identifies it purely by this string) plus every locale's translated
/// `playerBar.queueTitle` label (see src/lib/locales/*.ts), so a playlist
/// can't be named something a user would mistake for the real Queue in any
/// supported language. Keep this list in sync with `queueTitle` across
/// locale files.
const RESERVED_PLAYLIST_NAMES: &[&str] = &[
    "queue",          // en: playerBar.queueTitle
    "file d'attente", // fr: playerBar.queueTitle
];

/// Case/whitespace-insensitive check against `RESERVED_PLAYLIST_NAMES`.
pub fn is_reserved_playlist_name(name: &str) -> bool {
    let trimmed = name.trim().to_lowercase();
    RESERVED_PLAYLIST_NAMES.contains(&trimmed.as_str())
}

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

/// Playlist CRUD, item mutation, import/export, and auto-playlist sync, all
/// backed by SQLite. Mutating item operations (`add_songs_to_playlist`,
/// `remove_from_playlist`, `clear_playlist`, the `reorder_*` methods) push a
/// `PlaylistOp` onto `undo_stack` and clear `redo_stack`, so `undo`/`redo`
/// can step through them — this history is in-memory only (held for the
/// lifetime of the single long-lived `PlaylistManager` in app state), not
/// persisted, so it resets on app restart.
#[derive(Debug)]
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
        // The one legitimate "Queue" playlist is bootstrapped through this same
        // method (see ensureQueuePlaylist in playlists.svelte.ts) — allow that
        // single creation, but reject it (and any other reserved name) once a
        // Queue playlist already exists, so users can't create a duplicate.
        if is_reserved_playlist_name(name) {
            let queue_exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM playlists WHERE LOWER(TRIM(name)) = 'queue')",
                [],
                |row| row.get(0),
            )?;
            if queue_exists || !name.trim().eq_ignore_ascii_case("queue") {
                return Err(anyhow!(
                    "\"{}\" is reserved for the app's built-in Queue playlist",
                    name.trim()
                ));
            }
        }
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
            population_mode: QueuePopulationMode::default(),
            last_played_row: None,
            created: now,
            updated: now,
            track_count: 0,
            is_queue: Playlist::is_queue_row(name, false),
        })
    }

    /// The app's built-in Queue playlist. Creates it on first use, so callers
    /// can rely on it existing — "no Queue" is not a representable state.
    pub fn queue(&self) -> Result<Playlist> {
        if let Some(pl) = self.get_playlists()?.into_iter().find(|p| p.is_queue) {
            return Ok(pl);
        }
        self.create_playlist("Queue")
    }

    /// Replace the Queue's contents with `song_ids` and return the queue id
    /// plus its fresh items. Goes through remove/add (rather than a raw
    /// DELETE) so the replacement stays undoable like any other edit.
    pub fn replace_queue(&mut self, song_ids: &[i64]) -> Result<(i64, Vec<PlaylistItem>)> {
        let queue = self.queue()?;
        let existing = self.get_playlist_tracks(queue.id)?;
        if !existing.is_empty() {
            let uuids: Vec<String> = existing.into_iter().map(|i| i.uuid).collect();
            self.remove_from_playlist(queue.id, &uuids)?;
        }
        self.add_songs_to_playlist(queue.id, song_ids)?;
        let items = self.get_playlist_tracks(queue.id)?;
        Ok((queue.id, items))
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
        // Renaming never legitimately produces the Queue playlist (it's only
        // ever created directly, see create_playlist above), so any reserved
        // name is rejected outright here.
        if is_reserved_playlist_name(name) {
            return Err(anyhow!(
                "\"{}\" is reserved for the app's built-in Queue playlist",
                name.trim()
            ));
        }
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
            "SELECT p.id, p.name, p.dynamic_enabled, p.dynamic_spec, p.population_mode,
                    p.last_played_row, p.created, p.updated,
                    COUNT(pi.id) as track_count
             FROM playlists p
             LEFT JOIN playlist_items pi ON pi.playlist_id = p.id
             GROUP BY p.id
             ORDER BY p.created",
        )?;
        let playlists = stmt
            .query_map([], Playlist::from_row)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(playlists)
    }

    /// Playlists that *contain* at least one available track by `artist`
    /// (effective artist, album_artist falling back to artist) — not
    /// playlists named after the artist.
    pub fn get_playlists_by_artist(&self, artist: &str) -> Result<Vec<Playlist>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.dynamic_enabled, p.dynamic_spec, p.population_mode,
                    p.last_played_row, p.created, p.updated,
                    (SELECT COUNT(*) FROM playlist_items pi2 WHERE pi2.playlist_id = p.id) as track_count
             FROM playlists p
             WHERE EXISTS (
                 SELECT 1 FROM playlist_items pi
                 JOIN songs s ON s.id = pi.song_id
                 WHERE pi.playlist_id = p.id
                   AND COALESCE(NULLIF(s.album_artist, ''), s.artist, '') = ?1
                   AND s.unavailable = 0
             )
             ORDER BY p.created",
        )?;
        let playlists = stmt
            .query_map(params![artist], Playlist::from_row)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(playlists)
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
    fn test_reserved_playlist_names() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        let manager = PlaylistManager::new(db_arc.clone()).unwrap();

        // Bootstrapping the real Queue playlist (as ensureQueuePlaylist does) succeeds once.
        let queue = manager.create_playlist("Queue").unwrap();

        // A second attempt at any reserved name (any case/whitespace, any locale) is rejected.
        assert!(manager.create_playlist("queue").is_err());
        assert!(manager.create_playlist("  QUEUE  ").is_err());
        assert!(manager.create_playlist("File d'attente").is_err());

        // Renaming another playlist to a reserved name is always rejected.
        let other = manager.create_playlist("Chill Mix").unwrap();
        assert!(manager.rename_playlist(other.id, "Queue").is_err());
        assert!(manager.rename_playlist(other.id, "File D'Attente").is_err());
        let playlists = manager.get_playlists().unwrap();
        assert_eq!(
            playlists.iter().find(|p| p.id == other.id).unwrap().name,
            "Chill Mix"
        );

        // The real Queue playlist itself is untouched.
        assert_eq!(
            playlists.iter().find(|p| p.id == queue.id).unwrap().name,
            "Queue"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_queue_is_idempotent_and_flagged() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        let manager = PlaylistManager::new(db_arc.clone()).unwrap();

        let q1 = manager.queue().unwrap();
        let q2 = manager.queue().unwrap();
        assert_eq!(q1.id, q2.id, "queue() must not create duplicates");
        assert!(q1.is_queue);
        assert_eq!(manager.get_playlists().unwrap().len(), 1);

        // Ordinary playlists are never flagged as the Queue.
        let other = manager.create_playlist("Road Trip").unwrap();
        assert!(!other.is_queue);
        let listed = manager.get_playlists().unwrap();
        assert!(listed.iter().find(|p| p.id == q1.id).unwrap().is_queue);
        assert!(!listed.iter().find(|p| p.id == other.id).unwrap().is_queue);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_replace_queue_swaps_contents_and_is_undoable() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=3 {
                conn.execute(
                    "INSERT INTO songs (title, artist, path) VALUES (?1, 'A', ?2)",
                    params![format!("Song {i}"), format!("/s{i}.mp3")],
                )
                .unwrap();
            }
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let (queue_id, items) = manager.replace_queue(&[1, 2]).unwrap();
        assert_eq!(items.len(), 2);

        let (queue_id_2, items) = manager.replace_queue(&[3]).unwrap();
        assert_eq!(
            queue_id, queue_id_2,
            "replace must reuse the same Queue row"
        );
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].song.as_ref().unwrap().title.as_deref(),
            Some("Song 3")
        );

        // The replacement went through the normal edit ops, so undoing the
        // add restores the pre-replace state step by step.
        manager.undo().unwrap(); // undo add of [3]
        manager.undo().unwrap(); // undo removal of [1, 2]
        let tracks = manager.get_playlist_tracks(queue_id).unwrap();
        assert_eq!(tracks.len(), 2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_playlist_crud() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        let manager = PlaylistManager::new(db_arc.clone()).unwrap();

        let pl = manager.create_playlist("Chill Mix").unwrap();
        let pl_id = pl.id;
        assert!(pl_id > 0);

        let playlists = manager.get_playlists().unwrap();
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].name, "Chill Mix");

        manager.rename_playlist(pl_id, "Chill Beats").unwrap();
        let playlists = manager.get_playlists().unwrap();
        assert_eq!(playlists[0].name, "Chill Beats");

        manager.delete_playlist(pl_id).unwrap();
        let playlists = manager.get_playlists().unwrap();
        assert_eq!(playlists.len(), 0);

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
