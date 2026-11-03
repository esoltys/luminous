//! Album identity (#1752, #1932): which tracks belong to the same album.
//!
//! An album's identity is defined by its release artist (or compilation status)
//! and its title. Two tracks belong to the same album if and only if their
//! composite keys match:
//! - If `is_compilation` is true, or `album_artist` is "Various Artists" (case-insensitive),
//!   the artist portion is `"[va]"`.
//! - Otherwise, the artist portion is `effective_artist = COALESCE(NULLIF(album_artist, ''), artist)`
//!   trimmed and lowercased.
//! - The album portion is `album` trimmed and lowercased.
//! - The composite key is `"{artist_key}\u{1f}{album_key}"`.
//!
//! Tracks whose album tag differs only in letter case or surrounding
//! whitespace ("Songs of the Sea", "Songs Of The Sea", "Songs of the Sea ")
//! remain one album everywhere. `songs.album_key` stores this composite key
//! for every song (written by `collection::upsert_song` and the tag editor),
//! and every album-keyed table (`album_ratings`, `album_profiles`,
//! `album_chart_history`, album rows of `stats_exclusions`,
//! `subsonic_album_cache`) is keyed by it, so SQL compares folded keys with
//! plain `=` and never needs `COLLATE NOCASE` (which folds ASCII only).
//!
//! The frontend mirrors this rule in `src/lib/utils/albumKey.ts`; both sides
//! are checked against `src/lib/utils/albumKey.fixture.json`.

/// The separator between release artist and album title in composite `album_key`.
pub const COMPOSITE_KEY_SEPARATOR: char = '\u{1f}';
/// Artist key for Various Artists / compilations.
pub const COMPILATION_ARTIST_KEY: &str = "[va]";

/// Normalizes a string by trimming surrounding whitespace and Unicode lowercasing.
pub fn fold_key(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Returns true if the artist string represents "Various Artists" (or standard variants).
pub fn is_various_artists(artist: &str) -> bool {
    let folded = fold_key(artist);
    folded == "various artists"
        || folded == "various"
        || folded == "v/a"
        || folded == "va"
        || folded == "v.a."
        || folded == "v. a."
        || folded == "[va]"
}

/// Computes the composite identity key for an album release.
///
/// An album release is identified by:
/// 1. Compilation releases (`compilation != 0` or artist matching "Various Artists"):
///    `"[va]\u{1f}{folded_album}"`
/// 2. Single-artist releases:
///    `"{effective_artist}\u{1f}{folded_album}"`, where `effective_artist` prefers
///    `album_artist` over track `artist`.
/// 3. If `album` is empty after trimming, returns `""` (no album).
pub fn album_key_for(
    album: &str,
    album_artist: Option<&str>,
    artist: Option<&str>,
    is_compilation: bool,
) -> String {
    let folded_album = fold_key(album);
    if folded_album.is_empty() {
        return String::new();
    }

    if folded_album.contains(COMPOSITE_KEY_SEPARATOR) {
        return folded_album;
    }

    let eff_artist = album_artist
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| artist.map(str::trim).filter(|s| !s.is_empty()));

    if eff_artist.is_none() && !is_compilation {
        return folded_album;
    }

    let artist_key = if is_compilation || eff_artist.map_or(false, is_various_artists) {
        COMPILATION_ARTIST_KEY.to_string()
    } else {
        eff_artist.map(fold_key).unwrap_or_default()
    };

    format!("{artist_key}{COMPOSITE_KEY_SEPARATOR}{folded_album}")
}

/// The identity key for an album title: surrounding whitespace trimmed, then
/// Unicode lowercased. Unicode normalization forms (NFC vs NFD) are not
/// folded. A blank title folds to `""`, which callers treat as "no album".
pub fn album_key(album: &str) -> String {
    fold_key(album)
}

