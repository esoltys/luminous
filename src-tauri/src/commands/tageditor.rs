use crate::collection::WatcherPauseGuard;
use crate::models;
use crate::tag_cleanup;
use crate::AppState;
use luminous_metadata::audio;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

/// Progress update emitted during batch tag writing (#1087).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagBatchProgressPayload {
    pub current: usize,
    pub total: usize,
    pub title: String,
    pub done: bool,
}

/// One field of a multi-track tag edit. `Keep` leaves each track's own value
/// alone — what the album editors send when the selection's values differ
/// and the user didn't touch the field, so a multi-disc album keeps its
/// per-disc numbers and subtitles. `Set` writes one value to every track,
/// `None` clearing it. Sent over IPC as `{ mode: "keep" }` or
/// `{ mode: "set", value }`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "lowercase")]
pub enum FieldEdit<T> {
    #[default]
    Keep,
    Set(Option<T>),
}

impl<T: Clone> FieldEdit<T> {
    /// The value a track should end up with, given the one it has now.
    pub fn resolve(&self, current: Option<T>) -> Option<T> {
        match self {
            FieldEdit::Keep => current,
            FieldEdit::Set(value) => value.clone(),
        }
    }

    /// Transforms a `Set` value (e.g. normalizing it); `Keep` stays `Keep`.
    pub fn map(self, f: impl FnOnce(Option<T>) -> Option<T>) -> Self {
        match self {
            FieldEdit::Keep => FieldEdit::Keep,
            FieldEdit::Set(value) => FieldEdit::Set(f(value)),
        }
    }
}

/// Trims a free-text tag value, mapping blank to `None` so an emptied field
/// clears the tag instead of storing an empty string.
fn non_blank(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The album-level tags the album and batch editors edit, as one track has
/// them. Everything else about the track stays its own.
#[derive(Debug, Clone, Default, PartialEq)]
struct SharedTags {
    album: String,
    albumsort: Option<String>,
    album_artist: String,
    album_artist_sort: Option<String>,
    genre: String,
    genresort: Option<String>,
    year: Option<u32>,
    disc: Option<u32>,
    disc_subtitle: Option<String>,
    compilation: bool,
}

impl SharedTags {
    fn of(song: &models::Song) -> Self {
        SharedTags {
            album: song.album.clone().unwrap_or_default(),
            albumsort: song.albumsort.clone(),
            album_artist: song.album_artist.clone().unwrap_or_default(),
            album_artist_sort: song.album_artist_sort.clone(),
            genre: song.genre.clone().unwrap_or_default(),
            genresort: song.genresort.clone(),
            year: song.year.map(|y| y as u32),
            disc: song.disc.map(|d| d as u32),
            disc_subtitle: song.disc_subtitle.clone(),
            compilation: song.compilation,
        }
    }
}

/// An album or batch edit: each shared tag is kept per track or set on all
/// of them. Fields missing from the IPC payload are kept, so the editors send
/// only what the user changed. Sent as `{ album: { mode: "set", value }, … }`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SharedTagEdit {
    pub album: FieldEdit<String>,
    pub albumsort: FieldEdit<String>,
    pub album_artist: FieldEdit<String>,
    pub album_artist_sort: FieldEdit<String>,
    pub genre: FieldEdit<String>,
    pub genresort: FieldEdit<String>,
    pub year: FieldEdit<u32>,
    pub disc: FieldEdit<u32>,
    pub disc_subtitle: FieldEdit<String>,
    pub compilation: FieldEdit<bool>,
}

impl SharedTagEdit {
    /// One track's tags after the edit. Set values are stored the way the
    /// single-track editor stores them: text trimmed with blank meaning none,
    /// multi-value fields (album artist, genre) in their canonical `; ` form.
    fn apply(&self, current: SharedTags) -> SharedTags {
        let text = |edit: &FieldEdit<String>, now: Option<String>| {
            edit.clone().map(non_blank).resolve(now)
        };
        let multi = |edit: &FieldEdit<String>, now: String| {
            edit.clone()
                .map(|v| {
                    v.map(|s| models::join_multi_value(&models::parse_multi_value(&s)))
                        .and_then(|s| non_blank(Some(s)))
                })
                .resolve(Some(now))
                .unwrap_or_default()
        };
        SharedTags {
            album: text(&self.album, Some(current.album)).unwrap_or_default(),
            albumsort: text(&self.albumsort, current.albumsort),
            album_artist: multi(&self.album_artist, current.album_artist),
            album_artist_sort: text(&self.album_artist_sort, current.album_artist_sort),
            genre: multi(&self.genre, current.genre),
            genresort: text(&self.genresort, current.genresort),
            year: self.year.resolve(current.year),
            disc: self.disc.resolve(current.disc),
            disc_subtitle: text(&self.disc_subtitle, current.disc_subtitle),
            compilation: self
                .compilation
                .resolve(Some(current.compilation))
                .unwrap_or(false),
        }
    }
}

#[derive(serde::Serialize)]
pub struct SongDetails {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub titlesort: Option<String>,
    pub artist: String,
    pub artistsort: Option<String>,
    pub album: String,
    pub albumsort: Option<String>,
    pub album_artist: String,
    pub album_artist_sort: Option<String>,
    pub composer: String,
    pub composersort: Option<String>,
    /// `artist`, `album_artist`, `composer`, and `genre` are all `; `-delimited
    /// when the song carries multiple values — see
    /// `models::parse_multi_value`/`join_multi_value`, the single source of
    /// truth for this convention.
    pub genre: String,
    pub genresort: Option<String>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub disc_subtitle: Option<String>,
    pub year: Option<u32>,
    pub originalyear: Option<u32>,
    pub grouping: String,
    pub bpm: Option<f32>,
    pub initial_key: String,
    pub rating: f32,
    pub loved: i32,
    pub compilation: bool,
    pub art_embedded: bool,
    /// `true` when this song was cut from a CUE sheet (#78) — its tags live in
    /// the .cue file, not the shared media file's own embedded tags, so the
    /// tag editor should present it read-only rather than let a save silently
    /// overwrite every other track cut from the same file.
    pub is_cue_track: bool,
}

