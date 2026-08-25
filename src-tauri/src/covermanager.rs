//! Cover art acquisition, caching, and lookup.
//!
//! Art comes from three sources, tried in this order by the collection
//! scanner: embedded tag pictures (`extract_embedded_art`), image files
//! sitting next to the song (`scan_folder_art`), then an iTunes Search API
//! fallback (`fetch_remote_cover`). Whichever source succeeds writes into
//! `songs.art_automatic`; a user-picked cover instead goes in
//! `art_manual` and always takes precedence (see `get_cover_art_path`/
//! `get_cover_art_uri`). Extracted/downloaded images are cached as files
//! under `covers_dir`, keyed by `get_album_hash`.

use crate::db::Database;
use anyhow::{Context, Result};
use lofty::{file::TaggedFileExt, probe::Probe};
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub struct CoverManager {
    db: Arc<Database>,
    covers_dir: PathBuf,
}

/// Inspects raw image bytes to detect magic headers for PNG, JPEG, WEBP, GIF, BMP.
/// If extraneous bytes are prepended before valid magic headers (e.g. JPEG SOI 0xFF 0xD8 0xFF
/// or PNG header \x89PNG\r\n\x1a\n), it trims the slice to start at the valid magic header.
/// Returns `(cleaned_data_slice, mime_type, extension)`.
pub fn detect_image_format_and_clean(data: &[u8]) -> (&[u8], &'static str, &'static str) {
    if data.is_empty() {
        return (data, "image/jpeg", "jpg");
    }

    // 1. Direct magic byte checks
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return (data, "image/png", "png");
    }
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return (data, "image/jpeg", "jpg");
    }
    if data.starts_with(b"RIFF") && data.len() >= 12 && &data[8..12] == b"WEBP" {
        return (data, "image/webp", "webp");
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return (data, "image/gif", "gif");
    }
    if data.starts_with(b"BM") {
        return (data, "image/bmp", "bmp");
    }

    // 2. Scan for embedded JPEG SOI marker (0xFF, 0xD8, 0xFF) within the first 128KB if prepended with extraneous bytes
    const SCAN_LIMIT: usize = 131072;
    let search_buf = &data[..data.len().min(SCAN_LIMIT)];

    if let Some(pos) = search_buf.windows(3).position(|w| w == [0xFF, 0xD8, 0xFF]) {
        return (&data[pos..], "image/jpeg", "jpg");
    }

    // 3. Scan for embedded PNG header (\x89PNG\r\n\x1a\n)
    const PNG_HEADER: &[u8; 8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if let Some(pos) = search_buf.windows(8).position(|w| w == PNG_HEADER) {
        return (&data[pos..], "image/png", "png");
    }

    // 4. Fallback if no magic bytes found
    (data, "image/jpeg", "jpg")
}

impl CoverManager {
    pub fn new(db: Arc<Database>, app_data_dir: PathBuf) -> Self {
        let covers_dir = app_data_dir.join("covers");
        if !covers_dir.exists() {
            let _ = std::fs::create_dir_all(&covers_dir);
        }
        Self { db, covers_dir }
    }

