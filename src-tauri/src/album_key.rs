//! Album identity (#1752): which album titles count as the same album.
//!
//! Tracks whose album tag differs only in letter case or surrounding
//! whitespace ("Songs of the Sea", "Songs Of The Sea", "Songs of the Sea ")
//! are one album everywhere: the album grid, album detail, ratings, profiles,
//! charts, stats exclusions and playback grouping. `songs.album_key` stores
//! this fold for every song (written by `collection::upsert_song` and the tag
//! editor), and every album-keyed table (`album_ratings`, `album_profiles`,
//! `album_chart_history`, album rows of `stats_exclusions`,
//! `subsonic_album_cache`) is keyed by it, so SQL compares folded keys with
//! plain `=` and never needs `COLLATE NOCASE` (which folds ASCII only).
//!
//! The frontend mirrors this rule in `src/lib/utils/albumKey.ts`; both sides
//! are checked against `src/lib/utils/albumKey.fixture.json`.

/// The identity key for an album title: surrounding whitespace trimmed, then
/// Unicode lowercased. Unicode normalization forms (NFC vs NFD) are not
/// folded. A blank title folds to `""`, which callers treat as "no album".
pub fn album_key(album: &str) -> String {
    album.trim().to_lowercase()
}

/// Correlated SQL subquery yielding the display spelling of the album whose
/// `album_key` is the SQL expression `key_col`: the spelling most of its
/// library songs carry, ties going to the lowest binary value. Every query
/// that emits a title for a merged album uses this, so all views agree on
/// one name.
pub fn album_display_sql(key_col: &str) -> String {
    format!(
        "(SELECT d.album FROM songs d
          WHERE d.album_key = {key_col} AND d.source IN ({lib}) AND d.unavailable = 0
          GROUP BY d.album ORDER BY COUNT(*) DESC, d.album LIMIT 1)",
        lib = *crate::models::LIBRARY_SOURCES_SQL
    )
}

/// Brings album keys up to date after writers that bypass this module —
/// luminous-mcp edits `songs.album` and `album_profiles` in this database
/// directly. Fills `songs.album_key` where it is missing (new rows from such
/// a writer, or rows the `songs_album_key_stale` trigger nulled after an
/// album rename) and rekeys `album_profiles` rows saved under a raw title.
/// Cheap when nothing needs fixing (indexed `IS NULL` scan plus one small
/// table read). Runs at startup and before the album grid loads.
pub fn repair_album_keys(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    backfill_song_keys(conn)?;
    rekey_album_profiles(conn)
}

/// Sets `album_key` on every song with an album but no key.
pub(crate) fn backfill_song_keys(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let albums: Vec<String> = conn
        .prepare("SELECT DISTINCT album FROM songs WHERE album_key IS NULL AND album IS NOT NULL")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if albums.is_empty() {
        return Ok(());
    }
    let mut update =
        conn.prepare("UPDATE songs SET album_key = ?1 WHERE album = ?2 AND album_key IS NULL")?;
    for album in &albums {
        update.execute(rusqlite::params![album_key(album), album])?;
    }
    Ok(())
}

/// How many songs carry exactly this album spelling: the tie-break when
/// several spellings of one album each have a row in an album-keyed table.
pub(crate) fn songs_spelled(conn: &rusqlite::Connection, raw: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM songs WHERE album = ?1",
        rusqlite::params![raw],
        |r| r.get(0),
    )
}

