//! How much of the rolling five-hour limit is spent.
//!
//! The figure comes from the API — see `crate::usage_api`, which fetches it.
//! This module is the shape of it, the rules about when it may be shown, and
//! the parsing of the object the API answers with.
//!
//! Nothing local can derive the figure: the limit is enforced server-side and is
//! not a token count, so the transcripts — which carry per-message `usage` and
//! nothing else — cannot be summed into it.
//!
//! `resets_at` is what makes it safe to hold on to between fetches. Once the
//! window it describes has passed, the figure belongs to a window that is over
//! and is not shown at all. Within a live window a figure a few minutes old is
//! still a lower bound on what has been spent, which is the best available and
//! is not misleading in the way a lapsed one would be.
//!
//! Claude Code caches its own copy in `~/.claude.json`, and the widget used to
//! read it. It no longer does: that cache is refreshed only when Claude Code
//! fetches usage itself, so it ran hours behind — 5% against the API's 13% in
//! one measurement. The file is read now only when `CLAUDE_BUDDY_USAGE_FILE`
//! points at a fixture, which is how the documentation screenshots are taken.

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

/// Environment variable pointing the meter at a fixture file.
///
/// The companion of `REGISTRY_DIR_ENV` and `PROJECTS_DIR_ENV`, and there for
/// the same reason: the real figure is whatever the account has actually spent,
/// which is neither reproducible nor anyone else's business, so the
/// documentation screenshots cannot be taken against it. Set, it also stops the
/// live fetch, which would otherwise put the real figure straight back.
pub const USAGE_FILE_ENV: &str = "CLAUDE_BUDDY_USAGE_FILE";

/// Usage from the fixture file, or `None` when there is no fixture.
///
/// The only remaining reader of a file in the shape of `~/.claude.json`. There
/// is no fallback to the real one: a stale figure shown as if it were current is
/// worse than no meter, and the API is the only thing that knows the answer.
pub fn fixture(now_ms: i64) -> Option<Usage> {
    let path = std::env::var_os(USAGE_FILE_ENV)?;
    parse(&std::fs::read(path).ok()?, now_ms)
}

/// Parse usage out of a file in the shape of `~/.claude.json`.
///
/// Every step is fallible and every failure is `None`: this reads an
/// undocumented field of a file owned by another program, so a shape that is
/// not what is expected must leave the widget without a meter rather than
/// taking anything down.
pub fn parse(bytes: &[u8], now_ms: i64) -> Option<Usage> {
    let root: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let utilization = root.get("cachedUsageUtilization")?.get("utilization")?;
    parse_utilization(utilization, now_ms)
}

/// Parse usage out of a utilization object — the value the API returns and the
/// value the file caches verbatim under `cachedUsageUtilization.utilization`.
///
/// Shared so the live fetch in `crate::usage_api` and the cache read here agree
/// on every rule, the lapsed-window one included.
pub fn parse_utilization(utilization: &serde_json::Value, now_ms: i64) -> Option<Usage> {
    let five_hour = utilization.get("five_hour")?;

    let percent = five_hour.get("utilization")?.as_f64()?;
    // NaN would survive `as u8` as 0 and read as a fresh window.
    if !percent.is_finite() {
        return None;
    }
    let percent = percent.round().clamp(0.0, 100.0) as u8;

    let resets_at_ms = crate::rfc3339::epoch_ms(five_hour.get("resets_at")?.as_str()?)?;
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
    fn no_fixture_means_no_meter() {
        // Unset is the normal case, and it must not fall back to the real file:
        // `usage_api` is the only source now.
        std::env::remove_var(USAGE_FILE_ENV);
        assert_eq!(fixture(NOON), None);
    }

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
}
