//! What the user chose to put on a device (saved per device on this
//! computer by [`super::store`]), and the library songs it stands for.

use crate::album_key::album_key;
use crate::collection::{row_to_song, SONG_SELECT_COLS, SONG_SELECT_COLS_QUALIFIED};
use crate::library_paths;
use crate::models::{Song, LOCAL_SOURCES_SQL};
use crate::playlist::{fit_to_limit, PlaylistManager};
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// One thing the user checked to go on the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SyncSource {
    /// Every local song in the library.
    Library,
    /// A playlist's songs; for a size-capped playlist (#344), only those
    /// above its break, measured in the device's converted sizes.
    Playlist { id: i64 },
    /// An auto-playlist (e.g. "favourites", "recently_added", "most_played", "history").
    AutoPlaylist {
        auto_type: String,
        #[serde(default)]
        name: Option<String>,
    },
    /// An individual song by its library id.
    Song { id: i64 },
    /// Songs whose artist or album artist is `name` (ignoring case).
    Artist { name: String },
    /// An album as the library groups albums: its title and album artist
    /// (or artist), compared folded.
    Album { album: String, artist: String },
    /// Library songs under a folder. They are laid out by the profile's
    /// folder template like any other song.
    Folder { path: String },
}

/// What sync does with songs that were synced before and are no longer
/// selected ("Songs you unselect").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnselectedSongs {
    /// Remove them from the device. Only files sync copied are removed.
    #[default]
    Remove,
    /// Leave them on the device.
    Leave,
}

/// A device's sync selection on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncSettings {
    /// The storage synced to; `None` is the device's first storage.
    pub storage_id: Option<String>,
    pub sources: Vec<SyncSource>,
    pub unselected: UnselectedSongs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    Library,
    Playlist,
    AutoPlaylist,
    Artist,
    Album,
    Folder,
    Song,
}

/// A selected source as plans and the device's manifest name it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub kind: SourceKind,
    /// Identifies the source across syncs: the playlist id, the folded
    /// artist or album, the folder path.
    pub key: String,
    /// What to show: the playlist, artist or album name, the folder path;
    /// empty for the library.
    pub name: String,
}

/// A song to put on the device and the selected sources that include it,
/// in selection order.
#[derive(Debug, Clone)]
pub struct WantedSong {
    pub song: Song,
    pub sources: Vec<SourceRef>,
}

/// A playlist to write as a playlist file on the device.
#[derive(Debug, Clone)]
pub struct WantedPlaylist {
    pub name: String,
    /// In playlist order, only the songs that go on the device.
    pub songs: Vec<Song>,
}

/// The songs and playlists a [`SyncSettings`] stands for.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// One per library file, in the order sources first include them. CUE
    /// tracks sharing a file are synced once, as the first of them.
    pub songs: Vec<WantedSong>,
    pub playlists: Vec<WantedPlaylist>,
    /// Every selected source that still exists, in selection order.
    pub sources: Vec<SourceRef>,
    /// The size limit of each selected size-capped playlist.
    pub caps: HashMap<SourceRef, u64>,
}

