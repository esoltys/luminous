//! Cover art acquisition, caching, and lookup.
//!
//! Art comes from three sources, tried in this order by the collection
//! scanner: image files sitting next to the song (`scan_folder_art`),
//! embedded tag pictures (`extract_embedded_art`), then an iTunes Search API
//! fallback (`fetch_remote_cover`). Folder art goes first because it's used
//! in place, while embedded art costs a copy in the cache — so a library
//! that has both doesn't duplicate every cover. Albumless singles are the
//! exception: they check embedded art first, since a shared `cover.jpg` in
//! a singles folder isn't any one single's cover. Whichever source succeeds
//! writes into `songs.art_automatic`; a user-picked cover instead goes in
//! `art_manual` and always takes precedence (see `get_cover_art_path`/
//! `get_cover_art_uri`). Extracted/downloaded images are cached as files
//! under `covers_dir`, keyed by `get_album_hash`.

use crate::db::Database;
use anyhow::{Context, Result};
use lofty::{file::TaggedFileExt, picture::PictureType, probe::Probe};
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Position in the Standardized Artwork Hierarchy (#98). Declaration order is
/// significant: `derive(PartialOrd, Ord)` sorts variants in this order, which
/// is exactly the discovery priority the issue specifies — primary cover
/// first, unnamed subfolder finds last. Embedded tag pictures are folded into
/// the same ordering via `category_for_picture_type` so a `CoverFront` image
/// embedded in the file and a `cover.jpg` sitting next to it rank the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArtworkCategory {
    PrimaryCover,
    BackCover,
    DiscMedia,
    Booklet,
    Matrix,
    ArtistPortrait,
    BandLogo,
    FanartBanner,
    /// Catch-all for files found in a nested artwork subfolder (`Artwork/`,
    /// `Scans/`, etc.) whose name doesn't match any of the categories above.
    Subfolder,
}

impl ArtworkCategory {
    /// Stable snake_case key used across the IPC boundary (#758) — the
    /// frontend's `ExtendedArtworkCategory` union type (#759) mirrors these
    /// exact values, so don't rename a variant here without updating there.
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtworkCategory::PrimaryCover => "primary_cover",
            ArtworkCategory::BackCover => "back_cover",
            ArtworkCategory::DiscMedia => "disc_media",
            ArtworkCategory::Booklet => "booklet",
            ArtworkCategory::Matrix => "matrix",
            ArtworkCategory::ArtistPortrait => "artist_portrait",
            ArtworkCategory::BandLogo => "band_logo",
            ArtworkCategory::FanartBanner => "fanart_banner",
            ArtworkCategory::Subfolder => "subfolder",
        }
    }

    /// True for categories that belong to an album (primary cover, back cover,
    /// disc media, booklet, matrix, or unnamed subfolder finds). Excludes
    /// parent artist-level categories (portrait, band logo, fanart banner).
    pub fn is_album_level(&self) -> bool {
        matches!(
            self,
            ArtworkCategory::PrimaryCover
                | ArtworkCategory::BackCover
                | ArtworkCategory::DiscMedia
                | ArtworkCategory::Booklet
                | ArtworkCategory::Matrix
                | ArtworkCategory::Subfolder
        )
    }

    /// True for categories that belong to an artist (artist portrait,
    /// band logo, fanart/backdrop banner).
    pub fn is_artist_level(&self) -> bool {
        matches!(
            self,
            ArtworkCategory::ArtistPortrait
                | ArtworkCategory::BandLogo
                | ArtworkCategory::FanartBanner
        )
    }
}

/// One discovered artwork file, categorized and ready to sort by
/// `ArtworkCategory` (primary key) then filename (tiebreaker) — see
/// `ExtendedArtworkSet::sorted`.
#[derive(Debug, Clone)]
pub struct ArtworkEntry {
    pub category: ArtworkCategory,
    pub path: PathBuf,
}

/// Result of a hierarchical local-filesystem artwork scan for one song, built
/// by `scan_extended_artwork`. Purely a discovery/ordering result — callers
/// decide what to do with the paths (cache, expose via IPC, etc.).
#[derive(Debug, Clone, Default)]
pub struct ExtendedArtworkSet {
    pub entries: Vec<ArtworkEntry>,
}

impl ExtendedArtworkSet {
    /// Entries ordered by the Standardized Artwork Hierarchy (category, then
    /// filename as a tiebreaker) — not flat alphabetical, which would put
    /// e.g. `booklet.jpg` ahead of `cover.jpg`.
    pub fn sorted(mut self) -> Self {
        self.entries.sort_by(|a, b| {
            a.category.cmp(&b.category).then_with(|| {
                let a_name = a
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase());
                let b_name = b
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase());
                a_name.cmp(&b_name)
            })
        });
        self
    }

    /// The top-ranked entry, if any — the album cover-stack's thumbnail and
    /// the file the "Open Images" action opens first (#760).
    pub fn primary(&self) -> Option<&ArtworkEntry> {
        self.entries.first()
    }
}

const PRIMARY_COVER_NAMES: &[&str] = &["cover", "folder", "front", "album", "albumart"];
const BACK_COVER_NAMES: &[&str] = &["back", "rear", "backcover"];
const DISC_MEDIA_NAMES: &[&str] = &["disc", "cd", "discart", "medium"];
const BOOKLET_NAMES: &[&str] = &["booklet", "insert", "inlay", "liner"];
const MATRIX_NAMES: &[&str] = &["tray", "matrix", "spine"];
const ARTIST_PORTRAIT_NAMES: &[&str] = &["artist", "folder", "thumb", "photo"];
const BAND_LOGO_NAMES: &[&str] = &["logo", "clearlogo"];
const FANART_NAMES: &[&str] = &["fanart", "backdrop", "background", "banner"];
const SUBFOLDER_DIR_NAMES: &[&str] = &["artwork", "art", "scans", "extrafanart"];
const EXTENDED_ARTWORK_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

/// Categorize a same-album-directory (or subfolder) filename stem against the
/// album-level categories (Primary Cover, Back Cover, Disc/Vinyl Media,
/// Booklet & Inserts, Matrix/Tray), plus an exact `{AlbumName}.*` match for
/// Primary Cover. Returns `None` for names that don't match any album-level
/// category (artist/band/fanart names are matched separately by
/// `categorize_artist_folder_name`, since they only apply in the parent
/// artist directory).
fn categorize_album_art_name(
    stem_lower: &str,
    album_name: Option<&str>,
) -> Option<ArtworkCategory> {
    if PRIMARY_COVER_NAMES.contains(&stem_lower) {
        return Some(ArtworkCategory::PrimaryCover);
    }
    if let Some(album) = album_name {
        let album_lower = album.trim().to_lowercase();
        if !album_lower.is_empty() && stem_lower == album_lower {
            return Some(ArtworkCategory::PrimaryCover);
        }
    }
    if BACK_COVER_NAMES.contains(&stem_lower) {
        return Some(ArtworkCategory::BackCover);
    }
    if DISC_MEDIA_NAMES.contains(&stem_lower) {
        return Some(ArtworkCategory::DiscMedia);
    }
    if BOOKLET_NAMES.contains(&stem_lower) {
        return Some(ArtworkCategory::Booklet);
    }
    if MATRIX_NAMES.contains(&stem_lower) {
        return Some(ArtworkCategory::Matrix);
    }
    None
}

