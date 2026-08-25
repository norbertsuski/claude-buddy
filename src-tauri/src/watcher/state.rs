use serde::Serialize;

use crate::watcher::activity::ActivityProbe;
use crate::watcher::liveness::PidLiveness;
use crate::watcher::registry::RegistryFile;

/// Idle sessions older than this read as `Paused`.
pub const PAUSED_THRESHOLD_MS: i64 = 10 * 60 * 1000;

/// A statusless session whose transcript was touched this recently is treated
/// as working. Tool results land far more often than this while a session runs.
pub const BUSY_WINDOW_MS: i64 = 30 * 1000;

/// How long a crashed session stays on the list. Its registry file lingers with
/// a dead pid, and clawde-buddy never unlinks anything under `~/.claude`, so the
/// entry ages out of the display instead.
pub const DEAD_RETENTION_MS: i64 = 5 * 60 * 1000;

/// Entrypoints the user can actually answer. Everything else — notably
/// `sdk-cli`, which is plugin machinery — is dropped before any other layer
/// sees it, so no alert can ever fire for a session the user cannot reach.
pub const ALLOWED_ENTRYPOINTS: [&str; 2] = ["cli", "claude-desktop"];

/// Session kinds worth showing. `sdk` entries are library callers, not sessions.
pub const SHOWN_KINDS: [&str; 2] = ["interactive", "bg"];

/// Whether this entry is a background job rather than a session.
///
/// Mirrors Claude Code's own peer listing: a `bg` entry carrying a `jobId` is a
/// job or a pre-forked spare owned by a session. They are shown demoted, or
/// hidden entirely, but never counted as sessions.
pub fn is_background_job(kind: Option<&str>, job_id: Option<&str>) -> bool {
    kind == Some("bg") && job_id.is_some()
}

