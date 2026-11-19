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
    RadioBrowserClient, RadioBrowserFacetEntry, RadioBrowserStation, StationsPage,
    DEFAULT_CHANGED_LIMIT, DEFAULT_STATION_PAGE_SIZE, RADIO_BROWSER_ATTRIBUTION_URL,
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
    /// Stations whose name matches every search token first, then stations matched
    /// only through tags, country or language; each tier by popularity. Without a
    /// search it falls back to [`RadioDirectoryOrder::Clickcount`].
    Relevance,
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

fn is_change_batch_fresh(oldest_iso8601: Option<&str>, last_synced_at: Option<i64>) -> bool {
    let (Some(iso), Some(last_synced)) = (oldest_iso8601, last_synced_at) else {
        return true;
    };
    let Ok(ts) = iso.parse::<jiff::Timestamp>() else {
        return true;
    };
    // Allow 1 day of clock skew/overlap; if the server returned changes more than 24h
    // older than our last sync, it ignored our `lastchangeuuid` and dumped its 30-day history.
    ts.as_second() >= last_synced.saturating_sub(86_400)
}

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
                Ok(batch)
                    if batch.cursor_valid
                        && is_change_batch_fresh(
                            batch.oldest_change_iso8601.as_deref(),
                            meta.last_synced_at,
                        ) =>
                {
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
            // Page on the server's raw row count: normalisation can drop rows, and a full
            // page that lost one is still not the last page (#2199).
            let StationsPage {
                stations: page,
                raw_count,
            } = client.fetch_stations_page(offset, page_size).await?;

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

            if raw_count < page_size {
                break;
            }
            offset += raw_count;
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

/// Builds SQL WHERE clauses and bind parameters for `query_stations` and
/// `compute_filtered_facets`. When `include_facets` is false, `countrycode`,
/// `language`, and `codec` are omitted so cross-linked facet counts can be
/// accumulated in a single scan.
fn build_where_clauses(
    query: &RadioDirectoryQuery,
    include_facets: bool,
) -> (Vec<String>, Vec<Box<dyn ToSql>>) {
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

    if include_facets {
        if let Some(ref cc) = query.countrycode {
            let clean_cc = cc.trim().to_ascii_uppercase();
            if !clean_cc.is_empty() {
                bind_params.push(Box::new(clean_cc));
                let idx = bind_params.len();
                clauses.push(format!("(countrycode = ?{idx} OR UPPER(country) = ?{idx})"));
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
    }

    if let Some(min_br) = query.min_bitrate.filter(|&b| b > 0) {
        bind_params.push(Box::new(min_br));
        let idx = bind_params.len();
        clauses.push(format!("bitrate >= ?{idx}"));
    }

    if query.hide_unsupported {
        clauses.push("hls = 0".to_string());
    }

    (clauses, bind_params)
}

/// Queries stations from the local `radio_directory` mirror using FTS5 full-text search
/// and optional tag, country, language, codec, bitrate, and playability filters.
pub fn query_stations(
    conn: &Connection,
    query: &RadioDirectoryQuery,
) -> Result<Vec<RadioDirectoryStation>> {
    let (clauses, mut bind_params) = build_where_clauses(query, true);

    const POPULARITY: &str = "clickcount DESC, votes DESC, name COLLATE NOCASE ASC";
    let name_match = match query.order_by {
        RadioDirectoryOrder::Relevance => query.search.as_deref().and_then(build_fts_query),
        _ => None,
    };
    let order_sql = match query.order_by {
        RadioDirectoryOrder::Clickcount => POPULARITY.to_string(),
        RadioDirectoryOrder::Votes => {
            "votes DESC, clickcount DESC, name COLLATE NOCASE ASC".to_string()
        }
        RadioDirectoryOrder::Name => "name COLLATE NOCASE ASC, clickcount DESC".to_string(),
        RadioDirectoryOrder::Bitrate => {
            "bitrate DESC, clickcount DESC, name COLLATE NOCASE ASC".to_string()
        }
        RadioDirectoryOrder::Relevance => match name_match {
            // FTS5 column filter: every token must prefix-match a word of the name.
            Some(expr) => {
                bind_params.push(Box::new(format!("name : ({expr})")));
                let idx = bind_params.len();
                format!(
                    "CASE WHEN rowid IN (SELECT rowid FROM radio_directory_fts WHERE radio_directory_fts MATCH ?{idx}) THEN 0 ELSE 1 END, {POPULARITY}"
                )
            }
            None => POPULARITY.to_string(),
        },
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

/// Hard cap on [`search_stations`] results, whatever the caller asks for.
pub const MAX_STATION_SEARCH_RESULTS: usize = 20;

/// Global-search lookup over the local directory mirror (#2007): prefix match on
/// name, tags, country and language, ranked by [`RadioDirectoryOrder::Relevance`],
/// playable stations only, at most [`MAX_STATION_SEARCH_RESULTS`] rows.
///
/// Reads only the local mirror, so it works offline. Returns no rows (not an error)
/// for a query with no searchable words or when the user hasn't opted in to the
/// directory.
pub fn search_stations(
    conn: &Connection,
    search: &str,
    limit: usize,
) -> Result<Vec<RadioDirectoryStation>> {
    if build_fts_query(search).is_none() || !is_opted_in(conn) {
        return Ok(Vec::new());
    }
    query_stations(
        conn,
        &RadioDirectoryQuery {
            search: Some(search.to_string()),
            order_by: RadioDirectoryOrder::Relevance,
            limit: limit.clamp(1, MAX_STATION_SEARCH_RESULTS),
            ..Default::default()
        },
    )
}

/// Computes local tag, country, language, and codec counts directly from `radio_directory`.
/// When `hide_unsupported` is true, only stations playable by `luminous_audio::can_play_stream`
/// are counted.
pub fn compute_local_facets(
    conn: &Connection,
    hide_unsupported: bool,
    tag_limit: usize,
) -> Result<RadioDirectoryFacets> {
    let query = RadioDirectoryQuery {
        hide_unsupported,
        ..Default::default()
    };
    compute_filtered_facets(conn, &query, tag_limit)
}

/// Computes cross-linked facet counts for `query` in a single scan: each facet
/// (countries, languages, codecs) applies all other active filters except its own
/// dimension so users can still switch values within that dimension (#2005).
pub fn compute_filtered_facets(
    conn: &Connection,
    query: &RadioDirectoryQuery,
    tag_limit: usize,
) -> Result<RadioDirectoryFacets> {
    let (clauses, bind_params) = build_where_clauses(query, false);
    let sql = format!(
        "SELECT tags, country, countrycode, language, codec, hls
         FROM radio_directory
         WHERE {}",
        clauses.join(" AND ")
    );
    let param_refs: Vec<&dyn ToSql> = bind_params.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i32>(5)? != 0,
        ))
    })?;

    let filter_cc = query
        .countrycode
        .as_deref()
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty());
    let filter_lang = query
        .language
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    let filter_codec = query
        .codec
        .as_deref()
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty());

    let mut tag_counts: HashMap<String, i64> = HashMap::new();
    let mut country_counts: HashMap<String, (String, i64)> = HashMap::new();
    let mut language_counts: HashMap<String, i64> = HashMap::new();
    let mut codec_counts: HashMap<String, i64> = HashMap::new();

    for row in rows {
        let (tags, country, countrycode, language, codec, hls) = row?;
        if query.hide_unsupported && !can_play_stream(&codec, hls) {
            continue;
        }

        let matches_cc = filter_cc.as_ref().is_none_or(|cc| {
            countrycode.eq_ignore_ascii_case(cc) || country.eq_ignore_ascii_case(cc)
        });
        let matches_lang = filter_lang
            .as_ref()
            .is_none_or(|lang| language.split(',').any(|l| l.eq_ignore_ascii_case(lang)));
        let matches_codec = filter_codec
            .as_ref()
            .is_none_or(|c| codec.eq_ignore_ascii_case(c));

        if matches_cc && matches_lang && !codec.is_empty() {
            *codec_counts.entry(codec).or_default() += 1;
        }

        if matches_lang && matches_codec && (!countrycode.is_empty() || !country.is_empty()) {
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

        if matches_cc && matches_codec && !language.is_empty() {
            for lang in language.split(',').filter(|l| !l.is_empty()) {
                *language_counts.entry(lang.to_string()).or_default() += 1;
            }
        }

        if matches_cc && matches_lang && matches_codec && !tags.is_empty() {
            for tag in tags.split(',').filter(|t| !t.is_empty()) {
                *tag_counts.entry(tag.to_string()).or_default() += 1;
            }
        }
    }

    let mut tags = into_sorted_facets(tag_counts, tag_limit);
    let mut countries: Vec<RadioBrowserFacetEntry> = country_counts
        .into_iter()
        .map(|(code, (name, stationcount))| RadioBrowserFacetEntry {
            name: if name.is_empty() { code.clone() } else { name },
            code: Some(code),
            stationcount,
        })
        .collect();
    sort_facet_entries(&mut countries);

    let languages = into_sorted_facets(language_counts, 0);
    let codecs = into_sorted_facets(codec_counts, 0);

    Ok(RadioDirectoryFacets {
        tags: std::mem::take(&mut tags),
        countries,
        languages,
        codecs,
    })
}

