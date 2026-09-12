//! CUE sheet parsing (#78) — classic single-media-file case: one physical
//! audio file (typically FLAC or APE) split into multiple tracks by a
//! sidecar `.cue` sheet's `TRACK`/`INDEX 01` entries.
//!
//! A CUE sheet with more than one `FILE` line (one physical file per track,
//! rather than one file for the whole album) is a different, much more
//! common shape in the wild — real rips just tagged per-file rather than a
//! true single-file-plus-cue rip — and is intentionally out of scope here;
//! [`parse_single_file_cue`] returns `None` for it so the scanner falls back
//! to treating those files as ordinary per-file tracks.

/// One track parsed from a CUE sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct CueTrack {
    pub number: i32,
    pub title: Option<String>,
    pub performer: Option<String>,
    /// Offset into the referenced media file, from `INDEX 01`.
    pub start_nanosec: i64,
}

/// A parsed classic (single-`FILE`) CUE sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct CueSheet {
    pub album_title: Option<String>,
    pub album_performer: Option<String>,
    /// The media filename exactly as written in the sheet's `FILE` line
    /// (relative to the CUE sheet's own directory).
    pub media_file: String,
    /// In file order, ascending by `start_nanosec`.
    pub tracks: Vec<CueTrack>,
}

enum ParseState {
    Header,
    Track(CueTrack),
    /// A `TRACK` entry whose type isn't `AUDIO` (e.g. a data track on a
    /// mixed-mode disc) — its TITLE/PERFORMER/INDEX lines are discarded
    /// rather than misattributed to the album header or an adjacent track.
    SkippedTrack,
}

