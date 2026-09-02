//! RFC 3339 timestamps to epoch milliseconds.
//!
//! Hand-rolled rather than pulling in a date crate: two callers, one format.
//! The five-hour meter reads `resets_at` out of the usage API, and the task
//! probe reads `timestamp` off transcript records. Both are RFC 3339, and
//! both shapes — `2026-08-25T10:50:00.070318+00:00` and
//! `2026-08-28T08:42:47.177Z` — are covered.

/// Epoch milliseconds for an RFC 3339 timestamp.
///
/// Fractional seconds are truncated, not rounded: the values this reads are a
/// reset time being counted down to and a task's start time being aged, and a
/// millisecond either way is not visible in either.
pub fn epoch_ms(text: &str) -> Option<i64> {
    let (date, rest) = text.split_once('T')?;
    let mut date = date.splitn(3, '-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: u32 = date.next()?.parse().ok()?;
    let day: u32 = date.next()?.parse().ok()?;

    // Split the offset off before the time, so its own colons are not read as
    // part of the clock.
    let (clock, offset_secs) = split_offset(rest)?;
    let mut clock = clock.splitn(3, ':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    let seconds = clock.next()?;
    let (whole, frac) = seconds.split_once('.').unwrap_or((seconds, ""));
    let second: i64 = whole.parse().ok()?;

    // Pad or trim the fraction to exactly milliseconds.
    let millis: i64 = if frac.is_empty() {
        0
    } else {
        let mut ms: String = frac.chars().take(3).collect();
        while ms.len() < 3 {
            ms.push('0');
        }
        ms.parse().ok()?
    };

    let days = days_from_civil(year, month, day)?;
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_secs;
    Some(secs * 1_000 + millis)
}

/// Split a trailing UTC offset off a time, returning the time and the offset in
/// seconds east of UTC.
fn split_offset(time: &str) -> Option<(&str, i64)> {
    if let Some(clock) = time.strip_suffix('Z').or_else(|| time.strip_suffix('z')) {
        return Some((clock, 0));
    }
    // Search from the end: the offset sign is the last `+` or `-`, and a `-`
    // never appears inside the time itself.
    let at = time.rfind(['+', '-'])?;
    let (clock, offset) = time.split_at(at);
    let sign = if offset.starts_with('-') { -1 } else { 1 };
    let (hours, minutes) = offset[1..].split_once(':').unwrap_or((&offset[1..], "0"));
    let hours: i64 = hours.parse().ok()?;
    let minutes: i64 = minutes.parse().ok()?;
    Some((clock, sign * (hours * 3_600 + minutes * 60)))
}

/// Days between the Unix epoch and a civil date, negative before it.
///
/// Howard Hinnant's `days_from_civil`, which is exact for the whole proleptic
/// Gregorian calendar and needs no table.
fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let month = month as i64;
    let day = day as i64;
    // The year is shifted to start in March, which puts the leap day last and
    // makes the month-length series regular.
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOON: i64 = 1_787_745_600_000; // 2026-08-26T12:00:00Z

    #[test]
    fn epoch_ms_handles_the_shapes_this_field_carries() {
        // Checked against `date -u -j -f %Y-%m-%dT%H:%M:%S ... +%s`.
        assert_eq!(epoch_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch_ms("2026-08-26T12:00:00Z"), Some(NOON));
        assert_eq!(
            epoch_ms("2026-08-26T12:00:00.070318+00:00"),
            Some(NOON + 70)
        );
        // An offset moves the instant against the same wall clock.
        assert_eq!(epoch_ms("2026-08-26T14:00:00+02:00"), Some(NOON));
        assert_eq!(epoch_ms("2026-08-26T10:00:00-02:00"), Some(NOON));
        // Leap day, and the century rule either side of it.
        assert_eq!(epoch_ms("2024-02-29T00:00:00Z"), Some(1_709_164_800_000));
        assert_eq!(epoch_ms("2000-03-01T00:00:00Z"), Some(951_868_800_000));
        assert_eq!(epoch_ms("1900-03-01T00:00:00Z"), Some(-2_203_891_200_000));
    }

    #[test]
    fn epoch_ms_rejects_what_it_cannot_read() {
        assert_eq!(epoch_ms(""), None);
        assert_eq!(epoch_ms("2026-08-26"), None);
        assert_eq!(epoch_ms("2026-08-26T12:00"), None);
        assert_eq!(epoch_ms("2026-13-01T00:00:00Z"), None);
        assert_eq!(epoch_ms("2026-00-01T00:00:00Z"), None);
        assert_eq!(epoch_ms("2026-08-00T00:00:00Z"), None);
    }
}
