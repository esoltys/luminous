//! Dynamic/Smart Playlist domain — population-mode bias, spec dispatch
//! (decade:/bpmrange:/tag:/smart-rule), populate, and library-change
//! reconciliation. Split out of `playlist.rs` (#577 item 18) — a second
//! `impl PlaylistManager` block alongside the one in `playlist.rs` that owns
//! CRUD/item-mutation/undo logic.

use super::{PlaylistManager, NO_SONG_LIMIT};
use crate::collection::CollectionScanner;
use crate::db::Database;
use crate::models::{PlaylistItem, QueuePopulationMode, Song};
use crate::tags::TagManager;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

/// Size of the random-fill fallback for a Daypart Mix (#223) whose picked
/// genre grouping doesn't clear the auto-playlist minimum — a bounded
/// "shuffle across the library" sample rather than every song at once.
const DAYPART_RANDOM_FILL_LIMIT: i64 = 200;

/// What a reconcile pass changed in one dynamic playlist.
#[derive(Debug)]
pub struct DynamicPlaylistDelta {
    pub playlist_id: i64,
    /// Freshly inserted rows, carrying their real DB UUIDs.
    pub added: Vec<PlaylistItem>,
    pub removed_uuids: Vec<String>,
}

/// One dynamic playlist's current matches, resolved without the playlists
/// lock — the read half of a reconcile pass (see [`match_dynamic_playlists`]).
#[derive(Debug)]
pub struct DynamicPlaylistMatch {
    playlist_id: i64,
    /// The spec the match was resolved from, so the write half can skip a
    /// playlist whose definition changed in between.
    spec: String,
    /// Matching song ids, in population-mode order.
    song_ids: Vec<i64>,
}

/// The songs a dynamic spec resolves to, and whether they're its definition
/// or just a stand-in sample.
struct SpecSongs {
    songs: Vec<Song>,
    /// The spec has no membership definition to hold the playlist to — a
    /// Daypart Mix's random-fill fallback draws a fresh sample each call.
    random_sample: bool,
}

/// Set while a reconcile pass runs; a request arriving meanwhile sets
/// [`RECONCILE_REQUESTED`] instead of starting a second, overlapping pass.
static RECONCILE_RUNNING: AtomicBool = AtomicBool::new(false);
static RECONCILE_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Runs a reconcile pass and mirrors the outcome into the running app: a
/// currently-playing playlist gets new items appended to the live queue
/// (evicted rows removed — the playing item itself is protected by the
/// player), and the frontend learns which playlists changed. Spawned from
/// the `library-changed` / `song-stats-changed` listeners in lib.rs, so
/// "everything is immediate" without any per-call-site wiring.
///
/// Those events arrive in bursts (a scan, a run of stat updates), so calls
/// coalesce: while a pass runs, any number of new requests queue exactly one
/// follow-up pass, which sees every change they announced.
pub async fn reconcile_and_sync(app: tauri::AppHandle) {
    RECONCILE_REQUESTED.store(true, Ordering::Release);
    loop {
        if RECONCILE_RUNNING.swap(true, Ordering::AcqRel) {
            return; // the running pass picks the request up
        }
        while RECONCILE_REQUESTED.swap(false, Ordering::AcqRel) {
            reconcile_pass(&app).await;
        }
        RECONCILE_RUNNING.store(false, Ordering::Release);
        // A request that landed between the last swap and the store above
        // would otherwise wait for the next event.
        if !RECONCILE_REQUESTED.load(Ordering::Acquire) {
            return;
        }
    }
}