/// Rekeys `album_profiles` to `album_key`, keeping one row per album where
/// several spellings had one: the spelling with the most songs, then the row
/// with the most fields filled in.
pub(crate) fn rekey_album_profiles(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    use std::collections::HashMap;

    let profiles: Vec<(i64, String, i64)> = conn
        .prepare(
            "SELECT rowid, album_key,
                    (COALESCE(description, '') != '') + (COALESCE(website, '') != '')
                  + (COALESCE(links, '[]') != '[]')
                  + (COALESCE(fetched_cover_filename, '') != '') + (COALESCE(fetched_disc_filename, '') != '')
             FROM album_profiles",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if profiles.iter().all(|(_, raw, _)| album_key(raw) == *raw) {
        return Ok(());
    }

    // (songs with this spelling, filled fields, rowid) of the best row per key.
    let mut best: HashMap<String, (i64, i64, i64)> = HashMap::new();
    for (rowid, raw, filled) in &profiles {
        let rank = (songs_spelled(conn, raw)?, *filled, *rowid);
        best.entry(album_key(raw))
            .and_modify(|b| {
                if (rank.0, rank.1) > (b.0, b.1) {
                    *b = rank;
                }
            })
            .or_insert(rank);
    }
    let keep: std::collections::HashSet<i64> = best.values().map(|b| b.2).collect();
    // A savepoint rather than a transaction: migration 62 calls this inside its own.
    conn.execute_batch("SAVEPOINT rekey_album_profiles;")?;
    let rekeyed = (|| -> rusqlite::Result<()> {
        for (rowid, _, _) in profiles.iter().filter(|(id, _, _)| !keep.contains(id)) {
            conn.execute(
                "DELETE FROM album_profiles WHERE rowid = ?1",
                rusqlite::params![rowid],
            )?;
        }
        for (key, (_, _, rowid)) in &best {
            conn.execute(
                "UPDATE album_profiles SET album_key = ?1 WHERE rowid = ?2 AND album_key != ?1",
                rusqlite::params![key, rowid],
            )?;
        }
        Ok(())
    })();
    if rekeyed.is_err() {
        let _ = conn.execute_batch("ROLLBACK TO rekey_album_profiles;");
    }
    conn.execute_batch("RELEASE rekey_album_profiles;")?;
    Ok(rekeyed?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct FixtureCase {
        input: String,
        key: String,
    }

    /// The same cases `albumKey.test.ts` runs, so the TypeScript mirror
    /// can't drift from this rule.
    #[test]
    fn matches_shared_fixture() {
        let raw = include_str!("../../src/lib/utils/albumKey.fixture.json");
        let cases: Vec<FixtureCase> = serde_json::from_str(raw).unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(album_key(&case.input), case.key, "input {:?}", case.input);
        }
    }

    /// luminous-mcp writes `songs.album` and `album_profiles` without
    /// knowing about album keys; the trigger plus `repair_album_keys` must
    /// leave every row keyed as if the app had written it.
    #[test]
    fn repair_rekeys_rows_from_external_writers() {
        let dir = tempfile::Builder::new()
            .prefix("luminous_album_key_repair_")
            .tempdir()
            .unwrap();
        let db = crate::db::Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();
        let song = crate::models::Song {
            path: Some("a.mp3".into()),
            album: Some("Foo".into()),
            source: crate::models::SongSource::LocalFile,
            filetype: crate::models::FileType::Mp3,
            ..Default::default()
        };
        crate::collection::upsert_song(&conn, &song).unwrap();
        let key = || -> Option<String> {
            conn.query_row(
                "SELECT album_key FROM songs WHERE path = 'a.mp3'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(key().as_deref(), Some("foo"));

        // A rename that leaves album_key alone goes stale, then is repaired.
        conn.execute("UPDATE songs SET album = 'Bar' WHERE path = 'a.mp3'", [])
            .unwrap();
        assert_eq!(key(), None, "trigger clears the stale key");
        repair_album_keys(&conn).unwrap();
        assert_eq!(key().as_deref(), Some("bar"));

        // A case-only edit can't change the key, so it is kept.
        conn.execute("UPDATE songs SET album = 'BAR ' WHERE path = 'a.mp3'", [])
            .unwrap();
        assert_eq!(key().as_deref(), Some("bar"));

        // A profile saved under the raw title beside the app's folded row
        // merges into one row; the spelling the songs carry wins.
        conn.execute_batch(
            "INSERT INTO album_profiles (album_key, description) VALUES ('bar', 'app notes');
             INSERT INTO album_profiles (album_key, description) VALUES ('BAR ', 'mcp notes');",
        )
        .unwrap();
        repair_album_keys(&conn).unwrap();
        let rows: Vec<(String, String)> = conn
            .prepare("SELECT album_key, description FROM album_profiles")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(rows, vec![("bar".to_string(), "mcp notes".to_string())]);
    }
}
