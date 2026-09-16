//! Commands backing the user-curated "Pinned" shelf on Home (#222). A pin is
//! stored as a lightweight `(item_type, ref_key)` reference — `get_pinned_items`
//! resolves each reference against the live song/album/artist/playlist tables
//! on every read, so a pinned item always reflects current data (artwork,
//! rating, track count, ...) rather than a stale snapshot. A reference that no
//! longer resolves (e.g. a pinned song was deleted) is silently omitted from
//! the result rather than treated as an error — the orphan `pinned_items` row
//! is left in place and self-heals if the item reappears. The DB/query logic
//! itself lives in `crate::pins`; this file is just the Tauri wiring.

use crate::{collection::CollectionScanner, models::PinnedItem, pins, AppState};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn pin_item(
    item_type: String,
    ref_key: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    pins::pin(&conn, &item_type, &ref_key).map_err(|e| e.to_string())?;
    let _ = app.emit("pinned-items-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn unpin_item(
    item_type: String,
    ref_key: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    pins::unpin(&conn, &item_type, &ref_key).map_err(|e| e.to_string())?;
    let _ = app.emit("pinned-items-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn reorder_pinned_items(
    order: Vec<(String, String)>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    pins::reorder(&conn, &order).map_err(|e| e.to_string())?;
    let _ = app.emit("pinned-items-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn get_pinned_items(state: State<'_, AppState>) -> Result<Vec<PinnedItem>, String> {
    let refs = {
        let conn = state.db.pool.get().map_err(|e| e.to_string())?;
        pins::pinned_refs(&conn).map_err(|e| e.to_string())?
    };
    if refs.is_empty() {
        return Ok(Vec::new());
    }

    let scanner = CollectionScanner::new(state.db.clone());
    let mut albums_cache: Option<Vec<serde_json::Value>> = None;
    let mut artists_cache: Option<Vec<serde_json::Value>> = None;
    let mut playlists_cache: Option<Vec<crate::models::Playlist>> = None;

    let mut items = Vec::with_capacity(refs.len());
    for (item_type, ref_key) in refs {
        match item_type.as_str() {
            "song" => {
                let conn = state.db.pool.get().map_err(|e| e.to_string())?;
                if let Some(song) =
                    pins::resolve_song(&conn, &ref_key).map_err(|e| e.to_string())?
                {
                    items.push(PinnedItem::Song {
                        song: Box::new(song),
                    });
                }
            }
            "album" => {
                let albums = match &albums_cache {
                    Some(a) => a,
                    None => {
                        albums_cache = Some(pins::all_albums(&scanner).map_err(|e| e.to_string())?);
                        albums_cache.as_ref().unwrap()
                    }
                };
                if let Some(value) = pins::find_album(albums, &ref_key) {
                    items.push(PinnedItem::Album {
                        album: pins::album_item_from_json(value),
                    });
                }
            }
            "artist" => {
                let artists = match &artists_cache {
                    Some(a) => a,
                    None => {
                        artists_cache =
                            Some(pins::all_artists(&scanner).map_err(|e| e.to_string())?);
                        artists_cache.as_ref().unwrap()
                    }
                };
                if let Some(value) = pins::find_artist(artists, &ref_key) {
                    if let Ok(artist) = pins::artist_item_from_json(value) {
                        items.push(PinnedItem::Artist { artist });
                    }
                }
            }
            "playlist" => {
                let Ok(playlist_id) = ref_key.parse::<i64>() else {
                    continue;
                };
                let playlists = match &playlists_cache {
                    Some(p) => p,
                    None => {
                        let p = state
                            .playlists
                            .lock()
                            .await
                            .get_playlists()
                            .map_err(|e| e.to_string())?;
                        playlists_cache = Some(p);
                        playlists_cache.as_ref().unwrap()
                    }
                };
                if let Some(playlist) = playlists.iter().find(|p| p.id == playlist_id) {
                    items.push(PinnedItem::Playlist {
                        playlist: playlist.clone(),
                    });
                }
            }
            "auto_playlist" => {
                let playlists = match &playlists_cache {
                    Some(p) => p,
                    None => {
                        let p = state
                            .playlists
                            .lock()
                            .await
                            .get_playlists()
                            .map_err(|e| e.to_string())?;
                        playlists_cache = Some(p);
                        playlists_cache.as_ref().unwrap()
                    }
                };
                if let Some(auto_playlist) =
                    pins::resolve_auto_playlist(&scanner, playlists, &ref_key)
                        .map_err(|e| e.to_string())?
                {
                    items.push(PinnedItem::AutoPlaylist { auto_playlist });
                }
            }
            _ => {}
        }
    }

    Ok(items)
}
