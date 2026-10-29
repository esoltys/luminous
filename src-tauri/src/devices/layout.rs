//! Where synced files go on a device, and the playlist and folder-art files
//! sync writes beside them (#837, epic #834).
//!
//! [`DeviceLayout`] is the one place that knows a device's naming quirks:
//! the profile's folder template, FAT-safe names, the playlist format,
//! separator and encoding, and the art file names. Sync (#836) copies each
//! track to its planned path and writes the files the layout hands it,
//! without interpreting any of them. Planning reads no files, so a live sync
//! preview can re-plan on every change.

use crate::album_key::album_key;
use crate::collection::{row_to_song, SONG_SELECT_COLS};
use crate::covermanager::{clean_or_encode_to_jpeg, CoverManager};
use crate::models::{Song, LOCAL_SOURCES_SQL};
use crate::organizer::{
    apply_dir_casing_map, build_dir_casing_map, expand_template, sanitize_component,
};
use anyhow::Result;
use encoding_rs::WINDOWS_1252;
use luminous_devices::{DeviceProfile, LayoutSettings, PathSeparator, PlaylistFileFormat};
use luminous_metadata::playlist::{render_playlist, LineEnding, PlaylistEntry, PlaylistFormat};
use rusqlite::OptionalExtension;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Longest file or folder name FAT, exFAT and players accept, in bytes.
const MAX_NAME_BYTES: usize = 255;
/// Room left in a shortened name for a " (n)" suffix.
const SUFFIX_ROOM: usize = 8;
/// Quality of folder art that has to be re-encoded.
const ART_JPEG_QUALITY: u8 = 90;

/// A song sync puts on the device.
pub struct SyncedTrack<'a> {
    pub song: &'a Song,
    /// Extension of the file on the device, without the dot: the converted
    /// format's when sync converts it (`luminous_transcode::Prepared`).
    pub extension: &'a str,
}

/// A playlist sync writes to the device.
pub struct DevicePlaylist<'a> {
    pub name: &'a str,
    /// In playlist order. Songs the layout wasn't planned with are left out.
    pub songs: &'a [Song],
}

/// A small file to write on the device, complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFile {
    /// Relative to the storage root, `/`-separated.
    pub path: String,
    pub content: Vec<u8>,
}

/// An album-art file to write in an album's folder on the device. Its bytes
/// come from [`folder_art_jpeg`] when sync writes it, so planning stays cheap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderArtFile {
    /// Relative to the storage root, `/`-separated.
    pub path: String,
    /// A library file of the album to take the art from.
    pub album_track: PathBuf,
}

/// Where every synced track goes on one device, and the files that go with
/// them.
#[derive(Debug)]
pub struct DeviceLayout {
    settings: LayoutSettings,
    playlists_dir: Vec<String>,
    /// Song id to path relative to the storage root, `/`-separated.
    tracks: HashMap<i64, String>,
    folder_art: Vec<FolderArtFile>,
}

/// A track's planned place before collisions are resolved.
struct Draft<'a> {
    song: &'a Song,
    dirs: Vec<String>,
    stem: String,
    extension: String,
}

impl DeviceLayout {
    /// Places `tracks` on a device with `profile`: under `music_path`,
    /// following the folder template, each name FAT-safe (and, for
    /// Windows-1252 playlists, made of characters that encoding has).
    /// Folders whose names differ only in case get one spelling, and tracks
    /// that would land on the same file get " (2)", " (3)"... in song-id
    /// order, so the same songs get the same paths on every sync.
    pub fn plan(profile: &DeviceProfile, tracks: &[SyncedTrack]) -> Self {
        let settings = profile.layout.clone();
        let fold = settings.playlist_format == PlaylistFileFormat::M3uWindows1252;
        let music_dir = path_parts(&profile.music_path);

        let mut ordered: Vec<&SyncedTrack> = tracks.iter().collect();
        ordered.sort_by_key(|t| t.song.id);
        ordered.dedup_by_key(|t| t.song.id);

        let mut drafts: Vec<Draft> = ordered
            .iter()
            .map(|t| draft(t, &settings.folder_template, fold))
            .collect();
        let all_dirs: Vec<Vec<String>> = drafts.iter().map(|d| d.dirs.clone()).collect();
        let casing = build_dir_casing_map(&all_dirs);
        for d in &mut drafts {
            apply_dir_casing_map(&mut d.dirs, &casing);
        }

        let mut taken: HashSet<String> = HashSet::new();
        let mut tracks = HashMap::with_capacity(drafts.len());
        for d in &drafts {
            let dir: Vec<&str> = music_dir
                .iter()
                .map(String::as_str)
                .chain(d.dirs.iter().map(String::as_str))
                .collect();
            let path = unique_path(&dir, &d.stem, Some(&d.extension), &mut taken);
            tracks.insert(d.song.id, path);
        }

        let folder_art = plan_folder_art(&settings, &drafts, &tracks);
        Self {
            settings,
            playlists_dir: path_parts(&profile.playlists_path),
            tracks,
            folder_art,
        }
    }