fn sort_facet_entries(entries: &mut [RadioBrowserFacetEntry]) {
    entries.sort_by(|a, b| {
        b.stationcount
            .cmp(&a.stationcount)
            .then_with(|| a.name.cmp(&b.name))
    });
}

fn into_sorted_facets(counts: HashMap<String, i64>, limit: usize) -> Vec<RadioBrowserFacetEntry> {
    let mut entries: Vec<RadioBrowserFacetEntry> = counts
        .into_iter()
        .map(|(name, stationcount)| RadioBrowserFacetEntry {
            name,
            code: None,
            stationcount,
        })
        .collect();
    sort_facet_entries(&mut entries);
    if limit > 0 && entries.len() > limit {
        entries.truncate(limit);
    }
    entries
}

/// Default number of recently played/opened unreferenced station rows kept by
/// [`gc_unreferenced_stations`] (#2004).
pub const DEFAULT_RECENT_STATION_TAIL: usize = 20;

/// Canonical `radio://<stationuuid>` path stored on materialized `songs` rows.
pub fn station_uri(stationuuid: &str) -> String {
    format!("{}{}", crate::media_source::RADIO_URI_SCHEME, stationuuid)
}

/// Extracts `<stationuuid>` from a `radio://<stationuuid>` URI.
pub fn parse_station_uuid(location: &str) -> Option<&str> {
    location
        .strip_prefix(crate::media_source::RADIO_URI_SCHEME)
        .map(str::trim)
        .filter(|uuid| !uuid.is_empty())
}

