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

/// Launches MusicBrainz Picard with the given songs' *containing folders*
/// (#367, #804) — a one-way handoff, see `crate::picard` for details. Takes
/// song IDs rather than raw paths so only real library songs can be handed
/// to an external process, and paths are resolved server-side.
///
/// Resolves to each song's parent directory rather than its own file path,
/// deduplicated, so a same-album selection (the common case) collapses to
/// a single argument. This is deliberately coarser than the exact
/// selection — a folder pulls in every track Picard finds there, not just
/// the selected ones — but it's what keeps a large cross-album selection
/// (e.g. the whole Missing Metadata playlist) down to one argument per
/// album instead of one per song, which both avoids the OS command-line
/// length limit and, more importantly, avoids handing Picard's own
/// single-instance IPC more than a couple of paths to forward at once: on
/// Windows that IPC recreates its pipe after every message, so a burst of
/// them can silently drop everything after the first (or, if fired at
/// Picard in a tight loop across many chunks, destabilize it entirely).
#[tauri::command]
pub async fn open_in_picard(state: State<'_, AppState>, song_ids: Vec<i64>) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let mut dirs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in song_ids {
        let path: Option<String> = conn
            .query_row("SELECT path FROM songs WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .ok();
        if let Some(dir) = path
            .as_deref()
            .map(std::path::Path::new)
            .and_then(|p| p.parent())
        {
            if seen.insert(dir.to_path_buf()) {
                dirs.push(dir.to_path_buf());
            }
        }
    }
    drop(conn);

    if dirs.is_empty() {
        return Err("No local files found for the selected songs".to_string());
    }

    let custom_path = read_custom_picard_path(&state)?;
    let exe = picard::find_picard(custom_path.as_deref()).ok_or_else(|| {
        "MusicBrainz Picard not found. Install it from picard.musicbrainz.org, or set a custom path in Settings.".to_string()
    })?;

    picard::launch_picard(&exe, &dirs).map_err(|e| e.to_string())
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
