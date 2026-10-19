use crate::db::Database;
use crate::waveform::generate_visualizer_data;
use anyhow::Result;
pub use luminous_analysis::{
    compute_band_waveform_data, BAND_WAVEFORM_POINTS, BAND_WAVEFORM_STYLE_VERSION,
};
use rusqlite::params;
use std::path::Path;

/// Generate the layered low/mid/high band waveform data, save to SQLite, and return it.
pub fn generate_band_waveform(db: &Database, song_id: i64, path: &Path) -> Result<Vec<u8>> {
    match generate_visualizer_data(db, song_id, path) {
        Ok((_, band_waveform)) => Ok(band_waveform),
        Err(e) => {
            log::error!(
                "Failed to generate band waveform for song_id {} at {:?}: {:?}",
                song_id,
                path,
                e
            );
            Err(e)
        }
    }
}

/// Retrieve the cached band waveform data from the database, if it was written
/// by the current `BAND_WAVEFORM_STYLE_VERSION`. A blob cached under an older
/// style is treated as a cache miss so callers regenerate it rather than
/// misinterpret it.
pub fn get_cached_band_waveform(db: &Database, song_id: i64) -> Result<Option<Vec<u8>>> {
    let conn = db.pool.get()?;
    let row: Option<(Vec<u8>, i64)> = conn
        .query_row(
            "SELECT data, style FROM band_waveforms WHERE song_id = ?1",
            params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    Ok(row.and_then(|(data, style)| {
        if style == BAND_WAVEFORM_STYLE_VERSION {
            Some(data)
        } else {
            None
        }
    }))
}
