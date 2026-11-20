//! "On the radio now" (#1998): which internet radio stations are playing a song
//! right now, from TunePing's live search.
//!
//! One deep service owns the whole workflow, so callers never see TunePing's
//! JSON, match grades or HTTP errors. [`AirplayService::lookup`] never fails:
//! anything that keeps it from answering is reported in [`AirplayStatus`].
//!
//! It protects TunePing's anonymous quota (1,000 requests a day per IP, shared by
//! everyone behind one NAT): results are cached per normalised artist and title
//! for [`CACHE_TTL_SECS`], and once TunePing throttles us no request goes out
//! for any song until its `Retry-After` has passed. The frontend adds a debounce
//! and only asks while the section is on screen.
//!
//! Not named `radio`: that module is the local-library "similar songs" queue (#1579).
//! The section is beta-quality by design (TunePing is in public beta): removing it
//! means deleting this module, its command and the frontend section.

use luminous_remote::lyrics::clean_featured;
use luminous_remote::tuneping::{
    NowPlayingRow, TunepingClient, TunepingError, TUNEPING_ATTRIBUTION_URL,
};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, OnceLock};

/// How long a result for one artist and title is reused, in seconds.
pub const CACHE_TTL_SECS: i64 = 60;

/// Back-off after a network failure that carried no `Retry-After`, in seconds.
const OFFLINE_BACKOFF_SECS: i64 = 30;

/// Persisted opt-in (`app_state` key). Off by default: nothing is sent until the user agrees.
pub const AIRPLAY_ENABLED_KEY: &str = "airplay_enabled";

/// Whether a lookup could run, and if not, why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AirplayStatus {
    /// The user hasn't opted in, or Luminous is offline. Nothing was sent.
    Disabled,
    /// The song can't be looked up (a station, or no artist and title). Nothing was sent.
    Unsupported,
    /// Fresh result. Zero stations is a normal answer.
    Ready,
    /// TunePing couldn't be reached or asked us to wait. `stations` holds the last
    /// result for this song, if any, and `retry_after` is the seconds until the next try.
    Unavailable { retry_after: i64 },
}

/// A station playing the song, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AirplayStation {
    /// Radio Browser station UUID.
    pub id: String,
    pub name: String,
    pub country: Option<String>,
    pub codec: Option<String>,
    pub bitrate: Option<i32>,
    pub homepage: Option<String>,
    /// Unix seconds of the station's latest poll: "still playing as of".
    pub confirmed_at: i64,
}

/// The whole answer for one song.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AirplayNow {
    pub status: AirplayStatus,
    /// Every matching station, best first. The UI shows the first few and "+N more".
    pub stations: Vec<AirplayStation>,
    /// Unix seconds when `stations` was fetched; `None` when nothing was ever fetched.
    pub fetched_at: Option<i64>,
    /// The "Data by Tuneping" link TunePing requires next to free data.
    pub attribution_url: &'static str,
}

impl AirplayNow {
    fn empty(status: AirplayStatus) -> Self {
        Self {
            status,
            stations: Vec::new(),
            fetched_at: None,
            attribution_url: TUNEPING_ATTRIBUTION_URL,
        }
    }
}

/// What the service needs to know about a song.
#[derive(Debug, Clone, Default)]
pub struct AirplaySong {
    pub artist: String,
    pub title: String,
    pub recording_mbid: Option<String>,
    /// Internet radio stations are songs rows too (`radio://` paths); they are never looked up.
    pub is_station: bool,
}

/// The live search the service calls; [`TunepingClient`] in the app, a fake in tests.
pub trait NowPlayingSource: Send + Sync {
    fn search(
        &self,
        artist: &str,
        title: &str,
    ) -> impl Future<Output = Result<Vec<NowPlayingRow>, TunepingError>> + Send;
}

impl NowPlayingSource for TunepingClient {
    fn search(
        &self,
        artist: &str,
        title: &str,
    ) -> impl Future<Output = Result<Vec<NowPlayingRow>, TunepingError>> + Send {
        self.search_now_playing(artist, title)
    }
}

