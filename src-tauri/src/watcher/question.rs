use std::collections::HashMap;
use std::path::PathBuf;

/// The prose a waiting session is blocked on.
///
/// Injected rather than called directly so the watcher stays testable without a
/// transcript on disk, matching `PidLiveness` and `ActivityProbe`.
pub trait QuestionProbe {
    fn pending_question(&self, cwd: &str, session_id: &str) -> Option<String>;
}

/// Reads the question from the session transcript.
pub struct TranscriptQuestion {
    projects_dir: PathBuf,
}

impl TranscriptQuestion {
    pub fn new(projects_dir: PathBuf) -> Self {
        Self { projects_dir }
    }
}

impl QuestionProbe for TranscriptQuestion {
    fn pending_question(&self, cwd: &str, session_id: &str) -> Option<String> {
        use crate::bridge::transcript::{
            find_transcript, latest_assistant_text, read_tail, TAIL_BYTES,
        };

        let path = find_transcript(&self.projects_dir, cwd, session_id)?;
        let bytes = read_tail(&path, TAIL_BYTES).ok()?;
        latest_assistant_text(&bytes)
    }
}

/// Reports nothing.
pub struct NoQuestion;

impl QuestionProbe for NoQuestion {
    fn pending_question(&self, _cwd: &str, _session_id: &str) -> Option<String> {
        None
    }
}

/// Test double keyed by session id.
pub struct FakeQuestion {
    answers: HashMap<String, String>,
}

impl FakeQuestion {
    pub fn new() -> Self {
        Self {
            answers: HashMap::new(),
        }
    }

    pub fn with(mut self, session_id: &str, question: &str) -> Self {
        self.answers
            .insert(session_id.to_string(), question.to_string());
        self
    }
}

impl Default for FakeQuestion {
    fn default() -> Self {
        Self::new()
    }
}

impl QuestionProbe for FakeQuestion {
    fn pending_question(&self, _cwd: &str, session_id: &str) -> Option<String> {
        self.answers.get(session_id).cloned()
    }
}

/// Replace each needs-input alert's detail with the session's pending question.
///
/// Only alerts are enriched, never the snapshot: this runs once per transition
/// into `waiting`, which is rare, whereas the snapshot is rebuilt every two
/// seconds and tailing a transcript per session per tick is exactly what the
/// popover's lazy fetch exists to avoid.
///
/// The registry's `waitingFor` stands when the transcript yields nothing.
pub fn enrich_alerts(
    alerts: &mut [buddy_core::watcher::alerts::Alert],
    sessions: &[buddy_core::watcher::state::SessionSnapshot],
    probe: &dyn QuestionProbe,
) {
    use buddy_core::watcher::alerts::AlertKind;

    for alert in alerts.iter_mut() {
        if alert.kind != AlertKind::NeedsInput {
            continue;
        }
        let Some(session) = sessions.iter().find(|s| s.session_id == alert.session_id) else {
            continue;
        };
        if let Some(question) = probe.pending_question(&session.cwd, &session.session_id) {
            alert.detail = Some(question);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use buddy_core::watcher::alerts::{Alert, AlertKind};
    use buddy_core::watcher::state::{SessionSnapshot, SessionState};

    fn session(id: &str) -> SessionSnapshot {
        SessionSnapshot {
            pid: 7,
            session_id: id.to_string(),
            name: format!("name-{id}"),
            title: None,
            cwd: "/Users/n/Code/x".into(),
            entrypoint: "cli".into(),
            state: SessionState::Waiting,
            detail: Some("input needed".into()),
            elapsed_ms: 0,
            uptime_ms: 0,
            status_time_ms: 0,
            started_at_ms: 0,
            background: false,
            tasks: Vec::new(),
        }
    }

    fn alert(id: &str, kind: AlertKind) -> Alert {
        Alert {
            session_id: id.to_string(),
            pid: 7,
            name: format!("name-{id}"),
            kind,
            detail: Some("input needed".into()),
        }
    }

    #[test]
    fn a_needs_input_alert_gets_the_question_from_the_transcript() {
        let mut alerts = vec![alert("a", AlertKind::NeedsInput)];
        let probe = FakeQuestion::new().with("a", "Shall I delete the branch?");

        enrich_alerts(&mut alerts, &[session("a")], &probe);

        assert_eq!(
            alerts[0].detail.as_deref(),
            Some("Shall I delete the branch?")
        );
    }

    #[test]
    fn the_registry_reason_stands_when_the_transcript_yields_nothing() {
        let mut alerts = vec![alert("a", AlertKind::NeedsInput)];
        enrich_alerts(&mut alerts, &[session("a")], &NoQuestion);
        assert_eq!(alerts[0].detail.as_deref(), Some("input needed"));
    }

    #[test]
    fn a_died_alert_is_left_alone() {
        let mut alerts = vec![alert("a", AlertKind::Died)];
        let probe = FakeQuestion::new().with("a", "Shall I delete the branch?");

        enrich_alerts(&mut alerts, &[session("a")], &probe);

        assert_eq!(alerts[0].detail.as_deref(), Some("input needed"));
    }

    #[test]
    fn an_alert_with_no_matching_session_is_left_alone() {
        let mut alerts = vec![alert("gone", AlertKind::NeedsInput)];
        let probe = FakeQuestion::new().with("gone", "unreachable");

        enrich_alerts(&mut alerts, &[session("a")], &probe);

        assert_eq!(alerts[0].detail.as_deref(), Some("input needed"));
    }
}