/// The library songs and playlists `settings` selects. `size_on_device`
/// gives a song's size after converting, for size-capped playlists; `None`
/// when it can't be copied.
pub fn resolve(
    conn: &rusqlite::Connection,
    settings: &SyncSettings,
    mut size_on_device: impl FnMut(&Song) -> Option<u64>,
) -> Result<Selection> {
    let needs_library = settings.sources.iter().any(|s| {
        !matches!(
            s,
            SyncSource::Playlist { .. } | SyncSource::AutoPlaylist { .. }
        )
    });
    let library = if needs_library {
        local_songs(conn)?
    } else {
        Vec::new()
    };

    let mut selection = Selection::default();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut seen_sources: HashSet<SourceRef> = HashSet::new();
    let mut add = |selection: &mut Selection, song: &Song, source: &SourceRef| {
        let Some(path) = song.path.as_deref() else {
            return;
        };
        match index.get(path) {
            Some(&i) => {
                let sources = &mut selection.songs[i].sources;
                if !sources.contains(source) {
                    sources.push(source.clone());
                }
            }
            None => {
                index.insert(path.to_string(), selection.songs.len());
                selection.songs.push(WantedSong {
                    song: song.clone(),
                    sources: vec![source.clone()],
                });
            }
        }
    };

    for source in &settings.sources {
        let (source_ref, songs): (SourceRef, Vec<&Song>) = match source {
            SyncSource::Library => (
                SourceRef {
                    kind: SourceKind::Library,
                    key: String::new(),
                    name: String::new(),
                },
                library.iter().collect(),
            ),
            SyncSource::Playlist { id } => {
                let Some(PlaylistSongs { name, songs, cap }) =
                    playlist_songs(conn, *id, &mut size_on_device)?
                else {
                    continue;
                };
                let source_ref = SourceRef {
                    kind: SourceKind::Playlist,
                    key: id.to_string(),
                    name: name.clone(),
                };
                if seen_sources.insert(source_ref.clone()) {
                    for song in &songs {
                        add(&mut selection, song, &source_ref);
                    }
                    if let Some(cap) = cap {
                        selection.caps.insert(source_ref.clone(), cap);
                    }
                    selection.sources.push(source_ref);
                    selection.playlists.push(WantedPlaylist { name, songs });
                }
                continue;
            }
            SyncSource::AutoPlaylist { auto_type, name } => {
                let Some(AutoPlaylistSongs {
                    name: resolved_name,
                    songs,
                }) = auto_playlist_songs(conn, auto_type, name.as_deref())?
                else {
                    continue;
                };
                let source_ref = SourceRef {
                    kind: SourceKind::AutoPlaylist,
                    key: auto_type.clone(),
                    name: resolved_name.clone(),
                };
                if seen_sources.insert(source_ref.clone()) {
                    for song in &songs {
                        add(&mut selection, song, &source_ref);
                    }
                    selection.sources.push(source_ref);
                    selection.playlists.push(WantedPlaylist {
                        name: resolved_name,
                        songs,
                    });
                }
                continue;
            }
            SyncSource::Song { id } => {
                let Some(song) = library.iter().find(|s| s.id == *id) else {
                    continue;
                };
                let source_ref = SourceRef {
                    kind: SourceKind::Song,
                    key: id.to_string(),
                    name: song.title.clone().unwrap_or_default(),
                };
                (source_ref, vec![song])
            }
            SyncSource::Artist { name } => {
                let key = album_key(name);
                let songs = library
                    .iter()
                    .filter(|s| {
                        [s.artist.as_deref(), s.album_artist.as_deref()]
                            .into_iter()
                            .flatten()
                            .any(|a| album_key(a) == key)
                    })
                    .collect();
                (
                    SourceRef {
                        kind: SourceKind::Artist,
                        key,
                        name: name.clone(),
                    },
                    songs,
                )
            }
            SyncSource::Album { album, artist } => {
                let wanted = (album_key(album), album_key(artist));
                let songs = library
                    .iter()
                    .filter(|s| album_of(s).as_ref() == Some(&wanted))
                    .collect();
                (
                    SourceRef {
                        kind: SourceKind::Album,
                        key: format!("{}\u{1f}{}", wanted.0, wanted.1),
                        name: album.clone(),
                    },
                    songs,
                )
            }
            SyncSource::Folder { path } => {
                let songs = library
                    .iter()
                    .filter(|s| {
                        s.path
                            .as_deref()
                            .is_some_and(|p| library_paths::is_under(p, path))
                    })
                    .collect();
                (
                    SourceRef {
                        kind: SourceKind::Folder,
                        key: library_paths::key(path),
                        name: path.clone(),
                    },
                    songs,
                )
            }
        };
        if !seen_sources.insert(source_ref.clone()) {
            continue;
        }
        for song in songs {
            add(&mut selection, song, &source_ref);
        }
        selection.sources.push(source_ref);
    }
    Ok(selection)
}

/// Every local, available library song with a file.
pub(super) fn local_songs(conn: &rusqlite::Connection) -> Result<Vec<Song>> {
    let sql = format!(
        "SELECT {SONG_SELECT_COLS} FROM songs
          WHERE source IN ({}) AND unavailable = 0 AND path IS NOT NULL
          ORDER BY id",
        *LOCAL_SOURCES_SQL
    );
    let mut stmt = conn.prepare(&sql)?;
    let songs = stmt
        .query_map([], row_to_song)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(songs)
}

