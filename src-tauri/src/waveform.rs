use crate::db::Database;
use anyhow::{Context, Result};
pub use luminous_analysis::{
    compute_waveform_peaks, decode_mono, WAVEFORM_POINTS, WAVEFORM_STYLE_VERSION,
};
use parking_lot::Mutex;
use rusqlite::params;
use std::path::Path;

static VISUALIZER_GEN_LOCK: Mutex<()> = Mutex::new(());

/// Decodes audio file once and generates/caches both waveform and band waveform data.
pub fn generate_visualizer_data(
    db: &Database,
    song_id: i64,
    path: &Path,
) -> Result<(Vec<u8>, Vec<u8>)> {
    if let (Ok(Some(w)), Ok(Some(m))) = (
        get_cached_waveform(db, song_id),
        crate::band_waveform::get_cached_band_waveform(db, song_id),
    ) {
        return Ok((w, m));
    }

    let _guard = VISUALIZER_GEN_LOCK.lock();

    if let (Ok(Some(w)), Ok(Some(m))) = (
        get_cached_waveform(db, song_id),
        crate::band_waveform::get_cached_band_waveform(db, song_id),
    ) {
        return Ok((w, m));
    }

    let decoder = crate::media_source::open_offline(db, path).with_context(|| {
        format!(
            "failed to decode samples for song_id {} at path {:?}",
            song_id, path
        )
    })?;
    let analysis = luminous_analysis::analyze_visualizer(decoder);

    let mut conn = db.pool.get()?;
    let tx = conn.transaction()?;

    tx.execute(
        "INSERT OR REPLACE INTO waveforms (song_id, data, style) VALUES (?1, ?2, ?3)",
        params![song_id, analysis.waveform_peaks, WAVEFORM_STYLE_VERSION],
    )?;

    tx.execute(
        "INSERT OR REPLACE INTO band_waveforms (song_id, data, style) VALUES (?1, ?2, ?3)",
        params![
            song_id,
            analysis.band_waveform_data,
            crate::band_waveform::BAND_WAVEFORM_STYLE_VERSION
        ],
    )?;

    tx.commit()?;

    Ok((analysis.waveform_peaks, analysis.band_waveform_data))
}

/// Generate peak waveform amplitudes for a song, save to SQLite, and return them.
pub fn generate_waveform(db: &Database, song_id: i64, path: &Path) -> Result<Vec<u8>> {
    match generate_visualizer_data(db, song_id, path) {
        Ok((waveform, _)) => Ok(waveform),
        Err(e) => {
            log::error!(
                "Failed to generate waveform for song_id {} at {:?}: {:?}",
                song_id,
                path,
                e
            );
            Err(e)
        }
    }
}

