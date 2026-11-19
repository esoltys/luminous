//! Local Radio Browser station directory mirror, FTS5 search, facet aggregation,
//! and incremental/full sync orchestration (#2003).
//!
//! Mirrors the public Radio Browser directory into the `radio_directory` table
//! (never `songs`) so station browse, full-text search, and tag/country/language/codec
//! counts are instant and work offline.
//!
//! Sync policy:
//! - The first sync is an explicit opt-in ("Download station index") that sets
//!   `radio_directory_meta.opted_in = 1`, downloads the full directory in pages,
//!   and schedules daily background refreshes through `remote_scheduler.rs`.
//! - Subsequent syncs run incrementally via `/json/stations/changed` + `/json/stations/byuuid`
//!   when `last_synced_at` is within [`MAX_INCREMENTAL_SYNC_AGE_SECS`] (25 days, safely inside
//!   Radio Browser's 30-day change history retention) and a `last_change_uuid` cursor exists.
//! - When `last_synced_at` is older than 25 days, has no `last_change_uuid`, or the incremental
//!   change stream hits the batch cap ([`DEFAULT_CHANGED_LIMIT`]), sync transparently falls
//!   back to a full paged resync and prunes stations no longer present in the upstream directory.

use crate::db::Database;
use crate::remote_scheduler::{RemoteKind, SyncGuard};
use anyhow::Result;
use luminous_audio::can_play_stream;
use luminous_remote::radiobrowser::{
    RadioBrowserClient, RadioBrowserFacetEntry, RadioBrowserStation, DEFAULT_CHANGED_LIMIT,
    DEFAULT_STATION_PAGE_SIZE, RADIO_BROWSER_ATTRIBUTION_URL,
};
use rusqlite::{params, Connection, OptionalExtension, ToSql};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Build-derived `Luminous/<version>` User-Agent sent to Radio Browser mirrors (#2003).
pub const RADIO_BROWSER_USER_AGENT: &str = concat!("Luminous/", env!("CARGO_PKG_VERSION"));

/// Singleton server ID used by [`RemoteKind::RadioBrowser`] in `remote_scheduler.rs`.
pub const RADIO_BROWSER_SCHEDULER_ID: i64 = 0;

/// Daily auto-sync interval in minutes (24 hours) for the Radio Browser mirror.
pub const RADIO_BROWSER_SYNC_INTERVAL_MINUTES: i64 = 24 * 60;

/// Maximum age (25 days in seconds) of `last_synced_at` for which incremental sync
/// (`/json/stations/changed`) is trusted. Upstream retains changes for 30 days;
/// anything older falls back to a full paged resync.
pub const MAX_INCREMENTAL_SYNC_AGE_SECS: i64 = 25 * 24 * 3600;

/// Event emitted to the frontend while a directory sync is running.
pub const EVENT_RADIO_DIRECTORY_SYNC_PROGRESS: &str = "radio-directory-sync-progress";

/// Event emitted to the frontend when the local station directory mirror has changed.
pub const EVENT_RADIO_DIRECTORY_CHANGED: &str = "radio-directory-changed";

/// Current status of the local Radio Browser station directory mirror.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RadioDirectoryStatus {
    /// True once the user has explicitly opted in to downloading the station index.
    pub opted_in: bool,
    /// Total number of working stations currently stored in `radio_directory`.
    pub station_count: usize,
    /// Number of stored stations playable by the audio engine (`can_play_stream`).
    pub playable_station_count: usize,
    /// Unix timestamp (seconds) of the last successful sync (incremental or full).
    pub last_synced_at: Option<i64>,
    /// Unix timestamp (seconds) of the last full paged resync.
    pub last_full_sync_at: Option<i64>,
    /// Unix timestamp (seconds) when the next scheduled daily auto-sync will fire.
    pub next_auto_sync_at: Option<i64>,
    /// True if a sync is currently running in the background.
    pub sync_in_progress: bool,
    /// Error message from the most recent sync attempt, if it failed.
    pub last_error: Option<String>,
    /// Public domain directory attribution URL (`https://www.radio-browser.info`).
    pub attribution_url: String,
}

/// Progress payload emitted on [`EVENT_RADIO_DIRECTORY_SYNC_PROGRESS`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RadioDirectorySyncProgress {
    /// `"incremental"` or `"full"`.
    pub mode: String,
    /// Number of stations fetched/processed so far in this sync run.
    pub fetched_stations: usize,
    /// 1-based page number currently being processed.
    pub page: usize,
    /// True when the sync has finished.
    pub done: bool,
}

/// Summary returned when a directory sync completes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RadioDirectorySyncStats {
    /// `"incremental"` or `"full"`.
    pub mode: String,
    /// Number of stations upserted during the sync.
    pub upserted: usize,
    /// Number of broken or vanished stations removed from the mirror.
    pub removed: usize,
    /// Total stations in `radio_directory` after the sync.
    pub total_stations: usize,
    /// Unix timestamp (seconds) recorded as `last_synced_at`.
    pub synced_at: i64,
}

