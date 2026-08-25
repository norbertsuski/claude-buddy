use std::collections::HashMap;

/// How much newer than its registry entry a process may be and still be
/// considered the same one. Measured against live sessions the two agree within
/// about 1.5s; 2 minutes leaves room for a slow bootstrap.
pub const START_TOLERANCE_MS: i64 = 120_000;

/// Whether a session's process is still running.
///
/// A pid alone is not sufficient evidence: pid numbers are recycled, and a
/// recycled number would otherwise present a dead session as live. Callers pass
/// the registry's `startedAt` so implementations can confirm identity.
///
/// Identity is deliberately checked against an **epoch timestamp**, not against
/// the registry's `procStart` string. Claude Code writes `procStart` in a
/// different timezone than `ps -o lstart=` prints (observed two hours apart on
/// a CEST machine), so comparing those strings marks every live session dead.
///
/// The comparison is one-sided. A recycled pid belongs to a process that
/// started *after* the entry was written, so only that direction is
/// disqualifying. A process older than its entry is the same process: Claude
/// Code adopts pre-forked spares, so an entry's `startedAt` can be hours after
/// its process began — a symmetric tolerance reported those as dead.
pub trait PidLiveness {
    fn is_alive(&self, pid: i32, started_at_ms: Option<i64>, now_ms: i64) -> bool;
}

/// Parse `ps -o etime=` output into seconds. Formats: `MM:SS`, `HH:MM:SS`,
/// `DD-HH:MM:SS`. Elapsed time is used rather than a start timestamp because it
/// carries no timezone at all.
pub fn parse_etime(raw: &str) -> Option<i64> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }

    let (days, rest) = match text.split_once('-') {
        Some((d, rest)) => (d.trim().parse::<i64>().ok()?, rest),
        None => (0, text),
    };

    let mut parts = Vec::new();
    for piece in rest.split(':') {
        parts.push(piece.trim().parse::<i64>().ok()?);
    }
    let (hours, minutes, seconds) = match parts.as_slice() {
        [m, s] => (0, *m, *s),
        [h, m, s] => (*h, *m, *s),
        _ => return None,
    };

    Some(days * 86_400 + hours * 3_600 + minutes * 60 + seconds)
}

/// Real liveness, via `kill(pid, 0)` plus `ps -o etime=`.
pub struct SysLiveness;

