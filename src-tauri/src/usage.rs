//! How much of the rolling five-hour limit is spent.
//!
//! Read from `~/.claude.json`, where Claude Code caches what the API told it
//! under `cachedUsageUtilization`. Nothing local can derive this: the limit is
//! enforced server-side and is not a token count, so the transcripts — which
//! carry per-message `usage` and nothing else — cannot be summed into it.
//!
//! It is a cache, and Claude Code only refreshes it when it actually fetches
//! usage, so it can be hours behind while the file around it is rewritten
//! constantly. `resets_at` is what makes that safe to work with: once the
//! window it describes has passed, the figure belongs to a window that is over
//! and is not shown at all. Within a live window a stale figure is still a
//! lower bound on what has been spent, which is the best available and is not
//! misleading in the way a lapsed one would be.

use serde::Serialize;

/// Percentages at or above which the meter stops being merely informational.
const WARN_AT: u8 = 75;
const CRITICAL_AT: u8 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Normal,
    Warn,
    Critical,
}

/// The five-hour window's state, or nothing when it cannot be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// Whole percent of the window spent, clamped to 0..=100.
    pub percent: u8,
    /// Absolute epoch ms the window resets. The widget counts down from this
    /// rather than being sent a duration, so the countdown ticks without the
    /// watcher having to re-emit for it.
    pub resets_at_ms: i64,
    pub severity: Severity,
}

fn severity_for(percent: u8) -> Severity {
    if percent >= CRITICAL_AT {
        Severity::Critical
    } else if percent >= WARN_AT {
        Severity::Warn
    } else {
        Severity::Normal
    }
}

/// Where Claude Code keeps its global state.
pub fn usage_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/"))
        .join(".claude.json")
}

/// Current usage, or `None` if it is missing, unreadable or lapsed.
pub fn read(now_ms: i64) -> Option<Usage> {
    parse(&std::fs::read(usage_path()).ok()?, now_ms)
}

/// Parse usage out of the raw file.
///
/// Every step is fallible and every failure is `None`: this reads an
/// undocumented field of a file owned by another program, so a shape that is
/// not what is expected must leave the widget without a meter rather than
/// taking anything down.
pub fn parse(bytes: &[u8], now_ms: i64) -> Option<Usage> {
    let root: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let five_hour = root
        .get("cachedUsageUtilization")?
        .get("utilization")?
        .get("five_hour")?;

    let percent = five_hour.get("utilization")?.as_f64()?;
    // NaN would survive `as u8` as 0 and read as a fresh window.
    if !percent.is_finite() {
        return None;
    }
    let percent = percent.round().clamp(0.0, 100.0) as u8;

    let resets_at_ms = epoch_ms(five_hour.get("resets_at")?.as_str()?)?;
    // A window that has already reset says nothing about the one now running.
    if resets_at_ms <= now_ms {
        return None;
    }

    Some(Usage {
        percent,
        resets_at_ms,
        severity: severity_for(percent),
    })
}

/// Epoch milliseconds for an RFC 3339 timestamp.
///
/// Hand-rolled rather than pulling in a date crate for one field. Handles the
/// shape this file actually carries — `2026-08-25T10:50:00.070318+00:00` — plus
/// `Z` and a non-zero offset. Fractional seconds are truncated, not rounded:
/// the value is a reset time being counted down to, and a millisecond either
/// way is not visible.
fn epoch_ms(text: &str) -> Option<i64> {
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

    /// The real shape, trimmed to the fields that are read.
    fn file(percent: &str, resets_at: &str) -> Vec<u8> {
        format!(
            r#"{{
              "someOtherKey": 1,
              "cachedUsageUtilization": {{
                "fetchedAtMs": 1787650654099,
                "utilization": {{
                  "five_hour": {{
                    "utilization": {percent},
                    "resets_at": "{resets_at}",
                    "limit_dollars": null
                  }},
                  "seven_day": {{ "utilization": 34, "resets_at": "2026-08-27T00:00:00Z" }}
                }}
              }}
            }}"#
        )
        .into_bytes()
    }

    const NOON: i64 = 1_787_745_600_000; // 2026-08-26T12:00:00Z
    const LATER: &str = "2026-08-26T14:41:00.070318+00:00";

    #[test]
    fn reads_the_five_hour_window() {
        let usage = parse(&file("42", LATER), NOON).expect("a live window parses");
        assert_eq!(usage.percent, 42);
        assert_eq!(usage.severity, Severity::Normal);
        assert!(usage.resets_at_ms > NOON);
    }

    #[test]
    fn ignores_a_window_that_has_already_reset() {
        // The figure describes a window that is over, so it says nothing about
        // the one now running — showing it would be showing a stale number as
        // if it were current.
        assert_eq!(
            parse(&file("100", "2026-08-25T10:50:00.070318+00:00"), NOON),
            None
        );
    }

    #[test]
    fn treats_the_reset_instant_itself_as_lapsed() {
        let at_reset = parse(&file("100", LATER), NOON).unwrap().resets_at_ms;
        assert_eq!(parse(&file("100", LATER), at_reset), None);
        assert!(parse(&file("100", LATER), at_reset - 1).is_some());
    }

    #[test]
    fn grades_severity_by_how_much_is_left() {
        let at = |p: &str| parse(&file(p, LATER), NOON).unwrap().severity;
        assert_eq!(at("74"), Severity::Normal);
        assert_eq!(at("75"), Severity::Warn);
        assert_eq!(at("89"), Severity::Warn);
        assert_eq!(at("90"), Severity::Critical);
        assert_eq!(at("100"), Severity::Critical);
    }

    #[test]
    fn clamps_a_percentage_outside_the_range() {
        assert_eq!(parse(&file("140", LATER), NOON).unwrap().percent, 100);
        assert_eq!(parse(&file("-3", LATER), NOON).unwrap().percent, 0);
    }

    #[test]
    fn rounds_a_fractional_percentage() {
        assert_eq!(parse(&file("42.6", LATER), NOON).unwrap().percent, 43);
    }

    #[test]
    fn a_missing_or_malformed_file_yields_nothing_rather_than_failing() {
        // The field is undocumented and belongs to another program: an
        // unexpected shape must leave the widget without a meter, not break it.
        assert_eq!(parse(b"", NOON), None);
        assert_eq!(parse(b"{ not json", NOON), None);
        assert_eq!(parse(br#"{"cachedUsageUtilization": {}}"#, NOON), None);
        assert_eq!(
            parse(
                br#"{"cachedUsageUtilization":{"utilization":{"five_hour":null}}}"#,
                NOON
            ),
            None
        );
        assert_eq!(parse(&file("null", LATER), NOON), None);
        assert_eq!(parse(&file("42", "not a date"), NOON), None);
    }

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