    /// Where `song_id` goes on the device, relative to the storage root.
    pub fn track_path(&self, song_id: i64) -> Option<&str> {
        self.tracks.get(&song_id).map(String::as_str)
    }

    /// The playlist files to write under `playlists_path`, in the profile's
    /// format, each entry relative to the playlist file. Empty when the
    /// profile writes no playlist files. Playlists whose names clash on the
    /// device get " (2)", " (3)"... in the order given.
    pub fn playlist_files(&self, playlists: &[DevicePlaylist]) -> Vec<DeviceFile> {
        let (format, extension) = match self.settings.playlist_format {
            PlaylistFileFormat::M3u8 => (PlaylistFormat::M3u8, "m3u8"),
            PlaylistFileFormat::M3uWindows1252 => (PlaylistFormat::M3u, "m3u"),
            PlaylistFileFormat::Pls => (PlaylistFormat::Pls, "pls"),
            PlaylistFileFormat::None => return Vec::new(),
        };
        let fold = self.settings.playlist_format == PlaylistFileFormat::M3uWindows1252;
        let (separator, line_ending) = match self.settings.path_separator {
            PathSeparator::Slash => ("/", LineEnding::Lf),
            PathSeparator::Backslash => ("\\", LineEnding::CrLf),
        };
        let dir: Vec<&str> = self.playlists_dir.iter().map(String::as_str).collect();

        let mut taken = HashSet::new();
        playlists
            .iter()
            .map(|playlist| {
                let stem = device_name(playlist.name, fold);
                let path = unique_path(&dir, &stem, Some(extension), &mut taken);
                let entries: Vec<PlaylistEntry> = playlist
                    .songs
                    .iter()
                    .filter_map(|song| {
                        let track = self.tracks.get(&song.id)?;
                        Some(PlaylistEntry {
                            location: relative_location(&self.playlists_dir, track, separator),
                            title: song.title.as_deref(),
                            artist: song.artist.as_deref(),
                            album: song.album.as_deref(),
                            duration_sec: song.length_nanosec.map(|ns| ns / 1_000_000_000),
                        })
                    })
                    .collect();
                let text = render_playlist(playlist.name, &entries, format, line_ending);
                let content = if fold {
                    encode_windows_1252(&text)
                } else {
                    text.into_bytes()
                };
                DeviceFile { path, content }
            })
            .collect()
    }

    /// The art files the profile asks for, one set per album folder. A
    /// folder holding tracks of more than one album, or of none, gets none:
    /// a player would show that art for the wrong songs.
    pub fn folder_art(&self) -> &[FolderArtFile] {
        &self.folder_art
    }
}

