# Session Task Monitoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A session sitting still only because a background task is running reads `tasking` rather than `paused`, and its popover names the tasks.

**Architecture:** A new `TaskProbe` (`src-tauri/src/watcher/tasks.rs`) pairs task-start records in a session transcript against `<task-notification>` completion records, read incrementally because transcripts are append-only. `state::snapshot` maps a would-be `Idle`/`Paused` session with a running task to a sixth `SessionState::Tasking`, and folds live registry `bg` jobs onto their parent by `cwd`. `diff_alerts` gains a task-list diff, because a finishing task wakes its session and the state edge lands on `Busy` rather than on the task.

**Tech Stack:** Rust (Tauri v2), `serde_json` for transcript records, React 19 + TypeScript, Vitest, no new dependencies.

## Global Constraints

- macOS only. There is no other platform behind a feature flag.
- Run `git status` before starting and again before staging. A file you did not touch showing as modified belongs to another agent — leave it alone and say so.
- Stage explicit paths. Never `git add -A`, never `git commit -a`.
- Never commit real session data. `fixtures/` is the only acceptable source for a screenshot.
- Before every commit: `npm run typecheck && npm test` and `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`. `--test-threads=1` is not optional — the watcher-loop tests use real files and real wall-clock time.
- Do not reformat files you are not changing.
- Conventional commit subjects (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`), no scopes, a body explaining the reasoning, and keep the `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` trailer.
- Comments explain *why*, never *what*.
- Serialised names are `camelCase` in Rust and must match `src/types.ts` exactly.
- New state name: `Tasking` in Rust, `'tasking'` on the wire and in TypeScript.
- State rank order, exactly: `Waiting 0, Busy 1, Tasking 2, Idle 3, Paused 4, Dead 5`.
- Task kinds, exactly: `Shell`, `Watch`, `Subagent`, `Job` — serialised `"shell"`, `"watch"`, `"subagent"`, `"job"`.
- Task statuses, exactly: `Running`, `Completed`, `Failed`, `Killed`, `Stopped` — serialised lowercase.
- Spec: `docs/superpowers/specs/2026-09-02-session-tasks-design.md`.

**One deliberate deviation from the spec.** The spec puts the transcript parsing in `bridge/transcript.rs` beside `has_work_in_flight`. It goes in `watcher/tasks.rs` instead: the parsing returns the task domain types, and no file under `bridge/` imports from `watcher/` today. Inverting that direction for one parser is worse than keeping the parser next to the types it produces. `bridge/transcript.rs` keeps the generic file and record helpers, which `tasks.rs` reuses.

## File Structure

**Created**

| File | Responsibility |
|---|---|
| `src-tauri/src/rfc3339.rs` | RFC 3339 to epoch millis. Moved out of `usage.rs`, which is no longer the only caller. |
| `src-tauri/src/watcher/tasks.rs` | Task types, transcript event parsing, the fold, and the `TaskProbe` trait with its transcript-backed, no-op and fake implementations. |

**Modified**

| File | Change |
|---|---|
| `src-tauri/src/usage.rs` | Loses the date parser, calls `rfc3339::epoch_ms`. |
| `src-tauri/src/lib.rs` | Declares `rfc3339`, constructs `TranscriptTasks`. |
| `src-tauri/src/bridge/transcript.rs` | `assistant_content`, `message_content`, `clip_to` made public; `read_range` added; `FULL_SCAN_MAX_BYTES` given a single home. |
| `src-tauri/src/watcher/mod.rs` | Declares `tasks`. |
| `src-tauri/src/watcher/title.rs` | Its `FULL_SCAN_MAX_BYTES` becomes an alias of the transcript one. |
| `src-tauri/src/watcher/state.rs` | `SessionState::Tasking`, `SessionSnapshot.tasks`, the probe parameter, job folding, retention filter. |
| `src-tauri/src/watcher/watch.rs` | Probe threaded through `spawn_watcher`; `fingerprint` hashes task ids and statuses. |
| `src-tauri/src/watcher/alerts.rs` | `AlertKind::TaskDone` and the task diff. |
| `src-tauri/src/config.rs` | `alert_task_done`, defaulting to `true`. |
| `src-tauri/src/notify.rs` | `should_deliver` and `alert_text` arms for `TaskDone`. |
| `src-tauri/src/visibility.rs` | `nothingActive` counts `Tasking`. |
| `src-tauri/src/awake.rs` | `keepAwake` counts `Tasking`. |
| `src/types.ts` | `'tasking'`, `Task`, `SessionSnapshot.tasks`, `alertTaskDone`. |
| `src/format.ts` | `countByState` gains the key. |
| `src/views/dotRow/StateCounts.tsx` | `STATE_ORDER` gains `'tasking'`. |
| `src/views/dotRow/heat.ts` | `fire` counts tasking sessions, excluding `job`-only ones. |
| `src/views/dotRow/SessionPopover.tsx` | The tasks block. |
| `src/views/dotRow/dotRow.css` | `--tasking`, `.dot-tasking`, its animation, `.popover-tasks`. |
| `src/settings/SettingsPanel.tsx` | The fourth alert checkbox. |
| `fixtures/generate.sh` | A `tasking` cast member and the transcript it needs. |
| `fixtures/.gitignore`, `fixtures/README.md` | The one generated transcript. |
| `README.md`, `CHANGELOG.md` | User-visible documentation. |

---

### Task 1: Move the RFC 3339 parser out of `usage.rs`

`watcher/tasks.rs` needs to turn a transcript record's `timestamp` into epoch millis. `usage.rs` already has an exact hand-rolled parser for the same format, private. A second copy would be the worse of the two available mistakes, and `watcher` importing `usage` — the five-hour meter — would be the other. So it moves to its own module first. Pure move: no behaviour change.

**Files:**
- Create: `src-tauri/src/rfc3339.rs`
- Modify: `src-tauri/src/usage.rs` (delete `epoch_ms`, `split_offset`, `days_from_civil` and their two tests; call the new module)
- Modify: `src-tauri/src/lib.rs` (declare the module)

**Interfaces:**
- Consumes: nothing.
- Produces: `pub fn epoch_ms(text: &str) -> Option<i64>` in `crate::rfc3339`.

- [ ] **Step 1: Create the module with the moved code**

Create `src-tauri/src/rfc3339.rs`. Move the three functions from `src-tauri/src/usage.rs` verbatim — they are correct and tested — changing only `fn epoch_ms` to `pub fn epoch_ms`, and add the module doc:

```rust
//! RFC 3339 timestamps to epoch milliseconds.
//!
//! Hand-rolled rather than pulling in a date crate: two callers, one format.
//! The five-hour meter reads `resets_at` out of the usage API, and the task
//! probe reads `timestamp` off transcript records. Both are RFC 3339, and
//! both shapes — `2026-08-25T10:50:00.070318+00:00` and
//! `2026-08-28T08:42:47.177Z` — are covered.

/// Epoch milliseconds for an RFC 3339 timestamp.
///
/// Fractional seconds are truncated, not rounded: the values this reads are a
/// reset time being counted down to and a task's start time being aged, and a
/// millisecond either way is not visible in either.
pub fn epoch_ms(text: &str) -> Option<i64> {
    // ... body moved unchanged from usage.rs ...
}

/// Split a trailing UTC offset off a time, returning the time and the offset in
/// seconds east of UTC.
fn split_offset(time: &str) -> Option<(&str, i64)> {
    // ... body moved unchanged from usage.rs ...
}

/// Days between the Unix epoch and a civil date, negative before it.
///
/// Howard Hinnant's `days_from_civil`, which is exact for the whole proleptic
/// Gregorian calendar and needs no table.
fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    // ... body moved unchanged from usage.rs ...
}

#[cfg(test)]
mod tests {
    use super::*;

    // The two tests move here unchanged from usage.rs:
    // `epoch_ms_handles_the_shapes_this_field_carries`
    // `epoch_ms_rejects_what_it_cannot_read`
}
```

- [ ] **Step 2: Declare it and point `usage.rs` at it**

In `src-tauri/src/lib.rs`, add `pub mod rfc3339;` in alphabetical order among the existing `pub mod` lines (after `pub mod notify;`).

In `src-tauri/src/usage.rs`, delete the three functions and the two moved tests, and change the one call site:

```rust
    let resets_at_ms = crate::rfc3339::epoch_ms(five_hour.get("resets_at")?.as_str()?)?;
```

- [ ] **Step 3: Run the suite to prove the move changed nothing**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`
Expected: PASS, with the same total count as before the move — the two tests are in `rfc3339` now instead of `usage`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/rfc3339.rs src-tauri/src/usage.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
refactor: give the RFC 3339 parser its own module

The task probe needs a transcript record's `timestamp` as epoch millis, and
`usage.rs` already parses exactly that format for `resets_at`. Copying it
would be one mistake and having `watcher` import the five-hour meter for a
date parser would be another, so it moves out whole, tests included.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Widen the transcript helpers the probe needs

`tasks.rs` reuses three private helpers and needs one new read. Doing this as its own task keeps the diff that changes `bridge/transcript.rs` free of task logic.

