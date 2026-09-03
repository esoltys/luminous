//! System-managed "auto-playlist" sync — genre, decade, and BPM. Split out
//! of `playlist.rs` (#577 item 18) — a second `impl PlaylistManager` block
//! alongside the one in `playlist.rs` that owns CRUD/dynamic-playlist logic.

use super::{PlaylistManager, NO_SONG_LIMIT};
use crate::collection::CollectionScanner;
use crate::models::QueuePopulationMode;
use crate::tags::TagManager;
use anyhow::Result;
use chrono::Timelike;
use rand::seq::IndexedRandom;
use rusqlite::params;
use std::collections::HashSet;
use uuid::Uuid;

/// Minimum number of matching library songs required before a genre/decade
/// auto-playlist is created. Once created, an auto-playlist is populated
/// with every matching song (see [`NO_SONG_LIMIT`]), not just this many.
const MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST: i64 = 25;

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

/// The four fixed Daypart Mix buckets (#223): local-hour ranges deliberately
/// mirror `HomeView.svelte`'s `getTimeOfDayGreeting()` (05-11 / 12-16 /
/// 17-20 / else) so the Home greeting and the Daypart Mix card always agree
/// on "what part of the day is it" — if one changes, the other must too.
/// Pure/hour-only so boundary edges (04:59 vs 05:00, etc.) are directly
/// unit-testable without touching the wall clock.
fn daypart_bucket_for_hour(hour: u32) -> (&'static str, &'static str) {
    match hour {
        5..=11 => ("morning", "Morning Mix"),
        12..=16 => ("afternoon", "Afternoon Mix"),
        17..=20 => ("evening", "Evening Mix"),
        _ => ("latenight", "Late Night Mix"),
    }
}

/// Bucket id, display name, and local calendar date (the `dynamic_spec`
/// reroll cache key — see [`PlaylistManager::sync_daypart_auto_playlist`])
/// for a given local timestamp. Takes `now` as a parameter rather than
/// calling `Local::now()` internally so tests can inject arbitrary times.
fn daypart_bucket_and_date(
    now: chrono::DateTime<chrono::Local>,
) -> (&'static str, &'static str, String) {
    let (bucket, name) = daypart_bucket_for_hour(now.hour());
    (bucket, name, now.date_naive().to_string())
}