/// Station record stored in `radio_directory` with playability pre-evaluated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RadioDirectoryStation {
    pub stationuuid: String,
    pub name: String,
    pub url_resolved: String,
    pub homepage: String,
    pub favicon: String,
    pub tags: String,
    pub country: String,
    pub countrycode: String,
    pub language: String,
    pub codec: String,
    pub bitrate: i32,
    pub hls: bool,
    pub votes: i32,
    pub clickcount: i32,
    pub lastcheckok: bool,
    pub changeuuid: String,
    pub updated_at: i64,
    /// Evaluated via `luminous_audio::can_play_stream(&codec, hls)`.
    pub playable: bool,
}

/// Sort order for querying stations from the local directory mirror.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum RadioDirectoryOrder {
    #[default]
    Clickcount,
    Votes,
    Name,
    Bitrate,
}

/// Filter parameters for [`query_stations`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadioDirectoryQuery {
    /// Free-text search query matched against `radio_directory_fts` (name, tags, country, language).
    pub search: Option<String>,
    /// Exact tag filter (case-insensitive match against comma-separated `tags`).
    pub tag: Option<String>,
    /// Exact ISO 3166-1 alpha-2 country code filter (e.g. `"CA"`, `"DE"`).
    pub countrycode: Option<String>,
    /// Exact language token filter (e.g. `"english"`, `"french"`).
    pub language: Option<String>,
    /// Exact codec filter (e.g. `"MP3"`, `"FLAC"`, `"OGG"`).
    pub codec: Option<String>,
    /// Minimum bitrate in kbps.
    pub min_bitrate: Option<i32>,
    /// When true (the default in the UI), excludes stations for which
    /// `luminous_audio::can_play_stream(codec, hls)` is false (HLS, HE-AAC/AAC+).
    #[serde(default = "default_true")]
    pub hide_unsupported: bool,
    #[serde(default)]
    pub order_by: RadioDirectoryOrder,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_query_limit")]
    pub limit: usize,
}

