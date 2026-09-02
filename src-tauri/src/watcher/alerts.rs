use std::collections::HashMap;

use serde::Serialize;

use crate::watcher::state::{SessionSnapshot, SessionState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertKind {
    NeedsInput,
    Died,
    Finished,
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
fn alert_kind(was: Option<SessionState>, now: SessionState) -> Option<AlertKind> {
    match (was, now) {
        (_, SessionState::Waiting) => Some(AlertKind::NeedsInput),
        (_, SessionState::Dead) => Some(AlertKind::Died),
        (Some(SessionState::Busy), SessionState::Idle) => Some(AlertKind::Finished),
        _ => None,
    }
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

    next.iter()
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
        .collect()
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
}