/// Whether this entry belongs in the widget at all.
pub fn is_shown(kind: Option<&str>, job_id: Option<&str>, include_background: bool) -> bool {
    match kind {
        Some(k) if SHOWN_KINDS.contains(&k) => {
            include_background || !is_background_job(kind, job_id)
        }
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Waiting,
    Busy,
    Idle,
    Paused,
    Dead,
}

impl SessionState {
    /// Display order: what needs the user comes first.
    fn rank(self) -> u8 {
        match self {
            SessionState::Waiting => 0,
            SessionState::Busy => 1,
            SessionState::Idle => 2,
            SessionState::Paused => 3,
            SessionState::Dead => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub pid: i32,
    pub session_id: String,
    pub name: String,
    pub cwd: String,
    pub entrypoint: String,
    /// The registry's `waitingFor`, present only while `Waiting`.
    pub state: SessionState,
    pub detail: Option<String>,
    /// Age of the current state. Falls back to session age when the registry
    /// has not recorded a status time.
    pub elapsed_ms: i64,
    pub uptime_ms: i64,
    /// Absolute epoch time the current state began. The frontend derives a
    /// live elapsed value from this: `fingerprint` deliberately ignores
    /// clock-derived fields, so `elapsed_ms` is only refreshed when state
    /// changes and is stale for anything sitting still.
    pub status_time_ms: i64,
    /// Absolute epoch time the session started.
    pub started_at_ms: i64,
    /// A background job or subagent, not a session the user answers.
    pub background: bool,
}

/// Clock skew and stale files can both produce timestamps in the future.
/// Render those as zero rather than as a negative age.
fn age(now_ms: i64, then_ms: i64) -> i64 {
    (now_ms - then_ms).max(0)
}

fn display_name(file: &RegistryFile) -> String {
    if let Some(name) = file.name.as_deref().filter(|n| !n.is_empty()) {
        return name.to_string();
    }
    std::path::Path::new(&file.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Derive every session's state. Pure: all time and all liveness are injected,
/// so the whole state machine is testable without a filesystem or a clock.
pub fn snapshot(
    files: &[RegistryFile],
    liveness: &dyn PidLiveness,
    activity: &dyn ActivityProbe,
    now_ms: i64,
    paused_threshold_ms: i64,
    include_background: bool,
) -> Vec<SessionSnapshot> {
    let mut out: Vec<SessionSnapshot> = files
        .iter()
        .filter(|f| {
            f.entrypoint
                .as_deref()
                .is_some_and(|e| ALLOWED_ENTRYPOINTS.contains(&e))
                && is_shown(f.kind.as_deref(), f.job_id.as_deref(), include_background)
        })
        .map(|f| {
            // Only `cli` sessions report status. For the rest, transcript
            // activity is the only evidence that anything is happening.
            let reported_status = f.status_updated_at.is_some();
            let last_activity = if reported_status {
                None
            } else {
                activity.last_activity_ms(&f.cwd, &f.session_id)
            };
            let status_time = f
                .status_updated_at
                .or(last_activity)
                .unwrap_or(f.started_at);
            let elapsed_ms = age(now_ms, status_time);
            let alive = liveness.is_alive(f.pid, Some(f.started_at), now_ms);

            let state = if !alive {
                SessionState::Dead
            } else {
                match f.status.as_deref() {
                    Some("waiting") => SessionState::Waiting,
                    Some("busy") => SessionState::Busy,
                    // Recent transcript writes mean the session is working,
                    // even though it never says so.
                    _ if last_activity.is_some() && elapsed_ms < BUSY_WINDOW_MS => {
                        SessionState::Busy
                    }
                    _ if elapsed_ms >= paused_threshold_ms => SessionState::Paused,
                    _ => SessionState::Idle,
                }
            };

            SessionSnapshot {
                pid: f.pid,
                session_id: f.session_id.clone(),
                name: display_name(f),
                cwd: f.cwd.clone(),
                entrypoint: f.entrypoint.clone().unwrap_or_default(),
                state,
                detail: match state {
                    SessionState::Waiting => f.waiting_for.clone(),
                    _ => None,
                },
                elapsed_ms,
                uptime_ms: age(now_ms, f.started_at),
                status_time_ms: status_time,
                started_at_ms: f.started_at,
                background: is_background_job(f.kind.as_deref(), f.job_id.as_deref()),
            }
        })
        // A crash is worth showing once, not forever.
        .filter(|s| s.state != SessionState::Dead || s.elapsed_ms <= DEAD_RETENTION_MS)
        .collect();

    out.sort_by(|a, b| {
        a.state
            .rank()
            .cmp(&b.state.rank())
            .then(b.uptime_ms.cmp(&a.uptime_ms))
            .then(a.pid.cmp(&b.pid))
    });

    group_jobs_with_parents(out)
}

/// Reorder so each background job follows the session it belongs to.
///
/// Jobs are matched by working directory, which is the only link the registry
/// offers — a job carries its parent's `cwd`. Jobs whose parent is not listed
/// (hidden, or already gone) trail at the end rather than vanishing.
pub fn group_jobs_with_parents(sessions: Vec<SessionSnapshot>) -> Vec<SessionSnapshot> {
    let (own, jobs): (Vec<_>, Vec<_>) = sessions.into_iter().partition(|s| !s.background);

    let mut grouped = Vec::with_capacity(own.len() + jobs.len());
    let mut placed = vec![false; jobs.len()];

    for session in own {
        let cwd = session.cwd.clone();
        grouped.push(session);
        for (i, job) in jobs.iter().enumerate() {
            if !placed[i] && job.cwd == cwd {
                grouped.push(job.clone());
                placed[i] = true;
            }
        }
    }

    for (i, job) in jobs.into_iter().enumerate() {
        if !placed[i] {
            grouped.push(job);
        }
    }

    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::activity::{FakeActivity, NoActivity};
    use crate::watcher::liveness::FakeLiveness;
    use crate::watcher::registry::RegistryFile;

    const NOW: i64 = 1_787_662_300_000;
    const START: &str = "Tue Aug 25 05:53:49 2026";

    fn file(pid: i32, entrypoint: &str) -> RegistryFile {
        RegistryFile {
            pid,
            session_id: format!("session-{pid}"),
            cwd: format!("/Users/n/Code/project-{pid}"),
            started_at: NOW - 60_000,
            proc_start: Some(START.to_string()),
            entrypoint: Some(entrypoint.to_string()),
            kind: Some("interactive".to_string()),
            job_id: None,
            name: Some(format!("project-{pid}")),
            status: None,
            status_updated_at: None,
            waiting_for: None,
        }
    }

    fn alive(pid: i32) -> FakeLiveness {
        FakeLiveness::new().with_alive(pid, NOW - 60_000)
    }

    #[test]
    fn waiting_status_yields_waiting_with_detail() {
        let mut f = file(1, "cli");
        f.status = Some("waiting".into());
        f.waiting_for = Some("input needed".into());
        f.status_updated_at = Some(NOW - 6 * 60_000);

        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].state, SessionState::Waiting);
        assert_eq!(out[0].detail.as_deref(), Some("input needed"));
        assert_eq!(out[0].elapsed_ms, 6 * 60_000);
    }

    #[test]
    fn busy_status_yields_busy_with_no_detail() {
        let mut f = file(1, "cli");
        f.status = Some("busy".into());
        f.status_updated_at = Some(NOW - 3 * 60_000);

        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out[0].state, SessionState::Busy);
        assert_eq!(out[0].detail, None);
    }

    #[test]
    fn absent_status_within_threshold_yields_idle() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);
        assert_eq!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true)[0].state, SessionState::Idle);
    }

    #[test]
    fn idle_status_word_is_treated_as_idle() {
        let mut f = file(1, "cli");
        f.status = Some("idle".into());
        f.status_updated_at = Some(NOW - 60_000);
        assert_eq!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true)[0].state, SessionState::Idle);
    }

    #[test]
    fn running_status_word_is_treated_as_idle() {
        let mut f = file(1, "cli");
        f.status = Some("running".into());
        f.status_updated_at = Some(NOW - 60_000);
        assert_eq!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true)[0].state, SessionState::Idle);
    }

    #[test]
    fn idle_past_threshold_yields_paused() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 11 * 60_000);
        assert_eq!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true)[0].state, SessionState::Paused);
    }

    #[test]
    fn paused_boundary_is_inclusive() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - PAUSED_THRESHOLD_MS);
        assert_eq!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true)[0].state, SessionState::Paused);
    }

    #[test]
    fn busy_never_becomes_paused_however_stale() {
        let mut f = file(1, "cli");
        f.status = Some("busy".into());
        f.status_updated_at = Some(NOW - 60 * 60_000);
        assert_eq!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true)[0].state, SessionState::Busy);
    }

    #[test]
    fn waiting_never_becomes_paused_however_stale() {
        let mut f = file(1, "cli");
        f.status = Some("waiting".into());
        f.waiting_for = Some("input needed".into());
        f.status_updated_at = Some(NOW - 60 * 60_000);
        assert_eq!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true)[0].state, SessionState::Waiting);
    }

    #[test]
    fn dead_process_yields_dead_regardless_of_status() {
        let mut f = file(1, "cli");
        f.status = Some("busy".into());
        let out = snapshot(&[f], &FakeLiveness::new(), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);
        assert_eq!(out[0].state, SessionState::Dead);
    }

    #[test]
    fn sdk_cli_sessions_are_filtered_out() {
        let files = vec![file(1, "cli"), file(2, "sdk-cli"), file(3, "claude-desktop")];
        let live = FakeLiveness::new()
            .with_alive_any_start(1)
            .with_alive_any_start(2)
            .with_alive_any_start(3);

        let out = snapshot(&files, &live, &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![1, 3]);
    }

    fn job(pid: i32) -> RegistryFile {
        let mut f = file(pid, "cli");
        f.kind = Some("bg".into());
        f.job_id = Some(format!("job-{pid}"));
        f
    }

    #[test]
    fn a_job_follows_the_session_it_belongs_to() {
        // Two sessions, and a job whose cwd matches the second one. State
        // ordering alone would not put it there.
        let mut first = file(1, "cli");
        first.status = Some("waiting".into());
        let mut second = file(2, "cli");
        second.status = Some("busy".into());
        let mut owned = job(3);
        owned.cwd = second.cwd.clone();

        let live = FakeLiveness::new()
            .with_alive_any_start(1)
            .with_alive_any_start(2)
            .with_alive_any_start(3);
        let out = snapshot(&[owned, first, second], &live, &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(out[2].background);
    }

    #[test]
    fn a_job_whose_parent_is_not_listed_trails_at_the_end() {
        let mut orphan = job(9);
        orphan.cwd = "/Users/n/Code/somewhere-else".into();

        let live = FakeLiveness::new().with_alive_any_start(1).with_alive_any_start(9);
        let out = snapshot(&[orphan, file(1, "cli")], &live, &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![1, 9]);
    }

    #[test]
    fn several_jobs_under_one_session_stay_together() {
        let mut a = job(3);
        let mut b = job(4);
        let parent = file(1, "cli");
        a.cwd = parent.cwd.clone();
        b.cwd = parent.cwd.clone();

        let live = FakeLiveness::new()
            .with_alive_any_start(1)
            .with_alive_any_start(3)
            .with_alive_any_start(4);
        let out = snapshot(&[a, b, parent], &live, &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![1, 3, 4]);
    }

    #[test]
    fn background_jobs_are_dropped_when_the_setting_is_off() {
        let live = FakeLiveness::new().with_alive_any_start(1).with_alive_any_start(2);

        let out = snapshot(&[job(1), file(2, "cli")], &live, &NoActivity, NOW, PAUSED_THRESHOLD_MS, false);

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn a_session_is_never_marked_background() {
        let out = snapshot(&[file(1, "cli")], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);
        assert!(!out[0].background);
    }

    #[test]
    fn a_background_entry_without_a_job_id_is_kept() {
        let mut bg = file(1, "cli");
        bg.kind = Some("bg".into());
        assert_eq!(snapshot(&[bg], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true).len(), 1);
    }

    #[test]
    fn sdk_kind_entries_are_filtered_out() {
        let mut f = file(1, "cli");
        f.kind = Some("sdk".into());
        assert!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true).is_empty());
    }

    #[test]
    fn sessions_with_no_kind_are_filtered_out() {
        let mut f = file(1, "cli");
        f.kind = None;
        assert!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true).is_empty());
    }

    #[test]
    fn sessions_with_no_entrypoint_are_filtered_out() {
        let mut f = file(1, "cli");
        f.entrypoint = None;
        assert!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true).is_empty());
    }

    #[test]
    fn elapsed_falls_back_to_started_at_when_status_time_is_absent() {
        let f = file(1, "cli");
        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);
        assert_eq!(out[0].elapsed_ms, 60_000);
        assert_eq!(out[0].uptime_ms, 60_000);
    }

    #[test]
    fn future_timestamps_clamp_elapsed_to_zero() {
        // Clock skew must not render as a negative age.
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW + 3 * 60_000);
        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);
        assert_eq!(out[0].elapsed_ms, 0);
        assert_eq!(out[0].state, SessionState::Idle);
    }

    #[test]
    fn missing_name_falls_back_to_the_cwd_basename() {
        let mut f = file(1, "cli");
        f.name = None;
        assert_eq!(snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true)[0].name, "project-1");
    }

    #[test]
    fn ordering_is_waiting_then_busy_then_idle_then_paused_then_dead() {
        let mut waiting = file(10, "cli");
        waiting.status = Some("waiting".into());
        let mut busy = file(20, "cli");
        busy.status = Some("busy".into());
        let idle = file(30, "cli");
        let mut paused = file(40, "cli");
        paused.status_updated_at = Some(NOW - 30 * 60_000);
        let dead = file(50, "cli");

        let live = FakeLiveness::new()
            .with_alive_any_start(10)
            .with_alive_any_start(20)
            .with_alive_any_start(30)
            .with_alive_any_start(40);

        let out = snapshot(&[dead, paused, idle, busy, waiting], &live, &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![10, 20, 30, 40, 50]);
    }

    #[test]
    fn same_state_sessions_order_by_start_time_oldest_first() {
        // Pin status time so both stay Idle; only session age differs.
        let mut older = file(10, "cli");
        older.started_at = NOW - 600_000;
        older.status_updated_at = Some(NOW - 30_000);
        let mut newer = file(20, "cli");
        newer.started_at = NOW - 60_000;
        newer.status_updated_at = Some(NOW - 30_000);

        let live = FakeLiveness::new().with_alive_any_start(10).with_alive_any_start(20);
        let out = snapshot(&[newer, older], &live, &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![10, 20]);
    }

    #[test]
    fn a_localized_proc_start_string_does_not_affect_liveness() {
        // Regression: Claude Code writes procStart in a different timezone than
        // `ps -o lstart=` prints. Comparing those strings marked every live
        // session dead, and dead-retention then dropped them all, so the widget
        // rendered "no sessions" with three sessions running.
        let mut f = file(1, "cli");
        f.proc_start = Some("Tue Aug 25 03:53:49 2026".into());
        f.status = Some("busy".into());

        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out.len(), 1, "session must not be dropped as dead");
        assert_eq!(out[0].state, SessionState::Busy);
    }

    #[test]
    fn a_statusless_session_with_fresh_transcript_writes_is_busy() {
        // Regression: only cli sessions report status, so a claude-desktop
        // session the user was actively working in aged into `paused`.
        let f = file(1, "claude-desktop");
        let probe = FakeActivity::new().with("session-1", NOW - 2_000);

        let out = snapshot(&[f], &alive(1), &probe, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out[0].state, SessionState::Busy);
        assert_eq!(out[0].elapsed_ms, 2_000);
    }

    #[test]
    fn a_statusless_session_quiet_for_a_while_is_idle() {
        let f = file(1, "claude-desktop");
        let probe = FakeActivity::new().with("session-1", NOW - 5 * 60_000);

        let out = snapshot(&[f], &alive(1), &probe, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out[0].state, SessionState::Idle);
    }

    #[test]
    fn a_statusless_session_quiet_past_the_threshold_is_paused() {
        let f = file(1, "claude-desktop");
        let probe = FakeActivity::new().with("session-1", NOW - 20 * 60_000);

        assert_eq!(
            snapshot(&[f], &alive(1), &probe, NOW, PAUSED_THRESHOLD_MS, true)[0].state,
            SessionState::Paused
        );
    }

    #[test]
    fn a_statusless_session_with_no_transcript_falls_back_to_session_age() {
        let f = file(1, "claude-desktop");

        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out[0].state, SessionState::Idle);
        assert_eq!(out[0].elapsed_ms, 60_000);
    }

    #[test]
    fn a_reported_status_beats_transcript_activity() {
        // A cli session that says it is waiting stays waiting even though its
        // transcript was just written.
        let mut f = file(1, "cli");
        f.status = Some("waiting".into());
        f.waiting_for = Some("input needed".into());
        f.status_updated_at = Some(NOW - 4 * 60_000);
        let probe = FakeActivity::new().with("session-1", NOW - 1_000);

        let out = snapshot(&[f], &alive(1), &probe, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out[0].state, SessionState::Waiting);
        assert_eq!(out[0].elapsed_ms, 4 * 60_000);
    }

    #[test]
    fn a_recently_dead_session_is_retained() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);
        let out = snapshot(&[f], &FakeLiveness::new(), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);
        assert_eq!(out[0].state, SessionState::Dead);
    }

    #[test]
    fn a_long_dead_session_drops_off_the_list() {
        // Claude Code prunes stale registry files itself; clawde-buddy never
        // unlinks them, so it stops showing them instead.
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - DEAD_RETENTION_MS - 1);
        assert!(snapshot(&[f], &FakeLiveness::new(), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true).is_empty());
    }

    #[test]
    fn dead_retention_uses_started_at_when_no_status_time_exists() {
        let mut f = file(1, "cli");
        f.started_at = NOW - DEAD_RETENTION_MS - 1;
        f.status_updated_at = None;
        assert!(snapshot(&[f], &FakeLiveness::new(), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true).is_empty());
    }

    #[test]
    fn snapshot_carries_absolute_timestamps_alongside_derived_ages() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 6 * 60_000);

        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out[0].status_time_ms, NOW - 6 * 60_000);
        assert_eq!(out[0].started_at_ms, NOW - 60_000);
        assert_eq!(out[0].elapsed_ms, 6 * 60_000);
    }

    #[test]
    fn status_time_falls_back_to_started_at_when_absent() {
        let f = file(1, "cli");
        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);
        assert_eq!(out[0].status_time_ms, NOW - 60_000);
        assert_eq!(out[0].started_at_ms, NOW - 60_000);
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(snapshot(&[], &FakeLiveness::new(), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true).is_empty());
    }

    #[test]
    fn snapshot_serializes_camel_case_with_lowercase_state() {
        let mut f = file(1, "cli");
        f.status = Some("waiting".into());
        f.waiting_for = Some("input needed".into());
        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        let json = serde_json::to_value(&out[0]).unwrap();
        assert_eq!(json["state"], "waiting");
        assert_eq!(json["sessionId"], "session-1");
        assert_eq!(json["elapsedMs"], 60_000);
    }
}
