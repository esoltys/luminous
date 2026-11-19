//! Dynamic/Smart Playlist domain — population-mode bias, spec dispatch
//! (decade:/bpmrange:/tag:/smart-rule), populate, and library-change
//! reconciliation. Split out of `playlist.rs` (#577 item 18) — a second
//! `impl PlaylistManager` block alongside the one in `playlist.rs` that owns
//! CRUD/item-mutation/undo logic.

use super::{PlaylistManager, NO_SONG_LIMIT};
use crate::collection::CollectionScanner;
use crate::db::Database;
use crate::models::{PlaylistItem, QueuePopulationMode, Song};
use crate::tags::TagManager;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

/// Size of the random-fill fallback for a Daypart Mix (#223) whose picked
/// genre grouping doesn't clear the auto-playlist minimum — a bounded
/// "shuffle across the library" sample rather than every song at once.
const DAYPART_RANDOM_FILL_LIMIT: i64 = 200;

/// Default and maximum station count for a `radio:` station list (#2006).
pub const DEFAULT_RADIO_LIST_LIMIT: usize = 100;
const MAX_RADIO_LIST_LIMIT: usize = 500;

/// Structured representation of a dynamic radio station list spec
/// (`radio:tag=jazz,folk metal;country=CA;lang=english;minbitrate=96;order=clickcount;limit=100`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadioStationListSpec {
    #[serde(default)]
    pub tags: Vec<String>,
    pub country: Option<String>,
    pub lang: Option<String>,
    pub min_bitrate: Option<i32>,
    #[serde(default)]
    pub order: crate::radio_directory::RadioDirectoryOrder,
    #[serde(default = "default_radio_list_limit")]
    pub limit: usize,
}

fn default_radio_list_limit() -> usize {
    DEFAULT_RADIO_LIST_LIMIT
}

impl Default for RadioStationListSpec {
    fn default() -> Self {
        Self {
            tags: Vec::new(),
            country: None,
            lang: None,
            min_bitrate: None,
            order: crate::radio_directory::RadioDirectoryOrder::Clickcount,
            limit: DEFAULT_RADIO_LIST_LIMIT,
        }
    }
}

impl RadioStationListSpec {
    /// Parses a `radio:...` dynamic spec string. Returns `None` if `spec` does
    /// not start with the `radio:` prefix.
    pub fn parse(spec: &str) -> Option<Self> {
        let body = spec.trim().strip_prefix("radio:")?;
        let mut parsed = Self::default();
        for part in body.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let Some((raw_key, raw_val)) = part.split_once('=') else {
                continue;
            };
            let key = raw_key.trim().to_ascii_lowercase();
            let val = raw_val.trim();
            match key.as_str() {
                "tag" | "tags" => {
                    let mut seen = std::collections::HashSet::new();
                    parsed.tags = val
                        .split(',')
                        .map(str::trim)
                        .filter(|t| !t.is_empty() && seen.insert(t.to_lowercase()))
                        .map(ToString::to_string)
                        .collect();
                }
                "country" | "countrycode" => {
                    let upper = val.to_ascii_uppercase();
                    parsed.country = (!upper.is_empty()).then_some(upper);
                }
                "lang" | "language" => {
                    let lower = val.to_lowercase();
                    parsed.lang = (!lower.is_empty()).then_some(lower);
                }
                "minbitrate" | "min_bitrate" => {
                    parsed.min_bitrate = val.parse::<i32>().ok().filter(|&b| b > 0);
                }
                "order" => {
                    use crate::radio_directory::RadioDirectoryOrder;
                    parsed.order = match val.to_ascii_lowercase().as_str() {
                        "votes" => RadioDirectoryOrder::Votes,
                        "name" => RadioDirectoryOrder::Name,
                        "bitrate" => RadioDirectoryOrder::Bitrate,
                        _ => RadioDirectoryOrder::Clickcount,
                    };
                }
                "limit" => {
                    if let Ok(n) = val.parse::<usize>() {
                        parsed.limit = n.clamp(1, MAX_RADIO_LIST_LIMIT);
                    }
                }
                _ => {}
            }
        }
        Some(parsed)
    }

    /// Formats this spec into its canonical `radio:...` string.
    pub fn to_spec_string(&self) -> String {
        use crate::radio_directory::RadioDirectoryOrder;
        let mut parts = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let clean_tags: Vec<&str> = self
            .tags
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty() && seen.insert(t.to_lowercase()))
            .collect();
        if !clean_tags.is_empty() {
            parts.push(format!("tag={}", clean_tags.join(",")));
        }
        if let Some(ref cc) = self.country {
            let clean = cc.trim().to_ascii_uppercase();
            if !clean.is_empty() {
                parts.push(format!("country={clean}"));
            }
        }
        if let Some(ref lang) = self.lang {
            let clean = lang.trim().to_lowercase();
            if !clean.is_empty() {
                parts.push(format!("lang={clean}"));
            }
        }
        if let Some(br) = self.min_bitrate.filter(|&b| b > 0) {
            parts.push(format!("minbitrate={br}"));
        }
        let order_str = match self.order {
            RadioDirectoryOrder::Votes => "votes",
            RadioDirectoryOrder::Name => "name",
            RadioDirectoryOrder::Bitrate => "bitrate",
            RadioDirectoryOrder::Clickcount | RadioDirectoryOrder::Relevance => "clickcount",
        };
        parts.push(format!("order={order_str}"));
        parts.push(format!(
            "limit={}",
            self.limit.clamp(1, MAX_RADIO_LIST_LIMIT)
        ));
        format!("radio:{}", parts.join(";"))
    }
}

