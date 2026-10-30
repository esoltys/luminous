//! The Files view (#1841): every place Luminous reads music from, browsed as
//! it is really laid out rather than as the library interprets it. A source
//! is a watched folder, a Subsonic server, a WebDAV server or a connected
//! device; a location inside one is a list of folder names from its root.
//!
//! What a file *is* to the library (in it, not scanned yet, album art,
//! excluded...) is decided here, so the view only shows it.

use crate::collection::{
    directory_display_name, is_audio_file, is_reserved_dir_name, row_to_song, row_to_song_at,
    CollectionScanner, SONG_SELECT_COLS, SONG_SELECT_COLS_QUALIFIED,
};
use crate::db::Database;
use crate::library_paths;
use crate::models::{Song, SongSource, LOCAL_SOURCES_SQL};
use anyhow::{anyhow, Context, Result};
use luminous_devices::{Device, DeviceBackend, FsBackend};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A place music is read from or written to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FileSource {
    /// A watched music folder, by `directories.id`.
    Folder {
        id: i64,
    },
    Subsonic {
        id: i64,
    },
    Webdav {
        id: i64,
    },
    /// A connected device's first storage, by its device id.
    Device {
        id: String,
    },
}

/// A source as the sidebar lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSummary {
    pub source: FileSource,
    pub name: String,
    /// Library songs from this source.
    pub song_count: u64,
    /// Whether it can be browsed now as far as a cheap check can tell: the
    /// folder exists, the server is enabled.
    pub available: bool,
}

/// What a file or folder is to the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryStatus {
    /// A library song, or a CUE sheet the library reads tracks from.
    InLibrary,
    /// Audio the library hasn't scanned yet.
    NotScanned,
    AlbumArt,
    NotAudio,
    /// Left out of the library on purpose: a folder the scanner never looks
    /// in, anything inside one, or a song marked "Not included".
    Excluded,
}

/// One file or folder in a listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    /// Bytes, for files whose source reports it.
    pub size: Option<u64>,
    /// Unix seconds, when the source reports it.
    pub modified: Option<i64>,
    /// Upper-case extension ("FLAC"), or the codec for Subsonic tracks.
    pub file_type: Option<String>,
    /// `None` for ordinary folders and for device files.
    pub status: Option<EntryStatus>,
    /// The library song this file is, to play it.
    pub song_id: Option<i64>,
}

/// One folder of a source: folders first, then files, each by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    /// Where this folder really is, to show: a local path or a server URL;
    /// `None` where there is no such thing (a Subsonic index).
    pub location: Option<String>,
    pub entries: Vec<FileEntry>,
    pub file_count: usize,
    /// Total bytes of the files, `None` when the source doesn't report sizes.
    pub total_bytes: Option<u64>,
    /// How many of the files are library songs.
    pub in_library_count: usize,
    /// The folder is one the scanner never looks in, or inside one, so
    /// scanning it would find nothing.
    pub excluded: bool,
}

/// Every folder and server, in the order the sidebar shows them: folders,
/// then Subsonic, then WebDAV servers. Devices come from the device list.
pub fn list_sources(conn: &Connection, db: &Arc<Database>) -> Result<Vec<SourceSummary>> {
    let mut sources = Vec::new();
    for dir in CollectionScanner::new(Arc::clone(db)).get_directories()? {
        sources.push(SourceSummary {
            song_count: count_local_songs_under(conn, Path::new(&dir.path))?,
            name: directory_display_name(&dir),
            available: dir.is_available,
            source: FileSource::Folder { id: dir.id },
        });
    }
    let mut stmt = conn.prepare(
        "SELECT id, COALESCE(NULLIF(TRIM(nickname), ''), name), enabled FROM subsonic_servers ORDER BY id",
    )?;
    let servers: Vec<(i64, String, bool)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, name, enabled) in servers {
        let song_count: u64 = conn.query_row(
            "SELECT COUNT(*) FROM songs WHERE source = ?1 AND unavailable = 0 AND path LIKE ?2",
            params![SongSource::Subsonic as i32, format!("subsonic://{id}/%")],
            |r| r.get::<_, i64>(0).map(|n| n as u64),
        )?;
        sources.push(SourceSummary {
            source: FileSource::Subsonic { id },
            name,
            song_count,
            available: enabled,
        });
    }
    let mut stmt = conn.prepare(
        "SELECT id, COALESCE(NULLIF(TRIM(nickname), ''), name), enabled FROM webdav_servers ORDER BY id",
    )?;
    let servers: Vec<(i64, String, bool)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, name, enabled) in servers {
        let song_count: u64 = conn.query_row(
            "SELECT COUNT(*) FROM webdav_cache c JOIN songs s ON s.id = c.song_id
             WHERE c.server_id = ?1 AND s.unavailable = 0",
            params![id],
            |r| r.get::<_, i64>(0).map(|n| n as u64),
        )?;
        sources.push(SourceSummary {
            source: FileSource::Webdav { id },
            name,
            song_count,
            available: enabled,
        });
    }
    Ok(sources)
}

