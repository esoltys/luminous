//! Collection module — library scanner, file watcher, and DB integration.

use crate::{
    db::Database,
    models::{FileType, LibraryStats, MusicDirectory, ScanPhase, ScanProgress, Song, SongSource},
    covermanager::CoverManager,
};
use anyhow::{Context, Result};
use lofty::{
    file::TaggedFileExt,
    prelude::*,
    probe::Probe,
    tag::{Accessor, Tag},
};
use notify::Watcher;
use rusqlite::params;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::UNIX_EPOCH,
};
use tauri::{AppHandle, Emitter, Manager};
use walkdir::WalkDir;

pub struct CollectionScanner {
    db: Arc<Database>,
}

impl CollectionScanner {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Add a directory to the watched list.
    pub fn add_directory(&self, path: &str) -> Result<MusicDirectory> {
        let conn = self.db.pool.get()?;
        conn.execute(
            "INSERT OR IGNORE INTO directories (path, subdirs) VALUES (?1, 1)",
            params![path],
        )?;
        let id = conn.last_insert_rowid();
        Ok(MusicDirectory {
            id,
            path: path.to_string(),
            subdirs: true,
        })
    }

    /// Remove a directory from the watched list (songs remain but are marked unavailable).
    pub fn remove_directory(&self, path: &str) -> Result<()> {
        let conn = self.db.pool.get()?;
        conn.execute("DELETE FROM directories WHERE path = ?1", params![path])?;
        Ok(())
    }

    /// List all watched directories.
    pub fn get_directories(&self) -> Result<Vec<MusicDirectory>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare("SELECT id, path, subdirs FROM directories ORDER BY path")?;
        let dirs = stmt
            .query_map([], |row| {
                Ok(MusicDirectory {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    subdirs: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(dirs)
    }

    /// Prunes database records pointing to paths that no longer exist on disk.
    pub fn prune_missing_songs(&self) -> Result<usize> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare("SELECT id, path FROM songs WHERE path IS NOT NULL")?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            Ok((id, path))
        })?;

        let mut to_delete = Vec::new();
        for row in rows {
            if let Ok((id, path)) = row {
                let p = Path::new(&path);
                if !p.exists() {
                    to_delete.push(id);
                }
            }
        }

        let deleted_count = to_delete.len();
        if !to_delete.is_empty() {
            let tx = conn.unchecked_transaction()?;
            {
                let mut del_stmt = tx.prepare("DELETE FROM songs WHERE id = ?1")?;
                for id in to_delete {
                    del_stmt.execute(params![id])?;
                }
            }
            tx.commit()?;
        }
        Ok(deleted_count)
    }

