//! Parsers and exporters for M3U, M3U8, PLS, and XSPF playlist formats.

use anyhow::{anyhow, Result};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct ParsedTrack {
    pub path_or_url: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_sec: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistFormat {
    M3u,
    M3u8,
    Pls,
    Xspf,
}

impl PlaylistFormat {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Option<Self> {
        let ext = path.as_ref().extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "m3u" => Some(PlaylistFormat::M3u),
            "m3u8" => Some(PlaylistFormat::M3u8),
            "pls" => Some(PlaylistFormat::Pls),
            "xspf" => Some(PlaylistFormat::Xspf),
            _ => None,
        }
    }
}

pub struct ParsedPlaylist {
    pub title: Option<String>,
    pub tracks: Vec<ParsedTrack>,
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                decoded.push(val);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&decoded).to_string()
}

pub fn clean_track_path(raw: &str) -> String {
    let unquoted = raw
        .trim()
        .trim_matches('\0')
        .trim_matches('"')
        .trim_matches('\'')
        .trim();
    let mut s = unquoted;
    if let Some(stripped) = s.strip_prefix("file:///") {
        s = stripped;
    } else if let Some(stripped) = s.strip_prefix("file://") {
        s = stripped;
    }
    percent_decode(s)
}

pub fn parse_playlist<P: AsRef<Path>>(file_path: P) -> Result<ParsedPlaylist> {
    let path = file_path.as_ref();
    let format = PlaylistFormat::from_path(path)
        .ok_or_else(|| anyhow!("Unsupported playlist format for file: {}", path.display()))?;

    let bytes = std::fs::read(path)
        .map_err(|e| anyhow!("Failed to read playlist file '{}': {}", path.display(), e))?;

    let raw_content = if bytes.starts_with(&[0xFF, 0xFE]) {
        // UTF-16LE with BOM
        let u16_units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&u16_units)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        // UTF-16BE with BOM
        let u16_units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        String::from_utf16_lossy(&u16_units)
    } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        // UTF-8 with BOM
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else if bytes.len() >= 4 && bytes.iter().step_by(2).skip(1).all(|&b| b == 0) {
        // UTF-16LE without BOM
        let u16_units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&u16_units)
    } else {
        // Standard UTF-8 / ANSI / ISO-8859-1
        String::from_utf8_lossy(&bytes).into_owned()
    };

    let content = raw_content.strip_prefix('\u{feff}').unwrap_or(&raw_content);

    match format {
        PlaylistFormat::M3u | PlaylistFormat::M3u8 => Ok(ParsedPlaylist {
            title: None,
            tracks: parse_m3u(content),
        }),
        PlaylistFormat::Pls => Ok(ParsedPlaylist {
            title: None,
            tracks: parse_pls(content),
        }),
        PlaylistFormat::Xspf => parse_xspf(content),
    }
}

/// Parses M3U / M3U8 content.
pub fn parse_m3u(content: &str) -> Vec<ParsedTrack> {
    let mut tracks = Vec::new();
    let mut current_title: Option<String> = None;
    let mut current_artist: Option<String> = None;
    let mut current_duration: Option<i64> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(meta) = trimmed.strip_prefix("#EXTINF:") {
            if let Some((dur_str, rest)) = meta.split_once(',') {
                if let Ok(dur) = dur_str.trim().parse::<f64>() {
                    if dur > 0.0 {
                        current_duration = Some(dur.round() as i64);
                    }
                }
                let name = rest.trim();
                if let Some((artist, title)) = name.split_once(" - ") {
                    current_artist = Some(artist.trim().to_string());
                    current_title = Some(title.trim().to_string());
                } else if !name.is_empty() {
                    current_title = Some(name.to_string());
                }
            }
            continue;
        }

        if trimmed.starts_with('#') {
            continue;
        }

        let cleaned = clean_track_path(trimmed);
        if !cleaned.is_empty() {
            tracks.push(ParsedTrack {
                path_or_url: cleaned,
                title: current_title.take(),
                artist: current_artist.take(),
                album: None,
                duration_sec: current_duration.take(),
            });
        }
    }

    tracks
}

