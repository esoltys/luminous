//! Genre/tag browsing (#224) — reads the existing `songs.genre` column
//! (already multi-value, `; `-delimited, since #143) rather than a separate
//! tags table. Genre *is* the tag system; there is no parallel DB-native tag
//! concept distinct from it. Editing a song's genre (and therefore its tags)
//! goes through the existing full tag editor (`tageditor.rs`/`save_song_tags`),
//! which already writes both the embedded file tag and this column.
//!
//! No bundled genre taxonomy: the "Genre" browse view's hierarchy is derived
//! purely from how the user orders each song's own genre values — the first
//! value is treated as the song's main category, every other value on that
//! song as a subgenre of it (see `get_genre_graph`).

use crate::{
    collection::{mode_query_fragments, row_to_song, SONG_SELECT_COLS},
    db::Database,
    models::{
        join_multi_value, parse_multi_value, GenreGroup, QueuePopulationMode, Song, Tag, TagCount,
        TagGroup, TagGroupChild,
    },
};
use anyhow::Result;
use rusqlite::params;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Number of hues in the Genres page's fixed curated palette — colors are
/// stored as an index into it (`tag_groups.color_index`), the actual hue
/// values only matter to the frontend's swatch rendering.
const PALETTE_SIZE: i32 = 10;

/// Converts a caller-supplied `limit` into a `.take()` bound, honoring the
/// same "negative means no limit" convention `playlist.rs`'s `NO_SONG_LIMIT`
/// uses for SQL `LIMIT` clauses — `limit.max(0) as usize` would otherwise
/// silently truncate a "no limit" (-1) call to zero results, since these
/// component-match methods filter/take in Rust rather than in SQL.
fn take_limit(limit: i64) -> usize {
    if limit < 0 {
        usize::MAX
    } else {
        limit as usize
    }
}

#[derive(Debug)]
pub struct TagManager {
    db: Arc<Database>,
}

impl TagManager {
    /// Self-heals `tag_groups`/`tag_assignments` on every construction rather
    /// than trusting migration 18's `schema_version` bookkeeping alone to
    /// have actually created them — cheap (idempotent `CREATE TABLE IF NOT
    /// EXISTS`) and independent of whatever state a database's version
    /// counter is in, so a database that somehow skipped migration 18 (or
    /// had it only partially apply) still gets working tables instead of a
    /// permanent "no such table" error on every hierarchy read/write.
    pub fn new(db: Arc<Database>) -> Self {
        if let Ok(conn) = db.pool.get() {
            if let Err(e) = conn.execute_batch(crate::db::TAG_HIERARCHY_TABLES_SQL) {
                log::error!("Failed to ensure tag_groups/tag_assignments tables exist: {e}");
            }
        }
        Self { db }
    }