impl SysLiveness {
    /// Epoch millis at which `pid` started, derived from its elapsed time.
    fn process_start_ms(pid: i32, now_ms: i64) -> Option<i64> {
        let out = std::process::Command::new("ps")
            .args(["-o", "etime=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let elapsed = parse_etime(&String::from_utf8_lossy(&out.stdout))?;
        Some(now_ms - elapsed * 1000)
    }
}

impl PidLiveness for SysLiveness {
    fn is_alive(&self, pid: i32, started_at_ms: Option<i64>, now_ms: i64) -> bool {
        if pid <= 0 {
            return false;
        }

        // EPERM means the process exists but belongs to another user, which
        // still counts as alive.
        let signalled = unsafe { libc::kill(pid, 0) };
        let exists = signalled == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        if !exists {
            return false;
        }

        match started_at_ms {
            None => true,
            Some(expected) => match Self::process_start_ms(pid, now_ms) {
                // If `ps` cannot answer, trust the signal probe rather than
                // declaring a live session dead.
                None => true,
                Some(actual) => actual <= expected + START_TOLERANCE_MS,
            },
        }
    }
}

/// Test double. `with_alive_any_start` registers a pid whose start time always
/// matches, for tests that do not care about pid reuse.
pub struct FakeLiveness {
    alive: HashMap<i32, Option<i64>>,
}

impl FakeLiveness {
    pub fn new() -> Self {
        Self { alive: HashMap::new() }
    }

    pub fn with_alive(mut self, pid: i32, started_at_ms: i64) -> Self {
        self.alive.insert(pid, Some(started_at_ms));
        self
    }

    pub fn with_alive_any_start(mut self, pid: i32) -> Self {
        self.alive.insert(pid, None);
        self
    }
}

impl Default for FakeLiveness {
    fn default() -> Self {
        Self::new()
    }
}

impl PidLiveness for FakeLiveness {
    fn is_alive(&self, pid: i32, started_at_ms: Option<i64>, _now_ms: i64) -> bool {
        match self.alive.get(&pid) {
            None => false,
            Some(None) => true,
            Some(Some(registered)) => match started_at_ms {
                None => true,
                Some(expected) => *registered <= expected + START_TOLERANCE_MS,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_787_662_300_000;

    #[test]
    fn fake_reports_registered_pids_alive() {
        let fake = FakeLiveness::new().with_alive(7952, NOW - 60_000);
        assert!(fake.is_alive(7952, Some(NOW - 60_000), NOW));
    }

    #[test]
    fn fake_reports_unregistered_pids_dead() {
        let fake = FakeLiveness::new().with_alive(7952, NOW - 60_000);
        assert!(!fake.is_alive(1234, Some(NOW - 60_000), NOW));
    }

    #[test]
    fn recycled_pid_reads_as_dead() {
        // Same pid number, but the process started an hour after the entry was
        // written: the original session is gone and the number was reused.
        let fake = FakeLiveness::new().with_alive(7952, NOW - 60_000);
        assert!(!fake.is_alive(7952, Some(NOW - 3_660_000), NOW));
    }

    #[test]
    fn a_process_older_than_its_entry_is_still_the_same_process() {
        // Regression: Claude Code adopts pre-forked spares, so a `bg` entry's
        // startedAt can be hours after its process began. A symmetric tolerance
        // reported those as dead, and the widget showed them as "died".
        let fake = FakeLiveness::new().with_alive(20426, NOW - 4 * 3_600_000);
        assert!(fake.is_alive(20426, Some(NOW - 60_000), NOW));
    }

    #[test]
    fn small_clock_differences_still_match() {
        // The registry records its own startedAt a beat after the process began.
        let fake = FakeLiveness::new().with_alive(7952, NOW - 60_000);
        assert!(fake.is_alive(7952, Some(NOW - 60_000 + 1_500), NOW));
    }

    #[test]
    fn a_difference_beyond_the_tolerance_does_not_match() {
        let fake = FakeLiveness::new().with_alive(7952, NOW);
        assert!(!fake.is_alive(7952, Some(NOW - START_TOLERANCE_MS - 1), NOW));
    }

    #[test]
    fn missing_started_at_falls_back_to_pid_only() {
        let fake = FakeLiveness::new().with_alive(7952, NOW - 60_000);
        assert!(fake.is_alive(7952, None, NOW));
    }

    #[test]
    fn etime_parses_minutes_and_seconds() {
        assert_eq!(parse_etime("   03:26"), Some(3 * 60 + 26));
    }

    #[test]
    fn etime_parses_hours() {
        assert_eq!(parse_etime("08:55:20"), Some(8 * 3600 + 55 * 60 + 20));
    }

    #[test]
    fn etime_parses_days() {
        assert_eq!(parse_etime("2-01:02:03"), Some(2 * 86400 + 3600 + 2 * 60 + 3));
    }

    #[test]
    fn etime_rejects_junk_and_emptiness() {
        assert_eq!(parse_etime(""), None);
        assert_eq!(parse_etime("not-a-time"), None);
        assert_eq!(parse_etime("1:2:3:4"), None);
    }

    #[test]
    fn sys_liveness_sees_our_own_process() {
        let own = std::process::id() as i32;
        assert!(SysLiveness.is_alive(own, None, NOW));
    }

    #[test]
    fn sys_liveness_rejects_an_impossible_pid() {
        assert!(!SysLiveness.is_alive(-1, None, NOW));
    }

    #[test]
    fn sys_liveness_accepts_our_own_process_against_a_real_clock() {
        // Regression: the previous implementation compared the registry's
        // localized `procStart` string against `ps -o lstart=`, which differ by
        // the machine's UTC offset. Every live session read as dead.
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let own = std::process::id() as i32;
        let start = SysLiveness::process_start_ms(own, now_ms).expect("ps should answer");

        assert!(SysLiveness.is_alive(own, Some(start), now_ms));
    }

    #[test]
    fn sys_liveness_accepts_a_process_older_than_its_entry() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let own = std::process::id() as i32;

        // Entry written "now", process began earlier: the spare-adoption case.
        assert!(SysLiveness.is_alive(own, Some(now_ms), now_ms));
    }

    #[test]
    fn sys_liveness_rejects_our_pid_with_a_start_time_far_in_the_past() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let own = std::process::id() as i32;

        assert!(!SysLiveness.is_alive(own, Some(now_ms - 30 * 86_400_000), now_ms));
    }
}
