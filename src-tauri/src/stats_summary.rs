//! Personal Stats aggregation (#130) — range-filtered Top 10 artists/albums/
//! songs/genres and listening-clock timestamps, computed live from
//! `play_history` + `songs`. Deliberately does **not** touch
//! `album_chart_history` (the Home "Top Albums" weekly chart's snapshot
//! table, #662) — that table only tracks the top 10 albums per week, so
//! summing across weeks would silently undercount anything that never
//! cracked a weekly top 10. See issue #130's comment thread.

use crate::models::{parse_multi_value, StatsSummary, StatsTopItem};
use anyhow::Result;
use rusqlite::{params, Connection};

/// A Personal Stats time window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsRange {
    SevenDays,
    TwentyEightDays,
    OneYear,
}

impl StatsRange {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "7d" => Some(Self::SevenDays),
            "28d" => Some(Self::TwentyEightDays),
            "1y" => Some(Self::OneYear),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SevenDays => "7d",
            Self::TwentyEightDays => "28d",
            Self::OneYear => "1y",
        }
    }

    fn days(&self) -> i64 {
        match self {
            Self::SevenDays => 7,
            Self::TwentyEightDays => 28,
            Self::OneYear => 365,
        }
    }
}

const SECONDS_PER_DAY: i64 = 86_400;

/// Start of a rolling range ending `now` — plain `now - N*86400` arithmetic,
/// no calendar alignment needed (unlike the Monday-aligned weekly chart in
/// `collection::query::week_start_utc`). Split out with an explicit `now`
/// so tests can drive it deterministically.
pub fn range_start_unix(range: StatsRange, now: i64) -> i64 {
    now - range.days() * SECONDS_PER_DAY
}

const TOP_N: i64 = 10;

/// Build a full Personal Stats summary for `range`, excluding any song,
/// album, artist, or genre flagged in `stats_exclusions`.
pub fn get_summary(conn: &Connection, range: StatsRange) -> Result<StatsSummary> {
    get_summary_at(conn, range, chrono::Utc::now().timestamp())
}

fn get_summary_at(conn: &Connection, range: StatsRange, now: i64) -> Result<StatsSummary> {
    let range_start = range_start_unix(range, now);
    Ok(StatsSummary {
        range: range.as_str().to_string(),
        top_songs: top_songs(conn, range_start)?,
        top_albums: top_albums(conn, range_start)?,
        top_artists: top_artists(conn, range_start)?,
        top_genres: top_genres(conn, range_start)?,
        play_timestamps: play_timestamps(conn, range_start)?,
    })
}