/// Parses PLS (INI-style) content.
pub fn parse_pls(content: &str) -> Vec<ParsedTrack> {
    use std::collections::HashMap;

    let mut entries: HashMap<usize, ParsedTrack> = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with(';')
            || trimmed.starts_with('#')
            || trimmed.starts_with('[')
        {
            continue;
        }

        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim();
            let val = val.trim();

            if let Some(idx_str) = key.strip_prefix("File") {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    let cleaned = clean_track_path(val);
                    if !cleaned.is_empty() {
                        let track = entries.entry(idx).or_default();
                        track.path_or_url = cleaned;
                    }
                }
            } else if let Some(idx_str) = key.strip_prefix("Title") {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    let track = entries.entry(idx).or_default();
                    if let Some((artist, title)) = val.split_once(" - ") {
                        track.artist = Some(artist.trim().to_string());
                        track.title = Some(title.trim().to_string());
                    } else if !val.is_empty() {
                        track.title = Some(val.to_string());
                    }
                }
            } else if let Some(idx_str) = key.strip_prefix("Length") {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let Ok(dur) = val.parse::<i64>() {
                        if dur > 0 {
                            let track = entries.entry(idx).or_default();
                            track.duration_sec = Some(dur);
                        }
                    }
                }
            }
        }
    }

    let mut sorted_keys: Vec<usize> = entries.keys().copied().collect();
    sorted_keys.sort_unstable();

    sorted_keys
        .into_iter()
        .filter_map(|k| {
            let track = entries.remove(&k)?;
            if !track.path_or_url.is_empty() {
                Some(track)
            } else {
                None
            }
        })
        .collect()
}

/// Helper function to unescape basic XML entities (&amp;, &lt;, &gt;, &quot;, &apos;).
fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Helper function to escape basic XML entities for export.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Parses XSPF (XML shareable playlist) content.
pub fn parse_xspf(content: &str) -> Result<ParsedPlaylist> {
    let mut playlist_title: Option<String> = None;
    let mut tracks = Vec::new();

    // Check for main playlist title before trackList
    if let Some(tl_idx) = content.find("<trackList>") {
        let head = &content[..tl_idx];
        if let Some(title) = extract_xml_tag(head, "title") {
            playlist_title = Some(unescape_xml(&title));
        }
    }

    let mut rest = content;
    while let Some(start_idx) = rest.find("<track>") {
        rest = &rest[start_idx + "<track>".len()..];
        let end_idx = match rest.find("</track>") {
            Some(idx) => idx,
            None => break,
        };
        let track_block = &rest[..end_idx];
        rest = &rest[end_idx + "</track>".len()..];

        let location = extract_xml_tag(track_block, "location").map(|s| unescape_xml(&s));
        let title = extract_xml_tag(track_block, "title").map(|s| unescape_xml(&s));
        let creator = extract_xml_tag(track_block, "creator").map(|s| unescape_xml(&s));
        let album = extract_xml_tag(track_block, "album").map(|s| unescape_xml(&s));
        let duration_ms =
            extract_xml_tag(track_block, "duration").and_then(|d| d.trim().parse::<i64>().ok());

        if let Some(loc) = location {
            // Unescape file:// URI if present
            let path_or_url = if let Some(stripped) = loc.strip_prefix("file://") {
                let unencoded = percent_encoding::percent_decode_str(stripped)
                    .decode_utf8_lossy()
                    .to_string();
                #[cfg(windows)]
                {
                    // strip leading slash if windows path like /C:/...
                    if unencoded.starts_with('/') && unencoded.as_bytes().get(2) == Some(&b':') {
                        unencoded[1..].to_string()
                    } else {
                        unencoded
                    }
                }
                #[cfg(not(windows))]
                {
                    unencoded
                }
            } else {
                loc
            };

            tracks.push(ParsedTrack {
                path_or_url,
                title,
                artist: creator,
                album,
                duration_sec: duration_ms.map(|ms| ms / 1000),
            });
        }
    }

    Ok(ParsedPlaylist {
        title: playlist_title,
        tracks,
    })
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    let start = xml.find(&open_tag)?;
    let content_start = start + open_tag.len();
    let end = xml[content_start..].find(&close_tag)?;

    Some(xml[content_start..content_start + end].trim().to_string())
}

#[derive(Debug, Clone)]
pub struct ExportTrack<'a> {
    pub path: &'a Path,
    pub title: Option<&'a str>,
    pub artist: Option<&'a str>,
    pub album: Option<&'a str>,
    pub duration_sec: Option<i64>,
}

/// One track in a playlist being written: `location` is written exactly as
/// given, so the caller decides whether it is absolute or relative and which
/// separator it uses.
#[derive(Debug, Clone)]
pub struct PlaylistEntry<'a> {
    pub location: String,
    pub title: Option<&'a str>,
    pub artist: Option<&'a str>,
    pub album: Option<&'a str>,
    pub duration_sec: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    /// For players whose firmware expects DOS text files.
    CrLf,
}

impl LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

