# Session task monitoring

A sixth session state, `tasking`, for a session that is sitting still only
because it is waiting on a background task — and a popover block naming which
tasks those are.

## Problem

A session that fires off a background test run, a dev server, a watch or a
background subagent goes quiet. Nothing is appended to its transcript and it
writes no `busy` status, so after ten minutes it reads `paused` — visually
identical to a session somebody walked away from. The widget's whole claim is
that a glance tells you where every session stands, and this is the one case
where the glance lies: the session is not finished with you, it is going to
wake up, and there is no way to tell it apart from one that will not.

`work_in_flight` already covers the adjacent case — a *foreground* tool call
with no result yet keeps a statusless session `busy`. Background tasks are the
inverse: the tool call returned immediately, the turn ended, and the work
outlives it.

## What it does

A session whose state would otherwise be `idle` or `paused`, and which has at
least one background task still running, reads `tasking` instead: its own dot
glyph, its own count on the collapsed chip, and its own line in the popover
listing every running task with a label and a live age.

When a task ends, a notification fires naming it and how it ended.

`waiting`, `busy` and `dead` are untouched. A session asking a question is
never relabelled as merely tasking.

## Design

### What counts as a task

Four kinds, all folded into one list:

| Kind       | Origin |
|------------|--------|
| `Shell`    | `Bash` with `run_in_background: true` |
| `Watch`    | Long-poll task tools (`Monitor` and kin) |
| `Subagent` | A background `Agent` run |
| `Job`      | A registry entry with `kind: "bg"` and a `jobId` |

Foreground subagents are deliberately absent: they block the turn, so
`work_in_flight` already reads them as `busy`, and counting them twice would
move a genuinely working session out of the state that describes it.

### Evidence — what Claude Code already writes

The first three kinds are visible in the session transcript.

A task starts with a `user` record carrying its id in `toolUseResult`:

```json
{"type":"user","timestamp":"2026-08-28T08:42:47.177Z",
 "toolUseResult":{"backgroundTaskId":"bmd0i64ke","timedOutAfterMs":120000}}
```

`backgroundTaskId` is a shell; `taskId` (which arrives with a `timeoutMs`) is a
watch. The kind is refined to `Subagent` by looking up the tool name behind the
originating `tool_use_id`, which the same scan has already seen.

A task ends with a `<task-notification>` block:

```
<task-notification>
  <task-id>bmd0i64ke</task-id>
  <output-file>…/tasks/bmd0i64ke.output</output-file>
  <status>completed</status>
  <summary>Background command "npm test" completed…</summary>
</task-notification>
```

Four terminal statuses occur on disk: `completed`, `failed`, `killed`,
`stopped`. Every one of them is written by the session process itself, which is
why a task that ends while its session is paused still gets recorded — that
record *is* the wake-up.

Each notification lands **twice**, once as a `queue-operation` and once as an
`attachment` carrying the same text. Events are deduplicated on
`(id, status)`, first occurrence winning.

`<summary>` is the task's label, truncated to `ACTIVITY_MAX_CHARS`. A shell
with no summary yet falls back to its `Bash` description.

The `tasks/` scratch directory under `/private/tmp/claude-501/…` was
considered as a source and rejected: it keeps an `.output` file for every task
the session has *ever* run, with nothing to say which are still alive, so
liveness would have had to come from somewhere else anyway.

### Reading it without reading megabytes

Transcripts reach megabytes, and both halves of a task's story — start and
finish — can be arbitrarily far apart in the file. A fixed tail cannot see
them both, which is the bug already fixed once in `title.rs` when a title
buried early in a large transcript read as absent.

Transcripts are append-only, so the probe caches per session:

- mtime unchanged: cache hit, no read at all, as `TranscriptWork` does
- file grew: read only the appended bytes and fold the new events in
- file shrank: full re-scan, since the file is not the one that was cached

One full scan per session at first sight, size-guarded as `title.rs` is, then a
few kilobytes per tick.

### The phantom bound

A `Started` event with no matching terminal event is a running task. That is
only true within one process: a resumed session appends to the *same*
transcript, so the previous process's unfinished tasks would read as running
forever.

Started events older than the registry's `startedAt` for that session are
therefore dropped. This needs no arbitrary timeout, which matters — a dev
server legitimately runs for hours, so any age cap would either kill real tasks
or be too loose to help.

One residual case is accepted: a shell killed from outside Claude Code, with
`kill -9`, produces no notification and reads as running until its session
ends. Correcting it means a `ps` descendant walk on every tick, which is a real
cost for a rare case.

### Registry jobs

`kind: "bg"` entries are separate processes and are not in any transcript.
`snapshot()` folds them in itself, matched to their parent by `cwd` — the same
pairing `group_jobs_with_parents` already performs — and each counts as a
running `Job` task for as long as its pid is alive.

With `showBackgroundJobs` on, a job is therefore both a demoted row of its own
and a task on its parent. That is two views of one fact rather than a
double-count, and hiding one of them would make the other harder to read.

### `watcher/tasks.rs`

Built in the shape of `working.rs`: a trait, the transcript-backed
implementation, a no-op and a fake.