/// Categorize a filename stem found in the *parent* (artist-level) directory
/// against the artist/band categories (Artist Portraits, Band Logos,
/// Background & Hero Banners). Returns `None` for names that don't match.
fn categorize_artist_folder_name(stem_lower: &str) -> Option<ArtworkCategory> {
    if ARTIST_PORTRAIT_NAMES.contains(&stem_lower) {
        return Some(ArtworkCategory::ArtistPortrait);
    }
    if BAND_LOGO_NAMES.contains(&stem_lower) {
        return Some(ArtworkCategory::BandLogo);
    }
    if FANART_NAMES.contains(&stem_lower) {
        return Some(ArtworkCategory::FanartBanner);
    }
    None
}

fn has_extended_artwork_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| EXTENDED_ARTWORK_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Windows Media Player's album-art cache: alongside `Folder.jpg` it writes
/// `AlbumArtSmall.jpg` and `AlbumArt_{GUID}_Large.jpg`/`_Small.jpg`, which are
/// resized copies of the same cover. Counting them would show one cover as
/// four images, so the extended-artwork scan skips them and keeps `Folder.jpg`.
fn is_wmp_album_art_thumbnail(stem_lower: &str) -> bool {
    stem_lower == "albumartsmall"
        || (stem_lower.starts_with("albumart_{")
            && (stem_lower.ends_with("}_large") || stem_lower.ends_with("}_small")))
}

/// A file the extended-artwork scan should consider: a supported image
/// extension, and not one of WMP's derived thumbnails.
fn is_extended_artwork_candidate(path: &Path) -> bool {
    path.is_file()
        && has_extended_artwork_extension(path)
        && !path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|stem| is_wmp_album_art_thumbnail(&stem.to_lowercase()))
}

/// Map a lofty embedded-picture `PictureType` onto the same hierarchy used
/// for filesystem finds, so an embedded `CoverFront` picture and a
/// `cover.jpg` on disk sort identically. Unmapped/`Other` types fall back to
/// `Subfolder`, the lowest-priority category, per #98's ordering rules.
pub fn category_for_picture_type(picture_type: PictureType) -> ArtworkCategory {
    match picture_type {
        PictureType::CoverFront => ArtworkCategory::PrimaryCover,
        PictureType::CoverBack => ArtworkCategory::BackCover,
        PictureType::Leaflet => ArtworkCategory::Booklet,
        PictureType::Media => ArtworkCategory::DiscMedia,
        PictureType::Artist | PictureType::LeadArtist => ArtworkCategory::ArtistPortrait,
        PictureType::Band => ArtworkCategory::ArtistPortrait,
        PictureType::BandLogo => ArtworkCategory::BandLogo,
        _ => ArtworkCategory::Subfolder,
    }
}

/// Scan one directory (non-recursively) for files matching `categorize`,
/// pushing a categorized `ArtworkEntry` for each match into `out`.
fn scan_dir_for_category(
    dir: &Path,
    categorize: impl Fn(&str) -> Option<ArtworkCategory>,
    out: &mut Vec<ArtworkEntry>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !is_extended_artwork_candidate(&path) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Some(category) = categorize(stem.to_lowercase().as_str()) {
            out.push(ArtworkEntry { category, path });
        }
    }
}

/// Like `scan_dir_for_category`, but a file that doesn't match a named
/// category still gets included as `Subfolder` fallback instead of being
/// silently dropped — for a directory whose *entire* contents are known to
/// be artwork scans (the album directory itself, or a nested artwork
/// subfolder), a compound or numbered filename (`Booklet 1.jpg`,
/// `Front + OBI.jpg`) is still legitimate album artwork, just unranked
/// (#855).
fn scan_dir_for_category_with_fallback(
    dir: &Path,
    categorize: impl Fn(&str) -> Option<ArtworkCategory>,
    out: &mut Vec<ArtworkEntry>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !is_extended_artwork_candidate(&path) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let category =
            categorize(stem.to_lowercase().as_str()).unwrap_or(ArtworkCategory::Subfolder);
        out.push(ArtworkEntry { category, path });
    }
}

/// Scan a nested artwork subfolder (`Artwork/`, `Scans/`, etc.): files whose
/// name matches an album-level or artist-level category keep that category;
/// everything else with a supported extension falls back to `Subfolder`, per
/// #98's "Common Subfolder Conventions" — content is still discoverable, it
/// just sorts after the named categories.
fn scan_subfolder(dir: &Path, album_name: Option<&str>, out: &mut Vec<ArtworkEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !is_extended_artwork_candidate(&path) {
            continue;
        }
        let stem_lower = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        let category = categorize_album_art_name(&stem_lower, album_name)
            .or_else(|| categorize_artist_folder_name(&stem_lower))
            .unwrap_or(ArtworkCategory::Subfolder);
        out.push(ArtworkEntry { category, path });
    }
}

/// Full hierarchical artwork scan for a song at `audio_path`, per #98's
/// Standardized Artwork Hierarchy: the album directory (primary/secondary
/// categories, including an exact `{album_name}.*` match), its artwork
/// subfolders (`Artwork/`, `Art/`, `Scans/`, `extrafanart/`), and the parent
/// (artist-level) directory (artist portraits, band logos, fanart/backdrop
/// banners). Purely a discovery pass — doesn't touch the DB or copy/cache
/// anything; call `.sorted()` on the result for hierarchy order.
pub fn scan_extended_artwork(audio_path: &Path, album_name: Option<&str>) -> ExtendedArtworkSet {
    let mut entries = Vec::new();

    let Some(album_dir) = audio_path.parent() else {
        return ExtendedArtworkSet { entries };
    };

    scan_dir_for_category_with_fallback(
        album_dir,
        |stem| categorize_album_art_name(stem, album_name),
        &mut entries,
    );

    if let Ok(dir_entries) = std::fs::read_dir(album_dir) {
        for entry in dir_entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if SUBFOLDER_DIR_NAMES.contains(&name.to_lowercase().as_str()) {
                scan_subfolder(&path, album_name, &mut entries);
            }
        }
    }

    if let Some(artist_dir) = album_dir.parent() {
        scan_dir_for_category(artist_dir, categorize_artist_folder_name, &mut entries);
    }

    ExtendedArtworkSet { entries }
}

/// Build a `luminous-art://local/` webview URI for an absolute filesystem
/// path discovered by `scan_extended_artwork` — same convention the existing
/// single-cover folder-art path uses (see `get_cover_art_uri`'s
/// `local/` branch), so the frontend `<img>` tag handling doesn't need a
/// second code path for extended artwork.
pub fn local_artwork_uri(path: &Path) -> String {
    format!("luminous-art://local/{}", path.to_string_lossy())
}

/// Serves a `luminous-art://` request: `local/<percent-encoded absolute path>`
/// for folder art used in place, anything else a filename in `covers_dir`.
/// Blocking file I/O — the protocol handler in `lib.rs` runs it on the
/// blocking pool, never on the UI thread.
pub fn serve_art_request(covers_dir: &Path, uri: &str) -> tauri::http::Response<Vec<u8>> {
    let mut trimmed = uri;
    // On Windows WebView2, requests are made to `http://luminous-art.localhost/`
    // via the frontend rewrite in `getCoverArtUrl()`. wry intercepts the HTTP request
    // and runs `revert_uri_work_around` which rewrites the URI to `luminous-art://localhost/`
    // before calling this handler (see #715). We strip either prefix here.
    if let Some(t) = uri.strip_prefix("http://luminous-art.localhost/") {
        trimmed = t;
    } else if let Some(t) = uri.strip_prefix("luminous-art://") {
        trimmed = t;
    }

    // If the webview prepends localhost/ to the authority, strip it
    if trimmed.starts_with("localhost/") {
        trimmed = trimmed.strip_prefix("localhost/").unwrap_or(trimmed);
    }

    // Webviews normalize empty paths to trailing slashes (e.g. URI/ -> path/)
    trimmed = trimmed.trim_end_matches('/');

    if let Some(rest) = trimmed.strip_prefix("embedded/") {
        return serve_embedded_art(covers_dir, rest);
    }

    let file_path = if trimmed.starts_with("local/") {
        let local_path = trimmed.strip_prefix("local/").unwrap_or(trimmed);
        let decoded = percent_encoding::percent_decode_str(local_path)
            .decode_utf8_lossy()
            .into_owned();
        std::path::PathBuf::from(decoded)
    } else {
        let decoded = percent_encoding::percent_decode_str(trimmed)
            .decode_utf8_lossy()
            .into_owned();
        covers_dir.join(decoded)
    };

    log::trace!(
        "Custom protocol: URI = {}, Resolved path = {:?} (exists: {})",
        uri,
        file_path,
        file_path.exists()
    );

    serve_image_file(&file_path)
}

