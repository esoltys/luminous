//! Pure DB/query logic backing the user-curated "Pinned" Home shelf (#222).
//! Kept separate from the `#[tauri::command]` wrappers in `commands::pins`
//! so it's unit-testable against a plain `Connection`/`CollectionScanner`
//! rather than a full Tauri `AppState` — mirrors the `stats.rs` /
//! `commands::stats` split.

use crate::collection::{row_to_song, CollectionScanner, SONG_SELECT_COLS};
use crate::models::{AlbumItem, ArtistItem, Song};
use anyhow::Result;
use rusqlite::{params, Connection};

/// Pins `(item_type, ref_key)` at the front of the list (position 0),
/// shifting every existing pin back by one — a newly pinned item is the one
/// the user just acted on, so it should be the most reachable, not buried at
/// the end of a long row. Re-pinning an already-pinned item is a no-op
/// (its existing position is left alone).
pub fn pin(conn: &Connection, item_type: &str, ref_key: &str) -> Result<()> {
    let already_pinned: bool = conn
        .query_row(
            "SELECT 1 FROM pinned_items WHERE item_type = ?1 AND ref_key = ?2",
            params![item_type, ref_key],
            |_| Ok(()),
        )
        .is_ok();
    if already_pinned {
        return Ok(());
    }

    let pinned_at = chrono::Utc::now().timestamp();
    conn.execute("UPDATE pinned_items SET position = position + 1", [])?;
    conn.execute(
        "INSERT INTO pinned_items (item_type, ref_key, position, pinned_at) VALUES (?1, ?2, 0, ?3)",
        params![item_type, ref_key, pinned_at],
    )?;
    Ok(())
}

pub fn unpin(conn: &Connection, item_type: &str, ref_key: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM pinned_items WHERE item_type = ?1 AND ref_key = ?2",
        params![item_type, ref_key],
    )?;
    Ok(())
}

