//! Local half of the two-way ratings sync (#1386): applying pulled
//! ListenBrainz feedback and CritiqueBrainz star ratings to the library, and
//! working out which local loves still need pushing. HTTP lives in
//! `scrobbler.rs`; everything here is plain SQLite so it can be tested alone.

use crate::models::LIBRARY_SOURCES_SQL;
use anyhow::Result;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};

/// A CritiqueBrainz review that carries a star rating, reduced to what the
/// library needs. `entity_type` is `"recording"` or `"release_group"`.
#[derive(Debug, Clone, PartialEq)]
pub struct CritiqueRating {
    pub entity_type: String,
    pub entity_mbid: String,
    pub stars: f32,
}

/// What a pull changed locally. `song_ids` are the songs whose stats changed
/// (callers emit `song-stats-changed` for them).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PullOutcome {
    pub loved: u32,
    pub hated: u32,
    pub song_ratings: u32,
    pub album_ratings: u32,
    pub song_ids: Vec<i64>,
}

/// Extract the CritiqueBrainz user UUID from a pasted profile URL
/// (`https://critiquebrainz.org/user/<uuid>`) or a bare UUID.
pub fn parse_critiquebrainz_user_id(input: &str) -> Option<String> {
    let candidate = input
        .trim()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let groups: Vec<&str> = candidate.split('-').collect();
    let shape_ok = groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()));
    shape_ok.then_some(candidate)
}

/// Make local love/hate match ListenBrainz for every recording it has
/// feedback on. Remote wins: ListenBrainz is the source of truth for the
/// recordings it knows about.
pub fn apply_feedback(conn: &mut Connection, remote: &HashMap<String, i32>) -> Result<PullOutcome> {
    let mut out = PullOutcome::default();
    let tx = conn.transaction()?;
    {
        let mut find =
            tx.prepare("SELECT id FROM songs WHERE musicbrainz_recording_id = ?1 AND loved != ?2")?;
        let mut set = tx.prepare("UPDATE songs SET loved = ?1 WHERE id = ?2")?;
        for (mbid, &score) in remote {
            let score = crate::stats::normalize_loved(score);
            if score == 0 {
                continue;
            }
            let ids: Vec<i64> = find
                .query_map(params![mbid, score], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for id in ids {
                set.execute(params![score, id])?;
                out.song_ids.push(id);
                if score > 0 {
                    out.loved += 1;
                } else {
                    out.hated += 1;
                }
            }
        }
    }
    tx.commit()?;
    Ok(out)
}

/// Loves/hates ListenBrainz doesn't have yet: `(recording_mbid, score)` for
/// each distinct recording with local feedback absent from `remote`.
/// Call after [`apply_feedback`], so a recording the two sides disagree on
/// has already been settled in favour of ListenBrainz.
pub fn pending_pushes(
    conn: &Connection,
    remote: &HashMap<String, i32>,
) -> Result<Vec<(String, i32)>> {
    let sql = format!(
        "SELECT musicbrainz_recording_id, loved FROM songs
         WHERE loved != 0
           AND musicbrainz_recording_id IS NOT NULL AND musicbrainz_recording_id != ''
           AND source IN ({lib}) AND unavailable = 0 AND not_included = 0
         ORDER BY id",
        lib = *LIBRARY_SOURCES_SQL
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i32>(1)?)))?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for row in rows {
        let (mbid, score) = row?;
        let mbid = mbid.trim().to_string();
        if !remote.contains_key(&mbid) && seen.insert(mbid.clone()) {
            out.push((mbid, score));
        }
    }
    Ok(out)
}