    /// Every non-empty `songs.genre` value in the library, in storage order,
    /// for scanned/local songs that are currently available.
    fn all_song_genre_lists(&self) -> Result<Vec<Vec<String>>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT genre FROM songs
             WHERE source IN (1, 2)
               AND unavailable = 0
               AND genre IS NOT NULL
               AND genre != ''",
        )?;
        let lists = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .map(|raw| parse_multi_value(&raw))
            .filter(|values| !values.is_empty())
            .collect();
        Ok(lists)
    }

    fn compute_tags(lists: &[Vec<String>]) -> Vec<Tag> {
        let mut counts: HashMap<String, (String, i64)> = HashMap::new();
        for values in lists {
            for value in values {
                let key = value.to_lowercase();
                let entry = counts.entry(key).or_insert_with(|| (value.clone(), 0));
                entry.1 += 1;
            }
        }
        let mut tags: Vec<Tag> = counts
            .into_values()
            .map(|(name, song_count)| Tag { name, song_count })
            .collect();
        tags.sort_by_key(|a| a.name.to_lowercase());
        tags
    }

    fn compute_graph(lists: &[Vec<String>]) -> Vec<GenreGroup> {
        let mut root_counts: HashMap<String, (String, i64)> = HashMap::new();
        let mut child_counts: HashMap<String, HashMap<String, (String, i64)>> = HashMap::new();

        for values in lists {
            let Some(root) = values.first() else { continue };
            let root_key = root.to_lowercase();
            let root_entry = root_counts
                .entry(root_key.clone())
                .or_insert_with(|| (root.clone(), 0));
            root_entry.1 += 1;

            let children = child_counts.entry(root_key).or_default();
            for child in &values[1..] {
                let child_key = child.to_lowercase();
                let child_entry = children
                    .entry(child_key)
                    .or_insert_with(|| (child.clone(), 0));
                child_entry.1 += 1;
            }
        }

        let mut groups: Vec<GenreGroup> = root_counts
            .into_iter()
            .map(|(root_key, (main_tag, song_count))| {
                let mut children: Vec<TagCount> = child_counts
                    .remove(&root_key)
                    .map(|m| {
                        m.into_values()
                            .map(|(name, song_count)| TagCount { name, song_count })
                            .collect()
                    })
                    .unwrap_or_default();
                children.sort_by_key(|a| a.name.to_lowercase());
                GenreGroup {
                    main_tag,
                    song_count,
                    children,
                }
            })
            .collect();
        groups.sort_by_key(|a| a.main_tag.to_lowercase());
        groups
    }

    /// Every distinct genre/tag value in use, with how many songs carry it
    /// (at any position in their genre list), ordered by name.
    pub fn list_all_tags(&self) -> Result<Vec<Tag>> {
        Ok(Self::compute_tags(&self.all_song_genre_lists()?))
    }

    /// The emergent genre hierarchy: one [`GenreGroup`] per genre value that
    /// has ever appeared *first* in a song's genre list (its main category),
    /// each carrying every value seen as a subgenre of it (later in the same
    /// song's list) across the library, aggregated with counts. A value can
    /// appear as a child under more than one group if different songs
    /// disagree about its main category — both relationships are real and
    /// both are returned.
    pub fn get_genre_graph(&self) -> Result<Vec<GenreGroup>> {
        Ok(Self::compute_graph(&self.all_song_genre_lists()?))
    }

    /// Combines [`Self::list_all_tags`] and [`Self::get_genre_graph`] into a
    /// single DB scan (each independently re-scans every song's genre column
    /// otherwise) — used by `tagsStore.load()`, which needs both together
    /// every time the Genres tab opens, so the two don't each pay for their
    /// own full scan back to back. Also returns how many songs have no genre
    /// at all (empty/NULL), so the Genres tab can surface them as their own
    /// browsable group rather than silently excluding them.
    pub fn get_tags_overview(&self) -> Result<(Vec<Tag>, Vec<GenreGroup>, i64)> {
        let lists = self.all_song_genre_lists()?;
        let no_genre_count = self.count_songs_without_genre()?;
        Ok((
            Self::compute_tags(&lists),
            Self::compute_graph(&lists),
            no_genre_count,
        ))
    }

    /// How many songs have no genre value at all (empty or NULL) — excluded
    /// from `all_song_genre_lists` entirely, so tracked separately.
    fn count_songs_without_genre(&self) -> Result<i64> {
        let conn = self.db.pool.get()?;
        let count = conn.query_row(
            "SELECT COUNT(*) FROM songs
             WHERE source IN (1, 2)
               AND unavailable = 0
               AND (genre IS NULL OR genre = '')",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Songs with no genre value at all — the "No Genre" browsable group.
    pub fn get_songs_without_genre(
        &self,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
        let sql = format!(
            "SELECT {} FROM songs
             WHERE (genre IS NULL OR genre = '')
               AND source IN (1, 2)
               AND unavailable = 0
               {extra_where}
             ORDER BY {order_by}
             LIMIT ?1",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Songs whose genre list contains `tag_name` at any position
    /// (case-insensitive, exact component match — not a substring match).
    pub fn get_songs_by_tag(
        &self,
        tag_name: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
        // LIKE is just a cheap index-assisted pre-filter (genre is indexed);
        // the real match is the exact, case-insensitive component check
        // below, which is what avoids "Rock" incorrectly matching "Prog Rock".
        let sql = format!(
            "SELECT {} FROM songs
             WHERE genre LIKE '%' || ?1 || '%'
               AND source IN (1, 2)
               AND unavailable = 0
               {extra_where}
             ORDER BY {order_by}",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let candidates: Vec<Song> = stmt
            .query_map(params![tag_name], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();

        let target = tag_name.to_lowercase();
        let songs = candidates
            .into_iter()
            .filter(|song| {
                song.genre
                    .as_deref()
                    .map(|g| {
                        parse_multi_value(g)
                            .iter()
                            .any(|v| v.to_lowercase() == target)
                    })
                    .unwrap_or(false)
            })
            .take(take_limit(limit))
            .collect();
        Ok(songs)
    }

    /// Songs whose genre list contains `group_name` itself OR any tag
    /// currently curated (`tag_assignments`) as one of its children (#548) —
    /// the curated-hierarchy analog of the old position-based
    /// `get_songs_by_main_tag`: a top-level card's membership follows
    /// curation (drag a chip onto another card, its songs move with it),
    /// not each song's own incidental genre-list order. `group_name` doesn't
    /// have to actually be a `tag_groups` row — an unrecognized name simply
    /// has no children, so this degrades to a plain [`Self::get_songs_by_tag`]
    /// call for it.
    pub fn get_songs_by_curated_group(
        &self,
        group_name: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let mut targets: HashSet<String> = {
            let mut stmt = conn.prepare(
                "SELECT tag_assignments.tag_name FROM tag_assignments
                 JOIN tag_groups ON tag_groups.id = tag_assignments.group_id
                 WHERE tag_groups.name = ?1 COLLATE NOCASE",
            )?;
            let rows = stmt.query_map(params![group_name], |r| r.get::<_, String>(0))?;
            let names: HashSet<String> = rows
                .filter_map(|r| r.ok())
                .map(|s| s.to_lowercase())
                .collect();
            names
        };
        targets.insert(group_name.to_lowercase());

        // No cheap LIKE prefilter here (unlike get_songs_by_tag) — with
        // multiple target values a single `LIKE '%'||name||'%'` would miss a
        // song whose only matching value is a child that doesn't literally
        // contain `group_name` as a substring (e.g. group "Metal" with child
        // "Doom"). Mirrors songs_containing_any's full-scan-then-filter
        // shape instead.
        let (extra_where, order_by) = mode_query_fragments(mode);
        let sql = format!(
            "SELECT {} FROM songs
             WHERE source IN (1, 2)
               AND unavailable = 0
               AND genre IS NOT NULL
               AND genre != ''
               {extra_where}
             ORDER BY {order_by}",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map([], row_to_song)?
            .filter_map(|r| r.ok())
            .filter(|song: &Song| {
                song.genre
                    .as_deref()
                    .map(|g| {
                        parse_multi_value(g)
                            .iter()
                            .any(|v| targets.contains(&v.to_lowercase()))
                    })
                    .unwrap_or(false)
            })
            .take(take_limit(limit))
            .collect();
        Ok(songs)
    }

    /// Single dispatch point for a curated-hierarchy tag lookup (#548): if
    /// `name` is currently a `tag_groups` row (a top-level card), membership
    /// includes its curated children ([`Self::get_songs_by_curated_group`]);
    /// otherwise it's treated as a leaf/chip and matched exactly at any
    /// position ([`Self::get_songs_by_tag`]) — covers both a curated child
    /// and a name not (yet) present in the hierarchy at all. The one
    /// invariant this relies on (`reconcile_hierarchy`'s self-heal pass): a
    /// name is never simultaneously a group and a child, so this dispatch
    /// never has to pick between the two.
    pub fn get_songs_by_curated_tag(
        &self,
        name: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let is_group: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tag_groups WHERE name = ?1 COLLATE NOCASE)",
            params![name],
            |r| r.get(0),
        )?;
        if is_group {
            self.get_songs_by_curated_group(name, limit, mode)
        } else {
            self.get_songs_by_tag(name, limit, mode)
        }
    }

    // -----------------------------------------------------------------
    // Persisted Genres curation hierarchy (#545): `tag_groups` (primary
    // genre cards) and `tag_assignments` (sub-genre chip -> card), layered
    // on top of the `songs.genre` values read above. See migration 18's
    // doc comment in db.rs for why this can't just reuse the emergent,
    // per-song-order graph `get_genre_graph` computes.
    // -----------------------------------------------------------------

    /// The persisted hierarchy: one [`TagGroup`] per `tag_groups` row, each
    /// with its assigned children and current song counts (any-position
    /// match, matching [`Self::list_all_tags`]'s semantics). A tag that's
    /// also the name of some `tag_groups` row never appears as a child here
    /// — `reconcile_hierarchy` strips that link before this is read.
    pub fn get_tag_hierarchy(&self) -> Result<Vec<TagGroup>> {
        let conn = self.db.pool.get()?;
        let counts = Self::compute_tags(&self.all_song_genre_lists()?)
            .into_iter()
            .map(|t| (t.name.to_lowercase(), t.song_count))
            .collect::<HashMap<_, _>>();

        let mut group_stmt = conn.prepare(
            "SELECT id, name, color_index FROM tag_groups ORDER BY sort_order, name COLLATE NOCASE",
        )?;
        let groups_raw: Vec<(i64, String, i32)> = group_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut child_stmt = conn.prepare(
            "SELECT group_id, tag_name FROM tag_assignments ORDER BY sort_order, tag_name COLLATE NOCASE",
        )?;
        let children_raw: Vec<(i64, String)> = child_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let groups = groups_raw
            .into_iter()
            .map(|(id, name, color_index)| {
                let children = children_raw
                    .iter()
                    .filter(|(group_id, _)| *group_id == id)
                    .map(|(_, child_name)| TagGroupChild {
                        song_count: counts.get(&child_name.to_lowercase()).copied().unwrap_or(0),
                        name: child_name.clone(),
                    })
                    .collect();
                TagGroup {
                    song_count: counts.get(&name.to_lowercase()).copied().unwrap_or(0),
                    color_index,
                    name,
                    children,
                }
            })
            .collect();
        Ok(groups)
    }

    /// Reconciles `tag_groups`/`tag_assignments` against tags currently in
    /// use in the library — mirrors `playlist::reconcile_dynamic_playlists`'
    /// role for dynamic playlists. Auto-creates a row for any tag name in use
    /// that has none yet (a new group if it's ever a song's main/position-0
    /// value, otherwise assigned under whichever existing root it appears
    /// under most often), and evicts any existing row for a tag name no
    /// songs use anymore. Never re-derives or moves an *existing* row — once
    /// curated (or auto-assigned), an assignment is sticky. Returns whether
    /// anything changed, so the caller can skip emitting a refresh event.
    pub fn reconcile_hierarchy(&self) -> Result<bool> {
        let conn = self.db.pool.get()?;
        let lists = self.all_song_genre_lists()?;

        let mut usage: HashMap<String, String> = HashMap::new();
        let mut is_root: HashMap<String, bool> = HashMap::new();
        let mut child_root_counts: HashMap<String, HashMap<String, i64>> = HashMap::new();

        for values in &lists {
            for (i, v) in values.iter().enumerate() {
                let key = v.to_lowercase();
                usage.entry(key.clone()).or_insert_with(|| v.clone());
                if i == 0 {
                    is_root.insert(key, true);
                }
            }
            if let Some(root) = values.first() {
                let root_key = root.to_lowercase();
                for child in &values[1..] {
                    *child_root_counts
                        .entry(child.to_lowercase())
                        .or_default()
                        .entry(root_key.clone())
                        .or_insert(0) += 1;
                }
            }
        }

        let mut changed = false;

        let mut existing_groups: HashMap<String, i64> = HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT id, name FROM tag_groups")?;
            for row in stmt
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|r| r.ok())
            {
                existing_groups.insert(row.1.to_lowercase(), row.0);
            }
        }
        let mut existing_assignments: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        {
            let mut stmt = conn.prepare("SELECT tag_name FROM tag_assignments")?;
            for name in stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .filter_map(|r| r.ok())
            {
                existing_assignments.insert(name.to_lowercase());
            }
        }

        // A top-level genre can never meaningfully nest as a sub-genre
        // elsewhere — under its own card (no separate "child instance" of
        // the same literal genre value exists to drill into) or under a
        // different one (ambiguous: is a song tagged with it under the
        // top-level genre, the sub-genre, or both?). Strip any assignment
        // whose name collides with *any* existing `tag_groups` row,
        // regardless of which group it's currently under — covers stale
        // data from before reparent_tag/demote_group_to_child/apply_merge_hierarchy
        // guarded against creating one, and is the single enforcement point
        // every write path relies on instead of duplicating this check.
        {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT tag_assignments.tag_name
                 FROM tag_assignments
                 JOIN tag_groups ON tag_groups.name = tag_assignments.tag_name COLLATE NOCASE",
            )?;
            let colliding: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .collect();
            for name in colliding {
                conn.execute(
                    "DELETE FROM tag_assignments WHERE tag_name = ?1 COLLATE NOCASE",
                    params![name],
                )?;
                existing_assignments.remove(&name.to_lowercase());
                changed = true;
            }
        }

        // Evict rows for tags no longer used anywhere in the library.
        for key in existing_groups.keys().cloned().collect::<Vec<_>>() {
            if !usage.contains_key(&key) {
                conn.execute(
                    "DELETE FROM tag_groups WHERE name = ?1 COLLATE NOCASE",
                    params![key],
                )?;
                existing_groups.remove(&key);
                changed = true;
            }
        }
        for key in existing_assignments.iter().cloned().collect::<Vec<_>>() {
            if !usage.contains_key(&key) {
                conn.execute(
                    "DELETE FROM tag_assignments WHERE tag_name = ?1 COLLATE NOCASE",
                    params![key],
                )?;
                existing_assignments.remove(&key);
                changed = true;
            }
        }

        let mut next_group_sort: i32 = conn.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM tag_groups",
            [],
            |r| r.get(0),
        )?;
        let mut group_count: i32 =
            conn.query_row("SELECT COUNT(*) FROM tag_groups", [], |r| r.get(0))?;

        let create_group = |conn: &rusqlite::Connection,
                            name: &str,
                            next_group_sort: &mut i32,
                            group_count: &mut i32|
         -> Result<i64> {
            conn.execute(
                "INSERT OR IGNORE INTO tag_groups (name, color_index, sort_order) VALUES (?1, ?2, ?3)",
                params![name, *group_count % PALETTE_SIZE, *next_group_sort],
            )?;
            let id: i64 = conn.query_row(
                "SELECT id FROM tag_groups WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |r| r.get(0),
            )?;
            *next_group_sort += 1;
            *group_count += 1;
            Ok(id)
        };

        // Auto-create a group for any tag ever used as a main/position-0
        // value, that doesn't have one yet — UNLESS it already has a curated
        // assignment from a *previous* reconcile or an explicit demote (see
        // `demote_group_to_child`). Without this exception, demoting a card
        // would get silently undone the very next time the hierarchy is read
        // (`get_tag_hierarchy` reconciles before every read): the demote only
        // touches the curated tables, never the raw `songs.genre` text, so
        // the tag is still literally position-0 somewhere and this loop
        // would otherwise recreate its root on the spot.
        for (key, name) in usage.clone() {
            if *is_root.get(&key).unwrap_or(&false)
                && !existing_groups.contains_key(&key)
                && !existing_assignments.contains(&key)
            {
                let id = create_group(&conn, &name, &mut next_group_sort, &mut group_count)?;
                existing_groups.insert(key, id);
                changed = true;
            }
        }

        // Auto-assign every tag ever observed as a subgenre (appearing after
        // position 0 on some song) that doesn't have an assignment yet, to
        // whichever root it appeared under most often — unless it's already
        // its own top-level card (created just above, in this same pass, or
        // from an earlier one). A top-level genre never gets auto-curated as
        // anyone's child; the self-heal pass at the top of this function
        // strips that link if it's ever found, so creating one here would
        // just be undone on the very next reconcile.
        let mut next_assignment_sort: i32 = conn.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM tag_assignments",
            [],
            |r| r.get(0),
        )?;
        for (key, name) in usage {
            if existing_assignments.contains(&key) || existing_groups.contains_key(&key) {
                continue;
            }
            let Some(root_counts) = child_root_counts.get(&key) else {
                continue; // never observed as a subgenre — nothing to assign.
            };
            let best_root = root_counts
                .iter()
                .max_by_key(|(_, c)| **c)
                .map(|(k, _)| k.clone());
            let group_id = match best_root.and_then(|rk| existing_groups.get(&rk).copied()) {
                Some(id) => id,
                // Shouldn't normally happen (every root got a group above),
                // but fall back to giving the tag its own group rather than
                // silently dropping it.
                None => create_group(&conn, &name, &mut next_group_sort, &mut group_count)?,
            };
            existing_groups.entry(key.clone()).or_insert(group_id);
            conn.execute(
                "INSERT OR IGNORE INTO tag_assignments (tag_name, group_id, sort_order) VALUES (?1, ?2, ?3)",
                params![name, group_id, next_assignment_sort],
            )?;
            existing_assignments.insert(key);
            next_assignment_sort += 1;
            changed = true;
        }

        Ok(changed)
    }

    pub fn set_group_color(&self, name: &str, color_index: i32) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE tag_groups SET color_index = ?1 WHERE name = ?2 COLLATE NOCASE",
            params![color_index.rem_euclid(PALETTE_SIZE), name],
        )?;
        Ok(())
    }

    /// Moves `tag_name` to be a child of `new_group_name`, creating the
    /// assignment if it doesn't have one yet. `new_group_name` must already
    /// be a `tag_groups` row (the card being dropped onto). If `tag_name` is
    /// itself a top-level genre (its own card, whether `new_group_name` or
    /// some other one), the resulting link is invalid and gets stripped by
    /// `reconcile_hierarchy`'s self-heal pass before the next read — see its
    /// doc comment; this is the single enforcement point, not duplicated
    /// here.
    pub fn reparent_tag(&self, tag_name: &str, new_group_name: &str) -> Result<()> {
        // A tag can't be curated as a sub-genre of a card sharing its own
        // name — that's a self-loop with no meaningful drill-down (there's
        // no separate "child instance" of the same literal genre value).
        if tag_name.eq_ignore_ascii_case(new_group_name) {
            return Ok(());
        }
        let conn = self.db.pool.get()?;
        let group_id: i64 = conn.query_row(
            "SELECT id FROM tag_groups WHERE name = ?1 COLLATE NOCASE",
            params![new_group_name],
            |r| r.get(0),
        )?;
        let next_sort: i32 = conn.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM tag_assignments WHERE group_id = ?1",
            params![group_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO tag_assignments (tag_name, group_id, sort_order) VALUES (?1, ?2, ?3)
             ON CONFLICT(tag_name) DO UPDATE SET group_id = excluded.group_id, sort_order = excluded.sort_order",
            params![tag_name, group_id, next_sort],
        )?;
        Ok(())
    }

    /// Demotes `tag_name` from its own primary-genre card to a sub-genre
    /// chip under `new_group_name` — the reverse of `promote_tag`, and
    /// distinct from `reparent_tag` (which only ever moves an *assignment*
    /// and never touches a separately-existing `tag_groups` row). This one
    /// specifically removes `tag_name`'s own `tag_groups` row — used when
    /// the user drags a whole card's header
    /// onto another card. Any children that were curated under the deleted
    /// card cascade-delete (`tag_assignments.group_id` is `ON DELETE CASCADE`,
    /// `PRAGMA foreign_keys=ON`) and re-home themselves on the next reconcile
    /// pass, same as any other orphaned tag.
    pub fn demote_group_to_child(&self, tag_name: &str, new_group_name: &str) -> Result<()> {
        if tag_name.eq_ignore_ascii_case(new_group_name) {
            return Ok(());
        }
        let conn = self.db.pool.get()?;
        conn.execute(
            "DELETE FROM tag_groups WHERE name = ?1 COLLATE NOCASE",
            params![tag_name],
        )?;
        let group_id: i64 = conn.query_row(
            "SELECT id FROM tag_groups WHERE name = ?1 COLLATE NOCASE",
            params![new_group_name],
            |r| r.get(0),
        )?;
        let next_sort: i32 = conn.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM tag_assignments WHERE group_id = ?1",
            params![group_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO tag_assignments (tag_name, group_id, sort_order) VALUES (?1, ?2, ?3)
             ON CONFLICT(tag_name) DO UPDATE SET group_id = excluded.group_id, sort_order = excluded.sort_order",
            params![tag_name, group_id, next_sort],
        )?;
        Ok(())
    }

    /// Promotes `tag_name` from a sub-genre chip to its own primary-genre
    /// card: removes any existing assignment and creates a `tag_groups` row.
    pub fn promote_tag(&self, tag_name: &str) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "DELETE FROM tag_assignments WHERE tag_name = ?1 COLLATE NOCASE",
            params![tag_name],
        )?;
        let next_sort: i32 = conn.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM tag_groups",
            [],
            |r| r.get(0),
        )?;
        let group_count: i32 =
            conn.query_row("SELECT COUNT(*) FROM tag_groups", [], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO tag_groups (name, color_index, sort_order) VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO NOTHING",
            params![tag_name, group_count % PALETTE_SIZE, next_sort],
        )?;
        Ok(())
    }

    /// Reorders `tag_name` to `new_index` among its group's other children.
    pub fn reorder_tag_in_group(&self, tag_name: &str, new_index: i32) -> Result<()> {
        let conn = self.db.pool.get()?;
        let group_id: i64 = conn.query_row(
            "SELECT group_id FROM tag_assignments WHERE tag_name = ?1 COLLATE NOCASE",
            params![tag_name],
            |r| r.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT tag_name FROM tag_assignments WHERE group_id = ?1 ORDER BY sort_order, tag_name COLLATE NOCASE",
        )?;
        let mut siblings: Vec<String> = stmt
            .query_map(params![group_id], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        let Some(pos) = siblings
            .iter()
            .position(|n| n.eq_ignore_ascii_case(tag_name))
        else {
            return Ok(());
        };
        let moved = siblings.remove(pos);
        let target = (new_index.max(0) as usize).min(siblings.len());
        siblings.insert(target, moved);

        let tx = conn.unchecked_transaction()?;
        for (i, name) in siblings.iter().enumerate() {
            tx.execute(
                "UPDATE tag_assignments SET sort_order = ?1 WHERE tag_name = ?2 COLLATE NOCASE",
                params![i as i32, name],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Every song whose genre list contains any of `names` (case-insensitive,
    /// exact component match), as `(song_id, path, genre)` — the working set
    /// for [`merge_tags`]/[`delete_tags`] file+DB rewrites, computed here
    /// (not by the command layer) since it needs the same component-match
    /// semantics as [`Self::get_songs_by_tag`].
    ///
    /// [`merge_tags`]: crate::commands::tags::merge_tags
    /// [`delete_tags`]: crate::commands::tags::delete_tags
    pub fn songs_containing_any(&self, names: &[String]) -> Result<Vec<(i64, String, String)>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, path, genre FROM songs
             WHERE source IN (1, 2) AND unavailable = 0 AND genre IS NOT NULL AND genre != ''",
        )?;
        let targets: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        let rows: Vec<(i64, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .filter(|(_, _, genre): &(i64, String, String)| {
                parse_multi_value(genre)
                    .iter()
                    .any(|v| targets.contains(&v.to_lowercase()))
            })
            .collect();
        Ok(rows)
    }

    /// Replaces `from` with `into` at the same position in a `; `-delimited
    /// genre string, deduping if `into` is already present elsewhere in it.
    pub fn rewrite_genre_for_merge(genre: &str, from: &str, into: &str) -> String {
        let from_lower = from.to_lowercase();
        let mut values = parse_multi_value(genre);
        let mut seen_into = false;
        for v in values.iter_mut() {
            if v.eq_ignore_ascii_case(&from_lower) {
                *v = into.to_string();
            }
        }
        let mut deduped: Vec<String> = Vec::with_capacity(values.len());
        for v in values {
            let is_into = v.eq_ignore_ascii_case(into);
            if is_into && seen_into {
                continue;
            }
            if is_into {
                seen_into = true;
            }
            deduped.push(v);
        }
        join_multi_value(&deduped)
    }

    /// Strips every name in `names` out of a `; `-delimited genre string.
    pub fn rewrite_genre_for_delete(genre: &str, names: &[String]) -> String {
        let targets: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        let kept: Vec<String> = parse_multi_value(genre)
            .into_iter()
            .filter(|v| !targets.contains(&v.to_lowercase()))
            .collect();
        join_multi_value(&kept)
    }

    /// Merges `from`'s hierarchy bookkeeping into `into` after a
    /// [`Self::rewrite_genre_for_merge`] pass over every affected song: any
    /// children `from` had as a group are reparented under `into` (creating
    /// a group for `into` if it didn't have one), then every row keyed by
    /// `from` (its group and/or its own assignment) is removed, since `from`
    /// no longer exists as a distinct name. If one of `from`'s children was
    /// already literally named `into` (or any other top-level genre), the
    /// reassignment above leaves it as an invalid link — left for
    /// `reconcile_hierarchy`'s self-heal pass to strip before the next read,
    /// same single enforcement point every other write path relies on.
    pub fn apply_merge_hierarchy(&self, from: &str, into: &str) -> Result<()> {
        let conn = self.db.pool.get()?;

        let from_group_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM tag_groups WHERE name = ?1 COLLATE NOCASE",
                params![from],
                |r| r.get(0),
            )
            .ok();

        if let Some(from_id) = from_group_id {
            let into_group_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM tag_groups WHERE name = ?1 COLLATE NOCASE",
                    params![into],
                    |r| r.get(0),
                )
                .ok();
            let into_id = match into_group_id {
                Some(id) => id,
                None => {
                    conn.execute(
                        "UPDATE tag_groups SET name = ?1 WHERE id = ?2",
                        params![into, from_id],
                    )?;
                    from_id
                }
            };
            if into_id != from_id {
                conn.execute(
                    "UPDATE OR IGNORE tag_assignments SET group_id = ?1 WHERE group_id = ?2",
                    params![into_id, from_id],
                )?;
                conn.execute("DELETE FROM tag_groups WHERE id = ?1", params![from_id])?;
            }
        }
        // Independent of the group handling above — `from` may have had an
        // assignment row of its own at the same time.
        conn.execute(
            "DELETE FROM tag_assignments WHERE tag_name = ?1 COLLATE NOCASE",
            params![from],
        )?;
        Ok(())
    }

    /// Removes hierarchy rows for deleted tag names.
    pub fn apply_delete_hierarchy(&self, names: &[String]) -> Result<()> {
        let conn = self.db.pool.get()?;
        for name in names {
            conn.execute(
                "DELETE FROM tag_groups WHERE name = ?1 COLLATE NOCASE",
                params![name],
            )?;
            conn.execute(
                "DELETE FROM tag_assignments WHERE tag_name = ?1 COLLATE NOCASE",
                params![name],
            )?;
        }
        Ok(())
    }
}

/// Reconciles the persisted Genres hierarchy against the library and, if
/// anything changed, emits `tags-changed` so the frontend can refresh —
/// mirrors `playlist::reconcile_and_sync`'s role for dynamic playlists.
/// Listened for on `library-changed` only (unlike dynamic playlists, the
/// hierarchy has nothing to do with playback stats).
pub async fn reconcile_hierarchy_and_notify(app: tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    let state = app.state::<crate::AppState>();
    let manager = TagManager::new(state.db.clone());
    match manager.reconcile_hierarchy() {
        Ok(true) => {
            let _ = app.emit("tags-changed", ());
        }
        Ok(false) => {}
        Err(e) => log::error!("Tag hierarchy reconcile failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn test_db() -> (Arc<Database>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "luminous_tags_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(dir.clone()).unwrap();
        (Arc::new(db), dir)
    }

    fn insert_song(db: &Database, path: &str, genre: &str) -> i64 {
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO songs (source, filetype, path, genre) VALUES (1, 0, ?1, ?2)",
            params![path, genre],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn test_list_all_tags_splits_and_dedupes_case_insensitively() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Symphonic Metal");
        insert_song(&db, "/b.mp3", "metal");

        let manager = TagManager::new(db.clone());
        let tags = manager.list_all_tags().unwrap();
        assert_eq!(tags.len(), 2);
        let metal = tags
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case("metal"))
            .unwrap();
        assert_eq!(metal.song_count, 2);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_get_tags_overview_matches_separate_calls() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Symphonic Metal");
        insert_song(&db, "/b.mp3", "Ambient");

        let manager = TagManager::new(db.clone());
        let (tags, graph, no_genre_count) = manager.get_tags_overview().unwrap();
        assert_eq!(tags, manager.list_all_tags().unwrap());
        assert_eq!(graph, manager.get_genre_graph().unwrap());
        assert_eq!(no_genre_count, 0);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_songs_without_genre() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal");
        insert_song(&db, "/b.mp3", "");

        let manager = TagManager::new(db.clone());
        let (_, _, no_genre_count) = manager.get_tags_overview().unwrap();
        assert_eq!(no_genre_count, 1);

        let songs = manager
            .get_songs_without_genre(50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].path.as_deref(), Some("/b.mp3"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_genre_graph_star_relationship_and_lone_root() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Progressive Metal; Symphonic Metal");
        insert_song(&db, "/b.mp3", "Ambient");

        let manager = TagManager::new(db.clone());
        let graph = manager.get_genre_graph().unwrap();

        let metal = graph.iter().find(|g| g.main_tag == "Metal").unwrap();
        assert_eq!(metal.song_count, 1);
        let child_names: Vec<&str> = metal.children.iter().map(|c| c.name.as_str()).collect();
        assert!(child_names.contains(&"Progressive Metal"));
        assert!(child_names.contains(&"Symphonic Metal"));

        let ambient = graph.iter().find(|g| g.main_tag == "Ambient").unwrap();
        assert_eq!(ambient.song_count, 1);
        assert!(ambient.children.is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_genre_graph_same_tag_under_multiple_parents() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Symphonic Metal");
        insert_song(&db, "/b.mp3", "Classical; Symphonic Metal");

        let manager = TagManager::new(db.clone());
        let graph = manager.get_genre_graph().unwrap();

        let metal = graph.iter().find(|g| g.main_tag == "Metal").unwrap();
        assert!(metal.children.iter().any(|c| c.name == "Symphonic Metal"));
        let classical = graph.iter().find(|g| g.main_tag == "Classical").unwrap();
        assert!(classical
            .children
            .iter()
            .any(|c| c.name == "Symphonic Metal"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_get_songs_by_tag_matches_exact_component_not_substring() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Rock");
        insert_song(&db, "/b.mp3", "Prog Rock");

        let manager = TagManager::new(db.clone());
        let songs = manager
            .get_songs_by_tag("rock", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].genre.as_deref(), Some("Rock"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_get_songs_by_curated_group_matches_self_and_children_not_substrings() {
        let (db, dir) = test_db();
        // "Doom" doesn't contain "Metal" as a substring — regression test for
        // the LIKE-prefilter trap get_songs_by_curated_group deliberately
        // avoids (see its doc comment). It starts out as its own root card
        // (it's position-0 on /c.mp3, with nothing else around to curate it
        // as anyone's child) and is explicitly demoted under Metal —
        // demote_group_to_child, not reparent_tag, since Doom has its own
        // pre-existing tag_groups row to remove, not just an assignment to
        // create.
        insert_song(&db, "/a.mp3", "Metal");
        insert_song(&db, "/b.mp3", "Metal; Progressive Metal");
        insert_song(&db, "/c.mp3", "Doom");
        insert_song(&db, "/d.mp3", "Prog Rock"); // unrelated, must not match

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();
        manager.demote_group_to_child("Doom", "Metal").unwrap();

        let songs = manager
            .get_songs_by_curated_group("Metal", 50, QueuePopulationMode::All)
            .unwrap();
        let genres: Vec<Option<String>> = songs.iter().map(|s| s.genre.clone()).collect();
        assert_eq!(
            songs.len(),
            3,
            "self + curated child (Progressive Metal) + demoted child (Doom)"
        );
        assert!(genres.contains(&Some("Metal".to_string())));
        assert!(genres.contains(&Some("Metal; Progressive Metal".to_string())));
        assert!(genres.contains(&Some("Doom".to_string())));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_get_songs_by_curated_group_reflects_live_reparenting() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal");
        insert_song(&db, "/b.mp3", "Ambient");
        insert_song(&db, "/c.mp3", "Drone");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();
        manager.reparent_tag("Drone", "Ambient").unwrap();

        let under_ambient = manager
            .get_songs_by_curated_group("Ambient", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(under_ambient.len(), 2, "Drone is curated under Ambient");

        // Dragging Drone onto Metal instead moves its real playlist membership.
        manager.reparent_tag("Drone", "Metal").unwrap();
        let under_ambient = manager
            .get_songs_by_curated_group("Ambient", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(under_ambient.len(), 1, "Drone moved out of Ambient");
        let under_metal = manager
            .get_songs_by_curated_group("Metal", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(under_metal.len(), 2, "Drone moved under Metal");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_get_songs_by_curated_tag_dispatches_group_vs_child() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Progressive Metal");
        insert_song(&db, "/b.mp3", "Ambient");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();

        let group_songs = manager
            .get_songs_by_curated_tag("Metal", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(
            group_songs.len(),
            1,
            "group dispatch includes its curated child"
        );

        let child_songs = manager
            .get_songs_by_curated_tag("Progressive Metal", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(
            child_songs.len(),
            1,
            "child dispatch is an exact any-position match"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A genre value containing a literal colon (e.g. from a source that
    /// allows it in tag text) must not be misclassified anywhere in the
    /// curated dispatch — it's just an ordinary string component match, the
    /// same as any other tag name.
    #[test]
    fn test_get_songs_by_curated_tag_handles_colon_in_genre_value() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Sci-Fi: Space Opera");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();

        let songs = manager
            .get_songs_by_curated_tag("Sci-Fi: Space Opera", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(songs.len(), 1);

        let _ = std::fs::remove_dir_all(dir);
    }

    // -----------------------------------------------------------------
    // Persisted Genres curation hierarchy (#545)
    // -----------------------------------------------------------------

    #[test]
    fn test_hierarchy_reflects_existing_genres_after_reconcile() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Progressive Metal");
        insert_song(&db, "/b.mp3", "Metal; Symphonic Metal");
        insert_song(&db, "/c.mp3", "Ambient");

        let manager = TagManager::new(db.clone());
        // Migration 18 only seeds from whatever's in `songs` at that moment
        // (i.e. an existing library at upgrade time) — these tests insert
        // songs into a fresh DB afterward, so reconcile (normally triggered
        // by the app's "library-changed" listener) does the initial build.
        manager.reconcile_hierarchy().unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();

        let metal = hierarchy.iter().find(|g| g.name == "Metal").unwrap();
        let child_names: Vec<&str> = metal.children.iter().map(|c| c.name.as_str()).collect();
        assert!(child_names.contains(&"Progressive Metal"));
        assert!(child_names.contains(&"Symphonic Metal"));
        assert!(hierarchy.iter().any(|g| g.name == "Ambient"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_reconcile_hierarchy_adds_new_tags_and_evicts_stale_ones() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Symphonic Metal");
        let manager = TagManager::new(db.clone());
        // First call builds the hierarchy from scratch; a second call with
        // nothing new/removed should be a no-op.
        assert!(manager.reconcile_hierarchy().unwrap());
        assert!(!manager.reconcile_hierarchy().unwrap());

        // A brand new tag appears (e.g. a fresh scan/tag edit) — reconcile should pick it up.
        insert_song(&db, "/b.mp3", "Ambient; Drone");
        assert!(manager.reconcile_hierarchy().unwrap());
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        assert!(hierarchy.iter().any(|g| g.name == "Ambient"));
        let ambient = hierarchy.iter().find(|g| g.name == "Ambient").unwrap();
        assert!(ambient.children.iter().any(|c| c.name == "Drone"));

        // Every song using "Metal" is deleted — its group should be evicted.
        db.pool
            .get()
            .unwrap()
            .execute("DELETE FROM songs WHERE path = '/a.mp3'", [])
            .unwrap();
        assert!(manager.reconcile_hierarchy().unwrap());
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        assert!(!hierarchy.iter().any(|g| g.name == "Metal"));

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A tag that's a top-level genre on its own can never also be curated
    /// as a sub-genre elsewhere — even when both facts are discovered in the
    /// very same reconcile pass, only the top-level card should result.
    #[test]
    fn test_reconcile_hierarchy_never_links_a_top_level_genre_as_a_subgenre() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Electronic; Pop");
        insert_song(&db, "/b.mp3", "Pop");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        assert!(
            hierarchy.iter().any(|g| g.name == "Pop"),
            "Pop keeps its own card"
        );
        let electronic = hierarchy.iter().find(|g| g.name == "Electronic").unwrap();
        assert!(
            !electronic.children.iter().any(|c| c.name == "Pop"),
            "Pop must never be linked as a sub-genre of Electronic since it's already a top-level genre"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Regression test for stale/legacy data (or any write path that isn't
    /// as careful as `reconcile_hierarchy` itself): a `tag_assignments` row
    /// that collides with a *different* top-level card's name — not just its
    /// own — must get purged too, and stay purged on the next reconcile.
    #[test]
    fn test_reconcile_hierarchy_purges_a_cross_group_conflicting_assignment() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal");
        insert_song(&db, "/b.mp3", "Rock");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();

        {
            let conn = db.pool.get().unwrap();
            let metal_id: i64 = conn
                .query_row("SELECT id FROM tag_groups WHERE name = 'Metal'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            conn.execute(
                "INSERT INTO tag_assignments (tag_name, group_id, sort_order) VALUES ('Rock', ?1, 0)",
                params![metal_id],
            )
            .unwrap();
        }

        manager.reconcile_hierarchy().unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        assert!(
            hierarchy.iter().any(|g| g.name == "Rock"),
            "Rock keeps its own card"
        );
        let metal = hierarchy.iter().find(|g| g.name == "Metal").unwrap();
        assert!(!metal.children.iter().any(|c| c.name == "Rock"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_reparent_and_promote_tag() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Progressive Metal");
        insert_song(&db, "/b.mp3", "Ambient");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();
        manager
            .reparent_tag("Progressive Metal", "Ambient")
            .unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        let metal = hierarchy.iter().find(|g| g.name == "Metal").unwrap();
        assert!(!metal.children.iter().any(|c| c.name == "Progressive Metal"));
        let ambient = hierarchy.iter().find(|g| g.name == "Ambient").unwrap();
        assert!(ambient
            .children
            .iter()
            .any(|c| c.name == "Progressive Metal"));

        manager.promote_tag("Progressive Metal").unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        assert!(hierarchy.iter().any(|g| g.name == "Progressive Metal"));
        let ambient = hierarchy.iter().find(|g| g.name == "Ambient").unwrap();
        assert!(!ambient
            .children
            .iter()
            .any(|c| c.name == "Progressive Metal"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_reparent_and_demote_refuse_a_tag_becoming_a_child_of_its_own_name() {
        // A tag can't meaningfully be a sub-genre of a card sharing its own
        // name — there's no separate "child instance" of the same literal
        // genre value, so the drill-down would show 0 songs despite the
        // chip's own displayed count. Both operations should be no-ops.
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Electronic");
        insert_song(&db, "/b.mp3", "Rock; Electronic");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();

        manager.reparent_tag("Electronic", "Electronic").unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        let electronic = hierarchy.iter().find(|g| g.name == "Electronic").unwrap();
        assert!(
            !electronic.children.iter().any(|c| c.name == "Electronic"),
            "reparent_tag onto a same-named group should be a no-op"
        );

        manager
            .demote_group_to_child("Electronic", "Electronic")
            .unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        assert!(
            hierarchy.iter().any(|g| g.name == "Electronic"),
            "demote_group_to_child onto a same-named group should be a no-op, not delete the card"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_reconcile_hierarchy_evicts_a_preexisting_self_referential_assignment() {
        // Simulates data from before the reparent/demote guards existed —
        // reconcile should clean this up rather than leave a chip that
        // always shows 0 songs despite its own displayed count.
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Electronic");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();

        {
            let conn = db.pool.get().unwrap();
            let group_id: i64 = conn
                .query_row(
                    "SELECT id FROM tag_groups WHERE name = 'Electronic'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            conn.execute(
                "INSERT INTO tag_assignments (tag_name, group_id, sort_order) VALUES ('Electronic', ?1, 0)",
                params![group_id],
            )
            .unwrap();
        }

        manager.reconcile_hierarchy().unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        let electronic = hierarchy.iter().find(|g| g.name == "Electronic").unwrap();
        assert!(!electronic.children.iter().any(|c| c.name == "Electronic"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_demote_group_to_child_removes_own_card_and_reassigns_its_children() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Electronic");
        insert_song(&db, "/b.mp3", "Synth-Pop; Electronic");
        insert_song(&db, "/c.mp3", "Synth-Pop; New Retro Wave");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();
        // Sanity: Synth-Pop starts out as its own card (it's position-0 on
        // /c.mp3) with "New Retro Wave" curated under it.
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        assert!(hierarchy.iter().any(|g| g.name == "Synth-Pop"));

        manager
            .demote_group_to_child("Synth-Pop", "Electronic")
            .unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        assert!(
            !hierarchy.iter().any(|g| g.name == "Synth-Pop"),
            "Synth-Pop should no longer have its own card"
        );
        let electronic = hierarchy.iter().find(|g| g.name == "Electronic").unwrap();
        assert!(electronic.children.iter().any(|c| c.name == "Synth-Pop"));
        // Its former child cascade-deleted rather than dangling on a
        // deleted group_id.
        assert!(!hierarchy
            .iter()
            .flat_map(|g| &g.children)
            .any(|c| c.name == "New Retro Wave"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_demoted_group_is_not_resurrected_by_a_later_reconcile() {
        // Regression test: get_tag_hierarchy's command handler reconciles
        // before every read (self-heal-on-read), so a demote that only
        // touches the curated tables — never the raw songs.genre text, which
        // still has this tag at position 0 — must survive that next
        // reconcile pass, not get its root silently recreated by it.
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Alternative");
        insert_song(&db, "/b.mp3", "Shoegaze");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();
        manager
            .demote_group_to_child("Shoegaze", "Alternative")
            .unwrap();

        // Simulates the reconcile-before-read that get_tag_hierarchy's
        // command handler now does on every call.
        manager.reconcile_hierarchy().unwrap();

        let hierarchy = manager.get_tag_hierarchy().unwrap();
        assert!(
            !hierarchy.iter().any(|g| g.name == "Shoegaze"),
            "a later reconcile should not resurrect a tag the user just demoted"
        );
        let alternative = hierarchy.iter().find(|g| g.name == "Alternative").unwrap();
        assert!(alternative.children.iter().any(|c| c.name == "Shoegaze"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_reorder_tag_in_group() {
        let (db, dir) = test_db();
        insert_song(
            &db,
            "/a.mp3",
            "Metal; Progressive Metal; Symphonic Metal; Doom Metal",
        );

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();
        manager.reorder_tag_in_group("Doom Metal", 0).unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        let metal = hierarchy.iter().find(|g| g.name == "Metal").unwrap();
        assert_eq!(metal.children[0].name, "Doom Metal");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_rewrite_genre_for_merge_preserves_position_and_dedupes() {
        assert_eq!(
            TagManager::rewrite_genre_for_merge(
                "Metal; Prog Metal",
                "Prog Metal",
                "Progressive Metal"
            ),
            "Metal; Progressive Metal"
        );
        // Merging into a name already present elsewhere in the list dedupes.
        assert_eq!(
            TagManager::rewrite_genre_for_merge(
                "Prog Metal; Progressive Metal",
                "Prog Metal",
                "Progressive Metal"
            ),
            "Progressive Metal"
        );
        // Unaffected songs pass through unchanged.
        assert_eq!(
            TagManager::rewrite_genre_for_merge("Ambient", "Prog Metal", "Progressive Metal"),
            "Ambient"
        );
    }

    #[test]
    fn test_rewrite_genre_for_delete_strips_names() {
        assert_eq!(
            TagManager::rewrite_genre_for_delete(
                "Metal; Progressive Metal; Symphonic Metal",
                &["Progressive Metal".to_string()]
            ),
            "Metal; Symphonic Metal"
        );
    }

    #[test]
    fn test_merge_tags_rewrites_genre_and_hierarchy() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Prog Metal");
        insert_song(&db, "/b.mp3", "Metal; Progressive Metal");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();
        let affected = manager
            .songs_containing_any(&["Prog Metal".to_string()])
            .unwrap();
        assert_eq!(affected.len(), 1);

        let conn = db.pool.get().unwrap();
        for (id, _path, genre) in &affected {
            let new_genre =
                TagManager::rewrite_genre_for_merge(genre, "Prog Metal", "Progressive Metal");
            conn.execute(
                "UPDATE songs SET genre = ?1 WHERE id = ?2",
                params![new_genre, id],
            )
            .unwrap();
        }
        manager
            .apply_merge_hierarchy("Prog Metal", "Progressive Metal")
            .unwrap();

        let songs = manager
            .get_songs_by_tag("Progressive Metal", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(songs.len(), 2);
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        let metal = hierarchy.iter().find(|g| g.name == "Metal").unwrap();
        assert!(!metal.children.iter().any(|c| c.name == "Prog Metal"));
        assert!(metal.children.iter().any(|c| c.name == "Progressive Metal"));

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Regression test for a genre card showing up as its own sub-genre
    /// chip: merging a group into another group whose name collides with
    /// one of the *merged-away* group's existing children must not leave
    /// that child assigned under a group sharing its own literal name.
    #[test]
    fn test_merge_hierarchy_purges_self_referential_child() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "IDM");
        insert_song(&db, "/b.mp3", "Electronic");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();

        // Simulates "Electronic" having been curated as a child of "IDM"
        // from before this invariant existed — reconcile_hierarchy itself
        // would never create this link (see
        // `test_reconcile_hierarchy_never_links_a_top_level_genre_as_a_subgenre`),
        // but a merge shouldn't have to special-case it either; it's left
        // for the next reconcile pass, same as any other write path.
        {
            let conn = db.pool.get().unwrap();
            let idm_id: i64 = conn
                .query_row("SELECT id FROM tag_groups WHERE name = 'IDM'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            conn.execute(
                "INSERT INTO tag_assignments (tag_name, group_id, sort_order) VALUES ('Electronic', ?1, 0)",
                params![idm_id],
            )
            .unwrap();
        }

        // Renaming/merging "IDM" into "Electronic" folds IDM's children
        // under the "Electronic" card — including the "Electronic" child it
        // already had, which would otherwise become a self-referential
        // chip. The next reconcile (as every `get_tag_hierarchy` command
        // read performs) cleans it up.
        manager.apply_merge_hierarchy("IDM", "Electronic").unwrap();
        manager.reconcile_hierarchy().unwrap();

        let hierarchy = manager.get_tag_hierarchy().unwrap();
        let electronic = hierarchy.iter().find(|g| g.name == "Electronic").unwrap();
        assert!(
            !electronic
                .children
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case("Electronic")),
            "Electronic must not be nested as its own sub-genre"
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
