//! Playlist manager — CRUD, undo/redo, UUID-keyed items.

use crate::{
    collection::CollectionScanner,
    db::Database,
    models::{Playlist, PlaylistItem, PlaylistItemType, QueuePopulationMode, Song},
    tags::TagManager,
};
use anyhow::{anyhow, Result};
use rusqlite::{params, OptionalExtension};
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

/// Minimum number of matching library songs required before a genre/decade
/// auto-playlist is created. Once created, an auto-playlist is populated
/// with every matching song (see [`NO_SONG_LIMIT`]), not just this many.
const MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST: i64 = 25;

/// SQLite treats a negative `LIMIT` as "no limit" — used when populating an
/// auto-playlist so it includes every matching song, however large the
/// library (e.g. 500+ songs in a single decade).
const NO_SONG_LIMIT: i64 = -1;

/// Fixed BPM buckets for the BPM auto-playlist category: (display name, min
/// BPM inclusive, max BPM inclusive — `None` means "or higher"). Unlike
/// genre/decade auto-playlists (one per distinct library value), BPM
/// auto-playlists are always this same set of five, each created only if it
/// clears [`MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST`].
const BPM_BUCKETS: [(&str, f64, Option<f64>); 5] = [
    ("Down-Tempo BPM", 60.0, Some(90.0)),
    ("Mid-Tempo BPM", 90.0, Some(115.0)),
    ("Uptempo BPM", 115.0, Some(130.0)),
    ("High Energy BPM", 130.0, Some(150.0)),
    ("Extreme BPM", 150.0, None),
];

/// Formats a BPM bucket's `(min, max)` into its `dynamic_spec` suffix, e.g.
/// `"60-90"` or the open-ended `"150-"`.
fn format_bpm_range_spec(min: f64, max: Option<f64>) -> String {
    match max {
        Some(max) => format!("bpmrange:{}-{}", min, max),
        None => format!("bpmrange:{}-", min),
    }
}

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

/// What a reconcile pass changed in one dynamic playlist.
#[derive(Debug)]
pub struct DynamicPlaylistDelta {
    pub playlist_id: i64,
    /// Freshly inserted rows, carrying their real DB UUIDs.
    pub added: Vec<PlaylistItem>,
    pub removed_uuids: Vec<String>,
}

/// Runs a reconcile pass and mirrors the outcome into the running app: a
/// currently-playing playlist gets new items appended to the live queue
/// (evicted rows removed — the playing item itself is protected by the
/// player), and the frontend learns which playlists changed. Spawned from
/// the `library-changed` / `song-stats-changed` listeners in lib.rs, so
/// "everything is immediate" without any per-call-site wiring.
pub async fn reconcile_and_sync(app: tauri::AppHandle) {
    use tauri::{Emitter, Manager};

    let state = app.state::<crate::AppState>();
    let deltas = {
        let mut playlists = state.playlists.lock().await;
        match playlists.reconcile_dynamic_playlists() {
            Ok(deltas) => deltas,
            Err(e) => {
                log::error!("Dynamic playlist reconcile failed: {e}");
                return;
            }
        }
    };
    if deltas.is_empty() {
        return;
    }

    {
        let mut player = state.player.lock().await;
        for delta in &deltas {
            if player.current_playlist_id != Some(delta.playlist_id) {
                continue;
            }
            if !delta.removed_uuids.is_empty() {
                player.remove_songs_from_playlist_items(&delta.removed_uuids);
            }
            if !delta.added.is_empty() {
                player.append_songs_to_playlist_items(delta.added.clone());
            }
            let playback_state = player.get_state().await;
            let _ = app.emit("playback-state", playback_state);
        }
    }

    let changed_ids: Vec<i64> = deltas.iter().map(|d| d.playlist_id).collect();
    let _ = app.emit("playlists-changed", changed_ids);
}

