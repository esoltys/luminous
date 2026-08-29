use crate::{
    collection::WatcherPauseGuard,
    models::{GenreGroup, QueuePopulationMode, Song, Tag, TagGroup},
    tags::TagManager,
    AppState,
};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(serde::Serialize)]
pub struct TagsOverview {
    pub tags: Vec<Tag>,
    pub graph: Vec<GenreGroup>,
    pub no_genre_count: i64,
}

/// Combined `list_all_tags` + `get_genre_graph` in one call — the pair the
/// Genres tab always needs together, sharing a single DB scan instead of
/// each independently re-scanning every song's genre column. Also reports
/// how many songs carry no genre at all, so the tab can surface them as a
/// "No Genre" group.
#[tauri::command]
pub async fn get_tags_overview(state: State<'_, AppState>) -> Result<TagsOverview, String> {
    let manager = TagManager::new(state.db.clone());
    let (tags, graph, no_genre_count) = manager.get_tags_overview().map_err(|e| e.to_string())?;
    Ok(TagsOverview {
        tags,
        graph,
        no_genre_count,
    })
}

#[tauri::command]
pub async fn get_songs_without_genre(
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .get_songs_without_genre(limit.unwrap_or(50), mode.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_songs_by_tag(
    tag_name: String,
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .get_songs_by_tag(&tag_name, limit.unwrap_or(50), mode.unwrap_or_default())
        .map_err(|e| e.to_string())
}

/// Curated-hierarchy song lookup (#548) — the direct-query fallback used
/// both by a sub-threshold genre/tag's click-through (no backing playlist
/// row yet) and, via `PlaylistManager::songs_for_spec`'s `tag:` dispatch, by
/// every materialized genre auto-playlist's own population.
#[tauri::command]
pub async fn get_songs_by_curated_tag(
    tag_name: String,
    limit: Option<i64>,
    mode: Option<QueuePopulationMode>,
    state: State<'_, AppState>,
) -> Result<Vec<Song>, String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .get_songs_by_curated_tag(&tag_name, limit.unwrap_or(50), mode.unwrap_or_default())
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Persisted Genres curation hierarchy (#545)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_tag_hierarchy(state: State<'_, AppState>) -> Result<Vec<TagGroup>, String> {
    let manager = TagManager::new(state.db.clone());
    // Self-heals on every read rather than relying solely on the
    // `library-changed` listener having already caught up — cheap (a no-op
    // pass over already-in-sync data) and guarantees the Genres tab never
    // shows a stale-empty hierarchy just because reconciliation hasn't run
    // yet this session.
    if let Err(e) = manager.reconcile_hierarchy() {
        log::error!("Tag hierarchy reconcile-on-read failed: {e}");
    }
    manager.get_tag_hierarchy().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_tag_group_color(
    state: State<'_, AppState>,
    name: String,
    color_index: i32,
) -> Result<(), String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .set_group_color(&name, color_index)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reparent_tag(
    state: State<'_, AppState>,
    tag_name: String,
    new_group_name: String,
) -> Result<(), String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .reparent_tag(&tag_name, &new_group_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn promote_tag(state: State<'_, AppState>, tag_name: String) -> Result<(), String> {
    let manager = TagManager::new(state.db.clone());
    manager.promote_tag(&tag_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn demote_group_to_child(
    state: State<'_, AppState>,
    tag_name: String,
    new_group_name: String,
) -> Result<(), String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .demote_group_to_child(&tag_name, &new_group_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reorder_tag_in_group(
    state: State<'_, AppState>,
    tag_name: String,
    new_index: i32,
) -> Result<(), String> {
    let manager = TagManager::new(state.db.clone());
    manager
        .reorder_tag_in_group(&tag_name, new_index)
        .map_err(|e| e.to_string())
}

/// Every field [`crate::tageditor::write_tags`] needs, read fresh per song so
/// a bulk genre-only rewrite (merge/delete) can preserve everything else
/// exactly as it already is on disk and in the DB.
struct SongFullMetadata {
    id: i64,
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
    genre: String,
    track: Option<u32>,
    disc: Option<u32>,
    year: Option<u32>,
    grouping: String,
    bpm: Option<f32>,
    initial_key: String,
    compilation: bool,
}

/// Reads each song via the canonical `SONG_SELECT_COLS`/`row_to_song` mapping
/// (see `collection.rs`) rather than a hand-rolled column list, so a new or
/// reordered song column can't silently default here while staying correct
/// everywhere else.
fn load_full_metadata(conn: &rusqlite::Connection, song_ids: &[i64]) -> Vec<SongFullMetadata> {
    let sql = format!(
        "SELECT {} FROM songs WHERE id = ?1",
        crate::collection::SONG_SELECT_COLS
    );
    let mut out = Vec::with_capacity(song_ids.len());
    for &id in song_ids {
        let res = conn.query_row(&sql, rusqlite::params![id], crate::collection::row_to_song);
        if let Ok(song) = res {
            out.push(SongFullMetadata {
                id: song.id,
                path: song.path.unwrap_or_default(),
                title: song.title.unwrap_or_default(),
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
                track: song.track.map(|t| t as u32),
                disc: song.disc.map(|d| d as u32),
                year: song.year.map(|y| y as u32),
                grouping: song.grouping.unwrap_or_default(),
                bpm: song.bpm,
                initial_key: song.initial_key.unwrap_or_default(),
                compilation: song.compilation,
            });
        }
    }
    out
}

/// Shared by [`merge_tags`]/[`delete_tags`]: rewrites every affected song's
/// embedded genre tag on disk and in the DB (same dual-write as the regular
/// tag editor — see `tags.rs`'s module doc comment). `rewrite_genre` computes
/// each song's new genre string from its current one; callers supply
/// merge-into or delete-these-names logic. Returns the number of songs
/// whose on-disk write succeeded.
async fn rewrite_genre_and_persist(
    conn: r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>,
    song_ids: &[i64],
    rewrite_genre: impl Fn(&str) -> String + Send + 'static,
) -> Result<u32, String> {
    let metas = load_full_metadata(&conn, song_ids);

    let (updated_count, writes): (u32, Vec<(i64, String)>) =
        tauri::async_runtime::spawn_blocking(move || {
            let mut count = 0u32;
            let mut writes = Vec::with_capacity(metas.len());
            for item in &metas {
                let new_genre = rewrite_genre(&item.genre);
                let path = std::path::PathBuf::from(&item.path);
                let write_res = crate::tageditor::write_tags(
                    &path,
                    &crate::tageditor::TagWriteRequest {
                        title: &item.title,
                        titlesort: item.titlesort.as_deref(),
                        artist: &item.artist,
                        artistsort: item.artistsort.as_deref(),
                        album: &item.album,
                        albumsort: item.albumsort.as_deref(),
                        album_artist: &item.album_artist,
                        album_artist_sort: item.album_artist_sort.as_deref(),
                        composer: &item.composer,
                        composersort: item.composersort.as_deref(),
                        genre: &new_genre,
                        track: item.track,
                        disc: item.disc,
                        year: item.year,
                        grouping: &item.grouping,
                        bpm: item.bpm,
                        initial_key: &item.initial_key,
                        compilation: item.compilation,
                    },
                );
                if write_res.is_ok() {
                    count += 1;
                }
                writes.push((item.id, new_genre));
            }
            (count, writes)
        })
        .await
        .map_err(|e| e.to_string())?;

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for (song_id, new_genre) in &writes {
        tx.execute(
            "UPDATE songs SET genre = ?1 WHERE id = ?2",
            rusqlite::params![new_genre, song_id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(updated_count)
}

/// Merges `from` into `into`: rewrites every affected song's embedded genre
/// tag on disk and in the DB, then folds `from`'s curated hierarchy position
/// into `into`'s. Returns the number of songs updated.
#[tauri::command]
pub async fn merge_tags(
    app: AppHandle,
    state: State<'_, AppState>,
    from: String,
    into: String,
) -> Result<u32, String> {
    if from.trim().is_empty() || into.trim().is_empty() || from.eq_ignore_ascii_case(&into) {
        return Ok(0);
    }

    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let manager = TagManager::new(state.db.clone());
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let affected = manager
        .songs_containing_any(std::slice::from_ref(&from))
        .map_err(|e| e.to_string())?;
    let song_ids: Vec<i64> = affected.iter().map(|(id, _, _)| *id).collect();

    let from_c = from.clone();
    let into_c = into.clone();
    let updated_count = rewrite_genre_and_persist(conn, &song_ids, move |genre| {
        TagManager::rewrite_genre_for_merge(genre, &from_c, &into_c)
    })
    .await?;

    manager
        .apply_merge_hierarchy(&from, &into)
        .map_err(|e| e.to_string())?;

    let _ = app.emit("library-changed", ());

    Ok(updated_count)
}

/// Deletes `names` from every affected song's genre (both the embedded file
/// tag and the DB, same dual-write as [`merge_tags`]) and removes their
/// hierarchy rows. Returns the number of songs updated.
#[tauri::command]
pub async fn delete_tags(
    app: AppHandle,
    state: State<'_, AppState>,
    names: Vec<String>,
) -> Result<u32, String> {
    let names: Vec<String> = names.into_iter().filter(|n| !n.trim().is_empty()).collect();
    if names.is_empty() {
        return Ok(0);
    }

    let _watcher_pause_guard = WatcherPauseGuard::new(Arc::clone(&state.watcher_paused));

    let manager = TagManager::new(state.db.clone());
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let affected = manager
        .songs_containing_any(&names)
        .map_err(|e| e.to_string())?;
    let song_ids: Vec<i64> = affected.iter().map(|(id, _, _)| *id).collect();

    let names_c = names.clone();
    let updated_count = rewrite_genre_and_persist(conn, &song_ids, move |genre| {
        TagManager::rewrite_genre_for_delete(genre, &names_c)
    })
    .await?;

    manager
        .apply_delete_hierarchy(&names)
        .map_err(|e| e.to_string())?;

    let _ = app.emit("library-changed", ());

    Ok(updated_count)
}
