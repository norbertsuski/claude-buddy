use std::collections::HashMap;

use serde::Serialize;

use crate::watcher::state::{SessionSnapshot, SessionState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertKind {
    NeedsInput,
    Died,
    Finished,
    TaskDone,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub session_id: String,
    /// The session's process, so a clicked notification knows what to raise.
    pub pid: i32,
    pub name: String,
    pub kind: AlertKind,
    pub detail: Option<String>,
}

/// Which transitions are worth interrupting the user for.
///
/// A function of the edge, not the state: "finished" only means anything as a
/// move out of `Busy`, and a session first seen idle has finished nothing.
///
/// A turn that ends while a background task runs lands in `Tasking` rather
/// than `Idle`, and it is the same turn ending, so it reads the same. The
/// later `Tasking -> Idle` deliberately does not: the turn has already been
/// reported, the task's own ending is `TaskDone`'s to report, and two
/// "finished" notifications for one turn would be worse than the silence this
/// arm was added to fix.
fn alert_kind(was: Option<SessionState>, now: SessionState) -> Option<AlertKind> {
    match (was, now) {
        (_, SessionState::Waiting) => Some(AlertKind::NeedsInput),
        (_, SessionState::Dead) => Some(AlertKind::Died),
        (Some(SessionState::Busy), SessionState::Idle | SessionState::Tasking) => {
            Some(AlertKind::Finished)
        }
        _ => None,
    }
}

/// How a finished task reads in a notification.
fn task_outcome(task: &crate::watcher::tasks::Task) -> String {
    use crate::watcher::tasks::TaskStatus;

    let verb = match task.status {
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Killed => "was killed",
        TaskStatus::Stopped => "was stopped",
        // Not reachable: only terminal tasks are alerted about.
        TaskStatus::Running => "is running",
    };
    match task.label.as_deref() {
        Some(label) => format!("{label} {verb}"),
        None => format!("a background task {verb}"),
    }
}

/// Alerts for tasks that finished between two consecutive snapshots.
///
/// Not derived from the state edge, which is what every other alert here is.
/// A finishing task wakes its session, so the same tick that retires the task
/// usually moves the session out of `Tasking` and into `Busy` — a state diff
/// would report that as a session starting work and would say nothing about
/// the task, which is the part the user was waiting for.
fn task_alerts(prev: &[SessionSnapshot], next: &[SessionSnapshot]) -> Vec<Alert> {
    let mut was_running: HashMap<(&str, &str), ()> = HashMap::new();
    for session in prev {
        for task in &session.tasks {
            if task.status == crate::watcher::tasks::TaskStatus::Running {
                was_running.insert((session.session_id.as_str(), task.id.as_str()), ());
            }
        }
    }

    let mut alerts = Vec::new();
    for session in next {
        for task in &session.tasks {
            if !task.status.terminal() {
                continue;
            }
            // Only the transition is news. A finished task sits in several
            // consecutive snapshots while its retention window runs.
            if !was_running.contains_key(&(session.session_id.as_str(), task.id.as_str())) {
                continue;
            }
            alerts.push(Alert {
                session_id: session.session_id.clone(),
                pid: session.pid,
                name: session
                    .title
                    .clone()
                    .unwrap_or_else(|| session.name.clone()),
                kind: AlertKind::TaskDone,
                detail: Some(task_outcome(task)),
            });
        }
    }
    alerts
}

/// Alerts for transitions between two consecutive snapshots.
///
/// Edge-triggered: a session that was already in an alerting state stays quiet.
/// `prev == None` means this is the first snapshot after launch — it establishes
/// the baseline and fires nothing, so starting the app never floods the user
/// with alerts about state that predates it.
pub fn diff_alerts(prev: Option<&[SessionSnapshot]>, next: &[SessionSnapshot]) -> Vec<Alert> {
    let Some(prev) = prev else {
        return Vec::new();
    };

    let before: HashMap<&str, SessionState> = prev
        .iter()
        .map(|s| (s.session_id.as_str(), s.state))
        .collect();

    let mut alerts: Vec<Alert> = next
        .iter()
        .filter_map(|s| {
            let was = before.get(s.session_id.as_str()).copied();
            // Fire on entry only: unchanged alerting state is not an edge.
            if was == Some(s.state) {
                return None;
            }
            let kind = alert_kind(was, s.state)?;
            Some(Alert {
                session_id: s.session_id.clone(),
                pid: s.pid,
                // A notification says what the widget says. The title is what
                // the user recognises; the registry name is the folder, which
                // for three sessions in one repository names all three alike.
                name: s.title.clone().unwrap_or_else(|| s.name.clone()),
                kind,
                detail: s.detail.clone(),
            })
        })
        .collect();

    alerts.extend(task_alerts(prev, next));
    alerts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::state::{SessionSnapshot, SessionState};

    fn snap(id: &str, state: SessionState) -> SessionSnapshot {
        SessionSnapshot {
            pid: 1,
            session_id: id.to_string(),
            name: format!("name-{id}"),
            title: None,
            cwd: "/Users/n/Code/x".into(),
            entrypoint: "cli".into(),
            state,
            detail: match state {
                SessionState::Waiting => Some("input needed".into()),
                _ => None,
            },
            elapsed_ms: 0,
            uptime_ms: 0,
            status_time_ms: 0,
            started_at_ms: 0,
            background: false,
            tasks: Vec::new(),
        }
    }

    #[test]
    fn cold_start_fires_nothing_even_when_a_session_is_already_waiting() {
        // The first snapshot after launch establishes a baseline. Without this,
        // every launch produces a burst of alerts for pre-existing state.
        let next = vec![
            snap("a", SessionState::Waiting),
            snap("b", SessionState::Dead),
        ];
        assert!(diff_alerts(None, &next).is_empty());
    }

    #[test]
    fn transition_into_waiting_fires_needs_input() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Waiting)];

        let alerts = diff_alerts(Some(&prev), &next);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::NeedsInput);
        assert_eq!(alerts[0].session_id, "a");
        assert_eq!(alerts[0].detail.as_deref(), Some("input needed"));
    }

    #[test]
    fn staying_in_waiting_does_not_fire_again() {
        let prev = vec![snap("a", SessionState::Waiting)];
        let next = vec![snap("a", SessionState::Waiting)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn transition_into_dead_fires_died() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Dead)];

        let alerts = diff_alerts(Some(&prev), &next);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Died);
    }

    #[test]
    fn staying_dead_does_not_fire_again() {
        let prev = vec![snap("a", SessionState::Dead)];
        let next = vec![snap("a", SessionState::Dead)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn a_session_appearing_already_waiting_fires() {
        // Not a cold start: the app was running, a new session showed up blocked.
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![
            snap("a", SessionState::Busy),
            snap("b", SessionState::Waiting),
        ];

        let alerts = diff_alerts(Some(&prev), &next);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].session_id, "b");
    }

    #[test]
    fn a_session_appearing_busy_fires_nothing() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Busy), snap("b", SessionState::Busy)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn a_clean_exit_fires_nothing() {
        // The registry file was removed, so the session simply vanishes.
        let prev = vec![snap("a", SessionState::Busy)];
        assert!(diff_alerts(Some(&prev), &[]).is_empty());
    }

    #[test]
    fn finishing_a_turn_fires_finished() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Idle)];

        let alerts = diff_alerts(Some(&prev), &next);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Finished);
    }

    #[test]
    fn a_turn_ending_while_a_task_runs_still_fires_finished() {
        // The turn is over either way. Before `Tasking` existed this session
        // went `Busy -> Idle` and notified; leaving a background command
        // running must not silence it.
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Tasking)];

        let alerts = diff_alerts(Some(&prev), &next);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Finished);
    }

    #[test]
    fn a_task_landing_afterwards_does_not_fire_finished_a_second_time() {
        // `Busy -> Tasking` already reported this turn ending, and the task
        // finishing is `TaskDone`'s to report. Two "finished" notifications
        // for one turn would be worse than the bug that was fixed here.
        let prev = vec![snap("a", SessionState::Tasking)];
        let next = vec![snap("a", SessionState::Idle)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn sitting_idle_does_not_fire_finished() {
        let prev = vec![snap("a", SessionState::Idle)];
        let next = vec![snap("a", SessionState::Idle)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn a_session_appearing_idle_does_not_fire_finished() {
        // Never seen before, so there was no turn to finish.
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Busy), snap("b", SessionState::Idle)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn cold_start_does_not_fire_finished() {
        let next = vec![snap("a", SessionState::Idle)];
        assert!(diff_alerts(None, &next).is_empty());
    }

    #[test]
    fn waiting_to_idle_does_not_fire_finished() {
        // Answering a question and going quiet is not a completed turn.
        let prev = vec![snap("a", SessionState::Waiting)];
        let next = vec![snap("a", SessionState::Idle)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn drifting_into_paused_fires_nothing() {
        let prev = vec![snap("a", SessionState::Idle)];
        let next = vec![snap("a", SessionState::Paused)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn answering_then_blocking_again_fires_a_second_time() {
        let waiting = vec![snap("a", SessionState::Waiting)];
        let busy = vec![snap("a", SessionState::Busy)];

        assert!(diff_alerts(Some(&waiting), &busy).is_empty());
        assert_eq!(diff_alerts(Some(&busy), &waiting).len(), 1);
    }

    #[test]
    fn multiple_transitions_in_one_tick_all_fire() {
        let prev = vec![snap("a", SessionState::Busy), snap("b", SessionState::Busy)];
        let next = vec![
            snap("a", SessionState::Waiting),
            snap("b", SessionState::Dead),
        ];

        let alerts = diff_alerts(Some(&prev), &next);

        assert_eq!(alerts.len(), 2);
        assert!(alerts
            .iter()
            .any(|a| a.session_id == "a" && a.kind == AlertKind::NeedsInput));
        assert!(alerts
            .iter()
            .any(|a| a.session_id == "b" && a.kind == AlertKind::Died));
    }

    #[test]
    fn an_alert_carries_the_pid_so_it_can_be_raised() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Waiting)];
        assert_eq!(diff_alerts(Some(&prev), &next)[0].pid, 1);
    }

    #[test]
    fn alert_serializes_camel_case() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Waiting)];
        let json = serde_json::to_value(&diff_alerts(Some(&prev), &next)[0]).unwrap();

        assert_eq!(json["sessionId"], "a");
        assert_eq!(json["kind"], "needsInput");
    }

    use crate::watcher::tasks::{Task, TaskKind, TaskStatus};

    fn task(id: &str, status: TaskStatus, label: Option<&str>) -> Task {
        Task {
            id: id.to_string(),
            kind: TaskKind::Shell,
            label: label.map(str::to_string),
            started_at_ms: 0,
            ended_at_ms: match status {
                TaskStatus::Running => None,
                _ => Some(1_000),
            },
            status,
        }
    }

    /// A snapshot of one session carrying the given tasks.
    fn with_tasks(id: &str, state: SessionState, tasks: Vec<Task>) -> Vec<SessionSnapshot> {
        let mut snap = snap(id, state);
        snap.tasks = tasks;
        vec![snap]
    }

    #[test]
    fn a_task_finishing_fires_an_alert_naming_it() {
        let prev = with_tasks(
            "a",
            SessionState::Tasking,
            vec![task("t1", TaskStatus::Running, Some("npm test"))],
        );
        // The session wakes as the task lands, so its own state moves too.
        let next = with_tasks(
            "a",
            SessionState::Busy,
            vec![task("t1", TaskStatus::Completed, Some("npm test"))],
        );

        let alerts = diff_alerts(Some(&prev), &next);
        let done: Vec<&Alert> = alerts
            .iter()
            .filter(|a| a.kind == AlertKind::TaskDone)
            .collect();

        assert_eq!(done.len(), 1);
        assert_eq!(done[0].session_id, "a");
        assert_eq!(done[0].detail.as_deref(), Some("npm test completed"));
    }

    #[test]
    fn a_failed_task_reads_as_failed() {
        let prev = with_tasks(
            "a",
            SessionState::Tasking,
            vec![task("t1", TaskStatus::Running, Some("npm test"))],
        );
        let next = with_tasks(
            "a",
            SessionState::Tasking,
            vec![task("t1", TaskStatus::Failed, Some("npm test"))],
        );

        let alerts = diff_alerts(Some(&prev), &next);
        assert_eq!(
            alerts[0].detail.as_deref(),
            Some("npm test failed"),
            "a failure must not read like a success"
        );
    }

    #[test]
    fn a_task_with_no_label_still_alerts() {
        let prev = with_tasks(
            "a",
            SessionState::Tasking,
            vec![task("t1", TaskStatus::Running, None)],
        );
        let next = with_tasks(
            "a",
            SessionState::Idle,
            vec![task("t1", TaskStatus::Completed, None)],
        );

        let alerts = diff_alerts(Some(&prev), &next);
        let done: Vec<&Alert> = alerts
            .iter()
            .filter(|a| a.kind == AlertKind::TaskDone)
            .collect();
        assert_eq!(
            done[0].detail.as_deref(),
            Some("a background task completed")
        );
    }

    #[test]
    fn a_task_that_was_already_finished_does_not_alert_again() {
        // Finished tasks linger for a retention window, so the same terminal
        // task is in several consecutive snapshots.
        let finished = with_tasks(
            "a",
            SessionState::Idle,
            vec![task("t1", TaskStatus::Completed, Some("npm test"))],
        );
        assert!(diff_alerts(Some(&finished), &finished).is_empty());
    }

    #[test]
    fn a_task_still_running_does_not_alert() {
        let running = with_tasks(
            "a",
            SessionState::Tasking,
            vec![task("t1", TaskStatus::Running, Some("npm test"))],
        );
        assert!(diff_alerts(Some(&running), &running).is_empty());
    }

    #[test]
    fn a_task_that_vanished_without_a_terminal_status_does_not_alert() {
        // The retention window drops a finished task eventually, and a session
        // being resumed drops its predecessor's tasks. Neither is news.
        let prev = with_tasks(
            "a",
            SessionState::Tasking,
            vec![task("t1", TaskStatus::Running, Some("npm test"))],
        );
        let next = with_tasks("a", SessionState::Idle, vec![]);
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn cold_start_fires_no_task_alerts() {
        let next = with_tasks(
            "a",
            SessionState::Idle,
            vec![task("t1", TaskStatus::Completed, Some("npm test"))],
        );
        assert!(diff_alerts(None, &next).is_empty());
    }

    #[test]
    fn several_tasks_finishing_at_once_each_alert() {
        let prev = with_tasks(
            "a",
            SessionState::Tasking,
            vec![
                task("t1", TaskStatus::Running, Some("npm test")),
                task("t2", TaskStatus::Running, Some("cargo test")),
            ],
        );
        let next = with_tasks(
            "a",
            SessionState::Idle,
            vec![
                task("t1", TaskStatus::Completed, Some("npm test")),
                task("t2", TaskStatus::Killed, Some("cargo test")),
            ],
        );

        let alerts = diff_alerts(Some(&prev), &next);
        let done = alerts
            .iter()
            .filter(|a| a.kind == AlertKind::TaskDone)
            .count();
        assert_eq!(done, 2);
    }

    #[test]
    fn task_done_serialises_as_camel_case() {
        assert_eq!(
            serde_json::to_string(&AlertKind::TaskDone).unwrap(),
            "\"taskDone\""
        );
    }
}
