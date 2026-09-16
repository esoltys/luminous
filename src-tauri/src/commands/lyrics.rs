use crate::lyrics::{get_lyrics_for_song, LyricsManager};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_lyrics(
    state: State<'_, AppState>,
    song_id: i64,
    force_refresh: Option<bool>,
) -> Result<String, String> {
    let force_refresh = force_refresh.unwrap_or(false);
    eprintln!("[Luminous Backend] get_lyrics called for song_id: {song_id}, force_refresh: {force_refresh}");

    let lyrics_manager = LyricsManager::new();
    let result = get_lyrics_for_song(&state.db, &lyrics_manager, song_id, force_refresh).await;

    match &result {
        Ok(lyrics) => eprintln!(
            "[Luminous Backend] get_lyrics resolved (len: {})",
            lyrics.len()
        ),
        Err(e) => eprintln!("[Luminous Backend] get_lyrics failed: {e}"),
    }
    result
}

#[tauri::command]
pub async fn save_lyrics(
    state: State<'_, AppState>,
    song_id: i64,
    lyrics: String,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;

    let (path_str, title, track): (Option<String>, Option<String>, Option<i32>) = conn
        .query_row(
            "SELECT path, title, track FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap_or((None, None, None));

    if let Some(ref path_str) = path_str {
        let audio_path = std::path::Path::new(path_str);
        if let Some(lrc_path) = crate::lyrics::find_sidecar_lrc(audio_path, title.as_deref(), track)
        {
            let clean_lyrics = if lyrics.starts_with("[synced:false]\n") {
                &lyrics["[synced:false]\n".len()..]
            } else if lyrics.starts_with("[synced:false]") {
                &lyrics["[synced:false]".len()..]
            } else {
                &lyrics
            };
            let _ = std::fs::write(&lrc_path, clean_lyrics);
        }
    }

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
