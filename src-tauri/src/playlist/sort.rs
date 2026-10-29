//! A playlist's order: manual (by `position`) or sorted by a `Song` field.
//!
//! The order is persisted on the playlist (`sort_field`, `sort_asc`) and
//! applied by `get_playlist_tracks`, so the playlist view, playback, export,
//! the size limit's fit and device sync all see the same sequence. The
//! Queue is never sorted here: its order is the player's playback order, and
//! its view sorts on the frontend only.

use super::PlaylistManager;
use crate::models::{PlaylistItem, Song};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::cmp::Ordering;
use std::collections::HashMap;

/// `Song` fields a playlist can be sorted by — the sortable song-table
/// columns (`src/lib/utils/songColumns.ts`). Anything else is stored as
/// manual order.
const SORT_FIELDS: &[&str] = &[
    "track",
    "title",
    "artist",
    "album",
    "composer",
    "album_artist",
    "artist_tag",
    "filetype",
    "year",
    "originalyear",
    "genre",
    "grouping",
    "bpm",
    "initial_key",
    "musicbrainz_track_id",
    "bitrate",
    "samplerate",
    "bitdepth",
    "channels",
    "filesize",
    "rating",
    "playcount",
    "skipcount",
    "lastplayed",
    "added",
    "length_nanosec",
    "path",
];

/// A song's value for one sort field. `Missing` always sorts last, whatever
/// the direction.
#[derive(Debug, PartialEq)]
enum SortKey {
    Text(String),
    Number(f64),
    Missing,
}

fn text(value: Option<&str>) -> SortKey {
    value.map_or(SortKey::Missing, |s| SortKey::Text(s.to_string()))
}

fn number<T: Into<f64>>(value: Option<T>) -> SortKey {
    value.map_or(SortKey::Missing, |n| SortKey::Number(n.into()))
}

/// The first non-blank of `values`, trimmed only for the `*sort` overrides
/// (matching the frontend's `a.titlesort?.trim() || a.title`).
fn sort_name<'a>(overrides: &[&'a Option<String>], value: &'a Option<String>) -> SortKey {
    overrides
        .iter()
        .filter_map(|o| o.as_deref().map(str::trim))
        .find(|s| !s.is_empty())
        .map_or_else(|| text(value.as_deref()), |s| SortKey::Text(s.to_string()))
}

fn sort_key(song: &Song, field: &str, artist_tags: &HashMap<String, String>) -> SortKey {
    match field {
        "track" => number(song.track),
        "title" => sort_name(&[&song.titlesort], &song.title),
        "artist" => {
            let name = song
                .album_artist
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| song.artist.clone());
            sort_name(&[&song.album_artist_sort, &song.artistsort], &name)
        }
        "album" => text(song.album.as_deref()),
        "composer" => sort_name(&[&song.composersort], &song.composer),
        "album_artist" => text(song.album_artist.as_deref()),
        "artist_tag" => {
            let artist = song
                .album_artist
                .as_deref()
                .filter(|s| !s.is_empty())
                .or(song.artist.as_deref());
            text(artist.and_then(|a| artist_tags.get(&a.to_lowercase()).map(String::as_str)))
        }
        "filetype" => match serde_json::to_value(song.filetype) {
            Ok(serde_json::Value::String(s)) => SortKey::Text(s),
            _ => SortKey::Missing,
        },
        "year" => number(song.year),
        "originalyear" => number(song.originalyear),
        "genre" => sort_name(&[&song.genresort], &song.genre),
        "grouping" => text(song.grouping.as_deref()),
        "bpm" => number(song.bpm),
        "initial_key" => text(song.initial_key.as_deref()),
        "musicbrainz_track_id" => text(song.musicbrainz_track_id.as_deref()),
        "bitrate" => number(song.bitrate),
        "samplerate" => number(song.samplerate),
        "bitdepth" => number(song.bitdepth),
        "channels" => number(song.channels),
        "filesize" => number(song.filesize.map(|n| n as f64)),
        "rating" => number(Some(song.rating)),
        "playcount" => number(Some(song.playcount)),
        "skipcount" => number(Some(song.skipcount)),
        "lastplayed" => number(song.lastplayed.map(|n| n as f64)),
        "added" => number(song.added.map(|n| n as f64)),
        "length_nanosec" => number(song.length_nanosec.map(|n| n as f64)),
        "path" => text(song.path.as_deref()),
        _ => SortKey::Missing,
    }
}

