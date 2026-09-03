# Neutralize the Provider Seam Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `watcher/state.rs` and its neighbours depend on provider-neutral types instead of Claude Code's registry and task shapes, so they can later move to `buddy-core` unchanged.

**Architecture:** Introduce a `RawSession` record that carries the twelve fields `state.rs` actually reads, and a `From<RegistryFile>` that maps Claude Code's file format onto it. Retarget `snapshot()` at `&[RawSession]`. Split the `Task` data model away from the transcript scanner that produces it. Relocate two constants/types that agnostic modules borrow from provider-specific ones. Nothing leaves the repository in this plan.

**Tech Stack:** Rust 2021 (rust-version 1.77), Tauri v2, serde. No new dependencies.

## Global Constraints

- macOS only. There is no other platform behind a feature flag.
- Before every commit: `npm run typecheck && npm test`, then `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`.
- `--test-threads=1` is **not optional** — the watcher-loop tests use real files and real wall-clock time and interfere with each other in parallel.
- Run `git status` before starting and again before staging. A file you did not touch showing as modified belongs to another agent session — leave it alone, do not revert it, and do not fix a compile error that belongs to work in flight. Say what you found instead.
- Stage explicit paths. Never `git add -A`, never `git commit -a`.
- Conventional commit subjects (`feat:`, `fix:`, `refactor:`, `docs:`), no scopes, body explaining the reasoning. Keep the `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` trailer.
- Comments explain *why*, never *what*.
- Do not reformat files you are not changing.
- **No README.md or CHANGELOG.md entry for any task in this plan.** Per CLAUDE.md, internal refactors owe neither. Nothing here changes how the app behaves for a user.
- This plan does not create, push to, or tag `buddy-core` or `buddy-ui`. That is phase 2.

---

### Task 1: The neutral `RawSession` record

**Files:**
- Create: `src-tauri/src/watcher/session.rs`
- Modify: `src-tauri/src/watcher/mod.rs`
- Test: inline `#[cfg(test)] mod tests` in `src-tauri/src/watcher/session.rs`

**Interfaces:**
- Consumes: `crate::watcher::registry::RegistryFile` (existing, 12 pub fields).
- Produces: `crate::watcher::session::RawSession` with pub fields `pid: i32`, `session_id: String`, `cwd: String`, `started_at: i64`, `proc_start: Option<String>`, `entrypoint: Option<String>`, `kind: Option<String>`, `job_id: Option<String>`, `name: Option<String>`, `status: Option<String>`, `status_updated_at: Option<i64>`, `waiting_for: Option<String>`. Plus `impl From<RegistryFile> for RawSession`. Task 2 depends on both.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/watcher/session.rs` containing only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::registry::RegistryFile;

    fn registry_file() -> RegistryFile {
        RegistryFile {
            pid: 7952,
            session_id: "a1b2c3d4-0000-4000-8000-000000000001".to_string(),
            cwd: "/Users/n/Code/api-service".to_string(),
            started_at: 1_787_637_231_465,
            proc_start: Some("Tue Aug 25 05:53:49 2026".to_string()),
            entrypoint: Some("cli".to_string()),
            kind: Some("interactive".to_string()),
            job_id: Some("job-1".to_string()),
            name: Some("api-service".to_string()),
            status: Some("waiting".to_string()),
            status_updated_at: Some(1_787_637_299_000),
            waiting_for: Some("input".to_string()),
        }
    }

    /// Every field survives the mapping. A field silently dropped here would
    /// present as a state the widget never enters, which is expensive to
    /// diagnose from the UI end.
    #[test]
    fn conversion_preserves_every_field() {
        let raw = RawSession::from(registry_file());

        assert_eq!(raw.pid, 7952);
        assert_eq!(raw.session_id, "a1b2c3d4-0000-4000-8000-000000000001");
        assert_eq!(raw.cwd, "/Users/n/Code/api-service");
        assert_eq!(raw.started_at, 1_787_637_231_465);
        assert_eq!(raw.proc_start.as_deref(), Some("Tue Aug 25 05:53:49 2026"));
        assert_eq!(raw.entrypoint.as_deref(), Some("cli"));
        assert_eq!(raw.kind.as_deref(), Some("interactive"));
        assert_eq!(raw.job_id.as_deref(), Some("job-1"));
        assert_eq!(raw.name.as_deref(), Some("api-service"));
        assert_eq!(raw.status.as_deref(), Some("waiting"));
        assert_eq!(raw.status_updated_at, Some(1_787_637_299_000));
        assert_eq!(raw.waiting_for.as_deref(), Some("input"));
    }

    #[test]
    fn absent_optional_fields_stay_absent() {
        let mut file = registry_file();
        file.proc_start = None;
        file.entrypoint = None;
        file.kind = None;
        file.job_id = None;
        file.name = None;
        file.status = None;
        file.status_updated_at = None;
        file.waiting_for = None;

        let raw = RawSession::from(file);

        assert!(raw.proc_start.is_none());
        assert!(raw.entrypoint.is_none());
        assert!(raw.kind.is_none());
        assert!(raw.job_id.is_none());
        assert!(raw.name.is_none());
        assert!(raw.status.is_none());
        assert!(raw.status_updated_at.is_none());
        assert!(raw.waiting_for.is_none());
    }
}
```

Register the module by adding this line to `src-tauri/src/watcher/mod.rs`, in the existing alphabetical run of `pub mod` declarations (between `pub mod registry;` and `pub mod state;`):

```rust
pub mod session;
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test --lib watcher::session -- --test-threads=1`

Expected: FAIL to compile, with `cannot find type RawSession in this scope`.

- [ ] **Step 3: Write the minimal implementation**

Insert above the test module in `src-tauri/src/watcher/session.rs`:

```rust
use crate::watcher::registry::RegistryFile;

/// One session, in terms the state machine can reason about without knowing
/// which agent produced it.
///
/// This is deliberately the same twelve fields `RegistryFile` carries, because
/// `snapshot()` reads all twelve. The difference is that nothing here is tied
/// to one provider's file format: `RegistryFile` owns the serde spelling of
/// Claude Code's `~/.claude/sessions/<pid>.json`, and a second provider maps
/// its own source onto `RawSession` instead of being forced through that
/// schema.
#[derive(Debug, Clone, PartialEq)]
pub struct RawSession {
    pub pid: i32,
    pub session_id: String,
    pub cwd: String,
    pub started_at: i64,
    /// Process start time, used to tell a live pid from a recycled one.
    pub proc_start: Option<String>,
    pub entrypoint: Option<String>,
    /// `interactive`, `bg` or `sdk`.
    pub kind: Option<String>,
    /// Present on background jobs, which belong to a session rather than
    /// being one.
    pub job_id: Option<String>,
    pub name: Option<String>,
    pub status: Option<String>,
    pub status_updated_at: Option<i64>,
    pub waiting_for: Option<String>,
}

impl From<RegistryFile> for RawSession {
    fn from(f: RegistryFile) -> Self {
        Self {
            pid: f.pid,
            session_id: f.session_id,
            cwd: f.cwd,
            started_at: f.started_at,
            proc_start: f.proc_start,
            entrypoint: f.entrypoint,
            kind: f.kind,
            job_id: f.job_id,
            name: f.name,
            status: f.status,
            status_updated_at: f.status_updated_at,
            waiting_for: f.waiting_for,
        }
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd src-tauri && cargo test --lib watcher::session -- --test-threads=1`

