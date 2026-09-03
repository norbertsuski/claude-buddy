//! The task data model, and the trait that supplies it.
//!
//! Separate from `tasks.rs` because the shape a task has is not
//! provider-specific but the way you find one is: `tasks.rs` scans Claude
//! Code transcripts for subagent records, whereas everything here is what the
//! state machine stores and the widget renders.

use std::collections::HashMap;

use serde::Serialize;

/// What kind of work a task is.
///
/// `Shell`, `Watch` and `Subagent` all come out of the transcript. `Job` does
/// not: it is a `bg` registry entry, a separate process, folded onto its parent
/// by `state::snapshot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    Shell,
    Watch,
    Subagent,
    Job,
}

/// The four terminal statuses Claude Code writes, plus the one it does not
/// write because it is the absence of the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Running,
    Completed,
    Failed,
    Killed,
    Stopped,
}

impl TaskStatus {
    /// Whether this task is over.
    pub fn terminal(self) -> bool {
        self != TaskStatus::Running
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub kind: TaskKind,
    /// The notification's summary, else the originating tool's description.
    /// Absent for a task whose transcript records carried neither.
    pub label: Option<String>,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub status: TaskStatus,
    /// The file this task's output is being written to, when its start named
    /// one. Not sent to the frontend — it is a temp path nothing draws — but
    /// it is the only place a killed task's ending is ever recorded.
    #[serde(skip)]
    pub output: Option<String>,
}

/// A session's background tasks.
///
/// Injected rather than called directly, matching `PidLiveness`,
/// `ActivityProbe`, `BlockedProbe`, `WorkProbe` and `TitleProbe`, so the state
/// machine stays testable without a transcript on disk.
///
/// `started_at_ms` is the session's registry `startedAt`, and is the phantom
/// boundary: nothing before it can still be running. It is a parameter rather
/// than something the probe looks up because only the registry knows it, and
/// the probe never reads the registry.
pub trait TaskProbe {
    fn tasks(&self, cwd: &str, session_id: &str, started_at_ms: i64) -> Vec<Task>;
}

/// Reports nothing, for callers that do not care.
pub struct NoTasks;

impl TaskProbe for NoTasks {
    fn tasks(&self, _cwd: &str, _session_id: &str, _started_at_ms: i64) -> Vec<Task> {
        Vec::new()
    }
}

/// Test double keyed by session id.
pub struct FakeTasks {
    tasks: HashMap<String, Vec<Task>>,
}

impl FakeTasks {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    pub fn with(mut self, session_id: &str, tasks: Vec<Task>) -> Self {
        self.tasks.insert(session_id.to_string(), tasks);
        self
    }
}

impl Default for FakeTasks {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskProbe for FakeTasks {
    fn tasks(&self, _cwd: &str, session_id: &str, _started_at_ms: i64) -> Vec<Task> {
        self.tasks.get(session_id).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frontend reads these field names. Renaming one here is a silent
    /// break: the widget shows a task with no label rather than failing.
    #[test]
    fn a_task_serializes_with_the_names_the_frontend_reads() {
        let task = Task {
            id: "t1".to_string(),
            kind: TaskKind::Subagent,
            label: Some("reviewing the diff".to_string()),
            started_at_ms: 1_787_637_231_465,
            ended_at_ms: None,
            status: TaskStatus::Running,
            output: None,
        };

        let json = serde_json::to_value(&task).expect("task serializes");

        assert!(json.get("startedAtMs").is_some(), "got {json}");
        assert!(json.get("endedAtMs").is_some(), "got {json}");
        assert_eq!(json.get("kind").and_then(|k| k.as_str()), Some("subagent"));
        assert_eq!(json.get("status").and_then(|s| s.as_str()), Some("running"));
        assert!(
            json.get("output").is_none(),
            "output is #[serde(skip)] and must not reach the frontend: {json}"
        );
    }
}
