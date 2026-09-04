use crate::covermanager::{
    local_artwork_uri, scan_extended_artwork, ArtworkCategory, ExtendedArtworkSet,
};
use crate::AppState;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::path::Path;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn get_cover_art_uri(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<Option<String>, String> {
    state
        .cover_manager
        .get_cover_art_uri(song_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_remote_cover(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<Option<String>, String> {
    state
        .cover_manager
        .fetch_remote_cover(song_id)
        .await
        .map_err(|e| e.to_string())
}

/// One discovered artwork file, categorized and mapped to a
/// `luminous-art://` URI the frontend can drop straight into an `<img>` tag.
#[derive(Debug, Clone, Serialize)]
pub struct ExtendedArtworkItem {
    pub category: &'static str,
    pub uri: String,
}

/// Response shape for `get_extended_artwork_for_song`/`_for_artist` (#758):
/// `items` carries every discovered file in hierarchy order, and the
/// `*_uri` fields pull out the ones the UI needs directly without having to
/// scan `items` itself — `primary_uri` for the album cover-stack thumbnail
/// and "Open Images" target (#760), the artist fields for `ArtistDetailView`
/// (#761).
#[derive(Debug, Clone, Serialize, Default)]
pub struct ExtendedArtworkResponse {
    pub count: usize,
    pub primary_uri: Option<String>,
    pub artist_portrait_uri: Option<String>,
    pub band_logo_uri: Option<String>,
    pub fanart_uri: Option<String>,
    pub items: Vec<ExtendedArtworkItem>,
}

fn build_extended_artwork_response(set: ExtendedArtworkSet) -> ExtendedArtworkResponse {
    let mut response = ExtendedArtworkResponse {
        count: set.entries.len(),
        primary_uri: set.primary().map(|e| local_artwork_uri(&e.path)),
        ..Default::default()
    };

    for entry in &set.entries {
        let uri = local_artwork_uri(&entry.path);
        match entry.category {
            ArtworkCategory::ArtistPortrait if response.artist_portrait_uri.is_none() => {
                response.artist_portrait_uri = Some(uri.clone());
            }
            ArtworkCategory::BandLogo if response.band_logo_uri.is_none() => {
                response.band_logo_uri = Some(uri.clone());
            }
            ArtworkCategory::FanartBanner if response.fanart_uri.is_none() => {
                response.fanart_uri = Some(uri.clone());
            }
            _ => {}
        }
        response.items.push(ExtendedArtworkItem {
            category: entry.category.as_str(),
            uri,
        });
    }

    response
}

/// Full hierarchical artwork scan (#98/#757) for one song's album directory
/// and its parent artist directory, exposed over IPC. There's no dedicated
/// "album" row in the schema — an album is just a group of songs sharing an
/// `album` value — so this is keyed on any one representative song from the
/// album rather than a synthetic album id; the frontend already has a
/// song's id in hand wherever it renders album art.
///
/// Computed on demand rather than cached: this is called from low-frequency
/// detail views (album hero, cover-stack hover), not per row of a
/// virtualized list, so a filesystem scan per call is cheap enough — no
/// schema/cache table added here. Revisit if a future caller needs this at
/// list-row frequency.
#[tauri::command]
pub async fn get_extended_artwork_for_song(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<ExtendedArtworkResponse, String> {
    let (path, album) = {
        let conn = state.db.pool.get().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT path, album FROM songs WHERE id = ?1",
            params![song_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?
    };

    let Some(path) = path else {
        return Ok(ExtendedArtworkResponse::default());
    };

    let set = scan_extended_artwork(Path::new(&path), album.as_deref()).sorted();
    Ok(build_extended_artwork_response(set))
}

/// Artist-level slice of the same hierarchical scan (portrait, band logo,
/// fanart/backdrop banner only — album-level categories are excluded here
/// since they're not meaningful without a specific album in context).
/// Resolved via any one song credited to the artist (as `album_artist` or
/// `artist`), same rationale as `get_extended_artwork_for_song`.
#[tauri::command]
pub async fn get_extended_artwork_for_artist(
    state: State<'_, AppState>,
    artist: String,
) -> Result<ExtendedArtworkResponse, String> {
    let path = {
        let conn = state.db.pool.get().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT path FROM songs
             WHERE (album_artist = ?1 COLLATE NOCASE OR artist = ?1 COLLATE NOCASE)
               AND path IS NOT NULL
             LIMIT 1",
            params![artist],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .flatten()
    };

    let Some(path) = path else {
        return Ok(ExtendedArtworkResponse::default());
    };

    let set = scan_extended_artwork(Path::new(&path), None);
    let artist_only = ExtendedArtworkSet {
        entries: set
            .entries
            .into_iter()
            .filter(|e| {
                matches!(
                    e.category,
                    ArtworkCategory::ArtistPortrait
                        | ArtworkCategory::BandLogo
                        | ArtworkCategory::FanartBanner
                )
            })
            .collect(),
    }
    .sorted();

    Ok(build_extended_artwork_response(artist_only))
}

/// Open a discovered artwork path with the OS's default image viewer, for
/// the cover-stack's "Open Images" hover action (#760). `path` is a real
/// filesystem path (not a `luminous-art://` URI) — the frontend resolves it
/// from an `ExtendedArtworkItem.uri`'s `local/` form before calling this.
#[tauri::command]
pub async fn open_artwork_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::covermanager::ArtworkEntry;
    use std::path::PathBuf;

    #[test]
    fn test_build_extended_artwork_response_empty_set_has_no_uris() {
        let response = build_extended_artwork_response(ExtendedArtworkSet::default());
        assert_eq!(response.count, 0);
        assert_eq!(response.primary_uri, None);
        assert_eq!(response.artist_portrait_uri, None);
        assert_eq!(response.band_logo_uri, None);
        assert_eq!(response.fanart_uri, None);
        assert!(response.items.is_empty());
    }

    #[test]
    fn test_build_extended_artwork_response_primary_uri_is_top_ranked_entry() {
        let set = ExtendedArtworkSet {
            entries: vec![
                ArtworkEntry {
                    category: ArtworkCategory::PrimaryCover,
                    path: PathBuf::from("/music/Artist/Album/cover.jpg"),
                },
                ArtworkEntry {
                    category: ArtworkCategory::BackCover,
                    path: PathBuf::from("/music/Artist/Album/back.jpg"),
                },
            ],
        };

        let response = build_extended_artwork_response(set);

        assert_eq!(response.count, 2);
        assert_eq!(
            response.primary_uri.as_deref(),
            Some("luminous-art://local//music/Artist/Album/cover.jpg")
        );
        assert_eq!(response.items[0].category, "primary_cover");
        assert_eq!(response.items[1].category, "back_cover");
    }

    #[test]
    fn test_build_extended_artwork_response_picks_first_match_per_artist_field() {
        // Two fanart-category files (e.g. `fanart.jpg` and a subfolder find
        // that also mapped to FanartBanner) — the response should surface
        // only the first one as `fanart_uri`, not overwrite it with the
        // second.
        let set = ExtendedArtworkSet {
            entries: vec![
                ArtworkEntry {
                    category: ArtworkCategory::ArtistPortrait,
                    path: PathBuf::from("/music/Artist/artist.jpg"),
                },
                ArtworkEntry {
                    category: ArtworkCategory::BandLogo,
                    path: PathBuf::from("/music/Artist/logo.png"),
                },
                ArtworkEntry {
                    category: ArtworkCategory::FanartBanner,
                    path: PathBuf::from("/music/Artist/fanart.jpg"),
                },
                ArtworkEntry {
                    category: ArtworkCategory::FanartBanner,
                    path: PathBuf::from("/music/Artist/backdrop.jpg"),
                },
            ],
        };

        let response = build_extended_artwork_response(set);

        assert_eq!(
            response.artist_portrait_uri.as_deref(),
            Some("luminous-art://local//music/Artist/artist.jpg")
        );
        assert_eq!(
            response.band_logo_uri.as_deref(),
            Some("luminous-art://local//music/Artist/logo.png")
        );
        assert_eq!(
            response.fanart_uri.as_deref(),
            Some("luminous-art://local//music/Artist/fanart.jpg")
        );
        assert_eq!(response.count, 4);
    }
}