/// Formats a playlist into string output for saving to disk.
pub fn export_playlist(
    playlist_name: &str,
    tracks: &[ExportTrack],
    format: PlaylistFormat,
    export_path: &Path,
    relative: bool,
) -> Result<String> {
    let base_dir = export_path.parent();

    let format_path = |track_path: &Path| -> String {
        if relative {
            if let Some(base) = base_dir {
                if let Ok(rel) = pathdiff::diff_paths(track_path, base) {
                    return rel.to_string_lossy().replace('\\', "/");
                }
            }
        }
        track_path.to_string_lossy().to_string()
    };

    let entries: Vec<PlaylistEntry> = tracks
        .iter()
        .map(|track| PlaylistEntry {
            location: format_path(track.path),
            title: track.title,
            artist: track.artist,
            album: track.album,
            duration_sec: track.duration_sec,
        })
        .collect();
    Ok(render_playlist(
        playlist_name,
        &entries,
        format,
        LineEnding::Lf,
    ))
}

/// The text of a playlist in `format`, every line ended with `line_ending`.
/// The text is not encoded: M3U8, PLS and XSPF are UTF-8, and a caller
/// writing an `.m3u` in a legacy code page encodes it.
pub fn render_playlist(
    playlist_name: &str,
    entries: &[PlaylistEntry],
    format: PlaylistFormat,
    line_ending: LineEnding,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    match format {
        PlaylistFormat::M3u | PlaylistFormat::M3u8 => {
            lines.push("#EXTM3U".into());
            for entry in entries {
                let dur = entry.duration_sec.unwrap_or(-1);
                let title_display = match (entry.artist, entry.title) {
                    (Some(art), Some(t)) => format!("{} - {}", art, t),
                    (None, Some(t)) => t.to_string(),
                    (Some(art), None) => art.to_string(),
                    (None, None) => Path::new(&entry.location)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default(),
                };
                lines.push(format!("#EXTINF:{},{}", dur, title_display));
                lines.push(entry.location.clone());
            }
        }
        PlaylistFormat::Pls => {
            lines.push("[playlist]".into());
            lines.push(format!("NumberOfEntries={}", entries.len()));
            lines.push(String::new());
            for (idx, entry) in entries.iter().enumerate() {
                let i = idx + 1;
                lines.push(format!("File{}={}", i, entry.location));
                if let Some(title) = entry.title {
                    if let Some(artist) = entry.artist {
                        lines.push(format!("Title{}={} - {}", i, artist, title));
                    } else {
                        lines.push(format!("Title{}={}", i, title));
                    }
                }
                lines.push(format!("Length{}={}", i, entry.duration_sec.unwrap_or(-1)));
                lines.push(String::new());
            }
        }
        PlaylistFormat::Xspf => {
            lines.push("<?xml version=\"1.0\" encoding=\"UTF-8\"?>".into());
            lines.push("<playlist version=\"1\" xmlns=\"http://xspf.org/ns/0/\">".into());
            lines.push(format!("  <title>{}</title>", escape_xml(playlist_name)));
            lines.push("  <trackList>".into());
            for entry in entries {
                lines.push("    <track>".into());
                lines.push(format!(
                    "      <location>{}</location>",
                    escape_xml(&entry.location)
                ));
                if let Some(title) = entry.title {
                    lines.push(format!("      <title>{}</title>", escape_xml(title)));
                }
                if let Some(artist) = entry.artist {
                    lines.push(format!("      <creator>{}</creator>", escape_xml(artist)));
                }
                if let Some(album) = entry.album {
                    lines.push(format!("      <album>{}</album>", escape_xml(album)));
                }
                if let Some(dur) = entry.duration_sec {
                    lines.push(format!("      <duration>{}</duration>", dur * 1000));
                }
                lines.push("    </track>".into());
            }
            lines.push("  </trackList>".into());
            lines.push("</playlist>".into());
        }
    }
    let eol = line_ending.as_str();
    let mut out = lines.join(eol);
    out.push_str(eol);
    out
}

// Module for simple pathdiff functionality without needing pathdiff crate
mod pathdiff {
    use std::path::{Component, Path, PathBuf};