/// Overwrite local stars with the user's CritiqueBrainz ratings: recording
/// ratings land on `songs.rating`, release group ratings on every matching
/// album's `album_ratings` row. Remote wins.
pub fn apply_critique_ratings(
    conn: &mut Connection,
    ratings: &[CritiqueRating],
) -> Result<PullOutcome> {
    let mut out = PullOutcome::default();
    let tx = conn.transaction()?;
    for r in ratings {
        let stars = crate::stats::normalize_rating(r.stars);
        match r.entity_type.as_str() {
            "recording" => {
                let ids: Vec<i64> = {
                    let mut stmt = tx.prepare(
                        "SELECT id FROM songs WHERE musicbrainz_recording_id = ?1 AND rating != ?2",
                    )?;
                    let rows = stmt.query_map(params![r.entity_mbid, stars], |r| r.get(0))?;
                    rows.collect::<rusqlite::Result<_>>()?
                };
                for id in ids {
                    crate::stats::set_rating(&tx, id, stars)?;
                    out.song_ids.push(id);
                    out.song_ratings += 1;
                }
            }
            "release_group" => {
                let albums: Vec<String> = {
                    let mut stmt = tx.prepare(
                        "SELECT DISTINCT album FROM songs
                         WHERE musicbrainz_release_group_id = ?1 AND album IS NOT NULL AND album != ''",
                    )?;
                    let rows = stmt.query_map(params![r.entity_mbid], |r| r.get(0))?;
                    rows.collect::<rusqlite::Result<_>>()?
                };
                for album in albums {
                    if crate::stats::get_album_rating(&tx, &album)? != stars {
                        crate::stats::set_album_rating(&tx, &album, stars)?;
                        out.album_ratings += 1;
                    }
                }
            }
            _ => {}
        }
    }
    tx.commit()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn test_db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::Builder::new()
            .prefix("luminous_ratings_sync_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        (dir, db)
    }

    fn song(conn: &Connection, path: &str, rec: &str, rg: &str, album: &str, loved: i32) -> i64 {
        conn.execute(
            "INSERT INTO songs (path, title, album, musicbrainz_recording_id, musicbrainz_release_group_id, loved, source)
             VALUES (?1, 't', ?2, ?3, ?4, ?5, 1)",
            params![path, album, rec, rg, loved],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn loved_of(conn: &Connection, id: i64) -> i32 {
        conn.query_row("SELECT loved FROM songs WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .unwrap()
    }

    #[test]
    fn parses_bare_uuid_and_profile_url() {
        let id = "174405e7-894f-4b0d-af36-856631c0104f";
        assert_eq!(parse_critiquebrainz_user_id(id).as_deref(), Some(id));
        assert_eq!(
            parse_critiquebrainz_user_id(&format!(" https://critiquebrainz.org/user/{id}/ "))
                .as_deref(),
            Some(id)
        );
        assert_eq!(parse_critiquebrainz_user_id("RangerRick"), None);
        assert_eq!(parse_critiquebrainz_user_id(""), None);
    }

    #[test]
    fn feedback_pull_sets_love_and_hate_and_remote_wins() {
        let (_d, db) = test_db();
        let mut conn = db.pool.get().unwrap();
        let a = song(&conn, "a", "rec-a", "", "A", 0);
        let b = song(&conn, "b", "rec-b", "", "B", 0);
        let c = song(&conn, "c", "rec-c", "", "C", 1); // locally loved, remote hates
        let d = song(&conn, "d", "rec-d", "", "D", 1); // already agrees
        let remote = HashMap::from([
            ("rec-a".to_string(), 1),
            ("rec-b".to_string(), -1),
            ("rec-c".to_string(), -1),
            ("rec-d".to_string(), 1),
        ]);
        let out = apply_feedback(&mut conn, &remote).unwrap();
        assert_eq!((out.loved, out.hated), (1, 2));
        assert_eq!(loved_of(&conn, a), 1);
        assert_eq!(loved_of(&conn, b), -1);
        assert_eq!(loved_of(&conn, c), -1);
        assert_eq!(loved_of(&conn, d), 1);
        assert_eq!(out.song_ids.len(), 3);
    }

    #[test]
    fn pushes_only_local_feedback_remote_lacks_once_per_recording() {
        let (_d, db) = test_db();
        let conn = db.pool.get().unwrap();
        song(&conn, "a", "rec-a", "", "A", 1); // on remote already
        song(&conn, "b", "rec-b", "", "B", 1); // missing remotely
        song(&conn, "b2", "rec-b", "", "B", 1); // duplicate file of the same recording
        song(&conn, "c", "rec-c", "", "C", -1); // hate missing remotely
        song(&conn, "d", "rec-d", "", "D", 0); // neutral
        song(&conn, "e", "", "", "E", 1); // no MBID
        let remote = HashMap::from([("rec-a".to_string(), 1)]);
        let mut pushes = pending_pushes(&conn, &remote).unwrap();
        pushes.sort();
        assert_eq!(
            pushes,
            vec![("rec-b".to_string(), 1), ("rec-c".to_string(), -1)]
        );
    }

    #[test]
    fn critique_ratings_update_songs_and_albums_and_count_only_changes() {
        let (_d, db) = test_db();
        let mut conn = db.pool.get().unwrap();
        let s = song(&conn, "a", "rec-a", "rg-1", "Album One", 0);
        let s2 = song(&conn, "b", "rec-b", "rg-1", "Album One", 0);
        crate::stats::set_rating(&conn, s2, 3.0).unwrap();
        let rating = |t: &str, id: &str, stars: f32| CritiqueRating {
            entity_type: t.into(),
            entity_mbid: id.into(),
            stars,
        };
        let ratings = vec![
            rating("recording", "rec-a", 4.0),
            rating("recording", "rec-b", 3.0),
            rating("release_group", "rg-1", 5.0),
            rating("artist", "x", 5.0),
        ];
        let out = apply_critique_ratings(&mut conn, &ratings).unwrap();
        assert_eq!((out.song_ratings, out.album_ratings), (1, 1));
        assert_eq!(out.song_ids, vec![s]);
        assert_eq!(
            crate::stats::get_album_rating(&conn, "Album One").unwrap(),
            5.0
        );
        let again = apply_critique_ratings(&mut conn, &ratings).unwrap();
        assert_eq!((again.song_ratings, again.album_ratings), (0, 0));
    }
}