/// Resolves a `radio:` station list spec against `radio_directory` (expanding
/// followed tags through `GenreAliases::members_of`) and materializes matched
/// stations as `songs` rows (#2006).
fn resolve_radio_spec(db: &Arc<Database>, spec: &RadioStationListSpec) -> Result<Vec<Song>> {
    let conn = db.pool.get()?;
    let aliases = crate::genre_aliases::GenreAliases::load(&conn)?;
    let mut expanded_tags = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for tag in &spec.tags {
        for member in aliases.members_of(tag) {
            if !member.is_empty() && seen.insert(member.clone()) {
                expanded_tags.push(member);
            }
        }
    }

    let query = crate::radio_directory::RadioDirectoryQuery {
        tags: expanded_tags,
        countrycode: spec.country.clone(),
        language: spec.lang.clone(),
        min_bitrate: spec.min_bitrate,
        hide_unsupported: true,
        order_by: spec.order,
        offset: 0,
        limit: spec.limit.clamp(1, MAX_RADIO_LIST_LIMIT),
        ..Default::default()
    };
    let stations = crate::radio_directory::query_stations(&conn, &query)?;
    let mut songs = Vec::with_capacity(stations.len());
    for station in stations {
        songs.push(crate::radio_directory::materialize_station(
            &conn,
            &station.stationuuid,
        )?);
    }
    Ok(songs)
}

/// What a reconcile pass changed in one dynamic playlist.
#[derive(Debug)]
pub struct DynamicPlaylistDelta {
    pub playlist_id: i64,
    /// Freshly inserted rows, carrying their real DB UUIDs.
    pub added: Vec<PlaylistItem>,
    pub removed_uuids: Vec<String>,
}

/// One dynamic playlist's current matches, resolved without the playlists
/// lock — the read half of a reconcile pass (see [`match_dynamic_playlists`]).
#[derive(Debug)]
pub struct DynamicPlaylistMatch {
    playlist_id: i64,
    /// The spec the match was resolved from, so the write half can skip a
    /// playlist whose definition changed in between.
    spec: String,
    /// Matching song ids, in population-mode order.
    song_ids: Vec<i64>,
}

/// The songs a dynamic spec resolves to, and whether they're its definition
/// or just a stand-in sample.
struct SpecSongs {
    songs: Vec<Song>,
    /// The spec has no membership definition to hold the playlist to — a
    /// Daypart Mix's random-fill fallback draws a fresh sample each call.
    random_sample: bool,
}

/// Set while a reconcile pass runs; a request arriving meanwhile sets
/// [`RECONCILE_REQUESTED`] instead of starting a second, overlapping pass.
static RECONCILE_RUNNING: AtomicBool = AtomicBool::new(false);
static RECONCILE_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Runs a reconcile pass and mirrors the outcome into the running app: a
/// currently-playing playlist gets new items appended to the live queue
/// (evicted rows removed — the playing item itself is protected by the
/// player), and the frontend learns which playlists changed. Spawned from
/// the `library-changed` / `song-stats-changed` listeners in lib.rs, so
/// "everything is immediate" without any per-call-site wiring.
///
/// Those events arrive in bursts (a scan, a run of stat updates), so calls
/// coalesce: while a pass runs, any number of new requests queue exactly one
/// follow-up pass, which sees every change they announced.
pub async fn reconcile_and_sync(app: tauri::AppHandle) {
    RECONCILE_REQUESTED.store(true, Ordering::Release);
    loop {
        if RECONCILE_RUNNING.swap(true, Ordering::AcqRel) {
            return; // the running pass picks the request up
        }
        while RECONCILE_REQUESTED.swap(false, Ordering::AcqRel) {
            reconcile_pass(&app).await;
        }
        RECONCILE_RUNNING.store(false, Ordering::Release);
        // A request that landed between the last swap and the store above
        // would otherwise wait for the next event.
        if !RECONCILE_REQUESTED.load(Ordering::Acquire) {
            return;
        }
    }
}