**Files:**
- Modify: `src-tauri/src/bridge/transcript.rs`
- Modify: `src-tauri/src/watcher/title.rs:24` (the `FULL_SCAN_MAX_BYTES` const)
- Test: `src-tauri/src/bridge/transcript.rs` (its own `mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces, all in `crate::bridge::transcript`:
  - `pub const FULL_SCAN_MAX_BYTES: u64`
  - `pub fn clip_to(text: &str, max_chars: usize) -> String`
  - `pub fn assistant_content(record: &serde_json::Value) -> Option<&Vec<serde_json::Value>>`
  - `pub fn message_content(record: &serde_json::Value) -> Option<&Vec<serde_json::Value>>`
  - `pub fn read_range(path: &Path, from: u64, to: u64) -> std::io::Result<Vec<u8>>`

- [ ] **Step 1: Write the failing test for `read_range`**

Add to the `mod tests` block in `src-tauri/src/bridge/transcript.rs`:

```rust
    #[test]
    fn read_range_returns_only_the_requested_window() {
        let path = std::env::temp_dir().join(format!("cb-range-{}.txt", std::process::id()));
        std::fs::write(&path, b"aaaa\nbbbb\ncccc\n").unwrap();

        assert_eq!(read_range(&path, 5, 10).unwrap(), b"bbbb\n");
        // Past the end is clamped rather than an error: the file can be
        // truncated between the stat and the read.
        assert_eq!(read_range(&path, 10, 999).unwrap(), b"cccc\n");
        // An inverted window is empty, not a panic.
        assert!(read_range(&path, 10, 5).unwrap().is_empty());

        std::fs::remove_file(&path).unwrap();
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test -- --test-threads=1 read_range_returns_only`
Expected: FAIL — `cannot find function read_range in this scope`.

- [ ] **Step 3: Add `read_range` and widen the three helpers**

In `src-tauri/src/bridge/transcript.rs`, after `read_tail`:

```rust
/// Read the bytes of a file between two offsets.
///
/// The companion of `read_tail` for a file being followed rather than sampled:
/// a transcript is append-only, so everything new since the last read is one
/// window. `to` past the end is clamped and an inverted window is empty,
/// because the file can be truncated between the stat that produced these
/// offsets and this read.
pub fn read_range(path: &Path, from: u64, to: u64) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let from = from.min(len);
    let to = to.min(len);
    if to <= from {
        return Ok(Vec::new());
    }
    file.seek(SeekFrom::Start(from))?;
    let mut buf = Vec::with_capacity((to - from) as usize);
    file.take(to - from).read_to_end(&mut buf)?;
    Ok(buf)
}
```

Add the size guard next to `TAIL_BYTES` at the top of the file, moved from `title.rs` so there is one of it:

```rust
/// Largest transcript worth reading end to end.
///
/// A transcript is an append-only log with no upper bound — one left running
/// for a week should not be read into memory whole on the strength of a maybe.
/// Shared by the title probe, which scans once for a title older than the tail,
/// and by the task probe, which scans once to find tasks that are still
/// running.
pub const FULL_SCAN_MAX_BYTES: u64 = 32 * 1024 * 1024;
```

Change three existing signatures from private to `pub` — bodies untouched:

```rust
pub fn clip_to(text: &str, max_chars: usize) -> String {
pub fn assistant_content(record: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
pub fn message_content(record: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
```

Add to each of the three a line saying why it is public, e.g. above `clip_to`:

```rust
/// Shorten to fit, on a character boundary, with an ellipsis.
///
/// Public because task labels are clipped to the same width by the same rule;
/// two truncations that disagreed would show up as two different ellipses in
/// one popover.
```

In `src-tauri/src/watcher/title.rs`, replace the const's value with the shared one, keeping the name its tests use:

```rust
/// Largest transcript worth scanning end to end for a title.
///
/// Only reached once per session, and only when the tail had nothing. The
/// figure itself lives in `bridge::transcript`, shared with the task probe.
pub const FULL_SCAN_MAX_BYTES: u64 = crate::bridge::transcript::FULL_SCAN_MAX_BYTES;
```

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`
Expected: PASS, including `read_range_returns_only_the_requested_window` and every existing `title` test.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/bridge/transcript.rs src-tauri/src/watcher/title.rs
git commit -m "$(cat <<'EOF'
refactor: widen the transcript helpers a second probe needs

The task probe follows a transcript rather than sampling its tail, so it
needs a windowed read, and it reuses the record accessors and the clip rule
rather than growing second copies that could disagree. The full-scan size
guard gets a single home now that two probes want it.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Task types and transcript event parsing

**Files:**
- Create: `src-tauri/src/watcher/tasks.rs`
- Modify: `src-tauri/src/watcher/mod.rs`
- Test: `src-tauri/src/watcher/tasks.rs` (`mod tests`)

**Interfaces:**
- Consumes: `crate::rfc3339::epoch_ms`; `crate::bridge::transcript::{assistant_content, message_content, clip_to}`.
- Produces:
  - `pub enum TaskKind { Shell, Watch, Subagent, Job }`
  - `pub enum TaskStatus { Running, Completed, Failed, Killed, Stopped }`
  - `pub struct Task { id: String, kind: TaskKind, label: Option<String>, started_at_ms: i64, ended_at_ms: Option<i64>, status: TaskStatus }`
  - `pub enum TaskEvent { Started { id, kind, label, at_ms }, Ended { id, status, label, at_ms } }`
  - `pub const LABEL_MAX_CHARS: usize`
  - `pub fn task_events(bytes: &[u8]) -> Vec<TaskEvent>`

- [ ] **Step 1: Declare the module**

In `src-tauri/src/watcher/mod.rs`, add `pub mod tasks;` in the existing alphabetical order (between `pub mod state;` and `pub mod title;`).

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/watcher/tasks.rs` containing only the test module for now, so the first run fails on the missing implementation:

```rust
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
        let body = r#"{"type":"queue-operation","timestamp":"2026-08-28T08:49:13.537Z","content":"<task-notification> <task-id>x</task-id> <status>failed</status> <summary>python3 &lt;&lt; &#39;PY&#39; &amp; wait</summary> </task-notification>"}"#;
        match &events(body)[0] {
            TaskEvent::Ended { label, .. } => {
                assert_eq!(label.as_deref(), Some("python3 << 'PY' & wait"))
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
                assert_eq!(label.as_deref().unwrap().chars().count(), LABEL_MAX_CHARS + 1)
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
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -- --test-threads=1 watcher::tasks`
Expected: FAIL to compile — `cannot find type TaskEvent in this scope`.

- [ ] **Step 4: Write the implementation**

Put this above the `mod tests` block in `src-tauri/src/watcher/tasks.rs`:

```rust
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
        None => (result.get("taskId").and_then(|v| v.as_str())?, TaskKind::Watch),
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1 watcher::tasks`
Expected: PASS, 12 tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/watcher/tasks.rs src-tauri/src/watcher/mod.rs
git commit -m "$(cat <<'EOF'
feat: read task start and completion events from a transcript

A session's background work is already recorded: a `backgroundTaskId` or a
`taskId` on the result that starts it, and a `<task-notification>` carrying
one of four terminal statuses when it ends. This reads both halves.

Forward rather than newest-first, unlike the other transcript parsers: the
two halves of one task can be megabytes apart, so order is the point. An
unrecognised status is not an ending, so a progress notification cannot
retire a task that is still running.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Fold events into tasks

**Files:**
- Modify: `src-tauri/src/watcher/tasks.rs`
- Test: `src-tauri/src/watcher/tasks.rs` (`mod tests`)

**Interfaces:**
- Consumes: `TaskEvent`, `Task`, `TaskKind`, `TaskStatus` from Task 3.
- Produces:
  - `pub const MAX_TASKS: usize`
  - `pub fn apply_events(tasks: &mut Vec<Task>, events: &[TaskEvent], since_ms: i64)`
  - `pub fn tasks_from_events(events: &[TaskEvent], since_ms: i64) -> Vec<Task>`

Clock-free deliberately. Retention of finished tasks is a question about *now* and belongs where the clock is, which is `state::snapshot`; a fold that filtered by age would go stale inside the probe's mtime-keyed cache and never be re-read.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src-tauri/src/watcher/tasks.rs`:

```rust
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
        apply_events(&mut tasks, &[started("a", SESSION_START + 1)], SESSION_START);
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -- --test-threads=1 watcher::tasks`
Expected: FAIL to compile — `cannot find function tasks_from_events in this scope`.

- [ ] **Step 3: Write the implementation**

Add to `src-tauri/src/watcher/tasks.rs`, after `task_events`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1 watcher::tasks`
Expected: PASS, 21 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/watcher/tasks.rs
git commit -m "$(cat <<'EOF'
feat: fold task events into a session's task list

Additive and repeatable, because the probe applies each appended window of a
transcript to the list it already holds. Clock-free: how long a finished task
stays visible is a question about now, and belongs where the clock is rather
than inside a cache keyed on a file's mtime.

Starts older than the session's own start time are dropped. A resumed session
appends to the same transcript, so without that boundary a dead process's
unfinished tasks would read as running for good — and no age cap could do the
job, since a dev server legitimately runs for hours.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: The transcript-backed probe

**Files:**
- Modify: `src-tauri/src/watcher/tasks.rs`
- Test: `src-tauri/src/watcher/tasks.rs` (`mod tests`)

**Interfaces:**
- Consumes: `apply_events`, `Task`, `TaskStatus`, `TaskKind` from Tasks 3–4; `crate::bridge::transcript::{find_transcript, read_range, read_tail, FULL_SCAN_MAX_BYTES, TAIL_BYTES}`.
- Produces:
  - `pub trait TaskProbe { fn tasks(&self, cwd: &str, session_id: &str, started_at_ms: i64) -> Vec<Task>; }`
  - `pub struct TranscriptTasks` with `pub fn new(projects_dir: PathBuf) -> Self`
  - `pub struct NoTasks`
  - `pub struct FakeTasks` with `pub fn new() -> Self` and `pub fn with(self, session_id: &str, tasks: Vec<Task>) -> Self`

Note the extra parameter compared with the other probes: the session's start time is the phantom boundary, and only the registry knows it.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src-tauri/src/watcher/tasks.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -- --test-threads=1 watcher::tasks`
Expected: FAIL to compile — `cannot find type TranscriptTasks in this scope`.

- [ ] **Step 3: Write the implementation**

Add to the top of `src-tauri/src/watcher/tasks.rs`:

```rust
use std::path::PathBuf;
use std::sync::Mutex;
```

Add after `tasks_from_events`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1 watcher::tasks`
Expected: PASS, 31 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/watcher/tasks.rs
git commit -m "$(cat <<'EOF'
feat: follow a transcript for a session's background tasks

A fixed tail cannot answer this: a task's start and its completion are
separate records, minutes and megabytes apart, so a tail holds one half or
neither — the same shape of problem the title probe hit and the same
one-full-scan answer. A transcript is append-only, so after that first scan
everything new is a single window, and an unchanged mtime is no read at all.

The read stops at the last newline, because a transcript can be read
mid-write and consuming the fragment would lose the record it belongs to. A
file shorter than what has been consumed is not the file that was consumed,
so it is scanned again rather than spliced.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: `SessionState::Tasking`

**Files:**
- Modify: `src-tauri/src/watcher/state.rs`
- Test: `src-tauri/src/watcher/state.rs` (`mod tests`)

**Interfaces:**
- Consumes: `TaskProbe`, `Task`, `TaskKind`, `TaskStatus`, `FakeTasks`, `NoTasks` from Task 5.
- Produces:
  - `SessionState::Tasking` (serialised `"tasking"`), rank 2
  - `SessionSnapshot.tasks: Vec<Task>` (serialised `tasks`)
  - `pub const TERMINAL_TASK_RETENTION_MS: i64`
  - `snapshot(...)` gains a `tasks: &dyn TaskProbe` parameter, after `work`

Every existing `snapshot(` call site — `watch.rs` (three, one of them in tests) and every `state.rs` test helper — must be updated in this task or the crate will not compile.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src-tauri/src/watcher/state.rs`. The existing helpers in that module call `snapshot` with a fixed argument list; add these alongside them:

```rust
    fn running_task(id: &str) -> crate::watcher::tasks::Task {
        crate::watcher::tasks::Task {
            id: id.to_string(),
            kind: crate::watcher::tasks::TaskKind::Shell,
            label: Some(format!("run {id}")),
            started_at_ms: NOW - 30_000,
            ended_at_ms: None,
            status: crate::watcher::tasks::TaskStatus::Running,
        }
    }

    fn finished_task(id: &str, ended_at_ms: i64) -> crate::watcher::tasks::Task {
        crate::watcher::tasks::Task {
            id: id.to_string(),
            kind: crate::watcher::tasks::TaskKind::Shell,
            label: Some(format!("{id} done")),
            started_at_ms: NOW - 60_000,
            ended_at_ms: Some(ended_at_ms),
            status: crate::watcher::tasks::TaskStatus::Completed,
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
        assert_eq!(session.tasks[0].kind, crate::watcher::tasks::TaskKind::Job);
        assert_eq!(session.tasks[0].label.as_deref(), Some("migrate-schemas"));
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -- --test-threads=1 watcher::state`
Expected: FAIL to compile — `no variant named Tasking found for enum SessionState`.

- [ ] **Step 3: Write the implementation**

In `src-tauri/src/watcher/state.rs`:

Add to the imports:

```rust
use crate::watcher::tasks::{Task, TaskKind, TaskProbe, TaskStatus};
```

Add the retention constant beside `DEAD_RETENTION_MS`:

```rust
/// How long a finished task stays in the snapshot.
///
/// Long enough for the alert diff to see the `Running`-to-terminal edge, which
/// it would otherwise miss: a finishing task wakes its session, so the same
/// tick usually moves the session's own state as well. Mirrors
/// `DEAD_RETENTION_MS` — a thing that happened once is worth showing once.
pub const TERMINAL_TASK_RETENTION_MS: i64 = 60 * 1000;
```

Add the variant and its rank:

```rust
pub enum SessionState {
    Waiting,
    Busy,
    Tasking,
    Idle,
    Paused,
    Dead,
}
```

```rust
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
```

Add the field to `SessionSnapshot`, after `background`:

```rust
    /// Background work this session is waiting on: background shells, watches,
    /// subagents, and the registry jobs that share its working directory.
    /// Finished tasks linger for `TERMINAL_TASK_RETENTION_MS` so the alert diff
    /// can see them end.
    pub tasks: Vec<Task>,
```

Add the two helpers above `snapshot`:

```rust
/// Every live registry job, paired with the working directory it shares with
/// its parent.
///
/// Jobs are separate processes and appear in no transcript, so this is the only
/// place they can come from. Matched by `cwd` because that is the only link the
/// registry offers — the same pairing `group_jobs_with_parents` performs for
/// the row itself.
///
/// Taken from the unfiltered registry deliberately: `show_background_jobs`
/// governs whether a job gets a row of its own, not whether its parent is
/// waiting on it.
fn job_tasks<'a>(
    files: &'a [RegistryFile],
    liveness: &dyn PidLiveness,
    now_ms: i64,
) -> Vec<(&'a str, Task)> {
    files
        .iter()
        .filter(|f| is_background_job(f.kind.as_deref(), f.job_id.as_deref()))
        .filter(|f| liveness.is_alive(f.pid, Some(f.started_at), now_ms))
        .map(|f| {
            (
                f.cwd.as_str(),
                Task {
                    id: f.job_id.clone().unwrap_or_default(),
                    kind: TaskKind::Job,
                    label: Some(display_name(f)),
                    started_at_ms: f.started_at,
                    ended_at_ms: None,
                    status: TaskStatus::Running,
                },
            )
        })
        .collect()
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
```

Add the parameter to `snapshot`, after `work`:

```rust
    tasks: &dyn TaskProbe,
```

Inside `snapshot`, before the `.map(|f| ...)`:

```rust
    let jobs = job_tasks(files, liveness, now_ms);
```

Inside the `.map(|f| ...)` closure, after `work_in_flight` is computed and before `let state = ...`:

```rust
            // The probe's own tasks, minus the ones that finished long enough
            // ago to have been alerted about, plus any registry job sharing
            // this session's working directory. A job entry gets no jobs of
            // its own: it is one.
            let mut session_tasks: Vec<Task> = tasks
                .tasks(&f.cwd, &f.session_id, f.started_at)
                .into_iter()
                .filter(|t| match t.ended_at_ms {
                    None => true,
                    Some(ended) => age(now_ms, ended) <= TERMINAL_TASK_RETENTION_MS,
                })
                .collect();
            if !is_background_job(f.kind.as_deref(), f.job_id.as_deref()) {
                session_tasks.extend(
                    jobs.iter()
                        .filter(|(cwd, _)| *cwd == f.cwd.as_str())
                        .map(|(_, task)| task.clone()),
                );
            }
            let has_running_task = session_tasks
                .iter()
                .any(|t| t.status == TaskStatus::Running);
```

Change the state derivation's tail so the existing `match` is unchanged and the new state is a post-map — the precedence rule is then one readable line rather than four scattered branches:

```rust
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
```

And the snapshot's `detail` and new field:

```rust
                detail: match state {
                    SessionState::Waiting => pending_prompt.or_else(|| f.waiting_for.clone()),
                    SessionState::Tasking => Some(task_detail(&session_tasks)),
                    _ => None,
                },
```

```rust
                tasks: session_tasks,
```

- [ ] **Step 4: Fix every call site and test helper**

`snapshot` now takes eleven arguments. Update:

- `src-tauri/src/watcher/watch.rs` — `spawn_watcher` gains the parameter now, because the `snapshot(` call inside it needs a probe and there is nowhere else to get one from. Add `tasks: Arc<dyn TaskProbe + Send + Sync>,` to the signature after `work`, `tasks.as_ref(),` to the `snapshot(` call after `work.as_ref(),`, `use crate::watcher::tasks::{NoTasks, TaskProbe};` to the imports, `Arc::new(NoTasks),` to every `spawn_watcher(` call in its own tests, and `&NoTasks,` to the `snapshot(` call in `the_snapshot_store_starts_empty_and_holds_what_it_is_given`. Task 7 changes only `fingerprint`.
- `src-tauri/src/lib.rs` — the `spawn_watcher` call gains, after the `TranscriptWork` argument:

```rust
                Arc::new(crate::watcher::tasks::TranscriptTasks::new(
                    crate::bridge::transcript::projects_dir(),
                )),
```

- Every `snapshot(` call in `state.rs`'s existing tests — add `&NoTasks,` after the `&NoWork,` argument.
- Every `SessionSnapshot { ... }` literal in tests — `alerts.rs`, `visibility.rs`, `awake.rs`, `question.rs`, and any in `state.rs` — needs `tasks: Vec::new(),`. The compiler lists them all.
- Add `use crate::watcher::tasks::{FakeTasks, NoTasks};` to `state.rs`'s test module.

Run: `cd src-tauri && cargo test -- --test-threads=1`
Expected: every compile error resolved; all tests pass.

- [ ] **Step 5: Run the full suite**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/watcher/state.rs src-tauri/src/watcher/watch.rs src-tauri/src/lib.rs src-tauri/src/watcher/alerts.rs src-tauri/src/visibility.rs src-tauri/src/awake.rs src-tauri/src/watcher/question.rs
git commit -m "$(cat <<'EOF'
feat: a session waiting on a background task reads tasking

A session that fires off a background test run goes quiet, and after ten
minutes it read `paused` — indistinguishable from one nobody is driving. It
now reads `tasking`, ranked between busy and idle, and carries the tasks it
is waiting on.

Only stillness becomes tasking. Waiting is the one state that needs the user
and must never be buried, busy is the session working on its own turn, and a
dead session is waiting on nothing.

Registry `bg` jobs are folded on by cwd, the same link `group_jobs_with_parents`
uses, and independently of `showBackgroundJobs`: that setting governs whether
a job gets a row, not whether its parent is waiting on it.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: The watcher loop notices a task starting

`fingerprint` is what stops the loop re-emitting twice a second, and it deliberately ignores clock-derived fields. A task starting or ending changes nothing it currently hashes, so without this the frontend would never hear about one on a session whose state does not move.

**Files:**
- Modify: `src-tauri/src/watcher/watch.rs`
- Test: `src-tauri/src/watcher/watch.rs` (`mod tests`)

**Interfaces:**
- Consumes: `SessionSnapshot.tasks` and `TaskStatus` from Task 6; `spawn_watcher`'s `tasks` parameter, already threaded through in Task 6.
- Produces: `type Fingerprint`, private to `watch.rs`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src-tauri/src/watcher/watch.rs`:

```rust
    use crate::watcher::tasks::{Task, TaskKind, TaskStatus};

    /// One tasking session holding one running task.
    fn tasking_snapshot() -> Vec<SessionSnapshot> {
        vec![SessionSnapshot {
            pid: 1,
            session_id: "s".into(),
            name: "proj".into(),
            title: None,
            cwd: "/Users/n/Code/proj".into(),
            entrypoint: "cli".into(),
            state: SessionState::Tasking,
            detail: Some("run tests".into()),
            elapsed_ms: 0,
            uptime_ms: 0,
            status_time_ms: 0,
            started_at_ms: 0,
            background: false,
            tasks: vec![Task {
                id: "t1".into(),
                kind: TaskKind::Shell,
                label: Some("run tests".into()),
                started_at_ms: 0,
                ended_at_ms: None,
                status: TaskStatus::Running,
            }],
        }]
    }

    #[test]
    fn a_task_finishing_re_emits_even_though_nothing_else_moved() {
        // Two snapshots differing only in a task's status are two different
        // things to draw. `fingerprint` ignores the clock on purpose, so
        // without hashing tasks this would be filtered out as unchanged.
        let before = tasking_snapshot();
        let mut after = before.clone();
        after[0].tasks[0].status = TaskStatus::Completed;
        after[0].tasks[0].ended_at_ms = Some(1_000);

        assert_ne!(fingerprint(&before), fingerprint(&after));
    }

    #[test]
    fn a_new_task_appearing_re_emits() {
        let before = tasking_snapshot();
        let mut after = before.clone();
        let mut second = after[0].tasks[0].clone();
        second.id = "t2".into();
        after[0].tasks.push(second);

        assert_ne!(fingerprint(&before), fingerprint(&after));
    }

    #[test]
    fn the_clock_moving_under_an_unchanged_task_does_not_re_emit() {
        // The whole point of the filter: a task getting a second older is not
        // a change, and hashing its age would re-emit every tick.
        let before = tasking_snapshot();
        let mut after = before.clone();
        after[0].elapsed_ms = 90_000;
        after[0].uptime_ms = 90_000;
        after[0].status_time_ms = 90_000;

        assert_eq!(fingerprint(&before), fingerprint(&after));
    }
```

- [ ] **Step 2: Run the tests to verify the first two fail**

Run: `cd src-tauri && cargo test -- --test-threads=1 watcher::watch::tests::a_task`
Expected: FAIL on both — `assertion failed: left != right`, because `fingerprint` does not look at tasks. The third test already passes, which is what makes it worth keeping: it is the guard against fixing the first two by hashing everything.

- [ ] **Step 3: Hash task ids and statuses**

In `src-tauri/src/watcher/watch.rs`, replace `fingerprint`:

```rust
/// Identity of a snapshot for change detection: everything the UI renders
/// except the clock-derived fields. Without this, elapsed time alone would make
/// every tick look like a change and the UI would re-render twice a second.
///
/// Tasks are in it by id and status, never by age. A task starting or finishing
/// is a change to draw — on a session whose own state does not move, it is the
/// *only* change — and a task getting a second older is not.
type Fingerprint = (
    String,
    SessionState,
    Option<String>,
    Option<String>,
    Vec<(String, TaskStatus)>,
);

fn fingerprint(sessions: &[SessionSnapshot]) -> Vec<Fingerprint> {
    sessions
        .iter()
        .map(|s| {
            (
                s.session_id.clone(),
                s.state,
                s.detail.clone(),
                // Retitling changes nothing else about a session, so without
                // this the row would keep the name it was first given until
                // something else moved.
                s.title.clone(),
                s.tasks
                    .iter()
                    .map(|t| (t.id.clone(), t.status))
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}
```

`TaskStatus` needs to be in scope at the top of the file; Task 6 already added `use crate::watcher::tasks::{NoTasks, TaskProbe};`, so extend that line to `use crate::watcher::tasks::{NoTasks, TaskProbe, TaskStatus};`.

- [ ] **Step 4: Run the full suite**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`
Expected: PASS, including `identical_state_does_not_re_emit`, which proves the new field did not make every tick look like a change.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/watcher/watch.rs
git commit -m "$(cat <<'EOF'
feat: emit an update when a session's tasks change

`fingerprint` ignores clock-derived fields so the loop does not re-render
twice a second, which meant a task starting on a quiet session changed
nothing it hashed and never reached the frontend. Tasks are now in it by id
and status — never by age, which would defeat the whole filter.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Alert when a task finishes

The edge is not a state transition. A finishing task wakes its session, so the state normally goes `Tasking → Busy`, and a state diff would report that as a session starting work rather than as a task landing.

**Files:**
- Modify: `src-tauri/src/watcher/alerts.rs`
- Test: `src-tauri/src/watcher/alerts.rs` (`mod tests`)

**Interfaces:**
- Consumes: `SessionSnapshot.tasks`, `Task`, `TaskStatus` from Task 6.
- Produces: `AlertKind::TaskDone` (serialised `"taskDone"`).

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src-tauri/src/watcher/alerts.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test -- --test-threads=1 watcher::alerts`
Expected: FAIL to compile — `no variant named TaskDone found for enum AlertKind`.

- [ ] **Step 3: Write the implementation**

In `src-tauri/src/watcher/alerts.rs`:

```rust
pub enum AlertKind {
    NeedsInput,
    Died,
    Finished,
    TaskDone,
}
```

Add above `diff_alerts`:

```rust
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

    next.iter()
        .flat_map(|session| {
            session.tasks.iter().filter_map(move |task| {
                if !task.status.terminal() {
                    return None;
                }
                // Only the transition is news. A finished task sits in several
                // consecutive snapshots while its retention window runs.
                was_running.get(&(session.session_id.as_str(), task.id.as_str()))?;
                Some(Alert {
                    session_id: session.session_id.clone(),
                    pid: session.pid,
                    name: session
                        .title
                        .clone()
                        .unwrap_or_else(|| session.name.clone()),
                    kind: AlertKind::TaskDone,
                    detail: Some(task_outcome(task)),
                })
            })
        })
        .collect()
}
```

Then split `diff_alerts` so both diffs run over the same pair:

```rust
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
            // ... existing body unchanged ...
        })
        .collect();

    alerts.extend(task_alerts(prev, next));
    alerts
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1 watcher::alerts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/watcher/alerts.rs
git commit -m "$(cat <<'EOF'
feat: alert when a background task finishes

Derived from the task list rather than the state edge, deliberately: a
finishing task wakes its session, so the same tick moves it out of `tasking`
and into `busy`. A state diff would report that as a session starting work
and would say nothing about the thing the user was actually waiting for.

The alert names the task and how it ended, so a failure does not read like a
success.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: The setting and the notification text

**Files:**
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/notify.rs`
- Test: `src-tauri/src/notify.rs` (`mod tests`)

**Interfaces:**
- Consumes: `AlertKind::TaskDone` from Task 8.
- Produces: `Config.alert_task_done: bool` (serialised `alertTaskDone`), default `true`.

Reusing `AlertKind::Finished` and its existing toggle was the cheaper option and is wrong: `Finished` means the session finished its turn, and one switch governing both would make it impossible to ask for only one of them.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src-tauri/src/notify.rs`:

```rust
    #[test]
    fn a_task_done_alert_is_gated_on_its_own_switch() {
        let alert = Alert {
            session_id: "a".into(),
            pid: 1,
            name: "api-service".into(),
            kind: AlertKind::TaskDone,
            detail: Some("npm test completed".into()),
        };

        let mut config = Config::default();
        config.sound = true;
        config.alert_task_done = true;
        assert!(should_deliver(&alert, &config, 0));

        config.alert_task_done = false;
        assert!(!should_deliver(&alert, &config, 0));

        // The sound is the parent of every event switch.
        config.alert_task_done = true;
        config.sound = false;
        assert!(!should_deliver(&alert, &config, 0));
    }

    #[test]
    fn a_task_done_alert_says_which_session_and_which_task() {
        let alert = Alert {
            session_id: "a".into(),
            pid: 1,
            name: "api-service".into(),
            kind: AlertKind::TaskDone,
            detail: Some("npm test failed".into()),
        };
        let (title, body) = alert_text(&alert);
        assert_eq!(title, "api-service finished a task");
        assert_eq!(body, "npm test failed");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test -- --test-threads=1 notify::tests::a_task_done`
Expected: FAIL to compile — `no field alert_task_done on type Config`.

- [ ] **Step 3: Add the setting and the two match arms**

In `src-tauri/src/config.rs`, after `alert_finished`:

```rust
    /// Whether a background task finishing interrupts you. On by default,
    /// unlike `alert_finished`: a task the user launched and walked away from
    /// is the case where they asked to be told, and it fires once per task
    /// rather than once per turn.
    pub alert_task_done: bool,
```

and in `impl Default for Config`, after `alert_finished: false,`:

```rust
            alert_task_done: true,
```

In `src-tauri/src/notify.rs`:

```rust
    match alert.kind {
        AlertKind::NeedsInput => config.alert_needs_input,
        AlertKind::Died => config.alert_died,
        AlertKind::Finished => config.alert_finished,
        AlertKind::TaskDone => config.alert_task_done,
    }
```

```rust
        AlertKind::TaskDone => (
            format!("{} finished a task", alert.name),
            alert
                .detail
                .clone()
                .unwrap_or_else(|| "a background task finished".to_string()),
        ),
```

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`
Expected: PASS. Any config test asserting the whole `Config::default()` will need the new field; the compiler and the assertion diff say which.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs src-tauri/src/notify.rs
git commit -m "$(cat <<'EOF'
feat: a switch and a notification for a finished task

Its own switch rather than reusing `alertFinished`: that one means the
session finished its turn, and one setting governing both would make it
impossible to ask for only one of them. On by default, because a task you
launched and walked away from is the case where being told is the point.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 10: The widget stays up while a task runs

**Files:**
- Modify: `src-tauri/src/visibility.rs`
- Modify: `src-tauri/src/awake.rs`
- Test: both files' `mod tests`

**Interfaces:**
- Consumes: `SessionState::Tasking` from Task 6.
- Produces: no new names.

- [ ] **Step 1: Write the failing tests**

In `src-tauri/src/visibility.rs`'s `mod tests`:

```rust
    #[test]
    fn a_tasking_session_counts_as_active() {
        // Hiding the widget while a task cooks would hide it at exactly the
        // moment its answer is wanted.
        assert!(!auto(&[session(SessionState::Tasking)], "nothingActive"));
    }
```

In `src-tauri/src/awake.rs`'s `mod tests`:

```rust
    #[test]
    fn a_tasking_session_holds_the_display_on() {
        assert!(should_stay_awake(&[session(SessionState::Tasking)], true));
    }
```

Match each file's existing `session(...)` test helper — both already have one; the compiler will say if the signature differs.

- [ ] **Step 2: Run them to verify they fail**

Run: `cd src-tauri && cargo test -- --test-threads=1 tasking_session`
Expected: FAIL — both assertions.

- [ ] **Step 3: Add the state to both policies**

In `src-tauri/src/visibility.rs`:

```rust
        "nothingActive" => !sessions.iter().any(|s| {
            matches!(
                s.state,
                SessionState::Waiting | SessionState::Busy | SessionState::Tasking
            )
        }),
```

In `src-tauri/src/awake.rs`, extend both the doc comment and the policy:

```rust
/// `Waiting` counts alongside `Busy` on purpose: a session blocked on a
/// permission prompt is the case where a sleeping, locked display costs the
/// user the most — the question is behind it and the run is going nowhere.
/// `Tasking` counts for the mirror-image reason: the session is going
/// somewhere, just not in its own transcript.
pub fn should_stay_awake(sessions: &[SessionSnapshot], keep_awake: bool) -> bool {
    keep_awake
        && sessions.iter().any(|s| {
            matches!(
                s.state,
                SessionState::Waiting | SessionState::Busy | SessionState::Tasking
            )
        })
}
```

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/visibility.rs src-tauri/src/awake.rs
git commit -m "$(cat <<'EOF'
feat: a running task keeps the widget up and the display awake

`nothingActive` would otherwise hide the widget at exactly the moment its
answer is wanted, and a forty-minute test run would end behind a locked
screen.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: The frontend types and the count

**Files:**
- Modify: `src/types.ts`
- Modify: `src/format.ts:69-79` (`countByState`)
- Modify: `src/views/dotRow/StateCounts.tsx:14` (`STATE_ORDER`)
- Test: `src/format.test.ts`, `src/views/dotRow/StateCounts.test.tsx`

**Interfaces:**
- Consumes: the Rust serialisation from Tasks 6, 8, 9 — `'tasking'`, `tasks`, `taskDone`, `alertTaskDone`.
- Produces:
  - `export type TaskKind = 'shell' | 'watch' | 'subagent' | 'job'`
  - `export type TaskStatus = 'running' | 'completed' | 'failed' | 'killed' | 'stopped'`
  - `export interface Task { id, kind, label, startedAtMs, endedAtMs, status }`
  - `SessionSnapshot.tasks: Task[]`

- [ ] **Step 1: Write the failing tests**

In `src/format.test.ts`, inside the existing `describe('countByState', …)` block. The file's factory is `function s(state: SessionState): SessionSnapshot` at line 25:

```ts
  it('counts tasking sessions', () => {
    const counts = countByState([s('tasking'), s('tasking'), s('idle')])
    expect(counts.tasking).toBe(2)
    expect(counts.idle).toBe(1)
  })
```

In `src/views/dotRow/StateCounts.test.tsx`. Its factory is `function session(name: string, state: SessionState): SessionSnapshot` at line 6, and `STATE_ORDER` is what decides the rendered order:

```tsx
  it('shows a tasking count between busy and idle', () => {
    render(
      <StateCounts
        sessions={[
          session('a', 'idle'),
          session('b', 'tasking'),
          session('c', 'busy'),
        ]}
      />,
    )
    const rendered = screen
      .getAllByTestId(/^count-/)
      .map((el) => el.getAttribute('data-testid'))
    expect(rendered).toEqual(['count-busy', 'count-tasking', 'count-idle'])
  })
```

Both factories build a `SessionSnapshot` literal, so both need `tasks: []` added once — the type check in Step 4 will say so.

- [ ] **Step 2: Run them to verify they fail**

Run: `npm test -- src/format.test.ts src/views/dotRow/StateCounts.test.tsx`
Expected: FAIL — `counts.tasking` is `undefined`, and `count-tasking` is not rendered.

- [ ] **Step 3: Write the implementation**

In `src/types.ts`:

```ts
export type SessionState = 'waiting' | 'busy' | 'tasking' | 'idle' | 'paused' | 'dead'

/** Mirrors watcher::tasks::TaskKind. */
export type TaskKind = 'shell' | 'watch' | 'subagent' | 'job'

/** Mirrors watcher::tasks::TaskStatus. */
export type TaskStatus = 'running' | 'completed' | 'failed' | 'killed' | 'stopped'

/**
 * One piece of background work a session is waiting on; mirrors
 * watcher::tasks::Task.
 *
 * Finished tasks stay in the snapshot for a minute after they end, so a list
 * is not the same thing as a list of running tasks — filter on `status`.
 */
export interface Task {
  id: string
  kind: TaskKind
  /** What the task is, from its notification or the call that started it. */
  label: string | null
  startedAtMs: number
  endedAtMs: number | null
  status: TaskStatus
}
```

Add to `SessionSnapshot`, after `background`:

```ts
  /** Background work this session is waiting on, running and just-finished. */
  tasks: Task[]
```

Add `'taskDone'` to `AlertKind`:

```ts
export type AlertKind = 'needsInput' | 'died' | 'finished' | 'taskDone'
```

Add to `AppConfig`, after `alertFinished`:

```ts
  /** Whether a background task finishing raises a notification. */
  alertTaskDone: boolean
```

In `src/format.ts`:

```ts
  const counts: Record<SessionState, number> = {
    waiting: 0,
    busy: 0,
    tasking: 0,
    idle: 0,
    paused: 0,
    dead: 0,
  }
```

In `src/views/dotRow/StateCounts.tsx`:

```ts
export const STATE_ORDER: SessionState[] = [
  'waiting',
  'dead',
  'busy',
  'tasking',
  'idle',
  'paused',
]
```

- [ ] **Step 4: Run the tests and the type check**

Run: `npm run typecheck && npm test`
Expected: PASS. `typecheck` will name every test file building a `SessionSnapshot` literal without `tasks`; add `tasks: []` to each.

- [ ] **Step 5: Commit**

```bash
git add src/types.ts src/format.ts src/format.test.ts src/views/dotRow/StateCounts.tsx src/views/dotRow/StateCounts.test.tsx
git commit -m "$(cat <<'EOF'
feat: carry tasks and the tasking state to the frontend

Mirrors the Rust types exactly, including the detail that a snapshot's task
list holds just-finished tasks as well as running ones — the retention window
the alert diff depends on is visible from here too.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 12: The dot

The dots are glyphs, not colours: waiting is a triangle, busy solid, idle a hollow ring, paused two bars, dead a cross. Colour alone is unreadable to a red-green colourblind user, which the README states outright, so `tasking` needs a shape of its own.

**Files:**
- Modify: `src/views/dotRow/dotRow.css`
- Test: none — this is CSS, and the row's `data-state` attribute is already covered by `SessionEntry`'s existing tests.

**Interfaces:**
- Consumes: `'tasking'` from Task 11.
- Produces: the `--tasking` token and the `.dot-tasking` rules.

- [ ] **Step 1: Add the token**

In `src/views/dotRow/dotRow.css`, with the other five at the top of the file (line 12–16), between `--busy` and `--idle`:

```css
  --tasking: #37b8c4;
```

- [ ] **Step 2: Add the glyph**

After the `.dot-idle` rule:

```css
/* Tasking: idle's hollow ring with an arc turning inside it. Present, not
   working, waiting on something that is. The ring is the same shape as idle on
   purpose — the session is idle, in the sense that nobody is at the keyboard —
   and the arc is what says the stillness has an end.

   The arc animates `transform` on a pseudo-element rather than the dot's
   `box-shadow`, for the reason recorded on `.dot-waiting::after`: shadow
   keyframes repaint the whole pill every frame and read as choppiness. */
.dot-tasking {
  background: transparent;
  border: 2px solid var(--tasking);
}

.dot-tasking::before {
  content: '';
  position: absolute;
  inset: 1px;
  border-radius: 50%;
  border: 2px solid transparent;
  border-top-color: var(--tasking);
  animation: task-turn 1.4s linear infinite;
}

@keyframes task-turn {
  to {
    transform: rotate(360deg);
  }
}
```

- [ ] **Step 3: Honour reduced motion**

Find the existing `@media (prefers-reduced-motion: reduce)` block — the one holding the `.dot-waiting::after` rule around line 611 — and add inside it:

```css
  /* The arc still marks the state, it just stops turning. */
  .dot-tasking::before {
    animation: none;
  }
```

- [ ] **Step 4: Verify it renders**

Run: `scripts/dev-fixtures.sh`
Expected: the widget launches. The `tasking` dot is not in the cast until Task 15, so at this point confirm only that nothing regressed — the five existing dots draw as before and the console is clean. Quit with `Ctrl-C`.

- [ ] **Step 5: Commit**

```bash
git add src/views/dotRow/dotRow.css
git commit -m "$(cat <<'EOF'
feat: a dot for the tasking state

Idle's hollow ring with an arc turning inside it: the session is idle in the
sense that nobody is at the keyboard, and the arc is what says the stillness
has an end. A shape rather than only a hue, like the other five — colour
alone is unreadable to a red-green colourblind user.

The arc animates transform on a pseudo-element, not the dot's box-shadow:
shadow keyframes repaint the whole pill every frame.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 13: The popover lists the tasks

**Files:**
- Modify: `src/views/dotRow/SessionPopover.tsx`
- Modify: `src/views/dotRow/dotRow.css`
- Test: `src/views/dotRow/SessionPopover.test.tsx`

**Interfaces:**
- Consumes: `Task`, `TaskKind` from Task 11; `formatElapsed` from `src/format.ts`.
- Produces: nothing other components use.

- [ ] **Step 1: Write the failing tests**

Add to `src/views/dotRow/SessionPopover.test.tsx`. The file has one module-level `session: SessionSnapshot` const at line 14 and every test renders `<SessionPopover session={{ ...session, … }} />`, so these follow that. Add `Task` to the existing type import:

```tsx
const runningTask = (id: string, label: string | null): Task => ({
  id,
  kind: 'shell',
  label,
  startedAtMs: NOW - 120_000,
  endedAtMs: null,
  status: 'running',
})
```

```tsx
  it('lists the running tasks with their age', () => {
    render(
      <SessionPopover
        session={{
          ...session,
          state: 'tasking',
          detail: '2 tasks running',
          tasks: [runningTask('t1', 'npm test'), runningTask('t2', 'cargo test')],
        }}
      />,
    )
    const tasks = screen.getByTestId('popover-tasks')
    expect(tasks).toHaveTextContent('npm test')
    expect(tasks).toHaveTextContent('cargo test')
    expect(tasks).toHaveTextContent('2m')
  })

  it('names a task with no label by its id', () => {
    render(
      <SessionPopover
        session={{
          ...session,
          state: 'tasking',
          detail: '1 task running',
          tasks: [runningTask('bmd0i64ke', null)],
        }}
      />,
    )
    expect(screen.getByTestId('popover-tasks')).toHaveTextContent('bmd0i64ke')
  })

  it('leaves out finished tasks', () => {
    // They are in the snapshot for a minute after they end, so the popover
    // would otherwise keep showing a build that is over.
    render(
      <SessionPopover
        session={{
          ...session,
          state: 'idle',
          detail: null,
          tasks: [
            { ...runningTask('t1', 'npm test'), status: 'completed', endedAtMs: NOW - 1_000 },
          ],
        }}
      />,
    )
    expect(screen.queryByTestId('popover-tasks')).toBeNull()
  })

  it('shows no tasks block for a session with none', () => {
    render(<SessionPopover session={{ ...session, tasks: [] }} />)
    expect(screen.queryByTestId('popover-tasks')).toBeNull()
  })
```

- [ ] **Step 2: Run them to verify they fail**

Run: `npm test -- src/views/dotRow/SessionPopover.test.tsx`
Expected: FAIL — `Unable to find an element by: [data-testid="popover-tasks"]`.

- [ ] **Step 3: Write the implementation**

In `src/views/dotRow/SessionPopover.tsx`, add `Task`, `TaskKind` to the type import and add above the component:

```tsx
/**
 * How each kind of task is introduced in the popover.
 *
 * A word rather than an icon: the block is a list of lines of text, and one
 * glyph in a column of prose reads as a bullet rather than as a category.
 */
const TASK_KIND_LABEL: Record<TaskKind, string> = {
  shell: 'shell',
  watch: 'watch',
  subagent: 'agent',
  job: 'job',
}
```

Inside the component, beside the other derived values:

```tsx
  // Finished tasks stay in the snapshot for a minute so the alert diff can see
  // them end. The popover is about what is happening now.
  const running = session.tasks.filter((t) => t.status === 'running')
```

And in the `<dl className="popover-fields">`, after the `doing` pair:

```tsx
        {running.length > 0 && (
          <>
            <dt>tasks</dt>
            <dd data-testid="popover-tasks">
              <ul className="popover-tasks">
                {running.map((task: Task) => (
                  <li key={task.id}>
                    <span className="popover-task-kind">{TASK_KIND_LABEL[task.kind]}</span>
                    {task.label ?? task.id}
                    {' · '}
                    {formatElapsed(now - task.startedAtMs)}
                  </li>
                ))}
              </ul>
            </dd>
          </>
        )}
```

In `src/views/dotRow/dotRow.css`, beside the other `.popover-*` rules:

```css
/* The task list is a `ul` inside a `dd` so each task gets its own line without
   the field labels repeating down the left. */
.popover-tasks {
  margin: 0;
  padding: 0;
  list-style: none;
}

.popover-tasks li {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.popover-task-kind {
  color: var(--idle);
  margin-right: 5px;
}
```

- [ ] **Step 4: Run the tests and the type check**

Run: `npm run typecheck && npm test -- src/views/dotRow/SessionPopover.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/views/dotRow/SessionPopover.tsx src/views/dotRow/SessionPopover.test.tsx src/views/dotRow/dotRow.css
git commit -m "$(cat <<'EOF'
feat: the popover names the tasks a session is waiting on

One line per running task: what kind it is, what it is, and how long it has
been going. Finished tasks are filtered out — they sit in the snapshot for a
minute so the alert diff can see them end, and a popover still showing a
build that is over would be worse than one showing nothing.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 14: Crazy mode and the settings form

**Files:**
- Modify: `src/views/dotRow/heat.ts`
- Modify: `src/settings/SettingsPanel.tsx:13-24` (`soundOff`, `soundOn`) and its checkbox group
- Test: `src/views/dotRow/heat.test.ts`, `src/settings/SettingsPanel.test.tsx`

**Interfaces:**
- Consumes: `'tasking'`, `Task`, `alertTaskDone` from Task 11.
- Produces: nothing new.

- [ ] **Step 1: Write the failing tests**

In `src/views/dotRow/heat.test.ts`. Its factory is `function session(over: Partial<SessionSnapshot>): SessionSnapshot` at line 5, which needs `tasks: []` in its defaults; add `Task` to the type import:

```ts
  it('counts a tasking session towards the fire', () => {
    const heat = deriveHeat([session({ state: 'tasking', tasks: [shellTask()] })], null, [])
    expect(heat.fire).toBe(1)
  })

  it('does not count a session whose only task is a registry job', () => {
    // Background jobs are already excluded from heat because they are work you
    // did not start. Counting the parent instead would be the same mistake in
    // a louder voice.
    const heat = deriveHeat([session({ state: 'tasking', tasks: [jobTask()] })], null, [])
    expect(heat.fire).toBe(0)
  })

  it('caps the fire at three across busy and tasking together', () => {
    const heat = deriveHeat(
      [
        session({ state: 'busy' }),
        session({ state: 'busy' }),
        session({ state: 'tasking', tasks: [shellTask()] }),
        session({ state: 'tasking', tasks: [shellTask()] }),
      ],
      null,
      [],
    )
    expect(heat.fire).toBe(3)
  })
```

with these helpers beside the file's existing ones:

```ts
  const shellTask = (): Task => ({
    id: 't1',
    kind: 'shell',
    label: 'npm test',
    startedAtMs: 0,
    endedAtMs: null,
    status: 'running',
  })

  const jobTask = (): Task => ({ ...shellTask(), id: 'job_1', kind: 'job' })
```

In `src/settings/SettingsPanel.test.tsx`. The file has a module-level `config` object (line ~27) which needs `alertTaskDone: false` added to it, a `displays` const, and tests that stub `invoke` per command and then assert on the `set_config` call:

```tsx
  it('saves the task-done alert switch', async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_config') return Promise.resolve(config)
      if (cmd === 'list_displays') return Promise.resolve(displays)
      return Promise.resolve()
    })
    render(<SettingsPanel onClose={vi.fn()} />)
    await waitFor(() =>
      expect(screen.getByLabelText('when a background task finishes')).not.toBeChecked(),
    )

    await userEvent.click(screen.getByLabelText('when a background task finishes'))

    expect(invoke).toHaveBeenCalledWith(
      'set_config',
      expect.objectContaining({
        config: expect.objectContaining({ alertTaskDone: true }),
      }),
    )
  })
```

- [ ] **Step 2: Run them to verify they fail**

Run: `npm test -- src/views/dotRow/heat.test.ts src/settings/SettingsPanel.test.tsx`
Expected: FAIL — `heat.fire` is `0` for a tasking session, and the checkbox does not exist.

- [ ] **Step 3: Write the implementation**

In `src/views/dotRow/heat.ts`, replace the `busy`/`fire` derivation:

```ts
  const busy = own.filter((s) => s.state === 'busy').length

  // A task you launched yourself is work in progress and stokes the fire. A
  // registry job is not: the comment above stands, and a session whose only
  // running task is a job would be the same exclusion dodged by one hop.
  const tasking = own.filter(
    (s) =>
      s.state === 'tasking' &&
      s.tasks.some((t) => t.status === 'running' && t.kind !== 'job'),
  ).length

  const fire = Math.min(3, busy + tasking) as Heat['fire']
```

Add `Task` to the type import if the test helpers need it exported — they import from `../../types`, so no change to `heat.ts`'s imports is required.

In `src/settings/SettingsPanel.tsx`:

```tsx
/**
 * What the four alert checkboxes become when the sound is switched off: all
 * off, and disabled with it. They are the events that raise a notification, and
 * the notification is the sound, so leaving one armed under a silent parent
 * would be a setting with nothing behind it.
 */
function soundOff(): Partial<AppConfig> {
  return {
    sound: false,
    alertNeedsInput: false,
    alertDied: false,
    alertFinished: false,
    alertTaskDone: false,
  }
}

/**
 * And what they become when it is switched back on: the defaults, rather than
 * the all-off state the parent just wrote. Switching the group on and getting
 * nothing would read as a broken toggle.
 */
function soundOn(): Partial<AppConfig> {
  return {
    sound: true,
    alertNeedsInput: true,
    alertDied: true,
    alertFinished: false,
    alertTaskDone: true,
  }
}
```

And a fourth checkbox in the same group, after the "when a session dies" label:

```tsx
            <label>
              <input
                type="checkbox"
                checked={config.sound && config.alertTaskDone}
                disabled={!config.sound}
                onChange={(e) => update({ alertTaskDone: e.target.checked })}
              />
              when a background task finishes
            </label>
```

- [ ] **Step 4: Run everything**

Run: `npm run typecheck && npm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/views/dotRow/heat.ts src/views/dotRow/heat.test.ts src/settings/SettingsPanel.tsx src/settings/SettingsPanel.test.tsx
git commit -m "$(cat <<'EOF'
feat: tasking stokes the fire, and the form offers the new alert

A test run you launched yourself is work in progress and counts towards the
fire. A registry job still does not — that exclusion exists because a job is
work you did not start, and counting its parent instead would dodge it by one
hop.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 15: A tasking session in the fixtures

Without this the state cannot be looked at, and no screenshot of it can be taken — `fixtures/` is the only acceptable source for one.

The wrinkle: task records are time-sensitive, and `fixtures/projects/` is committed precisely because nothing in it was. A start event stamped in the past is dropped by the `startedAt` boundary, and `startedAt` is stamped at run time. So this one transcript is generated rather than committed, alongside `sessions/` and `usage.json`, for exactly the reason those are.

**Files:**
- Modify: `fixtures/generate.sh`
- Modify: `fixtures/.gitignore`
- Modify: `fixtures/README.md`
- Test: run it and look at the widget

**Interfaces:**
- Consumes: the whole feature.
- Produces: a sixth live cast member, `test-runner`, in `tasking`.

- [ ] **Step 1: Raise the borrowed-pid count**

In `fixtures/generate.sh`, the cast gains one live session, so:

```bash
HOT="${CB_FIXTURE_HOT:-}"
NEEDED=6
[ -n "$HOT" ] && NEEDED=8
```

- [ ] **Step 2: Add the transcript writer**

In `fixtures/generate.sh`, after `emit_session`, add:

```bash
# Write the one transcript that cannot be committed.
#
# Everything under projects/ is committed because none of the fields the
# widget reads out of a transcript is time-sensitive — except these. A task's
# start record is only believed when it is stamped no earlier than the
# session's own `startedAt` (`watcher::tasks::apply_events`), which stops a
# resumed session inheriting a dead process's tasks, and `startedAt` is
# stamped at run time. A committed timestamp would be dropped by that
# boundary and the session would read `paused` instead of `tasking`.
#
# Two tasks are left running and one is left finished, so the row shows the
# plural detail, the popover lists two lines, and the finished one proves it
# is filtered out of both.
emit_task_transcript() {
  local session_id="$1" cwd="$2" started_ms="$3"
  local dir="$PROJECTS/$(printf '%s' "$cwd" | tr './' '--')"
  mkdir -p "$dir"

  # Comfortably after startedAt and before now, in the format transcripts use.
  local at
  at="$(date -u -r "$(( started_ms / 1000 + 60 ))" '+%Y-%m-%dT%H:%M:%S.000Z')"

  cat > "$dir/$session_id.jsonl" <<JSON
{"type":"user","uuid":"cc000001-0000-4000-8000-000000000001","parentUuid":null,"sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","message":{"role":"user","content":[{"type":"text","text":"Run the suite in the background and keep an eye on CI while it goes."}]}}
{"type":"assistant","uuid":"cc000001-0000-4000-8000-000000000002","parentUuid":"cc000001-0000-4000-8000-000000000001","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","effort":"medium","message":{"role":"assistant","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"toolu_fixture_lint","name":"Bash","input":{"command":"npm run lint","description":"Lint the workspace","run_in_background":true}}]}}
{"type":"user","uuid":"cc000001-0000-4000-8000-000000000003","parentUuid":"cc000001-0000-4000-8000-000000000002","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","timestamp":"$at","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_fixture_lint"}]},"toolUseResult":{"backgroundTaskId":"bfixlint01"}}
{"type":"queue-operation","operation":"enqueue","timestamp":"$at","sessionId":"$session_id","content":"<task-notification> <task-id>bfixlint01</task-id> <status>completed</status> <summary>Background command \"npm run lint\" completed</summary> </task-notification>"}
{"type":"assistant","uuid":"cc000001-0000-4000-8000-000000000004","parentUuid":"cc000001-0000-4000-8000-000000000003","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","effort":"medium","message":{"role":"assistant","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"toolu_fixture_test","name":"Bash","input":{"command":"npm test","description":"Run the whole suite","run_in_background":true}}]}}
{"type":"user","uuid":"cc000001-0000-4000-8000-000000000005","parentUuid":"cc000001-0000-4000-8000-000000000004","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","timestamp":"$at","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_fixture_test"}]},"toolUseResult":{"backgroundTaskId":"bfixtest01"}}
{"type":"assistant","uuid":"cc000001-0000-4000-8000-000000000006","parentUuid":"cc000001-0000-4000-8000-000000000005","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","effort":"medium","message":{"role":"assistant","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"toolu_fixture_ci","name":"Monitor","input":{"description":"Watch the CI run"}}]}}
{"type":"user","uuid":"cc000001-0000-4000-8000-000000000007","parentUuid":"cc000001-0000-4000-8000-000000000006","sessionId":"$session_id","version":"2.1.234","cwd":"$cwd","gitBranch":"ci/flaky-suite","timestamp":"$at","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_fixture_ci"}]},"toolUseResult":{"taskId":"bfixci0001","timeoutMs":600000}}
{"type": "custom-title", "customTitle": "Flaky suite triage", "sessionId": "$session_id"}
JSON
}
```

- [ ] **Step 3: Add the cast member**

In `fixtures/generate.sh`, the transcript has to exist before `emit_session` looks for it, and `emit_session` computes the same `started_ms` from the same inputs. Add above the cast table:

```bash
# The tasking session's transcript, written before its registry entry because
# `emit_session` requires the transcript to be there. 1560 seconds of quiet, so
# it is past PAUSED_THRESHOLD_MS and would read `paused` if its tasks were not
# running.
TASK_UPTIME_S=4200
emit_task_transcript 6f7a8b9c-8293-4405-9f16-6b8c9dae0fb6 \
  /Users/n/Code/test-runner "$(( NOW_MS - TASK_UPTIME_S * 1000 ))"
```

and to the cast table, after the `docs-site` line:

```bash
# Two background shells and a CI watch are running, so ten minutes of quiet
# reads `tasking` rather than `paused`.
emit_session 6    6f7a8b9c-8293-4405-9f16-6b8c9dae0fb6   test-runner       /Users/n/Code/test-runner       interactive   ""                 idle      ""              $TASK_UPTIME_S  1560
```

Renumber the two hot-cast slots so they do not collide:

```bash
if [ -n "$HOT" ]; then
  emit_session 7  8f9e0d1c-a2b3-4c4d-9e5f-6a7b8c9d0e1f   payments-api      /Users/n/Code/payments-api      interactive   ""                 busy      ""              3600    30
  emit_session 8  3b4c5d6e-f708-4192-a3b4-c5d6e7f80912   search-index      /Users/n/Code/search-index      interactive   ""                 busy      ""              2400    20
fi
```

- [ ] **Step 4: Stop git tracking the generated transcript**

In `fixtures/.gitignore`:

```
# Written by generate.sh at run time, not committed: both carry timestamps and
# pids that are only true for a few minutes. See README.md in this directory.
/sessions/
/usage.json
# And the one transcript that carries timestamps too — task start records are
# only believed when stamped after the session's own startedAt.
/projects/-Users-n-Code-test-runner/
```

- [ ] **Step 5: Run it and look**

Run: `./fixtures/generate.sh`
Expected: seven lines of cast output, `test-runner` among them, and `git status --short` shows nothing new under `fixtures/projects/`.

Run: `scripts/dev-fixtures.sh`
Expected: the row shows `test-runner` behind a turning teal ring, between `web-app` (busy) and `design-system` (idle). The collapsed pill carries a `1` beside a tasking dot. Hovering `test-runner` opens a popover reading `2 tasks running`, a `tasks` block listing `shell npm test` and `watch Watch the CI run` with their ages, and **not** listing the finished lint. Quit with `Ctrl-C`.

- [ ] **Step 6: Document the cast**

In `fixtures/README.md`, add a row to the cast table after `docs-site`:

```markdown
| `test-runner` | tasking | Twenty-six minutes quiet, which would be `paused`, except that its transcript leaves two background shells and a CI watch running. A third task is left finished, which is what shows that the popover lists only what is still going. |
```

Change "Six entries" to "Seven entries" in the sentence above the table, and add to the "What is committed and what is not" section, after the three bullets:

```markdown
One transcript is generated rather than committed: `test-runner`'s. Everything
else under `projects/` is committed because none of the fields read out of a
transcript is time-sensitive, and task records are the exception — a task start
is only believed when it is stamped no earlier than the session's own
`startedAt`, which is what stops a resumed session inheriting a dead process's
tasks. A committed timestamp would fall the wrong side of that boundary and the
session would read `paused`.
```

- [ ] **Step 7: Commit**

```bash
git add fixtures/generate.sh fixtures/.gitignore fixtures/README.md
git commit -m "$(cat <<'EOF'
feat: a tasking session in the fixture cast

Without it the state cannot be looked at and no screenshot of it can be
taken. `test-runner` is twenty-six minutes quiet — `paused`, but for two
background shells and a CI watch still running — and carries a third,
finished task so the popover's filtering is on screen rather than only in a
test.

Its transcript is generated rather than committed, alongside sessions/ and
usage.json and for the same reason: a task start is only believed when
stamped after the session's own startedAt, and that is stamped at run time.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 16: What the change owes

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

**Interfaces:** none.

- [ ] **Step 1: Add the state to the README's table**

In `README.md`, the state table (around line 84–90) gains a row between green and grey:

```markdown
| teal | hollow ring with a turning arc | waiting on a background task — carries what it is waiting for |
```

The sentence directly above that table (line 82) ends "these five dots are the widget's whole vocabulary". Change `five` to `six`.

- [ ] **Step 2: Describe the state in the README's prose**

After the paragraph introducing the expanded row and before the popover paragraph, add:

```markdown
A session that fires off a background test run, a dev server, a watch or a
background subagent goes quiet, and used to read `paused` after ten minutes —
indistinguishable from one nobody was driving. It now reads `tasking`, and the
popover lists what it is waiting on: each running task's kind, what it is, and
how long it has been going. Registry jobs count too, so a session waiting on
one of those reads the same way whether or not the job has a row of its own.
When a task ends, a notification says which one and how it went; turn that off
with `alertTaskDone`.
```

- [ ] **Step 3: Note the setting**

`README.md`'s "Settings file" section is a JSON sample followed by a paragraph of prose about the keys that need explaining. In the sample (line 208), after `"alertFinished": false,`:

```json
  "alertTaskDone": true,
```

And in the prose paragraph below it, extend the `keepAwake` sentence — it currently says the setting "only has an effect while a session is working or waiting" — to read "while a session is working, waiting, or waiting on a background task".

- [ ] **Step 4: Write the changelog entry**

In `CHANGELOG.md`, add a new section directly under the header block and above `## v0.9.0`:

```markdown
## Unreleased

- **A session waiting on a background task says so.** A session that started a
  background test run, a dev server, a watch or a background subagent goes
  quiet, and after ten minutes it read `paused` — the same as a session nobody
  was driving. There is now a sixth state, `tasking`, drawn as a hollow ring
  with a turning arc, and the popover lists every task the session is waiting
  on: what kind it is, what it is, and how long it has been going. Registry
  background jobs count as tasks on the session they belong to, so a parent
  reads the same way whether or not the job is shown as its own row. The widget
  stays on screen and the display stays awake while a task runs, and in crazy
  mode a task you launched stokes the fire the way a working session does.
- **A notification when a background task finishes.** It names the task and how
  it ended, so a failure does not read like a success. On by default, and
  switched off with the new "when a background task finishes" checkbox in
  Settings.
```

- [ ] **Step 5: Verify the release script can still read the file**

Run: `scripts/release-notes.sh v0.9.0`
Expected: the v0.9.0 section prints unchanged — the new section above it must not be picked up.

- [ ] **Step 6: Commit**

```bash
git add README.md CHANGELOG.md
git commit -m "$(cat <<'EOF'
docs: document the tasking state and its notification

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

## Final verification

- [ ] `cd src-tauri && cargo fmt --check && cargo clippy -- -D warnings && cargo test -- --test-threads=1`
- [ ] `npm run typecheck && npm test`
- [ ] `scripts/dev-fixtures.sh` — `test-runner` reads tasking, its popover lists two running tasks and not the finished one, the collapsed pill counts it, and no console errors
- [ ] `git status --short` — nothing unexpected staged or generated into the tree