/// Every pinned `(item_type, ref_key)`, in display order.
pub fn pinned_refs(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt =
        conn.prepare("SELECT item_type, ref_key FROM pinned_items ORDER BY position ASC")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

/// Persists `order` (the full new pin sequence) by diffing it against the
/// current `position` values and updating only the rows that moved —
/// mirrors `reorder_playlist_items_batch`'s load/diff/update style.
pub fn reorder(conn: &Connection, order: &[(String, String)]) -> Result<()> {
    let current: Vec<(String, String, i64)> = {
        let mut stmt = conn.prepare("SELECT item_type, ref_key, position FROM pinned_items")?;
        let rows: Vec<(String, String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };

    for (new_pos, (item_type, ref_key)) in order.iter().enumerate() {
        let new_pos = new_pos as i64;
        let changed = current
            .iter()
            .find(|(t, k, _)| t == item_type && k == ref_key)
            .map(|(_, _, old_pos)| *old_pos != new_pos)
            .unwrap_or(false);
        if changed {
            conn.execute(
                "UPDATE pinned_items SET position = ?1 WHERE item_type = ?2 AND ref_key = ?3",
                params![new_pos, item_type, ref_key],
            )?;
        }
    }
    Ok(())
}

/// Resolves a pinned song reference against live data — `None` (not an
/// error) when the id doesn't parse or the song no longer exists/is
/// unavailable, so a stale pin is silently dropped by the caller.
pub fn resolve_song(conn: &Connection, ref_key: &str) -> Result<Option<Song>> {
    let Ok(song_id) = ref_key.parse::<i64>() else {
        return Ok(None);
    };
    let sql = format!("SELECT {SONG_SELECT_COLS} FROM songs WHERE id = ?1 AND unavailable = 0");
    Ok(conn.query_row(&sql, params![song_id], row_to_song).ok())
}

/// Builds an `AlbumItem` from one entry of `CollectionScanner::get_albums()`'s
/// JSON output. Built field-by-field rather than via `serde_json::from_value`:
/// that JSON shape carries extra fields (added, artist_sort, albumsort) and
/// omits `sample_song_id` (which `AlbumItem` declares as a plain,
/// non-defaulted `Option`), so a blanket deserialize would fail on the
/// missing key.
pub fn album_item_from_json(value: &serde_json::Value) -> AlbumItem {
    AlbumItem {
        artist: value
            .get("artist")
            .and_then(|v| v.as_str())
            .map(String::from),
        album: value
            .get("album")
            .and_then(|v| v.as_str())
            .map(String::from),
        year: value.get("year").and_then(|v| v.as_i64()).map(|n| n as i32),
        track_count: value
            .get("track_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32,
        disc_count: value
            .get("disc_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(1) as i32,
        art_embedded: value
            .get("art_embedded")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        art_automatic: value
            .get("art_automatic")
            .and_then(|v| v.as_str())
            .map(String::from),
        art_manual: value
            .get("art_manual")
            .and_then(|v| v.as_str())
            .map(String::from),
        genre: value
            .get("genre")
            .and_then(|v| v.as_str())
            .map(String::from),
        sample_song_id: None,
        rating: value.get("rating").and_then(|v| v.as_f64()).unwrap_or(-1.0) as f32,
        total_duration_nanosec: value
            .get("total_duration_nanosec")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    }
}

/// Finds the album entry (from `get_albums()`'s output) matching `ref_key`
/// (the bare album title, same key convention as `album_ratings`).
pub fn find_album<'a>(
    albums: &'a [serde_json::Value],
    ref_key: &str,
) -> Option<&'a serde_json::Value> {
    albums
        .iter()
        .find(|a| a.get("album").and_then(|v| v.as_str()) == Some(ref_key))
}

/// Finds the artist entry (from `get_artists()`'s output) matching `ref_key`,
/// case-insensitively — `get_artists` groups artists `COLLATE NOCASE` (#295),
/// so an exact-case match could miss a pin whose stored casing drifted from
/// the group's chosen display casing.
pub fn find_artist<'a>(
    artists: &'a [serde_json::Value],
    ref_key: &str,
) -> Option<&'a serde_json::Value> {
    artists.iter().find(|a| {
        a.get("name")
            .and_then(|v| v.as_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(ref_key))
    })
}

pub fn artist_item_from_json(value: &serde_json::Value) -> Result<ArtistItem> {
    Ok(serde_json::from_value(value.clone())?)
}

/// Fetches every current album summary — a thin pass-through kept here so
/// `commands::pins::get_pinned_items` doesn't need to import
/// `CollectionScanner` directly.
pub fn all_albums(scanner: &CollectionScanner) -> Result<Vec<serde_json::Value>> {
    scanner.get_albums()
}

