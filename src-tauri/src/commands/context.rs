//! `get_song_context` — the single command backing the Details pane's
//! "Context & Bio" tab. Combines MusicBrainz/CritiqueBrainz/Wikipedia data,
//! cached in the `context_enrichment`/`artist_context_enrichment` tables
//! (see `db.rs` migration 28) with a 30-day TTL. Each source degrades
//! independently on failure — see `context::ContextManager`'s doc comment.

use crate::context::{is_cache_fresh, ContextManager};
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

#[tauri::command]
pub async fn get_song_context(
    state: State<'_, AppState>,
    song_id: i64,
    force_refresh: Option<bool>,
) -> Result<SongContextEnrichment, String> {
    let force_refresh = force_refresh.unwrap_or(false);

    let (release_group_id, artist_id) = {
        let conn = state.db.pool.get().map_err(|e| e.to_string())?;
        if !context_enrichment_enabled(&conn) {
            return Ok(SongContextEnrichment::default());
        }
        let (rg, artist): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT musicbrainz_release_group_id, musicbrainz_artist_id FROM songs WHERE id = ?1",
                params![song_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((None, None));
        (rg, artist)
    };

    if release_group_id.is_none() && artist_id.is_none() {
        return Ok(SongContextEnrichment::default());
    }

    let now = now_unix();
    let mut result = SongContextEnrichment::default();
    let context_manager = ContextManager::new();

    if let Some(ref rg_id) = release_group_id {
        let cached = read_release_group_cache(&state, rg_id)?;
        let fresh = cached
            .as_ref()
            .map(|c| is_cache_fresh(c.fetched_at, now))
            .unwrap_or(false);

        if fresh && !force_refresh {
            apply_release_group_cache(&mut result, cached.unwrap());
        } else {
            let mb = context_manager
                .fetch_musicbrainz_release_group(rg_id)
                .await
                .ok();
            let cb = context_manager
                .fetch_critiquebrainz_reviews(rg_id)
                .await
                .ok();
            write_release_group_cache(&state, rg_id, &mb, &cb, now)?;
            if let Some(mb) = mb {
                result.mb_rating = mb.rating;
                result.mb_rating_votes = mb.rating_votes;
                result.mb_tags = mb.tags;
            }
            if let Some(cb) = cb {
                result.critiquebrainz_rating = cb.average_rating;
                result.critiquebrainz_review_count = Some(cb.review_count);
                result.critiquebrainz_review_links = cb.review_links;
            }
            result.fetched_at = Some(now);
        }
    }

    if let Some(ref artist_id) = artist_id {
        let cached = read_artist_cache(&state, artist_id)?;
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
            let bio = context_manager
                .fetch_wikipedia_bio_for_artist(artist_id)
                .await
                .ok()
                .flatten();
            write_artist_cache(&state, artist_id, &bio, now)?;
            if let Some(bio) = bio {
                result.wikipedia_extract = Some(bio.extract);
                result.wikipedia_page_url = bio.page_url;
                result.wikipedia_thumbnail_url = bio.thumbnail_url;
            }
            result.fetched_at = Some(now);
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

fn read_release_group_cache(
    state: &State<'_, AppState>,
    release_group_id: &str,
) -> Result<Option<ReleaseGroupCacheRow>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
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
        e => Err(e.to_string()),
    })
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

fn write_release_group_cache(
    state: &State<'_, AppState>,
    release_group_id: &str,
    mb: &Option<crate::context::MusicBrainzReleaseGroupData>,
    cb: &Option<crate::context::CritiqueBrainzData>,
    fetched_at: i64,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let mb_tags_json = serde_json::to_string(&mb.as_ref().map(|m| m.tags.clone()).unwrap_or_default())
        .unwrap_or_else(|_| "[]".to_string());
    let cb_links_json =
        serde_json::to_string(&cb.as_ref().map(|c| c.review_links.clone()).unwrap_or_default())
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
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

type ArtistCacheRow = (Option<String>, Option<String>, Option<String>, i64);

fn read_artist_cache(
    state: &State<'_, AppState>,
    artist_id: &str,
) -> Result<Option<ArtistCacheRow>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
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
        e => Err(e.to_string()),
    })
}

fn write_artist_cache(
    state: &State<'_, AppState>,
    artist_id: &str,
    bio: &Option<crate::context::WikipediaSummary>,
    fetched_at: i64,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
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
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
