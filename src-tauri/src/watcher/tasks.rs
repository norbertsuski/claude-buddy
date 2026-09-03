use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::bridge::transcript::{assistant_content, clip_to, message_content};
use buddy_core::watcher::task::{Task, TaskKind, TaskProbe, TaskStatus};

/// Longest task label the popover will draw. The same width `latest_activity`
/// clips to, because they sit one under the other in the same popover.
pub const LABEL_MAX_CHARS: usize = crate::bridge::transcript::ACTIVITY_MAX_CHARS;

/// One half of a task's life, as recorded in a transcript.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskEvent {
    Started {
        id: String,
        kind: TaskKind,
        label: Option<String>,
        /// Where the task writes, from the text of its own start result.
        output: Option<String>,
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
    task_events_with(bytes, &mut ToolCalls::default())
}

/// Every task event in these bytes, with a memory of the calls earlier windows
/// carried.
///
/// A task's start is two records: the assistant's `tool_use`, which is the only
/// place its kind and its description live, and the result that reports the new
/// task id. The incremental probe hands over only what was appended since the
/// last tick, so those two records routinely land in different windows — the
/// spec's own sample pair is seven seconds apart and a tick is two. Without the
/// carried calls a background agent read one tick late became a nameless
/// `Watch`, and stayed one for its whole life, since nothing re-derives a kind.
pub fn task_events_with(bytes: &[u8], calls: &mut ToolCalls) -> Vec<TaskEvent> {
    let text = String::from_utf8_lossy(bytes);
    let records: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    // A whole window first, so a call later in the file than the result it
    // belongs to is still found.
    for record in &records {
        remember_tool_uses(record, calls);
    }

    records
        .iter()
        .filter_map(|record| {
            let at_ms = record
                .get("timestamp")
                .and_then(|v| v.as_str())
                .and_then(buddy_core::rfc3339::epoch_ms)?;
            started_event(record, calls, at_ms).or_else(|| ended_event(record, at_ms))
        })
        .collect()
}

/// How many tool calls are remembered between reads.
///
/// Only the last call of a window can still be missing its result, so one
/// would nearly always do; this is deep enough that a burst of calls issued in
/// one assistant turn cannot push the pending one out, and small enough that a
/// per-session cache entry stays a few kilobytes.
const REMEMBERED_CALLS: usize = 64;

/// Tool calls seen so far, by `tool_use` id.
///
/// Owned rather than borrowed from the records, because it outlives the window
/// it was read from. Bounded and oldest-out, so following a session for hours
/// costs a fixed amount of memory.
#[derive(Debug, Clone, Default)]
pub struct ToolCalls {
    by_id: HashMap<String, (String, Option<String>)>,
    order: VecDeque<String>,
}

impl ToolCalls {
    fn insert(&mut self, id: &str, name: &str, description: Option<&str>) {
        let call = (name.to_string(), description.map(str::to_string));
        if self.by_id.insert(id.to_string(), call).is_some() {
            return;
        }
        self.order.push_back(id.to_string());
        if self.order.len() > REMEMBERED_CALLS {
            if let Some(oldest) = self.order.pop_front() {
                self.by_id.remove(&oldest);
            }
        }
    }

    fn get(&self, id: &str) -> Option<(&str, Option<&str>)> {
        self.by_id
            .get(id)
            .map(|(name, description)| (name.as_str(), description.as_deref()))
    }
}

/// How many tasks one session's list may hold.
///
/// A session that has run for hours has run hundreds of background commands,
/// and every one of them would otherwise sit in a snapshot that is emitted to
/// the frontend. The newest are kept: a finished task matters for the seconds
/// it takes to alert about it, and a running one is always among the newest.
pub const MAX_TASKS: usize = 50;

/// How much of a task's output file is read to find its closing marker.
///
/// The marker is the last line, so this only has to be longer than one line of
/// output. Generous, because the line before it can be arbitrarily long and a
/// read that lands mid-line still finds the marker after it.
const OUTPUT_TAIL_BYTES: u64 = 4096;