pub fn all_artists(scanner: &CollectionScanner) -> Result<Vec<serde_json::Value>> {
    scanner.get_artists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn test_db() -> (Database, std::path::PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_pins_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (Database::new(temp_dir.clone()).unwrap(), temp_dir)
    }

    fn insert_song(conn: &Connection, path: &str) -> i64 {
        conn.execute(
            "INSERT INTO songs (path, title, unavailable) VALUES (?1, ?2, 0)",
            params![path, "Test Song"],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn pinned_items_table_has_expected_columns() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();
        let mut stmt = conn.prepare("PRAGMA table_info('pinned_items')").unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        for expected in ["item_type", "ref_key", "position", "pinned_at"] {
            assert!(
                cols.contains(&expected.to_string()),
                "missing column {expected}"
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pin_unpin_round_trip_for_each_item_type() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();

        for item_type in ["song", "album", "artist", "playlist"] {
            pin(&conn, item_type, "ref-1").unwrap();
            assert!(pinned_refs(&conn)
                .unwrap()
                .contains(&(item_type.to_string(), "ref-1".to_string())));

            unpin(&conn, item_type, "ref-1").unwrap();
            assert!(!pinned_refs(&conn)
                .unwrap()
                .contains(&(item_type.to_string(), "ref-1".to_string())));
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pinning_inserts_at_the_front_and_shifts_existing_pins_back() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();

        pin(&conn, "song", "1").unwrap();
        pin(&conn, "song", "2").unwrap();
        pin(&conn, "song", "3").unwrap();

        // Most recently pinned first.
        assert_eq!(
            pinned_refs(&conn).unwrap(),
            vec![
                ("song".into(), "3".into()),
                ("song".into(), "2".into()),
                ("song".into(), "1".into()),
            ]
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn re_pinning_is_idempotent_and_keeps_original_position() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();

        pin(&conn, "song", "1").unwrap();
        pin(&conn, "song", "2").unwrap();
        pin(&conn, "song", "1").unwrap(); // re-pin, should be a no-op

        let refs = pinned_refs(&conn).unwrap();
        assert_eq!(refs.len(), 2, "re-pinning must not create a duplicate row");
        assert_eq!(refs[0], ("song".to_string(), "2".to_string()));
        assert_eq!(refs[1], ("song".to_string(), "1".to_string()));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reorder_updates_positions_for_a_partial_reorder() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();

        pin(&conn, "song", "1").unwrap();
        pin(&conn, "song", "2").unwrap();
        pin(&conn, "song", "3").unwrap();
        assert_eq!(
            pinned_refs(&conn).unwrap(),
            vec![
                ("song".into(), "3".into()),
                ("song".into(), "2".into()),
                ("song".into(), "1".into()),
            ]
        );

        // Explicitly reorder to "3", "1", "2" regardless of the pin order above.
        reorder(
            &conn,
            &[
                ("song".to_string(), "3".to_string()),
                ("song".to_string(), "1".to_string()),
                ("song".to_string(), "2".to_string()),
            ],
        )
        .unwrap();

        assert_eq!(
            pinned_refs(&conn).unwrap(),
            vec![
                ("song".into(), "3".into()),
                ("song".into(), "1".into()),
                ("song".into(), "2".into()),
            ]
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn resolve_song_returns_live_data_and_none_for_missing_or_unavailable() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();
        let id = insert_song(&conn, "/tmp/pinned_song.flac");

        let resolved = resolve_song(&conn, &id.to_string()).unwrap();
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().id, id);

        // A ref that no longer parses or doesn't exist resolves to None, not an error.
        assert!(resolve_song(&conn, "not-a-number").unwrap().is_none());
        assert!(resolve_song(&conn, "999999").unwrap().is_none());

        // Marking the song unavailable (soft-deleted) also drops it from resolution.
        conn.execute(
            "UPDATE songs SET unavailable = 1 WHERE id = ?1",
            params![id],
        )
        .unwrap();
        assert!(resolve_song(&conn, &id.to_string()).unwrap().is_none());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn album_item_from_json_maps_fields_and_defaults_missing_sample_song_id() {
        let value = serde_json::json!({
            "artist": "Test Artist",
            "album": "Test Album",
            "year": 2020,
            "track_count": 10,
            "disc_count": 1,
            "art_embedded": true,
            "art_automatic": null,
            "art_manual": null,
            "genre": "Rock",
            "rating": 4.5,
            "added": 1234,
            "total_duration_nanosec": 999,
            "artist_sort": "Artist, Test",
            "albumsort": null,
        });
        let album = album_item_from_json(&value);
        assert_eq!(album.album.as_deref(), Some("Test Album"));
        assert_eq!(album.artist.as_deref(), Some("Test Artist"));
        assert_eq!(album.track_count, 10);
        assert_eq!(album.rating, 4.5);
        assert_eq!(album.sample_song_id, None);
    }

    #[test]
    fn find_artist_matches_case_insensitively() {
        let artists = vec![serde_json::json!({
            "name": "The War On Drugs",
            "album_count": 3,
            "song_count": 20,
            "genre": null,
            "sort_artist": "War On Drugs, The",
            "total_playcount": 5,
        })];
        assert!(find_artist(&artists, "the war on drugs").is_some());
        assert!(find_artist(&artists, "THE WAR ON DRUGS").is_some());
        assert!(find_artist(&artists, "Someone Else").is_none());
    }
}
