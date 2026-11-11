//! Commands for managing metadata rejections and conflict blocklist (#1581).

use crate::lyrics::{get_lyrics_for_song, LyricsManager, LyricsPayload};
use crate::metadata_blocklist::{MetadataBlocklist, MetadataRejection};
use crate::AppState;
use tauri::State;

/// Rejects the online lyrics currently associated with `song_id`, records
/// the provider in the metadata blocklist, and attempts to resolve alternative
/// lyrics from another online provider (or leaves them empty if none exist).
#[tauri::command]
pub async fn reject_song_lyrics(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<Option<LyricsPayload>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;

    let lyrics_source: Option<String> = conn
        .query_row(
            "SELECT lyrics_source FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    if let Some(ref source) = lyrics_source {
        let parsed_source =
            crate::lyrics::LyricsSource::from(source.trim().to_lowercase().as_str());
        // Record blocklist rejection for online providers
        if parsed_source.is_online() {
            MetadataBlocklist::reject(
                &conn,
                "song",
                &song_id.to_string(),
                "lyrics",
                parsed_source.as_str(),
                "*",
            )
            .map_err(|e| e.to_string())?;
        }
    }

    // If there was a sidecar lyrics file created automatically or saved, check if we need to remove sidecar
    // Usually sidecars are user files, so we only clear DB cached lyrics.
    conn.execute(
        "UPDATE songs SET lyrics = NULL, lyrics_source = NULL WHERE id = ?1",
        rusqlite::params![song_id],
    )
    .map_err(|e| e.to_string())?;
    drop(conn);

    // Attempt to resolve lyrics again with the rejected provider excluded
    let lyrics_manager = LyricsManager::new();
    match get_lyrics_for_song(&state.db, &lyrics_manager, song_id, true).await {
        Ok(payload) => Ok(Some(payload)),
        Err(_) => Ok(None),
    }
}

/// Rejects the auto-downloaded cover art for the album containing `song_id`,
/// records the rejection in the blocklist, removes cached image files, and
/// sets `art_unset = 1` so automated lookups do not re-fetch the rejected art.
#[tauri::command]
pub async fn reject_album_cover(state: State<'_, AppState>, song_id: i64) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;

    let (artist, album, album_artist, album_key, art_automatic): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT artist, album, album_artist, album_key, art_automatic FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;

    let key = album_key.unwrap_or_else(|| {
        crate::album_key::album_key_for(
            album.as_deref().unwrap_or(""),
            album_artist.as_deref(),
            artist.as_deref(),
            false,
        )
    });

    // Record blocklist entry for the album
    MetadataBlocklist::reject(&conn, "album", &key, "cover_art", "itunes", "*")
        .map_err(|e| e.to_string())?;

    // Also record for this specific song
    MetadataBlocklist::reject(
        &conn,
        "song",
        &song_id.to_string(),
        "cover_art",
        "itunes",
        "*",
    )
    .map_err(|e| e.to_string())?;

    // If art_automatic was a cached album-* file in covers_dir, remove it from disk
    if let Some(ref auto) = art_automatic {
        if auto.starts_with("album-") {
            let cover_path = state.cover_manager.covers_dir().join(auto);
            if cover_path.is_file() {
                let _ = std::fs::remove_file(&cover_path);
            }
        }
    }

    // Clear art_automatic and set art_unset = 1 across sibling songs of this album
    let query_artist = artist.unwrap_or_default();
    let query_album = album.unwrap_or_default();
    conn.execute(
        "UPDATE songs SET art_automatic = NULL, art_unset = 1
         WHERE (album_key = ?1 OR (artist = ?2 COLLATE NOCASE AND album = ?3 COLLATE NOCASE))
           AND art_embedded = 0",
        rusqlite::params![key, query_artist, query_album],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// List rejected metadata entries, optionally filtered by `entity_type` or `category`.
#[tauri::command]
pub async fn list_metadata_rejections(
    state: State<'_, AppState>,
    entity_type: Option<String>,
    category: Option<String>,
) -> Result<Vec<MetadataRejection>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    MetadataBlocklist::list_rejections(&conn, entity_type.as_deref(), None, category.as_deref())
        .map_err(|e| e.to_string())
}

/// Unblock a specific metadata rejection by its entry ID.
#[tauri::command]
pub async fn unblock_metadata_rejection(
    state: State<'_, AppState>,
    rejection_id: i64,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    MetadataBlocklist::unblock(&conn, rejection_id).map_err(|e| e.to_string())
}

/// Clear all rejections for a given entity and category.
#[tauri::command]
pub async fn clear_metadata_rejections(
    state: State<'_, AppState>,
    entity_type: String,
    entity_id: String,
    category: String,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    MetadataBlocklist::clear(&conn, &entity_type, &entity_id, &category).map_err(|e| e.to_string())
}
