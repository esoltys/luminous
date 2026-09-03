use crate::collection::WatcherPauseGuard;
use crate::models;
use crate::tageditor::SuggestedTags;
use crate::AppState;
use std::sync::Arc;
use tauri::State;

#[derive(serde::Serialize)]
pub struct SongDetails {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub titlesort: Option<String>,
    pub artist: String,
    pub artistsort: Option<String>,
    pub album: String,
    pub albumsort: Option<String>,
    pub album_artist: String,
    pub album_artist_sort: Option<String>,
    pub composer: String,
    pub composersort: Option<String>,
    /// `artist`, `album_artist`, `composer`, and `genre` are all `; `-delimited
    /// when the song carries multiple values — see
    /// `models::parse_multi_value`/`join_multi_value`, the single source of
    /// truth for this convention.
    pub genre: String,
    pub genresort: Option<String>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub grouping: String,
    pub bpm: Option<f32>,
    pub initial_key: String,
    pub rating: f32,
    pub compilation: bool,
    pub art_embedded: bool,
}

#[tauri::command]
pub async fn get_song_details(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<SongDetails, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, path, title, titlesort, artist, artistsort, album, albumsort, album_artist, album_artist_sort, composer, composersort, genre, genresort, track, disc, year,
                grouping, bpm, initial_key, rating, compilation, art_embedded
         FROM songs WHERE id = ?1",
        rusqlite::params![song_id],
        |row| {
            Ok(SongDetails {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2).unwrap_or_default(),
                titlesort: row.get(3).ok(),
                artist: row.get(4).unwrap_or_default(),
                artistsort: row.get(5).ok(),
                album: row.get(6).unwrap_or_default(),
                albumsort: row.get(7).ok(),
                album_artist: row.get(8).unwrap_or_default(),
                album_artist_sort: row.get(9).ok(),
                composer: row.get(10).unwrap_or_default(),
                composersort: row.get(11).ok(),
                genre: row.get(12).unwrap_or_default(),
                genresort: row.get(13).ok(),
                track: row.get(14).ok(),
                disc: row.get(15).ok(),
                year: row.get(16).ok(),
                grouping: row.get(17).unwrap_or_default(),
                bpm: row.get(18).ok(),
                initial_key: row.get(19).unwrap_or_default(),
                rating: row.get(20).unwrap_or(crate::stats::RATING_UNRATED),
                compilation: row.get(21).unwrap_or(false),
                art_embedded: row.get(22).unwrap_or(false),
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn has_acoustid_env_key() -> Result<bool, String> {
    Ok(std::env::var("ACOUSTID_API_KEY").is_ok())
}

#[tauri::command]
pub async fn lookup_acoustid_tags(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<SuggestedTags, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let path_str: String = conn
        .query_row(
            "SELECT path FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| row.get(0),
        )
        .map_err(|_| "Song not found in library".to_string())?;

    let api_key: Option<String> = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = 'acoustid_api_key'",
            [],
            |row| row.get(0),
        )
        .ok()
        .filter(|s: &String| !s.trim().is_empty());

    let path = std::path::PathBuf::from(path_str);

    // 2. Generate fingerprint (blocking subprocess invocation)
    let (fingerprint, duration_sec) =
        tauri::async_runtime::spawn_blocking(move || crate::tageditor::generate_fingerprint(&path))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;

    // 3. Query AcoustID web service lookup
    let suggestions = crate::tageditor::lookup_acoustid(&fingerprint, duration_sec, api_key)
        .await
        .map_err(|e| e.to_string())?;

    Ok(suggestions)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn save_song_tags(
    state: State<'_, AppState>,
    song_id: i64,
    title: String,
    titlesort: Option<String>,
    artist: String,
    artistsort: Option<String>,
    album: String,
    albumsort: Option<String>,
    album_artist: String,
    album_artist_sort: Option<String>,
    composer: String,
    composersort: Option<String>,
    genre: Option<String>,
    genresort: Option<String>,
    track: Option<u32>,
    disc: Option<u32>,
    year: Option<u32>,
    grouping: String,
    bpm: Option<f32>,
    initial_key: String,
) -> Result<(), String> {
    // Written tags are an app-driven change Luminous already knows about, not
    // an external addition — without this, the realtime watcher would pick up
    // its own write and fire a spurious "song updated" toast on top of
    // whatever feedback the tag editor itself shows (#233).
    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let (path_str, compilation): (String, bool) = conn
        .query_row(
            "SELECT path, compilation FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "Song not found in library".to_string())?;

    let path = std::path::PathBuf::from(path_str);

    // 2. Write metadata back to disk (blocking lofty write in threadpool)
    // The single-song tag editor has no compilation toggle (that's an
    // album-level property owned by AlbumTagEditor.svelte), so preserve
    // whatever's already in the DB rather than clobbering it.
    let path_clone = path.clone();
    let title_c = title.clone();
    let titlesort_c = titlesort.clone();
    // Normalize every multi-value field to the canonical `; `-delimited,
    // trimmed, deduped form before it hits disk or the DB, regardless of
    // exactly what the chip input sent over the wire.
    let artist_str = models::join_multi_value(&models::parse_multi_value(&artist));
    let artist_c = artist_str.clone();
    let artistsort_c = artistsort.clone();
    let album_c = album.clone();
    let albumsort_c = albumsort.clone();
    let album_artist_str = models::join_multi_value(&models::parse_multi_value(&album_artist));
    let album_artist_c = album_artist_str.clone();
    let album_artist_sort_c = album_artist_sort.clone();
    let composer_str = models::join_multi_value(&models::parse_multi_value(&composer));
    let composer_c = composer_str.clone();
    let composersort_c = composersort.clone();
    let genre_str =
        models::join_multi_value(&models::parse_multi_value(&genre.unwrap_or_default()));
    let genre_c = genre_str.clone();
    let grouping_c = grouping.clone();
    let initial_key_c = initial_key.clone();

    tauri::async_runtime::spawn_blocking(move || {
        crate::tageditor::write_tags(
            &path_clone,
            &crate::tageditor::TagWriteRequest {
                title: &title_c,
                titlesort: titlesort_c.as_deref(),
                artist: &artist_c,
                artistsort: artistsort_c.as_deref(),
                album: &album_c,
                albumsort: albumsort_c.as_deref(),
                album_artist: &album_artist_c,
                album_artist_sort: album_artist_sort_c.as_deref(),
                composer: &composer_c,
                composersort: composersort_c.as_deref(),
                genre: &genre_c,
                track,
                disc,
                year,
                grouping: &grouping_c,
                bpm,
                initial_key: &initial_key_c,
                compilation,
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))?;

    // 3. Update SQLite database cache in-place
    conn.execute(
        "UPDATE songs SET
            title = ?1,
            titlesort = ?2,
            artist = ?3,
            artistsort = ?4,
            album = ?5,
            albumsort = ?6,
            album_artist = ?7,
            album_artist_sort = ?8,
            composer = ?9,
            composersort = ?10,
            genre = ?11,
            genresort = ?12,
            track = ?13,
            disc = ?14,
            year = ?15,
            grouping = ?16,
            bpm = ?17,
            initial_key = ?18
         WHERE id = ?19",
        rusqlite::params![
            title,
            titlesort,
            artist_str,
            artistsort,
            album,
            albumsort,
            album_artist_str,
            album_artist_sort,
            composer_str,
            composersort,
            genre_str,
            genresort,
            track,
            disc,
            year,
            grouping,
            bpm,
            initial_key,
            song_id
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn save_album_tags(
    state: State<'_, AppState>,
    song_ids: Vec<i64>,
    album: String,
    albumsort: Option<String>,
    album_artist: String,
    album_artist_sort: Option<String>,
    genre: Option<String>,
    genresort: Option<String>,
    year: Option<u32>,
    disc: Option<u32>,
    compilation: bool,
) -> Result<u32, String> {
    if song_ids.is_empty() {
        return Ok(0);
    }

    // See save_song_tags — pause the watcher for the whole batch write so it
    // doesn't misread its own tag writes across the album as an external
    // change and fire a spurious "songs updated" toast (#233).
    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;

    struct SongMetadata {
        id: i64,
        path: String,
        title: String,
        titlesort: Option<String>,
        artist: String,
        artistsort: Option<String>,
        composer: String,
        composersort: Option<String>,
        track: Option<u32>,
        grouping: String,
        bpm: Option<f32>,
        initial_key: String,
    }

    let mut songs_data = Vec::with_capacity(song_ids.len());
    for &song_id in &song_ids {
        let res = conn.query_row(
            "SELECT path, title, titlesort, artist, artistsort, composer, composersort, track, grouping, bpm, initial_key
             FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| {
                Ok(SongMetadata {
                    id: song_id,
                    path: row.get(0)?,
                    title: row.get(1).unwrap_or_default(),
                    titlesort: row.get(2).ok(),
                    artist: row.get(3).unwrap_or_default(),
                    artistsort: row.get(4).ok(),
                    composer: row.get(5).unwrap_or_default(),
                    composersort: row.get(6).ok(),
                    track: row.get(7).ok(),
                    grouping: row.get(8).unwrap_or_default(),
                    bpm: row.get(9).ok(),
                    initial_key: row.get(10).unwrap_or_default(),
                })
            },
        );
        if let Ok(meta) = res {
            songs_data.push(meta);
        }
    }

    let album_c = album.clone();
    let albumsort_c = albumsort.clone();
    // Normalize to the canonical `; `-delimited, trimmed, deduped form
    // before it hits disk or the DB, regardless of exactly what the chip
    // input sent over the wire.
    let album_artist_str = models::join_multi_value(&models::parse_multi_value(&album_artist));
    let album_artist_c = album_artist_str.clone();
    let album_artist_sort_c = album_artist_sort.clone();
    let genre_str =
        models::join_multi_value(&models::parse_multi_value(&genre.unwrap_or_default()));
    let genre_c = genre_str.clone();

    let updated_count = tauri::async_runtime::spawn_blocking(move || {
        let mut count = 0u32;
        for item in songs_data {
            let path = std::path::PathBuf::from(&item.path);
            let write_res = crate::tageditor::write_tags(
                &path,
                &crate::tageditor::TagWriteRequest {
                    title: &item.title,
                    titlesort: item.titlesort.as_deref(),
                    artist: &item.artist,
                    artistsort: item.artistsort.as_deref(),
                    album: &album_c,
                    albumsort: albumsort_c.as_deref(),
                    album_artist: &album_artist_c,
                    album_artist_sort: album_artist_sort_c.as_deref(),
                    composer: &item.composer,
                    composersort: item.composersort.as_deref(),
                    genre: &genre_c,
                    track: item.track,
                    disc,
                    year,
                    grouping: &item.grouping,
                    bpm: item.bpm,
                    initial_key: &item.initial_key,
                    compilation,
                },
            );
            match write_res {
                Ok(_) => count += 1,
                Err(ref e) => {
                    log::warn!(
                        "Failed to persist tags to disk for song {}: {e:#}",
                        item.id
                    );
                }
            }
        }
        count
    })
    .await
    .map_err(|e| e.to_string())?;

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for &song_id in &song_ids {
        tx.execute(
            "UPDATE songs SET
                album = ?1,
                albumsort = ?2,
                album_artist = ?3,
                album_artist_sort = ?4,
                genre = ?5,
                genresort = ?6,
                year = ?7,
                disc = ?8,
                compilation = ?9
             WHERE id = ?10",
            rusqlite::params![
                album,
                albumsort,
                album_artist_str,
                album_artist_sort,
                genre_str,
                genresort,
                year,
                disc,
                compilation,
                song_id
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(updated_count)
}

/// Clear a single song's embedded cover art (#386) — removes the picture(s)
/// from the file's tag on disk, then re-resolves the song's automatic art to
/// folder art (if any exists next to the file) so the UI doesn't briefly
/// show nothing before the collection's normal remote-lookup fallback kicks
/// in. A manually-picked cover (`art_manual`) always takes precedence over
/// automatic art regardless, so it's left untouched.
#[tauri::command]
pub async fn clear_song_cover_art(state: State<'_, AppState>, song_id: i64) -> Result<(), String> {
    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let path_str: String = conn
        .query_row(
            "SELECT path FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| row.get(0),
        )
        .map_err(|_| "Song not found in library".to_string())?;

    let path = std::path::PathBuf::from(path_str);
    let path_clone = path.clone();
    tauri::async_runtime::spawn_blocking(move || crate::tageditor::clear_embedded_art(&path_clone))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;

    let folder_art = state
        .cover_manager
        .scan_folder_art(&path)
        .map(|p| p.to_string_lossy().to_string());

    conn.execute(
        "UPDATE songs SET art_embedded = 0, art_automatic = ?1, art_unset = 0 WHERE id = ?2",
        rusqlite::params![folder_art, song_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Bulk version of `clear_song_cover_art` for clearing an entire album's
/// embedded artwork at once (#386). Skips (rather than fails) songs whose
/// file couldn't be cleared, returning the count that actually succeeded.
#[tauri::command]
pub async fn clear_album_cover_art(
    state: State<'_, AppState>,
    song_ids: Vec<i64>,
) -> Result<u32, String> {
    if song_ids.is_empty() {
        return Ok(0);
    }

    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;

    let mut paths = Vec::with_capacity(song_ids.len());
    for &song_id in &song_ids {
        if let Ok(path_str) = conn.query_row(
            "SELECT path FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| row.get::<_, String>(0),
        ) {
            paths.push((song_id, std::path::PathBuf::from(path_str)));
        }
    }

    let cleared: Vec<(i64, std::path::PathBuf)> = tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .filter(|(_, path)| crate::tageditor::clear_embedded_art(path).is_ok())
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?;

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for (song_id, path) in &cleared {
        let folder_art = state
            .cover_manager
            .scan_folder_art(path)
            .map(|p| p.to_string_lossy().to_string());
        tx.execute(
            "UPDATE songs SET art_embedded = 0, art_automatic = ?1, art_unset = 0 WHERE id = ?2",
            rusqlite::params![folder_art, song_id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(cleared.len() as u32)
}