fn top_songs(conn: &Connection, range_start: i64) -> Result<Vec<StatsTopItem>> {
    let mut stmt = conn.prepare(
        "SELECT CAST(s.id AS TEXT), s.title, s.artist, COUNT(*) AS play_count, s.album
         FROM play_history ph
         JOIN songs s ON s.id = ph.song_id
         WHERE ph.played_at >= ?1
           AND s.source IN (1, 2, 11) AND s.unavailable = 0
           AND NOT EXISTS (
               SELECT 1 FROM stats_exclusions se
               WHERE se.entity_type = 'song' AND se.entity_key = CAST(s.id AS TEXT)
           )
         GROUP BY s.id
         ORDER BY play_count DESC, s.title COLLATE NOCASE ASC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![range_start, TOP_N], |row| {
            Ok(StatsTopItem {
                key: row.get(0)?,
                label: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                secondary: row.get(2)?,
                play_count: row.get(3)?,
                excluded: false,
                album: row.get(4)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

/// Album play count is `MIN` of per-track play counts within range (a
/// completionist metric — full front-to-back listens), deliberately
/// different from the Home "Top Albums" chart's `SUM`-based engagement
/// metric (`CollectionScanner::get_top_albums`). The two numbers will
/// legitimately disagree for the same album/range — see #130's discussion.
fn top_albums(conn: &Connection, range_start: i64) -> Result<Vec<StatsTopItem>> {
    let mut stmt = conn.prepare(
        "SELECT s.album, MIN(COALESCE(s.album_artist, s.artist)), MIN(track_plays.plays) AS min_plays
         FROM songs s
         JOIN (
             SELECT s2.id AS song_id, COUNT(*) AS plays
             FROM play_history ph
             JOIN songs s2 ON s2.id = ph.song_id
             WHERE ph.played_at >= ?1
             GROUP BY s2.id
         ) track_plays ON track_plays.song_id = s.id
         WHERE s.source IN (1, 2, 11) AND s.unavailable = 0
           AND s.album IS NOT NULL AND s.album != ''
           AND NOT EXISTS (
               SELECT 1 FROM stats_exclusions se
               WHERE se.entity_type = 'album' AND se.entity_key = s.album COLLATE NOCASE
           )
         GROUP BY s.album
         ORDER BY min_plays DESC, s.album COLLATE NOCASE ASC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![range_start, TOP_N], |row| {
            Ok(StatsTopItem {
                key: row.get(0)?,
                label: row.get(0)?,
                secondary: row.get(1)?,
                play_count: row.get(2)?,
                excluded: false,
                album: None,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

fn top_artists(conn: &Connection, range_start: i64) -> Result<Vec<StatsTopItem>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(NULLIF(s.album_artist, ''), s.artist, '') AS effective_artist,
                COUNT(*) AS play_count
         FROM play_history ph
         JOIN songs s ON s.id = ph.song_id
         WHERE ph.played_at >= ?1
           AND s.source IN (1, 2, 11) AND s.unavailable = 0
           AND COALESCE(NULLIF(s.album_artist, ''), s.artist, '') != ''
           AND NOT EXISTS (
               SELECT 1 FROM stats_exclusions se
               WHERE se.entity_type = 'artist'
                 AND se.entity_key = COALESCE(NULLIF(s.album_artist, ''), s.artist, '') COLLATE NOCASE
           )
         GROUP BY effective_artist COLLATE NOCASE
         ORDER BY play_count DESC, effective_artist COLLATE NOCASE ASC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![range_start, TOP_N], |row| {
            Ok(StatsTopItem {
                key: row.get(0)?,
                label: row.get(0)?,
                secondary: None,
                play_count: row.get(1)?,
                excluded: false,
                album: None,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

/// `songs.genre` is a `; `-delimited multi-value string (see
/// `crate::models::parse_multi_value`/`crate::tags::TagManager`), so genre
/// counting can't be a plain SQL `GROUP BY` — each play is attributed to
/// every genre tag on its song, then tallied and ranked in Rust.
fn top_genres(conn: &Connection, range_start: i64) -> Result<Vec<StatsTopItem>> {
    let mut stmt = conn.prepare(
        "SELECT s.genre
         FROM play_history ph
         JOIN songs s ON s.id = ph.song_id
         WHERE ph.played_at >= ?1
           AND s.source IN (1, 2, 11) AND s.unavailable = 0
           AND s.genre IS NOT NULL AND s.genre != ''",
    )?;
    let genre_lists: Vec<String> = stmt
        .query_map(params![range_start], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let excluded_genres = excluded_keys(conn, "genre")?;
    let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for raw in genre_lists {
        for genre in parse_multi_value(&raw) {
            if excluded_genres.contains(&genre.to_lowercase()) {
                continue;
            }
            *counts.entry(genre).or_insert(0) += 1;
        }
    }

    let mut ranked: Vec<(String, i64)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(TOP_N as usize);

    Ok(ranked
        .into_iter()
        .map(|(genre, play_count)| StatsTopItem {
            key: genre.clone(),
            label: genre,
            secondary: None,
            play_count,
            excluded: false,
            album: None,
        })
        .collect())
}

fn excluded_keys(
    conn: &Connection,
    entity_type: &str,
) -> Result<std::collections::HashSet<String>> {
    let mut stmt =
        conn.prepare("SELECT entity_key FROM stats_exclusions WHERE entity_type = ?1")?;
    let keys = stmt
        .query_map(params![entity_type], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .map(|k| k.to_lowercase())
        .collect();
    Ok(keys)
}

/// Raw `played_at` timestamps for every non-excluded, in-range play, for the
/// frontend to bucket into a local-time listening clock. Bucketing happens
/// client-side (`new Date(ts * 1000).getHours()`) rather than via a
/// server-side UTC-offset param — simpler, no DST logic, and immune to
/// stale-offset bugs if the user's timezone changes between listen-time and
/// view-time.
fn play_timestamps(conn: &Connection, range_start: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT ph.played_at
         FROM play_history ph
         JOIN songs s ON s.id = ph.song_id
         WHERE ph.played_at >= ?1
           AND s.source IN (1, 2, 11) AND s.unavailable = 0
           AND NOT EXISTS (
               SELECT 1 FROM stats_exclusions se
               WHERE se.entity_type = 'song' AND se.entity_key = CAST(s.id AS TEXT)
           )",
    )?;
    let rows = stmt
        .query_map(params![range_start], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn test_db() -> (Database, std::path::PathBuf) {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_stats_summary_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (Database::new(temp_dir.clone()).unwrap(), temp_dir)
    }

    fn insert_song(
        conn: &Connection,
        path: &str,
        title: &str,
        artist: &str,
        album: &str,
        genre: &str,
    ) -> i64 {
        conn.execute(
            "INSERT INTO songs (path, title, artist, album, genre, source, unavailable)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, 0)",
            params![path, title, artist, album, genre],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_play(conn: &Connection, song_id: i64, played_at: i64) {
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, played_at) VALUES ('song', ?1, ?2)",
            params![song_id, played_at],
        )
        .unwrap();
    }

    #[test]
    fn test_range_start_unix() {
        let now = 1_700_000_000;
        assert_eq!(
            range_start_unix(StatsRange::SevenDays, now),
            now - 7 * SECONDS_PER_DAY
        );
        assert_eq!(
            range_start_unix(StatsRange::TwentyEightDays, now),
            now - 28 * SECONDS_PER_DAY
        );
        assert_eq!(
            range_start_unix(StatsRange::OneYear, now),
            now - 365 * SECONDS_PER_DAY
        );
    }

    #[test]
    fn test_stats_range_parse() {
        assert_eq!(StatsRange::parse("7d"), Some(StatsRange::SevenDays));
        assert_eq!(StatsRange::parse("28d"), Some(StatsRange::TwentyEightDays));
        assert_eq!(StatsRange::parse("1y"), Some(StatsRange::OneYear));
        assert_eq!(StatsRange::parse("bogus"), None);
    }

    #[test]
    fn test_top_songs_excludes_out_of_range_and_flagged_songs() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();
        let now = 1_700_000_000;
        let range_start = range_start_unix(StatsRange::SevenDays, now);

        let in_range = insert_song(&conn, "/a.flac", "In Range", "Artist A", "Album A", "Rock");
        let out_of_range = insert_song(&conn, "/b.flac", "Out", "Artist B", "Album B", "Pop");
        let excluded = insert_song(&conn, "/c.flac", "Excluded", "Artist C", "Album C", "Jazz");

        insert_play(&conn, in_range, range_start + 10);
        insert_play(&conn, out_of_range, range_start - 10);
        insert_play(&conn, excluded, range_start + 10);
        conn.execute(
            "INSERT INTO stats_exclusions (entity_type, entity_key) VALUES ('song', ?1)",
            params![excluded.to_string()],
        )
        .unwrap();

        let songs = top_songs(&conn, range_start).unwrap();
        let labels: Vec<&str> = songs.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["In Range"]);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_top_albums_uses_min_not_sum() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();
        let now = 1_700_000_000;
        let range_start = range_start_unix(StatsRange::SevenDays, now);

        // Track 1 played 5x, track 2 played 1x — MIN should be 1, not SUM (6).
        let t1 = insert_song(&conn, "/t1.flac", "T1", "Artist", "Album", "Rock");
        let t2 = insert_song(&conn, "/t2.flac", "T2", "Artist", "Album", "Rock");
        for _ in 0..5 {
            insert_play(&conn, t1, range_start + 10);
        }
        insert_play(&conn, t2, range_start + 10);

        let albums = top_albums(&conn, range_start).unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].play_count, 1);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_top_genres_splits_multi_value_and_respects_exclusions() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();
        let now = 1_700_000_000;
        let range_start = range_start_unix(StatsRange::SevenDays, now);

        let song = insert_song(
            &conn,
            "/multi.flac",
            "Multi",
            "Artist",
            "Album",
            "Metal; Symphonic Metal",
        );
        insert_play(&conn, song, range_start + 10);
        conn.execute(
            "INSERT INTO stats_exclusions (entity_type, entity_key) VALUES ('genre', 'symphonic metal')",
            params![],
        )
        .unwrap();

        let genres = top_genres(&conn, range_start).unwrap();
        let labels: Vec<&str> = genres.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, vec!["Metal"]);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_get_summary_at_returns_all_sections() {
        let (db, dir) = test_db();
        let conn = db.pool.get().unwrap();
        let now = 1_700_000_000;
        let range_start = range_start_unix(StatsRange::SevenDays, now);

        let song = insert_song(&conn, "/s.flac", "Song", "Artist", "Album", "Rock");
        insert_play(&conn, song, range_start + 10);

        let summary = get_summary_at(&conn, StatsRange::SevenDays, now).unwrap();
        assert_eq!(summary.range, "7d");
        assert_eq!(summary.top_songs.len(), 1);
        assert_eq!(summary.top_albums.len(), 1);
        assert_eq!(summary.top_artists.len(), 1);
        assert_eq!(summary.top_genres.len(), 1);
        assert_eq!(summary.play_timestamps, vec![range_start + 10]);

        let _ = std::fs::remove_dir_all(dir);
    }
}