fn serve_image_file(file_path: &Path) -> tauri::http::Response<Vec<u8>> {
    if file_path.exists() && file_path.is_file() {
        if let Ok(data) = std::fs::read(file_path) {
            image_response(&data)
        } else {
            empty_response(500)
        }
    } else {
        empty_response(404)
    }
}

fn image_response(data: &[u8]) -> tauri::http::Response<Vec<u8>> {
    let (cleaned_data, mime, _) = detect_image_format_and_clean(data);
    tauri::http::Response::builder()
        .status(200)
        .header("content-type", mime)
        .header("access-control-allow-origin", "*")
        .body(cleaned_data.to_vec())
        .unwrap()
}

fn empty_response(status: u16) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header("access-control-allow-origin", "*")
        .body(Vec::new())
        .unwrap()
}

/// Build the `luminous-art://embedded/` URI that serves a song's
/// full-resolution embedded picture (#1272): `cache_file` is the downscaled
/// covers-cache copy served instead if the audio file can't be read (moved,
/// drive asleep), and `audio_path` is percent-encoded whole — slashes
/// included — so it stays one path segment after the cache filename.
pub fn embedded_art_uri(cache_file: &str, audio_path: &str) -> String {
    let encoded =
        percent_encoding::utf8_percent_encode(audio_path, percent_encoding::NON_ALPHANUMERIC);
    format!("luminous-art://embedded/{cache_file}/{encoded}")
}

/// Serve the `embedded/<cache file>/<encoded audio path>` form built by
/// `embedded_art_uri`: the audio file's top-ranked embedded picture at full
/// resolution, read on demand rather than cached, falling back to the
/// downscaled cache file.
fn serve_embedded_art(covers_dir: &Path, rest: &str) -> tauri::http::Response<Vec<u8>> {
    let Some((cache_file, encoded_path)) = rest.split_once('/') else {
        return empty_response(404);
    };
    let audio_path = percent_encoding::percent_decode_str(encoded_path)
        .decode_utf8_lossy()
        .into_owned();
    match CoverManager::extract_all_embedded_pictures(Path::new(&audio_path)) {
        Ok(pictures) => {
            if let Some((_category, data)) = pictures.into_iter().min_by_key(|(c, _)| *c) {
                return image_response(&data);
            }
        }
        Err(e) => log::debug!("Full-resolution embedded art unavailable for {audio_path}: {e}"),
    }
    let cache_file = percent_encoding::percent_decode_str(cache_file)
        .decode_utf8_lossy()
        .into_owned();
    // Only a bare cache filename — never let the fallback segment walk out
    // of `covers_dir`.
    if cache_file.contains(['/', '\\']) || cache_file.contains("..") {
        return empty_response(404);
    }
    serve_image_file(&covers_dir.join(cache_file))
}

#[derive(Debug)]
pub struct CoverManager {
    db: Arc<Database>,
    covers_dir: PathBuf,
    itunes_base_url: String,
    /// Album hash -> cache filename for embedded art already extracted by
    /// this instance. `None` unless `with_per_scan_album_dedup` enabled it:
    /// only a scan's short-lived manager may skip re-extraction, since the
    /// long-lived watcher/app managers must pick up art changed by a retag.
    extracted_albums: Option<parking_lot::Mutex<std::collections::HashMap<String, String>>>,
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

/// Longest edge, in pixels, of cover art written to the covers cache (#1272).
/// Matches the 600x600 the iTunes fallback already downloads; the large
/// "Now Playing" view reads the full-resolution picture straight from the
/// audio file instead (see `get_full_resolution_cover_art_uri`).
pub const CACHE_MAX_EDGE: u32 = 600;
const CACHE_JPEG_QUALITY: u8 = 85;

/// The bytes and extension to write to the covers cache for image `data`:
/// decoded, shrunk to fit `CACHE_MAX_EDGE`, and re-encoded as JPEG (PNG when
/// the image has an alpha channel). Images already within the bound are
/// cached as-is, so no generation loss. Anything the `image` crate can't
/// decode is cached as-is too — a cache entry the webview may still manage
/// to render beats none at all.
pub fn downscale_for_cache(data: &[u8]) -> (Vec<u8>, &'static str) {
    let (cleaned, _mime, ext) = detect_image_format_and_clean(data);
    let Ok(img) = image::load_from_memory(cleaned) else {
        return (cleaned.to_vec(), ext);
    };
    if img.width() <= CACHE_MAX_EDGE && img.height() <= CACHE_MAX_EDGE {
        return (cleaned.to_vec(), ext);
    }
    let format = if img.color().has_alpha() {
        image::ImageFormat::Png
    } else {
        image::ImageFormat::Jpeg
    };
    match encode_downscaled(&img, format) {
        Some(encoded) if encoded.len() < cleaned.len() => (encoded, cache_ext_for(format)),
        _ => (cleaned.to_vec(), ext),
    }
}

fn cache_ext_for(format: image::ImageFormat) -> &'static str {
    if format == image::ImageFormat::Png {
        "png"
    } else {
        "jpg"
    }
}

/// Resize `img` to fit `CACHE_MAX_EDGE` (keeping its aspect ratio) and encode
/// it as `format` — JPEG at `CACHE_JPEG_QUALITY`, or PNG.
fn encode_downscaled(img: &image::DynamicImage, format: image::ImageFormat) -> Option<Vec<u8>> {
    let resized = img.resize(
        CACHE_MAX_EDGE,
        CACHE_MAX_EDGE,
        image::imageops::FilterType::CatmullRom,
    );
    let mut out = Vec::new();
    if format == image::ImageFormat::Png {
        resized
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .ok()?;
    } else {
        let encoder =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, CACHE_JPEG_QUALITY);
        resized.to_rgb8().write_with_encoder(encoder).ok()?;
    }
    Some(out)
}

/// Shrink an existing oversized cache file in place, keeping its filename —
/// and so its format, since the filename is what `songs.art_automatic`
/// stores. Returns the bytes saved, or `None` when the file is already within
/// `CACHE_MAX_EDGE`, isn't a JPEG/PNG, or re-encoding wouldn't make it
/// smaller. Only the image header is read for files that are already small,
/// so sweeping an already-downscaled cache stays cheap.
fn recompress_cache_file(path: &Path) -> Option<u64> {
    let format = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => image::ImageFormat::Jpeg,
        "png" => image::ImageFormat::Png,
        _ => return None,
    };
    let (width, height) = image::image_dimensions(path).ok()?;
    if width <= CACHE_MAX_EDGE && height <= CACHE_MAX_EDGE {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&data).ok()?;
    let encoded = encode_downscaled(&img, format)?;
    if encoded.len() >= data.len() {
        return None;
    }
    // Write-then-rename so a crash mid-write never leaves a truncated cover
    // behind. A leftover `.tmp` is unreferenced, so the next sweep prunes it.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &encoded).ok()?;
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    Some((data.len() - encoded.len()) as u64)
}