/// Parses raw CUE sheet text. Returns `None` when the sheet isn't the
/// classic single-`FILE` case, or has no usable `AUDIO` tracks.
pub fn parse_single_file_cue(text: &str) -> Option<CueSheet> {
    let mut album_title: Option<String> = None;
    let mut album_performer: Option<String> = None;
    let mut media_file: Option<String> = None;
    let mut file_count = 0u32;
    let mut tracks: Vec<CueTrack> = Vec::new();
    let mut state = ParseState::Header;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let (cmd, rest) = split_command(line);

        match cmd.to_ascii_uppercase().as_str() {
            "FILE" => {
                file_count += 1;
                if file_count > 1 {
                    // Multi-file CUE sheet — not the classic case.
                    return None;
                }
                media_file = parse_quoted_or_bare(rest);
            }
            "TRACK" => {
                if let ParseState::Track(t) = std::mem::replace(&mut state, ParseState::Header) {
                    tracks.push(t);
                }
                let mut parts = rest.split_whitespace();
                let number = parts
                    .next()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or((tracks.len() + 1) as i32);
                let is_audio = parts
                    .next()
                    .map(|t| t.eq_ignore_ascii_case("AUDIO"))
                    .unwrap_or(true);
                state = if is_audio {
                    ParseState::Track(CueTrack {
                        number,
                        title: None,
                        performer: None,
                        start_nanosec: 0,
                    })
                } else {
                    ParseState::SkippedTrack
                };
            }
            "TITLE" => {
                let title = parse_quoted_or_bare(rest);
                match &mut state {
                    ParseState::Track(t) => t.title = title,
                    ParseState::Header => album_title = title,
                    ParseState::SkippedTrack => {}
                }
            }
            "PERFORMER" => {
                let performer = parse_quoted_or_bare(rest);
                match &mut state {
                    ParseState::Track(t) => t.performer = performer,
                    ParseState::Header => album_performer = performer,
                    ParseState::SkippedTrack => {}
                }
            }
            "INDEX" => {
                if let ParseState::Track(t) = &mut state {
                    let mut parts = rest.split_whitespace();
                    let index_num = parts.next();
                    let time_str = parts.next();
                    if index_num == Some("01") {
                        if let Some(ns) = time_str.and_then(parse_cue_time) {
                            t.start_nanosec = ns;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if let ParseState::Track(t) = state {
        tracks.push(t);
    }

    let media_file = media_file?;
    if tracks.is_empty() {
        return None;
    }

    Some(CueSheet {
        album_title,
        album_performer,
        media_file,
        tracks,
    })
}

/// Splits a CUE command line into its uppercase keyword and the remainder.
fn split_command(line: &str) -> (&str, &str) {
    match line.find(char::is_whitespace) {
        Some(idx) => (&line[..idx], line[idx..].trim_start()),
        None => (line, ""),
    }
}

/// A CUE value is normally `"quoted, possibly with spaces"`, but tolerate a
/// bare unquoted token too since not every sheet in the wild is strict.
fn parse_quoted_or_bare(rest: &str) -> Option<String> {
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    if let Some(inner) = rest.strip_prefix('"') {
        return inner.rfind('"').map(|end| inner[..end].to_string());
    }
    Some(rest.to_string())
}

/// `MM:SS:FF` (frames, 75/sec — the CD-DA standard) to nanoseconds.
fn parse_cue_time(s: &str) -> Option<i64> {
    let mut parts = s.splitn(3, ':');
    let mm: i64 = parts.next()?.parse().ok()?;
    let ss: i64 = parts.next()?.parse().ok()?;
    let ff: i64 = parts.next()?.parse().ok()?;
    let total_frames = mm * 60 * 75 + ss * 75 + ff;
    Some(total_frames * 1_000_000_000 / 75)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSIC: &str = r#"
REM GENRE Rock
REM DATE 1989
PERFORMER "Joe Satriani"
TITLE "Flying In A Blue Dream"
FILE "Flying In A Blue Dream.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Flying In A Blue Dream"
    PERFORMER "Joe Satriani"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "The Mystical Potato Head Groove Thing"
    PERFORMER "Joe Satriani"
    INDEX 00 03:58:50
    INDEX 01 04:00:50
  TRACK 03 AUDIO
    TITLE "Can't Slow Down"
    PERFORMER "Joe Satriani"
    INDEX 01 07:32:00
"#;

    #[test]
    fn parses_classic_single_file_sheet() {
        let sheet = parse_single_file_cue(CLASSIC).expect("should parse");
        assert_eq!(sheet.media_file, "Flying In A Blue Dream.flac");
        assert_eq!(sheet.album_title.as_deref(), Some("Flying In A Blue Dream"));
        assert_eq!(sheet.album_performer.as_deref(), Some("Joe Satriani"));
        assert_eq!(sheet.tracks.len(), 3);

        assert_eq!(sheet.tracks[0].number, 1);
        assert_eq!(sheet.tracks[0].start_nanosec, 0);
        assert_eq!(
            sheet.tracks[0].title.as_deref(),
            Some("Flying In A Blue Dream")
        );

        // INDEX 00 (pre-gap) must be ignored in favor of INDEX 01.
        assert_eq!(sheet.tracks[1].start_nanosec, parse_cue_time("04:00:50").unwrap());

        assert_eq!(
            sheet.tracks[2].start_nanosec,
            parse_cue_time("07:32:00").unwrap()
        );
    }

    #[test]
    fn rejects_multi_file_sheet() {
        let text = r#"
FILE "01 Track One.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE "02 Track Two.flac" WAVE
  TRACK 02 AUDIO
    INDEX 01 00:00:00
"#;
        assert!(parse_single_file_cue(text).is_none());
    }

    #[test]
    fn rejects_sheet_with_no_tracks() {
        let text = "FILE \"album.flac\" WAVE\n";
        assert!(parse_single_file_cue(text).is_none());
    }

    #[test]
    fn skips_non_audio_tracks() {
        let text = r#"
FILE "album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Real Track"
    INDEX 01 00:00:00
  TRACK 02 MODE1/2352
    TITLE "Data Track"
    INDEX 01 05:00:00
"#;
        let sheet = parse_single_file_cue(text).expect("should parse");
        assert_eq!(sheet.tracks.len(), 1);
        assert_eq!(sheet.tracks[0].title.as_deref(), Some("Real Track"));
    }

    #[test]
    fn parses_cue_time_with_frames() {
        // 1 second = 75 frames = 1_000_000_000 ns.
        assert_eq!(parse_cue_time("00:01:00"), Some(1_000_000_000));
        assert_eq!(parse_cue_time("01:00:00"), Some(60_000_000_000));
        assert_eq!(parse_cue_time("00:00:75"), Some(1_000_000_000));
    }
}
