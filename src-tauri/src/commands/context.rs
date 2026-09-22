//! `get_song_context` — the single command backing the Details pane's
//! "Context & Bio" tab. Combines MusicBrainz/CritiqueBrainz/Wikipedia data,
//! cached in the `context_enrichment`/`artist_context_enrichment` tables
//! (see `db.rs` migration 28) with a 30-day TTL. Each source degrades
//! independently on failure — see `context::ContextManager`'s doc comment.

use crate::collection::get_artist_profile_conn;
use crate::context::{
    is_cache_fresh, ContextManager, ARTIST_FLIGHT, RELEASE_GROUP_FLIGHT,
};
use crate::db::Database;
use crate::AppState;
use rusqlite::params;
use serde::Serialize;
use tauri::State;

#[derive(Serialize, Default, Clone, Debug)]
pub struct SongContextEnrichment {
    pub mb_rating: Option<f32>,
    pub mb_rating_votes: Option<u32>,
    pub mb_tags: Vec<String>,
    pub critiquebrainz_rating: Option<f32>,
    pub critiquebrainz_review_count: Option<u32>,
    pub critiquebrainz_review_links: Vec<String>,
    pub wikipedia_extract: Option<String>,
    pub wikipedia_page_url: Option<String>,
    pub wikipedia_thumbnail_url: Option<String>,
    pub fetched_at: Option<i64>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Reads `context_enrichment_enabled` from the generic `app_state` KV table
/// (same mechanism as `set_app_setting`/other toggles). Defaults to enabled
/// — absent means "never explicitly turned off".
fn context_enrichment_enabled(conn: &rusqlite::Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = 'context_enrichment_enabled'",
            [],
            |row| row.get(0),
        )
        .ok();
    stored.map(|v| v != "false").unwrap_or(true)
}

/// Resolves the MusicBrainz artist ID `get_song_context` uses to drive the
/// Wikipedia bio lookup: prefers a song's own tagged `musicbrainz_artist_id`
/// (falling back to `musicbrainz_album_artist_id`, first entry if
/// multi-valued), and only when the song has neither falls back to the
/// artist's own persisted `ArtistProfile.musicbrainz_artist_id` (#1123,
/// captured by "Retrieve Album/Artist Details" or a *different* song's tag)
/// — so the Wikipedia bio isn't limited to songs that happen to carry a
/// usable tagged MBID themselves, which is the common case for a library
/// tagged before Picard-level MBID tagging was consistently used.
fn resolve_song_context_artist_mbid(
    conn: &rusqlite::Connection,
    tagged_artist_mbid: Option<&str>,
    tagged_album_artist_mbid: Option<&str>,
    artist_name: Option<&str>,
    album_artist_name: Option<&str>,
) -> Option<String> {
    let first_value = |raw: &str| {
        raw.split(&[';', '/'][..])
            .next()
            .unwrap_or(raw)
            .trim()
            .to_string()
    };

    let resolved_from_tags = tagged_artist_mbid
        .filter(|a| !a.trim().is_empty())
        .or_else(|| tagged_album_artist_mbid.filter(|a| !a.trim().is_empty()))
        .map(first_value)
        .filter(|a| !a.is_empty());
    if resolved_from_tags.is_some() {
        return resolved_from_tags;
    }

    let effective_artist_name = album_artist_name
        .filter(|a| !a.trim().is_empty())
        .or_else(|| artist_name.filter(|a| !a.trim().is_empty()))
        .map(first_value)
        .filter(|a| !a.is_empty())?;
    get_artist_profile_conn(conn, &effective_artist_name)
        .ok()
        .and_then(|profile| profile.musicbrainz_artist_id)
}

