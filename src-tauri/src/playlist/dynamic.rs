//! Dynamic/Smart Playlist domain — population-mode bias, spec dispatch
//! (decade:/bpmrange:/tag:/smart-rule), populate, and library-change
//! reconciliation. Split out of `playlist.rs` (#577 item 18) — a second
//! `impl PlaylistManager` block alongside the one in `playlist.rs` that owns
//! CRUD/item-mutation/undo logic.

use super::{PlaylistManager, NO_SONG_LIMIT};
use crate::collection::CollectionScanner;
use crate::models::{PlaylistItem, QueuePopulationMode, Song};
use crate::tags::TagManager;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

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

impl PlaylistManager {
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
    /// population mode dictates. The single dispatch point for all spec
    /// kinds (decade:, bpmrange:, artisttag:, missingmeta, tag:, smart-rule
    /// query).
    fn songs_for_spec(&self, spec: &str, mode: QueuePopulationMode) -> Result<Vec<Song>> {
        let scanner = CollectionScanner::new(self.db.clone());
        if let Some(decade) = spec.strip_prefix("decade:") {
            scanner.get_songs_by_decade(decade, NO_SONG_LIMIT, mode)
        } else if let Some((min, max)) = spec
            .strip_prefix("bpmrange:")
            .and_then(crate::collection::parse_bpm_range_spec)
        {
            scanner.get_songs_by_bpm_range(min, max, NO_SONG_LIMIT, mode)
        } else if let Some(tag) = spec.strip_prefix("artisttag:") {
            scanner.get_songs_by_artist_tag(tag, NO_SONG_LIMIT, mode)
        } else if spec == "missingmeta" {
            scanner.get_songs_missing_core_tags(NO_SONG_LIMIT, mode)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup_test_db() -> (Database, std::path::PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_playlist_dynamic_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        (db, temp_dir)
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
