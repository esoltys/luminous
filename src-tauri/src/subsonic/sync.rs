//! Library sync for OpenSubsonic servers (#1162).
//!
//! Three phases, so a network failure part-way through never leaves the
//! library half-updated:
//! 1. **List** — enumerate every album and song on the server
//!    ([`fetch_library`]). Any failed request aborts the whole sync before a
//!    single row is written.
//! 2. **Artwork** — download one cover per album that doesn't have one yet
//!    ([`fetch_album_art`]). Runs outside any DB transaction; a failed image
//!    is counted but doesn't abort the sync.
//! 3. **Apply** — one transaction ([`apply_library`]) that upserts changed
//!    tracks, imports server ratings/favourites, and marks tracks that are no
//!    longer on the server unavailable (never deleted — they come back if the
//!    track reappears).

use super::{track_uri, AlbumId3, Child, SubsonicClient, URI_SCHEME};
use crate::covermanager::CoverManager;
use crate::models::{join_multi_value, FileType, Song, SongSource, SubsonicSyncStats};
use crate::stats::{self, RATING_UNRATED};
use anyhow::Result;
use md5::{Digest, Md5};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};

/// Page size for `search3` / `getAlbumList2` (500 is Navidrome's maximum).
pub const PAGE_SIZE: usize = 500;

/// Pixel size requested from `getCoverArt`, matching the iTunes fallback.
const COVER_ART_SIZE: u32 = 600;

/// Rating given to a favourited (starred) item that has no star rating of its
/// own — Luminous's Favourites are `rating >= 4`.
const STARRED_RATING: f32 = 4.0;

/// Everything the server reported in a complete enumeration.
#[derive(Debug, Default)]
pub struct RemoteLibrary {
    pub albums: Vec<AlbumId3>,
    pub songs: Vec<Child>,
}

/// Pages through `fetch(offset)` until a short or empty page, de-duplicating
/// by `id` and stopping if a page adds nothing new (a server that ignores the
/// offset would otherwise loop forever).
fn fetch_all_pages<T>(
    mut fetch: impl FnMut(usize) -> Result<Vec<T>>,
    id: impl Fn(&T) -> &str,
    mut on_page: impl FnMut(usize),
) -> Result<Vec<T>> {
    let mut all = Vec::new();
    let mut seen = HashSet::new();
    let mut offset = 0;
    loop {
        let page = fetch(offset)?;
        let page_len = page.len();
        let mut new_items = 0;
        for item in page {
            if seen.insert(id(&item).to_string()) {
                all.push(item);
                new_items += 1;
            }
        }
        on_page(all.len());
        if page_len < PAGE_SIZE || new_items == 0 {
            return Ok(all);
        }
        offset += page_len;
    }
}

/// Phase 1: enumerate the server's albums and songs. Songs come from an
/// empty-query `search3`; if that finds nothing while albums exist (servers
/// that don't support the empty query), fall back to `getAlbum` per album.
/// `progress` receives the number of songs listed so far.
pub fn fetch_library(
    client: &SubsonicClient,
    mut progress: impl FnMut(usize),
) -> Result<RemoteLibrary> {
    let albums = fetch_all_pages(
        |offset| client.album_list2_page(offset, PAGE_SIZE),
        |a: &AlbumId3| a.id.as_str(),
        |_| {},
    )?;

    let mut songs = fetch_all_pages(
        |offset| client.search3_songs_page(offset, PAGE_SIZE),
        |c: &Child| c.id.as_str(),
        &mut progress,
    )?;

    if songs.is_empty() && !albums.is_empty() {
        log::info!(
            "search3 returned no songs; listing {} albums individually",
            albums.len()
        );
        let mut seen = HashSet::new();
        for album in &albums {
            let full = client.get_album(&album.id)?;
            songs.extend(full.song.into_iter().filter(|s| seen.insert(s.id.clone())));
            progress(songs.len());
        }
    }

    songs.retain(|s| !s.is_dir && !s.id.is_empty());
    Ok(RemoteLibrary { albums, songs })
}

