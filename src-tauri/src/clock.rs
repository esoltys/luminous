//! Wall-clock helpers shared across the app.
//!
//! Everything that stamps a row or compares against "now" in Unix seconds goes
//! through [`unix_now`], so there is one definition of "now" for the crate.
//! Calendar logic (local dates, week boundaries) uses `jiff` directly.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current time as Unix seconds. A system clock set before 1970 reads as 0
/// rather than failing: callers store or compare the value and have no use for
/// an error here.
pub fn unix_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

/// Current local time as an RFC 3339 string with a numeric UTC offset
/// (`2026-06-15T09:00:00-06:00`), for human-read log and diagnostics lines.
pub fn local_rfc3339() -> String {
    rfc3339_of(&jiff::Zoned::now())
}

fn rfc3339_of(now: &jiff::Zoned) -> String {
    // Not `Zoned`'s own Display, which appends a `[Zone/Name]` annotation.
    now.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// Parses an RFC 3339 timestamp (any offset, or `Z`) into Unix seconds.
pub fn parse_rfc3339_unix(raw: &str) -> Option<i64> {
    raw.parse::<jiff::Timestamp>().ok().map(|t| t.as_second())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_now_is_after_2023() {
        assert!(unix_now() > 1_700_000_000);
    }

    #[test]
    fn rfc3339_has_numeric_offset_and_no_zone_annotation() {
        let tz = jiff::tz::TimeZone::fixed(jiff::tz::offset(-6));
        let t = jiff::civil::date(2026, 6, 15)
            .at(9, 0, 0, 0)
            .to_zoned(tz)
            .unwrap();
        assert_eq!(rfc3339_of(&t), "2026-06-15T09:00:00-06:00");
    }

    #[test]
    fn parse_rfc3339_handles_offsets_and_rejects_garbage() {
        assert_eq!(parse_rfc3339_unix("1970-01-01T00:00:10Z"), Some(10));
        assert_eq!(parse_rfc3339_unix("1970-01-01T01:00:10+01:00"), Some(10));
        assert_eq!(
            parse_rfc3339_unix("2024-01-01T00:00:00.5Z"),
            Some(1_704_067_200)
        );
        assert_eq!(parse_rfc3339_unix("not a date"), None);
        assert_eq!(parse_rfc3339_unix("2024-01-01"), None);
    }
}
