use crate::{
    models::{PlaybackState, PlaylistItem, RepeatMode, ShuffleMode},
    AppState,
};
use tauri::State;

#[tauri::command]
pub async fn play_song(song_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    use rusqlite::params;
    let c = state.db.pool.get().map_err(|e| e.to_string())?;
    // Use a direct query to get by ID
    let song = c.query_row(
        "SELECT id, source, filetype, path, url, stream_url,
                title, titlesort, artist, artistsort,
                album, albumsort, album_artist, album_artist_sort,
                composer, composersort, performer, performersort,
                grouping, comment, lyrics,
                track, disc, year, originalyear, genre, compilation,
                bpm, mood, initial_key,
                length_nanosec, beginning_nanosec, end_nanosec,
                bitrate, samplerate, bitdepth, channels, filesize, mtime,
                rating, playcount, skipcount, lastplayed, lastseen,
                art_embedded, art_automatic, art_manual, art_unset,
                cue_path, ebur128_integrated_loudness_lufs, ebur128_loudness_range_lu
         FROM songs WHERE id = ?1",
        params![song_id],
        |row| {
            Ok(crate::models::Song {
                id: row.get(0)?,
                source: row.get::<_, i64>(1).map(crate::models::SongSource::from)?,
                filetype: row.get::<_, i64>(2).map(crate::models::FileType::from)?,
                path: row.get(3)?,
                url: row.get(4)?,
                stream_url: row.get(5)?,
                title: row.get(6)?,
                titlesort: row.get(7)?,
                artist: row.get(8)?,
                artistsort: row.get(9)?,
                album: row.get(10)?,
                albumsort: row.get(11)?,
                album_artist: row.get(12)?,
                album_artist_sort: row.get(13)?,
                composer: row.get(14)?,
                composersort: row.get(15)?,
                performer: row.get(16)?,
                performersort: row.get(17)?,
                grouping: row.get(18)?,
                comment: row.get(19)?,
                lyrics: row.get(20)?,
                track: row.get(21)?,
                disc: row.get(22)?,
                year: row.get(23)?,
                originalyear: row.get(24)?,
                genre: row.get(25)?,
                compilation: row.get(26)?,
                bpm: row.get(27)?,
                mood: row.get(28)?,
                initial_key: row.get(29)?,
                length_nanosec: row.get(30)?,
                beginning_nanosec: row.get::<_, Option<i64>>(31)?.unwrap_or(0),
                end_nanosec: row.get::<_, Option<i64>>(32)?.unwrap_or(0),
                bitrate: row.get(33)?,
                samplerate: row.get(34)?,
                bitdepth: row.get(35)?,
                channels: row.get(36)?,
                filesize: row.get(37)?,
                mtime: row.get(38)?,
                rating: row.get::<_, Option<f32>>(39)?.unwrap_or(-1.0),
                playcount: row.get::<_, Option<i32>>(40)?.unwrap_or(0),
                skipcount: row.get::<_, Option<i32>>(41)?.unwrap_or(0),
                lastplayed: row.get(42)?,
                lastseen: row.get(43)?,
                art_embedded: row.get(44)?,
                art_automatic: row.get(45)?,
                art_manual: row.get(46)?,
                art_unset: row.get(47)?,
                cue_path: row.get(48)?,
                ebur128_integrated_loudness_lufs: row.get(49)?,
                ebur128_loudness_range_lu: row.get(50)?,
                ..Default::default()
            })
        },
    )
    .map_err(|e| e.to_string())?;

    let item = PlaylistItem::new_song(0, 0, song);
    let mut player = state.player.lock().await;
    player
        .play_playlist(vec![item], 0, 0)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn play_songs(
    song_ids: Vec<i64>,
    start_index: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use rusqlite::params;
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;

    let mut songs = Vec::with_capacity(song_ids.len());
    for &id in &song_ids {
        let song = conn.query_row(
            "SELECT id, source, filetype, path, url, stream_url,
                    title, titlesort, artist, artistsort,
                    album, albumsort, album_artist, album_artist_sort,
                    composer, composersort, performer, performersort,
                    grouping, comment, lyrics,
                    track, disc, year, originalyear, genre, compilation,
                    bpm, mood, initial_key,
                    length_nanosec, beginning_nanosec, end_nanosec,
                    bitrate, samplerate, bitdepth, channels, filesize, mtime,
                    rating, playcount, skipcount, lastplayed, lastseen,
                    art_embedded, art_automatic, art_manual, art_unset,
                    cue_path, ebur128_integrated_loudness_lufs, ebur128_loudness_range_lu
             FROM songs WHERE id = ?1",
            params![id],
            |row| {
                Ok(crate::models::Song {
                    id: row.get(0)?,
                    source: row.get::<_, i64>(1).map(crate::models::SongSource::from)?,
                    filetype: row.get::<_, i64>(2).map(crate::models::FileType::from)?,
                    path: row.get(3)?,
                    url: row.get(4)?,
                    stream_url: row.get(5)?,
                    title: row.get(6)?,
                    titlesort: row.get(7)?,
                    artist: row.get(8)?,
                    artistsort: row.get(9)?,
                    album: row.get(10)?,
                    albumsort: row.get(11)?,
                    album_artist: row.get(12)?,
                    album_artist_sort: row.get(13)?,
                    composer: row.get(14)?,
                    composersort: row.get(15)?,
                    performer: row.get(16)?,
                    performersort: row.get(17)?,
                    grouping: row.get(18)?,
                    comment: row.get(19)?,
                    lyrics: row.get(20)?,
                    track: row.get(21)?,
                    disc: row.get(22)?,
                    year: row.get(23)?,
                    originalyear: row.get(24)?,
                    genre: row.get(25)?,
                    compilation: row.get(26)?,
                    bpm: row.get(27)?,
                    mood: row.get(28)?,
                    initial_key: row.get(29)?,
                    length_nanosec: row.get(30)?,
                    beginning_nanosec: row.get::<_, Option<i64>>(31)?.unwrap_or(0),
                    end_nanosec: row.get::<_, Option<i64>>(32)?.unwrap_or(0),
                    bitrate: row.get(33)?,
                    samplerate: row.get(34)?,
                    bitdepth: row.get(35)?,
                    channels: row.get(36)?,
                    filesize: row.get(37)?,
                    mtime: row.get(38)?,
                    rating: row.get::<_, Option<f32>>(39)?.unwrap_or(-1.0),
                    playcount: row.get::<_, Option<i32>>(40)?.unwrap_or(0),
                    skipcount: row.get::<_, Option<i32>>(41)?.unwrap_or(0),
                    lastplayed: row.get(42)?,
                    lastseen: row.get(43)?,
                    art_embedded: row.get(44)?,
                    art_automatic: row.get(45)?,
                    art_manual: row.get(46)?,
                    art_unset: row.get(47)?,
                    cue_path: row.get(48)?,
                    ebur128_integrated_loudness_lufs: row.get(49)?,
                    ebur128_loudness_range_lu: row.get(50)?,
                    ..Default::default()
                })
            },
        );
        if let Ok(s) = song {
            songs.push(s);
        }
    }

    let items = songs
        .into_iter()
        .enumerate()
        .map(|(i, s)| PlaylistItem::new_song(0, i as i32, s))
        .collect::<Vec<_>>();

    let mut player = state.player.lock().await;
    player
        .play_playlist(items, start_index, 0)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn play_playlist_item(
    playlist_id: i64,
    item_index: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let items = {
        let playlists = state.playlists.lock().await;
        playlists
            .get_playlist_tracks(playlist_id)
            .map_err(|e| e.to_string())?
    };
    let mut player = state.player.lock().await;
    player
        .play_playlist(items, item_index, playlist_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause(state: State<'_, AppState>) -> Result<(), String> {
    state.player.lock().await.pause().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resume(state: State<'_, AppState>) -> Result<(), String> {
    state.player.lock().await.resume().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop(state: State<'_, AppState>) -> Result<(), String> {
    state.player.lock().await.stop().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn next_track(state: State<'_, AppState>) -> Result<(), String> {
    state
        .player
        .lock()
        .await
        .next_track()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn previous_track(state: State<'_, AppState>) -> Result<(), String> {
    state
        .player
        .lock()
        .await
        .previous_track()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn seek_to(position_nanosec: i64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .player
        .lock()
        .await
        .seek_to(position_nanosec as u64)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_volume(volume: f32, state: State<'_, AppState>) -> Result<(), String> {
    state
        .player
        .lock()
        .await
        .set_volume(volume)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_playback_state(state: State<'_, AppState>) -> Result<PlaybackState, String> {
    Ok(state.player.lock().await.get_state().await)
}

#[tauri::command]
pub async fn set_shuffle_mode(mode: ShuffleMode, state: State<'_, AppState>) -> Result<(), String> {
    state.player.lock().await.set_shuffle_mode(mode);
    Ok(())
}

#[tauri::command]
pub async fn set_repeat_mode(mode: RepeatMode, state: State<'_, AppState>) -> Result<(), String> {
    state.player.lock().await.set_repeat_mode(mode);
    Ok(())
}