/// Fold events into an existing task list.
///
/// Additive and repeatable, because the probe applies each newly appended
/// window of a transcript to the list it already had, and a window can be read
/// twice when a file is truncated and re-scanned.
///
/// `events` must be oldest-first, as `task_events` emits them: an `Ended`
/// ahead of its own `Started` in the same slice finds no running task and is
/// dropped, leaving the task stuck `Running`.
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
                output,
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
                    output: output.clone(),
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

    // Over the cap, finished tasks go first. Draining by age alone let fifty
    // completed shells evict a dev server that was still running and had been
    // started before all of them — the session left `tasking` while its one
    // real task was still going. By position among the terminal tasks rather
    // than by age, because nothing here may read the clock.
    let over = tasks.len().saturating_sub(MAX_TASKS);
    if over > 0 {
        let mut dropped = 0;
        tasks.retain(|t| {
            let expendable = dropped < over && t.status.terminal();
            if expendable {
                dropped += 1;
            }
            !expendable
        });
    }
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

/// One session's cached answer.
struct CachedTasks {
    /// Transcript mtime the answer was read at.
    at_ms: i64,
    /// How much of the file has been folded in. The next read starts here.
    consumed: u64,
    tasks: Vec<Task>,
    /// The tool calls the consumed bytes named, for the results that have not
    /// arrived yet.
    calls: ToolCalls,
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

/// Only the tasks a session that began at `started_at_ms` could have started.
///
/// The phantom boundary, applied to what a caller is told rather than only to
/// the events a read newly parsed. The cache is keyed on the session id alone,
/// and a session id outlives a process: a resumed session appends to the same
/// transcript, and during dead retention the dead entry and the new one ask in
/// the same tick with two different boundaries. Filtering the answer means
/// neither can inherit the other's.
fn since_start(tasks: Vec<Task>, started_at_ms: i64) -> Vec<Task> {
    tasks
        .into_iter()
        .filter(|t| t.started_at_ms >= started_at_ms)
        .collect()
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

    /// End any task whose own output file says it is over.
    ///
    /// The transcript is authoritative for everything except a kill, which it
    /// never records at all — so for a task still believed to be running, the
    /// file it writes to is the only witness. Returns whether anything changed,
    /// so the caller knows whether its cache is now stale.
    ///
    /// Only running tasks with a known output file are read, which in practice
    /// is none of them: a session with nothing in the background reads no files
    /// here at all.
    ///
    /// A missing or unreadable file leaves the task alone. Absence is not an
    /// ending — a swept temp directory would otherwise retire everything.
    fn retire_by_output(tasks: &mut [Task]) -> bool {
        let mut changed = false;

        for task in tasks.iter_mut() {
            if task.status.terminal() {
                continue;
            }
            let Some(path) = task.output.as_deref() else {
                continue;
            };
            let path = std::path::Path::new(path);
            // The marker is the last line, so the tail is all that is needed
            // however long the task has been logging.
            let Ok(bytes) = crate::bridge::transcript::read_tail(path, OUTPUT_TAIL_BYTES) else {
                continue;
            };
            let Some(status) = status_from_output(&bytes) else {
                continue;
            };

            task.status = status;
            // When the marker was written, not when it was noticed: a widget
            // that was not running while a task ended would otherwise date
            // every one of them to its own start.
            task.ended_at_ms = Self::modified_ms(path);
            changed = true;
        }

        changed
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

        let (mut tasks, mut calls, from) = {
            let cache = self.cache.lock().expect("task cache poisoned");
            match cache.get(session_id) {
                Some(entry) if entry.at_ms == mtime => {
                    // Still read the output files. A kill writes nothing to the
                    // transcript, so the mtime that answers everything else
                    // says nothing about whether a task is still alive.
                    let mut tasks = entry.tasks.clone();
                    drop(cache);
                    if Self::retire_by_output(&mut tasks) {
                        let mut cache = self.cache.lock().expect("task cache poisoned");
                        if let Some(entry) = cache.get_mut(session_id) {
                            entry.tasks = tasks.clone();
                        }
                    }
                    return Some(since_start(tasks, started_at_ms));
                }
                // A file shorter than what has been consumed is not the file
                // that was consumed. Start again rather than splicing two.
                Some(entry) if entry.consumed <= len => {
                    (entry.tasks.clone(), entry.calls.clone(), entry.consumed)
                }
                _ => (Vec::new(), ToolCalls::default(), 0),
            }
        };

        let (bytes, window_from) = if from == 0 && len > max_scan_bytes {
            // Too big to read whole. The tail still holds anything started
            // recently, and the offset is the tail read's true start — not
            // `len` — so the trailing-newline trim below still lands on a
            // real boundary within the window instead of being discarded.
            let tail_from = len.saturating_sub(TAIL_BYTES);
            (read_tail(&path, TAIL_BYTES).ok()?, tail_from)
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

        let events = task_events_with(&bytes[..complete], &mut calls);
        apply_events(&mut tasks, &events, started_at_ms);
        Self::retire_by_output(&mut tasks);

        // Bytes past `complete` (an unterminated trailing line) are left
        // unconsumed on both paths, so they are re-read — not lost — once
        // the rest of the line lands.
        let consumed = window_from + complete as u64;

        self.cache.lock().expect("task cache poisoned").insert(
            session_id.to_string(),
            CachedTasks {
                at_ms: mtime,
                consumed,
                tasks: tasks.clone(),
                calls,
            },
        );

        Some(since_start(tasks, started_at_ms))
    }
}

impl TaskProbe for TranscriptTasks {
    fn tasks(&self, cwd: &str, session_id: &str, started_at_ms: i64) -> Vec<Task> {
        use crate::bridge::transcript::FULL_SCAN_MAX_BYTES;

        self.read_within(cwd, session_id, started_at_ms, FULL_SCAN_MAX_BYTES)
            .unwrap_or_default()
    }
}

/// Record every `tool_use` this record carries, with its tool name and the
/// `description` its input held.
///
/// A task's start record names only its own new task id; what kind of task it
/// is, and what it is for, live on the call that produced it.
fn remember_tool_uses(record: &serde_json::Value, calls: &mut ToolCalls) {
    let Some(content) = assistant_content(record) else {
        return;
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
        calls.insert(id, name, description);
    }
}

/// The `tool_use_id` this record is a result for, if it is one.
fn result_for(record: &serde_json::Value) -> Option<&str> {
    message_content(record)?
        .iter()
        .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))?
        .get("tool_use_id")?
        .as_str()
}