impl Default for RadioDirectoryQuery {
    fn default() -> Self {
        Self {
            search: None,
            tag: None,
            countrycode: None,
            language: None,
            codec: None,
            min_bitrate: None,
            hide_unsupported: default_true(),
            order_by: RadioDirectoryOrder::default(),
            offset: 0,
            limit: default_query_limit(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_query_limit() -> usize {
    200
}

/// Locally computed facet counts (tags, countries, languages, codecs) from `radio_directory`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RadioDirectoryFacets {
    pub tags: Vec<RadioBrowserFacetEntry>,
    pub countries: Vec<RadioBrowserFacetEntry>,
    pub languages: Vec<RadioBrowserFacetEntry>,
    pub codecs: Vec<RadioBrowserFacetEntry>,
}

// ---------------------------------------------------------------------------
// Metadata & Status queries
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub(crate) struct DirectoryMetaRow {
    pub opted_in: bool,
    pub last_synced_at: Option<i64>,
    pub last_full_sync_at: Option<i64>,
    pub last_change_uuid: Option<String>,
    pub last_error: Option<String>,
}

pub(crate) fn load_meta(conn: &Connection) -> Result<DirectoryMetaRow> {
    let row = conn
        .query_row(
            "SELECT opted_in, last_synced_at, last_full_sync_at, last_change_uuid, last_error
             FROM radio_directory_meta WHERE id = 1",
            [],
            |r| {
                Ok(DirectoryMetaRow {
                    opted_in: r.get::<_, i32>(0)? != 0,
                    last_synced_at: r.get(1)?,
                    last_full_sync_at: r.get(2)?,
                    last_change_uuid: r.get(3)?,
                    last_error: r.get(4)?,
                })
            },
        )
        .optional()?
        .unwrap_or_default();
    Ok(row)
}

/// Returns true if the user has opted in to downloading and syncing the Radio Browser index.
pub fn is_opted_in(conn: &Connection) -> bool {
    load_meta(conn).map(|m| m.opted_in).unwrap_or(false)
}

/// Computes the current [`RadioDirectoryStatus`] from the database and scheduler state.
pub fn get_status(
    conn: &Connection,
    next_auto_sync_at: Option<i64>,
    sync_in_progress: bool,
) -> Result<RadioDirectoryStatus> {
    let meta = load_meta(conn)?;

    let mut stmt = conn.prepare(
        "SELECT codec, hls, COUNT(*) FROM radio_directory WHERE lastcheckok = 1 GROUP BY codec, hls",
    )?;
    let groups = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i32>(1)? != 0,
            r.get::<_, i64>(2)? as usize,
        ))
    })?;

    let mut station_count = 0usize;
    let mut playable_station_count = 0usize;
    for group in groups {
        let (codec, hls, count) = group?;
        station_count += count;
        if can_play_stream(&codec, hls) {
            playable_station_count += count;
        }
    }

    Ok(RadioDirectoryStatus {
        opted_in: meta.opted_in,
        station_count,
        playable_station_count,
        last_synced_at: meta.last_synced_at,
        last_full_sync_at: meta.last_full_sync_at,
        next_auto_sync_at,
        sync_in_progress,
        last_error: meta.last_error,
        attribution_url: RADIO_BROWSER_ATTRIBUTION_URL.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Sync Engine (Incremental + Full Resync Fallback)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncPlan {
    Incremental,
    Full,
}

fn choose_sync_plan(meta: &DirectoryMetaRow, now: i64, existing_rows: i64) -> SyncPlan {
    if existing_rows == 0 {
        return SyncPlan::Full;
    }
    let Some(last_synced) = meta.last_synced_at else {
        return SyncPlan::Full;
    };
    let Some(ref change_uuid) = meta.last_change_uuid else {
        return SyncPlan::Full;
    };
    if change_uuid.trim().is_empty() {
        return SyncPlan::Full;
    }
    if now.saturating_sub(last_synced) > MAX_INCREMENTAL_SYNC_AGE_SECS {
        return SyncPlan::Full;
    }
    SyncPlan::Incremental
}

fn upsert_stations_tx(
    tx: &rusqlite::Transaction<'_>,
    stations: &[RadioBrowserStation],
    updated_at: i64,
) -> Result<(usize, usize)> {
    let mut upsert_stmt = tx.prepare(
        "INSERT INTO radio_directory (
            stationuuid, name, url_resolved, homepage, favicon, tags,
            country, countrycode, language, codec, bitrate, hls,
            votes, clickcount, lastcheckok, changeuuid, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16, ?17
         )
         ON CONFLICT(stationuuid) DO UPDATE SET
            name         = excluded.name,
            url_resolved = excluded.url_resolved,
            homepage     = excluded.homepage,
            favicon      = excluded.favicon,
            tags         = excluded.tags,
            country      = excluded.country,
            countrycode  = excluded.countrycode,
            language     = excluded.language,
            codec        = excluded.codec,
            bitrate      = excluded.bitrate,
            hls          = excluded.hls,
            votes        = excluded.votes,
            clickcount   = excluded.clickcount,
            lastcheckok  = excluded.lastcheckok,
            changeuuid   = excluded.changeuuid,
            updated_at   = excluded.updated_at",
    )?;
    let mut delete_stmt = tx.prepare("DELETE FROM radio_directory WHERE stationuuid = ?1")?;

    let mut upserted = 0usize;
    let mut removed = 0usize;
    for s in stations {
        if !s.lastcheckok {
            removed += delete_stmt.execute(params![s.stationuuid])?;
            continue;
        }
        upsert_stmt.execute(params![
            s.stationuuid,
            s.name,
            s.url_resolved,
            s.homepage,
            s.favicon,
            s.tags,
            s.country,
            s.countrycode,
            s.language,
            s.codec,
            s.bitrate,
            i32::from(s.hls),
            s.votes,
            s.clickcount,
            i32::from(s.lastcheckok),
            s.changeuuid,
            updated_at,
        ])?;
        upserted += 1;
    }
    Ok((upserted, removed))
}

/// Core sync routine that accepts any [`RadioBrowserClient`] (live DNS or test mock),
/// a page size, and a progress callback.
pub async fn sync_with_client<F>(
    db: &Arc<Database>,
    client: &RadioBrowserClient,
    page_size: usize,
    changed_limit: usize,
    now: i64,
    mut on_progress: F,
) -> Result<RadioDirectorySyncStats, String>
where
    F: FnMut(RadioDirectorySyncProgress),
{
    let page_size = page_size.max(1);
    let changed_limit = changed_limit.max(1);

    // Mark opted_in = 1 immediately so the user's explicit opt-in is persisted
    // even if the network fails mid-download (allowing retry).
    let (meta, existing_rows) = crate::db::run_blocking(db, |conn| {
        conn.execute(
            "INSERT INTO radio_directory_meta (id, opted_in) VALUES (1, 1)
             ON CONFLICT(id) DO UPDATE SET opted_in = 1",
            [],
        )?;
        let meta = load_meta(conn)?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM radio_directory", [], |r| r.get(0))?;
        Ok((meta, count))
    })
    .await
    .map_err(|e| e.to_string())?;

    let initial_plan = choose_sync_plan(&meta, now, existing_rows);

    let result = async {
        if initial_plan == SyncPlan::Incremental {
            let last_uuid = meta.last_change_uuid.clone().unwrap_or_default();
            on_progress(RadioDirectorySyncProgress {
                mode: "incremental".to_string(),
                fetched_stations: 0,
                page: 1,
                done: false,
            });

            match client.fetch_changed_stations(&last_uuid, changed_limit).await {
                Ok(batch) if batch.cursor_valid => {
                    let newest_uuid = batch.newest_change_uuid.or(Some(last_uuid));
                    let stations = batch.stations;
                    let fetched_len = stations.len();
                    let stats = crate::db::run_blocking(db, move |conn| {
                        let tx = conn.unchecked_transaction()?;
                        let (upserted, removed) = upsert_stations_tx(&tx, &stations, now)?;
                        tx.execute(
                            "UPDATE radio_directory_meta
                             SET last_synced_at = ?1,
                                 last_change_uuid = COALESCE(?2, last_change_uuid),
                                 last_error = NULL
                             WHERE id = 1",
                            params![now, newest_uuid],
                        )?;
                        let total_stations: i64 = tx.query_row(
                            "SELECT COUNT(*) FROM radio_directory",
                            [],
                            |r| r.get(0),
                        )?;
                        tx.commit()?;
                        Ok(RadioDirectorySyncStats {
                            mode: "incremental".to_string(),
                            upserted,
                            removed,
                            total_stations: total_stations as usize,
                            synced_at: now,
                        })
                    })
                    .await
                    .map_err(|e| e.to_string())?;

                    on_progress(RadioDirectorySyncProgress {
                        mode: "incremental".to_string(),
                        fetched_stations: fetched_len,
                        page: 1,
                        done: true,
                    });
                    return Ok(stats);
                }
                Ok(_) => {
                    log::info!(
                        "Radio Browser incremental sync hit change limit ({changed_limit}); falling back to full resync"
                    );
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        // Full paged resync: fetch all pages first, upserting each page with `updated_at = now`.
        // Only after the final page succeeds do we evict rows with `updated_at < now`, ensuring
        // an interrupted download never wipes out an existing mirror.
        let mut offset = 0usize;
        let mut page_num = 0usize;
        let mut total_upserted = 0usize;
        let mut latest_change_uuid: Option<(Option<String>, String)> = None;

        loop {
            page_num += 1;
            let page = client.fetch_stations_page(offset, page_size).await?;
            let page_len = page.len();

            for s in &page {
                if !s.changeuuid.is_empty() {
                    let candidate_ts = s.lastchangetime_iso8601.clone();
                    let should_replace = match &latest_change_uuid {
                        None => true,
                        Some((best_ts, _)) => candidate_ts >= *best_ts,
                    };
                    if should_replace {
                        latest_change_uuid = Some((candidate_ts, s.changeuuid.clone()));
                    }
                }
            }

            if !page.is_empty() {
                let (upserted, _) = crate::db::run_blocking(db, move |conn| {
                    let tx = conn.unchecked_transaction()?;
                    let counts = upsert_stations_tx(&tx, &page, now)?;
                    tx.commit()?;
                    Ok(counts)
                })
                .await
                .map_err(|e| e.to_string())?;
                total_upserted += upserted;
            }

            on_progress(RadioDirectorySyncProgress {
                mode: "full".to_string(),
                fetched_stations: total_upserted,
                page: page_num,
                done: false,
            });

            if page_len < page_size {
                break;
            }
            offset += page_len;
        }

        let newest_uuid = latest_change_uuid.map(|(_, uuid)| uuid);
        let stats = crate::db::run_blocking(db, move |conn| {
            let tx = conn.unchecked_transaction()?;
            // Only prune stale rows if we received at least one station (protects against
            // an upstream mirror bug returning an empty 200 array on page 1).
            let removed = if total_upserted > 0 {
                tx.execute(
                    "DELETE FROM radio_directory WHERE updated_at < ?1",
                    params![now],
                )?
            } else {
                0
            };
            tx.execute(
                "UPDATE radio_directory_meta
                 SET opted_in = 1,
                     last_synced_at = ?1,
                     last_full_sync_at = ?1,
                     last_change_uuid = COALESCE(?2, last_change_uuid),
                     last_error = NULL
                 WHERE id = 1",
                params![now, newest_uuid],
            )?;
            let total_stations: i64 =
                tx.query_row("SELECT COUNT(*) FROM radio_directory", [], |r| r.get(0))?;
            tx.commit()?;
            Ok(RadioDirectorySyncStats {
                mode: "full".to_string(),
                upserted: total_upserted,
                removed,
                total_stations: total_stations as usize,
                synced_at: now,
            })
        })
        .await
        .map_err(|e| e.to_string())?;

        on_progress(RadioDirectorySyncProgress {
            mode: "full".to_string(),
            fetched_stations: total_upserted,
            page: page_num,
            done: true,
        });

        Ok(stats)
    }
    .await;

    if let Err(ref err_msg) = result {
        let err_clone = err_msg.clone();
        let _ = crate::db::run_blocking(db, move |conn| {
            conn.execute(
                "UPDATE radio_directory_meta SET last_error = ?1 WHERE id = 1",
                params![err_clone],
            )?;
            Ok(())
        })
        .await;
    }

    result
}

/// Runs a guarded Radio Browser sync (used by both the manual IPC command and
/// `AutoSyncScheduler`'s daily `RemoteKind::RadioBrowser` tick) and emits
/// `radio-directory-sync-progress` and `radio-directory-changed` events.
pub async fn sync_radio_directory_inner(
    app: AppHandle,
    db: Arc<Database>,
) -> Result<RadioDirectorySyncStats, String> {
    let _guard = SyncGuard::acquire(RemoteKind::RadioBrowser, RADIO_BROWSER_SCHEDULER_ID)?;
    let client = RadioBrowserClient::new(RADIO_BROWSER_USER_AGENT);
    let now = crate::clock::unix_now();
    let app_for_progress = app.clone();

    let stats = sync_with_client(
        &db,
        &client,
        DEFAULT_STATION_PAGE_SIZE,
        DEFAULT_CHANGED_LIMIT,
        now,
        move |progress| {
            let _ = app_for_progress.emit(EVENT_RADIO_DIRECTORY_SYNC_PROGRESS, progress);
        },
    )
    .await?;

    let _ = app.emit(EVENT_RADIO_DIRECTORY_CHANGED, &stats);
    Ok(stats)
}

// ---------------------------------------------------------------------------
// Local Search, Browse & Facet Queries
// ---------------------------------------------------------------------------

/// Builds an FTS5 prefix-match query string from free-text user input.
/// Strips FTS5 syntax characters so arbitrary user punctuation never causes
/// an SQLite FTS5 syntax error.
pub fn build_fts_query(raw: &str) -> Option<String> {
    let tokens: Vec<String> = raw
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.to_lowercase()))
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" AND "))
    }
}