/// What of a playlist goes on a device.
struct PlaylistSongs {
    name: String,
    /// Its local songs that go on the device, in its order.
    songs: Vec<Song>,
    /// Its size limit (#344), which cut `songs` short.
    cap: Option<u64>,
}

/// What of playlist `id` goes on the device; `None` when it no longer
/// exists.
fn playlist_songs(
    conn: &rusqlite::Connection,
    id: i64,
    size_on_device: &mut impl FnMut(&Song) -> Option<u64>,
) -> Result<Option<PlaylistSongs>> {
    let row: Option<(String, Option<i64>)> = conn
        .query_row(
            "SELECT name, size_limit_bytes FROM playlists WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((name, limit)) = row else {
        return Ok(None);
    };
    let songs: Vec<Song> = PlaylistManager::get_playlist_tracks_from_conn(conn, id)?
        .into_iter()
        .filter_map(|item| item.song)
        .collect();
    let limit = limit.and_then(|b| u64::try_from(b).ok()).filter(|&b| b > 0);
    let songs = match limit {
        Some(limit_bytes) => {
            // Measured over every item, as the playlist view does, so the
            // break falls where the user sees it.
            let sizes: Vec<Option<u64>> = songs
                .iter()
                .map(|s| copyable(s).then(|| size_on_device(s)).flatten())
                .collect();
            let fit = fit_to_limit(&sizes, limit_bytes);
            songs.into_iter().take(fit.break_index).collect()
        }
        None => songs,
    };
    Ok(Some(PlaylistSongs {
        name,
        songs: songs.into_iter().filter(copyable).collect(),
        cap: limit,
    }))
}

/// What of an auto-playlist goes on a device.
struct AutoPlaylistSongs {
    name: String,
    /// Its local songs that go on the device, in its order.
    songs: Vec<Song>,
}

fn auto_playlist_songs(
    conn: &rusqlite::Connection,
    auto_type: &str,
    custom_name: Option<&str>,
) -> Result<Option<AutoPlaylistSongs>> {
    let (default_name, sql, params_limit): (&str, String, Option<i64>) = match auto_type {
        "favourites" => (
            "Favourites",
            format!(
                "SELECT {SONG_SELECT_COLS} FROM songs
                 WHERE loved = 1
                   AND source IN ({})
                   AND unavailable = 0
                   AND not_included = 0
                   AND path IS NOT NULL
                 ORDER BY COALESCE(album_artist_sort, album_artist), COALESCE(albumsort, album), disc, track",
                *LOCAL_SOURCES_SQL
            ),
            None,
        ),
        "recently_added" => (
            "Recently Added",
            format!(
                "SELECT {SONG_SELECT_COLS} FROM songs
                 WHERE source IN ({})
                   AND unavailable = 0
                   AND not_included = 0
                   AND loved != -1
                   AND added IS NOT NULL
                   AND path IS NOT NULL
                 ORDER BY added DESC
                 LIMIT ?1",
                *LOCAL_SOURCES_SQL
            ),
            Some(100),
        ),
        "most_played" => (
            "Most Played",
            format!(
                "SELECT {SONG_SELECT_COLS_QUALIFIED}
                 FROM songs s
                 JOIN (
                     SELECT song_id, COUNT(*) AS play_count
                     FROM play_history
                     GROUP BY song_id
                 ) ph ON ph.song_id = s.id
                 WHERE s.source IN ({})
                   AND s.unavailable = 0
                   AND s.not_included = 0
                   AND s.loved != -1
                   AND s.path IS NOT NULL
                 ORDER BY ph.play_count DESC, s.added DESC
                 LIMIT ?1",
                *LOCAL_SOURCES_SQL
            ),
            Some(100),
        ),
        "history" => (
            "History",
            format!(
                "SELECT {SONG_SELECT_COLS_QUALIFIED}
                 FROM songs s
                 JOIN play_history ph ON ph.song_id = s.id
                 WHERE s.source IN ({})
                   AND s.unavailable = 0
                   AND s.not_included = 0
                   AND s.loved != -1
                   AND s.path IS NOT NULL
                 ORDER BY ph.played_at DESC
                 LIMIT ?1",
                *LOCAL_SOURCES_SQL
            ),
            Some(100),
        ),
        spec => {
            let row: Option<i64> = conn
                .query_row(
                    "SELECT id FROM playlists WHERE dynamic_enabled = 1 AND dynamic_spec = ?1",
                    params![spec],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(id) = row {
                let mut dummy_size = |_s: &Song| None;
                if let Some(pl) = playlist_songs(conn, id, &mut dummy_size)? {
                    let name = custom_name
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&pl.name)
                        .to_string();
                    return Ok(Some(AutoPlaylistSongs {
                        name,
                        songs: pl.songs,
                    }));
                }
            }
            return Ok(None);
        }
    };

    let mut stmt = conn.prepare(&sql)?;
    let songs: Vec<Song> = match params_limit {
        Some(limit) => stmt
            .query_map(params![limit], row_to_song)?
            .filter_map(|r| r.ok())
            .filter(copyable)
            .collect(),
        None => stmt
            .query_map([], row_to_song)?
            .filter_map(|r| r.ok())
            .filter(copyable)
            .collect(),
    };

    let name = custom_name
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(default_name)
        .to_string();

    Ok(Some(AutoPlaylistSongs { name, songs }))
}