/// Joins an OpenSubsonic `artists`-style array with Luminous's `; ` multi-value
/// convention, falling back to the single-string field.
fn artist_names(refs: &[super::ArtistRef], fallback: Option<&str>) -> Option<String> {
    let names: Vec<String> = refs
        .iter()
        .map(|r| r.name.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        fallback
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    } else {
        Some(join_multi_value(&names))
    }
}

fn filetype_for(child: &Child) -> FileType {
    match child.suffix.as_deref() {
        Some(suffix) => crate::webdav::detect_filetype_from_url(&format!("track.{suffix}")),
        None => FileType::Unknown,
    }
}

/// Maps a server track to a library `Song` (without artwork or rating, which
/// the apply phase fills in).
pub fn song_from_child(server_id: i64, child: &Child) -> Song {
    let uri = track_uri(server_id, &child.id);
    let genre = child.genre.clone().or_else(|| {
        let names: Vec<String> = child
            .genres
            .iter()
            .map(|g| g.name.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect();
        (!names.is_empty()).then(|| join_multi_value(&names))
    });
    let title = if child.title.trim().is_empty() {
        child
            .path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .map(|s| s.to_string_lossy().to_string())
    } else {
        Some(child.title.clone())
    };
    Song {
        source: SongSource::Subsonic,
        filetype: filetype_for(child),
        path: Some(uri.clone()),
        url: Some(uri.clone()),
        stream_url: Some(uri),
        title,
        artist: artist_names(
            &child.artists,
            child.display_artist.as_deref().or(child.artist.as_deref()),
        ),
        album: child.album.clone(),
        album_artist: artist_names(&child.album_artists, child.display_album_artist.as_deref()),
        comment: child.comment.clone(),
        track: child.track.map(|n| n as i32),
        disc: child.disc_number.map(|n| n as i32),
        year: child.year.map(|n| n as i32),
        genre,
        bpm: child.bpm.map(|n| n as f32),
        length_nanosec: child.duration.map(|secs| secs * 1_000_000_000),
        bitrate: child.bit_rate.map(|n| n as i32),
        samplerate: child.sampling_rate.map(|n| n as i32),
        bitdepth: child.bit_depth.map(|n| n as i32),
        channels: child.channel_count.map(|n| n as i32),
        filesize: child.size,
        replaygain_track_gain: child.replay_gain.as_ref().and_then(|rg| rg.track_gain),
        replaygain_album_gain: child.replay_gain.as_ref().and_then(|rg| rg.album_gain),
        musicbrainz_recording_id: child.music_brainz_id.clone(),
        rating: RATING_UNRATED,
        ..Default::default()
    }
}

/// Stable hash of a mapped song's metadata, so a re-sync can skip tracks the
/// server hasn't changed. Taken before artwork/rating are filled in.
fn fingerprint(song: &Song) -> String {
    let digest = Md5::digest(format!("{song:?}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The rating a server-side star rating / favourite implies in Luminous:
/// the star rating when set, otherwise 4 for a favourite, otherwise none.
pub fn server_rating(user_rating: Option<i64>, starred: bool) -> Option<f32> {
    match user_rating {
        Some(r) if (1..=5).contains(&r) => Some(r as f32),
        _ if starred => Some(STARRED_RATING),
        _ => None,
    }
}

fn is_unrated(r: f32) -> bool {
    r < 0.0
}

/// Decides whether to adopt the server's rating, given what the server said
/// last sync (`previous`, `None` = never synced), what it says now, and the
/// local rating. Returns the rating to write, or `None` to leave it alone.
///
/// - First import: fill in an unrated song/album; never overwrite a local rating.
/// - Later syncs: only react when the server value changed since last sync,
///   and only if the local rating still matches what the server said before
///   (i.e. the user hasn't edited it in Luminous since — a local edit wins
///   and is pushed back to the server by #1165).
pub fn rating_to_adopt(
    previous: Option<Option<f32>>,
    current: Option<f32>,
    local: f32,
) -> Option<f32> {
    match previous {
        None => match current {
            Some(r) if is_unrated(local) => Some(r),
            _ => None,
        },
        Some(prev) if prev != current => {
            let local_matches_prev = match prev {
                Some(p) => (local - p).abs() < f32::EPSILON,
                None => is_unrated(local),
            };
            if is_unrated(local) || local_matches_prev {
                Some(current.unwrap_or(RATING_UNRATED))
            } else {
                None
            }
        }
        Some(_) => None,
    }
}

/// Artwork already cached for this server's albums (`album_id → filename`),
/// read up front so [`fetch_album_art`] needn't hold a pooled DB connection
/// across its network requests.
pub fn existing_album_art(conn: &Connection, server_id: i64) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT c.album_id, s.art_automatic FROM subsonic_cache c JOIN songs s ON s.id = c.song_id
         WHERE c.server_id = ?1 AND c.album_id IS NOT NULL AND s.art_automatic IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![server_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    Ok(rows.flatten().collect())
}

/// Phase 2: one cover per album that doesn't have art yet. Returns
/// `album_id → cached filename`, plus the number of failed downloads. The
/// album's own `coverArt` id is preferred — Navidrome gives every song its
/// own id (`mf-…`) even when they all share the album image, so deduping on
/// the song's id would download the same picture once per track.
pub fn fetch_album_art(
    existing_art: &HashMap<String, String>,
    client: &SubsonicClient,
    cover_manager: &CoverManager,
    server_id: i64,
    library: &RemoteLibrary,
    mut progress: impl FnMut(usize),
) -> (HashMap<String, String>, usize) {
    let album_cover: HashMap<&str, &str> = library
        .albums
        .iter()
        .filter_map(|a| a.cover_art.as_deref().map(|c| (a.id.as_str(), c)))
        .collect();

    let mut art = HashMap::new();
    let mut errors = 0;
    let mut done = 0;
    let mut handled = HashSet::new();
    for song in &library.songs {
        let Some(album_id) = song.album_id.as_deref() else {
            continue;
        };
        if !handled.insert(album_id.to_string()) {
            continue;
        }

        if let Some(filename) = existing_art.get(album_id) {
            art.insert(album_id.to_string(), filename.clone());
            continue;
        }

        let Some(cover_id) = album_cover
            .get(album_id)
            .copied()
            .or(song.cover_art.as_deref())
        else {
            continue;
        };
        match client.get_cover_art(cover_id, COVER_ART_SIZE) {
            Ok(bytes) => {
                let mapped = song_from_child(server_id, song);
                let artist = mapped
                    .album_artist
                    .clone()
                    .or(mapped.artist.clone())
                    .unwrap_or_default();
                let album = mapped.album.clone().unwrap_or_default();
                match cover_manager.cache_art_bytes(&artist, &album, &bytes) {
                    Ok(filename) => {
                        art.insert(album_id.to_string(), filename);
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to cache Subsonic cover art for album {album_id}: {e:#}"
                        );
                        errors += 1;
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to download Subsonic cover art for album {album_id}: {e:#}");
                errors += 1;
            }
        }
        done += 1;
        progress(done);
    }
    (art, errors)
}

struct CachedTrack {
    song_id: Option<i64>,
    fingerprint: Option<String>,
    server_rating: Option<i64>,
    server_starred: bool,
}

struct ExistingSong {
    id: i64,
    unavailable: bool,
    rating: f32,
    art_automatic: Option<String>,
}

/// Parses an RFC 3339 `created` timestamp into Unix seconds.
fn created_unix(created: Option<&str>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(created?)
        .ok()
        .map(|dt| dt.timestamp())
}

/// Phase 3: write the enumeration into the library in one transaction.
/// Must only be called with a *complete* [`RemoteLibrary`] — anything not in
/// it is treated as removed from the server.
pub fn apply_library(
    conn: &Connection,
    server_id: i64,
    library: &RemoteLibrary,
    album_art: &HashMap<String, String>,
) -> Result<SubsonicSyncStats> {
    let tx = conn.unchecked_transaction()?;
    let mut stats = SubsonicSyncStats::default();
    let mut seen_song_ids = HashSet::new();

    for child in &library.songs {
        let mut song = song_from_child(server_id, child);
        let fp = fingerprint(&song);
        let uri = song.path.clone().unwrap_or_default();

        let cached: Option<CachedTrack> = tx
            .query_row(
                "SELECT song_id, fingerprint, server_rating, server_starred FROM subsonic_cache
                 WHERE server_id = ?1 AND remote_id = ?2",
                params![server_id, child.id],
                |r| {
                    Ok(CachedTrack {
                        song_id: r.get(0)?,
                        fingerprint: r.get(1)?,
                        server_rating: r.get(2)?,
                        server_starred: r.get(3)?,
                    })
                },
            )
            .optional()?;
        let existing: Option<ExistingSong> = tx
            .query_row(
                "SELECT id, unavailable, rating, art_automatic FROM songs WHERE path = ?1 AND beginning_nanosec = 0",
                params![uri],
                |r| {
                    Ok(ExistingSong {
                        id: r.get(0)?,
                        unavailable: r.get(1)?,
                        rating: r.get(2)?,
                        art_automatic: r.get(3)?,
                    })
                },
            )
            .optional()?;

        let album_art_file = child.album_id.as_deref().and_then(|id| album_art.get(id));
        let metadata_changed =
            cached.as_ref().and_then(|c| c.fingerprint.as_deref()) != Some(fp.as_str());
        let needs_art = album_art_file.is_some()
            && existing.as_ref().is_some_and(|e| e.art_automatic.is_none());

        let song_id = match &existing {
            Some(e) if !metadata_changed && !e.unavailable && !needs_art => e.id,
            _ => {
                song.art_automatic = album_art_file
                    .cloned()
                    .or_else(|| existing.as_ref().and_then(|e| e.art_automatic.clone()));
                crate::collection::upsert_song(&tx, &song)?;
                let id: i64 = tx.query_row(
                    "SELECT id FROM songs WHERE path = ?1 AND beginning_nanosec = 0",
                    params![uri],
                    |r| r.get(0),
                )?;
                match &existing {
                    None => {
                        stats.added += 1;
                        if let Some(ts) = created_unix(child.created.as_deref()) {
                            tx.execute(
                                "UPDATE songs SET added = ?1 WHERE id = ?2",
                                params![ts, id],
                            )?;
                        }
                    }
                    Some(e) if metadata_changed || e.unavailable => stats.updated += 1,
                    Some(_) => {}
                }
                id
            }
        };
        seen_song_ids.insert(song_id);

        // Ratings / favourites.
        let starred = child.starred.is_some();
        let current = server_rating(child.user_rating, starred);
        // A cache row that lost its song (pruned) is a fresh import.
        let previous = cached
            .as_ref()
            .filter(|c| c.song_id.is_some())
            .map(|c| server_rating(c.server_rating, c.server_starred));
        let local = existing
            .as_ref()
            .map(|e| e.rating)
            .unwrap_or(RATING_UNRATED);
        if let Some(rating) = rating_to_adopt(previous, current, local) {
            stats::set_rating(&tx, song_id, rating)?;
        }

        tx.execute(
            "INSERT INTO subsonic_cache (server_id, remote_id, song_id, album_id, cover_art_id, server_rating, server_starred, fingerprint)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(server_id, remote_id) DO UPDATE SET
               song_id = excluded.song_id, album_id = excluded.album_id,
               cover_art_id = excluded.cover_art_id, server_rating = excluded.server_rating,
               server_starred = excluded.server_starred, fingerprint = excluded.fingerprint,
               cached_at = strftime('%s', 'now')",
            params![
                server_id,
                child.id,
                song_id,
                child.album_id,
                child.cover_art,
                child.user_rating,
                starred,
                fp
            ],
        )?;
    }

    // Tracks no longer on the server: mark unavailable, never delete.
    let prefix = format!("{URI_SCHEME}{server_id}/");
    let stale: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM songs WHERE source = ?1 AND unavailable = 0 AND substr(path, 1, length(?2)) = ?2",
        )?;
        let rows = stmt.query_map(params![SongSource::SUBSONIC_ID, prefix], |r| {
            r.get::<_, i64>(0)
        })?;
        rows.flatten()
            .filter(|id| !seen_song_ids.contains(id))
            .collect()
    };
    for id in &stale {
        tx.execute(
            "UPDATE songs SET unavailable = 1 WHERE id = ?1",
            params![id],
        )?;
    }
    stats.removed = stale.len();

    apply_album_ratings(&tx, server_id, &library.albums)?;

    tx.commit()?;
    Ok(stats)
}

/// Album favourites/ratings → `album_ratings` (keyed by bare album title).
/// Titles shared by several server albums are only imported when those
/// albums agree; otherwise they're skipped rather than guessed.
fn apply_album_ratings(tx: &Connection, server_id: i64, albums: &[AlbumId3]) -> Result<()> {
    let mut by_title: HashMap<&str, Vec<&AlbumId3>> = HashMap::new();
    for album in albums {
        let title = album.name.trim();
        if !title.is_empty() {
            by_title.entry(title).or_default().push(album);
        }
    }

    for (title, group) in by_title {
        let currents: HashSet<Option<u32>> = group
            .iter()
            .map(|a| server_rating(a.user_rating, a.starred.is_some()).map(|r| (r * 2.0) as u32))
            .collect();

        for album in &group {
            let previous: Option<(Option<i64>, bool)> = tx
                .query_row(
                    "SELECT server_rating, server_starred FROM subsonic_album_cache
                     WHERE server_id = ?1 AND remote_album_id = ?2",
                    params![server_id, album.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            tx.execute(
                "INSERT INTO subsonic_album_cache (server_id, remote_album_id, album_key, server_rating, server_starred)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(server_id, remote_album_id) DO UPDATE SET
                   album_key = excluded.album_key, server_rating = excluded.server_rating,
                   server_starred = excluded.server_starred, cached_at = strftime('%s', 'now')",
                params![server_id, album.id, title, album.user_rating, album.starred.is_some()],
            )?;

            if currents.len() > 1 {
                continue;
            }
            let current = server_rating(album.user_rating, album.starred.is_some());
            let previous = previous.map(|(r, s)| server_rating(r, s));
            let local = stats::get_album_rating(tx, title)?;
            if let Some(rating) = rating_to_adopt(previous, current, local) {
                stats::set_album_rating(tx, title, rating)?;
            }
        }
        if currents.len() > 1 {
            log::info!(
                "Skipping album rating import for '{title}': {} server albums share the title with different ratings",
                group.len()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use serde_json::json;
    use std::sync::Arc;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn child(id: &str, album_id: &str, title: &str) -> Child {
        Child {
            id: id.into(),
            title: title.into(),
            album: Some(format!("Album {album_id}")),
            album_id: Some(album_id.into()),
            artist: Some("Artist".into()),
            suffix: Some("flac".into()),
            duration: Some(200),
            cover_art: Some(format!("mf-{id}")),
            ..Default::default()
        }
    }

    fn album(id: &str, name: &str) -> AlbumId3 {
        AlbumId3 {
            id: id.into(),
            name: name.into(),
            cover_art: Some(format!("al-{id}")),
            ..Default::default()
        }
    }

    fn temp_db(tag: &str) -> (Arc<Database>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "luminous_subsonic_sync_{tag}_{}",
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
                "INSERT INTO subsonic_servers (id, name, url, username, password) VALUES (1, 'Home', 'http://x', 'u', 'p')",
                [],
            )
            .unwrap();
        (db, dir)
    }

    fn song_id(conn: &Connection, remote_id: &str) -> i64 {
        conn.query_row(
            "SELECT id FROM songs WHERE path = ?1",
            params![track_uri(1, remote_id)],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn rating_of(conn: &Connection, remote_id: &str) -> f32 {
        conn.query_row(
            "SELECT rating FROM songs WHERE path = ?1",
            params![track_uri(1, remote_id)],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn unavailable(conn: &Connection, remote_id: &str) -> bool {
        conn.query_row(
            "SELECT unavailable FROM songs WHERE path = ?1",
            params![track_uri(1, remote_id)],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn server_rating_prefers_star_rating_over_favourite() {
        assert_eq!(server_rating(None, false), None);
        assert_eq!(server_rating(None, true), Some(4.0));
        assert_eq!(server_rating(Some(2), true), Some(2.0));
        assert_eq!(server_rating(Some(5), false), Some(5.0));
        assert_eq!(server_rating(Some(9), false), None);
    }

    #[test]
    fn rating_to_adopt_rules() {
        // First import fills an unrated item but never overwrites a local rating.
        assert_eq!(rating_to_adopt(None, Some(4.0), RATING_UNRATED), Some(4.0));
        assert_eq!(rating_to_adopt(None, Some(4.0), 2.5), None);
        assert_eq!(rating_to_adopt(None, None, RATING_UNRATED), None);
        // Unchanged on the server → nothing to do.
        assert_eq!(rating_to_adopt(Some(Some(4.0)), Some(4.0), 1.0), None);
        // Server changed and the local value is still what it imported → adopt.
        assert_eq!(rating_to_adopt(Some(Some(4.0)), Some(5.0), 4.0), Some(5.0));
        // Unstarred on the server → clear the imported rating.
        assert_eq!(
            rating_to_adopt(Some(Some(4.0)), None, 4.0),
            Some(RATING_UNRATED)
        );
        // Server changed but the user also edited it locally → local wins.
        assert_eq!(rating_to_adopt(Some(Some(4.0)), Some(5.0), 3.0), None);
        // Server gained a rating while local is unrated → adopt.
        assert_eq!(
            rating_to_adopt(Some(None), Some(3.0), RATING_UNRATED),
            Some(3.0)
        );
    }

    #[test]
    fn song_from_child_maps_metadata() {
        let c = Child {
            id: "a/b".into(),
            title: "Knowing".into(),
            album: Some("Bloom".into()),
            artists: vec![
                crate::subsonic::ArtistRef {
                    id: "1".into(),
                    name: "A".into(),
                },
                crate::subsonic::ArtistRef {
                    id: "2".into(),
                    name: "B".into(),
                },
            ],
            display_artist: Some("A • B".into()),
            display_album_artist: Some("A".into()),
            genres: vec![crate::subsonic::ItemGenre {
                name: "Ambient".into(),
            }],
            suffix: Some("FLAC".into()),
            duration: Some(3),
            bit_depth: Some(24),
            music_brainz_id: Some("mbid".into()),
            replay_gain: Some(crate::subsonic::ReplayGain {
                track_gain: Some(-6.5),
                ..Default::default()
            }),
            ..Default::default()
        };
        let s = song_from_child(7, &c);
        assert_eq!(s.source, SongSource::Subsonic);
        assert_eq!(s.path.as_deref(), Some("subsonic://7/a%2Fb"));
        assert_eq!(s.artist.as_deref(), Some("A; B"));
        assert_eq!(s.album_artist.as_deref(), Some("A"));
        assert_eq!(s.genre.as_deref(), Some("Ambient"));
        assert_eq!(s.filetype, FileType::Flac);
        assert_eq!(s.length_nanosec, Some(3_000_000_000));
        assert_eq!(s.bitdepth, Some(24));
        assert_eq!(s.replaygain_track_gain, Some(-6.5));
        assert_eq!(s.musicbrainz_recording_id.as_deref(), Some("mbid"));
    }

    #[test]
    fn fetch_all_pages_stops_on_short_page_and_on_repeats() {
        let mut calls = 0;
        let items = fetch_all_pages(
            |offset| {
                calls += 1;
                let n = if offset == 0 { PAGE_SIZE } else { 3 };
                Ok((offset..offset + n).map(|i| i.to_string()).collect())
            },
            |s: &String| s.as_str(),
            |_| {},
        )
        .unwrap();
        assert_eq!(items.len(), PAGE_SIZE + 3);
        assert_eq!(calls, 2);

        // A server that ignores the offset returns the same full page forever.
        let mut calls = 0;
        let items = fetch_all_pages(
            |_| {
                calls += 1;
                Ok((0..PAGE_SIZE).map(|i| i.to_string()).collect())
            },
            |s: &String| s.as_str(),
            |_| {},
        )
        .unwrap();
        assert_eq!(items.len(), PAGE_SIZE);
        assert_eq!(calls, 2);
    }

    #[test]
    fn apply_imports_updates_skips_unchanged_and_marks_removed() {
        let (db, dir) = temp_db("apply");
        let conn = db.pool.get().unwrap();
        let mut lib = RemoteLibrary {
            albums: vec![album("al1", "Album al1")],
            songs: vec![child("s1", "al1", "One"), child("s2", "al1", "Two")],
        };
        let art = HashMap::from([("al1".to_string(), "cover.jpg".to_string())]);

        let stats = apply_library(&conn, 1, &lib, &art).unwrap();
        assert_eq!((stats.added, stats.updated, stats.removed), (2, 0, 0));
        let art_file: Option<String> = conn
            .query_row(
                "SELECT art_automatic FROM songs WHERE path = ?1",
                params![track_uri(1, "s1")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(art_file.as_deref(), Some("cover.jpg"));

        // Nothing changed on the server → nothing rewritten.
        let stats = apply_library(&conn, 1, &lib, &art).unwrap();
        assert_eq!((stats.added, stats.updated, stats.removed), (0, 0, 0));

        // s2 removed on the server, s1 retitled.
        lib.songs = vec![child("s1", "al1", "One (Remastered)")];
        let stats = apply_library(&conn, 1, &lib, &art).unwrap();
        assert_eq!((stats.added, stats.updated, stats.removed), (0, 1, 1));
        assert!(unavailable(&conn, "s2"));
        assert!(!unavailable(&conn, "s1"));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM songs WHERE source = 5", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 2, "removed tracks are kept, not deleted");

        // s2 comes back.
        lib.songs.push(child("s2", "al1", "Two"));
        let stats = apply_library(&conn, 1, &lib, &art).unwrap();
        assert_eq!((stats.added, stats.updated, stats.removed), (0, 1, 0));
        assert!(!unavailable(&conn, "s2"));

        drop(conn);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn apply_imports_ratings_and_respects_local_edits() {
        let (db, dir) = temp_db("ratings");
        let conn = db.pool.get().unwrap();
        let mut fav = child("fav", "al1", "Fav");
        fav.starred = Some("2026-09-23T15:19:04Z".into());
        let mut rated = child("rated", "al1", "Rated");
        rated.starred = Some("2026-09-23T15:19:04Z".into());
        rated.user_rating = Some(5);
        let plain = child("plain", "al1", "Plain");
        let mut lib = RemoteLibrary {
            albums: vec![],
            songs: vec![fav, rated, plain],
        };

        apply_library(&conn, 1, &lib, &HashMap::new()).unwrap();
        assert_eq!(rating_of(&conn, "fav"), 4.0);
        assert_eq!(rating_of(&conn, "rated"), 5.0);
        assert_eq!(rating_of(&conn, "plain"), RATING_UNRATED);

        // The user re-rates "fav" in Luminous, then the server unfavourites it;
        // "rated" loses both its rating and its favourite on the server.
        stats::set_rating(&conn, song_id(&conn, "fav"), 2.0).unwrap();
        lib.songs[0].starred = None;
        lib.songs[1].user_rating = None;
        lib.songs[1].starred = None;
        apply_library(&conn, 1, &lib, &HashMap::new()).unwrap();
        assert_eq!(rating_of(&conn, "fav"), 2.0, "local edit wins");
        assert_eq!(
            rating_of(&conn, "rated"),
            RATING_UNRATED,
            "server change adopted"
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn apply_imports_album_stars_and_skips_conflicting_titles() {
        let (db, dir) = temp_db("albums");
        let conn = db.pool.get().unwrap();
        let mut bloom = album("al1", "Bloom");
        bloom.starred = Some("2026-09-23T15:16:26Z".into());
        let mut hits_a = album("al2", "Greatest Hits");
        hits_a.user_rating = Some(5);
        let hits_b = album("al3", "Greatest Hits");
        let mut rated = album("al4", "Something to Lose");
        rated.starred = Some("2026-09-23T15:16:27Z".into());
        rated.user_rating = Some(3);
        let lib = RemoteLibrary {
            albums: vec![bloom, hits_a, hits_b, rated],
            songs: vec![],
        };

        apply_library(&conn, 1, &lib, &HashMap::new()).unwrap();
        assert_eq!(stats::get_album_rating(&conn, "Bloom").unwrap(), 4.0);
        assert_eq!(
            stats::get_album_rating(&conn, "Something to Lose").unwrap(),
            3.0
        );
        assert_eq!(
            stats::get_album_rating(&conn, "Greatest Hits").unwrap(),
            RATING_UNRATED,
            "albums sharing a title with different ratings are skipped"
        );

        drop(conn);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn envelope(body: serde_json::Value) -> ResponseTemplate {
        let mut resp =
            json!({"status": "ok", "version": "1.16.1", "type": "navidrome", "openSubsonic": true});
        resp.as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        ResponseTemplate::new(200).set_body_raw(
            json!({ "subsonic-response": resp }).to_string(),
            "application/json",
        )
    }

    fn song_json(id: &str, album_id: &str) -> serde_json::Value {
        json!({"id": id, "isDir": false, "title": id, "album": "Bloom", "albumId": album_id,
               "coverArt": format!("mf-{id}"), "suffix": "mp3", "duration": 10})
    }

    #[tokio::test]
    async fn full_sync_downloads_one_cover_per_album_and_reuses_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/getAlbumList2.view"))
            .respond_with(envelope(json!({"albumList2": {"album": [
                {"id": "al1", "name": "Bloom", "coverArt": "al-al1", "songCount": 2}
            ]}})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/search3.view"))
            .respond_with(envelope(json!({"searchResult3": {"song": [
                song_json("s1", "al1"), song_json("s2", "al1")
            ]}})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/getCoverArt.view"))
            .and(query_param("id", "al-al1"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec(),
                "image/png",
            ))
            .expect(1)
            .mount(&server)
            .await;

        let (db, dir) = temp_db("full");
        let uri = server.uri();
        let db2 = Arc::clone(&db);
        let dir2 = dir.clone();
        let stats = tokio::task::spawn_blocking(move || {
            let client = SubsonicClient::new(&uri, "u", "p").unwrap();
            let covers = CoverManager::new(Arc::clone(&db2), dir2);
            let lib = fetch_library(&client, |_| {}).unwrap();
            assert_eq!(lib.songs.len(), 2);
            let (art, errors) = fetch_album_art(&HashMap::new(), &client, &covers, 1, &lib, |_| {});
            assert_eq!(errors, 0);
            assert_eq!(art.len(), 1);
            let conn = db2.pool.get().unwrap();
            let stats = apply_library(&conn, 1, &lib, &art).unwrap();

            // A second sync reuses the cached cover instead of downloading it again.
            let existing = existing_album_art(&conn, 1).unwrap();
            let (art2, _) = fetch_album_art(&existing, &client, &covers, 1, &lib, |_| {});
            assert_eq!(art2, art);
            stats
        })
        .await
        .unwrap();
        assert_eq!(stats.added, 2);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn fetch_library_falls_back_to_get_album() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/getAlbumList2.view"))
            .respond_with(envelope(json!({"albumList2": {"album": [
                {"id": "al1", "name": "Bloom"}, {"id": "al2", "name": "Other"}
            ]}})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/search3.view"))
            .respond_with(envelope(json!({"searchResult3": {}})))
            .mount(&server)
            .await;
        for (id, song) in [("al1", "s1"), ("al2", "s2")] {
            Mock::given(method("GET"))
                .and(path("/rest/getAlbum.view"))
                .and(query_param("id", id))
                .respond_with(envelope(
                    json!({"album": {"id": id, "name": id, "song": [song_json(song, id)]}}),
                ))
                .expect(1)
                .mount(&server)
                .await;
        }

        let uri = server.uri();
        let lib = tokio::task::spawn_blocking(move || {
            fetch_library(&SubsonicClient::new(&uri, "u", "p").unwrap(), |_| {})
        })
        .await
        .unwrap()
        .unwrap();
        let ids: Vec<&str> = lib.songs.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["s1", "s2"]);
    }

    #[tokio::test]
    async fn fetch_library_fails_whole_sync_on_any_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/getAlbumList2.view"))
            .respond_with(envelope(json!({"albumList2": {"album": []}})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/search3.view"))
            .respond_with(ResponseTemplate::new(502))
            .mount(&server)
            .await;

        let uri = server.uri();
        let result = tokio::task::spawn_blocking(move || {
            fetch_library(&SubsonicClient::new(&uri, "u", "p").unwrap(), |_| {})
        })
        .await
        .unwrap();
        assert!(result.is_err(), "a partial listing must not be applied");
    }
}
