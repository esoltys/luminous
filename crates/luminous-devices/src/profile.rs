//! What a user keeps about each device.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Space kept free for the device's own use (databases, settings), shown in
/// the UI as "Kept free for the device".
pub const DEFAULT_RESERVE_BYTES: u64 = 2_000_000_000;

/// A user's settings for one device. Every field has a usable value, so
/// callers never handle "not configured". `nickname`, `icon` and `color`
/// are `None` until the user picks them, like a music folder's or server's
/// in Settings > Sources; the UI then shows the device's default name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceProfile {
    pub nickname: Option<String>,
    /// Badge icon id, as for sources.
    pub icon: Option<String>,
    /// `#rrggbb`, as for sources.
    pub color: Option<String>,
    /// Folder music is synced into, relative to the volume root with `/`
    /// separators. Empty means the root.
    pub music_path: String,
    /// Folder playlist files are written to, relative like `music_path`.
    pub playlists_path: String,
    /// Bytes sync must leave free on the device.
    pub reserve_bytes: u64,
    /// Lower-case format names the device plays (`mp3`, `aac`, `flac`...).
    pub supported_formats: Vec<String>,
    pub max_sample_rate: u32,
    pub max_bit_depth: u8,
}

impl Default for DeviceProfile {
    /// A conservative player: MP3 and AAC up to 48 kHz / 16-bit, music at
    /// the root.
    fn default() -> Self {
        Self {
            nickname: None,
            icon: None,
            color: None,
            music_path: String::new(),
            playlists_path: String::new(),
            reserve_bytes: DEFAULT_RESERVE_BYTES,
            supported_formats: vec!["mp3".into(), "aac".into()],
            max_sample_rate: 48_000,
            max_bit_depth: 16,
        }
    }
}

impl DeviceProfile {
    /// The profile a device starts with: the defaults, with music (and
    /// playlists) in a top-level `Music` folder if the device has one.
    pub fn defaults_for(root: &Path) -> Self {
        let music_dir = std::fs::read_dir(root).ok().and_then(|entries| {
            entries.flatten().find_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                (name.eq_ignore_ascii_case("music") && e.path().is_dir()).then_some(name)
            })
        });
        let music_path = music_dir.unwrap_or_default();
        Self {
            playlists_path: music_path.clone(),
            music_path,
            ..Self::default()
        }
    }

    /// The same profile with paths that cannot leave the device (no `..`,
    /// no drive or root prefix, `/` separators) and blank text fields as
    /// `None`. Saving always stores the normalized form.
    pub fn normalized(mut self) -> Self {
        self.music_path = relative_path(&self.music_path);
        self.playlists_path = relative_path(&self.playlists_path);
        for field in [&mut self.nickname, &mut self.icon, &mut self.color] {
            *field = field
                .take()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        let mut seen = std::collections::HashSet::new();
        self.supported_formats = self
            .supported_formats
            .iter()
            .map(|f| f.trim().to_ascii_lowercase())
            .filter(|f| !f.is_empty() && seen.insert(f.clone()))
            .collect();
        self
    }
}

fn relative_path(path: &str) -> String {
    path.split(['/', '\\'])
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != "." && *part != ".." && !part.ends_with(':'))
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_for_uses_an_existing_music_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(DeviceProfile::defaults_for(dir.path()).music_path, "");

        std::fs::create_dir(dir.path().join("MUSIC")).unwrap();
        let profile = DeviceProfile::defaults_for(dir.path());
        assert_eq!(profile.music_path, "MUSIC");
        assert_eq!(profile.playlists_path, "MUSIC");
        assert_eq!(profile.reserve_bytes, DEFAULT_RESERVE_BYTES);
    }

    #[test]
    fn normalized_keeps_paths_on_the_device() {
        let profile = DeviceProfile {
            nickname: Some("  ".into()),
            color: Some(" #3b82f6 ".into()),
            music_path: "E:\\..\\Music\\./Rock/".into(),
            playlists_path: "/../../etc".into(),
            supported_formats: vec![
                "MP3".into(),
                " ".into(),
                "flac".into(),
                "mp3".into(),
                "FLAC".into(),
            ],
            ..DeviceProfile::default()
        }
        .normalized();
        assert_eq!(profile.nickname, None);
        assert_eq!(profile.color.as_deref(), Some("#3b82f6"));
        assert_eq!(profile.music_path, "Music/Rock");
        assert_eq!(profile.playlists_path, "etc");
        assert_eq!(profile.supported_formats, vec!["mp3", "flac"]);
    }

    #[test]
    fn missing_fields_deserialize_to_defaults() {
        let profile: DeviceProfile = serde_json::from_str(r#"{"nickname":"R1"}"#).unwrap();
        assert_eq!(profile.nickname.as_deref(), Some("R1"));
        assert_eq!(profile.max_sample_rate, 48_000);
    }
}