async fn reconcile_pass(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};

    let state = app.state::<crate::AppState>();
    // Resolving every spec is the expensive half — a full-library query per
    // dynamic playlist — and only reads, so it runs on the blocking pool
    // without the playlists lock; playlist IPC stays responsive meanwhile.
    let db = state.db.clone();
    let matches = match tokio::task::spawn_blocking(move || match_dynamic_playlists(&db)).await {
        Ok(Ok(matches)) => matches,
        Ok(Err(e)) => {
            log::error!("Dynamic playlist reconcile failed: {e}");
            return;
        }
        Err(e) => {
            log::error!("Dynamic playlist reconcile task failed: {e}");
            return;
        }
    };
    // `with_playlists` runs the synchronous rusqlite writes via
    // `block_in_place` (#1097) — see its doc comment in playlist.rs.
    let deltas = match crate::playlist::with_playlists(&state.playlists, |pm| {
        pm.apply_dynamic_matches(matches)
    })
    .await
    {
        Ok(deltas) => deltas,
        Err(e) => {
            log::error!("Dynamic playlist reconcile failed: {e}");
            return;
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

/// The read half of a reconcile pass: every dynamic playlist's current
/// matches. Needs only the database, not the `PlaylistManager`, so callers
/// can run it without holding the playlists lock. Random-sample specs are
/// left out — there's no membership to hold them to, and reconciling one
/// would swap in a fresh sample on every library change.
pub fn match_dynamic_playlists(db: &Arc<Database>) -> Result<Vec<DynamicPlaylistMatch>> {
    let targets = dynamic_playlist_specs(db)?;
    let mut matches = Vec::with_capacity(targets.len());
    for (playlist_id, spec, mode) in targets {
        match spec_songs(db, &spec, mode) {
            Ok(resolved) if resolved.random_sample => {}
            Ok(resolved) => matches.push(DynamicPlaylistMatch {
                playlist_id,
                spec,
                song_ids: resolved.songs.iter().map(|s| s.id).collect(),
            }),
            Err(e) => log::error!("Failed to reconcile dynamic playlist {playlist_id}: {e}"),
        }
    }
    Ok(matches)
}

/// Every enabled dynamic playlist with a non-empty spec, as
/// `(id, spec, population_mode)`.
fn dynamic_playlist_specs(db: &Database) -> Result<Vec<(i64, String, QueuePopulationMode)>> {
    let conn = db.pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, dynamic_spec, COALESCE(population_mode, 'all') FROM playlists
         WHERE dynamic_enabled = 1 AND TRIM(COALESCE(dynamic_spec, '')) != ''",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, spec, mode)| (id, spec, QueuePopulationMode::from(mode.as_str())))
        .collect())
}

/// Every library song matching a dynamic spec, in the order the spec's
/// population mode dictates. The single dispatch point for all spec kinds
/// (decade:, bpmrange:, artisttag:, missingmeta, tag:, daypart:, smart-rule
/// query).
fn spec_songs(db: &Arc<Database>, spec: &str, mode: QueuePopulationMode) -> Result<SpecSongs> {
    let matched = |songs| {
        Ok(SpecSongs {
            songs,
            random_sample: false,
        })
    };
    let random_fill = |scanner: &CollectionScanner| {
        // Unlike a genre match (naturally bounded by how many songs carry
        // that genre), "random songs from anywhere" has no natural size —
        // cap it to a real mix-sized sample rather than shuffling the
        // entire library into one playlist.
        Ok(SpecSongs {
            songs: scanner.get_random_songs(DAYPART_RANDOM_FILL_LIMIT)?,
            random_sample: true,
        })
    };
    let scanner = CollectionScanner::new(db.clone());
    if let Some(decade) = spec.strip_prefix("decade:") {
        matched(scanner.get_songs_by_decade(decade, NO_SONG_LIMIT, mode)?)
    } else if let Some((min, max)) = spec
        .strip_prefix("bpmrange:")
        .and_then(crate::collection::parse_bpm_range_spec)
    {
        matched(scanner.get_songs_by_bpm_range(min, max, NO_SONG_LIMIT, mode)?)
    } else if let Some(tag) = spec.strip_prefix("artisttag:") {
        matched(scanner.get_songs_by_artist_tag(tag, NO_SONG_LIMIT, mode)?)
    } else if spec == "missingmeta" {
        matched(scanner.get_songs_missing_core_tags(NO_SONG_LIMIT, mode)?)
    } else if spec == "missingmbid" {
        matched(scanner.get_songs_missing_musicbrainz_id(NO_SONG_LIMIT, mode)?)
    } else if let Some(name) = spec.strip_prefix("tag:") {
        // A system genre auto-playlist, keyed on a curated tag name (#548)
        // rather than a Smart Playlist rule spec (which always contains a
        // "field:" rule).
        let tag_manager = TagManager::new(db.clone());
        matched(tag_manager.get_songs_by_curated_tag(name, NO_SONG_LIMIT, mode)?)
    } else if let Some(rest) = spec.strip_prefix("daypart:") {
        // "<bucket>:<date>:<resolved-name>" — bucket/date are only the
        // reroll cache key `sync_daypart_auto_playlist` checks; here we only
        // care about the already-resolved trailing name (empty means the
        // random-fill fallback was chosen). This never re-picks anything
        // itself, so calling this via `populate_dynamic_playlist`/reconcile
        // does not reroll the mix.
        let name = rest.splitn(3, ':').nth(2).unwrap_or("");
        if name.is_empty() {
            random_fill(&scanner)
        } else {
            let tag_manager = TagManager::new(db.clone());
            let songs = tag_manager.get_songs_by_curated_tag(name, NO_SONG_LIMIT, mode)?;
            if songs.is_empty() {
                random_fill(&scanner)
            } else {
                matched(songs)
            }
        }
    } else {
        let query = spec.replace(';', " ");
        matched(scanner.search_songs_by_mode(&query, NO_SONG_LIMIT, mode)?)
    }
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

    /// Every library song matching a dynamic spec — see [`spec_songs`].
    /// `pub(super)` so `auto_sync.rs`'s `sync_daypart_auto_playlist` can
    /// materialize a freshly-resolved `daypart:` spec through the same path
    /// every other category uses.
    pub(super) fn songs_for_spec(
        &self,
        spec: &str,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        Ok(spec_songs(&self.db, spec, mode)?.songs)
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
        // `songs_for_spec` takes its own connection; re-acquire for the
        // writes instead of holding two.
        drop(conn);

        let songs = self.songs_for_spec(&spec, mode)?;

        let conn = self.db.pool.get()?;
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
        // Populating takes its own connection; holding this one meanwhile
        // needs two at once and can exhaust the pool under load.
        drop(conn);
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
    fn apply_dynamic_match(
        &mut self,
        m: &DynamicPlaylistMatch,
    ) -> Result<Option<DynamicPlaylistDelta>> {
        let playlist_id = m.playlist_id;
        let conn = self.db.pool.get()?;
        // The match was resolved without the playlists lock; skip a playlist
        // deleted or redefined since — its own edit already repopulated it.
        let still_current: bool = conn
            .query_row(
                "SELECT 1 FROM playlists WHERE id = ?1 AND dynamic_enabled = 1 AND dynamic_spec = ?2",
                params![playlist_id, m.spec],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !still_current {
            return Ok(None);
        }

        let matching_ids: std::collections::HashSet<i64> = m.song_ids.iter().copied().collect();
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
        let to_add: Vec<i64> = m
            .song_ids
            .iter()
            .copied()
            .filter(|id| !current_ids.contains(id))
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
        for (next_pos, song_id) in (start_pos..).zip(to_add.iter()) {
            let uuid = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type) VALUES (?1, ?2, ?3, ?4, 0)",
                params![playlist_id, song_id, next_pos, uuid],
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

    /// The write half of a reconcile pass: applies matches from
    /// [`match_dynamic_playlists`]. Returns one delta per playlist that
    /// actually changed.
    pub fn apply_dynamic_matches(
        &mut self,
        matches: Vec<DynamicPlaylistMatch>,
    ) -> Result<Vec<DynamicPlaylistDelta>> {
        let mut deltas = Vec::new();
        for m in &matches {
            match self.apply_dynamic_match(m) {
                Ok(Some(delta)) => deltas.push(delta),
                Ok(None) => {}
                Err(e) => log::error!(
                    "Failed to reconcile dynamic playlist {}: {e}",
                    m.playlist_id
                ),
            }
        }
        Ok(deltas)
    }

    /// Reconcile every dynamic playlist (auto categories and user smart
    /// playlists alike) against the current library in one call — both
    /// halves back to back. The app's listener runs them separately so the
    /// read half doesn't hold the playlists lock (see [`reconcile_and_sync`]).
    pub fn reconcile_dynamic_playlists(&mut self) -> Result<Vec<DynamicPlaylistDelta>> {
        let matches = match_dynamic_playlists(&self.db)?;
        self.apply_dynamic_matches(matches)
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
        let temp_dir = tempfile::tempdir().unwrap().keep();
        let db = Database::new(temp_dir.clone()).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_songs_for_spec_daypart_prefix_dispatches_to_curated_tag_lookup() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=3 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, source, unavailable) VALUES (?1, 'A', 'Jazz', 1, 0)",
                    params![format!("Jazz Song {i}")],
                )
                .unwrap();
            }
            conn.execute(
                "INSERT INTO songs (title, artist, genre, source, unavailable) VALUES ('Other', 'A', 'Blues', 1, 0)",
                [],
            )
            .unwrap();
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let daypart_songs = manager
            .songs_for_spec("daypart:morning:2026-09-03:Jazz", QueuePopulationMode::All)
            .unwrap();
        let tag_songs = manager
            .songs_for_spec("tag:Jazz", QueuePopulationMode::All)
            .unwrap();

        assert_eq!(daypart_songs.len(), 3);
        let mut daypart_ids: Vec<_> = daypart_songs.iter().map(|s| s.id).collect();
        let mut tag_ids: Vec<_> = tag_songs.iter().map(|s| s.id).collect();
        daypart_ids.sort();
        tag_ids.sort();
        assert_eq!(
            daypart_ids, tag_ids,
            "a daypart: spec's trailing name must resolve to the same songs as a tag: spec (mode All orders randomly, so order isn't compared)"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_songs_for_spec_daypart_empty_name_uses_random_fill() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=5 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, source, unavailable) VALUES (?1, 'A', ?2, 1, 0)",
                    params![
                        format!("Song {i}"),
                        if i % 2 == 0 { "Jazz" } else { "Blues" },
                    ],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let songs = manager
            .songs_for_spec("daypart:morning:2026-09-03:", QueuePopulationMode::All)
            .unwrap();

        assert_eq!(
            songs.len(),
            5,
            "empty trailing name marks the random-fill fallback — every available song, capped by DAYPART_RANDOM_FILL_LIMIT"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
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
    fn test_reconcile_leaves_random_fill_daypart_mix_alone() {
        // A random-fill Daypart Mix gets a fresh random sample on each query,
        // so reconciling it against one would swap its whole membership on
        // every library/stats event.
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=20 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, path, source, unavailable) VALUES (?1, 'A', 'Jazz', ?2, 1, 0)",
                    params![format!("Song {i}"), format!("/s{i}.mp3")],
                )
                .unwrap();
            }
        }
        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Morning Mix").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "daypart:morning:2026-09-03:")
            .unwrap();
        let before: Vec<_> = manager
            .get_playlist_tracks(pl.id)
            .unwrap()
            .into_iter()
            .map(|i| i.uuid)
            .collect();
        assert!(!before.is_empty());

        let deltas = manager.reconcile_dynamic_playlists().unwrap();
        assert!(deltas.is_empty(), "random-fill mix must not be reconciled");
        let after: Vec<_> = manager
            .get_playlist_tracks(pl.id)
            .unwrap()
            .into_iter()
            .map(|i| i.uuid)
            .collect();
        assert_eq!(before, after);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_apply_skips_match_whose_spec_changed_after_it_was_computed() {
        // Matching runs without the playlists lock, so the spec can change
        // between computing a match and applying it.
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for (i, genre) in ["Rock", "Jazz"].iter().enumerate() {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, path) VALUES (?1, 'A', ?2, ?3)",
                    params![format!("Song {}", i + 1), genre, format!("/s{}.mp3", i + 1)],
                )
                .unwrap();
            }
        }
        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Smart").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "genre:Rock")
            .unwrap();
        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute("UPDATE songs SET genre = 'Rock' WHERE title = 'Song 2'", [])
                .unwrap();
        }
        let matches = match_dynamic_playlists(&db_arc).unwrap();

        manager
            .set_playlist_dynamic_spec(pl.id, "genre:Jazz")
            .unwrap();
        let jazz: Vec<_> = manager
            .get_playlist_tracks(pl.id)
            .unwrap()
            .into_iter()
            .map(|i| i.uuid)
            .collect();

        let deltas = manager.apply_dynamic_matches(matches).unwrap();
        assert!(deltas.is_empty(), "stale genre:Rock match must be dropped");
        let after: Vec<_> = manager
            .get_playlist_tracks(pl.id)
            .unwrap()
            .into_iter()
            .map(|i| i.uuid)
            .collect();
        assert_eq!(jazz, after);

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

    #[test]
    fn test_songs_for_spec_daypart_empty_tag_matches_falls_back_to_random_fill() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=5 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, source, unavailable) VALUES (?1, 'A', 'Rock', 1, 0)",
                    params![format!("Rock Song {i}")],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        // A genre tag that matches 0 songs in the library must fall back to random library fill
        let songs = manager
            .songs_for_spec(
                "daypart:morning:2026-09-12:NonExistentGenre",
                QueuePopulationMode::All,
            )
            .unwrap();

        assert_eq!(
            songs.len(),
            5,
            "an empty genre match in a daypart spec must fall back to random library fill rather than returning 0 songs"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_playlist_paths_never_hold_one_connection_while_taking_another() {
        // Regression test: each of these held a pooled connection while a
        // helper took a second, so enough concurrent callers could exhaust
        // the pool and stall every DB caller for r2d2's 30s timeout. With a
        // one-connection pool, any nested acquisition times out instead.
        let (db, temp_dir) = setup_test_db();
        {
            let conn = db.pool.get().unwrap();
            for i in 1..=30 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, path, source, unavailable) VALUES (?1, 'A', 'Rock', ?2, 1, 0)",
                    params![format!("Song {i}"), format!("/s{i}.mp3")],
                )
                .unwrap();
            }
        }
        let single = std::sync::Arc::new(Database {
            pool: r2d2::Pool::builder()
                .max_size(1)
                .connection_timeout(std::time::Duration::from_secs(2))
                .build(r2d2_sqlite::SqliteConnectionManager::file(
                    temp_dir.join("luminous.db"),
                ))
                .unwrap(),
            schema_version: db.schema_version,
        });
        drop(db);

        let mut manager = PlaylistManager::new(single).unwrap();
        let pl = manager.create_playlist("Rock Smart").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "genre:Rock")
            .unwrap();

        let items = manager.get_playlist_tracks(pl.id).unwrap();
        manager
            .reorder_playlist_item_by_uuid(pl.id, &items[0].uuid, &items[2].uuid)
            .unwrap();

        manager
            .export_playlist(pl.id, temp_dir.join("out.m3u"), false)
            .unwrap();

        manager.sync_daypart_auto_playlist().unwrap();

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
