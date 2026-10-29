//! A playlist's size limit and which of its songs fit within it.
//!
//! The rule: songs fit from the top of the playlist's order until the first
//! one that would take the total past the limit; nothing after that point
//! counts, even smaller songs that would still fit. So everything above the
//! break fits, and the break is a single index.

use super::PlaylistManager;
use crate::models::PlaylistItem;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
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

/// The device a fit's sizes were estimated for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FitDevice {
    pub id: String,
    /// The device's nickname, or its default name.
    pub name: String,
}

/// The size limit applied to a playlist, with the item order it was
/// computed for so a view can tell when the answer is stale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlaylistSizeFit {
    pub limit_bytes: u64,
    #[serde(flatten)]
    pub fit: SizeFit,
    /// Item UUIDs in the order the fit was computed over.
    pub order: Vec<String>,
    /// The device whose converted sizes were counted; `None` counts the
    /// library files as they are.
    pub device: Option<FitDevice>,
}

/// What a playlist's size fit is computed from. Read in one quick query, so
/// measuring the songs (slow when converted sizes are estimated) happens
/// without holding the playlist lock or a database connection.
#[derive(Debug, Clone)]
pub struct SizeLimitInput {
    pub limit_bytes: u64,
    /// The device the limit is measured for, if one was chosen.
    pub device_id: Option<String>,
    /// The playlist's items in its order.
    pub items: Vec<PlaylistItem>,
}

impl SizeLimitInput {
    /// Applies the limit. `size_on_device` gets each copyable item and its
    /// library file size and returns the size it takes where it's going;
    /// items that can't be copied take no space. `device` names whose sizes
    /// those are.
    pub fn fit(
        self,
        device: Option<FitDevice>,
        mut size_on_device: impl FnMut(&PlaylistItem, u64) -> u64,
    ) -> PlaylistSizeFit {
        let sizes: Vec<Option<u64>> = self
            .items
            .iter()
            .map(|item| library_size(item).map(|bytes| size_on_device(item, bytes)))
            .collect();
        PlaylistSizeFit {
            limit_bytes: self.limit_bytes,
            fit: fit_to_limit(&sizes, self.limit_bytes),
            order: self.items.into_iter().map(|i| i.uuid).collect(),
            device,
        }
    }
}

/// An item's library file size, or `None` when it can't be copied.
fn library_size(item: &PlaylistItem) -> Option<u64> {
    let song = item.song.as_ref().filter(|s| !s.unavailable)?;
    song.filesize
        .and_then(|b| u64::try_from(b).ok())
        .filter(|&b| b > 0)
}

impl PlaylistManager {
    /// Sets a playlist's size limit in bytes and the device (a
    /// `device_profiles` id) it's measured for, `None` for library files as
    /// they are. A limit of `None` or 0 removes both. A no-op for the Queue.
    pub fn set_playlist_size_limit(
        &self,
        playlist_id: i64,
        limit_bytes: Option<u64>,
        device_id: Option<&str>,
    ) -> Result<()> {
        if self.is_queue_id(playlist_id)? {
            return Ok(());
        }
        let limit = limit_bytes
            .filter(|&b| b > 0)
            .map(|b| i64::try_from(b).unwrap_or(i64::MAX));
        let device = limit.and(device_id);
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE playlists SET size_limit_bytes = ?1, size_limit_device = ?2 WHERE id = ?3",
            params![limit, device, playlist_id],
        )?;
        Ok(())
    }

    /// The playlist's limit, device and items in its order, for
    /// [`SizeLimitInput::fit`]. `None` when it has no limit (or doesn't
    /// exist).
    pub fn size_limit_input(&self, playlist_id: i64) -> Result<Option<SizeLimitInput>> {
        let row: Option<(Option<i64>, Option<String>)> = {
            let conn = self.db.pool.get()?;
            conn.query_row(
                "SELECT size_limit_bytes, size_limit_device FROM playlists WHERE id = ?1",
                params![playlist_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
        };
        let Some((limit, device_id)) = row else {
            return Ok(None);
        };
        let Some(limit_bytes) = limit.and_then(|b| u64::try_from(b).ok()).filter(|&b| b > 0) else {
            return Ok(None);
        };
        Ok(Some(SizeLimitInput {
            limit_bytes,
            device_id,
            items: self.get_playlist_tracks(playlist_id)?,
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

    fn item(uuid: &str, filesize: Option<i64>, unavailable: bool) -> PlaylistItem {
        let song = crate::models::Song {
            filesize,
            unavailable,
            ..Default::default()
        };
        PlaylistItem {
            uuid: uuid.into(),
            ..PlaylistItem::new_song(1, 0, song)
        }
    }

    #[test]
    fn device_sizes_replace_library_sizes_but_uncopyable_songs_stay_out() {
        let input = SizeLimitInput {
            limit_bytes: 10,
            device_id: Some("vol-1".into()),
            items: vec![
                item("a", Some(40), false),
                item("gone", Some(40), true),
                item("b", Some(40), false),
                item("c", Some(40), false),
            ],
        };
        let device = FitDevice {
            id: "vol-1".into(),
            name: "HiBy R1".into(),
        };
        // Converted, each song takes a tenth of its library size: 4 + 4 fit, 4 more doesn't.
        let mut asked = Vec::new();
        let f = input.fit(Some(device.clone()), |item, library| {
            asked.push(item.uuid.clone());
            library / 10
        });
        assert_eq!(
            asked,
            ["a", "b", "c"],
            "the unavailable song is never measured"
        );
        assert_eq!(
            (
                f.fit.break_index,
                f.fit.fit_songs,
                f.fit.fit_bytes,
                f.fit.total_bytes
            ),
            (3, 2, 8, 12)
        );
        assert_eq!(f.order, ["a", "gone", "b", "c"]);
        assert_eq!(f.device, Some(device));
    }
}
