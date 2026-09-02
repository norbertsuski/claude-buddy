use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

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

/// One session's cached answer.
struct CachedTasks {
    /// Transcript mtime the answer was read at.
    at_ms: i64,
    /// How much of the file has been folded in. The next read starts here.
    consumed: u64,
    tasks: Vec<Task>,
}

/// Follows a session transcript for task events.
///
/// The other transcript probes read a fixed tail, and this one cannot: a task's
/// start and its completion are separate records that can be minutes and
/// megabytes apart, so a tail can hold either half or neither. It is the same
/// shape of problem `title.rs` records — a title 1.8MB from the end of a 1.9MB
/// transcript — and the same one-full-scan answer, except that a transcript is
/// append-only, so after the first scan everything new is one window.
///
/// So: one scan when a session is first seen, then a read of the appended bytes
/// whenever mtime moves, and no read at all when it has not.
pub struct TranscriptTasks {
    projects_dir: PathBuf,
    cache: Mutex<HashMap<String, CachedTasks>>,
}

impl TranscriptTasks {
    pub fn new(projects_dir: PathBuf) -> Self {
        Self {
            projects_dir,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn modified_ms(path: &std::path::Path) -> Option<i64> {
        let modified = std::fs::metadata(path).ok()?.modified().ok()?;
        let since_epoch = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(since_epoch.as_millis() as i64)
    }

    /// The size guard, taken as an argument so a test can exercise the
    /// fall-back without writing a 32MB file.
    fn read_within(
        &self,
        cwd: &str,
        session_id: &str,
        started_at_ms: i64,
        max_scan_bytes: u64,
    ) -> Option<Vec<Task>> {
        use crate::bridge::transcript::{find_transcript, read_range, read_tail, TAIL_BYTES};

        let path = find_transcript(&self.projects_dir, cwd, session_id)?;
        // No mtime means no cache key, so read rather than guess.
        let mtime = Self::modified_ms(&path)?;
        let len = std::fs::metadata(&path).ok()?.len();

        let (mut tasks, from) = {
            let cache = self.cache.lock().expect("task cache poisoned");
            match cache.get(session_id) {
                Some(entry) if entry.at_ms == mtime => return Some(entry.tasks.clone()),
                // A file shorter than what has been consumed is not the file
                // that was consumed. Start again rather than splicing two.
                Some(entry) if entry.consumed <= len => (entry.tasks.clone(), entry.consumed),
                _ => (Vec::new(), 0),
            }
        };

        let (bytes, window_from) = if from == 0 && len > max_scan_bytes {
            // Too big to read whole. The tail still holds anything started
            // recently, and the offset is set past it so the follow continues
            // from here — reporting nothing at all would take the state with
            // it.
            (read_tail(&path, TAIL_BYTES).ok()?, len)
        } else {
            (read_range(&path, from, len).ok()?, from)
        };

        // Stop at the last newline. A transcript can be read mid-write, and
        // consuming the fragment would lose the record it belongs to.
        let complete = bytes
            .iter()
            .rposition(|b| *b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        apply_events(&mut tasks, &task_events(&bytes[..complete]), started_at_ms);

        let consumed = if window_from == len {
            len
        } else {
            window_from + complete as u64
        };

        self.cache.lock().expect("task cache poisoned").insert(
            session_id.to_string(),
            CachedTasks {
                at_ms: mtime,
                consumed,
                tasks: tasks.clone(),
            },
        );

        Some(tasks)
    }
}

impl TaskProbe for TranscriptTasks {
    fn tasks(&self, cwd: &str, session_id: &str, started_at_ms: i64) -> Vec<Task> {
        use crate::bridge::transcript::FULL_SCAN_MAX_BYTES;

        self.read_within(cwd, session_id, started_at_ms, FULL_SCAN_MAX_BYTES)
            .unwrap_or_default()
    }
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

    struct Fixture {
        root: PathBuf,
        transcript: PathBuf,
    }

    impl Fixture {
        fn new(tag: &str, body: &str) -> Self {
            let root = std::env::temp_dir().join(format!("cb-tasks-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let dir = root.join("-Users-n-Code-proj");
            std::fs::create_dir_all(&dir).unwrap();
            let transcript = dir.join("session-1.jsonl");
            std::fs::write(&transcript, body).unwrap();
            Self { root, transcript }
        }

        fn probe(&self) -> TranscriptTasks {
            TranscriptTasks::new(self.root.clone())
        }

        fn ask(&self, probe: &TranscriptTasks) -> Vec<Task> {
            probe.tasks("/Users/n/Code/proj", "session-1", 0)
        }

        fn mtime(&self) -> std::time::SystemTime {
            std::fs::metadata(&self.transcript)
                .unwrap()
                .modified()
                .unwrap()
        }

        /// Append, and move mtime on by a second.
        ///
        /// Setting the mtime rather than hoping for one, for the reason
        /// `working.rs` records: the cache key is whole milliseconds and two
        /// writes in a row land inside the same one often enough to matter.
        fn append(&self, body: &str) {
            let was = self.mtime();
            let existing = std::fs::read_to_string(&self.transcript).unwrap();
            std::fs::write(&self.transcript, format!("{existing}{body}")).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&self.transcript)
                .unwrap()
                .set_modified(was + std::time::Duration::from_secs(1))
                .unwrap();
        }

        /// Replace the whole file with something shorter, mtime advanced.
        fn truncate_to(&self, body: &str) {
            let was = self.mtime();
            std::fs::write(&self.transcript, body).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&self.transcript)
                .unwrap()
                .set_modified(was + std::time::Duration::from_secs(1))
                .unwrap();
        }

        /// Rewrite while pinning mtime, so a re-read would be visible in the
        /// answer and a cache hit would not.
        fn rewrite_keeping_mtime(&self, body: &str) {
            let was = self.mtime();
            std::fs::write(&self.transcript, body).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&self.transcript)
                .unwrap()
                .set_modified(was)
                .unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_running_task_is_reported_from_the_transcript() {
        let fixture = Fixture::new("running", SHELL_START);
        let tasks = fixture.ask(&fixture.probe());
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "bmd0i64ke");
        assert_eq!(tasks[0].status, TaskStatus::Running);
    }

    #[test]
    fn a_completion_appended_later_is_picked_up() {
        let fixture = Fixture::new("appended", SHELL_START);
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe)[0].status, TaskStatus::Running);

        fixture.append(SHELL_DONE);
        assert_eq!(fixture.ask(&probe)[0].status, TaskStatus::Completed);
    }

    #[test]
    fn an_unchanged_transcript_is_answered_from_cache() {
        let fixture = Fixture::new("cache-hit", SHELL_START);
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe).len(), 1);

