//! Tauri IPC commands for lossless audio provenance verification (#1373).

use crate::provenance::{get_cached_provenance, verify_song_provenance, SongProvenance};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_song_provenance(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<Option<SongProvenance>, String> {
    get_cached_provenance(&state.db, song_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn analyze_song_provenance(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<SongProvenance, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || verify_song_provenance(&db, song_id))
        .await
        .map_err(|e| format!("Worker thread failed: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn analyze_songs_provenance(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    song_ids: Vec<i64>,
) -> Result<usize, String> {
    use tauri::Emitter;
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let mut count = 0;
        for id in song_ids {
            if let Ok(res) = verify_song_provenance(&db, id) {
                let _ = app.emit("song-provenance-changed", &res);
                count += 1;
            }
        }
        count
    })
    .await
    .map_err(|e| format!("Worker thread failed: {e}"))
}