fn draft<'a>(track: &SyncedTrack<'a>, template: &str, fold: bool) -> Draft<'a> {
    let extension = track.extension.trim_start_matches('.').to_ascii_lowercase();
    let expanded = expand_template(template, track.song, &extension);
    let mut parts: Vec<String> = expanded
        .split(['/', '\\'])
        .filter(|p| !p.trim().is_empty())
        .map(|p| device_name(p, fold))
        .collect();
    let mut stem = parts.pop().unwrap_or_else(|| "Unknown".into());
    // A template ending in `.%extension` already names the extension.
    if let Some(cut) = stem.len().checked_sub(extension.len() + 1) {
        if stem.is_char_boundary(cut) && stem[cut..].eq_ignore_ascii_case(&format!(".{extension}"))
        {
            stem.truncate(cut);
        }
    }
    Draft {
        song: track.song,
        dirs: parts,
        stem: truncate_name(&stem, MAX_NAME_BYTES - SUFFIX_ROOM - extension.len() - 1),
        extension,
    }
}

/// One file or folder name, safe on FAT and exFAT and within the length
/// limit; in Windows-1252 characters when `fold`.
fn device_name(raw: &str, fold: bool) -> String {
    let name = if fold {
        fold_to_windows_1252(raw)
    } else {
        raw.to_string()
    };
    truncate_name(
        &sanitize_component(&name, false, false),
        MAX_NAME_BYTES - SUFFIX_ROOM,
    )
}

/// `dir/stem.extension` (or `dir/stem` without one), with " (n)" added to
/// the stem until no path in `taken` matches it ignoring case, as FAT and
/// MTP do. Records the result in `taken`.
fn unique_path(
    dir: &[&str],
    stem: &str,
    extension: Option<&str>,
    taken: &mut HashSet<String>,
) -> String {
    let join = |name: &str| {
        let file = match extension {
            Some(ext) => format!("{name}.{ext}"),
            None => name.to_string(),
        };
        dir.iter()
            .copied()
            .chain(std::iter::once(file.as_str()))
            .collect::<Vec<_>>()
            .join("/")
    };
    let mut path = join(stem);
    let mut n = 2;
    while !taken.insert(path.to_lowercase()) {
        path = join(&format!("{stem} ({n})"));
        n += 1;
    }
    path
}

fn plan_folder_art(
    settings: &LayoutSettings,
    drafts: &[Draft],
    tracks: &HashMap<i64, String>,
) -> Vec<FolderArtFile> {
    let names = settings.folder_art.file_names();
    if names.is_empty() {
        return Vec::new();
    }
    struct Folder<'a> {
        dir: String,
        album: Option<(String, String)>,
        mixed: bool,
        album_track: Option<&'a str>,
    }
    let mut folders: Vec<Folder> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for d in drafts.iter().filter(|d| !d.dirs.is_empty()) {
        let Some(track_path) = tracks.get(&d.song.id) else {
            continue;
        };
        let dir = match track_path.rfind('/') {
            Some(i) => track_path[..i].to_string(),
            None => continue,
        };
        let album = album_identity(d.song);
        let i = *index.entry(dir.to_lowercase()).or_insert_with(|| {
            folders.push(Folder {
                dir,
                album: album.clone(),
                mixed: false,
                album_track: None,
            });
            folders.len() - 1
        });
        let folder = &mut folders[i];
        folder.mixed |= album.is_none() || folder.album != album;
        if folder.album_track.is_none() {
            folder.album_track = d.song.path.as_deref();
        }
    }
    folders
        .into_iter()
        .filter(|f| !f.mixed)
        .filter_map(|f| Some((f.dir, PathBuf::from(f.album_track?))))
        .flat_map(|(dir, album_track)| {
            names.iter().map(move |name| FolderArtFile {
                path: format!("{dir}/{name}"),
                album_track: album_track.clone(),
            })
        })
        .collect()
}

/// Which album a song is on, as the library groups albums: the folded
/// title and album artist (or artist). `None` without an album title.
fn album_identity(song: &Song) -> Option<(String, String)> {
    let album = album_key(song.album.as_deref().unwrap_or(""));
    if album.is_empty() {
        return None;
    }
    let artist = song
        .album_artist
        .as_deref()
        .filter(|a| !a.trim().is_empty())
        .or(song.artist.as_deref())
        .unwrap_or("");
    Some((album, album_key(artist)))
}

