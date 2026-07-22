use crate::tageditor::SuggestedTags;
use crate::AppState;
use tauri::State;

#[derive(serde::Serialize)]
pub struct SongDetails {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub composer: String,
    pub genre: String,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub rating: f32,
}

#[tauri::command]
pub async fn get_song_details(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<SongDetails, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, path, title, artist, album, album_artist, composer, genre, track, disc, year, rating
         FROM songs WHERE id = ?1",
        rusqlite::params![song_id],
        |row| {
            Ok(SongDetails {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2).unwrap_or_default(),
                artist: row.get(3).unwrap_or_default(),
                album: row.get(4).unwrap_or_default(),
                album_artist: row.get(5).unwrap_or_default(),
                composer: row.get(6).unwrap_or_default(),
                genre: row.get(7).unwrap_or_default(),
                track: row.get(8).ok(),
                disc: row.get(9).ok(),
                year: row.get(10).ok(),
                rating: row.get(11).unwrap_or(crate::stats::RATING_UNRATED),
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lookup_acoustid_tags(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<SuggestedTags, String> {
    // 1. Fetch file path from database
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let path_str: String = conn
        .query_row(
            "SELECT path FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| row.get(0),
        )
        .map_err(|_| "Song not found in library".to_string())?;

    let path = std::path::PathBuf::from(path_str);

    // 2. Generate fingerprint (blocking subprocess invocation)
    let (fingerprint, duration_sec) =
        tauri::async_runtime::spawn_blocking(move || crate::tageditor::generate_fingerprint(&path))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;

    // 3. Query AcoustID web service lookup
    let suggestions = crate::tageditor::lookup_acoustid(&fingerprint, duration_sec)
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
    artist: String,
    album: String,
    album_artist: String,
    composer: String,
    genre: String,
    track: Option<u32>,
    disc: Option<u32>,
    year: Option<u32>,
) -> Result<(), String> {
    // 1. Fetch file path from database
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let path_str: String = conn
        .query_row(
            "SELECT path FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| row.get(0),
        )
        .map_err(|_| "Song not found in library".to_string())?;

    let path = std::path::PathBuf::from(path_str);

    // 2. Write metadata back to disk (blocking lofty write in threadpool)
    let path_clone = path.clone();
    let title_c = title.clone();
    let artist_c = artist.clone();
    let album_c = album.clone();
    let album_artist_c = album_artist.clone();
    let composer_c = composer.clone();
    let genre_c = genre.clone();

    tauri::async_runtime::spawn_blocking(move || {
        crate::tageditor::write_tags(
            &path_clone,
            &title_c,
            &artist_c,
            &album_c,
            &album_artist_c,
            &composer_c,
            &genre_c,
            track,
            disc,
            year,
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    // 3. Update SQLite database cache in-place
    conn.execute(
        "UPDATE songs SET
            title = ?1,
            artist = ?2,
            album = ?3,
            album_artist = ?4,
            composer = ?5,
            genre = ?6,
            track = ?7,
            disc = ?8,
            year = ?9
         WHERE id = ?10",
        rusqlite::params![
            title,
            artist,
            album,
            album_artist,
            composer,
            genre,
            track,
            disc,
            year,
            song_id
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn save_album_tags(
    state: State<'_, AppState>,
    song_ids: Vec<i64>,
    album: String,
    album_artist: String,
    genre: String,
    year: Option<u32>,
) -> Result<u32, String> {
    if song_ids.is_empty() {
        return Ok(0);
    }

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;

    struct SongMetadata {
        path: String,
        title: String,
        artist: String,
        composer: String,
        track: Option<u32>,
        disc: Option<u32>,
    }

    let mut songs_data = Vec::with_capacity(song_ids.len());
    for &song_id in &song_ids {
        let res = conn.query_row(
            "SELECT path, title, artist, composer, track, disc FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| {
                Ok(SongMetadata {
                    path: row.get(0)?,
                    title: row.get(1).unwrap_or_default(),
                    artist: row.get(2).unwrap_or_default(),
                    composer: row.get(3).unwrap_or_default(),
                    track: row.get(4).ok(),
                    disc: row.get(5).ok(),
                })
            },
        );
        if let Ok(meta) = res {
            songs_data.push(meta);
        }
    }

    let album_c = album.clone();
    let album_artist_c = album_artist.clone();
    let genre_c = genre.clone();

    let updated_count = tauri::async_runtime::spawn_blocking(move || {
        let mut count = 0u32;
        for item in songs_data {
            let path = std::path::PathBuf::from(&item.path);
            let write_res = crate::tageditor::write_tags(
                &path,
                &item.title,
                &item.artist,
                &album_c,
                &album_artist_c,
                &item.composer,
                &genre_c,
                item.track,
                item.disc,
                year,
            );
            if write_res.is_ok() {
                count += 1;
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
                album_artist = ?2,
                genre = ?3,
                year = ?4
             WHERE id = ?5",
            rusqlite::params![album, album_artist, genre, year, song_id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(updated_count)
}