Expected: PASS, 2 passed.

- [ ] **Step 5: Run the full gate**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`

Expected: PASS. The existing 300+ tests are untouched by this task — `RawSession` has no consumers yet.

- [ ] **Step 6: Commit**

```bash
git status
git add src-tauri/src/watcher/session.rs src-tauri/src/watcher/mod.rs
git commit -m "$(cat <<'EOF'
refactor: add a provider-neutral session record

snapshot() reads all twelve RegistryFile fields, so the record the state
machine needs is that struct without the serde attributes spelling out
Claude Code's on-disk JSON. RawSession is those twelve fields as plain
Rust, with the mapping in one place.

Nothing consumes it yet. Splitting the type introduction from the
retarget keeps the commit that touches state.rs readable.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Retarget `snapshot()` at `RawSession`

**Files:**
- Modify: `src-tauri/src/watcher/state.rs:245` (the `files` parameter), `:457-472` (the `file` test helper), `:879-884` (the `job` test helper), and the `use` block at `:8`
- Modify: `src-tauri/src/watcher/watch.rs:16` (import), `:171` (the call site)
- Test: the existing `#[cfg(test)] mod tests` in `src-tauri/src/watcher/state.rs` is the test — it must keep passing unchanged in behaviour

**Interfaces:**
- Consumes: `crate::watcher::session::RawSession` and `impl From<RegistryFile> for RawSession` from Task 1.
- Produces: `snapshot()` with its first parameter changed to `files: &[RawSession]`. Every other parameter, and `SnapshotResult`, are unchanged. Task 4 does not depend on this; phase 2 does.

This is a type substitution, not a behaviour change. The field names on `RawSession` are identical to `RegistryFile`'s, so the ~170 field reads inside `snapshot()` need no edits at all. Only the signature, two test helpers and one call site change.

- [ ] **Step 1: Confirm the current suite is green before touching anything**

Run: `cd src-tauri && cargo test --lib watcher::state -- --test-threads=1`

Expected: PASS. Note the exact test count from the output — the same number must pass at the end of this task. A changed count means a test was accidentally disabled.

- [ ] **Step 2: Change the signature and the import in `state.rs`**

In `src-tauri/src/watcher/state.rs`, replace line 8:

```rust
use crate::watcher::registry::RegistryFile;
```

with:

```rust
use crate::watcher::session::RawSession;
```

Then change the first parameter of `snapshot()` at line 245 from:

```rust
    files: &[RegistryFile],
```

to:

```rust
    files: &[RawSession],
```

- [ ] **Step 3: Change the two test helpers in `state.rs`**

At line 457, change the return type and the constructed type:

```rust
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
```

At line 879, change only the return type — the body already delegates to `file`:

```rust
    fn job(pid: i32) -> RawSession {
        let mut f = file(pid, "cli");
        f.kind = Some("bg".into());
        f.job_id = Some(format!("job-{pid}"));
        f
    }
```

- [ ] **Step 4: Map at the call site in `watch.rs`**

The watcher loop is the one place a Claude Code registry becomes sessions, so the mapping belongs here. Change line 171 from:

```rust
                &read_registry_dir(&dir),
```

to:

```rust
                &read_registry_dir(&dir)
                    .into_iter()
                    .map(RawSession::from)
                    .collect::<Vec<_>>(),
```

And add the import alongside the existing `use crate::watcher::registry::read_registry_dir;` at line 16:

```rust
use crate::watcher::session::RawSession;
```

- [ ] **Step 5: Run the state tests**

Run: `cd src-tauri && cargo test --lib watcher::state -- --test-threads=1`

Expected: PASS, with exactly the count noted in Step 1.

If a compile error names `RegistryFile` in a file other than `registry.rs`, `session.rs` or `watch.rs`, stop: that is a consumer this plan did not account for. Report it rather than widening the change.

- [ ] **Step 6: Run the full gate**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`

Expected: PASS. `watch.rs`'s own tests construct snapshots through `state.rs` helpers, so they follow automatically.

- [ ] **Step 7: Verify against fixtures, not just tests**

Run: `./scripts/dev-fixtures.sh`

Expected: the widget launches and shows the fixture sessions with the same states as before the change. Quit it when satisfied. This is the check that the `watch.rs` mapping is wired correctly — a mapping error compiles cleanly and produces an empty dot row.

- [ ] **Step 8: Commit**

```bash
git status
git add src-tauri/src/watcher/state.rs src-tauri/src/watcher/watch.rs
git commit -m "$(cat <<'EOF'
refactor: state the snapshot in terms of RawSession

snapshot() took &[RegistryFile], which tied the whole state machine to
one provider's on-disk JSON schema. It now takes &[RawSession], and the
watcher loop maps the registry into that shape at the point it reads it.

No behaviour change and no test changes beyond two helper return types:
RawSession carries the same field names, so the field reads inside
snapshot() are untouched.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Separate the task data model from the transcript scanner

**Files:**
- Create: `src-tauri/src/watcher/task.rs`
- Modify: `src-tauri/src/watcher/tasks.rs` (remove the moved items, re-import them), `src-tauri/src/watcher/mod.rs`, `src-tauri/src/watcher/state.rs:9`
- Test: the existing `tasks.rs` tests cover the scanner; add one inline test in `task.rs` for the data model's serde spelling

**Interfaces:**
- Consumes: nothing from Tasks 1-2.
- Produces: `crate::watcher::task::{Task, TaskKind, TaskStatus, TaskProbe}`. `crate::watcher::tasks` keeps its scanner and re-exports nothing.

`tasks.rs` is 1618 lines, and almost all of it is the transcript scanning that finds Claude Code's subagent tasks. The `Task` struct, its two enums and the `TaskProbe` trait are the shape the UI renders and the state machine stores — those are agnostic. The scanner that fills them in is not.

- [ ] **Step 1: Read the current definitions before moving them**

Run: `sed -n '1,60p' src-tauri/src/watcher/tasks.rs && grep -n 'pub trait TaskProbe' -A 12 src-tauri/src/watcher/tasks.rs`

Copy the exact text of `TaskKind` (line 18), `TaskStatus` (line 30), the `impl TaskStatus` block carrying `terminal()` (line 39), `Task` (line 45) and the `TaskProbe` trait (line 300), including every derive, serde attribute and doc comment. Moving these must be verbatim — a dropped `#[serde(rename_all = ...)]` changes the JSON the frontend receives, and the frontend's own tests would not catch it because they use their own fixtures.

`TaskEvent` (line 66) does **not** move. It is "one half of a task's life, as recorded in a transcript" — a Claude Code transcript shape. It references `TaskKind`, which the import added in Step 4 covers.