    /// Scan all watched directories, emitting progress events to the frontend.
    pub async fn scan_all(&self, app: AppHandle) -> Result<()> {
        let dirs = self.get_directories()?;
        if dirs.is_empty() {
            return Ok(());
        }

        // Phase 1: discover all files
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                phase: ScanPhase::Discovering,
                scanned: 0,
                total: 0,
                current_path: None,
            },
        );

        let mut all_paths: Vec<PathBuf> = Vec::new();
        for dir in &dirs {
            let walker = WalkDir::new(&dir.path).follow_links(true);
            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                let path = entry.path().to_path_buf();
                if path.is_file() && is_audio_file(&path) {
                    all_paths.push(path);
                }
            }
        }

        let total = all_paths.len() as u64;
        log::info!("Scan found {total} audio files");

        // Phase 2: read tags
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                phase: ScanPhase::ReadingTags,
                scanned: 0,
                total,
                current_path: None,
            },
        );

        let conn = self.db.pool.get()?;
        let mut scanned = 0u64;

        let app_data_dir = app.path().app_data_dir().expect("no app data dir");
        let cover_manager = CoverManager::new(Arc::clone(&self.db), app_data_dir);

        for path in &all_paths {
            let path_str = path.to_string_lossy().to_string();

            // mtime-based incremental scan: skip if mtime unchanged
            let mtime = get_mtime(path).unwrap_or(0);
            let existing_mtime: Option<i64> = conn
                .query_row(
                    "SELECT mtime FROM songs WHERE path = ?1",
                    params![path_str],
                    |row| row.get(0),
                )
                .ok();

            if existing_mtime == Some(mtime) {
                scanned += 1;
                continue; // No change — skip tag re-read
            }

            // Read tags
            match read_tags(path) {
                Ok(mut song) => {
                    if song.art_embedded {
                        let artist = song.album_artist.as_deref().unwrap_or(song.artist.as_deref().unwrap_or(""));
                        let album = song.album.as_deref().unwrap_or("");
                        if let Ok(Some(cached_filename)) = cover_manager.extract_embedded_art(path, artist, album) {
                            song.art_automatic = Some(cached_filename);
                            song.art_unset = false;
                        }
                    } else if let Some(folder_art_path) = cover_manager.scan_folder_art(path) {
                        song.art_automatic = Some(folder_art_path.to_string_lossy().to_string());
                        song.art_unset = false;
                    }
                    upsert_song(&conn, &song)?;
                }
                Err(e) => {
                    log::warn!("Failed to read tags for {}: {e}", path.display());
                }
            }

            scanned += 1;

            // Emit progress every 50 files to avoid flooding
            if scanned % 50 == 0 || scanned == total {
                let _ = app.emit(
                    "scan-progress",
                    ScanProgress {
                        phase: ScanPhase::ReadingTags,
                        scanned,
                        total,
                        current_path: Some(path_str),
                    },
                );
            }
        }

        // Mark songs from these directories that no longer exist as unavailable
        // (soft-delete: set lastseen to 0 rather than deleting)
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                phase: ScanPhase::Updating,
                scanned: total,
                total,
                current_path: None,
            },
        );

        if let Err(e) = self.prune_missing_songs() {
            log::error!("Failed to prune missing songs during scan: {e}");
        }

        // Done
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                phase: ScanPhase::Done,
                scanned: total,
                total,
                current_path: None,
            },
        );

        Ok(())
    }

    /// Full-text + field search across the library.
    pub fn search_songs(&self, query: &str, limit: i64) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        // Use FTS5 for queries with non-trivial content
        let sql = if query.trim().is_empty() {
            format!(
                "SELECT {} FROM songs ORDER BY album_artist, album, disc, track LIMIT ?2",
                SONG_SELECT_COLS
            )
        } else {
            format!(
                "SELECT {} FROM songs WHERE id IN (
                    SELECT rowid FROM songs_fts WHERE songs_fts MATCH ?1
                 )
                 UNION
                 SELECT {} FROM songs WHERE
                    title LIKE ?3 OR artist LIKE ?3 OR album LIKE ?3
                 ORDER BY album_artist, album, disc, track
                 LIMIT ?2",
                SONG_SELECT_COLS, SONG_SELECT_COLS
            )
        };

        let like_query = format!("%{query}%");
        let fts_query = format!("{query}*");

        let mut stmt = conn.prepare(&sql)?;
        let songs = if query.trim().is_empty() {
            stmt.query_map(params![query, limit], row_to_song)?
                .filter_map(|r| r.ok())
                .collect()
        } else {
            stmt.query_map(params![fts_query, limit, like_query], row_to_song)?
                .filter_map(|r| r.ok())
                .collect()
        };

        Ok(songs)
    }

    /// Get all songs, optionally filtered by source.
    pub fn get_songs(&self, limit: i64, offset: i64) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE source IN (1, 2)
             ORDER BY album_artist, album, disc, track
             LIMIT ?1 OFFSET ?2",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![limit, offset], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    pub fn get_songs_by_album(&self, album_artist: &str, album: &str) -> Result<Vec<Song>> {
        let conn = self.db.pool.get()?;
        let sql = format!(
            "SELECT {} FROM songs
             WHERE (album_artist = ?1 OR (album_artist IS NULL AND artist = ?1))
               AND album = ?2
             ORDER BY disc, track",
            SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let songs = stmt
            .query_map(params![album_artist, album], row_to_song)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(songs)
    }

    pub fn get_albums(&self) -> Result<Vec<serde_json::Value>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT
                COALESCE(NULLIF(album_artist, ''), artist) AS effective_artist,
                album,
                MIN(year) AS year,
                COUNT(*) AS track_count,
                art_embedded,
                art_automatic,
                art_manual
             FROM songs
             WHERE source IN (1, 2) AND album IS NOT NULL
             GROUP BY effective_artist, album
             ORDER BY effective_artist, album",
        )?;
        let albums: Vec<serde_json::Value> = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "artist": row.get::<_, Option<String>>(0)?,
                    "album": row.get::<_, Option<String>>(1)?,
                    "year": row.get::<_, Option<i32>>(2)?,
                    "track_count": row.get::<_, i32>(3)?,
                    "art_embedded": row.get::<_, bool>(4)?,
                    "art_automatic": row.get::<_, Option<String>>(5)?,
                    "art_manual": row.get::<_, Option<String>>(6)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(albums)
    }

    pub fn get_artists(&self) -> Result<Vec<serde_json::Value>> {
        let conn = self.db.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT
                COALESCE(NULLIF(album_artist, ''), artist) AS effective_artist,
                COUNT(DISTINCT album) AS album_count,
                COUNT(*) AS song_count
             FROM songs
             WHERE source IN (1, 2)
             GROUP BY effective_artist
             ORDER BY effective_artist",
        )?;
        let artists: Vec<serde_json::Value> = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "name": row.get::<_, Option<String>>(0)?,
                    "album_count": row.get::<_, i32>(1)?,
                    "song_count": row.get::<_, i32>(2)?,
                }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(artists)
    }

    pub fn get_library_stats(&self) -> Result<LibraryStats> {
        let conn = self.db.pool.get()?;
        let stats = conn.query_row(
            "SELECT
                COUNT(*) as total_songs,
                COUNT(DISTINCT COALESCE(NULLIF(album_artist,''), artist)) as total_artists,
                COUNT(DISTINCT album) as total_albums,
                COALESCE(SUM(length_nanosec), 0) as total_duration,
                COALESCE(SUM(filesize), 0) as total_filesize
             FROM songs WHERE source IN (1, 2)",
            [],
            |row| {
                Ok(LibraryStats {
                    total_songs: row.get(0)?,
                    total_artists: row.get(1)?,
                    total_albums: row.get(2)?,
                    total_duration_nanosec: row.get(3)?,
                    total_filesize_bytes: row.get(4)?,
                })
            },
        )?;
        Ok(stats)
    }
}

