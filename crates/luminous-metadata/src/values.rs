//! Canonical multi-value tag string splitting, trimming, and joining.
//!
//! Shared across tag reading, tag writing, and application models for
//! `; `-delimited fields (`genre`, `artist`, `album_artist`, `composer`).

use std::collections::HashSet;

/// The canonical delimiter used to join multi-value strings in storage
/// (`songs.genre`, `artist`, `album_artist`, `composer`).
pub const MULTI_VALUE_DELIMITER: &str = "; ";

/// Splits `raw` on `;`, trims whitespace, drops empty items, and dedupes
/// case-insensitively while preserving first-seen casing and original order.
///
/// Deliberately does *not* also split on `/`, even though that's a
/// long-standing convention for legacy-joined multi-value fields (TPE1's own
/// "Lead performer(s)/Soloist(s)" convention, and Picard's default
/// `id3v23_join_with = "/"` for ID3v2.3) — `/` shows up too often inside a
/// single legitimate value (band names like "AC/DC", not to mention genre
/// names) to use as a splitting heuristic without silently corrupting them.
pub fn parse_multi_value(raw: &str) -> Vec<String> {
    split_and_dedup(raw, &[';'])
}

/// Joins a value list back into the canonical `; `-delimited form.
pub fn join_multi_value(values: &[String]) -> String {
    values.join(MULTI_VALUE_DELIMITER)
}

/// Splits `raw` on any of `delimiters`, trims, drops empties, and dedupes
/// case-insensitively while preserving first-seen casing and order.
fn split_and_dedup(raw: &str, delimiters: &[char]) -> Vec<String> {
    let mut seen = HashSet::new();
    raw.split(delimiters)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.to_lowercase()))
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_multi_value_splits_trims_and_dedupes() {
        assert_eq!(
            parse_multi_value("Rock; Jazz Fusion; Live"),
            vec!["Rock", "Jazz Fusion", "Live"]
        );
        // Whitespace-only separators still split; empties dropped.
        assert_eq!(parse_multi_value("Rock;;  Jazz "), vec!["Rock", "Jazz"]);
        // Case-insensitive dedup, first-seen casing wins.
        assert_eq!(parse_multi_value("Rock; rock; ROCK"), vec!["Rock"]);
        // A single value with no delimiter round-trips unchanged.
        assert_eq!(parse_multi_value("Metal"), vec!["Metal"]);
        assert_eq!(parse_multi_value(""), Vec::<String>::new());
        assert_eq!(parse_multi_value("   "), Vec::<String>::new());
    }

    #[test]
    fn test_parse_multi_value_does_not_split_on_slash() {
        // '/' is a real-world legacy multi-value join convention (TPE1,
        // Picard's ID3v2.3 fallback) but is deliberately *not* treated as a
        // splitting delimiter here, since it collides with band names that
        // legitimately contain a slash — "AC/DC" must survive intact rather
        // than becoming two artists, "AC" and "DC".
        assert_eq!(parse_multi_value("AC/DC"), vec!["AC/DC"]);
        // A real multi-value string still splits correctly on ';', with the
        // slash-containing value passing through as one of the items.
        assert_eq!(
            parse_multi_value("David Guetta; AC/DC"),
            vec!["David Guetta", "AC/DC"]
        );
    }

    #[test]
    fn test_join_multi_value() {
        assert_eq!(
            join_multi_value(&["Rock".to_string(), "Jazz Fusion".to_string()]),
            "Rock; Jazz Fusion"
        );
        assert_eq!(join_multi_value(&[]), "");
        assert_eq!(join_multi_value(&["Metal".to_string()]), "Metal");
    }

    #[test]
    fn test_multi_value_round_trips_through_join_and_parse() {
        let values = vec!["Rock".to_string(), "Jazz Fusion".to_string()];
        assert_eq!(parse_multi_value(&join_multi_value(&values)), values);
    }
}
