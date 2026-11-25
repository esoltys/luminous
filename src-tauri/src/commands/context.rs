//! `get_song_context` — the single command backing the Details pane's
//! "Context & Bio" tab. Combines MusicBrainz/CritiqueBrainz/Wikipedia data,
//! cached in the `context_enrichment`/`artist_context_enrichment` tables
//! (see `db.rs` migration 28) with a 30-day TTL. Each source degrades
//! independently on failure — see `context::ContextManager`'s doc comment.

use crate::context::{
    is_cache_fresh, is_events_cache_fresh, ArtistEvent, ContextManager, StationContextLookupResult,
    ARTIST_EVENTS_FLIGHT, ARTIST_FLIGHT, RELEASE_GROUP_FLIGHT, STATION_CONTEXT_FLIGHT,
};
use crate::db::Database;
use crate::profiles::get_artist_profile;
use crate::AppState;
use rusqlite::params;
use serde::Serialize;
use tauri::State;

struct SongLookupMeta {
    online: bool,
    is_radio: bool,
    path: Option<String>,
    title: Option<String>,
    url: Option<String>,
    artist: Option<String>,
    release_group_id: Option<String>,
    artist_id: Option<String>,
}

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
    pub artist_sort_name: Option<String>,
    pub artist_type: Option<String>,
    pub artist_gender: Option<String>,
    pub artist_begin_date: Option<String>,
    pub artist_end_date: Option<String>,
    pub artist_ended: Option<bool>,
    pub artist_begin_area_name: Option<String>,
    pub artist_begin_area_mbid: Option<String>,
    pub artist_area_name: Option<String>,
    pub artist_area_mbid: Option<String>,
    pub station_country: Option<String>,
    pub station_language: Option<String>,
    pub station_homepage: Option<String>,
    pub station_social_links: Vec<crate::models::ArtistSocialLink>,
    pub station_wikidata_id: Option<String>,
    pub fetched_at: Option<i64>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Key of the Online/Offline master toggle (#1398) in the `app_state` KV table.
/// The name predates the toggle: it began as the context-enrichment switch.
pub const ONLINE_ENABLED_KEY: &str = "context_enrichment_enabled";

/// Error returned by network entry points while the master toggle is Offline.
pub const OFFLINE_ERROR: &str = "Luminous is offline: turn Online on in Settings › Integrations";

/// Master Online/Offline toggle. When `false`, Luminous must make no requests
/// to third-party internet services (art, lyrics, bios, ListenBrainz,
/// MusicBrainz, update checks). Subsonic/WebDAV libraries are the user's own
/// servers and are exempt. Defaults to enabled: absent means "never turned off".
pub fn is_online_enabled(conn: &rusqlite::Connection) -> bool {
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = 'context_enrichment_enabled'",
            [],
            |row| row.get(0),
        )
        .ok();
    stored.map(|v| v != "false").unwrap_or(true)
}

/// Persist the master toggle, apply its side effects (Discord, ListenBrainz)
/// and tell every window via `online-mode-changed`.
#[tauri::command]
pub async fn set_online_enabled(
    enabled: bool,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    crate::db::run_blocking(&state.db, move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
            rusqlite::params![ONLINE_ENABLED_KEY, if enabled { "true" } else { "false" }],
        )?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?;
    state.scrobbler.set_online(enabled).await;
    let _ = tauri::Emitter::emit(&app, "online-mode-changed", enabled);
    Ok(())
}

#[tauri::command]
pub async fn is_context_enrichment_enabled(state: State<'_, AppState>) -> Result<bool, String> {
    crate::db::run_blocking(&state.db, |conn| Ok(is_online_enabled(conn)))
        .await
        .map_err(|e| e.to_string())
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
    get_artist_profile(conn, &effective_artist_name)
        .ok()
        .and_then(|profile| profile.musicbrainz_artist_id)
}

