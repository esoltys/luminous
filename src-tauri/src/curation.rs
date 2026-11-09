//! Curation queries identifying concrete library items that benefit from collector attention (#1586).
//!
//! Provides inspection of local library tracks for:
//! - Albums missing front cover artwork across all tracks
//! - Multi-track albums with incomplete or colliding track numbering
//! - Tracks missing core identity metadata (title, artist, or album)
//! - Lossless tracks with suspected lossy transcode or upsampling provenance warnings
//! - Corrupted, truncated, or bit-rotted tracks failing physical audio integrity checks (#1938)

use crate::db::Database;
use crate::models::{Playlist, Song, SongSource, LOCAL_SOURCES_SQL};
use anyhow::{Context, Result};
use luminous_analysis::IntegrityVerdict;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Summary of an album surfaced during curation audit.
///
/// Shaped to be directly consumable by frontend album cards (`AlbumRowCard.svelte`)
/// and album management editors (`AlbumProfileEditor.svelte`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CurationAlbumItem {
    pub album_key: String,
    pub album: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub track_count: i32,
    pub disc_count: i32,
    pub art_embedded: bool,
    pub art_automatic: Option<String>,
    pub art_manual: Option<String>,
    pub genre: Option<String>,
    pub rating: f32,
    pub sample_song_id: Option<i64>,
    pub song_ids: Vec<i64>,
}

/// A multi-track album diagnosed with incomplete or conflicting tracklist metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncompleteAlbumItem {
    #[serde(flatten)]
    pub album: CurationAlbumItem,
    pub unnumbered_count: i32,
    pub duplicate_track_number_count: i32,
}

/// A candidate track representation within a duplicate recording cluster (#1678).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateCandidate {
    pub song: Song,
    pub playlist_count: usize,
    pub tags: Vec<String>,
    pub provenance: Option<String>,
}

/// A cluster of two or more candidate copies representing the same musical recording (#1678).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateTrackGroup {
    pub group_key: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub candidates: Vec<DuplicateCandidate>,
}

/// A local library track identified with structural, container, or checksum corruption (#1938).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorruptSongItem {
    pub id: i64,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub path: String,
    pub format: String,
    pub error_detail: String,
    pub verified_at: i64,
}

/// Comprehensive overview of collection health and curation candidates across local library tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurationOverview {
    pub unillustrated_albums: Vec<CurationAlbumItem>,
    pub incomplete_albums: Vec<IncompleteAlbumItem>,
    pub missing_core_tags_count: usize,
    pub suspected_provenance_songs: Vec<Song>,
    pub duplicate_groups: Vec<DuplicateTrackGroup>,
    pub corrupt_songs: Vec<CorruptSongItem>,
}

/// Common CTEs computing top genres per album and pre-ordering tracks for album aggregation.
fn base_curation_album_ctes() -> String {
    format!(
        "album_genres AS (
            SELECT album_key, genre, genresort, COUNT(*) AS g_count
            FROM songs
            WHERE source IN ({local_sources}) AND unavailable = 0 AND not_included = 0 AND loved != -1
              AND album_key IS NOT NULL AND TRIM(album_key) != ''
              AND genre IS NOT NULL AND TRIM(genre) != ''
            GROUP BY album_key, genre
        ),
        top_album_genres AS (
            SELECT album_key, genre
            FROM (
                SELECT album_key, genre,
                       ROW_NUMBER() OVER (PARTITION BY album_key ORDER BY g_count DESC, COALESCE(genresort, genre) ASC) AS rn
                FROM album_genres
            )
            WHERE rn = 1
        ),
        album_tracks_ordered AS (
            SELECT * FROM songs
            WHERE source IN ({local_sources})
              AND unavailable = 0
              AND not_included = 0
              AND loved != -1
              AND album_key IS NOT NULL AND TRIM(album_key) != ''
            ORDER BY COALESCE(disc, 1) ASC, COALESCE(track, 0) ASC, id ASC
        )",
        local_sources = *LOCAL_SOURCES_SQL
    )
}

/// The 13 standard album columns mapped by `row_to_curation_album_item`.
fn curation_album_select_cols() -> String {
    format!(
        "songs.album_key,
        COALESCE({display}, MIN(songs.album), '') AS album,
        CASE
            WHEN songs.album_key LIKE '[va]\u{1f}%' THEN 'Various Artists'
            WHEN COUNT(DISTINCT NULLIF(songs.album_artist, '')) = 1 THEN MAX(NULLIF(songs.album_artist, ''))
            WHEN COUNT(DISTINCT NULLIF(songs.album_artist, '')) = 0 AND COUNT(DISTINCT NULLIF(songs.artist, '')) = 1 THEN MAX(NULLIF(songs.artist, ''))
            ELSE COALESCE(MAX(NULLIF(songs.album_artist, '')), MAX(NULLIF(songs.artist, '')))
        END AS album_artist,
        MIN(songs.year) AS year,
        COUNT(*) AS track_count,
        MAX(COALESCE(songs.disc, 1)) AS disc_count,
        MAX(CAST(songs.art_embedded AS INTEGER)) AS art_embedded,
        MAX(songs.art_automatic) AS art_automatic,
        MAX(songs.art_manual) AS art_manual,
        MAX(tag.genre) AS genre,
        COALESCE(MAX(ar.rating), -1.0) AS rating,
        COALESCE(
            MIN(CASE WHEN songs.art_manual IS NOT NULL OR songs.art_automatic IS NOT NULL OR songs.art_embedded != 0 THEN songs.id END),
            MIN(songs.id)
        ) AS sample_song_id,
        GROUP_CONCAT(songs.id) AS song_ids",
        display = crate::album_key::album_display_sql("songs.album_key"),
    )
}

/// Canonical album ordering: artist sort/display, then album sort/display.
const CURATION_ALBUM_ORDER_BY: &str =
    "ORDER BY COALESCE(MAX(songs.album_artist_sort), MAX(songs.artistsort), MAX(songs.album_artist), MAX(songs.artist)) COLLATE NOCASE, \
     COALESCE(MAX(songs.albumsort), MIN(songs.album)) COLLATE NOCASE";

/// Helper mapping standard album aggregation columns (0..12) into a `CurationAlbumItem`.
fn row_to_curation_album_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<CurationAlbumItem> {
    let song_ids_raw: Option<String> = row.get(12)?;
    let song_ids: Vec<i64> = song_ids_raw
        .map(|s| {
            s.split(',')
                .filter_map(|id| id.trim().parse::<i64>().ok())
                .collect()
        })
        .unwrap_or_default();

    Ok(CurationAlbumItem {
        album_key: row.get(0)?,
        album: row.get(1)?,
        artist: row.get(2)?,
        year: row.get(3)?,
        track_count: row.get(4)?,
        disc_count: row.get(5)?,
        art_embedded: row.get::<_, i64>(6)? != 0,
        art_automatic: row.get(7)?,
        art_manual: row.get(8)?,
        genre: row.get(9)?,
        rating: row.get::<_, f64>(10)? as f32,
        sample_song_id: row.get(11)?,
        song_ids,
    })
}

