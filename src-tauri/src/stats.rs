//! Play statistics & ratings — write paths for playcount, skipcount,
//! lastplayed, and the user rating stored on `songs`.

use anyhow::Result;
use rusqlite::{params, Connection};

/// Sentinel stored in `songs.rating` for "no rating".
pub const RATING_UNRATED: f32 = -1.0;

/// Record a completed listen: bump playcount and stamp lastplayed.
pub fn record_play(conn: &Connection, song_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE songs
         SET playcount = playcount + 1,
             lastplayed = strftime('%s', 'now')
         WHERE id = ?1",
        params![song_id],
    )?;
    Ok(())
}

/// Record a manual skip that happened before the scrobble point.
pub fn record_skip(conn: &Connection, song_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE songs SET skipcount = skipcount + 1 WHERE id = ?1",
        params![song_id],
    )?;
    Ok(())
}

/// Persist a rating for a song, returning the normalized value actually stored.
pub fn set_rating(conn: &Connection, song_id: i64, rating: f32) -> Result<f32> {
    let normalized = normalize_rating(rating);
    conn.execute(
        "UPDATE songs SET rating = ?1 WHERE id = ?2",
        params![normalized, song_id],
    )?;
    Ok(normalized)
}

/// Negative values clear the rating; anything else snaps to half-star steps
/// within 0.5–5.0.
pub fn normalize_rating(rating: f32) -> f32 {
    if rating < 0.0 {
        return RATING_UNRATED;
    }
    let snapped = (rating * 2.0).round() / 2.0;
    snapped.clamp(0.5, 5.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn test_db() -> (Database, std::path::PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_stats_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (Database::new(temp_dir.clone()).unwrap(), temp_dir)
    }

    fn insert_song(conn: &Connection, path: &str) -> i64 {
        conn.execute(
            "INSERT INTO songs (path, title) VALUES (?1, ?2)",
            params![path, "Test Song"],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn stats_row(conn: &Connection, id: i64) -> (i32, i32, Option<i64>, f32) {
        conn.query_row(
            "SELECT playcount, skipcount, lastplayed, rating FROM songs WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap()
    }

    #[test]
    fn test_record_play_increments_and_stamps() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();
        let id = insert_song(&conn, "/tmp/a.flac");

        record_play(&conn, id).unwrap();
        record_play(&conn, id).unwrap();

        let (playcount, skipcount, lastplayed, _) = stats_row(&conn, id);
        assert_eq!(playcount, 2);
        assert_eq!(skipcount, 0);
        assert!(lastplayed.is_some());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_record_skip_increments_only_skipcount() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();
        let id = insert_song(&conn, "/tmp/b.flac");

        record_skip(&conn, id).unwrap();

        let (playcount, skipcount, lastplayed, _) = stats_row(&conn, id);
        assert_eq!(playcount, 0);
        assert_eq!(skipcount, 1);
        assert!(lastplayed.is_none());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_set_rating_persists_normalized_value() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();
        let id = insert_song(&conn, "/tmp/c.flac");

        let stored = set_rating(&conn, id, 3.3).unwrap();
        assert_eq!(stored, 3.5);
        let (_, _, _, rating) = stats_row(&conn, id);
        assert_eq!(rating, 3.5);

        let cleared = set_rating(&conn, id, -1.0).unwrap();
        assert_eq!(cleared, RATING_UNRATED);
        let (_, _, _, rating) = stats_row(&conn, id);
        assert_eq!(rating, RATING_UNRATED);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_normalize_rating_snaps_and_clamps() {
        assert_eq!(normalize_rating(-0.5), RATING_UNRATED);
        assert_eq!(normalize_rating(0.0), 0.5);
        assert_eq!(normalize_rating(0.2), 0.5);
        assert_eq!(normalize_rating(2.75), 3.0);
        assert_eq!(normalize_rating(4.4), 4.5);
        assert_eq!(normalize_rating(5.0), 5.0);
        assert_eq!(normalize_rating(9.9), 5.0);
    }
}