fn row_to_station(row: &rusqlite::Row<'_>) -> rusqlite::Result<RadioDirectoryStation> {
    let codec: String = row.get(9)?;
    let hls: bool = row.get::<_, i32>(11)? != 0;
    let playable = can_play_stream(&codec, hls);
    Ok(RadioDirectoryStation {
        stationuuid: row.get(0)?,
        name: row.get(1)?,
        url_resolved: row.get(2)?,
        homepage: row.get(3)?,
        favicon: row.get(4)?,
        tags: row.get(5)?,
        country: row.get(6)?,
        countrycode: row.get(7)?,
        language: row.get(8)?,
        codec,
        bitrate: row.get(10)?,
        hls,
        votes: row.get(12)?,
        clickcount: row.get(13)?,
        lastcheckok: row.get::<_, i32>(14)? != 0,
        changeuuid: row.get(15)?,
        updated_at: row.get(16)?,
        playable,
    })
}

const STATION_SELECT_COLS: &str =
    "stationuuid, name, url_resolved, homepage, favicon, tags, country, countrycode, language, codec, bitrate, hls, votes, clickcount, lastcheckok, changeuuid, updated_at";

/// Looks up a single station by `stationuuid` from the local mirror.
pub fn get_station_by_uuid(
    conn: &Connection,
    stationuuid: &str,
) -> Result<Option<RadioDirectoryStation>> {
    let clean = stationuuid.trim().to_ascii_lowercase();
    if clean.is_empty() {
        return Ok(None);
    }
    let sql = format!("SELECT {STATION_SELECT_COLS} FROM radio_directory WHERE stationuuid = ?1");
    Ok(conn
        .query_row(&sql, params![clean], row_to_station)
        .optional()?)
}