/// Retrieve the cached waveform data from the database, if it was written by
/// the current `WAVEFORM_STYLE_VERSION`. A blob cached under an older style is
/// treated as a cache miss so callers regenerate it rather than misinterpret it.
pub fn get_cached_waveform(db: &Database, song_id: i64) -> Result<Option<Vec<u8>>> {
    let conn = db.pool.get()?;
    let row: Option<(Vec<u8>, i64)> = conn
        .query_row(
            "SELECT data, style FROM waveforms WHERE song_id = ?1",
            params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    Ok(row.and_then(|(data, style)| {
        if style == WAVEFORM_STYLE_VERSION {
            Some(data)
        } else {
            None
        }
    }))
}

/// Queries database for songs missing either waveform or band waveform cache, or whose
/// cached waveform/band waveform was written by an older style version and needs
/// regenerating under the current format.
pub fn get_missing_visualizer_songs(db: &Database) -> Result<Vec<(i64, String)>> {
    let conn = db.pool.get()?;
    let sql = format!(
        "SELECT s.id, s.path FROM songs s
         LEFT JOIN waveforms w ON s.id = w.song_id
         LEFT JOIN band_waveforms m ON s.id = m.song_id
         WHERE s.source IN ({lib}) AND s.unavailable = 0 AND s.path IS NOT NULL
           AND (w.song_id IS NULL OR m.song_id IS NULL OR w.style != ?1 OR m.style != ?2)",
        lib = *crate::models::LOCAL_SOURCES_SQL
    );
    let mut stmt = conn.prepare(&sql)?;

    let missing_songs: Vec<(i64, String)> = stmt
        .query_map(
            params![
                WAVEFORM_STYLE_VERSION,
                crate::band_waveform::BAND_WAVEFORM_STYLE_VERSION
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .filter_map(|r| r.ok())
        .collect();
    Ok(missing_songs)
}

/// Scans database for songs missing visualizer cache and generates them, invoking a progress callback for each song.
pub fn backfill_missing_visualizers_with_progress<F>(
    db: &Database,
    mut progress_cb: F,
) -> Result<usize>
where
    F: FnMut(usize, usize, &str),
{
    let missing_songs = get_missing_visualizer_songs(db)?;
    if missing_songs.is_empty() {
        return Ok(0);
    }

    let total = missing_songs.len();
    log::info!("Backfilling missing visualizers for {total} songs");
    let mut count = 0;

    for (idx, (song_id, path_str)) in missing_songs.into_iter().enumerate() {
        let path = Path::new(&path_str);
        let file_name = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| path_str.clone());

        progress_cb(idx + 1, total, &file_name);

        if !path.exists() {
            continue;
        }

        if let Err(e) = generate_visualizer_data(db, song_id, path) {
            log::warn!(
                "Failed backfilling visualizer data for song {} ({}): {:?}",
                song_id,
                path_str,
                e
            );
        } else {
            count += 1;
        }
    }

    log::info!("Completed visualizer backfill for {count} songs");
    Ok(count)
}

/// Scans database for songs that are missing either waveform or band waveform cache and generates them.
pub fn backfill_missing_visualizers(db: &Database) -> Result<usize> {
    backfill_missing_visualizers_with_progress(db, |_, _, _| {})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stale_waveform_style_is_treated_as_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, unavailable) VALUES ('/tmp/song.mp3', 0)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        // Cached under a stale style (pre-RMS-fix peak-based format).
        conn.execute(
            "INSERT INTO waveforms (song_id, data, style) VALUES (?1, ?2, 0)",
            params![song_id, vec![0u8; 150]],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO band_waveforms (song_id, data, style) VALUES (?1, ?2, ?3)",
            params![
                song_id,
                vec![0u8; crate::band_waveform::BAND_WAVEFORM_POINTS * 3],
                crate::band_waveform::BAND_WAVEFORM_STYLE_VERSION
            ],
        )
        .unwrap();
        drop(conn);

        assert_eq!(get_cached_waveform(&db, song_id).unwrap(), None);

        let missing = get_missing_visualizer_songs(&db).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].0, song_id);
    }

    #[test]
    fn test_backfill_missing_visualizers_empty_db() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        let backfilled = backfill_missing_visualizers(&db).unwrap();
        assert_eq!(backfilled, 0);
    }

    #[test]
    fn test_stale_band_waveform_style_is_treated_as_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO songs (path, unavailable) VALUES ('/tmp/song.mp3', 0)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO waveforms (song_id, data) VALUES (?1, ?2)",
            params![song_id, vec![0u8; 150]],
        )
        .unwrap();
        // Cached under a stale style (pre-#217 blended-color format).
        conn.execute(
            "INSERT INTO band_waveforms (song_id, data, style) VALUES (?1, ?2, 0)",
            params![song_id, vec![0u8; 450]],
        )
        .unwrap();
        drop(conn);

        assert_eq!(
            crate::band_waveform::get_cached_band_waveform(&db, song_id).unwrap(),
            None
        );

        let missing = get_missing_visualizer_songs(&db).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].0, song_id);
    }

    #[test]
    fn test_missing_visualizer_songs_ignores_remote_sources() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        // Local song without waveform -> should be included
        conn.execute(
            "INSERT INTO songs (path, unavailable, source) VALUES ('/tmp/local.mp3', 0, ?1)",
            params![crate::models::SongSource::LocalFile as i32],
        )
        .unwrap();
        let local_id = conn.last_insert_rowid();

        // WebDAV song without waveform -> must NOT be included (#1785)
        conn.execute(
            "INSERT INTO songs (path, unavailable, source) VALUES ('https://nas/remote.mp3', 0, ?1)",
            params![crate::models::SongSource::WebDav as i32],
        )
        .unwrap();

        // Subsonic song without waveform -> must NOT be included (#1785)
        conn.execute(
            "INSERT INTO songs (path, unavailable, source) VALUES ('subsonic://1/1', 0, ?1)",
            params![crate::models::SongSource::Subsonic as i32],
        )
        .unwrap();

        drop(conn);

        let missing = get_missing_visualizer_songs(&db).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].0, local_id);
    }
}
