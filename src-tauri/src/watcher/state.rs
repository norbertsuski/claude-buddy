use std::collections::HashMap;

use serde::Serialize;

use crate::watcher::liveness::PidLiveness;
use crate::watcher::probes::{ActivityProbe, BlockedProbe};
use crate::watcher::session::RawSession;
use crate::watcher::task::{Task, TaskKind, TaskProbe, TaskStatus};
use crate::watcher::title::TitleProbe;
use crate::watcher::working::WorkProbe;

/// Idle sessions older than this read as `Paused`.
pub const PAUSED_THRESHOLD_MS: i64 = 10 * 60 * 1000;

/// A statusless session whose transcript was touched this recently is treated
/// as working. Tool results land far more often than this while a session runs.
pub const BUSY_WINDOW_MS: i64 = 30 * 1000;

/// How long a crashed session stays on the list. Its registry file lingers with
/// a dead pid, and claude-buddy never unlinks anything under `~/.claude`, so the
/// entry ages out of the display instead.
pub const DEAD_RETENTION_MS: i64 = 5 * 60 * 1000;

/// How long a finished task stays in the snapshot.
///
/// Long enough for the alert diff to see the `Running`-to-terminal edge, which
/// it would otherwise miss: a finishing task wakes its session, so the same
/// tick usually moves the session's own state as well. Mirrors
/// `DEAD_RETENTION_MS` — a thing that happened once is worth showing once.
pub const TERMINAL_TASK_RETENTION_MS: i64 = 60 * 1000;

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
    Tasking,
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
            SessionState::Tasking => 2,
            SessionState::Idle => 3,
            SessionState::Paused => 4,
            SessionState::Dead => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub pid: i32,
    pub session_id: String,
    pub name: String,
    /// What the session calls itself, from the transcript. Absent until Claude
    /// Code has titled it, which is why the row falls back to `name`.
    pub title: Option<String>,
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
    /// Background work this session is waiting on: background shells, watches,
    /// subagents, and the registry jobs that share its working directory.
    /// Finished tasks linger for `TERMINAL_TASK_RETENTION_MS` so the alert diff
    /// can see them end.
    pub tasks: Vec<Task>,
}

/// Clock skew and stale files can both produce timestamps in the future.
/// Render those as zero rather than as a negative age.
fn age(now_ms: i64, then_ms: i64) -> i64 {
    (now_ms - then_ms).max(0)
}

fn display_name(file: &RawSession) -> String {
    if let Some(name) = file.name.as_deref().filter(|n| !n.is_empty()) {
        return name.to_string();
    }
    std::path::Path::new(&file.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// One derivation pass.
///
/// `dead_now` lists every session observed dead this tick, *including* those
/// the retention filter removed — the caller keys its first-seen-dead map off
/// this, and dropping the entry for a session that is still dead would make it
/// look newly dead again on the next tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapshotResult {
    pub sessions: Vec<SessionSnapshot>,
    pub dead_now: Vec<String>,
}

/// Whether the user could reach this entry at all.
fn allowed_entrypoint(file: &RawSession) -> bool {
    file.entrypoint
        .as_deref()
        .is_some_and(|e| ALLOWED_ENTRYPOINTS.contains(&e))
}

/// Whether this entry is a session in its own right: reachable, a shown kind,
/// and not a background job. The `snapshot` filter is this plus
/// `show_background_jobs`, which governs job rows only.
fn is_own_session(file: &RawSession) -> bool {
    allowed_entrypoint(file) && is_shown(file.kind.as_deref(), file.job_id.as_deref(), false)
}

/// Which session in a directory a job belongs to.
///
/// The oldest, and the lowest pid among equals. That is also the session
/// `group_jobs_with_parents` puts the job's *row* under: folding the job on
/// makes this one `Tasking`, which outranks the `Idle` or `Paused` the others
/// in the directory keep, so it sorts ahead of them.
fn job_parent<'a>(files: &'a [RawSession], cwd: &str) -> Option<&'a RawSession> {
    files
        .iter()
        .filter(|f| is_own_session(f) && f.cwd == cwd)
        .min_by_key(|f| (f.started_at, f.pid))
}

/// Every live registry job, keyed by the session it counts as a task on.
///
/// Jobs are separate processes and appear in no transcript, so this is the only
/// place they can come from. A job carries its parent's `cwd`, the only link
/// the registry offers — but a directory can hold several sessions and a job is
/// one session's work, so it goes to exactly one of them, as
/// `group_jobs_with_parents` does for the row.
///
/// Taken from the unfiltered registry deliberately, but only with respect to
/// `show_background_jobs`: that setting governs whether a job gets a row of its
/// own, not whether its parent is waiting on it. The entrypoint allowlist still
/// applies at both ends, so an `sdk-cli` plugin's job cannot reach a `cli`
/// session's task list.
fn job_tasks(
    files: &[RawSession],
    liveness: &dyn PidLiveness,
    now_ms: i64,
) -> HashMap<String, Vec<Task>> {
    let mut out: HashMap<String, Vec<Task>> = HashMap::new();
    for file in files.iter().filter(|f| {
        allowed_entrypoint(f)
            && is_background_job(f.kind.as_deref(), f.job_id.as_deref())
            && liveness.is_alive(f.pid, Some(f.started_at), now_ms)
    }) {
        let Some(parent) = job_parent(files, &file.cwd) else {
            continue;
        };
        out.entry(parent.session_id.clone())
            .or_default()
            .push(Task {
                id: file.job_id.clone().unwrap_or_default(),
                kind: TaskKind::Job,
                label: Some(display_name(file)),
                started_at_ms: file.started_at,
                ended_at_ms: None,
                status: TaskStatus::Running,
                // A job is a process, not a shell the session launched: it
                // writes no task output file, and its ending is its process
                // going away.
                output: None,
            });
    }
    out
}

