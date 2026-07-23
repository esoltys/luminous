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

impl CoverManager {
    pub fn new(db: Arc<Database>, app_data_dir: PathBuf) -> Self {
        let covers_dir = app_data_dir.join("covers");
        if !covers_dir.exists() {
            let _ = std::fs::create_dir_all(&covers_dir);
        }
        Self { db, covers_dir }
    }

    /// Helper to hash artist + album to generate a unique cover filename
    pub fn get_album_hash(&self, album_artist: &str, album: &str) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        let combined = format!("{}:{}", album_artist.to_lowercase(), album.to_lowercase());
        for &byte in combined.as_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3u64);
        }
        format!("album-{:016x}", hash)
    }

    /// Extract embedded picture from audio file tags and save it to the covers cache directory
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

        let tag = match tagged_file.primary_tag() {
            Some(t) => t,
            None => return Ok(None),
        };

        let picture = match tag.pictures().first() {
            Some(p) => p,
            None => return Ok(None),
        };

        // Determine extension based on mime type
        let ext = match picture.mime_type() {
            Some(lofty::picture::MimeType::Png) => "png",
            _ => "jpg",
        };

        let hash_name = self.get_album_hash(album_artist, album);
        let filename = format!("{}.{}", hash_name, ext);
        let dest_path = self.covers_dir.join(&filename);

        // Write the picture data to the covers directory
        std::fs::write(&dest_path, picture.data())
            .context("failed to write cover art file to cache")?;

        log::info!("Extracted embedded cover art to: {}", dest_path.display());
        Ok(Some(filename))
    }

    /// Scan directory containing the song for common image names (cover.jpg, folder.png, etc.)
    pub fn scan_folder_art(&self, audio_path: &Path) -> Option<PathBuf> {
        Self::scan_folder_art_static(audio_path)
    }

    /// Static version of scan_folder_art that does not require a CoverManager instance
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
        let common_extensions = ["jpg", "jpeg", "png"];

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

    /// Fetch cover art from iTunes Search API and cache it to covers directory
    pub async fn fetch_remote_cover(&self, song_id: i64) -> Result<Option<String>> {
        let conn = self.db.pool.get()?;
        let (artist, album, album_artist) = conn.query_row(
            "SELECT artist, album, album_artist FROM songs WHERE id = ?1",
            params![song_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )?;

        let artist_query = album_artist.as_ref().or(artist.as_ref());
        let (query_artist, query_album) = match (artist_query, album.as_ref()) {
            (Some(art), Some(alb)) => (art, alb),
            _ => return Ok(None),
        };

        log::info!(
            "Fetching remote cover art for: {} - {}",
            query_artist,
            query_album
        );
        let client = reqwest::Client::new();
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
                    let ext = if url_600.contains(".png") {
                        "png"
                    } else {
                        "jpg"
                    };
                    let hash_name = self.get_album_hash(query_artist, query_album);
                    let filename = format!("{}.{}", hash_name, ext);
                    let dest_path = self.covers_dir.join(&filename);

                    std::fs::write(&dest_path, img_bytes)?;
                    log::info!("Saved remote cover art to: {}", dest_path.display());

                    // Update song database row
                    conn.execute(
                        "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE id = ?2",
                        params![filename, song_id],
                    )?;

                    return Ok(Some(filename));
                }
            }
        }

        // If no artwork found, mark it as unset so we don't spam requests
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

    /// Resolve URI string (luminous-art://...) for a given song ID
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
}