/// Queries local albums where no track has effective front cover artwork.
///
/// An album is deemed unillustrated if every track either has `art_unset = 1` or lacks
/// embedded, automatic, and manual art.
pub(crate) fn query_unillustrated_albums(conn: &Connection) -> Result<Vec<CurationAlbumItem>> {
    let sql = format!(
        "WITH {base_ctes}
        SELECT
            {select_cols}
        FROM album_tracks_ordered songs
        LEFT JOIN top_album_genres tag ON songs.album_key = tag.album_key
        LEFT JOIN album_ratings ar ON songs.album_key = ar.album_key
        GROUP BY songs.album_key
        HAVING MAX(
            CASE
                WHEN songs.art_unset = 0 AND (
                    songs.art_embedded != 0
                    OR (songs.art_automatic IS NOT NULL AND TRIM(songs.art_automatic) != '')
                    OR (songs.art_manual IS NOT NULL AND TRIM(songs.art_manual) != '')
                ) THEN 1
                ELSE 0
            END
        ) = 0
        {CURATION_ALBUM_ORDER_BY}",
        base_ctes = base_curation_album_ctes(),
        select_cols = curation_album_select_cols(),
    );

    let mut stmt = conn.prepare(&sql)?;
    let albums = stmt
        .query_map([], row_to_curation_album_item)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(albums)
}

/// Queries multi-track local albums (`track_count >= 2`) with missing track numbers or collisions.
///
/// Single-track releases are intentionally excluded from missing track number checks.
/// CUE sheet slices sharing the same file path with distinct `beginning_nanosec` values are
/// recognized as valid sub-tracks and excluded from collision counts.
pub(crate) fn query_incomplete_albums(conn: &Connection) -> Result<Vec<IncompleteAlbumItem>> {
    let sql = format!(
        "WITH {base_ctes},
        numbered_path_tracks AS (
            SELECT
                album_key,
                COALESCE(disc, 1) AS disc_num,
                track AS track_num,
                path,
                COUNT(DISTINCT beginning_nanosec) AS distinct_cues,
                COUNT(*) AS total_rows
            FROM songs
            WHERE source IN ({local_sources})
              AND unavailable = 0
              AND not_included = 0
              AND loved != -1
              AND album_key IS NOT NULL AND TRIM(album_key) != ''
              AND track IS NOT NULL AND track > 0
            GROUP BY album_key, COALESCE(disc, 1), track, path
        ),
        disc_track_counts AS (
            SELECT
                album_key,
                disc_num,
                track_num,
                SUM(CASE WHEN total_rows > distinct_cues THEN total_rows - distinct_cues + 1 ELSE 1 END) AS logical_tracks
            FROM numbered_path_tracks
            GROUP BY album_key, disc_num, track_num
        ),
        album_duplicate_counts AS (
            SELECT
                album_key,
                SUM(logical_tracks - 1) AS duplicate_track_number_count
            FROM disc_track_counts
            WHERE logical_tracks > 1
            GROUP BY album_key
        )
        SELECT
            {select_cols},
            CAST(SUM(CASE WHEN songs.track IS NULL OR songs.track <= 0 THEN 1 ELSE 0 END) AS INTEGER) AS unnumbered_count,
            CAST(COALESCE(MAX(adc.duplicate_track_number_count), 0) AS INTEGER) AS duplicate_track_number_count
        FROM album_tracks_ordered songs
        LEFT JOIN top_album_genres tag ON songs.album_key = tag.album_key
        LEFT JOIN album_ratings ar ON songs.album_key = ar.album_key
        LEFT JOIN album_duplicate_counts adc ON songs.album_key = adc.album_key
        GROUP BY songs.album_key
        HAVING COUNT(*) >= 2
           AND (
               SUM(CASE WHEN songs.track IS NULL OR songs.track <= 0 THEN 1 ELSE 0 END) > 0
               OR COALESCE(MAX(adc.duplicate_track_number_count), 0) > 0
           )
        {CURATION_ALBUM_ORDER_BY}",
        base_ctes = base_curation_album_ctes(),
        select_cols = curation_album_select_cols(),
        local_sources = *LOCAL_SOURCES_SQL,
    );

    let mut stmt = conn.prepare(&sql)?;
    let albums = stmt
        .query_map([], |row| {
            let album = row_to_curation_album_item(row)?;
            let unnumbered_count: i32 = row.get(13)?;
            let duplicate_track_number_count: i32 = row.get(14)?;
            Ok(IncompleteAlbumItem {
                album,
                unnumbered_count,
                duplicate_track_number_count,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(albums)
}

/// Counts local included library tracks missing any of the core tags: title, artist, or album.
///
/// Matches the predicate of `CollectionScanner::get_songs_missing_core_tags` to guarantee 1:1
/// parity with the `Missing Metadata` auto-playlist (`missingmeta`, #367).
pub(crate) fn query_missing_core_tags_count(conn: &Connection) -> Result<usize> {
    let sql = format!(
        "SELECT COUNT(*) FROM songs
         WHERE {}
           AND source IN ({local_sources})
           AND unavailable = 0
           AND not_included = 0
           AND loved != -1",
        crate::collection::MISSING_CORE_TAGS_SQL,
        local_sources = *LOCAL_SOURCES_SQL
    );
    let count: i64 = conn.query_row(&sql, [], |r| r.get(0))?;
    Ok(count.max(0) as usize)
}

/// Returns local lossless tracks flagged with suspected lossy transcode or upsampling warnings.
///
/// Safely probes SQLite table metadata for the `provenance` column (#1373). If the column is
/// not present in the current database schema, returns an empty vector without raising an error.
pub(crate) fn query_suspected_provenance_songs(conn: &Connection) -> Result<Vec<Song>> {
    let mut pragma_stmt =
        conn.prepare("SELECT 1 FROM pragma_table_info('songs') WHERE name = 'provenance' LIMIT 1")?;
    let mut rows = pragma_stmt.query([])?;
    if rows.next()?.is_none() {
        return Ok(Vec::new());
    }

    let sql = format!(
        "SELECT {} FROM songs
         WHERE source IN ({local_sources})
           AND unavailable = 0
           AND not_included = 0
           AND loved != -1
           AND filetype IN (1, 6)
           AND provenance IS NOT NULL AND TRIM(provenance) != '' AND provenance != 'authentic'
         ORDER BY COALESCE(album_artist_sort, album_artist), COALESCE(albumsort, album), disc, track",
        crate::collection::SONG_SELECT_COLS,
        local_sources = *LOCAL_SOURCES_SQL
    );

    let mut stmt = conn.prepare(&sql)?;
    let songs = stmt
        .query_map([], crate::collection::row_to_song)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(songs)
}

/// Normalizes an identity string token (title, artist, album) for duplicate comparison (#1678).
/// Trims, lowercases, collapses whitespace, underscores, and dashes into a single space,
/// and strips common punctuation brackets.
pub fn normalize_identity_token(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_is_space = true;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            for lower_ch in ch.to_lowercase() {
                result.push(lower_ch);
            }
            prev_is_space = false;
        } else if (ch.is_whitespace() || ch == '_' || ch == '-') && !prev_is_space {
            result.push(' ');
            prev_is_space = true;
        }
    }
    result.trim().to_string()
}

/// Checks whether a given file path resides in a `/Duplicates/` directory segment (#1678).
pub fn is_duplicates_folder(path: Option<&str>) -> bool {
    let Some(p) = path else { return false };
    let normalized = p.replace('\\', "/").to_lowercase();
    normalized.contains("/duplicates/")
        || normalized.starts_with("duplicates/")
        || normalized.ends_with("/duplicates")
}

fn get_filename_stem(path: Option<&str>) -> Option<String> {
    let p = path?;
    let path_obj = std::path::Path::new(p);
    path_obj
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

fn get_parent_dir_normalized(path: Option<&str>) -> Option<String> {
    let p = path?;
    let path_obj = std::path::Path::new(p);
    path_obj
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/").to_lowercase())
}

fn are_cue_siblings(a: &Song, b: &Song) -> bool {
    match (&a.path, &b.path) {
        (Some(pa), Some(pb)) => pa == pb && a.beginning_nanosec != b.beginning_nanosec,
        _ => false,
    }
}

struct DisjointSet {
    parent: Vec<usize>,
}

impl DisjointSet {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
        }
    }

    fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while root != self.parent[root] {
            root = self.parent[root];
        }
        let mut curr = i;
        while curr != root {
            let next = self.parent[curr];
            self.parent[curr] = root;
            curr = next;
        }
        root
    }

    fn union(&mut self, i: usize, j: usize) {
        let root_i = self.find(i);
        let root_j = self.find(j);
        if root_i != root_j {
            self.parent[root_i] = root_j;
        }
    }
}

