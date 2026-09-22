use crate::{
    biomanager,
    collection::CollectionScanner,
    context::ContextManager,
    models::{
        AlbumLink, AlbumProfile, ArtistProfile, HomeItem, LibraryStats, MusicDirectory,
        PruneResult, Song, Tag, TopAlbumItem,
    },
    AppState,
};
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn add_directory(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<MusicDirectory, String> {
    let res = crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.add_directory(&path)
    })
    .await
    .map_err(|e| e.to_string())?;
    crate::collection::start_watcher(app, &state);
    Ok(res)
}

#[tauri::command]
pub async fn remove_directory(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.remove_directory(&path)
    })
    .await
    .map_err(|e| e.to_string())?;
    crate::collection::start_watcher(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn get_directories(state: State<'_, AppState>) -> Result<Vec<MusicDirectory>, String> {
    crate::collection::with_collection_scanner(state.db.clone(), |scanner| {
        scanner.get_directories()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_directory_metadata(
    id: i64,
    nickname: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.update_directory_metadata(id, nickname, icon, color)
    })
    .await
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

/// Force re-reads embedded tags from disk for exactly these songs and
/// reconciles the DB to match, bypassing the mtime-skip a normal (non-force)
/// scan uses. Unlike the whole-library `scan_directories`, this is scoped to
/// a specific set of tracks, so a view like the album detail page can offer
/// a fast "resync from disk" action instead of only reloading whatever the
/// DB already has (which a plain library snapshot reload can't distinguish
/// from a genuine on-disk change — see #956). WebDAV songs (no local file)
/// and CUE-derived songs (tags live in the .cue sheet, not embedded — #78)
/// are skipped, same as the tag editor's other bulk-write paths.
#[tauri::command]
pub async fn rescan_songs(
    app: AppHandle,
    song_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let paths: Vec<std::path::PathBuf> = crate::db::run_blocking(&state.db, move |conn| {
        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );
        Ok(song_ids
            .iter()
            .filter_map(|id| {
                conn.query_row(&sql, [id], crate::collection::row_to_song)
                    .ok()
            })
            .filter(|song| {
                song.source != crate::models::SongSource::WebDav && song.cue_path.is_none()
            })
            .filter_map(|song| song.path)
            .map(std::path::PathBuf::from)
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?;

    let scanner = CollectionScanner::new(state.db.clone());
    scanner
        .rescan_paths(&app, paths)
        .await
        .map_err(|e| e.to_string())?;

    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn prune_missing_songs(state: State<'_, AppState>) -> Result<PruneResult, String> {
    crate::collection::with_collection_scanner(state.db.clone(), |scanner| {
        scanner.prune_missing_songs()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_library_stats(state: State<'_, AppState>) -> Result<LibraryStats, String> {
    crate::collection::with_collection_scanner(state.db.clone(), |scanner| {
        scanner.get_library_stats()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Runs the backend-consistency steps a completed scan requires: persist
/// `last_scan_time`, resync the live playback queue with the DB (a scan can
/// repoint a moved file's path or drop a missing one out from under an
/// already-queued track), and resync auto-playlists (a scan can add new
/// genres/decades or shift which songs qualify). Callers no longer need to
/// remember to fire all three separately.
#[tauri::command]
pub async fn finish_scan(last_scan_time: String, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.clone();
    if let Err(e) = crate::db::run_blocking(&db, move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_scan_time', ?1)",
            [&last_scan_time],
        )?;
        Ok(())
    })
    .await
    {
        log::error!("Failed to persist last_scan_time: {e}");
    }

    if let Err(e) =
        crate::player::with_player(&state.player, |p| p.resync_queue_with_db()).await
    {
        log::error!("Failed to resync playback queue after scan: {e}");
    }

    crate::playlist::with_playlists(&state.playlists, |pm| pm.sync_all_auto_playlists())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_songs(
    query: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let limit = limit.unwrap_or(500);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.search_songs(&query, limit)
    })
    .await
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
    crate::collection::with_collection_scanner(state.db.clone(), |scanner| {
        Ok(LibrarySnapshot {
            songs: scanner.get_songs(-1, 0)?,
            albums: scanner.get_albums()?,
            artists: scanner.get_artists()?,
        })
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_album(
    album: String,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_songs_by_album(&album)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_artist(
    artist: String,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_songs_by_artist(&artist)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_compilations_by_artist(
    artist: String,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_compilations_by_artist(&artist)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_favourite_songs(state: State<'_, AppState>) -> Result<Vec<Song>, String> {
    crate::collection::with_collection_scanner(state.db.clone(), |scanner| {
        scanner.get_favourite_songs()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_added_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let limit = limit.unwrap_or(50);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_recently_added_songs(limit)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_most_played_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let limit = limit.unwrap_or(50);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_most_played_songs(limit)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_top_artists(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(10);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_top_artists(limit)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_played(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let limit = limit.unwrap_or(10);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_recently_played(limit)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_played_songs(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let limit = limit.unwrap_or(100);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_recently_played_songs(limit)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_play_history(state: State<'_, AppState>) -> Result<(), String> {
    crate::collection::with_collection_scanner(state.db.clone(), |scanner| {
        scanner.clear_play_history()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recently_added(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let limit = limit.unwrap_or(10);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_recently_added(limit)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_featured_albums(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeItem>, String> {
    let limit = limit.unwrap_or(10);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_featured_albums(limit)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_top_albums(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<TopAlbumItem>, String> {
    let limit = limit.unwrap_or(10);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_top_albums(limit)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Extracts just the prose portion of a sidecar file's content: everything
/// before the first `## `-prefixed Markdown heading (`## Tags`, `## Links`)
/// that `build_artist_md_content`/`build_album_md_content` append. Without
/// this, reading back a file we wrote ourselves would fold the rendered
/// Tags/Links sections into the plain-text bio/description field, corrupting
/// it the moment only tags or links (no bio) were saved — the whole file
/// content (e.g. `"## Tags\n- canadian"`) would get adopted as the bio.
fn extract_bio_prose(content: &str) -> Option<String> {
    let mut prose_sections: Vec<&str> = Vec::new();
    for section in content.split("\n\n") {
        if section.trim_start().starts_with("## ") {
            break;
        }
        prose_sections.push(section);
    }
    let joined = prose_sections.join("\n\n");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Reads an artist/album's bio/description sidecar file, if the
/// corresponding song folder can be resolved and the file exists — returning
/// only its prose portion (see `extract_bio_prose`).
fn read_bio_sidecar(
    song_path: Option<String>,
    resolve_dir: impl Fn(&Path) -> Option<std::path::PathBuf>,
    filename: &str,
) -> Option<String> {
    let dir = resolve_dir(Path::new(&song_path?))?;
    let content = biomanager::read_bio(&dir, filename)?;
    extract_bio_prose(&content)
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
/// `links` fields). No "## Tags" section here, unlike the artist version —
/// an album has no curated tag list of its own to mirror (#962 removed
/// `album_profiles.tags`; the embedded `songs.genre` tag is the only tag
/// list an album has, and it's already on disk in each track's own file).
fn build_album_md_content(profile: &AlbumProfile) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();
    if let Some(description) = profile.description.as_deref() {
        let trimmed = description.trim();
        if !trimmed.is_empty() {
            sections.push(trimmed.to_string());
        }
    }
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
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        let mut profile = scanner.get_artist_profile(&artist)?;

        // Self-heal a bio value polluted by an earlier bug where the whole
        // sidecar file — including its generated "## Tags"/"## Links" sections —
        // was adopted as the bio text instead of just its prose.
        let cleaned_bio = profile.bio.as_deref().and_then(extract_bio_prose);
        if cleaned_bio != profile.bio {
            profile.bio = cleaned_bio;
            if let Ok(saved) = scanner.set_artist_profile(&profile) {
                profile = saved;
            }
        }

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
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_artist_profile(
    profile: ArtistProfile,
    state: State<'_, AppState>,
) -> Result<ArtistProfile, String> {
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        let saved = scanner.set_artist_profile(&profile)?;

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
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_all_artist_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<ArtistProfile>, String> {
    crate::collection::with_collection_scanner(state.db.clone(), |scanner| {
        scanner.get_all_artist_profiles()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Retrieve an album's bio profile, adopting one from an `album.md` sidecar
/// file next to the album's songs (same convention as `cover.jpg`) if the DB
/// doesn't have one saved yet.
#[tauri::command]
pub async fn get_album_profile(
    album: String,
    state: State<'_, AppState>,
) -> Result<AlbumProfile, String> {
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        let mut profile = scanner.get_album_profile(&album)?;

        // Self-heal a description value polluted by an earlier bug — see the
        // matching comment in `get_artist_profile`.
        let cleaned_description = profile.description.as_deref().and_then(extract_bio_prose);
        if cleaned_description != profile.description {
            profile.description = cleaned_description;
            if let Ok(saved) = scanner.set_album_profile(&profile) {
                profile = saved;
            }
        }

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
    })
    .await
    .map_err(|e| e.to_string())
}

/// Saves an album profile and mirrors it to the `album.md` sidecar, shared
/// by `set_album_profile` (a user's manual edit) and `retrieve_album_details`
/// (a MusicBrainz-sourced link merge) so both go through the same
/// persistence path.
fn save_album_profile_with_sidecar(
    scanner: &CollectionScanner,
    profile: &AlbumProfile,
) -> anyhow::Result<AlbumProfile> {
    let saved = scanner.set_album_profile(profile)?;

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
pub async fn set_album_profile(
    profile: AlbumProfile,
    state: State<'_, AppState>,
) -> Result<AlbumProfile, String> {
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        save_album_profile_with_sidecar(scanner, &profile)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_all_album_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<AlbumProfile>, String> {
    crate::collection::with_collection_scanner(state.db.clone(), |scanner| {
        scanner.get_all_album_profiles()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Maps a MusicBrainz release-group `url-rels` relation type to the album
/// link platform id we render it under. Only the relation types the album
/// details overflow menu's "Retrieve Album Details" action is scoped to
/// (Discogs, AllMusic, Wikidata, lyrics sites, other databases) are
/// recognized — MusicBrainz returns many more relation types (streaming,
/// purchase links, etc.) that are out of scope here and are simply dropped.
fn platform_for_release_group_rel_type(rel_type: &str) -> Option<&'static str> {
    match rel_type {
        "discogs" => Some("discogs"),
        "allmusic" => Some("allmusic"),
        "wikidata" => Some("wikidata"),
        "lyrics" => Some("lyrics"),
        "other databases" => Some("other_databases"),
        _ => None,
    }
}

/// Appends `fetched` links onto `existing`, skipping any that are already
/// present (same platform and URL) so re-running "Retrieve Album Details"
/// is idempotent rather than piling up duplicates. Multiple links of the
/// same platform (e.g. several lyrics sites) are intentionally allowed to
/// coexist.
fn merge_album_links(mut existing: Vec<AlbumLink>, fetched: Vec<AlbumLink>) -> (Vec<AlbumLink>, usize) {
    let mut added = 0;
    for link in fetched {
        let already_present = existing
            .iter()
            .any(|l| l.platform == link.platform && l.handle_or_url == link.handle_or_url);
        if !already_present {
            existing.push(link);
            added += 1;
        }
    }
    (existing, added)
}

#[derive(Serialize, Clone, Debug)]
pub struct AlbumDetailsRetrievalResult {
    pub profile: AlbumProfile,
    pub added_count: usize,
}

/// The album detail overflow menu's "Retrieve Album Details" action: looks
/// up the album's representative MusicBrainz release-group MBID, fetches
/// its `url-rels` relations, and merges the ones we recognize (Discogs,
/// AllMusic, Wikidata, lyrics, other databases) into the album's curated
/// link list — the same `album_profiles.links` the manual editor and the
/// derived ListenBrainz link already render (#950).
#[tauri::command]
pub async fn retrieve_album_details(
    album: String,
    state: State<'_, AppState>,
) -> Result<AlbumDetailsRetrievalResult, String> {
    let album_for_lookup = album.clone();
    let (release_group_id, current_profile) =
        crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
            let release_group_id =
                scanner.get_representative_release_group_id_for_album(&album_for_lookup)?;
            let profile = scanner.get_album_profile(&album_for_lookup)?;
            Ok((release_group_id, profile))
        })
        .await
        .map_err(|e| e.to_string())?;

    let Some(release_group_id) = release_group_id else {
        return Err(
            "No MusicBrainz release group ID found for this album — tag it with Picard first."
                .to_string(),
        );
    };

    let relations = ContextManager::new()
        .fetch_musicbrainz_release_group_relations(&release_group_id)
        .await
        .map_err(|e| e.to_string())?;

    let fetched_links: Vec<AlbumLink> = relations
        .into_iter()
        .filter_map(|(rel_type, url)| {
            platform_for_release_group_rel_type(&rel_type).map(|platform| AlbumLink {
                platform: platform.to_string(),
                handle_or_url: url,
            })
        })
        .collect();

    let mut updated_profile = current_profile;
    updated_profile.album_key = album;
    let existing_links = std::mem::take(&mut updated_profile.links);
    let (merged_links, added_count) = merge_album_links(existing_links, fetched_links);
    updated_profile.links = merged_links;

    let profile = crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        save_album_profile_with_sidecar(scanner, &updated_profile)
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(AlbumDetailsRetrievalResult {
        profile,
        added_count,
    })
}

/// Every artist tag in the library with its song count, for the Genres
/// page's browsable-only "Artist Tags" section (see `get_artist_tag_counts`
/// doc comment for why artist tags don't get a full mergeable/colorable
/// hierarchy entry like genre does).
#[tauri::command]
pub async fn get_artist_tags_overview(state: State<'_, AppState>) -> Result<Vec<Tag>, String> {
    crate::collection::with_collection_scanner(state.db.clone(), |scanner| {
        scanner.get_artist_tag_counts()
    })
    .await
    .map_err(|e| e.to_string())
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
    let song_ids_for_write = song_ids.clone();
    crate::db::run_blocking(&state.db, move |conn| {
        let placeholders = song_ids_for_write
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("UPDATE songs SET not_included = ?1 WHERE id IN ({placeholders})");
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&not_included];
        params.extend(song_ids_for_write.iter().map(|id| id as &dyn rusqlite::ToSql));
        conn.execute(&sql, params.as_slice())?;
        Ok(())
    })
    .await
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
    let limit = limit.unwrap_or(-1);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_songs_missing_musicbrainz_id(limit, crate::models::QueuePopulationMode::All)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_missing_metadata(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let limit = limit.unwrap_or(-1);
    crate::collection::with_collection_scanner(state.db.clone(), move |scanner| {
        scanner.get_songs_missing_core_tags(limit, crate::models::QueuePopulationMode::All)
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AlbumLink, ArtistSocialLink};

    #[test]
    fn test_platform_for_release_group_rel_type_maps_recognized_types() {
        assert_eq!(platform_for_release_group_rel_type("discogs"), Some("discogs"));
        assert_eq!(platform_for_release_group_rel_type("allmusic"), Some("allmusic"));
        assert_eq!(platform_for_release_group_rel_type("wikidata"), Some("wikidata"));
        assert_eq!(platform_for_release_group_rel_type("lyrics"), Some("lyrics"));
        assert_eq!(
            platform_for_release_group_rel_type("other databases"),
            Some("other_databases")
        );
        assert_eq!(platform_for_release_group_rel_type("streaming"), None);
        assert_eq!(platform_for_release_group_rel_type("free streaming"), None);
    }

    #[test]
    fn test_merge_album_links_appends_new_and_skips_exact_duplicates() {
        let existing = vec![AlbumLink {
            platform: "discogs".to_string(),
            handle_or_url: "https://discogs.com/master/1".to_string(),
        }];
        let fetched = vec![
            // Exact duplicate of an existing link — should not be re-added.
            AlbumLink {
                platform: "discogs".to_string(),
                handle_or_url: "https://discogs.com/master/1".to_string(),
            },
            // New platform.
            AlbumLink {
                platform: "wikidata".to_string(),
                handle_or_url: "https://www.wikidata.org/wiki/Q1".to_string(),
            },
            // Second link of a platform that can have several (lyrics sites).
            AlbumLink {
                platform: "lyrics".to_string(),
                handle_or_url: "https://genius.com/albums/x".to_string(),
            },
        ];
        let (merged, added) = merge_album_links(existing, fetched);
        assert_eq!(added, 2);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[1].platform, "wikidata");
        assert_eq!(merged[2].platform, "lyrics");
    }

    #[test]
    fn test_extract_bio_prose_none_for_tags_only_content() {
        // Regression: saving tags with no bio wrote "## Tags\n- canadian" to
        // the sidecar file; reading it back must not treat that as the bio.
        assert_eq!(extract_bio_prose("## Tags\n- canadian"), None);
    }

    #[test]
    fn test_extract_bio_prose_strips_trailing_generated_sections() {
        assert_eq!(
            extract_bio_prose("A real bio.\n\n## Tags\n- canadian\n\n## Links\n- [Website](https://example.com)"),
            Some("A real bio.".to_string())
        );
    }

    #[test]
    fn test_extract_bio_prose_returns_plain_prose_unchanged() {
        assert_eq!(
            extract_bio_prose("Just a bio, no sections."),
            Some("Just a bio, no sections.".to_string())
        );
    }

    #[test]
    fn test_extract_bio_prose_multi_paragraph_prose_is_preserved() {
        assert_eq!(
            extract_bio_prose("Paragraph one.\n\nParagraph two.\n\n## Tags\n- rock"),
            Some("Paragraph one.\n\nParagraph two.".to_string())
        );
    }

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
    fn test_build_album_md_content_appends_website_and_links_but_no_tags_section() {
        // #962: albums have no curated tag list of their own to mirror
        // anymore -- only the embedded genre tag, already on disk per-file.
        let profile = AlbumProfile {
            album_key: "Come On Over".to_string(),
            artist_key: Some("Shania Twain".to_string()),
            description: Some("Iconic 1997 studio album.".to_string()),
            website: Some("https://shaniatwain.com/music/come-on-over".to_string()),
            links: vec![AlbumLink {
                platform: "discogs".to_string(),
                handle_or_url: "https://www.discogs.com/master/132556".to_string(),
            }],
        };

        let content = build_album_md_content(&profile).unwrap();
        assert_eq!(
            content,
            "Iconic 1997 studio album.\n\n## Links\n\
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
