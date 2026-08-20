//! Luminous-native song tags (#224) — a many-to-many tag system independent
//! of the embedded `songs.genre` column (see `tageditor.rs`/`collection.rs`
//! for that). No bundled genre taxonomy: the "Genre" browse view's hierarchy
//! is derived purely from how the user orders each song's own tags — the
//! first tag entered on a song is treated as its main category, every other
//! tag on that song as a subgenre of it (see `get_genre_graph`).

use crate::{
    collection::{mode_query_fragments, row_to_song, SONG_SELECT_COLS},
    db::Database,
    models::{GenreGroup, QueuePopulationMode, Song, Tag, TagCount},
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

    /// Every tag currently in use, with how many songs carry it (at any
    /// position), ordered by name.
    pub fn list_all_tags(&self) -> Result<Vec<Tag>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT t.name, COUNT(st.song_id)
             FROM tags t
             JOIN song_tags st ON st.tag_id = t.id
             GROUP BY t.id
             ORDER BY t.name COLLATE NOCASE",
        )?;
        let tags = stmt
            .query_map([], |row| {
                Ok(Tag {
                    name: row.get(0)?,
                    song_count: row.get(1)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }

    /// A song's tags in stored order — position 0 is its main-category tag.
    pub fn get_song_tags(&self, song_id: i64) -> Result<Vec<String>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT t.name
             FROM song_tags st
             JOIN tags t ON t.id = st.tag_id
             WHERE st.song_id = ?1
             ORDER BY st.position",
        )?;
        let tags = stmt
            .query_map(params![song_id], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }

    /// Replaces a song's whole tag list with `tag_names`, in the given order
    /// (position 0 = main category). Blank/whitespace-only names are
    /// dropped; a name repeated within the list keeps only its first
    /// position. Reuses an existing tag case-insensitively rather than
    /// creating a duplicate (`tags.name` is `COLLATE NOCASE UNIQUE`).
    pub fn set_song_tags(&self, song_id: i64, tag_names: &[String]) -> Result<()> {
        let mut ordered: Vec<String> = Vec::with_capacity(tag_names.len());
        for name in tag_names {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !ordered
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(trimmed))
            {
                ordered.push(trimmed.to_string());
            }
        }

        let mut conn = self.db.pool.get()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM song_tags WHERE song_id = ?1",
            params![song_id],
        )?;
        for (position, name) in ordered.iter().enumerate() {
            tx.execute(
                "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
                params![name],
            )?;
            let tag_id: i64 = tx.query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |row| row.get(0),
            )?;
            tx.execute(
                "INSERT INTO song_tags (song_id, tag_id, position) VALUES (?1, ?2, ?3)",
                params![song_id, tag_id, position as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Songs carrying `tag_name` at any position. Mirrors
    /// `CollectionScanner::get_songs_by_genre`'s signature/shape.
    pub fn get_songs_by_tag(
        &self,
        tag_name: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
        let sql = format!(
            "SELECT {} FROM songs
             WHERE id IN (
                 SELECT st.song_id FROM song_tags st
                 JOIN tags t ON t.id = st.tag_id
                 WHERE t.name = ?1 COLLATE NOCASE
             )
               AND source IN (1, 2)
               AND unavailable = 0
               {extra_where}
             ORDER BY {order_by}
             LIMIT ?2",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![tag_name, limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// The emergent genre hierarchy: one [`GenreGroup`] per tag that has ever
    /// appeared at position 0 (a song's main category), each carrying every
    /// tag seen as a subgenre of it (position > 0 on the same song) across
    /// the whole library, aggregated with counts. A tag can appear as a
    /// child under more than one group if different songs disagree about its
    /// main category — both relationships are real and both are returned.
    pub fn get_genre_graph(&self) -> Result<Vec<GenreGroup>> {
        let conn = self.db.pool.get()?;

        let mut roots_stmt = conn.prepare(
            "SELECT t.name, COUNT(DISTINCT st.song_id)
             FROM song_tags st
             JOIN tags t ON t.id = st.tag_id
             WHERE st.position = 0
             GROUP BY t.id
             ORDER BY t.name COLLATE NOCASE",
        )?;
        let mut groups: Vec<GenreGroup> = roots_stmt
            .query_map([], |row| {
                Ok(GenreGroup {
                    main_tag: row.get(0)?,
                    song_count: row.get(1)?,
                    children: Vec::new(),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut children_by_root: HashMap<String, Vec<TagCount>> = HashMap::new();
        let mut edges_stmt = conn.prepare(
            "SELECT root_tag.name, child_tag.name, COUNT(DISTINCT root.song_id)
             FROM song_tags root
             JOIN song_tags child ON child.song_id = root.song_id AND child.position > 0
             JOIN tags root_tag ON root_tag.id = root.tag_id
             JOIN tags child_tag ON child_tag.id = child.tag_id
             WHERE root.position = 0
             GROUP BY root_tag.id, child_tag.id
             ORDER BY child_tag.name COLLATE NOCASE",
        )?;
        let edges = edges_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for edge in edges.filter_map(|r| r.ok()) {
            let (root, child, count) = edge;
            children_by_root.entry(root).or_default().push(TagCount {
                name: child,
                song_count: count,
            });
        }

        for group in &mut groups {
            if let Some(children) = children_by_root.remove(&group.main_tag) {
                group.children = children;
            }
        }

        Ok(groups)
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

    fn insert_song(db: &Database, path: &str) -> i64 {
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO songs (source, filetype, path) VALUES (1, 0, ?1)",
            params![path],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn test_set_song_tags_creates_and_reuses_case_insensitively() {
        let (db, dir) = test_db();
        let manager = TagManager::new(db.clone());
        let song_id = insert_song(&db, "/a.mp3");

        manager
            .set_song_tags(song_id, &["Metal".into(), "Symphonic Metal".into()])
            .unwrap();
        assert_eq!(
            manager.get_song_tags(song_id).unwrap(),
            vec!["Metal", "Symphonic Metal"]
        );

        let song_id_2 = insert_song(&db, "/b.mp3");
        manager
            .set_song_tags(song_id_2, &["metal".into()])
            .unwrap();
        // Reused the existing "Metal" tag rather than creating a duplicate.
        assert_eq!(manager.list_all_tags().unwrap().len(), 2);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_set_song_tags_replaces_order_and_drops_blanks() {
        let (db, dir) = test_db();
        let manager = TagManager::new(db.clone());
        let song_id = insert_song(&db, "/a.mp3");

        manager
            .set_song_tags(song_id, &["Metal".into(), "Symphonic Metal".into()])
            .unwrap();
        manager
            .set_song_tags(song_id, &["  ".into(), "Classical".into()])
            .unwrap();
        assert_eq!(manager.get_song_tags(song_id).unwrap(), vec!["Classical"]);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_genre_graph_star_relationship_and_lone_root() {
        let (db, dir) = test_db();
        let manager = TagManager::new(db.clone());

        let s1 = insert_song(&db, "/a.mp3");
        manager
            .set_song_tags(
                s1,
                &[
                    "Metal".into(),
                    "Progressive Metal".into(),
                    "Symphonic Metal".into(),
                ],
            )
            .unwrap();

        let s2 = insert_song(&db, "/b.mp3");
        manager.set_song_tags(s2, &["Ambient".into()]).unwrap();

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
        let manager = TagManager::new(db.clone());

        let s1 = insert_song(&db, "/a.mp3");
        manager
            .set_song_tags(s1, &["Metal".into(), "Symphonic Metal".into()])
            .unwrap();
        let s2 = insert_song(&db, "/b.mp3");
        manager
            .set_song_tags(s2, &["Classical".into(), "Symphonic Metal".into()])
            .unwrap();

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
    fn test_get_songs_by_tag_matches_any_position() {
        let (db, dir) = test_db();
        let manager = TagManager::new(db.clone());
        let s1 = insert_song(&db, "/a.mp3");
        manager
            .set_song_tags(s1, &["Metal".into(), "Symphonic Metal".into()])
            .unwrap();

        let songs = manager
            .get_songs_by_tag("symphonic metal", 50, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(songs.len(), 1);

        let _ = std::fs::remove_dir_all(dir);
    }
}
