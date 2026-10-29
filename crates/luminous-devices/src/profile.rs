//! What a user keeps about each device.

use crate::backend::components;
use crate::DeviceBackend;
use serde::{Deserialize, Serialize};

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
    /// Lower-case codec names the device plays: `mp3`, `aac`, `alac`,
    /// `flac`, `ogg` (Vorbis), `opus`, `wav`, `aiff`, `wma`. By codec, not
    /// container: `aac` never implies ALAC in an `.m4a`.
    pub supported_formats: Vec<String>,
    pub max_sample_rate: u32,
    pub max_bit_depth: u8,
    /// What sync converts on the way to the device (#838).
    pub convert: ConvertSettings,
    /// How synced files and playlists are laid out on the device (#837).
    pub layout: LayoutSettings,
}

/// The template a new device's music folders follow: album artist, then
/// year and album, then disc, track and title, e.g.
/// `Insomnium/2019 - Heart Like a Grave/02 Valediction`.
pub const DEFAULT_FOLDER_TEMPLATE: &str = "%albumartist/{%year - }{%album/}{%disc-}{%track }%title";

/// Where synced files go on the device and how playlist files are written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LayoutSettings {
    /// Path of each track under `music_path`, in the Organize view's
    /// template syntax (`%albumartist`, `{%year - }`, ...). The extension
    /// is added: the one the file has on the device.
    pub folder_template: String,
    pub playlist_format: PlaylistFileFormat,
    /// Separator inside playlist files. Backslash playlists also end lines
    /// with CRLF, as the DOS-era firmware that wants one expects the other.
    pub path_separator: PathSeparator,
    /// Album art written beside the tracks of each album, for players that
    /// don't read embedded art.
    pub folder_art: FolderArt,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            folder_template: DEFAULT_FOLDER_TEMPLATE.into(),
            playlist_format: PlaylistFileFormat::M3u8,
            path_separator: PathSeparator::Slash,
            folder_art: FolderArt::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaylistFileFormat {
    /// `.m3u8`, UTF-8 without a BOM.
    #[default]
    M3u8,
    /// `.m3u` in plain ASCII, for players that fail on UTF-8. Such firmware
    /// reads its own code page (Windows-1252, Shift-JIS, GBK...), and only
    /// ASCII reads the same in all of them. File and folder names on the
    /// device are folded to ASCII too, so every line still points at its
    /// file.
    M3uAscii,
    /// `.pls`, UTF-8.
    Pls,
    /// No playlist files.
    None,
}

impl PlaylistFileFormat {
    /// Whether file and folder names on the device must be ASCII so this
    /// format's playlist lines can spell them.
    pub fn ascii_names(self) -> bool {
        self == Self::M3uAscii
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathSeparator {
    #[default]
    Slash,
    Backslash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FolderArt {
    #[default]
    None,
    /// `cover.jpg`
    Cover,
    /// `folder.jpg`
    Folder,
    /// Both `cover.jpg` and `folder.jpg`.
    Both,
}

impl FolderArt {
    /// The art file names to write in each album folder.
    pub fn file_names(self) -> &'static [&'static str] {
        match self {
            Self::None => &[],
            Self::Cover => &["cover.jpg"],
            Self::Folder => &["folder.jpg"],
            Self::Both => &["cover.jpg", "folder.jpg"],
        }
    }
}

/// Which files sync converts, and to what. The format and bitrate are a
/// request: luminous-transcode resolves them against the encoders this
/// computer has, so a profile written on another machine never fails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConvertSettings {
    pub mode: ConvertMode,
    pub format: TargetFormat,
    pub bitrate_kbps: u32,
    /// Longest side, in pixels, of embedded cover art on the device. Larger
    /// art is downscaled, also in files that are otherwise copied as-is.
    /// `None` keeps art as it is.
    pub max_art_px: Option<u32>,
}

impl Default for ConvertSettings {
    fn default() -> Self {
        Self {
            mode: ConvertMode::Unsupported,
            format: TargetFormat::Aac,
            bitrate_kbps: 192,
            max_art_px: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConvertMode {
    /// Convert files the device can't play: a format missing from
    /// `supported_formats`, or a sample rate or bit depth above its maximum.
    #[default]
    Unsupported,
    /// Also convert playable files that would come out smaller: lossless
    /// files and lossy ones above the target bitrate. Lossy files at or
    /// below it are copied, since converting them would cost quality and
    /// save nothing.
    SaveSpace,
    /// Copy every file as it is.
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetFormat {
    Aac,
    Mp3,
    Opus,
}

impl TargetFormat {
    /// The name `supported_formats` uses for this format.
    pub fn format_name(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
        }
    }

    /// Extension of a file in this format, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Aac => "m4a",
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
        }
    }
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
            convert: ConvertSettings::default(),
            layout: LayoutSettings::default(),
        }
    }
}

