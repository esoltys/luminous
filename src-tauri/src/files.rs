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
use crate::devices::sync::manifest::{EntryKind, Manifest, MANIFEST_FILE_NAME};
use crate::library_paths;
use crate::models::{Song, SongSource, LOCAL_SOURCES_SQL};
use anyhow::{anyhow, Context, Result};
use luminous_devices::{
    is_partial_file, is_system_clutter, DeviceBackend, DeviceProfile, FsBackend, MARKER_FILE_NAME,
};
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
    /// A connected device's music folder, on the storage `storage` names
    /// (the device's first when `None`).
    Device {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        storage: Option<String>,
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
    /// On a device, a file sync didn't copy or a folder holding none: sync
    /// never removes these.
    NotFromLuminous,
}

/// What a file is, by its type. The Files view lists only these: anything
/// else (programs, shortcuts, system files) is counted as not shown, so
/// nothing listed can run when opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Audio,
    Image,
    Lyrics,
    /// Plain text: bios, liner notes, `.nfo` release notes.
    Notes,
    Pdf,
    CueSheet,
    /// A rip or dynamic range log.
    Log,
    Playlist,
}

impl FileKind {
    /// The kind of the file named `name`; `None` for a type the Files view
    /// doesn't list.
    pub fn of(name: &str) -> Option<FileKind> {
        if is_audio_file(Path::new(name)) {
            return Some(FileKind::Audio);
        }
        Some(match extension(name)?.as_str() {
            "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" => FileKind::Image,
            "lrc" => FileKind::Lyrics,
            "md" | "txt" | "nfo" => FileKind::Notes,
            "pdf" => FileKind::Pdf,
            "cue" => FileKind::CueSheet,
            "log" => FileKind::Log,
            "m3u" | "m3u8" | "pls" | "xspf" => FileKind::Playlist,
            _ => return None,
        })
    }
}

/// The tag a Subsonic folder's songs lack: such a folder's name is empty,
/// which the UI words ("Unknown Artist") rather than shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MissingTag {
    Artist,
    Album,
}

/// One file or folder in a listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Its identity in a path. Empty only for a `missing_tag` folder.
    pub name: String,
    pub is_dir: bool,
    /// Bytes, for files whose source reports it.
    pub size: Option<u64>,
    /// Unix seconds, when the source reports it.
    pub modified: Option<i64>,
    /// Upper-case extension ("FLAC"), or the codec for Subsonic tracks.
    pub file_type: Option<String>,
    /// `None` for folders; a file whose type isn't a [`FileKind`] is never
    /// listed.
    pub kind: Option<FileKind>,
    /// `None` for ordinary folders, and on a device for synced files that
    /// aren't library songs (playlist files, art, songs since removed).
    pub status: Option<EntryStatus>,
    /// The library song this file is, to play it.
    pub song_id: Option<i64>,
    pub missing_tag: Option<MissingTag>,
    /// Where it is on this computer, to show, copy or open it there; `None`
    /// on servers and devices, which have no such path.
    pub location: Option<String>,
}

impl FileEntry {
    fn folder(name: String) -> Self {
        FileEntry {
            name,
            is_dir: true,
            size: None,
            modified: None,
            file_type: None,
            kind: None,
            status: None,
            song_id: None,
            missing_tag: None,
            location: None,
        }
    }
}

/// One folder of a source: folders first, then files, each by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    /// Where this folder really is, to show: a local path or a server URL;
    /// `None` where there is no such thing (a Subsonic index).
    pub location: Option<String>,
    pub entries: Vec<FileEntry>,
    /// Files listed; `entries` holds them and the folders.
    pub file_count: usize,
    /// Files left out because the Files view doesn't list their type.
    pub hidden_count: usize,
    /// Total bytes of the files, `None` when the source doesn't report sizes.
    pub total_bytes: Option<u64>,
    /// How many of the files are library songs.
    pub in_library_count: usize,
    /// The folder is one the scanner never looks in, or inside one, so
    /// scanning it would find nothing.
    pub excluded: bool,
    /// For each name of the listed path, the tag it stands for when it is a
    /// `missing_tag` folder; empty for sources that have none.
    pub path_missing_tags: Vec<Option<MissingTag>>,
}

/// Every folder and server, in the order the sidebar shows them: folders,
/// then Subsonic, then WebDAV servers. Devices come from the device list.
pub fn list_sources(conn: &Connection, db: &Arc<Database>) -> Result<Vec<SourceSummary>> {
    let mut sources = Vec::new();
    for dir in CollectionScanner::new(Arc::clone(db)).get_directories()? {
        sources.push(SourceSummary {
            song_count: local_songs_under(conn, Path::new(&dir.path))?.len() as u64,
            name: directory_display_name(&dir),
            available: dir.is_available,
            source: FileSource::Folder { id: dir.id },
        });
    }
    for (id, name, enabled) in servers(conn, "subsonic_servers")? {
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
    for (id, name, enabled) in servers(conn, "webdav_servers")? {
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

/// Every server in `table` (`subsonic_servers` or `webdav_servers`), as its
/// id, shown name and whether it is enabled.
fn servers(conn: &Connection, table: &str) -> Result<Vec<(i64, String, bool)>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT id, COALESCE(NULLIF(TRIM(nickname), ''), name), enabled FROM {table} ORDER BY id"
    ))?;
    let servers = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(servers)
}