- [ ] **Step 2: Create `task.rs` with the moved items**

Create `src-tauri/src/watcher/task.rs`. Paste the four items copied in Step 1 verbatim, preceded by:

```rust
//! The task data model, and the trait that supplies it.
//!
//! Separate from `tasks.rs` because the shape a task has is not
//! provider-specific but the way you find one is: `tasks.rs` scans Claude
//! Code transcripts for subagent records, whereas everything here is what the
//! state machine stores and the widget renders.
```

Add whatever `use` lines the pasted items need — at minimum `use serde::Serialize;`.

Register the module in `src-tauri/src/watcher/mod.rs`, in the alphabetical run, before `pub mod tasks;`:

```rust
pub mod task;
```

- [ ] **Step 3: Add a serde-spelling test to `task.rs`**

Append to `src-tauri/src/watcher/task.rs`:

```rust
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
```

`Task` has seven fields — `output` is `#[serde(skip)]`, so it must be present in
the constructor and absent from the JSON. `TaskKind` is
`Shell | Watch | Subagent | Job` and `TaskStatus` is
`Running | Completed | Failed | Killed | Stopped`, both `rename_all = "lowercase"`;
`Task` itself is `rename_all = "camelCase"`.

- [ ] **Step 4: Delete the moved items from `tasks.rs` and import them**

Remove `TaskKind`, `TaskStatus`, `Task` and the `TaskProbe` trait from `src-tauri/src/watcher/tasks.rs`. Add at the top of its `use` block:

```rust
use crate::watcher::task::{Task, TaskKind, TaskProbe, TaskStatus};
```

Keep every `impl TaskProbe for ...` in `tasks.rs` — the implementations are provider-specific and stay.

- [ ] **Step 5: Point `state.rs` at the new module**

In `src-tauri/src/watcher/state.rs`, replace line 9:

```rust
use crate::watcher::tasks::{Task, TaskKind, TaskProbe, TaskStatus};
```

with:

```rust
use crate::watcher::task::{Task, TaskKind, TaskProbe, TaskStatus};
```

- [ ] **Step 6: Fix the remaining importers**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A 4 | head -40`

Expected: errors naming any other module that imported these four items from `watcher::tasks`. Change each such import to `watcher::task`. Do not change anything else in those files.

- [ ] **Step 7: Run the full gate**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`

Expected: PASS, with one more test than before (the new serde test) and no other count change.

- [ ] **Step 8: Commit**

```bash
git status
git add src-tauri/src/watcher/task.rs src-tauri/src/watcher/tasks.rs src-tauri/src/watcher/mod.rs src-tauri/src/watcher/state.rs
git commit -m "$(cat <<'EOF'
refactor: split the task data model from the transcript scanner

Task, its two enums and the TaskProbe trait describe what a task is, which
is not specific to any agent. The 1600 lines around them in tasks.rs scan
Claude Code transcripts for subagent records, which is entirely specific
to one. Keeping both in one file meant state.rs imported a provider's
scanner to name a type.

The impls stay in tasks.rs. Only the shape moved.

Adds a test pinning the serialized field names, because the frontend
reads them and a rename would degrade a task to a blank row rather than
failing anything.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Relocate the two borrowed items

**Files:**
- Modify: `src-tauri/src/commands.rs:9` (remove the const), `:57` (re-import it), `src-tauri/src/config.rs` (gain the const), `src-tauri/src/tray.rs:265`
- Modify: `src-tauri/src/watcher/watch.rs:98-108` (remove `SnapshotStore`), `src-tauri/src/watcher/store.rs` (gain it), `src-tauri/src/window.rs:90`
- Create: `src-tauri/src/watcher/store.rs`
- Test: existing tests cover both; no new test — these are a const and an 11-line mutex wrapper with no logic to assert

**Interfaces:**
- Consumes: nothing from Tasks 1-3.
- Produces: `crate::config::CONFIG_EVENT` (was `crate::commands::CONFIG_EVENT`) and `crate::watcher::store::SnapshotStore` (was `crate::watcher::watch::SnapshotStore`).

Two agnostic modules currently reach into provider-adjacent ones for trivia. `tray.rs:265` reads a const from `commands.rs`, and `window.rs:90` reads a type from `watch.rs`. Both would drag their whole host module into core.

- [ ] **Step 1: Move `CONFIG_EVENT` to `config.rs`**

Delete line 9 of `src-tauri/src/commands.rs`:

```rust
pub const CONFIG_EVENT: &str = "config://update";
```

Add it near the top of `src-tauri/src/config.rs`, after the existing `use` block:

```rust
/// The event name the frontend listens on for settings changes.
///
/// It lives beside the config it announces rather than in `commands.rs`, so
/// that emitting it does not require the command layer.
pub const CONFIG_EVENT: &str = "config://update";
```

In `src-tauri/src/commands.rs`, add to the `use` block:

```rust
use crate::config::CONFIG_EVENT;
```

In `src-tauri/src/tray.rs:265`, change:

```rust
    let _ = app.emit(crate::commands::CONFIG_EVENT, config);
```

to:

```rust
    let _ = app.emit(crate::config::CONFIG_EVENT, config);
```

- [ ] **Step 2: Build to confirm nothing else referenced the const**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A 4 | head -20`

Expected: no output. If a third file referenced `commands::CONFIG_EVENT`, point it at `config::CONFIG_EVENT` too.

- [ ] **Step 3: Move `SnapshotStore` to its own module**

Create `src-tauri/src/watcher/store.rs`:

```rust
//! Where the latest snapshot lives between watcher ticks.
//!
//! Its own module rather than part of `watch.rs`, because the windowing code
//! reads the store but has no business with the loop that fills it.

use crate::watcher::state::SessionSnapshot;

#[derive(Default)]
pub struct SnapshotStore(std::sync::Mutex<Vec<SessionSnapshot>>);

impl SnapshotStore {
    pub fn set(&self, sessions: Vec<SessionSnapshot>) {
        *self.0.lock().expect("snapshot store poisoned") = sessions;
    }

    pub fn get(&self) -> Vec<SessionSnapshot> {
        self.0.lock().expect("snapshot store poisoned").clone()
    }
}
```

Copy the four-line doc comment currently above `watch.rs:98` across as well — it explains why a fetchable copy exists at all (the first snapshot lands before the webview subscribes, and the change filter suppresses every later emission while state holds), which is not recoverable from the code.

Delete lines 94-108 from `src-tauri/src/watcher/watch.rs` and add to its `use` block:

```rust
use crate::watcher::store::SnapshotStore;
```

Register the module in `src-tauri/src/watcher/mod.rs`, before `pub mod tasks;`:

```rust
pub mod store;
```

In `src-tauri/src/window.rs:90`, change:

```rust
        let sessions = app.state::<crate::watcher::watch::SnapshotStore>().get();
```

to:

```rust
        let sessions = app.state::<crate::watcher::store::SnapshotStore>().get();
```