fn candidate_sort_key(c: &DuplicateCandidate) -> (bool, bool, u32, u32, u32, i32, i64) {
    let is_dup_folder = is_duplicates_folder(c.song.path.as_deref());
    let is_lossless = c.song.filetype.is_lossless();
    let bitdepth = c.song.bitdepth.map(|b| b.max(0) as u32).unwrap_or(0);
    let samplerate = c.song.samplerate.map(|s| s.max(0) as u32).unwrap_or(0);
    let bitrate = c.song.bitrate.map(|b| b.max(0) as u32).unwrap_or(0);
    let playcount = c.song.playcount;
    let id = c.song.id;

    (
        !is_dup_folder,
        is_lossless,
        bitdepth,
        samplerate,
        bitrate,
        playcount,
        -id,
    )
}

/// Queries local library tracks to cluster and detect duplicate recordings (#1678).
///
/// Tracks are clustered using disjoint set union over recording identity matches and
/// duplicates folder path counterparts. CUE sheet tracks sharing a physical file
/// with distinct starting positions are strictly excluded from clustering.
pub fn query_duplicate_track_groups(conn: &Connection) -> Result<Vec<DuplicateTrackGroup>> {
    let sql = format!(
        "SELECT {} FROM songs
         WHERE source IN ({local_sources})
           AND unavailable = 0
           AND not_included = 0
           AND loved != -1",
        crate::collection::SONG_SELECT_COLS,
        local_sources = *LOCAL_SOURCES_SQL
    );

    let mut stmt = conn.prepare(&sql)?;
    let songs: Vec<Song> = stmt
        .query_map([], crate::collection::row_to_song)?
        .filter_map(|r| r.ok())
        .collect();

    if songs.len() < 2 {
        return Ok(Vec::new());
    }

    let n = songs.len();
    let mut uf = DisjointSet::new(n);

    struct SongMeta {
        norm_title: String,
        norm_artist: String,
        norm_album: String,
        is_dup_folder: bool,
        stem: Option<String>,
        parent_dir: Option<String>,
        disc: i32,
        track: i32,
    }

    let metas: Vec<SongMeta> = songs
        .iter()
        .map(|s| {
            let artist = s
                .artist
                .as_deref()
                .or(s.album_artist.as_deref())
                .unwrap_or("");
            let norm_artist = normalize_identity_token(artist);
            let norm_title = normalize_identity_token(s.title.as_deref().unwrap_or(""));
            let norm_album = normalize_identity_token(s.album.as_deref().unwrap_or(""));
            let is_dup_folder = is_duplicates_folder(s.path.as_deref());
            let stem = get_filename_stem(s.path.as_deref()).map(|st| normalize_identity_token(&st));
            let parent_dir = get_parent_dir_normalized(s.path.as_deref());
            let disc = s.disc.unwrap_or(1);
            let track = s.track.unwrap_or(0);

            SongMeta {
                norm_title,
                norm_artist,
                norm_album,
                is_dup_folder,
                stem,
                parent_dir,
                disc,
                track,
            }
        })
        .collect();

    let mut artist_title_buckets: HashMap<(String, String), Vec<usize>> = HashMap::new();
    let mut counterpart_buckets: HashMap<String, Vec<usize>> = HashMap::new();

    for (i, meta) in metas.iter().enumerate() {
        if !meta.norm_artist.is_empty() && !meta.norm_title.is_empty() {
            artist_title_buckets
                .entry((meta.norm_artist.clone(), meta.norm_title.clone()))
                .or_default()
                .push(i);
        }

        if let Some(ref stem) = meta.stem {
            if !stem.is_empty() {
                if meta.is_dup_folder {
                    if let Some(ref pd) = meta.parent_dir {
                        if let Some(grandparent) = std::path::Path::new(pd).parent() {
                            let gp_norm = grandparent
                                .to_string_lossy()
                                .replace('\\', "/")
                                .to_lowercase();
                            counterpart_buckets
                                .entry(format!("{}:{}", gp_norm, stem))
                                .or_default()
                                .push(i);
                        }
                    }
                } else if let Some(ref pd) = meta.parent_dir {
                    counterpart_buckets
                        .entry(format!("{}:{}", pd, stem))
                        .or_default()
                        .push(i);
                }
            }
        }
    }

    let are_duplicates = |i: usize, j: usize| -> bool {
        let s_a = &songs[i];
        let s_b = &songs[j];
        if are_cue_siblings(s_a, s_b) {
            return false;
        }

        let m_a = &metas[i];
        let m_b = &metas[j];

        // 1. Recording Identity Match
        if !m_a.norm_artist.is_empty()
            && !m_b.norm_artist.is_empty()
            && !m_a.norm_title.is_empty()
            && !m_b.norm_title.is_empty()
            && m_a.norm_artist == m_b.norm_artist
            && m_a.norm_title == m_b.norm_title
        {
            let duration_match = match (s_a.length_nanosec, s_b.length_nanosec) {
                (Some(la), Some(lb)) if la > 0 && lb > 0 => (la - lb).abs() <= 5_000_000_000,
                _ => true,
            };

            // Same normalized (artist, title) and one is in Duplicates folder
            if (m_a.is_dup_folder || m_b.is_dup_folder) && duration_match {
                return true;
            }

            // Same normalized (artist, album, track, title) when track > 0 and album is non-empty
            if !m_a.norm_album.is_empty()
                && m_a.norm_album == m_b.norm_album
                && m_a.track > 0
                && m_a.track == m_b.track
                && duration_match
            {
                return true;
            }

            // Same normalized (artist, album, title) and (same (disc, track) or at least one missing track)
            // and duration within ±5s when both have length_nanosec > 0
            if m_a.norm_album == m_b.norm_album {
                let track_match = (m_a.disc == m_b.disc && m_a.track == m_b.track)
                    || m_a.track == 0
                    || m_b.track == 0;

                if track_match && duration_match {
                    return true;
                }
            }
        }

        // 2. Duplicates Folder / Path Counterpart
        if (m_a.is_dup_folder && !m_b.is_dup_folder) || (!m_a.is_dup_folder && m_b.is_dup_folder) {
            if let (Some(ref st_a), Some(ref st_b)) = (&m_a.stem, &m_b.stem) {
                if st_a == st_b {
                    let (dup_meta, orig_meta) = if m_a.is_dup_folder {
                        (m_a, m_b)
                    } else {
                        (m_b, m_a)
                    };
                    if let (Some(ref dp), Some(ref op)) =
                        (&dup_meta.parent_dir, &orig_meta.parent_dir)
                    {
                        if let Some(grandparent) = std::path::Path::new(dp).parent() {
                            let gp_norm = grandparent
                                .to_string_lossy()
                                .replace('\\', "/")
                                .to_lowercase();
                            if &gp_norm == op {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        false
    };

    for indices in artist_title_buckets.values() {
        if indices.len() >= 2 {
            for idx_a in 0..indices.len() {
                for idx_b in (idx_a + 1)..indices.len() {
                    let i = indices[idx_a];
                    let j = indices[idx_b];
                    if are_duplicates(i, j) {
                        uf.union(i, j);
                    }
                }
            }
        }
    }

    for indices in counterpart_buckets.values() {
        if indices.len() >= 2 {
            for idx_a in 0..indices.len() {
                for idx_b in (idx_a + 1)..indices.len() {
                    let i = indices[idx_a];
                    let j = indices[idx_b];
                    if are_duplicates(i, j) {
                        uf.union(i, j);
                    }
                }
            }
        }
    }

    let mut raw_groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = uf.find(i);
        raw_groups.entry(root).or_default().push(i);
    }

    let candidate_groups: Vec<Vec<usize>> = raw_groups
        .into_values()
        .filter(|indices| indices.len() >= 2)
        .collect();

    if candidate_groups.is_empty() {
        return Ok(Vec::new());
    }

    let mut playlist_counts: HashMap<i64, usize> = HashMap::new();
    let mut non_queue_playlists: HashSet<i64> = HashSet::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, name, dynamic_enabled FROM playlists") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
            ))
        }) {
            for (id, name, dynamic_enabled) in rows.flatten() {
                if !Playlist::is_queue_row(&name, dynamic_enabled) {
                    non_queue_playlists.insert(id);
                }
            }
        }
    }

    if !non_queue_playlists.is_empty() {
        let mut song_to_playlists: HashMap<i64, HashSet<i64>> = HashMap::new();
        if let Ok(mut stmt) = conn.prepare("SELECT song_id, playlist_id FROM playlist_items") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            {
                for (song_id, playlist_id) in rows.flatten() {
                    if non_queue_playlists.contains(&playlist_id) {
                        song_to_playlists
                            .entry(song_id)
                            .or_default()
                            .insert(playlist_id);
                    }
                }
            }
        }
        for (song_id, playlists) in song_to_playlists {
            playlist_counts.insert(song_id, playlists.len());
        }
    }

    let mut provenance_map: HashMap<i64, String> = HashMap::new();
    let has_provenance: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('songs') WHERE name = 'provenance' LIMIT 1")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false);

    if has_provenance {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, provenance FROM songs WHERE provenance IS NOT NULL AND TRIM(provenance) != ''",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
                for row in rows.flatten() {
                    provenance_map.insert(row.0, row.1);
                }
            }
        }
    }

    let mut result_groups = Vec::new();

    for indices in candidate_groups {
        let mut candidates: Vec<DuplicateCandidate> = indices
            .into_iter()
            .map(|i| {
                let song = songs[i].clone();
                let playlist_count = playlist_counts.get(&song.id).copied().unwrap_or(0);
                let tags = crate::models::parse_multi_value(song.genre.as_deref().unwrap_or(""));
                let provenance = provenance_map.get(&song.id).cloned();

                DuplicateCandidate {
                    song,
                    playlist_count,
                    tags,
                    provenance,
                }
            })
            .collect();

        candidates.sort_by_key(|a| std::cmp::Reverse(candidate_sort_key(a)));

        let canonical = &candidates[0].song;
        let title = canonical
            .title
            .clone()
            .unwrap_or_else(|| "Unknown Title".to_string());
        let artist = canonical
            .artist
            .clone()
            .or_else(|| canonical.album_artist.clone())
            .unwrap_or_else(|| "Unknown Artist".to_string());
        let album = canonical.album.clone();
        let group_key = format!(
            "dup-{}-{}-{}",
            normalize_identity_token(&artist),
            normalize_identity_token(&title),
            canonical.id
        );

        result_groups.push(DuplicateTrackGroup {
            group_key,
            title,
            artist,
            album,
            candidates,
        });
    }

    result_groups.sort_by(|a, b| {
        a.artist
            .to_lowercase()
            .cmp(&b.artist.to_lowercase())
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });

    Ok(result_groups)
}

