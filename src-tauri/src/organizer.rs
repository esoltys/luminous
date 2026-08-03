//! Tag-based file organizer module.
//!
//! Provides template parsing, dry-run path computation, conflict detection,
//! and batch file relocation with atomic SQLite path updates.

use crate::covermanager::CoverManager;
use crate::db::Database;
use crate::models::Song;
use anyhow::{anyhow, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeOptions {
    pub destination_dir: Option<String>,
    pub replace_spaces_with_underscores: bool,
    pub ascii_only: bool,
    pub clean_empty_dirs: bool,
    #[serde(default)]
    pub move_extra_files: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrganizePreviewStatus {
    Ok,
    Unchanged,
    Collision,
    MissingTag,
    CrossDevice,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizePreviewItem {
    pub song_id: i64,
    pub from_path: String,
    pub to_path: String,
    pub status: OrganizePreviewStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeApplyItem {
    pub song_id: i64,
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeResult {
    pub moved_count: usize,
    pub skipped_count: usize,
    pub errors: Vec<String>,
}

/// Sanitize a path component by removing or replacing OS-illegal characters.
pub fn sanitize_component(segment: &str, replace_spaces: bool, ascii_only: bool) -> String {
    let mut result = String::with_capacity(segment.len());
    for ch in segment.chars() {
        match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => {
                result.push('_');
            }
            ' ' if replace_spaces => {
                result.push('_');
            }
            c if ascii_only && !c.is_ascii() => {
                let ascii_char = match c {
                    'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
                    'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
                    'è' | 'é' | 'ê' | 'ë' => 'e',
                    'È' | 'É' | 'Ê' | 'Ë' => 'E',
                    'ì' | 'í' | 'î' | 'ï' => 'i',
                    'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
                    'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
                    'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => 'O',
                    'ù' | 'ú' | 'û' | 'ü' => 'u',
                    'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
                    'ç' | 'Ç' => 'c',
                    'ñ' | 'Ñ' => 'n',
                    'ÿ' => 'y',
                    _ => '_',
                };
                result.push(ascii_char);
            }
            c => result.push(c),
        }
    }
    let trimmed = result.trim_matches(['.', ' ']).to_string();
    if trimmed.is_empty() {
        "Unknown".to_string()
    } else {
        trimmed
    }
}

/// Replace placeholders in template with song metadata.
/// Supports conditional blocks like `{Disc %disc/}`.
pub fn expand_template(template: &str, song: &Song, ext: &str) -> String {
    let mut expanded = template.to_string();

    // 1. Process conditional blocks `{...}` first
    while let Some(start) = expanded.find('{') {
        if let Some(end) = expanded[start..].find('}') {
            let actual_end = start + end;
            let block_content = &expanded[start + 1..actual_end];

            let has_disc = block_content.contains("%disc") && song.disc.is_some_and(|d| d > 0);
            let has_year = block_content.contains("%year") && song.year.is_some_and(|y| y > 0);
            let has_genre = block_content.contains("%genre")
                && song.genre.as_deref().is_some_and(|g| !g.trim().is_empty());
            let has_album_artist = block_content.contains("%albumartist")
                && song
                    .album_artist
                    .as_deref()
                    .is_some_and(|a| !a.trim().is_empty());
            let has_album = block_content.contains("%album")
                && !block_content.contains("%albumartist")
                && song.album.as_deref().is_some_and(|a| !a.trim().is_empty());
            let has_artist = block_content.contains("%artist")
                && !block_content.contains("%albumartist")
                && song.artist.as_deref().is_some_and(|a| !a.trim().is_empty());
            let has_track = (block_content.contains("%track")
                || block_content.contains("%track3")
                || block_content.contains("%rawtrack"))
                && song.track.is_some_and(|t| t > 0);

            let should_render = has_disc
                || has_year
                || has_genre
                || has_album_artist
                || has_album
                || has_artist
                || has_track;
            let replacement = if should_render {
                block_content.to_string()
            } else {
                String::new()
            };

            expanded.replace_range(start..=actual_end, &replacement);
        } else {
            break;
        }
    }

    // 2. Perform variable replacement with fallbacks for empty/whitespace string metadata
    let file_stem_fallback = song
        .path
        .as_ref()
        .map(PathBuf::from)
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "Unknown Title".to_string());

    let artist_str = song
        .artist
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Unknown Artist");

    let album_artist_str = song
        .album_artist
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| song.artist.as_deref().filter(|s| !s.trim().is_empty()))
        .unwrap_or("Unknown Artist");

    let album_str = song
        .album
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Unknown Album");

    let title_str = song
        .title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&file_stem_fallback);

    let genre_str = song
        .genre
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Unknown Genre");

    let year_str = song.year.map(|y| y.to_string()).unwrap_or_default();
    let disc_str = song
        .disc
        .map(|d| d.to_string())
        .unwrap_or_else(|| "1".to_string());

    let track_2digit = match song.track {
        Some(t) if t > 0 => format!("{:02}", t),
        _ => "00".to_string(),
    };
    let track_unpadded = match song.track {
        Some(t) if t > 0 => t.to_string(),
        _ => "0".to_string(),
    };
    let track_3digit = match song.track {
        Some(t) if t > 0 => format!("{:03}", t),
        _ => "000".to_string(),
    };

    let artist_clean = sanitize_component(artist_str, false, false);
    let album_artist_clean = sanitize_component(album_artist_str, false, false);
    let album_clean = sanitize_component(album_str, false, false);
    let title_clean = sanitize_component(title_str, false, false);
    let genre_clean = sanitize_component(genre_str, false, false);

    expanded = expanded.replace("%albumartist", &album_artist_clean);
    expanded = expanded.replace("%artist", &artist_clean);
    expanded = expanded.replace("%album", &album_clean);
    expanded = expanded.replace("%disc", &disc_str);
    expanded = expanded.replace("%track3", &track_3digit);
    expanded = expanded.replace("%rawtrack", &track_unpadded);
    expanded = expanded.replace("%track", &track_2digit);
    expanded = expanded.replace("%title", &title_clean);
    expanded = expanded.replace("%year", &year_str);
    expanded = expanded.replace("%genre", &genre_clean);
    expanded = expanded.replace("%extension", ext);

    expanded
}

/// Compute target path for a song given a template, options, and base directory.
pub fn build_target_path(
    song: &Song,
    template: &str,
    options: &OrganizeOptions,
    library_dirs: &[String],
) -> Result<PathBuf> {
    let source_path_str = song
        .path
        .as_ref()
        .ok_or_else(|| anyhow!("Song has no path"))?;
    let source_path = PathBuf::from(source_path_str);

    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("flac");

    // Determine root destination directory
    let root_dir: PathBuf = if let Some(ref dest) = options.destination_dir {
        PathBuf::from(dest)
    } else {
        library_dirs
            .iter()
            .map(PathBuf::from)
            .find(|dir| source_path.starts_with(dir))
            .or_else(|| source_path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    };

    let expanded = expand_template(template, song, ext);

    let parts: Vec<&str> = expanded
        .split(&['/', '\\'][..])
        .filter(|p| !p.trim().is_empty())
        .collect();

    let mut relative_path = PathBuf::new();
    for (idx, part) in parts.iter().enumerate() {
        let is_last = idx == parts.len() - 1;
        let sanitized = sanitize_component(
            part,
            options.replace_spaces_with_underscores,
            options.ascii_only,
        );
        if is_last {
            if !sanitized
                .to_lowercase()
                .ends_with(&format!(".{}", ext.to_lowercase()))
            {
                relative_path.push(format!("{}.{}", sanitized, ext));
            } else {
                relative_path.push(sanitized);
            }
        } else {
            relative_path.push(sanitized);
        }
    }

    Ok(root_dir.join(relative_path))
}

/// Compute dry-run preview items for given song IDs.
pub fn compute_preview(
    db: &Database,
    song_ids: &[i64],
    template: &str,
    options: &OrganizeOptions,
) -> Result<Vec<OrganizePreviewItem>> {
    let conn = db.pool.get()?;

    let mut stmt = conn.prepare("SELECT path FROM directories")?;
    let library_dirs: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let songs: Vec<Song> = if song_ids.is_empty() {
        let mut stmt = conn.prepare(
            "SELECT id, path, title, artist, album, album_artist, track, disc, year, genre
             FROM songs WHERE path IS NOT NULL AND TRIM(path) != '' AND source IN (1, 2) AND unavailable = 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Song {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2)?,
                artist: row.get(3)?,
                album: row.get(4)?,
                album_artist: row.get(5)?,
                track: row.get(6)?,
                disc: row.get(7)?,
                year: row.get(8)?,
                genre: row.get(9)?,
                ..Default::default()
            })
        })?;
        let items: Vec<Song> = rows.filter_map(|r| r.ok()).collect();
        items
    } else {
        let mut result = Vec::new();
        for chunk in song_ids.chunks(500) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "SELECT id, path, title, artist, album, album_artist, track, disc, year, genre
                 FROM songs WHERE id IN ({}) AND path IS NOT NULL AND TRIM(path) != '' AND unavailable = 0",
                placeholders
            );
            let mut stmt = conn.prepare(&sql)?;
            let params = rusqlite::params_from_iter(chunk.iter());
            let items: Vec<Song> = stmt
                .query_map(params, |row| {
                    Ok(Song {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        title: row.get(2)?,
                        artist: row.get(3)?,
                        album: row.get(4)?,
                        album_artist: row.get(5)?,
                        track: row.get(6)?,
                        disc: row.get(7)?,
                        year: row.get(8)?,
                        genre: row.get(9)?,
                        ..Default::default()
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            result.extend(items);
        }
        result
    };

    let mut preview_items = Vec::with_capacity(songs.len());
    let mut target_paths_map: HashMap<String, Vec<usize>> = HashMap::new();

    for song in &songs {
        let from_path = song.path.clone().unwrap_or_default();

        if from_path.trim().is_empty() {
            preview_items.push(OrganizePreviewItem {
                song_id: song.id,
                from_path: "(No source path)".to_string(),
                to_path: String::new(),
                status: OrganizePreviewStatus::Error,
                error_message: Some("Song record in database has no file path".to_string()),
            });
            continue;
        }

        let source_exists = Path::new(&from_path).exists();

        match build_target_path(song, template, options, &library_dirs) {
            Ok(target_buf) => {
                let to_path = target_buf.to_string_lossy().to_string();

                let (status, err_msg) = if !source_exists {
                    (
                        OrganizePreviewStatus::Error,
                        Some(format!("File missing on disk: {}", from_path)),
                    )
                } else if from_path == to_path {
                    (OrganizePreviewStatus::Unchanged, None)
                } else if song.title.is_none() && song.artist.is_none() {
                    (
                        OrganizePreviewStatus::MissingTag,
                        Some("Missing title and artist tags".to_string()),
                    )
                } else if song.title.is_none() {
                    (
                        OrganizePreviewStatus::MissingTag,
                        Some("Missing title tag".to_string()),
                    )
                } else if song.artist.is_none() {
                    (
                        OrganizePreviewStatus::MissingTag,
                        Some("Missing artist tag".to_string()),
                    )
                } else {
                    (OrganizePreviewStatus::Ok, None)
                };

                let idx = preview_items.len();
                target_paths_map
                    .entry(to_path.clone())
                    .or_default()
                    .push(idx);

                preview_items.push(OrganizePreviewItem {
                    song_id: song.id,
                    from_path,
                    to_path,
                    status,
                    error_message: err_msg,
                });
            }
            Err(e) => {
                preview_items.push(OrganizePreviewItem {
                    song_id: song.id,
                    from_path: from_path.clone(),
                    to_path: String::new(),
                    status: OrganizePreviewStatus::Error,
                    error_message: Some(format!("Template error: {}", e)),
                });
            }
        }
    }

    for (_target_path, indices) in target_paths_map {
        if indices.len() > 1 {
            // Find the canonical item (prefer Unchanged)
            let mut canonical_idx = indices[0];
            for &idx in &indices {
                if preview_items[idx].status == OrganizePreviewStatus::Unchanged {
                    canonical_idx = idx;
                    break;
                }
            }

            let mut duplicate_counter = 1;

            for &idx in &indices {
                if idx == canonical_idx {
                    continue; // Canonical item gets to keep its target, but it might still have an external collision (checked below)
                }

                if preview_items[idx].status == OrganizePreviewStatus::Error {
                    continue;
                }

                // If these are duplicate database records pointing to the exact same physical file
                // (e.g. case-only differences on Windows), ignore the collision for this extra record.
                if let (Ok(canon_curr), Ok(canon_base)) = (
                    Path::new(&preview_items[idx].from_path).canonicalize(),
                    Path::new(&preview_items[canonical_idx].from_path).canonicalize(),
                ) {
                    if canon_curr == canon_base {
                        preview_items[idx].status = OrganizePreviewStatus::Unchanged;
                        preview_items[idx].to_path = preview_items[idx].from_path.clone();
                        preview_items[idx].error_message = None;
                        continue;
                    }
                }

                let original_target = Path::new(&preview_items[idx].to_path);
                let source_path = Path::new(&preview_items[idx].from_path);

                let dest_folder: PathBuf = if let Some(ref dest) = options.destination_dir {
                    PathBuf::from(dest)
                } else {
                    library_dirs
                        .iter()
                        .map(PathBuf::from)
                        .find(|dir| source_path.starts_with(dir))
                        .or_else(|| source_path.parent().map(|p| p.to_path_buf()))
                        .unwrap_or_else(|| PathBuf::from("."))
                };

                let rel_path = original_target
                    .strip_prefix(&dest_folder)
                    .unwrap_or(original_target);

                let mut dup_path = dest_folder.to_path_buf();
                dup_path.push("Duplicates");
                dup_path.push(rel_path);

                let mut final_dup_path = dup_path.clone();
                let mut is_unchanged = false;

                while final_dup_path.exists() {
                    if final_dup_path == source_path {
                        is_unchanged = true;
                        break;
                    }
                    if let Some(ext) = dup_path.extension().map(|s| s.to_owned()) {
                        if let Some(stem) = dup_path
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                        {
                            final_dup_path = dup_path.with_file_name(format!(
                                "{} ({}).{}",
                                stem,
                                duplicate_counter,
                                ext.to_string_lossy()
                            ));
                        }
                    }
                    duplicate_counter += 1;
                }

                preview_items[idx].to_path = final_dup_path.to_string_lossy().to_string();
                if is_unchanged {
                    preview_items[idx].status = OrganizePreviewStatus::Unchanged;
                    preview_items[idx].error_message = None;
                } else {
                    preview_items[idx].status = OrganizePreviewStatus::Ok;
                    preview_items[idx].error_message = Some(format!(
                        "Routed to Duplicates (collision with {})",
                        preview_items[canonical_idx].from_path
                    ));
                }
            }
        }
    }

    // Second pass: check for external collisions (where the target path already exists on disk)
    for item in preview_items.iter_mut() {
        if item.status == OrganizePreviewStatus::Error
            || item.status == OrganizePreviewStatus::MissingTag
        {
            continue;
        }

        // If it's Unchanged, it means it already exists on disk at to_path because it IS the file at to_path.
        if item.from_path == item.to_path {
            continue;
        }

        let current_target = PathBuf::from(&item.to_path);
        let source_path = Path::new(&item.from_path);

        if current_target.exists() {
            // Check if they are actually the exact same physical file (e.g. a case-only rename on Windows)
            if let (Ok(canon_from), Ok(canon_to)) =
                (source_path.canonicalize(), current_target.canonicalize())
            {
                if canon_from == canon_to {
                    continue;
                }
            }

            // External collision!
            let dest_folder: PathBuf = if let Some(ref dest) = options.destination_dir {
                PathBuf::from(dest)
            } else {
                library_dirs
                    .iter()
                    .map(PathBuf::from)
                    .find(|dir| source_path.starts_with(dir))
                    .or_else(|| source_path.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| PathBuf::from("."))
            };

            let rel_path = current_target
                .strip_prefix(&dest_folder)
                .unwrap_or(&current_target);

            let mut dup_path = dest_folder.to_path_buf();
            dup_path.push("Duplicates");
            dup_path.push(rel_path);

            let mut duplicate_counter = 1;
            let mut final_dup_path = dup_path.clone();
            let mut is_unchanged = false;

            while final_dup_path.exists() {
                if final_dup_path == source_path {
                    is_unchanged = true;
                    break;
                }
                if let Some(ext) = dup_path.extension().map(|s| s.to_owned()) {
                    if let Some(stem) = dup_path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                    {
                        final_dup_path = dup_path.with_file_name(format!(
                            "{} ({}).{}",
                            stem,
                            duplicate_counter,
                            ext.to_string_lossy()
                        ));
                    }
                }
                duplicate_counter += 1;
            }

            item.to_path = final_dup_path.to_string_lossy().to_string();
            if is_unchanged {
                item.status = OrganizePreviewStatus::Unchanged;
                item.error_message = None;
            } else {
                item.status = OrganizePreviewStatus::Ok;
                item.error_message =
                    Some("Routed to Duplicates (collision with canonical file)".to_string());
            }
        }
    }

    Ok(preview_items)
}

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "ogg", "opus", "wav", "aiff", "aif", "wma", "alac", "dsf", "dff",
];

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn move_companion_files(
    conn: &rusqlite::Connection,
    src_dir: &Path,
    dst_dir: &Path,
) -> Vec<String> {
    let mut errors = Vec::new();
    if !src_dir.exists() || !src_dir.is_dir() || src_dir == dst_dir {
        return errors;
    }

    let entries = match fs::read_dir(src_dir) {
        Ok(e) => e,
        Err(_) => return errors,
    };

    let remaining_entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    let has_audio_left = remaining_entries
        .iter()
        .any(|entry| is_audio_file(&entry.path()));

    if !has_audio_left {
        if let Err(e) = fs::create_dir_all(dst_dir) {
            errors.push(format!(
                "Failed to create directory {}: {}",
                dst_dir.display(),
                e
            ));
            return errors;
        }

        for entry in remaining_entries {
            let path = entry.path();
            if path.is_file() && !is_audio_file(&path) {
                if let Some(file_name) = path.file_name() {
                    let dst_file = dst_dir.join(file_name);
                    if !dst_file.exists() {
                        let moved = fs::rename(&path, &dst_file).or_else(|_| {
                            fs::copy(&path, &dst_file).and_then(|_| fs::remove_file(&path))
                        });
                        if moved.is_ok() {
                            // Folder-level cover art is stored as an absolute
                            // path in art_automatic (see scan_folder_art()) —
                            // without this, moved albums would show broken
                            // art until a future full rescan re-derives it.
                            let _ = conn.execute(
                                "UPDATE songs SET art_automatic = ?1 WHERE art_automatic = ?2",
                                params![
                                    dst_file.to_string_lossy().to_string(),
                                    path.to_string_lossy().to_string()
                                ],
                            );
                        }
                    }
                }
            }
        }
    }

    errors
}

