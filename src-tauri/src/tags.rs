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
use std::{collections::HashMap, sync::Arc};

/// Number of hues in the Genres page's fixed curated palette — colors are
/// stored as an index into it (`tag_groups.color_index`), the actual hue
/// values only matter to the frontend's swatch rendering.
const PALETTE_SIZE: i32 = 10;

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
        tags.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        tags
    }

    fn compute_graph(lists: &[Vec<String>]) -> Vec<GenreGroup> {
        let mut root_counts: HashMap<String, (String, i64)> = HashMap::new();
        let mut child_counts: HashMap<String, HashMap<String, (String, i64)>> = HashMap::new();

        for values in lists {
            let Some(root) = values.first() else { continue };
            let root_key = root.to_lowercase();
            let root_entry = root_counts.entry(root_key.clone()).or_insert_with(|| (root.clone(), 0));
            root_entry.1 += 1;

            let children = child_counts.entry(root_key).or_default();
            for child in &values[1..] {
                let child_key = child.to_lowercase();
                let child_entry = children.entry(child_key).or_insert_with(|| (child.clone(), 0));
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
                children.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                GenreGroup {
                    main_tag,
                    song_count,
                    children,
                }
            })
            .collect();
        groups.sort_by(|a, b| a.main_tag.to_lowercase().cmp(&b.main_tag.to_lowercase()));
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
    pub fn get_songs_without_genre(&self, limit: i64, mode: QueuePopulationMode) -> Result<Vec<Song>> {
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
    /// Mirrors `CollectionScanner::get_songs_by_genre`'s signature/shape, but
    /// matches component-wise rather than requiring an exact full-column
    /// match.
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
            .take(limit.max(0) as usize)
            .collect();
        Ok(songs)
    }

    /// Songs whose *main* (position-0) genre value is exactly `tag_name`
    /// (case-insensitive). Used when drilling into a root of the Genre-view
    /// hierarchy — narrower than [`Self::get_songs_by_tag`], which also
    /// matches songs that merely carry the value as a subgenre.
    pub fn get_songs_by_main_tag(
        &self,
        tag_name: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
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
                    .and_then(|g| parse_multi_value(g).into_iter().next())
                    .map(|main| main.to_lowercase() == target)
                    .unwrap_or(false)
            })
            .take(limit.max(0) as usize)
            .collect();
        Ok(songs)
    }

    /// Songs matching the exact `get_genre_graph` edge: main (position-0)
    /// genre value is `root_tag`, and `child_tag` appears somewhere after
    /// it. Used when drilling into a child under a specific root in the
    /// Genre view — a tag appearing as a child under multiple roots (see
    /// `get_genre_graph`'s doc comment) needs this to show only the songs
    /// for *this* root/child relationship, not every song carrying
    /// `child_tag` anywhere regardless of its own main tag.
    pub fn get_songs_by_genre_edge(
        &self,
        root_tag: &str,
        child_tag: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
        let sql = format!(
            "SELECT {} FROM songs
             WHERE genre LIKE '%' || ?1 || '%'
               AND genre LIKE '%' || ?2 || '%'
               AND source IN (1, 2)
               AND unavailable = 0
               {extra_where}
             ORDER BY {order_by}",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let candidates: Vec<Song> = stmt
            .query_map(params![root_tag, child_tag], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();

        let root_target = root_tag.to_lowercase();
        let child_target = child_tag.to_lowercase();
        let songs = candidates
            .into_iter()
            .filter(|song| {
                let Some(values) = song.genre.as_deref().map(parse_multi_value) else {
                    return false;
                };
                let Some(main) = values.first() else {
                    return false;
                };
                main.to_lowercase() == root_target
                    && values[1..]
                        .iter()
                        .any(|v| v.to_lowercase() == child_target)
            })
            .take(limit.max(0) as usize)
            .collect();
        Ok(songs)
    }

    // -----------------------------------------------------------------
    // Persisted Genres curation hierarchy (#545): `tag_groups` (primary
    // genre cards) and `tag_assignments` (sub-genre chip -> card), layered
    // on top of the `songs.genre` values read above. See migration 18's
    // doc comment in db.rs for why this can't just reuse the emergent,
    // per-song-order graph `get_genre_graph` computes.
    // -----------------------------------------------------------------

    /// The persisted hierarchy: one [`TagGroup`] per `tag_groups` row, each
    /// with its assigned children, current song counts (any-position match,
    /// matching [`Self::list_all_tags`]'s semantics), and a per-child
    /// `is_conflict` flag when that child's name is also the name of some
    /// (other) top-level group.
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
        let group_names: std::collections::HashSet<String> = groups_raw
            .iter()
            .map(|(_, name, _)| name.to_lowercase())
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
                        is_conflict: group_names.contains(&child_name.to_lowercase()),
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
        let mut existing_assignments: std::collections::HashSet<String> = std::collections::HashSet::new();
        {
            let mut stmt = conn.prepare("SELECT tag_name FROM tag_assignments")?;
            for name in stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .filter_map(|r| r.ok())
            {
                existing_assignments.insert(name.to_lowercase());
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

        let mut next_group_sort: i32 =
            conn.query_row("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM tag_groups", [], |r| {
                r.get(0)
            })?;
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
        // value, that doesn't have one yet. Independent of whether the same
        // tag also gets an assignment below — a tag genuinely used as a main
        // category on some songs and a subgenre on others (the library
        // disagreeing with itself, same as `get_genre_graph` already has to
        // handle) legitimately ends up with both, which is exactly what the
        // conflict badge is for.
        for (key, name) in usage.clone() {
            if *is_root.get(&key).unwrap_or(&false) && !existing_groups.contains_key(&key) {
                let id = create_group(&conn, &name, &mut next_group_sort, &mut group_count)?;
                existing_groups.insert(key, id);
                changed = true;
            }
        }

        // Auto-assign every tag ever observed as a subgenre (appearing after
        // position 0 on some song) that doesn't have an assignment yet, to
        // whichever root it appeared under most often.
        let mut next_assignment_sort: i32 = conn.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM tag_assignments",
            [],
            |r| r.get(0),
        )?;
        for (key, name) in usage {
            if existing_assignments.contains(&key) {
                continue;
            }
            let Some(root_counts) = child_root_counts.get(&key) else {
                continue; // never observed as a subgenre — nothing to assign.
            };
            let best_root = root_counts.iter().max_by_key(|(_, c)| **c).map(|(k, _)| k.clone());
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
    /// be a `tag_groups` row (the card being dropped onto).
    pub fn reparent_tag(&self, tag_name: &str, new_group_name: &str) -> Result<()> {
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
        let group_count: i32 = conn.query_row("SELECT COUNT(*) FROM tag_groups", [], |r| r.get(0))?;
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
        let Some(pos) = siblings.iter().position(|n| n.eq_ignore_ascii_case(tag_name)) else {
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
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
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
    /// `from` (its group and/or its own assignment — a conflict tag can have
    /// both) is removed, since `from` no longer exists as a distinct name.
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
        // assignment row of its own at the same time (the conflict case).
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

    /// Normalizes a tag name for similarity comparison: lowercase, strip
    /// everything but alphanumerics.
    fn normalize_for_similarity(name: &str) -> String {
        name.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    }

    /// Levenshtein edit distance between two strings (byte-length bounded,
    /// tag names are short so this is never a performance concern).
    fn edit_distance(a: &str, b: &str) -> usize {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        let mut curr = vec![0; b.len() + 1];
        for i in 1..=a.len() {
            curr[0] = i;
            for j in 1..=b.len() {
                curr[j] = if a[i - 1] == b[j - 1] {
                    prev[j - 1]
                } else {
                    1 + prev[j - 1].min(prev[j]).min(curr[j - 1])
                };
            }
            std::mem::swap(&mut prev, &mut curr);
        }
        prev[b.len()]
    }

    /// True when `needle`'s characters all appear in `haystack` in order
    /// (not necessarily contiguously) — an abbreviation match, e.g.
    /// "progmetal" is a subsequence of "progressivemetal" even though it's
    /// not a contiguous substring.
    fn is_subsequence(needle: &str, haystack: &str) -> bool {
        let mut hay = haystack.chars();
        for nc in needle.chars() {
            if hay.find(|hc| *hc == nc).is_none() {
                return false;
            }
        }
        true
    }

    /// True when two normalized tag names are near-duplicates:
    /// - identical once normalized (e.g. "Synthpop" vs "Synth Pop" — same
    ///   letters, just different spacing), or
    /// - the shorter is at least half the longer's length and is a
    ///   subsequence of it (an abbreviation, e.g. "Prog Metal" vs
    ///   "Progressive Metal" — the length ratio guards against trivial
    ///   matches like "a" being "found" in almost anything), or
    /// - their normalized edit-distance similarity ratio is at least 80%
    ///   (`1 - distance / max_len`).
    fn is_similar(norm_a: &str, norm_b: &str) -> bool {
        if norm_a.is_empty() || norm_b.is_empty() {
            return false;
        }
        if norm_a == norm_b {
            return true;
        }
        let (shorter, longer) = if norm_a.chars().count() <= norm_b.chars().count() {
            (norm_a, norm_b)
        } else {
            (norm_b, norm_a)
        };
        let len_ratio = shorter.chars().count() as f64 / longer.chars().count() as f64;
        if len_ratio >= 0.5 && Self::is_subsequence(shorter, longer) {
            return true;
        }
        let max_len = norm_a.chars().count().max(norm_b.chars().count());
        let distance = Self::edit_distance(norm_a, norm_b);
        let similarity = 1.0 - (distance as f64 / max_len as f64);
        similarity >= 0.8
    }

    /// Near-duplicate tag-name pairs across the current tag list — see
    /// [`Self::is_similar`]. Excludes any pair that's already a genuine
    /// parent/subgenre relationship in the persisted hierarchy (e.g. "Metal"
    /// and "Black Metal" both contain "metal" and would otherwise look like
    /// a name variant, but one is deliberately curated as the other's
    /// subgenre, not a duplicate spelling of it). Ordered for determinism;
    /// the frontend owns per-session dismissal, not persisted here.
    pub fn get_merge_suggestions(&self) -> Result<Vec<(String, String)>> {
        let tags = self.list_all_tags()?;
        let related = self.parent_child_pairs()?;
        let mut pairs = Vec::new();
        for i in 0..tags.len() {
            for j in (i + 1)..tags.len() {
                let norm_a = Self::normalize_for_similarity(&tags[i].name);
                let norm_b = Self::normalize_for_similarity(&tags[j].name);
                if Self::is_similar(&norm_a, &norm_b) && !related.contains(&(norm_a, norm_b)) {
                    pairs.push((tags[i].name.clone(), tags[j].name.clone()));
                }
            }
        }
        Ok(pairs)
    }

    /// Every (normalized child, normalized parent) pair from the persisted
    /// hierarchy, in both orderings, so callers can cheaply check "are these
    /// two names already a curated parent/subgenre relationship?".
    fn parent_child_pairs(&self) -> Result<std::collections::HashSet<(String, String)>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT tag_assignments.tag_name, tag_groups.name
             FROM tag_assignments JOIN tag_groups ON tag_groups.id = tag_assignments.group_id",
        )?;
        let mut pairs = std::collections::HashSet::new();
        let rows = stmt.query_map([], |row| {
            let child: String = row.get(0)?;
            let parent: String = row.get(1)?;
            Ok((child, parent))
        })?;
        for row in rows {
            let (child, parent) = row?;
            let norm_child = Self::normalize_for_similarity(&child);
            let norm_parent = Self::normalize_for_similarity(&parent);
            pairs.insert((norm_child.clone(), norm_parent.clone()));
            pairs.insert((norm_parent, norm_child));
        }
        Ok(pairs)
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
        let metal = tags.iter().find(|t| t.name.eq_ignore_ascii_case("metal")).unwrap();
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
        assert!(classical.children.iter().any(|c| c.name == "Symphonic Metal"));

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
    fn test_get_songs_by_main_tag_excludes_subgenre_only_matches() {
        let (db, dir) = test_db();
        // Main tag is Ambient; Ambient Folk is only a subgenre here.
        insert_song(&db, "/a.mp3", "Ambient; Ambient Folk");
        // Main tag is genuinely Ambient Folk.
        insert_song(&db, "/b.mp3", "Ambient Folk");

        let manager = TagManager::new(db.clone());
        let by_tag = manager
            .get_songs_by_tag("Ambient Folk", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(by_tag.len(), 2, "any-position match includes both songs");

        let by_main = manager
            .get_songs_by_main_tag("Ambient Folk", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(by_main.len(), 1, "main-tag match excludes the subgenre-only song");
        assert_eq!(by_main[0].genre.as_deref(), Some("Ambient Folk"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_get_songs_by_genre_edge_disambiguates_shared_child_tag() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Symphonic Metal");
        insert_song(&db, "/b.mp3", "Classical; Symphonic Metal");
        // Reordered so Symphonic Metal is no longer a subgenre of Metal here.
        insert_song(&db, "/c.mp3", "Symphonic Metal; Metal");

        let manager = TagManager::new(db.clone());
        let under_metal = manager
            .get_songs_by_genre_edge("Metal", "Symphonic Metal", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(under_metal.len(), 1);
        assert_eq!(under_metal[0].genre.as_deref(), Some("Metal; Symphonic Metal"));

        let under_classical = manager
            .get_songs_by_genre_edge("Classical", "Symphonic Metal", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(under_classical.len(), 1);
        assert_eq!(under_classical[0].genre.as_deref(), Some("Classical; Symphonic Metal"));

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

    #[test]
    fn test_conflict_flag_when_child_name_is_also_a_top_level_group() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Electronic; Pop");
        insert_song(&db, "/b.mp3", "Pop");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        let electronic = hierarchy.iter().find(|g| g.name == "Electronic").unwrap();
        let pop_child = electronic.children.iter().find(|c| c.name == "Pop").unwrap();
        assert!(pop_child.is_conflict, "Pop is also its own top-level group");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_reparent_and_promote_tag() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Progressive Metal");
        insert_song(&db, "/b.mp3", "Ambient");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();
        manager.reparent_tag("Progressive Metal", "Ambient").unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        let metal = hierarchy.iter().find(|g| g.name == "Metal").unwrap();
        assert!(!metal.children.iter().any(|c| c.name == "Progressive Metal"));
        let ambient = hierarchy.iter().find(|g| g.name == "Ambient").unwrap();
        assert!(ambient.children.iter().any(|c| c.name == "Progressive Metal"));

        manager.promote_tag("Progressive Metal").unwrap();
        let hierarchy = manager.get_tag_hierarchy().unwrap();
        assert!(hierarchy.iter().any(|g| g.name == "Progressive Metal"));
        let ambient = hierarchy.iter().find(|g| g.name == "Ambient").unwrap();
        assert!(!ambient.children.iter().any(|c| c.name == "Progressive Metal"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_reorder_tag_in_group() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Progressive Metal; Symphonic Metal; Doom Metal");

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
            TagManager::rewrite_genre_for_merge("Metal; Prog Metal", "Prog Metal", "Progressive Metal"),
            "Metal; Progressive Metal"
        );
        // Merging into a name already present elsewhere in the list dedupes.
        assert_eq!(
            TagManager::rewrite_genre_for_merge("Prog Metal; Progressive Metal", "Prog Metal", "Progressive Metal"),
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
            let new_genre = TagManager::rewrite_genre_for_merge(genre, "Prog Metal", "Progressive Metal");
            conn.execute(
                "UPDATE songs SET genre = ?1 WHERE id = ?2",
                params![new_genre, id],
            )
            .unwrap();
        }
        manager.apply_merge_hierarchy("Prog Metal", "Progressive Metal").unwrap();

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

    #[test]
    fn test_get_merge_suggestions_matches_mock_examples() {
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal; Prog Metal");
        insert_song(&db, "/b.mp3", "Metal; Progressive Metal");
        insert_song(&db, "/c.mp3", "Synthpop");
        insert_song(&db, "/d.mp3", "Synth Pop");
        insert_song(&db, "/e.mp3", "Ambient");

        let manager = TagManager::new(db.clone());
        let suggestions = manager.get_merge_suggestions().unwrap();
        let has_pair = |a: &str, b: &str| {
            suggestions
                .iter()
                .any(|(x, y)| (x == a && y == b) || (x == b && y == a))
        };
        assert!(has_pair("Prog Metal", "Progressive Metal"));
        assert!(has_pair("Synthpop", "Synth Pop"));
        assert!(!has_pair("Ambient", "Metal"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_get_merge_suggestions_excludes_curated_subgenres() {
        // "Black Metal" contains "Metal" and would otherwise pass the
        // containment/similarity check, but it's a genuine curated subgenre
        // of "Metal" (not a duplicate spelling of it) — see #545 feedback.
        let (db, dir) = test_db();
        insert_song(&db, "/a.mp3", "Metal");
        insert_song(&db, "/b.mp3", "Metal; Black Metal");

        let manager = TagManager::new(db.clone());
        manager.reconcile_hierarchy().unwrap();

        let suggestions = manager.get_merge_suggestions().unwrap();
        assert!(
            !suggestions
                .iter()
                .any(|(x, y)| (x == "Metal" && y == "Black Metal")
                    || (x == "Black Metal" && y == "Metal")),
            "curated parent/subgenre pairs should not be suggested for merging"
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