        fixture.rewrite_keeping_mtime("");
        assert_eq!(
            fixture.ask(&probe).len(),
            1,
            "same mtime should not be re-read"
        );
    }

    #[test]
    fn a_truncated_transcript_is_re_scanned_rather_than_read_from_where_it_was() {
        // Reading from the old offset would either fail or splice two
        // different files together.
        let fixture = Fixture::new("truncated", &format!("{SHELL_START}{SHELL_DONE}"));
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe)[0].status, TaskStatus::Completed);

        fixture.truncate_to(SHELL_START);
        let tasks = fixture.ask(&probe);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, TaskStatus::Running);
    }

    #[test]
    fn a_partial_trailing_line_is_not_consumed() {
        // A transcript can be read mid-write. Consuming the fragment would
        // lose the record it belongs to when the rest of it lands.
        let fixture = Fixture::new("partial", SHELL_START);
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe).len(), 1);

        // The completion arrives without its newline, then completed.
        let half = SHELL_DONE.trim_end_matches('\n');
        fixture.append(half);
        assert_eq!(
            fixture.ask(&probe)[0].status,
            TaskStatus::Running,
            "an unterminated line is not a record yet"
        );

        fixture.append("\n");
        assert_eq!(fixture.ask(&probe)[0].status, TaskStatus::Completed);
    }

    #[test]
    fn a_start_before_the_session_began_is_not_reported() {
        let fixture = Fixture::new("boundary", SHELL_START);
        // The start record is stamped 2026-08-28T08:42:47.177Z.
        let after = 1_787_906_567_178;
        assert!(fixture
            .probe()
            .tasks("/Users/n/Code/proj", "session-1", after)
            .is_empty());
    }

    #[test]
    fn a_missing_transcript_reports_nothing() {
        let probe = TranscriptTasks::new(std::env::temp_dir().join("cb-tasks-missing"));
        assert!(probe.tasks("/Users/n/Code/proj", "session-1", 0).is_empty());
    }

    #[test]
    fn a_transcript_past_the_size_guard_falls_back_to_its_tail() {
        // Reporting nothing for a huge transcript would take the state with
        // it; the tail still holds anything started recently.
        let fixture = Fixture::new("huge", SHELL_START);
        let tasks = TranscriptTasks::new(fixture.root.clone()).read_within(
            "/Users/n/Code/proj",
            "session-1",
            0,
            1,
        );
        assert_eq!(tasks.unwrap().len(), 1);
    }

    #[test]
    fn the_fake_answers_from_its_table() {
        let task = Task {
            id: "t".into(),
            kind: TaskKind::Shell,
            label: None,
            started_at_ms: 1,
            ended_at_ms: None,
            status: TaskStatus::Running,
        };
        let fake = FakeTasks::new().with("session-1", vec![task.clone()]);
        assert_eq!(fake.tasks("/any", "session-1", 0), vec![task]);
        assert!(fake.tasks("/any", "session-2", 0).is_empty());
    }

    #[test]
    fn the_no_op_probe_reports_nothing() {
        assert!(NoTasks.tasks("/any", "session-1", 0).is_empty());
    }
}