/// Resolves a duplicate track cluster in the database by folding redundant candidate rows into
/// the preserved song row (#1678).
///
/// Verifies that the preserved track exists and (when `delete_files_from_disk` is requested) that
/// its file is present on disk before deleting any redundant physical files.
pub fn resolve_duplicate_group_in_db(
    conn: &Connection,
    keep_song_id: i64,
    remove_song_ids: &[i64],
    delete_files_from_disk: bool,
) -> Result<Vec<String>> {
    if remove_song_ids.is_empty() {
        return Ok(Vec::new());
    }
    if remove_song_ids.contains(&keep_song_id) {
        anyhow::bail!("Cannot remove preserved song ID {keep_song_id}");
    }

    let keep_path: String = conn
        .query_row(
            "SELECT path FROM songs WHERE id = ?1",
            params![keep_song_id],
            |r| r.get(0),
        )
        .map_err(|_| anyhow::anyhow!("Preserved song ID {keep_song_id} not found in database"))?;

    if delete_files_from_disk {
        let keep_file = std::path::Path::new(&keep_path);
        if !keep_file.exists() {
            anyhow::bail!(
                "Preserved song file does not exist on disk at '{}'; aborting resolution to prevent data loss",
                keep_path
            );
        }
    }

    let mut deleted_paths = Vec::new();

    for &remove_id in remove_song_ids {
        let remove_path: Option<String> = conn
            .query_row(
                "SELECT path FROM songs WHERE id = ?1",
                params![remove_id],
                |r| r.get(0),
            )
            .optional()?;

        crate::collection::reconcile::merge_song_rows(conn, keep_song_id, remove_id)?;

        if delete_files_from_disk {
            if let Some(rem_path) = remove_path {
                if rem_path == keep_path {
                    continue;
                }
                let sharing_count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM songs WHERE path = ?1",
                    params![rem_path],
                    |r| r.get(0),
                )?;
                if sharing_count == 0 {
                    let p = std::path::Path::new(&rem_path);
                    if p.exists() {
                        if let Err(e) = std::fs::remove_file(p) {
                            log::warn!(
                                "Failed to delete redundant duplicate file at '{rem_path}': {e}"
                            );
                        } else {
                            deleted_paths.push(rem_path);
                        }
                    }
                }
            }
        }
    }

    Ok(deleted_paths)
}

/// Safely queries SQLite for corrupt tracks detected across the local library (#1938).
pub(crate) fn query_corrupt_songs(conn: &Connection) -> Result<Vec<CorruptSongItem>> {
    let has_table: bool = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'song_integrity'")?
        .exists([])?;
    if !has_table {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT s.id, s.title, s.artist, s.album, s.path, s.filetype, i.error_detail, i.verified_at
         FROM song_integrity i
         JOIN songs s ON s.id = i.song_id
         WHERE s.source IN ({local_sources})
           AND s.unavailable = 0
           AND i.status = 'corrupt'
           AND i.mtime = s.mtime
         ORDER BY s.added DESC, s.id DESC",
        local_sources = *LOCAL_SOURCES_SQL
    ))?;

    let rows = stmt.query_map([], |row| {
        let ft_num: i64 = row.get(5).unwrap_or(0);
        let format = crate::models::FileType::from(ft_num)
            .display_name()
            .to_string();
        Ok(CorruptSongItem {
            id: row.get(0)?,
            title: row.get(1)?,
            artist: row.get(2)?,
            album: row.get(3)?,
            path: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            format,
            error_detail: row
                .get::<_, Option<String>>(6)?
                .unwrap_or_else(|| "Corrupted file".to_string()),
            verified_at: row.get(7)?,
        })
    })?;

    let mut items = Vec::new();
    for r in rows {
        items.push(r?);
    }
    Ok(items)
}