#[tauri::command]
pub async fn get_song_context(
    state: State<'_, AppState>,
    song_id: i64,
    force_refresh: Option<bool>,
) -> Result<SongContextEnrichment, String> {
    let force_refresh = force_refresh.unwrap_or(false);

    let context_result = crate::db::run_blocking(&state.db, move |conn| {
        if !context_enrichment_enabled(conn) {
            return Ok(None);
        }
        let (rg, tagged_artist_mbid, tagged_album_artist_mbid, artist_name, album_artist_name): (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT musicbrainz_release_group_id, musicbrainz_artist_id, musicbrainz_album_artist_id, artist, album_artist FROM songs WHERE id = ?1",
                params![song_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap_or((None, None, None, None, None));
        let resolved_artist = resolve_song_context_artist_mbid(
            conn,
            tagged_artist_mbid.as_deref(),
            tagged_album_artist_mbid.as_deref(),
            artist_name.as_deref(),
            album_artist_name.as_deref(),
        );
        Ok(Some((rg, resolved_artist)))
    })
    .await
    .map_err(|e| e.to_string())?;

    let (release_group_id, artist_id) = match context_result {
        Some(v) => v,
        None => return Ok(SongContextEnrichment::default()),
    };

    if release_group_id.is_none() && artist_id.is_none() {
        return Ok(SongContextEnrichment::default());
    }

    let now = now_unix();
    let mut result = SongContextEnrichment::default();
    let context_manager = ContextManager::new();
    let db = state.db.clone();

    if let Some(ref rg_id) = release_group_id {
        let cached = read_release_group_cache(&db, rg_id).await?;
        let fresh = cached
            .as_ref()
            .map(|c| is_cache_fresh(c.fetched_at, now))
            .unwrap_or(false);

        if fresh && !force_refresh {
            apply_release_group_cache(&mut result, cached.unwrap());
        } else {
            let db_clone = db.clone();
            let rg_id_clone = rg_id.clone();
            let cm = context_manager.clone();
            let (mb_res, cb_res) = RELEASE_GROUP_FLIGHT
                .work(rg_id, move || async move {
                    let mb = cm
                        .fetch_musicbrainz_release_group(&rg_id_clone)
                        .await
                        .map_err(|e| e.to_string());
                    let cb = cm
                        .fetch_critiquebrainz_reviews(&rg_id_clone)
                        .await
                        .map_err(|e| e.to_string());

                    let mb_ok = mb.as_ref().ok().cloned();
                    let cb_ok = cb.as_ref().ok().cloned();

                    if mb.is_ok() || cb.is_ok() {
                        let _ = write_release_group_cache(
                            &db_clone,
                            &rg_id_clone,
                            &mb_ok,
                            &cb_ok,
                            now,
                        )
                        .await;
                    }
                    (mb, cb)
                })
                .await;

            if let Ok(ref mb) = mb_res {
                result.mb_rating = mb.rating;
                result.mb_rating_votes = mb.rating_votes;
                result.mb_tags = mb.tags.clone();
            } else if let Some(ref cached_row) = cached {
                result.mb_rating = cached_row.mb_rating;
                result.mb_rating_votes = cached_row.mb_rating_votes;
                result.mb_tags = cached_row
                    .mb_tags
                    .as_ref()
                    .and_then(|t| serde_json::from_str(t).ok())
                    .unwrap_or_default();
            }

            if let Ok(ref cb) = cb_res {
                result.critiquebrainz_rating = cb.average_rating;
                result.critiquebrainz_review_count = Some(cb.review_count);
                result.critiquebrainz_review_links = cb.review_links.clone();
            } else if let Some(ref cached_row) = cached {
                result.critiquebrainz_rating = cached_row.critiquebrainz_rating;
                result.critiquebrainz_review_count = cached_row.critiquebrainz_review_count;
                result.critiquebrainz_review_links = cached_row
                    .critiquebrainz_review_links
                    .as_ref()
                    .and_then(|l| serde_json::from_str(l).ok())
                    .unwrap_or_default();
            }

            if mb_res.is_ok() || cb_res.is_ok() {
                result.fetched_at = Some(now);
            } else if let Some(ref cached_row) = cached {
                result.fetched_at = Some(cached_row.fetched_at);
            }
        }
    }

    if let Some(ref artist_id) = artist_id {
        let cached = read_artist_cache(&db, artist_id).await?;
        let fresh = cached
            .as_ref()
            .map(|c| is_cache_fresh(c.3, now))
            .unwrap_or(false);

        if fresh && !force_refresh {
            if let Some((extract, page_url, thumbnail_url, _)) = cached {
                result.wikipedia_extract = extract;
                result.wikipedia_page_url = page_url;
                result.wikipedia_thumbnail_url = thumbnail_url;
            }
        } else {
            let db_clone = db.clone();
            let artist_id_clone = artist_id.clone();
            let bio_res = ARTIST_FLIGHT
                .work(artist_id, move || async move {
                    let res = context_manager
                        .fetch_wikipedia_bio_for_artist(&artist_id_clone)
                        .await
                        .map_err(|e| e.to_string());

                    match &res {
                        Ok(bio) => {
                            let _ = write_artist_cache(
                                &db_clone,
                                &artist_id_clone,
                                bio,
                                now,
                            )
                            .await;
                        }
                        Err(err) => {
                            log::warn!(
                                "Failed to fetch Wikipedia bio for artist {}: {}",
                                artist_id_clone,
                                err
                            );
                        }
                    }
                    res
                })
                .await;

            match bio_res {
                Ok(Some(bio)) => {
                    result.wikipedia_extract = Some(bio.extract);
                    result.wikipedia_page_url = bio.page_url;
                    result.wikipedia_thumbnail_url = bio.thumbnail_url;
                    result.fetched_at = Some(now);
                }
                Ok(None) => {
                    result.fetched_at = Some(now);
                }
                Err(_) => {
                    if let Some((extract, page_url, thumbnail_url, fetched_at)) = cached {
                        result.wikipedia_extract = extract;
                        result.wikipedia_page_url = page_url;
                        result.wikipedia_thumbnail_url = thumbnail_url;
                        result.fetched_at = Some(fetched_at);
                    }
                }
            }
        }
    }

    Ok(result)
}

struct ReleaseGroupCacheRow {
    mb_rating: Option<f32>,
    mb_rating_votes: Option<u32>,
    mb_tags: Option<String>,
    critiquebrainz_rating: Option<f32>,
    critiquebrainz_review_count: Option<u32>,
    critiquebrainz_review_links: Option<String>,
    fetched_at: i64,
}

async fn read_release_group_cache(
    db: &std::sync::Arc<Database>,
    release_group_id: &str,
) -> Result<Option<ReleaseGroupCacheRow>, String> {
    let release_group_id = release_group_id.to_string();
    crate::db::run_blocking(db, move |conn| {
        conn.query_row(
            "SELECT mb_rating, mb_rating_votes, mb_tags, critiquebrainz_rating, critiquebrainz_review_count, critiquebrainz_review_links, fetched_at
             FROM context_enrichment WHERE release_group_id = ?1",
            params![release_group_id],
            |row| {
                Ok(ReleaseGroupCacheRow {
                    mb_rating: row.get(0)?,
                    mb_rating_votes: row.get(1)?,
                    mb_tags: row.get(2)?,
                    critiquebrainz_rating: row.get(3)?,
                    critiquebrainz_review_count: row.get(4)?,
                    critiquebrainz_review_links: row.get(5)?,
                    fetched_at: row.get(6)?,
                })
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(e.into()),
        })
    })
    .await
    .map_err(|e| e.to_string())
}

fn apply_release_group_cache(result: &mut SongContextEnrichment, cached: ReleaseGroupCacheRow) {
    result.mb_rating = cached.mb_rating;
    result.mb_rating_votes = cached.mb_rating_votes;
    result.mb_tags = cached
        .mb_tags
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    result.critiquebrainz_rating = cached.critiquebrainz_rating;
    result.critiquebrainz_review_count = cached.critiquebrainz_review_count;
    result.critiquebrainz_review_links = cached
        .critiquebrainz_review_links
        .and_then(|l| serde_json::from_str(&l).ok())
        .unwrap_or_default();
    result.fetched_at = Some(cached.fetched_at);
}

async fn write_release_group_cache(
    db: &std::sync::Arc<Database>,
    release_group_id: &str,
    mb: &Option<crate::context::MusicBrainzReleaseGroupData>,
    cb: &Option<crate::context::CritiqueBrainzData>,
    fetched_at: i64,
) -> Result<(), String> {
    let release_group_id = release_group_id.to_string();
    let mb = mb.clone();
    let cb = cb.clone();
    crate::db::run_blocking(db, move |conn| {
        let mb_tags_json =
            serde_json::to_string(&mb.as_ref().map(|m| m.tags.clone()).unwrap_or_default())
                .unwrap_or_else(|_| "[]".to_string());
        let cb_links_json = serde_json::to_string(
            &cb.as_ref().map(|c| c.review_links.clone()).unwrap_or_default(),
        )
        .unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO context_enrichment
                (release_group_id, mb_rating, mb_rating_votes, mb_tags, critiquebrainz_rating, critiquebrainz_review_count, critiquebrainz_review_links, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(release_group_id) DO UPDATE SET
                mb_rating = excluded.mb_rating,
                mb_rating_votes = excluded.mb_rating_votes,
                mb_tags = excluded.mb_tags,
                critiquebrainz_rating = excluded.critiquebrainz_rating,
                critiquebrainz_review_count = excluded.critiquebrainz_review_count,
                critiquebrainz_review_links = excluded.critiquebrainz_review_links,
                fetched_at = excluded.fetched_at",
            params![
                release_group_id,
                mb.as_ref().and_then(|m| m.rating),
                mb.as_ref().and_then(|m| m.rating_votes),
                mb_tags_json,
                cb.as_ref().and_then(|c| c.average_rating),
                cb.as_ref().map(|c| c.review_count),
                cb_links_json,
                fetched_at,
            ],
        )?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())
}

type ArtistCacheRow = (Option<String>, Option<String>, Option<String>, i64);

async fn read_artist_cache(
    db: &std::sync::Arc<Database>,
    artist_id: &str,
) -> Result<Option<ArtistCacheRow>, String> {
    let artist_id = artist_id.to_string();
    crate::db::run_blocking(db, move |conn| {
        conn.query_row(
            "SELECT wikipedia_extract, wikipedia_page_url, wikipedia_thumbnail_url, fetched_at
             FROM artist_context_enrichment WHERE artist_id = ?1",
            params![artist_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                ))
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(e.into()),
        })
    })
    .await
    .map_err(|e| e.to_string())
}

async fn write_artist_cache(
    db: &std::sync::Arc<Database>,
    artist_id: &str,
    bio: &Option<crate::context::WikipediaSummary>,
    fetched_at: i64,
) -> Result<(), String> {
    let artist_id = artist_id.to_string();
    let bio = bio.clone();
    crate::db::run_blocking(db, move |conn| {
        conn.execute(
            "INSERT INTO artist_context_enrichment
                (artist_id, wikidata_id, wikipedia_extract, wikipedia_page_url, wikipedia_thumbnail_url, fetched_at)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5)
             ON CONFLICT(artist_id) DO UPDATE SET
                wikipedia_extract = excluded.wikipedia_extract,
                wikipedia_page_url = excluded.wikipedia_page_url,
                wikipedia_thumbnail_url = excluded.wikipedia_thumbnail_url,
                fetched_at = excluded.fetched_at",
            params![
                artist_id,
                bio.as_ref().map(|b| b.extract.clone()),
                bio.as_ref().and_then(|b| b.page_url.clone()),
                bio.as_ref().and_then(|b| b.thumbnail_url.clone()),
                fetched_at,
            ],
        )?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::ArtistProfile;

    fn temp_db(name: &str) -> Database {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_{}_{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        Database::new(temp_dir).unwrap()
    }

    #[test]
    fn test_resolve_song_context_artist_mbid_prefers_tagged_mbid() {
        let db = temp_db("context_resolve_tagged");
        let conn = db.pool.get().unwrap();
        assert_eq!(
            resolve_song_context_artist_mbid(
                &conn,
                Some("tagged-mbid"),
                None,
                Some("Some Artist"),
                None
            ),
            Some("tagged-mbid".to_string())
        );
    }

    #[test]
    fn test_resolve_song_context_artist_mbid_falls_back_to_artist_profile() {
        // Reproduces a reported gap (#1123): a song with no tagged MBID
        // still couldn't get a Wikipedia bio, even after "Retrieve Artist
        // Details" had persisted the artist's MBID onto their profile.
        let db = temp_db("context_resolve_profile_fallback");
        let conn = db.pool.get().unwrap();
        crate::collection::set_artist_profile_conn(
            &conn,
            &ArtistProfile {
                artist_key: "Shania Twain".to_string(),
                musicbrainz_artist_id: Some("profile-mbid".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            resolve_song_context_artist_mbid(&conn, None, None, Some("Shania Twain"), None),
            Some("profile-mbid".to_string())
        );
        // Album artist takes precedence over track artist, same convention
        // as elsewhere (effective_artist resolution).
        assert_eq!(
            resolve_song_context_artist_mbid(
                &conn,
                None,
                None,
                Some("Feature Artist"),
                Some("Shania Twain")
            ),
            Some("profile-mbid".to_string())
        );
    }

    #[test]
    fn test_resolve_song_context_artist_mbid_none_when_nothing_resolves() {
        let db = temp_db("context_resolve_none");
        let conn = db.pool.get().unwrap();
        assert_eq!(resolve_song_context_artist_mbid(&conn, None, None, None, None), None);
        // An artist with no saved profile at all yields None, not an error.
        assert_eq!(
            resolve_song_context_artist_mbid(&conn, None, None, Some("Nobody Known"), None),
            None
        );
    }
}