/// Execute batch file relocation and SQLite path updates.
pub fn execute_apply(
    db: &Database,
    watcher_paused: &Arc<AtomicU32>,
    items: &[OrganizeApplyItem],
    clean_empty_dirs: bool,
    move_extra_files: bool,
    cover_manager: Option<&CoverManager>,
) -> Result<OrganizeResult> {
    let _watcher_pause_guard =
        crate::collection::WatcherPauseGuard::new(Arc::clone(watcher_paused));

    let mut moved_count = 0;
    let mut skipped_count = 0;
    let mut errors = Vec::new();
    let mut source_parents_to_clean: HashSet<PathBuf> = HashSet::new();
    let mut moved_dir_pairs: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut moved_items: Vec<(i64, String)> = Vec::new();

    let mut conn = db.pool.get()?;
    let tx = conn.transaction()?;

    for item in items {
        if item.from_path == item.to_path {
            skipped_count += 1;
            continue;
        }

        let src = Path::new(&item.from_path);
        let dst = Path::new(&item.to_path);

        if !src.exists() {
            errors.push(format!("Source file missing: {}", item.from_path));
            skipped_count += 1;
            continue;
        }

        if let (Some(src_parent), Some(dst_parent)) = (src.parent(), dst.parent()) {
            source_parents_to_clean.insert(src_parent.to_path_buf());
            moved_dir_pairs.insert(src_parent.to_path_buf(), dst_parent.to_path_buf());
        }

        if let Some(dst_parent) = dst.parent() {
            if let Err(e) = fs::create_dir_all(dst_parent) {
                errors.push(format!(
                    "Failed to create directory {}: {}",
                    dst_parent.display(),
                    e
                ));
                skipped_count += 1;
                continue;
            }
        }

        let move_res =
            fs::rename(src, dst).or_else(|_| fs::copy(src, dst).and_then(|_| fs::remove_file(src)));

        match move_res {
            Ok(_) => {
                // Clear any existing row that has this exact path to avoid UNIQUE constraint failure.
                // This can happen if the target path was previously scanned into the DB, but the file
                // was overwritten or replaced by this organize operation.
                let _ = tx.execute(
                    "DELETE FROM songs WHERE path = ?1 AND id != ?2",
                    params![item.to_path, item.song_id],
                );

                if let Err(e) = tx.execute(
                    "UPDATE songs SET path = ?1 WHERE id = ?2",
                    params![item.to_path, item.song_id],
                ) {
                    errors.push(format!(
                        "Failed to update DB for song {}: {}",
                        item.song_id, e
                    ));
                } else {
                    moved_count += 1;
                    moved_items.push((item.song_id, item.to_path.clone()));
                }
            }
            Err(e) => {
                errors.push(format!(
                    "Failed to move {} -> {}: {}",
                    item.from_path, item.to_path, e
                ));
                skipped_count += 1;
            }
        }
    }

    if let Err(e) = tx.commit() {
        errors.push(format!("Transaction commit error: {}", e));
    }

    if move_extra_files {
        for (src_dir, dst_dir) in moved_dir_pairs {
            let companion_errors = move_companion_files(&conn, &src_dir, &dst_dir);
            errors.extend(companion_errors);
        }
    }

    if clean_empty_dirs {
        for parent in source_parents_to_clean {
            let _ = remove_empty_dirs_recursive(&parent);
        }
    }

    // Post-relocation cover art refresh for moved tracks
    for (song_id, to_path_str) in moved_items {
        let to_path = Path::new(&to_path_str);
        let mut new_art_auto: Option<String> = None;

        if let Some(folder_art_path) = CoverManager::scan_folder_art_static(to_path) {
            new_art_auto = Some(folder_art_path.to_string_lossy().to_string());
        } else if let Some(cm) = cover_manager {
            if let Ok((artist, album, art_embedded)) = conn.query_row(
                "SELECT COALESCE(NULLIF(album_artist, ''), artist, ''), album, art_embedded FROM songs WHERE id = ?1",
                params![song_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        row.get::<_, bool>(2)?,
                    ))
                },
            ) {
                if art_embedded {
                    if let Ok(Some(cached_fn)) = cm.extract_embedded_art(to_path, &artist, &album) {
                        new_art_auto = Some(cached_fn);
                    }
                }
            }
        }

        if let Some(art_path) = new_art_auto {
            let _ = conn.execute(
                "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE id = ?2",
                params![art_path, song_id],
            );
        } else {
            // Check if existing art_automatic points to an unaccessible/non-existent old path
            let current_art: Option<String> = conn
                .query_row(
                    "SELECT art_automatic FROM songs WHERE id = ?1",
                    params![song_id],
                    |row| row.get(0),
                )
                .unwrap_or(None);

            if let Some(ref art) = current_art {
                let art_p = Path::new(art);
                if (art_p.is_absolute() || art.contains('/') || art.contains('\\'))
                    && !art_p.exists()
                {
                    let _ = conn.execute(
                        "UPDATE songs SET art_automatic = NULL WHERE id = ?1",
                        params![song_id],
                    );
                }
            }
        }
    }

    Ok(OrganizeResult {
        moved_count,
        skipped_count,
        errors,
    })
}

