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
    models::{parse_multi_value, GenreGroup, QueuePopulationMode, Song, Tag, TagCount},
};
use anyhow::Result;
use rusqlite::params;
use std::{collections::HashMap, sync::Arc};

#[derive(Debug)]
pub struct TagManager {
    db: Arc<Database>,
}

impl TagManager {
    pub fn new(db: Arc<Database>) -> Self {
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
                    && values[1..].iter().any(|v| v.to_lowercase() == child_target)
            })
            .take(limit.max(0) as usize)
            .collect();
        Ok(songs)
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
        assert_eq!(
            by_main.len(),
            1,
            "main-tag match excludes the subgenre-only song"
        );
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
        assert_eq!(
            under_metal[0].genre.as_deref(),
            Some("Metal; Symphonic Metal")
        );

        let under_classical = manager
            .get_songs_by_genre_edge("Classical", "Symphonic Metal", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(under_classical.len(), 1);
        assert_eq!(
            under_classical[0].genre.as_deref(),
            Some("Classical; Symphonic Metal")
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