/// Resolves the current `url_resolved` stream URL for `stationuuid` from the
/// local `radio_directory` mirror.
pub fn resolve_station_stream_url(conn: &Connection, stationuuid: &str) -> Result<Option<String>> {
    let url: Option<String> = conn
        .query_row(
            "SELECT NULLIF(TRIM(url_resolved), '') FROM radio_directory WHERE stationuuid = ?1 LIMIT 1",
            params![stationuuid],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(url)
}

/// Materialises a directory station as a `songs` row (`source = RadioBrowser`,
/// `path = radio://<stationuuid>`) so the Queue, playlists, and pins treat it as
/// a normal playable item (#2004). Idempotent: updates station metadata if the
/// row already exists.
pub fn materialize_station(conn: &Connection, stationuuid: &str) -> Result<crate::models::Song> {
    let uri = station_uri(stationuuid);
    let Some(station) = get_station_by_uuid(conn, stationuuid)? else {
        // If a saved/recent station was pruned from `radio_directory` upstream,
        // fall back to its already-materialized `songs` row when present.
        let sql = format!(
            "SELECT {} FROM songs WHERE path = ?1 LIMIT 1",
            crate::collection::SONG_SELECT_COLS
        );
        return conn
            .query_row(&sql, params![uri], crate::collection::row_to_song)
            .map_err(|_| anyhow::anyhow!("Radio station '{stationuuid}' not found in directory"));
    };
    let now = crate::clock::unix_now();
    let bitrate = (station.bitrate > 0).then_some(station.bitrate);
    let genre = (!station.tags.trim().is_empty()).then_some(station.tags.as_str());
    let homepage = (!station.homepage.trim().is_empty()).then_some(station.homepage.as_str());
    let favicon_trimmed = station.favicon.trim();
    let favicon = (!favicon_trimmed.is_empty()
        && !favicon_trimmed.eq_ignore_ascii_case("null")
        && !favicon_trimmed.eq_ignore_ascii_case("undefined"))
    .then_some(favicon_trimmed);
    let artist = (!station.country.trim().is_empty()).then_some(station.country.as_str());

    let existing_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM songs WHERE path = ?1 LIMIT 1",
            params![uri],
            |r| r.get(0),
        )
        .optional()?;

    if let Some(song_id) = existing_id {
        conn.execute(
            "UPDATE songs SET
                source = ?1,
                filetype = ?2,
                url = ?3,
                stream_url = ?4,
                title = ?5,
                artist = ?6,
                genre = ?7,
                bitrate = ?8,
                length_nanosec = NULL,
                art_manual = COALESCE(?9, NULLIF(art_manual, 'null')),
                mtime = ?10,
                unavailable = 0
             WHERE id = ?11",
            params![
                crate::models::SongSource::RADIO_BROWSER_ID,
                crate::models::FileType::Stream as i32,
                homepage,
                station.url_resolved,
                station.name,
                artist,
                genre,
                bitrate,
                favicon,
                now,
                song_id,
            ],
        )?;
    } else {
        conn.execute(
            "INSERT INTO songs (
                source, filetype, path, url, stream_url, title, artist, genre,
                bitrate, length_nanosec, art_manual, mtime, unavailable
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                ?9, NULL, ?10, ?11, 0
             )",
            params![
                crate::models::SongSource::RADIO_BROWSER_ID,
                crate::models::FileType::Stream as i32,
                uri,
                homepage,
                station.url_resolved,
                station.name,
                artist,
                genre,
                bitrate,
                favicon,
                now,
            ],
        )?;
    }

    let sql = format!(
        "SELECT {} FROM songs WHERE path = ?1 LIMIT 1",
        crate::collection::SONG_SELECT_COLS
    );
    let song = conn.query_row(&sql, params![uri], crate::collection::row_to_song)?;
    Ok(song)
}