fn force_remove_file(path: &Path) -> std::io::Result<()> {
    if let Ok(metadata) = fs::metadata(path) {
        let mut perms = metadata.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = fs::set_permissions(path, perms);
        }
    }
    fs::remove_file(path)
}

fn force_remove_dir(path: &Path) -> std::io::Result<()> {
    if let Ok(metadata) = fs::metadata(path) {
        let mut perms = metadata.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = fs::set_permissions(path, perms);
        }
    }
    fs::remove_dir(path)
}

fn clean_if_effectively_empty(dir: &Path) -> std::io::Result<bool> {
    let mut junk_files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            return Ok(false); // contains a subdirectory, not empty
        }

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let lower = name.to_lowercase();
            let is_junk = lower == ".ds_store"
                || lower == "._.ds_store"
                || lower == "desktop.ini"
                || lower == "thumbs.db"
                || lower == "ehthumbs.db"
                || name.starts_with("._"); // AppleDouble resource forks

            if is_junk {
                junk_files.push(path);
            } else {
                return Ok(false); // Found a real file
            }
        } else {
            return Ok(false); // Unparseable file name
        }
    }

    for junk in junk_files {
        let _ = force_remove_file(&junk);
    }

    Ok(true)
}

/// Removes `dir` if empty, then walks upward removing each newly-emptied
/// ancestor in turn. Returns how many directories were actually removed.
pub(crate) fn remove_empty_dirs_recursive(dir: &Path) -> std::io::Result<usize> {
    if !dir.exists() || !dir.is_dir() {
        return Ok(0);
    }

    if clean_if_effectively_empty(dir).unwrap_or(false) {
        force_remove_dir(dir)?;
        let mut removed = 1;
        if let Some(parent) = dir.parent() {
            removed += remove_empty_dirs_recursive(parent).unwrap_or(0);
        }
        return Ok(removed);
    }

    Ok(0)
}