- [ ] **Step 4: Fix the remaining importers**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A 4 | head -40`

Expected: errors at exactly these five sites, all spelling the old path. Point each at `crate::watcher::store::SnapshotStore`:

- `lib.rs:103` — `app.manage(...::SnapshotStore::default())`
- `lib.rs:129` — `.state::<...::SnapshotStore>()`
- `commands.rs:27` — `store: tauri::State<'_, ...::SnapshotStore>`
- `tray.rs:234` — `.try_state::<...::SnapshotStore>()`
- `watch.rs:373` — `SnapshotStore::default()` in a test

`window.rs:90` is the sixth and was already changed in Step 3.

The type must resolve to the same type everywhere — Tauri's state lookup is by `TypeId`, so a stale path that still compiles via a re-export is fine, but two distinct types would panic at runtime with a missing-state error rather than failing to build.

- [ ] **Step 5: Run the full gate**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`

Expected: PASS, same count as after Task 3.

- [ ] **Step 6: Verify the app actually runs**

Run: `./scripts/dev-fixtures.sh`

Expected: the widget launches, shows fixture sessions, and opening Settings from the tray menu still applies changes live. That last part exercises both moved items — `CONFIG_EVENT` carries the settings update, and `SnapshotStore` is what the window reads to size itself. A `TypeId` mismatch from Step 4 shows up here and nowhere else.

Quit it when satisfied.

- [ ] **Step 7: Commit**

```bash
git status
git add src-tauri/src/commands.rs src-tauri/src/config.rs src-tauri/src/tray.rs src-tauri/src/window.rs src-tauri/src/watcher/store.rs src-tauri/src/watcher/watch.rs src-tauri/src/watcher/mod.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
refactor: stop agnostic modules borrowing from provider ones

tray.rs read CONFIG_EVENT out of commands.rs, and window.rs read
SnapshotStore out of watch.rs. Both are trivia — a const and an eleven-line
mutex wrapper — but both would have dragged their whole host module along
behind them, and neither host is agnostic.

CONFIG_EVENT now sits beside the config it announces. SnapshotStore gets
its own module, since the windowing code reads the store but has no
business with the loop that fills it.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Record what is left entangled

**Files:**
- Modify: `docs/superpowers/specs/2026-09-03-multi-provider-repo-split-design.md` (the "Open questions" section)

**Interfaces:**
- Consumes: the findings from Tasks 1-4.
- Produces: nothing code depends on. This closes the loop the spec left open.

The spec's open question about `alerts.rs`, `blocked.rs`, `working.rs` and `question.rs` (1,281 lines) said the honest answer arrives while extracting. Tasks 1-4 are that extraction.

- [ ] **Step 1: Determine which of the four are agnostic**

Run:

```bash
cd src-tauri/src && for f in watcher/alerts.rs watcher/blocked.rs watcher/working.rs watcher/question.rs; do
  echo "--- $f"
  grep -o 'crate::[a-z_]*\(::[a-z_]*\)*' "$f" | sort -u
done
```

A module importing only `watcher::state`, `watcher::session`, `watcher::task` or `config` is agnostic and moves to core. One importing `bridge::transcript`, `watcher::registry`, `watcher::title` or `watcher::tasks` is provider-specific and stays. Record which is which.

- [ ] **Step 2: Replace the open question with the answer**

In `docs/superpowers/specs/2026-09-03-multi-provider-repo-split-design.md`, replace the second bullet of "Open questions" with a short "Settled during phase 1" subsection listing each of the four modules and its destination, with the import that decided it. Delete the bullet.

Do not restate the whole classification for modules the spec already covers.

- [ ] **Step 3: Commit**

```bash
git status
git add docs/superpowers/specs/2026-09-03-multi-provider-repo-split-design.md
git commit -m "$(cat <<'EOF'
docs: settle where the four undecided watcher modules go

The spec said the answer would arrive while extracting, and it did.
Records the destination for alerts.rs, blocked.rs, working.rs and
question.rs, and the import in each that decided it, so phase 2 does not
have to re-derive the classification.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

## Done when

- `snapshot()` takes `&[RawSession]` and `state.rs` imports nothing from `registry.rs` or `tasks.rs`.
- `cargo test -- --test-threads=1` passes with the same count as before this plan, plus three new tests.
- `./scripts/dev-fixtures.sh` shows the same states it did before.
- The spec records where all four previously-undecided modules go.

Phase 2 — seeding `buddy-core` and `buddy-ui`, creating `buddy-dev`, and the claude-buddy migration PR — gets its own plan, written once this one lands.

---

## Phase 1 extension: the four remaining probe traits

Added after Tasks 1-5 landed. Task 5's investigation showed `snapshot()` takes
six injected traits, and four of them are declared in files whose only real
implementation reads Claude Code transcripts — so `state.rs` still cannot move
to `buddy-core` unchanged, which was this plan's stated goal. `PidLiveness`
(`liveness.rs`, OS-level pid checks) and `TaskProbe` (`task.rs`, split out by
Task 3) are unaffected.

Every one of the four was checked before these tasks were written: each trait
declares a single method taking `(&str, &str)` and returning `Option<i64>`,
`Option<String>` or `bool`. No transcript type appears in any signature, so all
four are clean lifts. Their doc comments already describe them as one family,
which is why they land in a single `watcher/probes.rs` rather than four
modules.

Each task is the Task 3 pattern: move the trait declaration verbatim, leave
every `impl` behind, repoint importers. Task 6 creates the module; 7-9 add to
it.

### Task 6: `ActivityProbe` into `watcher/probes.rs`

**Files:**
- Create: `src-tauri/src/watcher/probes.rs`
- Modify: `src-tauri/src/watcher/activity.rs`, `src-tauri/src/watcher/mod.rs`, `src-tauri/src/watcher/state.rs:5`
- Test: no new test. A moved trait declaration has no behaviour to assert, and the existing suite fails to compile if the move is wrong.

**Interfaces:**
- Produces: `crate::watcher::probes::ActivityProbe`. Tasks 7-9 append to the same module.

- [ ] **Step 1: Create the module with the trait moved verbatim**

Create `src-tauri/src/watcher/probes.rs`:

```rust
//! The traits `state::snapshot` takes as inputs.
//!
//! Separate from the modules that implement them because the questions are not
//! provider-specific but every answer is: each implementation here reads a
//! Claude Code transcript, whereas the trait is just "can you tell me whether
//! this session is busy". A second agent answers the same questions from its
//! own source.
```

Then move the `ActivityProbe` declaration out of `activity.rs` into it, taking
its full doc comment verbatim — the comment explains why the probe exists at
all (only `cli` sessions write `status`, so a `claude-desktop` session would
otherwise age into `paused` while being worked in), and that reasoning is not
recoverable from the code.

Register it in `src-tauri/src/watcher/mod.rs`, alphabetically:

```rust
pub mod probes;
```

- [ ] **Step 2: Repoint `activity.rs` and `state.rs`**

In `activity.rs`, add `use crate::watcher::probes::ActivityProbe;`. Every
`impl ActivityProbe for ...` stays — `TranscriptActivity`, `NoActivity` and
`FakeActivity` all remain in `activity.rs`.

In `state.rs`, change line 5 from `use crate::watcher::activity::ActivityProbe;`
to `use crate::watcher::probes::ActivityProbe;`.

- [ ] **Step 3: Fix any other importers**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A 4 | head -40`