    /// Derive a stable cache filename stem from `album_artist` + `album`
    /// (case-insensitive). Callers pass a song's own title as `album` for
    /// albumless singles (#106) rather than an empty string, so that two
    /// singles by the same artist don't collide onto the same cached file.
    pub fn get_album_hash(&self, album_artist: &str, album: &str) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        let combined = format!("{}:{}", album_artist.to_lowercase(), album.to_lowercase());
        for &byte in combined.as_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3u64);
        }
        format!("album-{:016x}", hash)
    }

    /// Save the file's first embedded tag picture (if any) to the covers
    /// cache and return its cache filename. Returns `Ok(None)` — not an
    /// error — when the file has no tag or the tag has no picture; callers
    /// are expected to fall through to `scan_folder_art`/`fetch_remote_cover`
    /// in that case.
    pub fn extract_embedded_art(
        &self,
        audio_path: &Path,
        album_artist: &str,
        album: &str,
    ) -> Result<Option<String>> {
        let tagged_file = Probe::open(audio_path)
            .context("failed to open audio file for cover extraction")?
            .read()
            .context("failed to read audio file tags")?;

        let picture = tagged_file
            .primary_tag()
            .and_then(|t| t.pictures().first())
            .or_else(|| tagged_file.tags().iter().find_map(|t| t.pictures().first()));

        let picture = match picture {
            Some(p) => p,
            None => return Ok(None),
        };

        let raw_data = picture.data();
        let (cleaned_data, _mime, ext) = detect_image_format_and_clean(raw_data);

        let hash_name = self.get_album_hash(album_artist, album);
        let filename = format!("{}.{}", hash_name, ext);
        let dest_path = self.covers_dir.join(&filename);

        std::fs::write(&dest_path, cleaned_data)
            .context("failed to write cover art file to cache")?;

        log::info!("Extracted embedded cover art to: {}", dest_path.display());
        Ok(Some(filename))
    }

    /// Look for a same-named-by-convention image file (`cover.jpg`,
    /// `folder.png`, etc.) next to `audio_path` and return its canonical
    /// absolute path, or `None` if the song has embedded/manual art already
    /// or no match exists. Doesn't copy into the covers cache — the
    /// returned path is used directly (see `get_cover_art_path`).
    pub fn scan_folder_art(&self, audio_path: &Path) -> Option<PathBuf> {
        Self::scan_folder_art_static(audio_path)
    }

    /// Same as `scan_folder_art`, callable without a `CoverManager`
    /// instance — used by `organizer.rs` after relocating a file, where no
    /// manager is in scope.
    pub fn scan_folder_art_static(audio_path: &Path) -> Option<PathBuf> {
        let parent_dir = audio_path.parent()?;
        let common_names = [
            "cover",
            "folder",
            "album",
            "front",
            "artwork",
            "cover-art",
            "album-art",
            "folder-art",
        ];
        let common_extensions = ["jpg", "jpeg", "png", "webp", "gif", "bmp"];

        if let Ok(entries) = std::fs::read_dir(parent_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let stem_lower = stem.to_lowercase();
                        if common_names.contains(&stem_lower.as_str()) {
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                if common_extensions.contains(&ext.to_lowercase().as_str()) {
                                    let canonical = path.canonicalize().unwrap_or(path);
                                    let s = canonical.to_string_lossy();
                                    #[cfg(windows)]
                                    let cleaned_s = match s.strip_prefix(r"\\?\") {
                                        Some(stripped) => stripped.to_string(),
                                        None => s.to_string(),
                                    };
                                    #[cfg(not(windows))]
                                    let cleaned_s = s.to_string();
                                    return Some(PathBuf::from(cleaned_s));
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Look up the song's artist/album on the iTunes Search API and cache
    /// the top result's 600x600 artwork. Returns `Ok(None)` — not an error —
    /// both when the song lacks artist/album metadata to search with and
    /// when the API returns no match; either way, the song's `art_unset`
    /// flag is set on a miss so future scans (and the frontend's per-row
    /// retry-on-mount) don't keep re-querying it.
    ///
    /// Deliberately never holds a pooled DB connection across the network
    /// `.await`s below — the pool only has a handful of connections (see
    /// `Database::new`), and a stalled/slow request holding one would
    /// starve every other DB-backed command in the app (#362 follow-up).
    pub async fn fetch_remote_cover(&self, song_id: i64) -> Result<Option<String>> {
        let (artist, album, album_artist, art_unset) = {
            let conn = self.db.pool.get()?;
            conn.query_row(
                "SELECT artist, album, album_artist, art_unset FROM songs WHERE id = ?1",
                params![song_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, bool>(3)?,
                    ))
                },
            )?
        };

        // Already tried and failed (no metadata or no API match) — don't
        // re-hit the network every time a row remounts.
        if art_unset {
            return Ok(None);
        }

        let artist_query = album_artist.as_ref().or(artist.as_ref());
        let (query_artist, query_album) = match (artist_query, album.as_ref()) {
            (Some(art), Some(alb)) => (art, alb),
            _ => {
                // No artist/album to search with — mark unset immediately so
                // untagged songs don't get re-queried on every remount.
                let conn = self.db.pool.get()?;
                conn.execute(
                    "UPDATE songs SET art_unset = 1 WHERE id = ?1",
                    params![song_id],
                )?;
                return Ok(None);
            }
        };

        log::info!(
            "Fetching remote cover art for: {} - {}",
            query_artist,
            query_album
        );
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let search_url = format!(
            "https://itunes.apple.com/search?term={}&entity=album&limit=1",
            percent_encoding::utf8_percent_encode(
                &format!("{} {}", query_artist, query_album),
                percent_encoding::NON_ALPHANUMERIC
            )
        );

        let response = client
            .get(&search_url)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        let results = response.get("results").and_then(|r| r.as_array());

        if let Some(results) = results {
            if let Some(first_result) = results.first() {
                // Get 100x100 URL and replace with 600x600 for higher resolution
                if let Some(url_100) = first_result.get("artworkUrl100").and_then(|u| u.as_str()) {
                    let url_600 = url_100.replace("100x100bb.jpg", "600x600bb.jpg");
                    log::info!("Downloading remote cover art from: {}", url_600);

                    let img_bytes = client.get(&url_600).send().await?.bytes().await?;
                    let (cleaned_bytes, _mime, ext) = detect_image_format_and_clean(&img_bytes);
                    let hash_name = self.get_album_hash(query_artist, query_album);
                    let filename = format!("{}.{}", hash_name, ext);
                    let dest_path = self.covers_dir.join(&filename);

                    std::fs::write(&dest_path, cleaned_bytes)?;
                    log::info!("Saved remote cover art to: {}", dest_path.display());

                    let conn = self.db.pool.get()?;
                    conn.execute(
                        "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE id = ?2",
                        params![filename, song_id],
                    )?;

                    return Ok(Some(filename));
                }
            }
        }

        // If no artwork found, mark it as unset so we don't spam requests
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE songs SET art_unset = 1 WHERE id = ?1",
            params![song_id],
        )?;

        Ok(None)
    }

    /// Resolve the on-disk filesystem path (not a `luminous-art://` URI) for
    /// a song's cover art. For consumers that need a real file rather than a
    /// webview-protocol URL — e.g. the OS "Now Playing" media session (#80),
    /// which loads artwork directly.
    pub fn get_cover_art_path(&self, song_id: i64) -> Result<Option<PathBuf>> {
        let conn = self.db.pool.get()?;
        let (art_automatic, art_manual, art_unset) = conn.query_row(
            "SELECT art_automatic, art_manual, art_unset FROM songs WHERE id = ?1",
            params![song_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            },
        )?;

        if art_unset {
            return Ok(None);
        }

        if let Some(manual) = art_manual {
            return Ok(Some(self.covers_dir.join(manual)));
        }

        if let Some(auto) = art_automatic {
            return Ok(Some(if auto.starts_with("album-") {
                self.covers_dir.join(auto)
            } else {
                PathBuf::from(auto)
            }));
        }

        Ok(None)
    }

    /// Same precedence as `get_cover_art_path` (manual > cached automatic >
    /// folder-art path > unset), but returns a `luminous-art://` webview URI
    /// instead of a filesystem path — the form the frontend `<img>` tags use.
    pub fn get_cover_art_uri(&self, song_id: i64) -> Result<Option<String>> {
        let conn = self.db.pool.get()?;
        let (_art_embedded, art_automatic, art_manual, art_unset) = conn.query_row(
            "SELECT art_embedded, art_automatic, art_manual, art_unset FROM songs WHERE id = ?1",
            params![song_id],
            |row| {
                Ok((
                    row.get::<_, bool>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            },
        )?;

        if art_unset {
            return Ok(None);
        }

        if let Some(ref manual) = art_manual {
            return Ok(Some(format!("luminous-art://{}", manual)));
        }

        if let Some(ref auto) = art_automatic {
            // If it's a cached filename (starts with album-), serve via custom protocol
            if auto.starts_with("album-") {
                return Ok(Some(format!("luminous-art://{}", auto)));
            } else {
                // If it's an absolute local path (folder art), serve via luminous-art://local/
                return Ok(Some(format!("luminous-art://local/{}", auto)));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_album_hash_distinguishes_same_artist_by_second_key() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_covermanager_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let manager = CoverManager::new(db, temp_dir.clone());

        // Loose singles (no album tag) key their cover cache on title instead
        // of an empty album string (#106) — two singles by the same artist
        // must not collide onto the same cached filename.
        let hash_a = manager.get_album_hash("Eric Soltys", "You Wreck Me");
        let hash_b = manager.get_album_hash("Eric Soltys", "Wildflowers");
        assert_ne!(hash_a, hash_b);

        // Same inputs are still stable/idempotent across scans.
        assert_eq!(
            hash_a,
            manager.get_album_hash("Eric Soltys", "You Wreck Me")
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// Regression test for the #362 follow-up: a song with no artist/album
    /// tags must be marked `art_unset` on the very first remote-fetch
    /// attempt (not left perpetually "not yet tried"), so `CoverArt.svelte`
    /// remounting the row on every navigation doesn't re-trigger this call
    /// forever — and, separately, once `art_unset` is set, a second call
    /// must short-circuit before doing any network I/O.
    #[tokio::test]
    async fn test_fetch_remote_cover_marks_untagged_song_unset_without_network() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_covermanager_untagged_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        {
            let conn = db.pool.get().unwrap();
            crate::collection::upsert_song(
                &conn,
                &crate::models::Song {
                    artist: None,
                    album: None,
                    album_artist: None,
                    title: None,
                    source: crate::models::SongSource::LocalFile,
                    path: Some(r"C:\Music\untagged.ogg".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let song_id: i64 = {
            let conn = db.pool.get().unwrap();
            conn.query_row(
                "SELECT id FROM songs WHERE path = ?1",
                params![r"C:\Music\untagged.ogg"],
                |r| r.get(0),
            )
            .unwrap()
        };

        let manager = CoverManager::new(db.clone(), temp_dir.clone());

        let result = manager.fetch_remote_cover(song_id).await.unwrap();
        assert_eq!(result, None);

        let art_unset: bool = {
            let conn = db.pool.get().unwrap();
            conn.query_row(
                "SELECT art_unset FROM songs WHERE id = ?1",
                params![song_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert!(
            art_unset,
            "untagged song should be marked art_unset after the first fetch attempt"
        );

        // Second call must short-circuit on the art_unset check before ever
        // touching the network — if it didn't, this would hang/fail in a
        // sandboxed test environment with no network access.
        let result2 = manager.fetch_remote_cover(song_id).await.unwrap();
        assert_eq!(result2, None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_image_format_and_clean_png() {
        let raw_png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR";
        let (data, mime, ext) = detect_image_format_and_clean(raw_png);
        assert_eq!(data, raw_png);
        assert_eq!(mime, "image/png");
        assert_eq!(ext, "png");
    }

    #[test]
    fn test_detect_image_format_and_clean_jpeg() {
        let raw_jpeg = b"\xFF\xD8\xFF\xE0\x00\x10JFIF";
        let (data, mime, ext) = detect_image_format_and_clean(raw_jpeg);
        assert_eq!(data, raw_jpeg);
        assert_eq!(mime, "image/jpeg");
        assert_eq!(ext, "jpg");
    }

    #[test]
    fn test_detect_image_format_and_clean_prepended_extraneous_bytes_jpeg() {
        // Simulated ID3 tag junk or extraneous metadata prepended to a JPEG image
        let mut junk = vec![0x00; 23712];
        let jpeg_data = b"\xFF\xD8\xFF\xE2\x00\x10MPF";
        junk.extend_from_slice(jpeg_data);

        let (cleaned, mime, ext) = detect_image_format_and_clean(&junk);
        assert_eq!(cleaned, jpeg_data);
        assert_eq!(mime, "image/jpeg");
        assert_eq!(ext, "jpg");
    }

    #[test]
    fn test_detect_image_format_and_clean_webp() {
        let raw_webp = b"RIFF\x00\x00\x00\x00WEBPVP8 ";
        let (data, mime, ext) = detect_image_format_and_clean(raw_webp);
        assert_eq!(data, raw_webp);
        assert_eq!(mime, "image/webp");
        assert_eq!(ext, "webp");
    }

    #[test]
    fn test_scan_folder_art_supports_webp() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_cover_webp_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);

        let audio_path = temp_dir.join("song.mp3");
        let cover_path = temp_dir.join("cover.webp");
        std::fs::write(&audio_path, b"fake audio").unwrap();
        std::fs::write(&cover_path, b"RIFF....WEBPVP8 ").unwrap();

        let found = CoverManager::scan_folder_art_static(&audio_path);
        assert!(found.is_some());
        assert_eq!(found.unwrap().extension().unwrap(), "webp");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