/// Case-, accent- and digit-aware text order: "Track 2" before "Track 10",
/// "élan" next to "Elan" (the frontend's `Intl.Collator` with
/// `numeric: true, sensitivity: "base"`).
fn compare_text(a: &str, b: &str) -> Ordering {
    let fold = |s: &str| deunicode::deunicode(s).to_lowercase();
    let (a, b) = (fold(a), fold(b));
    let (mut a, mut b) = (a.as_str(), b.as_str());
    loop {
        match (a.chars().next(), b.chars().next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let da = a.find(|c: char| !c.is_ascii_digit()).unwrap_or(a.len());
                let db = b.find(|c: char| !c.is_ascii_digit()).unwrap_or(b.len());
                let (na, nb) = (
                    a[..da].trim_start_matches('0'),
                    b[..db].trim_start_matches('0'),
                );
                let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
                if ord != Ordering::Equal {
                    return ord;
                }
                a = &a[da..];
                b = &b[db..];
            }
            (Some(ca), Some(cb)) => {
                if ca != cb {
                    return ca.cmp(&cb);
                }
                a = &a[ca.len_utf8()..];
                b = &b[cb.len_utf8()..];
            }
        }
    }
}

fn compare_keys(a: &SortKey, b: &SortKey, asc: bool) -> Ordering {
    let ord = match (a, b) {
        (SortKey::Missing, SortKey::Missing) => return Ordering::Equal,
        (SortKey::Missing, _) => return Ordering::Greater,
        (_, SortKey::Missing) => return Ordering::Less,
        (SortKey::Text(x), SortKey::Text(y)) => compare_text(x, y),
        (SortKey::Number(x), SortKey::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        _ => Ordering::Equal,
    };
    if asc {
        ord
    } else {
        ord.reverse()
    }
}

/// Puts `items` (in position order) into the playlist's order. Songs with
/// equal values keep their manual order; items with no song, or no value
/// for the field, go last. `field: None` is manual order (`asc: false`
/// reverses it).
pub(super) fn apply_sort(
    items: &mut [PlaylistItem],
    field: Option<&str>,
    asc: bool,
    artist_tags: &HashMap<String, String>,
) {
    match field {
        None => {
            if !asc {
                items.reverse();
            }
        }
        Some(field) => {
            let mut keyed: Vec<(SortKey, PlaylistItem)> = items
                .iter()
                .map(|item| {
                    let key = item
                        .song
                        .as_ref()
                        .map_or(SortKey::Missing, |s| sort_key(s, field, artist_tags));
                    (key, item.clone())
                })
                .collect();
            keyed.sort_by(|(a, _), (b, _)| compare_keys(a, b, asc));
            for (slot, (_, item)) in items.iter_mut().zip(keyed) {
                *slot = item;
            }
        }
    }
}

/// Each artist's first tag, keyed by lowercased artist name — only loaded
/// when sorting by `artist_tag`.
fn load_first_artist_tags(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt =
        conn.prepare("SELECT artist_key, tags FROM artist_profiles WHERE tags != '[]'")?;
    let map = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(artist, tags)| {
            let first = serde_json::from_str::<Vec<String>>(&tags)
                .ok()?
                .into_iter()
                .next()?;
            Some((artist.to_lowercase(), first))
        })
        .collect();
    Ok(map)
}