#[tauri::command]
pub async fn get_song_details(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<SongDetails, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, path, title, titlesort, artist, artistsort, album, albumsort, album_artist, album_artist_sort, composer, composersort, genre, genresort, track, disc, year,
                originalyear, grouping, bpm, initial_key, rating, loved, compilation, art_embedded, cue_path,
                disc_subtitle
         FROM songs WHERE id = ?1",
        rusqlite::params![song_id],
        |row| {
            Ok(SongDetails {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2).unwrap_or_default(),
                titlesort: row.get(3).ok(),
                artist: row.get(4).unwrap_or_default(),
                artistsort: row.get(5).ok(),
                album: row.get(6).unwrap_or_default(),
                albumsort: row.get(7).ok(),
                album_artist: row.get(8).unwrap_or_default(),
                album_artist_sort: row.get(9).ok(),
                composer: row.get(10).unwrap_or_default(),
                composersort: row.get(11).ok(),
                genre: row.get(12).unwrap_or_default(),
                genresort: row.get(13).ok(),
                track: row.get(14).ok(),
                disc: row.get(15).ok(),
                year: row.get(16).ok(),
                originalyear: row.get(17).ok(),
                grouping: row.get(18).unwrap_or_default(),
                bpm: row.get(19).ok(),
                initial_key: row.get(20).unwrap_or_default(),
                rating: row.get(21).unwrap_or(crate::stats::RATING_UNRATED),
                loved: row.get(22).unwrap_or(0),
                compilation: row.get(23).unwrap_or(false),
                art_embedded: row.get(24).unwrap_or(false),
                is_cue_track: row.get::<_, Option<String>>(25)?.is_some(),
                disc_subtitle: row.get(26)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

/// Reads metadata tags directly from an audio file on disk (#1887) —
/// used by the Tag Editor when opened for an un-scanned audio file.
#[tauri::command]
pub async fn get_file_song_details(path: String) -> Result<SongDetails, String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("File not found: {path}"));
    }
    let song = crate::collection::read_tags(p).map_err(|e| e.to_string())?;
    Ok(SongDetails {
        id: 0,
        path: path.clone(),
        title: song.title.unwrap_or_else(|| {
            p.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        }),
        titlesort: song.titlesort,
        artist: song.artist.unwrap_or_default(),
        artistsort: song.artistsort,
        album: song.album.unwrap_or_default(),
        albumsort: song.albumsort,
        album_artist: song.album_artist.unwrap_or_default(),
        album_artist_sort: song.album_artist_sort,
        composer: song.composer.unwrap_or_default(),
        composersort: song.composersort,
        genre: song.genre.unwrap_or_default(),
        genresort: None,
        track: song.track.map(|t| t as u32),
        disc: song.disc.map(|d| d as u32),
        disc_subtitle: song.disc_subtitle,
        year: song.year.map(|y| y as u32),
        originalyear: song.originalyear.map(|y| y as u32),
        grouping: song.grouping.unwrap_or_default(),
        bpm: song.bpm,
        initial_key: song.initial_key.unwrap_or_default(),
        rating: crate::stats::RATING_UNRATED,
        loved: 0,
        compilation: song.compilation,
        art_embedded: song.art_embedded,
        is_cue_track: false,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn save_song_tags(
    app: AppHandle,
    state: State<'_, AppState>,
    song_id: i64,
    title: String,
    titlesort: Option<String>,
    artist: String,
    artistsort: Option<String>,
    album: String,
    albumsort: Option<String>,
    album_artist: String,
    album_artist_sort: Option<String>,
    composer: String,
    composersort: Option<String>,
    genre: Option<String>,
    genresort: Option<String>,
    track: Option<u32>,
    disc: Option<u32>,
    disc_subtitle: Option<String>,
    year: Option<u32>,
    originalyear: Option<u32>,
    grouping: String,
    bpm: Option<f32>,
    initial_key: String,
) -> Result<(), String> {
    // Written tags are an app-driven change Luminous already knows about, not
    // an external addition — without this, the realtime watcher would pick up
    // its own write and fire a spurious "song updated" toast on top of
    // whatever feedback the tag editor itself shows (#233).
    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let (path_str, source, compilation, cue_path): (String, i32, bool, Option<String>) = conn
        .query_row(
            "SELECT path, source, compilation, cue_path FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| "Song not found in library".to_string())?;
    // A CUE sheet track's tags live in the .cue file, not the shared media
    // file's own embedded tags — writing here would silently overwrite every
    // other track cut from the same file with just this one's values (#78).
    // There's no CUE-sheet write-back yet, so refuse rather than corrupt.
    if cue_path.is_some() {
        return Err(
            "This track's tags come from its CUE sheet and can't be edited yet.".to_string(),
        );
    }
    // Remote songs (WebDAV, OpenSubsonic) have no local file to write lofty
    // tags to — there's no write-back to the remote server implemented, so the
    // edit is saved to Luminous's own DB only (the tag editor surfaces this to
    // the user). Attempting the on-disk write here would always fail and abort
    // the whole save before the DB update below ever ran.
    let is_remote = models::SongSource::from(source as i64).is_remote();

    let path = std::path::PathBuf::from(path_str);
    // Close the timing race the coarse guard above can't (#514): the OS's own
    // change notification for this write may arrive after the guard's grace
    // window elapses, so track the exact path too.
    state
        .self_writes
        .mark_written(std::iter::once(path.clone()));

    // 2. Write metadata back to disk (blocking lofty write in threadpool)
    // The single-song tag editor has no compilation toggle (that's an
    // album-level property owned by AlbumTagEditor.svelte), so preserve
    // whatever's already in the DB rather than clobbering it.
    let path_clone = path.clone();
    let title_c = title.clone();
    let titlesort_c = titlesort.clone();
    // Normalize every multi-value field to the canonical `; `-delimited,
    // trimmed, deduped form before it hits disk or the DB, regardless of
    // exactly what the chip input sent over the wire.
    let artist_str = models::join_multi_value(&models::parse_multi_value(&artist));
    let artist_c = artist_str.clone();
    let artistsort_c = artistsort.clone();
    let album_c = album.clone();
    let albumsort_c = albumsort.clone();
    let album_artist_str = models::join_multi_value(&models::parse_multi_value(&album_artist));
    let album_artist_c = album_artist_str.clone();
    let album_artist_sort_c = album_artist_sort.clone();
    let composer_str = models::join_multi_value(&models::parse_multi_value(&composer));
    let composer_c = composer_str.clone();
    let composersort_c = composersort.clone();
    let genre_str =
        models::join_multi_value(&models::parse_multi_value(&genre.unwrap_or_default()));
    let genre_c = genre_str.clone();
    let disc_subtitle = non_blank(disc_subtitle);
    let disc_subtitle_c = disc_subtitle.clone();
    let grouping_c = grouping.clone();
    let initial_key_c = initial_key.clone();

    if !is_remote {
        tauri::async_runtime::spawn_blocking(move || {
            audio::write_tags(
                &path_clone,
                &audio::TagWriteRequest {
                    title: &title_c,
                    titlesort: titlesort_c.as_deref(),
                    artist: &artist_c,
                    artistsort: artistsort_c.as_deref(),
                    album: &album_c,
                    albumsort: albumsort_c.as_deref(),
                    album_artist: &album_artist_c,
                    album_artist_sort: album_artist_sort_c.as_deref(),
                    composer: &composer_c,
                    composersort: composersort_c.as_deref(),
                    genre: &genre_c,
                    track,
                    disc,
                    disc_subtitle: disc_subtitle_c.as_deref(),
                    year,
                    originalyear,
                    grouping: &grouping_c,
                    bpm,
                    initial_key: &initial_key_c,
                    compilation,
                },
            )
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;
    }

    let (new_mtime, new_filesize) = if !is_remote {
        crate::tag_history::file_mtime_and_size(&path)
    } else {
        (None, None)
    };

    let before = crate::tag_history::snapshot_by_id(&conn, song_id).map_err(|e| e.to_string())?;

    // 3. Update SQLite database cache in-place
    conn.execute(
        "UPDATE songs SET
            title = ?1,
            titlesort = ?2,
            artist = ?3,
            artistsort = ?4,
            album = ?5,
            albumsort = ?6,
            album_artist = ?7,
            album_artist_sort = ?8,
            composer = ?9,
            composersort = ?10,
            genre = ?11,
            genresort = ?12,
            track = ?13,
            disc = ?14,
            year = ?15,
            originalyear = ?16,
            grouping = ?17,
            bpm = ?18,
            initial_key = ?19,
            album_key = ?21,
            disc_subtitle = ?22,
            mtime = COALESCE(?23, mtime),
            filesize = COALESCE(?24, filesize)
         WHERE id = ?20",
        rusqlite::params![
            title,
            titlesort,
            artist_str,
            artistsort,
            album,
            albumsort,
            album_artist_str,
            album_artist_sort,
            composer_str,
            composersort,
            genre_str,
            genresort,
            track,
            disc,
            year,
            originalyear,
            grouping,
            bpm,
            initial_key,
            song_id,
            crate::album_key::album_key_for(
                &album,
                if album_artist_str.trim().is_empty() {
                    None
                } else {
                    Some(album_artist_str.as_str())
                },
                if artist_str.trim().is_empty() {
                    None
                } else {
                    Some(artist_str.as_str())
                },
                compilation,
            ),
            disc_subtitle,
            new_mtime,
            new_filesize,
        ],
    )
    .map_err(|e| e.to_string())?;

    crate::tag_history::record_diff_after_update(
        &conn,
        before.as_ref(),
        crate::tag_history::HistoryActor::Luminous,
        None,
    )
    .map_err(|e| e.to_string())?;

    // Continuous auto-organization hook (#1468)
    if let Ok(auto_res) = crate::organizer::auto_organize_song_ids(
        &state.db,
        &state.watcher_paused,
        &state.self_writes,
        Some(&state.cover_manager),
        &[song_id],
    ) {
        if auto_res.moved_count > 0 || auto_res.duplicates_count > 0 || !auto_res.errors.is_empty()
        {
            let _ = app.emit("auto-organize-result", &auto_res);
        }
        if auto_res.moved_count > 0 {
            let _ = app.emit("library-changed", ());
        }
    }

    Ok(())
}

/// Writes metadata tags directly to an audio file on disk (#1887) —
/// used when saving tags on an audio file from the Files view that is not yet in the library.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn save_file_tags(
    state: State<'_, AppState>,
    path: String,
    title: String,
    titlesort: Option<String>,
    artist: String,
    artistsort: Option<String>,
    album: String,
    albumsort: Option<String>,
    album_artist: String,
    album_artist_sort: Option<String>,
    composer: String,
    composersort: Option<String>,
    genre: Option<String>,
    genresort: Option<String>,
    track: Option<u32>,
    disc: Option<u32>,
    disc_subtitle: Option<String>,
    year: Option<u32>,
    originalyear: Option<u32>,
    grouping: String,
    bpm: Option<f32>,
    initial_key: String,
) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("File not found: {path}"));
    }

    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));
    state.self_writes.mark_written(std::iter::once(p.clone()));

    let artist_str = models::join_multi_value(&models::parse_multi_value(&artist));
    let album_artist_str = models::join_multi_value(&models::parse_multi_value(&album_artist));
    let composer_str = models::join_multi_value(&models::parse_multi_value(&composer));
    let genre_str =
        models::join_multi_value(&models::parse_multi_value(&genre.unwrap_or_default()));

    let compilation = crate::collection::read_tags(&p)
        .map(|s| s.compilation)
        .unwrap_or(false);

    let path_clone = p.clone();
    let title_c = title.clone();
    let titlesort_c = titlesort.clone();
    let artist_c = artist_str.clone();
    let artistsort_c = artistsort.clone();
    let album_c = album.clone();
    let albumsort_c = albumsort.clone();
    let album_artist_c = album_artist_str.clone();
    let album_artist_sort_c = album_artist_sort.clone();
    let composer_c = composer_str.clone();
    let composersort_c = composersort.clone();
    let genre_c = genre_str.clone();
    let disc_subtitle = non_blank(disc_subtitle);
    let disc_subtitle_c = disc_subtitle.clone();
    let grouping_c = grouping.clone();
    let initial_key_c = initial_key.clone();

    tauri::async_runtime::spawn_blocking(move || {
        audio::write_tags(
            &path_clone,
            &audio::TagWriteRequest {
                title: &title_c,
                titlesort: titlesort_c.as_deref(),
                artist: &artist_c,
                artistsort: artistsort_c.as_deref(),
                album: &album_c,
                albumsort: albumsort_c.as_deref(),
                album_artist: &album_artist_c,
                album_artist_sort: album_artist_sort_c.as_deref(),
                composer: &composer_c,
                composersort: composersort_c.as_deref(),
                genre: &genre_c,
                track,
                disc,
                disc_subtitle: disc_subtitle_c.as_deref(),
                year,
                originalyear,
                grouping: &grouping_c,
                bpm,
                initial_key: &initial_key_c,
                compilation,
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))?;

    // If this song already happens to be in the database, keep SQLite in sync
    if let Ok(conn) = state.db.pool.get() {
        let before = crate::tag_history::snapshot_by_path(&conn, &path, 0)
            .ok()
            .flatten();
        let (new_mtime, new_filesize) = crate::tag_history::file_mtime_and_size(&p);

        let _ = conn.execute(
            "UPDATE songs SET
                title = ?1,
                titlesort = ?2,
                artist = ?3,
                artistsort = ?4,
                album = ?5,
                albumsort = ?6,
                album_artist = ?7,
                album_artist_sort = ?8,
                composer = ?9,
                composersort = ?10,
                genre = ?11,
                genresort = ?12,
                track = ?13,
                disc = ?14,
                year = ?15,
                originalyear = ?16,
                grouping = ?17,
                bpm = ?18,
                initial_key = ?19,
                disc_subtitle = ?21,
                mtime = COALESCE(?22, mtime),
                filesize = COALESCE(?23, filesize)
             WHERE path = ?20",
            rusqlite::params![
                title,
                titlesort
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty()),
                artist_str,
                artistsort
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty()),
                album,
                albumsort
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty()),
                album_artist_str,
                album_artist_sort
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty()),
                composer_str,
                composersort
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty()),
                genre_str,
                genresort
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty()),
                track,
                disc,
                year,
                originalyear,
                grouping,
                bpm,
                initial_key,
                path,
                disc_subtitle,
                new_mtime,
                new_filesize,
            ],
        );

        let _ = crate::tag_history::record_diff_after_update(
            &conn,
            before.as_ref(),
            crate::tag_history::HistoryActor::Luminous,
            None,
        );
    }

    Ok(())
}