/// Garbage-collects unreferenced `RadioBrowser` station rows from `songs` (#2004).
/// Keeps any row referenced by `playlist_items` (including the Queue) or
/// `pinned_items`, `protected_song_id` (if active), and the `keep_recent` most
/// recently played/materialized rows.
pub fn gc_unreferenced_stations(
    conn: &Connection,
    keep_recent: usize,
    protected_song_id: Option<i64>,
) -> Result<usize> {
    let deleted = conn.execute(
        "DELETE FROM songs
         WHERE source = ?1
           AND id != COALESCE(?2, -1)
           AND id NOT IN (
               SELECT song_id FROM playlist_items WHERE song_id IS NOT NULL
           )
           AND CAST(id AS TEXT) NOT IN (
               SELECT ref_key FROM pinned_items WHERE item_type = 'song'
           )
           AND path NOT IN (
               SELECT ?4 || stationuuid FROM radio_saved
           )
           AND id NOT IN (
               SELECT id FROM songs
               WHERE source = ?1
               ORDER BY COALESCE(lastplayed, mtime, added, 0) DESC, id DESC
               LIMIT ?3
           )",
        params![
            crate::models::SongSource::RADIO_BROWSER_ID,
            protected_song_id,
            keep_recent as i64,
            crate::media_source::RADIO_URI_SCHEME,
        ],
    )?;
    Ok(deleted)
}

/// Saves a station in `radio_saved` (idempotent) and materializes its fallback
/// `songs` row if present in `radio_directory` so it survives directory prunes (#2005).
pub fn save_station(conn: &Connection, stationuuid: &str) -> Result<()> {
    let uuid = stationuuid.trim();
    if uuid.is_empty() {
        return Ok(());
    }
    let now = crate::clock::unix_now();
    conn.execute(
        "INSERT OR IGNORE INTO radio_saved (stationuuid, saved_at) VALUES (?1, ?2)",
        params![uuid, now],
    )?;
    let _ = materialize_station(conn, uuid);
    Ok(())
}

/// Removes a station from `radio_saved` (idempotent) (#2005).
pub fn unsave_station(conn: &Connection, stationuuid: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM radio_saved WHERE stationuuid = ?1",
        params![stationuuid.trim()],
    )?;
    Ok(())
}

fn station_from_joined_row(
    row: &rusqlite::Row<'_>,
    stationuuid: String,
    require_song_fallback: bool,
) -> rusqlite::Result<Option<RadioDirectoryStation>> {
    let dir_uuid: Option<String> = row.get(1)?;
    if let Some(dir_uuid) = dir_uuid {
        let codec: String = row.get(10)?;
        let hls: bool = row.get::<_, i32>(12)? != 0;
        let playable = can_play_stream(&codec, hls);
        return Ok(Some(RadioDirectoryStation {
            stationuuid: dir_uuid,
            name: row.get(2)?,
            url_resolved: row.get(3)?,
            homepage: row.get(4)?,
            favicon: row.get(5)?,
            tags: row.get(6)?,
            country: row.get(7)?,
            countrycode: row.get(8)?,
            language: row.get(9)?,
            codec,
            bitrate: row.get(11)?,
            hls,
            votes: row.get(13)?,
            clickcount: row.get(14)?,
            lastcheckok: row.get::<_, i32>(15)? != 0,
            changeuuid: row.get(16)?,
            updated_at: row.get(17)?,
            playable,
        }));
    }

    let song_title: Option<String> = row.get(18)?;
    let song_stream_url: Option<String> = row.get(19)?;
    if require_song_fallback && song_title.is_none() && song_stream_url.is_none() {
        return Ok(None);
    }

    Ok(Some(RadioDirectoryStation {
        stationuuid,
        name: song_title.unwrap_or_default(),
        url_resolved: song_stream_url.unwrap_or_default(),
        homepage: row.get::<_, Option<String>>(20)?.unwrap_or_default(),
        favicon: row.get::<_, Option<String>>(21)?.unwrap_or_default(),
        tags: row.get::<_, Option<String>>(22)?.unwrap_or_default(),
        country: row.get::<_, Option<String>>(23)?.unwrap_or_default(),
        countrycode: String::new(),
        language: String::new(),
        codec: String::new(),
        bitrate: row.get::<_, Option<i32>>(24)?.unwrap_or(0),
        hls: false,
        votes: 0,
        clickcount: 0,
        lastcheckok: true,
        changeuuid: String::new(),
        updated_at: row.get::<_, Option<i64>>(25)?.unwrap_or(0),
        playable: true,
    }))
}

const JOINED_STATION_COLS: &str = "
    rd.stationuuid, rd.name, rd.url_resolved, rd.homepage, rd.favicon,
    rd.tags, rd.country, rd.countrycode, rd.language, rd.codec,
    rd.bitrate, rd.hls, rd.votes, rd.clickcount, rd.lastcheckok,
    rd.changeuuid, rd.updated_at,
    s.title, s.stream_url, s.url, s.art_manual, s.genre, s.artist,
    s.bitrate, COALESCE(s.lastplayed, s.mtime, s.added, 0)
";

fn collect_joined_stations<I>(rows: I, hide_unsupported: bool) -> Result<Vec<RadioDirectoryStation>>
where
    I: Iterator<Item = rusqlite::Result<Option<RadioDirectoryStation>>>,
{
    let mut stations = Vec::new();
    for item in rows {
        if let Some(st) = item? {
            if !hide_unsupported || st.playable {
                stations.push(st);
            }
        }
    }
    Ok(stations)
}