/// The folder at `path` in `source`. A folder that isn't there lists as
/// empty; `path` can't leave the source (`..` and empty names are dropped).
/// Fails when the source itself is gone (`source-not-found`) or a server
/// can't be reached. Lists WebDAV over the network, so call it off the
/// async runtime.
pub fn list_dir(
    conn: &Connection,
    source: &FileSource,
    path: &[String],
    device: impl FnOnce(&str) -> Option<Device>,
) -> Result<DirListing> {
    let path = clean_path(path);
    let listing = match source {
        FileSource::Folder { id } => list_folder(conn, &folder_root(conn, *id)?, &path)?,
        FileSource::Subsonic { id } => list_subsonic(conn, *id, &path)?,
        FileSource::Webdav { id } => list_webdav(conn, *id, &path)?,
        FileSource::Device { id } => {
            let device = device(id).ok_or_else(|| anyhow!("source-not-found"))?;
            let storage = device
                .storage(&device.storages[0].id)
                .ok_or_else(|| anyhow!("source-not-found"))?;
            list_device(storage.as_ref(), &path)?
        }
    };
    Ok(listing)
}

/// The library songs at `path` in `source` and in every folder below it,
/// in the order they are laid out (by path; a Subsonic album by disc and
/// track). Empty for devices, whose files aren't library songs.
pub fn songs_under(conn: &Connection, source: &FileSource, path: &[String]) -> Result<Vec<Song>> {
    let path = clean_path(path);
    match source {
        FileSource::Folder { id } => {
            let folder = join_local(&folder_root(conn, *id)?, &path);
            let mut songs = local_songs_under(conn, &folder)?;
            songs.sort_by_cached_key(|s| {
                (
                    library_paths::key(s.path.as_deref().unwrap_or("")),
                    s.beginning_nanosec,
                )
            });
            Ok(songs)
        }
        FileSource::Subsonic { id } => {
            let mut songs: Vec<Song> = subsonic_songs(conn, *id)?
                .into_iter()
                .filter(|s| {
                    let (artist, album) = subsonic_place(s);
                    match path.as_slice() {
                        [] => true,
                        [a] => *a == artist,
                        [a, b, ..] => *a == artist && *b == album,
                    }
                })
                .collect();
            songs.sort_by_cached_key(|s| {
                let (artist, album) = subsonic_place(s);
                (
                    artist.to_lowercase(),
                    album.to_lowercase(),
                    s.disc.unwrap_or(0),
                    s.track.unwrap_or(0),
                )
            });
            Ok(songs)
        }
        FileSource::Webdav { id } => {
            let dir = webdav_dir_href(&webdav_server(conn, *id)?.remote_path, &path);
            let mut stmt = conn.prepare(&format!(
                "SELECT c.remote_path, {SONG_SELECT_COLS_QUALIFIED} FROM webdav_cache c
                 JOIN songs s ON s.id = c.song_id
                 WHERE c.server_id = ?1 AND s.unavailable = 0"
            ))?;
            let mut songs: Vec<(String, Song)> = stmt
                .query_map(params![id], |r| {
                    Ok((r.get::<_, String>(0)?, row_to_song_at(r, 1)?))
                })?
                .filter_map(|r| r.ok())
                .filter(|(href, _)| href.starts_with(&dir))
                .collect();
            songs.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(songs.into_iter().map(|(_, s)| s).collect())
        }
        FileSource::Device { .. } => Ok(Vec::new()),
    }
}

/// The local folder at `path` in a folder source, for scanning or showing
/// in the file manager. `None` for other sources.
pub fn local_folder(
    conn: &Connection,
    source: &FileSource,
    path: &[String],
) -> Result<Option<PathBuf>> {
    match source {
        FileSource::Folder { id } => Ok(Some(join_local(
            &folder_root(conn, *id)?,
            &clean_path(path),
        ))),
        _ => Ok(None),
    }
}