/// Outcome of `CoverManager::sweep_cache`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CacheSweepResult {
    pub pruned: usize,
    pub recompressed: usize,
    pub bytes_reclaimed: u64,
}

/// Bytes the covers cache occupies on disk, split by what the files are:
/// `album-*` cover art vs. everything else (artist photos, band logos and
/// header banners cached under `artist-*`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CacheUsage {
    pub album_art_bytes: u64,
    pub artist_art_bytes: u64,
}

impl CoverManager {
    pub fn new(db: Arc<Database>, app_data_dir: PathBuf) -> Self {
        let covers_dir = app_data_dir.join("covers");
        if !covers_dir.exists() {
            let _ = std::fs::create_dir_all(&covers_dir);
        }
        Self {
            db,
            covers_dir,
            itunes_base_url: "https://itunes.apple.com".to_string(),
            extracted_albums: None,
        }
    }

    /// Extract each album's embedded art at most once for this manager's
    /// lifetime, instead of re-reading and re-writing the same cache file for
    /// every track. For managers scoped to a single library scan only.
    pub fn with_per_scan_album_dedup(mut self) -> Self {
        self.extracted_albums = Some(parking_lot::Mutex::new(std::collections::HashMap::new()));
        self
    }

    /// Override the iTunes Search API base URL — used by BDD tests to point
    /// `fetch_remote_cover` at a local mock server instead of the real
    /// network, so the remote-cover-art fallback can be exercised
    /// deterministically and offline (see `tests/cover_art_bdd.rs`).
    /// Production code never calls this; `new()`'s default is the real API.
    pub fn with_itunes_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.itunes_base_url = base_url.into();
        self
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

