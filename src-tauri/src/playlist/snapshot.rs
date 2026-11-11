//! What a playlist item remembers about a track that isn't in the library: a
//! song that was deleted (written by the `songs_bd_playlist_items` trigger,
//! see `MIGRATION_82` in db.rs) or an imported track that matched nothing
//! (written by `import_playlist`). Stored as keys merged into
//! `playlist_items.additional_metadata`, alongside unrelated keys such as the
//! Queue's `autoContinue` marker. This module is the one Rust owner of those
//! keys; the SQL triggers name them too, so keep both in step.

use crate::models::Song;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The snapshot keys, removed again when the item is relinked.
const KEYS: &[&str] = &[
    "title",
    "artist",
    "album",
    "path",
    "duration_sec",
    "beginning_nanosec",
];

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(super) struct TrackSnapshot {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub path: Option<String>,
    pub duration_sec: Option<i64>,
    /// Start of a CUE track within its audio file; 0 (and omitted) otherwise.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub beginning_nanosec: i64,
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

impl TrackSnapshot {
    /// The snapshot in an item's `additional_metadata`, or `None` when that
    /// isn't a JSON object. Absent keys are `None`; other keys are ignored.
    pub fn parse(additional_metadata: &str) -> Option<Self> {
        serde_json::from_str::<Value>(additional_metadata)
            .ok()
            .filter(Value::is_object)
            .and_then(|v| serde_json::from_value(v).ok())
    }

    /// The `additional_metadata` JSON for a new item holding only this snapshot.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// The song the playlist view shows for the item: the remembered fields,
    /// flagged unavailable, with id 0 (no library song).
    pub fn to_song(&self) -> Song {
        Song {
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            path: self.path.clone(),
            length_nanosec: self.duration_sec.map(|s| s * 1_000_000_000),
            beginning_nanosec: self.beginning_nanosec,
            unavailable: true,
            ..Default::default()
        }
    }
}

/// `additional_metadata` with the snapshot keys removed, for an item that is
/// linked to a library song again; `None` when nothing else is left.
pub(super) fn strip_snapshot(additional_metadata: &str) -> Option<String> {
    let mut map: Map<String, Value> = serde_json::from_str(additional_metadata).ok()?;
    for key in KEYS {
        map.remove(*key);
    }
    (!map.is_empty()).then(|| Value::Object(map).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_known_keys_and_ignores_others() {
        let snap = TrackSnapshot::parse(
            r#"{"title":"T","artist":null,"duration_sec":185,"beginning_nanosec":5,"autoContinue":true}"#,
        )
        .unwrap();
        assert_eq!(snap.title.as_deref(), Some("T"));
        assert_eq!(snap.artist, None);
        assert_eq!(snap.duration_sec, Some(185));
        assert_eq!(snap.beginning_nanosec, 5);
        assert!(TrackSnapshot::parse("not json").is_none());
        assert!(TrackSnapshot::parse("[1]").is_none());
    }

    #[test]
    fn to_json_omits_a_zero_offset_and_round_trips() {
        let snap = TrackSnapshot {
            title: Some("T".into()),
            duration_sec: Some(3),
            ..Default::default()
        };
        let json = snap.to_json();
        assert!(!json.contains("beginning_nanosec"));
        let song = TrackSnapshot::parse(&json).unwrap().to_song();
        assert_eq!(song.title.as_deref(), Some("T"));
        assert_eq!(song.length_nanosec, Some(3_000_000_000));
        assert!(song.unavailable);
        assert_eq!(song.id, 0);
    }

    #[test]
    fn strip_snapshot_keeps_other_keys() {
        assert_eq!(
            strip_snapshot(r#"{"title":"T","path":"/a","autoContinue":true}"#).as_deref(),
            Some(r#"{"autoContinue":true}"#)
        );
        assert_eq!(strip_snapshot(r#"{"title":"T"}"#), None);
        assert_eq!(strip_snapshot("not json"), None);
    }
}