impl DeviceProfile {
    /// The profile a device starts with: the defaults, with music (and
    /// playlists) in a top-level `Music` folder if `storage` has one, and
    /// the formats the device reports it plays when it reports any.
    pub(crate) fn defaults_for(storage: &dyn DeviceBackend, reported_formats: &[String]) -> Self {
        let music_dir = storage.list("").ok().and_then(|entries| {
            entries
                .into_iter()
                .find(|e| e.is_dir && e.name.eq_ignore_ascii_case("music"))
                .map(|e| e.name)
        });
        let music_path = music_dir.unwrap_or_default();
        let defaults = Self::default();
        Self {
            playlists_path: music_path.clone(),
            music_path,
            supported_formats: if reported_formats.is_empty() {
                defaults.supported_formats.clone()
            } else {
                reported_formats.to_vec()
            },
            ..defaults
        }
    }

    /// The same profile with paths that cannot leave the device (no `..`,
    /// no drive or root prefix, `/` separators) and blank text fields as
    /// `None`. Saving always stores the normalized form.
    pub fn normalized(mut self) -> Self {
        self.music_path = relative_path(&self.music_path);
        self.playlists_path = relative_path(&self.playlists_path);
        let template = self.layout.folder_template.trim();
        self.layout.folder_template = if template.is_empty() {
            DEFAULT_FOLDER_TEMPLATE.into()
        } else {
            template.into()
        };
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
    components(path).join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FsBackend;

    #[test]
    fn defaults_for_uses_an_existing_music_folder() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FsBackend::new(dir.path());
        assert_eq!(DeviceProfile::defaults_for(&storage, &[]).music_path, "");

        std::fs::create_dir(dir.path().join("MUSIC")).unwrap();
        let profile = DeviceProfile::defaults_for(&storage, &[]);
        assert_eq!(profile.music_path, "MUSIC");
        assert_eq!(profile.playlists_path, "MUSIC");
        assert_eq!(profile.reserve_bytes, DEFAULT_RESERVE_BYTES);
        assert_eq!(profile.supported_formats, vec!["mp3", "aac"]);
    }

    #[test]
    fn defaults_for_takes_the_formats_a_device_reports() {
        let dir = tempfile::tempdir().unwrap();
        let reported = vec!["mp3".to_string(), "flac".to_string()];
        let profile = DeviceProfile::defaults_for(&FsBackend::new(dir.path()), &reported);
        assert_eq!(profile.supported_formats, reported);
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
        assert_eq!(profile.convert, ConvertSettings::default());
    }

    #[test]
    fn layout_settings_fill_missing_fields_and_a_blank_template() {
        let profile: DeviceProfile = serde_json::from_str(
            r#"{"layout":{"playlistFormat":"m3uAscii","folderTemplate":"  "}}"#,
        )
        .unwrap();
        assert_eq!(profile.layout.playlist_format, PlaylistFileFormat::M3uAscii);
        assert_eq!(profile.layout.path_separator, PathSeparator::Slash);
        assert_eq!(
            profile.normalized().layout.folder_template,
            DEFAULT_FOLDER_TEMPLATE
        );
    }

    #[test]
    fn convert_settings_fill_missing_fields() {
        let profile: DeviceProfile =
            serde_json::from_str(r#"{"convert":{"mode":"saveSpace","format":"opus"}}"#).unwrap();
        assert_eq!(profile.convert.mode, ConvertMode::SaveSpace);
        assert_eq!(profile.convert.format, TargetFormat::Opus);
        assert_eq!(profile.convert.bitrate_kbps, 192);
    }
}