// ---------------------------------------------------------------------------
// Audio file detection
// ---------------------------------------------------------------------------

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "opus", "m4a", "aac", "alac", "wav", "aiff", "aif",
    "wv", "mpc", "ape", "tta", "dsf", "dff", "asf", "wma", "m4b",
];

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn get_mtime(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

fn detect_filetype(path: &Path) -> FileType {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp3") => FileType::Mp3,
        Some("flac") => FileType::Flac,
        Some("ogg") => FileType::OggVorbis,
        Some("opus") => FileType::OggOpus,
        Some("m4a") | Some("aac") => FileType::Aac,
        Some("alac") => FileType::Alac,
        Some("wav") => FileType::Wav,
        Some("aiff") | Some("aif") => FileType::Aiff,
        Some("wv") => FileType::WavPack,
        Some("mpc") => FileType::Mpc,
        Some("ape") => FileType::Ape,
        Some("tta") => FileType::TrueAudio,
        Some("dsf") => FileType::Dsf,
        Some("dff") => FileType::Dsdiff,
        Some("asf") | Some("wma") => FileType::Asf,
        _ => FileType::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Tag reading via lofty
// ---------------------------------------------------------------------------

fn read_tags(path: &Path) -> Result<Song> {
    let path_str = path.to_string_lossy().to_string();
    let mtime = get_mtime(path);
    let filesize = std::fs::metadata(path).ok().map(|m| m.len() as i64);
    let filetype = detect_filetype(path);

    let tagged_file = Probe::open(path)
        .context("lofty: cannot open file")?
        .read()
        .context("lofty: cannot read file")?;

    let properties = tagged_file.properties();

    let duration_ns = (properties.duration().as_secs_f64() * 1_000_000_000.0) as i64;
    let bitrate = properties.audio_bitrate().map(|b| b as i32);
    let samplerate = properties.sample_rate().map(|r| r as i32);
    let channels = properties.channels().map(|c| c as i32);
    let bitdepth = properties.bit_depth().map(|b| b as i32);

    // Use the primary tag (ID3v2, VorbisComment, etc.)
    let tag: Option<&Tag> = tagged_file.primary_tag();

    let mut song = Song {
        source: SongSource::LocalFile,
        filetype,
        path: Some(path_str),
        length_nanosec: Some(duration_ns),
        bitrate,
        samplerate,
        channels,
        bitdepth,
        filesize,
        mtime,
        ..Default::default()
    };

    if let Some(tag) = tag {
        song.title = tag.title().map(|t| t.to_string());
        song.artist = tag.artist().map(|a| a.to_string());
        song.album = tag.album().map(|a| a.to_string());
        song.genre = tag.genre().map(|g| g.to_string());
        song.comment = tag.comment().map(|c| c.to_string());
        song.year = tag.year().map(|y| y as i32);
        song.track = tag.track().map(|t| t as i32);
        song.disc = tag.disk().map(|d| d as i32);

        // Album artist (various tag formats store this differently)
        song.album_artist = tag
            .get_string(&lofty::tag::ItemKey::AlbumArtist)
            .map(|s| s.to_string());

        song.composer = tag
            .get_string(&lofty::tag::ItemKey::Composer)
            .map(|s| s.to_string());

        song.lyrics = tag
            .get_string(&lofty::tag::ItemKey::Lyrics)
            .map(|s| s.to_string());

        // Check for embedded art
        song.art_embedded = !tag.pictures().is_empty();
    }

    Ok(song)
}

// ---------------------------------------------------------------------------
// Database upsert
// ---------------------------------------------------------------------------

fn upsert_song(conn: &rusqlite::Connection, song: &Song) -> Result<()> {
    conn.execute(
        &format!("INSERT INTO songs ({}) VALUES ({})
                  ON CONFLICT(path) DO UPDATE SET
                    title=excluded.title, artist=excluded.artist,
                    album=excluded.album, album_artist=excluded.album_artist,
                    track=excluded.track, disc=excluded.disc,
                    year=excluded.year, genre=excluded.genre,
                    composer=excluded.composer, lyrics=excluded.lyrics,
                    comment=excluded.comment, length_nanosec=excluded.length_nanosec,
                    bitrate=excluded.bitrate, samplerate=excluded.samplerate,
                    channels=excluded.channels, bitdepth=excluded.bitdepth,
                    filesize=excluded.filesize, mtime=excluded.mtime,
                    art_embedded=excluded.art_embedded,
                    art_automatic=excluded.art_automatic,
                    art_unset=excluded.art_unset,
                    filetype=excluded.filetype, source=excluded.source",
            SONG_INSERT_COLS, SONG_INSERT_PLACEHOLDERS),
        params![
            song.source as i32,
            song.filetype as i32,
            song.path,
            song.title,
            song.artist,
            song.album,
            song.album_artist,
            song.composer,
            song.lyrics,
            song.comment,
            song.track,
            song.disc,
            song.year,
            song.genre,
            song.length_nanosec,
            song.bitrate,
            song.samplerate,
            song.channels,
            song.bitdepth,
            song.filesize,
            song.mtime,
            song.art_embedded,
            song.art_automatic,
            song.art_unset,
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// SQL column helpers
// ---------------------------------------------------------------------------

const SONG_SELECT_COLS: &str = "
    id, source, filetype, path, url, stream_url,
    title, titlesort, artist, artistsort,
    album, albumsort, album_artist, album_artist_sort,
    composer, composersort, performer, performersort,
    grouping, comment, lyrics,
    track, disc, year, originalyear, genre, compilation,
    bpm, mood, initial_key,
    length_nanosec, beginning_nanosec, end_nanosec,
    bitrate, samplerate, bitdepth, channels, filesize, mtime,
    rating, playcount, skipcount, lastplayed, lastseen,
    art_embedded, art_automatic, art_manual, art_unset,
    cue_path,
    ebur128_integrated_loudness_lufs, ebur128_loudness_range_lu
";

const SONG_INSERT_COLS: &str = "
    source, filetype, path, title, artist, album, album_artist,
    composer, lyrics, comment, track, disc, year, genre,
    length_nanosec, bitrate, samplerate, channels, bitdepth,
    filesize, mtime, art_embedded, art_automatic, art_unset
";

const SONG_INSERT_PLACEHOLDERS: &str = "?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24";

fn row_to_song(row: &rusqlite::Row) -> rusqlite::Result<Song> {
    Ok(Song {
        id: row.get(0)?,
        source: row.get::<_, i64>(1).map(SongSource::from)?,
        filetype: row.get::<_, i64>(2).map(FileType::from)?,
        path: row.get(3)?,
        url: row.get(4)?,
        stream_url: row.get(5)?,
        title: row.get(6)?,
        titlesort: row.get(7)?,
        artist: row.get(8)?,
        artistsort: row.get(9)?,
        album: row.get(10)?,
        albumsort: row.get(11)?,
        album_artist: row.get(12)?,
        album_artist_sort: row.get(13)?,
        composer: row.get(14)?,
        composersort: row.get(15)?,
        performer: row.get(16)?,
        performersort: row.get(17)?,
        grouping: row.get(18)?,
        comment: row.get(19)?,
        lyrics: row.get(20)?,
        track: row.get(21)?,
        disc: row.get(22)?,
        year: row.get(23)?,
        originalyear: row.get(24)?,
        genre: row.get(25)?,
        compilation: row.get(26)?,
        bpm: row.get(27)?,
        mood: row.get(28)?,
        initial_key: row.get(29)?,
        length_nanosec: row.get(30)?,
        beginning_nanosec: row.get::<_, Option<i64>>(31)?.unwrap_or(0),
        end_nanosec: row.get::<_, Option<i64>>(32)?.unwrap_or(0),
        bitrate: row.get(33)?,
        samplerate: row.get(34)?,
        bitdepth: row.get(35)?,
        channels: row.get(36)?,
        filesize: row.get(37)?,
        mtime: row.get(38)?,
        rating: row.get::<_, Option<f32>>(39)?.unwrap_or(-1.0),
        playcount: row.get::<_, Option<i32>>(40)?.unwrap_or(0),
        skipcount: row.get::<_, Option<i32>>(41)?.unwrap_or(0),
        lastplayed: row.get(42)?,
        lastseen: row.get(43)?,
        art_embedded: row.get(44)?,
        art_automatic: row.get(45)?,
        art_manual: row.get(46)?,
        art_unset: row.get(47)?,
        cue_path: row.get(48)?,
        ebur128_integrated_loudness_lufs: row.get(49)?,
        ebur128_loudness_range_lu: row.get(50)?,
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// File Watcher & Deletion Sync
// ---------------------------------------------------------------------------

/// Helper to delete a path and its subpaths from the SQLite database.
pub fn delete_path_and_subpaths(db: &Database, path_str: &str) -> Result<usize> {
    let conn = db.pool.get()?;
    let deleted = conn.execute(
        "DELETE FROM songs WHERE path = ?1 OR path LIKE ?1 || '/%'",
        params![path_str],
    )?;
    Ok(deleted)
}

/// Start background directory watching using notify.
pub fn start_watcher(app: AppHandle, state: &crate::AppState) {
    let db = Arc::clone(&state.db);
    let app_clone = app.clone();

    // Create a channel to receive events
    let (tx, rx) = std::sync::mpsc::channel();

    // Create recommended watcher
    let watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    });

    let mut watcher = match watcher {
        Ok(w) => w,
        Err(e) => {
            log::error!("Failed to create file watcher: {e}");
            return;
        }
    };

    // Watch all monitored directories
    if let Ok(conn) = db.pool.get() {
        let mut stmt = match conn.prepare("SELECT path FROM directories") {
            Ok(s) => s,
            Err(_) => return,
        };
        let dirs = stmt.query_map([], |row| row.get::<_, String>(0));
        if let Ok(dirs) = dirs {
            for dir_path in dirs.flatten() {
                let p = PathBuf::from(dir_path);
                if p.exists() {
                    let _ = watcher.watch(&p, notify::RecursiveMode::Recursive);
                }
            }
        }
    }

    // Save the watcher inside AppState to keep it alive
    {
        let mut w_guard = state.watcher.lock();
        *w_guard = Some(watcher);
    }

    // Spawn the background thread to handle watcher events
    let db_for_thread = Arc::clone(&db);
    std::thread::Builder::new()
        .name("luminous-watcher".to_string())
        .spawn(move || {
            for event in rx {
                for path in event.paths {
                    let path_str = path.to_string_lossy().to_string();
                    if !path.exists() {
                        log::info!("Watcher detected deletion: {}", path_str);
                        match delete_path_and_subpaths(&db_for_thread, &path_str) {
                            Ok(deleted) => {
                                if deleted > 0 {
                                    log::info!("Pruned {} deleted song(s) from db", deleted);
                                    let _ = app_clone.emit("library-changed", ());
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to delete path from db: {e}");
                            }
                        }
                    }
                }
            }
        })
        .expect("failed to spawn watcher thread");
}