/// Lists user-saved stations ordered by `saved_at DESC` (#2005).
/// Prefers live `radio_directory` metadata and falls back to materialized `songs`
/// metadata if a saved station was pruned upstream.
pub fn list_saved_stations(
    conn: &Connection,
    hide_unsupported: bool,
) -> Result<Vec<RadioDirectoryStation>> {
    let sql = format!(
        "SELECT rs.stationuuid, {JOINED_STATION_COLS}
         FROM radio_saved rs
         LEFT JOIN radio_directory rd ON rd.stationuuid = rs.stationuuid
         LEFT JOIN songs s ON s.path = (?1 || rs.stationuuid)
         ORDER BY rs.saved_at DESC, rs.stationuuid ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![crate::media_source::RADIO_URI_SCHEME], |row| {
        let saved_uuid: String = row.get(0)?;
        station_from_joined_row(row, saved_uuid, true)
    })?;
    collect_joined_stations(rows, hide_unsupported)
}

/// Lists recently played/opened radio stations ordered most-recent first (#2005).
pub fn list_recent_stations(
    conn: &Connection,
    limit: usize,
    hide_unsupported: bool,
) -> Result<Vec<RadioDirectoryStation>> {
    let limit = limit.clamp(1, 200);
    let prefix_len = crate::media_source::RADIO_URI_SCHEME.len() as i64;
    let sql = format!(
        "SELECT SUBSTR(s.path, ?1 + 1) AS stationuuid, {JOINED_STATION_COLS}
         FROM songs s
         LEFT JOIN radio_directory rd ON rd.stationuuid = SUBSTR(s.path, ?1 + 1)
         WHERE s.source = ?2
           AND s.path LIKE (?3 || '%')
         ORDER BY COALESCE(s.lastplayed, s.mtime, s.added, 0) DESC, s.id DESC
         LIMIT ?4"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![
            prefix_len,
            crate::models::SongSource::RADIO_BROWSER_ID,
            crate::media_source::RADIO_URI_SCHEME,
            limit as i64
        ],
        |row| {
            let uuid: String = row.get(0)?;
            station_from_joined_row(row, uuid, false)
        },
    )?;
    collect_joined_stations(rows, hide_unsupported)
}