/// Verify a single song's file integrity, saving the result to `song_integrity`.
pub fn verify_song_integrity(db: &Database, song_id: i64) -> Result<Option<CorruptSongItem>> {
    let conn = db.pool.get().context("failed to get db connection")?;

    let row = conn.query_row(
        "SELECT path, source, mtime, title, artist, album, format FROM songs WHERE id = ?1",
        params![song_id],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        },
    );

    let (path_str, source_raw, mtime_val, title, artist, album, format_str) = match row {
        Ok(vals) => vals,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let source = SongSource::from(source_raw);
    if !source.is_local() {
        return Ok(None);
    }

    let Some(path) = path_str else {
        return Ok(None);
    };

    let mtime = mtime_val.unwrap_or(0);
    let p = Path::new(&path);

    let verdict = luminous_analysis::verify_audio_file(p);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    match verdict {
        IntegrityVerdict::Valid => {
            conn.execute(
                "INSERT INTO song_integrity (song_id, mtime, status, error_detail, verified_at)
                 VALUES (?1, ?2, 'valid', NULL, ?3)
                 ON CONFLICT(song_id) DO UPDATE SET
                    mtime = excluded.mtime,
                    status = excluded.status,
                    error_detail = excluded.error_detail,
                    verified_at = excluded.verified_at",
                params![song_id, mtime, now],
            )?;
            Ok(None)
        }
        IntegrityVerdict::Corrupt { reason } => {
            conn.execute(
                "INSERT INTO song_integrity (song_id, mtime, status, error_detail, verified_at)
                 VALUES (?1, ?2, 'corrupt', ?3, ?4)
                 ON CONFLICT(song_id) DO UPDATE SET
                    mtime = excluded.mtime,
                    status = excluded.status,
                    error_detail = excluded.error_detail,
                    verified_at = excluded.verified_at",
                params![song_id, mtime, reason, now],
            )?;
            Ok(Some(CorruptSongItem {
                id: song_id,
                title,
                artist,
                album,
                path,
                format: format_str.unwrap_or_else(|| "Unknown".to_string()),
                error_detail: reason,
                verified_at: now,
            }))
        }
        IntegrityVerdict::Unverified => Ok(None),
    }
}