/// Which kind of task a tool produces. The tool name decides where it is
/// known, and the id field is the fallback for a call this window of the
/// transcript did not include.
fn kind_for_tool(name: &str, fallback: TaskKind) -> TaskKind {
    match name {
        "Bash" => TaskKind::Shell,
        "Agent" | "Task" => TaskKind::Subagent,
        _ => fallback,
    }
}

fn started_event(record: &serde_json::Value, calls: &ToolCalls, at_ms: i64) -> Option<TaskEvent> {
    let result = record.get("toolUseResult")?;
    // Three tools, three id fields. A launched agent reports `agentId`, and
    // nothing else in its result is named like a task — reading only the other
    // two left every background agent out of the list, and left the
    // notification that ends one with no running task to retire.
    let (id, fallback) = [
        ("backgroundTaskId", TaskKind::Shell),
        ("agentId", TaskKind::Subagent),
        ("taskId", TaskKind::Watch),
    ]
    .into_iter()
    .find_map(|(field, kind)| Some((result.get(field)?.as_str()?, kind)))?;

    let call = result_for(record).and_then(|id| calls.get(id));
    let (kind, label) = match call {
        Some((name, description)) => (
            kind_for_tool(name, fallback),
            description.map(|d| clip_to(d, LABEL_MAX_CHARS)),
        ),
        // An agent's launch result repeats the description its call carried, so
        // one read a window late is still named. A shell's result says nothing
        // about what it ran, so there is nothing there to fall back to.
        None => (
            fallback,
            result
                .get("description")
                .and_then(|v| v.as_str())
                .map(|d| clip_to(d, LABEL_MAX_CHARS)),
        ),
    };

    Some(TaskEvent::Started {
        id: id.to_string(),
        kind,
        label,
        output: output_file(record, id),
        at_ms,
    })
}

