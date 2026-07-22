//! Tag-based file organizer module.
//!
//! Provides template parsing, dry-run path computation, conflict detection,
//! and batch file relocation with atomic SQLite path updates.

use crate::db::Database;
use crate::models::Song;
use anyhow::{anyhow, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

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

            let should_render = has_disc || has_year || has_genre || has_album_artist;
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

    let track_str = match song.track {
        Some(t) if t > 0 => format!("{:02}", t),
        _ => "00".to_string(),
    };

    expanded = expanded.replace("%albumartist", album_artist_str);
    expanded = expanded.replace("%artist", artist_str);
    expanded = expanded.replace("%album", album_str);
    expanded = expanded.replace("%disc", &disc_str);
    expanded = expanded.replace("%track", &track_str);
    expanded = expanded.replace("%title", title_str);
    expanded = expanded.replace("%year", &year_str);
    expanded = expanded.replace("%genre", genre_str);
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
             FROM songs WHERE path IS NOT NULL AND TRIM(path) != '' AND source IN (1, 2)",
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
                 FROM songs WHERE id IN ({}) AND path IS NOT NULL AND TRIM(path) != ''",
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

    for (target_path, indices) in target_paths_map {
        if indices.len() > 1 && !target_path.trim().is_empty() {
            for idx in indices {
                let status = preview_items[idx].status.clone();
                if status != OrganizePreviewStatus::Unchanged
                    && status != OrganizePreviewStatus::Error
                {
                    preview_items[idx].status = OrganizePreviewStatus::Collision;
                    preview_items[idx].error_message = Some(format!(
                        "Multiple files map to target path: {}",
                        target_path
                    ));
                }
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

fn move_companion_files(src_dir: &Path, dst_dir: &Path) -> Vec<String> {
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
                        let _ = fs::rename(&path, &dst_file).or_else(|_| {
                            fs::copy(&path, &dst_file).and_then(|_| fs::remove_file(&path))
                        });
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
    watcher_paused: &AtomicBool,
    items: &[OrganizeApplyItem],
    clean_empty_dirs: bool,
    move_extra_files: bool,
) -> Result<OrganizeResult> {
    watcher_paused.store(true, Ordering::Relaxed);

    let mut moved_count = 0;
    let mut skipped_count = 0;
    let mut errors = Vec::new();
    let mut source_parents_to_clean: HashSet<PathBuf> = HashSet::new();
    let mut moved_dir_pairs: HashMap<PathBuf, PathBuf> = HashMap::new();

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
            let companion_errors = move_companion_files(&src_dir, &dst_dir);
            errors.extend(companion_errors);
        }
    }

    if clean_empty_dirs {
        for parent in source_parents_to_clean {
            let _ = remove_empty_dirs_recursive(&parent);
        }
    }

    watcher_paused.store(false, Ordering::Relaxed);

    Ok(OrganizeResult {
        moved_count,
        skipped_count,
        errors,
    })
}

fn remove_empty_dirs_recursive(dir: &Path) -> std::io::Result<()> {
    if !dir.exists() || !dir.is_dir() {
        return Ok(());
    }

    let mut entries = fs::read_dir(dir)?;
    if entries.next().is_none() {
        fs::remove_dir(dir)?;
        if let Some(parent) = dir.parent() {
            let _ = remove_empty_dirs_recursive(parent);
        }
    }

    Ok(())
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
}
