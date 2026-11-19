//! What is playing right now, as the OS media session, Discord presence and
//! tray tooltip show it (#2008).
//!
//! For a library song that is the song's own tags. For an internet radio
//! station it is the track the station announces in its ICY `StreamTitle`,
//! parsed here once, held on the player (never written to the song row) and
//! sent to the frontend on `PlaybackState::station_now_playing`. The
//! frontend never re-parses it.

use crate::models::PlaybackState;
use serde::{Deserialize, Serialize};

/// The track a station says it is playing, from its ICY `StreamTitle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StationNowPlaying {
    /// The performer, when the title follows the usual "Artist - Title" form.
    pub artist: Option<String>,
    /// The track title, or the whole announced text when it has no separator.
    pub title: String,
}

/// Separators stations put between artist and title. Spaces on both sides
/// keep hyphenated names ("Jay-Z", "Blink-182") in one piece.
const SEPARATORS: [&str; 3] = [" - ", " – ", " — "];

/// Parses a raw `StreamTitle` into artist and title. Returns `None` for text
/// that names no track: empty or punctuation-only lines, and a title that just
/// repeats the station's own name (many stations send it between songs).
pub fn parse_stream_title(raw: &str, station_name: Option<&str>) -> Option<StationNowPlaying> {
    // A dash at either edge is a separator with one side missing
    // (" - Title", "Artist - "): drop it with the padding.
    let text = raw.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '–' | '—'));
    if !has_words(text) {
        return None;
    }
    if station_name.is_some_and(|name| name.trim().eq_ignore_ascii_case(text)) {
        return None;
    }
    let split = SEPARATORS
        .iter()
        .filter_map(|sep| text.find(sep).map(|at| (at, sep.len())))
        .min_by_key(|&(at, _)| at);
    let Some((at, sep_len)) = split else {
        return Some(StationNowPlaying {
            artist: None,
            title: text.to_string(),
        });
    };
    let artist = text[..at].trim();
    let title = text[at + sep_len..].trim();
    match (has_words(artist), has_words(title)) {
        (true, true) => Some(StationNowPlaying {
            artist: Some(artist.to_string()),
            title: title.to_string(),
        }),
        (false, true) => Some(StationNowPlaying {
            artist: None,
            title: title.to_string(),
        }),
        (true, false) => Some(StationNowPlaying {
            artist: None,
            title: artist.to_string(),
        }),
        (false, false) => None,
    }
}

fn has_words(s: &str) -> bool {
    s.chars().any(char::is_alphanumeric)
}

/// Title, artist and album to show for a playback snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlayingLabels {
    pub title: String,
    pub artist: Option<String>,
    /// The album, or for a station with a now-playing track the station name.
    pub album: Option<String>,
}

/// The labels every "what's playing" surface shows for `state`: the station's
/// announced track when there is one (with the station name as the album),
/// otherwise the current song's own tags. `None` when nothing is loaded.
pub fn labels(state: &PlaybackState) -> Option<NowPlayingLabels> {
    let song = state.current_song.as_ref()?;
    Some(match &state.station_now_playing {
        Some(np) => NowPlayingLabels {
            title: np.title.clone(),
            artist: np.artist.clone(),
            album: Some(song.display_title().to_string()),
        },
        None => NowPlayingLabels {
            title: song.display_title().to_string(),
            artist: song.artist.clone(),
            album: song.album.clone(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Song;

    fn parse(raw: &str) -> Option<StationNowPlaying> {
        parse_stream_title(raw, Some("Groove Salad"))
    }

    fn np(artist: Option<&str>, title: &str) -> Option<StationNowPlaying> {
        Some(StationNowPlaying {
            artist: artist.map(str::to_string),
            title: title.to_string(),
        })
    }

    #[test]
    fn splits_artist_and_title_on_the_first_spaced_dash() {
        assert_eq!(
            parse("Boards of Canada - Roygbiv"),
            np(Some("Boards of Canada"), "Roygbiv")
        );
        assert_eq!(
            parse("  Air – La Femme d'Argent  "),
            np(Some("Air"), "La Femme d'Argent"),
            "en dash, outer whitespace trimmed"
        );
        assert_eq!(
            parse("Lamb - Gorecki - Live at Koko"),
            np(Some("Lamb"), "Gorecki - Live at Koko"),
            "later dashes stay in the title"
        );
    }

    #[test]
    fn keeps_hyphenated_names_whole() {
        assert_eq!(
            parse("Jay-Z - 99 Problems"),
            np(Some("Jay-Z"), "99 Problems")
        );
        assert_eq!(
            parse("Blink-182"),
            np(None, "Blink-182"),
            "no spaced separator: title only"
        );
    }

    #[test]
    fn text_without_a_separator_is_the_title() {
        assert_eq!(
            parse("Live from the studio"),
            np(None, "Live from the studio")
        );
    }

    #[test]
    fn one_empty_side_leaves_the_other_as_title() {
        assert_eq!(parse(" - Roygbiv"), np(None, "Roygbiv"));
        assert_eq!(parse("Boards of Canada - "), np(None, "Boards of Canada"));
    }

    #[test]
    fn empty_and_junk_lines_name_no_track() {
        for junk in ["", "   ", "-", " - ", "--- * ---", "' '"] {
            assert_eq!(parse(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn the_station_repeating_its_own_name_is_not_a_track() {
        assert_eq!(parse("Groove Salad"), None);
        assert_eq!(parse("  groove salad "), None, "case and padding ignored");
        assert_eq!(
            parse_stream_title("Groove Salad", None),
            np(None, "Groove Salad"),
            "unknown station name: kept"
        );
    }

    fn station_state(np: Option<StationNowPlaying>) -> PlaybackState {
        PlaybackState {
            current_song: Some(Song {
                title: Some("Groove Salad".into()),
                artist: Some("US".into()),
                ..Default::default()
            }),
            station_now_playing: np,
            ..Default::default()
        }
    }

    #[test]
    fn labels_prefer_the_station_track_with_the_station_as_album() {
        let state = station_state(Some(StationNowPlaying {
            artist: Some("Air".into()),
            title: "Alone in Kyoto".into(),
        }));
        assert_eq!(
            labels(&state),
            Some(NowPlayingLabels {
                title: "Alone in Kyoto".into(),
                artist: Some("Air".into()),
                album: Some("Groove Salad".into()),
            })
        );
    }

    #[test]
    fn labels_fall_back_to_the_song_and_to_nothing() {
        let state = station_state(None);
        let l = labels(&state).unwrap();
        assert_eq!(l.title, "Groove Salad");
        assert_eq!(l.artist.as_deref(), Some("US"));
        assert_eq!(labels(&PlaybackState::default()), None);
    }
}