Expected: errors naming `lib.rs`, `watch.rs` or any other file importing
`ActivityProbe` from `watcher::activity`. Change only the import path in each.
Task 3 found two importers the plan had not listed, so treat this step's output
as the authority rather than any list.

- [ ] **Step 4: Run the full gate**

Run: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`

Expected: PASS at 409, unchanged. No new warnings.

- [ ] **Step 5: Commit**

```bash
git status
git add src-tauri/src/watcher/probes.rs src-tauri/src/watcher/activity.rs src-tauri/src/watcher/mod.rs src-tauri/src/watcher/state.rs
git commit -m "$(cat <<'EOF'
refactor: move ActivityProbe to a module of its own

state.rs imported the trait from activity.rs, whose only real
implementation reads a Claude Code transcript. That made the state
machine depend on a provider's file format to name one of its inputs.

The question "when did this session last do anything" is not specific to
any agent. The answer always is. probes.rs holds the questions; the impls
stay where they are.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

### Task 7: `BlockedProbe` into `watcher/probes.rs`

**Files:**
- Modify: `src-tauri/src/watcher/probes.rs`, `src-tauri/src/watcher/blocked.rs`, `src-tauri/src/watcher/state.rs:6`
- Test: none, for the reason given in Task 6.

**Interfaces:**
- Consumes: `watcher/probes.rs` as created by Task 6.
- Produces: `crate::watcher::probes::BlockedProbe`.

Same shape as Task 6. Move the `BlockedProbe` declaration from `blocked.rs:15`
into `probes.rs` with its full doc comment — which explains that a session
sitting on an unanswered `AskUserQuestion` is quiet and renders grey while
being genuinely blocked. `TranscriptBlocked`, `NoBlocked` and `FakeBlocked`
stay in `blocked.rs`, which gains
`use crate::watcher::probes::BlockedProbe;`. `state.rs:6` moves to the new path.

- [ ] **Step 1: Move the declaration, add the import in `blocked.rs`, repoint `state.rs:6`**
- [ ] **Step 2: `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A 4 | head -40`, fix each import path it names**
- [ ] **Step 3: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1` — expect 409, no new warnings**
- [ ] **Step 4: Commit**

```bash
git status
git add src-tauri/src/watcher/probes.rs src-tauri/src/watcher/blocked.rs src-tauri/src/watcher/state.rs
git commit -m "$(cat <<'EOF'
refactor: move BlockedProbe to probes.rs

Same reason as ActivityProbe before it: state.rs named one of its inputs
through blocked.rs, whose only real implementation reads a Claude Code
transcript.

Whether a session is waiting on its user is a question any agent has. The
transcript is only this one's way of answering it.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

### Task 8: `WorkProbe` into `watcher/probes.rs`

**Files:**
- Modify: `src-tauri/src/watcher/probes.rs`, `src-tauri/src/watcher/working.rs`, `src-tauri/src/watcher/state.rs:11`
- Test: none, for the reason given in Task 6.

**Interfaces:**
- Consumes: `watcher/probes.rs`.
- Produces: `crate::watcher::probes::WorkProbe`.

Move the `WorkProbe` declaration from `working.rs:15` into `probes.rs` with its
full doc comment — which explains that a transcript is silent for as long as a
single tool call takes, so a build or test run reads as `idle` on mtime alone.
`TranscriptWork`, `NoWork` and `FakeWork` stay in `working.rs`.

- [ ] **Step 1: Move the declaration, add the import in `working.rs`, repoint `state.rs:11`**
- [ ] **Step 2: `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A 4 | head -40`, fix each import path it names**
- [ ] **Step 3: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1` — expect 409, no new warnings**
- [ ] **Step 4: Commit**

```bash
git status
git add src-tauri/src/watcher/probes.rs src-tauri/src/watcher/working.rs src-tauri/src/watcher/state.rs
git commit -m "$(cat <<'EOF'
refactor: move WorkProbe to probes.rs

Third of the four. state.rs named the trait through working.rs, whose
only real implementation reads a Claude Code transcript.

"Is a tool call still running" is a question with an answer for every
agent that runs tools.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

### Task 9: `TitleProbe` into `watcher/probes.rs`

**Files:**
- Modify: `src-tauri/src/watcher/probes.rs`, `src-tauri/src/watcher/title.rs`, `src-tauri/src/watcher/state.rs:10`
- Test: none, for the reason given in Task 6.

**Interfaces:**
- Consumes: `watcher/probes.rs`.
- Produces: `crate::watcher::probes::TitleProbe`. After this task `state.rs`
  imports only `probes`, `session`, `task` and `liveness` — no module that
  reads a transcript.

Move the `TitleProbe` declaration from `title.rs:16` into `probes.rs` with its
full doc comment — which explains that the registry only carries
`<folder>-<2 chars>` because `nameSource` is `derived` for every session, so
the row would read as a list of repositories.

**`FULL_SCAN_MAX_BYTES` at `title.rs:24` does not move.** It is
`pub const FULL_SCAN_MAX_BYTES: u64 = crate::bridge::transcript::FULL_SCAN_MAX_BYTES;`
— a re-export of a transcript constant, which belongs with the implementation
that scans transcripts, not with the trait. `TranscriptTitle`, `NoTitle` and
any fake stay in `title.rs`.

- [ ] **Step 1: Move the declaration only, leaving `FULL_SCAN_MAX_BYTES` and every impl in `title.rs`; add the import there; repoint `state.rs:10`**
- [ ] **Step 2: `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A 4 | head -40`, fix each import path it names**
- [ ] **Step 3: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1` — expect 409, no new warnings**
- [ ] **Step 4: Verify the goal is actually met**

Run: `grep -n '^use crate' src-tauri/src/watcher/state.rs`

Expected: imports from `probes`, `session`, `task` and `liveness` only. If any
line still names `activity`, `blocked`, `working`, `title`, `tasks`, `registry`
or `bridge`, the extension is incomplete — report it rather than adding a
further module on your own initiative.

- [ ] **Step 5: Commit**

```bash
git status
git add src-tauri/src/watcher/probes.rs src-tauri/src/watcher/title.rs src-tauri/src/watcher/state.rs
git commit -m "$(cat <<'EOF'
refactor: move TitleProbe to probes.rs

Last of the four. state.rs now imports probes, session, task and
liveness, and nothing that reads a transcript — which was what this plan
set out to achieve and had not, until the four probe traits followed
TaskProbe out of their implementation files.

FULL_SCAN_MAX_BYTES stays in title.rs: it re-exports a transcript
constant, so it belongs with the code that scans transcripts.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

## Done when (revised)

- `snapshot()` takes `&[RawSession]`, and `state.rs` imports only `probes`,
  `session`, `task` and `liveness` — nothing that reads a transcript, and
  nothing from `registry.rs` or `tasks.rs`.