/// Queries stations from the local `radio_directory` mirror using FTS5 full-text search
/// and optional tag, country, language, codec, bitrate, and playability filters.
pub fn query_stations(
    conn: &Connection,
    query: &RadioDirectoryQuery,
) -> Result<Vec<RadioDirectoryStation>> {
    let mut clauses = vec!["lastcheckok = 1".to_string()];
    let mut bind_params: Vec<Box<dyn ToSql>> = Vec::new();

    if let Some(ref search) = query.search {
        if let Some(fts_expr) = build_fts_query(search) {
            bind_params.push(Box::new(fts_expr));
            let idx = bind_params.len();
            clauses.push(format!(
                "rowid IN (SELECT rowid FROM radio_directory_fts WHERE radio_directory_fts MATCH ?{idx})"
            ));
        }
    }

    if let Some(ref tag) = query.tag {
        let clean_tag = tag.trim().to_lowercase();
        if !clean_tag.is_empty() {
            // Exact match inside comma-separated `tags`
            bind_params.push(Box::new(format!("%,{clean_tag},%")));
            let idx = bind_params.len();
            clauses.push(format!("(',' || tags || ',') LIKE ?{idx}"));
        }
    }

    if let Some(ref cc) = query.countrycode {
        let clean_cc = cc.trim().to_ascii_uppercase();
        if !clean_cc.is_empty() {
            bind_params.push(Box::new(clean_cc));
            let idx = bind_params.len();
            clauses.push(format!("countrycode = ?{idx}"));
        }
    }

    if let Some(ref lang) = query.language {
        let clean_lang = lang.trim().to_lowercase();
        if !clean_lang.is_empty() {
            bind_params.push(Box::new(format!("%,{clean_lang},%")));
            let idx = bind_params.len();
            clauses.push(format!("(',' || language || ',') LIKE ?{idx}"));
        }
    }

    if let Some(ref codec) = query.codec {
        let clean_codec = codec.trim().to_ascii_uppercase();
        if !clean_codec.is_empty() {
            bind_params.push(Box::new(clean_codec));
            let idx = bind_params.len();
            clauses.push(format!("codec = ?{idx}"));
        }
    }

    if let Some(min_br) = query.min_bitrate.filter(|&b| b > 0) {
        bind_params.push(Box::new(min_br));
        let idx = bind_params.len();
        clauses.push(format!("bitrate >= ?{idx}"));
    }

    if query.hide_unsupported {
        clauses.push("hls = 0".to_string());
    }

    let order_sql = match query.order_by {
        RadioDirectoryOrder::Clickcount => "clickcount DESC, votes DESC, name COLLATE NOCASE ASC",
        RadioDirectoryOrder::Votes => "votes DESC, clickcount DESC, name COLLATE NOCASE ASC",
        RadioDirectoryOrder::Name => "name COLLATE NOCASE ASC, clickcount DESC",
        RadioDirectoryOrder::Bitrate => "bitrate DESC, clickcount DESC, name COLLATE NOCASE ASC",
    };

    let sql = format!(
        "SELECT {STATION_SELECT_COLS} FROM radio_directory WHERE {} ORDER BY {order_sql}",
        clauses.join(" AND ")
    );

    let param_refs: Vec<&dyn ToSql> = bind_params.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), row_to_station)?;

    let limit = query.limit.max(1);
    let mut results = Vec::with_capacity(limit.min(512));
    let mut skipped = 0usize;

    for row in rows {
        let station = row?;
        if query.hide_unsupported && !station.playable {
            continue;
        }
        if skipped < query.offset {
            skipped += 1;
            continue;
        }
        results.push(station);
        if results.len() >= limit {
            break;
        }
    }

    Ok(results)
}