/// `path` without the names that would leave the source or mean nothing.
fn clean_path(path: &[String]) -> Vec<String> {
    path.iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains(['/', '\\']))
        .map(str::to_string)
        .collect()
}

fn folder_root(conn: &Connection, id: i64) -> Result<PathBuf> {
    conn.query_row(
        "SELECT path FROM directories WHERE id = ?1",
        params![id],
        |r| r.get::<_, String>(0),
    )
    .optional()?
    .map(PathBuf::from)
    .ok_or_else(|| anyhow!("source-not-found"))
}

fn join_local(root: &Path, path: &[String]) -> PathBuf {
    let mut full = root.to_path_buf();
    full.extend(path);
    full
}

/// Every available local library song whose file, or CUE sheet, is under
/// `folder` at any depth.
fn local_songs_under(conn: &Connection, folder: &Path) -> Result<Vec<Song>> {
    let folder_key = library_paths::key(&folder.to_string_lossy());
    // SQLite's LIKE ignores ASCII case, so this is a superset on every
    // platform; `library_paths` decides.
    let pattern = format!("{}/%", escape_like(&folder_key.to_lowercase()));
    let mut stmt = conn.prepare(&format!(
        "SELECT {SONG_SELECT_COLS} FROM songs
         WHERE source IN ({lib}) AND unavailable = 0
           AND (REPLACE(path, '\\', '/') LIKE ?1 ESCAPE '!'
                OR REPLACE(cue_path, '\\', '/') LIKE ?1 ESCAPE '!')",
        lib = *LOCAL_SOURCES_SQL
    ))?;
    let songs = stmt
        .query_map(params![pattern], row_to_song)?
        .filter_map(|r| r.ok())
        .filter(|s| {
            [s.path.as_deref(), s.cue_path.as_deref()]
                .into_iter()
                .flatten()
                .any(|p| library_paths::is_under_key(&library_paths::key(p), &folder_key))
        })
        .collect();
    Ok(songs)
}

/// How many available local library songs are under `folder`, without
/// loading them.
fn count_local_songs_under(conn: &Connection, folder: &Path) -> Result<u64> {
    let folder_key = library_paths::key(&folder.to_string_lossy());
    let pattern = format!("{}/%", escape_like(&folder_key.to_lowercase()));
    let mut stmt = conn.prepare(&format!(
        "SELECT path FROM songs
         WHERE source IN ({lib}) AND unavailable = 0
           AND REPLACE(path, '\\', '/') LIKE ?1 ESCAPE '!'",
        lib = *LOCAL_SOURCES_SQL
    ))?;
    let count = stmt
        .query_map(params![pattern], |r| r.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .filter(|p| library_paths::is_under_key(&library_paths::key(p), &folder_key))
        .count();
    Ok(count as u64)
}

fn escape_like(s: &str) -> String {
    s.replace('!', "!!").replace('%', "!%").replace('_', "!_")
}

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

fn extension(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
}

/// The status of a file that is not a library song, by its name alone.
fn status_by_name(name: &str) -> EntryStatus {
    if is_audio_file(Path::new(name)) {
        EntryStatus::NotScanned
    } else if extension(name).is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.as_str())) {
        EntryStatus::AlbumArt
    } else {
        EntryStatus::NotAudio
    }
}

fn file_type(name: &str) -> Option<String> {
    extension(name).map(|e| e.to_uppercase())
}