/// Removes every empty subdirectory under `root` (never the root itself),
/// deepest first, so a folder left empty only after its own empty children
/// are removed still gets cleaned up in the same pass. Used by "Clean Up" to
/// sweep the whole watched library, not just folders tied to a song that was
/// just pruned from the database.
pub(crate) fn remove_empty_dirs_under_root(root: &Path) -> usize {
    if !root.is_dir() {
        return 0;
    }

    let mut removed = 0;
    for entry in walkdir::WalkDir::new(root)
        .min_depth(1)
        .contents_first(true)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        // Cloud-synced folders (e.g. OneDrive Files On-Demand placeholders)
        // carry a reparse-point attribute, so `entry.file_type()` (which
        // doesn't follow reparse points) reports them as something other
        // than a plain directory even though they are one. Check the
        // resolved path instead so these aren't silently skipped.
        if !entry.path().is_dir() {
            continue;
        }
        let dir = entry.path();
        if clean_if_effectively_empty(dir).unwrap_or(false) {
            if force_remove_dir(dir).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_expansion_default_pattern() {
        let song = Song {
            id: 1,
            title: Some("Tokyo".to_string()),
            artist: Some("Imagine Dragons".to_string()),
            album: Some("Night Visions".to_string()),
            album_artist: Some("Imagine Dragons".to_string()),
            disc: Some(2),
            track: Some(11),
            ..Default::default()
        };

        let template = "%albumartist/%album/%disc-%track %title";
        let expanded = expand_template(template, &song, "flac");
        assert_eq!(expanded, "Imagine Dragons/Night Visions/2-11 Tokyo");
    }

    #[test]
    fn test_template_expansion_conditional_disc() {
        let song_with_disc = Song {
            id: 1,
            title: Some("Track 1".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            disc: Some(2),
            track: Some(1),
            ..Default::default()
        };

        let song_no_disc = Song {
            id: 2,
            title: Some("Track 1".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            disc: None,
            track: Some(1),
            ..Default::default()
        };

        let template = "%albumartist/%album/{Disc %disc/}%track %title";
        assert_eq!(
            expand_template(template, &song_with_disc, "flac"),
            "Artist/Album/Disc 2/01 Track 1"
        );
        assert_eq!(
            expand_template(template, &song_no_disc, "flac"),
            "Artist/Album/01 Track 1"
        );
    }

    #[test]
    fn test_template_expansion_optional_album() {
        let song_with_album = Song {
            id: 1,
            title: Some("Single Track".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Greatest Hits".to_string()),
            track: Some(1),
            ..Default::default()
        };

        let song_no_album = Song {
            id: 2,
            title: Some("Single Track".to_string()),
            artist: Some("Artist".to_string()),
            album: None,
            track: Some(1),
            ..Default::default()
        };

        let template = "%albumartist/{%album/}{%disc-}%track %title";
        assert_eq!(
            expand_template(template, &song_with_album, "flac"),
            "Artist/Greatest Hits/01 Single Track"
        );
        assert_eq!(
            expand_template(template, &song_no_album, "flac"),
            "Artist/01 Single Track"
        );
    }

    #[test]
    fn test_template_expansion_optional_track() {
        let song_with_track = Song {
            id: 1,
            title: Some("Single Track".to_string()),
            artist: Some("Artist".to_string()),
            track: Some(3),
            ..Default::default()
        };

        let song_no_track = Song {
            id: 2,
            title: Some("Single Track".to_string()),
            artist: Some("Artist".to_string()),
            track: None,
            ..Default::default()
        };

        let template = "%albumartist/{%album/}{%disc-}{%track }%title";
        assert_eq!(
            expand_template(template, &song_with_track, "flac"),
            "Artist/03 Single Track"
        );
        assert_eq!(
            expand_template(template, &song_no_track, "flac"),
            "Artist/Single Track"
        );
    }

    #[test]
    fn test_sanitization_and_options() {
        let name = "AC/DC: Highway to Hell?";
        assert_eq!(
            sanitize_component(name, false, false),
            "AC_DC_ Highway to Hell_"
        );
        assert_eq!(
            sanitize_component(name, true, false),
            "AC_DC__Highway_to_Hell_"
        );

        let unicode_name = "Céline Dion — Café";
        assert_eq!(
            sanitize_component(unicode_name, false, true),
            "Celine Dion _ Cafe"
        );
    }

    #[test]
    fn test_compute_preview_excludes_unavailable_songs() {
        use crate::collection::upsert_song;
        use crate::db::Database;
        use crate::models::{FileType, SongSource};
        use std::sync::Arc;

        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_organizer_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let conn = db.pool.get().unwrap();

        let available = Song {
            path: Some("/music/available.mp3".to_string()),
            title: Some("Available Track".to_string()),
            artist: Some("Artist".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Mp3,
            unavailable: false,
            ..Default::default()
        };
        upsert_song(&conn, &available).unwrap();

        // Simulates a song whose file was deleted and then pruned via
        // "Clean Up Missing Songs" — upsert_song() always writes a fresh
        // row as available (unavailable=0 on conflict), since it's only
        // ever called for files the scanner just found on disk. Marking a
        // song unavailable only ever happens via prune_missing_songs()'s
        // direct UPDATE, so replicate that here rather than going through
        // upsert_song(), which has no way to insert one pre-pruned.
        let pruned = Song {
            path: Some("/music/deleted-folder/gone.mp3".to_string()),
            title: Some("Gone Track".to_string()),
            artist: Some("Artist".to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Mp3,
            ..Default::default()
        };
        upsert_song(&conn, &pruned).unwrap();
        conn.execute(
            "UPDATE songs SET unavailable = 1 WHERE path = ?1",
            rusqlite::params![pruned.path],
        )
        .unwrap();
        drop(conn);

        let options = OrganizeOptions {
            destination_dir: None,
            replace_spaces_with_underscores: false,
            ascii_only: false,
            clean_empty_dirs: false,
            move_extra_files: false,
        };

        let items = compute_preview(&db, &[], "%artist/%title", &options).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].from_path, "/music/available.mp3");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_execute_apply_relocates_folder_art_and_updates_db() {
        use crate::collection::upsert_song;
        use crate::db::Database;
        use crate::models::{FileType, SongSource};

        let base = std::env::temp_dir().join(format!(
            "luminous_organizer_apply_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let src_dir = base.join("src_album");
        let dst_dir = base.join("dst_album");
        fs::create_dir_all(&src_dir).unwrap();

        let src_song_path = src_dir.join("track.mp3");
        let src_cover_path = src_dir.join("cover.jpg");
        fs::write(&src_song_path, b"fake audio").unwrap();
        fs::write(&src_cover_path, b"fake cover").unwrap();

        let db = Arc::new(Database::new(base.join("appdata")).unwrap());
        let conn = db.pool.get().unwrap();

        let song = Song {
            path: Some(src_song_path.to_string_lossy().to_string()),
            title: Some("Track".to_string()),
            artist: Some("Artist".to_string()),
            art_automatic: Some(src_cover_path.to_string_lossy().to_string()),
            source: SongSource::LocalFile,
            filetype: FileType::Mp3,
            ..Default::default()
        };
        upsert_song(&conn, &song).unwrap();
        let song_id: i64 = conn
            .query_row(
                "SELECT id FROM songs WHERE path = ?1",
                params![song.path],
                |r| r.get(0),
            )
            .unwrap();
        drop(conn);

        let dst_song_path = dst_dir.join("track.mp3");
        let items = vec![OrganizeApplyItem {
            song_id,
            from_path: src_song_path.to_string_lossy().to_string(),
            to_path: dst_song_path.to_string_lossy().to_string(),
        }];

        let watcher_paused = Arc::new(AtomicU32::new(0));
        let result = execute_apply(&db, &watcher_paused, &items, true, true, None).unwrap();
        assert_eq!(result.moved_count, 1);
        assert!(result.errors.is_empty());

        // The companion cover file should have followed the track to its new folder...
        let dst_cover_path = dst_dir.join("cover.jpg");
        assert!(dst_cover_path.exists());
        assert!(!src_cover_path.exists());

        // ...and the song's art_automatic reference should point at the new location,
        // not the now-nonexistent source path.
        let conn = db.pool.get().unwrap();
        let art_automatic: Option<String> = conn
            .query_row(
                "SELECT art_automatic FROM songs WHERE id = ?1",
                params![song_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            art_automatic,
            Some(dst_cover_path.to_string_lossy().to_string())
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