/// Spawn a low-priority background thread that checks unverified local tracks (#1938, epic #1441).
/// Throttles between tracks so it never interferes with playback or UI responsiveness.
pub fn spawn_background_integrity_checker(app: AppHandle, db: Arc<Database>) {
    std::thread::Builder::new()
        .name("luminous-integrity".to_string())
        .spawn(move || {
            loop {
                let next: Option<(i64, String)> = match db.pool.get() {
                    Ok(conn) => conn
                        .query_row(
                            &format!(
                                "SELECT s.id, s.path FROM songs s
                                 LEFT JOIN song_integrity i ON s.id = i.song_id
                                 WHERE s.source IN ({lib}) AND s.unavailable = 0 AND s.path IS NOT NULL
                                   AND (i.song_id IS NULL OR i.mtime != s.mtime)
                                 ORDER BY s.added DESC, s.id DESC LIMIT 1",
                                lib = *LOCAL_SOURCES_SQL
                            ),
                            [],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()
                        .unwrap_or(None),
                    Err(_) => None,
                };

                let Some((song_id, _path)) = next else {
                    std::thread::sleep(Duration::from_secs(20));
                    continue;
                };

                match verify_song_integrity(&db, song_id) {
                    Ok(Some(corrupt_item)) => {
                        let _ = app.emit("song-integrity-changed", &corrupt_item);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::debug!("Background integrity checker skipped song {song_id}: {e}");
                    }
                }

                // Throttle between tracks to keep CPU/disk gentle
                std::thread::sleep(Duration::from_millis(300));
            }
        })
        .expect("failed to spawn integrity checker thread");
}

/// Triggers verification of all unverified local songs across the library (#1938).
pub fn verify_library_integrity(app: &AppHandle, db: &Database) -> Result<usize> {
    let conn = db.pool.get().context("failed to get db connection")?;
    let mut stmt = conn.prepare(&format!(
        "SELECT s.id FROM songs s
         LEFT JOIN song_integrity i ON s.id = i.song_id
         WHERE s.source IN ({lib}) AND s.unavailable = 0 AND s.path IS NOT NULL
           AND (i.song_id IS NULL OR i.mtime != s.mtime)
         ORDER BY s.added DESC, s.id DESC",
        lib = *LOCAL_SOURCES_SQL
    ))?;

    let song_ids: Vec<i64> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let count = song_ids.len();
    for id in song_ids {
        if let Ok(Some(corrupt_item)) = verify_song_integrity(db, id) {
            let _ = app.emit("song-integrity-changed", &corrupt_item);
        }
    }
    Ok(count)
}

/// Computes the overall collection curation overview across local library files.
///
/// Ensures album keys are up to date via `repair_album_keys` before executing groupings.
pub fn compute_curation_overview(conn: &Connection) -> Result<CurationOverview> {
    crate::album_key::repair_album_keys(conn)?;

    let unillustrated_albums = query_unillustrated_albums(conn)?;
    let incomplete_albums = query_incomplete_albums(conn)?;
    let missing_core_tags_count = query_missing_core_tags_count(conn)?;
    let suspected_provenance_songs = query_suspected_provenance_songs(conn)?;
    let duplicate_groups = query_duplicate_track_groups(conn)?;
    let corrupt_songs = query_corrupt_songs(conn)?;

    Ok(CurationOverview {
        unillustrated_albums,
        incomplete_albums,
        missing_core_tags_count,
        suspected_provenance_songs,
        duplicate_groups,
        corrupt_songs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{upsert_song, CollectionScanner};
    use crate::db::Database;
    use crate::models::{FileType, QueuePopulationMode, Song, SongSource};
    use std::sync::Arc;

    fn setup_test_db() -> (tempfile::TempDir, Arc<Database>) {
        let temp_dir = tempfile::Builder::new()
            .prefix("luminous_curation_test_")
            .tempdir()
            .unwrap();
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).unwrap());
        (temp_dir, db)
    }

    #[test]
    fn test_empty_database_returns_zero_counts_and_empty_vectors() {
        let (_temp, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        let overview = compute_curation_overview(&conn).unwrap();
        assert!(overview.unillustrated_albums.is_empty());
        assert!(overview.incomplete_albums.is_empty());
        assert_eq!(overview.missing_core_tags_count, 0);
        assert!(overview.suspected_provenance_songs.is_empty());
        assert!(overview.duplicate_groups.is_empty());
        assert!(overview.corrupt_songs.is_empty());
    }

    #[test]
    fn test_unillustrated_albums_detection() {
        let (_temp, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        // 1. Album with no art anywhere (should be flagged)
        let song_no_art = Song {
            path: Some("path/no_art.flac".to_string()),
            title: Some("Track 1".to_string()),
            artist: Some("Artist A".to_string()),
            album: Some("Album No Art".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            art_embedded: false,
            art_automatic: None,
            art_manual: None,
            art_unset: false,
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_no_art).unwrap();

        // 2. Album with embedded art (should NOT be flagged)
        let song_embedded = Song {
            path: Some("path/embedded.flac".to_string()),
            title: Some("Track 1".to_string()),
            artist: Some("Artist B".to_string()),
            album: Some("Album Embedded".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            art_embedded: true,
            art_automatic: None,
            art_manual: None,
            art_unset: false,
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_embedded).unwrap();

        // 3. Album with automatic art on track 2 only (should NOT be flagged)
        let song_auto_1 = Song {
            path: Some("path/auto_1.flac".to_string()),
            title: Some("Track 1".to_string()),
            artist: Some("Artist C".to_string()),
            album: Some("Album Auto".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            art_embedded: false,
            art_automatic: None,
            art_manual: None,
            art_unset: false,
            unavailable: false,
            ..Default::default()
        };
        let song_auto_2 = Song {
            path: Some("path/auto_2.flac".to_string()),
            title: Some("Track 2".to_string()),
            artist: Some("Artist C".to_string()),
            album: Some("Album Auto".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            art_embedded: false,
            art_automatic: Some("cover.jpg".to_string()),
            art_manual: None,
            art_unset: false,
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_auto_1).unwrap();
        upsert_song(&conn, &song_auto_2).unwrap();

        // 4. Album with manual art (should NOT be flagged)
        let song_manual = Song {
            path: Some("path/manual.flac".to_string()),
            title: Some("Track 1".to_string()),
            artist: Some("Artist D".to_string()),
            album: Some("Album Manual".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            art_embedded: false,
            art_automatic: None,
            art_manual: None,
            art_unset: false,
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_manual).unwrap();
        conn.execute(
            "UPDATE songs SET art_manual = 'custom_cover.png' WHERE path = 'path/manual.flac'",
            [],
        )
        .unwrap();

        // 5. Album with art set but art_unset = true on every track (should be flagged)
        let song_unset = Song {
            path: Some("path/unset.flac".to_string()),
            title: Some("Track 1".to_string()),
            artist: Some("Artist E".to_string()),
            album: Some("Album Unset".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            art_embedded: true,
            art_automatic: Some("cover.jpg".to_string()),
            art_manual: None,
            art_unset: true,
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_unset).unwrap();

        // 6. Album where track 1 has art_unset = true, but track 2 has valid art (should NOT be flagged)
        let song_mixed_1 = Song {
            path: Some("path/mixed_1.flac".to_string()),
            title: Some("Track 1".to_string()),
            artist: Some("Artist F".to_string()),
            album: Some("Album Mixed Unset".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            art_embedded: false,
            art_automatic: None,
            art_manual: None,
            art_unset: true,
            unavailable: false,
            ..Default::default()
        };
        let song_mixed_2 = Song {
            path: Some("path/mixed_2.flac".to_string()),
            title: Some("Track 2".to_string()),
            artist: Some("Artist F".to_string()),
            album: Some("Album Mixed Unset".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            art_embedded: true,
            art_automatic: None,
            art_manual: None,
            art_unset: false,
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_mixed_1).unwrap();
        upsert_song(&conn, &song_mixed_2).unwrap();

        let unillustrated = query_unillustrated_albums(&conn).unwrap();
        let unillustrated_titles: Vec<&str> =
            unillustrated.iter().map(|a| a.album.as_str()).collect();

        assert_eq!(unillustrated.len(), 2);
        assert!(unillustrated_titles.contains(&"Album No Art"));
        assert!(unillustrated_titles.contains(&"Album Unset"));
        assert!(!unillustrated_titles.contains(&"Album Embedded"));
        assert!(!unillustrated_titles.contains(&"Album Auto"));
        assert!(!unillustrated_titles.contains(&"Album Manual"));
        assert!(!unillustrated_titles.contains(&"Album Mixed Unset"));
    }

    #[test]
    fn test_incomplete_albums_detection() {
        let (_temp, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        // Single-track release with no track number is NOT flagged as incomplete
        let single_track = Song {
            path: Some("path/single.flac".to_string()),
            title: Some("Single".to_string()),
            artist: Some("Solo Artist".to_string()),
            album: Some("Single Release".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            track: None,
            disc: Some(1),
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &single_track).unwrap();

        // Multi-track album with missing track numbers IS flagged with exact unnumbered_count
        let album_unnum_1 = Song {
            path: Some("path/unnum_1.flac".to_string()),
            title: Some("Track 1".to_string()),
            artist: Some("Band A".to_string()),
            album: Some("Unnumbered Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            track: Some(1),
            disc: Some(1),
            unavailable: false,
            ..Default::default()
        };
        let album_unnum_2 = Song {
            path: Some("path/unnum_2.flac".to_string()),
            title: Some("Track 2".to_string()),
            artist: Some("Band A".to_string()),
            album: Some("Unnumbered Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            track: None, // unnumbered
            disc: Some(1),
            unavailable: false,
            ..Default::default()
        };
        let album_unnum_3 = Song {
            path: Some("path/unnum_3.flac".to_string()),
            title: Some("Track 3".to_string()),
            artist: Some("Band A".to_string()),
            album: Some("Unnumbered Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            track: Some(0), // <= 0 also considered unnumbered
            disc: Some(1),
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &album_unnum_1).unwrap();
        upsert_song(&conn, &album_unnum_2).unwrap();
        upsert_song(&conn, &album_unnum_3).unwrap();

        // Multi-disc album with Track 1 on Disc 1 and Track 1 on Disc 2 is NOT flagged
        let multidisc_d1 = Song {
            path: Some("path/md_d1.flac".to_string()),
            title: Some("Disc 1 Track 1".to_string()),
            artist: Some("Band B".to_string()),
            album: Some("Multi-Disc Complete".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            track: Some(1),
            disc: Some(1),
            unavailable: false,
            ..Default::default()
        };
        let multidisc_d2 = Song {
            path: Some("path/md_d2.flac".to_string()),
            title: Some("Disc 2 Track 1".to_string()),
            artist: Some("Band B".to_string()),
            album: Some("Multi-Disc Complete".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            track: Some(1),
            disc: Some(2),
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &multidisc_d1).unwrap();
        upsert_song(&conn, &multidisc_d2).unwrap();

        // Two tracks on Disc 1 sharing Track 1 IS flagged with duplicate_track_number_count
        let dup_1 = Song {
            path: Some("path/dup_1.flac".to_string()),
            title: Some("First Track 1".to_string()),
            artist: Some("Band C".to_string()),
            album: Some("Duplicate Track Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            track: Some(1),
            disc: Some(1),
            unavailable: false,
            ..Default::default()
        };
        let dup_2 = Song {
            path: Some("path/dup_2.flac".to_string()),
            title: Some("Second Track 1".to_string()),
            artist: Some("Band C".to_string()),
            album: Some("Duplicate Track Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            track: Some(1),
            disc: Some(1),
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &dup_1).unwrap();
        upsert_song(&conn, &dup_2).unwrap();

        // CUE sheet tracks sharing the same path with distinct beginning_nanosec are NOT flagged
        let cue_slice_1 = Song {
            path: Some("path/cue_album.flac".to_string()),
            title: Some("Part 1".to_string()),
            artist: Some("Orchestra".to_string()),
            album: Some("Symphony CUE".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            track: Some(1),
            disc: Some(1),
            beginning_nanosec: 0,
            end_nanosec: 300_000_000_000,
            unavailable: false,
            ..Default::default()
        };
        let cue_slice_2 = Song {
            path: Some("path/cue_album.flac".to_string()),
            title: Some("Part 2".to_string()),
            artist: Some("Orchestra".to_string()),
            album: Some("Symphony CUE".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            track: Some(1), // same track number but distinct cue slice
            disc: Some(1),
            beginning_nanosec: 300_000_000_000,
            end_nanosec: 600_000_000_000,
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &cue_slice_1).unwrap();
        upsert_song(&conn, &cue_slice_2).unwrap();

        let incomplete = query_incomplete_albums(&conn).unwrap();
        assert_eq!(incomplete.len(), 2);

        let unnum_item = incomplete
            .iter()
            .find(|i| i.album.album == "Unnumbered Album")
            .expect("Unnumbered Album should be present");
        assert_eq!(unnum_item.unnumbered_count, 2);
        assert_eq!(unnum_item.duplicate_track_number_count, 0);

        let dup_item = incomplete
            .iter()
            .find(|i| i.album.album == "Duplicate Track Album")
            .expect("Duplicate Track Album should be present");
        assert_eq!(dup_item.unnumbered_count, 0);
        assert_eq!(dup_item.duplicate_track_number_count, 1);

        assert!(!incomplete.iter().any(|i| i.album.album == "Single Release"));
        assert!(!incomplete
            .iter()
            .any(|i| i.album.album == "Multi-Disc Complete"));
        assert!(!incomplete.iter().any(|i| i.album.album == "Symphony CUE"));
    }

    #[test]
    fn test_missing_core_tags_parity_with_collection_scanner() {
        let (_temp, db) = setup_test_db();
        let conn = db.pool.get().unwrap();
        let scanner = CollectionScanner::new(db.clone());

        // Song 1: Full valid tags
        let song_valid = Song {
            path: Some("path/valid.flac".to_string()),
            title: Some("Title".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            unavailable: false,
            not_included: false,
            loved: 0,
            ..Default::default()
        };
        upsert_song(&conn, &song_valid).unwrap();

        // Song 2: Missing title
        let song_no_title = Song {
            path: Some("path/no_title.flac".to_string()),
            title: None,
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            unavailable: false,
            not_included: false,
            loved: 0,
            ..Default::default()
        };
        upsert_song(&conn, &song_no_title).unwrap();

        // Song 3: Empty whitespace artist
        let song_blank_artist = Song {
            path: Some("path/blank_artist.flac".to_string()),
            title: Some("Title".to_string()),
            artist: Some("   ".to_string()),
            album: Some("Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            unavailable: false,
            not_included: false,
            loved: 0,
            ..Default::default()
        };
        upsert_song(&conn, &song_blank_artist).unwrap();

        // Song 4: Missing album
        let song_no_album = Song {
            path: Some("path/no_album.flac".to_string()),
            title: Some("Title".to_string()),
            artist: Some("Artist".to_string()),
            album: None,
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            unavailable: false,
            not_included: false,
            loved: 0,
            ..Default::default()
        };
        upsert_song(&conn, &song_no_album).unwrap();

        // Song 5: Missing title BUT marked not_included (should be excluded)
        let song_not_included = Song {
            path: Some("path/not_included.flac".to_string()),
            title: None,
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_not_included).unwrap();
        conn.execute(
            "UPDATE songs SET not_included = 1 WHERE path = 'path/not_included.flac'",
            [],
        )
        .unwrap();

        // Song 6: Missing title BUT marked loved = -1 (hated / hidden, should be excluded)
        let song_hated = Song {
            path: Some("path/hated.flac".to_string()),
            title: None,
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac,
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_hated).unwrap();
        conn.execute(
            "UPDATE songs SET loved = -1 WHERE path = 'path/hated.flac'",
            [],
        )
        .unwrap();

        // Compare parity
        let count = query_missing_core_tags_count(&conn).unwrap();
        let scanner_songs = scanner
            .get_songs_missing_core_tags(100, QueuePopulationMode::All)
            .unwrap();

        assert_eq!(count, 3);
        assert_eq!(scanner_songs.len(), count);
    }

    #[test]
    fn test_suspected_provenance_query_absent_vs_present() {
        let (_temp, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        // 1. When provenance column is absent: returns empty list without error
        let songs = query_suspected_provenance_songs(&conn).unwrap();
        assert!(songs.is_empty());

        // 2. Add provenance column dynamically
        conn.execute("ALTER TABLE songs ADD COLUMN provenance TEXT", [])
            .unwrap();

        let song_authentic = Song {
            path: Some("path/authentic.flac".to_string()),
            title: Some("Authentic FLAC".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Mp3, // filetype = 1
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_authentic).unwrap();
        conn.execute(
            "UPDATE songs SET provenance = 'authentic' WHERE path = 'path/authentic.flac'",
            [],
        )
        .unwrap();

        let song_suspected = Song {
            path: Some("path/suspected.flac".to_string()),
            title: Some("Suspected Transcode".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Mp3, // filetype = 1
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_suspected).unwrap();
        conn.execute(
            "UPDATE songs SET provenance = 'suspected_lossy_transcode' WHERE path = 'path/suspected.flac'",
            [],
        )
        .unwrap();

        let song_speex_suspected = Song {
            path: Some("path/speex_suspected.flac".to_string()),
            title: Some("Speex Suspected".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::OggSpeex, // filetype = 6
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_speex_suspected).unwrap();
        conn.execute(
            "UPDATE songs SET provenance = 'upsampled_cutoff' WHERE path = 'path/speex_suspected.flac'",
            [],
        )
        .unwrap();

        // Different filetype (e.g. 2 / Flac) should NOT match `filetype IN (1, 6)`
        let song_flac = Song {
            path: Some("path/flac_suspected.flac".to_string()),
            title: Some("FLAC Suspected".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Flac, // filetype = 2
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &song_flac).unwrap();
        conn.execute(
            "UPDATE songs SET provenance = 'suspected_lossy_transcode' WHERE path = 'path/flac_suspected.flac'",
            [],
        )
        .unwrap();

        let suspected_songs = query_suspected_provenance_songs(&conn).unwrap();
        assert_eq!(suspected_songs.len(), 2);
        let titles: Vec<&str> = suspected_songs
            .iter()
            .map(|s| s.title.as_deref().unwrap())
            .collect();
        assert!(titles.contains(&"Suspected Transcode"));
        assert!(titles.contains(&"Speex Suspected"));
        assert!(!titles.contains(&"Authentic FLAC"));
        assert!(!titles.contains(&"FLAC Suspected"));
    }

    #[test]
    fn test_duplicate_track_groups_clustering_and_cue_exclusion() {
        let (_temp, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        // 1. Two duplicate tracks (one FLAC, one MP3 in Duplicates/)
        let track_canonical = Song {
            path: Some("C:/Music/Radiohead/OK Computer/01 - Airbag.flac".to_string()),
            title: Some("Airbag".to_string()),
            artist: Some("Radiohead".to_string()),
            album: Some("OK Computer".to_string()),
            track: Some(1),
            disc: Some(1),
            length_nanosec: Some(284_000_000_000), // 4m 44s
            filetype: FileType::Flac,
            bitdepth: Some(16),
            samplerate: Some(44100),
            source: SongSource::LocalFile,
            genre: Some("Alternative Rock".to_string()),
            rating: 0.8,
            playcount: 15,
            ..Default::default()
        };
        let track_duplicate = Song {
            path: Some("C:/Music/Radiohead/OK Computer/Duplicates/01 - Airbag.mp3".to_string()),
            title: Some("Airbag".to_string()),
            artist: Some("Radiohead".to_string()),
            album: Some("OK Computer".to_string()),
            track: Some(1),
            disc: Some(1),
            length_nanosec: Some(284_000_000_000),
            filetype: FileType::Mp3,
            bitrate: Some(320),
            source: SongSource::LocalFile,
            genre: Some("Rock; 90s".to_string()),
            rating: 1.0,
            playcount: 40,
            ..Default::default()
        };

        // 2. CUE sheet tracks sharing physical file with distinct beginning_nanosec (#78)
        let cue_track_1 = Song {
            path: Some("C:/Music/Pink Floyd/The Dark Side of the Moon/album.flac".to_string()),
            title: Some("Speak to Me".to_string()),
            artist: Some("Pink Floyd".to_string()),
            album: Some("The Dark Side of the Moon".to_string()),
            track: Some(1),
            disc: Some(1),
            beginning_nanosec: 0,
            length_nanosec: Some(65_000_000_000),
            filetype: FileType::Flac,
            source: SongSource::LocalFile,
            ..Default::default()
        };
        let cue_track_2 = Song {
            path: Some("C:/Music/Pink Floyd/The Dark Side of the Moon/album.flac".to_string()),
            title: Some("Breathe".to_string()),
            artist: Some("Pink Floyd".to_string()),
            album: Some("The Dark Side of the Moon".to_string()),
            track: Some(2),
            disc: Some(1),
            beginning_nanosec: 65_000_000_000,
            length_nanosec: Some(169_000_000_000),
            filetype: FileType::Flac,
            source: SongSource::LocalFile,
            ..Default::default()
        };

        upsert_song(&conn, &track_canonical).unwrap();
        upsert_song(&conn, &track_duplicate).unwrap();
        upsert_song(&conn, &cue_track_1).unwrap();
        upsert_song(&conn, &cue_track_2).unwrap();

        let groups = query_duplicate_track_groups(&conn).unwrap();
        assert_eq!(groups.len(), 1, "Expected exactly 1 duplicate group");

        let group = &groups[0];
        assert_eq!(group.title, "Airbag");
        assert_eq!(group.artist, "Radiohead");
        assert_eq!(group.album.as_deref(), Some("OK Computer"));
        assert_eq!(group.candidates.len(), 2);

        // Canonical/best quality copy must be sorted first (non-Duplicates folder + FLAC before MP3 in Duplicates)
        assert_eq!(group.candidates[0].song.filetype, FileType::Flac);
        assert!(!is_duplicates_folder(
            group.candidates[0].song.path.as_deref()
        ));
        assert_eq!(group.candidates[1].song.filetype, FileType::Mp3);
        assert!(is_duplicates_folder(
            group.candidates[1].song.path.as_deref()
        ));
    }

    #[test]
    fn test_duplicate_track_groups_duration_tolerance() {
        let (_temp, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        // Track with length 200s
        let track_a = Song {
            path: Some("C:/Music/Artist/Album/01 Track.flac".to_string()),
            title: Some("Tolerance Track".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            track: Some(1),
            disc: Some(1),
            length_nanosec: Some(200_000_000_000),
            filetype: FileType::Flac,
            source: SongSource::LocalFile,
            ..Default::default()
        };
        // Track with length 204s (within 5s tolerance)
        let track_b = Song {
            path: Some("C:/Music/Artist/Album 2/01 Track.flac".to_string()),
            title: Some("tolerance track".to_string()),
            artist: Some("artist".to_string()),
            album: Some("album".to_string()),
            track: Some(1),
            disc: Some(1),
            length_nanosec: Some(204_000_000_000),
            filetype: FileType::Flac,
            source: SongSource::LocalFile,
            ..Default::default()
        };
        // Track with length 210s (outside 5s tolerance: 10s difference)
        let track_c = Song {
            path: Some("C:/Music/Artist/Album 3/01 Track.flac".to_string()),
            title: Some("Tolerance Track".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            track: Some(1),
            disc: Some(1),
            length_nanosec: Some(210_000_000_000),
            filetype: FileType::Flac,
            source: SongSource::LocalFile,
            ..Default::default()
        };

        upsert_song(&conn, &track_a).unwrap();
        upsert_song(&conn, &track_b).unwrap();
        upsert_song(&conn, &track_c).unwrap();

        let groups = query_duplicate_track_groups(&conn).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].candidates.len(),
            2,
            "Only tracks within 5s tolerance should group"
        );
        let candidate_lengths: Vec<i64> = groups[0]
            .candidates
            .iter()
            .map(|c| c.song.length_nanosec.unwrap())
            .collect();
        assert!(candidate_lengths.contains(&200_000_000_000));
        assert!(candidate_lengths.contains(&204_000_000_000));
        assert!(!candidate_lengths.contains(&210_000_000_000));
    }

    #[test]
    fn test_resolve_duplicate_group_in_db() {
        let (temp_dir_guard, db) = setup_test_db();
        let conn = db.pool.get().unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();

        let file_keep = temp_dir.join("keep.flac");
        let file_dup = temp_dir.join("dup.mp3");
        std::fs::write(&file_keep, b"flac data").unwrap();
        std::fs::write(&file_dup, b"mp3 data").unwrap();

        let song_keep = Song {
            path: Some(file_keep.to_string_lossy().to_string()),
            title: Some("Resolution Song".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            filetype: FileType::Flac,
            source: SongSource::LocalFile,
            playcount: 5,
            ..Default::default()
        };
        let song_dup = Song {
            path: Some(file_dup.to_string_lossy().to_string()),
            title: Some("Resolution Song".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            filetype: FileType::Mp3,
            source: SongSource::LocalFile,
            playcount: 20,
            ..Default::default()
        };

        upsert_song(&conn, &song_keep).unwrap();
        upsert_song(&conn, &song_dup).unwrap();

        let keep_id: i64 = conn
            .query_row(
                "SELECT id FROM songs WHERE path = ?1",
                params![file_keep.to_string_lossy().to_string()],
                |r| r.get(0),
            )
            .unwrap();
        let dup_id: i64 = conn
            .query_row(
                "SELECT id FROM songs WHERE path = ?1",
                params![file_dup.to_string_lossy().to_string()],
                |r| r.get(0),
            )
            .unwrap();

        conn.execute(
            "UPDATE songs SET playcount = 5 WHERE id = ?1",
            params![keep_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE songs SET playcount = 20 WHERE id = ?1",
            params![dup_id],
        )
        .unwrap();

        // 1. Trying to remove the keep_id should bail
        assert!(resolve_duplicate_group_in_db(&conn, keep_id, &[keep_id], false).is_err());

        // 2. Successful resolution with delete_files_from_disk = true
        let deleted = resolve_duplicate_group_in_db(&conn, keep_id, &[dup_id], true).unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0], file_dup.to_string_lossy().to_string());

        // Dup physical file should be deleted on disk
        assert!(!file_dup.exists());
        // Kept file must remain intact
        assert!(file_keep.exists());

        // Kept song should have inherited playcount
        let playcount: i32 = conn
            .query_row(
                "SELECT playcount FROM songs WHERE id = ?1",
                params![keep_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(playcount, 25);
    }

    #[test]
    fn test_corrupt_songs_query_and_mtime_invalidation() {
        let (_temp, db) = setup_test_db();
        let conn = db.pool.get().unwrap();

        // Initially no corrupt songs
        let initial = query_corrupt_songs(&conn).unwrap();
        assert!(initial.is_empty());

        // Insert local song
        conn.execute(
            "INSERT INTO songs (path, title, artist, album, filetype, source, mtime)
             VALUES ('/music/corrupt.mp3', 'Broken', 'Artist', 'Album', 1, 2, 1000)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        // Mark as corrupt in song_integrity
        conn.execute(
            "INSERT INTO song_integrity (song_id, mtime, status, error_detail, verified_at)
             VALUES (?1, 1000, 'corrupt', 'Truncated audio stream', 123456)",
            params![song_id],
        )
        .unwrap();

        let corrupt_list = query_corrupt_songs(&conn).unwrap();
        assert_eq!(corrupt_list.len(), 1);
        assert_eq!(corrupt_list[0].id, song_id);
        assert_eq!(corrupt_list[0].title.as_deref(), Some("Broken"));
        assert_eq!(corrupt_list[0].error_detail, "Truncated audio stream");

        // Verify overview includes the corrupt song
        let overview = compute_curation_overview(&conn).unwrap();
        assert_eq!(overview.corrupt_songs.len(), 1);
        assert_eq!(overview.corrupt_songs[0].id, song_id);

        // When the file is updated on disk (mtime changes in songs table), the cached corrupt status becomes stale and is omitted
        conn.execute(
            "UPDATE songs SET mtime = 2000 WHERE id = ?1",
            params![song_id],
        )
        .unwrap();

        let after_update = query_corrupt_songs(&conn).unwrap();
        assert!(
            after_update.is_empty(),
            "stale corrupt record with mismatched mtime must be excluded"
        );
    }
}