/// What a tasking session's row says it is waiting on.
///
/// One task names itself; several are counted, because the popover lists them
/// and the row has no space to.
fn task_detail(tasks: &[Task]) -> String {
    let running: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Running)
        .collect();
    match running.as_slice() {
        [one] => one
            .label
            .clone()
            .unwrap_or_else(|| "1 task running".to_string()),
        many => format!("{} tasks running", many.len()),
    }
}

/// Derive every session's state. Pure: all time and all liveness are injected,
/// so the whole state machine is testable without a filesystem or a clock.
// Eleven parameters — the registry, six injected dependencies, the clock, two
// settings and the dead-since map — all of them load-bearing, is the price of
// keeping this function pure and its whole state machine testable.
#[allow(clippy::too_many_arguments)]
pub fn snapshot(
    files: &[RawSession],
    liveness: &dyn PidLiveness,
    activity: &dyn ActivityProbe,
    blocked: &dyn BlockedProbe,
    work: &dyn WorkProbe,
    tasks: &dyn TaskProbe,
    titles: &dyn TitleProbe,
    now_ms: i64,
    paused_threshold_ms: i64,
    include_background: bool,
    first_seen_dead: &HashMap<String, i64>,
) -> SnapshotResult {
    let jobs = job_tasks(files, liveness, now_ms);

    let derived: Vec<SessionSnapshot> = files
        .iter()
        .filter(|f| {
            allowed_entrypoint(f)
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

            // Statusless sessions never say they are waiting, so a transcript
            // holding an unanswered user-blocking tool call is the only
            // evidence there is. Sessions that do report status are left
            // alone: what they say beats what the transcript implies.
            let pending_prompt = if reported_status {
                None
            } else {
                blocked.pending_prompt(&f.cwd, &f.session_id)
            };

            // A tool call with no result yet is the session working, however
            // long the transcript has been quiet: the transcript is only
            // appended when the call finishes. Bounded by the paused threshold
            // so an interrupted turn, which leaves its call unanswered
            // forever, still settles rather than reading busy for good.
            let work_in_flight = !reported_status && work.in_flight(&f.cwd, &f.session_id);

            // The probe's own tasks, minus the ones that finished long enough
            // ago to have been alerted about, plus any registry job placed
            // under this session. A job entry gets no jobs of its own: it is
            // one.
            let mut session_tasks: Vec<Task> = tasks
                .tasks(&f.cwd, &f.session_id, f.started_at)
                .into_iter()
                .filter(|t| match t.ended_at_ms {
                    None => true,
                    Some(ended) => age(now_ms, ended) <= TERMINAL_TASK_RETENTION_MS,
                })
                .collect();
            if !is_background_job(f.kind.as_deref(), f.job_id.as_deref()) {
                if let Some(mine) = jobs.get(f.session_id.as_str()) {
                    session_tasks.extend(mine.iter().cloned());
                }
            }
            let has_running_task = session_tasks
                .iter()
                .any(|t| t.status == TaskStatus::Running);

            let state = if !alive {
                // Death outranks everything, including an unanswered question:
                // there is no longer anyone to answer.
                SessionState::Dead
            } else if pending_prompt.is_some() {
                SessionState::Waiting
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
                    _ if work_in_flight => SessionState::Busy,
                    _ => SessionState::Idle,
                }
            };

            // Only stillness becomes tasking. `Waiting` is the one state that
            // needs the user and must never be buried; `Busy` is the session
            // working on its own turn, which is the more immediate fact; and a
            // dead session is waiting on nothing.
            let state = match state {
                SessionState::Idle | SessionState::Paused if has_running_task => {
                    SessionState::Tasking
                }
                settled => settled,
            };

            SessionSnapshot {
                pid: f.pid,
                session_id: f.session_id.clone(),
                name: display_name(f),
                title: titles.title(&f.cwd, &f.session_id),
                cwd: f.cwd.clone(),
                entrypoint: f.entrypoint.clone().unwrap_or_default(),
                state,
                detail: match state {
                    SessionState::Waiting => pending_prompt.or_else(|| f.waiting_for.clone()),
                    SessionState::Tasking => Some(task_detail(&session_tasks)),
                    _ => None,
                },
                elapsed_ms,
                uptime_ms: age(now_ms, f.started_at),
                status_time_ms: status_time,
                started_at_ms: f.started_at,
                background: is_background_job(f.kind.as_deref(), f.job_id.as_deref()),
                tasks: session_tasks,
            }
        })
        .collect();

    // Every session seen dead, recorded before retention can remove any of them.
    let dead_now: Vec<String> = derived
        .iter()
        .filter(|s| s.state == SessionState::Dead)
        .map(|s| s.session_id.clone())
        .collect();

    // A crash is worth showing once, not forever. Measured from when death was
    // first observed: `statusUpdatedAt` is the age of the last status write,
    // which for a session that had been quiet a while is already past the
    // window, so it would be filtered out before it could ever be shown.
    let mut out: Vec<SessionSnapshot> = derived
        .into_iter()
        .filter(|s| {
            if s.state != SessionState::Dead {
                return true;
            }
            let since = first_seen_dead
                .get(&s.session_id)
                .copied()
                .unwrap_or(now_ms);
            age(now_ms, since) <= DEAD_RETENTION_MS
        })
        .collect();

    out.sort_by(|a, b| {
        a.state
            .rank()
            .cmp(&b.state.rank())
            .then(b.uptime_ms.cmp(&a.uptime_ms))
            .then(a.pid.cmp(&b.pid))
    });

    SnapshotResult {
        sessions: group_jobs_with_parents(out),
        dead_now,
    }
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
    use crate::watcher::blocked::{FakeBlocked, NoBlocked};
    use crate::watcher::liveness::FakeLiveness;
    use crate::watcher::tasks::{FakeTasks, NoTasks};
    use crate::watcher::title::{FakeTitle, NoTitle};
    use crate::watcher::working::{FakeWork, NoWork};

    const NOW: i64 = 1_787_662_300_000;
    const START: &str = "Tue Aug 25 05:53:49 2026";

    fn file(pid: i32, entrypoint: &str) -> RawSession {
        RawSession {
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

        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

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

        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Busy);
        assert_eq!(out[0].detail, None);
    }

    #[test]
    fn absent_status_within_threshold_yields_idle() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);
        assert_eq!(
            snapshot(
                &[f],
                &alive(1),
                &NoActivity,
                &NoBlocked,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Idle
        );
    }

    #[test]
    fn idle_status_word_is_treated_as_idle() {
        let mut f = file(1, "cli");
        f.status = Some("idle".into());
        f.status_updated_at = Some(NOW - 60_000);
        assert_eq!(
            snapshot(
                &[f],
                &alive(1),
                &NoActivity,
                &NoBlocked,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Idle
        );
    }

    #[test]
    fn running_status_word_is_treated_as_idle() {
        let mut f = file(1, "cli");
        f.status = Some("running".into());
        f.status_updated_at = Some(NOW - 60_000);
        assert_eq!(
            snapshot(
                &[f],
                &alive(1),
                &NoActivity,
                &NoBlocked,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Idle
        );
    }

    #[test]
    fn idle_past_threshold_yields_paused() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 11 * 60_000);
        assert_eq!(
            snapshot(
                &[f],
                &alive(1),
                &NoActivity,
                &NoBlocked,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Paused
        );
    }

    #[test]
    fn paused_boundary_is_inclusive() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - PAUSED_THRESHOLD_MS);
        assert_eq!(
            snapshot(
                &[f],
                &alive(1),
                &NoActivity,
                &NoBlocked,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Paused
        );
    }

    #[test]
    fn busy_never_becomes_paused_however_stale() {
        let mut f = file(1, "cli");
        f.status = Some("busy".into());
        f.status_updated_at = Some(NOW - 60 * 60_000);
        assert_eq!(
            snapshot(
                &[f],
                &alive(1),
                &NoActivity,
                &NoBlocked,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Busy
        );
    }

    #[test]
    fn waiting_never_becomes_paused_however_stale() {
        let mut f = file(1, "cli");
        f.status = Some("waiting".into());
        f.waiting_for = Some("input needed".into());
        f.status_updated_at = Some(NOW - 60 * 60_000);
        assert_eq!(
            snapshot(
                &[f],
                &alive(1),
                &NoActivity,
                &NoBlocked,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Waiting
        );
    }

    #[test]
    fn dead_process_yields_dead_regardless_of_status() {
        let mut f = file(1, "cli");
        f.status = Some("busy".into());
        let out = snapshot(
            &[f],
            &FakeLiveness::new(),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;
        assert_eq!(out[0].state, SessionState::Dead);
    }

    #[test]
    fn a_statusless_session_running_a_tool_is_busy_past_the_busy_window() {
        // The mtime blind spot: a long tool call appends nothing until it
        // finishes, so the transcript looks as quiet as an abandoned session.
        let f = file(1, "claude-desktop");
        let activity = FakeActivity::new().with("session-1", NOW - 4 * 60_000);

        let out = snapshot(
            &[f],
            &alive(1),
            &activity,
            &NoBlocked,
            &FakeWork::new().with("session-1"),
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Busy);
    }

    #[test]
    fn a_quiet_statusless_session_running_nothing_is_still_idle() {
        let f = file(1, "claude-desktop");
        let activity = FakeActivity::new().with("session-1", NOW - 4 * 60_000);

        let out = snapshot(
            &[f],
            &alive(1),
            &activity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Idle);
    }

    #[test]
    fn a_tool_call_left_unanswered_past_the_paused_threshold_still_pauses() {
        // An interrupted turn leaves its call unanswered for good. Without the
        // ceiling that session would read busy until it exits.
        let f = file(1, "claude-desktop");
        let activity = FakeActivity::new().with("session-1", NOW - 30 * 60_000);

        let out = snapshot(
            &[f],
            &alive(1),
            &activity,
            &NoBlocked,
            &FakeWork::new().with("session-1"),
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Paused);
    }

    #[test]
    fn a_pending_question_outranks_a_tool_call_in_flight() {
        let f = file(1, "claude-desktop");
        let activity = FakeActivity::new().with("session-1", NOW - 4 * 60_000);

        let out = snapshot(
            &[f],
            &alive(1),
            &activity,
            &FakeBlocked::new().with("session-1", "question pending"),
            &FakeWork::new().with("session-1"),
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Waiting);
        assert_eq!(out[0].detail.as_deref(), Some("question pending"));
    }

    #[test]
    fn a_reported_status_is_not_overridden_by_a_tool_call_in_flight() {
        // Sessions that report status are believed: what they say beats what
        // the transcript implies.
        let mut f = file(1, "cli");
        f.status = Some("waiting".into());
        f.status_updated_at = Some(NOW - 4 * 60_000);

        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &FakeWork::new().with("session-1"),
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Waiting);
    }

    #[test]
    fn sdk_cli_sessions_are_filtered_out() {
        let files = vec![
            file(1, "cli"),
            file(2, "sdk-cli"),
            file(3, "claude-desktop"),
        ];
        let live = FakeLiveness::new()
            .with_alive_any_start(1)
            .with_alive_any_start(2)
            .with_alive_any_start(3);

        let out = snapshot(
            &files,
            &live,
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![1, 3]);
    }

    fn job(pid: i32) -> RawSession {
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
        let out = snapshot(
            &[owned, first, second],
            &live,
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(out[2].background);
    }

    #[test]
    fn a_job_whose_parent_is_not_listed_trails_at_the_end() {
        let mut orphan = job(9);
        orphan.cwd = "/Users/n/Code/somewhere-else".into();

        let live = FakeLiveness::new()
            .with_alive_any_start(1)
            .with_alive_any_start(9);
        let out = snapshot(
            &[orphan, file(1, "cli")],
            &live,
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

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
        let out = snapshot(
            &[a, b, parent],
            &live,
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![1, 3, 4]);
    }

    #[test]
    fn background_jobs_are_dropped_when_the_setting_is_off() {
        let live = FakeLiveness::new()
            .with_alive_any_start(1)
            .with_alive_any_start(2);

        let out = snapshot(
            &[job(1), file(2, "cli")],
            &live,
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            false,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn a_session_is_never_marked_background() {
        let out = snapshot(
            &[file(1, "cli")],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;
        assert!(!out[0].background);
    }

    #[test]
    fn a_background_entry_without_a_job_id_is_kept() {
        let mut bg = file(1, "cli");
        bg.kind = Some("bg".into());
        assert_eq!(
            snapshot(
                &[bg],
                &alive(1),
                &NoActivity,
                &NoBlocked,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions
            .len(),
            1
        );
    }

    #[test]
    fn sdk_kind_entries_are_filtered_out() {
        let mut f = file(1, "cli");
        f.kind = Some("sdk".into());
        assert!(snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new()
        )
        .sessions
        .is_empty());
    }

    #[test]
    fn sessions_with_no_kind_are_filtered_out() {
        let mut f = file(1, "cli");
        f.kind = None;
        assert!(snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new()
        )
        .sessions
        .is_empty());
    }

    #[test]
    fn sessions_with_no_entrypoint_are_filtered_out() {
        let mut f = file(1, "cli");
        f.entrypoint = None;
        assert!(snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new()
        )
        .sessions
        .is_empty());
    }

    #[test]
    fn elapsed_falls_back_to_started_at_when_status_time_is_absent() {
        let f = file(1, "cli");
        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;
        assert_eq!(out[0].elapsed_ms, 60_000);
        assert_eq!(out[0].uptime_ms, 60_000);
    }

    #[test]
    fn future_timestamps_clamp_elapsed_to_zero() {
        // Clock skew must not render as a negative age.
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW + 3 * 60_000);
        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;
        assert_eq!(out[0].elapsed_ms, 0);
        assert_eq!(out[0].state, SessionState::Idle);
    }

    #[test]
    fn missing_name_falls_back_to_the_cwd_basename() {
        let mut f = file(1, "cli");
        f.name = None;
        assert_eq!(
            snapshot(
                &[f],
                &alive(1),
                &NoActivity,
                &NoBlocked,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .name,
            "project-1"
        );
    }

    #[test]
    fn a_titled_session_carries_its_title() {
        let f = file(1, "cli");
        let session_id = f.session_id.clone();
        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &FakeTitle::new().with(&session_id, "Rebase and conflicts"),
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].title.as_deref(), Some("Rebase and conflicts"));
        // The registry name is still carried: the popover shows both, and the
        // row needs something to fall back to.
        assert_eq!(out[0].name, "project-1");
    }

    #[test]
    fn an_untitled_session_has_no_title() {
        let out = snapshot(
            &[file(1, "cli")],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].title, None);
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

        let out = snapshot(
            &[dead, paused, idle, busy, waiting],
            &live,
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(
            out.iter().map(|s| s.pid).collect::<Vec<_>>(),
            vec![10, 20, 30, 40, 50]
        );
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

        let live = FakeLiveness::new()
            .with_alive_any_start(10)
            .with_alive_any_start(20);
        let out = snapshot(
            &[newer, older],
            &live,
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

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

        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out.len(), 1, "session must not be dropped as dead");
        assert_eq!(out[0].state, SessionState::Busy);
    }

    #[test]
    fn a_statusless_session_with_fresh_transcript_writes_is_busy() {
        // Regression: only cli sessions report status, so a claude-desktop
        // session the user was actively working in aged into `paused`.
        let f = file(1, "claude-desktop");
        let probe = FakeActivity::new().with("session-1", NOW - 2_000);

        let out = snapshot(
            &[f],
            &alive(1),
            &probe,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Busy);
        assert_eq!(out[0].elapsed_ms, 2_000);
    }

    #[test]
    fn a_statusless_session_quiet_for_a_while_is_idle() {
        let f = file(1, "claude-desktop");
        let probe = FakeActivity::new().with("session-1", NOW - 5 * 60_000);

        let out = snapshot(
            &[f],
            &alive(1),
            &probe,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Idle);
    }

    #[test]
    fn a_statusless_session_quiet_past_the_threshold_is_paused() {
        let f = file(1, "claude-desktop");
        let probe = FakeActivity::new().with("session-1", NOW - 20 * 60_000);

        assert_eq!(
            snapshot(
                &[f],
                &alive(1),
                &probe,
                &NoBlocked,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Paused
        );
    }

    #[test]
    fn a_statusless_session_with_no_transcript_falls_back_to_session_age() {
        let f = file(1, "claude-desktop");

        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

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

        let out = snapshot(
            &[f],
            &alive(1),
            &probe,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Waiting);
        assert_eq!(out[0].elapsed_ms, 4 * 60_000);
    }

    #[test]
    fn a_statusless_session_with_a_pending_prompt_is_waiting() {
        // Regression: Claude Desktop writes no status, statusUpdatedAt or
        // waitingFor at all, so a session blocked on AskUserQuestion read as
        // idle — grey — while it was in fact waiting on the user.
        let f = file(1, "claude-desktop");
        let activity = FakeActivity::new().with("session-1", NOW - 3 * 60_000);
        let blocked = FakeBlocked::new().with("session-1", "question pending");

        let out = snapshot(
            &[f],
            &alive(1),
            &activity,
            &blocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Waiting);
        assert_eq!(out[0].detail.as_deref(), Some("question pending"));
    }

    #[test]
    fn a_pending_prompt_beats_a_fresh_transcript() {
        // A transcript touched seconds ago would otherwise read as busy.
        let f = file(1, "claude-desktop");
        let activity = FakeActivity::new().with("session-1", NOW - 2_000);
        let blocked = FakeBlocked::new().with("session-1", "plan approval");

        let out = snapshot(
            &[f],
            &alive(1),
            &activity,
            &blocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Waiting);
        assert_eq!(out[0].detail.as_deref(), Some("plan approval"));
    }

    #[test]
    fn a_pending_prompt_beats_a_stale_transcript() {
        let f = file(1, "claude-desktop");
        let activity = FakeActivity::new().with("session-1", NOW - 30 * 60_000);
        let blocked = FakeBlocked::new().with("session-1", "question pending");

        let out = snapshot(
            &[f],
            &alive(1),
            &activity,
            &blocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Waiting);
    }

    #[test]
    fn a_statusless_session_with_no_pending_prompt_keeps_its_mtime_derived_state() {
        // Guard on the existing behaviour: the blocked probe must only ever
        // add a Waiting, never disturb what mtime already decided.
        let quiet = FakeBlocked::new();

        let busy = FakeActivity::new().with("session-1", NOW - 2_000);
        assert_eq!(
            snapshot(
                &[file(1, "claude-desktop")],
                &alive(1),
                &busy,
                &quiet,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Busy
        );

        let idle = FakeActivity::new().with("session-1", NOW - 5 * 60_000);
        assert_eq!(
            snapshot(
                &[file(1, "claude-desktop")],
                &alive(1),
                &idle,
                &quiet,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Idle
        );

        let paused = FakeActivity::new().with("session-1", NOW - 20 * 60_000);
        assert_eq!(
            snapshot(
                &[file(1, "claude-desktop")],
                &alive(1),
                &paused,
                &quiet,
                &NoWork,
                &NoTasks,
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new()
            )
            .sessions[0]
                .state,
            SessionState::Paused
        );
    }

    #[test]
    fn a_session_that_reports_status_ignores_the_blocked_probe() {
        // A cli session says what it is doing. A stale AskUserQuestion left in
        // its transcript must not override a reported busy.
        let mut f = file(1, "cli");
        f.status = Some("busy".into());
        f.status_updated_at = Some(NOW - 60_000);
        let blocked = FakeBlocked::new().with("session-1", "question pending");

        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &blocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Busy);
        assert_eq!(out[0].detail, None);
    }

    #[test]
    fn a_dead_statusless_session_with_a_pending_prompt_is_still_dead() {
        // Nobody is left to answer the question.
        let f = file(1, "claude-desktop");
        let blocked = FakeBlocked::new().with("session-1", "question pending");

        let out = snapshot(
            &[f],
            &FakeLiveness::new(),
            &NoActivity,
            &blocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Dead);
        assert_eq!(out[0].detail, None);
    }

    #[test]
    fn a_waiting_session_sorts_ahead_even_when_its_waiting_was_inferred() {
        let mut reported_busy = file(2, "cli");
        reported_busy.status = Some("busy".into());
        reported_busy.status_updated_at = Some(NOW - 60_000);
        let inferred = file(1, "claude-desktop");
        let blocked = FakeBlocked::new().with("session-1", "question pending");

        let live = FakeLiveness::new()
            .with_alive_any_start(1)
            .with_alive_any_start(2);
        let out = snapshot(
            &[reported_busy, inferred],
            &live,
            &NoActivity,
            &blocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn a_long_idle_session_that_dies_is_shown_and_not_swallowed() {
        // Regression: retention was measured from statusUpdatedAt, so a session
        // quiet for longer than the retention window was filtered out on the
        // very tick it was first seen dead — no red dot, and no died alert,
        // because diff_alerts only sees what survives this filter.
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 12 * 60_000);

        let out = snapshot(
            &[f],
            &FakeLiveness::new(),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        );

        assert_eq!(out.sessions.len(), 1);
        assert_eq!(out.sessions[0].state, SessionState::Dead);
        assert_eq!(out.dead_now, vec!["session-1".to_string()]);
    }

    #[test]
    fn a_session_dead_longer_than_the_retention_window_drops_off() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);
        let seen = HashMap::from([("session-1".to_string(), NOW - DEAD_RETENTION_MS - 1)]);

        let out = snapshot(
            &[f],
            &FakeLiveness::new(),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &seen,
        );

        assert!(out.sessions.is_empty());
    }

    #[test]
    fn a_session_dropped_by_retention_is_still_reported_as_dead_this_tick() {
        // dead_now must list every session observed dead, including those the
        // retention filter removed. Reporting only the survivors would drop the
        // map entry, making the same session look newly dead on the next tick
        // and resurrecting it forever.
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);
        let seen = HashMap::from([("session-1".to_string(), NOW - DEAD_RETENTION_MS - 1)]);

        let out = snapshot(
            &[f],
            &FakeLiveness::new(),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &seen,
        );

        assert_eq!(out.dead_now, vec!["session-1".to_string()]);
    }

    #[test]
    fn a_live_session_is_not_reported_dead() {
        let out = snapshot(
            &[file(1, "cli")],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        );
        assert!(out.dead_now.is_empty());
    }

    #[test]
    fn a_recently_dead_session_is_retained() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);
        let seen = HashMap::from([("session-1".to_string(), NOW - 60_000)]);
        let out = snapshot(
            &[f],
            &FakeLiveness::new(),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &seen,
        );
        assert_eq!(out.sessions[0].state, SessionState::Dead);
    }

    #[test]
    fn a_long_dead_session_drops_off_the_list() {
        // Claude Code prunes stale registry files itself; claude-buddy never
        // unlinks them, so it stops showing them instead.
        let f = file(1, "cli");
        let seen = HashMap::from([("session-1".to_string(), NOW - DEAD_RETENTION_MS - 1)]);
        assert!(snapshot(
            &[f],
            &FakeLiveness::new(),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &seen
        )
        .sessions
        .is_empty());
    }

    #[test]
    fn snapshot_carries_absolute_timestamps_alongside_derived_ages() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 6 * 60_000);

        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].status_time_ms, NOW - 6 * 60_000);
        assert_eq!(out[0].started_at_ms, NOW - 60_000);
        assert_eq!(out[0].elapsed_ms, 6 * 60_000);
    }

    #[test]
    fn status_time_falls_back_to_started_at_when_absent() {
        let f = file(1, "cli");
        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;
        assert_eq!(out[0].status_time_ms, NOW - 60_000);
        assert_eq!(out[0].started_at_ms, NOW - 60_000);
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(snapshot(
            &[],
            &FakeLiveness::new(),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new()
        )
        .sessions
        .is_empty());
    }

    #[test]
    fn snapshot_serializes_camel_case_with_lowercase_state() {
        let mut f = file(1, "cli");
        f.status = Some("waiting".into());
        f.waiting_for = Some("input needed".into());
        let out = snapshot(
            &[f],
            &alive(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        let json = serde_json::to_value(&out[0]).unwrap();
        assert_eq!(json["state"], "waiting");
        assert_eq!(json["sessionId"], "session-1");
        assert_eq!(json["elapsedMs"], 60_000);
    }

    fn running_task(id: &str) -> crate::watcher::task::Task {
        crate::watcher::task::Task {
            id: id.to_string(),
            kind: crate::watcher::task::TaskKind::Shell,
            label: Some(format!("run {id}")),
            started_at_ms: NOW - 30_000,
            ended_at_ms: None,
            status: crate::watcher::task::TaskStatus::Running,
            output: None,
        }
    }

    fn finished_task(id: &str, ended_at_ms: i64) -> crate::watcher::task::Task {
        crate::watcher::task::Task {
            id: id.to_string(),
            kind: crate::watcher::task::TaskKind::Shell,
            label: Some(format!("{id} done")),
            started_at_ms: NOW - 60_000,
            ended_at_ms: Some(ended_at_ms),
            status: crate::watcher::task::TaskStatus::Completed,
            output: None,
        }
    }

    #[test]
    fn a_paused_session_with_a_running_task_is_tasking() {
        let mut f = file(1, "cli");
        f.status = Some("idle".into());
        f.status_updated_at = Some(NOW - PAUSED_THRESHOLD_MS - 1);

        let out = snapshot(
            &[f],
            &FakeLiveness::new().with_alive_any_start(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &FakeTasks::new().with("session-1", vec![running_task("a")]),
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Tasking);
        assert_eq!(out[0].detail.as_deref(), Some("run a"));
    }

    #[test]
    fn an_idle_session_with_a_running_task_is_tasking() {
        let mut f = file(1, "cli");
        f.status = Some("idle".into());
        f.status_updated_at = Some(NOW - 60_000);

        let out = snapshot(
            &[f],
            &FakeLiveness::new().with_alive_any_start(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &FakeTasks::new().with("session-1", vec![running_task("a")]),
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Tasking);
    }

    #[test]
    fn more_than_one_running_task_is_counted_in_the_detail() {
        let mut f = file(1, "cli");
        f.status = Some("idle".into());
        f.status_updated_at = Some(NOW - 60_000);

        let out = snapshot(
            &[f],
            &FakeLiveness::new().with_alive_any_start(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &FakeTasks::new().with(
                "session-1",
                vec![running_task("a"), running_task("b"), running_task("c")],
            ),
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].detail.as_deref(), Some("3 tasks running"));
    }

    #[test]
    fn waiting_and_busy_and_dead_all_outrank_a_running_task() {
        // A session asking a question must never be relabelled as merely
        // tasking; a session working on its own turn is the more immediate
        // fact; and a dead session has nothing running at all.
        for (status, alive, expected) in [
            ("waiting", true, SessionState::Waiting),
            ("busy", true, SessionState::Busy),
            ("busy", false, SessionState::Dead),
        ] {
            let mut f = file(1, "cli");
            f.status = Some(status.into());
            f.waiting_for = Some("input needed".into());
            f.status_updated_at = Some(NOW - 60_000);

            let liveness = if alive {
                FakeLiveness::new().with_alive_any_start(1)
            } else {
                FakeLiveness::new()
            };

            let out = snapshot(
                &[f],
                &liveness,
                &NoActivity,
                &NoBlocked,
                &NoWork,
                &FakeTasks::new().with("session-1", vec![running_task("a")]),
                &NoTitle,
                NOW,
                PAUSED_THRESHOLD_MS,
                true,
                &HashMap::new(),
            )
            .sessions;

            assert_eq!(out[0].state, expected, "status {status}, alive {alive}");
        }
    }

    #[test]
    fn a_session_whose_tasks_have_all_finished_is_not_tasking() {
        let mut f = file(1, "cli");
        f.status = Some("idle".into());
        f.status_updated_at = Some(NOW - 60_000);

        let out = snapshot(
            &[f],
            &FakeLiveness::new().with_alive_any_start(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &FakeTasks::new().with("session-1", vec![finished_task("a", NOW - 1_000)]),
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Idle);
        // Still carried, so the alert diff can see the edge.
        assert_eq!(out[0].tasks.len(), 1);
    }

    #[test]
    fn a_task_that_finished_long_ago_is_dropped_from_the_snapshot() {
        let mut f = file(1, "cli");
        f.status = Some("idle".into());
        f.status_updated_at = Some(NOW - 60_000);

        let out = snapshot(
            &[f],
            &FakeLiveness::new().with_alive_any_start(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &FakeTasks::new().with(
                "session-1",
                vec![finished_task("a", NOW - TERMINAL_TASK_RETENTION_MS - 1)],
            ),
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert!(out[0].tasks.is_empty());
    }

    #[test]
    fn a_live_registry_job_is_a_task_on_the_session_sharing_its_cwd() {
        let mut parent = file(1, "cli");
        parent.status = Some("idle".into());
        parent.status_updated_at = Some(NOW - 60_000);

        let mut job = file(2, "cli");
        job.cwd = parent.cwd.clone();
        job.kind = Some("bg".into());
        job.job_id = Some("job_01hq8w2n4k".into());
        job.name = Some("migrate-schemas".into());
        job.status = Some("busy".into());
        job.status_updated_at = Some(NOW - 5_000);

        let out = snapshot(
            &[parent, job],
            &FakeLiveness::new()
                .with_alive_any_start(1)
                .with_alive_any_start(2),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        let session = out.iter().find(|s| !s.background).expect("parent shown");
        assert_eq!(session.state, SessionState::Tasking);
        assert_eq!(session.tasks.len(), 1);
        assert_eq!(session.tasks[0].kind, crate::watcher::task::TaskKind::Job);
        assert_eq!(session.tasks[0].label.as_deref(), Some("migrate-schemas"));
    }

    #[test]
    fn one_job_is_one_session_s_task_even_when_a_directory_holds_two() {
        // Two interactive sessions in one checkout is normal. Filtering the
        // jobs by cwd for every session made a single `bg` job read as a task
        // on both: both rows said tasking, the collapsed pill said "2 on
        // tasks", and both popovers listed the same job.
        let mut first = file(1, "cli");
        first.status = Some("idle".into());
        first.status_updated_at = Some(NOW - 60_000);

        let mut second = file(2, "cli");
        second.cwd = first.cwd.clone();
        second.status = Some("idle".into());
        second.status_updated_at = Some(NOW - 60_000);

        let mut job = file(3, "cli");
        job.cwd = first.cwd.clone();
        job.kind = Some("bg".into());
        job.job_id = Some("job_01hq8w2n4k".into());

        let out = snapshot(
            &[first, second, job],
            &FakeLiveness::new()
                .with_alive_any_start(1)
                .with_alive_any_start(2)
                .with_alive_any_start(3),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        let jobs_counted: usize = out
            .iter()
            .map(|s| {
                s.tasks
                    .iter()
                    .filter(|t| t.kind == crate::watcher::task::TaskKind::Job)
                    .count()
            })
            .sum();
        assert_eq!(jobs_counted, 1, "one job, counted once");
        assert_eq!(
            out.iter()
                .filter(|s| s.state == SessionState::Tasking)
                .count(),
            1
        );

        // And the parent chosen is the one the row grouping puts the job
        // under, so the popover and the row order tell the same story.
        let ids: Vec<&str> = out.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, ["session-1", "session-3", "session-2"]);
        assert_eq!(out[0].state, SessionState::Tasking);
    }

    #[test]
    fn a_job_from_a_disallowed_entrypoint_is_not_a_task() {
        // `sdk-cli` is plugin machinery, dropped before any other layer sees
        // it. Taking jobs from the unfiltered registry is deliberate about
        // `show_background_jobs` only.
        let mut parent = file(1, "cli");
        parent.status = Some("idle".into());
        parent.status_updated_at = Some(NOW - 60_000);

        let mut job = file(2, "sdk-cli");
        job.cwd = parent.cwd.clone();
        job.kind = Some("bg".into());
        job.job_id = Some("job_01hq8w2n4k".into());

        let out = snapshot(
            &[parent, job],
            &FakeLiveness::new()
                .with_alive_any_start(1)
                .with_alive_any_start(2),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out.len(), 1, "the job is not a row either");
        assert_eq!(out[0].state, SessionState::Idle);
        assert!(out[0].tasks.is_empty());
    }

    #[test]
    fn a_hidden_registry_job_is_still_a_task_on_its_parent() {
        // `showBackgroundJobs` governs whether a job gets a row of its own, not
        // whether its parent is waiting on it.
        let mut parent = file(1, "cli");
        parent.status = Some("idle".into());
        parent.status_updated_at = Some(NOW - 60_000);

        let mut job = file(2, "cli");
        job.cwd = parent.cwd.clone();
        job.kind = Some("bg".into());
        job.job_id = Some("job_01hq8w2n4k".into());

        let out = snapshot(
            &[parent, job],
            &FakeLiveness::new()
                .with_alive_any_start(1)
                .with_alive_any_start(2),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            false,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out.len(), 1, "the job itself is hidden");
        assert_eq!(out[0].state, SessionState::Tasking);
    }

    #[test]
    fn a_dead_registry_job_is_not_a_task() {
        let mut parent = file(1, "cli");
        parent.status = Some("idle".into());
        parent.status_updated_at = Some(NOW - 60_000);

        let mut job = file(2, "cli");
        job.cwd = parent.cwd.clone();
        job.kind = Some("bg".into());
        job.job_id = Some("job_01hq8w2n4k".into());

        let out = snapshot(
            &[parent, job],
            &FakeLiveness::new().with_alive_any_start(1),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &NoTasks,
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            false,
            &HashMap::new(),
        )
        .sessions;

        assert_eq!(out[0].state, SessionState::Idle);
        assert!(out[0].tasks.is_empty());
    }

    #[test]
    fn tasking_sorts_between_busy_and_idle() {
        let mut busy = file(1, "cli");
        busy.status = Some("busy".into());
        busy.status_updated_at = Some(NOW - 1_000);

        let mut tasking = file(2, "cli");
        tasking.status = Some("idle".into());
        tasking.status_updated_at = Some(NOW - 60_000);

        let mut idle = file(3, "cli");
        idle.status = Some("idle".into());
        idle.status_updated_at = Some(NOW - 60_000);

        let out = snapshot(
            &[idle, tasking, busy],
            &FakeLiveness::new()
                .with_alive_any_start(1)
                .with_alive_any_start(2)
                .with_alive_any_start(3),
            &NoActivity,
            &NoBlocked,
            &NoWork,
            &FakeTasks::new().with("session-2", vec![running_task("a")]),
            &NoTitle,
            NOW,
            PAUSED_THRESHOLD_MS,
            true,
            &HashMap::new(),
        )
        .sessions;

        let states: Vec<SessionState> = out.iter().map(|s| s.state).collect();
        assert_eq!(
            states,
            [
                SessionState::Busy,
                SessionState::Tasking,
                SessionState::Idle
            ]
        );
    }

    #[test]
    fn tasking_serialises_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&SessionState::Tasking).unwrap(),
            "\"tasking\""
        );
    }
}