/// Folds a name for comparison: ASCII-transliterated, lowercase, with "feat."
/// credits, bracketed suffixes ("(Remastered 2011)", "[Live]") and punctuation removed.
pub fn normalize_name(s: &str) -> String {
    let without_feat = clean_featured(s);
    let mut out = String::with_capacity(without_feat.len());
    let mut depth = 0usize;
    for c in without_feat.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let ascii = deunicode::deunicode(&out).to_lowercase();
    // "Title - 2011 Remaster" and similar version suffixes after a spaced dash.
    let ascii = match ascii.find(" - ") {
        Some(pos)
            if [
                "remaster", "version", "edit", "mix", "live", "mono", "stereo",
            ]
            .iter()
            .any(|k| ascii[pos..].contains(k)) =>
        {
            ascii[..pos].to_string()
        }
        _ => ascii,
    };
    let ascii = ascii.replace('&', " and ");
    ascii
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a TunePing row is this song: the same MusicBrainz recording, or the
/// same artist and title once both are normalised. A wrong station is worse than
/// none, so anything fuzzier is rejected.
pub fn row_matches(song: &AirplaySong, row: &NowPlayingRow) -> bool {
    if let (Some(ours), Some(theirs)) = (
        song.recording_mbid
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty()),
        row.recording_mbid(),
    ) {
        if ours.eq_ignore_ascii_case(theirs) {
            return true;
        }
    }
    let (Some(artist), Some(title)) = (row.artist.as_deref(), row.title.as_deref()) else {
        return false;
    };
    let song_artist = normalize_name(&song.artist);
    let song_title = normalize_name(&song.title);
    !song_artist.is_empty()
        && !song_title.is_empty()
        && normalize_name(artist) == song_artist
        && normalize_name(title) == song_title
}

fn parse_rfc3339(s: &str) -> Option<i64> {
    s.parse::<jiff::Timestamp>().ok().map(|t| t.as_second())
}

/// The matching rows as stations: one per station (duplicate streams collapsed to
/// the freshest), freshest first, then highest bitrate.
pub fn rank_stations(song: &AirplaySong, rows: &[NowPlayingRow]) -> Vec<AirplayStation> {
    let mut by_station: HashMap<&str, AirplayStation> = HashMap::new();
    for row in rows.iter().filter(|r| row_matches(song, r)) {
        let Some(confirmed_at) = row.confirmed_at.as_deref().and_then(parse_rfc3339) else {
            continue;
        };
        let candidate = AirplayStation {
            id: row.station.id.clone(),
            name: row.station.name.trim().to_string(),
            country: row.station.country.clone().filter(|c| !c.is_empty()),
            codec: row.station.codec.clone().filter(|c| !c.is_empty()),
            bitrate: row.station.bitrate.filter(|&b| b > 0),
            homepage: row
                .station
                .homepage
                .clone()
                .filter(|h| !h.trim().is_empty()),
            confirmed_at,
        };
        match by_station.get(row.station.id.as_str()) {
            Some(existing)
                if (existing.confirmed_at, existing.bitrate)
                    >= (candidate.confirmed_at, candidate.bitrate) => {}
            _ => {
                by_station.insert(row.station.id.as_str(), candidate);
            }
        }
    }
    let mut stations: Vec<AirplayStation> = by_station.into_values().collect();
    stations.sort_by(|a, b| {
        b.confirmed_at
            .cmp(&a.confirmed_at)
            .then(b.bitrate.cmp(&a.bitrate))
            .then_with(|| a.name.cmp(&b.name))
    });
    stations
}

struct CacheEntry {
    stations: Vec<AirplayStation>,
    fetched_at: i64,
}

/// The cache, back-off and matching around a [`NowPlayingSource`].
pub struct AirplayService<S: NowPlayingSource> {
    source: S,
    clock: Box<dyn Fn() -> i64 + Send + Sync>,
    cache: Mutex<HashMap<String, CacheEntry>>,
    /// No request goes out before this Unix second, whatever song is asked for.
    backoff_until: Mutex<i64>,
}

impl<S: NowPlayingSource> AirplayService<S> {
    pub fn new(source: S, clock: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self {
            source,
            clock: Box::new(clock),
            cache: Mutex::new(HashMap::new()),
            backoff_until: Mutex::new(0),
        }
    }