/// Correlated SQL subquery yielding the display spelling of the album whose
/// `album_key` is the SQL expression `key_col`: the spelling most of its
/// library songs carry, ties going to the lowest binary value. Every query
/// that emits a title for a merged album uses this, so all views agree on
/// one name.
pub fn album_display_sql(key_col: &str) -> String {
    format!(
        "(SELECT d.album FROM songs d
          WHERE (d.album_key = {key_col} OR d.album_key LIKE '%\u{1f}' || {key_col}) AND d.source IN ({lib}) AND d.unavailable = 0
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
    let mut stmt = conn.prepare(
        "SELECT id, album, album_artist, artist, compilation
         FROM songs
         WHERE album_key IS NULL AND album IS NOT NULL AND TRIM(album) != ''",
    )?;
    let songs: Vec<(i64, String, Option<String>, Option<String>, i64)> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get::<_, Option<i64>>(4)?.unwrap_or(0),
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;

    if songs.is_empty() {
        return Ok(());
    }

    let mut update = conn.prepare("UPDATE songs SET album_key = ?1 WHERE id = ?2")?;
    for (id, album, album_artist, artist, compilation) in &songs {
        let key = album_key_for(
            album,
            album_artist.as_deref(),
            artist.as_deref(),
            *compilation != 0,
        );
        update.execute(rusqlite::params![key, id])?;
    }
    Ok(())
}

/// How many songs carry exactly this album spelling: the tie-break when
/// several spellings of one album each have a row in an album-keyed table.
pub(crate) fn songs_spelled(conn: &rusqlite::Connection, raw: &str) -> rusqlite::Result<i64> {
    let album_title = if let Some((_, album)) = raw.split_once(COMPOSITE_KEY_SEPARATOR) {
        album
    } else {
        raw
    };
    conn.query_row(
        "SELECT COUNT(*) FROM songs WHERE album = ?1",
        rusqlite::params![album_title],
        |r| r.get(0),
    )
}

/// Rekeys `album_profiles` to composite `album_key`, keeping one row per album where
/// several spellings had one: the spelling with the most songs, then the row
/// with the most fields filled in.
pub(crate) fn rekey_album_profiles(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    use std::collections::HashMap;

    let profiles: Vec<(i64, String, Option<String>, i64)> = conn
        .prepare(
            "SELECT rowid, album_key, artist_key,
                    (COALESCE(description, '') != '') + (COALESCE(website, '') != '')
                  + (COALESCE(links, '[]') != '[]')
                  + (COALESCE(fetched_cover_filename, '') != '') + (COALESCE(fetched_disc_filename, '') != '')
             FROM album_profiles",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;

    let resolve_key = |raw: &str, artist_key: Option<&str>| -> String {
        if raw.contains(COMPOSITE_KEY_SEPARATOR) {
            return fold_key(raw);
        }
        if let Some(art) = artist_key.filter(|s| !s.trim().is_empty()) {
            return album_key_for(raw, None, Some(art), false);
        }
        conn.query_row(
            "SELECT album_key FROM songs WHERE album = ?1 AND album_key IS NOT NULL LIMIT 1",
            rusqlite::params![raw],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_else(|_| album_key_for(raw, None, None, false))
    };

    if profiles
        .iter()
        .all(|(_, raw, art, _)| resolve_key(raw, art.as_deref()) == *raw)
    {
        return Ok(());
    }

    // (songs with this spelling, filled fields, rowid) of the best row per key.
    let mut best: HashMap<String, (i64, i64, i64)> = HashMap::new();
    for (rowid, raw, art, filled) in &profiles {
        let key = resolve_key(raw, art.as_deref());
        let rank = (songs_spelled(conn, raw)?, *filled, *rowid);
        best.entry(key)
            .and_modify(|b| {
                if (rank.0, rank.1) > (b.0, b.1) {
                    *b = rank;
                }
            })
            .or_insert(rank);
    }
    let keep: std::collections::HashSet<i64> = best.values().map(|b| b.2).collect();
    // A savepoint rather than a transaction: migration 62/73 calls this inside its own.
    conn.execute_batch("SAVEPOINT rekey_album_profiles;")?;
    let rekeyed = (|| -> rusqlite::Result<()> {
        for (rowid, _, _, _) in profiles.iter().filter(|(id, _, _, _)| !keep.contains(id)) {
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

    #[test]
    fn test_album_key_for_artists_distinguishes_same_name_albums() {
        let kacey = album_key_for("Golden Hour", Some("Kacey Musgraves"), None, false);
        let balcony = album_key_for("Golden Hour", None, Some("Balcony Sunrise"), false);
        assert_ne!(kacey, balcony);
        assert_eq!(
            kacey,
            format!("kacey musgraves{}golden hour", COMPOSITE_KEY_SEPARATOR)
        );
        assert_eq!(
            balcony,
            format!("balcony sunrise{}golden hour", COMPOSITE_KEY_SEPARATOR)
        );

        // Case/whitespace normalization on same album preserves identity
        let kacey2 = album_key_for("  Golden Hour  ", None, Some("kacey musgraves "), false);
        assert_eq!(kacey, kacey2);
    }

    #[test]
    fn test_album_key_for_compilations() {
        let va1 = album_key_for(
            "Greatest Hits",
            Some("Various Artists"),
            Some("Artist 1"),
            false,
        );
        let va2 = album_key_for("Greatest Hits", None, Some("Artist 2"), true);
        assert_eq!(va1, format!("[va]{}greatest hits", COMPOSITE_KEY_SEPARATOR));
        assert_eq!(va2, format!("[va]{}greatest hits", COMPOSITE_KEY_SEPARATOR));
        assert_eq!(va1, va2);
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
            artist: Some("The Band".into()),
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
        assert_eq!(
            key().as_deref(),
            Some(format!("the band{}foo", COMPOSITE_KEY_SEPARATOR).as_str())
        );

        // A rename that leaves album_key alone goes stale, then is repaired.
        conn.execute("UPDATE songs SET album = 'Bar' WHERE path = 'a.mp3'", [])
            .unwrap();
        assert_eq!(key(), None, "trigger clears the stale key");
        repair_album_keys(&conn).unwrap();
        assert_eq!(
            key().as_deref(),
            Some(format!("the band{}bar", COMPOSITE_KEY_SEPARATOR).as_str())
        );

        // A case-only edit can't change the key, so it is kept.
        conn.execute("UPDATE songs SET album = 'BAR ' WHERE path = 'a.mp3'", [])
            .unwrap();
        assert_eq!(
            key().as_deref(),
            Some(format!("the band{}bar", COMPOSITE_KEY_SEPARATOR).as_str())
        );

        // A profile saved under the raw title beside the app's folded row
        // merges into one row; the spelling the songs carry wins.
        let prof_key_1 = format!("the band{}bar", COMPOSITE_KEY_SEPARATOR);
        let prof_key_2 = format!("the band{}BAR ", COMPOSITE_KEY_SEPARATOR);
        conn.execute(
            "INSERT INTO album_profiles (album_key, description) VALUES (?1, 'app notes')",
            rusqlite::params![prof_key_1],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO album_profiles (album_key, description) VALUES (?1, 'mcp notes')",
            rusqlite::params![prof_key_2],
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
        assert_eq!(
            rows,
            vec![(
                format!("the band{}bar", COMPOSITE_KEY_SEPARATOR),
                "mcp notes".to_string()
            )]
        );
    }
}