/// The JPEG to write as an album's folder art: `album_track`'s embedded
/// front cover, else the image beside it in the library (`cover.jpg` and
/// the like), with the longer side at most `max_px`. `None` when the album
/// has no art, in which case sync writes no art file.
pub fn folder_art_jpeg(album_track: &Path, max_px: Option<u32>) -> Option<Vec<u8>> {
    let embedded = lofty::probe::Probe::open(album_track)
        .and_then(|probe| probe.read())
        .ok()
        .and_then(|file| CoverManager::best_embedded_picture(&file));
    let data = match embedded {
        Some(data) => data,
        None => std::fs::read(CoverManager::scan_folder_art_static(album_track)?).ok()?,
    };
    let image = image::load_from_memory(&data).ok()?;
    let fits = max_px.is_none_or(|px| image.width().max(image.height()) <= px);
    if fits {
        return Some(clean_or_encode_to_jpeg(&data));
    }
    let px = max_px.unwrap_or(u32::MAX);
    let small = image.resize(px, px, image::imageops::FilterType::Lanczos3);
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, ART_JPEG_QUALITY)
        .encode_image(&small.to_rgb8())
        .ok()?;
    Some(jpeg)
}

/// `song_id` when it is a local library song, else the most recently added
/// one with an album, title and track number (a year too when any has one):
/// the song a template example is shown with.
pub fn example_song(conn: &rusqlite::Connection, song_id: Option<i64>) -> Result<Option<Song>> {
    let local = format!(
        "source IN ({}) AND unavailable = 0 AND path IS NOT NULL",
        *LOCAL_SOURCES_SQL
    );
    if let Some(id) = song_id {
        let sql = format!("SELECT {SONG_SELECT_COLS} FROM songs WHERE id = ?1 AND {local}");
        if let Some(song) = conn.query_row(&sql, [id], row_to_song).optional()? {
            return Ok(Some(song));
        }
    }
    let sql = format!(
        "SELECT {SONG_SELECT_COLS} FROM songs WHERE {local}
         ORDER BY (COALESCE(album, '') != '' AND COALESCE(title, '') != ''
                   AND COALESCE(track, 0) > 0) DESC,
                  COALESCE(year, 0) > 0 DESC, id DESC
         LIMIT 1"
    );
    Ok(conn.query_row(&sql, [], row_to_song).optional()?)
}

/// `track` (from the storage root) as seen from the `playlist_dir` folder:
/// `../Music/A/b.mp3`, joined with `separator`. Folder names compare
/// ignoring case, as the storage does.
fn relative_location(playlist_dir: &[String], track: &str, separator: &str) -> String {
    let track: Vec<&str> = track.split('/').collect();
    let (track_dirs, _) = track.split_at(track.len() - 1);
    let common = playlist_dir
        .iter()
        .zip(track_dirs)
        .take_while(|(a, b)| a.to_lowercase() == b.to_lowercase())
        .count();
    std::iter::repeat_n("..", playlist_dir.len() - common)
        .chain(track[common..].iter().copied())
        .collect::<Vec<_>>()
        .join(separator)
}

/// The parts of a profile path, which is already relative and `/`-separated.
fn path_parts(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// `name` cut to at most `max` bytes on a character boundary, without the
/// trailing dots or spaces FAT drops.
fn truncate_name(name: &str, max: usize) -> String {
    if name.len() <= max {
        return name.to_string();
    }
    let mut end = max;
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].trim_end_matches(['.', ' ']).to_string()
}

fn in_windows_1252(c: char) -> bool {
    let mut buf = [0u8; 4];
    let (_, _, unmappable) = WINDOWS_1252.encode(c.encode_utf8(&mut buf));
    !unmappable
}

/// `text` with every character Windows-1252 lacks replaced by an ASCII
/// look-alike ("Сплин" -> "Splin"), or `_` when there is none.
fn fold_to_windows_1252(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if in_windows_1252(c) {
            out.push(c);
        } else {
            match deunicode::deunicode_char(c) {
                Some(ascii) if !ascii.is_empty() => out.push_str(ascii),
                _ => out.push('_'),
            }
        }
    }
    out
}

