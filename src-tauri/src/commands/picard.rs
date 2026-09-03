use crate::picard;
use crate::AppState;
use tauri::State;

fn read_custom_picard_path(state: &State<'_, AppState>) -> Result<Option<String>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    Ok(conn
        .query_row(
            "SELECT value FROM app_state WHERE key = 'picard_path'",
            [],
            |row| row.get(0),
        )
        .ok()
        .filter(|s: &String| !s.trim().is_empty()))
}

/// Launches MusicBrainz Picard with the given songs' file paths (#367) — a
/// one-way handoff, see `crate::picard` for details. Takes song IDs rather
/// than raw paths so only real library songs can be handed to an external
/// process, and paths are resolved server-side.
#[tauri::command]
pub async fn open_in_picard(state: State<'_, AppState>, song_ids: Vec<i64>) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    for id in song_ids {
        let path: Option<String> = conn
            .query_row("SELECT path FROM songs WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .ok();
        if let Some(path) = path {
            paths.push(std::path::PathBuf::from(path));
        }
    }
    drop(conn);

    if paths.is_empty() {
        return Err("No local files found for the selected songs".to_string());
    }

    let custom_path = read_custom_picard_path(&state)?;
    let exe = picard::find_picard(custom_path.as_deref()).ok_or_else(|| {
        "MusicBrainz Picard not found. Install it from picard.musicbrainz.org, or set a custom path in Settings.".to_string()
    })?;

    picard::launch_picard(&exe, &paths).map_err(|e| e.to_string())
}

/// Resolves the Picard executable's current path, if found — `None` means
/// not installed/not locatable. Serves both the cheap "is it available"
/// check the frontend uses to disable "Open in Picard" actions, and the
/// Settings page's integration status display (which also wants the actual
/// path, not just a bool).
#[tauri::command]
pub async fn get_picard_path(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let custom_path = read_custom_picard_path(&state)?;
    Ok(picard::find_picard(custom_path.as_deref()).map(|p| p.to_string_lossy().to_string()))
}
