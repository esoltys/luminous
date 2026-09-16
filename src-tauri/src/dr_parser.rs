//! Parser for `foo_dr.txt` dynamic-range log files produced by foobar2000's
//! Dynamic Range Meter plugin (#57). Pure parsing/matching logic only — no
//! filesystem or database access, so it's directly unit-testable; the
//! collection scanner (`collection.rs`) is responsible for finding these
//! files on disk and applying the results to `songs` rows.
//!
//! Example table row this parses:
//! `DR14      -0.80 dB   -17.46 dB      4:16 01-Free Fallin'`
//! Column widths vary with track duration/name length and the peak/RMS unit
//! varies across DR Meter versions ("dB" vs "dBFS"), so parsing anchors on
//! the tokens themselves rather than fixed character positions.

use std::collections::HashSet;

/// One track row parsed from the DR log's table.
#[derive(Debug, Clone, PartialEq)]
pub struct DrTrackEntry {
    /// The `DRnn` rating for this track (e.g. `14` for "DR14").
    pub dr: i32,
    pub peak_db: f64,
    pub rms_db: f64,
    /// Leading track number parsed from the label, when present (e.g. `1`
    /// for "01-Free Fallin'"). `None` for vinyl-style labels ("A1", "B2")
    /// where the prefix isn't a plain integer track number.
    pub track_number: Option<i32>,
    /// The label with any leading track-number prefix stripped, for
    /// title-based matching when track-number matching fails.
    pub title: String,
}

/// A fully parsed `foo_dr.txt` log.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrLog {
    pub tracks: Vec<DrTrackEntry>,
    /// Album-wide DR rating. Prefers the "Weighted" value when present
    /// (a more accurate album-level figure some DR Meter versions add
    /// alongside the plain "Official DR value") over the unweighted one.
    pub album_dr: Option<i32>,
}

/// Parses a full `foo_dr.txt` file's contents.
pub fn parse(content: &str) -> DrLog {
    let mut tracks = Vec::new();
    let mut official_dr = None;
    let mut weighted_dr = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(entry) = parse_track_line(trimmed) {
            tracks.push(entry);
            continue;
        }
        if let Some(v) = extract_dr_value(trimmed, "official dr value") {
            official_dr = Some(v);
        }
        if let Some(v) = extract_dr_value(trimmed, "weighted") {
            weighted_dr = Some(v);
        }
    }

    DrLog {
        tracks,
        album_dr: weighted_dr.or(official_dr),
    }
}

fn parse_track_line(line: &str) -> Option<DrTrackEntry> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let dr_token = parts.next()?;
    let rest = parts.next()?.trim_start();
    if dr_token.len() < 3 || !dr_token.starts_with("DR") {
        return None;
    }
    let dr: i32 = dr_token[2..].parse().ok()?;

    let mut tokens = rest.split_whitespace();
    let peak_str = tokens.next()?;
    let peak_unit = tokens.next()?;
    if !peak_unit.starts_with("dB") {
        return None;
    }
    let peak_db: f64 = peak_str.parse().ok()?;

    let rms_str = tokens.next()?;
    let rms_unit = tokens.next()?;
    if !rms_unit.starts_with("dB") {
        return None;
    }
    let rms_db: f64 = rms_str.parse().ok()?;

    let _duration = tokens.next()?; // e.g. "4:16" — not currently needed

    let label = tokens.collect::<Vec<_>>().join(" ");
    if label.is_empty() {
        return None;
    }

    let (track_number, title) = split_track_label(&label);
    Some(DrTrackEntry {
        dr,
        peak_db,
        rms_db,
        track_number,
        title,
    })
}

/// Splits a track-table label like `"01-Free Fallin'"`, `"01 - Wildflowers"`,
/// or `"A1-Wildflowers"` into an optional leading track number and the
/// remaining title. Vinyl-side-prefixed labels ("A1", "B2") have no plain
/// integer track number, so `track_number` is `None` for those — title-based
/// matching in `match_tracks` picks up the slack.
fn split_track_label(label: &str) -> (Option<i32>, String) {
    let digit_count = label.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return (None, label.trim().to_string());
    }
    let number: i32 = match label[..digit_count].parse() {
        Ok(n) => n,
        Err(_) => return (None, label.trim().to_string()),
    };
    let rest = label[digit_count..]
        .trim_start_matches(['-', '.', ' '])
        .trim();
    let title = if rest.is_empty() {
        label.trim().to_string()
    } else {
        rest.to_string()
    };
    (Some(number), title)
}

