use crate::collection::WatcherPauseGuard;
use crate::models;
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
    pub originalyear: Option<u32>,
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
                originalyear, grouping, bpm, initial_key, rating, compilation, art_embedded
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
                originalyear: row.get(17).ok(),
                grouping: row.get(18).unwrap_or_default(),
                bpm: row.get(19).ok(),
                initial_key: row.get(20).unwrap_or_default(),
                rating: row.get(21).unwrap_or(crate::stats::RATING_UNRATED),
                compilation: row.get(22).unwrap_or(false),
                art_embedded: row.get(23).unwrap_or(false),
            })
        },
    )
    .map_err(|e| e.to_string())
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
    originalyear: Option<u32>,
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
    let (path_str, source, compilation): (String, i32, bool) = conn
        .query_row(
            "SELECT path, source, compilation FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| "Song not found in library".to_string())?;
    // WebDAV songs (source 11) have no local file to write lofty tags to —
    // there's no write-back to the remote server implemented, so the edit is
    // saved to Luminous's own DB only (the tag editor surfaces this to the
    // user). Attempting the on-disk write here would always fail and abort
    // the whole save before the DB update below ever ran.
    let is_webdav = source == models::SongSource::WebDav as i32;

    let path = std::path::PathBuf::from(path_str);
    // Close the timing race the coarse guard above can't (#514): the OS's own
    // change notification for this write may arrive after the guard's grace
    // window elapses, so track the exact path too.
    state
        .self_writes
        .mark_written(std::iter::once(path.clone()));

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

    if !is_webdav {
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
                    originalyear,
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
    }

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
            originalyear = ?16,
            grouping = ?17,
            bpm = ?18,
            initial_key = ?19
         WHERE id = ?20",
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
            originalyear,
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
        source: i32,
        title: String,
        titlesort: Option<String>,
        artist: String,
        artistsort: Option<String>,
        composer: String,
        composersort: Option<String>,
        track: Option<u32>,
        originalyear: Option<u32>,
        grouping: String,
        bpm: Option<f32>,
        initial_key: String,
    }

    let mut songs_data = Vec::with_capacity(song_ids.len());
    for &song_id in &song_ids {
        let res = conn.query_row(
            "SELECT path, source, title, titlesort, artist, artistsort, composer, composersort, track, originalyear, grouping, bpm, initial_key
             FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| {
                Ok(SongMetadata {
                    id: song_id,
                    path: row.get(0)?,
                    source: row.get(1)?,
                    title: row.get(2).unwrap_or_default(),
                    titlesort: row.get(3).ok(),
                    artist: row.get(4).unwrap_or_default(),
                    artistsort: row.get(5).ok(),
                    composer: row.get(6).unwrap_or_default(),
                    composersort: row.get(7).ok(),
                    track: row.get(8).ok(),
                    originalyear: row.get(9).ok(),
                    grouping: row.get(10).unwrap_or_default(),
                    bpm: row.get(11).ok(),
                    initial_key: row.get(12).unwrap_or_default(),
                })
            },
        );
        if let Ok(meta) = res {
            songs_data.push(meta);
        }
    }

    // See save_song_tags — close the timing race the coarse guard above can't
    // (#514) by tracking every path this batch is about to write.
    state
        .self_writes
        .mark_written(songs_data.iter().map(|m| std::path::PathBuf::from(&m.path)));

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
            // WebDAV songs (source 11) have no local file to write lofty tags to,
            // and there's no write-back to the remote server implemented — the
            // change is saved to Luminous's own DB only (the tag editor surfaces
            // this to the user), same as save_song_tags/rewrite_genre_and_persist.
            if item.source == models::SongSource::WebDav as i32 {
                count += 1;
                continue;
            }
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
                    originalyear: item.originalyear,
                    grouping: &item.grouping,
                    bpm: item.bpm,
                    initial_key: &item.initial_key,
                    compilation,
                },
            );
            match write_res {
                Ok(_) => count += 1,
                Err(ref e) => {
                    log::warn!("Failed to persist tags to disk for song {}: {e:#}", item.id);
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
    let (path_str, source): (String, i32) = conn
        .query_row(
            "SELECT path, source FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "Song not found in library".to_string())?;

    let path = std::path::PathBuf::from(path_str);
    // WebDAV songs (source 11) have no local file to clear an embedded
    // picture from, and there's no write-back to the remote server
    // implemented — same as tag edits (see save_song_tags), this is
    // DB-only. The tag editor hides the Clear Artwork button for these
    // songs; this guard is what keeps it from erroring if it's ever
    // reached some other way.
    if source != models::SongSource::WebDav as i32 {
        // See save_song_tags — close the timing race the coarse guard above can't (#514).
        state
            .self_writes
            .mark_written(std::iter::once(path.clone()));
        let path_clone = path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::tageditor::clear_embedded_art(&path_clone)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;
    }

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

    let mut local_paths = Vec::with_capacity(song_ids.len());
    let mut webdav_paths = Vec::new();
    for &song_id in &song_ids {
        if let Ok((path_str, source)) = conn.query_row(
            "SELECT path, source FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
        ) {
            let path = std::path::PathBuf::from(path_str);
            // WebDAV songs (source 11) have no local file to clear an embedded
            // picture from — DB-only, same as clear_song_cover_art above.
            if source == models::SongSource::WebDav as i32 {
                webdav_paths.push((song_id, path));
            } else {
                local_paths.push((song_id, path));
            }
        }
    }

    // See save_song_tags — close the timing race the coarse guard above can't (#514).
    state
        .self_writes
        .mark_written(local_paths.iter().map(|(_, p)| p.clone()));

    let mut cleared: Vec<(i64, std::path::PathBuf)> =
        tauri::async_runtime::spawn_blocking(move || {
            local_paths
                .into_iter()
                .filter(|(_, path)| crate::tageditor::clear_embedded_art(path).is_ok())
                .collect()
        })
        .await
        .map_err(|e| e.to_string())?;
    cleared.extend(webdav_paths);

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