/// The file a background task writes to, as named by its own start result.
///
/// Claude Code says it in prose rather than in a field of `toolUseResult`:
///
/// ```text
/// Command running in background with ID: b0b5tfx9k. Output is being written
/// to: /private/tmp/.../tasks/b0b5tfx9k.output. You will be notified when it
/// completes. To check interim output, use Read on that file path.
/// ```
///
/// Worth reading because that file is the *only* record of a task being
/// killed: a kill appends `[killed]` to it and writes nothing whatever to the
/// transcript.
///
/// The end of the path is found by looking for the task's own id rather than
/// by running to the end of the line, which is what the sentence above makes
/// clear it cannot do — two more sentences follow the path, unpunctuated by any
/// newline. `<id>.output` is how every one of these files is named, and a
/// result that does not contain it yields nothing rather than a guess.
///
/// The content of a `tool_result` is a bare string for a shell and an array of
/// blocks for an agent; both are handled, though only a shell has ever carried
/// this line.
fn output_file(record: &serde_json::Value, id: &str) -> Option<String> {
    const MARKER: &str = "Output is being written to: ";

    let content = message_content(record)?
        .iter()
        .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))?
        .get("content")?;

    let text = match content {
        serde_json::Value::String(s) => std::borrow::Cow::Borrowed(s.as_str()),
        serde_json::Value::Array(blocks) => std::borrow::Cow::Owned(
            blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => return None,
    };

    let start = text.find(MARKER)? + MARKER.len();
    let rest = &text[start..];
    let end = rest.find(&format!("{id}.output"))? + id.len() + ".output".len();
    let path = rest[..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// How a task ended, read from the last line of its own output file.
///
/// Claude Code closes the file with one of two markers, and only one of them
/// is also reported as a notification: a task that exits announces itself, and
/// a task that is *killed* does not. Without this a killed task stayed
/// `Running` for the life of the session, holding it in `tasking`.
///
/// The marker must be the whole of the last line. A task whose own output
/// contains the word must not retire itself by printing it.
fn status_from_output(bytes: &[u8]) -> Option<TaskStatus> {
    let text = String::from_utf8_lossy(bytes);
    let last = text.lines().filter(|l| !l.trim().is_empty()).next_back()?;

    match last.trim() {
        "[killed]" => Some(TaskStatus::Killed),
        line => {
            let code = line
                .strip_prefix("[exited with code ")?
                .strip_suffix(']')?
                .trim()
                .parse::<i32>()
                .ok()?;
            Some(if code == 0 {
                TaskStatus::Completed
            } else {
                TaskStatus::Failed
            })
        }
    }
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
/// Claude Code writes each notification three times — as a `queue-operation`
/// with `operation: "enqueue"` and the text in `content`, as an `attachment`
/// with the same text in `attachment.prompt`, and again as a `queue-operation`
/// with `operation: "remove"` once the turn has absorbed it. All three are
/// read: which lands first is not something to depend on, and the fold
/// deduplicates them anyway, since only a running task can end.
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
    use buddy_core::watcher::task::{FakeTasks, NoTasks};

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
                ..
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
        // The tool name wins over the id field's own fallback: a result whose
        // only id is a `taskId` is a watch by default, but not when the call
        // behind it was an Agent.
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
    fn a_background_agent_start_is_read_from_its_agent_id() {
        // A launched agent reports `agentId`, not `taskId`: the two id fields
        // belong to different tools. Reading only `taskId` left every
        // background agent out of the list, so a session running four of them
        // read `idle`, and the notification that ended one found no running
        // task to retire.
        let body = concat!(
            r#"{"type":"assistant","timestamp":"2026-09-02T19:36:50.000Z","message":{"content":[{"type":"tool_use","id":"toolu_4","name":"Agent","input":{"description":"Check the guard"}}]}}"#,
            "\n",
            r#"{"type":"user","timestamp":"2026-09-02T19:36:51.564Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_4"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a745d48aa8c8c4839","description":"Check the guard"}}"#,
            "\n",
        );
        match &events(body)[0] {
            TaskEvent::Started {
                id,
                kind,
                label,
                at_ms,
                ..
            } => {
                assert_eq!(id, "a745d48aa8c8c4839");
                assert_eq!(*kind, TaskKind::Subagent);
                assert_eq!(label.as_deref(), Some("Check the guard"));
                assert_eq!(*at_ms, 1788377811564);
            }
            other => panic!("expected a start, got {other:?}"),
        }
    }

    #[test]
    fn an_agent_start_without_its_call_still_names_itself() {
        // The launch result carries its own `description`, so an agent whose
        // `tool_use` landed in an earlier window is still named — unlike a
        // shell, whose result says nothing about what it ran.
        let body = concat!(
            r#"{"type":"user","timestamp":"2026-09-02T19:36:51.564Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_4"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a745d48aa8c8c4839","description":"Check the guard"}}"#,
            "\n",
        );
        match &events(body)[0] {
            TaskEvent::Started { kind, label, .. } => {
                assert_eq!(*kind, TaskKind::Subagent);
                assert_eq!(label.as_deref(), Some("Check the guard"));
            }
            other => panic!("expected a start, got {other:?}"),
        }
    }

    #[test]
    fn an_agent_ends_on_the_notification_that_carries_its_agent_id() {
        // Claude Code writes the agent's id as the notification's `task-id`,
        // which is what lets a start read from `agentId` meet its own ending.
        let body = concat!(
            r#"{"type":"user","timestamp":"2026-09-02T19:36:51.564Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_4"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a745d48aa8c8c4839","description":"Check the guard"}}"#,
            "\n",
            r#"{"type":"queue-operation","timestamp":"2026-09-02T19:40:00.000Z","content":"<task-notification>\n<task-id>a745d48aa8c8c4839</task-id>\n<status>completed</status>\n<summary>Agent \"Check the guard\" finished</summary>\n</task-notification>"}"#,
            "\n",
        );
        let tasks = tasks_from_events(&events(body), 0);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, TaskStatus::Completed);
        assert_eq!(tasks[0].kind, TaskKind::Subagent);
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
            output: None,
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

    #[test]
    fn a_running_task_survives_a_crowd_of_finished_ones() {
        // The dev server launched first, then fifty shells came and went. The
        // cap drained the oldest and the still-running server went with them:
        // the session dropped out of `tasking` and the popover forgot the one
        // task it was actually waiting on.
        let mut events = vec![started("dev-server", SESSION_START)];
        for i in 0..MAX_TASKS + 10 {
            let at = SESSION_START + 1 + i as i64;
            events.push(started(&format!("t{i}"), at));
            events.push(ended(&format!("t{i}"), TaskStatus::Completed, at));
        }

        let tasks = tasks_from_events(&events, SESSION_START);

        assert_eq!(tasks.len(), MAX_TASKS);
        assert!(
            tasks.iter().any(|t| t.id == "dev-server"),
            "a finished task must not evict a running one"
        );
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
    fn a_task_id_read_after_its_call_keeps_the_call_kind_and_label() {
        // The `tool_use` and the result that reports the task id are separate
        // records, and a tick can fall between them: the spec's own sample
        // records are seven seconds apart against a two-second tick. The
        // window carrying the result then holds no call to look the kind and
        // the description up in, and the task landed as a nameless `Watch`.
        let call = concat!(
            r#"{"type":"assistant","timestamp":"2026-08-28T09:00:00.000Z","message":{"content":[{"type":"tool_use","id":"toolu_5","name":"Agent","input":{"description":"Audit the payment flow"}}]}}"#,
            "\n",
        );
        let result = concat!(
            r#"{"type":"user","timestamp":"2026-08-28T09:00:07.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_5"}]},"toolUseResult":{"taskId":"agent9","timeoutMs":600000}}"#,
            "\n",
        );

        let fixture = Fixture::new("split-call", call);
        let probe = fixture.probe();
        assert!(fixture.ask(&probe).is_empty(), "no task has started yet");

        fixture.append(result);
        let tasks = fixture.ask(&probe);
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].kind,
            TaskKind::Subagent,
            "an agent read a tick after its call is still an agent"
        );
        assert_eq!(tasks[0].label.as_deref(), Some("Audit the payment flow"));
    }

    /// A start whose result names the file the task writes to.
    ///
    /// The `content` string is Claude Code's own, to the letter: the path is
    /// mid-sentence with two more sentences after it and no newline anywhere,
    /// which is what defeated a parser that read to the end of the line.
    /// `OUTPUT_PATH` is the only substitution.
    const SHELL_START_WITH_OUTPUT: &str = concat!(
        r#"{"type":"assistant","timestamp":"2026-08-28T08:42:40.000Z","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"npm test","description":"Run the suite","run_in_background":true}}]}}"#,
        "\n",
        r#"{"type":"user","timestamp":"2026-08-28T08:42:47.177Z","message":{"content":[{"tool_use_id":"toolu_1","type":"tool_result","content":"Command running in background with ID: bmd0i64ke. Output is being written to: OUTPUT_PATH. You will be notified when it completes. To check interim output, use Read on that file path.","is_error":false}]},"toolUseResult":{"stdout":"","stderr":"","interrupted":false,"isImage":false,"noOutputExpected":false,"backgroundTaskId":"bmd0i64ke"}}"#,
        "\n",
    );

    #[test]
    fn a_start_records_the_file_its_task_writes_to() {
        // The only route to a killed task's ending: the kill is written into
        // that file and nowhere in the transcript.
        let body = SHELL_START_WITH_OUTPUT.replace("OUTPUT_PATH", "/tmp/tasks/bmd0i64ke.output");
        match &events(&body)[0] {
            TaskEvent::Started { output, .. } => {
                // Not "…bmd0i64ke.output. You will be notified when it
                // completes. …", which is what the rest of the line says.
                assert_eq!(output.as_deref(), Some("/tmp/tasks/bmd0i64ke.output"));
            }
            other => panic!("expected a start, got {other:?}"),
        }
    }

    #[test]
    fn a_start_whose_result_never_names_its_file_yields_nothing() {
        // Rather than the rest of a sentence. A result that does not carry the
        // `<id>.output` name is one this cannot read, and a wrong path would
        // be read as a missing file forever.
        let body = SHELL_START_WITH_OUTPUT.replace("OUTPUT_PATH", "/tmp/tasks/somewhere-else");
        match &events(&body)[0] {
            TaskEvent::Started { output, .. } => assert_eq!(*output, None),
            other => panic!("expected a start, got {other:?}"),
        }
    }

    #[test]
    fn a_start_that_names_no_output_file_has_none() {
        match &events(SHELL_START)[0] {
            TaskEvent::Started { output, .. } => assert_eq!(*output, None),
            other => panic!("expected a start, got {other:?}"),
        }
    }

    #[test]
    fn an_output_tail_reports_how_its_task_ended() {
        assert_eq!(
            status_from_output(b"tick 3\n[killed]\n"),
            Some(TaskStatus::Killed)
        );
        assert_eq!(
            status_from_output(b"tick 3\n[exited with code 0]\n"),
            Some(TaskStatus::Completed)
        );
        assert_eq!(
            status_from_output(b"tick 3\n[exited with code 2]\n"),
            Some(TaskStatus::Failed)
        );
    }

    #[test]
    fn an_output_tail_with_no_marker_is_still_running() {
        assert_eq!(status_from_output(b"dummy-1 tick 4\n"), None);
        assert_eq!(status_from_output(b""), None);
        // A line that merely looks like one. The marker is the whole last line.
        assert_eq!(status_from_output(b"see [killed] in the log\n"), None);
    }

    #[test]
    fn a_task_killed_without_a_notification_is_retired() {
        // The bug: killing a background task writes `[killed]` into its output
        // file and leaves the transcript untouched, so a task that only ever
        // ended by notification stayed `Running` for as long as the session did.
        let fixture = Fixture::new("killed", "");
        let output = fixture.root.join("bmd0i64ke.output");
        std::fs::write(&output, "tick 1\n").unwrap();
        fixture.append(&SHELL_START_WITH_OUTPUT.replace("OUTPUT_PATH", output.to_str().unwrap()));

        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe)[0].status, TaskStatus::Running);

        std::fs::write(&output, "tick 1\n[killed]\n").unwrap();
        // The transcript has not moved — only the output file has, which is
        // the whole point.
        let tasks = fixture.ask(&probe);
        assert_eq!(tasks[0].status, TaskStatus::Killed);
        assert!(
            tasks[0].ended_at_ms.is_some(),
            "ended when the marker landed"
        );
    }

    #[test]
    fn a_task_whose_output_file_is_gone_is_left_alone() {
        // A missing file says nothing about the task. Retiring on absence
        // would kill every task whose temp directory had been swept.
        let fixture = Fixture::new("no-output", "");
        let missing = fixture.root.join("never-written.output");
        fixture.append(&SHELL_START_WITH_OUTPUT.replace("OUTPUT_PATH", missing.to_str().unwrap()));

        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe)[0].status, TaskStatus::Running);
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
    fn a_cached_start_before_a_later_session_start_is_dropped() {
        // A resumed session appends to the same transcript under the same id,
        // so the cache still holds the previous process's unfinished tasks.
        // The new process's later `startedAt` has to re-filter them, or the
        // session reads `tasking` on a dead dev server indefinitely — the
        // exact failure this boundary exists to remove the need for a timeout
        // to prevent.
        let fixture = Fixture::new("resumed", SHELL_START);
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe).len(), 1);

        // Anything at all, so mtime moves and the probe reads again.
        fixture.append(concat!(
            r#"{"type":"assistant","timestamp":"2026-08-28T09:10:00.000Z","message":{"content":[{"type":"text","text":"resumed"}]}}"#,
            "\n",
        ));

        // The start record is stamped 2026-08-28T08:42:47.177Z.
        let resumed_at = 1_787_906_567_178;
        assert!(
            probe
                .tasks("/Users/n/Code/proj", "session-1", resumed_at)
                .is_empty(),
            "a task the previous process started is not this one's"
        );
    }

    #[test]
    fn two_boundaries_in_one_tick_each_get_their_own_answer() {
        // During dead retention the dead entry and the resumed one share a
        // session id and both ask in the same tick with different `startedAt`.
        // Whichever reads the file first must not decide for the other.
        let fixture = Fixture::new("two-boundaries", SHELL_START);
        let probe = fixture.probe();
        assert_eq!(fixture.ask(&probe).len(), 1);

        let resumed_at = 1_787_906_567_178;
        assert!(
            probe
                .tasks("/Users/n/Code/proj", "session-1", resumed_at)
                .is_empty(),
            "a cache hit still owes the caller its own boundary"
        );
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
    fn a_fallback_read_defers_its_partial_trailing_line_instead_of_losing_it() {
        // The fallback used to set `consumed` to the whole file length
        // regardless of where the tail read actually stopped, so a line
        // still being written when the fallback fired was marked consumed
        // without ever being parsed — silently dropped instead of deferred.
        let half = SHELL_DONE.trim_end_matches('\n');
        let fixture = Fixture::new("huge-partial", &format!("{SHELL_START}{half}"));
        let probe = TranscriptTasks::new(fixture.root.clone());

        let tasks = probe
            .read_within("/Users/n/Code/proj", "session-1", 0, 1)
            .unwrap();
        assert_eq!(
            tasks[0].status,
            TaskStatus::Running,
            "the completion's line is not terminated yet"
        );

        // Complete the line that fell past the fallback's boundary.
        fixture.append("\n");
        let tasks = probe
            .read_within("/Users/n/Code/proj", "session-1", 0, 1)
            .unwrap();
        assert_eq!(
            tasks[0].status,
            TaskStatus::Completed,
            "the deferred line should be picked up on the next read, not lost"
        );
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
            output: None,
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
