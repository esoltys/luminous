//! A playlist's size limit and which of its songs fit within it.
//!
//! The rule: songs fit from the top of the playlist's order until the first
//! one that would take the total past the limit; nothing after that point
//! counts, even smaller songs that would still fit. So everything above the
//! break fits, and the break is a single index.

use super::PlaylistManager;
use crate::models::PlaylistItem;
use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

/// Where a size limit falls in a sequence of songs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SizeFit {
    /// How many entries sit above the break — the index of the first entry
    /// that doesn't fit, or the length when everything fits.
    pub break_index: usize,
    /// Songs above the break that take space (excludes entries with no size).
    pub fit_songs: usize,
    pub fit_bytes: u64,
    /// Size of every sized entry, above and below the break.
    pub total_bytes: u64,
}

/// Applies the size limit to `sizes`, given in playlist order. `None` is an
/// entry that can't be copied (missing file, unknown size): it takes no
/// space and is never counted as fitting. The sizes are whatever the caller
/// measures — library file sizes for the playlist view, sizes after
/// converting for a device sync.
pub fn fit_to_limit(sizes: &[Option<u64>], limit_bytes: u64) -> SizeFit {
    let total_bytes = sizes.iter().flatten().sum();
    let mut fit = SizeFit {
        break_index: sizes.len(),
        fit_songs: 0,
        fit_bytes: 0,
        total_bytes,
    };
    for (index, size) in sizes.iter().enumerate() {
        let Some(size) = *size else { continue };
        if fit.fit_bytes + size > limit_bytes {
            fit.break_index = index;
            break;
        }
        fit.fit_bytes += size;
        fit.fit_songs += 1;
    }
    fit
}

/// The size limit applied to a playlist's library files, with the item
/// order it was computed for so a view can tell when the answer is stale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlaylistSizeFit {
    pub limit_bytes: u64,
    #[serde(flatten)]
    pub fit: SizeFit,
    /// Item UUIDs in the order the fit was computed over.
    pub order: Vec<String>,
}

/// An item's library file size, or `None` when it can't be copied.
fn library_size(item: &PlaylistItem) -> Option<u64> {
    let song = item.song.as_ref().filter(|s| !s.unavailable)?;
    song.filesize
        .and_then(|b| u64::try_from(b).ok())
        .filter(|&b| b > 0)
}

impl PlaylistManager {
    /// Sets a playlist's size limit in bytes; `None` or 0 removes it. A
    /// no-op for the Queue.
    pub fn set_playlist_size_limit(
        &self,
        playlist_id: i64,
        limit_bytes: Option<u64>,
    ) -> Result<()> {
        if self.is_queue_id(playlist_id)? {
            return Ok(());
        }
        let limit = limit_bytes
            .filter(|&b| b > 0)
            .map(|b| i64::try_from(b).unwrap_or(i64::MAX));
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE playlists SET size_limit_bytes = ?1 WHERE id = ?2",
            params![limit, playlist_id],
        )?;
        Ok(())
    }

    /// Which of the playlist's songs fit its size limit, measured by their
    /// library file sizes in the playlist's order. `None` when the playlist
    /// has no limit (or doesn't exist).
    pub fn playlist_size_fit(&self, playlist_id: i64) -> Result<Option<PlaylistSizeFit>> {
        let limit: Option<i64> = {
            let conn = self.db.pool.get()?;
            conn.query_row(
                "SELECT size_limit_bytes FROM playlists WHERE id = ?1",
                params![playlist_id],
                |row| row.get(0),
            )
            .unwrap_or(None)
        };
        let Some(limit_bytes) = limit.and_then(|b| u64::try_from(b).ok()).filter(|&b| b > 0) else {
            return Ok(None);
        };
        let items = self.get_playlist_tracks(playlist_id)?;
        let sizes: Vec<Option<u64>> = items.iter().map(library_size).collect();
        Ok(Some(PlaylistSizeFit {
            limit_bytes,
            fit: fit_to_limit(&sizes, limit_bytes),
            order: items.into_iter().map(|i| i.uuid).collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(sizes: &[Option<u64>], limit: u64) -> (usize, usize, u64, u64) {
        let f = fit_to_limit(sizes, limit);
        (f.break_index, f.fit_songs, f.fit_bytes, f.total_bytes)
    }

    #[test]
    fn everything_fits_below_the_limit() {
        assert_eq!(fit(&[Some(3), Some(4)], 10), (2, 2, 7, 7));
    }

    #[test]
    fn an_exact_fit_includes_the_last_song() {
        assert_eq!(fit(&[Some(4), Some(6), Some(1)], 10), (2, 2, 10, 11));
    }

    #[test]
    fn songs_after_the_first_overflow_never_fit() {
        // The 1-byte song would fit in the space left, but sits below the break.
        assert_eq!(fit(&[Some(5), Some(9), Some(1)], 10), (1, 1, 5, 15));
    }

    #[test]
    fn a_first_song_larger_than_the_limit_puts_the_break_at_the_top() {
        assert_eq!(fit(&[Some(20), Some(1)], 10), (0, 0, 0, 21));
    }

    #[test]
    fn unsized_entries_take_no_space_and_are_not_counted() {
        assert_eq!(fit(&[None, Some(6), None, Some(6)], 10), (3, 1, 6, 12));
    }

    #[test]
    fn an_empty_playlist_fits() {
        assert_eq!(fit(&[], 10), (0, 0, 0, 0));
    }
}
