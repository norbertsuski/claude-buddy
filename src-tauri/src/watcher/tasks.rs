use std::collections::HashMap;

use serde::Serialize;

use crate::bridge::transcript::{assistant_content, clip_to, message_content};

/// Longest task label the popover will draw. The same width `latest_activity`
/// clips to, because they sit one under the other in the same popover.
pub const LABEL_MAX_CHARS: usize = crate::bridge::transcript::ACTIVITY_MAX_CHARS;

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
}

/// One half of a task's life, as recorded in a transcript.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskEvent {
    Started {
        id: String,
        kind: TaskKind,
        label: Option<String>,
        at_ms: i64,
    },
    Ended {
        id: String,
        status: TaskStatus,
        label: Option<String>,
        at_ms: i64,
    },
}

impl TaskEvent {
    pub fn id(&self) -> &str {
        match self {
            TaskEvent::Started { id, .. } | TaskEvent::Ended { id, .. } => id,
        }
    }
}

/// Every task event in these bytes, oldest first.
///
/// Forward, unlike every other function in `bridge::transcript`, and for the
/// opposite reason: those want the newest value of one field, and this wants
/// both halves of a story whose halves can be minutes and megabytes apart. A
/// start and its end are separate records, so order is the whole point.
///
/// Unparseable lines are skipped, as everywhere else: a windowed read almost
/// always begins mid-record.
pub fn task_events(bytes: &[u8]) -> Vec<TaskEvent> {
    let text = String::from_utf8_lossy(bytes);
    let records: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    let tools = tool_uses(&records);

    records
        .iter()
        .filter_map(|record| {
            let at_ms = record
                .get("timestamp")
                .and_then(|v| v.as_str())
                .and_then(crate::rfc3339::epoch_ms)?;
            started_event(record, &tools, at_ms).or_else(|| ended_event(record, at_ms))
        })
        .collect()
}

/// How many tasks one session's list may hold.
///
/// A session that has run for hours has run hundreds of background commands,
/// and every one of them would otherwise sit in a snapshot that is emitted to
/// the frontend. The newest are kept: a finished task matters for the seconds
/// it takes to alert about it, and a running one is always among the newest.
pub const MAX_TASKS: usize = 50;

/// Fold events into an existing task list.
///
/// Additive and repeatable, because the probe applies each newly appended
/// window of a transcript to the list it already had, and a window can be read
/// twice when a file is truncated and re-scanned.
///
/// `since_ms` is the session's `startedAt`. Starts older than it belong to a
/// previous process — a resumed session appends to the same transcript — and
/// are dropped, which is what stops a dead process's unfinished tasks reading
/// as running forever. It needs no timeout to do it, which matters: a dev
/// server legitimately runs for hours, so any age cap would either retire real
/// tasks or be too loose to catch anything.
pub fn apply_events(tasks: &mut Vec<Task>, events: &[TaskEvent], since_ms: i64) {
    for event in events {
        match event {
            TaskEvent::Started {
                id,
                kind,
                label,
                at_ms,
            } => {
                if *at_ms < since_ms || tasks.iter().any(|t| t.id == *id) {
                    continue;
                }
                tasks.push(Task {
                    id: id.clone(),
                    kind: *kind,
                    label: label.clone(),
                    started_at_ms: *at_ms,
                    ended_at_ms: None,
                    status: TaskStatus::Running,
                });
            }
            TaskEvent::Ended {
                id,
                status,
                label,
                at_ms,
            } => {
                // Only a running task can end. The second copy of a duplicated
                // notification finds it already retired and leaves the first
                // ending's time in place.
                let Some(task) = tasks
                    .iter_mut()
                    .find(|t| t.id == *id && t.status == TaskStatus::Running)
                else {
                    continue;
                };
                task.status = *status;
                task.ended_at_ms = Some(*at_ms);
                // The notification says what happened; the call's description
                // only said what was intended.
                if label.is_some() {
                    task.label = label.clone();
                }
            }
        }
    }

    tasks.sort_by_key(|t| t.started_at_ms);
    if tasks.len() > MAX_TASKS {
        tasks.drain(..tasks.len() - MAX_TASKS);
    }
}

/// The tasks a whole set of events describes, for a session that began at
/// `since_ms`.
pub fn tasks_from_events(events: &[TaskEvent], since_ms: i64) -> Vec<Task> {
    let mut tasks = Vec::new();
    apply_events(&mut tasks, events, since_ms);
    tasks
}