```rust
pub enum TaskKind { Shell, Watch, Subagent, Job }
pub enum TaskStatus { Running, Completed, Failed, Killed, Stopped }

pub struct Task {
    pub id: String,
    pub kind: TaskKind,
    /// The notification's summary, else the Bash description.
    pub label: Option<String>,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub status: TaskStatus,
}

pub trait TaskProbe {
    fn tasks(&self, cwd: &str, session_id: &str) -> Vec<Task>;
}
```

The parsing itself is pure functions in `bridge/transcript.rs`, beside
`has_work_in_flight` and `pending_user_prompt`, so it is tested on byte
slices with no filesystem.

Terminal tasks stay in the list for 60 seconds after they end, mirroring
`DEAD_RETENTION_MS`, so the alert diff is guaranteed to observe the edge even
though the session usually wakes and changes state in the same tick.

### State

```
Waiting 0 · Busy 1 · Tasking 2 · Idle 3 · Paused 4 · Dead 5
```

`Tasking` is reached only where the current derivation yields `Idle` or
`Paused` and at least one task is `Running`. `detail` carries the single task's
label, or `"3 tasks running"` when there is more than one.

Ranked below `Busy` because a session working on its own turn is the more
immediate fact, and above `Idle` because tasking is not stillness.

### Snapshot payload

`SessionSnapshot` gains `tasks: Vec<Task>`, serialised `tasks`. It ships in the
snapshot rather than being fetched lazily like `TranscriptDetail`, because
`diff_alerts` needs the labels in Rust at the moment of the edge, and a handful
of rows with 64-character labels is a small addition to an update that already
carries every session.

`fingerprint` must hash task ids and statuses — not ages — or a task starting
on an otherwise-unchanged session would never reach the frontend.

### The dot

Dots are glyphs, not colours: waiting is a triangle, busy solid, idle a hollow
ring, paused two bars, dead a cross. `tasking` is idle's hollow ring with a
rotating arc inside it, in a new `--tasking` token.

The arc animates `transform` on a pseudo-element rather than the dot's
`box-shadow`, for the reason recorded on `.dot-waiting::after`: shadow
keyframes repaint the whole pill every frame and read as general choppiness.
It is suppressed under `prefers-reduced-motion`, where the arc is drawn static.

`STATE_ORDER` in `StateCounts.tsx` gains `tasking` between `busy` and `idle`;
`countByState` in `format.ts` gains the key.

### Popover

A `tasks` block below `activity`: one line per running task, its kind, its
label, and an age ticking from `startedAtMs` the way `elapsedMs` already does.
Absent entirely when there are no tasks, so nothing changes for the sessions
that have none.

### Alerts

The interesting moment is not a state transition. A finishing task wakes its
session, so the state normally goes `Tasking → Busy`, and an edge-triggered
state diff would report that as a session starting work rather than as a task
landing.

`diff_alerts` therefore gains a task diff beside its state diff: an id that
moved from `Running` to any terminal status fires one alert, carrying the
label and the status, so a failure reads as a failure.

New `AlertKind::TaskDone`, with `Config` gaining `alert_task_done: bool`
(serialised `alertTaskDone`) defaulting to `true` and a fourth checkbox in the
settings panel. Reusing `Finished` was the cheaper option and was rejected:
`Finished` means the session finished its turn, and one toggle governing both
would make it impossible to ask for only one of them.

`Config` is `#[serde(default)]` throughout, so an existing settings file loads
unchanged.

### Knock-ons

- `visibility::should_hide` — `nothingActive` counts `Tasking`, so the widget
  stays on screen while a task cooks. That is precisely when it is wanted.
- `awake::should_stay_awake` — `Tasking` holds the display up, like `Busy` and
  `Waiting`.
- `heat.ts` — `fire` counts tasking sessions alongside busy, still capped at
  three, **except** a session whose only running tasks are `Job`. That keeps
  the existing rule that a background job is work you did not start, while a
  test run you launched yourself does stoke the widget.

## Testing

- `tasks.rs` — start and terminal event parsing; the double notification
  deduplicated; an appended transcript read incrementally; a truncated one
  re-scanned in full; started events before `startedAt` dropped; a missing
  transcript reporting nothing
- `transcript.rs` — the parse functions over byte slices, including a tail that
  begins mid-record
- `state.rs` — `Tasking` reached from `Idle` and from `Paused`; `Waiting`,
  `Busy` and `Dead` each winning over it; rank ordering; registry jobs folded
  onto the parent by cwd
- `alerts.rs` — the `Running`-to-terminal edge firing once with its label; a
  failed task reading as failed; cold start firing nothing; the retention
  window making the edge observable
- `visibility.rs`, `awake.rs`, `heat.test.ts` — the new state counted, and
  `Job`-only tasking excluded from heat
- Frontend — `StateCounts` rendering the count, `SessionEntry` carrying
  `data-state="tasking"`, the popover's tasks block appearing and staying
  absent when empty, `format.countByState`
- `fixtures/` — a session with running tasks of more than one kind, so
  `scripts/dev-fixtures.sh` shows the state without real session data

## What this owes

- **README** — the new state and its dot in the state table, and the new alert
  toggle in the settings section
- **CHANGELOG** — user-visible: a new state, a new notification, a new setting
