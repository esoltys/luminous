//! Wall-clock helpers shared across the app.
//!
//! Everything that stamps a row or compares against "now" in Unix seconds goes
//! through [`unix_now`], so there is one definition of "now" for the crate.
//! Calendar logic (local dates, week boundaries) uses `jiff`, but always through
//! [`local_tz`] / [`local_now`] / [`day_start_in_tz`] and never
//! `TimeZone::system()` / `ZonedDateTime::now()` directly, so the unknown-zone
//! fallback below applies everywhere.

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

/// The user's local time zone.
///
/// `jiff` resolves the Windows zone ID through a built-in table; a zone newer
/// than that table (for example `British Columbia Standard Time` on older
/// releases, #2169) comes back unknown, which behaves as UTC: the Daypart Mix
/// then picked "Late Night" at 16:00 while the Home greeting said "Good
/// Afternoon". When that happens on Windows this falls back to the OS's current
/// UTC offset, which has the right local hour and date; only a DST change in a
/// historical date is approximate.
pub fn local_tz() -> jiff::tz::TimeZone {
    let tz = jiff::tz::TimeZone::system();
    if !tz.is_unknown() {
        return tz;
    }
    match os_utc_offset_seconds().and_then(|s| jiff::tz::Offset::from_seconds(s).ok()) {
        Some(offset) => {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                log::warn!(
                    "system time zone is unknown to jiff; using the OS UTC offset ({offset}) instead"
                );
            });
            jiff::tz::TimeZone::fixed(offset)
        }
        None => tz,
    }
}

/// The current time in the user's local zone (see [`local_tz`]).
pub fn local_now() -> jiff::ZonedDateTime {
    jiff::Timestamp::now().to_zoned_datetime(local_tz())
}

/// Midnight starting `day` in `tz`, as epoch seconds. A zone whose clocks skip
/// midnight (a DST gap) starts the day at its first existing local time via
/// `jiff`'s default `"compatible"` disambiguation.
pub fn day_start_in_tz(day: jiff::civil::Date, tz: &jiff::tz::TimeZone) -> Option<i64> {
    day.to_zoned_datetime(tz.clone())
        .ok()
        .map(|z| z.timestamp().as_second())
}

/// Seconds east of UTC from the Windows `TIME_ZONE_INFORMATION` biases, which
/// are minutes *west* of UTC. `id` is the value `GetTimeZoneInformation`
/// returns: 1 standard time, 2 daylight time, anything else unknown.
#[cfg_attr(not(windows), allow(dead_code))]
fn offset_from_windows_bias(id: u32, bias: i32, standard_bias: i32, daylight_bias: i32) -> i32 {
    let extra = if id == 2 {
        daylight_bias
    } else {
        standard_bias
    };
    -(bias + extra) * 60
}

#[cfg(windows)]
fn os_utc_offset_seconds() -> Option<i32> {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    // SAFETY: `GetTimeZoneInformation` only writes into the struct we pass.
    let mut info: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    let id = unsafe { GetTimeZoneInformation(&mut info) };
    if id == 0xFFFF_FFFF {
        return None;
    }
    Some(offset_from_windows_bias(
        id,
        info.Bias,
        info.StandardBias,
        info.DaylightBias,
    ))
}

#[cfg(not(windows))]
fn os_utc_offset_seconds() -> Option<i32> {
    None
}

/// Current local time as an RFC 3339 string with a numeric UTC offset
/// (`2026-06-15T09:00:00-06:00`), for human-read log and diagnostics lines.
pub fn local_rfc3339() -> String {
    rfc3339_of(&local_now())
}

fn rfc3339_of(now: &jiff::ZonedDateTime) -> String {
    // Not `ZonedDateTime`'s own Display, which appends a `[Zone/Name]` annotation.
    now.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_now_is_after_2023() {
        assert!(unix_now() > 1_700_000_000);
    }

    #[test]
    fn windows_bias_gives_seconds_east_of_utc() {
        // Pacific: 480 minutes west, DST adds -60 (an hour east).
        assert_eq!(offset_from_windows_bias(1, 480, 0, -60), -8 * 3600);
        assert_eq!(offset_from_windows_bias(2, 480, 0, -60), -7 * 3600);
        // A zone east of UTC has a negative bias.
        assert_eq!(offset_from_windows_bias(1, -330, 0, -60), 330 * 60);
        // An unknown id is treated as standard time.
        assert_eq!(offset_from_windows_bias(0, 480, 0, -60), -8 * 3600);
    }

    #[test]
    fn local_tz_is_never_unknown_on_a_machine_with_a_known_offset() {
        // On Windows an unmapped zone ID (#2169) used to resolve to UTC.
        if cfg!(windows) {
            assert!(!local_tz().is_unknown());
        }
    }

    #[test]
    fn rfc3339_has_numeric_offset_and_no_zone_annotation() {
        let tz = jiff::tz::TimeZone::fixed(jiff::tz::offset(-6));
        let t = jiff::civil::date(2026, 6, 15)
            .at(9, 0, 0, 0)
            .to_zoned_datetime(tz)
            .unwrap();
        assert_eq!(rfc3339_of(&t), "2026-06-15T09:00:00-06:00");
    }

    #[test]
    fn day_start_in_tz_returns_midnight_epoch_seconds() {
        let day = jiff::civil::date(2026, 6, 15);
        let utc_midnight = day_start_in_tz(day, &jiff::tz::TimeZone::UTC).unwrap();
        let minus_six = jiff::tz::TimeZone::fixed(jiff::tz::offset(-6));
        let local_midnight = day_start_in_tz(day, &minus_six).unwrap();
        assert_eq!(local_midnight - utc_midnight, 6 * 3600);
    }
}
