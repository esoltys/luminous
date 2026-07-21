//! Play statistics & ratings — write paths for playcount, skipcount,
//! lastplayed, and the user rating stored on `songs`.

use crate::models::PlayContext;
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

/// Record what the user was inside (album/playlist/standalone song) when a
/// listen completed, so "Recently Played" can reflect that context instead
/// of a post-hoc heuristic.
pub fn record_play_context(conn: &Connection, context: &PlayContext, song_id: i64) -> Result<()> {
    let (context_type, playlist_id) = match context {
        PlayContext::Song => ("song", None),
        PlayContext::Album { .. } => ("album", None),
        PlayContext::Playlist { playlist_id } => ("playlist", Some(*playlist_id)),
    };
    conn.execute(
        "INSERT INTO play_history (context_type, song_id, playlist_id, played_at)
         VALUES (?1, ?2, ?3, strftime('%s','now'))",
        params![context_type, song_id, playlist_id],
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

/// Build the `song-stats-changed` event payload carrying the song's current
/// stats so every open view can sync without refetching.
pub fn stats_payload(conn: &Connection, song_id: i64) -> serde_json::Value {
    let row = conn.query_row(
        "SELECT playcount, skipcount, lastplayed, rating FROM songs WHERE id = ?1",
        params![song_id],
        |r| {
            Ok((
                r.get::<_, i32>(0)?,
                r.get::<_, i32>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, f32>(3)?,
            ))
        },
    );
    match row {
        Ok((playcount, skipcount, lastplayed, rating)) => serde_json::json!({
            "song_id": song_id,
            "playcount": playcount,
            "skipcount": skipcount,
            "lastplayed": lastplayed,
            "rating": rating,
        }),
        Err(_) => serde_json::json!({ "song_id": song_id }),
    }
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
    fn test_record_play_context_persists_by_type() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();
        let id = insert_song(&conn, "/tmp/context.flac");
        conn.execute(
            "INSERT INTO playlists (name) VALUES ('Test Playlist')",
            params![],
        )
        .unwrap();
        let playlist_id = conn.last_insert_rowid();

        record_play_context(&conn, &PlayContext::Song, id).unwrap();
        record_play_context(
            &conn,
            &PlayContext::Album {
                album: "Test Album".into(),
                album_artist: Some("Test Artist".into()),
            },
            id,
        )
        .unwrap();
        record_play_context(&conn, &PlayContext::Playlist { playlist_id }, id).unwrap();

        let rows: Vec<(String, Option<i64>)> = conn
            .prepare(
                "SELECT context_type, playlist_id FROM play_history WHERE song_id = ?1 ORDER BY id",
            )
            .unwrap()
            .query_map(params![id], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(
            rows,
            vec![
                ("song".to_string(), None),
                ("album".to_string(), None),
                ("playlist".to_string(), Some(playlist_id)),
            ]
        );

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
