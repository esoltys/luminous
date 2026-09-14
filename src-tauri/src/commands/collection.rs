use crate::{
    biomanager,
    collection::CollectionScanner,
    models::{
        AlbumProfile, ArtistProfile, HomeItem, LibraryStats, MusicDirectory, PruneResult, Song,
        TopAlbumItem,
    },
    AppState,
};
use std::path::Path;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn add_directory(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<MusicDirectory, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    let res = scanner.add_directory(&path).map_err(|e| e.to_string())?;
    crate::collection::start_watcher(app, &state);
    Ok(res)
}

#[tauri::command]
pub async fn remove_directory(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.remove_directory(&path).map_err(|e| e.to_string())?;
    crate::collection::start_watcher(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn get_directories(state: State<'_, AppState>) -> Result<Vec<MusicDirectory>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_directories().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_directory_metadata(
    id: i64,
    nickname: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .update_directory_metadata(id, nickname, icon, color)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_directories(
    app: AppHandle,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .scan_all(app, force.unwrap_or(false), false)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn prune_missing_songs(state: State<'_, AppState>) -> Result<PruneResult, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.prune_missing_songs().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_library_stats(state: State<'_, AppState>) -> Result<LibraryStats, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_library_stats().map_err(|e| e.to_string())
}

/// Runs the backend-consistency steps a completed scan requires: persist
/// `last_scan_time`, resync the live playback queue with the DB (a scan can
/// repoint a moved file's path or drop a missing one out from under an
/// already-queued track), and resync auto-playlists (a scan can add new
/// genres/decades or shift which songs qualify). Callers no longer need to
/// remember to fire all three separately.
#[tauri::command]
pub async fn finish_scan(last_scan_time: String, state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(conn) = state.db.pool.get() {
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_scan_time', ?1)",
            [&last_scan_time],
        ) {
            log::error!("Failed to persist last_scan_time: {e}");
        }
    }

    if let Err(e) = state.player.lock().await.resync_queue_with_db() {
        log::error!("Failed to resync playback queue after scan: {e}");
    }

    state
        .playlists
        .lock()
        .await
        .sync_all_auto_playlists()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_songs(
    query: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .search_songs(&query, limit.unwrap_or(500))
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct LibrarySnapshot {
    pub songs: Vec<Song>,
    pub albums: Vec<serde_json::Value>,
    pub artists: Vec<serde_json::Value>,
}

/// The Collection view's "give me everything" read — songs, albums, and
/// artists always get refreshed together (initial load, post-scan, on
/// library-changed events), so callers no longer have to remember to fire
/// all three round trips themselves.
#[tauri::command]
pub async fn get_library_snapshot(state: State<'_, AppState>) -> Result<LibrarySnapshot, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    Ok(LibrarySnapshot {
        songs: scanner.get_songs(-1, 0).map_err(|e| e.to_string())?,
        albums: scanner.get_albums().map_err(|e| e.to_string())?,
        artists: scanner.get_artists().map_err(|e| e.to_string())?,
    })
}

#[tauri::command]
pub async fn get_songs_by_album(
    album: String,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_album(&album)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_artist(
    artist: String,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_by_artist(&artist)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_compilations_by_artist(
    artist: String,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_compilations_by_artist(&artist)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_favourite_songs(state: State<'_, AppState>) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_favourite_songs().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_added_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_added_songs(limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_most_played_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_most_played_songs(limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_top_artists(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_top_artists(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_played(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_played(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_played_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_played_songs(limit.unwrap_or(100))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_play_history(state: State<'_, AppState>) -> Result<(), String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.clear_play_history().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_added(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_recently_added(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_featured_albums(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_featured_albums(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_top_albums(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<TopAlbumItem>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_top_albums(limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

/// Reads an artist/album's bio/description sidecar file, if the
/// corresponding song folder can be resolved and the file exists.
fn read_bio_sidecar(
    song_path: Option<String>,
    resolve_dir: impl Fn(&Path) -> Option<std::path::PathBuf>,
    filename: &str,
) -> Option<String> {
    let dir = resolve_dir(Path::new(&song_path?))?;
    biomanager::read_bio(&dir, filename)
}

/// Writes `content` out to its `artist.md`/`album.md` sidecar file (or
/// removes the file when there's nothing to write) so the folder stays
/// portable across Luminous instances (see `biomanager.rs`). Failures are
/// logged, not propagated — the DB write is what the caller actually
/// depends on.
fn write_bio_sidecar(
    song_path: Option<String>,
    resolve_dir: impl Fn(&Path) -> Option<std::path::PathBuf>,
    filename: &str,
    content: Option<&str>,
    key: &str,
) {
    let Some(dir) = song_path.as_deref().map(Path::new).and_then(resolve_dir) else {
        return;
    };
    let result = match content {
        Some(text) if !text.trim().is_empty() => biomanager::write_bio(&dir, filename, text),
        _ => biomanager::remove_bio(&dir, filename),
    };
    if let Err(e) = result {
        log::warn!("Failed to sync {} for '{}': {}", filename, key, e);
    }
}

/// Renders a `## Links` section as a Markdown bullet list: `[label](url)`
/// for anything URL-shaped, a plain `label: value` bullet otherwise. Shared
/// by the artist (website + social links) and album (website + links)
/// sidecar writers so both mirror the same visual convention.
fn format_links_section(website: Option<&str>, links: &[(&str, &str)]) -> Option<String> {
    let mut bullets: Vec<String> = Vec::new();
    if let Some(site) = website {
        let site = site.trim();
        if !site.is_empty() {
            bullets.push(format!("- [Website]({site})"));
        }
    }
    for (platform, handle_or_url) in links {
        let handle = handle_or_url.trim();
        if handle.is_empty() {
            continue;
        }
        let mut chars = platform.chars();
        let label = match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => continue,
        };
        if handle.starts_with("http://") || handle.starts_with("https://") {
            bullets.push(format!("- [{label}]({handle})"));
        } else {
            bullets.push(format!("- {label}: {handle}"));
        }
    }
    if bullets.is_empty() {
        None
    } else {
        Some(format!("## Links\n{}", bullets.join("\n")))
    }
}

/// Renders a `## Tags` section as a Markdown bullet list, or `None` if
/// there are no tags.
fn format_tags_section(tags: &[String]) -> Option<String> {
    let bullets: Vec<String> = tags
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| format!("- {t}"))
        .collect();
    if bullets.is_empty() {
        None
    } else {
        Some(format!("## Tags\n{}", bullets.join("\n")))
    }
}

/// Builds the full text to write to `artist.md`: the bio, followed by tags
/// and by the website/social links as Markdown lists. Keeps the sidecar file
/// a complete, human-readable mirror of the profile rather than just the bio
/// paragraph. Returns `None` when there's nothing to write.
fn build_artist_md_content(profile: &ArtistProfile) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();
    if let Some(bio) = profile.bio.as_deref() {
        let trimmed = bio.trim();
        if !trimmed.is_empty() {
            sections.push(trimmed.to_string());
        }
    }
    sections.extend(format_tags_section(&profile.tags));
    let links: Vec<(&str, &str)> = profile
        .social_links
        .iter()
        .map(|l| (l.platform.as_str(), l.handle_or_url.as_str()))
        .collect();
    sections.extend(format_links_section(profile.website.as_deref(), &links));

    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
}

/// Same as `build_artist_md_content`, for `album.md` (#950's `description`/
/// `links` fields).
fn build_album_md_content(profile: &AlbumProfile) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();
    if let Some(description) = profile.description.as_deref() {
        let trimmed = description.trim();
        if !trimmed.is_empty() {
            sections.push(trimmed.to_string());
        }
    }
    sections.extend(format_tags_section(&profile.tags));
    let links: Vec<(&str, &str)> = profile
        .links
        .iter()
        .map(|l| (l.platform.as_str(), l.handle_or_url.as_str()))
        .collect();
    sections.extend(format_links_section(profile.website.as_deref(), &links));

    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
}

/// Retrieve an artist's customizable profile (#473). If the DB has no bio
/// saved yet, this adopts one from an `artist.md` sidecar file sitting next
/// to the artist's music, if present, persisting it to the DB so subsequent
/// reads don't need to touch the filesystem.
#[tauri::command]
pub async fn get_artist_profile(
    artist: String,
    state: State<'_, AppState>,
) -> Result<ArtistProfile, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    let mut profile = scanner
        .get_artist_profile(&artist)
        .map_err(|e| e.to_string())?;

    if profile.bio.is_none() {
        let song_path = scanner
            .get_representative_song_path_for_artist(&artist)
            .unwrap_or(None);
        if let Some(bio) = read_bio_sidecar(
            song_path,
            biomanager::artist_dir,
            biomanager::ARTIST_BIO_FILENAME,
        ) {
            profile.bio = Some(bio);
            if let Ok(saved) = scanner.set_artist_profile(&profile) {
                profile = saved;
            }
        }
    }

    Ok(profile)
}

#[tauri::command]
pub async fn set_artist_profile(
    profile: ArtistProfile,
    state: State<'_, AppState>,
) -> Result<ArtistProfile, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    let saved = scanner
        .set_artist_profile(&profile)
        .map_err(|e| e.to_string())?;

    let song_path = scanner
        .get_representative_song_path_for_artist(&saved.artist_key)
        .unwrap_or(None);
    let content = build_artist_md_content(&saved);
    write_bio_sidecar(
        song_path,
        biomanager::artist_dir,
        biomanager::ARTIST_BIO_FILENAME,
        content.as_deref(),
        &saved.artist_key,
    );

    Ok(saved)
}

#[tauri::command]
pub async fn get_all_artist_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<ArtistProfile>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_all_artist_profiles().map_err(|e| e.to_string())
}

/// Retrieve an album's bio profile, adopting one from an `album.md` sidecar
/// file next to the album's songs (same convention as `cover.jpg`) if the DB
/// doesn't have one saved yet.
#[tauri::command]
pub async fn get_album_profile(
    album: String,
    state: State<'_, AppState>,
) -> Result<AlbumProfile, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    let mut profile = scanner
        .get_album_profile(&album)
        .map_err(|e| e.to_string())?;

    if profile.description.is_none() {
        let song_path = scanner
            .get_representative_song_path_for_album(&album)
            .unwrap_or(None);
        if let Some(description) = read_bio_sidecar(
            song_path,
            biomanager::album_dir,
            biomanager::ALBUM_BIO_FILENAME,
        ) {
            profile.description = Some(description);
            if let Ok(saved) = scanner.set_album_profile(&profile) {
                profile = saved;
            }
        }
    }

    Ok(profile)
}

#[tauri::command]
pub async fn set_album_profile(
    profile: AlbumProfile,
    state: State<'_, AppState>,
) -> Result<AlbumProfile, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    let saved = scanner
        .set_album_profile(&profile)
        .map_err(|e| e.to_string())?;

    let song_path = scanner
        .get_representative_song_path_for_album(&saved.album_key)
        .unwrap_or(None);
    let content = build_album_md_content(&saved);
    write_bio_sidecar(
        song_path,
        biomanager::album_dir,
        biomanager::ALBUM_BIO_FILENAME,
        content.as_deref(),
        &saved.album_key,
    );

    Ok(saved)
}

#[tauri::command]
pub async fn get_all_album_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<AlbumProfile>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner.get_all_album_profiles().map_err(|e| e.to_string())
}

/// Marks (or unmarks) one or more songs "Not included" (#104): excluded from
/// auto/smart-playlist generation and Auto-Play refill, but still fully
/// visible and playable in Album/Artist views. Fires a `song-stats-changed`
/// event per song (the same event ratings/playcounts use to patch cached
/// `Song` rows in place, e.g. `AlbumDetailView`'s badge/context-menu label)
/// so open views update immediately, plus `library-changed` so dynamic
/// playlists reconcile their membership.
#[tauri::command]
pub async fn set_songs_not_included(
    app: AppHandle,
    song_ids: Vec<i64>,
    not_included: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if song_ids.is_empty() {
        return Ok(());
    }
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let placeholders = song_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("UPDATE songs SET not_included = ?1 WHERE id IN ({placeholders})");
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&not_included];
    params.extend(song_ids.iter().map(|id| id as &dyn rusqlite::ToSql));
    conn.execute(&sql, params.as_slice())
        .map_err(|e| e.to_string())?;

    for song_id in &song_ids {
        let _ = app.emit(
            "song-stats-changed",
            serde_json::json!({ "song_id": song_id, "not_included": not_included }),
        );
    }
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn get_songs_missing_musicbrainz_id(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_missing_musicbrainz_id(limit.unwrap_or(-1), crate::models::QueuePopulationMode::All)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_missing_metadata(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .get_songs_missing_core_tags(limit.unwrap_or(-1), crate::models::QueuePopulationMode::All)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AlbumLink, ArtistSocialLink};

    #[test]
    fn test_build_artist_md_content_none_when_profile_is_empty() {
        let profile = ArtistProfile {
            artist_key: "Empty Artist".to_string(),
            ..Default::default()
        };
        assert_eq!(build_artist_md_content(&profile), None);
    }

    #[test]
    fn test_build_artist_md_content_bio_only_has_no_links_or_tags_section() {
        let profile = ArtistProfile {
            artist_key: "Solo Artist".to_string(),
            bio: Some("A short bio.".to_string()),
            ..Default::default()
        };
        assert_eq!(
            build_artist_md_content(&profile),
            Some("A short bio.".to_string())
        );
    }

    #[test]
    fn test_build_artist_md_content_appends_tags_then_website_and_social_links() {
        let profile = ArtistProfile {
            artist_key: "Shania Twain".to_string(),
            bio: Some("Canadian singer-songwriter.".to_string()),
            website: Some("https://www.shaniatwain.com".to_string()),
            tags: vec!["pop".to_string(), "country".to_string()],
            social_links: vec![
                ArtistSocialLink {
                    platform: "instagram".to_string(),
                    handle_or_url: "@shaniatwain".to_string(),
                },
                ArtistSocialLink {
                    platform: "youtube".to_string(),
                    handle_or_url: "https://youtube.com/@ShaniaTwain".to_string(),
                },
            ],
        };

        let content = build_artist_md_content(&profile).unwrap();
        assert_eq!(
            content,
            "Canadian singer-songwriter.\n\n## Tags\n- pop\n- country\n\n## Links\n\
             - [Website](https://www.shaniatwain.com)\n\
             - Instagram: @shaniatwain\n\
             - [Youtube](https://youtube.com/@ShaniaTwain)"
        );
    }

    #[test]
    fn test_build_album_md_content_appends_tags_then_website_and_links() {
        let profile = AlbumProfile {
            album_key: "Come On Over".to_string(),
            artist_key: Some("Shania Twain".to_string()),
            description: Some("Iconic 1997 studio album.".to_string()),
            website: Some("https://shaniatwain.com/music/come-on-over".to_string()),
            tags: vec!["country pop".to_string()],
            links: vec![AlbumLink {
                platform: "discogs".to_string(),
                handle_or_url: "https://www.discogs.com/master/132556".to_string(),
            }],
        };

        let content = build_album_md_content(&profile).unwrap();
        assert_eq!(
            content,
            "Iconic 1997 studio album.\n\n## Tags\n- country pop\n\n## Links\n\
             - [Website](https://shaniatwain.com/music/come-on-over)\n\
             - [Discogs](https://www.discogs.com/master/132556)"
        );
    }

    #[test]
    fn test_build_album_md_content_none_when_profile_is_empty() {
        let profile = AlbumProfile {
            album_key: "Empty Album".to_string(),
            ..Default::default()
        };
        assert_eq!(build_album_md_content(&profile), None);
    }
}