/// Returns IDs of playlists that contain at least one `RadioBrowser` station (#2005).
pub fn list_radio_playlist_ids(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT pi.playlist_id
         FROM playlist_items pi
         JOIN songs s ON s.id = pi.song_id
         WHERE s.source = ?1
         ORDER BY pi.playlist_id ASC",
    )?;
    let ids = stmt
        .query_map(params![crate::models::SongSource::RADIO_BROWSER_ID], |r| {
            r.get(0)
        })?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
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

    /// #2199: a full page with a row that normalisation drops is still a full page, so the
    /// next page is fetched, at the raw offset.
    #[tokio::test]
    async fn full_sync_keeps_paging_when_normalisation_drops_a_row() {
        let (_dir, db) = test_db();
        let server = MockServer::start().await;

        let station = |uuid: &str, chg: &str| {
            sample_station_json(
                uuid,
                chg,
                uuid,
                "pop",
                "Canada",
                "CA",
                "english",
                "MP3",
                128,
                0,
                1,
                1,
                "2026-11-10T10:00:00Z",
            )
        };
        let mut urlless = station("s-no-url", "chg-2");
        urlless["url_resolved"] = serde_json::json!("");

        Mock::given(method("GET"))
            .and(path("/json/stations"))
            .and(query_param("offset", "0"))
            .and(query_param("limit", "2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([station("s-a", "chg-1"), urlless])),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/json/stations"))
            .and(query_param("offset", "2"))
            .and(query_param("limit", "2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([station("s-b", "chg-3")])),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = RadioBrowserClient::with_servers(vec![server.uri()], RADIO_BROWSER_USER_AGENT);
        let stats = sync_with_client(&db, &client, 2, 100, 1_700_000_000, |_| {})
            .await
            .expect("full sync succeeds");

        assert_eq!(stats.mode, "full");
        assert_eq!(stats.upserted, 2);
        assert_eq!(stats.total_stations, 2);
        let conn = db.pool.get().unwrap();
        assert!(get_station_by_uuid(&conn, "s-b").unwrap().is_some());
        drop(conn);
        server.verify().await;
    }

    /// #2199: a mirror whose change cursor was dropped by migration 94 gets a full resync,
    /// even though it was synced recently.
    #[test]
    fn recent_mirror_without_change_cursor_gets_full_resync() {
        let meta = DirectoryMetaRow {
            opted_in: true,
            last_synced_at: Some(1_700_000_000),
            last_full_sync_at: Some(1_700_000_000),
            last_change_uuid: None,
            last_error: None,
        };
        assert_eq!(
            choose_sync_plan(&meta, 1_700_000_100, 10_017),
            SyncPlan::Full
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

    #[tokio::test]
    async fn falls_back_to_full_resync_when_server_ignores_unknown_change_uuid() {
        let (_dir, db) = test_db();
        let server = MockServer::start().await;

        // Initial sync at 2026-11-10T10:00:00Z (1794304800)
        let t0 = "2026-11-10T10:00:00Z"
            .parse::<jiff::Timestamp>()
            .unwrap()
            .as_second();
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
                    10,
                    1,
                    "2026-11-10T10:00:00Z"
                )
            ])))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let client = RadioBrowserClient::with_servers(vec![server.uri()], RADIO_BROWSER_USER_AGENT);
        sync_with_client(&db, &client, 10, 100, t0, |_| {})
            .await
            .unwrap();

        // 2 days later, we attempt incremental sync with `lastchangeuuid=chg-1`,
        // but the server had a DB restore and doesn't recognize `chg-1`, so it dumps
        // changes starting from 20 days earlier ("2026-10-20T00:00:00Z").
        Mock::given(method("GET"))
            .and(path("/json/stations/changed"))
            .and(query_param("lastchangeuuid", "chg-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "changeuuid": "chg-ancient",
                    "stationuuid": "s-1",
                    "name": "Station One Ancient",
                    "lastchangetime_iso8601": "2026-10-20T00:00:00Z"
                }
            ])))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/json/stations/byuuid"))
            .and(query_param("uuids", "s-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                sample_station_json(
                    "s-1",
                    "chg-ancient",
                    "Station One Ancient",
                    "rock",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    128,
                    0,
                    10,
                    1,
                    "2026-10-20T00:00:00Z"
                )
            ])))
            .mount(&server)
            .await;

        // Full resync fallback endpoint
        Mock::given(method("GET"))
            .and(path("/json/stations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                sample_station_json(
                    "s-2",
                    "chg-2",
                    "Station Two Rebuilt",
                    "jazz",
                    "Canada",
                    "CA",
                    "english",
                    "MP3",
                    256,
                    0,
                    50,
                    1,
                    "2026-11-12T10:00:00Z"
                )
            ])))
            .mount(&server)
            .await;

        let t1 = t0 + 2 * 86_400;
        let stats = sync_with_client(&db, &client, 10, 100, t1, |_| {})
            .await
            .expect("falls back to full resync when server dumps old change history");

        assert_eq!(stats.mode, "full");
        assert_eq!(stats.total_stations, 1);
        let conn = db.pool.get().unwrap();
        assert!(get_station_by_uuid(&conn, "s-1").unwrap().is_none());
        assert!(get_station_by_uuid(&conn, "s-2").unwrap().is_some());
    }

    #[test]
    fn materialize_station_is_idempotent_and_gc_preserves_refs_and_tail() {
        let (_dir, db) = test_db();
        let mut conn = db.pool.get().unwrap();

        let stations: Vec<RadioBrowserStation> = (1..=5)
            .map(|i| RadioBrowserStation {
                stationuuid: format!("uuid-{i}"),
                changeuuid: format!("c-{i}"),
                name: format!("Station {i}"),
                url_resolved: format!("https://resolved.example/{i}.mp3"),
                homepage: "https://example.org".into(),
                favicon: String::new(),
                tags: "ambient,chill".into(),
                country: "Canada".into(),
                countrycode: "CA".into(),
                language: "english".into(),
                codec: "MP3".into(),
                bitrate: 192,
                hls: false,
                votes: 10,
                clickcount: 20,
                lastcheckok: true,
                lastchangetime_iso8601: None,
            })
            .collect();
        let tx = conn.transaction().unwrap();
        upsert_stations_tx(&tx, &stations, 1000).unwrap();
        tx.commit().unwrap();

        let s1 = materialize_station(&conn, "uuid-1").unwrap();
        assert_eq!(s1.source, crate::models::SongSource::RadioBrowser);
        assert_eq!(s1.filetype, crate::models::FileType::Stream);
        assert_eq!(s1.path.as_deref(), Some("radio://uuid-1"));
        assert_eq!(s1.length_nanosec, None);

        // Idempotent re-materialize returns the same row id.
        let s1_again = materialize_station(&conn, "uuid-1").unwrap();
        assert_eq!(s1_again.id, s1.id);

        let s2 = materialize_station(&conn, "uuid-2").unwrap();
        let s3 = materialize_station(&conn, "uuid-3").unwrap();
        let s4 = materialize_station(&conn, "uuid-4").unwrap();
        let s5 = materialize_station(&conn, "uuid-5").unwrap();

        // Reference s1 via a playlist, and s2 via pinned_items.
        conn.execute(
            "INSERT INTO playlists (id, name, dynamic_enabled) VALUES (10, 'Radio Favs', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO playlist_items (playlist_id, position, uuid, type, song_id) VALUES (10, 0, 'item-1', 0, ?1)",
            params![s1.id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO pinned_items (item_type, ref_key, position, pinned_at) VALUES ('song', ?1, 0, 1000)",
            params![s2.id.to_string()],
        )
        .unwrap();

        // Keep tail of 1 most recent (s5) + protected s3; s4 should be GC'd, while s1, s2, s3, s5 survive.
        let deleted = gc_unreferenced_stations(&conn, 1, Some(s3.id)).unwrap();
        assert_eq!(deleted, 1);

        let surviving_ids: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT id FROM songs WHERE source = ?1 ORDER BY id")
                .unwrap();
            stmt.query_map(params![crate::models::SongSource::RADIO_BROWSER_ID], |r| {
                r.get(0)
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
        };
        assert_eq!(surviving_ids, vec![s1.id, s2.id, s3.id, s5.id]);
        let _ = s4;
    }

    #[test]
    fn save_unsave_and_recent_stations_round_trip_with_pruned_fallback() {
        let (_dir, db) = test_db();
        let mut conn = db.pool.get().unwrap();

        let stations = vec![
            RadioBrowserStation {
                stationuuid: "uuid-a".into(),
                changeuuid: "c-a".into(),
                name: "Station A".into(),
                url_resolved: "https://stream.example/uuid-a.mp3".into(),
                homepage: "https://example.com/uuid-a".into(),
                favicon: String::new(),
                tags: "jazz".into(),
                country: "Canada".into(),
                countrycode: "CA".into(),
                language: "english".into(),
                codec: "MP3".into(),
                bitrate: 128,
                hls: false,
                votes: 10,
                clickcount: 50,
                lastcheckok: true,
                lastchangetime_iso8601: Some("2026-06-01T00:00:00Z".into()),
            },
            RadioBrowserStation {
                stationuuid: "uuid-b".into(),
                changeuuid: "c-b".into(),
                name: "Station B".into(),
                url_resolved: "https://stream.example/uuid-b.aac".into(),
                homepage: "https://example.com/uuid-b".into(),
                favicon: String::new(),
                tags: "rock".into(),
                country: "Germany".into(),
                countrycode: "DE".into(),
                language: "german".into(),
                codec: "AAC".into(),
                bitrate: 192,
                hls: true, // HLS -> unsupported
                votes: 10,
                clickcount: 30,
                lastcheckok: true,
                lastchangetime_iso8601: Some("2026-06-01T00:00:00Z".into()),
            },
        ];
        let tx = conn.transaction().unwrap();
        upsert_stations_tx(&tx, &stations, 1000).unwrap();
        tx.commit().unwrap();

        // Save both (idempotent), verify hide_unsupported filtering.
        save_station(&conn, "uuid-a").unwrap();
        save_station(&conn, "uuid-a").unwrap();
        save_station(&conn, "uuid-b").unwrap();

        let playable_saved = list_saved_stations(&conn, true).unwrap();
        assert_eq!(playable_saved.len(), 1);
        assert_eq!(playable_saved[0].stationuuid, "uuid-a");

        let all_saved = list_saved_stations(&conn, false).unwrap();
        assert_eq!(all_saved.len(), 2);

        // Recent stations includes materialized stations.
        let recent = list_recent_stations(&conn, 10, true).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].stationuuid, "uuid-a");

        // Country facet -> query_stations round-trip
        let facets = compute_local_facets(&conn, true, 50).unwrap();
        let canada = facets
            .countries
            .iter()
            .find(|c| c.name == "Canada")
            .unwrap();
        assert_eq!(canada.code.as_deref(), Some("CA"));
        let ca_stations = query_stations(
            &conn,
            &RadioDirectoryQuery {
                countrycode: canada.code.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ca_stations.len(), 1);
        assert_eq!(ca_stations[0].stationuuid, "uuid-a");

        // Cross-linked facets: filtering by CA narrows languages to CA's languages while keeping all countries selectable.
        let ca_facets = compute_filtered_facets(
            &conn,
            &RadioDirectoryQuery {
                countrycode: Some("CA".into()),
                hide_unsupported: false,
                ..Default::default()
            },
            50,
        )
        .unwrap();
        assert_eq!(ca_facets.languages.len(), 1);
        assert_eq!(ca_facets.languages[0].name, "english");
        assert_eq!(ca_facets.countries.len(), 2);

        // Prune uuid-a from radio_directory; saved + materialize still resolve via songs fallback,
        // and GC keeps uuid-a because it is saved.
        conn.execute(
            "DELETE FROM radio_directory WHERE stationuuid = 'uuid-a'",
            [],
        )
        .unwrap();
        let deleted = gc_unreferenced_stations(&conn, 0, None).unwrap();
        assert_eq!(deleted, 0);

        let fallback_saved = list_saved_stations(&conn, true).unwrap();
        assert_eq!(fallback_saved.len(), 1);
        assert_eq!(fallback_saved[0].name, "Station A");

        let rematerialized = materialize_station(&conn, "uuid-a").unwrap();
        assert_eq!(rematerialized.title.as_deref(), Some("Station A"));

        unsave_station(&conn, "uuid-a").unwrap();
        unsave_station(&conn, "uuid-a").unwrap();
        unsave_station(&conn, "uuid-b").unwrap();
        assert!(list_saved_stations(&conn, false).unwrap().is_empty());

        // list_radio_playlist_ids executes cleanly against the real playlists schema.
        let pl_ids = list_radio_playlist_ids(&conn).unwrap();
        assert!(pl_ids.is_empty());
    }

    fn search_station(
        uuid: &str,
        name: &str,
        tags: &str,
        codec: &str,
        clickcount: i32,
    ) -> RadioBrowserStation {
        RadioBrowserStation {
            stationuuid: uuid.into(),
            changeuuid: format!("c-{uuid}"),
            name: name.into(),
            url_resolved: format!("https://resolved.example/{uuid}.mp3"),
            homepage: String::new(),
            favicon: String::new(),
            tags: tags.into(),
            country: "Canada".into(),
            countrycode: "CA".into(),
            language: "english".into(),
            codec: codec.into(),
            bitrate: 128,
            hls: false,
            votes: 0,
            clickcount,
            lastcheckok: true,
            lastchangetime_iso8601: None,
        }
    }

    fn seed_search_directory(conn: &mut Connection, stations: &[RadioBrowserStation]) {
        let tx = conn.transaction().unwrap();
        upsert_stations_tx(&tx, stations, 1000).unwrap();
        tx.execute(
            "INSERT INTO radio_directory_meta (id, opted_in) VALUES (1, 1)
             ON CONFLICT(id) DO UPDATE SET opted_in = 1",
            [],
        )
        .unwrap();
        tx.commit().unwrap();
    }

    #[test]
    fn search_stations_ranks_name_matches_before_tag_matches_then_popularity() {
        let (_dir, db) = test_db();
        let mut conn = db.pool.get().unwrap();
        seed_search_directory(
            &mut conn,
            &[
                // Tag-only match, by far the most popular: still ranks below name matches.
                search_station("tag-only", "Smooth Lounge", "jazz,lounge", "MP3", 9000),
                search_station("name-small", "Jazz Corner", "", "MP3", 5),
                search_station("name-big", "Radio Jazzy Nights", "", "MP3", 400),
                search_station("no-match", "Rock Hits", "rock", "MP3", 10_000),
            ],
        );

        let uuids: Vec<String> = search_stations(&conn, "jazz", 10)
            .unwrap()
            .into_iter()
            .map(|s| s.stationuuid)
            .collect();
        assert_eq!(uuids, vec!["name-big", "name-small", "tag-only"]);

        // Every token must match; country and language are searchable too.
        let multi = search_stations(&conn, "jaz canad", 10).unwrap();
        assert_eq!(multi.len(), 3);
        assert!(search_stations(&conn, "jazz germany", 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn search_stations_caps_results_and_counts_only_playable_stations() {
        let (_dir, db) = test_db();
        let mut conn = db.pool.get().unwrap();
        let mut stations: Vec<RadioBrowserStation> = (0..30)
            .map(|i| search_station(&format!("ok-{i}"), &format!("Ambient {i}"), "", "MP3", i))
            .collect();
        // Unplayable rows (no known codec) are the most popular, so they'd fill the cap
        // if the playable filter ran after it.
        stations.extend((0..5).map(|i| {
            search_station(
                &format!("nocodec-{i}"),
                &format!("Ambient X {i}"),
                "",
                "",
                99_999,
            )
        }));
        seed_search_directory(&mut conn, &stations);

        let five = search_stations(&conn, "ambient", 5).unwrap();
        assert_eq!(five.len(), 5);
        assert!(five.iter().all(|s| s.playable));
        assert_eq!(five[0].stationuuid, "ok-29");

        // The backend clamps whatever the caller asks for.
        let capped = search_stations(&conn, "ambient", 1_000).unwrap();
        assert_eq!(capped.len(), MAX_STATION_SEARCH_RESULTS);
        assert_eq!(search_stations(&conn, "ambient", 0).unwrap().len(), 1);
    }

    #[test]
    fn search_stations_reads_the_local_mirror_offline_and_is_empty_when_not_opted_in() {
        let (_dir, db) = test_db();
        let mut conn = db.pool.get().unwrap();
        let stations = [search_station("s1", "Folk Roots", "folk", "MP3", 1)];
        let tx = conn.transaction().unwrap();
        upsert_stations_tx(&tx, &stations, 1000).unwrap();
        tx.commit().unwrap();

        // Rows present but not opted in: no results, no error.
        assert!(search_stations(&conn, "folk", 5).unwrap().is_empty());

        conn.execute(
            "INSERT INTO radio_directory_meta (id, opted_in) VALUES (1, 1)
             ON CONFLICT(id) DO UPDATE SET opted_in = 1",
            [],
        )
        .unwrap();
        // Online lookups off: the search never touches the network, so it still answers.
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('context_enrichment_enabled', 'false')",
            [],
        )
        .unwrap();
        assert!(!crate::commands::context::is_online_enabled(&conn));
        let found = search_stations(&conn, "folk", 5).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].stationuuid, "s1");

        // Punctuation-only input has no searchable words.
        assert!(search_stations(&conn, ":*\"", 5).unwrap().is_empty());
    }
}
