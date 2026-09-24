//! Two-way sync with OpenSubsonic servers (#1165): plays, ratings and
//! favourites made in Luminous are pushed back to the server a track came
//! from.
//!
//! Everything here is fire-and-forget. Failures are logged and never
//! surfaced, because the local write has already succeeded and the next
//! sync reconciles any difference. Plays are the exception: a play that
//! couldn't be reported waits in `subsonic_scrobble_queue` and is retried
//! the next time the server answers.
//!
//! The functions are blocking (network + SQLite) and never hold a pooled
//! connection across a request; the `spawn_*` wrappers run them on a
//! blocking thread.

use super::{parse_track_uri, StarTarget, SubsonicApiError, SubsonicClient};
use crate::db::Database;
use crate::models::Song;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Arc;

/// Luminous rating at or above which a song/album counts as a server-side
/// favourite (star) — the inverse of `sync::server_rating`'s mapping.
pub const STAR_THRESHOLD: f32 = 4.0;

/// A queued play is dropped after this many failed attempts.
const MAX_QUEUE_ATTEMPTS: i64 = 20;

/// API error codes that mean a queued play can never succeed (missing
/// parameter, unknown track), so retrying it is pointless.
const PERMANENT_ERROR_CODES: [i64; 2] = [10, 70];

/// Serializes queue drains so two reports can't submit the same row twice.
static FLUSH_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

struct Server {
    id: i64,
    report_plays: bool,
    client: SubsonicClient,
}

