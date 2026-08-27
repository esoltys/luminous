//! Playlist item mutation and undo/redo. Split out of `playlist.rs` (#577
//! item 18) — a second `impl PlaylistManager` block alongside the one in
//! `playlist.rs` that owns top-level playlist CRUD. Mutating item operations
//! (`add_songs_to_playlist`, `remove_from_playlist`, `clear_playlist`, the
//! `reorder_*` methods) push a `PlaylistOp` onto `undo_stack` and clear
//! `redo_stack`, so `undo`/`redo` can step through them.

use super::{ClearedItem, PlaylistManager, PlaylistOp};
use crate::models::{PlaylistItem, PlaylistItemType, Song};
use anyhow::Result;
use rusqlite::params;
use std::collections::HashSet;
use uuid::Uuid;

impl PlaylistManager {
    pub fn get_playlist_tracks(&self, playlist_id: i64) -> Result<Vec<PlaylistItem>> {
        let conn = self.db.pool.get()?;
        Self::get_playlist_tracks_from_conn(&conn, playlist_id)
    }

    /// Same query as `get_playlist_tracks`, taking an already-open
    /// connection instead of getting a fresh one from the pool — for
    /// callers that already hold a connection and would otherwise need a
    /// second one from the (size-limited) pool just for this call, e.g.
    /// `player.rs`'s startup restore.
    pub fn get_playlist_tracks_from_conn(
        conn: &rusqlite::Connection,
        playlist_id: i64,
    ) -> Result<Vec<PlaylistItem>> {
        // Song columns start right after these `pi.*` columns — passed as the
        // offset into the shared `row_to_song_at`, so this query's song
        // fields can never drift out of sync with `SONG_SELECT_COLS_QUALIFIED`.
        const PI_COLS_LEN: usize = 9;

        let mut stmt = conn.prepare(&format!(
            "SELECT pi.id, pi.playlist_id, pi.song_id, pi.position,
                         pi.uuid, pi.type, pi.url, pi.stream_url,
                         pi.additional_metadata,
                         {}
                  FROM playlist_items pi
                  LEFT JOIN songs s ON s.id = pi.song_id
                  WHERE pi.playlist_id = ?1
                  ORDER BY pi.position",
            crate::collection::SONG_SELECT_COLS_QUALIFIED
        ))?;

        let items = stmt
            .query_map(params![playlist_id], |row| {
                let additional_meta_str: Option<String> = row.get(8)?;

                let song = if row.get::<_, Option<i64>>(2)?.is_some() {
                    Some(crate::collection::row_to_song_at(row, PI_COLS_LEN)?)
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

        self.renumber_positions(&conn, playlist_id)?;
        self.touch_updated(&conn, playlist_id)?;

        self.undo_stack.push(PlaylistOp::Remove {
            playlist_id,
            items: removed,
        });
        self.redo_stack.clear();

        Ok(())
    }

    /// Removes duplicate items from a playlist, keeping the first occurrence
    /// of each. Identity is: DB song id when the item is a library song,
    /// else its url (streamed/imported items), else its own uuid (which is
    /// always unique, so that tier never actually flags a duplicate — it's
    /// just the fallback for items with neither). Returns the removed uuids.
    pub fn deduplicate_playlist(&mut self, playlist_id: i64) -> Result<Vec<String>> {
        let tracks = self.get_playlist_tracks(playlist_id)?;
        let mut seen = HashSet::new();
        let mut duplicate_uuids = Vec::new();

        for item in &tracks {
            let key = if let Some(song) = &item.song {
                format!("song-{}", song.id)
            } else if let Some(url) = &item.url {
                format!("url-{url}")
            } else {
                format!("uuid-{}", item.uuid)
            };

            if !seen.insert(key) {
                duplicate_uuids.push(item.uuid.clone());
            }
        }

        if !duplicate_uuids.is_empty() {
            self.remove_from_playlist(playlist_id, &duplicate_uuids)?;
        }

        Ok(duplicate_uuids)
    }

    /// Removes every item ahead of `uuid` (in current DB order) from
    /// `playlist_id`. Resolves `uuid`'s position and removes in the same
    /// call rather than trusting a caller-supplied index, so there's no gap
    /// between reading a position and acting on it.
    pub fn trim_playlist_before_uuid(
        &mut self,
        playlist_id: i64,
        uuid: &str,
    ) -> Result<Vec<String>> {
        let tracks = self.get_playlist_tracks(playlist_id)?;
        let Some(index) = tracks.iter().position(|t| t.uuid == uuid) else {
            return Ok(Vec::new());
        };
        if index == 0 {
            return Ok(Vec::new());
        }

        let uuids_to_remove: Vec<String> = tracks[..index].iter().map(|t| t.uuid.clone()).collect();
        self.remove_from_playlist(playlist_id, &uuids_to_remove)?;
        Ok(uuids_to_remove)
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

    /// Move the single item at position `from` to position `to`, shifting
    /// everything between. The base single-item reorder that
    /// `reorder_playlist_item_by_uuid` resolves down to; for moving more
    /// than one item at once see `reorder_playlist_items_batch`.
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

    /// Same move as `reorder_playlist_item`, but addressed by item UUID
    /// instead of position — for callers (drag-and-drop in the UI) that
    /// only know which items moved, not their current numeric positions.
    /// A no-op (not an error) if either UUID isn't found in the playlist.
    pub fn reorder_playlist_item_by_uuid(
        &mut self,
        playlist_id: i64,
        source_uuid: &str,
        target_uuid: &str,
    ) -> Result<()> {
        if source_uuid == target_uuid {
            return Ok(());
        }

        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT uuid, position FROM playlist_items WHERE playlist_id = ?1 ORDER BY position",
        )?;
        let items: Vec<(String, i32)> = stmt
            .query_map(params![playlist_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let from_idx = items.iter().position(|(u, _)| u == source_uuid);
        let to_idx = items.iter().position(|(u, _)| u == target_uuid);

        if let (Some(from), Some(to)) = (from_idx, to_idx) {
            self.reorder_playlist_item(playlist_id, from as i32, to as i32)?;
        }

        Ok(())
    }

    /// Move every item at a position in `from_indices` to sit consecutively
    /// starting at `to_index`, preserving their relative order — for
    /// multi-select drag-and-drop. Delegates to `reorder_playlist_item` for
    /// the single-index case.
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

    /// Pops and applies the most recent undo op. Returns `Ok(false)` rather
    /// than erroring when the stack is empty — the frontend calls this with
    /// no error handling, and "nothing left to undo" is a routine state a
    /// user reaches by clicking one time too many, not a failure.
    pub fn undo(&mut self) -> Result<bool> {
        let Some(op) = self.undo_stack.pop() else {
            return Ok(false);
        };
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
                // Restore items in ascending original-position order, opening a
                // gap for each one before inserting it. Relying on
                // renumber_positions' `ORDER BY position` tiebreak instead would
                // be nondeterministic: SQLite doesn't guarantee tie order, so a
                // reinserted row can land one slot off from the row it displaces.
                let mut items_by_position: Vec<&ClearedItem> = items.iter().collect();
                items_by_position.sort_by_key(|item| item.position);
                for item in items_by_position {
                    conn.execute(
                        "UPDATE playlist_items SET position = position + 1 WHERE playlist_id = ?1 AND position >= ?2",
                        params![playlist_id, item.position],
                    )?;
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
        Ok(true)
    }

    /// Pops and applies the most recent redo op. See [`Self::undo`] for why
    /// an empty stack is `Ok(false)` rather than an error.
    pub fn redo(&mut self) -> Result<bool> {
        let Some(op) = self.redo_stack.pop() else {
            return Ok(false);
        };
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
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup_test_db() -> (Database, std::path::PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_playlist_mutation_undo_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_undo_redo_on_empty_stack_returns_false() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();

        // A brand new manager has nothing to undo or redo — this must be a
        // routine `Ok(false)`, not an error, since the frontend calls these
        // with no error handling and reaching the end of the stack is
        // something a user can trigger just by clicking one time too many.
        assert!(!manager.undo().unwrap());
        assert!(!manager.redo().unwrap());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_clear_playlist_undo_redo() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

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

        manager.clear_playlist(pl.id).unwrap();
        let tracks_after_clear = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks_after_clear.len(), 0);

        manager.undo().unwrap();
        let tracks_after_undo = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks_after_undo.len(), 1);
        assert_eq!(
            tracks_after_undo[0].song.as_ref().unwrap().title.as_deref(),
            Some("Test Song")
        );

        manager.redo().unwrap();
        let tracks_after_redo = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks_after_redo.len(), 0);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_remove_middle_item_undo_restores_exact_position() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=5 {
                conn.execute(
                    "INSERT INTO songs (id, title) VALUES (?1, ?2)",
                    params![i, format!("Song {i}")],
                )
                .unwrap();
            }
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Middle Remove Test").unwrap();
        manager
            .add_songs_to_playlist(pl.id, &[1, 2, 3, 4, 5])
            .unwrap();

        let original_tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(original_tracks.len(), 5);
        let original_uuids: Vec<String> = original_tracks.iter().map(|t| t.uuid.clone()).collect();
        // Song 3 sits at index 2, flanked by items that will shift when it's removed.
        let middle_uuid = original_tracks[2].uuid.clone();
        assert_eq!(
            original_tracks[2].song.as_ref().unwrap().title.as_deref(),
            Some("Song 3")
        );

        manager
            .remove_from_playlist(pl.id, std::slice::from_ref(&middle_uuid))
            .unwrap();
        let after_remove = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(after_remove.len(), 4);
        assert!(after_remove.iter().all(|t| t.uuid != middle_uuid));

        manager.undo().unwrap();
        let after_undo = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(after_undo.len(), 5);

        // The restored item must land back at its exact original index, not
        // merely somewhere in the list — and every other item's position must
        // be untouched too.
        let restored_uuids: Vec<String> = after_undo.iter().map(|t| t.uuid.clone()).collect();
        assert_eq!(restored_uuids, original_uuids);
        assert_eq!(after_undo[2].uuid, middle_uuid);
        assert_eq!(
            after_undo[2].song.as_ref().unwrap().title.as_deref(),
            Some("Song 3")
        );
        for (idx, track) in after_undo.iter().enumerate() {
            assert_eq!(track.position, idx as i32);
        }

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_reorder_playlist_item_by_uuid() {
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
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Test UUID Reorder").unwrap();
        manager.add_songs_to_playlist(pl.id, &[1]).unwrap();
        manager.add_songs_to_playlist(pl.id, &[2]).unwrap();
        manager.add_songs_to_playlist(pl.id, &[3]).unwrap();

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        let uuid0 = tracks[0].uuid.clone();
        let uuid2 = tracks[2].uuid.clone();

        manager
            .reorder_playlist_item_by_uuid(pl.id, &uuid0, &uuid2)
            .unwrap();

        let reordered = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(reordered[0].song.as_ref().unwrap().id, 2);
        assert_eq!(reordered[1].song.as_ref().unwrap().id, 3);
        assert_eq!(reordered[2].song.as_ref().unwrap().id, 1);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_deduplicate_playlist_removes_repeat_songs_keeping_first_occurrence() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute("INSERT INTO songs (id, title) VALUES (1, 'Song 1')", [])
                .unwrap();
            conn.execute("INSERT INTO songs (id, title) VALUES (2, 'Song 2')", [])
                .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Dedup Test").unwrap();
        manager.add_songs_to_playlist(pl.id, &[1]).unwrap();
        manager.add_songs_to_playlist(pl.id, &[2]).unwrap();
        manager.add_songs_to_playlist(pl.id, &[1]).unwrap();

        let removed = manager.deduplicate_playlist(pl.id).unwrap();
        assert_eq!(removed.len(), 1);

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].song.as_ref().unwrap().id, 1);
        assert_eq!(tracks[1].song.as_ref().unwrap().id, 2);

        // No duplicates left — a second pass is a no-op.
        let removed_again = manager.deduplicate_playlist(pl.id).unwrap();
        assert!(removed_again.is_empty());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_playlist_tracks_includes_song_added_timestamp() {
        // Regression test: get_playlist_tracks_from_conn used to hand-roll its
        // own song column list and row mapping, which silently omitted
        // `added` (and other trailing Song columns) instead of failing —
        // the Queue view showed "—" in the Added column while the Collection
        // view (backed by a different, complete query) showed a real date
        // for the exact same songs.
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute("INSERT INTO songs (id, title) VALUES (1, 'Song 1')", [])
                .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Added Column Test").unwrap();
        manager.add_songs_to_playlist(pl.id, &[1]).unwrap();

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks.len(), 1);
        assert!(
            tracks[0].song.as_ref().unwrap().added.is_some(),
            "song.added should be populated from the `songs.added` column, not left at its Default::default() of None"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_trim_playlist_before_uuid_removes_only_earlier_items() {
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
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Trim Test").unwrap();
        manager.add_songs_to_playlist(pl.id, &[1]).unwrap();
        manager.add_songs_to_playlist(pl.id, &[2]).unwrap();
        manager.add_songs_to_playlist(pl.id, &[3]).unwrap();

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        let uuid2 = tracks[2].uuid.clone();

        let removed = manager.trim_playlist_before_uuid(pl.id, &uuid2).unwrap();
        assert_eq!(removed.len(), 2);

        let remaining = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].song.as_ref().unwrap().id, 3);

        // The first item in the playlist has nothing ahead of it — a no-op.
        let removed_at_head = manager
            .trim_playlist_before_uuid(pl.id, &remaining[0].uuid)
            .unwrap();
        assert!(removed_at_head.is_empty());

        // An unknown uuid is also a no-op rather than an error.
        let removed_unknown = manager
            .trim_playlist_before_uuid(pl.id, "not-a-real-uuid")
            .unwrap();
        assert!(removed_unknown.is_empty());

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

        manager.undo().unwrap();
        let tracks_undo = manager.get_playlist_tracks(pl.id).unwrap();
        let titles_undo: Vec<&str> = tracks_undo
            .iter()
            .map(|t| t.song.as_ref().unwrap().title.as_deref().unwrap())
            .collect();
        assert_eq!(titles_undo, vec!["Song 1", "Song 2", "Song 3", "Song 4"]);

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
