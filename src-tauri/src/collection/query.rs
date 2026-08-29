//! Read-query API for the library: search, browse-by-album/artist/genre/
//! decade/BPM, home-screen sections (recently played/added, most frequently
//! played), and artist-profile reads. Split out of `collection.rs` (#577
//! item 17) — a second `impl CollectionScanner` block alongside the one in
//! `collection.rs` that owns scanning/directory management.

use super::{
    mode_query_fragments, parse_decade_range, row_to_song, CollectionScanner, SONG_SELECT_COLS,
    SONG_SELECT_COLS_QUALIFIED, SONG_SELECT_COL_COUNT,
};
use crate::models::{
    AlbumItem, ArtistProfile, ArtistSocialLink, HomeItem, LibraryStats, Playlist,
    QueuePopulationMode, Song,
};
use anyhow::Result;
use rusqlite::{params, ToSql};

impl CollectionScanner {
    /// Full-text + field search across the library.
    pub fn search_songs(&self, query: &str, limit: i64) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let query_trimmed = query.trim();
        if query_trimmed.is_empty() {
            let sql = format!(
                "SELECT {} FROM songs WHERE unavailable = 0 ORDER BY COALESCE(album_artist_sort, album_artist), COALESCE(albumsort, album), disc, track LIMIT ?1",
                SONG_SELECT_COLS
            );
            let mut stmt = conn.prepare(&sql)?;
            let songs = stmt
                .query_map(params![limit], row_to_song)?
                .filter_map(|r| r.ok())
                .collect();
            return Ok(songs);
        }

        let parsed = crate::filter_parser::parse_query(query_trimmed);

        let mut where_clauses = vec!["unavailable = 0".to_string()];
        let mut query_params: Vec<Box<dyn ToSql>> = Vec::new();

        // If bare terms exist, match against FTS5 or LIKE
        if !parsed.bare_terms.is_empty() {
            let bare_str = parsed.bare_terms.join(" ");
            let fts_query = format!("{bare_str}*");
            let like_query = format!("%{bare_str}%");

            query_params.push(Box::new(fts_query));
            let fts_param_idx = query_params.len();
            query_params.push(Box::new(like_query));
            let like_param_idx = query_params.len();

            let bare_sql = format!(
                "(id IN (SELECT rowid FROM songs_fts WHERE songs_fts MATCH ?{fts_param_idx}) OR (title LIKE ?{like_param_idx} OR artist LIKE ?{like_param_idx} OR album LIKE ?{like_param_idx}))"
            );
            where_clauses.push(bare_sql);
        }

        // Add qualified field filter clauses
        for filter in parsed.field_filters {
            let param_idx = query_params.len() + 1;
            let clause = filter.to_sql_clause(param_idx);
            query_params.push(Box::new(filter.value));
            where_clauses.push(clause);
        }

        query_params.push(Box::new(limit));
        let limit_param_idx = query_params.len();

        let where_str = where_clauses.join(" AND ");
        let sql = format!(
            "SELECT {} FROM songs WHERE {} ORDER BY COALESCE(album_artist_sort, album_artist), COALESCE(albumsort, album), disc, track LIMIT ?{}",
            SONG_SELECT_COLS, where_str, limit_param_idx
        );