    /// Same FNV-1a hashing as `get_album_hash`, keyed on an artist's
    /// MusicBrainz ID rather than artist/album name, for a fetched artist
    /// image's cache filename stem (#1127). `artist-`-prefixed so it can
    /// never collide with an `album-*` cache filename in the same
    /// `covers_dir`.
    pub fn get_artist_image_hash(&self, artist_mbid: &str) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        for &byte in artist_mbid.to_lowercase().as_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3u64);
        }
        format!("artist-{:016x}", hash)
    }

    /// The on-disk directory cached cover art (and, since #1127, fetched
    /// artist images) is stored under — exposed so other modules can cache
    /// into the same directory the `luminous-art://` protocol handler
    /// resolves non-`local/` filenames against, without duplicating that
    /// directory-resolution logic.
    pub fn covers_dir(&self) -> &Path {
        &self.covers_dir
    }

    /// Save the file's best embedded tag picture (if any) to the covers
    /// cache and return its cache filename. Returns `Ok(None)` — not an
    /// error — when the file has no tag or the tag has no picture; callers
    /// are expected to fall through to `scan_folder_art`/`fetch_remote_cover`
    /// in that case.
    ///
    /// "Best" ranks every picture in every tag by `category_for_picture_type`
    /// and keeps the top-ranked one (Primary Cover first) — not whichever
    /// picture lofty happened to store first, which previously meant a
    /// `CoverFront` image could silently lose to an unrelated `Other`-typed
    /// picture stored earlier in the tag (#98/#757). Callers that need every
    /// embedded picture, not just the winner, should use
    /// `extract_all_embedded_pictures` instead.
    pub fn extract_embedded_art(
        &self,
        audio_path: &Path,
        album_artist: &str,
        album: &str,
    ) -> Result<Option<String>> {
        let hash_name = self.get_album_hash(album_artist, album);
        if let Some(extracted) = &self.extracted_albums {
            if let Some(filename) = extracted.lock().get(&hash_name) {
                return Ok(Some(filename.clone()));
            }
        }

        let pictures = Self::extract_all_embedded_pictures(audio_path)?;
        let Some((_category, raw_data)) = pictures.into_iter().min_by_key(|(c, _)| *c) else {
            return Ok(None);
        };

        let (cache_data, ext) = downscale_for_cache(&raw_data);

        let filename = format!("{}.{}", hash_name, ext);
        let dest_path = self.covers_dir.join(&filename);

        std::fs::write(&dest_path, cache_data)
            .context("failed to write cover art file to cache")?;

        log::info!("Extracted embedded cover art to: {}", dest_path.display());
        if let Some(extracted) = &self.extracted_albums {
            extracted.lock().insert(hash_name, filename.clone());
        }
        Ok(Some(filename))
    }

    /// Enumerate every embedded picture across every tag on the file — not
    /// just the first one lofty happened to store — paired with the
    /// `ArtworkCategory` its `PictureType` maps to. Read-only: doesn't write
    /// anything to disk. Used by `extract_embedded_art` to pick the
    /// best-ranked picture, and available directly for consumers that want
    /// the full set (e.g. a later "extended artwork" IPC command).
    pub fn extract_all_embedded_pictures(
        audio_path: &Path,
    ) -> Result<Vec<(ArtworkCategory, Vec<u8>)>> {
        let tagged_file = Probe::open(audio_path)
            .context("failed to open audio file for cover extraction")?
            .read()
            .context("failed to read audio file tags")?;

        let mut pictures = Vec::new();
        for tag in tagged_file.tags() {
            for picture in tag.pictures() {
                let category = category_for_picture_type(picture.pic_type());
                pictures.push((category, picture.data().to_vec()));
            }
        }
        Ok(pictures)
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

        if let Ok(entries) = std::fs::read_dir(parent_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    let stem = path.file_stem().and_then(|s| s.to_str());
                    let ext = path.extension().and_then(|e| e.to_str());
                    if let (Some(stem), Some(ext)) = (stem, ext) {
                        if Self::is_folder_art_filename(stem, ext) {
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
        None
    }

    /// Whether a file's stem/extension match the standalone-folder-art
    /// naming convention (`cover.jpg`, `folder.png`, etc.) — shared by
    /// `scan_folder_art_static` (local filesystem scan) and the WebDAV sync
    /// loop (`commands::webdav::sync_webdav_server_inner`, #1082), which has
    /// no filesystem to `read_dir` but gets the same filenames from a
    /// directory's PROPFIND listing.
    pub fn is_folder_art_filename(stem: &str, extension: &str) -> bool {
        const COMMON_NAMES: [&str; 8] = [
            "cover",
            "folder",
            "album",
            "front",
            "artwork",
            "cover-art",
            "album-art",
            "folder-art",
        ];
        const COMMON_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "gif", "bmp"];

        COMMON_NAMES.contains(&stem.to_lowercase().as_str())
            && COMMON_EXTENSIONS.contains(&extension.to_lowercase().as_str())
    }

    /// Cleans/detects `data`'s image format and caches it under `covers_dir`
    /// keyed by `get_album_hash(album_artist, album)`, exactly like
    /// `extract_embedded_art`'s tail — but for bytes that didn't come from
    /// an embedded tag picture (e.g. a WebDAV folder-art image downloaded
    /// over HTTP, #1082). Returns the cache filename to store in
    /// `songs.art_automatic`.
    pub fn cache_art_bytes(&self, album_artist: &str, album: &str, data: &[u8]) -> Result<String> {
        let (cache_data, ext) = downscale_for_cache(data);
        let hash_name = self.get_album_hash(album_artist, album);
        let filename = format!("{}.{}", hash_name, ext);
        let dest_path = self.covers_dir.join(&filename);

        std::fs::write(&dest_path, cache_data)
            .context("failed to write cover art file to cache")?;

        Ok(filename)
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
            "{}/search?term={}&entity=album&limit=1",
            self.itunes_base_url,
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
                    let (cache_bytes, ext) = downscale_for_cache(&img_bytes);
                    let hash_name = self.get_album_hash(query_artist, query_album);
                    let filename = format!("{}.{}", hash_name, ext);
                    let dest_path = self.covers_dir.join(&filename);

                    std::fs::write(&dest_path, cache_bytes)?;
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
            // No embedded, folder or iTunes cover: fall back to fanart.tv's.
            let (cover, _) = self.fanart_album_art(song_id)?;
            return Ok(cover.map(|f| self.covers_dir.join(f)));
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
            // No embedded, folder or iTunes cover: fall back to fanart.tv's.
            let (cover, _) = self.fanart_album_art(song_id)?;
            return Ok(cover.map(|f| format!("luminous-art://{f}")));
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

    /// `get_cover_art_uri` for views that show art large (the immersive Now
    /// Playing view, #1272). The covers cache holds only a
    /// `CACHE_MAX_EDGE`-bounded copy, so when that copy came from the song's
    /// own embedded picture this returns an `embedded_art_uri` that reads
    /// the original picture from the audio file on demand. Every other case —
    /// a manual pick, folder art (already served at full size, in place), a
    /// remote cover, a non-local song — gets the same URI as
    /// `get_cover_art_uri`.
    pub fn get_full_resolution_cover_art_uri(&self, song_id: i64) -> Result<Option<String>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT art_embedded, art_automatic, art_manual, art_unset, path,
                    source IN ({lib})
             FROM songs WHERE id = ?1",
            lib = *crate::models::LOCAL_SOURCES_SQL
        );
        let (art_embedded, art_automatic, art_manual, art_unset, path, is_local) =
            conn.query_row(&sql, params![song_id], |row| {
                Ok((
                    row.get::<_, bool>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            })?;
        drop(conn);

        if !art_unset && art_manual.is_none() && art_embedded && is_local {
            if let (Some(auto), Some(path)) = (art_automatic, path) {
                if auto.starts_with("album-") && !path.contains("://") {
                    return Ok(Some(embedded_art_uri(&auto, &path)));
                }
            }
        }
        self.get_cover_art_uri(song_id)
    }

    /// Sum the on-disk size of the covers cache for the Folders settings'
    /// Disk Size breakdown. A missing cache dir (nothing cached yet) is zero,
    /// not an error.
    pub fn cache_usage(&self) -> CacheUsage {
        let mut usage = CacheUsage::default();
        let Ok(entries) = std::fs::read_dir(&self.covers_dir) else {
            return usage;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            if entry.file_name().to_string_lossy().starts_with("album-") {
                usage.album_art_bytes += meta.len();
            } else {
                usage.artist_art_bytes += meta.len();
            }
        }
        usage
    }

    /// Post-scan covers-cache maintenance (#1272). Deletes `album-*` files no
    /// song references any more (left behind by retags, removed albums, or a
    /// write whose extension changed), and shrinks referenced files cached
    /// before downscaling existed. Unreferenced files younger than
    /// `orphan_grace` are kept: a WebDAV/Subsonic sync or a watcher event may
    /// have just written one and not yet stored its filename on the row.
    /// `artist-*` images and anything else in `covers_dir` are never touched.
    pub fn sweep_cache(&self, orphan_grace: std::time::Duration) -> Result<CacheSweepResult> {
        let referenced: std::collections::HashSet<String> = {
            let conn = self.db.pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT art_automatic FROM songs WHERE art_automatic LIKE 'album-%'
                 UNION
                 SELECT art_manual FROM songs WHERE art_manual LIKE 'album-%'",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        // fanart.tv cover/disc art (#1277) shares the `album-<hash>` stem but
        // is referenced from `album_profiles`, and is a download the user
        // opted into — neither pruned nor re-encoded, like artist images.
        let fanart: std::collections::HashSet<String> = {
            let conn = self.db.pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT fetched_cover_filename FROM album_profiles
                 WHERE fetched_cover_filename IS NOT NULL
                 UNION
                 SELECT fetched_disc_filename FROM album_profiles
                 WHERE fetched_disc_filename IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };

        let mut result = CacheSweepResult::default();
        let now = std::time::SystemTime::now();
        for entry in std::fs::read_dir(&self.covers_dir)?.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if !name.starts_with("album-") || fanart.contains(&name) {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let path = entry.path();
            if referenced.contains(&name) {
                if let Some(saved) = recompress_cache_file(&path) {
                    result.recompressed += 1;
                    result.bytes_reclaimed += saved;
                }
                continue;
            }
            let age = meta
                .modified()
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .unwrap_or_default();
            if age >= orphan_grace && std::fs::remove_file(&path).is_ok() {
                result.pruned += 1;
                result.bytes_reclaimed += meta.len();
            }
        }
        Ok(result)
    }

    /// Cache filenames of the fanart.tv cover and disc art fetched for this
    /// song's album (#1277), each `None` unless fetched and its type is
    /// enabled in Settings › Integrations › fanart.tv, so unchecking a type
    /// hides what was already fetched.
    ///
    /// The cover is only a fallback: the resolvers above consult it only
    /// once the song is `art_unset` (no embedded or folder art, and iTunes
    /// missed), so it never displaces a cover found any other way.
    pub fn fanart_album_art(&self, song_id: i64) -> Result<(Option<String>, Option<String>)> {
        let conn = self.db.pool.get()?;
        let prefs = crate::commands::settings::load_ui_preferences(&conn);
        if !prefs.fanart_fetch_album_cover && !prefs.fanart_fetch_disc_art {
            return Ok((None, None));
        }
        let fetched = conn
            .query_row(
                "SELECT p.fetched_cover_filename, p.fetched_disc_filename
                 FROM songs s
                 JOIN album_profiles p ON p.album_key = s.album COLLATE NOCASE
                 WHERE s.id = ?1",
                params![song_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()?;
        let (cover, disc) = fetched.unwrap_or_default();
        Ok((
            cover.filter(|_| prefs.fanart_fetch_album_cover),
            disc.filter(|_| prefs.fanart_fetch_disc_art),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_album_hash_distinguishes_same_artist_by_second_key() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_covermanager_test_")
            .tempdir()
            .unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();
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
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_covermanager_untagged_test_")
            .tempdir()
            .unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();
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
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_cover_webp_test_")
            .tempdir()
            .unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();
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

    #[test]
    fn test_is_folder_art_filename_matches_common_names_case_insensitively() {
        assert!(CoverManager::is_folder_art_filename("cover", "jpg"));
        assert!(CoverManager::is_folder_art_filename("Album", "PNG"));
        assert!(CoverManager::is_folder_art_filename("folder-art", "webp"));
        assert!(!CoverManager::is_folder_art_filename("cover", "txt"));
        assert!(!CoverManager::is_folder_art_filename("thumbnail", "jpg"));
    }

    /// A WebDAV directory's PROPFIND listing has no filesystem to `read_dir`
    /// (#1082 follow-up), so the sync loop downloads a matched folder-art
    /// image's bytes directly and caches them the same way an embedded tag
    /// picture would be.
    #[test]
    fn test_cache_art_bytes_writes_to_covers_dir_keyed_by_album_hash() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_cover_webdav_art_test_")
            .tempdir()
            .unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let manager = CoverManager::new(db, temp_dir.clone());

        let raw_png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR";
        let filename = manager
            .cache_art_bytes("My NAS Artist", "My NAS Album", raw_png)
            .unwrap();

        assert!(filename.starts_with("album-"));
        assert!(filename.ends_with(".png"));
        assert_eq!(
            filename,
            format!(
                "{}.png",
                manager.get_album_hash("My NAS Artist", "My NAS Album")
            )
        );
        assert!(manager.covers_dir.join(&filename).exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_serve_art_request_resolves_cache_and_local_paths() {
        let temp_dir = tempfile::tempdir().unwrap();
        let covers_dir = temp_dir.path().join("covers");
        std::fs::create_dir_all(&covers_dir).unwrap();
        let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR";
        std::fs::write(covers_dir.join("album-1.png"), png).unwrap();
        let folder = temp_dir.path().join("Def Leppard").join("Hysteria");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Folder.jpg"), b"\xFF\xD8\xFF\xE0").unwrap();

        let cached = serve_art_request(&covers_dir, "http://luminous-art.localhost/album-1.png");
        assert_eq!(cached.status(), 200);
        assert_eq!(cached.headers()["content-type"], "image/png");
        assert_eq!(cached.body().as_slice(), png);

        let encoded = percent_encoding::utf8_percent_encode(
            &folder.join("Folder.jpg").to_string_lossy(),
            percent_encoding::NON_ALPHANUMERIC,
        )
        .to_string();
        let local = serve_art_request(
            &covers_dir,
            &format!("luminous-art://localhost/local/{encoded}"),
        );
        assert_eq!(local.status(), 200);
        assert_eq!(local.headers()["content-type"], "image/jpeg");

        let missing = serve_art_request(&covers_dir, "luminous-art://album-missing.jpg");
        assert_eq!(missing.status(), 404);
    }

    fn jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95)
            .encode_image(&img)
            .unwrap();
        out
    }

    #[test]
    fn test_downscale_for_cache_bounds_longest_edge() {
        let (bytes, ext) = downscale_for_cache(&jpeg_bytes(1200, 800));
        assert_eq!(ext, "jpg");
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!((img.width(), img.height()), (CACHE_MAX_EDGE, 400));
    }

    #[test]
    fn test_downscale_for_cache_passes_small_and_undecodable_through() {
        let small = jpeg_bytes(300, 300);
        assert_eq!(downscale_for_cache(&small), (small.clone(), "jpg"));

        let stub = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        assert_eq!(downscale_for_cache(&stub), (stub.clone(), "png"));
    }

    #[test]
    fn test_cache_usage_splits_album_art_from_artist_images() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).unwrap());
        let manager = CoverManager::new(Arc::clone(&db), temp_dir.path().to_path_buf());
        let covers = manager.covers_dir().to_path_buf();

        std::fs::write(covers.join("album-a.jpg"), [0u8; 100]).unwrap();
        std::fs::write(covers.join("album-b.png"), [0u8; 50]).unwrap();
        std::fs::write(covers.join("artist-x.jpg"), [0u8; 30]).unwrap();
        std::fs::write(covers.join("artist-x-logo.png"), [0u8; 7]).unwrap();

        assert_eq!(
            manager.cache_usage(),
            CacheUsage {
                album_art_bytes: 150,
                artist_art_bytes: 37,
            }
        );
    }

    #[test]
    fn test_sweep_cache_prunes_orphans_and_shrinks_referenced_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).unwrap());
        let manager = CoverManager::new(Arc::clone(&db), temp_dir.path().to_path_buf());
        let covers = manager.covers_dir().to_path_buf();

        let big = jpeg_bytes(1200, 1200);
        std::fs::write(covers.join("album-kept.jpg"), &big).unwrap();
        std::fs::write(covers.join("album-orphan.jpg"), b"\xFF\xD8\xFF\xE0").unwrap();
        std::fs::write(covers.join("album-stale.tmp"), b"partial").unwrap();
        std::fs::write(covers.join("artist-x.jpg"), b"\xFF\xD8\xFF\xE0").unwrap();
        // fanart.tv art is referenced from album_profiles, not songs (#1277).
        std::fs::write(covers.join("album-kept_fanart_cover.jpg"), &big).unwrap();
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO songs (title, art_automatic) VALUES ('t', 'album-kept.jpg')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO album_profiles (album_key, fetched_cover_filename)
             VALUES ('a', 'album-kept_fanart_cover.jpg')",
            [],
        )
        .unwrap();
        drop(conn);

        // Within the grace period nothing unreferenced is removed.
        let fresh = manager
            .sweep_cache(std::time::Duration::from_secs(600))
            .unwrap();
        assert_eq!(fresh.pruned, 0);
        assert_eq!(fresh.recompressed, 1);
        assert!(covers.join("album-orphan.jpg").exists());

        let swept = manager.sweep_cache(std::time::Duration::ZERO).unwrap();
        assert_eq!(swept.pruned, 2);
        assert_eq!(swept.recompressed, 0, "already-shrunk file is left alone");
        assert!(!covers.join("album-orphan.jpg").exists());
        assert!(!covers.join("album-stale.tmp").exists());
        assert!(covers.join("artist-x.jpg").exists());
        assert_eq!(
            std::fs::read(covers.join("album-kept_fanart_cover.jpg")).unwrap(),
            big,
            "fanart art is neither pruned nor re-encoded"
        );

        let kept = image::open(covers.join("album-kept.jpg")).unwrap();
        assert_eq!(
            (kept.width(), kept.height()),
            (CACHE_MAX_EDGE, CACHE_MAX_EDGE)
        );
    }

    #[test]
    fn test_serve_embedded_art_reads_audio_file_and_falls_back_to_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let covers_dir = temp_dir.path().join("covers");
        std::fs::create_dir_all(&covers_dir).unwrap();
        std::fs::write(covers_dir.join("album-1.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        let track = temp_dir.path().join("Track #1 & 50%.wav");
        write_wav_with_embedded_art(&track);

        let embedded = serve_art_request(
            &covers_dir,
            &embedded_art_uri("album-1.png", &track.to_string_lossy()),
        );
        assert_eq!(embedded.status(), 200);
        assert_eq!(embedded.body().as_slice(), &[0xFF, 0xD8, 0xFF, 0xE0]);

        let missing_audio = temp_dir.path().join("gone.wav");
        let fallback = serve_art_request(
            &covers_dir,
            &embedded_art_uri("album-1.png", &missing_audio.to_string_lossy()),
        );
        assert_eq!(fallback.status(), 200);
        assert_eq!(fallback.headers()["content-type"], "image/png");

        let traversal = serve_art_request(
            &covers_dir,
            &embedded_art_uri("..\\secret.png", &missing_audio.to_string_lossy()),
        );
        assert_eq!(traversal.status(), 404);
    }

    #[test]
    fn test_full_resolution_uri_only_redirects_local_embedded_art() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).unwrap());
        let manager = CoverManager::new(Arc::clone(&db), temp_dir.path().to_path_buf());
        let local = crate::models::SongSource::LocalFile as i32;
        let insert = |path: &str, embedded: bool, manual: Option<&str>, source: i32| -> i64 {
            let conn = db.pool.get().unwrap();
            conn.execute(
                "INSERT INTO songs (title, path, source, art_embedded, art_automatic, art_manual)
                 VALUES ('t', ?1, ?2, ?3, 'album-1.jpg', ?4)",
                params![path, source, embedded, manual],
            )
            .unwrap();
            conn.last_insert_rowid()
        };

        let embedded = insert("C:\\Music\\a.flac", true, None, local);
        assert_eq!(
            manager.get_full_resolution_cover_art_uri(embedded).unwrap(),
            Some(embedded_art_uri("album-1.jpg", "C:\\Music\\a.flac"))
        );

        for id in [
            insert("C:\\Music\\b.flac", false, None, local),
            insert("C:\\Music\\c.flac", true, Some("album-2.jpg"), local),
            insert(
                "C:\\Music\\d.flac",
                true,
                None,
                crate::models::SongSource::SUBSONIC_ID,
            ),
        ] {
            assert_eq!(
                manager.get_full_resolution_cover_art_uri(id).unwrap(),
                manager.get_cover_art_uri(id).unwrap()
            );
        }
    }

    fn write_wav_with_embedded_art(path: &Path) {
        use lofty::{
            config::WriteOptions,
            file::AudioFile,
            picture::{MimeType, Picture},
            tag::Tag,
        };
        let data = [0u8; 1600];
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&8_000u32.to_le_bytes());
        wav.extend_from_slice(&16_000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        std::fs::write(path, wav).unwrap();

        let mut tagged_file = Probe::open(path).unwrap().read().unwrap();
        let mut tag = Tag::new(tagged_file.primary_tag_type());
        tag.push_picture(
            Picture::unchecked(vec![0xFF, 0xD8, 0xFF, 0xE0])
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Jpeg)
                .build(),
        );
        tagged_file.insert_tag(tag);
        tagged_file
            .save_to_path(path, WriteOptions::default())
            .unwrap();
    }

    #[test]
    fn test_per_scan_album_dedup_extracts_each_album_once() {
        let temp_dir = tempfile::tempdir().unwrap();
        let track_1 = temp_dir.path().join("01.wav");
        let track_2 = temp_dir.path().join("02.wav");
        write_wav_with_embedded_art(&track_1);
        write_wav_with_embedded_art(&track_2);
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).unwrap());

        let scan_manager = CoverManager::new(Arc::clone(&db), temp_dir.path().to_path_buf())
            .with_per_scan_album_dedup();
        let filename = scan_manager
            .extract_embedded_art(&track_1, "Artist", "Album")
            .unwrap()
            .unwrap();
        let cached = scan_manager.covers_dir().join(&filename);
        std::fs::remove_file(&cached).unwrap();

        // Second track of the same album: same filename, no second write.
        assert_eq!(
            scan_manager
                .extract_embedded_art(&track_2, "Artist", "Album")
                .unwrap(),
            Some(filename.clone())
        );
        assert!(!cached.exists());

        // A long-lived manager (watcher/app) still re-extracts, so a retag
        // is picked up.
        let manager = CoverManager::new(db, temp_dir.path().to_path_buf());
        manager
            .extract_embedded_art(&track_2, "Artist", "Album")
            .unwrap();
        assert!(cached.exists());
    }

    fn unique_temp_dir(label: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("luminous_extended_art_{label}_"))
            .tempdir()
            .unwrap()
    }

    #[test]
    fn test_artwork_category_ordering_matches_hierarchy() {
        // Declaration order drives Ord — this pins the hierarchy priority
        // from #98 so a future variant reorder is caught here, not silently
        // in the UI (which relies on this for the cover-stack thumbnail and
        // "Open Images" target).
        assert!(ArtworkCategory::PrimaryCover < ArtworkCategory::BackCover);
        assert!(ArtworkCategory::BackCover < ArtworkCategory::DiscMedia);
        assert!(ArtworkCategory::DiscMedia < ArtworkCategory::Booklet);
        assert!(ArtworkCategory::Booklet < ArtworkCategory::Matrix);
        assert!(ArtworkCategory::Matrix < ArtworkCategory::ArtistPortrait);
        assert!(ArtworkCategory::ArtistPortrait < ArtworkCategory::BandLogo);
        assert!(ArtworkCategory::BandLogo < ArtworkCategory::FanartBanner);
        assert!(ArtworkCategory::FanartBanner < ArtworkCategory::Subfolder);
    }

    #[test]
    fn test_category_for_picture_type_maps_known_types() {
        assert_eq!(
            category_for_picture_type(PictureType::CoverFront),
            ArtworkCategory::PrimaryCover
        );
        assert_eq!(
            category_for_picture_type(PictureType::CoverBack),
            ArtworkCategory::BackCover
        );
        assert_eq!(
            category_for_picture_type(PictureType::Leaflet),
            ArtworkCategory::Booklet
        );
        assert_eq!(
            category_for_picture_type(PictureType::Media),
            ArtworkCategory::DiscMedia
        );
        assert_eq!(
            category_for_picture_type(PictureType::Artist),
            ArtworkCategory::ArtistPortrait
        );
        assert_eq!(
            category_for_picture_type(PictureType::Band),
            ArtworkCategory::ArtistPortrait
        );
        assert_eq!(
            category_for_picture_type(PictureType::BandLogo),
            ArtworkCategory::BandLogo
        );
        // Unmapped types fall back to the lowest-priority category rather
        // than being dropped.
        assert_eq!(
            category_for_picture_type(PictureType::Other),
            ArtworkCategory::Subfolder
        );
    }

    #[test]
    fn test_artwork_category_scopes() {
        let album_categories = [
            ArtworkCategory::PrimaryCover,
            ArtworkCategory::BackCover,
            ArtworkCategory::DiscMedia,
            ArtworkCategory::Booklet,
            ArtworkCategory::Matrix,
            ArtworkCategory::Subfolder,
        ];
        for cat in &album_categories {
            assert!(cat.is_album_level(), "{cat:?} must be album level");
            assert!(!cat.is_artist_level(), "{cat:?} must not be artist level");
        }

        let artist_categories = [
            ArtworkCategory::ArtistPortrait,
            ArtworkCategory::BandLogo,
            ArtworkCategory::FanartBanner,
        ];
        for cat in &artist_categories {
            assert!(cat.is_artist_level(), "{cat:?} must be artist level");
            assert!(!cat.is_album_level(), "{cat:?} must not be album level");
        }
    }

    #[test]
    fn test_scan_extended_artwork_categorizes_album_level_hierarchy() {
        let temp_dir_guard = unique_temp_dir("album_hierarchy");
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let _ = std::fs::create_dir_all(&temp_dir);

        let audio_path = temp_dir.join("song.mp3");
        std::fs::write(&audio_path, b"fake audio").unwrap();
        std::fs::write(temp_dir.join("cover.jpg"), b"cover").unwrap();
        std::fs::write(temp_dir.join("back.jpg"), b"back").unwrap();
        std::fs::write(temp_dir.join("booklet.png"), b"booklet").unwrap();
        std::fs::write(temp_dir.join("disc.webp"), b"disc").unwrap();
        std::fs::write(temp_dir.join("tray.jpg"), b"tray").unwrap();
        // Not a recognized name — still counted, just unranked as Subfolder.
        std::fs::write(temp_dir.join("random.jpg"), b"random").unwrap();

        let set = scan_extended_artwork(&audio_path, None).sorted();

        let categories: Vec<ArtworkCategory> = set.entries.iter().map(|e| e.category).collect();
        assert_eq!(
            categories,
            vec![
                ArtworkCategory::PrimaryCover,
                ArtworkCategory::BackCover,
                ArtworkCategory::DiscMedia,
                ArtworkCategory::Booklet,
                ArtworkCategory::Matrix,
                ArtworkCategory::Subfolder,
            ]
        );
        assert_eq!(
            set.primary().unwrap().path.file_name().unwrap(),
            "cover.jpg"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scan_extended_artwork_matches_exact_album_name() {
        let temp_dir_guard = unique_temp_dir("album_name_match");
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let _ = std::fs::create_dir_all(&temp_dir);

        let audio_path = temp_dir.join("song.mp3");
        std::fs::write(&audio_path, b"fake audio").unwrap();
        std::fs::write(temp_dir.join("Wildflowers.jpg"), b"cover").unwrap();

        let set = scan_extended_artwork(&audio_path, Some("Wildflowers")).sorted();

        assert_eq!(set.entries.len(), 1);
        assert_eq!(set.entries[0].category, ArtworkCategory::PrimaryCover);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scan_extended_artwork_finds_artist_folder_media() {
        // Layout: {artist_dir}/{album_dir}/song.mp3, artist-level images
        // (artist.jpg, logo.png, fanart.jpg) sit in {artist_dir}.
        let artist_dir_guard = unique_temp_dir("artist_media");
        let artist_dir = artist_dir_guard.path().to_path_buf();
        let album_dir = artist_dir.join("Greatest Hits");
        let _ = std::fs::create_dir_all(&album_dir);

        let audio_path = album_dir.join("song.mp3");
        std::fs::write(&audio_path, b"fake audio").unwrap();
        std::fs::write(artist_dir.join("artist.jpg"), b"portrait").unwrap();
        std::fs::write(artist_dir.join("logo.png"), b"logo").unwrap();
        std::fs::write(artist_dir.join("fanart.jpg"), b"fanart").unwrap();

        let set = scan_extended_artwork(&audio_path, None).sorted();

        let categories: Vec<ArtworkCategory> = set.entries.iter().map(|e| e.category).collect();
        assert_eq!(
            categories,
            vec![
                ArtworkCategory::ArtistPortrait,
                ArtworkCategory::BandLogo,
                ArtworkCategory::FanartBanner,
            ]
        );

        let _ = std::fs::remove_dir_all(&artist_dir);
    }

    #[test]
    fn test_scan_extended_artwork_recurses_named_subfolders() {
        let temp_dir_guard = unique_temp_dir("subfolder");
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let artwork_dir = temp_dir.join("Artwork");
        let scans_dir = temp_dir.join("Scans");
        let _ = std::fs::create_dir_all(&artwork_dir);
        let _ = std::fs::create_dir_all(&scans_dir);

        let audio_path = temp_dir.join("song.mp3");
        std::fs::write(&audio_path, b"fake audio").unwrap();
        // Named match inside a subfolder keeps its named category.
        std::fs::write(artwork_dir.join("back.jpg"), b"back").unwrap();
        // Unnamed file inside a subfolder falls back to Subfolder, not
        // dropped.
        std::fs::write(scans_dir.join("scan001.jpg"), b"scan").unwrap();

        let set = scan_extended_artwork(&audio_path, None).sorted();

        let categories: Vec<ArtworkCategory> = set.entries.iter().map(|e| e.category).collect();
        assert_eq!(
            categories,
            vec![ArtworkCategory::BackCover, ArtworkCategory::Subfolder]
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scan_extended_artwork_counts_compound_named_album_scans() {
        // Regression for #855 review feedback: a real-world reissue booklet
        // scan set (numbered/compound filenames living directly in the album
        // directory, not a named subfolder) was being silently dropped down
        // to only the 2-3 exactly-named files instead of all of them.
        let temp_dir_guard = unique_temp_dir("compound_named_scans");
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let _ = std::fs::create_dir_all(&temp_dir);

        let audio_path = temp_dir.join("song.mp3");
        std::fs::write(&audio_path, b"fake audio").unwrap();
        for name in [
            "Booklet 1.jpg",
            "Booklet 2.jpg",
            "Booklet 3.jpg",
            "Booklet 4.jpg",
            "CD.jpg",
            "Folder.jpg",
            "Front + Inlay.jpg",
            "Front + OBI.jpg",
            "Front.jpg",
            "obi.jpg",
        ] {
            std::fs::write(temp_dir.join(name), b"scan").unwrap();
        }

        let set = scan_extended_artwork(&audio_path, None);

        assert_eq!(set.entries.len(), 10);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// #1277: fanart.tv album art is a fallback only, and each type hides
    /// when unchecked in Settings.
    #[test]
    fn test_fanart_album_cover_is_fallback_and_follows_prefs() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_covermanager_fanart_test_")
            .tempdir()
            .unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let db = Arc::new(Database::new(temp_dir.clone()).unwrap());
        let song_id: i64 = {
            let conn = db.pool.get().unwrap();
            crate::collection::upsert_song(
                &conn,
                &crate::models::Song {
                    artist: Some("Nightwish".to_string()),
                    album: Some("Oceanborn".to_string()),
                    title: Some("Stargazers".to_string()),
                    source: crate::models::SongSource::LocalFile,
                    path: Some(r"C:\Music\stargazers.flac".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
            conn.execute_batch(
                "INSERT INTO album_profiles (album_key, fetched_cover_filename, fetched_disc_filename)
                 VALUES ('oceanborn', 'fan_cover.jpg', 'fan_disc.png');",
            )
            .unwrap();
            conn.query_row("SELECT id FROM songs", [], |r| r.get(0))
                .unwrap()
        };
        let manager = CoverManager::new(db.clone(), temp_dir.clone());
        let set = |sql: &str| db.pool.get().unwrap().execute_batch(sql).unwrap();

        // Not yet tried remotely: iTunes still gets its turn first.
        assert_eq!(manager.get_cover_art_uri(song_id).unwrap(), None);

        set("UPDATE songs SET art_unset = 1");
        assert_eq!(
            manager.get_cover_art_uri(song_id).unwrap().as_deref(),
            Some("luminous-art://fan_cover.jpg")
        );
        assert_eq!(
            manager.get_cover_art_path(song_id).unwrap(),
            Some(manager.covers_dir().join("fan_cover.jpg"))
        );
        assert_eq!(
            manager.fanart_album_art(song_id).unwrap(),
            (Some("fan_cover.jpg".into()), Some("fan_disc.png".into()))
        );

        set("INSERT INTO app_state VALUES ('fanart_fetch_album_cover', 'false');");
        assert_eq!(manager.get_cover_art_uri(song_id).unwrap(), None);
        assert_eq!(
            manager.fanart_album_art(song_id).unwrap(),
            (None, Some("fan_disc.png".into()))
        );

        // A cover found any other way always wins.
        set(
            "DELETE FROM app_state WHERE key = 'fanart_fetch_album_cover';
             UPDATE songs SET art_unset = 0, art_automatic = 'album-abc.jpg';",
        );
        assert_eq!(
            manager.get_cover_art_uri(song_id).unwrap().as_deref(),
            Some("luminous-art://album-abc.jpg")
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scan_extended_artwork_skips_wmp_thumbnails() {
        // Windows Media Player leaves resized copies of the cover next to
        // Folder.jpg; they must not be counted as separate images.
        let temp_dir_guard = unique_temp_dir("wmp_thumbnails");
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let _ = std::fs::create_dir_all(&temp_dir);

        let audio_path = temp_dir.join("song.mp3");
        std::fs::write(&audio_path, b"fake audio").unwrap();
        for name in [
            "AlbumArt_{F10C7A6B-3B4E-4506-B7EB-53E2A1D4C9F0}_Large.jpg",
            "AlbumArt_{F10C7A6B-3B4E-4506-B7EB-53E2A1D4C9F0}_Small.jpg",
            "AlbumArtSmall.jpg",
            "Folder.jpg",
        ] {
            std::fs::write(temp_dir.join(name), b"scan").unwrap();
        }

        let set = scan_extended_artwork(&audio_path, None).sorted();

        assert_eq!(set.entries.len(), 1);
        assert_eq!(
            set.primary().and_then(|e| e.path.file_name()),
            Some(std::ffi::OsStr::new("Folder.jpg"))
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