    pub fn diff_paths<P, B>(path: P, base: B) -> Result<PathBuf, ()>
    where
        P: AsRef<Path>,
        B: AsRef<Path>,
    {
        let path = path.as_ref();
        let base = base.as_ref();

        if path.is_absolute() != base.is_absolute() {
            if path.is_absolute() {
                return Ok(PathBuf::from(path));
            } else {
                return Err(());
            }
        }

        let mut ita = path.components();
        let mut itb = base.components();
        let mut comps: Vec<Component> = vec![];

        loop {
            match (ita.next(), itb.next()) {
                (None, None) => break,
                (Some(a), None) => {
                    comps.push(a);
                    comps.extend(ita);
                    break;
                }
                (None, Some(_)) => {
                    comps.push(Component::ParentDir);
                }
                (Some(a), Some(_b)) if comps.is_empty() && a == _b => (),
                (Some(a), Some(_b)) => {
                    comps.push(Component::ParentDir);
                    for _ in itb {
                        comps.push(Component::ParentDir);
                    }
                    comps.push(a);
                    comps.extend(ita);
                    break;
                }
            }
        }

        Ok(comps.iter().map(|c| c.as_os_str()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_m3u_parser() {
        let content = r#"#EXTM3U
#EXTINF:180,Artist One - Song Alpha
/path/to/song1.mp3
#EXTINF:240,Song Beta
relative/song2.flac
"#;
        let tracks = parse_m3u(content);
        assert_eq!(tracks.len(), 2);

        assert_eq!(tracks[0].path_or_url, "/path/to/song1.mp3");
        assert_eq!(tracks[0].artist.as_deref(), Some("Artist One"));
        assert_eq!(tracks[0].title.as_deref(), Some("Song Alpha"));
        assert_eq!(tracks[0].duration_sec, Some(180));

        assert_eq!(tracks[1].path_or_url, "relative/song2.flac");
        assert_eq!(tracks[1].artist, None);
        assert_eq!(tracks[1].title.as_deref(), Some("Song Beta"));
        assert_eq!(tracks[1].duration_sec, Some(240));
    }

    #[test]
    fn test_pls_parser() {
        let content = r#"[playlist]
NumberOfEntries=2

File1=/music/track1.mp3
Title1=Cool Artist - Cool Track
Length1=210

File2=folder/track2.ogg
Title2=Another Song
Length2=150
"#;
        let tracks = parse_pls(content);
        assert_eq!(tracks.len(), 2);

        assert_eq!(tracks[0].path_or_url, "/music/track1.mp3");
        assert_eq!(tracks[0].artist.as_deref(), Some("Cool Artist"));
        assert_eq!(tracks[0].title.as_deref(), Some("Cool Track"));
        assert_eq!(tracks[0].duration_sec, Some(210));

        assert_eq!(tracks[1].path_or_url, "folder/track2.ogg");
        assert_eq!(tracks[1].title.as_deref(), Some("Another Song"));
        assert_eq!(tracks[1].duration_sec, Some(150));
    }

    #[test]
    fn test_xspf_parser() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<playlist version="1" xmlns="http://xspf.org/ns/0/">
  <title>My Favorite XSPF</title>
  <trackList>
    <track>
      <location>/music/track1.mp3</location>
      <title>Track One</title>
      <creator>Creator Name</creator>
      <album>Album Title</album>
      <duration>185000</duration>
    </track>
  </trackList>
</playlist>
"#;
        let parsed = parse_xspf(content).unwrap();
        assert_eq!(parsed.title.as_deref(), Some("My Favorite XSPF"));
        assert_eq!(parsed.tracks.len(), 1);

        let t = &parsed.tracks[0];
        assert_eq!(t.path_or_url, "/music/track1.mp3");
        assert_eq!(t.title.as_deref(), Some("Track One"));
        assert_eq!(t.artist.as_deref(), Some("Creator Name"));
        assert_eq!(t.album.as_deref(), Some("Album Title"));
        assert_eq!(t.duration_sec, Some(185));
    }

    #[test]
    fn test_export_playlist_m3u8() {
        let track1 = ExportTrack {
            path: Path::new("/music/album/song1.mp3"),
            title: Some("Song 1"),
            artist: Some("Artist 1"),
            album: Some("Album 1"),
            duration_sec: Some(200),
        };

        let tracks = vec![track1];
        let out = export_playlist(
            "Test",
            &tracks,
            PlaylistFormat::M3u8,
            Path::new("/music/playlist.m3u8"),
            true,
        )
        .unwrap();

        assert!(out.contains("#EXTM3U"));
        assert!(out.contains("#EXTINF:200,Artist 1 - Song 1"));
        assert!(out.contains("album/song1.mp3"));
    }

    #[test]
    fn render_playlist_writes_locations_as_given_with_the_line_ending() {
        let entries = [PlaylistEntry {
            location: "..\\Music\\A\\01 Song.mp3".into(),
            title: Some("Song"),
            artist: None,
            album: None,
            duration_sec: None,
        }];
        let m3u = render_playlist("Mix", &entries, PlaylistFormat::M3u, LineEnding::CrLf);
        assert_eq!(
            m3u,
            "#EXTM3U\r\n#EXTINF:-1,Song\r\n..\\Music\\A\\01 Song.mp3\r\n"
        );
        let pls = render_playlist("Mix", &entries, PlaylistFormat::Pls, LineEnding::Lf);
        assert_eq!(
            pls,
            "[playlist]\nNumberOfEntries=1\n\nFile1=..\\Music\\A\\01 Song.mp3\nTitle1=Song\nLength1=-1\n\n"
        );
    }
}