/// Writes the album editor's shared tags to library songs. Each field of
/// `edit` is either kept per track or set on all of them, so a selection that
/// spans albums, discs or genres keeps whatever the user didn't change.
#[tauri::command]
pub async fn save_album_tags(
    app: AppHandle,
    state: State<'_, AppState>,
    song_ids: Vec<i64>,
    edit: SharedTagEdit,
) -> Result<u32, String> {
    if song_ids.is_empty() {
        return Ok(0);
    }

    // See save_song_tags — pause the watcher for the whole batch write so it
    // doesn't misread its own tag writes across the album as an external
    // change and fire a spurious "songs updated" toast (#233).
    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;

    // Each song with the tags it's saved with.
    let mut edited: Vec<(models::Song, SharedTags)> = Vec::with_capacity(song_ids.len());
    {
        let sql = format!(
            "SELECT {} FROM songs WHERE id = ?1",
            crate::collection::SONG_SELECT_COLS
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        for &song_id in &song_ids {
            if let Ok(song) = stmt.query_row([song_id], crate::collection::row_to_song) {
                let tags = edit.apply(SharedTags::of(&song));
                edited.push((song, tags));
            }
        }
    }

    // See save_song_tags — close the timing race the coarse guard above can't
    // (#514) by tracking every path this batch is about to write.
    state.self_writes.mark_written(
        edited
            .iter()
            .filter_map(|(song, _)| song.path.as_deref().map(std::path::PathBuf::from)),
    );

    let total_songs = edited.len();
    let batch_id = crate::tag_history::new_batch_id();
    let app_clone = app.clone();
    let (updated_count, edited) = tauri::async_runtime::spawn_blocking(move || {
        let mut count = 0u32;
        let mut with_meta = Vec::with_capacity(edited.len());
        for (idx, (song, tags)) in edited.into_iter().enumerate() {
            let _ = app_clone.emit(
                "tag-batch-progress",
                TagBatchProgressPayload {
                    current: idx + 1,
                    total: total_songs,
                    title: song.title.clone().unwrap_or_default(),
                    done: false,
                },
            );

            // Remote songs (WebDAV, OpenSubsonic) have no local file to write lofty
            // tags to, and there's no write-back to the remote server implemented —
            // the change is saved to Luminous's own DB only (the tag editor surfaces
            // this to the user), same as save_song_tags/rewrite_genre_and_persist.
            if song.source.is_remote() {
                count += 1;
                with_meta.push((song, tags, None, None));
                continue;
            }
            let path = std::path::PathBuf::from(song.path.as_deref().unwrap_or_default());
            match write_shared_tags(&path, &song, &tags) {
                Ok(_) => {
                    count += 1;
                    let (mtime, size) = crate::tag_history::file_mtime_and_size(&path);
                    with_meta.push((song, tags, mtime, size));
                }
                Err(ref e) => {
                    log::warn!("Failed to persist tags to disk for song {}: {e:#}", song.id);
                }
            }
        }
        (count, with_meta)
    })
    .await
    .map_err(|e| e.to_string())?;

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for (song, tags, new_mtime, new_filesize) in &edited {
        let before = crate::tag_history::snapshot_by_id(&tx, song.id).map_err(|e| e.to_string())?;
        let key = crate::album_key::album_key_for(
            &tags.album,
            Some(tags.album_artist.as_str()).filter(|s| !s.trim().is_empty()),
            song.artist.as_deref(),
            tags.compilation,
        );
        tx.execute(
            "UPDATE songs SET
                album = ?1,
                albumsort = ?2,
                album_artist = ?3,
                album_artist_sort = ?4,
                genre = ?5,
                genresort = ?6,
                year = ?7,
                disc = ?8,
                compilation = ?9,
                album_key = ?11,
                disc_subtitle = ?12,
                mtime = COALESCE(?13, mtime),
                filesize = COALESCE(?14, filesize)
             WHERE id = ?10",
            rusqlite::params![
                tags.album,
                tags.albumsort,
                tags.album_artist,
                tags.album_artist_sort,
                tags.genre,
                tags.genresort,
                tags.year,
                tags.disc,
                tags.compilation,
                song.id,
                key,
                tags.disc_subtitle,
                new_mtime,
                new_filesize,
            ],
        )
        .map_err(|e| e.to_string())?;

        crate::tag_history::record_diff_after_update(
            &tx,
            before.as_ref(),
            crate::tag_history::HistoryActor::Luminous,
            Some(&batch_id),
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    let _ = app.emit(
        "tag-batch-progress",
        TagBatchProgressPayload {
            current: total_songs,
            total: total_songs,
            title: edited
                .first()
                .map(|(_, tags, _, _)| tags.album.clone())
                .unwrap_or_default(),
            done: true,
        },
    );

    // Continuous auto-organization hook (#1468)
    if let Ok(auto_res) = crate::organizer::auto_organize_song_ids(
        &state.db,
        &state.watcher_paused,
        &state.self_writes,
        Some(&state.cover_manager),
        &song_ids,
    ) {
        if auto_res.moved_count > 0 || auto_res.duplicates_count > 0 || !auto_res.errors.is_empty()
        {
            let _ = app.emit("auto-organize-result", &auto_res);
        }
        if auto_res.moved_count > 0 {
            let _ = app.emit("library-changed", ());
        }
    }

    Ok(updated_count)
}

/// Writes the album editor's shared tags to audio files on disk (#1887) —
/// used when batch-editing tags for a folder or multi-file selection from
/// Files view. Each field of `edit` is kept per file or set on all of them,
/// as in `save_album_tags`.
#[tauri::command]
pub async fn save_batch_file_tags(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
    edit: SharedTagEdit,
) -> Result<u32, String> {
    if paths.is_empty() {
        return Ok(0);
    }

    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let path_bufs: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
    state.self_writes.mark_written(path_bufs.iter().cloned());

    let total = path_bufs.len();
    let batch_id = crate::tag_history::new_batch_id();
    let app_clone = app.clone();

    // Each written file with the tags it was saved with, for the DB update.
    let (updated_count, written) = tauri::async_runtime::spawn_blocking(move || {
        let mut count = 0u32;
        let mut written: Vec<(std::path::PathBuf, SharedTags, Option<i64>, Option<i64>)> =
            Vec::new();
        for (idx, p) in path_bufs.into_iter().enumerate() {
            let mut song = match crate::collection::read_tags(&p) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // A file with no title tag keeps its name as one.
            if song.title.is_none() {
                song.title = p.file_stem().map(|s| s.to_string_lossy().to_string());
            }

            let _ = app_clone.emit(
                "tag-batch-progress",
                TagBatchProgressPayload {
                    current: idx + 1,
                    total,
                    title: song.title.clone().unwrap_or_default(),
                    done: false,
                },
            );

            let tags = edit.apply(SharedTags::of(&song));
            if write_shared_tags(&p, &song, &tags).is_ok() {
                count += 1;
                let (mtime, size) = crate::tag_history::file_mtime_and_size(&p);
                written.push((p, tags, mtime, size));
            }
        }
        (count, written)
    })
    .await
    .map_err(|e| e.to_string())?;

    // Also update any matching rows in SQLite DB
    if let Ok(conn) = state.db.pool.get() {
        for (path, tags, mtime, size) in &written {
            let path_str = path.to_string_lossy();
            let before = crate::tag_history::snapshot_by_path(&conn, &path_str, 0)
                .ok()
                .flatten();
            let _ = conn.execute(
                "UPDATE songs SET
                    album = ?1,
                    albumsort = ?2,
                    album_artist = ?3,
                    album_artist_sort = ?4,
                    genre = ?5,
                    genresort = ?6,
                    year = ?7,
                    disc = ?8,
                    compilation = ?9,
                    disc_subtitle = ?11,
                    mtime = COALESCE(?12, mtime),
                    filesize = COALESCE(?13, filesize)
                 WHERE path = ?10",
                rusqlite::params![
                    tags.album,
                    tags.albumsort,
                    tags.album_artist,
                    tags.album_artist_sort,
                    tags.genre,
                    tags.genresort,
                    tags.year,
                    tags.disc,
                    tags.compilation,
                    path_str,
                    tags.disc_subtitle,
                    mtime,
                    size,
                ],
            );

            let _ = crate::tag_history::record_diff_after_update(
                &conn,
                before.as_ref(),
                crate::tag_history::HistoryActor::Luminous,
                Some(&batch_id),
            );
        }
    }

    let _ = app.emit(
        "tag-batch-progress",
        TagBatchProgressPayload {
            current: total,
            total,
            title: String::new(),
            done: true,
        },
    );

    Ok(updated_count)
}

/// Writes `tags` to the file at `path`, keeping `song`'s own per-track tags
/// (title, artist, track number and the rest).
fn write_shared_tags(
    path: &std::path::Path,
    song: &models::Song,
    tags: &SharedTags,
) -> anyhow::Result<()> {
    audio::write_tags(
        path,
        &audio::TagWriteRequest {
            title: song.title.as_deref().unwrap_or_default(),
            titlesort: song.titlesort.as_deref(),
            artist: song.artist.as_deref().unwrap_or_default(),
            artistsort: song.artistsort.as_deref(),
            album: &tags.album,
            albumsort: tags.albumsort.as_deref(),
            album_artist: &tags.album_artist,
            album_artist_sort: tags.album_artist_sort.as_deref(),
            composer: song.composer.as_deref().unwrap_or_default(),
            composersort: song.composersort.as_deref(),
            genre: &tags.genre,
            track: song.track.map(|t| t as u32),
            disc: tags.disc,
            disc_subtitle: tags.disc_subtitle.as_deref(),
            year: tags.year,
            originalyear: song.originalyear.map(|y| y as u32),
            grouping: song.grouping.as_deref().unwrap_or_default(),
            bpm: song.bpm,
            initial_key: song.initial_key.as_deref().unwrap_or_default(),
            compilation: tags.compilation,
        },
    )
}

/// Clear a single song's embedded cover art (#386) — removes the picture(s)
/// from the file's tag on disk, then re-resolves the song's automatic art to
/// folder art (if any exists next to the file) so the UI doesn't briefly
/// show nothing before the collection's normal remote-lookup fallback kicks
/// in. A manually-picked cover (`art_manual`) always takes precedence over
/// automatic art regardless, so it's left untouched.
#[tauri::command]
pub async fn clear_song_cover_art(state: State<'_, AppState>, song_id: i64) -> Result<(), String> {
    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let (path_str, source): (String, i32) = conn
        .query_row(
            "SELECT path, source FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "Song not found in library".to_string())?;

    let path = std::path::PathBuf::from(path_str);
    // Remote songs (WebDAV, OpenSubsonic) have no local file to clear an
    // embedded picture from, and there's no write-back to the remote server
    // implemented — same as tag edits (see save_song_tags), this is
    // DB-only. The tag editor hides the Clear Artwork button for these
    // songs; this guard is what keeps it from erroring if it's ever
    // reached some other way.
    if !models::SongSource::from(source as i64).is_remote() {
        // See save_song_tags — close the timing race the coarse guard above can't (#514).
        state
            .self_writes
            .mark_written(std::iter::once(path.clone()));
        let path_clone = path.clone();
        tauri::async_runtime::spawn_blocking(move || audio::clear_embedded_art(&path_clone))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:#}"))?;
    }

    let folder_art = state
        .cover_manager
        .scan_folder_art(&path)
        .map(|p| p.to_string_lossy().to_string());

    conn.execute(
        "UPDATE songs SET art_embedded = 0, art_automatic = ?1, art_unset = 0 WHERE id = ?2",
        rusqlite::params![folder_art, song_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Bulk version of `clear_song_cover_art` for clearing an entire album's
/// embedded artwork at once (#386). Skips (rather than fails) songs whose
/// file couldn't be cleared, returning the count that actually succeeded.
#[tauri::command]
pub async fn clear_album_cover_art(
    state: State<'_, AppState>,
    song_ids: Vec<i64>,
) -> Result<u32, String> {
    if song_ids.is_empty() {
        return Ok(0);
    }

    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let conn = state.db.pool.get().map_err(|e| e.to_string())?;

    let mut local_paths = Vec::with_capacity(song_ids.len());
    let mut remote_paths = Vec::new();
    for &song_id in &song_ids {
        if let Ok((path_str, source)) = conn.query_row(
            "SELECT path, source FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
        ) {
            let path = std::path::PathBuf::from(path_str);
            // Remote songs (WebDAV, OpenSubsonic) have no local file to clear an
            // embedded picture from — DB-only, same as clear_song_cover_art above.
            if models::SongSource::from(source as i64).is_remote() {
                remote_paths.push((song_id, path));
            } else {
                local_paths.push((song_id, path));
            }
        }
    }

    // See save_song_tags — close the timing race the coarse guard above can't (#514).
    state
        .self_writes
        .mark_written(local_paths.iter().map(|(_, p)| p.clone()));

    let mut cleared: Vec<(i64, std::path::PathBuf)> =
        tauri::async_runtime::spawn_blocking(move || {
            local_paths
                .into_iter()
                .filter(|(_, path)| audio::clear_embedded_art(path).is_ok())
                .collect()
        })
        .await
        .map_err(|e| e.to_string())?;
    cleared.extend(remote_paths);

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for (song_id, path) in &cleared {
        let folder_art = state
            .cover_manager
            .scan_folder_art(path)
            .map(|p| p.to_string_lossy().to_string());
        tx.execute(
            "UPDATE songs SET art_embedded = 0, art_automatic = ?1, art_unset = 0 WHERE id = ?2",
            rusqlite::params![folder_art, song_id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(cleared.len() as u32)
}

fn is_disc_folder_name(name: &str) -> bool {
    let s = name.trim().to_ascii_lowercase();
    if let Some(rest) = s
        .strip_prefix("disc")
        .or_else(|| s.strip_prefix("disk"))
        .or_else(|| s.strip_prefix("cd"))
    {
        let trimmed_rest = rest.trim();
        if trimmed_rest.is_empty() {
            return false;
        }
        if trimmed_rest.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
        let id = rest.trim_matches(|c: char| c.is_whitespace() || c == '-' || c == '_');
        if (rest.starts_with(' ') || rest.starts_with('-') || rest.starts_with('_'))
            && id.len() == 1
            && id.chars().next().unwrap().is_ascii_alphabetic()
        {
            return true;
        }
    }
    false
}

fn resolve_album_dir(dir: &std::path::Path) -> std::path::PathBuf {
    if let Some(file_name) = dir.file_name().and_then(|n| n.to_str()) {
        if is_disc_folder_name(file_name) {
            if let Some(parent) = dir.parent() {
                return parent.to_path_buf();
            }
        }
    }
    dir.to_path_buf()
}

/// Opens the containing folder of the first of `song_ids` that has a local
/// file (in the OS file manager), for the Song/Album Details "Open Folder"
/// action. Songs sharing an album normally share one folder, so the first
/// hit is enough -- this mirrors `open_in_picard`'s song-id-to-parent-dir
/// resolution rather than trusting a raw path from the frontend.
/// If multiple songs are selected (album context), disc subfolders (e.g.
/// `Disc 1`, `CD 1`) are resolved up to the common album directory.
#[tauri::command]
pub async fn open_song_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    song_ids: Vec<i64>,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let dir = song_ids.iter().find_map(|id| {
        let row: Option<(String, i32)> = conn
            .query_row(
                "SELECT path, source FROM songs WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        // A remote song's path is a URL / `subsonic://` URI, not a folder
        // that exists on disk (#916).
        let path = row
            .filter(|(_, source)| !crate::models::SongSource::from(*source as i64).is_remote())
            .map(|(path, _)| path);
        path.as_deref()
            .map(std::path::Path::new)
            .and_then(|p| p.parent())
            .map(|p| {
                if song_ids.len() > 1 {
                    resolve_album_dir(p)
                } else {
                    p.to_path_buf()
                }
            })
    });
    drop(conn);

    let Some(dir) = dir else {
        return Err("No local files found for the selected songs".to_string());
    };

    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The album editors send `{ mode: "keep" }` for a mixed field the user
    /// didn't touch, which must leave every track's own value alone (#1940).
    #[test]
    fn test_field_edit_wire_format_and_resolve() {
        let keep: FieldEdit<u32> = serde_json::from_str(r#"{"mode":"keep"}"#).unwrap();
        let set: FieldEdit<u32> = serde_json::from_str(r#"{"mode":"set","value":2}"#).unwrap();
        let clear: FieldEdit<u32> = serde_json::from_str(r#"{"mode":"set","value":null}"#).unwrap();

        assert_eq!(keep, FieldEdit::Keep);
        assert_eq!(keep.resolve(Some(1)), Some(1));
        assert_eq!(keep.resolve(None), None);
        assert_eq!(set.resolve(Some(1)), Some(2));
        assert_eq!(clear.resolve(Some(1)), None);
        assert_eq!(keep.map(|_| Some(9)), FieldEdit::Keep);
        assert_eq!(set.map(|v| v.map(|n| n + 1)), FieldEdit::Set(Some(3)));
    }

    /// A selection spanning albums keeps every field the user didn't touch,
    /// and normalizes the ones they set like the single-track editor does.
    #[test]
    fn test_shared_tag_edit_keeps_omitted_fields_and_normalizes_set_ones() {
        let edit: SharedTagEdit = serde_json::from_str(
            r#"{
                "genre": { "mode": "set", "value": " Rock ;Blues; " },
                "discSubtitle": { "mode": "set", "value": "  Remixes " },
                "year": { "mode": "set", "value": null },
                "compilation": { "mode": "keep" }
            }"#,
        )
        .unwrap();
        let track = SharedTags {
            album: "Night Visions".into(),
            albumsort: Some("Night Visions".into()),
            album_artist: "Imagine Dragons".into(),
            genre: "Pop".into(),
            year: Some(2012),
            disc: Some(3),
            compilation: true,
            ..SharedTags::default()
        };

        let saved = edit.apply(track.clone());

        assert_eq!(
            saved,
            SharedTags {
                genre: "Rock; Blues".into(),
                disc_subtitle: Some("Remixes".into()),
                year: None,
                ..track.clone()
            }
        );
        assert_eq!(SharedTagEdit::default().apply(track.clone()), track);

        let cleared: SharedTagEdit =
            serde_json::from_str(r#"{ "albumArtist": { "mode": "set", "value": "  " } }"#).unwrap();
        assert_eq!(cleared.apply(track).album_artist, "");
    }

    #[test]
    fn test_non_blank_trims_and_drops_empty() {
        assert_eq!(non_blank(Some("  Live  ".into())), Some("Live".into()));
        assert_eq!(non_blank(Some("   ".into())), None);
        assert_eq!(non_blank(None), None);
    }

    #[test]
    fn test_is_disc_folder_name() {
        assert!(is_disc_folder_name("Disc 1"));
        assert!(is_disc_folder_name("disc 02"));
        assert!(is_disc_folder_name("CD1"));
        assert!(is_disc_folder_name("CD 2"));
        assert!(is_disc_folder_name("Disk 1"));
        assert!(is_disc_folder_name("CD A"));
        assert!(is_disc_folder_name("disc - b"));

        assert!(!is_disc_folder_name(""));
        assert!(!is_disc_folder_name("Albatross"));
        assert!(!is_disc_folder_name("2012 - Albatross"));
        assert!(!is_disc_folder_name("Discography"));
        assert!(!is_disc_folder_name("CDs"));
    }

    #[test]
    fn test_resolve_album_dir() {
        let disc_dir = Path::new("/Music/Pink Floyd/The Wall/Disc 1");
        assert_eq!(
            resolve_album_dir(disc_dir),
            Path::new("/Music/Pink Floyd/The Wall")
        );

        let album_dir = Path::new("/Music/Big Wreck/2012 - Albatross");
        assert_eq!(resolve_album_dir(album_dir), album_dir);
    }
}

/// Lists removable tag cruft (promotional comments, encoder banners,
/// `iTunNORM`, duplicate ID3v1 tags) in `song_ids`' files, or in the whole
/// local library when empty (#1939). Writes nothing.
/// Bumped by `cancel_tag_cleanup`; a library-wide preview stops as soon as it
/// changes, so the user can call off a scan that reads every file.
static TAG_CLEANUP_GENERATION: AtomicU64 = AtomicU64::new(0);

/// An empty `song_ids` checks the whole library: that runs as a background
/// task, emitting `tag-cleanup-progress` (`{ checked, total }`) as files are
/// read and stopping on `cancel_tag_cleanup`. A list of songs is checked
/// quietly and can't be cancelled, so a single-track check from the tag
/// editor never disturbs a library scan in progress.
#[tauri::command]
pub async fn preview_tag_cleanup(
    app: AppHandle,
    state: State<'_, AppState>,
    song_ids: Vec<i64>,
) -> Result<tag_cleanup::TagCleanupPreview, String> {
    let db = Arc::clone(&state.db);
    let library = song_ids.is_empty();
    let generation = TAG_CLEANUP_GENERATION.load(Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || {
        tag_cleanup::preview(&db, &song_ids, |checked, total| {
            if !library {
                return true;
            }
            let _ = app.emit(
                "tag-cleanup-progress",
                serde_json::json!({ "checked": checked, "total": total }),
            );
            TAG_CLEANUP_GENERATION.load(Ordering::SeqCst) == generation
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stops a running library-wide `preview_tag_cleanup`.
#[tauri::command]
pub async fn cancel_tag_cleanup() -> Result<(), String> {
    TAG_CLEANUP_GENERATION.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

/// Removes the fields the user kept ticked in the clean-up review (#1939),
/// emitting `tag-cleanup-apply-progress` (`{ done, total }`) as it goes.
#[tauri::command]
pub async fn apply_tag_cleanup(
    app: AppHandle,
    state: State<'_, AppState>,
    selections: Vec<tag_cleanup::TagCleanupSelection>,
) -> Result<tag_cleanup::TagCleanupResult, String> {
    // See save_song_tags: these writes are Luminous's own, not external changes (#233).
    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));
    let db = Arc::clone(&state.db);
    let self_writes = Arc::clone(&state.self_writes);
    let progress_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut last_emit = std::time::Instant::now();
        tag_cleanup::apply(&db, &self_writes, &selections, |done, total| {
            // Each write is a round trip on a network share; a few updates a second is plenty.
            if done == total || last_emit.elapsed() >= std::time::Duration::from_millis(250) {
                last_emit = std::time::Instant::now();
                let _ = progress_app.emit(
                    "tag-cleanup-apply-progress",
                    serde_json::json!({ "done": done, "total": total }),
                );
            }
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    if result.cleaned_songs > 0 {
        let _ = app.emit("library-changed", ());
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_song_tag_history(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<crate::tag_history::SongTagHistory, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    crate::tag_history::get_song_history(&conn, song_id)
}

#[tauri::command]
pub async fn clear_song_tag_history(
    state: State<'_, AppState>,
    song_id: i64,
) -> Result<(), String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    crate::tag_history::clear_song_history(&conn, song_id)
}
