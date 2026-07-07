use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_lyrics(state: State<'_, AppState>, song_id: i64) -> Result<String, String> {
    // 1. Check database cache
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let cached_lyrics: Option<String> = conn
        .query_row(
            "SELECT lyrics FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| row.get(0),
        )
        .ok();

    if let Some(lyrics) = cached_lyrics {
        if !lyrics.trim().is_empty() {
            return Ok(lyrics);
        }
    }

    // 2. Fetch metadata from DB to search online
    let song_metadata = conn
        .query_row(
            "SELECT artist, title, album, length_nanosec FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| {
                let artist: String = row.get(0).unwrap_or_default();
                let title: String = row.get(1).unwrap_or_default();
                let album: String = row.get(2).unwrap_or_default();
                let len_ns: i64 = row.get(3).unwrap_or(0);
                Ok((artist, title, album, len_ns))
            },
        )
        .map_err(|e| e.to_string())?;

    let (artist, title, album, len_ns) = song_metadata;
    if artist.trim().is_empty() || title.trim().is_empty() {
        return Err("insufficient song metadata (artist/title) to fetch online lyrics".to_string());
    }

    let duration_sec = (len_ns / 1_000_000_000) as u32;

    // 3. Query online APIs (LRCLIB -> Lyrics.ovh)
    let lyrics_manager = crate::lyrics::LyricsManager::new();
    let fetched = lyrics_manager
        .fetch_lyrics(&artist, &title, &album, duration_sec)
        .await
        .map_err(|e| e.to_string())?;

    // 4. Cache back in SQLite
    conn.execute(
        "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
        rusqlite::params![fetched, song_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(fetched)
}

#[tauri::command]
pub async fn save_lyrics(state: State<'_, AppState>, song_id: i64, lyrics: String) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
        rusqlite::params![lyrics, song_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