fn list_folder(conn: &Connection, root: &Path, path: &[String]) -> Result<DirListing> {
    let dir = join_local(root, path);
    let raw = FsBackend::new(root).list(&path.join("/"))?;
    let in_reserved = path.iter().any(|p| is_reserved_dir_name(p.as_ref()));

    // Library songs and CUE sheets directly in this folder, by file key.
    let dir_key = library_paths::key(&dir.to_string_lossy());
    let mut songs: HashMap<String, (i64, bool)> = HashMap::new();
    let mut cue_sheets: HashSet<String> = HashSet::new();
    for song in local_songs_under(conn, &dir)? {
        if let Some(p) = song.path.as_deref() {
            let key = library_paths::key(p);
            if parent_key(&key) == dir_key {
                // CUE tracks share their media file; any of them stands for it.
                songs.entry(key).or_insert((song.id, song.not_included));
            }
        }
        if let Some(cue) = song.cue_path.as_deref() {
            cue_sheets.insert(library_paths::key(cue));
        }
    }

    let entries = raw
        .into_iter()
        .map(|e| {
            if e.is_dir {
                let excluded = in_reserved || is_reserved_dir_name(e.name.as_ref());
                return FileEntry {
                    status: excluded.then_some(EntryStatus::Excluded),
                    size: None,
                    file_type: None,
                    song_id: None,
                    name: e.name,
                    is_dir: true,
                    modified: e.modified,
                };
            }
            let key = library_paths::key(&dir.join(&e.name).to_string_lossy());
            let song = songs.get(&key);
            let status = if in_reserved || song.is_some_and(|(_, not_included)| *not_included) {
                EntryStatus::Excluded
            } else if song.is_some() || cue_sheets.contains(&key) {
                EntryStatus::InLibrary
            } else {
                status_by_name(&e.name)
            };
            FileEntry {
                file_type: file_type(&e.name),
                status: Some(status),
                song_id: song.map(|(id, _)| *id),
                size: Some(e.size),
                modified: e.modified,
                name: e.name,
                is_dir: false,
            }
        })
        .collect();
    Ok(DirListing {
        excluded: in_reserved,
        ..finish(Some(dir.to_string_lossy().into_owned()), entries)
    })
}

fn parent_key(key: &str) -> &str {
    key.rfind('/').map_or("", |i| &key[..i])
}

fn list_device(storage: &dyn DeviceBackend, path: &[String]) -> Result<DirListing> {
    let entries = storage
        .list(&path.join("/"))?
        .into_iter()
        .filter(|e| !luminous_devices::is_partial_file(&e.name))
        .map(|e| FileEntry {
            file_type: (!e.is_dir).then(|| file_type(&e.name)).flatten(),
            size: (!e.is_dir).then_some(e.size),
            status: None,
            song_id: None,
            modified: e.modified,
            is_dir: e.is_dir,
            name: e.name,
        })
        .collect();
    Ok(finish(None, entries))
}

/// Sorts `entries` and totals the files.
fn finish(location: Option<String>, mut entries: Vec<FileEntry>) -> DirListing {
    entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()));
    let files = entries.iter().filter(|e| !e.is_dir);
    let total_bytes = files
        .clone()
        .map(|e| e.size)
        .try_fold(0u64, |sum, size| size.map(|s| sum + s));
    DirListing {
        location,
        file_count: files.clone().count(),
        total_bytes,
        in_library_count: files
            .filter(|e| e.status == Some(EntryStatus::InLibrary) && e.song_id.is_some())
            .count(),
        entries,
        excluded: false,
    }
}

fn subsonic_songs(conn: &Connection, id: i64) -> Result<Vec<Song>> {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM subsonic_servers WHERE id = ?1",
            params![id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Err(anyhow!("source-not-found"));
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT {SONG_SELECT_COLS} FROM songs
         WHERE source = ?1 AND unavailable = 0 AND path LIKE ?2"
    ))?;
    let songs = stmt
        .query_map(
            params![SongSource::Subsonic as i32, format!("subsonic://{id}/%")],
            row_to_song,
        )?
        .filter_map(|r| r.ok())
        .collect();
    Ok(songs)
}

/// Where a Subsonic song sits in the Artists > Albums > Tracks index.
fn subsonic_place(song: &Song) -> (String, String) {
    let artist = song
        .album_artist
        .as_deref()
        .filter(|a| !a.trim().is_empty())
        .or(song.artist.as_deref())
        .unwrap_or("")
        .trim()
        .to_string();
    let album = song.album.as_deref().unwrap_or("").trim().to_string();
    (artist, album)
}