/// Reconciles once at startup and again whenever the local date changes, for
/// the rules whose answer moves with the clock: "played in the last 30 days"
/// gains and loses songs at midnight with no library event to announce it.
/// Polls the date rather than sleeping until midnight so a suspended machine
/// catches up on wake. Runs for the life of the app.
pub async fn reconcile_each_day(app: tauri::AppHandle) {
    // Stagger startup daily reconciliation to allow window initialization and initial playback (#1807).
    tokio::time::sleep(crate::playlist::STARTUP_RECONCILE_DELAY).await;
    let mut last_reconciled = None;
    loop {
        let today = crate::clock::local_now().date();
        if last_reconciled != Some(today) {
            last_reconciled = Some(today);
            reconcile_and_sync(app.clone()).await;
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

async fn reconcile_pass(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};

    let state = app.state::<crate::AppState>();
    // Resolving every spec is the expensive half — a full-library query per
    // dynamic playlist — and only reads, so it runs on the blocking pool
    // without the playlists lock; playlist IPC stays responsive meanwhile.
    let db = state.db.clone();
    let matches = match tokio::task::spawn_blocking(move || match_dynamic_playlists(&db)).await {
        Ok(Ok(matches)) => matches,
        Ok(Err(e)) => {
            log::error!("Dynamic playlist reconcile failed: {e}");
            return;
        }
        Err(e) => {
            log::error!("Dynamic playlist reconcile task failed: {e}");
            return;
        }
    };
    // Apply dynamic matches one playlist at a time via `with_playlists`, releasing
    // the lock and yielding between matches (#1807). This ensures user playback
    // commands (`replace_queue`, `play_song`) never queue behind a monolithic lock hold.
    let mut deltas = Vec::new();
    for m in matches {
        let delta_res =
            crate::playlist::with_playlists(&state.playlists, move |pm| pm.apply_dynamic_match(&m))
                .await;
        match delta_res {
            Ok(Some(delta)) => deltas.push(delta),
            Ok(None) => {}
            Err(e) => log::error!("Dynamic playlist reconcile failed: {e}"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    if deltas.is_empty() {
        return;
    }

    {
        let mut player = state.player.lock().await;
        for delta in &deltas {
            if player.current_playlist_id != Some(delta.playlist_id) {
                continue;
            }
            if !delta.removed_uuids.is_empty() {
                player.remove_songs_from_playlist_items(&delta.removed_uuids);
            }
            if !delta.added.is_empty() {
                player.append_songs_to_playlist_items(delta.added.clone());
            }
            let playback_state = player.get_state().await;
            let _ = app.emit("playback-state", playback_state);
        }
    }

    let changed_ids: Vec<i64> = deltas.iter().map(|d| d.playlist_id).collect();
    let _ = app.emit("playlists-changed", changed_ids);
}

/// The read half of a reconcile pass: every dynamic playlist's current
/// matches. Needs only the database, not the `PlaylistManager`, so callers
/// can run it without holding the playlists lock. Random-sample specs are
/// left out — there's no membership to hold them to, and reconciling one
/// would swap in a fresh sample on every library change.
pub fn match_dynamic_playlists(db: &Arc<Database>) -> Result<Vec<DynamicPlaylistMatch>> {
    let targets = dynamic_playlist_specs(db)?;
    let mut matches = Vec::with_capacity(targets.len());
    for (playlist_id, spec, is_system, mode) in targets {
        match spec_songs(db, &spec, is_system, mode) {
            Ok(resolved) if resolved.random_sample => {}
            Ok(resolved) => matches.push(DynamicPlaylistMatch {
                playlist_id,
                spec,
                song_ids: resolved.songs.iter().map(|s| s.id).collect(),
            }),
            Err(e) => log::error!("Failed to reconcile dynamic playlist {playlist_id}: {e}"),
        }
    }
    Ok(matches)
}

/// Every enabled dynamic playlist with a non-empty spec, as
/// `(id, spec, is_system, population_mode)`.
fn dynamic_playlist_specs(db: &Database) -> Result<Vec<(i64, String, bool, QueuePopulationMode)>> {
    let conn = db.pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, dynamic_spec, COALESCE(is_system, 0), COALESCE(population_mode, 'all') FROM playlists
         WHERE dynamic_enabled = 1 AND TRIM(COALESCE(dynamic_spec, '')) != ''",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, spec, is_system, mode)| {
            (
                id,
                spec,
                is_system,
                QueuePopulationMode::from(mode.as_str()),
            )
        })
        .collect())
}

/// Every library song matching a dynamic spec, in the order the spec's
/// population mode dictates. When `is_system` is true, dispatches system
/// auto-playlist specs (`decade:`, `bpmrange:`, `artisttag:`, `artistcountry:`,
/// `language:`, `missingmeta`, `missingmbid`, `tag:`, `daypart:`); user Smart
/// Playlists (`is_system == false`) always resolve as search-rule queries so
/// filters like `language:ja` or `tag:canadian` never collide (#2136).
fn spec_songs(
    db: &Arc<Database>,
    spec: &str,
    is_system: bool,
    mode: QueuePopulationMode,
) -> Result<SpecSongs> {
    let matched = |songs| {
        Ok(SpecSongs {
            songs,
            random_sample: false,
        })
    };
    if let Some(radio_spec) = RadioStationListSpec::parse(spec) {
        return matched(resolve_radio_spec(db, &radio_spec)?);
    }
    let random_fill = |scanner: &CollectionScanner| {
        // Unlike a genre match (naturally bounded by how many songs carry
        // that genre), "random songs from anywhere" has no natural size —
        // cap it to a real mix-sized sample rather than shuffling the
        // entire library into one playlist.
        Ok(SpecSongs {
            songs: scanner.get_random_songs(DAYPART_RANDOM_FILL_LIMIT)?,
            random_sample: true,
        })
    };
    let scanner = CollectionScanner::new(db.clone());
    if is_system {
        if let Some(decade) = spec.strip_prefix("decade:") {
            return matched(scanner.get_songs_by_decade(decade, NO_SONG_LIMIT, mode)?);
        } else if let Some((min, max)) = spec
            .strip_prefix("bpmrange:")
            .and_then(crate::collection::parse_bpm_range_spec)
        {
            return matched(scanner.get_songs_by_bpm_range(min, max, NO_SONG_LIMIT, mode)?);
        } else if let Some(tag) = spec.strip_prefix("artisttag:") {
            return matched(scanner.get_songs_by_artist_tag(tag, NO_SONG_LIMIT, mode)?);
        } else if spec == "missingmeta" {
            return matched(scanner.get_songs_missing_core_tags(NO_SONG_LIMIT, mode)?);
        } else if spec == "missingmbid" {
            return matched(scanner.get_songs_missing_musicbrainz_id(NO_SONG_LIMIT, mode)?);
        } else if let Some(code) = spec.strip_prefix("artistcountry:") {
            return matched(scanner.get_songs_by_artist_country(code, NO_SONG_LIMIT, mode)?);
        } else if let Some(lang) = spec.strip_prefix("language:") {
            let tag_manager = TagManager::new(db.clone());
            return matched(tag_manager.get_songs_by_language(lang, NO_SONG_LIMIT, mode)?);
        } else if let Some(name) = spec.strip_prefix("tag:") {
            let tag_manager = TagManager::new(db.clone());
            return matched(tag_manager.get_songs_by_curated_tag(name, NO_SONG_LIMIT, mode)?);
        } else if let Some(rest) = spec.strip_prefix("daypart:") {
            // "<bucket>:<date>:<resolved-name>" — bucket/date are only the
            // reroll cache key `sync_daypart_auto_playlist` checks; here we only
            // care about the already-resolved trailing name (empty means the
            // random-fill fallback was chosen). This never re-picks anything
            // itself, so calling this via `populate_dynamic_playlist`/reconcile
            // does not reroll the mix.
            let name = rest.splitn(3, ':').nth(2).unwrap_or("");
            return if name.is_empty() {
                random_fill(&scanner)
            } else {
                let tag_manager = TagManager::new(db.clone());
                let songs = tag_manager.get_songs_by_curated_tag(name, NO_SONG_LIMIT, mode)?;
                if songs.is_empty() {
                    random_fill(&scanner)
                } else {
                    matched(songs)
                }
            };
        }
    }
    let query = spec.replace(';', " ");
    matched(scanner.search_songs_by_mode(&query, NO_SONG_LIMIT, mode)?)
}

impl PlaylistManager {
    /// Persist the `population_mode` bias for a playlist row (see #120).
    pub fn set_playlist_population_mode(&self, id: i64, mode: QueuePopulationMode) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE playlists SET population_mode = ?1 WHERE id = ?2",
            params![mode.as_str(), id],
        )?;
        Ok(())
    }

    /// Every library song matching a dynamic spec — see [`spec_songs`].
    /// `pub(super)` so `auto_sync.rs`'s `sync_daypart_auto_playlist` can
    /// materialize a freshly-resolved `daypart:` spec through the same path
    /// every other category uses.
    pub(super) fn songs_for_spec(
        &self,
        spec: &str,
        is_system: bool,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        Ok(spec_songs(&self.db, spec, is_system, mode)?.songs)
    }

    /// Populate/refresh tracks for any dynamic playlist based on its `dynamic_spec`,
    /// selected per its own `population_mode` bias (see #120).
    pub fn populate_dynamic_playlist(&mut self, playlist_id: i64) -> Result<()> {
        let conn = self.db.pool.get()?;
        let row: Option<(Option<String>, bool, String)> = conn
            .query_row(
                "SELECT dynamic_spec, COALESCE(is_system, 0), COALESCE(population_mode, 'all') FROM playlists WHERE id = ?1 AND dynamic_enabled = 1",
                params![playlist_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let (spec, is_system, mode) = match row {
            Some((Some(s), is_system, mode)) if !s.trim().is_empty() => {
                (s, is_system, QueuePopulationMode::from(mode.as_str()))
            }
            _ => return Ok(()),
        };
        // `songs_for_spec` takes its own connection; re-acquire for the
        // writes instead of holding two.
        drop(conn);

        let songs = self.songs_for_spec(&spec, is_system, mode)?;

        let conn = self.db.pool.get()?;
        let now = crate::clock::unix_now();
        conn.execute(
            "UPDATE playlists SET updated = ?1 WHERE id = ?2",
            params![now, playlist_id],
        )?;

        conn.execute(
            "DELETE FROM playlist_items WHERE playlist_id = ?1",
            params![playlist_id],
        )?;

        for (position, song) in songs.iter().enumerate() {
            conn.execute(
                "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type) VALUES (?1, ?2, ?3, ?4, 0)",
                params![playlist_id, song.id, position as i32, Uuid::new_v4().to_string()],
            )?;
        }

        Ok(())
    }

    /// Update the `dynamic_spec` and `dynamic_enabled` fields for a playlist row, and populate its matching songs.
    pub fn set_playlist_dynamic_spec(&mut self, id: i64, spec: &str) -> Result<()> {
        let conn = self.db.pool.get()?;
        let enabled = !spec.trim().is_empty();
        conn.execute(
            "UPDATE playlists SET dynamic_spec = ?1, dynamic_enabled = ?2 WHERE id = ?3",
            params![spec, enabled, id],
        )?;
        // Populating takes its own connection; holding this one meanwhile
        // needs two at once and can exhaust the pool under load.
        drop(conn);
        if enabled {
            self.populate_dynamic_playlist(id)?;
        }
        Ok(())
    }

    /// Set a dynamic playlist's population-mode bias and its spec together,
    /// populating once. The frontend used to make these as two separate
    /// calls — set-mode (which itself repopulates via the
    /// `set_playlist_population_mode` command) then set-spec (which
    /// populates again) — regenerating the playlist's tracks twice per edit.
    pub fn set_playlist_dynamic_config(
        &mut self,
        id: i64,
        mode: QueuePopulationMode,
        spec: &str,
    ) -> Result<()> {
        self.set_playlist_population_mode(id, mode)?;
        self.set_playlist_dynamic_spec(id, spec)
    }

    /// Bring one dynamic playlist's membership in line with its definition:
    /// append every song that newly matches (ordered among themselves by the
    /// playlist's population-mode rules), delete rows whose song no longer
    /// matches. Surviving rows keep their positions and UUIDs — a full
    /// re-sort only ever happens on the explicit Refresh path
    /// (`populate_dynamic_playlist`). Maintenance writes bypass the undo
    /// stack: only user edits belong there.
    fn apply_dynamic_match(
        &mut self,
        m: &DynamicPlaylistMatch,
    ) -> Result<Option<DynamicPlaylistDelta>> {
        let playlist_id = m.playlist_id;
        let conn = self.db.pool.get()?;
        // The match was resolved without the playlists lock; skip a playlist
        // deleted or redefined since — its own edit already repopulated it.
        let still_current: bool = conn
            .query_row(
                "SELECT 1 FROM playlists WHERE id = ?1 AND dynamic_enabled = 1 AND dynamic_spec = ?2",
                params![playlist_id, m.spec],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !still_current {
            return Ok(None);
        }

        let matching_ids: std::collections::HashSet<i64> = m.song_ids.iter().copied().collect();
        let mut stmt = conn.prepare(
            "SELECT song_id, uuid, position FROM playlist_items
             WHERE playlist_id = ?1 AND song_id IS NOT NULL",
        )?;
        let current: Vec<(i64, String, i32)> = stmt
            .query_map(params![playlist_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        let current_ids: std::collections::HashSet<i64> =
            current.iter().map(|(id, _, _)| *id).collect();
        let removed_uuids: Vec<String> = current
            .iter()
            .filter(|(id, _, _)| !matching_ids.contains(id))
            .map(|(_, uuid, _)| uuid.clone())
            .collect();
        let to_add: Vec<i64> = m
            .song_ids
            .iter()
            .copied()
            .filter(|id| !current_ids.contains(id))
            .collect();

        if removed_uuids.is_empty() && to_add.is_empty() {
            return Ok(None);
        }

        for uuid in &removed_uuids {
            conn.execute(
                "DELETE FROM playlist_items WHERE playlist_id = ?1 AND uuid = ?2",
                params![playlist_id, uuid],
            )?;
        }

        let start_pos: i32 = current.iter().map(|(_, _, p)| *p).max().unwrap_or(-1) + 1;
        let mut added_uuids = std::collections::HashSet::new();
        for (next_pos, song_id) in (start_pos..).zip(to_add.iter()) {
            let uuid = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO playlist_items (playlist_id, song_id, position, uuid, type) VALUES (?1, ?2, ?3, ?4, 0)",
                params![playlist_id, song_id, next_pos, uuid],
            )?;
            added_uuids.insert(uuid);
        }
        self.renumber_positions(&conn, playlist_id)?;
        drop(conn);

        // Re-fetch so the returned items carry the row UUIDs the frontend
        // and live player queue will see.
        let added: Vec<PlaylistItem> = self
            .get_playlist_tracks(playlist_id)?
            .into_iter()
            .filter(|item| added_uuids.contains(&item.uuid))
            .collect();

        Ok(Some(DynamicPlaylistDelta {
            playlist_id,
            added,
            removed_uuids,
        }))
    }

    /// The write half of a reconcile pass: applies matches from
    /// [`match_dynamic_playlists`]. Returns one delta per playlist that
    /// actually changed.
    pub fn apply_dynamic_matches(
        &mut self,
        matches: Vec<DynamicPlaylistMatch>,
    ) -> Result<Vec<DynamicPlaylistDelta>> {
        let mut deltas = Vec::new();
        for m in &matches {
            match self.apply_dynamic_match(m) {
                Ok(Some(delta)) => deltas.push(delta),
                Ok(None) => {}
                Err(e) => log::error!(
                    "Failed to reconcile dynamic playlist {}: {e}",
                    m.playlist_id
                ),
            }
        }
        Ok(deltas)
    }

    /// Reconcile every dynamic playlist (auto categories and user smart
    /// playlists alike) against the current library in one call — both
    /// halves back to back. The app's listener runs them separately so the
    /// read half doesn't hold the playlists lock (see [`reconcile_and_sync`]).
    pub fn reconcile_dynamic_playlists(&mut self) -> Result<Vec<DynamicPlaylistDelta>> {
        let matches = match_dynamic_playlists(&self.db)?;
        self.apply_dynamic_matches(matches)
    }

    /// Force-regenerates a dynamic/auto playlist's tracks (e.g. when user clicks
    /// the "Refresh" button in the auto-playlist header), replacing its contents
    /// with a fresh selection of matching songs from the library.
    pub fn refresh_auto_playlist(&mut self, playlist_id: i64) -> Result<()> {
        self.populate_dynamic_playlist(playlist_id)
    }

    /// Force-regenerates every dynamic/auto playlist's tracks in one pass —
    /// the frontend used to fan this out as one IPC call per playlist id.
    pub fn refresh_all_dynamic_playlists(&mut self) -> Result<()> {
        let ids: Vec<i64> = self
            .get_playlists()?
            .into_iter()
            .filter(|p| p.dynamic_enabled)
            .map(|p| p.id)
            .collect();
        for id in ids {
            self.populate_dynamic_playlist(id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup_test_db() -> (Database, std::path::PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap().keep();
        let db = Database::new(temp_dir.clone()).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_songs_for_spec_daypart_prefix_dispatches_to_curated_tag_lookup() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=3 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, source, unavailable) VALUES (?1, 'A', 'Jazz', 1, 0)",
                    params![format!("Jazz Song {i}")],
                )
                .unwrap();
            }
            conn.execute(
                "INSERT INTO songs (title, artist, genre, source, unavailable) VALUES ('Other', 'A', 'Blues', 1, 0)",
                [],
            )
            .unwrap();
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let daypart_songs = manager
            .songs_for_spec(
                "daypart:morning:2026-09-03:Jazz",
                true,
                QueuePopulationMode::All,
            )
            .unwrap();
        let tag_songs = manager
            .songs_for_spec("tag:Jazz", true, QueuePopulationMode::All)
            .unwrap();

        assert_eq!(daypart_songs.len(), 3);
        let mut daypart_ids: Vec<_> = daypart_songs.iter().map(|s| s.id).collect();
        let mut tag_ids: Vec<_> = tag_songs.iter().map(|s| s.id).collect();
        daypart_ids.sort();
        tag_ids.sort();
        assert_eq!(
            daypart_ids, tag_ids,
            "a daypart: spec's trailing name must resolve to the same songs as a tag: spec (mode All orders randomly, so order isn't compared)"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_songs_for_spec_daypart_empty_name_uses_random_fill() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=5 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, source, unavailable) VALUES (?1, 'A', ?2, 1, 0)",
                    params![
                        format!("Song {i}"),
                        if i % 2 == 0 { "Jazz" } else { "Blues" },
                    ],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let songs = manager
            .songs_for_spec(
                "daypart:morning:2026-09-03:",
                true,
                QueuePopulationMode::All,
            )
            .unwrap();

        assert_eq!(
            songs.len(),
            5,
            "empty trailing name marks the random-fill fallback — every available song, capped by DAYPART_RANDOM_FILL_LIMIT"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// #1995: a `genre:` smart playlist follows genre aliases — it gains the
    /// alias-tagged songs once the alias exists and drops them when it is
    /// removed, with no change to the songs' own tags.
    #[test]
    fn test_reconcile_genre_smart_playlist_follows_aliases() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for (i, genre) in ["Progressive Rock", "Prog Rock", "Jazz"].iter().enumerate() {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, path) VALUES (?1, 'A', ?2, ?3)",
                    params![format!("Song {}", i + 1), genre, format!("/s{}.mp3", i + 1)],
                )
                .unwrap();
            }
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Prog Smart").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "genre:\"Progressive Rock\"")
            .unwrap();
        assert_eq!(manager.get_playlist_tracks(pl.id).unwrap().len(), 1);

        {
            let mut conn = db_arc.pool.get().unwrap();
            crate::genre_aliases::set_alias(&mut conn, "Prog Rock", "Progressive Rock").unwrap();
        }
        let deltas = manager.reconcile_dynamic_playlists().unwrap();
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].added.len(), 1);
        assert_eq!(
            deltas[0].added[0].song.as_ref().unwrap().title.as_deref(),
            Some("Song 2")
        );
        assert_eq!(manager.get_playlist_tracks(pl.id).unwrap().len(), 2);

        {
            let conn = db_arc.pool.get().unwrap();
            crate::genre_aliases::remove_alias(&conn, "Prog Rock").unwrap();
        }
        let deltas = manager.reconcile_dynamic_playlists().unwrap();
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].removed_uuids.len(), 1);
        assert_eq!(manager.get_playlist_tracks(pl.id).unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_reconcile_appends_new_matches_and_evicts_stale_without_reordering() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for (i, genre) in ["Rock", "Rock", "Jazz"].iter().enumerate() {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, path) VALUES (?1, 'A', ?2, ?3)",
                    params![format!("Song {}", i + 1), genre, format!("/s{}.mp3", i + 1)],
                )
                .unwrap();
            }
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        // A smart playlist over genre — dynamic like the auto categories.
        let pl = manager.create_playlist("Rock Smart").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "genre:Rock")
            .unwrap();
        let before = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(before.len(), 2);

        // Library gains a matching song; a member's genre is edited away.
        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, genre, path) VALUES ('Song 4', 'A', 'Rock', '/s4.mp3')",
                [],
            )
            .unwrap();
            conn.execute("UPDATE songs SET genre = 'Pop' WHERE title = 'Song 1'", [])
                .unwrap();
        }

        let deltas = manager.reconcile_dynamic_playlists().unwrap();
        assert_eq!(deltas.len(), 1);
        let delta = &deltas[0];
        assert_eq!(delta.playlist_id, pl.id);
        assert_eq!(delta.added.len(), 1);
        assert_eq!(
            delta.added[0].song.as_ref().unwrap().title.as_deref(),
            Some("Song 4")
        );
        assert_eq!(delta.removed_uuids.len(), 1);

        // Surviving row keeps its UUID and position slot; new match appended last.
        let after = manager.get_playlist_tracks(pl.id).unwrap();
        let titles: Vec<_> = after
            .iter()
            .map(|i| i.song.as_ref().unwrap().title.clone().unwrap())
            .collect();
        assert_eq!(titles.last().map(String::as_str), Some("Song 4"));
        let surviving_before = before
            .iter()
            .find(|i| i.song.as_ref().unwrap().title.as_deref() != Some("Song 1"))
            .unwrap();
        assert!(after.iter().any(|i| i.uuid == surviving_before.uuid));

        // A second pass with nothing changed is a no-op.
        let deltas = manager.reconcile_dynamic_playlists().unwrap();
        assert!(deltas.is_empty());

        // Maintenance must not pollute the user's undo stack: undo() should
        // fail (nothing to undo beyond the initial spec population, which is
        // also not an undoable op).
        assert!(
            manager.undo().is_err()
                || manager.get_playlist_tracks(pl.id).unwrap().len() == after.len()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_reconcile_leaves_random_fill_daypart_mix_alone() {
        // A random-fill Daypart Mix gets a fresh random sample on each query,
        // so reconciling it against one would swap its whole membership on
        // every library/stats event.
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=20 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, path, source, unavailable) VALUES (?1, 'A', 'Jazz', ?2, 1, 0)",
                    params![format!("Song {i}"), format!("/s{i}.mp3")],
                )
                .unwrap();
            }
        }
        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Morning Mix").unwrap();
        db_arc
            .pool
            .get()
            .unwrap()
            .execute(
                "UPDATE playlists SET is_system = 1 WHERE id = ?1",
                params![pl.id],
            )
            .unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "daypart:morning:2026-09-03:")
            .unwrap();
        let before: Vec<_> = manager
            .get_playlist_tracks(pl.id)
            .unwrap()
            .into_iter()
            .map(|i| i.uuid)
            .collect();
        assert!(!before.is_empty());

        let deltas = manager.reconcile_dynamic_playlists().unwrap();
        assert!(deltas.is_empty(), "random-fill mix must not be reconciled");
        let after: Vec<_> = manager
            .get_playlist_tracks(pl.id)
            .unwrap()
            .into_iter()
            .map(|i| i.uuid)
            .collect();
        assert_eq!(before, after);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_apply_skips_match_whose_spec_changed_after_it_was_computed() {
        // Matching runs without the playlists lock, so the spec can change
        // between computing a match and applying it.
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for (i, genre) in ["Rock", "Jazz"].iter().enumerate() {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, path) VALUES (?1, 'A', ?2, ?3)",
                    params![format!("Song {}", i + 1), genre, format!("/s{}.mp3", i + 1)],
                )
                .unwrap();
            }
        }
        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Smart").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "genre:Rock")
            .unwrap();
        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute("UPDATE songs SET genre = 'Rock' WHERE title = 'Song 2'", [])
                .unwrap();
        }
        let matches = match_dynamic_playlists(&db_arc).unwrap();

        manager
            .set_playlist_dynamic_spec(pl.id, "genre:Jazz")
            .unwrap();
        let jazz: Vec<_> = manager
            .get_playlist_tracks(pl.id)
            .unwrap()
            .into_iter()
            .map(|i| i.uuid)
            .collect();

        let deltas = manager.apply_dynamic_matches(matches).unwrap();
        assert!(deltas.is_empty(), "stale genre:Rock match must be dropped");
        let after: Vec<_> = manager
            .get_playlist_tracks(pl.id)
            .unwrap()
            .into_iter()
            .map(|i| i.uuid)
            .collect();
        assert_eq!(jazz, after);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    #[test]
    fn test_smart_playlist_genre_rule_populates_via_filter_not_exact_genre_match() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Rock Song', 'Classic Rock', 1, 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Jazz Song', 'Jazz', 1, 0)",
                [],
            )
            .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Rock Mix").unwrap();

        // Mirrors the spec the Smart Playlist builder serialises for a single
        // "genre contains rock" rule — must NOT be routed to the curated-tag
        // "tag:" path, which would never match "Classic Rock" (no such
        // curated tag exists).
        manager
            .set_playlist_dynamic_spec(pl.id, "genre:rock")
            .unwrap();

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(
            tracks.len(),
            1,
            "expected the contains-style genre rule to match 'Classic Rock' via LIKE, not require an exact 'rock' genre"
        );
        assert_eq!(
            tracks[0].song.as_ref().unwrap().title.as_deref(),
            Some("Rock Song")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Changing a Smart Playlist's `population_mode` must be picked up the
    /// next time its tracks are (re)populated — see #120.
    #[test]
    fn test_set_playlist_population_mode_changes_populated_tracks() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, source, unavailable, rating, playcount, lastplayed) VALUES ('Old Favourite', 'Miles Davis', 1, 0, 5, 40, 1700000000)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, source, unavailable, rating, playcount, lastplayed) VALUES ('Unheard Cut', 'Miles Davis', 1, 0, 0, 0, NULL)",
                [],
            )
            .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Miles Mix").unwrap();
        assert_eq!(pl.population_mode, QueuePopulationMode::All);

        manager
            .set_playlist_dynamic_spec(pl.id, "artist:Miles Davis")
            .unwrap();
        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(
            tracks.len(),
            2,
            "both songs should populate under the default All mode"
        );

        // Switch to Deep Cuts — only the never-played song should remain.
        manager
            .set_playlist_population_mode(pl.id, QueuePopulationMode::DeepCuts)
            .unwrap();
        manager.refresh_auto_playlist(pl.id).unwrap();

        let playlists = manager.get_playlists().unwrap();
        let updated = playlists.iter().find(|p| p.id == pl.id).unwrap();
        assert_eq!(updated.population_mode, QueuePopulationMode::DeepCuts);

        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(
            tracks[0].song.as_ref().unwrap().title.as_deref(),
            Some("Unheard Cut")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_songs_for_spec_dispatches_tag_prefix_through_curated_hierarchy() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);

        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Song A', 'Metal', 1, 0)",
                [],
            )
            .unwrap();
            // Position-0/alone, so reconcile_hierarchy would otherwise give
            // it its own root card — demoted under Metal below to prove
            // curated-child membership (not song order) is what
            // "tag:Metal" now matches on.
            conn.execute(
                "INSERT INTO songs (title, genre, source, unavailable) VALUES ('Song B', 'Progressive Metal', 1, 0)",
                [],
            )
            .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        // Force the hierarchy to exist before populate_dynamic_playlist
        // dispatches through songs_for_spec's "tag:" branch.
        let tag_manager = crate::tags::TagManager::new(db_arc.clone());
        tag_manager.reconcile_hierarchy().unwrap();
        tag_manager
            .demote_group_to_child("Progressive Metal", "Metal")
            .unwrap();

        let pl = manager.create_playlist("Metal Auto").unwrap();
        db_arc
            .pool
            .get()
            .unwrap()
            .execute(
                "UPDATE playlists SET is_system = 1 WHERE id = ?1",
                params![pl.id],
            )
            .unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "tag:Metal")
            .unwrap();
        let tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(
            tracks.len(),
            2,
            "tag: dispatch for a group includes its curated child's songs"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_songs_for_spec_daypart_empty_tag_matches_falls_back_to_random_fill() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let conn = db_arc.pool.get().unwrap();
            for i in 1..=5 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, source, unavailable) VALUES (?1, 'A', 'Rock', 1, 0)",
                    params![format!("Rock Song {i}")],
                )
                .unwrap();
            }
        }

        let manager = PlaylistManager::new(db_arc.clone()).unwrap();
        // A genre tag that matches 0 songs in the library must fall back to random library fill
        let songs = manager
            .songs_for_spec(
                "daypart:morning:2026-09-12:NonExistentGenre",
                true,
                QueuePopulationMode::All,
            )
            .unwrap();

        assert_eq!(
            songs.len(),
            5,
            "an empty genre match in a daypart spec must fall back to random library fill rather than returning 0 songs"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_playlist_paths_never_hold_one_connection_while_taking_another() {
        // Regression test: each of these held a pooled connection while a
        // helper took a second, so enough concurrent callers could exhaust
        // the pool and stall every DB caller for r2d2's 30s timeout. With a
        // one-connection pool, any nested acquisition times out instead.
        let (db, temp_dir) = setup_test_db();
        {
            let conn = db.pool.get().unwrap();
            for i in 1..=30 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, path, source, unavailable) VALUES (?1, 'A', 'Rock', ?2, 1, 0)",
                    params![format!("Song {i}"), format!("/s{i}.mp3")],
                )
                .unwrap();
            }
        }
        let single = std::sync::Arc::new(Database {
            pool: r2d2::Pool::builder()
                .max_size(1)
                .connection_timeout(std::time::Duration::from_secs(2))
                .build(r2d2_sqlite::SqliteConnectionManager::file(
                    temp_dir.join("luminous.db"),
                ))
                .unwrap(),
            schema_version: db.schema_version,
            scan_gate: Default::default(),
        });
        drop(db);

        let mut manager = PlaylistManager::new(single).unwrap();
        let pl = manager.create_playlist("Rock Smart").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "genre:Rock")
            .unwrap();

        let items = manager.get_playlist_tracks(pl.id).unwrap();
        manager
            .reorder_playlist_item_by_uuid(pl.id, &items[0].uuid, &items[2].uuid)
            .unwrap();

        manager
            .export_playlist(pl.id, temp_dir.join("out.m3u"), false)
            .unwrap();

        manager.sync_daypart_auto_playlist().unwrap();

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_apply_dynamic_match_individual_delta() {
        let (db, temp_dir) = setup_test_db();
        {
            let conn = db.pool.get().unwrap();
            for i in 1..=5 {
                conn.execute(
                    "INSERT INTO songs (title, artist, genre, path, source, unavailable) VALUES (?1, 'A', 'Rock', ?2, 1, 0)",
                    params![format!("Song {i}"), format!("/s{i}.mp3")],
                )
                .unwrap();
            }
        }
        let db_arc = std::sync::Arc::new(db);
        let mut manager = PlaylistManager::new(db_arc).unwrap();
        let pl = manager.create_playlist("Dynamic Rock").unwrap();
        manager
            .set_playlist_dynamic_spec(pl.id, "genre:Rock")
            .unwrap();

        let dynamic_match = DynamicPlaylistMatch {
            playlist_id: pl.id,
            spec: "genre:Rock".to_string(),
            song_ids: vec![1, 2, 3],
        };

        let delta = manager.apply_dynamic_match(&dynamic_match).unwrap();
        assert!(delta.is_some());
        let delta = delta.unwrap();
        assert_eq!(delta.playlist_id, pl.id);
        assert_eq!(delta.removed_uuids.len(), 2);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_user_smart_playlist_with_colliding_prefix_uses_search_rules() {
        let (db, temp_dir) = setup_test_db();
        {
            let conn = db.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, genre, rating, language, path, source, unavailable)
                 VALUES ('High', 'ArtistCan', 'Rock', 5, 'ja', '/high.mp3', 1, 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO songs (title, artist, genre, rating, language, path, source, unavailable)
                 VALUES ('Low', 'ArtistCan', 'Rock', 1, 'en', '/low.mp3', 1, 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO artist_profiles (artist_key, tags) VALUES ('ArtistCan', '[\"canadian\"]')",
                [],
            )
            .unwrap();
        }
        let db_arc = std::sync::Arc::new(db);
        let mut manager = PlaylistManager::new(db_arc).unwrap();

        let pl_tag = manager.create_playlist("High Rated Canadian").unwrap();
        manager
            .set_playlist_dynamic_spec(pl_tag.id, "tag:canadian; rating:>=4")
            .unwrap();
        let tag_tracks = manager.get_playlist_tracks(pl_tag.id).unwrap();
        assert_eq!(tag_tracks.len(), 1);
        assert_eq!(
            tag_tracks[0].song.as_ref().unwrap().title.as_deref(),
            Some("High")
        );

        let pl_lang = manager.create_playlist("Japanese").unwrap();
        manager
            .set_playlist_dynamic_spec(pl_lang.id, "language:ja")
            .unwrap();
        let lang_tracks = manager.get_playlist_tracks(pl_lang.id).unwrap();
        assert_eq!(lang_tracks.len(), 1);
        assert_eq!(
            lang_tracks[0].song.as_ref().unwrap().title.as_deref(),
            Some("High")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn radio_station_list_spec_round_trip() {
        let spec = RadioStationListSpec {
            tags: vec!["Folk Metal".to_string(), "Jazz".to_string()],
            country: Some("CA".to_string()),
            lang: Some("english".to_string()),
            min_bitrate: Some(96),
            order: crate::radio_directory::RadioDirectoryOrder::Votes,
            limit: 50,
        };
        let encoded = spec.to_spec_string();
        assert_eq!(
            encoded,
            "radio:tag=Folk Metal,Jazz;country=CA;lang=english;minbitrate=96;order=votes;limit=50"
        );
        let decoded = RadioStationListSpec::parse(&encoded).expect("should parse radio: spec");
        assert_eq!(decoded, spec);

        assert!(RadioStationListSpec::parse("genre:rock").is_none());
        let empty = RadioStationListSpec::parse("radio:").expect("empty radio: spec is valid");
        assert_eq!(empty, RadioStationListSpec::default());
    }

    #[test]
    fn radio_station_list_reconcile_adds_and_evicts_preserving_survivor_order() {
        let (db, temp_dir) = setup_test_db();
        let db_arc = std::sync::Arc::new(db);
        {
            let mut conn = db_arc.pool.get().unwrap();
            crate::genre_aliases::set_alias(&mut conn, "pagan metal", "Folk Metal").unwrap();
            conn.execute(
                "INSERT INTO radio_directory
                 (stationuuid, name, url_resolved, tags, country, countrycode, language, codec, bitrate, hls, votes, clickcount, lastcheckok, updated_at)
                 VALUES
                 ('uuid-1', 'Survivor FM', 'http://stream.test/1', 'pagan metal,metal', 'Canada', 'CA', 'english', 'MP3', 128, 0, 10, 500, 1, 1000),
                 ('uuid-2', 'Evicted FM', 'http://stream.test/2', 'folk metal', 'Canada', 'CA', 'english', 'MP3', 128, 0, 5, 200, 1, 1000)",
                [],
            )
            .unwrap();
        }

        let mut manager = PlaylistManager::new(db_arc.clone()).unwrap();
        let pl = manager.create_playlist("Folk Metal CA").unwrap();
        manager
            .set_playlist_dynamic_spec(
                pl.id,
                "radio:tag=Folk Metal;country=CA;minbitrate=96;order=clickcount;limit=100",
            )
            .unwrap();

        let initial_tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(initial_tracks.len(), 2);
        let survivor_uuid = initial_tracks[0].uuid.clone();
        assert_eq!(
            initial_tracks[0].song.as_ref().unwrap().path.as_deref(),
            Some("radio://uuid-1")
        );
        assert_eq!(
            initial_tracks[1].song.as_ref().unwrap().path.as_deref(),
            Some("radio://uuid-2")
        );

        // Directory syncs: uuid-2 goes offline (lastcheckok = 0), uuid-3 appears.
        {
            let conn = db_arc.pool.get().unwrap();
            conn.execute(
                "UPDATE radio_directory SET lastcheckok = 0 WHERE stationuuid = 'uuid-2'",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO radio_directory
                 (stationuuid, name, url_resolved, tags, country, countrycode, language, codec, bitrate, hls, votes, clickcount, lastcheckok, updated_at)
                 VALUES
                 ('uuid-3', 'Newcomer FM', 'http://stream.test/3', 'folk metal', 'Canada', 'CA', 'english', 'OGG', 192, 0, 20, 900, 1, 2000)",
                [],
            )
            .unwrap();
        }

        let matches = match_dynamic_playlists(&db_arc).unwrap();
        let deltas = manager.apply_dynamic_matches(matches).unwrap();
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].removed_uuids.len(), 1);
        assert_eq!(deltas[0].added.len(), 1);

        let updated_tracks = manager.get_playlist_tracks(pl.id).unwrap();
        assert_eq!(updated_tracks.len(), 2);
        // Survivor stays first with its original row UUID; newcomer is appended.
        assert_eq!(updated_tracks[0].uuid, survivor_uuid);
        assert_eq!(
            updated_tracks[0].song.as_ref().unwrap().path.as_deref(),
            Some("radio://uuid-1")
        );
        assert_eq!(
            updated_tracks[1].song.as_ref().unwrap().path.as_deref(),
            Some("radio://uuid-3")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
