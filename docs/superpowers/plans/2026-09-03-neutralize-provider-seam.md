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