/// Subsonic has no file tree, so its songs are browsed as artists, then
/// albums, then tracks. Sizes aren't shown and there's nothing but audio.
fn list_subsonic(conn: &Connection, id: i64, path: &[String]) -> Result<DirListing> {
    let songs = subsonic_songs(conn, id)?;
    let folder = |name: String| FileEntry {
        name,
        is_dir: true,
        size: None,
        modified: None,
        file_type: None,
        status: None,
        song_id: None,
    };
    let entries = match path {
        [] => songs
            .iter()
            .map(|s| subsonic_place(s).0)
            .collect::<HashSet<_>>()
            .into_iter()
            .map(folder)
            .collect(),
        [artist] => songs
            .iter()
            .map(subsonic_place)
            .filter(|(a, _)| a == artist)
            .map(|(_, album)| album)
            .collect::<HashSet<_>>()
            .into_iter()
            .map(folder)
            .collect(),
        [artist, album, ..] => {
            let mut tracks: BTreeMap<(i32, i32, String), &Song> = BTreeMap::new();
            for s in songs
                .iter()
                .filter(|s| subsonic_place(s) == (artist.clone(), album.clone()))
            {
                tracks.insert(
                    (
                        s.disc.unwrap_or(0),
                        s.track.unwrap_or(0),
                        s.display_title().to_lowercase(),
                    ),
                    s,
                );
            }
            let entries: Vec<FileEntry> = tracks
                .into_values()
                .map(|s| FileEntry {
                    name: match s.track {
                        Some(n) => format!("{n:02} {}", s.display_title()),
                        None => s.display_title().to_string(),
                    },
                    is_dir: false,
                    size: None,
                    modified: None,
                    file_type: Some(format!("{:?}", s.filetype).to_uppercase())
                        .filter(|t| t != "UNKNOWN"),
                    status: Some(EntryStatus::InLibrary),
                    song_id: Some(s.id),
                })
                .collect();
            // Already in disc and track order, which name order would break.
            let in_library_count = entries.len();
            return Ok(DirListing {
                location: None,
                file_count: entries.len(),
                total_bytes: None,
                in_library_count,
                entries,
                excluded: false,
            });
        }
    };
    Ok(finish(None, entries))
}

struct WebDavServerRow {
    url: String,
    username: Option<String>,
    password: Option<String>,
    remote_path: String,
}