/// Extracts the integer from a line containing `"<prefix> DRnn"`
/// (case-insensitive on both the prefix and the "DR"), e.g.
/// `"Official DR value: DR13"` or `"    Weighted: DR12"`.
fn extract_dr_value(line: &str, prefix: &str) -> Option<i32> {
    let lower = line.to_ascii_lowercase();
    let idx = lower.find(prefix)?;
    let after = &line[idx + prefix.len()..];
    let dr_idx = after.to_ascii_uppercase().find("DR")?;
    let digits: String = after[dr_idx + 2..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

// ---------------------------------------------------------------------------
// Track matching
// ---------------------------------------------------------------------------

/// The minimal projection of a `songs` row `match_tracks` needs — kept
/// separate from `models::Song` so this module stays free of a database
/// dependency and is directly unit-testable.
pub struct SongCandidate {
    pub id: i64,
    pub track_number: Option<i32>,
    pub title: Option<String>,
    /// File stem (filename without extension), used as a title fallback for
    /// untagged songs.
    pub filename_stem: String,
}

/// Matches parsed DR log entries to the songs found in the same album
/// folder. Track-number matches are tried first since they're unambiguous;
/// entries left over fall back to a normalized substring match against the
/// song's title or filename. A song or entry with no confident match is
/// simply omitted — a wrong per-track match would silently corrupt gain
/// calculations later, which is worse than leaving a track unmatched.
pub fn match_tracks<'a>(
    entries: &'a [DrTrackEntry],
    candidates: &[SongCandidate],
) -> Vec<(i64, &'a DrTrackEntry)> {
    let mut matches: Vec<(i64, &DrTrackEntry)> = Vec::new();
    let mut used_songs: HashSet<i64> = HashSet::new();
    let mut used_entries: HashSet<usize> = HashSet::new();

    for (i, entry) in entries.iter().enumerate() {
        let Some(num) = entry.track_number else {
            continue;
        };
        if let Some(song) = candidates
            .iter()
            .find(|c| !used_songs.contains(&c.id) && c.track_number == Some(num))
        {
            matches.push((song.id, entry));
            used_songs.insert(song.id);
            used_entries.insert(i);
        }
    }

    for (i, entry) in entries.iter().enumerate() {
        if used_entries.contains(&i) {
            continue;
        }
        let normalized_entry = normalize(&entry.title);
        if normalized_entry.is_empty() {
            continue;
        }
        if let Some(song) = candidates.iter().filter(|c| !used_songs.contains(&c.id)).find(|c| {
            let title_norm = c.title.as_deref().map(normalize).unwrap_or_default();
            let stem_norm = normalize(&c.filename_stem);
            (!title_norm.is_empty()
                && (title_norm == normalized_entry
                    || title_norm.contains(&normalized_entry)
                    || normalized_entry.contains(&title_norm)))
                || stem_norm.contains(&normalized_entry)
        }) {
            matches.push((song.id, entry));
            used_songs.insert(song.id);
        }
    }

    matches
}

/// Lowercased alphanumeric-only projection of a string, for tolerant title
/// comparison (ignores punctuation, apostrophes, and case differences like
/// "Runnin'" vs "Running").
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LOG: &str = "foobar2000 2.24.1 / Dynamic Range Meter 1.1.1\n\
log date: 2025-10-12 12:22:45\n\
\n\
--------------------------------------------------------------------------------\n\
Analyzed: Tom Petty / Full Moon Fever\n\
--------------------------------------------------------------------------------\n\
\n\
DR         Peak         RMS     Duration Track\n\
--------------------------------------------------------------------------------\n\
DR14      -0.80 dB   -17.46 dB      4:16 01-Free Fallin'\n\
DR13      -0.70 dB   -17.56 dB      2:58 02-I Won't Back Down\n\
DR11      -6.84 dB   -23.26 dB      2:02 10-Alright for Now\n\
--------------------------------------------------------------------------------\n\
\n\
Number of tracks:  12\n\
Official DR value: DR13\n\
\n\
Samplerate:        96000 Hz\n\
Channels:          2\n\
Bits per sample:   24\n\
Bitrate:           3049 kbps\n\
Codec:             FLAC\n\
================================================================================\n";

    #[test]
    fn parses_track_rows() {
        let log = parse(SAMPLE_LOG);
        assert_eq!(log.tracks.len(), 3);
        assert_eq!(
            log.tracks[0],
            DrTrackEntry {
                dr: 14,
                peak_db: -0.80,
                rms_db: -17.46,
                track_number: Some(1),
                title: "Free Fallin'".to_string(),
            }
        );
        assert_eq!(log.tracks[2].track_number, Some(10));
        assert_eq!(log.tracks[2].title, "Alright for Now");
    }

    #[test]
    fn parses_official_dr_value() {
        let log = parse(SAMPLE_LOG);
        assert_eq!(log.album_dr, Some(13));
    }

    #[test]
    fn weighted_dr_takes_priority_over_official() {
        let content = "DR14      -0.80 dB   -17.46 dB      4:16 01-Free Fallin'\n\
Official DR value: DR13 (Weighted: DR12)\n";
        let log = parse(content);
        assert_eq!(log.album_dr, Some(12));
    }

    #[test]
    fn handles_dbfs_unit_variant() {
        let content = "DR14      -1.73 dBFS   -18.85 dBFS      3:02 01-Track One\n";
        let log = parse(content);
        assert_eq!(log.tracks.len(), 1);
        assert_eq!(log.tracks[0].peak_db, -1.73);
        assert_eq!(log.tracks[0].rms_db, -18.85);
    }

    #[test]
    fn parses_vinyl_side_labels_without_track_number() {
        let content = "DR12      -3.18 dB   -18.07 dB      3:06 A1-Wildflowers\n";
        let log = parse(content);
        assert_eq!(log.tracks[0].track_number, None);
    }

    #[test]
    fn ignores_header_and_separator_lines() {
        let content = "DR         Peak         RMS     Duration Track\n\
--------------------------------------------------------------------------------\n\
Number of tracks:  12\n";
        let log = parse(content);
        assert!(log.tracks.is_empty());
        assert_eq!(log.album_dr, None);
    }

    #[test]
    fn match_tracks_prefers_track_number() {
        let log = parse(SAMPLE_LOG);
        let candidates = vec![
            SongCandidate {
                id: 1,
                track_number: Some(1),
                title: Some("Something Else Entirely".to_string()),
                filename_stem: "01 Free Fallin".to_string(),
            },
            SongCandidate {
                id: 2,
                track_number: Some(2),
                title: Some("I Won't Back Down".to_string()),
                filename_stem: "02 I Wont Back Down".to_string(),
            },
        ];
        let matched = match_tracks(&log.tracks, &candidates);
        assert_eq!(matched.len(), 2);
        assert!(matched.iter().any(|(id, e)| *id == 1 && e.track_number == Some(1)));
        assert!(matched.iter().any(|(id, e)| *id == 2 && e.track_number == Some(2)));
    }

    #[test]
    fn match_tracks_falls_back_to_title_similarity() {
        let entries = vec![DrTrackEntry {
            dr: 14,
            peak_db: -0.8,
            rms_db: -17.46,
            track_number: None,
            title: "Free Fallin'".to_string(),
        }];
        let candidates = vec![SongCandidate {
            id: 42,
            track_number: None,
            title: Some("Free Fallin".to_string()),
            filename_stem: "01 Free Fallin".to_string(),
        }];
        let matched = match_tracks(&entries, &candidates);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].0, 42);
    }

    #[test]
    fn match_tracks_omits_unconfident_matches() {
        let entries = vec![DrTrackEntry {
            dr: 14,
            peak_db: -0.8,
            rms_db: -17.46,
            track_number: None,
            title: "Completely Unrelated Title".to_string(),
        }];
        let candidates = vec![SongCandidate {
            id: 1,
            track_number: None,
            title: Some("Nothing Alike".to_string()),
            filename_stem: "nothing_alike".to_string(),
        }];
        assert!(match_tracks(&entries, &candidates).is_empty());
    }
}