/// `text` in Windows-1252, without a BOM. Characters it lacks are folded
/// first, so nothing is turned into the `&#...;` references the encoder
/// otherwise writes.
fn encode_windows_1252(text: &str) -> Vec<u8> {
    let folded = fold_to_windows_1252(text);
    let (bytes, _, unmappable) = WINDOWS_1252.encode(&folded);
    debug_assert!(!unmappable, "folded text is all Windows-1252");
    bytes.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use luminous_devices::FolderArt;

    fn song(id: i64, artist: &str, album: &str, year: i32, track: i32, title: &str) -> Song {
        Song {
            id,
            source: crate::models::SongSource::Collection,
            path: Some(format!(
                "C:/Library/{artist}/{album}/{track:02} {title}.flac"
            )),
            artist: Some(artist.into()),
            album_artist: Some(artist.into()),
            album: Some(album.into()),
            year: Some(year),
            track: Some(track),
            title: Some(title.into()),
            length_nanosec: Some(321_000_000_000),
            ..Song::default()
        }
    }

    fn profile(layout: LayoutSettings) -> DeviceProfile {
        DeviceProfile {
            music_path: "Music".into(),
            playlists_path: "Playlists".into(),
            layout,
            ..DeviceProfile::default()
        }
    }

    fn plan(profile: &DeviceProfile, songs: &[Song], extension: &str) -> DeviceLayout {
        let tracks: Vec<SyncedTrack> = songs
            .iter()
            .map(|song| SyncedTrack { song, extension })
            .collect();
        DeviceLayout::plan(profile, &tracks)
    }

    fn valediction() -> Song {
        song(1, "Insomnium", "Heart Like a Grave", 2019, 2, "Valediction")
    }

    #[test]
    fn tracks_follow_the_template_with_the_device_extension() {
        let layout = plan(&profile(LayoutSettings::default()), &[valediction()], "m4a");
        assert_eq!(
            layout.track_path(1),
            Some("Music/Insomnium/2019 - Heart Like a Grave/02 Valediction.m4a")
        );
    }

    #[test]
    fn a_template_naming_the_extension_does_not_double_it() {
        let layout = plan(
            &profile(LayoutSettings {
                folder_template: "%artist/%title.%extension".into(),
                ..LayoutSettings::default()
            }),
            &[valediction()],
            "mp3",
        );
        assert_eq!(
            layout.track_path(1),
            Some("Music/Insomnium/Valediction.mp3")
        );
    }

    #[test]
    fn clashing_tracks_get_numbered_in_song_id_order() {
        let a = song(7, "Katatonia", "Bootlegs", 2001, 1, "Untitled");
        let b = Song {
            id: 3,
            title: Some("UNTITLED".into()),
            ..a.clone()
        };
        let layout = plan(&profile(LayoutSettings::default()), &[a, b], "mp3");
        assert_eq!(
            layout.track_path(3),
            Some("Music/Katatonia/2001 - Bootlegs/01 UNTITLED.mp3")
        );
        assert_eq!(
            layout.track_path(7),
            Some("Music/Katatonia/2001 - Bootlegs/01 Untitled (2).mp3")
        );
    }

    #[test]
    fn folders_differing_only_in_case_share_one_spelling() {
        let a = song(1, "The War on Drugs", "Lost in the Dream", 2014, 1, "Under");
        let b = song(
            2,
            "The War On Drugs",
            "Lost in the Dream",
            2014,
            2,
            "Red Eyes",
        );
        let c = song(
            3,
            "The War on Drugs",
            "Lost in the Dream",
            2014,
            3,
            "Suffering",
        );
        let layout = plan(&profile(LayoutSettings::default()), &[a, b, c], "mp3");
        assert!(layout
            .track_path(2)
            .unwrap()
            .starts_with("Music/The War on Drugs/"));
    }

    #[test]
    fn names_are_fat_safe_and_short_enough() {
        let mut s = song(1, "CON", "A:B", 2020, 1, &"x".repeat(400));
        s.album_artist = Some("CON".into());
        let layout = plan(&profile(LayoutSettings::default()), &[s], "mp3");
        let path = layout.track_path(1).unwrap();
        assert!(path.starts_with("Music/CON_/2020 - A_B/01 xxx"), "{path}");
        let file = path.rsplit('/').next().unwrap();
        assert!(file.len() <= MAX_NAME_BYTES && file.ends_with(".mp3"));
    }

    #[test]
    fn m3u8_entries_are_relative_to_the_playlist_folder() {
        let songs = [valediction()];
        let layout = plan(&profile(LayoutSettings::default()), &songs, "m4a");
        let files = layout.playlist_files(&[DevicePlaylist {
            name: "Commute",
            songs: &songs,
        }]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "Playlists/Commute.m3u8");
        let text = String::from_utf8(files[0].content.clone()).unwrap();
        assert_eq!(
            text,
            "#EXTM3U\n#EXTINF:321,Insomnium - Valediction\n\
             ../Music/Insomnium/2019 - Heart Like a Grave/02 Valediction.m4a\n"
        );
    }

    #[test]
    fn playlists_in_the_music_folder_need_no_parent_steps() {
        let songs = [valediction()];
        let layout = plan(
            &DeviceProfile {
                playlists_path: "music".into(),
                ..profile(LayoutSettings::default())
            },
            &songs,
            "mp3",
        );
        let files = layout.playlist_files(&[DevicePlaylist {
            name: "Mix",
            songs: &songs,
        }]);
        let text = String::from_utf8(files[0].content.clone()).unwrap();
        assert!(text.contains("\nInsomnium/2019 - Heart Like a Grave/02 Valediction.mp3\n"));
    }

    #[test]
    fn backslash_playlists_use_dos_paths_and_line_endings() {
        let songs = [valediction()];
        let layout = plan(
            &profile(LayoutSettings {
                playlist_format: PlaylistFileFormat::Pls,
                path_separator: PathSeparator::Backslash,
                ..LayoutSettings::default()
            }),
            &songs,
            "mp3",
        );
        let files = layout.playlist_files(&[DevicePlaylist {
            name: "Mix",
            songs: &songs,
        }]);
        assert_eq!(files[0].path, "Playlists/Mix.pls");
        let text = String::from_utf8(files[0].content.clone()).unwrap();
        assert!(text.starts_with("[playlist]\r\nNumberOfEntries=1\r\n"));
        assert!(text.contains(
            "File1=..\\Music\\Insomnium\\2019 - Heart Like a Grave\\02 Valediction.mp3\r\n"
        ));
        assert!(!text.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn windows_1252_playlists_point_at_names_it_can_encode() {
        let songs = [song(1, "Сплин", "Гранатовый альбом", 1998, 1, "Café №1")];
        let layout = plan(
            &profile(LayoutSettings {
                playlist_format: PlaylistFileFormat::M3uWindows1252,
                ..LayoutSettings::default()
            }),
            &songs,
            "mp3",
        );
        let path = layout.track_path(1).unwrap();
        assert_eq!(path, "Music/Splin/1998 - Granatovyi al'bom/01 Café No1.mp3");

        let files = layout.playlist_files(&[DevicePlaylist {
            name: "Русский рок",
            songs: &songs,
        }]);
        assert_eq!(files[0].path, "Playlists/Russkii rok.m3u");
        let bytes = &files[0].content;
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "no UTF-8 BOM");
        let (text, _, malformed) = WINDOWS_1252.decode(bytes);
        assert!(!malformed);
        assert!(text.contains("#EXTINF:321,Splin - Café No1\n"));
        assert!(text.contains(&format!("\n../{path}\n")));
        assert!(!text.contains("&#"));
    }

    #[test]
    fn no_playlist_files_when_the_profile_wants_none() {
        let songs = [valediction()];
        let layout = plan(
            &profile(LayoutSettings {
                playlist_format: PlaylistFileFormat::None,
                ..LayoutSettings::default()
            }),
            &songs,
            "mp3",
        );
        assert!(layout
            .playlist_files(&[DevicePlaylist {
                name: "Mix",
                songs: &songs,
            }])
            .is_empty());
    }

    #[test]
    fn playlists_skip_unplanned_songs_and_get_unique_names() {
        let planned = valediction();
        let other = song(2, "Katatonia", "The Fall of Hearts", 2016, 1, "Takeover");
        let layout = plan(
            &profile(LayoutSettings::default()),
            std::slice::from_ref(&planned),
            "mp3",
        );
        let both = [planned, other];
        let files = layout.playlist_files(&[
            DevicePlaylist {
                name: "Mix",
                songs: &both,
            },
            DevicePlaylist {
                name: "MIX",
                songs: &both,
            },
        ]);
        assert_eq!(files[0].path, "Playlists/Mix.m3u8");
        assert_eq!(files[1].path, "Playlists/MIX (2).m3u8");
        let text = String::from_utf8(files[0].content.clone()).unwrap();
        assert_eq!(text.matches("#EXTINF").count(), 1);
    }

    #[test]
    fn folder_art_goes_in_single_album_folders_only() {
        let a1 = song(
            1,
            "Insomnium",
            "Heart Like a Grave",
            2019,
            1,
            "Wail of the North",
        );
        let a2 = song(2, "Insomnium", "Heart Like a Grave", 2019, 2, "Valediction");
        let b = song(3, "Insomnium", "Anno 1696", 2023, 1, "1696");
        let settings = LayoutSettings {
            folder_art: FolderArt::Both,
            ..LayoutSettings::default()
        };
        let layout = plan(
            &profile(settings.clone()),
            &[a1.clone(), a2, b.clone()],
            "mp3",
        );
        let art: Vec<&str> = layout
            .folder_art()
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert_eq!(
            art,
            [
                "Music/Insomnium/2019 - Heart Like a Grave/cover.jpg",
                "Music/Insomnium/2019 - Heart Like a Grave/folder.jpg",
                "Music/Insomnium/2023 - Anno 1696/cover.jpg",
                "Music/Insomnium/2023 - Anno 1696/folder.jpg",
            ]
        );
        assert_eq!(
            layout.folder_art()[0].album_track,
            PathBuf::from(a1.path.unwrap())
        );

        let by_artist = plan(
            &profile(LayoutSettings {
                folder_template: "%artist/%title".into(),
                ..settings
            }),
            &[
                song(1, "Insomnium", "Heart Like a Grave", 2019, 1, "Wail"),
                b,
            ],
            "mp3",
        );
        assert!(by_artist.folder_art().is_empty());
    }

    #[test]
    fn example_song_prefers_the_asked_for_song_then_a_well_tagged_one() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();
        assert!(example_song(&conn, None).unwrap().is_none());

        let tagged = valediction();
        let bare = Song {
            source: crate::models::SongSource::Collection,
            path: Some("C:/Library/loose.mp3".into()),
            ..Song::default()
        };
        crate::collection::upsert_song(&conn, &tagged).unwrap();
        crate::collection::upsert_song(&conn, &bare).unwrap();
        let id_of = |path: &str| -> i64 {
            conn.query_row("SELECT id FROM songs WHERE path = ?1", [path], |r| r.get(0))
                .unwrap()
        };
        let bare_id = id_of("C:/Library/loose.mp3");

        let picked = example_song(&conn, None).unwrap().unwrap();
        assert_eq!(picked.title.as_deref(), Some("Valediction"));
        let asked = example_song(&conn, Some(bare_id)).unwrap().unwrap();
        assert_eq!(asked.id, bare_id);
        let missing = example_song(&conn, Some(9999)).unwrap().unwrap();
        assert_eq!(missing.title.as_deref(), Some("Valediction"));
    }

    #[test]
    fn folder_art_is_shrunk_to_the_max_size() {
        let dir = tempfile::tempdir().unwrap();
        let track = dir.path().join("01 Track.flac");
        std::fs::write(&track, b"not audio").unwrap();
        let image = image::RgbImage::from_fn(800, 400, |x, y| image::Rgb([x as u8, y as u8, 9]));
        image.save(dir.path().join("cover.png")).unwrap();

        let jpeg = folder_art_jpeg(&track, Some(200)).unwrap();
        let shrunk = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((shrunk.width(), shrunk.height()), (200, 100));
        assert_eq!(
            image::guess_format(&jpeg).unwrap(),
            image::ImageFormat::Jpeg
        );

        let full = image::load_from_memory(&folder_art_jpeg(&track, None).unwrap()).unwrap();
        assert_eq!(full.width(), 800);

        std::fs::remove_file(dir.path().join("cover.png")).unwrap();
        assert_eq!(folder_art_jpeg(&track, None), None);
    }

    /// Lays out a folder of real library files on a real device the way sync
    /// will, so a player's firmware can be checked by hand: copies the tracks
    /// as they are, then writes one playlist of them and the folder art, all
    /// under `Luminous Layout Test/` on the device.
    ///
    /// ```text
    /// LUMINOUS_LAYOUT_DEVICE=E:\ LUMINOUS_LAYOUT_SOURCE=D:\Music\Album
    /// LUMINOUS_LAYOUT_FORMAT=m3u8|m3u|pls LUMINOUS_LAYOUT_SEPARATOR=slash|backslash
    /// cargo test -p luminous --lib lay_out_on_a_real_device -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "writes to the device named by LUMINOUS_LAYOUT_DEVICE"]
    fn lay_out_on_a_real_device() {
        use luminous_devices::{DeviceBackend, FsBackend};
        use std::ops::ControlFlow;

        let device = std::env::var("LUMINOUS_LAYOUT_DEVICE").expect("LUMINOUS_LAYOUT_DEVICE");
        let source = std::env::var("LUMINOUS_LAYOUT_SOURCE").expect("LUMINOUS_LAYOUT_SOURCE");
        let playlist_format = match std::env::var("LUMINOUS_LAYOUT_FORMAT").as_deref() {
            Ok("m3u") => PlaylistFileFormat::M3uWindows1252,
            Ok("pls") => PlaylistFileFormat::Pls,
            _ => PlaylistFileFormat::M3u8,
        };
        let path_separator = match std::env::var("LUMINOUS_LAYOUT_SEPARATOR").as_deref() {
            Ok("backslash") => PathSeparator::Backslash,
            _ => PathSeparator::Slash,
        };

        let mut files = Vec::new();
        let mut dirs = vec![PathBuf::from(&source)];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if crate::collection::is_audio_file(&path) {
                    files.push(path);
                }
            }
        }
        files.sort();
        let songs: Vec<Song> = files
            .iter()
            .enumerate()
            .map(|(i, path)| Song {
                id: i as i64 + 1,
                ..crate::collection::read_tags(path).unwrap()
            })
            .collect();
        let extensions: Vec<String> = files
            .iter()
            .map(|f| f.extension().unwrap().to_string_lossy().to_lowercase())
            .collect();
        let tracks: Vec<SyncedTrack> = songs
            .iter()
            .zip(&extensions)
            .map(|(song, extension)| SyncedTrack { song, extension })
            .collect();
        let profile = DeviceProfile {
            music_path: "Luminous Layout Test/Music".into(),
            playlists_path: "Luminous Layout Test/Playlists".into(),
            layout: LayoutSettings {
                playlist_format,
                path_separator,
                folder_art: FolderArt::Both,
                ..LayoutSettings::default()
            },
            ..DeviceProfile::default()
        };
        let layout = DeviceLayout::plan(&profile, &tracks);

        let backend = FsBackend::new(Path::new(&device));
        let write = |path: &str, bytes: &[u8]| {
            println!("{path}");
            backend
                .write(path, &mut &bytes[..], bytes.len() as u64, &mut |_| {
                    ControlFlow::Continue(())
                })
                .unwrap();
        };
        for (song, file) in songs.iter().zip(&files) {
            write(
                layout.track_path(song.id).unwrap(),
                &std::fs::read(file).unwrap(),
            );
        }
        for art in layout.folder_art() {
            if let Some(jpeg) = folder_art_jpeg(&art.album_track, Some(500)) {
                write(&art.path, &jpeg);
            }
        }
        for file in layout.playlist_files(&[DevicePlaylist {
            name: "Luminous layout test",
            songs: &songs,
        }]) {
            write(&file.path, &file.content);
        }
    }
}