    /// Stations playing `song` now. `force` skips the cache (a manual refresh) but
    /// never the back-off. Callers check the opt-in first; see [`get_airplay_now`].
    pub async fn lookup(&self, song: &AirplaySong, force: bool) -> AirplayNow {
        if song.is_station || song.artist.trim().is_empty() || song.title.trim().is_empty() {
            return AirplayNow::empty(AirplayStatus::Unsupported);
        }
        let key = format!(
            "{}|{}",
            normalize_name(&song.artist),
            normalize_name(&song.title)
        );
        let now = (self.clock)();

        let cached = self
            .cache
            .lock()
            .get(&key)
            .map(|e| (e.stations.clone(), e.fetched_at));
        if let Some((stations, fetched_at)) = &cached {
            if !force && now - fetched_at < CACHE_TTL_SECS {
                return AirplayNow {
                    status: AirplayStatus::Ready,
                    stations: stations.clone(),
                    fetched_at: Some(*fetched_at),
                    attribution_url: TUNEPING_ATTRIBUTION_URL,
                };
            }
        }

        let unavailable = |retry_after: i64| {
            let (stations, fetched_at) = cached
                .clone()
                .map(|(s, f)| (s, Some(f)))
                .unwrap_or_default();
            AirplayNow {
                status: AirplayStatus::Unavailable {
                    retry_after: retry_after.max(1),
                },
                stations,
                fetched_at,
                attribution_url: TUNEPING_ATTRIBUTION_URL,
            }
        };

        let backoff_until = *self.backoff_until.lock();
        if now < backoff_until {
            return unavailable(backoff_until - now);
        }

        match self.source.search(&song.artist, &song.title).await {
            Ok(rows) => {
                let stations = rank_stations(song, &rows);
                self.cache.lock().insert(
                    key,
                    CacheEntry {
                        stations: stations.clone(),
                        fetched_at: now,
                    },
                );
                AirplayNow {
                    status: AirplayStatus::Ready,
                    stations,
                    fetched_at: Some(now),
                    attribution_url: TUNEPING_ATTRIBUTION_URL,
                }
            }
            Err(err) => {
                let wait = match err {
                    TunepingError::Throttled { retry_after_secs } => {
                        i64::try_from(retry_after_secs).unwrap_or(i64::MAX / 2)
                    }
                    TunepingError::Unavailable(reason) => {
                        log::debug!("TunePing lookup failed: {reason}");
                        OFFLINE_BACKOFF_SECS
                    }
                };
                let until = now.saturating_add(wait);
                let mut guard = self.backoff_until.lock();
                *guard = (*guard).max(until);
                unavailable(*guard - now)
            }
        }
    }
}

/// The app's single service, so the cache and back-off are shared by every caller.
pub fn service() -> &'static AirplayService<TunepingClient> {
    static SERVICE: OnceLock<AirplayService<TunepingClient>> = OnceLock::new();
    SERVICE.get_or_init(|| {
        AirplayService::new(
            TunepingClient::new(crate::radio_directory::RADIO_BROWSER_USER_AGENT),
            crate::clock::unix_now,
        )
    })
}

/// Whether lookups may be sent: the user opted in and Luminous is Online.
pub fn lookups_allowed(conn: &rusqlite::Connection) -> bool {
    crate::commands::context::is_online_enabled(conn)
        && crate::commands::settings::read_bool_pref(conn, AIRPLAY_ENABLED_KEY, false)
}

/// Reads what the service needs about a song row, or `None` when it doesn't exist.
pub fn load_song(conn: &rusqlite::Connection, song_id: i64) -> Option<AirplaySong> {
    conn.query_row(
        "SELECT artist, title, musicbrainz_recording_id, path FROM songs WHERE id = ?1",
        [song_id],
        |row| {
            let path: Option<String> = row.get(3)?;
            Ok(AirplaySong {
                artist: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                title: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                recording_mbid: row.get(2)?,
                is_station: path.is_some_and(|p| p.starts_with("radio://")),
            })
        },
    )
    .ok()
}

/// Filters `stations` to those present in our local `radio_directory` or `radio_saved`
/// tables, so the UI never shows stations that can't be opened in Internet Radio.
pub fn filter_known_stations(
    conn: &rusqlite::Connection,
    stations: Vec<AirplayStation>,
) -> crate::core::error::Result<Vec<AirplayStation>> {
    if stations.is_empty() {
        return Ok(stations);
    }
    let mut stmt = conn.prepare(
        "SELECT 1 FROM radio_directory WHERE stationuuid = ?1
         UNION ALL
         SELECT 1 FROM radio_saved WHERE stationuuid = ?1
         LIMIT 1",
    )?;
    let mut kept = Vec::with_capacity(stations.len());
    for station in stations {
        if stmt.exists([&station.id])? {
            kept.push(station);
        }
    }
    Ok(kept)
}