fn copyable(song: &Song) -> bool {
    song.source.is_local() && !song.unavailable && song.path.is_some()
}

/// Which album a song is on, as [`SyncSource::Album`] compares it.
fn album_of(song: &Song) -> Option<(String, String)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_to_removing_unselected_songs() {
        let settings: SyncSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.unselected, UnselectedSongs::Remove);
        assert!(settings.sources.is_empty());
    }

    #[test]
    fn sources_round_trip_as_tagged_json() {
        let settings = SyncSettings {
            storage_id: Some("s1".into()),
            sources: vec![
                SyncSource::Library,
                SyncSource::Playlist { id: 4 },
                SyncSource::AutoPlaylist {
                    auto_type: "favourites".into(),
                    name: Some("Coups de cœur".into()),
                },
                SyncSource::Song { id: 42 },
                SyncSource::Album {
                    album: "Halo".into(),
                    artist: "Amorphis".into(),
                },
            ],
            unselected: UnselectedSongs::Leave,
        };
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(json["sources"][1]["kind"], "playlist");
        assert_eq!(json["sources"][2]["kind"], "autoPlaylist");
        assert_eq!(json["sources"][2]["autoType"], "favourites");
        assert_eq!(json["sources"][2]["name"], "Coups de cœur");
        assert_eq!(json["sources"][3]["kind"], "song");
        assert_eq!(json["sources"][3]["id"], 42);
        assert_eq!(json["unselected"], "leave");
        let back: SyncSettings = serde_json::from_value(json).unwrap();
        assert_eq!(back, settings);
    }

    #[test]
    fn auto_playlist_and_song_resolve_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();
        conn.execute_batch(
            "INSERT INTO songs (id, title, artist, album, path, loved, unavailable, not_included, source)
             VALUES (1, 'Song One', 'Artist A', 'Album X', '/music/1.flac', 1, 0, 0, 1);
             INSERT INTO songs (id, title, artist, album, path, loved, unavailable, not_included, source)
             VALUES (2, 'Song Two', 'Artist B', 'Album Y', '/music/2.flac', 0, 0, 0, 1);",
        )
        .unwrap();

        let auto = auto_playlist_songs(&conn, "favourites", Some("My Faves"))
            .unwrap()
            .unwrap();
        assert_eq!(auto.name, "My Faves");
        assert_eq!(auto.songs.len(), 1);
        assert_eq!(auto.songs[0].id, 1);

        let settings = SyncSettings {
            storage_id: None,
            sources: vec![
                SyncSource::AutoPlaylist {
                    auto_type: "favourites".into(),
                    name: Some("My Faves".into()),
                },
                SyncSource::Song { id: 2 },
            ],
            unselected: UnselectedSongs::Remove,
        };
        let selection = resolve(&conn, &settings, |_| None).unwrap();
        assert_eq!(selection.playlists.len(), 1);
        assert_eq!(selection.playlists[0].name, "My Faves");
        assert_eq!(selection.playlists[0].songs.len(), 1);
        assert_eq!(selection.songs.len(), 2);
    }
}