- `cargo test -- --test-threads=1` passes at 409, with no new warnings.
- The fixture run shows the same session states it did before the branch.
- The spec records where all four previously-undecided modules go, and that the
  probe-trait entanglement was resolved here rather than deferred.

---

## Phase 1, second extension: the couplings the whole-branch review found

Added after Tasks 1-9 landed. The final review traced every import in the
core-bound set and found four survivors, three of which are the same shape as
couplings this branch already fixed. Finding 4 (a `RawSession.proc_start` doc
comment asserting a use the codebase deliberately avoids) was fixed
immediately in `03426c0`. These tasks close the rest.

The argument for doing them here is the one that pulled the probe splits into
phase 1: each is a compiler-checked change in one repository now, or a broken
dependency between two repositories later.

### Task 10: the `From<RegistryFile>` impl belongs in `registry.rs`

**Files:**
- Modify: `src-tauri/src/watcher/session.rs` (lose the impl, its import, and its two tests), `src-tauri/src/watcher/registry.rs` (gain them)

**Interfaces:**
- Produces: `impl From<RegistryFile> for RawSession` at its new home. `RawSession::from(file)` keeps working for every caller, so `watch.rs` needs no change.

The spec says the app keeps `RegistryFile` "plus a `From<RegistryFile> for
RawSession`". Task 1 put the impl in `session.rs` instead, which the plan
specified and nobody caught for nine tasks. The result is that `session.rs` is
the only core-bound module importing a staying one, so phase 2 would have to
split the file rather than move it.

Rust's orphan rule permits `impl From<LocalType> for ForeignType` here — there
are no uncovered type parameters — so this still compiles once `RawSession`
lives in another crate.

- [ ] **Step 1: Move the impl and both tests**

Move `impl From<RegistryFile> for RawSession` out of `session.rs` into
`registry.rs`, and with it the two tests `conversion_preserves_every_field` and
`absent_optional_fields_stay_absent` plus their `registry_file()` helper.
`registry.rs` gains `use crate::watcher::session::RawSession;`. `session.rs`
loses `use crate::watcher::registry::RegistryFile;` and should end with **zero**
`crate::` imports, matching `task.rs`, `probes.rs` and `liveness.rs`.

- [ ] **Step 2: `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A 4 | head -30`, fix each import path it names**
- [ ] **Step 3: Confirm the goal**

Run: `grep -c 'crate::' src-tauri/src/watcher/session.rs`

Expected: `0`. If not, report what remains rather than moving more on your own.

- [ ] **Step 4: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1` — expect 409, no new warnings**
- [ ] **Step 5: Commit**

```bash
git status
git add src-tauri/src/watcher/session.rs src-tauri/src/watcher/registry.rs
git commit -m "$(cat <<'EOF'
refactor: put the registry conversion with the registry

The spec always said the app keeps RegistryFile plus its From impl, and
this plan's own Task 1 put the impl beside RawSession instead. That made
session.rs the one core-bound module importing a module that stays, so
phase 2 would have had to split the file rather than move it.

Mapping one provider's file format onto the neutral record is that
provider's business. The orphan rule still allows the impl there once
RawSession is foreign.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

### Task 11: `now_ms` into `clock.rs`

**Files:**
- Create: `src-tauri/src/clock.rs`
- Modify: `src-tauri/src/lib.rs` (register the module), `src-tauri/src/watcher/watch.rs` (lose `now_ms`, import it), `src-tauri/src/tray.rs:25`, `src-tauri/src/notify.rs:9`, `src-tauri/src/commands.rs:36`, `src-tauri/src/usage_api.rs:95,211,223,264`

**Interfaces:**
- Produces: `crate::clock::now_ms() -> i64`.

`tray.rs` and `notify.rs` are both bound for core and both read `now_ms` out of
`watch.rs`, which stays — it imports `read_registry_dir`, `crate::usage` and
`crate::usage_api`. This is the fourth coupling of the shape the spec inventoried
three of.

**It goes in a new `clock.rs`, not in `rfc3339.rs`.** That module's own doc
scopes it to "RFC 3339 timestamps to epoch milliseconds — hand-rolled rather
than pulling in a date crate: two callers, one format". Reading the system
clock is a different job, and widening `rfc3339.rs` to hold it would falsify
its first line.

- [ ] **Step 1: Create `src-tauri/src/clock.rs`**

```rust
//! What time it is, in the units everything else here counts in.
//!
//! Its own module because the watcher loop is not the authority on the clock:
//! the tray, the notifier and the usage meter all need it, and half of those
//! are provider-agnostic while `watch.rs` is not.

/// Epoch milliseconds now.
///
/// A clock that cannot be read is treated as the epoch rather than a panic:
/// every caller is either rendering an age or comparing against a deadline, and
/// a widget that draws the wrong duration is better than one that does not draw.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
```

Move the function out of `watch.rs` (currently `watch.rs:46`), carrying whatever
doc comment it has there — if the original explains the `unwrap_or(0)`, prefer
the original's wording over the draft above and say so in your report.

Register it in `src-tauri/src/lib.rs` beside the other top-level modules,
alphabetically.

- [ ] **Step 2: Repoint every caller**

`watch.rs` gains `use crate::clock::now_ms;` — it calls `now_ms()` itself.
`tray.rs:25` and `notify.rs:9` change their `use` path. `commands.rs:36` and
`usage_api.rs:95,211,223,264` call it fully qualified as
`crate::watcher::watch::now_ms()`; change those to `crate::clock::now_ms()`.

- [ ] **Step 3: `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A 4 | head -30`, fix each site it names. Treat the compiler as the authority — the list above may be incomplete**
- [ ] **Step 4: Confirm no caller reaches the old path**

Run: `grep -rn 'watch::now_ms' src-tauri/src`

Expected: no output.

- [ ] **Step 5: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1` — expect 409, no new warnings**
- [ ] **Step 6: Commit**

