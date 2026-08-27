//! Playlist manager — CRUD, undo/redo, UUID-keyed items.

use crate::{
    db::Database,
    models::{Playlist, PlaylistItem, PlaylistItemType, QueuePopulationMode, Song},
};
use anyhow::{anyhow, Result};
use rusqlite::params;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

mod auto_sync;
mod dynamic;
mod import_export;

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

    // -----------------------------------------------------------------------
    // Playlist item operations
    // -----------------------------------------------------------------------

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
