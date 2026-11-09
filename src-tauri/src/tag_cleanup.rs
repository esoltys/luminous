//! Tag clean-up for library songs (#1939): maps song ids to files for
//! `luminous_metadata::sanitize`, skips songs whose tags can't be written
//! here, and keeps the stored comment in step with the file afterwards.

use crate::collection::SelfWriteTracker;
use crate::db::Database;
use crate::models::{SongSource, LOCAL_SOURCES_SQL};
use luminous_metadata::{audio, find_cruft, strip_cruft, CruftField};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One song's removable fields, as shown in the review dialog.
#[derive(Debug, Clone, Serialize)]
pub struct SongCruft {
    pub song_id: i64,
    pub title: String,
    pub artist: String,
    pub fields: Vec<CruftField>,
}

/// What [`preview`] found. Clean songs are omitted from `songs`. `skipped`
/// counts songs that couldn't be checked: cut from a CUE sheet (their tags
/// live in the .cue file), remote (no local file), or unreadable.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TagCleanupPreview {
    pub songs: Vec<SongCruft>,
    pub skipped: usize,
}

/// The fields the user kept ticked for one song.
#[derive(Debug, Clone, Deserialize)]
pub struct TagCleanupSelection {
    pub song_id: i64,
    pub fields: Vec<CruftField>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TagCleanupResult {
    pub cleaned_songs: usize,
    pub removed_fields: usize,
    pub errors: Vec<String>,
}

struct SongFile {
    id: i64,
    path: String,
    title: String,
    artist: String,
    writable: bool,
}

/// Files read in parallel between progress reports.
const PREVIEW_CHUNK: usize = 64;

/// Lists the cruft in `song_ids`' files; an empty list means the whole local
/// library. Reads every file (in parallel, since libraries often sit on a
/// network share where each read is a round trip), so callers run it off the
/// async runtime. `on_progress(checked, total)` is called after each chunk
/// and returns whether to go on; returning `false` abandons the preview.
pub fn preview(
    db: &Database,
    song_ids: &[i64],
    mut on_progress: impl FnMut(usize, usize) -> bool,
) -> Result<TagCleanupPreview, String> {
    let songs = load_songs(db, song_ids)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(crate::collection::scan_thread_count())
        .build()
        .map_err(|e| e.to_string())?;
    let total = songs.len();
    let mut result = TagCleanupPreview::default();
    let mut checked = 0;

    for chunk in songs.chunks(PREVIEW_CHUNK) {
        let found: Vec<Option<Vec<CruftField>>> = pool.install(|| {
            chunk
                .par_iter()
                .map(|song| {
                    if !song.writable {
                        return None;
                    }
                    find_cruft(&PathBuf::from(&song.path))
                        .map_err(|e| log::warn!("tag clean-up: cannot read {}: {e:#}", song.path))
                        .ok()
                })
                .collect()
        });
        for (song, fields) in chunk.iter().zip(found) {
            match fields {
                None => result.skipped += 1,
                Some(fields) if fields.is_empty() => {}
                Some(fields) => result.songs.push(SongCruft {
                    song_id: song.id,
                    title: song.title.clone(),
                    artist: song.artist.clone(),
                    fields,
                }),
            }
        }
        checked += chunk.len();
        if !on_progress(checked, total) {
            return Err("Tag clean-up preview cancelled".to_string());
        }
    }
    Ok(result)
}

/// Removes the selected fields and refreshes each changed song's stored
/// comment. A song that fails is reported in `errors` without stopping the
/// rest; selections for songs that can't be written are ignored.
/// `on_progress(done, total)` is called after each selection.
pub fn apply(
    db: &Database,
    self_writes: &SelfWriteTracker,
    selections: &[TagCleanupSelection],
    mut on_progress: impl FnMut(usize, usize),
) -> Result<TagCleanupResult, String> {
    let ids: Vec<i64> = selections.iter().map(|s| s.song_id).collect();
    if ids.is_empty() {
        return Ok(TagCleanupResult::default());
    }
    let songs = load_songs(db, &ids)?;
    let conn = db.pool.get().map_err(|e| e.to_string())?;
    let mut result = TagCleanupResult::default();

    for (done, selection) in selections.iter().enumerate() {
        on_progress(done, selections.len());
        let Some(song) = songs
            .iter()
            .find(|s| s.id == selection.song_id && s.writable)
            .filter(|_| !selection.fields.is_empty())
        else {
            continue;
        };
        let path = PathBuf::from(&song.path);
        self_writes.mark_written(std::iter::once(path.clone()));
        match strip_cruft(&path, &selection.fields) {
            Ok(0) => {}
            Ok(removed) => {
                result.cleaned_songs += 1;
                result.removed_fields += removed;
                let comment = audio::read_tags(&path).ok().and_then(|m| m.comment);
                let before = crate::tag_history::snapshot_by_id(&conn, song.id)
                    .ok()
                    .flatten();
                let (new_mtime, new_filesize) = crate::tag_history::file_mtime_and_size(&path);
                if let Err(e) = conn.execute(
                    "UPDATE songs SET comment = ?1, mtime = COALESCE(?2, mtime), filesize = COALESCE(?3, filesize) WHERE id = ?4",
                    rusqlite::params![comment, new_mtime, new_filesize, song.id],
                ) {
                    log::warn!("tag clean-up: cannot update comment for {}: {e}", song.id);
                } else {
                    let _ = crate::tag_history::record_diff_after_update(
                        &conn,
                        before.as_ref(),
                        crate::tag_history::HistoryActor::Luminous,
                        None,
                    );
                }
            }
            Err(e) => result.errors.push(format!("{}: {e:#}", song.path)),
        }
    }
    on_progress(selections.len(), selections.len());
    Ok(result)
}

/// Loads `song_ids` (or every local song when empty), marking which ones
/// have a local, non-CUE file whose tags this module may write.
fn load_songs(db: &Database, song_ids: &[i64]) -> Result<Vec<SongFile>, String> {
    let conn = db.pool.get().map_err(|e| e.to_string())?;
    let columns = "id, path, COALESCE(title, ''), COALESCE(artist, ''), source, cue_path";
    let row_to_song = |row: &rusqlite::Row| -> rusqlite::Result<SongFile> {
        let source: i64 = row.get(4)?;
        let cue_path: Option<String> = row.get(5)?;
        Ok(SongFile {
            id: row.get(0)?,
            path: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            writable: cue_path.is_none() && !SongSource::from(source).is_remote(),
        })
    };

    let mut songs = Vec::new();
    if song_ids.is_empty() {
        let sql = format!(
            "SELECT {columns} FROM songs
             WHERE path IS NOT NULL AND TRIM(path) != '' AND source IN ({lib}) AND unavailable = 0",
            lib = *LOCAL_SOURCES_SQL
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], row_to_song).map_err(|e| e.to_string())?;
        songs.extend(rows.filter_map(Result::ok));
    } else {
        for chunk in song_ids.chunks(500) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!("SELECT {columns} FROM songs WHERE id IN ({placeholders})");
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(chunk), row_to_song)
                .map_err(|e| e.to_string())?;
            songs.extend(rows.filter_map(Result::ok));
        }
    }
    Ok(songs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::SelfWriteTracker;
    use crate::db::Database;
    use std::path::Path;
    use tempfile::Builder;

    #[test]
    fn test_apply_records_tag_history() {
        let temp_dir = Builder::new()
            .prefix("luminous_tag_cleanup_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();
        let self_writes = SelfWriteTracker::default();

        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio/song_alpha.mp3");
        let dest = temp_dir.path().join("song_alpha.mp3");
        std::fs::copy(&src, &dest).expect("copy fixture");

        use lofty::config::WriteOptions;
        use lofty::id3::v2::{CommentFrame, Frame, Id3v2Tag};
        use lofty::tag::TagExt;
        use lofty::TextEncoding;

        let promo = "Visit FreeMusicSite.com for more free music!!!";
        let mut tag = Id3v2Tag::new();
        tag.insert(Frame::Comment(CommentFrame::new(
            TextEncoding::UTF8,
            *b"eng",
            "".to_string(),
            promo.to_string(),
        )));
        tag.save_to_path(&dest, WriteOptions::default()).unwrap();

        let mut initial_song = crate::collection::read_tags(&dest).unwrap();
        initial_song.source = crate::models::SongSource::LocalFile;
        crate::collection::upsert_song(&conn, &initial_song).unwrap();

        let song_id: i64 = conn
            .query_row(
                "SELECT id FROM songs WHERE path = ?1",
                rusqlite::params![dest.to_string_lossy().to_string()],
                |r| r.get(0),
            )
            .unwrap();

        let cruft = find_cruft(&dest).unwrap();
        assert!(!cruft.is_empty(), "should have found cruft");

        let selections = vec![TagCleanupSelection {
            song_id,
            fields: cruft,
        }];

        let result = apply(&db, &self_writes, &selections, |_, _| {}).unwrap();
        assert_eq!(result.cleaned_songs, 1);

        let history = crate::tag_history::get_song_history(&conn, song_id).unwrap();
        assert_eq!(history.entries.len(), 1);
        assert_eq!(
            history.entries[0].actor,
            crate::tag_history::HistoryActor::Luminous
        );
        assert_eq!(history.entries[0].field, "comment");
        assert_eq!(history.entries[0].old_value.as_deref(), Some(promo));
        assert_eq!(history.entries[0].new_value, None);
    }
}