/// The persisted `(sort_field, sort_asc)` of a playlist; manual ascending
/// when the playlist is missing or the stored field isn't sortable.
pub(super) fn load_sort(conn: &Connection, playlist_id: i64) -> Result<(Option<String>, bool)> {
    let row: Option<(Option<String>, Option<bool>)> = conn
        .query_row(
            "SELECT sort_field, sort_asc FROM playlists WHERE id = ?1",
            params![playlist_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (field, asc) = row.unwrap_or((None, None));
    let field = field.filter(|f| SORT_FIELDS.contains(&f.as_str()));
    Ok((field, asc.unwrap_or(true)))
}

/// Reorders position-ordered `items` of `playlist_id` into its persisted order.
pub(super) fn order_items(
    conn: &Connection,
    playlist_id: i64,
    items: &mut [PlaylistItem],
) -> Result<()> {
    let (field, asc) = load_sort(conn, playlist_id)?;
    let tags = if field.as_deref() == Some("artist_tag") {
        load_first_artist_tags(conn)?
    } else {
        HashMap::new()
    };
    apply_sort(items, field.as_deref(), asc, &tags);
    Ok(())
}

impl PlaylistManager {
    /// Sorts a playlist by `field` (a sortable `Song` field), or returns it
    /// to manual order with `field: None`. A no-op for the Queue, whose
    /// order is the playback order. An unknown field is stored as manual.
    pub fn set_playlist_sort(
        &self,
        playlist_id: i64,
        field: Option<&str>,
        asc: bool,
    ) -> Result<()> {
        if self.is_queue_id(playlist_id)? {
            return Ok(());
        }
        let field = field.filter(|f| SORT_FIELDS.contains(f));
        let conn = self.db.pool.get()?;
        conn.execute(
            "UPDATE playlists SET sort_field = ?1, sort_asc = ?2 WHERE id = ?3",
            params![field, asc, playlist_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PlaylistItemType, Song};

    fn item(uuid: &str, song: Option<Song>) -> PlaylistItem {
        PlaylistItem {
            id: 0,
            playlist_id: 1,
            position: 0,
            uuid: uuid.to_string(),
            item_type: PlaylistItemType::Song,
            song,
            url: None,
            stream_url: None,
            additional_metadata: None,
        }
    }

    fn titled(uuid: &str, title: Option<&str>) -> PlaylistItem {
        item(
            uuid,
            Some(Song {
                title: title.map(str::to_string),
                ..Default::default()
            }),
        )
    }

    fn uuids(items: &[PlaylistItem]) -> Vec<&str> {
        items.iter().map(|i| i.uuid.as_str()).collect()
    }

    #[test]
    fn text_order_is_numeric_and_ignores_case_and_accents() {
        assert_eq!(compare_text("Track 2", "Track 10"), Ordering::Less);
        assert_eq!(compare_text("élan", "Elan"), Ordering::Equal);
        assert_eq!(compare_text("apple", "Banana"), Ordering::Less);
        assert_eq!(compare_text("a", "ab"), Ordering::Less);
        assert_eq!(compare_text("007", "7"), Ordering::Equal);
    }

    #[test]
    fn missing_values_and_songless_items_sort_last_in_both_directions() {
        for asc in [true, false] {
            let mut items = vec![
                titled("none", None),
                item("no-song", None),
                titled("b", Some("B")),
                titled("a", Some("a")),
            ];
            apply_sort(&mut items, Some("title"), asc, &HashMap::new());
            let expected = if asc {
                ["a", "b", "none", "no-song"]
            } else {
                ["b", "a", "none", "no-song"]
            };
            assert_eq!(uuids(&items), expected);
        }
    }

    #[test]
    fn equal_values_keep_manual_order() {
        let mut items = vec![
            titled("1", Some("Same")),
            titled("2", Some("Other")),
            titled("3", Some("same")),
        ];
        apply_sort(&mut items, Some("title"), true, &HashMap::new());
        assert_eq!(uuids(&items), ["2", "1", "3"]);
    }

    #[test]
    fn sort_names_override_the_displayed_value() {
        let mut items = vec![
            item(
                "beatles",
                Some(Song {
                    artist: Some("The Beatles".into()),
                    artistsort: Some("Beatles, The".into()),
                    ..Default::default()
                }),
            ),
            item(
                "abba",
                Some(Song {
                    album_artist: Some("ABBA".into()),
                    artist: Some("Zzz".into()),
                    ..Default::default()
                }),
            ),
            item(
                "blank-override",
                Some(Song {
                    artist: Some("Aardvark".into()),
                    artistsort: Some("  ".into()),
                    ..Default::default()
                }),
            ),
        ];
        apply_sort(&mut items, Some("artist"), true, &HashMap::new());
        assert_eq!(uuids(&items), ["blank-override", "abba", "beatles"]);
    }

    #[test]
    fn numbers_and_artist_tags_sort() {
        let sized = |uuid: &str, bytes: Option<i64>, artist: &str| {
            item(
                uuid,
                Some(Song {
                    filesize: bytes,
                    artist: Some(artist.into()),
                    ..Default::default()
                }),
            )
        };
        let mut items = vec![
            sized("big", Some(900), "Opeth"),
            sized("small", Some(5), "Abba"),
            sized("unknown", None, "Nobody"),
        ];
        apply_sort(&mut items, Some("filesize"), false, &HashMap::new());
        assert_eq!(uuids(&items), ["big", "small", "unknown"]);

        let tags = HashMap::from([
            ("opeth".to_string(), "Prog".to_string()),
            ("abba".to_string(), "Pop".to_string()),
        ]);
        apply_sort(&mut items, Some("artist_tag"), true, &tags);
        assert_eq!(uuids(&items), ["small", "big", "unknown"]);
    }

    #[test]
    fn manual_order_descending_reverses() {
        let mut items = vec![titled("1", None), titled("2", None)];
        apply_sort(&mut items, None, false, &HashMap::new());
        assert_eq!(uuids(&items), ["2", "1"]);
    }
}