/// Every `tool_use` id in these records, paired with its tool name and the
/// `description` its input carried.
///
/// A task's start record names only its own new task id; what kind of task it
/// is, and what it is for, live on the call that produced it.
fn tool_uses<'a>(records: &'a [serde_json::Value]) -> HashMap<&'a str, (&'a str, Option<&'a str>)> {
    let mut out = HashMap::new();
    for record in records {
        let Some(content) = assistant_content(record) else {
            continue;
        };
        for block in content {
            if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                continue;
            }
            let Some(id) = block.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(name) = block.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let description = block
                .get("input")
                .and_then(|i| i.get("description"))
                .and_then(|v| v.as_str());
            out.insert(id, (name, description));
        }
    }
    out
}

/// The `tool_use_id` this record is a result for, if it is one.
fn result_for(record: &serde_json::Value) -> Option<&str> {
    message_content(record)?
        .iter()
        .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))?
        .get("tool_use_id")?
        .as_str()
}

/// Which kind of task a tool produces. The id field alone cannot tell a
/// background agent from a watch — both report `taskId` — so the tool name
/// decides, and the id field is only the fallback for a call this window of the
/// transcript did not include.
fn kind_for_tool(name: &str, fallback: TaskKind) -> TaskKind {
    match name {
        "Bash" => TaskKind::Shell,
        "Agent" | "Task" => TaskKind::Subagent,
        _ => fallback,
    }
}

fn started_event(
    record: &serde_json::Value,
    tools: &HashMap<&str, (&str, Option<&str>)>,
    at_ms: i64,
) -> Option<TaskEvent> {
    let result = record.get("toolUseResult")?;
    let (id, fallback) = match result.get("backgroundTaskId").and_then(|v| v.as_str()) {
        Some(id) => (id, TaskKind::Shell),
        None => (
            result.get("taskId").and_then(|v| v.as_str())?,
            TaskKind::Watch,
        ),
    };

    let call = result_for(record).and_then(|id| tools.get(id));
    let (kind, label) = match call {
        Some((name, description)) => (
            kind_for_tool(name, fallback),
            description.map(|d| clip_to(d, LABEL_MAX_CHARS)),
        ),
        None => (fallback, None),
    };

    Some(TaskEvent::Started {
        id: id.to_string(),
        kind,
        label,
        at_ms,
    })
}

fn ended_event(record: &serde_json::Value, at_ms: i64) -> Option<TaskEvent> {
    let text = notification_text(record)?;
    let id = tag(&text, "task-id")?;
    let status = terminal_status(&tag(&text, "status")?)?;
    let label = tag(&text, "summary").map(|s| clip_to(&unescape(&s), LABEL_MAX_CHARS));

    Some(TaskEvent::Ended {
        id,
        status,
        label,
        at_ms,
    })
}

/// A task notification's text, wherever this record carries it.
///
/// Claude Code writes each notification twice — once as a `queue-operation`
/// with the text in `content`, once as an `attachment` with the same text in
/// `attachment.prompt`. Both are read: which one lands first is not something
/// to depend on, and the fold deduplicates them anyway.
fn notification_text(record: &serde_json::Value) -> Option<String> {
    [
        record.get("content"),
        record.get("attachment").and_then(|a| a.get("prompt")),
    ]
    .into_iter()
    .flatten()
    .filter_map(|v| v.as_str())
    .find(|text| text.contains("<task-notification>"))
    .map(str::to_string)
}

/// The contents of one tag in a notification block.
fn tag(text: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim().to_string())
}

/// A status word, or `None` for anything that is not a task ending.
///
/// Unknown words are not endings. A progress notification that retired a
/// running task would be worse than one that is ignored.
fn terminal_status(word: &str) -> Option<TaskStatus> {
    match word {
        "completed" => Some(TaskStatus::Completed),
        "failed" => Some(TaskStatus::Failed),
        "killed" => Some(TaskStatus::Killed),
        "stopped" => Some(TaskStatus::Stopped),
        _ => None,
    }
}