#[tauri::command]
pub async fn get_song_context(
    state: State<'_, AppState>,
    song_id: i64,
    force_refresh: Option<bool>,
    locale: Option<String>,
    app: tauri::AppHandle,
) -> Result<SongContextEnrichment, String> {
    let force_refresh = force_refresh.unwrap_or(false);
    let wiki_lang = crate::context::wikipedia_language(locale.as_deref());

    let song_meta = crate::db::run_blocking(&state.db, move |conn| {
        let online = is_online_enabled(conn);
        let row = conn
            .query_row(
                "SELECT source, path, title, url, artist,
                        musicbrainz_release_group_id, musicbrainz_artist_id,
                        musicbrainz_album_artist_id, album_artist
                 FROM songs WHERE id = ?1",
                params![song_id],
                |row| {
                    Ok((
                        row.get::<_, i32>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .ok();
        let Some((
            source,
            path,
            title,
            url,
            artist_name,
            rg,
            tagged_artist_mbid,
            tagged_album_artist_mbid,
            album_artist_name,
        )) = row
        else {
            return Ok(None);
        };

        let is_radio = source == crate::models::SongSource::RADIO_BROWSER_ID
            || source == crate::models::SongSource::STREAM_ID;
        if is_radio {
            return Ok(Some(SongLookupMeta {
                online,
                is_radio: true,
                path,
                title,
                url,
                artist: artist_name,
                release_group_id: None,
                artist_id: None,
            }));
        }

        if !online {
            return Ok(None);
        }
        let resolved_artist = resolve_song_context_artist_mbid(
            conn,
            tagged_artist_mbid.as_deref(),
            tagged_album_artist_mbid.as_deref(),
            artist_name.as_deref(),
            album_artist_name.as_deref(),
        );
        Ok(Some(SongLookupMeta {
            online,
            is_radio: false,
            path,
            title,
            url,
            artist: artist_name,
            release_group_id: rg,
            artist_id: resolved_artist,
        }))
    })
    .await
    .map_err(|e| e.to_string())?;

    let Some(meta) = song_meta else {
        return Ok(SongContextEnrichment::default());
    };

    if meta.is_radio {
        return get_radio_station_context(&state.db, force_refresh, &wiki_lang, &meta).await;
    }

    let (release_group_id, artist_id) = (meta.release_group_id, meta.artist_id);

    if release_group_id.is_none() && artist_id.is_none() {
        return Ok(SongContextEnrichment::default());
    }

    let now = now_unix();
    let mut result = SongContextEnrichment::default();
    let context_manager = ContextManager::new();
    let db = state.db.clone();

    if let Some(ref rg_id) = release_group_id {
        let cached = read_release_group_cache(&db, rg_id).await?;
        // Review links are ordered by language, so a row cached for another
        // language is stale (#1480); pre-migration-61 rows were English.
        let fresh = cached
            .as_ref()
            .map(|c| {
                is_cache_fresh(c.fetched_at, now)
                    && c.critiquebrainz_lang.as_deref().unwrap_or("en") == wiki_lang
            })
            .unwrap_or(false);

        if fresh && !force_refresh {
            apply_release_group_cache(&mut result, cached.unwrap());
        } else {
            let db_clone = db.clone();
            let rg_id_clone = rg_id.clone();
            let cm = context_manager.clone();
            let lang = wiki_lang.clone();
            let flight_key = format!("{rg_id}:{wiki_lang}");
            let (mb_res, cb_res) = RELEASE_GROUP_FLIGHT
                .work(&flight_key, move || async move {
                    let mb = cm
                        .fetch_musicbrainz_release_group(&rg_id_clone)
                        .await
                        .map_err(|e| e.to_string());
                    let cb = cm
                        .fetch_critiquebrainz_reviews(&rg_id_clone, &lang)
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
                            cb.is_ok().then_some(lang.as_str()),
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
        // If sort_name is missing, the row was cached before migration 42
        // introduced structured MusicBrainz artist details (#1128, #1146);
        // treat it as stale so details are fetched.
        // A bio cached for another language is stale too (#1480); rows from
        // before migration 61 have no language and were fetched in English.
        let fresh = cached
            .as_ref()
            .map(|c| {
                is_cache_fresh(c.fetched_at, now)
                    && c.sort_name.is_some()
                    // A row cached before migration 85 has no country yet (#2095).
                    && c.country.is_some()
                    && c.wikipedia_lang.as_deref().unwrap_or("en") == wiki_lang
            })
            .unwrap_or(false);

        // What MusicBrainz says this artist's country is, from whichever path
        // below supplies the details; it fills a blank profile country.
        let mut artist_country: Option<String> = None;

        if fresh && !force_refresh {
            if let Some(ref row) = cached {
                apply_artist_cache(&mut result, row);
                artist_country = row.country.clone();
            }
        } else {
            let db_clone = db.clone();
            let artist_id_clone = artist_id.clone();
            let cm = context_manager.clone();
            let lang = wiki_lang.clone();
            let flight_key = format!("{artist_id}:{wiki_lang}");
            let (details_res, bio_res) = ARTIST_FLIGHT
                .work(&flight_key, move || async move {
                    // Fetch MusicBrainz details + wikidata_id in one request (#1128)
                    let mb_res = cm
                        .fetch_musicbrainz_artist_details_and_wikidata_id(&artist_id_clone)
                        .await;

                    let (details, wikidata_id) = match mb_res {
                        Ok((d, qid)) => (Ok(d), qid),
                        Err(e) => (Err(e.to_string()), None),
                    };

                    let bio = if let Some(ref qid) = wikidata_id {
                        cm.fetch_wikipedia_bio_from_wikidata_id(qid, &lang)
                            .await
                            .map_err(|e| e.to_string())
                    } else {
                        cm.fetch_wikipedia_bio_for_artist(&artist_id_clone, &lang)
                            .await
                            .map_err(|e| e.to_string())
                    };

                    let bio_ok = bio.as_ref().ok().cloned().flatten();
                    let details_ok = details.as_ref().ok().cloned();

                    if bio.is_ok() || details.is_ok() {
                        let _ = write_artist_cache(
                            &db_clone,
                            &artist_id_clone,
                            &bio_ok,
                            &details_ok,
                            // Stamp the language only when the bio lookup
                            // actually answered, so a failed fetch stays stale.
                            bio.is_ok().then_some(lang.as_str()),
                            now,
                        )
                        .await;
                    }
                    (details, bio)
                })
                .await;

            if let Ok(ref details) = details_res {
                result.artist_sort_name = details.sort_name.clone();
                result.artist_type = details.artist_type.clone();
                result.artist_gender = details.gender.clone();
                result.artist_begin_date = details.begin_date.clone();
                result.artist_end_date = details.end_date.clone();
                result.artist_ended = details.ended;
                result.artist_begin_area_name = details.begin_area_name.clone();
                result.artist_begin_area_mbid = details.begin_area_mbid.clone();
                result.artist_area_name = details.area_name.clone();
                result.artist_area_mbid = details.area_mbid.clone();
                artist_country = details.country.clone();
                result.fetched_at = Some(now);
            } else if let Some(ref c) = cached {
                artist_country = c.country.clone();
                result.artist_sort_name = c.sort_name.clone();
                result.artist_type = c.artist_type.clone();
                result.artist_gender = c.gender.clone();
                result.artist_begin_date = c.begin_date.clone();
                result.artist_end_date = c.end_date.clone();
                result.artist_ended = c.ended;
                result.artist_begin_area_name = c.begin_area_name.clone();
                result.artist_begin_area_mbid = c.begin_area_mbid.clone();
                result.artist_area_name = c.area_name.clone();
                result.artist_area_mbid = c.area_mbid.clone();
            }

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
                    if let Some(ref c) = cached {
                        result.wikipedia_extract = c.wikipedia_extract.clone();
                        result.wikipedia_page_url = c.wikipedia_page_url.clone();
                        result.wikipedia_thumbnail_url = c.wikipedia_thumbnail_url.clone();
                        result.fetched_at = Some(c.fetched_at);
                    }
                }
            }
        }

        backfill_artist_country(&app, artist_id, artist_country.as_deref()).await;
    }

    Ok(result)
}

const STATION_NEGATIVE_CACHE_TTL_SECS: i64 = 7 * 86_400;

struct StationContextCacheRow {
    wikidata_id: Option<String>,
    wikipedia_extract: Option<String>,
    wikipedia_page_url: Option<String>,
    wikipedia_lang: Option<String>,
    social_links_json: Option<String>,
    fetched_at: i64,
}

async fn get_radio_station_context(
    db: &std::sync::Arc<Database>,
    force_refresh: bool,
    wiki_lang: &str,
    meta: &SongLookupMeta,
) -> Result<SongContextEnrichment, String> {
    let online = meta.online;
    let stationuuid = meta
        .path
        .as_deref()
        .and_then(|p| p.strip_prefix(crate::media_source::RADIO_URI_SCHEME))
        .filter(|u| !u.is_empty())
        .map(str::to_string);

    let fallback_title = meta.title.clone();
    let fallback_url = meta.url.clone();
    let fallback_country = meta.artist.clone();
    let uuid_for_db = stationuuid.clone();

    let (dir_meta, cached) = crate::db::run_blocking(db, move |conn| {
        let Some(ref uuid) = uuid_for_db else {
            return Ok((None, None));
        };
        let dir_meta: Option<(String, String, String, String)> = conn
            .query_row(
                "SELECT name, country, language, homepage FROM radio_directory WHERE stationuuid = ?1",
                params![uuid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .ok();
        let cached = conn
            .query_row(
                "SELECT wikidata_id, wikipedia_extract, wikipedia_page_url,
                        wikipedia_lang, social_links_json, fetched_at
                 FROM radio_station_context WHERE stationuuid = ?1",
                params![uuid],
                |r| {
                    Ok(StationContextCacheRow {
                        wikidata_id: r.get(0)?,
                        wikipedia_extract: r.get(1)?,
                        wikipedia_page_url: r.get(2)?,
                        wikipedia_lang: r.get(3)?,
                        social_links_json: r.get(4)?,
                        fetched_at: r.get(5)?,
                    })
                },
            )
            .ok();
        Ok((dir_meta, cached))
    })
    .await
    .map_err(|e| e.to_string())?;

    let mut result = SongContextEnrichment::default();
    let (station_name, dir_country, dir_language, dir_homepage) = match dir_meta {
        Some((n, c, l, h)) => (Some(n), Some(c), Some(l), Some(h)),
        None => (None, None, None, None),
    };

    let non_empty = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    result.station_country = non_empty(dir_country).or_else(|| non_empty(fallback_country));
    result.station_language = non_empty(dir_language);
    result.station_homepage = non_empty(dir_homepage).or_else(|| non_empty(fallback_url));

    let now = now_unix();
    let cache_is_usable = cached.as_ref().is_some_and(|c| {
        if !online {
            return true;
        }
        if force_refresh {
            return false;
        }
        let lang_matches = c.wikipedia_lang.as_deref().unwrap_or("en") == wiki_lang;
        if !lang_matches {
            return false;
        }
        let has_content = c.wikipedia_extract.is_some()
            || c.social_links_json
                .as_deref()
                .is_some_and(|j| j != "[]" && !j.is_empty());
        if has_content {
            is_cache_fresh(c.fetched_at, now)
        } else {
            now.saturating_sub(c.fetched_at) < STATION_NEGATIVE_CACHE_TTL_SECS
        }
    });

    if cache_is_usable {
        if let Some(c) = cached {
            apply_station_context_cache(&mut result, c);
        }
        return Ok(result);
    }

    let lookup_name = non_empty(station_name).or_else(|| non_empty(fallback_title));
    if !online {
        return Ok(result);
    }
    let (Some(uuid), Some(name)) = (stationuuid, lookup_name) else {
        return Ok(result);
    };

    let cm = ContextManager::new();
    let homepage_arg = result.station_homepage.clone();
    let lang = wiki_lang.to_string();
    let flight_key = format!("{uuid}:{lang}");
    let lookup_res = STATION_CONTEXT_FLIGHT
        .work(&flight_key, move || async move {
            cm.fetch_station_context(&name, homepage_arg.as_deref(), &lang)
                .await
                .map_err(|e| e.to_string())
        })
        .await;

    if let Ok(lookup) = lookup_res {
        let _ = write_station_context_cache(db, &uuid, wiki_lang, &lookup, now).await;
        apply_station_lookup(&mut result, lookup, now);
    }

    Ok(result)
}

fn apply_station_context_cache(result: &mut SongContextEnrichment, cached: StationContextCacheRow) {
    result.station_wikidata_id = cached.wikidata_id;
    result.wikipedia_extract = cached.wikipedia_extract;
    result.wikipedia_page_url = cached.wikipedia_page_url;
    if let Some(json) = cached.social_links_json {
        if let Ok(links) = serde_json::from_str::<Vec<crate::models::ArtistSocialLink>>(&json) {
            result.station_social_links = links;
        }
    }
    result.fetched_at = Some(cached.fetched_at);
}

fn apply_station_lookup(
    result: &mut SongContextEnrichment,
    lookup: StationContextLookupResult,
    fetched_at: i64,
) {
    result.station_wikidata_id = lookup.wikidata_id;
    if let Some(wiki) = lookup.wikipedia {
        result.wikipedia_extract = Some(wiki.extract);
        result.wikipedia_page_url = wiki.page_url;
    }
    result.station_social_links = lookup.social_links;
    result.fetched_at = Some(fetched_at);
}

async fn write_station_context_cache(
    db: &std::sync::Arc<Database>,
    stationuuid: &str,
    wiki_lang: &str,
    lookup: &StationContextLookupResult,
    now: i64,
) -> Result<(), String> {
    let stationuuid = stationuuid.to_string();
    let wiki_lang = wiki_lang.to_string();
    let wikidata_id = lookup.wikidata_id.clone();
    let wikipedia_extract = lookup.wikipedia.as_ref().map(|w| w.extract.clone());
    let wikipedia_page_url = lookup.wikipedia.as_ref().and_then(|w| w.page_url.clone());
    let social_links_json =
        serde_json::to_string(&lookup.social_links).unwrap_or_else(|_| "[]".to_string());
    crate::db::run_blocking(db, move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO radio_station_context (
                stationuuid, wikidata_id, wikipedia_extract, wikipedia_page_url,
                wikipedia_lang, social_links_json, fetched_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                stationuuid,
                wikidata_id,
                wikipedia_extract,
                wikipedia_page_url,
                wiki_lang,
                social_links_json,
                now,
            ],
        )?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())
}

struct ReleaseGroupCacheRow {
    mb_rating: Option<f32>,
    mb_rating_votes: Option<u32>,
    mb_tags: Option<String>,
    critiquebrainz_rating: Option<f32>,
    critiquebrainz_review_count: Option<u32>,
    critiquebrainz_review_links: Option<String>,
    /// Language the review links were ordered for; `None` for pre-migration-61 rows.
    critiquebrainz_lang: Option<String>,
    fetched_at: i64,
}

async fn read_release_group_cache(
    db: &std::sync::Arc<Database>,
    release_group_id: &str,
) -> Result<Option<ReleaseGroupCacheRow>, String> {
    let release_group_id = release_group_id.to_string();
    crate::db::run_blocking(db, move |conn| {
        conn.query_row(
            "SELECT mb_rating, mb_rating_votes, mb_tags, critiquebrainz_rating, critiquebrainz_review_count, critiquebrainz_review_links, fetched_at, critiquebrainz_lang
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
                    critiquebrainz_lang: row.get(7)?,
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
    critiquebrainz_lang: Option<&str>,
    fetched_at: i64,
) -> Result<(), String> {
    let release_group_id = release_group_id.to_string();
    let critiquebrainz_lang = critiquebrainz_lang.map(str::to_string);
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
                (release_group_id, mb_rating, mb_rating_votes, mb_tags, critiquebrainz_rating, critiquebrainz_review_count, critiquebrainz_review_links, fetched_at, critiquebrainz_lang)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(release_group_id) DO UPDATE SET
                mb_rating = excluded.mb_rating,
                mb_rating_votes = excluded.mb_rating_votes,
                mb_tags = excluded.mb_tags,
                critiquebrainz_rating = excluded.critiquebrainz_rating,
                critiquebrainz_review_count = excluded.critiquebrainz_review_count,
                critiquebrainz_review_links = excluded.critiquebrainz_review_links,
                critiquebrainz_lang = CASE WHEN excluded.critiquebrainz_lang IS NOT NULL THEN excluded.critiquebrainz_lang ELSE context_enrichment.critiquebrainz_lang END,
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
                critiquebrainz_lang,
            ],
        )?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(Default, Debug, Clone)]
struct ArtistCacheRow {
    wikipedia_extract: Option<String>,
    wikipedia_page_url: Option<String>,
    wikipedia_thumbnail_url: Option<String>,
    sort_name: Option<String>,
    artist_type: Option<String>,
    gender: Option<String>,
    begin_date: Option<String>,
    end_date: Option<String>,
    ended: Option<bool>,
    begin_area_name: Option<String>,
    begin_area_mbid: Option<String>,
    area_name: Option<String>,
    area_mbid: Option<String>,
    /// Alpha-2 country MusicBrainz gave the artist; `""` when it has none, and
    /// `None` for rows cached before migration 85 (stale once, so it is fetched).
    country: Option<String>,
    /// Language the bio was requested in; `None` for rows from before migration 61.
    wikipedia_lang: Option<String>,
    fetched_at: i64,
}

fn apply_artist_cache(result: &mut SongContextEnrichment, cached: &ArtistCacheRow) {
    result.wikipedia_extract = cached.wikipedia_extract.clone();
    result.wikipedia_page_url = cached.wikipedia_page_url.clone();
    result.wikipedia_thumbnail_url = cached.wikipedia_thumbnail_url.clone();
    result.artist_sort_name = cached.sort_name.clone();
    result.artist_type = cached.artist_type.clone();
    result.artist_gender = cached.gender.clone();
    result.artist_begin_date = cached.begin_date.clone();
    result.artist_end_date = cached.end_date.clone();
    result.artist_ended = cached.ended;
    result.artist_begin_area_name = cached.begin_area_name.clone();
    result.artist_begin_area_mbid = cached.begin_area_mbid.clone();
    result.artist_area_name = cached.area_name.clone();
    result.artist_area_mbid = cached.area_mbid.clone();
    result.fetched_at = Some(cached.fetched_at);
}

async fn read_artist_cache(
    db: &std::sync::Arc<Database>,
    artist_id: &str,
) -> Result<Option<ArtistCacheRow>, String> {
    let artist_id = artist_id.to_string();
    crate::db::run_blocking(db, move |conn| {
        conn.query_row(
            "SELECT wikipedia_extract, wikipedia_page_url, wikipedia_thumbnail_url,
                    sort_name, artist_type, gender, begin_date, end_date, ended,
                    begin_area_name, begin_area_mbid, area_name, area_mbid, fetched_at, wikipedia_lang,
                    country
             FROM artist_context_enrichment WHERE artist_id = ?1",
            params![artist_id],
            |row| {
                let ended_int: Option<i64> = row.get(8)?;
                Ok(ArtistCacheRow {
                    wikipedia_extract: row.get(0)?,
                    wikipedia_page_url: row.get(1)?,
                    wikipedia_thumbnail_url: row.get(2)?,
                    sort_name: row.get(3)?,
                    artist_type: row.get(4)?,
                    gender: row.get(5)?,
                    begin_date: row.get(6)?,
                    end_date: row.get(7)?,
                    ended: ended_int.map(|v| v != 0),
                    begin_area_name: row.get(9)?,
                    begin_area_mbid: row.get(10)?,
                    area_name: row.get(11)?,
                    area_mbid: row.get(12)?,
                    fetched_at: row.get(13)?,
                    wikipedia_lang: row.get(14)?,
                    country: row.get(15)?,
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

async fn write_artist_cache(
    db: &std::sync::Arc<Database>,
    artist_id: &str,
    bio: &Option<crate::context::WikipediaSummary>,
    details: &Option<crate::context::MusicBrainzArtistDetails>,
    wikipedia_lang: Option<&str>,
    fetched_at: i64,
) -> Result<(), String> {
    let artist_id = artist_id.to_string();
    let bio = bio.clone();
    let details = details.clone();
    let wikipedia_lang = wikipedia_lang.map(str::to_string);
    crate::db::run_blocking(db, move |conn| {
        conn.execute(
            "INSERT INTO artist_context_enrichment
                (artist_id, wikidata_id, wikipedia_extract, wikipedia_page_url, wikipedia_thumbnail_url,
                 sort_name, artist_type, gender, begin_date, end_date, ended,
                 begin_area_name, begin_area_mbid, area_name, area_mbid, fetched_at, wikipedia_lang, country)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
             ON CONFLICT(artist_id) DO UPDATE SET
                wikipedia_extract = CASE WHEN excluded.wikipedia_extract IS NOT NULL THEN excluded.wikipedia_extract ELSE artist_context_enrichment.wikipedia_extract END,
                wikipedia_page_url = CASE WHEN excluded.wikipedia_page_url IS NOT NULL THEN excluded.wikipedia_page_url ELSE artist_context_enrichment.wikipedia_page_url END,
                wikipedia_thumbnail_url = CASE WHEN excluded.wikipedia_thumbnail_url IS NOT NULL THEN excluded.wikipedia_thumbnail_url ELSE artist_context_enrichment.wikipedia_thumbnail_url END,
                sort_name = CASE WHEN excluded.sort_name IS NOT NULL THEN excluded.sort_name ELSE artist_context_enrichment.sort_name END,
                artist_type = CASE WHEN excluded.artist_type IS NOT NULL THEN excluded.artist_type ELSE artist_context_enrichment.artist_type END,
                gender = CASE WHEN excluded.gender IS NOT NULL THEN excluded.gender ELSE artist_context_enrichment.gender END,
                begin_date = CASE WHEN excluded.begin_date IS NOT NULL THEN excluded.begin_date ELSE artist_context_enrichment.begin_date END,
                end_date = CASE WHEN excluded.end_date IS NOT NULL THEN excluded.end_date ELSE artist_context_enrichment.end_date END,
                ended = CASE WHEN excluded.ended IS NOT NULL THEN excluded.ended ELSE artist_context_enrichment.ended END,
                begin_area_name = CASE WHEN excluded.begin_area_name IS NOT NULL THEN excluded.begin_area_name ELSE artist_context_enrichment.begin_area_name END,
                begin_area_mbid = CASE WHEN excluded.begin_area_mbid IS NOT NULL THEN excluded.begin_area_mbid ELSE artist_context_enrichment.begin_area_mbid END,
                area_name = CASE WHEN excluded.area_name IS NOT NULL THEN excluded.area_name ELSE artist_context_enrichment.area_name END,
                area_mbid = CASE WHEN excluded.area_mbid IS NOT NULL THEN excluded.area_mbid ELSE artist_context_enrichment.area_mbid END,
                wikipedia_lang = CASE WHEN excluded.wikipedia_lang IS NOT NULL THEN excluded.wikipedia_lang ELSE artist_context_enrichment.wikipedia_lang END,
                country = CASE WHEN excluded.country IS NOT NULL THEN excluded.country ELSE artist_context_enrichment.country END,
                fetched_at = excluded.fetched_at",
            params![
                artist_id,
                bio.as_ref().map(|b| b.extract.clone()),
                bio.as_ref().and_then(|b| b.page_url.clone()),
                bio.as_ref().and_then(|b| b.thumbnail_url.clone()),
                details.as_ref().and_then(|d| d.sort_name.clone()),
                details.as_ref().and_then(|d| d.artist_type.clone()),
                details.as_ref().and_then(|d| d.gender.clone()),
                details.as_ref().and_then(|d| d.begin_date.clone()),
                details.as_ref().and_then(|d| d.end_date.clone()),
                details.as_ref().and_then(|d| d.ended.map(|b| if b { 1 } else { 0 })),
                details.as_ref().and_then(|d| d.begin_area_name.clone()),
                details.as_ref().and_then(|d| d.begin_area_mbid.clone()),
                details.as_ref().and_then(|d| d.area_name.clone()),
                details.as_ref().and_then(|d| d.area_mbid.clone()),
                fetched_at,
                wikipedia_lang,
                // `""` records "MusicBrainz was asked and has no country".
                details.as_ref().map(|d| d.country.clone().unwrap_or_default()),
            ],
        )?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())
}

/// Fills a blank profile country from what MusicBrainz said for `artist_id`, and
/// tells the rest of the app only when a profile actually changed: smart
/// playlists are re-checked (a country rule can now match) and the frontend
/// refreshes that profile's cached copy.
async fn backfill_artist_country(app: &tauri::AppHandle, artist_id: &str, country: Option<&str>) {
    use tauri::{Emitter, Manager};

    let Some(country) = country.filter(|c| !c.is_empty()) else {
        return;
    };
    let state = app.state::<AppState>();
    let (id, code) = (artist_id.to_string(), country.to_string());
    let changed = crate::db::run_blocking(&state.db, move |conn| {
        crate::profiles::fill_blank_artist_country_by_mbid(conn, &id, &code)
    })
    .await;
    match changed {
        Ok(keys) if !keys.is_empty() => {
            for key in keys {
                let _ = app.emit("artist-profile-changed", key);
            }
            crate::playlist::reconcile_and_sync(app.clone()).await;
        }
        Ok(_) => {}
        Err(e) => log::warn!("Filling the artist country failed: {e}"),
    }
}

/// Fetches upcoming and recent concerts, tour dates, and festival appearances
/// for an artist from MusicBrainz (#1431).
///
/// Looks up the artist's MBID (from `ArtistProfile`, a specific song if provided,
/// or any song by that artist in the database), checks the local SQLite cache
/// (`artist_events_cache`, 7-day TTL), and falls back to MusicBrainz's event browse API.
#[tauri::command]
pub async fn get_artist_events(
    artist: String,
    song_id: Option<i64>,
    force_refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<ArtistEvent>, String> {
    let force_refresh = force_refresh.unwrap_or(false);

    let (artist_mbid, cached_events): (Option<String>, Option<Vec<ArtistEvent>>) = {
        let artist_name = artist.clone();
        crate::db::run_blocking(&state.db, move |conn| {
            if !is_online_enabled(conn) {
                return Ok((None, None));
            }

            // 1. Resolve MBID
            let mut resolved_mbid: Option<String> = None;

            // Check artist profile first
            if let Ok(profile) = get_artist_profile(conn, &artist_name) {
                if let Some(mbid) = profile.musicbrainz_artist_id {
                    if !mbid.trim().is_empty() {
                        resolved_mbid = Some(mbid.trim().to_string());
                    }
                }
            }

            // If not found and a song_id was given, check that song
            if resolved_mbid.is_none() {
                if let Some(sid) = song_id {
                    let row: Option<(Option<String>, Option<String>)> = conn
                        .query_row(
                            "SELECT musicbrainz_artist_id, musicbrainz_album_artist_id FROM songs WHERE id = ?1",
                            params![sid],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .ok();
                    if let Some((aid, aaid)) = row {
                        resolved_mbid = aid
                            .filter(|s| !s.trim().is_empty())
                            .or_else(|| aaid.filter(|s| !s.trim().is_empty()))
                            .map(|s| s.split(&[';', '/'][..]).next().unwrap_or(&s).trim().to_string());
                    }
                }
            }

            // If still not found, check any song tagged by this artist
            if resolved_mbid.is_none() {
                let row: Option<(Option<String>, Option<String>)> = conn
                    .query_row(
                        "SELECT musicbrainz_artist_id, musicbrainz_album_artist_id FROM songs \
                         WHERE (artist = ?1 COLLATE NOCASE OR album_artist = ?1 COLLATE NOCASE) \
                           AND (musicbrainz_artist_id IS NOT NULL OR musicbrainz_album_artist_id IS NOT NULL) \
                         LIMIT 1",
                        params![artist_name],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .ok();
                if let Some((aid, aaid)) = row {
                    resolved_mbid = aid
                        .filter(|s| !s.trim().is_empty())
                        .or_else(|| aaid.filter(|s| !s.trim().is_empty()))
                        .map(|s| s.split(&[';', '/'][..]).next().unwrap_or(&s).trim().to_string());
                }
            }

            let Some(mbid) = resolved_mbid else {
                return Ok((None, None));
            };

            // Check cache
            let now = now_unix();
            let cache_row: Option<(String, i64)> = conn
                .query_row(
                    "SELECT events_json, fetched_at FROM artist_events_cache WHERE artist_mbid = ?1",
                    params![mbid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .ok();

            if let Some((json, fetched_at)) = cache_row {
                if !force_refresh && is_events_cache_fresh(fetched_at, now) {
                    let parsed: Vec<ArtistEvent> = serde_json::from_str(&json).unwrap_or_default();
                    return Ok((Some(mbid), Some(parsed)));
                }
            }

            Ok((Some(mbid), None))
        })
        .await
        .map_err(|e| e.to_string())?
    };

    if let Some(events) = cached_events {
        return Ok(events);
    }

    let Some(artist_mbid) = artist_mbid else {
        return Ok(Vec::new());
    };

    // SingleFlight fetch from MusicBrainz
    let mbid_clone = artist_mbid.clone();
    let fetch_result = ARTIST_EVENTS_FLIGHT
        .work(&artist_mbid, move || async move {
            ContextManager::new()
                .fetch_musicbrainz_artist_events(&mbid_clone)
                .await
                .map_err(|e| e.to_string())
        })
        .await;

    match fetch_result {
        Ok(events) => {
            // Write to cache
            let mbid_for_cache = artist_mbid.clone();
            let json_to_cache = serde_json::to_string(&events).unwrap_or_else(|_| "[]".to_string());
            let fetched_at = now_unix();
            let _ = crate::db::run_blocking(&state.db, move |conn| {
                conn.execute(
                    "INSERT INTO artist_events_cache (artist_mbid, events_json, fetched_at) \
                     VALUES (?1, ?2, ?3) \
                     ON CONFLICT(artist_mbid) DO UPDATE SET \
                         events_json = excluded.events_json, \
                         fetched_at = excluded.fetched_at",
                    params![mbid_for_cache, json_to_cache, fetched_at],
                )?;
                Ok(())
            })
            .await;
            Ok(events)
        }
        Err(err) => {
            // If network fails, try to return stale cache as fallback rather than erroring
            let fallback: Option<Vec<ArtistEvent>> = {
                let mbid = artist_mbid.clone();
                crate::db::run_blocking(&state.db, move |conn| {
                    let json: Option<String> = conn
                        .query_row(
                            "SELECT events_json FROM artist_events_cache WHERE artist_mbid = ?1",
                            params![mbid],
                            |r| r.get(0),
                        )
                        .ok();
                    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
                })
                .await
                .ok()
                .flatten()
            };
            if let Some(cached) = fallback {
                Ok(cached)
            } else {
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::ArtistProfile;
    use std::sync::Arc;

    fn temp_db(name: &str) -> (tempfile::TempDir, Database) {
        let temp_dir = tempfile::Builder::new()
            .prefix(&format!("luminous_{name}_"))
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        (temp_dir, db)
    }

    #[test]
    fn test_is_online_enabled_defaults_on_and_follows_stored_value() {
        let (_temp_dir, db) = temp_db("online_toggle");
        let conn = db.pool.get().unwrap();
        assert!(is_online_enabled(&conn), "absent row means online");
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, 'false')",
            params![ONLINE_ENABLED_KEY],
        )
        .unwrap();
        assert!(!is_online_enabled(&conn));
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, 'true')",
            params![ONLINE_ENABLED_KEY],
        )
        .unwrap();
        assert!(is_online_enabled(&conn));
    }

    #[test]
    fn test_resolve_song_context_artist_mbid_prefers_tagged_mbid() {
        let (_temp_dir, db) = temp_db("context_resolve_tagged");
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
        let (_temp_dir, db) = temp_db("context_resolve_profile_fallback");
        let conn = db.pool.get().unwrap();
        crate::profiles::set_artist_profile(
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
        let (_temp_dir, db) = temp_db("context_resolve_none");
        let conn = db.pool.get().unwrap();
        assert_eq!(
            resolve_song_context_artist_mbid(&conn, None, None, None, None),
            None
        );
        // An artist with no saved profile at all yields None, not an error.
        assert_eq!(
            resolve_song_context_artist_mbid(&conn, None, None, Some("Nobody Known"), None),
            None
        );
    }

    #[tokio::test]
    async fn test_artist_cache_country_distinguishes_unknown_from_none_and_is_not_lost() {
        let (_temp_dir, db) = temp_db("artist_cache_country");
        let db = Arc::new(db);
        let details = |country: Option<&str>| crate::context::MusicBrainzArtistDetails {
            sort_name: Some("Sort".to_string()),
            country: country.map(str::to_string),
            ..Default::default()
        };
        let country_of = |db: &Arc<Database>, id: &'static str| {
            let db = db.clone();
            async move { read_artist_cache(&db, id).await.unwrap().unwrap().country }
        };

        // MusicBrainz answered with no country: "" (checked), not NULL (never checked).
        write_artist_cache(&db, "none", &None, &Some(details(None)), None, 1)
            .await
            .unwrap();
        assert_eq!(country_of(&db, "none").await.as_deref(), Some(""));

        // A row cached before the column existed reads NULL, so it counts as stale.
        {
            let conn = db.pool.get().unwrap();
            conn.execute(
                "INSERT INTO artist_context_enrichment (artist_id, sort_name, fetched_at) VALUES ('old', 'Sort', 1)",
                [],
            )
            .unwrap();
        }
        assert_eq!(country_of(&db, "old").await, None);

        // A later write with no details (bio only) keeps the stored country.
        write_artist_cache(&db, "kept", &None, &Some(details(Some("US"))), None, 1)
            .await
            .unwrap();
        write_artist_cache(&db, "kept", &None, &None, None, 2)
            .await
            .unwrap();
        assert_eq!(country_of(&db, "kept").await.as_deref(), Some("US"));
    }

    #[tokio::test]
    async fn test_write_and_read_artist_cache_with_mb_details() {
        let (_temp_dir, db) = temp_db("artist_cache_mb_details");
        let db = Arc::new(db);
        let details = crate::context::MusicBrainzArtistDetails {
            sort_name: Some("Twain, Shania".to_string()),
            artist_type: Some("Person".to_string()),
            gender: Some("Female".to_string()),
            begin_date: Some("1965-08-28".to_string()),
            end_date: None,
            ended: Some(false),
            begin_area_name: Some("Windsor".to_string()),
            begin_area_mbid: Some("e4f1e288-92a0-4f36-8f0e-23397e261a99".to_string()),
            area_name: Some("Canada".to_string()),
            area_mbid: Some("71bbafaa-e825-3e15-8ca9-017dcad1748b".to_string()),
            country: Some("CA".to_string()),
        };
        let bio = crate::context::WikipediaSummary {
            extract: "Shania Twain is a Canadian singer-songwriter.".to_string(),
            page_url: Some("https://en.wikipedia.org/wiki/Shania_Twain".to_string()),
            thumbnail_url: None,
        };

        write_artist_cache(
            &db,
            "artist-123",
            &Some(bio),
            &Some(details),
            Some("fr"),
            1000,
        )
        .await
        .unwrap();

        let cached = read_artist_cache(&db, "artist-123").await.unwrap().unwrap();
        assert_eq!(cached.sort_name.as_deref(), Some("Twain, Shania"));
        assert_eq!(cached.artist_type.as_deref(), Some("Person"));
        assert_eq!(cached.gender.as_deref(), Some("Female"));
        assert_eq!(cached.begin_date.as_deref(), Some("1965-08-28"));
        assert_eq!(cached.ended, Some(false));
        assert_eq!(cached.begin_area_name.as_deref(), Some("Windsor"));
        assert_eq!(
            cached.begin_area_mbid.as_deref(),
            Some("e4f1e288-92a0-4f36-8f0e-23397e261a99")
        );
        assert_eq!(cached.area_name.as_deref(), Some("Canada"));
        assert_eq!(
            cached.area_mbid.as_deref(),
            Some("71bbafaa-e825-3e15-8ca9-017dcad1748b")
        );
        assert_eq!(
            cached.wikipedia_extract.as_deref(),
            Some("Shania Twain is a Canadian singer-songwriter.")
        );
        assert_eq!(cached.fetched_at, 1000);
        assert_eq!(cached.wikipedia_lang.as_deref(), Some("fr"));
        assert_eq!(cached.country.as_deref(), Some("CA"));

        let mut enrichment = SongContextEnrichment::default();
        apply_artist_cache(&mut enrichment, &cached);
        assert_eq!(
            enrichment.artist_sort_name.as_deref(),
            Some("Twain, Shania")
        );
        assert_eq!(enrichment.artist_gender.as_deref(), Some("Female"));
        assert_eq!(enrichment.artist_begin_date.as_deref(), Some("1965-08-28"));
    }

    #[tokio::test]
    async fn test_artist_cache_without_sort_name_is_stale() {
        let (_temp_dir, db) = temp_db("artist_cache_no_sort_name");
        let db = Arc::new(db);
        let bio = crate::context::WikipediaSummary {
            extract: "Bio only".to_string(),
            page_url: None,
            thumbnail_url: None,
        };

        // Write row with bio but no MB details (legacy pre-migration 42 shape)
        write_artist_cache(&db, "artist-legacy", &Some(bio), &None, None, 1000)
            .await
            .unwrap();

        let cached = read_artist_cache(&db, "artist-legacy")
            .await
            .unwrap()
            .unwrap();
        assert!(cached.sort_name.is_none());
        assert_eq!(cached.wikipedia_extract.as_deref(), Some("Bio only"));
    }

    #[tokio::test]
    async fn test_artist_cache_keeps_language_when_bio_fetch_failed() {
        let (_temp_dir, db) = temp_db("artist_cache_lang_kept");
        let db = Arc::new(db);
        let bio = crate::context::WikipediaSummary {
            extract: "Bio".to_string(),
            page_url: None,
            thumbnail_url: None,
        };

        write_artist_cache(&db, "artist-lang", &Some(bio), &None, Some("en"), 1000)
            .await
            .unwrap();
        // A later refresh whose bio lookup errored passes no language.
        write_artist_cache(&db, "artist-lang", &None, &None, None, 2000)
            .await
            .unwrap();

        let cached = read_artist_cache(&db, "artist-lang")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cached.wikipedia_lang.as_deref(), Some("en"));
        assert_eq!(cached.wikipedia_extract.as_deref(), Some("Bio"));
    }

    #[tokio::test]
    async fn test_release_group_cache_records_review_language() {
        let (_temp_dir, db) = temp_db("rg_cache_lang");
        let db = Arc::new(db);
        let cb = crate::context::CritiqueBrainzData {
            average_rating: Some(4.0),
            review_count: 1,
            review_links: vec!["https://critiquebrainz.org/review/a".to_string()],
        };

        write_release_group_cache(&db, "rg-1", &None, &Some(cb), Some("fr"), 1000)
            .await
            .unwrap();
        // A refresh whose CritiqueBrainz lookup failed must not relabel the row.
        write_release_group_cache(&db, "rg-1", &None, &None, None, 2000)
            .await
            .unwrap();

        let cached = read_release_group_cache(&db, "rg-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cached.critiquebrainz_lang.as_deref(), Some("fr"));
    }

    #[tokio::test]
    async fn test_artist_events_cache_read_write() {
        let (_dir, db) = temp_db("events_cache");
        let conn = db.pool.get().unwrap();
        let mbid = "mbid-test-123";
        let events = vec![ArtistEvent {
            id: "evt-1".to_string(),
            name: "Live at Wembley".to_string(),
            event_type: Some("Concert".to_string()),
            begin_date: Some("2026-12-01".to_string()),
            end_date: Some("2026-12-01".to_string()),
            time: Some("19:30".to_string()),
            cancelled: false,
            venue_name: Some("Wembley Stadium".to_string()),
            venue_address: Some("Wembley, London".to_string()),
            venue_city: Some("London".to_string()),
            venue_country: Some("United Kingdom".to_string()),
            venue_latitude: Some(51.556),
            venue_longitude: Some(-0.279),
            ticket_urls: vec!["https://tickets.example.com".to_string()],
            event_urls: vec![],
            disambiguation: None,
        }];

        let json = serde_json::to_string(&events).unwrap();
        let fetched_at = 1_700_000_000_i64;
        conn.execute(
            "INSERT INTO artist_events_cache (artist_mbid, events_json, fetched_at) VALUES (?1, ?2, ?3)",
            params![mbid, json, fetched_at],
        )
        .unwrap();

        let (read_json, read_time): (String, i64) = conn
            .query_row(
                "SELECT events_json, fetched_at FROM artist_events_cache WHERE artist_mbid = ?1",
                params![mbid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(read_time, fetched_at);
        let read_events: Vec<ArtistEvent> = serde_json::from_str(&read_json).unwrap();
        assert_eq!(read_events.len(), 1);
        assert_eq!(read_events[0].name, "Live at Wembley");
        assert_eq!(read_events[0].venue_city.as_deref(), Some("London"));
        assert_eq!(
            read_events[0].ticket_urls,
            vec!["https://tickets.example.com"]
        );
    }
}