```bash
git status
git add src-tauri/src/clock.rs src-tauri/src/lib.rs src-tauri/src/watcher/watch.rs src-tauri/src/tray.rs src-tauri/src/notify.rs src-tauri/src/commands.rs src-tauri/src/usage_api.rs
git commit -m "$(cat <<'EOF'
refactor: read the clock somewhere other than the watcher loop

tray.rs and notify.rs are both bound for the shared crate and both took
now_ms out of watch.rs, which is not — it reads the Claude Code registry
and the usage API. That is the same one-line borrow as CONFIG_EVENT and
SnapshotStore, and the spec's inventory of those missed it.

Not folded into rfc3339.rs: that module's first line scopes it to one
timestamp format with two callers, and reading the system clock is a
different job.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

### Task 12: the test doubles follow their traits

**Files:**
- Modify: `src-tauri/src/watcher/probes.rs` (gains eight types), `src-tauri/src/watcher/task.rs` (gains two), `src-tauri/src/watcher/activity.rs`, `blocked.rs`, `working.rs`, `title.rs`, `tasks.rs` (each loses its doubles), plus whatever `cargo build` names
- Test: no new tests. The existing ~90 call sites in `state.rs`'s test module are the test.

**Interfaces:**
- Produces: `crate::watcher::probes::{NoActivity, FakeActivity, NoBlocked, FakeBlocked, NoWork, FakeWork, NoTitle, FakeTitle}` and `crate::watcher::task::{NoTasks, FakeTasks}`.

Tasks 6-9 moved the four probe traits but left their test doubles behind, which
is half the pattern. `liveness.rs` is the model and the reason `FakeLiveness`
needs no move: trait, real impl and fake all in one agnostic file, so it moves
whole. `state.rs`'s test module imports ten doubles from five modules that stay,
at roughly ninety call sites — so `state.rs` cannot move with its own tests.

The doubles are agnostic: they hold `HashMap`/`HashSet` and answer from them,
with no `bridge::transcript` and no provider knowledge. Only the five
`Transcript*` implementations are provider-specific.

**What moves** (verbatim, with doc comments, and with their `impl` blocks,
because unlike the `Transcript*` types these implementations are the agnostic
part):

- `activity.rs:27` `NoActivity` and `:36` `FakeActivity`, with their impls at `:29` and `:59`
- `blocked.rs:65` `NoBlocked` and `:74` `FakeBlocked`, with their impls at `:67` and `:98`
- `working.rs:70` `NoWork` and `:79` `FakeWork`, with their impls at `:72` and `:102`
- `title.rs:133` `NoTitle` and `:142` `FakeTitle`, with their impls at `:135` and `:166`
- `tasks.rs:433` `NoTasks` and `:442` `FakeTasks`, with their impls at `:435` and `:465` — these two go to `task.rs`, beside `TaskProbe`

**What stays:** `TranscriptActivity`, `TranscriptBlocked`, `TranscriptWork`,
`TranscriptTitle`, `TranscriptTasks`, `FULL_SCAN_MAX_BYTES`, and every test in
those five files.

Line numbers will drift as you go — locate by name.

- [ ] **Step 1: Move the eight probe doubles into `probes.rs`**

Append after the four traits. Group each double with its trait's neighbours or
keep all eight together, whichever reads better once written — say which you
chose and why in your report.

If a moved type needs `std::collections::HashMap` or `HashSet`, add the import
to `probes.rs`; check whether the source file still needs its own afterwards and
remove it if not, since a stray unused import is a new warning.

- [ ] **Step 2: Move `NoTasks` and `FakeTasks` into `task.rs`**
- [ ] **Step 3: Add the imports each stripped file now needs, and repoint every consumer**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" -A 4 | head -60`

There will be many. `state.rs`'s test module, `watch.rs`'s tests and `lib.rs`
are all expected. Change import paths only.

- [ ] **Step 4: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1` — expect exactly 409, no new warnings**

A count other than 409 means a test was lost in the move. Investigate before
committing.

- [ ] **Step 5: Confirm `state.rs` is free, tests included**

Run: `grep -n 'crate::watcher::\(activity\|blocked\|working\|title\|tasks\|registry\)' src-tauri/src/watcher/state.rs`

Expected: no output — including inside `#[cfg(test)]`. The Task 9 check used
`^use crate` anchored at column 0 and so could not see the test module's
indented imports, which is why this coupling survived nine tasks. Put the
actual output in your report.

- [ ] **Step 6: Commit**

```bash
git status
git add src-tauri/src/watcher/probes.rs src-tauri/src/watcher/task.rs src-tauri/src/watcher/activity.rs src-tauri/src/watcher/blocked.rs src-tauri/src/watcher/working.rs src-tauri/src/watcher/title.rs src-tauri/src/watcher/tasks.rs src-tauri/src/watcher/state.rs src-tauri/src/watcher/watch.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
refactor: let the test doubles follow their traits

Moving the four probe traits without their No*/Fake* implementations was
half the job. state.rs's test module — most of its 2344 lines — imported
ten doubles from five modules that stay behind, so state.rs could not
have moved with its own tests.

liveness.rs is the pattern and the reason FakeLiveness is not in this
commit: trait, real impl and fake in one agnostic file, so it travels
whole. The doubles here answer from a HashMap and know nothing about any
agent; only the Transcript* implementations are provider-specific, and
those stay.

The check that missed this was `grep '^use crate'`, anchored at column
zero and therefore blind to an indented import inside a test module.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

### Task 13: record what phase 2 still has to decide

**Files:**
- Modify: `docs/superpowers/specs/2026-09-03-multi-provider-repo-split-design.md`, `src-tauri/src/watcher/probes.rs` (one doc line), `src-tauri/src/watcher/state.rs` (parameter name and prose)

Three documentation debts and one rename, all called out by the whole-branch
review.

- [ ] **Step 1: Add the five new modules to the spec's inventory**

`session.rs`, `task.rs`, `probes.rs` and `store.rs` appear in neither the "moves
to `buddy-core`" list nor the "stays" table, and `watch.rs` is unassigned in
either direction. Add all five with a line each. `store.rs` matters most: this
branch created it so a core-bound `window.rs` would not reach into `watch.rs`,
and the document phase 2 gets planned from does not say it goes to core.

- [ ] **Step 2: Record the vocabulary problem as an open phase 2 decision**

`state.rs` has `ALLOWED_ENTRYPOINTS = ["cli", "claude-desktop"]` and
`SHOWN_KINDS = ["interactive", "bg"]`, and `RawSession.kind`'s doc pins the
spelling as "`interactive`, `bg` or `sdk`". A Cursor adapter is not blocked — it
can emit those strings — but it has to speak Claude Code's enum spellings to be
rendered at all, and `"claude-desktop"` means nothing to it. So `RawSession`
drops the provider's serde spelling and its file, and keeps the provider's
vocabulary.

Record it as a decision the `Provider` trait has to make: an injected allowlist,
or core `SessionKind`/`Entrypoint` enums each adapter maps onto. Do not decide
it here.

- [ ] **Step 3: Fix `probes.rs`'s module doc**

It says the traits are "injected into `state::snapshot` … matching
`PidLiveness` (`liveness.rs`) and `QuestionProbe` (`question.rs`)".
`QuestionProbe` is not a `snapshot()` parameter — it goes to `spawn_watcher` and
`question::enrich_alerts`. `TaskProbe` is one, and does live in another file.
Swap them.

- [ ] **Step 4: Rename `snapshot()`'s first parameter and sweep the prose**

The parameter is `files`, its doc comment calls it "the registry", and
`job_tasks`'s doc says "Every live registry job", "the only link the registry
offers" and "Taken from the unfiltered registry". In `buddy-core` there is no
registry. Rename the parameter to `sessions` and correct those references — the
compiler checks the rename, which is why it belongs in one pass rather than
being left for later.

- [ ] **Step 5: `cd src-tauri && cargo fmt && cargo test -- --test-threads=1` — expect 409. `npm run typecheck && npm test` — expect 254**
- [ ] **Step 6: Commit**

```bash
git status
git add docs/superpowers/specs/2026-09-03-multi-provider-repo-split-design.md src-tauri/src/watcher/probes.rs src-tauri/src/watcher/state.rs
git commit -m "$(cat <<'EOF'
docs: say what phase 2 inherits, and stop calling sessions files

