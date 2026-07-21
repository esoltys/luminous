use crate::lyrics::is_synced_lrc;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_lyrics(state: State<'_, AppState>, song_id: i64) -> Result<String, String> {
    eprintln!("[Luminous Backend] get_lyrics called for song_id: {song_id}");

    // 1. Check database cache and instrumental flag
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let (cached_lyrics, is_instrumental): (Option<String>, bool) = conn
        .query_row(
            "SELECT lyrics, COALESCE(is_instrumental, 0) FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or((None, false));

    if is_instrumental {
        eprintln!(
            "[Luminous Backend] Track is marked instrumental. Skipping online lyrics search."
        );
        return Err("Song is marked as instrumental".to_string());
    }

    if let Some(ref lyrics) = cached_lyrics {
        if !lyrics.trim().is_empty() {
            let is_synced = is_synced_lrc(lyrics);
            let has_plain_marker = lyrics.starts_with("[synced:false]");

            // If the cached lyrics are synced LRC, or if we have already checked online and marked it unsynced,
            // return immediately without hitting the network!
            if is_synced || has_plain_marker {
                eprintln!(
                    "[Luminous Backend] Cache hit in SQLite. Returning cached lyrics (len: {}, synced: {is_synced}, marked_unsynced: {has_plain_marker})",
                    lyrics.len()
                );
                return Ok(lyrics.clone());
            }
            eprintln!("[Luminous Backend] Cache has plain lyrics from tags (not marked checked). Querying online for synced version...");
        }
    } else {
        eprintln!("[Luminous Backend] Cache miss. Querying online...");
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
    eprintln!(
        "[Luminous Backend] Metadata found: artist='{artist}', title='{title}', album='{album}', length={len_ns}ns"
    );

    if artist.trim().is_empty() || title.trim().is_empty() {
        if let Some(lyrics) = cached_lyrics {
            if !lyrics.trim().is_empty() {
                eprintln!("[Luminous Backend] Insufficient metadata for online fetch, returning cached local lyrics");
                return Ok(lyrics);
            }
        }
        eprintln!(
            "[Luminous Backend] Error: insufficient metadata (artist/title) to query lyrics online"
        );
        return Err("insufficient song metadata (artist/title) to fetch online lyrics".to_string());
    }

    let duration_sec = (len_ns / 1_000_000_000) as u32;

    // 3. Query online APIs (LRCLIB -> Lyrics.ovh)
    eprintln!("[Luminous Backend] Querying online lyrics providers for '{artist}' - '{title}' (duration: {duration_sec}s)...");
    let lyrics_manager = crate::lyrics::LyricsManager::new();
    match lyrics_manager
        .fetch_lyrics(&artist, &title, &album, duration_sec)
        .await
    {
        Ok(fetched) => {
            let is_synced = is_synced_lrc(&fetched);
            let final_lyrics = if is_synced {
                fetched
            } else {
                format!("[synced:false]\n{fetched}")
            };
            eprintln!(
                "[Luminous Backend] Successfully fetched online lyrics (len: {}, synced: {is_synced}). Caching in SQLite...",
                final_lyrics.len()
            );
            // Cache back in SQLite
            conn.execute(
                "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
                rusqlite::params![final_lyrics, song_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(final_lyrics)
        }
        Err(e) => {
            eprintln!("[Luminous Backend] Online search failed: {e}");
            // Online search failed, fall back to cached local lyrics if available
            if let Some(lyrics) = cached_lyrics {
                if !lyrics.trim().is_empty() {
                    // Mark as checked to prevent future online lookup spamming
                    let marked_lyrics = if lyrics.starts_with("[synced:false]") {
                        lyrics.clone()
                    } else {
                        format!("[synced:false]\n{lyrics}")
                    };
                    let _ = conn.execute(
                        "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
                        rusqlite::params![marked_lyrics, song_id],
                    );
                    eprintln!("[Luminous Backend] Falling back to cached local lyrics (marked unsynced in SQLite)");
                    return Ok(marked_lyrics);
                }
            }
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn save_lyrics(
    state: State<'_, AppState>,
    song_id: i64,
    lyrics: String,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
        rusqlite::params![lyrics, song_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn set_instrumental(
    state: State<'_, AppState>,
    song_id: i64,
    is_instrumental: bool,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE songs SET is_instrumental = ?1 WHERE id = ?2",
        rusqlite::params![is_instrumental, song_id],
    )
    .map_err(|e| e.to_string())?;

    let mut player = state.player.lock().await;
    player.update_song_instrumental(song_id, is_instrumental);
    Ok(())
}