/// Undo the XML escaping a notification's summary carries.
///
/// The summary quotes the command it is about, so a shell heredoc arrives as
/// `python3 &lt;&lt; &#39;PY&#39;` and would otherwise be shown that way.
/// `&amp;` is last, so an escaped ampersand cannot be re-read as the start of
/// another entity.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A background `Bash` call and the result that reports its task id. The
    /// two records are the real shapes, trimmed to the fields that are read.
    const SHELL_START: &str = concat!(
        r#"{"type":"assistant","timestamp":"2026-08-28T08:42:40.000Z","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"npm test","description":"Run the suite","run_in_background":true}}]}}"#,
        "\n",
        r#"{"type":"user","timestamp":"2026-08-28T08:42:47.177Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1"}]},"toolUseResult":{"backgroundTaskId":"bmd0i64ke","timedOutAfterMs":120000}}"#,
        "\n",
    );

    /// The completion, as a `queue-operation` record.
    const SHELL_DONE: &str = concat!(
        r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-08-28T08:49:13.537Z","content":"<task-notification> <task-id>bmd0i64ke</task-id> <status>completed</status> <summary>Background command \"npm test\" completed</summary> </task-notification>"}"#,
        "\n",
    );

    /// The same completion again, as the `attachment` record that always
    /// follows it.
    const SHELL_DONE_AGAIN: &str = concat!(
        r#"{"type":"attachment","timestamp":"2026-08-28T08:49:14.000Z","attachment":{"type":"queued_command","prompt":"<task-notification> <task-id>bmd0i64ke</task-id> <status>completed</status> <summary>Background command \"npm test\" completed</summary> </task-notification>"}}"#,
        "\n",
    );

    fn events(body: &str) -> Vec<TaskEvent> {
        task_events(body.as_bytes())
    }

    #[test]
    fn a_background_shell_start_is_read_with_its_description_as_the_label() {
        let events = events(SHELL_START);
        assert_eq!(events.len(), 1);
        match &events[0] {
            TaskEvent::Started {
                id,
                kind,
                label,
                at_ms,
            } => {
                assert_eq!(id, "bmd0i64ke");
                assert_eq!(*kind, TaskKind::Shell);
                assert_eq!(label.as_deref(), Some("Run the suite"));
                assert_eq!(*at_ms, 1787906567177);
            }
            other => panic!("expected a start, got {other:?}"),
        }
    }

    #[test]
    fn a_watch_task_start_is_read_from_its_task_id() {
        let body = concat!(
            r#"{"type":"assistant","timestamp":"2026-08-28T08:49:50.000Z","message":{"content":[{"type":"tool_use","id":"toolu_2","name":"Monitor","input":{}}]}}"#,
            "\n",
            r#"{"type":"user","timestamp":"2026-08-28T08:49:58.821Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_2"}]},"toolUseResult":{"taskId":"b4w30xxzw","timeoutMs":600000}}"#,
            "\n",
        );
        match &events(body)[0] {
            TaskEvent::Started { id, kind, .. } => {
                assert_eq!(id, "b4w30xxzw");
                assert_eq!(*kind, TaskKind::Watch);
            }
            other => panic!("expected a start, got {other:?}"),
        }
    }

    #[test]
    fn a_background_agent_start_is_a_subagent_not_a_watch() {
        // The id field says `taskId` for an Agent exactly as it does for a
        // watch, so the tool name behind the call is the only thing that
        // separates them.
        let body = concat!(
            r#"{"type":"assistant","timestamp":"2026-08-28T09:00:00.000Z","message":{"content":[{"type":"tool_use","id":"toolu_3","name":"Agent","input":{"description":"Review the diff"}}]}}"#,
            "\n",
            r#"{"type":"user","timestamp":"2026-08-28T09:00:01.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_3"}]},"toolUseResult":{"taskId":"agent7"}}"#,
            "\n",
        );
        match &events(body)[0] {
            TaskEvent::Started { kind, label, .. } => {
                assert_eq!(*kind, TaskKind::Subagent);
                assert_eq!(label.as_deref(), Some("Review the diff"));
            }
            other => panic!("expected a start, got {other:?}"),
        }
    }

    #[test]
    fn a_notification_is_read_as_an_end_with_its_status_and_summary() {
        let events = events(SHELL_DONE);
        assert_eq!(events.len(), 1);
        match &events[0] {
            TaskEvent::Ended {
                id,
                status,
                label,
                at_ms,
            } => {
                assert_eq!(id, "bmd0i64ke");
                assert_eq!(*status, TaskStatus::Completed);
                assert_eq!(
                    label.as_deref(),
                    Some("Background command \"npm test\" completed")
                );
                assert_eq!(*at_ms, 1787906953537);
            }
            other => panic!("expected an end, got {other:?}"),
        }
    }

    #[test]
    fn every_terminal_status_is_recognised() {
        for (word, expected) in [
            ("completed", TaskStatus::Completed),
            ("failed", TaskStatus::Failed),
            ("killed", TaskStatus::Killed),
            ("stopped", TaskStatus::Stopped),
        ] {
            let body = format!(
                r#"{{"type":"queue-operation","timestamp":"2026-08-28T08:49:13.537Z","content":"<task-notification> <task-id>x</task-id> <status>{word}</status> </task-notification>"}}"#
            );
            match &events(&body)[0] {
                TaskEvent::Ended { status, .. } => assert_eq!(*status, expected),
                other => panic!("expected an end for {word}, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_status_that_is_not_terminal_is_not_an_end() {
        // A progress notification must not retire a running task.
        let body = r#"{"type":"queue-operation","timestamp":"2026-08-28T08:49:13.537Z","content":"<task-notification> <task-id>x</task-id> <status>running</status> </task-notification>"}"#;
        assert!(events(body).is_empty());
    }

    #[test]
    fn the_duplicate_attachment_notification_is_read_too() {
        // Both records are events at this layer; the fold is what deduplicates
        // them. Reading only one of the two shapes would be a silent
        // dependency on which of them Claude Code writes first.
        let body = format!("{SHELL_DONE}{SHELL_DONE_AGAIN}");
        assert_eq!(events(&body).len(), 2);
    }

    #[test]
    fn escaped_entities_in_a_summary_are_decoded() {
        let body = r#"{"type":"queue-operation","timestamp":"2026-08-28T08:49:13.537Z","content":"<task-notification> <task-id>x</task-id> <status>failed</status> <summary>python3 &lt;&lt; &#39;PY&#39; &amp; wait &quot;file&quot; &amp;lt;</summary> </task-notification>"}"#;
        match &events(body)[0] {
            TaskEvent::Ended { label, .. } => {
                // &amp;lt; must decode to &lt;, not <. This pins the ordering:
                // &amp; must be replaced last, so an escaped ampersand cannot
                // be re-read as the start of another entity.
                assert_eq!(
                    label.as_deref(),
                    Some("python3 << 'PY' & wait \"file\" &lt;")
                )
            }
            other => panic!("expected an end, got {other:?}"),
        }
    }

    #[test]
    fn a_long_label_is_clipped_to_the_popover_width() {
        let long = "x".repeat(200);
        let body = format!(
            r#"{{"type":"queue-operation","timestamp":"2026-08-28T08:49:13.537Z","content":"<task-notification> <task-id>x</task-id> <status>completed</status> <summary>{long}</summary> </task-notification>"}}"#
        );
        match &events(&body)[0] {
            TaskEvent::Ended { label, .. } => {
                assert_eq!(
                    label.as_deref().unwrap().chars().count(),
                    LABEL_MAX_CHARS + 1
                )
            }
            other => panic!("expected an end, got {other:?}"),
        }
    }

    #[test]
    fn a_record_with_no_timestamp_is_skipped() {
        // Without a timestamp a start cannot be placed against the session's
        // own start time, which is the only thing bounding a phantom task.
        let body = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1"}]},"toolUseResult":{"backgroundTaskId":"b1"}}"#;
        assert!(events(body).is_empty());
    }

    #[test]
    fn unparseable_lines_are_skipped() {
        // A fixed-size tail almost always begins mid-record.
        let body = format!("{{\"type\":\"assis\n{SHELL_START}");
        assert_eq!(events(&body).len(), 1);
    }

    #[test]
    fn a_foreground_tool_call_produces_nothing() {
        let body = concat!(
            r#"{"type":"assistant","timestamp":"2026-08-28T08:42:40.000Z","message":{"content":[{"type":"tool_use","id":"toolu_9","name":"Grep","input":{}}]}}"#,
            "\n",
            r#"{"type":"user","timestamp":"2026-08-28T08:42:41.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_9"}]},"toolUseResult":{"stdout":"hits"}}"#,
            "\n",
        );
        assert!(events(body).is_empty());
    }

    const SESSION_START: i64 = 1_787_906_000_000;

    fn started(id: &str, at_ms: i64) -> TaskEvent {
        TaskEvent::Started {
            id: id.to_string(),
            kind: TaskKind::Shell,
            label: Some(format!("run {id}")),
            at_ms,
        }
    }

    fn ended(id: &str, status: TaskStatus, at_ms: i64) -> TaskEvent {
        TaskEvent::Ended {
            id: id.to_string(),
            status,
            label: Some(format!("{id} finished")),
            at_ms,
        }
    }

    #[test]
    fn a_start_with_no_end_is_a_running_task() {
        let tasks = tasks_from_events(&[started("a", SESSION_START + 1)], SESSION_START);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "a");
        assert_eq!(tasks[0].status, TaskStatus::Running);
        assert_eq!(tasks[0].ended_at_ms, None);
        assert_eq!(tasks[0].label.as_deref(), Some("run a"));
    }

    #[test]
    fn an_end_retires_its_task_and_takes_over_the_label() {
        // The notification's summary says what happened; the call's
        // description only said what was intended.
        let tasks = tasks_from_events(
            &[
                started("a", SESSION_START + 1),
                ended("a", TaskStatus::Failed, SESSION_START + 500),
            ],
            SESSION_START,
        );
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, TaskStatus::Failed);
        assert_eq!(tasks[0].ended_at_ms, Some(SESSION_START + 500));
        assert_eq!(tasks[0].label.as_deref(), Some("a finished"));
    }

    #[test]
    fn the_duplicate_notification_does_not_change_the_answer() {
        let tasks = tasks_from_events(
            &[
                started("a", SESSION_START + 1),
                ended("a", TaskStatus::Completed, SESSION_START + 500),
                ended("a", TaskStatus::Completed, SESSION_START + 501),
            ],
            SESSION_START,
        );
        assert_eq!(tasks.len(), 1);
        // The first ending stands, so the age of a finished task does not
        // creep forward as the second record lands.
        assert_eq!(tasks[0].ended_at_ms, Some(SESSION_START + 500));
    }

    #[test]
    fn a_start_before_the_session_began_is_dropped() {
        // A resumed session appends to the same transcript, so the previous
        // process's unfinished tasks would otherwise read as running forever.
        let tasks = tasks_from_events(&[started("old", SESSION_START - 1)], SESSION_START);
        assert!(tasks.is_empty());
    }

    #[test]
    fn an_end_for_a_task_that_was_never_started_is_ignored() {
        let tasks = tasks_from_events(
            &[ended("ghost", TaskStatus::Completed, SESSION_START + 1)],
            SESSION_START,
        );
        assert!(tasks.is_empty());
    }

    #[test]
    fn tasks_come_back_oldest_first() {
        let tasks = tasks_from_events(
            &[
                started("b", SESSION_START + 200),
                started("a", SESSION_START + 100),
                started("c", SESSION_START + 300),
            ],
            SESSION_START,
        );
        let ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
    }

    #[test]
    fn applying_the_same_events_twice_is_not_two_tasks() {
        // The probe folds each appended window into the list it already has,
        // and a window can be re-read after a truncation.
        let events = [started("a", SESSION_START + 1)];
        let mut tasks = Vec::new();
        apply_events(&mut tasks, &events, SESSION_START);
        apply_events(&mut tasks, &events, SESSION_START);
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn an_end_applied_in_a_later_window_retires_the_task_from_an_earlier_one() {
        let mut tasks = Vec::new();
        apply_events(
            &mut tasks,
            &[started("a", SESSION_START + 1)],
            SESSION_START,
        );
        assert_eq!(tasks[0].status, TaskStatus::Running);

        apply_events(
            &mut tasks,
            &[ended("a", TaskStatus::Killed, SESSION_START + 9)],
            SESSION_START,
        );
        assert_eq!(tasks[0].status, TaskStatus::Killed);
    }

    #[test]
    fn the_list_is_capped_at_the_newest_tasks() {
        let events: Vec<TaskEvent> = (0..MAX_TASKS + 10)
            .map(|i| started(&format!("t{i}"), SESSION_START + i as i64))
            .collect();
        let tasks = tasks_from_events(&events, SESSION_START);
        assert_eq!(tasks.len(), MAX_TASKS);
        // The oldest went, not the newest.
        assert_eq!(tasks[0].id, "t10");
    }
}
