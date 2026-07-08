use crate::AppState;
use tauri::State;

fn is_synced_lrc(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 6 {
        return false;
    }
    for i in 0..(bytes.len() - 5) {
        if bytes[i] == b'['
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3] == b':'
            && bytes[i + 4].is_ascii_digit()
            && bytes[i + 5].is_ascii_digit()
        {
            return true;
        }
    }
    false
}

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
        .ok()
        .flatten();

    if let Some(ref lyrics) = cached_lyrics {
        if !lyrics.trim().is_empty() && is_synced_lrc(lyrics) {
            return Ok(lyrics.clone());
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
        if let Some(lyrics) = cached_lyrics {
            if !lyrics.trim().is_empty() {
                return Ok(lyrics);
            }
        }
        return Err("insufficient song metadata (artist/title) to fetch online lyrics".to_string());
    }

    let duration_sec = (len_ns / 1_000_000_000) as u32;

    // 3. Query online APIs (LRCLIB -> Lyrics.ovh)
    let lyrics_manager = crate::lyrics::LyricsManager::new();
    match lyrics_manager
        .fetch_lyrics(&artist, &title, &album, duration_sec)
        .await
    {
        Ok(fetched) => {
            // Cache back in SQLite
            conn.execute(
                "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
                rusqlite::params![fetched, song_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(fetched)
        }
        Err(e) => {
            // Online search failed, fall back to cached plain text if available
            if let Some(lyrics) = cached_lyrics {
                if !lyrics.trim().is_empty() {
                    return Ok(lyrics);
                }
            }
            Err(e.to_string())
        }
    }
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