Four modules this branch created were in neither half of the spec's
inventory, and watch.rs was in neither either. store.rs matters most: it
exists so a core-bound window.rs would not reach into the watcher loop,
and the document phase 2 gets planned from did not say where it goes.

Also records the one thing RawSession did not neutralize. It drops Claude
Code's serde spelling and its file, but ALLOWED_ENTRYPOINTS still contains
"claude-desktop" and SHOWN_KINDS still contains "bg", so an adapter has to
speak this provider's vocabulary to be rendered. That is the Provider
trait's problem to solve, and better written down than rediscovered while
writing cursor-buddy.

The parameter rename is mechanical but the compiler can only check it
while the code is in one crate.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

## Done when (final)

- `session.rs`, `task.rs`, `probes.rs` and `liveness.rs` have zero `crate::` imports.
- `grep -rn 'crate::watcher::\(activity\|blocked\|working\|title\|tasks\|registry\)' state.rs` is empty, test module included.
- No core-bound module imports anything from `watch.rs`.
- 409 Rust and 254 frontend tests pass, `cargo fmt --check` and `tsc --noEmit` clean.
- The spec's inventory names every module in the tree, and records the vocabulary question as phase 2's to answer.

---

### Task 14: close the map phase 2 reads

The second whole-branch review withheld merge for one reason: the spec is the
artifact phase 2 gets planned from, and it does not name every module. Seed
`buddy-core` from its lists as they stand and the crate does not compile.

Decided by the author, so this task records rather than asks: **`bridge/raise.rs`
and `bridge/proc_tree.rs` both move to core.** Verified — `raise.rs` depends only
on `proc_tree`, and `proc_tree.rs`'s production code is a generic walk from a pid
to its hosting `.app` bundle. "Claude" appears in one `proc_tree.rs` doc comment,
as the example of a nested `.app`, and in test fixtures. So `notify.rs:145`'s
concrete call to `raise_pid` needs no injection: callee and caller travel
together.

**Files:** `docs/superpowers/specs/2026-09-03-multi-provider-repo-split-design.md`,
`src-tauri/src/watcher/session.rs`, `src-tauri/src/watcher/state.rs`,
`src-tauri/src/watcher/probes.rs`

- [ ] **Step 1: Place the four unassigned modules in the spec inventory**

`liveness.rs` → core. It has zero `crate::` imports and `state.rs:5` takes
`PidLiveness` from it, so a core without it cannot build. This is the one that
withheld merge.

`bridge/raise.rs` → core, and `bridge/proc_tree.rs` → core, per the decision
above. Note in the spec that the Claude references in `proc_tree.rs` are a doc
example and test fixtures, not behaviour, so an adapter inherits nothing it must
satisfy.

`commands.rs` → stays. It is the Tauri command surface for this app.

- [ ] **Step 2: Rewrite the stale "further couplings" paragraph**

It is wrong in both directions. It still lists `tray.rs:265` reading
`crate::commands::CONFIG_EVENT`, which `cc0646e` moved to `config.rs`. And it
says `raise_pid` is "already behind an `Activator` trait" — it is not, from
`notify.rs`'s side: the trait is used *inside* `raise()`, while `raise_pid` is
the wrapper that hardcodes `PsProcTree::snapshot()` and `OpenActivator`, and
`notify.rs:145` calls that wrapper. Replace the paragraph with what is now true:
all of these are resolved, three by relocation and the fourth by sending callee
and caller to core together.

- [ ] **Step 3: Correct the last two stale spec passages**

The paragraph beginning "`watcher/state.rs` (2344) moves to core, but **not
as-is**" still cites `state.rs:8-9` for imports that are no longer there. It
reads as the rationale for phase 1, which is fine, but mark it as describing the
branch point rather than the present. And one line where `03426c0` spliced in a
correction runs to about 200 characters — rewrap it to the document's ~78.

- [ ] **Step 4: Fix the `RawSession` doc comment that still contradicts itself**

`session.rs:4-5` says the record is "the same twelve fields `RegistryFile`
carries, because `snapshot()` reads all twelve". That is the claim `03426c0`
corrected fifteen lines below, on the same type. Eleven are read in production.
Say that, and say the twelfth is carried so the record stays a faithful picture
of a session rather than of one state machine's current appetite — which is the
reasoning already written on the field itself.

- [ ] **Step 5: Finish the rename inside `state.rs`**

`f63c2f6` renamed the public parameter; the private helpers kept the old
vocabulary. `display_name` (`:122`), `allowed_entrypoint` (`:146`) and
`is_own_session` (`:155`) each take `file: &RawSession`; `job_parent` (`:165`)
takes `files`; `job_tasks` (`:185`) takes `files`; and the test factory
`fn file(…) -> RawSession` (`:453`) has roughly 87 call sites. In core there is
no file — a `RawSession` came from wherever its provider keeps sessions, and
core's own tests would otherwise read `file(1, "claude-desktop")`.

Rename them to `session`/`sessions`, and the factory to `session`. The compiler
checks all of it, which is the whole reason to do it before the file changes
crates. `job_tasks`'s doc currently says "Taken from the unfiltered `sessions`"
— a name that does not exist in that function until this step lands, so it
becomes correct rather than needing a further edit.

As in Task 13: leave every "registry" reference that genuinely describes Claude
Code's on-disk registry or the `job_id`/`kind` semantics it defines.

- [ ] **Step 6: One wording fix in `probes.rs`**

`probes.rs:3-4` says "each implementation **here** reads a Claude Code
transcript" in the same sentence that explains the implementations live in other
modules. "each implementation of them" removes the stumble.

- [ ] **Step 7: Full gate**

`npm run typecheck && npm test`, then `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`. Expect exactly 409 and 254 — a rename and doc edits change neither.

- [ ] **Step 8: Commit**

```bash
git status
git add docs/superpowers/specs/2026-09-03-multi-provider-repo-split-design.md src-tauri/src/watcher/session.rs src-tauri/src/watcher/state.rs src-tauri/src/watcher/probes.rs
git commit -m "$(cat <<'EOF'
docs: finish the map, and stop calling a session a file

Seeding buddy-core from the spec's lists would not have compiled:
liveness.rs is a snapshot() input that no list assigned. raise.rs and
proc_tree.rs are now placed too, both in core — proc_tree's production
code walks a pid to its hosting .app and mentions Claude only in a doc
example and its fixtures, so notify.rs's concrete call to raise_pid needs
no injection when callee and caller travel together.

The paragraph naming what was left coupled had gone wrong in both
directions, stale on CONFIG_EVENT, which cc0646e moved, and reassuring
about raise_pid, which was never behind the trait from notify.rs's side.

Also finishes the rename f63c2f6 started at the public boundary only. The
private helpers and the test factory still said file, so core's own state
tests would have read file(1, "claude-desktop").

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```