impl PlaylistManager {
    /// Runs all three auto-playlist syncs (genre, decade, BPM) in one call —
    /// the frontend used to invoke these as three separate IPC round trips
    /// in lockstep at every call site.
    pub fn sync_all_auto_playlists(&self) -> Result<()> {
        self.sync_genre_auto_playlists()?;
        self.sync_decade_auto_playlists()?;
        self.sync_bpm_auto_playlists()?;
        self.sync_artist_tag_auto_playlists()?;
        self.sync_missing_metadata_auto_playlist()?;
        self.sync_daypart_auto_playlist()?;
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

    /// Regenerates each artist tag "auto-playlist" — a system-managed `playlists` row
    /// with `dynamic_enabled = 1` and `dynamic_spec` set to `artisttag:<tag>` (e.g. `artisttag:canadian`) — if
    /// it's missing or its `updated` timestamp is more than 24h old, and prunes
    /// rows for artist tags no longer present in the library.
    pub fn sync_artist_tag_auto_playlists(&self) -> Result<()> {
        const STALE_AFTER_SECS: i64 = 24 * 60 * 60;

        let scanner = CollectionScanner::new(self.db.clone());
        let tags = scanner.get_library_artist_tags()?;
        let conn = self.db.pool.get()?;
        let now = chrono::Utc::now().timestamp();

        let mut stmt = conn.prepare(
            "SELECT id, dynamic_spec FROM playlists WHERE dynamic_enabled = 1 AND dynamic_spec LIKE 'artisttag:%'",
        )?;
        let existing: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        for (id, spec) in &existing {
            let tag = spec.strip_prefix("artisttag:").unwrap_or(spec);
            if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                conn.execute(
                    "DELETE FROM playlist_items WHERE playlist_id = ?1",
                    params![id],
                )?;
                conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
            }
        }

        for tag in &tags {
            let spec = format!("artisttag:{}", tag);
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
            // has at least MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST total songs for this artist tag
            // (QueuePopulationMode::All).
            // Once created, an auto-playlist is only pruned if all songs for this tag are removed (0 songs).
            let total_songs = scanner.get_songs_by_artist_tag(
                tag,
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

            let songs = scanner.get_songs_by_artist_tag(tag, NO_SONG_LIMIT, mode)?;

            let needs_generation = match existing_row {
                None => true,
                Some((_, updated, _, _)) => now - updated > STALE_AFTER_SECS,
            };
            if !needs_generation {
                continue;
            }

            let display_name = to_title_case(tag);
            let playlist_id = match existing_row {
                Some((id, _, _, _)) => {
                    conn.execute(
                        "UPDATE playlists SET name = ?1, updated = ?2 WHERE id = ?3",
                        params![display_name, now, id],
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
                        params![display_name, spec, now],
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

    /// Regenerates the single "Missing Metadata" auto-playlist (#367) — a
    /// system-managed `playlists` row with `dynamic_enabled = 1` and
    /// `dynamic_spec = "missingmeta"` — if missing or its `updated`
    /// timestamp is more than 24h old. Unlike genre/decade/BPM/artist-tag
    /// auto-playlists there is exactly one row and no
    /// `MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST` gate: the point is to surface
    /// library health issues even when only one song is affected. The row
    /// is created unconditionally (even at 0 matches) so it never has to
    /// wait for a first offending song; the frontend hides the card when
    /// `track_count == 0`, the same convention used for genre/decade/BPM at
    /// 0 songs.
    pub fn sync_missing_metadata_auto_playlist(&self) -> Result<()> {
        const STALE_AFTER_SECS: i64 = 24 * 60 * 60;
        const SPEC: &str = "missingmeta";
        const NAME: &str = "Missing Metadata";

        let scanner = CollectionScanner::new(self.db.clone());
        let conn = self.db.pool.get()?;
        let now = chrono::Utc::now().timestamp();

        let existing_row: Option<(i64, i64, String)> = conn
            .query_row(
                "SELECT id, COALESCE(updated, 0), COALESCE(population_mode, 'all') FROM playlists WHERE dynamic_enabled = 1 AND dynamic_spec = ?1",
                params![SPEC],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();
        let mode = existing_row
            .as_ref()
            .map(|(_, _, m)| QueuePopulationMode::from(m.as_str()))
            .unwrap_or_default();

        let needs_generation = match existing_row {
            None => true,
            Some((_, updated, _)) => now - updated > STALE_AFTER_SECS,
        };
        if !needs_generation {
            return Ok(());
        }

        let songs = scanner.get_songs_missing_core_tags(NO_SONG_LIMIT, mode)?;

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
                    "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec, created, updated) VALUES (?1, 1, ?2, ?3, ?3)",
                    params![NAME, SPEC, now],
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

        Ok(())
    }

    /// Picks a genre grouping for the Daypart Mix (#223) by walking the
    /// curated Genre hierarchy (`TagManager::get_tag_hierarchy`, #548): pick
    /// a random node — a top-level group or one of its curated children —
    /// and resolve it to a name with enough songs to be worth a mix.
    ///
    /// A picked child under the 25-song minimum on its own (e.g. "Soft
    /// Rock" with 10 songs) walks up to its parent group instead (e.g.
    /// "Rock" with 408) rather than falling straight to a library-wide
    /// shuffle — the parent's rollup already includes every curated child,
    /// so this still reads as a coherent, related-genre mix. Returns `None`
    /// (the random-fill fallback marker) when there's no curated hierarchy
    /// at all, or the resolved grouping is still under the threshold even
    /// at the parent level (a small/sparse library).
    fn pick_daypart_genre_grouping(&self) -> Result<Option<String>> {
        let tag_manager = TagManager::new(self.db.clone());
        // Self-contained and idempotent, same rationale as
        // `sync_genre_auto_playlists` — don't rely on `sync_genre_auto_playlists`
        // having already run earlier in the same `sync_all_auto_playlists()`
        // call (or the async `library-changed` listener) to have populated
        // `tag_groups`/`tag_assignments` first.
        tag_manager.reconcile_hierarchy()?;
        let hierarchy = tag_manager.get_tag_hierarchy()?;

        enum Candidate<'a> {
            Group {
                name: &'a str,
                song_count: i64,
            },
            Child {
                name: &'a str,
                song_count: i64,
                parent_name: &'a str,
                parent_count: i64,
            },
        }

        let mut candidates: Vec<Candidate> = Vec::new();
        for group in &hierarchy {
            candidates.push(Candidate::Group {
                name: &group.name,
                song_count: group.song_count,
            });
            for child in &group.children {
                candidates.push(Candidate::Child {
                    name: &child.name,
                    song_count: child.song_count,
                    parent_name: &group.name,
                    parent_count: group.song_count,
                });
            }
        }

        let Some(picked) = candidates.choose(&mut rand::rng()) else {
            return Ok(None);
        };

        let (resolved_name, resolved_count) = match picked {
            Candidate::Group { name, song_count } => (*name, *song_count),
            Candidate::Child {
                name,
                song_count,
                parent_name,
                parent_count,
            } => {
                if *song_count >= MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST {
                    (*name, *song_count)
                } else {
                    (*parent_name, *parent_count)
                }
            }
        };

        if resolved_count < MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST {
            return Ok(None);
        }
        Ok(Some(resolved_name.to_string()))
    }

    /// Regenerates the single "Daypart Mix" auto-playlist (#223) — a
    /// system-managed `playlists` row with `dynamic_enabled = 1` and
    /// `dynamic_spec = "daypart:<bucket>:<local-date>:<resolved-name>"`.
    /// Unlike every other auto-playlist category, there is exactly ONE row
    /// for all four dayparts (Morning/Afternoon/Evening/Late Night) — the
    /// same row's `name` and `dynamic_spec` are rewritten in place as the
    /// local-time bucket changes, rather than materializing one row per
    /// bucket. This is what lets a Home-page pin (keyed on `Playlist.id`,
    /// not on `dynamic_spec`) survive every boundary crossing untouched.
    ///
    /// Reroll timing: the genre grouping is only re-picked when the row's
    /// stored `bucket:date` prefix no longer matches today's real local
    /// bucket/date (see [`daypart_bucket_and_date`]) — repeated calls within
    /// the same bucket on the same calendar day are a no-op, so this is safe
    /// to call from every `sync_all_auto_playlists()` call site (startup,
    /// `finish_scan`, manual refresh, and the frontend's periodic
    /// boundary-check timer) without thrashing the selection.
    ///
    /// Playback continuity (confirmed design decision, #223): this does a
    /// full delete+reinsert of `playlist_items` — the same full-rewrite
    /// shape every other `sync_*_auto_playlists` function already uses —
    /// and deliberately does NOT touch `AppState::player`'s in-memory queue.
    /// A currently-playing session is therefore never interrupted, skipped,
    /// or reordered by a boundary crossing; it simply keeps playing its
    /// already-loaded songs; the DB becomes correct immediately, and a
    /// long-running live queue only catches up the next time the playlist
    /// is loaded/replayed. This is deliberate, not an oversight: hot-
    /// relabeling an in-progress queue's upcoming tracks was considered and
    /// rejected as unnecessary complexity for a rare edge case.
    pub fn sync_daypart_auto_playlist(&self) -> Result<()> {
        const SPEC_PREFIX: &str = "daypart:";

        let (bucket, bucket_name, today) = daypart_bucket_and_date(chrono::Local::now());

        let conn = self.db.pool.get()?;
        let now = chrono::Utc::now().timestamp();

        let existing_row: Option<(i64, Option<String>, String)> = conn
            .query_row(
                "SELECT id, dynamic_spec, COALESCE(population_mode, 'all') FROM playlists WHERE dynamic_enabled = 1 AND dynamic_spec LIKE 'daypart:%'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        if let Some((_, Some(spec), _)) = &existing_row {
            let mut parts = spec.strip_prefix(SPEC_PREFIX).unwrap_or("").splitn(3, ':');
            let stored_bucket = parts.next().unwrap_or("");
            let stored_date = parts.next().unwrap_or("");
            if stored_bucket == bucket && stored_date == today {
                // Already correct for today's bucket — no reroll, no rewrite.
                return Ok(());
            }
        }

        let mode = existing_row
            .as_ref()
            .map(|(_, _, m)| QueuePopulationMode::from(m.as_str()))
            .unwrap_or_default();

        let resolved_name = self.pick_daypart_genre_grouping()?.unwrap_or_default();
        let new_spec = format!("{SPEC_PREFIX}{bucket}:{today}:{resolved_name}");
        let songs = self.songs_for_spec(&new_spec, mode)?;

        // Creation gate: only skip creating a brand-new row if this pass
        // didn't find enough songs (mirrors genre/decade/BPM's "need an
        // existing row OR >= threshold songs" gate). An existing row is
        // always updated in place regardless of the new count — same
        // tolerance genre/decade/BPM already have once created.
        if existing_row.is_none() && songs.len() < MIN_LIBRARY_SONGS_FOR_AUTO_PLAYLIST as usize {
            return Ok(());
        }

        let playlist_id = match &existing_row {
            Some((id, _, _)) => {
                conn.execute(
                    "UPDATE playlists SET name = ?1, dynamic_spec = ?2, updated = ?3 WHERE id = ?4",
                    params![bucket_name, new_spec, now, id],
                )?;
                conn.execute(
                    "DELETE FROM playlist_items WHERE playlist_id = ?1",
                    params![id],
                )?;
                *id
            }
            None => {
                conn.execute(
                    "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec, created, updated) VALUES (?1, 1, ?2, ?3, ?3)",
                    params![bucket_name, new_spec, now],
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

        Ok(())
    }
}

/// Converts a lowercase or normalized tag string into Title Case (e.g. "canadian" -> "Canadian", "prog-rock" -> "Prog-Rock").
fn to_title_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            if capitalize_next {
                result.extend(ch.to_uppercase());
                capitalize_next = false;
            } else {
                result.extend(ch.to_lowercase());
            }
        } else {
            result.push(ch);
            capitalize_next = true;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup_test_db() -> (Database, std::path::PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_playlist_auto_sync_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        (db, temp_dir)
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
    fn test_sync_artist_tag_auto_playlists() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();

            // Insert artist profiles
            conn.execute(
                "INSERT INTO artist_profiles (artist_key, tags) VALUES ('Rush', json('[\"canadian\", \"prog-rock\"]'))",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO artist_profiles (artist_key, tags) VALUES ('IndieArtist', json('[\"indie\"]'))",
                [],
            )
            .unwrap();

            // Insert 25 songs for Rush
            for i in 1..=25 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, artist, source, unavailable) VALUES ('Rush Song {}', 'Rush', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }

            // Insert 1 song for IndieArtist (< 25 songs threshold)
            conn.execute(
                "INSERT INTO songs (title, artist, source, unavailable) VALUES ('Indie Song 1', 'IndieArtist', 1, 0)",
                [],
            )
            .unwrap();
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_artist_tag_auto_playlists().unwrap();

        let playlists = manager.get_playlists().unwrap();
        let canadian_pl = playlists
            .iter()
            .find(|p| p.dynamic_spec.as_deref() == Some("artisttag:canadian"))
            .expect("Rush has 25 songs so artisttag:canadian should be created");
        assert_eq!(
            canadian_pl.name, "Canadian",
            "artist tag playlist name must be Title Case"
        );

        let prog_pl = playlists
            .iter()
            .find(|p| p.dynamic_spec.as_deref() == Some("artisttag:prog-rock"))
            .expect("Rush has 25 songs so artisttag:prog-rock should be created");
        assert_eq!(
            prog_pl.name, "Prog-Rock",
            "artist tag playlist name must be Title Case"
        );

        assert!(
            !playlists
                .iter()
                .any(|p| p.dynamic_spec.as_deref() == Some("artisttag:indie")),
            "IndieArtist has 1 song (< 25) so artisttag:indie should not be created"
        );

        // Deleting Rush songs should prune the artisttag playlists
        db_arc
            .pool
            .get()
            .unwrap()
            .execute("DELETE FROM songs WHERE artist = 'Rush'", [])
            .unwrap();

        manager.sync_artist_tag_auto_playlists().unwrap();
        let updated_playlists = manager.get_playlists().unwrap();
        assert!(
            !updated_playlists
                .iter()
                .any(|p| p.dynamic_spec.as_deref() == Some("artisttag:canadian")),
            "artisttag:canadian must be pruned when 0 songs remain"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_missing_metadata_auto_playlist() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            // Complete — should not appear.
            conn.execute(
                "INSERT INTO songs (title, artist, album, source, unavailable) VALUES ('Complete Song', 'Artist', 'Album', 1, 0)",
                [],
            )
            .unwrap();
            // Missing artist (empty string, not NULL).
            conn.execute(
                "INSERT INTO songs (title, artist, album, source, unavailable) VALUES ('No Artist', '', 'Album', 1, 0)",
                [],
            )
            .unwrap();
            // Missing album (NULL).
            conn.execute(
                "INSERT INTO songs (title, artist, source, unavailable) VALUES ('No Album', 'Artist', 1, 0)",
                [],
            )
            .unwrap();
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        // Unlike genre/decade/BPM/artist-tag, this must be created even with
        // only a couple of matching songs — no 25-song threshold gate.
        manager.sync_missing_metadata_auto_playlist().unwrap();

        let playlists = manager.get_playlists().unwrap();
        let pl = playlists
            .iter()
            .find(|p| p.dynamic_spec.as_deref() == Some("missingmeta"))
            .expect("Missing Metadata auto-playlist should always be created, even below any song-count threshold");
        assert_eq!(pl.name, "Missing Metadata");

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        let titles: Vec<_> = tracks
            .iter()
            .map(|t| t.song.as_ref().unwrap().title.clone().unwrap())
            .collect();
        assert_eq!(titles.len(), 2);
        assert!(titles.contains(&"No Artist".to_string()));
        assert!(titles.contains(&"No Album".to_string()));
        assert!(!titles.contains(&"Complete Song".to_string()));

        // Fixing the tags and reconciling should drop the songs out again.
        db_arc
            .pool
            .get()
            .unwrap()
            .execute(
                "UPDATE songs SET artist = 'Artist' WHERE title = 'No Artist'",
                [],
            )
            .unwrap();
        let mut manager = manager;
        let deltas = manager.reconcile_dynamic_playlists().unwrap();
        assert!(deltas.iter().any(|d| d.playlist_id == pl.id));
        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(
            tracks[0].song.as_ref().unwrap().title.as_deref(),
            Some("No Album")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    // -----------------------------------------------------------------------
    // Daypart Mix auto-playlist (#223)
    // -----------------------------------------------------------------------

    #[test]
    fn test_daypart_bucket_for_hour_matches_home_greeting_boundaries() {
        assert_eq!(daypart_bucket_for_hour(5).0, "morning");
        assert_eq!(daypart_bucket_for_hour(11).0, "morning");
        assert_eq!(daypart_bucket_for_hour(12).0, "afternoon");
        assert_eq!(daypart_bucket_for_hour(16).0, "afternoon");
        assert_eq!(daypart_bucket_for_hour(17).0, "evening");
        assert_eq!(daypart_bucket_for_hour(20).0, "evening");
        assert_eq!(daypart_bucket_for_hour(21).0, "latenight");
        assert_eq!(daypart_bucket_for_hour(4).0, "latenight");
        assert_eq!(daypart_bucket_for_hour(0).0, "latenight");
    }

    #[test]
    fn test_sync_daypart_creates_singleton_row_with_bucket_name() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=30 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Rock Song {}', 'Rock', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_daypart_auto_playlist().unwrap();

        let playlists = manager.get_playlists().unwrap();
        let daypart_playlists: Vec<_> = playlists
            .iter()
            .filter(|p| {
                p.dynamic_enabled
                    && p.dynamic_spec
                        .as_deref()
                        .unwrap_or("")
                        .starts_with("daypart:")
            })
            .collect();

        assert_eq!(
            daypart_playlists.len(),
            1,
            "exactly one Daypart Mix row must exist, never one per bucket"
        );
        assert!(daypart_playlists[0].name.ends_with("Mix"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_daypart_does_not_reroll_within_same_bucket_and_date() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=30 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Rock Song {}', 'Rock', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_daypart_auto_playlist().unwrap();

        let playlists = manager.get_playlists().unwrap();
        let first = playlists
            .iter()
            .find(|p| {
                p.dynamic_spec
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("daypart:")
            })
            .unwrap()
            .clone();

        // Repeated calls within the same bucket/date must be a complete
        // no-op (same `updated` timestamp, same spec) — this is what keeps
        // every other `sync_all_auto_playlists()` call site (finish_scan,
        // manual refresh, mount) from thrashing the genre selection.
        manager.sync_daypart_auto_playlist().unwrap();
        manager.sync_daypart_auto_playlist().unwrap();

        let playlists_after = manager.get_playlists().unwrap();
        let after = playlists_after
            .iter()
            .find(|p| {
                p.dynamic_spec
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("daypart:")
            })
            .unwrap();

        assert_eq!(first.id, after.id);
        assert_eq!(first.dynamic_spec, after.dynamic_spec);
        assert_eq!(first.updated, after.updated);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_daypart_renames_and_repopulates_same_row_across_bucket_change() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=30 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Rock Song {}', 'Rock', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_daypart_auto_playlist().unwrap();

        let playlists = manager.get_playlists().unwrap();
        let before = playlists
            .iter()
            .find(|p| {
                p.dynamic_spec
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("daypart:")
            })
            .unwrap()
            .clone();

        // Simulate a boundary crossing by directly rewriting the stored spec
        // to an earlier bucket/date than "now" would ever compute, so the
        // next sync is forced to treat it as stale and regenerate — same
        // effect as time actually advancing, without depending on the wall
        // clock in the test.
        const STALE_SPEC: &str = "daypart:latenight:2000-01-01:Rock";
        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "UPDATE playlists SET dynamic_spec = ?1 WHERE id = ?2",
                params![STALE_SPEC, before.id],
            )
            .unwrap();
        }

        manager.sync_daypart_auto_playlist().unwrap();

        let playlists_after = manager.get_playlists().unwrap();
        let after = playlists_after
            .iter()
            .find(|p| {
                p.dynamic_spec
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("daypart:")
            })
            .unwrap();

        assert_eq!(
            before.id, after.id,
            "boundary crossing must rewrite the SAME row, never create a second one"
        );
        assert_ne!(
            after.dynamic_spec.as_deref(),
            Some(STALE_SPEC),
            "the stale bucket/date must trigger a regen with a fresh spec, not be left as-is"
        );
        assert_eq!(
            playlists_after
                .iter()
                .filter(|p| p
                    .dynamic_spec
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("daypart:"))
                .count(),
            1,
            "still exactly one Daypart Mix row after the boundary crossing"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_daypart_falls_back_to_parent_group_when_child_below_threshold() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            // 10 "Soft Rock" songs (curated as a child of "Rock") — below the
            // 25-song minimum on its own.
            for i in 1..=10 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Soft Rock Song {}', 'Rock; Soft Rock', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
            // 398 more plain "Rock" songs, so the "Rock" group rolls up to
            // 408 total — the owner's own Soft Rock (10) / Rock (408) example.
            for i in 1..=398 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Rock Song {}', 'Rock', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        // Only one curated grouping exists ("Rock" / "Soft Rock"), so the
        // random pick is deterministic regardless of which node it lands on:
        // "Rock" resolves directly, and "Soft Rock" (10 songs) must walk up
        // to "Rock" (408) rather than falling to a random-library-fill.
        manager.sync_daypart_auto_playlist().unwrap();

        let playlists = manager.get_playlists().unwrap();
        let daypart_pl = playlists
            .iter()
            .find(|p| {
                p.dynamic_spec
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("daypart:")
            })
            .expect("Daypart Mix should be created");

        assert!(
            daypart_pl
                .dynamic_spec
                .as_deref()
                .unwrap()
                .ends_with(":Rock"),
            "must resolve to the parent group \"Rock\", not the undersized child \"Soft Rock\": got {:?}",
            daypart_pl.dynamic_spec
        );
        assert_eq!(
            manager.get_playlist_tracks(daypart_pl.id).unwrap().len(),
            408
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_daypart_falls_back_to_random_fill_when_no_grouping_clears_threshold() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            // Two curated groups, each with only 15 songs (below 25) and no
            // children to roll up into — total library is 30, well above 25,
            // but no single grouping clears the threshold on its own.
            for i in 1..=15 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Jazz Song {}', 'Jazz', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
            for i in 1..=15 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Blues Song {}', 'Blues', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_daypart_auto_playlist().unwrap();

        let playlists = manager.get_playlists().unwrap();
        let daypart_pl = playlists
            .iter()
            .find(|p| {
                p.dynamic_spec
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("daypart:")
            })
            .expect("Daypart Mix should still be created via random-fill fallback");

        assert!(
            daypart_pl.dynamic_spec.as_deref().unwrap().ends_with(':'),
            "empty trailing name marks the random-fill fallback: got {:?}",
            daypart_pl.dynamic_spec
        );
        assert_eq!(
            manager.get_playlist_tracks(daypart_pl.id).unwrap().len(),
            30,
            "random-fill should include the whole (small) library"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_daypart_skips_creation_when_library_too_small() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=10 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Song {}', 'Rock', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_daypart_auto_playlist().unwrap();

        let playlists = manager.get_playlists().unwrap();
        assert!(
            !playlists.iter().any(|p| p
                .dynamic_spec
                .as_deref()
                .unwrap_or("")
                .starts_with("daypart:")),
            "a library with only 10 songs total must not get a Daypart Mix"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_sync_daypart_tolerant_once_created_below_threshold() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=30 {
                conn.execute(
                    &format!(
                        "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Rock Song {}', 'Rock', 1, 0)",
                        i
                    ),
                    [],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        manager.sync_daypart_auto_playlist().unwrap();
        let playlists = manager.get_playlists().unwrap();
        let daypart_pl = playlists
            .iter()
            .find(|p| {
                p.dynamic_spec
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("daypart:")
            })
            .unwrap();
        let id = daypart_pl.id;

        // Force a "stale" spec (as in the boundary-change test above) but
        // shrink the library below the threshold first — once created, a
        // reroll landing below 25 songs must still update in place, not
        // delete the row (same tolerance genre/decade/BPM already have).
        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "UPDATE playlists SET dynamic_spec = 'daypart:latenight:2000-01-01:Rock' WHERE id = ?1",
                params![id],
            )
            .unwrap();
            conn.execute(
                "DELETE FROM songs WHERE id NOT IN (SELECT id FROM songs LIMIT 5)",
                [],
            )
            .unwrap();
        }

        manager.sync_daypart_auto_playlist().unwrap();

        let playlists_after = manager.get_playlists().unwrap();
        assert!(
            playlists_after.iter().any(|p| p.id == id),
            "Daypart Mix must not be deleted just because a reroll landed below 25 songs"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