        let params_refs: Vec<&dyn ToSql> = query_params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params_refs.as_slice(), row_to_song)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(songs)
    }

    /// Same rule-query parsing as `search_songs`, but replaces the default
    /// alphabetical ordering with `mode`'s weighted-random bias (see #120),
    /// for populating a Smart Playlist that has a `population_mode` set.
    /// `query` is expected to be a non-empty filter-rule string (as produced
    /// from a playlist's `dynamic_spec`), not a blank/browse-all query.
    pub fn search_songs_by_mode(
        &self,
        query: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);

        let parsed = crate::filter_parser::parse_query(query.trim());

        let mut where_clauses = vec!["unavailable = 0".to_string()];
        if !extra_where.is_empty() {
            where_clauses.push(extra_where.trim_start_matches(" AND ").to_string());
        }
        let mut query_params: Vec<Box<dyn ToSql>> = Vec::new();

        if !parsed.bare_terms.is_empty() {
            let bare_str = parsed.bare_terms.join(" ");
            let fts_query = format!("{bare_str}*");
            let like_query = format!("%{bare_str}%");

            query_params.push(Box::new(fts_query));
            let fts_param_idx = query_params.len();
            query_params.push(Box::new(like_query));
            let like_param_idx = query_params.len();

            let bare_sql = format!(
                "(id IN (SELECT rowid FROM songs_fts WHERE songs_fts MATCH ?{fts_param_idx}) OR (title LIKE ?{like_param_idx} OR artist LIKE ?{like_param_idx} OR album LIKE ?{like_param_idx}))"
            );
            where_clauses.push(bare_sql);
        }

        for filter in parsed.field_filters {
            let param_idx = query_params.len() + 1;
            let clause = filter.to_sql_clause(param_idx);
            query_params.push(Box::new(filter.value));
            where_clauses.push(clause);
        }

        query_params.push(Box::new(limit));
        let limit_param_idx = query_params.len();

        let where_str = where_clauses.join(" AND ");
        let sql = format!(
            "SELECT {} FROM songs WHERE {} ORDER BY {} LIMIT ?{}",
            SONG_SELECT_COLS, where_str, order_by, limit_param_idx
        );

        let params_refs: Vec<&dyn ToSql> = query_params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params_refs.as_slice(), row_to_song)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(songs)
    }

    /// Paginated library listing — every available local/collection song
    /// (not streaming sources), ordered by album artist/album/disc/track.
    pub fn get_songs(&self, limit: i64, offset: i64) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE source IN (1, 2) AND unavailable = 0
             ORDER BY COALESCE(album_artist_sort, album_artist), COALESCE(albumsort, album), disc, track
             LIMIT ?1 OFFSET ?2",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![limit, offset], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    pub fn get_songs_by_album(&self, album: &str) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE album = ?1
               AND source IN (1, 2)
               AND unavailable = 0
             ORDER BY disc, track",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![album], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Matches `artist` against either the per-track `artist` column or the
    /// `album_artist` column (via `multi_value_contains_pattern`), whichever
    /// contains it as one of its individual `; `-delimited values — not just
    /// the "effective" (album_artist-preferred) column this used to check
    /// with a single exact-equality comparison. Checking both columns
    /// separately, rather than collapsing to one via `COALESCE` first,
    /// matters for two real cases a single-column check misses:
    /// - A collab/featured credit on an otherwise normal (non-compilation)
    ///   album, e.g. artist = "Evergrey; Mikael Stanne", album_artist =
    ///   "Evergrey" — clicking "Mikael Stanne" only finds this track by
    ///   checking the raw `artist` column, since `album_artist` alone would
    ///   win under a `COALESCE` and never mention him.
    /// - A Various Artists compilation track, e.g. artist = "Artist X",
    ///   album_artist = "Various Artists" — clicking "Artist X" only finds
    ///   it by checking `artist` directly (this case was previously only
    ///   reachable via the separate album-level `get_compilations_by_artist`
    ///   query, which still exists for the album-card view of the same data).
    ///
    /// The library-wide Artists browse grouping (`get_artists`) is
    /// intentionally *not* fanned out the same way — a collab track still
    /// contributes to one combined "Evergrey; Mikael Stanne" card there
    /// rather than to both artists' cards/counts. Full fan-out of that
    /// grouping is tracked as separate follow-up work, same scope decision
    /// #143 made for genre.
    pub fn get_songs_by_artist(&self, artist: &str) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE ({} OR {})
               AND source IN (1, 2)
               AND unavailable = 0
             ORDER BY COALESCE(albumsort, album), disc, track",
            SONG_SELECT_COLS,
            multi_value_contains_sql("COALESCE(artist, '')", "?1"),
            multi_value_contains_sql("COALESCE(album_artist, '')", "?1")
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![multi_value_contains_pattern(artist)], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Compilation albums featuring `artist` on at least one track — the
    /// mirror image of `get_songs_by_artist`'s effective-artist match: this
    /// matches on the *raw per-track* `artist` column (via
    /// `multi_value_contains_pattern`, so a per-track collab credit still
    /// matches), since a Various Artists compilation's effective
    /// (album-level) artist is never the individual track artist, so it
    /// would otherwise never surface on that artist's own detail page
    /// (#343). An album counts as a compilation if any track has
    /// `compilation = 1`, its tracks disagree on `album_artist`, or
    /// `album_artist` is literally "Various Artists". Returns the same
    /// aggregated shape as `get_albums()`, but with `artist` always
    /// "Various Artists" since these are compilations by definition.
    pub fn get_compilations_by_artist(&self, artist: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT
                album,
                MIN(year) AS year,
                COUNT(*) AS track_count,
                MAX(COALESCE(disc, 1)) AS disc_count,
                MAX(CAST(art_embedded AS INTEGER)) AS art_embedded,
                MAX(art_automatic) AS art_automatic,
                MAX(art_manual) AS art_manual,
                (
                    SELECT genre
                    FROM songs g
                    WHERE g.album = songs.album AND g.source IN (1, 2) AND g.unavailable = 0
                      AND g.genre IS NOT NULL AND g.genre != ''
                    GROUP BY genre
                    ORDER BY COUNT(*) DESC, COALESCE(genresort, genre) ASC
                    LIMIT 1
                ) AS genre,
                COALESCE(
                    (SELECT rating FROM album_ratings ar WHERE ar.album_key = songs.album),
                    -1
                ) AS rating,
                MAX(added) AS added,
                COALESCE(SUM(length_nanosec), 0) AS total_duration_nanosec
             FROM songs
             WHERE source IN (1, 2) AND unavailable = 0 AND album IS NOT NULL AND album != ''
               AND album IN (
                 SELECT album FROM songs s2
                 WHERE s2.source IN (1, 2) AND s2.unavailable = 0 AND {}
               )
               AND album IN (
                 SELECT album FROM songs s3
                 WHERE s3.source IN (1, 2) AND s3.unavailable = 0
                 GROUP BY album
                 -- Mirrors get_albums()'s various-artists fallback: a compilation
                 -- either has TCMP set, is explicitly credited to Various
                 -- Artists, or fails to agree on a single effective album
                 -- artist (the same condition that makes get_albums() emit
                 -- NULL and the UI fall back to displaying Various Artists).
                 HAVING MAX(s3.compilation) = 1
                    OR MAX(CASE WHEN s3.album_artist = 'Various Artists' THEN 1 ELSE 0 END) = 1
                    OR NOT (
                         COUNT(DISTINCT NULLIF(s3.album_artist, '')) = 1
                         OR (
                           COUNT(DISTINCT NULLIF(s3.album_artist, '')) = 0
                           AND COUNT(DISTINCT NULLIF(s3.artist, '')) = 1
                         )
                       )
               )
             GROUP BY album
             ORDER BY COALESCE(MAX(albumsort), album)",
            multi_value_contains_sql("s2.artist", "?1")
        );
        let mut stmt = conn.prepare(&sql)?;
        let albums: Vec<serde_json::Value> = stmt
            .query_map(params![multi_value_contains_pattern(artist)], |row| {
                Ok(serde_json::json!({
                    "artist": "Various Artists",
                    "album": row.get::<_, Option<String>>(0)?,
                    "year": row.get::<_, Option<i32>>(1)?,
                    "track_count": row.get::<_, i32>(2)?,
                    "disc_count": row.get::<_, i32>(3)?,
                    "art_embedded": row.get::<_, bool>(4)?,
                    "art_automatic": row.get::<_, Option<String>>(5)?,
                    "art_manual": row.get::<_, Option<String>>(6)?,
                    "genre": row.get::<_, Option<String>>(7)?,
                    "rating": row.get::<_, f32>(8)?,
                    "added": row.get::<_, Option<i64>>(9)?,
                    "total_duration_nanosec": row.get::<_, i64>(10)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(albums)
    }

    /// Songs favourited via the 5-star/heart rating, for the "Favourites" auto-playlist.
    pub fn get_favourite_songs(&self) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE rating = 5
               AND source IN (1, 2)
               AND unavailable = 0
             ORDER BY COALESCE(album_artist_sort, album_artist), COALESCE(albumsort, album), disc, track",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map([], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Most recently added songs, for the "Recently Added" auto-playlist.
    pub fn get_recently_added_songs(&self, limit: i64) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE source IN (1, 2)
               AND unavailable = 0
               AND added IS NOT NULL
             ORDER BY added DESC
             LIMIT ?1",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Distinct non-empty genres present in the library, used to build one
    /// auto-playlist per genre.
    pub fn get_library_genres(&self) -> Result<Vec<String>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT genre FROM songs
             WHERE source IN (1, 2)
               AND unavailable = 0
               AND genre IS NOT NULL
               AND genre != ''
             ORDER BY COALESCE(genresort, genre)",
        )?;
        let genres = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(genres)
    }

    /// Distinct decades present in the library (e.g. "1980s", "1990s"), used to build one
    /// auto-playlist per decade.
    pub fn get_library_decades(&self) -> Result<Vec<String>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT (COALESCE(year, originalyear) / 10 * 10) AS decade_start
             FROM songs
             WHERE source IN (1, 2)
               AND unavailable = 0
               AND COALESCE(year, originalyear) IS NOT NULL
               AND COALESCE(year, originalyear) >= 1000
               AND COALESCE(year, originalyear) <= 9999
             ORDER BY decade_start ASC",
        )?;
        let decades = stmt
            .query_map([], |row| {
                let start: i32 = row.get(0)?;
                Ok(format!("{}s", start))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(decades)
    }

    /// Songs in a decade (e.g. "1980s"), selected per `mode`'s bias (see
    /// #120), for per-decade auto-playlists.
    pub fn get_songs_by_decade(
        &self,
        decade: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let (start, end) = match parse_decade_range(decade) {
            Some(range) => range,
            None => return Ok(Vec::new()),
        };
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
        let sql = format!(
            "SELECT {} FROM songs
             WHERE COALESCE(year, originalyear) >= ?1
               AND COALESCE(year, originalyear) <= ?2
               AND source IN (1, 2)
               AND unavailable = 0
               {extra_where}
             ORDER BY {order_by}
             LIMIT ?3",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![start, end, limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Songs whose BPM falls within `[min, max]` (an unbounded `max` means "or
    /// higher"), selected per `mode`'s bias (see #120), for the fixed-bucket
    /// BPM auto-playlists (Down-Tempo, Mid-Tempo, Uptempo, High Energy, Extreme).
    pub fn get_songs_by_bpm_range(
        &self,
        min: f64,
        max: Option<f64>,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
        let upper_bound = match max {
            Some(_) => "AND bpm <= ?2",
            None => "",
        };
        let sql = format!(
            "SELECT {} FROM songs
             WHERE bpm >= ?1
               {upper_bound}
               AND source IN (1, 2)
               AND unavailable = 0
               {extra_where}
             ORDER BY {order_by}
             LIMIT ?3",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![min, max.unwrap_or(0.0), limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Songs by artists having a given custom profile tag (e.g. "canadian"),
    /// selected per `mode`'s bias (see #120), for per-artist-tag auto-playlists.
    pub fn get_songs_by_artist_tag(
        &self,
        tag: &str,
        limit: i64,
        mode: QueuePopulationMode,
    ) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let (extra_where, order_by) = mode_query_fragments(mode);
        let sql = format!(
            "SELECT {} FROM songs
             WHERE COALESCE(NULLIF(album_artist, ''), artist) IN (
                 SELECT artist_key FROM artist_profiles, json_each(artist_profiles.tags)
                 WHERE json_each.value = ?1 COLLATE NOCASE
             )
               AND source IN (1, 2)
               AND unavailable = 0
               {extra_where}
             ORDER BY {order_by}
             LIMIT ?2",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![tag, limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    /// Distinct artist tags across all artist profiles in the library.
    pub fn get_library_artist_tags(&self) -> Result<Vec<String>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT json_each.value
             FROM artist_profiles, json_each(artist_profiles.tags)
             ORDER BY json_each.value COLLATE NOCASE",
        )?;
        let tags = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }

    /// One aggregated entry per distinct `album` value across the whole
    /// library (untyped JSON, not a `Song`/`AlbumItem` — see the query below
    /// for the exact field set), each summarizing every track that shares
    /// that album name regardless of which artist tagged it.
    pub fn get_albums(&self) -> Result<Vec<serde_json::Value>> {
        let conn = self.db.pool.get()?;
        // Group only by album name so that tracks with different per-track artists
        // but the same album title are consolidated into a single entry.
        // album_artist is taken as the shared value when all tracks agree on it;
        // if they differ (true various-artist albums), it comes back as NULL.
        let mut stmt = conn.prepare(
            "SELECT
                CASE
                    WHEN COUNT(DISTINCT NULLIF(album_artist, '')) = 1 THEN MAX(NULLIF(album_artist, ''))
                    WHEN COUNT(DISTINCT NULLIF(album_artist, '')) = 0 AND COUNT(DISTINCT NULLIF(artist, '')) = 1 THEN MAX(NULLIF(artist, ''))
                    ELSE NULL
                END AS album_artist,
                album,
                MIN(year) AS year,
                COUNT(*) AS track_count,
                MAX(COALESCE(disc, 1)) AS disc_count,
                MAX(CAST(art_embedded AS INTEGER)) AS art_embedded,
                MAX(art_automatic) AS art_automatic,
                MAX(art_manual) AS art_manual,
                (
                    SELECT genre
                    FROM songs g
                    WHERE g.album = songs.album AND g.source IN (1, 2) AND g.unavailable = 0
                      AND g.genre IS NOT NULL AND g.genre != ''
                    GROUP BY genre
                    ORDER BY COUNT(*) DESC, COALESCE(genresort, genre) ASC
                    LIMIT 1
                ) AS genre,
                COALESCE(
                    (SELECT rating FROM album_ratings ar WHERE ar.album_key = songs.album),
                    -1
                ) AS rating,
                MAX(added) AS added,
                COALESCE(SUM(length_nanosec), 0) AS total_duration_nanosec,
                COALESCE(MAX(NULLIF(album_artist_sort, '')), MAX(NULLIF(artistsort, ''))) AS artist_sort,
                MAX(NULLIF(albumsort, '')) AS albumsort
             FROM songs
             WHERE source IN (1, 2) AND album IS NOT NULL AND album != '' AND unavailable = 0
             GROUP BY album
             ORDER BY COALESCE(MAX(album_artist_sort), MAX(artistsort), MAX(album_artist), MAX(artist)), COALESCE(MAX(albumsort), album)",
        )?;
        let albums: Vec<serde_json::Value> = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "artist": row.get::<_, Option<String>>(0)?,
                    "album": row.get::<_, Option<String>>(1)?,
                    "year": row.get::<_, Option<i32>>(2)?,
                    "track_count": row.get::<_, i32>(3)?,
                    "disc_count": row.get::<_, i32>(4)?,
                    "art_embedded": row.get::<_, bool>(5)?,
                    "art_automatic": row.get::<_, Option<String>>(6)?,
                    "art_manual": row.get::<_, Option<String>>(7)?,
                    "genre": row.get::<_, Option<String>>(8)?,
                    "rating": row.get::<_, f32>(9)?,
                    "added": row.get::<_, Option<i64>>(10)?,
                    "total_duration_nanosec": row.get::<_, i64>(11)?,
                    "artist_sort": row.get::<_, Option<String>>(12)?,
                    "albumsort": row.get::<_, Option<String>>(13)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(albums)
    }

    /// One aggregated entry per effective artist (album_artist, falling back
    /// to artist) across the library — untyped JSON, see the query below for
    /// the exact field set. Multi-value album_artist/artist columns group as
    /// one combined pseudo-artist rather than fanning out per individual
    /// artist — see `get_songs_by_artist` for why that's out of scope here.
    pub fn get_artists(&self) -> Result<Vec<serde_json::Value>> {
        let conn = self.db.pool.get()?;
        // Artists are grouped case-insensitively (COLLATE NOCASE) so that tag-casing
        // drift across files/albums for the same real-world artist (e.g. "The War On
        // Drugs" vs "The War on Drugs") doesn't show up as two separate cards — see
        // issue #295. `MIN(...)` picks one deterministic casing per group to display.
        let mut stmt = conn.prepare(
            "WITH album_counts AS (
                SELECT album, COUNT(*) AS track_count
                FROM songs
                WHERE source IN (1, 2) AND album IS NOT NULL AND album != '' AND unavailable = 0
                GROUP BY album
             ),
             base AS (
                SELECT s.id, s.album,
                       COALESCE(NULLIF(s.album_artist, ''), s.artist, '') AS effective_artist,
                       COALESCE(NULLIF(s.album_artist_sort, ''), NULLIF(s.album_artist, ''), NULLIF(s.artistsort, ''), s.artist, '') AS sort_artist
                FROM songs s
                WHERE s.source IN (1, 2) AND s.unavailable = 0
             ),
             grouped AS (
                SELECT MIN(effective_artist) AS effective_artist,
                       MIN(sort_artist) AS sort_artist,
                       COUNT(*) AS song_count
                FROM base
                GROUP BY effective_artist COLLATE NOCASE
             )
             SELECT
                g.effective_artist,
                (
                    SELECT COUNT(DISTINCT CASE WHEN ac.track_count > 7 THEN b.album END)
                    FROM base b
                    LEFT JOIN album_counts ac ON b.album = ac.album
                    WHERE b.effective_artist = g.effective_artist COLLATE NOCASE
                ) AS album_count,
                g.song_count,
                (
                    SELECT genre
                    FROM songs sg
                    WHERE COALESCE(NULLIF(sg.album_artist, ''), sg.artist, '') = g.effective_artist COLLATE NOCASE
                      AND sg.source IN (1, 2) AND sg.unavailable = 0 AND sg.genre IS NOT NULL AND sg.genre != ''
                    GROUP BY sg.genre
                    ORDER BY COUNT(*) DESC, COALESCE(sg.genresort, sg.genre) ASC
                    LIMIT 1
                ) AS genre,
                g.sort_artist
             FROM grouped g
             ORDER BY g.sort_artist COLLATE NOCASE",
        )?;
        let artists: Vec<serde_json::Value> = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "name": row.get::<_, Option<String>>(0)?,
                    "album_count": row.get::<_, i32>(1)?,
                    "song_count": row.get::<_, i32>(2)?,
                    "genre": row.get::<_, Option<String>>(3)?,
                    "sort_artist": row.get::<_, Option<String>>(4)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(artists)
    }

    /// Artists ranked by total play count across their songs (ties broken
    /// alphabetically). When the library has no play history at all (a
    /// freshly scanned collection), falls back to ranking by song count
    /// instead of excluding every artist — this powers the Home "Top
    /// Artists" carousel, not a full artist directory (see `get_artists`
    /// for that).
    pub fn get_top_artists(&self, limit: i64) -> Result<Vec<serde_json::Value>> {
        let conn = self.db.pool.get()?;
        // See get_artists() for why grouping is case-insensitive (issue #295).
        let mut stmt = conn.prepare(
            "WITH album_counts AS (
                SELECT album, COUNT(*) AS track_count
                FROM songs
                WHERE source IN (1, 2) AND album IS NOT NULL AND album != '' AND unavailable = 0
                GROUP BY album
             ),
             base AS (
                SELECT s.id, s.album, s.playcount,
                       COALESCE(NULLIF(s.album_artist, ''), s.artist, '') AS effective_artist,
                       COALESCE(NULLIF(s.album_artist_sort, ''), NULLIF(s.album_artist, ''), NULLIF(s.artistsort, ''), s.artist, '') AS sort_artist
                FROM songs s
                WHERE s.source IN (1, 2) AND s.unavailable = 0
             ),
             totals AS (
                SELECT SUM(COALESCE(playcount, 0)) AS lib_total_playcount FROM base
             ),
             grouped AS (
                SELECT
                    MIN(effective_artist) AS effective_artist,
                    MIN(sort_artist) AS sort_artist,
                    COUNT(*) AS song_count,
                    SUM(COALESCE(playcount, 0)) AS total_playcount
                FROM base
                GROUP BY effective_artist COLLATE NOCASE
             )
             SELECT
                g.effective_artist,
                (
                    SELECT COUNT(DISTINCT CASE WHEN ac.track_count > 7 THEN b.album END)
                    FROM base b
                    LEFT JOIN album_counts ac ON b.album = ac.album
                    WHERE b.effective_artist = g.effective_artist COLLATE NOCASE
                ) AS album_count,
                g.song_count,
                g.total_playcount,
                (
                    SELECT genre
                    FROM songs sg
                    WHERE COALESCE(NULLIF(sg.album_artist, ''), sg.artist, '') = g.effective_artist COLLATE NOCASE
                      AND sg.source IN (1, 2) AND sg.unavailable = 0 AND sg.genre IS NOT NULL AND sg.genre != ''
                    GROUP BY sg.genre
                    ORDER BY COUNT(*) DESC, COALESCE(sg.genresort, sg.genre) ASC
                    LIMIT 1
                ) AS genre
             FROM grouped g, totals t
             ORDER BY
                g.total_playcount DESC,
                CASE WHEN t.lib_total_playcount = 0 THEN g.song_count END DESC,
                g.sort_artist COLLATE NOCASE
             LIMIT ?1",
        )?;
        let artists: Vec<serde_json::Value> = stmt
            .query_map(params![limit], |row| {
                Ok(serde_json::json!({
                    "name": row.get::<_, Option<String>>(0)?,
                    "album_count": row.get::<_, i32>(1)?,
                    "song_count": row.get::<_, i32>(2)?,
                    "genre": row.get::<_, Option<String>>(4)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(artists)
    }

    /// Retrieve the customizable profile (website, tags, social links, bio)
    /// for an artist (#473). Returns empty/default profile if none saved yet.
    pub fn get_artist_profile(&self, artist: &str) -> Result<ArtistProfile> {
        let conn = self.db.pool.get()?;
        get_artist_profile_conn(&conn, artist)
    }

    /// Save or update an artist's customizable profile (#473).
    pub fn set_artist_profile(&self, profile: &ArtistProfile) -> Result<ArtistProfile> {
        let conn = self.db.pool.get()?;
        set_artist_profile_conn(&conn, profile)
    }

    /// Retrieve all custom artist profiles in the library (#473).
    pub fn get_all_artist_profiles(&self) -> Result<Vec<ArtistProfile>> {
        let conn = self.db.pool.get()?;
        get_all_artist_profiles_conn(&conn)
    }

    pub fn get_library_stats(&self) -> Result<LibraryStats> {
        let conn = self.db.pool.get()?;
        let stats = conn.query_row(
            "SELECT
                COUNT(*) as total_songs,
                COUNT(DISTINCT COALESCE(NULLIF(album_artist,''), artist)) as total_artists,
                COUNT(DISTINCT album) as total_albums,
                COALESCE(SUM(length_nanosec), 0) as total_duration,
                COALESCE(SUM(filesize), 0) as total_filesize
             FROM songs WHERE source IN (1, 2) AND unavailable = 0",
            [],
            |row| {
                Ok(LibraryStats {
                    total_songs: row.get(0)?,
                    total_artists: row.get(1)?,
                    total_albums: row.get(2)?,
                    total_duration_nanosec: row.get(3)?,
                    total_filesize_bytes: row.get(4)?,
                })
            },
        )?;
        Ok(stats)
    }

    /// Flat list of the `limit` most recently played distinct songs, one row
    /// per song regardless of how many times or contexts it was played in.
    /// Unlike `get_recently_played`, this doesn't group by playback context
    /// into Album/Playlist/Song cards — used where a plain song list is
    /// wanted (e.g. building a "Recently Played" auto-playlist) rather than
    /// Home-screen cards.
    pub fn get_recently_played_songs(&self, limit: i64) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {SONG_SELECT_COLS}
             FROM songs s
             JOIN (
                 SELECT song_id, MAX(played_at) as last_played_at
                 FROM play_history
                 GROUP BY song_id
             ) ph ON s.id = ph.song_id
             WHERE s.source IN (1, 2) AND s.unavailable = 0
             ORDER BY ph.last_played_at DESC
             LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![limit], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    pub fn clear_play_history(&self) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute("DELETE FROM play_history", [])?;
        Ok(())
    }

    /// Recently played, grouped by what the user actually played from —
    /// an Album card if they were browsing an album, a Playlist card if
    /// they played from a playlist, or a Song card for a standalone pick.
    /// See `play_history` (migration 10) and `PlayContext`.
    pub fn get_recently_played(&self, limit: i64) -> Result<Vec<HomeItem>> {
        let conn = self.db.pool.get()?;
        // Every track of an album collapses into a single Album card, so
        // `limit` raw song rows can produce far fewer than `limit` HomeItems.
        // 20x is a heuristic overfetch so grouping still has enough rows to
        // reach `limit` items for a normal-sized library; it isn't a
        // guarantee for pathological cases (e.g. one giant album).
        let query_limit = limit * 20;
        let home_item_select_cols = home_item_select_cols();
        let sql = format!(
            "SELECT {home_item_select_cols}, ph.context_type, ph.playlist_id
             FROM play_history ph
             JOIN songs s ON s.id = ph.song_id
             WHERE s.source IN (1, 2) AND s.unavailable = 0
               AND NOT (
                   ph.context_type = 'playlist'
                   AND ph.playlist_id IN (
                       SELECT id FROM playlists WHERE dynamic_enabled = 0 AND LOWER(name) = 'queue'
                   )
               )
             ORDER BY ph.played_at DESC
             LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows: Vec<(Song, i64, i64, String, Option<i64>)> = stmt
            .query_map(params![query_limit], |row| {
                let song = row_to_song(row)?;
                let album_track_count: i64 = row.get(SONG_SELECT_COL_COUNT)?;
                let album_disc_count: i64 = row.get(SONG_SELECT_COL_COUNT + 1)?;
                let context_type: String = row.get(SONG_SELECT_COL_COUNT + 2)?;
                let playlist_id: Option<i64> = row.get(SONG_SELECT_COL_COUNT + 3)?;
                Ok((
                    song,
                    album_track_count,
                    album_disc_count,
                    context_type,
                    playlist_id,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let playlist_ids: Vec<i64> = {
            use std::collections::HashSet;
            rows.iter()
                .filter(|(_, _, _, context_type, playlist_id)| {
                    context_type == "playlist" && playlist_id.is_some()
                })
                .filter_map(|(_, _, _, _, playlist_id)| *playlist_id)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect()
        };
        let playlists_by_id = get_playlists_by_ids(&conn, &playlist_ids)?;

        Ok(group_by_play_context(
            rows,
            limit as usize,
            &playlists_by_id,
        ))
    }

    /// Most frequently played, grouped the same way as `get_recently_played`
    /// (Album/Playlist/Song by recorded context) but ordered by total play
    /// count per context instead of recency.
    pub fn get_most_frequently_played(&self, limit: i64) -> Result<Vec<HomeItem>> {
        let conn = self.db.pool.get()?;
        let sql = "
            SELECT
                ph.context_type,
                ph.playlist_id,
                COUNT(*) as play_count,
                MAX(ph.played_at) as last_played,
                MAX(ph.song_id) as representative_song_id
            FROM play_history ph
            JOIN songs s ON s.id = ph.song_id
            WHERE s.source IN (1, 2) AND s.unavailable = 0
              AND NOT (
                  ph.context_type = 'playlist'
                  AND ph.playlist_id IN (
                      SELECT id FROM playlists WHERE dynamic_enabled = 0 AND LOWER(name) = 'queue'
                  )
              )
            GROUP BY
                ph.context_type,
                CASE ph.context_type WHEN 'playlist' THEN ph.playlist_id END,
                CASE ph.context_type WHEN 'album' THEN s.album END,
                CASE ph.context_type WHEN 'album' THEN COALESCE(s.album_artist, s.artist) END,
                CASE ph.context_type WHEN 'song' THEN ph.song_id END
            ORDER BY play_count DESC, last_played DESC
            LIMIT ?1
        ";
        let mut stmt = conn.prepare(sql)?;
        struct AggRow {
            context_type: String,
            playlist_id: Option<i64>,
            representative_song_id: i64,
        }
        let agg_rows: Vec<AggRow> = stmt
            .query_map(params![limit], |row| {
                Ok(AggRow {
                    context_type: row.get(0)?,
                    playlist_id: row.get(1)?,
                    representative_song_id: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        let playlist_ids: Vec<i64> = {
            use std::collections::HashSet;
            agg_rows
                .iter()
                .filter(|r| r.context_type == "playlist")
                .filter_map(|r| r.playlist_id)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect()
        };
        let playlists_by_id = get_playlists_by_ids(&conn, &playlist_ids)?;

        let song_ids: Vec<i64> = agg_rows.iter().map(|r| r.representative_song_id).collect();
        let songs_by_id = get_songs_by_ids(&conn, &song_ids)?;

        let mut items: Vec<HomeItem> = agg_rows
            .into_iter()
            .filter_map(|row| {
                let (song, album_track_count, album_disc_count) =
                    songs_by_id.get(&row.representative_song_id)?.clone();
                Some(home_item_for_context(
                    &row.context_type,
                    row.playlist_id,
                    song,
                    album_track_count,
                    album_disc_count,
                    &playlists_by_id,
                ))
            })
            .collect();
        attach_album_ratings(&conn, &mut items)?;
        Ok(items)
    }

    /// Recently added songs grouped into Album cards where an album's other
    /// tracks were also added together, or standalone Song cards otherwise —
    /// same grouping mechanism as `get_recently_played`, see its comments.
    pub fn get_recently_added(&self, limit: i64) -> Result<Vec<HomeItem>> {
        let conn = self.db.pool.get()?;
        // See get_recently_played's identical overfetch-then-group comment.
        let query_limit = limit * 20;
        let home_item_select_cols = home_item_select_cols();
        let sql = format!(
            "SELECT {home_item_select_cols}
             FROM songs s
             WHERE s.source IN (1, 2) AND s.unavailable = 0 AND s.added IS NOT NULL
             ORDER BY s.added DESC
             LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs_with_counts: Vec<(Song, i64, i64)> = stmt
            .query_map(params![query_limit], |row| {
                let song = row_to_song(row)?;
                let count: i64 = row.get(SONG_SELECT_COL_COUNT)?;
                let disc_count: i64 = row.get(SONG_SELECT_COL_COUNT + 1)?;
                Ok((song, count, disc_count))
            })?
            .filter_map(|r| r.ok())
            .collect();
        let mut items = group_songs_into_home_items(songs_with_counts, limit as usize);
        attach_album_ratings(&conn, &mut items)?;
        Ok(items)
    }

    /// A shuffled sample of full albums in the library, for users with
    /// little or no play history yet. Reuses the same Album grouping as
    /// `get_recently_added`; only the ordering (random) and the album-only
    /// filter differ. Reshuffles on every call by design — freshness over
    /// stability across refreshes.
    pub fn get_featured_albums(&self, limit: i64) -> Result<Vec<HomeItem>> {
        let conn = self.db.pool.get()?;
        // See get_recently_played's identical overfetch-then-group comment.
        let query_limit = limit * 20;
        let home_item_select_cols = home_item_select_cols();
        let sql = format!(
            "SELECT {home_item_select_cols}
             FROM songs s
             WHERE s.source IN (1, 2) AND s.unavailable = 0
               AND s.album IS NOT NULL AND s.album != ''
             ORDER BY RANDOM()
             LIMIT ?1"
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs_with_counts: Vec<(Song, i64, i64)> = stmt
            .query_map(params![query_limit], |row| {
                let song = row_to_song(row)?;
                let count: i64 = row.get(SONG_SELECT_COL_COUNT)?;
                let disc_count: i64 = row.get(SONG_SELECT_COL_COUNT + 1)?;
                Ok((song, count, disc_count))
            })?
            .filter_map(|r| r.ok())
            .collect();
        let mut items = group_songs_into_home_items(songs_with_counts, limit as usize);
        attach_album_ratings(&conn, &mut items)?;
        Ok(items)
    }
}

fn group_songs_into_home_items(
    songs_with_counts: Vec<(Song, i64, i64)>,
    limit: usize,
) -> Vec<HomeItem> {
    use std::collections::HashSet;
    let mut items = Vec::new();
    let mut seen_albums = HashSet::new();

    for (song, album_track_count, album_disc_count) in songs_with_counts {
        if items.len() >= limit {
            break;
        }

        if let Some(ref album_name) = song.album {
            if !album_name.trim().is_empty() && album_track_count > 1 {
                let artist_name = song
                    .album_artist
                    .clone()
                    .or_else(|| song.artist.clone())
                    .unwrap_or_default();
                let album_key = album_name.trim().to_lowercase();

                if !seen_albums.contains(&album_key) {
                    seen_albums.insert(album_key);
                    items.push(HomeItem::Album {
                        album: AlbumItem {
                            artist: Some(artist_name),
                            album: Some(album_name.clone()),
                            year: song.year,
                            track_count: album_track_count as i32,
                            disc_count: album_disc_count as i32,
                            art_embedded: song.art_embedded,
                            art_automatic: song.art_automatic.clone(),
                            art_manual: song.art_manual.clone(),
                            genre: song.genre.clone(),
                            sample_song_id: Some(song.id),
                            rating: crate::stats::RATING_UNRATED,
                            total_duration_nanosec: 0,
                        },
                    });
                }
                continue;
            }
        }

        items.push(HomeItem::Song {
            song: Box::new(song),
        });
    }

    items
}

/// Fill in the real rating for every `HomeItem::Album` in `items`, looked up
/// from `album_ratings`. `group_songs_into_home_items`/`home_item_for_context`
/// build `AlbumItem`s without a DB connection in scope, so they default to
/// unrated — this backfills the actual value once a connection is available.
fn attach_album_ratings(conn: &rusqlite::Connection, items: &mut [HomeItem]) -> Result<()> {
    for item in items.iter_mut() {
        if let HomeItem::Album { album } = item {
            if let Some(ref name) = album.album {
                album.rating = crate::stats::get_album_rating(conn, name)?;
            }
        }
    }
    Ok(())
}

/// Retrieve customizable profile for an artist from SQLite (#473).
pub fn get_artist_profile_conn(conn: &rusqlite::Connection, artist: &str) -> Result<ArtistProfile> {
    let mut stmt = conn.prepare(
        "SELECT artist_key, website, tags, social_links, bio FROM artist_profiles WHERE artist_key = ?1 COLLATE NOCASE",
    )?;
    let result = stmt.query_row(params![artist], |row| {
        let artist_key: String = row.get(0)?;
        let website: Option<String> = row.get(1)?;
        let tags_json: String = row.get(2)?;
        let social_links_json: String = row.get(3)?;
        let bio: Option<String> = row.get(4)?;

        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let social_links: Vec<ArtistSocialLink> =
            serde_json::from_str(&social_links_json).unwrap_or_default();

        Ok(ArtistProfile {
            artist_key,
            website,
            tags,
            social_links,
            bio,
        })
    });

    match result {
        Ok(profile) => Ok(profile),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ArtistProfile {
            artist_key: artist.to_string(),
            website: None,
            tags: Vec::new(),
            social_links: Vec::new(),
            bio: None,
        }),
        Err(e) => Err(e.into()),
    }
}

/// Upsert an artist profile into SQLite (#473).
pub fn set_artist_profile_conn(
    conn: &rusqlite::Connection,
    profile: &ArtistProfile,
) -> Result<ArtistProfile> {
    let tags_json = serde_json::to_string(&profile.tags)?;
    let social_links_json = serde_json::to_string(&profile.social_links)?;

    conn.execute(
        "INSERT INTO artist_profiles (artist_key, website, tags, social_links, bio)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(artist_key) DO UPDATE SET
            website = excluded.website,
            tags = excluded.tags,
            social_links = excluded.social_links,
            bio = excluded.bio",
        params![
            profile.artist_key,
            profile.website,
            tags_json,
            social_links_json,
            profile.bio
        ],
    )?;

    Ok(profile.clone())
}

/// Retrieve all saved artist profiles in SQLite (#473).
pub fn get_all_artist_profiles_conn(conn: &rusqlite::Connection) -> Result<Vec<ArtistProfile>> {
    let mut stmt = conn.prepare(
        "SELECT artist_key, website, tags, social_links, bio FROM artist_profiles ORDER BY artist_key COLLATE NOCASE",
    )?;
    let profiles = stmt
        .query_map([], |row| {
            let artist_key: String = row.get(0)?;
            let website: Option<String> = row.get(1)?;
            let tags_json: String = row.get(2)?;
            let social_links_json: String = row.get(3)?;
            let bio: Option<String> = row.get(4)?;

            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let social_links: Vec<ArtistSocialLink> =
                serde_json::from_str(&social_links_json).unwrap_or_default();

            Ok(ArtistProfile {
                artist_key,
                website,
                tags,
                social_links,
                bio,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(profiles)
}

/// Dedup key for context-aware Recently Played — one entry per album,
/// playlist, or standalone song, keeping only the most recent occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PlayContextKey {
    Playlist(i64),
    Album(String),
    Song(i64),
}

fn group_by_play_context(
    rows: Vec<(Song, i64, i64, String, Option<i64>)>,
    limit: usize,
    playlists_by_id: &std::collections::HashMap<i64, Playlist>,
) -> Vec<HomeItem> {
    use std::collections::HashSet;
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for (song, album_track_count, album_disc_count, context_type, playlist_id) in rows {
        if items.len() >= limit {
            break;
        }

        let key = match context_type.as_str() {
            "playlist" if playlist_id.is_some() => PlayContextKey::Playlist(playlist_id.unwrap()),
            "album" if song.album.as_deref().is_some_and(|a| !a.trim().is_empty()) => {
                PlayContextKey::Album(song.album.clone().unwrap().trim().to_lowercase())
            }
            _ => PlayContextKey::Song(song.id),
        };

        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);

        items.push(home_item_for_context(
            &context_type,
            playlist_id,
            song,
            album_track_count,
            album_disc_count,
            playlists_by_id,
        ));
    }

    items
}

/// Build the HomeItem a play-history context maps to: a Playlist card when
/// resolvable, an Album card when the representative song carries an album
/// tag, otherwise a standalone Song card.
fn home_item_for_context(
    context_type: &str,
    playlist_id: Option<i64>,
    song: Song,
    album_track_count: i64,
    album_disc_count: i64,
    playlists_by_id: &std::collections::HashMap<i64, Playlist>,
) -> HomeItem {
    match context_type {
        "playlist" => match playlist_id.and_then(|id| playlists_by_id.get(&id)) {
            Some(playlist) => HomeItem::Playlist {
                playlist: playlist.clone(),
            },
            None => HomeItem::Song {
                song: Box::new(song),
            },
        },
        "album" if song.album.as_deref().is_some_and(|a| !a.trim().is_empty()) => {
            let artist_name = song
                .album_artist
                .clone()
                .or_else(|| song.artist.clone())
                .unwrap_or_default();
            HomeItem::Album {
                album: AlbumItem {
                    artist: Some(artist_name),
                    album: song.album.clone(),
                    year: song.year,
                    track_count: album_track_count as i32,
                    disc_count: album_disc_count as i32,
                    art_embedded: song.art_embedded,
                    art_automatic: song.art_automatic.clone(),
                    art_manual: song.art_manual.clone(),
                    genre: song.genre.clone(),
                    sample_song_id: Some(song.id),
                    rating: crate::stats::RATING_UNRATED,
                    total_duration_nanosec: 0,
                },
            }
        }
        _ => HomeItem::Song {
            song: Box::new(song),
        },
    }
}

fn get_playlists_by_ids(
    conn: &rusqlite::Connection,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, Playlist>> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT p.id, p.name, p.dynamic_enabled, p.dynamic_spec, p.population_mode,
                p.last_played_row, p.created, p.updated,
                (SELECT COUNT(*) FROM playlist_items pi WHERE pi.playlist_id = p.id) as track_count
         FROM playlists p WHERE p.id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let map = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            let playlist = Playlist::from_row(row)?;
            Ok((playlist.id, playlist))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(map)
}

fn get_songs_by_ids(
    conn: &rusqlite::Connection,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, (Song, i64, i64)>> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let home_item_select_cols = home_item_select_cols();
    let sql = format!("SELECT {home_item_select_cols} FROM songs s WHERE s.id IN ({placeholders})");
    let mut stmt = conn.prepare(&sql)?;
    let map = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            let song = row_to_song(row)?;
            let album_track_count: i64 = row.get(SONG_SELECT_COL_COUNT)?;
            let album_disc_count: i64 = row.get(SONG_SELECT_COL_COUNT + 1)?;
            Ok((song.id, (song, album_track_count, album_disc_count)))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(map)
}

/// SQL `WHERE`-clause fragment testing whether `column_expr` (assumed to be
/// a `; `-delimited multi-value column like `artist`/`album_artist`, or a
/// `COALESCE`/`NULLIF` expression over one) contains `param` — bound via
/// `multi_value_contains_pattern` — as one of its individual values, not
/// merely as a substring. Both sides are wrapped in `;` boundary markers so
/// a value can't be matched by being a substring of an unrelated, longer
/// value in the same position (e.g. clicking artist "Stan" must not also
/// match a song credited only to "Stan Getz").
fn multi_value_contains_sql(column_expr: &str, param: &str) -> String {
    format!("(';' || REPLACE({column_expr}, '; ', ';') || ';') LIKE {param} ESCAPE '\\'")
}

/// Builds the bound LIKE pattern paired with `multi_value_contains_sql`,
/// escaping `value` so any literal `%`, `_`, or `\` in an artist/composer
/// name can't be misread as a LIKE wildcard.
fn multi_value_contains_pattern(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%;{escaped};%")
}

/// `SONG_SELECT_COLS_QUALIFIED` plus correlated `album_track_count` and
/// `album_disc_count` subqueries. Shared by the home-screen queries
/// (`get_recently_played`, `get_most_frequently_played`, `get_recently_added`),
/// which all join on `songs s`.
fn home_item_select_cols() -> String {
    format!(
        "{SONG_SELECT_COLS_QUALIFIED},
    (SELECT COUNT(*) FROM songs s2
     WHERE s2.source IN (1, 2) AND s2.unavailable = 0 AND s2.album = s.album
    ) AS album_track_count,
    (SELECT COALESCE(MAX(COALESCE(s2.disc, 1)), 1) FROM songs s2
     WHERE s2.source IN (1, 2) AND s2.unavailable = 0 AND s2.album = s.album
    ) AS album_disc_count"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::upsert_song;
    use crate::db::Database;
    use crate::models::{FileType, SongSource};
    use std::sync::Arc;

    #[test]
    fn test_multi_value_contains_pattern_escapes_like_wildcards() {
        // An artist/composer name containing a literal '%' or '_' must not
        // be misread as a LIKE wildcard when embedded in the pattern.
        assert_eq!(multi_value_contains_pattern("50%"), "%;50\\%;%");
        assert_eq!(
            multi_value_contains_pattern("Under_score"),
            "%;Under\\_score;%"
        );
        assert_eq!(
            multi_value_contains_pattern(r"back\slash"),
            r"%;back\\slash;%"
        );
        // The common case: no wildcard characters, just wrapped in ';'.
        assert_eq!(multi_value_contains_pattern("Evergrey"), "%;Evergrey;%");
    }

    #[test]
    fn test_get_albums_artist_resolution() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_coll_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str,
                           title: &str,
                           artist: Option<&str>,
                           album: Option<&str>,
                           album_artist: Option<&str>| {
            let song = Song {
                path: Some(path.to_string()),
                title: Some(title.to_string()),
                artist: artist.map(|s| s.to_string()),
                album: album.map(|s| s.to_string()),
                album_artist: album_artist.map(|s| s.to_string()),
                source: SongSource::LocalFile,
                filetype: FileType::Mp3,
                unavailable: false,
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        };

        // Scenario 1: Album where all tracks have the same artist, and album_artist is None
        insert_song(
            "path/1.mp3",
            "Track 1",
            Some("Artist A"),
            Some("Album One"),
            None,
        );
        insert_song(
            "path/2.mp3",
            "Track 2",
            Some("Artist A"),
            Some("Album One"),
            None,
        );

        // Scenario 2: Album with different artists, and album_artist is None (Various Artists fallback)
        insert_song(
            "path/3.mp3",
            "Track 3",
            Some("Artist B"),
            Some("Album Two"),
            None,
        );
        insert_song(
            "path/4.mp3",
            "Track 4",
            Some("Artist C"),
            Some("Album Two"),
            None,
        );

        // Scenario 3: Album where all tracks have same album_artist but different track artists
        insert_song(
            "path/5.mp3",
            "Track 5",
            Some("Artist B"),
            Some("Album Three"),
            Some("Artist A"),
        );
        insert_song(
            "path/6.mp3",
            "Track 6",
            Some("Artist C"),
            Some("Album Three"),
            Some("Artist A"),
        );

        // Scenario 4: Album where tracks have different album_artists
        insert_song(
            "path/7.mp3",
            "Track 7",
            Some("Artist X"),
            Some("Album Four"),
            Some("Artist Y"),
        );
        insert_song(
            "path/8.mp3",
            "Track 8",
            Some("Artist Z"),
            Some("Album Four"),
            Some("Artist W"),
        );

        let albums = scanner.get_albums().unwrap();

        let find_album = |name: &str| -> &serde_json::Value {
            albums
                .iter()
                .find(|a| a["album"].as_str() == Some(name))
                .unwrap()
        };

        // Assert Album One -> album_artist is "Artist A"
        let album_one = find_album("Album One");
        assert_eq!(album_one["artist"].as_str(), Some("Artist A"));
        assert_eq!(album_one["track_count"].as_i64(), Some(2));
        assert!(album_one["added"].is_number());

        // Assert Album Two -> album_artist is None (will fall back to Various Artists in UI)
        let album_two = find_album("Album Two");
        assert_eq!(album_two["artist"].as_str(), None);
        assert_eq!(album_two["track_count"].as_i64(), Some(2));

        // Assert Album Three -> album_artist is "Artist A"
        let album_three = find_album("Album Three");
        assert_eq!(album_three["artist"].as_str(), Some("Artist A"));
        assert_eq!(album_three["track_count"].as_i64(), Some(2));

        // Assert Album Four -> album_artist is None (Various Artists fallback)
        let album_four = find_album("Album Four");
        assert_eq!(album_four["artist"].as_str(), None);
        assert_eq!(album_four["track_count"].as_i64(), Some(2));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_albums_excludes_empty_string_album() {
        // A present-but-blank album tag (as opposed to a missing one, which
        // is NULL) must not surface as a pseudo-album — otherwise untagged
        // singles from unrelated artists collapse into one bogus "Unknown
        // Album" / "Various Artists" card (#issue: singles without an album
        // title showing up in the Albums grid).
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_empty_album_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str, title: &str, artist: &str, album: Option<&str>| {
            let song = Song {
                path: Some(path.to_string()),
                title: Some(title.to_string()),
                artist: Some(artist.to_string()),
                album: album.map(|s| s.to_string()),
                source: SongSource::LocalFile,
                filetype: FileType::Mp3,
                unavailable: false,
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        };

        insert_song("path/1.mp3", "Single One", "Artist A", Some(""));
        insert_song("path/2.mp3", "Single Two", "Artist B", Some(""));
        insert_song(
            "path/3.mp3",
            "Track In Album",
            "Artist C",
            Some("Real Album"),
        );

        let albums = scanner.get_albums().unwrap();

        assert!(
            albums.iter().all(|a| a["album"].as_str() != Some("")),
            "empty-string album should not produce a pseudo-album entry: {albums:?}"
        );
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0]["album"].as_str(), Some("Real Album"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_compilations_by_artist() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_compilations_by_artist_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str,
                           artist: &str,
                           album: &str,
                           album_artist: Option<&str>,
                           compilation: bool| {
            let song = Song {
                path: Some(path.to_string()),
                title: Some(path.to_string()),
                artist: Some(artist.to_string()),
                album: Some(album.to_string()),
                album_artist: album_artist.map(|s| s.to_string()),
                source: SongSource::LocalFile,
                filetype: FileType::Mp3,
                unavailable: false,
                compilation,
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        };

        // A properly tagged compilation: TCMP=1, shared "Various Artists" album_artist.
        insert_song(
            "path/comp1.mp3",
            "Artist A",
            "Compilation One",
            Some("Various Artists"),
            true,
        );
        insert_song(
            "path/comp2.mp3",
            "Artist B",
            "Compilation One",
            Some("Various Artists"),
            true,
        );

        // A compilation identifiable only by disagreeing album_artist (no TCMP).
        insert_song("path/comp3.mp3", "Artist A", "Compilation Two", None, false);
        insert_song("path/comp4.mp3", "Artist C", "Compilation Two", None, false);

        // Artist A's own solo album should NOT show up as a compilation.
        insert_song(
            "path/solo1.mp3",
            "Artist A",
            "Solo Album",
            Some("Artist A"),
            false,
        );
        insert_song(
            "path/solo2.mp3",
            "Artist A",
            "Solo Album",
            Some("Artist A"),
            false,
        );

        let comps = scanner.get_compilations_by_artist("Artist A").unwrap();
        let names: Vec<&str> = comps.iter().map(|a| a["album"].as_str().unwrap()).collect();
        assert!(names.contains(&"Compilation One"));
        assert!(names.contains(&"Compilation Two"));
        assert!(!names.contains(&"Solo Album"));
        assert_eq!(comps.len(), 2);

        for comp in &comps {
            assert_eq!(comp["artist"].as_str(), Some("Various Artists"));
        }

        // Artist B only appears on Compilation One.
        let comps_b = scanner.get_compilations_by_artist("Artist B").unwrap();
        assert_eq!(comps_b.len(), 1);
        assert_eq!(comps_b[0]["album"].as_str(), Some("Compilation One"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_artists_album_count_filtering() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_artist_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        // Artist A: Single with 1 track (track_count <= 7) -> should count as 0 albums
        let song_a = Song {
            artist: Some("Artist Single".to_string()),
            album: Some("Single Album".to_string()),
            title: Some("Single Track".to_string()),
            source: SongSource::LocalFile,
            path: Some(r"C:\Music\Artist Single\single.mp3".to_string()),
            ..Default::default()
        };
        upsert_song(&conn, &song_a).unwrap();

        // Artist B: Full Album with 8 tracks (track_count > 7) -> should count as 1 album
        for i in 1..=8 {
            let song_b = Song {
                artist: Some("Artist Full".to_string()),
                album: Some("Full Album".to_string()),
                title: Some(format!("Track {}", i)),
                source: SongSource::LocalFile,
                path: Some(format!(r"C:\Music\Artist Full\track{}.mp3", i)),
                ..Default::default()
            };
            upsert_song(&conn, &song_b).unwrap();
        }

        let scanner = CollectionScanner::new(db.clone());
        let artists = scanner.get_artists().unwrap();

        let single_artist = artists
            .iter()
            .find(|a| a["name"].as_str() == Some("Artist Single"))
            .unwrap();
        assert_eq!(single_artist["album_count"].as_i64(), Some(0));
        assert_eq!(single_artist["song_count"].as_i64(), Some(1));

        let full_artist = artists
            .iter()
            .find(|a| a["name"].as_str() == Some("Artist Full"))
            .unwrap();
        assert_eq!(full_artist["album_count"].as_i64(), Some(1));
        assert_eq!(full_artist["song_count"].as_i64(), Some(8));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression test for #295: songs whose artist tag only differs in case
    /// (e.g. from albums organized/tagged at different times) must be merged
    /// into a single artist entry rather than shown as two separate artists.
    #[test]
    fn test_get_artists_merges_case_only_variants() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_artist_case_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                artist: Some("The War on Drugs".to_string()),
                album: Some("Lost in the Dream".to_string()),
                title: Some("Under the Pressure".to_string()),
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\The War on Drugs\track1.mp3".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        upsert_song(
            &conn,
            &Song {
                artist: Some("The War On Drugs".to_string()),
                album: Some("A Deeper Understanding".to_string()),
                title: Some("Holding On".to_string()),
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\The War On Drugs\track2.mp3".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());
        let artists = scanner.get_artists().unwrap();

        let matches: Vec<&serde_json::Value> = artists
            .iter()
            .filter(|a| {
                a["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case("the war on drugs"))
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "expected a single merged artist entry, got {:?}",
            artists
        );
        assert_eq!(matches[0]["song_count"].as_i64(), Some(2));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression test for #362: a song with no artist/album_artist tags at
    /// all (both NULL) must still be reachable via the same "effective
    /// artist" value that get_artists() groups it under — previously
    /// get_artists() grouped NULL artists together as SQL NULL while
    /// get_songs_by_artist() only matched against `''`, so NULL never
    /// equaled `''` and the click-through returned zero songs.
    #[test]
    fn test_untagged_song_reachable_via_get_artists_grouping() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_untagged_artist_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                artist: None,
                album_artist: None,
                album: None,
                title: None,
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\untagged.ogg".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());
        let artists = scanner.get_artists().unwrap();

        let untagged = artists
            .iter()
            .find(|a| a["name"].as_str() == Some(""))
            .expect("untagged song should surface as an empty-string artist entry");
        assert_eq!(untagged["song_count"].as_i64(), Some(1));

        let songs = scanner.get_songs_by_artist("").unwrap();
        assert_eq!(
            songs.len(),
            1,
            "get_songs_by_artist(\"\") must return the song grouped under the empty-string artist"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression test for the artist click-through gap found while testing
    /// #150: `get_songs_by_artist` used to match with exact equality against
    /// a single "effective" column (album_artist, falling back to artist),
    /// so a collab track credited to artist = "Evergrey; Mikael Stanne" with
    /// album_artist = "Evergrey" was unreachable by clicking "Mikael
    /// Stanne" — the effective column resolved to just "Evergrey" and never
    /// mentioned him at all, regardless of how the artist column was split.
    /// Checking the raw `artist` column too (not only the COALESCE'd
    /// effective one) is what actually fixes this.
    #[test]
    fn test_get_songs_by_artist_finds_individual_values_in_a_multi_artist_credit() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_multi_artist_click_through_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                artist: Some("Evergrey; Mikael Stanne".to_string()),
                album_artist: Some("Evergrey".to_string()),
                album: Some("Architects Of A New Weave".to_string()),
                title: Some("A Burning Flame".to_string()),
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\a_burning_flame.flac".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        // A solo Evergrey track, to confirm the match isn't overly broad.
        upsert_song(
            &conn,
            &Song {
                artist: Some("Evergrey".to_string()),
                album_artist: None,
                album: Some("Escape Of The Phoenix".to_string()),
                title: Some("Where August Mourns".to_string()),
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\where_august_mourns.flac".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());

        let evergrey_songs = scanner.get_songs_by_artist("Evergrey").unwrap();
        assert_eq!(
            evergrey_songs.len(),
            2,
            "clicking Evergrey must surface both the solo track and the collab track"
        );

        let stanne_songs = scanner.get_songs_by_artist("Mikael Stanne").unwrap();
        assert_eq!(
            stanne_songs.len(),
            1,
            "clicking Mikael Stanne must surface the collab track"
        );
        assert_eq!(stanne_songs[0].title.as_deref(), Some("A Burning Flame"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// A Various Artists compilation track (album_artist = "Various
    /// Artists", per-track artist = the actual performer) must be reachable
    /// by clicking the individual track artist, not just via the separate
    /// `get_compilations_by_artist` album-card query.
    #[test]
    fn test_get_songs_by_artist_finds_various_artists_compilation_track() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_compilation_click_through_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                artist: Some("Artist X".to_string()),
                album_artist: Some("Various Artists".to_string()),
                album: Some("Now That's What I Call Tests".to_string()),
                title: Some("Track One".to_string()),
                compilation: true,
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\track_one.flac".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());
        let songs = scanner.get_songs_by_artist("Artist X").unwrap();
        assert_eq!(
            songs.len(),
            1,
            "clicking a compilation track's own artist must surface it directly"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// A value that's a substring of a different, unrelated artist's full
    /// name must not false-positive match — clicking "Stan" (if that were a
    /// real credited artist) must not also surface a song credited only to
    /// "Stan Getz".
    #[test]
    fn test_get_songs_by_artist_does_not_match_substrings_of_unrelated_names() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_artist_substring_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                artist: Some("Stan Getz".to_string()),
                album_artist: None,
                album: Some("Getz/Gilberto".to_string()),
                title: Some("The Girl From Ipanema".to_string()),
                source: SongSource::LocalFile,
                path: Some(r"C:\Music\girl_from_ipanema.flac".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let scanner = CollectionScanner::new(db.clone());
        let songs = scanner.get_songs_by_artist("Stan").unwrap();
        assert_eq!(
            songs.len(),
            0,
            "\"Stan\" must not match \"Stan Getz\" as a substring"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_top_artists_ranks_by_playcount_and_ranks_zero_plays_last() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_top_artists_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        // upsert_song() deliberately never touches playcount (it's owned by
        // the stats.rs write path, preserved across rescans) — so seed songs
        // via upsert_song(), then set playcount directly.
        let seed = |path: &str, artist: &str, title: &str, playcount: i32| {
            upsert_song(
                &conn,
                &Song {
                    artist: Some(artist.to_string()),
                    title: Some(title.to_string()),
                    source: SongSource::LocalFile,
                    path: Some(path.to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
            conn.execute(
                "UPDATE songs SET playcount = ?1 WHERE path = ?2",
                params![playcount, path],
            )
            .unwrap();
        };

        // Artist Low: one song, played twice.
        seed(r"C:\Music\Artist Low\low.mp3", "Artist Low", "Low Track", 2);

        // Artist High: two songs, playcounts sum to 10.
        seed(
            r"C:\Music\Artist High\a.mp3",
            "Artist High",
            "High Track A",
            6,
        );
        seed(
            r"C:\Music\Artist High\b.mp3",
            "Artist High",
            "High Track B",
            4,
        );

        // Artist Unplayed: never played. The library as a whole has play
        // history (High/Low), so the zero-play fallback must NOT kick in —
        // Unplayed is still included, but ranked last by total_playcount.
        seed(
            r"C:\Music\Artist Unplayed\track.mp3",
            "Artist Unplayed",
            "Unplayed Track",
            0,
        );

        let scanner = CollectionScanner::new(db.clone());
        let top_artists = scanner.get_top_artists(10).unwrap();

        let names: Vec<&str> = top_artists
            .iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["Artist High", "Artist Low", "Artist Unplayed"]);

        let high = &top_artists[0];
        assert_eq!(high["song_count"].as_i64(), Some(2));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_top_artists_falls_back_to_song_count_when_library_has_no_plays() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_top_artists_fallback_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let seed = |path: &str, artist: &str, title: &str| {
            upsert_song(
                &conn,
                &Song {
                    artist: Some(artist.to_string()),
                    title: Some(title.to_string()),
                    source: SongSource::LocalFile,
                    path: Some(path.to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        };

        // A freshly-scanned library: no song anywhere has ever been played.
        // Artist Big has more songs than Artist Small.
        seed(r"C:\Music\Artist Big\a.mp3", "Artist Big", "Track A");
        seed(r"C:\Music\Artist Big\b.mp3", "Artist Big", "Track B");
        seed(r"C:\Music\Artist Small\a.mp3", "Artist Small", "Track A");

        let scanner = CollectionScanner::new(db.clone());
        let top_artists = scanner.get_top_artists(10).unwrap();

        // Zero-play library: nothing gets excluded, and ranking falls back
        // to song_count DESC instead of collapsing to alphabetical order.
        let names: Vec<&str> = top_artists
            .iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["Artist Big", "Artist Small"]);
        assert_eq!(top_artists[0]["song_count"].as_i64(), Some(2));
        assert_eq!(top_artists[1]["song_count"].as_i64(), Some(1));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_library_decades_and_songs() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_decade_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let s1 = Song {
            title: Some("80s Song".to_string()),
            year: Some(1984),
            source: SongSource::LocalFile,
            path: Some("/music/80s.mp3".to_string()),
            ..Default::default()
        };
        let s2 = Song {
            title: Some("90s Song".to_string()),
            originalyear: Some(1995),
            source: SongSource::LocalFile,
            path: Some("/music/90s.mp3".to_string()),
            ..Default::default()
        };
        upsert_song(&conn, &s1).unwrap();
        upsert_song(&conn, &s2).unwrap();

        let scanner = CollectionScanner::new(db);
        let decades = scanner.get_library_decades().unwrap();
        assert_eq!(decades, vec!["1980s".to_string(), "1990s".to_string()]);

        let songs_80s = scanner
            .get_songs_by_decade("1980s", 10, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(songs_80s.len(), 1);
        assert_eq!(songs_80s[0].title.as_deref(), Some("80s Song"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Verifies each `QueuePopulationMode`'s WHERE-clause bias (see #120)
    /// selects the correct subset of songs. Ordering is randomized by
    /// design, so this only asserts set membership, not order. Exercised via
    /// `TagManager::get_songs_by_tag`, which splices in the same
    /// `mode_query_fragments` this test targets.
    #[test]
    fn test_get_recently_played_groups_by_play_context() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_recent_played_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str, title: &str, album: Option<&str>| {
            let song = Song {
                path: Some(path.to_string()),
                title: Some(title.to_string()),
                artist: Some("Artist".to_string()),
                album: album.map(|s| s.to_string()),
                album_artist: album.map(|_| "Artist".to_string()),
                source: SongSource::LocalFile,
                filetype: FileType::Mp3,
                unavailable: false,
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
            conn.query_row("SELECT id FROM songs WHERE path = ?1", params![path], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        };

        let standalone_id = insert_song("path/standalone.mp3", "Standalone", None);
        let album_track_1 = insert_song("path/album_a1.mp3", "Album Track 1", Some("Album A"));
        let album_track_2 = insert_song("path/album_a2.mp3", "Album Track 2", Some("Album A"));
        let playlist_track = insert_song("path/playlist_track.mp3", "Playlist Track", None);

        conn.execute(
            "INSERT INTO playlists (name) VALUES ('My Playlist')",
            params![],
        )
        .unwrap();
        let playlist_id = conn.last_insert_rowid();

        // played_at ascending: standalone (oldest) -> two album plays -> playlist play (newest)
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('song', ?1, NULL, 100)",
            params![standalone_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('album', ?1, NULL, 200)",
            params![album_track_1],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('album', ?1, NULL, 201)",
            params![album_track_2],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('playlist', ?1, ?2, 300)",
            params![playlist_track, playlist_id],
        )
        .unwrap();

        let recent = scanner.get_recently_played(10).unwrap();
        assert_eq!(recent.len(), 3, "album plays should collapse into one card");
        match &recent[0] {
            HomeItem::Playlist { playlist } => assert_eq!(playlist.id, playlist_id),
            other => panic!("expected Playlist as most recent, got {other:?}"),
        }
        match &recent[1] {
            HomeItem::Album { album } => assert_eq!(album.album.as_deref(), Some("Album A")),
            other => panic!("expected Album next, got {other:?}"),
        }
        match &recent[2] {
            HomeItem::Song { song } => assert_eq!(song.id, standalone_id),
            other => panic!("expected standalone Song last, got {other:?}"),
        }

        // Most frequently played: play the standalone song two more times so it
        // outranks the (2-play) album and (1-play) playlist contexts.
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('song', ?1, NULL, 400)",
            params![standalone_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('song', ?1, NULL, 500)",
            params![standalone_id],
        )
        .unwrap();

        let frequent = scanner.get_most_frequently_played(10).unwrap();
        assert_eq!(frequent.len(), 3);
        match &frequent[0] {
            HomeItem::Song { song } => assert_eq!(song.id, standalone_id),
            other => panic!("expected most-played standalone Song first, got {other:?}"),
        }

        // Test exclusion of internal 'Queue' playlist from recently played & most frequently played
        conn.execute(
            "INSERT INTO playlists (name, dynamic_enabled) VALUES ('Queue', 0)",
            params![],
        )
        .unwrap();
        let queue_playlist_id = conn.last_insert_rowid();

        // Play from Queue playlist with high played_at, and many times
        for i in 0..10 {
            conn.execute(
                "INSERT INTO play_history (context_type, song_id, playlist_id, played_at) VALUES ('playlist', ?1, ?2, ?3)",
                params![playlist_track, queue_playlist_id, 1000 + i],
            )
            .unwrap();
        }

        // Verify that 'Queue' does not show up in recently played (still length 3)
        let recent_after_queue = scanner.get_recently_played(10).unwrap();
        assert_eq!(recent_after_queue.len(), 3);
        for item in &recent_after_queue {
            if let HomeItem::Playlist { playlist } = item {
                assert_ne!(playlist.id, queue_playlist_id);
            }
        }

        // Verify that 'Queue' does not show up in most frequently played (still length 3)
        let frequent_after_queue = scanner.get_most_frequently_played(10).unwrap();
        assert_eq!(frequent_after_queue.len(), 3);
        for item in &frequent_after_queue {
            if let HomeItem::Playlist { playlist } = item {
                assert_ne!(playlist.id, queue_playlist_id);
            }
        }

        // Verify that plays with explicit album context are attributed to their album
        for i in 0..15 {
            conn.execute(
                "INSERT INTO play_history (context_type, song_id, played_at) VALUES ('album', ?1, ?2)",
                params![album_track_1, 2000 + i],
            )
            .unwrap();
        }
        let frequent_after_album = scanner.get_most_frequently_played(10).unwrap();
        assert!(frequent_after_album.iter().any(|item| matches!(item, HomeItem::Album { album, .. } if album.album.as_deref() == Some("Album A"))));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_recently_added_collapses_album_with_varying_track_artists() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_rec_added_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str, title: &str, artist: &str, album: &str| -> i64 {
            let song = Song {
                source: SongSource::LocalFile,
                path: Some(path.to_string()),
                title: Some(title.to_string()),
                artist: Some(artist.to_string()),
                album: Some(album.to_string()),
                album_artist: None,
                added: Some(1000),
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
            conn.query_row("SELECT id FROM songs WHERE path = ?1", params![path], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        };

        // 7 tracks with Artist A, 3 tracks with Artist B (total 10 tracks)
        for i in 1..=7 {
            insert_song(
                &format!("path/track_{i}.mp3"),
                &format!("Track {i}"),
                "Artist A",
                "Mixed Album",
            );
        }
        for i in 8..=10 {
            insert_song(
                &format!("path/track_{i}.mp3"),
                &format!("Track {i}"),
                "Artist B",
                "Mixed Album",
            );
        }

        let items = scanner.get_recently_added(10).unwrap();
        assert_eq!(
            items.len(),
            1,
            "all 10 tracks should collapse into a single album card"
        );
        match &items[0] {
            HomeItem::Album { album } => {
                assert_eq!(album.album.as_deref(), Some("Mixed Album"));
                assert_eq!(
                    album.track_count, 10,
                    "should count all 10 tracks, not split by artist"
                );
            }
            other => panic!("expected Album item, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_featured_albums_returns_albums_without_play_history() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_featured_albums_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str, title: &str, artist: &str, album: Option<&str>| {
            let song = Song {
                source: SongSource::LocalFile,
                path: Some(path.to_string()),
                title: Some(title.to_string()),
                artist: Some(artist.to_string()),
                album: album.map(|a| a.to_string()),
                added: Some(1000),
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        };

        // Two full albums (no play history — playcount defaults to 0/unset).
        for i in 1..=3 {
            insert_song(
                &format!("path/album_a_{i}.mp3"),
                &format!("A Track {i}"),
                "Artist A",
                Some("Album A"),
            );
        }
        for i in 1..=3 {
            insert_song(
                &format!("path/album_b_{i}.mp3"),
                &format!("B Track {i}"),
                "Artist B",
                Some("Album B"),
            );
        }
        // A standalone single with no album tag — must be excluded.
        insert_song("path/single.mp3", "Single Track", "Artist C", None);

        let items = scanner.get_featured_albums(10).unwrap();
        assert_eq!(items.len(), 2, "should surface both albums, no singles");
        for item in &items {
            match item {
                HomeItem::Album { album } => {
                    assert!(
                        matches!(album.album.as_deref(), Some("Album A") | Some("Album B")),
                        "unexpected album: {album:?}"
                    );
                }
                other => panic!("expected Album item, got {other:?}"),
            }
        }

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_featured_albums_respects_limit() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_featured_albums_limit_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        let insert_song = |path: &str, title: &str, artist: &str, album: &str| {
            let song = Song {
                source: SongSource::LocalFile,
                path: Some(path.to_string()),
                title: Some(title.to_string()),
                artist: Some(artist.to_string()),
                album: Some(album.to_string()),
                added: Some(1000),
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        };

        for album_idx in 1..=5 {
            for track_idx in 1..=2 {
                insert_song(
                    &format!("path/album_{album_idx}_track_{track_idx}.mp3"),
                    &format!("Track {track_idx}"),
                    "Various Artist",
                    &format!("Album {album_idx}"),
                );
            }
        }

        let items = scanner.get_featured_albums(2).unwrap();
        assert_eq!(items.len(), 2);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_recently_added_surfaces_existing_album_rating() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_rec_added_rating_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        // Needs 2+ tracks — a single-track "album" renders as a Song item, not
        // an Album item (see group_songs_into_home_items's album_track_count > 1 check).
        for i in 1..=2 {
            let song = Song {
                source: SongSource::LocalFile,
                path: Some(format!("path/rated_{i}.mp3")),
                title: Some(format!("Rated Track {i}")),
                artist: Some("Artist A".to_string()),
                album: Some("Rated Album".to_string()),
                album_artist: None,
                added: Some(1000),
                ..Default::default()
            };
            upsert_song(&conn, &song).unwrap();
        }

        crate::stats::set_album_rating(&conn, "Rated Album", 4.5).unwrap();

        let items = scanner.get_recently_added(10).unwrap();
        match &items[0] {
            HomeItem::Album { album } => {
                assert_eq!(album.album.as_deref(), Some("Rated Album"));
                assert_eq!(
                    album.rating, 4.5,
                    "should surface the rating set via set_album_rating, not default to unrated"
                );
            }
            other => panic!("expected Album item, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_artist_profile_crud() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_artist_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        let conn = db.pool.get().unwrap();

        // Initially unconfigured artist returns default profile
        let initial = get_artist_profile_conn(&conn, "Shania Twain").unwrap();
        assert_eq!(initial.artist_key, "Shania Twain");
        assert_eq!(initial.website, None);
        assert!(initial.tags.is_empty());
        assert!(initial.social_links.is_empty());
        assert_eq!(initial.bio, None);

        // Save profile
        let profile = ArtistProfile {
            artist_key: "Shania Twain".to_string(),
            website: Some("https://www.shaniatwain.com".to_string()),
            tags: vec![
                "pop".to_string(),
                "country".to_string(),
                "canadian".to_string(),
            ],
            social_links: vec![
                ArtistSocialLink {
                    platform: "instagram".to_string(),
                    handle_or_url: "@shaniatwain".to_string(),
                },
                ArtistSocialLink {
                    platform: "youtube".to_string(),
                    handle_or_url: "https://youtube.com/@ShaniaTwain".to_string(),
                },
            ],
            bio: Some("Canadian singer-songwriter".to_string()),
        };

        set_artist_profile_conn(&conn, &profile).unwrap();

        // Retrieve saved profile (case-insensitive key match)
        let loaded = get_artist_profile_conn(&conn, "shania twain").unwrap();
        assert_eq!(loaded.artist_key, "Shania Twain");
        assert_eq!(
            loaded.website,
            Some("https://www.shaniatwain.com".to_string())
        );
        assert_eq!(loaded.tags, vec!["pop", "country", "canadian"]);
        assert_eq!(loaded.social_links.len(), 2);
        assert_eq!(loaded.social_links[0].platform, "instagram");
        assert_eq!(loaded.social_links[0].handle_or_url, "@shaniatwain");
        assert_eq!(loaded.bio, Some("Canadian singer-songwriter".to_string()));

        // Get all profiles
        let all = get_all_artist_profiles_conn(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].artist_key, "Shania Twain");

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