fn webdav_server(conn: &Connection, id: i64) -> Result<WebDavServerRow> {
    conn.query_row(
        "SELECT url, username, password, remote_path FROM webdav_servers WHERE id = ?1",
        params![id],
        |r| {
            Ok(WebDavServerRow {
                url: r.get(0)?,
                username: r.get(1)?,
                password: r.get(2)?,
                remote_path: r.get(3)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow!("source-not-found"))
}

/// Characters escaped in a WebDAV path segment.
const SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/');

/// The href of the folder at `path` below the server's music folder, with a
/// trailing `/`.
fn webdav_dir_href(remote_path: &str, path: &[String]) -> String {
    let mut href = remote_path.trim_end_matches('/').to_string();
    if !href.starts_with('/') {
        href.insert(0, '/');
    }
    for name in path {
        href.push('/');
        href.extend(utf8_percent_encode(name, SEGMENT));
    }
    href.push('/');
    href
}

fn list_webdav(conn: &Connection, id: i64, path: &[String]) -> Result<DirListing> {
    let server = webdav_server(conn, id)?;
    let dir = webdav_dir_href(&server.remote_path, path);
    let client = crate::webdav::WebDavClient::new(server.url, server.username, server.password)?;
    let items = client.list_directory(&dir).context("server-unreachable")?;

    let mut stmt = conn.prepare(
        "SELECT c.remote_path, s.id, s.not_included FROM webdav_cache c
         JOIN songs s ON s.id = c.song_id
         WHERE c.server_id = ?1 AND s.unavailable = 0",
    )?;
    let songs: HashMap<String, (i64, bool)> = stmt
        .query_map(params![id], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
        .filter_map(|r| r.ok())
        .collect();

    let entries = items
        .into_iter()
        .filter(|item| !crate::webdav::is_listed_collection(&item.href, &dir))
        .filter_map(|item| {
            let encoded = item.href.trim_end_matches('/').rsplit('/').next()?;
            let name = percent_decode_str(encoded).decode_utf8_lossy().into_owned();
            if item.is_directory {
                return Some(FileEntry {
                    name,
                    is_dir: true,
                    size: None,
                    modified: None,
                    file_type: None,
                    status: None,
                    song_id: None,
                });
            }
            let song = songs.get(&item.href);
            let status = match song {
                Some((_, true)) => EntryStatus::Excluded,
                Some(_) => EntryStatus::InLibrary,
                None => status_by_name(&name),
            };
            Some(FileEntry {
                file_type: file_type(&name),
                size: item.content_length,
                modified: None,
                status: Some(status),
                song_id: song.map(|(id, _)| *id),
                is_dir: false,
                name,
            })
        })
        .collect();
    let location = client.playback_url(&dir);
    Ok(finish(
        Some(
            percent_decode_str(&location)
                .decode_utf8_lossy()
                .into_owned(),
        ),
        entries,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _temp: tempfile::TempDir,
        db: Arc<Database>,
        root: PathBuf,
    }

    /// A watched folder `music` holding `files` (relative paths), with the
    /// ones in `songs` scanned into the library.
    fn fixture(files: &[&str], songs: &[&str]) -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("db")).unwrap());
        let root = temp.path().join("music");
        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"x").unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO directories (id, path, subdirs) VALUES (1, ?1, 1)",
            params![root.to_string_lossy()],
        )
        .unwrap();
        for song in songs {
            conn.execute(
                "INSERT INTO songs (source, path, title) VALUES (?1, ?2, ?3)",
                params![
                    SongSource::LocalFile as i32,
                    root.join(song).to_string_lossy(),
                    song
                ],
            )
            .unwrap();
        }
        drop(conn);
        Fixture {
            _temp: temp,
            db,
            root,
        }
    }

    fn list(f: &Fixture, path: &[&str]) -> DirListing {
        let conn = f.db.pool.get().unwrap();
        let path: Vec<String> = path.iter().map(|p| p.to_string()).collect();
        list_dir(&conn, &FileSource::Folder { id: 1 }, &path, |_| None).unwrap()
    }

    fn status_of(listing: &DirListing, name: &str) -> Option<EntryStatus> {
        listing
            .entries
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name} not listed"))
            .status
    }

    #[test]
    fn files_say_what_they_are_to_the_library() {
        let f = fixture(
            &[
                "A/Album/01.flac",
                "A/Album/02.flac",
                "A/Album/cover.jpg",
                "A/Album/rip.log",
                "A/Duplicates/01.flac",
            ],
            &["A/Album/01.flac"],
        );
        let album = list(&f, &["A", "Album"]);
        assert_eq!(status_of(&album, "01.flac"), Some(EntryStatus::InLibrary));
        assert_eq!(status_of(&album, "02.flac"), Some(EntryStatus::NotScanned));
        assert_eq!(status_of(&album, "cover.jpg"), Some(EntryStatus::AlbumArt));
        assert_eq!(status_of(&album, "rip.log"), Some(EntryStatus::NotAudio));
        assert_eq!(album.file_count, 4);
        assert_eq!(album.total_bytes, Some(4));
        assert_eq!(album.in_library_count, 1);
        assert_eq!(
            album.location.as_deref(),
            Some(f.root.join("A").join("Album").to_string_lossy().as_ref())
        );

        let artist = list(&f, &["A"]);
        assert_eq!(status_of(&artist, "Album"), None);
        assert_eq!(
            status_of(&artist, "Duplicates"),
            Some(EntryStatus::Excluded)
        );
        let reserved = list(&f, &["A", "Duplicates"]);
        assert_eq!(status_of(&reserved, "01.flac"), Some(EntryStatus::Excluded));
        assert!(reserved.excluded);
        assert!(!album.excluded);
    }

    #[cfg(windows)]
    #[test]
    fn library_paths_match_whatever_their_case_on_windows() {
        let f = fixture(&["A/01.flac"], &[]);
        f.db.pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO songs (source, path) VALUES (?1, ?2)",
                params![
                    SongSource::LocalFile as i32,
                    f.root
                        .join("a")
                        .join("01.FLAC")
                        .to_string_lossy()
                        .to_uppercase()
                ],
            )
            .unwrap();
        assert_eq!(
            status_of(&list(&f, &["A"]), "01.flac"),
            Some(EntryStatus::InLibrary)
        );
    }

    #[test]
    fn songs_marked_not_included_are_excluded() {
        let f = fixture(&["01.flac"], &["01.flac"]);
        f.db.pool
            .get()
            .unwrap()
            .execute("UPDATE songs SET not_included = 1", [])
            .unwrap();
        assert_eq!(
            status_of(&list(&f, &[]), "01.flac"),
            Some(EntryStatus::Excluded)
        );
    }

    #[test]
    fn a_cue_sheet_the_library_reads_is_in_it() {
        let f = fixture(&["album.flac", "album.cue"], &["album.flac"]);
        f.db.pool
            .get()
            .unwrap()
            .execute(
                "UPDATE songs SET cue_path = ?1",
                params![f.root.join("album.cue").to_string_lossy()],
            )
            .unwrap();
        let listing = list(&f, &[]);
        assert_eq!(
            status_of(&listing, "album.cue"),
            Some(EntryStatus::InLibrary)
        );
        assert_eq!(listing.in_library_count, 1);
    }

    #[test]
    fn folders_come_first_then_files_by_name() {
        let f = fixture(&["b.flac", "A.flac", "z/x.flac", "c/x.flac"], &[]);
        let names: Vec<String> = list(&f, &[]).entries.into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["c", "z", "A.flac", "b.flac"]);
    }

    #[test]
    fn a_path_cannot_leave_the_source() {
        let f = fixture(&["in.flac"], &[]);
        std::fs::write(f.root.parent().unwrap().join("outside.flac"), b"x").unwrap();
        let listing = list(&f, &["..", ""]);
        assert!(listing.entries.iter().any(|e| e.name == "in.flac"));
        assert!(!listing.entries.iter().any(|e| e.name == "outside.flac"));
    }

    #[test]
    fn a_missing_folder_lists_as_empty() {
        let f = fixture(&[], &[]);
        assert!(list(&f, &["gone"]).entries.is_empty());
    }

    #[test]
    fn a_removed_source_is_not_found() {
        let f = fixture(&[], &[]);
        let conn = f.db.pool.get().unwrap();
        let err = list_dir(&conn, &FileSource::Folder { id: 9 }, &[], |_| None).unwrap_err();
        assert_eq!(err.to_string(), "source-not-found");
    }

    #[test]
    fn play_folder_takes_songs_below_it_in_path_order() {
        let f = fixture(
            &["A/2.flac", "A/1.flac", "A/x/3.flac", "AB/4.flac"],
            &["A/2.flac", "A/1.flac", "A/x/3.flac", "AB/4.flac"],
        );
        let conn = f.db.pool.get().unwrap();
        let titles: Vec<String> =
            songs_under(&conn, &FileSource::Folder { id: 1 }, &["A".to_string()])
                .unwrap()
                .into_iter()
                .filter_map(|s| s.title)
                .collect();
        assert_eq!(titles, ["A/1.flac", "A/2.flac", "A/x/3.flac"]);
    }

    #[test]
    fn sources_count_their_library_songs() {
        let f = fixture(&["a.flac", "b.flac"], &["a.flac", "b.flac"]);
        let conn = f.db.pool.get().unwrap();
        let sources = list_sources(&conn, &f.db).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].song_count, 2);
        assert_eq!(sources[0].name, "music");
        assert!(sources[0].available);
    }

    #[test]
    fn subsonic_is_browsed_as_artists_albums_and_tracks() {
        let f = fixture(&[], &[]);
        let conn = f.db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO subsonic_servers (id, name, url, username) VALUES (3, 'Navidrome', 'http://x', 'u')",
            [],
        )
        .unwrap();
        for (id, artist, album, track, title) in [
            ("t2", "Katatonia", "Viva Emptiness", 2, "Wealth"),
            ("t1", "Katatonia", "Viva Emptiness", 1, "Ghost of the Sun"),
            ("t3", "Opeth", "Damnation", 1, "Windowpane"),
        ] {
            conn.execute(
                "INSERT INTO songs (source, path, artist, album, track, title) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![SongSource::Subsonic as i32, format!("subsonic://3/{id}"), artist, album, track, title],
            )
            .unwrap();
        }
        let source = FileSource::Subsonic { id: 3 };
        let names = |path: &[&str]| -> Vec<String> {
            let path: Vec<String> = path.iter().map(|p| p.to_string()).collect();
            list_dir(&conn, &source, &path, |_| None)
                .unwrap()
                .entries
                .into_iter()
                .map(|e| e.name)
                .collect()
        };
        assert_eq!(names(&[]), ["Katatonia", "Opeth"]);
        assert_eq!(names(&["Katatonia"]), ["Viva Emptiness"]);
        assert_eq!(
            names(&["Katatonia", "Viva Emptiness"]),
            ["01 Ghost of the Sun", "02 Wealth"]
        );
        let songs = songs_under(&conn, &source, &["Katatonia".to_string()]).unwrap();
        assert_eq!(songs.len(), 2);
    }

    #[test]
    fn webdav_folders_are_percent_encoded_hrefs() {
        assert_eq!(webdav_dir_href("/dav/Music/", &[]), "/dav/Music/");
        assert_eq!(
            webdav_dir_href("dav", &["AC/DC?".into(), "Live #1".into()]),
            "/dav/AC%2FDC%3F/Live%20%231/"
        );
    }
}
