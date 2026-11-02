//! Curation queries identifying concrete library items that benefit from collector attention (#1586).
//!
//! Provides inspection of local library tracks for:
//! - Albums missing front cover artwork across all tracks
//! - Multi-track albums with incomplete or colliding track numbering
//! - Tracks missing core identity metadata (title, artist, or album)
//! - Lossless tracks with suspected lossy transcode or upsampling provenance warnings

use crate::models::{Song, LOCAL_SOURCES_SQL};
use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

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

/// Comprehensive overview of collection health and curation candidates across local library tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurationOverview {
    pub unillustrated_albums: Vec<CurationAlbumItem>,
    pub incomplete_albums: Vec<IncompleteAlbumItem>,
    pub missing_core_tags_count: usize,
    pub suspected_provenance_songs: Vec<Song>,
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
            WHEN COUNT(DISTINCT NULLIF(songs.album_artist, '')) = 1 THEN MAX(NULLIF(songs.album_artist, ''))
            WHEN COUNT(DISTINCT NULLIF(songs.album_artist, '')) = 0 AND COUNT(DISTINCT NULLIF(songs.artist, '')) = 1 THEN MAX(NULLIF(songs.artist, ''))
            ELSE NULL
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

/// Computes the overall collection curation overview across local library files.
///
/// Ensures album keys are up to date via `repair_album_keys` before executing groupings.
pub fn compute_curation_overview(conn: &Connection) -> Result<CurationOverview> {
    crate::album_key::repair_album_keys(conn)?;

    let unillustrated_albums = query_unillustrated_albums(conn)?;
    let incomplete_albums = query_incomplete_albums(conn)?;
    let missing_core_tags_count = query_missing_core_tags_count(conn)?;
    let suspected_provenance_songs = query_suspected_provenance_songs(conn)?;

    Ok(CurationOverview {
        unillustrated_albums,
        incomplete_albums,
        missing_core_tags_count,
        suspected_provenance_songs,
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
}