/// The enabled server `server_id`, with a client for it. `None` when it
/// was removed or disabled.
fn load_server(conn: &Connection, server_id: i64) -> Result<Option<Server>> {
    let row: Option<(String, String, Option<String>, bool)> = conn
        .query_row(
            "SELECT url, username, password, report_plays FROM subsonic_servers
             WHERE id = ?1 AND enabled = 1",
            params![server_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((url, username, password, report_plays)) = row else {
        return Ok(None);
    };
    let client = SubsonicClient::new(&url, &username, &password.unwrap_or_default())?;
    Ok(Some(Server {
        id: server_id,
        report_plays,
        client,
    }))
}

fn load_server_from_pool(db: &Database, server_id: i64) -> Result<Option<Server>> {
    let conn = db.pool.get()?;
    load_server(&conn, server_id)
}

/// Luminous rating (0.5–5 in half steps, negative = unrated) → Subsonic
/// `setRating` value (1–5, 0 = clear). Half stars round up.
pub fn rating_to_server(rating: f32) -> u8 {
    if rating < 0.0 {
        0
    } else {
        rating.round().clamp(1.0, 5.0) as u8
    }
}

fn wants_star(rating: f32) -> bool {
    rating >= STAR_THRESHOLD
}

// ---------------------------------------------------------------------------
// Plays
// ---------------------------------------------------------------------------

/// Reports `path` as "now playing" (`scrobble?submission=false`) when it's
/// a Subsonic track on a server with Report Plays on. A successful call
/// also drains any plays queued while the server was unreachable.
pub fn report_now_playing(db: &Database, path: &str) {
    let Some((server_id, track_id)) = parse_track_uri(path) else {
        return;
    };
    let server = match load_server_from_pool(db, server_id) {
        Ok(Some(s)) if s.report_plays => s,
        Ok(_) => return,
        Err(e) => {
            log::warn!("Subsonic now-playing: couldn't load server {server_id}: {e:#}");
            return;
        }
    };
    match server.client.scrobble(&track_id, false, None) {
        Ok(()) => flush_queue(db, &server),
        Err(e) => log::warn!("Subsonic now-playing failed for server {server_id}: {e:#}"),
    }
}

/// Records a completed play of `path` at `listened_at` (Unix seconds) on
/// its server: queues it, then drains the server's queue.
pub fn report_play(db: &Database, path: &str, listened_at: i64) {
    let Some((server_id, track_id)) = parse_track_uri(path) else {
        return;
    };
    let queued = (|| -> Result<Option<Server>> {
        let conn = db.pool.get()?;
        let Some(server) = load_server(&conn, server_id)?.filter(|s| s.report_plays) else {
            return Ok(None);
        };
        conn.execute(
            "INSERT INTO subsonic_scrobble_queue (server_id, remote_id, listened_at) VALUES (?1, ?2, ?3)",
            params![server_id, track_id, listened_at],
        )?;
        Ok(Some(server))
    })();
    match queued {
        Ok(Some(server)) => flush_queue(db, &server),
        Ok(None) => {}
        Err(e) => {
            log::warn!("Subsonic scrobble: couldn't queue play for server {server_id}: {e:#}")
        }
    }
}

fn queued_plays(db: &Database, server_id: i64) -> Result<Vec<(i64, String, i64, i64)>> {
    let conn = db.pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, remote_id, listened_at, attempts FROM subsonic_scrobble_queue
         WHERE server_id = ?1 ORDER BY listened_at, id",
    )?;
    let rows = stmt
        .query_map(params![server_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Submits `server`'s queued plays oldest first. Stops at the first
/// transient failure (the server is likely down; the rest wait for the
/// next report) and drops rows that can never succeed.
fn flush_queue(db: &Database, server: &Server) {
    let _guard = FLUSH_LOCK.lock();
    let rows = match queued_plays(db, server.id) {
        Ok(rows) => rows,
        Err(e) => {
            log::warn!("Subsonic scrobble: couldn't read queue: {e:#}");
            return;
        }
    };

    for (row_id, track_id, listened_at, attempts) in rows {
        let result = server.client.scrobble(&track_id, true, Some(listened_at));
        let Ok(conn) = db.pool.get() else { return };
        let delete = || {
            let _ = conn.execute(
                "DELETE FROM subsonic_scrobble_queue WHERE id = ?1",
                params![row_id],
            );
        };
        match result {
            Ok(()) => delete(),
            Err(e) => {
                let permanent = e
                    .downcast_ref::<SubsonicApiError>()
                    .is_some_and(|api| PERMANENT_ERROR_CODES.contains(&api.0.code));
                if permanent || attempts + 1 >= MAX_QUEUE_ATTEMPTS {
                    log::warn!(
                        "Dropping queued Subsonic play of '{track_id}' on server {}: {e:#}",
                        server.id
                    );
                    delete();
                    continue;
                }
                log::warn!(
                    "Subsonic scrobble failed on server {} (will retry): {e:#}",
                    server.id
                );
                let _ = conn.execute(
                    "UPDATE subsonic_scrobble_queue SET attempts = attempts + 1 WHERE id = ?1",
                    params![row_id],
                );
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ratings & favourites
// ---------------------------------------------------------------------------

/// Pushes a song's new Luminous rating to its server: `setRating`, plus
/// `star`/`unstar` when the rating crosses the favourite threshold relative
/// to what the server last had. On success the cache records the server's
/// new state, so the next sync sees no server-side change and leaves the
/// local rating alone.
pub fn push_song_rating(db: &Database, path: &str, rating: f32) {
    let Some((server_id, track_id)) = parse_track_uri(path) else {
        return;
    };
    let loaded = (|| -> Result<Option<(Server, bool)>> {
        let conn = db.pool.get()?;
        let Some(server) = load_server(&conn, server_id)? else {
            return Ok(None);
        };
        let starred: bool = conn
            .query_row(
                "SELECT server_starred FROM subsonic_cache WHERE server_id = ?1 AND remote_id = ?2",
                params![server_id, track_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false);
        Ok(Some((server, starred)))
    })();
    let (server, was_starred) = match loaded {
        Ok(Some(v)) => v,
        Ok(None) => return,
        Err(e) => {
            log::warn!("Subsonic rating: couldn't load server {server_id}: {e:#}");
            return;
        }
    };

    let pushed = push_rating(
        &server.client,
        &track_id,
        StarTarget::Song(&track_id),
        rating,
        was_starred,
    );
    match pushed {
        Ok((server_rating, starred)) => {
            let updated = (|| -> Result<()> {
                db.pool.get()?.execute(
                    "UPDATE subsonic_cache SET server_rating = ?1, server_starred = ?2
                     WHERE server_id = ?3 AND remote_id = ?4",
                    params![server_rating, starred, server_id, track_id],
                )?;
                Ok(())
            })();
            if let Err(e) = updated {
                log::warn!("Subsonic rating: couldn't update cache: {e:#}");
            }
        }
        Err(e) => log::warn!("Subsonic rating push failed for server {server_id}: {e:#}"),
    }
}

/// Pushes an album's new rating to the server album it maps to. Album
/// ratings are keyed by bare title, so this only happens when exactly one
/// server album (across all servers) has that title — pushing to several
/// would be a guess.
pub fn push_album_rating(db: &Database, album_key: &str, rating: f32) {
    let loaded = (|| -> Result<Option<(Server, String, bool)>> {
        let conn = db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT server_id, remote_album_id, server_starred FROM subsonic_album_cache
             WHERE album_key = ?1",
        )?;
        let matches = stmt
            .query_map(params![album_key], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        match matches.as_slice() {
            [(server_id, album_id, starred)] => {
                Ok(load_server(&conn, *server_id)?.map(|s| (s, album_id.clone(), *starred)))
            }
            [] => Ok(None),
            _ => {
                log::info!(
                    "Not pushing album rating for '{album_key}': {} server albums share the title",
                    matches.len()
                );
                Ok(None)
            }
        }
    })();
    let (server, album_id, was_starred) = match loaded {
        Ok(Some(v)) => v,
        Ok(None) => return,
        Err(e) => {
            log::warn!("Subsonic album rating: couldn't load server: {e:#}");
            return;
        }
    };

    let pushed = push_rating(
        &server.client,
        &album_id,
        StarTarget::Album(&album_id),
        rating,
        was_starred,
    );
    match pushed {
        Ok((server_rating, starred)) => {
            let updated = (|| -> Result<()> {
                db.pool.get()?.execute(
                    "UPDATE subsonic_album_cache SET server_rating = ?1, server_starred = ?2
                     WHERE server_id = ?3 AND remote_album_id = ?4",
                    params![server_rating, starred, server.id, album_id],
                )?;
                Ok(())
            })();
            if let Err(e) = updated {
                log::warn!("Subsonic album rating: couldn't update cache: {e:#}");
            }
        }
        Err(e) => log::warn!(
            "Subsonic album rating push failed for server {}: {e:#}",
            server.id
        ),
    }
}

/// `setRating` + `star`/`unstar` as needed. Returns the server's resulting
/// (`server_rating`, `server_starred`) in the shape the caches store.
fn push_rating(
    client: &SubsonicClient,
    id: &str,
    star_target: StarTarget<'_>,
    rating: f32,
    was_starred: bool,
) -> Result<(Option<i64>, bool)> {
    let value = rating_to_server(rating);
    client.set_rating(id, value)?;
    let starred = wants_star(rating);
    if starred != was_starred {
        client.set_starred(star_target, starred)?;
    }
    Ok(((value > 0).then_some(value as i64), starred))
}

// ---------------------------------------------------------------------------
// Async entry points (scrobbler hooks and rating commands)
// ---------------------------------------------------------------------------

fn subsonic_path(song: &Song) -> Option<String> {
    let path = song.path.as_deref()?;
    parse_track_uri(path).map(|_| path.to_string())
}

pub fn spawn_now_playing(db: Arc<Database>, song: &Song) {
    if let Some(path) = subsonic_path(song) {
        tauri::async_runtime::spawn_blocking(move || report_now_playing(&db, &path));
    }
}

pub fn spawn_play(db: Arc<Database>, song: &Song, listened_at: i64) {
    if let Some(path) = subsonic_path(song) {
        tauri::async_runtime::spawn_blocking(move || report_play(&db, &path, listened_at));
    }
}

pub fn spawn_song_rating(db: Arc<Database>, song: &Song, rating: f32) {
    if let Some(path) = subsonic_path(song) {
        tauri::async_runtime::spawn_blocking(move || push_song_rating(&db, &path, rating));
    }
}

pub fn spawn_album_rating(db: Arc<Database>, album_key: String, rating: f32) {
    tauri::async_runtime::spawn_blocking(move || push_album_rating(&db, &album_key, rating));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsonic::track_uri;
    use wiremock::matchers::{any, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const OK: &str =
        r#"{"subsonic-response":{"status":"ok","version":"1.16.1","openSubsonic":true}}"#;
    const NOT_FOUND: &str = r#"{"subsonic-response":{"status":"failed","version":"1.16.1","openSubsonic":true,"error":{"code":70,"message":"data not found"}}}"#;

    fn json(body: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(body.to_string(), "application/json")
    }

    fn endpoint(name: &str) -> wiremock::MockBuilder {
        Mock::given(method("GET")).and(path(format!("/rest/{name}.view")))
    }

    /// A temp DB with server 1 pointing at `url`.
    fn temp_db(tag: &str, url: &str, report_plays: bool) -> (Arc<Database>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "luminous_subsonic_report_{tag}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(dir.clone()).unwrap());
        db.pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO subsonic_servers (id, name, url, username, password, report_plays)
                 VALUES (1, 'Home', ?1, 'u', 'p', ?2)",
                params![url, report_plays],
            )
            .unwrap();
        (db, dir)
    }

    async fn blocking<F: FnOnce() + Send + 'static>(f: F) {
        tokio::task::spawn_blocking(f).await.unwrap();
    }

    fn queue(db: &Database) -> Vec<(String, i64, i64)> {
        let conn = db.pool.get().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT remote_id, listened_at, attempts FROM subsonic_scrobble_queue ORDER BY id",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }

    fn song_cache(db: &Database, remote_id: &str) -> (Option<i64>, bool) {
        db.pool
            .get()
            .unwrap()
            .query_row(
                "SELECT server_rating, server_starred FROM subsonic_cache WHERE remote_id = ?1",
                params![remote_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    }

    #[test]
    fn rating_maps_to_server_scale() {
        assert_eq!(rating_to_server(-1.0), 0);
        assert_eq!(rating_to_server(0.5), 1);
        assert_eq!(rating_to_server(2.5), 3);
        assert_eq!(rating_to_server(3.0), 3);
        assert_eq!(rating_to_server(5.0), 5);
        assert!(!wants_star(3.5));
        assert!(wants_star(4.0));
    }

    #[tokio::test]
    async fn now_playing_reports_and_drains_queued_plays() {
        let server = MockServer::start().await;
        endpoint("scrobble")
            .and(query_param("id", "t1"))
            .and(query_param("submission", "false"))
            .respond_with(json(OK))
            .expect(1)
            .mount(&server)
            .await;
        endpoint("scrobble")
            .and(query_param("id", "t0"))
            .and(query_param("submission", "true"))
            .and(query_param("time", "1700000000000"))
            .respond_with(json(OK))
            .expect(1)
            .mount(&server)
            .await;

        let (db, dir) = temp_db("now_playing", &server.uri(), true);
        db.pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO subsonic_scrobble_queue (server_id, remote_id, listened_at) VALUES (1, 't0', 1700000000)",
                [],
            )
            .unwrap();
        let db2 = db.clone();
        blocking(move || report_now_playing(&db2, &track_uri(1, "t1"))).await;
        assert!(queue(&db).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn report_plays_off_sends_nothing() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(json(OK))
            .expect(0)
            .mount(&server)
            .await;

        let (db, dir) = temp_db("report_off", &server.uri(), false);
        let db2 = db.clone();
        blocking(move || {
            report_now_playing(&db2, &track_uri(1, "t1"));
            report_play(&db2, &track_uri(1, "t1"), 1_700_000_000);
            // Local files are ignored outright.
            report_play(&db2, "/music/a.flac", 1_700_000_000);
        })
        .await;
        assert!(queue(&db).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn failed_play_is_queued_and_retried_on_next_report() {
        let server = MockServer::start().await;
        endpoint("scrobble")
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;

        let (db, dir) = temp_db("retry", &server.uri(), true);
        let db2 = db.clone();
        blocking(move || report_play(&db2, &track_uri(1, "t1"), 100)).await;
        assert_eq!(queue(&db), vec![("t1".to_string(), 100, 1)]);

        endpoint("scrobble")
            .and(query_param("submission", "true"))
            .respond_with(json(OK))
            .expect(2)
            .mount(&server)
            .await;
        let db2 = db.clone();
        blocking(move || report_play(&db2, &track_uri(1, "t2"), 200)).await;
        assert!(queue(&db).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn play_rejected_by_server_is_dropped() {
        let server = MockServer::start().await;
        endpoint("scrobble")
            .respond_with(json(NOT_FOUND))
            .expect(1)
            .mount(&server)
            .await;

        let (db, dir) = temp_db("permanent", &server.uri(), true);
        let db2 = db.clone();
        blocking(move || report_play(&db2, &track_uri(1, "gone"), 100)).await;
        assert!(queue(&db).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn song_rating_stars_and_unstars_across_threshold() {
        let server = MockServer::start().await;
        endpoint("setRating")
            .and(query_param("id", "t1"))
            .and(query_param("rating", "5"))
            .respond_with(json(OK))
            .expect(1)
            .mount(&server)
            .await;
        endpoint("star")
            .and(query_param("id", "t1"))
            .respond_with(json(OK))
            .expect(1)
            .mount(&server)
            .await;

        let (db, dir) = temp_db("song_rating", &server.uri(), false);
        db.pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO subsonic_cache (server_id, remote_id) VALUES (1, 't1')",
                [],
            )
            .unwrap();
        let db2 = db.clone();
        blocking(move || push_song_rating(&db2, &track_uri(1, "t1"), 4.5)).await;
        assert_eq!(song_cache(&db, "t1"), (Some(5), true));

        server.verify().await;
        server.reset().await;
        endpoint("setRating")
            .and(query_param("rating", "2"))
            .respond_with(json(OK))
            .expect(1)
            .mount(&server)
            .await;
        endpoint("unstar")
            .and(query_param("id", "t1"))
            .respond_with(json(OK))
            .expect(1)
            .mount(&server)
            .await;
        let db2 = db.clone();
        blocking(move || push_song_rating(&db2, &track_uri(1, "t1"), 2.0)).await;
        assert_eq!(song_cache(&db, "t1"), (Some(2), false));

        // Clearing the rating clears it on the server; the song stays
        // unstarred, so there's no star/unstar call.
        server.verify().await;
        server.reset().await;
        endpoint("setRating")
            .and(query_param("rating", "0"))
            .respond_with(json(OK))
            .expect(1)
            .mount(&server)
            .await;
        let db2 = db.clone();
        blocking(move || push_song_rating(&db2, &track_uri(1, "t1"), -1.0)).await;
        assert_eq!(song_cache(&db, "t1"), (None, false));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn album_rating_pushes_only_for_a_unique_title() {
        let server = MockServer::start().await;
        endpoint("setRating")
            .and(query_param("id", "al1"))
            .and(query_param("rating", "4"))
            .respond_with(json(OK))
            .expect(1)
            .mount(&server)
            .await;
        endpoint("star")
            .and(query_param("albumId", "al1"))
            .respond_with(json(OK))
            .expect(1)
            .mount(&server)
            .await;

        let (db, dir) = temp_db("album_rating", &server.uri(), false);
        db.pool
            .get()
            .unwrap()
            .execute_batch(
                "INSERT INTO subsonic_album_cache (server_id, remote_album_id, album_key) VALUES
                    (1, 'al1', 'Unique'), (1, 'al2', 'Shared'), (1, 'al3', 'Shared');",
            )
            .unwrap();
        let db2 = db.clone();
        blocking(move || {
            push_album_rating(&db2, "Unique", 4.0);
            push_album_rating(&db2, "Shared", 5.0);
            push_album_rating(&db2, "Unknown", 5.0);
        })
        .await;
        // setRating + star for 'Unique' only.
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
        let cached: (Option<i64>, bool) = db
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT server_rating, server_starred FROM subsonic_album_cache WHERE remote_album_id = 'al1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(cached, (Some(4), true));
        let _ = std::fs::remove_dir_all(dir);
    }
}