/// Computes local tag, country, language, and codec counts directly from `radio_directory`.
/// When `hide_unsupported` is true, only stations playable by `luminous_audio::can_play_stream`
/// are counted.
pub fn compute_local_facets(
    conn: &Connection,
    hide_unsupported: bool,
    tag_limit: usize,
) -> Result<RadioDirectoryFacets> {
    let mut stmt = conn.prepare(
        "SELECT tags, country, countrycode, language, codec, hls
         FROM radio_directory
         WHERE lastcheckok = 1",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i32>(5)? != 0,
        ))
    })?;

    let mut tag_counts: HashMap<String, i64> = HashMap::new();
    let mut country_counts: HashMap<String, (String, i64)> = HashMap::new();
    let mut language_counts: HashMap<String, i64> = HashMap::new();
    let mut codec_counts: HashMap<String, i64> = HashMap::new();

    for row in rows {
        let (tags, country, countrycode, language, codec, hls) = row?;
        if hide_unsupported && !can_play_stream(&codec, hls) {
            continue;
        }

        if !codec.is_empty() {
            *codec_counts.entry(codec).or_default() += 1;
        }

        if !countrycode.is_empty() || !country.is_empty() {
            let key = if !countrycode.is_empty() {
                countrycode.clone()
            } else {
                country.to_uppercase()
            };
            let entry = country_counts
                .entry(key)
                .or_insert_with(|| (country.clone(), 0));
            if entry.0.is_empty() && !country.is_empty() {
                entry.0 = country;
            }
            entry.1 += 1;
        }

        if !language.is_empty() {
            for lang in language.split(',').filter(|l| !l.is_empty()) {
                *language_counts.entry(lang.to_string()).or_default() += 1;
            }
        }

        if !tags.is_empty() {
            for tag in tags.split(',').filter(|t| !t.is_empty()) {
                *tag_counts.entry(tag.to_string()).or_default() += 1;
            }
        }
    }

    let mut tags: Vec<RadioBrowserFacetEntry> = tag_counts
        .into_iter()
        .map(|(name, stationcount)| RadioBrowserFacetEntry {
            name,
            code: None,
            stationcount,
        })
        .collect();
    tags.sort_by(|a, b| {
        b.stationcount
            .cmp(&a.stationcount)
            .then_with(|| a.name.cmp(&b.name))
    });
    if tag_limit > 0 && tags.len() > tag_limit {
        tags.truncate(tag_limit);
    }

    let mut countries: Vec<RadioBrowserFacetEntry> = country_counts
        .into_iter()
        .map(|(code, (name, stationcount))| RadioBrowserFacetEntry {
            name: if name.is_empty() { code.clone() } else { name },
            code: Some(code),
            stationcount,
        })
        .collect();
    countries.sort_by(|a, b| {
        b.stationcount
            .cmp(&a.stationcount)
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut languages: Vec<RadioBrowserFacetEntry> = language_counts
        .into_iter()
        .map(|(name, stationcount)| RadioBrowserFacetEntry {
            name,
            code: None,
            stationcount,
        })
        .collect();
    languages.sort_by(|a, b| {
        b.stationcount
            .cmp(&a.stationcount)
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut codecs: Vec<RadioBrowserFacetEntry> = codec_counts
        .into_iter()
        .map(|(name, stationcount)| RadioBrowserFacetEntry {
            name,
            code: None,
            stationcount,
        })
        .collect();
    codecs.sort_by(|a, b| {
        b.stationcount
            .cmp(&a.stationcount)
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(RadioDirectoryFacets {
        tags,
        countries,
        languages,
        codecs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_db() -> (tempfile::TempDir, Arc<Database>) {
        let dir = tempfile::Builder::new()
            .prefix("luminous_radio_dir_test_")
            .tempdir()
            .unwrap();
        let db = Arc::new(Database::new(dir.path().to_path_buf()).unwrap());
        (dir, db)
    }

    #[allow(clippy::too_many_arguments)]
    fn sample_station_json(
        uuid: &str,
        changeuuid: &str,
        name: &str,
        tags: &str,
        country: &str,
        countrycode: &str,
        language: &str,
        codec: &str,
        bitrate: i32,
        hls: i32,
        clickcount: i32,
        lastcheckok: i32,
        changetime: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "stationuuid": uuid,
            "changeuuid": changeuuid,
            "name": name,
            "url_resolved": format!("https://stream.example/{uuid}.mp3"),
            "homepage": format!("https://example.com/{uuid}"),
            "favicon": "",
            "tags": tags,
            "country": country,
            "countrycode": countrycode,
            "language": language,
            "codec": codec,
            "bitrate": bitrate,
            "hls": hls,
            "votes": 10,
            "clickcount": clickcount,
            "lastcheckok": lastcheckok,
            "lastchangetime_iso8601": changetime
        })
    }

    #[tokio::test]
    async fn first_sync_pages_full_directory_and_populates_fts_and_facets() {
        let (_dir, db) = test_db();
        let server = MockServer::start().await;

        // Before first sync: not opted in, empty mirror
        {
            let conn = db.pool.get().unwrap();
            let status = get_status(&conn, None, false).unwrap();
            assert!(!status.opted_in);
            assert_eq!(status.station_count, 0);
        }

        // Page 1 (limit=2 -> full page, triggers page 2)
        Mock::given(method("GET"))
            .and(path("/json/stations"))
            .and(query_param("hidebroken", "true"))
            .and(query_param("offset", "0"))
            .and(query_param("limit", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                sample_station_json(
                    "s-jazz-ca",
                    "chg-1",
                    "Jazz FM91 Toronto",
                    "jazz,smooth jazz",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    192,
                    0,
                    500,
                    1,
                    "2026-11-10T10:00:00Z"
                ),
                sample_station_json(
                    "s-metal-fi",
                    "chg-2",
                    "Radio Rock Helsinki",
                    "folk metal,metal,rock",
                    "Finland",
                    "FI",
                    "finnish",
                    "FLAC",
                    1000,
                    0,
                    300,
                    1,
                    "2026-11-12T10:00:00Z"
                )
            ])))
            .mount(&server)
            .await;

        // Page 2 (limit=2 -> returns 1 item, ends pagination)
        Mock::given(method("GET"))
            .and(path("/json/stations"))
            .and(query_param("hidebroken", "true"))
            .and(query_param("offset", "2"))
            .and(query_param("limit", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                sample_station_json(
                    "s-hls-us",
                    "chg-3",
                    "Unsupported HLS Stream",
                    "jazz,news",
                    "United States",
                    "US",
                    "english",
                    "AAC+",
                    64,
                    1,
                    900,
                    1,
                    "2026-11-11T10:00:00Z"
                )
            ])))
            .mount(&server)
            .await;

        let client = RadioBrowserClient::with_servers(vec![server.uri()], RADIO_BROWSER_USER_AGENT);
        let mut progress_events = Vec::new();
        let stats = sync_with_client(&db, &client, 2, 100, 1_700_000_000, |p| {
            progress_events.push(p);
        })
        .await
        .expect("initial full sync succeeds");

        assert_eq!(stats.mode, "full");
        assert_eq!(stats.upserted, 3);
        assert_eq!(stats.total_stations, 3);
        assert!(progress_events.last().unwrap().done);

        let conn = db.pool.get().unwrap();
        let status = get_status(&conn, Some(1_700_086_400), false).unwrap();
        assert!(status.opted_in);
        assert_eq!(status.station_count, 3);
        assert_eq!(status.playable_station_count, 2);
        assert_eq!(status.last_synced_at, Some(1_700_000_000));
        assert_eq!(status.last_full_sync_at, Some(1_700_000_000));

        // chg-2 had the newest timestamp ("2026-11-12T10:00:00Z")
        let meta = load_meta(&conn).unwrap();
        assert_eq!(meta.last_change_uuid.as_deref(), Some("chg-2"));

        // Playable filter hides HLS / AAC+ by default
        let playable_jazz = query_stations(
            &conn,
            &RadioDirectoryQuery {
                tag: Some("jazz".to_string()),
                hide_unsupported: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(playable_jazz.len(), 1);
        assert_eq!(playable_jazz[0].stationuuid, "s-jazz-ca");

        // Including unsupported returns the HLS station too, ordered by clickcount DESC
        let all_jazz = query_stations(
            &conn,
            &RadioDirectoryQuery {
                tag: Some("jazz".to_string()),
                hide_unsupported: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all_jazz.len(), 2);
        assert_eq!(all_jazz[0].stationuuid, "s-hls-us");

        // FTS5 search matches prefix on name/tags/country
        let fts_results = query_stations(
            &conn,
            &RadioDirectoryQuery {
                search: Some("helsin folk".to_string()),
                hide_unsupported: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(fts_results.len(), 1);
        assert_eq!(fts_results[0].stationuuid, "s-metal-fi");

        // Local facet counts respect hide_unsupported
        let facets = compute_local_facets(&conn, true, 50).unwrap();
        assert_eq!(facets.countries.len(), 2);
        assert_eq!(
            facets
                .tags
                .iter()
                .find(|t| t.name == "jazz")
                .map(|t| t.stationcount),
            Some(1)
        );
    }

    #[tokio::test]
    async fn incremental_sync_updates_changed_and_evicts_newly_broken_stations() {
        let (_dir, db) = test_db();
        let server = MockServer::start().await;

        // Seed initial state from 1 day ago
        let t0 = 1_700_000_000i64;
        Mock::given(method("GET"))
            .and(path("/json/stations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                sample_station_json(
                    "s-1",
                    "chg-1",
                    "Station One",
                    "rock",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    128,
                    0,
                    100,
                    1,
                    "2026-11-10T10:00:00Z"
                ),
                sample_station_json(
                    "s-2",
                    "chg-2",
                    "Station Two",
                    "pop",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    128,
                    0,
                    50,
                    1,
                    "2026-11-11T10:00:00Z"
                )
            ])))
            .mount(&server)
            .await;

        let client = RadioBrowserClient::with_servers(vec![server.uri()], RADIO_BROWSER_USER_AGENT);
        sync_with_client(&db, &client, 10, 100, t0, |_| {})
            .await
            .unwrap();

        // Incremental sync 1 day later: s-1 updated its bitrate, s-2 became broken (lastcheckok=0), s-3 added
        Mock::given(method("GET"))
            .and(path("/json/stations/changed"))
            .and(query_param("lastchangeuuid", "chg-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "changeuuid": "chg-3", "stationuuid": "s-1", "name": "Station One HD" },
                { "changeuuid": "chg-4", "stationuuid": "s-2", "name": "Station Two" },
                { "changeuuid": "chg-5", "stationuuid": "s-3", "name": "Station Three" }
            ])))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/json/stations/byuuid"))
            .and(query_param("uuids", "s-1,s-2,s-3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                sample_station_json(
                    "s-1",
                    "chg-3",
                    "Station One HD",
                    "rock",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    320,
                    0,
                    150,
                    1,
                    "2026-11-12T10:00:00Z"
                ),
                sample_station_json(
                    "s-2",
                    "chg-4",
                    "Station Two",
                    "pop",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    128,
                    0,
                    50,
                    0,
                    "2026-11-12T11:00:00Z"
                ),
                sample_station_json(
                    "s-3",
                    "chg-5",
                    "Station Three",
                    "ambient",
                    "Germany",
                    "DE",
                    "german",
                    "OGG",
                    192,
                    0,
                    80,
                    1,
                    "2026-11-12T12:00:00Z"
                )
            ])))
            .mount(&server)
            .await;

        let t1 = t0 + 86_400;
        let stats = sync_with_client(&db, &client, 10, 100, t1, |_| {})
            .await
            .expect("incremental sync succeeds");

        assert_eq!(stats.mode, "incremental");
        assert_eq!(stats.upserted, 2);
        assert_eq!(stats.removed, 1);
        assert_eq!(stats.total_stations, 2);

        let conn = db.pool.get().unwrap();
        let s1 = get_station_by_uuid(&conn, "s-1").unwrap().unwrap();
        assert_eq!(s1.name, "Station One HD");
        assert_eq!(s1.bitrate, 320);
        assert!(get_station_by_uuid(&conn, "s-2").unwrap().is_none());
        assert!(get_station_by_uuid(&conn, "s-3").unwrap().is_some());
        assert_eq!(
            load_meta(&conn).unwrap().last_change_uuid.as_deref(),
            Some("chg-5")
        );
    }

    #[tokio::test]
    async fn falls_back_to_full_resync_when_last_sync_is_older_than_30_day_window() {
        let (_dir, db) = test_db();
        let server = MockServer::start().await;

        let t0 = 1_700_000_000i64;
        Mock::given(method("GET"))
            .and(path("/json/stations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                sample_station_json(
                    "s-old",
                    "chg-old",
                    "Old Station",
                    "rock",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    128,
                    0,
                    10,
                    1,
                    "2026-10-01T10:00:00Z"
                ),
                sample_station_json(
                    "s-survivor",
                    "chg-surv",
                    "Survivor Station",
                    "rock",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    128,
                    0,
                    20,
                    1,
                    "2026-10-02T10:00:00Z"
                )
            ])))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let client = RadioBrowserClient::with_servers(vec![server.uri()], RADIO_BROWSER_USER_AGENT);
        sync_with_client(&db, &client, 10, 100, t0, |_| {})
            .await
            .unwrap();

        // 31 days later: s-old is gone from the directory, s-survivor remains, s-fresh added.
        // `/json/stations/changed` is NOT even called because `now - last_synced_at > 25 days`.
        Mock::given(method("GET"))
            .and(path("/json/stations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                sample_station_json(
                    "s-survivor",
                    "chg-surv-2",
                    "Survivor Station",
                    "rock",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    128,
                    0,
                    25,
                    1,
                    "2026-11-15T10:00:00Z"
                ),
                sample_station_json(
                    "s-fresh",
                    "chg-fresh",
                    "Fresh Station",
                    "jazz",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    256,
                    0,
                    40,
                    1,
                    "2026-11-16T10:00:00Z"
                )
            ])))
            .mount(&server)
            .await;

        let t_expired = t0 + 31 * 86_400;
        let stats = sync_with_client(&db, &client, 10, 100, t_expired, |_| {})
            .await
            .expect("full resync fallback succeeds");

        assert_eq!(stats.mode, "full");
        assert_eq!(stats.upserted, 2);
        assert_eq!(stats.removed, 1); // s-old pruned!
        assert_eq!(stats.total_stations, 2);

        let conn = db.pool.get().unwrap();
        assert!(get_station_by_uuid(&conn, "s-old").unwrap().is_none());
        assert!(get_station_by_uuid(&conn, "s-survivor").unwrap().is_some());
        assert!(get_station_by_uuid(&conn, "s-fresh").unwrap().is_some());
    }
}