/// Shared entry point for the command: decides from the database whether anything may
/// be sent, then asks the service.
pub async fn get_airplay_now(
    db: &Arc<crate::db::Database>,
    song_id: i64,
    force: bool,
) -> AirplayNow {
    let loaded = crate::db::run_blocking(db, move |conn| {
        Ok((lookups_allowed(conn), load_song(conn, song_id)))
    })
    .await;
    match loaded {
        Ok((false, _)) => AirplayNow::empty(AirplayStatus::Disabled),
        Ok((true, None)) => AirplayNow::empty(AirplayStatus::Unsupported),
        Ok((true, Some(song))) => {
            let mut answer = service().lookup(&song, force).await;
            if !answer.stations.is_empty() {
                let stations = std::mem::take(&mut answer.stations);
                match crate::db::run_blocking(db, move |conn| filter_known_stations(conn, stations))
                    .await
                {
                    Ok(known) => answer.stations = known,
                    Err(e) => {
                        log::warn!("airplay: failed to filter stations against directory: {e}");
                    }
                }
            }
            answer
        }
        Err(e) => {
            log::warn!("airplay: could not read song {song_id}: {e}");
            AirplayNow::empty(AirplayStatus::Unsupported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luminous_remote::tuneping::{TunepingStation, TunepingTrack};
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

    /// 2026-10-29T15:10:30Z, the fixed "now" of these tests.
    const NOW: i64 = 1_793_286_630;

    fn row(station: &str, artist: &str, title: &str, confirmed: &str) -> NowPlayingRow {
        NowPlayingRow {
            station: TunepingStation {
                id: station.into(),
                name: format!("Station {station}"),
                country: Some("NL".into()),
                codec: Some("MP3".into()),
                bitrate: Some(128),
                homepage: Some(format!("https://{station}.example")),
                stream_url: None,
            },
            artist: Some(artist.into()),
            title: Some(title.into()),
            track: None,
            confirmed_at: Some(confirmed.into()),
        }
    }

    fn song(artist: &str, title: &str) -> AirplaySong {
        AirplaySong {
            artist: artist.into(),
            title: title.into(),
            ..Default::default()
        }
    }

    struct FakeSource {
        calls: Arc<AtomicUsize>,
        answer: Mutex<Result<Vec<NowPlayingRow>, TunepingError>>,
    }

    impl NowPlayingSource for FakeSource {
        fn search(
            &self,
            _artist: &str,
            _title: &str,
        ) -> impl Future<Output = Result<Vec<NowPlayingRow>, TunepingError>> + Send {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let answer = self.answer.lock().clone();
            async move { answer }
        }
    }

    fn service_with(
        answer: Result<Vec<NowPlayingRow>, TunepingError>,
    ) -> (AirplayService<FakeSource>, Arc<AtomicUsize>, Arc<AtomicI64>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = Arc::new(AtomicI64::new(NOW));
        let clock_read = clock.clone();
        let svc = AirplayService::new(
            FakeSource {
                calls: calls.clone(),
                answer: Mutex::new(answer),
            },
            move || clock_read.load(Ordering::SeqCst),
        );
        (svc, calls, clock)
    }

    #[test]
    fn normalize_folds_accents_case_feat_and_suffixes() {
        assert_eq!(normalize_name("Björk"), "bjork");
        assert_eq!(normalize_name("Holiday (Remastered 2011)"), "holiday");
        assert_eq!(normalize_name("Holiday [Live]"), "holiday");
        assert_eq!(normalize_name("Holiday - 2011 Remaster"), "holiday");
        assert_eq!(normalize_name("Song (feat. Someone)"), "song");
        assert_eq!(normalize_name("Artist feat. Other"), "artist");
        assert_eq!(normalize_name("Simon & Garfunkel"), "simon and garfunkel");
        assert_eq!(normalize_name("  Don't   Stop!  "), "don t stop");
        // A dash that isn't a version suffix stays part of the title.
        assert_eq!(normalize_name("Re - Education"), "re education");
    }

    #[test]
    fn matches_on_recording_mbid_even_when_names_differ() {
        let mut s = song("Madonna", "Holiday");
        s.recording_mbid = Some("D4708B79-62E7-4CE4-A6CA-247237D4EF4E".into());
        let mut r = row("a", "MADONNA & Friends", "Holidays", "2026-10-29T15:10:00Z");
        r.track = Some(TunepingTrack {
            mbid: Some("d4708b79-62e7-4ce4-a6ca-247237d4ef4e".into()),
        });
        assert!(row_matches(&s, &r));
    }

    #[test]
    fn matches_on_normalised_artist_and_title() {
        let s = song("Grönemeyer", "Männer (Remastered)");
        assert!(row_matches(
            &s,
            &row("a", "Gronemeyer", "Manner", "2026-10-29T15:10:00Z")
        ));
    }

    #[test]
    fn rejects_fuzzy_or_partial_matches() {
        let s = song("Madonna", "Holiday");
        assert!(!row_matches(
            &s,
            &row("a", "Madonna", "Holiday Rap", "2026-10-29T15:10:00Z")
        ));
        assert!(!row_matches(
            &s,
            &row("a", "Madonna Tribute", "Holiday", "2026-10-29T15:10:00Z")
        ));
        let mut no_names = row("a", "", "", "2026-10-29T15:10:00Z");
        no_names.artist = None;
        assert!(!row_matches(&s, &no_names));
    }

    #[test]
    fn ranking_collapses_duplicate_stations_and_orders_by_freshness_then_bitrate() {
        let s = song("Madonna", "Holiday");
        let mut low = row("b", "Madonna", "Holiday", "2026-10-29T15:09:00Z");
        low.station.bitrate = Some(64);
        let mut high = row("c", "Madonna", "Holiday", "2026-10-29T15:09:00Z");
        high.station.bitrate = Some(320);
        let rows = vec![
            row("a", "Madonna", "Holiday", "2026-10-29T15:05:00Z"),
            row("a", "Madonna", "Holiday", "2026-10-29T15:10:00Z"),
            low,
            high,
            row("d", "Someone", "Else", "2026-10-29T15:10:30Z"),
        ];
        let ranked = rank_stations(&s, &rows);
        let ids: Vec<&str> = ranked.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "c", "b"]);
        assert_eq!(ranked[0].confirmed_at, NOW - 30);
    }

    #[tokio::test]
    async fn stations_and_missing_names_are_unsupported_without_a_request() {
        let (svc, calls, _) = service_with(Ok(vec![]));
        let mut station = song("Radio X", "Live");
        station.is_station = true;
        assert_eq!(
            svc.lookup(&station, false).await.status,
            AirplayStatus::Unsupported
        );
        assert_eq!(
            svc.lookup(&song("", "Holiday"), false).await.status,
            AirplayStatus::Unsupported
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn results_are_cached_for_the_ttl_and_refetched_after() {
        let (svc, calls, clock) = service_with(Ok(vec![row(
            "a",
            "Madonna",
            "Holiday",
            "2026-10-29T15:10:00Z",
        )]));
        let s = song("Madonna", "Holiday");

        let first = svc.lookup(&s, false).await;
        assert_eq!(first.status, AirplayStatus::Ready);
        assert_eq!(first.stations.len(), 1);
        assert_eq!(first.fetched_at, Some(NOW));

        // The same song spelled differently shares the cache entry.
        clock.store(NOW + CACHE_TTL_SECS - 1, Ordering::SeqCst);
        svc.lookup(&song("MADONNA", "Holiday (Remastered)"), false)
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        clock.store(NOW + CACHE_TTL_SECS, Ordering::SeqCst);
        svc.lookup(&s, false).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn force_skips_the_cache() {
        let (svc, calls, _) = service_with(Ok(vec![]));
        let s = song("Madonna", "Holiday");
        svc.lookup(&s, false).await;
        svc.lookup(&s, true).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn zero_stations_is_ready() {
        let (svc, _, _) = service_with(Ok(vec![row("a", "Other", "Song", "2026-10-29T15:10:00Z")]));
        let now = svc.lookup(&song("Madonna", "Holiday"), false).await;
        assert_eq!(now.status, AirplayStatus::Ready);
        assert!(now.stations.is_empty());
    }

    #[tokio::test]
    async fn throttling_blocks_every_song_until_retry_after_and_serves_stale_results() {
        let (svc, calls, clock) = service_with(Ok(vec![row(
            "a",
            "Madonna",
            "Holiday",
            "2026-10-29T15:10:00Z",
        )]));
        let s = song("Madonna", "Holiday");
        svc.lookup(&s, false).await;

        *svc.source.answer.lock() = Err(TunepingError::Throttled {
            retry_after_secs: 3600,
        });
        clock.store(NOW + 120, Ordering::SeqCst);
        let throttled = svc.lookup(&s, false).await;
        assert_eq!(
            throttled.status,
            AirplayStatus::Unavailable { retry_after: 3600 }
        );
        // The last result is served, marked by its old fetch time.
        assert_eq!(throttled.stations.len(), 1);
        assert_eq!(throttled.fetched_at, Some(NOW));
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        // Another song, and a forced refresh, still send nothing during the back-off.
        clock.store(NOW + 1000, Ordering::SeqCst);
        let other = svc.lookup(&song("Prince", "Kiss"), true).await;
        assert_eq!(
            other.status,
            AirplayStatus::Unavailable {
                retry_after: 120 + 3600 - 1000
            }
        );
        assert!(other.stations.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        // Once it has passed, lookups resume.
        *svc.source.answer.lock() = Ok(vec![]);
        clock.store(NOW + 120 + 3600, Ordering::SeqCst);
        assert_eq!(
            svc.lookup(&song("Prince", "Kiss"), false).await.status,
            AirplayStatus::Ready
        );
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn network_failure_backs_off_briefly() {
        let (svc, calls, clock) = service_with(Err(TunepingError::Unavailable("offline".into())));
        let s = song("Madonna", "Holiday");
        assert_eq!(
            svc.lookup(&s, false).await.status,
            AirplayStatus::Unavailable {
                retry_after: OFFLINE_BACKOFF_SECS
            }
        );
        clock.store(NOW + OFFLINE_BACKOFF_SECS - 1, Ordering::SeqCst);
        svc.lookup(&s, false).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn lookups_need_both_the_opt_in_and_online() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE app_state (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();
        assert!(!lookups_allowed(&conn), "off by default");

        conn.execute(
            "INSERT INTO app_state VALUES ('airplay_enabled', 'true')",
            [],
        )
        .unwrap();
        assert!(lookups_allowed(&conn));

        conn.execute(
            "INSERT INTO app_state VALUES ('context_enrichment_enabled', 'false')",
            [],
        )
        .unwrap();
        assert!(!lookups_allowed(&conn), "offline wins");
    }

    #[test]
    fn station_rows_load_as_stations() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE songs (id INTEGER PRIMARY KEY, artist TEXT, title TEXT, musicbrainz_recording_id TEXT, path TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO songs VALUES (1, 'Madonna', 'Holiday', 'mbid-1', '/music/holiday.flac'),
                                      (2, NULL, 'Radio X', NULL, 'radio://abc')",
            [],
        )
        .unwrap();
        let song = load_song(&conn, 1).unwrap();
        assert_eq!(song.artist, "Madonna");
        assert_eq!(song.recording_mbid.as_deref(), Some("mbid-1"));
        assert!(!song.is_station);
        assert!(load_song(&conn, 2).unwrap().is_station);
        assert!(load_song(&conn, 3).is_none());
    }

    #[test]
    fn filter_known_stations_keeps_only_directory_or_saved_stations() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE radio_directory (stationuuid TEXT PRIMARY KEY);
             CREATE TABLE radio_saved (stationuuid TEXT PRIMARY KEY);
             INSERT INTO radio_directory VALUES ('in-dir');
             INSERT INTO radio_saved VALUES ('in-saved');",
        )
        .unwrap();

        let mk = |id: &str| AirplayStation {
            id: id.into(),
            name: id.into(),
            country: None,
            codec: None,
            bitrate: None,
            homepage: None,
            confirmed_at: NOW,
        };

        let kept = filter_known_stations(
            &conn,
            vec![mk("in-dir"), mk("unknown-station"), mk("in-saved")],
        )
        .unwrap();
        let ids: Vec<&str> = kept.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["in-dir", "in-saved"]);
    }
}