/// Whether `table` (`subsonic_servers` or `webdav_servers`) has server `id`.
fn server_exists(conn: &Connection, table: &str, id: i64) -> Result<bool> {
    Ok(conn
        .query_row(
            &format!("SELECT 1 FROM {table} WHERE id = ?1"),
            params![id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// The folder at `path` in `source`. A folder that isn't there lists as
/// empty. In a folder, WebDAV server or device, `path` can't leave the
/// source (see `clean_path`); a Subsonic path names an artist and album by
/// their exact tags instead, `""` for a missing one. Fails when the source
/// itself is gone (`source-not-found`) or a server can't be reached. Blocks
/// on the disk or the network, so call it off the async runtime; it takes a
/// database connection only around its queries, never while waiting on a
/// slow drive or server.
pub fn list_dir(
    db: &Database,
    source: &FileSource,
    path: &[String],
    device: impl FnOnce(&str, Option<&str>) -> Option<(Box<dyn DeviceBackend>, DeviceProfile)>,
) -> Result<DirListing> {
    match source {
        FileSource::Folder { id } => list_folder(db, *id, &clean_path(path)),
        FileSource::Subsonic { id } => list_subsonic(&*db.pool.get()?, *id, path),
        FileSource::Webdav { id } => list_webdav(db, *id, &clean_path(path)),
        FileSource::Device { id, storage } => {
            let (storage, profile) =
                device(id, storage.as_deref()).ok_or_else(|| anyhow!("source-not-found"))?;
            list_device(db, storage.as_ref(), &device_path(&profile, path))
        }
    }
}

/// The library songs at `path` in `source` and in every folder below it,
/// in the order they are laid out (by path; a Subsonic album by disc and
/// track). `path` is read as `list_dir` reads it. Empty for devices, whose
/// files aren't library songs.
pub fn songs_under(conn: &Connection, source: &FileSource, path: &[String]) -> Result<Vec<Song>> {
    match source {
        FileSource::Folder { id } => {
            let folder = join_local(&folder_root(conn, *id)?, &clean_path(path));
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
                    match path {
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
            let dir = webdav_dir_href(&webdav_server(conn, *id)?.remote_path, &clean_path(path));
            let mut stmt = conn.prepare(&format!(
                "SELECT c.remote_path, {SONG_SELECT_COLS_QUALIFIED} FROM webdav_cache c
                 JOIN songs s ON s.id = c.song_id
                 WHERE c.server_id = ?1 AND s.unavailable = 0"
            ))?;
            let mut songs: Vec<(String, Song)> = stmt
                .query_map(params![id], |r| {
                    Ok((r.get::<_, String>(0)?, row_to_song_at(r, 1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
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

/// The local file at `path` (its folder's names, then its own) in a folder
/// source, when the Files view opens it in another app: a listed kind other
/// than audio, which plays instead. `None` for anything else, files on other
/// sources included, so only a file inside a watched folder can be opened.
pub fn openable_file(
    conn: &Connection,
    source: &FileSource,
    path: &[String],
) -> Result<Option<PathBuf>> {
    let Some(name) = path.last() else {
        return Ok(None);
    };
    let openable = FileKind::of(name).is_some_and(|k| k != FileKind::Audio);
    // A name `clean_path` would drop leaves a folder, not this file.
    if !openable || clean_path(path).len() != path.len() {
        return Ok(None);
    }
    local_folder(conn, source, path)
}

/// Where a library song or album is in the Files view: the folder to open
/// and the entry to select in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNavigationTarget {
    pub source: FileSource,
    /// Folder names from the source's root, as [`list_dir`] takes them.
    pub path: Vec<String>,
    /// The entry to select in that folder's listing, by its listed name;
    /// `None` when the target is more than one file (an album) or its file
    /// is gone.
    pub selected_name: Option<String>,
    /// False when the target wasn't where the last scan saw it, and `path`
    /// is the nearest folder that is still there.
    pub exact: bool,
}

/// Where the library song `song_id` is in the Files view. `None` when it has
/// no place there: a stream or radio station, a file opened from outside
/// the library or outside every watched folder, an unknown id. Never a
/// device: device files aren't library songs. Blocks on the disk, so call it
/// off the async runtime.
pub fn locate_song(db: &Database, song_id: i64) -> Result<Option<FileNavigationTarget>> {
    let song = db
        .pool
        .get()?
        .query_row(
            &format!("SELECT {SONG_SELECT_COLS} FROM songs WHERE id = ?1"),
            params![song_id],
            row_to_song,
        )
        .optional()?;
    locate(db, song.as_slice())
}

/// The folder the album `album` is in, as [`locate_song`] finds a song: its
/// songs by the rule every album view uses (the title, any spelling, #1752),
/// in the source holding most of them, at the deepest folder they share
/// there (`Album` for an album split into `Album/CD1` and `Album/CD2`).
pub fn locate_album(db: &Arc<Database>, album: &str) -> Result<Option<FileNavigationTarget>> {
    let songs = CollectionScanner::new(Arc::clone(db)).get_songs_by_album(album)?;
    locate(db, &songs)
}

/// One song's place in the Files view, as the last scan or sync saw it.
struct Place {
    source: FileSource,
    folder: Vec<String>,
    /// `None` for a server's song the last sync didn't find.
    name: Option<String>,
    /// The watched folder, for a local song: where to check it's still there.
    root: Option<PathBuf>,
}

/// Where `songs` are: the source holding most of them (the first such, on a
/// tie), the deepest folder they all share in it, and their file when they
/// are all in the same one (one song, or the tracks of one CUE sheet).
fn locate(db: &Database, songs: &[Song]) -> Result<Option<FileNavigationTarget>> {
    let places = {
        let conn = db.pool.get()?;
        let watched: Vec<(i64, String)> = conn
            .prepare("SELECT id, path FROM directories")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        songs
            .iter()
            .filter_map(|s| place(&conn, &watched, s).transpose())
            .collect::<Result<Vec<Place>>>()?
    };
    let mut groups: Vec<Vec<Place>> = Vec::new();
    for p in places {
        match groups.iter_mut().find(|g| g[0].source == p.source) {
            Some(group) => group.push(p),
            None => groups.push(vec![p]),
        }
    }
    let most = groups
        .into_iter()
        .reduce(|best, g| if g.len() > best.len() { g } else { best });
    let Some(places) = most else {
        return Ok(None);
    };

    let first = &places[0];
    let mut path = first.folder.clone();
    for p in &places[1..] {
        let shared = path.iter().zip(&p.folder).take_while(|(a, b)| a == b);
        path.truncate(shared.count());
    }
    let one_file = places
        .iter()
        .all(|p| p.folder == first.folder && p.name == first.name);
    let mut selected_name = one_file.then(|| first.name.clone()).flatten();
    // One file with no name to select is a server song gone since its sync.
    let mut exact = !one_file || selected_name.is_some();
    // Only a local song can be checked: a server's songs are where its last
    // sync saw them, and asking it again could wait until it times out. The
    // disk is asked with no connection held, as it can be a network drive.
    if let Some(root) = &first.root {
        if let Some(name) = &selected_name {
            if !join_local(root, &path).join(name).exists() {
                selected_name = None;
                exact = false;
            }
        }
        while !path.is_empty() && !join_local(root, &path).is_dir() {
            path.pop();
            exact = false;
        }
    }
    Ok(Some(FileNavigationTarget {
        source: first.source.clone(),
        path,
        selected_name,
        exact,
    }))
}

/// Where `song` is, as its source's listing names it; `None` when it has no
/// place in the Files view. `watched` is every watched folder, by id.
fn place(conn: &Connection, watched: &[(i64, String)], song: &Song) -> Result<Option<Place>> {
    let Some(path) = song.path.as_deref() else {
        return Ok(None);
    };
    let (source, mut names, root) = match song.source {
        s if s.is_local() => {
            // The deepest watched folder holding it: watched folders can nest.
            // A CUE track's `path` is its media file, the entry carrying the
            // song in the listing.
            let deepest = watched
                .iter()
                .filter_map(|(id, root)| {
                    Some((*id, root, library_paths::segments_under(path, root)?))
                })
                .min_by_key(|(_, _, names)| names.len());
            let Some((id, root, names)) = deepest else {
                return Ok(None);
            };
            (FileSource::Folder { id }, names, Some(PathBuf::from(root)))
        }
        SongSource::Subsonic => {
            let Some((id, _)) = crate::subsonic::parse_track_uri(path) else {
                return Ok(None);
            };
            if !server_exists(conn, "subsonic_servers", id)? {
                return Ok(None);
            }
            let (artist, album) = subsonic_place(song);
            let names = vec![artist, album, subsonic_entry_name(song)];
            (FileSource::Subsonic { id }, names, None)
        }
        SongSource::WebDav => {
            let cached = conn
                .query_row(
                    "SELECT c.server_id, s.remote_path, c.remote_path FROM webdav_cache c
                     JOIN webdav_servers s ON s.id = c.server_id
                     WHERE c.song_id = ?1",
                    params![song.id],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            let Some((id, names)) =
                cached.and_then(|(id, music, href)| Some((id, webdav_names_under(&music, &href)?)))
            else {
                return Ok(None);
            };
            (FileSource::Webdav { id }, names, None)
        }
        _ => return Ok(None),
    };
    // A server's song its last sync didn't find isn't in the listing, so
    // there is nothing to select; a local file's presence is checked on disk
    // by `locate` instead, whatever the last scan said.
    let name = names.pop().filter(|_| root.is_some() || !song.unavailable);
    Ok(Some(Place {
        source,
        folder: names,
        name,
        root,
    }))
}

/// `path` without the names that would leave the source or mean nothing, for
/// sources that are real folder trees: only plain names are kept, so a `..`,
/// a separator or a drive (`C:`, which replaces a Windows path it is joined
/// to) never is. Not for Subsonic, whose names are tags and may be empty or
/// hold a `/` ("AC/DC").
fn clean_path(path: &[String]) -> Vec<String> {
    let is_plain_name = |p: &str| {
        let mut parts = Path::new(p).components();
        !p.contains(['/', '\\'])
            && matches!(
                (parts.next(), parts.next()),
                (Some(std::path::Component::Normal(_)), None)
            )
    };
    path.iter()
        .map(|p| p.trim())
        .filter(|p| is_plain_name(p))
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
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|s| {
            [s.path.as_deref(), s.cue_path.as_deref()]
                .into_iter()
                .flatten()
                .any(|p| library_paths::is_under_key(&library_paths::key(p), &folder_key))
        })
        .collect();
    Ok(songs)
}

fn escape_like(s: &str) -> String {
    s.replace('!', "!!").replace('%', "!%").replace('_', "!_")
}

fn extension(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
}

/// The kind of the file named `name`; a library song is audio whatever its
/// name says.
fn kind_of(name: &str, is_song: bool) -> Option<FileKind> {
    if is_song {
        Some(FileKind::Audio)
    } else {
        FileKind::of(name)
    }
}

/// The status of a file of `kind` that is not a library song.
fn status_by_kind(kind: Option<FileKind>) -> EntryStatus {
    match kind {
        Some(FileKind::Audio) => EntryStatus::NotScanned,
        Some(FileKind::Image) => EntryStatus::AlbumArt,
        _ => EntryStatus::NotAudio,
    }
}

fn file_type(name: &str) -> Option<String> {
    extension(name).map(|e| e.to_uppercase())
}

fn list_folder(db: &Database, id: i64, path: &[String]) -> Result<DirListing> {
    let root = folder_root(&*db.pool.get()?, id)?;
    let dir = join_local(&root, path);
    // A music folder can be on a network drive: read it with no connection held.
    let raw = FsBackend::new(&root).list(&path.join("/"))?;
    let conn = db.pool.get()?;
    let conn = &*conn;
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
                    modified: e.modified,
                    location: Some(dir.join(&e.name).to_string_lossy().into_owned()),
                    ..FileEntry::folder(e.name)
                };
            }
            let location = dir.join(&e.name).to_string_lossy().into_owned();
            let key = library_paths::key(&location);
            let song = songs.get(&key);
            let kind = kind_of(&e.name, song.is_some());
            let status = if in_reserved || song.is_some_and(|(_, not_included)| *not_included) {
                EntryStatus::Excluded
            } else if song.is_some() || cue_sheets.contains(&key) {
                EntryStatus::InLibrary
            } else {
                status_by_kind(kind)
            };
            FileEntry {
                file_type: file_type(&e.name),
                kind,
                status: Some(status),
                song_id: song.map(|(id, _)| *id),
                missing_tag: None,
                size: Some(e.size),
                modified: e.modified,
                name: e.name,
                is_dir: false,
                location: Some(location),
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

/// Where `path`, a location the Files view browses on a device, is on the
/// storage: the profile's music folder is the device's root there.
pub fn device_path(profile: &DeviceProfile, path: &[String]) -> String {
    profile
        .music_path
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .chain(clean_path(path))
        .collect::<Vec<_>>()
        .join("/")
}

/// The device folder `dir` (storage-relative), told apart by the storage's
/// sync manifest: what sync didn't copy is `NotFromLuminous`, a synced
/// track still in the library carries its song, to play the library copy.
fn list_device(db: &Database, storage: &dyn DeviceBackend, dir: &str) -> Result<DirListing> {
    let listed = storage.list(dir)?;
    let manifest = Manifest::read(storage)?;
    let full = |name: &str| {
        if dir.is_empty() {
            name.to_string()
        } else {
            format!("{dir}/{name}")
        }
    };
    let synced_folders: HashSet<String> = manifest
        .entries()
        .flat_map(|e| {
            let path = e.path.to_lowercase();
            path.match_indices('/')
                .map(|(i, _)| path[..i].to_string())
                .collect::<Vec<_>>()
        })
        .collect();

    let entries: Vec<(FileEntry, Option<String>)> = listed
        .into_iter()
        .filter(|e| {
            !is_partial_file(&e.name)
                && (e.is_dir || !is_system_clutter(&e.name))
                && !(dir.is_empty() && [MANIFEST_FILE_NAME, MARKER_FILE_NAME].contains(&&*e.name))
        })
        .map(|e| {
            let path = full(&e.name);
            if e.is_dir {
                let synced = synced_folders.contains(&path.to_lowercase());
                let entry = FileEntry {
                    status: (!synced).then_some(EntryStatus::NotFromLuminous),
                    modified: e.modified,
                    ..FileEntry::folder(e.name)
                };
                return (entry, None);
            }
            let synced = manifest.get(&path);
            let library_path = synced
                .filter(|m| m.kind == EntryKind::Track)
                .and_then(|m| m.source.as_ref())
                .map(|s| s.path.clone());
            let entry = FileEntry {
                file_type: file_type(&e.name),
                kind: kind_of(&e.name, library_path.is_some()),
                status: synced.is_none().then_some(EntryStatus::NotFromLuminous),
                song_id: None,
                missing_tag: None,
                location: None,
                size: Some(e.size),
                modified: e.modified,
                name: e.name,
                is_dir: false,
            };
            (entry, library_path)
        })
        .collect();

    let conn = db.pool.get()?;
    let mut song_at = conn.prepare(&format!(
        "SELECT id FROM songs WHERE path = ?1 AND source IN ({}) AND unavailable = 0
         ORDER BY id LIMIT 1",
        *LOCAL_SOURCES_SQL
    ))?;
    let mut files = Vec::with_capacity(entries.len());
    for (mut entry, library_path) in entries {
        if let Some(library_path) = library_path {
            entry.song_id = song_at
                .query_row(params![library_path], |r| r.get(0))
                .optional()?;
            if entry.song_id.is_some() {
                entry.status = Some(EntryStatus::InLibrary);
            }
        }
        files.push(entry);
    }
    Ok(finish(None, files))
}

/// Sorts `entries` (folders, then files, by name) and totals the files.
fn finish(location: Option<String>, mut entries: Vec<FileEntry>) -> DirListing {
    entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()));
    listing(location, entries)
}

/// Totals the files of `entries`, keeping their order. Every listing goes
/// through here, which is what keeps files of no [`FileKind`] out of all of
/// them. System clutter (`Thumbs.db`) isn't counted as hidden: nobody put
/// it there.
fn listing(location: Option<String>, mut entries: Vec<FileEntry>) -> DirListing {
    let mut hidden_count = 0;
    entries.retain(|e| {
        let shown = e.is_dir || e.kind.is_some();
        if !shown && !is_system_clutter(&e.name) {
            hidden_count += 1;
        }
        shown
    });
    let files = entries.iter().filter(|e| !e.is_dir);
    let total_bytes = files
        .clone()
        .map(|e| e.size)
        .try_fold(0u64, |sum, size| size.map(|s| sum + s));
    DirListing {
        location,
        file_count: files.clone().count(),
        hidden_count,
        total_bytes,
        in_library_count: files
            .filter(|e| e.status == Some(EntryStatus::InLibrary) && e.song_id.is_some())
            .count(),
        entries,
        excluded: false,
        path_missing_tags: Vec::new(),
    }
}

fn subsonic_songs(conn: &Connection, id: i64) -> Result<Vec<Song>> {
    if !server_exists(conn, "subsonic_servers", id)? {
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
        .collect::<rusqlite::Result<_>>()?;
    Ok(songs)
}

/// Where a Subsonic song sits in the Artists > Albums > Tracks index: the
/// artist and album folder names, `""` for a missing tag. Listing, Play
/// folder and anything that finds a song's folder must all use this, so a
/// path one of them gives is one the others open.
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

/// The tag a Subsonic folder named `name` at `depth` (0 for artists, 1 for
/// albums) stands for, when it is the folder of songs missing it.
fn subsonic_missing_tag(depth: usize, name: &str) -> Option<MissingTag> {
    match depth {
        0 if name.is_empty() => Some(MissingTag::Artist),
        1 if name.is_empty() => Some(MissingTag::Album),
        _ => None,
    }
}

/// A Subsonic track's name in its album's listing.
fn subsonic_entry_name(song: &Song) -> String {
    match song.track {
        Some(n) => format!("{n:02} {}", song.display_title()),
        None => song.display_title().to_string(),
    }
}

/// Subsonic has no file tree, so its songs are browsed as artists, then
/// albums, then tracks. Sizes aren't shown and there's nothing but audio.
fn list_subsonic(conn: &Connection, id: i64, path: &[String]) -> Result<DirListing> {
    let songs = subsonic_songs(conn, id)?;
    let path_missing_tags = path
        .iter()
        .enumerate()
        .map(|(depth, name)| subsonic_missing_tag(depth, name))
        .collect();
    let folder = |name: String| FileEntry {
        missing_tag: subsonic_missing_tag(path.len(), &name),
        ..FileEntry::folder(name)
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
            // The id keeps same-titled untagged tracks apart.
            let mut tracks: BTreeMap<(i32, i32, String, i64), &Song> = BTreeMap::new();
            for s in songs
                .iter()
                .filter(|s| subsonic_place(s) == (artist.clone(), album.clone()))
            {
                tracks.insert(
                    (
                        s.disc.unwrap_or(0),
                        s.track.unwrap_or(0),
                        s.display_title().to_lowercase(),
                        s.id,
                    ),
                    s,
                );
            }
            let entries: Vec<FileEntry> = tracks
                .into_values()
                .map(|s| FileEntry {
                    name: subsonic_entry_name(s),
                    is_dir: false,
                    size: None,
                    modified: None,
                    file_type: Some(format!("{:?}", s.filetype).to_uppercase())
                        .filter(|t| t != "UNKNOWN"),
                    kind: Some(FileKind::Audio),
                    status: Some(EntryStatus::InLibrary),
                    song_id: Some(s.id),
                    missing_tag: None,
                    location: None,
                })
                .collect();
            // Already in disc and track order, which name order would break.
            return Ok(DirListing {
                path_missing_tags,
                ..listing(None, entries)
            });
        }
    };
    Ok(DirListing {
        path_missing_tags,
        ..finish(None, entries)
    })
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

/// The names leading from the server's music folder `remote_path` down to
/// the file at `href`, decoded as the listing shows them; the inverse of
/// [`webdav_dir_href`]. `None` when `href` isn't below the music folder.
fn webdav_names_under(remote_path: &str, href: &str) -> Option<Vec<String>> {
    // Split before decoding: a name can hold an encoded `/`.
    let names = |p: &str| -> Vec<String> {
        p.split('/')
            .filter(|s| !s.is_empty())
            .map(|s| percent_decode_str(s).decode_utf8_lossy().into_owned())
            .collect()
    };
    let (mut file, music) = (names(href), names(remote_path));
    (file.len() > music.len() && file.starts_with(&music)).then(|| file.split_off(music.len()))
}

fn list_webdav(db: &Database, id: i64, path: &[String]) -> Result<DirListing> {
    // Everything the database knows first; the connection goes back to the
    // pool before the server is asked, which can take until it times out.
    let (server, songs) = {
        let conn = db.pool.get()?;
        let server = webdav_server(&conn, id)?;
        let mut stmt = conn.prepare(
            "SELECT c.remote_path, s.id, s.not_included FROM webdav_cache c
             JOIN songs s ON s.id = c.song_id
             WHERE c.server_id = ?1 AND s.unavailable = 0",
        )?;
        let songs: HashMap<String, (i64, bool)> = stmt
            .query_map(params![id], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
            .collect::<rusqlite::Result<_>>()?;
        (server, songs)
    };
    let dir = webdav_dir_href(&server.remote_path, path);
    let client = crate::webdav::WebDavClient::new(server.url, server.username, server.password)?;
    let items = client.list_directory(&dir).context("server-unreachable")?;

    let entries = items
        .into_iter()
        .filter(|item| !crate::webdav::is_listed_collection(&item.href, &dir))
        .filter_map(|item| {
            let encoded = item.href.trim_end_matches('/').rsplit('/').next()?;
            let name = percent_decode_str(encoded).decode_utf8_lossy().into_owned();
            if item.is_directory {
                return Some(FileEntry::folder(name));
            }
            let song = songs.get(&item.href);
            let kind = kind_of(&name, song.is_some());
            let status = match song {
                Some((_, true)) => EntryStatus::Excluded,
                Some(_) => EntryStatus::InLibrary,
                None => status_by_kind(kind),
            };
            Some(FileEntry {
                file_type: file_type(&name),
                kind,
                size: item.content_length,
                modified: None,
                status: Some(status),
                song_id: song.map(|(id, _)| *id),
                missing_tag: None,
                location: None,
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
        let path: Vec<String> = path.iter().map(|p| p.to_string()).collect();
        list_dir(&f.db, &FileSource::Folder { id: 1 }, &path, |_, _| None).unwrap()
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
        assert_eq!(album.hidden_count, 0);
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

    #[test]
    fn only_kinds_luminous_uses_are_listed() {
        let f = fixture(
            &[
                "01.flac",
                "lyrics.LRC",
                "album.md",
                "booklet.pdf",
                "rip.cue",
                "mix.m3u8",
                "setup.exe",
                "site.url",
                "checksums.sfv",
                "Thumbs.db",
            ],
            &["01.flac"],
        );
        let root = list(&f, &[]);
        let kind = |name: &str| {
            root.entries
                .iter()
                .find(|e| e.name == name)
                .and_then(|e| e.kind)
        };
        assert_eq!(kind("01.flac"), Some(FileKind::Audio));
        assert_eq!(kind("lyrics.LRC"), Some(FileKind::Lyrics));
        assert_eq!(kind("album.md"), Some(FileKind::Notes));
        assert_eq!(kind("booklet.pdf"), Some(FileKind::Pdf));
        assert_eq!(kind("rip.cue"), Some(FileKind::CueSheet));
        assert_eq!(kind("mix.m3u8"), Some(FileKind::Playlist));
        let names: Vec<&str> = root.entries.iter().map(|e| e.name.as_str()).collect();
        for gone in ["setup.exe", "site.url", "checksums.sfv", "Thumbs.db"] {
            assert!(!names.contains(&gone), "{gone} is listed");
        }
        assert_eq!(root.file_count, 6);
        assert_eq!(root.total_bytes, Some(6));
        // Thumbs.db is clutter, not a file anyone put there.
        assert_eq!(root.hidden_count, 3);
    }

    #[test]
    fn a_path_never_leaves_its_source() {
        let path =
            |names: &[&str]| clean_path(&names.iter().map(|n| n.to_string()).collect::<Vec<_>>());
        assert_eq!(path(&["A", "..", "B"]), vec!["A", "B"]);
        assert_eq!(path(&["A/..", "B\\C", ".", ""]), Vec::<String>::new());
        #[cfg(windows)]
        assert_eq!(
            path(&["C:", "Windows", "notepad.txt"]),
            vec!["Windows", "notepad.txt"]
        );
    }

    #[test]
    fn only_listed_sidecars_in_a_watched_folder_open() {
        let f = fixture(&["A/notes.txt", "A/01.flac", "A/setup.exe"], &[]);
        let conn = f.db.pool.get().unwrap();
        let open = |source: FileSource, names: &[&str]| {
            let path: Vec<String> = names.iter().map(|n| n.to_string()).collect();
            openable_file(&conn, &source, &path).unwrap()
        };
        let folder = FileSource::Folder { id: 1 };
        assert_eq!(
            open(folder.clone(), &["A", "notes.txt"]),
            Some(f.root.join("A").join("notes.txt"))
        );
        assert_eq!(open(folder.clone(), &["A", "01.flac"]), None);
        assert_eq!(open(folder.clone(), &["A", "setup.exe"]), None);
        assert_eq!(open(folder.clone(), &["A", "..", "notes.txt"]), None);
        assert_eq!(open(folder, &[]), None);
        assert_eq!(open(FileSource::Webdav { id: 1 }, &["notes.txt"]), None);
    }

    #[test]
    fn local_entries_carry_their_real_path() {
        let f = fixture(&["A/Album/01.flac"], &[]);
        let location = |listing: &DirListing, name: &str| {
            listing
                .entries
                .iter()
                .find(|e| e.name == name)
                .and_then(|e| e.location.clone())
        };
        let album_dir = f.root.join("A").join("Album");
        assert_eq!(
            location(&list(&f, &["A", "Album"]), "01.flac"),
            Some(album_dir.join("01.flac").to_string_lossy().into_owned())
        );
        assert_eq!(
            location(&list(&f, &["A"]), "Album"),
            Some(album_dir.to_string_lossy().into_owned())
        );
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
        let err = list_dir(&f.db, &FileSource::Folder { id: 9 }, &[], |_, _| None).unwrap_err();
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
            list_dir(&f.db, &source, &path, |_, _| None)
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

    fn song_id(f: &Fixture, title: &str) -> i64 {
        f.db.pool
            .get()
            .unwrap()
            .query_row(
                "SELECT id FROM songs WHERE title = ?1",
                params![title],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn target(
        source: FileSource,
        path: &[&str],
        selected_name: Option<&str>,
        exact: bool,
    ) -> Option<FileNavigationTarget> {
        Some(FileNavigationTarget {
            source,
            path: path.iter().map(|p| p.to_string()).collect(),
            selected_name: selected_name.map(str::to_string),
            exact,
        })
    }

    /// Files the songs of `f` titled `titles` as album `album`.
    fn set_album(f: &Fixture, album: &str, titles: &[&str]) {
        let conn = f.db.pool.get().unwrap();
        for title in titles {
            conn.execute(
                "UPDATE songs SET album = ?1, album_key = ?2 WHERE title = ?3",
                params![album, crate::album_key::album_key(album), title],
            )
            .unwrap();
        }
    }

    #[test]
    fn a_song_is_found_in_its_folder() {
        let f = fixture(&["A/Album/01.flac"], &["A/Album/01.flac"]);
        assert_eq!(
            locate_song(&f.db, song_id(&f, "A/Album/01.flac")).unwrap(),
            target(
                FileSource::Folder { id: 1 },
                &["A", "Album"],
                Some("01.flac"),
                true
            )
        );
    }

    #[test]
    fn a_song_is_found_in_the_deepest_watched_folder_holding_it() {
        let f = fixture(&["A/Album/01.flac"], &["A/Album/01.flac"]);
        f.db.pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO directories (id, path, subdirs) VALUES (2, ?1, 1)",
                params![f.root.join("A").to_string_lossy()],
            )
            .unwrap();
        assert_eq!(
            locate_song(&f.db, song_id(&f, "A/Album/01.flac")).unwrap(),
            target(
                FileSource::Folder { id: 2 },
                &["Album"],
                Some("01.flac"),
                true
            )
        );
    }

    #[test]
    fn a_moved_song_falls_back_to_the_nearest_folder_still_there() {
        let f = fixture(
            &["A/other.flac"],
            &["A/Album/gone.flac", "A/x.flac", "B/y.flac"],
        );
        assert_eq!(
            locate_song(&f.db, song_id(&f, "A/Album/gone.flac")).unwrap(),
            target(FileSource::Folder { id: 1 }, &["A"], None, false)
        );
        assert_eq!(
            locate_song(&f.db, song_id(&f, "A/x.flac")).unwrap(),
            target(FileSource::Folder { id: 1 }, &["A"], None, false)
        );
        assert_eq!(
            locate_song(&f.db, song_id(&f, "B/y.flac")).unwrap(),
            target(FileSource::Folder { id: 1 }, &[], None, false)
        );
    }

    #[test]
    fn songs_with_no_place_in_files_are_not_found() {
        let f = fixture(&[], &[]);
        let conn = f.db.pool.get().unwrap();
        for (source, path, title) in [
            (SongSource::LocalFile, "/elsewhere/x.flac", "outside"),
            (
                SongSource::OpenedFile,
                &*f.root.join("o.flac").to_string_lossy(),
                "opened",
            ),
            (SongSource::Stream, "http://radio/stream", "stream"),
        ] {
            conn.execute(
                "INSERT INTO songs (source, path, title) VALUES (?1, ?2, ?3)",
                params![source as i32, path, title],
            )
            .unwrap();
        }
        drop(conn);
        for title in ["outside", "opened", "stream"] {
            assert_eq!(
                locate_song(&f.db, song_id(&f, title)).unwrap(),
                None,
                "{title}"
            );
        }
        assert_eq!(locate_song(&f.db, 999).unwrap(), None);
        assert_eq!(locate_album(&f.db, "No Such Album").unwrap(), None);
    }

    #[test]
    fn an_album_is_found_in_the_folder_its_discs_share() {
        let f = fixture(
            &["X/CD1/01.flac", "X/CD2/01.flac"],
            &["X/CD1/01.flac", "X/CD2/01.flac"],
        );
        set_album(&f, "Live", &["X/CD1/01.flac", "X/CD2/01.flac"]);
        assert_eq!(
            locate_album(&f.db, "LIVE ").unwrap(),
            target(FileSource::Folder { id: 1 }, &["X"], None, true)
        );
    }

    #[test]
    fn a_cue_album_selects_its_media_file() {
        let f = fixture(&["X/album.flac", "X/album.cue"], &["X/album.flac"]);
        let conn = f.db.pool.get().unwrap();
        // A second track of the same media file, starting a minute in.
        conn.execute(
            "INSERT INTO songs (source, path, title, beginning_nanosec)
             SELECT source, path, 'track 2', 60000000000 FROM songs",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE songs SET album = 'Live', album_key = 'live', cue_path = ?1",
            params![f.root.join("X").join("album.cue").to_string_lossy()],
        )
        .unwrap();
        drop(conn);
        assert_eq!(
            locate_album(&f.db, "Live").unwrap(),
            target(
                FileSource::Folder { id: 1 },
                &["X"],
                Some("album.flac"),
                true
            )
        );
    }

    #[test]
    fn subsonic_songs_are_found_by_their_listed_names() {
        let f = fixture(&[], &[]);
        let conn = f.db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO subsonic_servers (id, name, url, username) VALUES (3, 'Navidrome', 'http://x', 'u')",
            [],
        )
        .unwrap();
        for (unavailable, track, title) in [(0, 1, "Ghost of the Sun"), (1, 2, "Wealth")] {
            conn.execute(
                "INSERT INTO songs (source, path, artist, album, track, title, unavailable)
                 VALUES (?1, ?2, 'Katatonia', 'Viva Emptiness', ?3, ?4, ?5)",
                params![
                    SongSource::Subsonic as i32,
                    format!("subsonic://3/t{track}"),
                    track,
                    title,
                    unavailable
                ],
            )
            .unwrap();
        }
        drop(conn);
        let source = FileSource::Subsonic { id: 3 };
        let found = locate_song(&f.db, song_id(&f, "Ghost of the Sun"))
            .unwrap()
            .unwrap();
        assert_eq!(
            Some(found.clone()),
            target(
                source.clone(),
                &["Katatonia", "Viva Emptiness"],
                Some("01 Ghost of the Sun"),
                true
            )
        );
        let listed = list_dir(&f.db, &source, &found.path, |_, _| None).unwrap();
        assert!(listed
            .entries
            .iter()
            .any(|e| Some(&e.name) == found.selected_name.as_ref()));
        // The last sync didn't find it: its album, nothing to select.
        assert_eq!(
            locate_song(&f.db, song_id(&f, "Wealth")).unwrap(),
            target(source, &["Katatonia", "Viva Emptiness"], None, false)
        );
    }

    #[test]
    fn webdav_songs_are_found_by_their_decoded_names() {
        let f = fixture(&[], &[]);
        let conn = f.db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO webdav_servers (id, name, url, remote_path) VALUES (4, 'NAS', 'https://nas', '/dav/Music/')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO songs (source, path, title) VALUES (?1, 'https://nas/x', 'dav')",
            params![SongSource::WebDav as i32],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO webdav_cache (server_id, remote_path, song_id)
             VALUES (4, '/dav/Music/AC%2FDC/Live%20%231/01%20Intro.flac', last_insert_rowid())",
            [],
        )
        .unwrap();
        drop(conn);
        assert_eq!(
            locate_song(&f.db, song_id(&f, "dav")).unwrap(),
            target(
                FileSource::Webdav { id: 4 },
                &["AC/DC", "Live #1"],
                Some("01 Intro.flac"),
                true
            )
        );
    }

    #[test]
    fn webdav_names_are_the_inverse_of_folder_hrefs() {
        let path = vec!["AC/DC?".to_string(), "Live #1".to_string()];
        let href = format!("{}01.flac", webdav_dir_href("/dav", &path));
        let mut names = path.clone();
        names.push("01.flac".into());
        assert_eq!(webdav_names_under("/dav/", &href), Some(names));
        assert_eq!(webdav_names_under("/other", &href), None);
    }

    /// Subsonic folder names are tags, so a missing tag (`""`) or one a
    /// filesystem path couldn't hold ("AC/DC") still opens and plays.
    #[test]
    fn subsonic_folders_open_whatever_their_tags() {
        let f = fixture(&[], &[]);
        let conn = f.db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO subsonic_servers (id, name, url, username) VALUES (3, 'Navidrome', 'http://x', 'u')",
            [],
        )
        .unwrap();
        for (id, artist, album, title) in [
            ("t1", None, None, "Untitled"),
            ("t2", Some("  "), Some("Demos"), "Demo"),
            ("t3", Some("AC/DC"), None, "Thunderstruck"),
        ] {
            conn.execute(
                "INSERT INTO songs (source, path, artist, album, title) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![SongSource::Subsonic as i32, format!("subsonic://3/{id}"), artist, album, title],
            )
            .unwrap();
        }
        let source = FileSource::Subsonic { id: 3 };
        let open = |path: &[&str]| -> DirListing {
            let path: Vec<String> = path.iter().map(|p| p.to_string()).collect();
            list_dir(&f.db, &source, &path, |_, _| None).unwrap()
        };
        let folders = |listing: &DirListing| -> Vec<(String, Option<MissingTag>)> {
            listing
                .entries
                .iter()
                .map(|e| (e.name.clone(), e.missing_tag))
                .collect()
        };
        let root = open(&[]);
        assert_eq!(
            folders(&root),
            [
                (String::new(), Some(MissingTag::Artist)),
                ("AC/DC".into(), None)
            ]
        );
        assert!(root.path_missing_tags.is_empty());

        let unknown = open(&[""]);
        assert_eq!(
            folders(&unknown),
            [
                (String::new(), Some(MissingTag::Album)),
                ("Demos".into(), None)
            ]
        );
        assert_eq!(unknown.path_missing_tags, [Some(MissingTag::Artist)]);

        let tracks = open(&["", ""]);
        assert_eq!(folders(&tracks), [("Untitled".into(), None)]);
        assert_eq!(
            tracks.path_missing_tags,
            [Some(MissingTag::Artist), Some(MissingTag::Album)]
        );
        assert_eq!(
            folders(&open(&["AC/DC"])),
            [(String::new(), Some(MissingTag::Album))]
        );
        assert_eq!(open(&["AC/DC", ""]).entries.len(), 1);

        let titles = |path: &[&str]| -> Vec<String> {
            let path: Vec<String> = path.iter().map(|p| p.to_string()).collect();
            songs_under(&conn, &source, &path)
                .unwrap()
                .into_iter()
                .filter_map(|s| s.title)
                .collect()
        };
        assert_eq!(titles(&[""]), ["Untitled", "Demo"]);
        assert_eq!(titles(&["", ""]), ["Untitled"]);
        assert_eq!(titles(&["AC/DC"]), ["Thunderstruck"]);

        // The resolver finds them where the listing puts them.
        drop(conn);
        for (title, path) in [("Untitled", ["", ""]), ("Thunderstruck", ["AC/DC", ""])] {
            let found = locate_song(&f.db, song_id(&f, title)).unwrap();
            assert_eq!(found, target(source.clone(), &path, Some(title), true));
            let listed = list_dir(&f.db, &source, &found.unwrap().path, |_, _| None).unwrap();
            assert!(listed.entries.iter().any(|e| e.name == title));
        }
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