/// Normalize a path to the same form used for `songs.path` in the DB, so an
/// imported playlist's track paths (from `.m3u`/`.pls`, often relative or
/// with `./`/`../` segments) can be matched against already-scanned library
/// songs by exact string comparison. Prefers `canonicalize` (resolves
/// symlinks too, and strips Windows' `\\?\` verbatim prefix so the result
/// matches how `collection.rs` stores paths); if the path doesn't exist on
/// disk yet, falls back to lexically collapsing `.`/`..` components without
/// touching the filesystem.
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

    /// Runs all three auto-playlist syncs (genre, decade, BPM) in one call —
    /// the frontend used to invoke these as three separate IPC round trips
    /// in lockstep at every call site.
    pub fn sync_all_auto_playlists(&self) -> Result<()> {
        self.sync_genre_auto_playlists()?;
        self.sync_decade_auto_playlists()?;
        self.sync_bpm_auto_playlists()?;
        Ok(())
    }

    /// Regenerates each genre "auto-playlist" — a system-managed `playlists`
    /// row with `dynamic_enabled = 1` and `dynamic_spec` set to `tag:<name>`
    /// — if it's missing or its `updated` timestamp is more than 24h old,
    /// and prunes rows for curated tags no longer present in the hierarchy.
    /// `updated` doubles as the "last (re)generated at" timestamp shown in
    /// the UI.
    ///
    /// One auto-playlist is created per *curated tag* (#548) — every
    /// `tag_groups` row (a top-level genre card) and every `tag_assignments`
    /// row (a sub-genre chip) — rather than per distinct raw `songs.genre`
    /// string as before #548. A top-level card's membership includes every
    /// song carrying it OR any tag currently curated as its child; a chip's
    /// membership is an exact match at any position. See
    /// `TagManager::get_songs_by_curated_tag`. Mirrors
    /// `sync_decade_auto_playlists`'s prefix-based prune/threshold-gate
    /// shape.
    pub fn sync_genre_auto_playlists(&self) -> Result<()> {
        const STALE_AFTER_SECS: i64 = 24 * 60 * 60;

        let tag_manager = TagManager::new(self.db.clone());
        // Self-contained and idempotent — doesn't rely on the async
        // "library-changed" listener (tags::reconcile_hierarchy_and_notify)
        // having already run first, which would otherwise be a real
        // ordering bug: this sync could run against a stale hierarchy right
        // after a scan.
        tag_manager.reconcile_hierarchy()?;
        let hierarchy = tag_manager.get_tag_hierarchy()?;

        let mut wanted: HashSet<String> = HashSet::new();
        for group in &hierarchy {
            wanted.insert(group.name.clone());
            for child in &group.children {
                wanted.insert(child.name.clone());
            }
        }

        let conn = self.db.pool.get()?;
        let now = chrono::Utc::now().timestamp();

        // Prune genre auto-playlists for curated tags no longer in the
        // hierarchy. Only touch rows using the "tag:" convention — decade/
        // BPM auto-playlists and user-created Smart Playlist rule specs use
        // their own prefixes/shapes and must not be swept up here.
        let mut stmt = conn.prepare(
            "SELECT id, dynamic_spec FROM playlists WHERE dynamic_enabled = 1 AND dynamic_spec LIKE 'tag:%'",
        )?;
        let existing: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        for (id, spec) in &existing {
            let name = spec.strip_prefix("tag:").unwrap_or(spec);
            if !wanted.contains(name) {
                conn.execute(
                    "DELETE FROM playlist_items WHERE playlist_id = ?1",
                    params![id],
                )?;
                conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
            }
        }

        for name in &wanted {
            let spec = format!("tag:{}", name);
            let existing_row: Option<(i64, i64, i64, String)> = conn
                .query_row(
                    "SELECT p.id, COALESCE(p.updated, 0), COUNT(pi.id), COALESCE(p.population_mode, 'all') FROM playlists p LEFT JOIN playlist_items pi ON pi.playlist_id = p.id WHERE p.dynamic_enabled = 1 AND p.dynamic_spec = ?1 GROUP BY p.id",
                    params![spec],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .ok();
            let mode = existing_row
                .as_ref()
                .map(|(_, _, _, m)| QueuePopulationMode::from(m.as_str()))
                .unwrap_or_default();

            // Check library threshold first. Auto-playlists are created if the library
            // has at least MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST total songs for this tag
            // (QueuePopulationMode::All).
            // Once created, an auto-playlist is only pruned if all songs for this tag are removed (0 songs).
            let total_songs = tag_manager.get_songs_by_curated_tag(
                name,
                MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST,
                QueuePopulationMode::All,
            )?;
            if existing_row.is_none()
                && total_songs.len() < MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST as usize
            {
                continue;
            }
            if existing_row.is_some() && total_songs.is_empty() {
                if let Some((id, _, _, _)) = existing_row {
                    conn.execute(
                        "DELETE FROM playlist_items WHERE playlist_id = ?1",
                        params![id],
                    )?;
                    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
                }
                continue;
            }

            let songs = tag_manager.get_songs_by_curated_tag(name, NO_SONG_LIMIT, mode)?;

            let needs_generation = match existing_row {
                None => true,
                Some((_, updated, _, _)) => now - updated > STALE_AFTER_SECS,
            };
            if !needs_generation {
                continue;
            }

            let playlist_id = match existing_row {
                Some((id, _, _, _)) => {
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
                        "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec, created, updated) VALUES (?1, 1, ?2, ?3, ?3)",
                        params![name, spec, now],
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
            let existing_row: Option<(i64, i64, i64, String)> = conn
                .query_row(
                    "SELECT p.id, COALESCE(p.updated, 0), COUNT(pi.id), COALESCE(p.population_mode, 'all') FROM playlists p LEFT JOIN playlist_items pi ON pi.playlist_id = p.id WHERE p.dynamic_enabled = 1 AND p.dynamic_spec = ?1 GROUP BY p.id",
                    params![spec],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .ok();
            let mode = existing_row
                .as_ref()
                .map(|(_, _, _, m)| QueuePopulationMode::from(m.as_str()))
                .unwrap_or_default();

            // Check library threshold first. Auto-playlists are created if the library
            // has at least MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST total songs for this decade
            // (QueuePopulationMode::All).
            // Once created, an auto-playlist is only pruned if all songs for this decade are removed (0 songs).
            let total_songs = scanner.get_songs_by_decade(
                decade,
                MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST,
                QueuePopulationMode::All,
            )?;
            if existing_row.is_none()
                && total_songs.len() < MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST as usize
            {
                continue;
            }
            if existing_row.is_some() && total_songs.is_empty() {
                if let Some((id, _, _, _)) = existing_row {
                    conn.execute(
                        "DELETE FROM playlist_items WHERE playlist_id = ?1",
                        params![id],
                    )?;
                    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
                }
                continue;
            }

            let songs = scanner.get_songs_by_decade(decade, NO_SONG_LIMIT, mode)?;

            let needs_generation = match existing_row {
                None => true,
                Some((_, updated, _, _)) => now - updated > STALE_AFTER_SECS,
            };
            if !needs_generation {
                continue;
            }

            let playlist_id = match existing_row {
                Some((id, _, _, _)) => {
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
                        "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec, created, updated) VALUES (?1, 1, ?2, ?3, ?3)",
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

    /// Regenerates each fixed-bucket BPM "auto-playlist" — a system-managed
    /// `playlists` row with `dynamic_enabled = 1` and `dynamic_spec` set to
    /// `bpmrange:<min>-<max>` (e.g. `bpmrange:60-90`, or `bpmrange:150-` for
    /// the open-ended top bucket) — if it's missing or its `updated`
    /// timestamp is more than 24h old. Unlike genre/decade, the bucket set
    /// is fixed (see [`BPM_BUCKETS`]), not derived from distinct library
    /// values, so there is no pruning pass for "buckets no longer present".
    pub fn sync_bpm_auto_playlists(&self) -> Result<()> {
        const STALE_AFTER_SECS: i64 = 24 * 60 * 60;

        let scanner = CollectionScanner::new(self.db.clone());
        let conn = self.db.pool.get()?;
        let now = chrono::Utc::now().timestamp();

        for (name, min, max) in BPM_BUCKETS {
            let spec = format_bpm_range_spec(min, max);
            let existing_row: Option<(i64, i64, i64, String)> = conn
                .query_row(
                    "SELECT p.id, COALESCE(p.updated, 0), COUNT(pi.id), COALESCE(p.population_mode, 'all') FROM playlists p LEFT JOIN playlist_items pi ON pi.playlist_id = p.id WHERE p.dynamic_enabled = 1 AND p.dynamic_spec = ?1 GROUP BY p.id",
                    params![spec],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .ok();
            let mode = existing_row
                .as_ref()
                .map(|(_, _, _, m)| QueuePopulationMode::from(m.as_str()))
                .unwrap_or_default();

            // Check library threshold first, same precedent as genre/decade:
            // created once >= MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST songs fall in
            // this bucket; once created, only pruned if it drops to 0 songs.
            let total_songs = scanner.get_songs_by_bpm_range(
                min,
                max,
                MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST,
                QueuePopulationMode::All,
            )?;
            if existing_row.is_none()
                && total_songs.len() < MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST as usize
            {
                continue;
            }
            if existing_row.is_some() && total_songs.is_empty() {
                if let Some((id, _, _, _)) = existing_row {
                    conn.execute(
                        "DELETE FROM playlist_items WHERE playlist_id = ?1",
                        params![id],
                    )?;
                    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
                }
                continue;
            }

            let songs = scanner.get_songs_by_bpm_range(min, max, NO_SONG_LIMIT, mode)?;

            let needs_generation = match existing_row {
                None => true,
                Some((_, updated, _, _)) => now - updated > STALE_AFTER_SECS,
            };
            if !needs_generation {
                continue;
            }

            let playlist_id = match existing_row {
                Some((id, _, _, _)) => {
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
                        "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec, created, updated) VALUES (?1, 1, ?2, ?3, ?3)",
                        params![name, spec, now],
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

    /// Persist the `population_mode` bias for a playlist row (see #120).
    pub fn set_playlist_population_mode(&self, id: i64, mode: QueuePopulationMode) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE playlists SET population_mode = ?1 WHERE id = ?2",
            params![mode.as_str(), id],
        )?;
        Ok(())
    }

    /// Every library song matching a dynamic spec, in the order the spec's
    /// population mode dictates. The single dispatch point for all four spec
    /// kinds (decade:, bpmrange:, tag:, smart-rule query).
    fn songs_for_spec(&self, spec: &str, mode: QueuePopulationMode) -> Result<Vec<Song>> {
        let scanner = CollectionScanner::new(self.db.clone());
        if let Some(decade) = spec.strip_prefix("decade:") {
            scanner.get_songs_by_decade(decade, NO_SONG_LIMIT, mode)
        } else if let Some((min, max)) = spec
            .strip_prefix("bpmrange:")
            .and_then(crate::collection::parse_bpm_range_spec)
        {
            scanner.get_songs_by_bpm_range(min, max, NO_SONG_LIMIT, mode)
        } else if let Some(name) = spec.strip_prefix("tag:") {
            // A system genre auto-playlist, keyed on a curated tag name
            // (#548) rather than a Smart Playlist rule spec (which always
            // contains a "field:" rule).
            let tag_manager = TagManager::new(self.db.clone());
            tag_manager.get_songs_by_curated_tag(name, NO_SONG_LIMIT, mode)
        } else {
            let query = spec.replace(';', " ");
            scanner.search_songs_by_mode(&query, NO_SONG_LIMIT, mode)
        }
    }

    /// Populate/refresh tracks for any dynamic playlist based on its `dynamic_spec`,
    /// selected per its own `population_mode` bias (see #120).
    pub fn populate_dynamic_playlist(&mut self, playlist_id: i64) -> Result<()> {
        let conn = self.db.pool.get()?;
        let row: Option<(Option<String>, String)> = conn
            .query_row(
                "SELECT dynamic_spec, COALESCE(population_mode, 'all') FROM playlists WHERE id = ?1 AND dynamic_enabled = 1",
                params![playlist_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let (spec, mode) = match row {
            Some((Some(s), mode)) if !s.trim().is_empty() => {
                (s, QueuePopulationMode::from(mode.as_str()))
            }
            _ => return Ok(()),
        };

        let songs = self.songs_for_spec(&spec, mode)?;

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

    /// Set a dynamic playlist's population-mode bias and its spec together,
    /// populating once. The frontend used to make these as two separate
    /// calls — set-mode (which itself repopulates via the
    /// `set_playlist_population_mode` command) then set-spec (which
    /// populates again) — regenerating the playlist's tracks twice per edit.
    pub fn set_playlist_dynamic_config(
        &mut self,
        id: i64,
        mode: QueuePopulationMode,
        spec: &str,
    ) -> Result<()> {
        self.set_playlist_population_mode(id, mode)?;
        self.set_playlist_dynamic_spec(id, spec)
    }

    /// Bring one dynamic playlist's membership in line with its definition:
    /// append every song that newly matches (ordered among themselves by the
    /// playlist's population-mode rules), delete rows whose song no longer
    /// matches. Surviving rows keep their positions and UUIDs — a full
    /// re-sort only ever happens on the explicit Refresh path
    /// (`populate_dynamic_playlist`). Maintenance writes bypass the undo
    /// stack: only user edits belong there.
    fn reconcile_dynamic_playlist(
        &mut self,
        playlist_id: i64,
        spec: &str,
        mode: QueuePopulationMode,
    ) -> Result<Option<DynamicPlaylistDelta>> {
        let matching = self.songs_for_spec(spec, mode)?;
        let matching_ids: std::collections::HashSet<i64> = matching.iter().map(|s| s.id).collect();

        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT song_id, uuid, position FROM playlist_items
             WHERE playlist_id = ?1 AND song_id IS NOT NULL",
        )?;
        let current: Vec<(i64, String, i32)> = stmt
            .query_map(params![playlist_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        let current_ids: std::collections::HashSet<i64> =
            current.iter().map(|(id, _, _)| *id).collect();
        let removed_uuids: Vec<String> = current
            .iter()
            .filter(|(id, _, _)| !matching_ids.contains(id))
            .map(|(_, uuid, _)| uuid.clone())
            .collect();
        let to_add: Vec<&Song> = matching
            .iter()
            .filter(|s| !current_ids.contains(&s.id))
            .collect();

        if removed_uuids.is_empty() && to_add.is_empty() {
            return Ok(None);
        }

        for uuid in &removed_uuids {
            conn.execute(
                "DELETE FROM playlist_items WHERE playlist_id = ?1 AND uuid = ?2",
                params![playlist_id, uuid],
            )?;
        }

        let start_pos: i32 = current.iter().map(|(_, _, p)| *p).max().unwrap_or(-1) + 1;
        let mut added_uuids = std::collections::HashSet::new();
        for (next_pos, song) in (start_pos..).zip(to_add.iter()) {
            let uuid = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type) VALUES (?1, ?2, ?3, ?4, 0)",
                params![playlist_id, song.id, next_pos, uuid],
            )?;
            added_uuids.insert(uuid);
        }
        self.renumber_positions(&conn, playlist_id)?;
        drop(conn);

        // Re-fetch so the returned items carry the row UUIDs the frontend
        // and live player queue will see.
        let added: Vec<PlaylistItem> = self
            .get_playlist_tracks(playlist_id)?
            .into_iter()
            .filter(|item| added_uuids.contains(&item.uuid))
            .collect();

        Ok(Some(DynamicPlaylistDelta {
            playlist_id,
            added,
            removed_uuids,
        }))
    }

    /// Reconcile every dynamic playlist (auto categories and user smart
    /// playlists alike) against the current library. Returns one delta per
    /// playlist that actually changed.
    pub fn reconcile_dynamic_playlists(&mut self) -> Result<Vec<DynamicPlaylistDelta>> {
        let targets: Vec<(i64, String, QueuePopulationMode)> = self
            .get_playlists()?
            .into_iter()
            .filter(|p| p.dynamic_enabled)
            .filter_map(|p| {
                let spec = p.dynamic_spec.unwrap_or_default();
                if spec.trim().is_empty() {
                    None
                } else {
                    Some((p.id, spec, p.population_mode))
                }
            })
            .collect();

        let mut deltas = Vec::new();
        for (id, spec, mode) in targets {
            match self.reconcile_dynamic_playlist(id, &spec, mode) {
                Ok(Some(delta)) => deltas.push(delta),
                Ok(None) => {}
                Err(e) => log::error!("Failed to reconcile dynamic playlist {id}: {e}"),
            }
        }
        Ok(deltas)
    }

    /// Force-regenerates a dynamic/auto playlist's tracks (e.g. when user clicks
    /// the "Refresh" button in the auto-playlist header), replacing its contents
    /// with a fresh selection of matching songs from the library.
    pub fn refresh_auto_playlist(&mut self, playlist_id: i64) -> Result<()> {
        self.populate_dynamic_playlist(playlist_id)
    }

    /// Force-regenerates every dynamic/auto playlist's tracks in one pass —
    /// the frontend used to fan this out as one IPC call per playlist id.
    pub fn refresh_all_dynamic_playlists(&mut self) -> Result<()> {
        let ids: Vec<i64> = self
            .get_playlists()?
            .into_iter()
            .filter(|p| p.dynamic_enabled)
            .map(|p| p.id)
            .collect();
        for id in ids {
            self.populate_dynamic_playlist(id)?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Import & Export
    // -----------------------------------------------------------------------

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
    fn test_reconcile_appends_new_matches_and_evicts_stale_without_reordering() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for (i, genre) in ["Rock", "Rock", "Jazz"].iter().enumerate() {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, path) VALUES (?1, 'A', ?2, ?3)",
                    params![format!("Song {}", i + 1), genre, format!("/s{}.mp3", i + 1)],
                )
                .unwrap();
            }
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        // A smart playlist over genre — dynamic like the auto categories.
        let pl = manager.create_playlist("Rock Smart").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "genre:Rock")
            .unwrap();
        let before = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(before.len(), 2);

        // Library gains a matching song; a member's genre is edited away.
        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, genre, path) VALUES ('Song 4', 'A', 'Rock', '/s4.mp3')",
                [],
            )
            .unwrap();
            conn.execute("UPDATE songs SET genre = 'Pop' WHERE title = 'Song 1'", [])
                .unwrap();
        }

        let deltas = manager.reconcile_dynamic_playlists().unwrap();
        assert_eq!(deltas.len(), 1);
        let delta = &deltas[0];
        assert_eq!(delta.playlist_id, pl.id);
        assert_eq!(delta.added.len(), 1);
        assert_eq!(
            delta.added[0].song.as_ref().unwrap().title.as_deref(),
            Some("Song 4")
        );
        assert_eq!(delta.removed_uuids.len(), 1);

        // Surviving row keeps its UUID and position slot; new match appended last.
        let after = manager.get_playlist_tracks(pl.id).unwrap();
        let titles: Vec<_> = after
            .iter()
            .map(|i| i.song.as_ref().unwrap().title.clone().unwrap())
            .collect();
        assert_eq!(titles.last().map(String::as_str), Some("Song 4"));
        let surviving_before = before
            .iter()
            .find(|i| i.song.as_ref().unwrap().title.as_deref() != Some("Song 1"))
            .unwrap();
        assert!(after.iter().any(|i| i.uuid == surviving_before.uuid));

        // A second pass with nothing changed is a no-op.
        let deltas = manager.reconcile_dynamic_playlists().unwrap();
        assert!(deltas.is_empty());

        // Maintenance must not pollute the user's undo stack: undo() should
        // fail (nothing to undo beyond the initial spec population, which is
        // also not an undoable op).
        assert!(
            manager.undo().is_err()
                || manager.get_playlist_tracks(pl.id).unwrap().len() == after.len()
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
    fn test_sync_bpm_auto_playlists() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();

            // 10 songs at BPM 70 (Down-Tempo bucket) — below the 25-song threshold, should be skipped.
            for i in 0..10 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, bpm, source, unavailable) VALUES ('Chill {}', 70, 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }

            // 25 songs at BPM 140 (High Energy bucket) — meets the threshold.
            for i in 0..25 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, bpm, source, unavailable) VALUES ('Banger {}', 140, 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_bpm_auto_playlists().unwrap();

        let playlists = manager.get_playlists().unwrap();
        let bpm_playlists: Vec<_> = playlists
            .iter()
            .filter(|p| {
                p.dynamic_enabled
                    && p.dynamic_spec
                        .as_deref()
                        .unwrap_or("")
                        .starts_with("bpmrange:")
            })
            .collect();

        assert!(
            !bpm_playlists.iter().any(|p| p.name == "Down-Tempo BPM"),
            "expected Down-Tempo bucket to be skipped (< 25 songs)"
        );

        assert_eq!(bpm_playlists.len(), 1);
        assert_eq!(bpm_playlists[0].name, "High Energy BPM");
        assert_eq!(
            bpm_playlists[0].dynamic_spec.as_deref(),
            Some("bpmrange:130-150")
        );

        let tracks = manager.get_playlist_tracks(bpm_playlists[0].id).unwrap();
        assert_eq!(tracks.len(), 25);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_decade_auto_playlists_includes_every_matching_song() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            // Well above the old hardcoded 25-song population cap.
            for i in 0..60 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, originalyear, source, unavailable) VALUES ('Track 80s {}', 1985, 1, 0)",
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
        let decade_playlist = playlists
            .iter()
            .find(|p| p.dynamic_enabled && p.dynamic_spec.as_deref() == Some("decade:1980s"))
            .expect("expected 1980s auto-playlist to be created");

        let tracks = manager.get_playlist_tracks(decade_playlist.id).unwrap();
        assert_eq!(
            tracks.len(),
            60,
            "auto-playlist should include every matching song, not just the first 25"
        );

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
        // "genre contains rock" rule — must NOT be routed to the curated-tag
        // "tag:" path, which would never match "Classic Rock" (no such
        // curated tag exists).
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

    /// Changing a Smart Playlist's `population_mode` must be picked up the
    /// next time its tracks are (re)populated — see #120.
    #[test]
    fn test_set_playlist_population_mode_changes_populated_tracks() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, source, unavailable, rating, playcount, lastplayed) VALUES ('Old Favourite', 'Miles Davis', 1, 0, 5, 40, 1700000000)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, source, unavailable, rating, playcount, lastplayed) VALUES ('Unheard Cut', 'Miles Davis', 1, 0, 0, 0, NULL)",
                [],
            )
            .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Miles Mix").unwrap();
        assert_eq!(pl.population_mode, QueuePopulationMode::All);

        manager
            .set_playlist_dynamic_spec(pl.id, "artist:Miles Davis")
            .unwrap();
        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(
            tracks.len(),
            2,
            "both songs should populate under the default All mode"
        );

        // Switch to Deep Cuts — only the never-played song should remain.
        manager
            .set_playlist_population_mode(pl.id, QueuePopulationMode::DeepCuts)
            .unwrap();
        manager.refresh_auto_playlist(pl.id).unwrap();

        let playlists = manager.get_playlists().unwrap();
        let updated = playlists.iter().find(|p| p.id == pl.id).unwrap();
        assert_eq!(updated.population_mode, QueuePopulationMode::DeepCuts);

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(
            tracks[0].song.as_ref().unwrap().title.as_deref(),
            Some("Unheard Cut")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_auto_playlists_does_not_delete_when_filtered_mode_has_less_than_25_songs() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            // Add 30 Rock songs: 29 played, 1 deep cut
            for i in 1..=29 {
                conn.execute(
                    &format!("INSERT INTO songs (title, genre, source, unavailable, playcount) VALUES ('Rock Song {}', 'Rock', 1, 0, 5)", i),
                    [],
                )
                .unwrap();
            }
            conn.execute(
                "INSERT INTO songs (title, genre, source, unavailable, playcount) VALUES ('Deep Cut Rock', 'Rock', 1, 0, 0)",
                [],
            )
            .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_genre_auto_playlists().unwrap();

        let playlists = manager.get_playlists().unwrap();
        let rock_pl = playlists
            .iter()
            .find(|p| p.dynamic_spec.as_deref() == Some("tag:Rock"))
            .expect("Rock auto playlist should be created");

        // Change mode to DeepCuts (which has only 1 matching track < 25)
        manager
            .set_playlist_population_mode(rock_pl.id, QueuePopulationMode::DeepCuts)
            .unwrap();
        manager.populate_dynamic_playlist(rock_pl.id).unwrap();

        manager.sync_genre_auto_playlists().unwrap();

        let playlists_after = manager.get_playlists().unwrap();
        let rock_pl_after = playlists_after
            .iter()
            .find(|p| p.dynamic_spec.as_deref() == Some("tag:Rock"));

        assert!(
            rock_pl_after.is_some(),
            "Rock auto playlist should NOT be deleted during sync even though DeepCuts has < 25 songs"
        );
        assert_eq!(
            rock_pl_after.unwrap().population_mode,
            QueuePopulationMode::DeepCuts
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    // -----------------------------------------------------------------------
    // Curated-hierarchy genre auto-playlists (#548)
    // -----------------------------------------------------------------------

    #[test]
    fn test_sync_genre_auto_playlists_creates_one_row_per_card_and_chip() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=30 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Metal Song {}', 'Metal; Progressive Metal', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_genre_auto_playlists().unwrap();

        let playlists = manager.get_playlists().unwrap();
        assert!(
            playlists
                .iter()
                .any(|p| p.dynamic_spec.as_deref() == Some("tag:Metal")),
            "the top-level card gets its own auto-playlist row"
        );
        assert!(
            playlists
                .iter()
                .any(|p| p.dynamic_spec.as_deref() == Some("tag:Progressive Metal")),
            "the curated chip gets its own auto-playlist row too"
        );

        let metal_pl = playlists
            .iter()
            .find(|p| p.dynamic_spec.as_deref() == Some("tag:Metal"))
            .unwrap();
        assert_eq!(
            metal_pl.track_count, 30,
            "the card's playlist includes songs carrying its curated child too"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_genre_auto_playlists_prunes_on_decuration() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=30 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Ambient Song {}', 'Ambient', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_genre_auto_playlists().unwrap();
        assert!(manager
            .get_playlists()
            .unwrap()
            .iter()
            .any(|p| p.dynamic_spec.as_deref() == Some("tag:Ambient")));

        // Every "Ambient" song is deleted — the curated tag drops out of the
        // hierarchy entirely, and the next sync must prune its playlist row.
        db_arc
            .pool
            .get()
            .unwrap()
            .execute("DELETE FROM songs WHERE genre = 'Ambient'", [])
            .unwrap();

        manager.sync_genre_auto_playlists().unwrap();
        assert!(
            !manager
                .get_playlists()
                .unwrap()
                .iter()
                .any(|p| p.dynamic_spec.as_deref() == Some("tag:Ambient")),
            "a de-curated (no-longer-used) tag's auto-playlist must be pruned"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_songs_for_spec_dispatches_tag_prefix_through_curated_hierarchy() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Song A', 'Metal', 1, 0)",
                [],
            )
            .unwrap();
            // Position-0/alone, so reconcile_hierarchy would otherwise give
            // it its own root card — demoted under Metal below to prove
            // curated-child membership (not song order) is what
            // "tag:Metal" now matches on.
            conn.execute(
                "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Song B', 'Progressive Metal', 1, 0)",
                [],
            )
            .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        // Force the hierarchy to exist before populate_dynamic_playlist
        // dispatches through songs_for_spec's "tag:" branch.
        let tag_manager = crate::tags::TagManager::new(db_arc.clone());
        tag_manager.reconcile_hierarchy().unwrap();
        tag_manager
            .demote_group_to_child("Progressive Metal", "Metal")
            .unwrap();

        let pl = manager.create_playlist("Metal Auto").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "tag:Metal")
            .unwrap();
        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(
            tracks.len(),
            2,
            "tag: dispatch for a group includes its curated child's songs"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
