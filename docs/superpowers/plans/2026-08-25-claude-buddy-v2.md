# claude-buddy v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix three correctness bugs in the shipped widget and add five features, removing the dead view-mode UI along the way.

**Architecture:** The existing three layers are unchanged — a Rust watcher owning all derivation, a Rust bridge for what a webview cannot do, and a React frontend that renders precomputed snapshots. Every new decision is a pure function tested without a filesystem or a clock; the impure parts (notification delivery, panel visibility, transcript reads) are injected behind traits exactly as `PidLiveness` and `ActivityProbe` already are.

**Tech Stack:** Rust, Tauri 2, `tauri-nspanel`, `mac-notification-sys`, `tauri-plugin-updater`, React 19, TypeScript, Vitest.

## Global Constraints

- macOS only. Minimum system version 13.0.
- claude-buddy is strictly read-only against `~/.claude`. Never write, move or unlink anything there.
- Rust tests run with `--test-threads=1` because the watcher-loop tests use real files and real time.
- `cargo` is not on the default `PATH` in this environment. Prefix Rust commands with `PATH="$HOME/.cargo/bin:$PATH"`.
- Rust serializes `camelCase`; `src/types.ts` mirrors the Rust structs and the names must match exactly.
- No new runtime network calls except the updater's manifest check.
- Every task ends with a passing full suite and a commit.

---

### Task 1: Elapsed time from absolute timestamps

**Files:**
- Modify: `src-tauri/src/watcher/state.rs`
- Modify: `src-tauri/src/watcher/alerts.rs` (test fixture struct literal)
- Modify: `src/types.ts`
- Modify: `src/views/dotRow/SessionPopover.tsx`
- Test: `src-tauri/src/watcher/state.rs` (inline `mod tests`)
- Test: `src/views/dotRow/SessionPopover.test.tsx`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `SessionSnapshot.status_time_ms: i64` and `SessionSnapshot.started_at_ms: i64`, serialized as `statusTimeMs` and `startedAtMs`. Tasks 2, 6 and 7 construct `SessionSnapshot` values in tests and must set both.

- [ ] **Step 1: Write the failing Rust test**

Add to the `mod tests` block in `src-tauri/src/watcher/state.rs`, just above `fn empty_input_yields_empty_output`:

```rust
    #[test]
    fn snapshot_carries_absolute_timestamps_alongside_derived_ages() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 6 * 60_000);

        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);

        assert_eq!(out[0].status_time_ms, NOW - 6 * 60_000);
        assert_eq!(out[0].started_at_ms, NOW - 60_000);
        assert_eq!(out[0].elapsed_ms, 6 * 60_000);
    }

    #[test]
    fn status_time_falls_back_to_started_at_when_absent() {
        let f = file(1, "cli");
        let out = snapshot(&[f], &alive(1), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true);
        assert_eq!(out[0].status_time_ms, NOW - 60_000);
        assert_eq!(out[0].started_at_ms, NOW - 60_000);
    }
```

- [ ] **Step 2: Run it to make sure it fails**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test snapshot_carries_absolute -- --test-threads=1`
Expected: FAIL — `no field 'status_time_ms' on type 'SessionSnapshot'`

- [ ] **Step 3: Add the fields**

In `src-tauri/src/watcher/state.rs`, add to `struct SessionSnapshot` immediately after `pub uptime_ms: i64,`:

```rust
    /// Absolute epoch time the current state began. The frontend derives a
    /// live elapsed value from this: `fingerprint` deliberately ignores
    /// clock-derived fields, so `elapsed_ms` is only refreshed when state
    /// changes and is stale for anything sitting still.
    pub status_time_ms: i64,
    /// Absolute epoch time the session started.
    pub started_at_ms: i64,
```

In the `.map(|f| { ... })` closure, add to the `SessionSnapshot { ... }` literal after `uptime_ms: age(now_ms, f.started_at),`:

```rust
                status_time_ms: status_time,
                started_at_ms: f.started_at,
```

- [ ] **Step 4: Fix the alerts test fixture**

In `src-tauri/src/watcher/alerts.rs`, in `mod tests`, add to the `SessionSnapshot { ... }` literal inside `fn snap` after `uptime_ms: 0,`:

```rust
            status_time_ms: 0,
            started_at_ms: 0,
```

- [ ] **Step 5: Run the Rust suite**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test -- --test-threads=1`
Expected: PASS, 169 tests.

- [ ] **Step 6: Mirror the fields in TypeScript**

In `src/types.ts`, add to `interface SessionSnapshot` after `uptimeMs: number`:

```ts
  /** Absolute epoch ms the current state began; the popover ticks from this. */
  statusTimeMs: number
  /** Absolute epoch ms the session started. */
  startedAtMs: number
```

- [ ] **Step 7: Write the failing frontend test**

Add to `src/views/dotRow/SessionPopover.test.tsx`. Keep the file's existing imports and add `act` from `@testing-library/react` and `vi` from `vitest` if they are not already imported:

```tsx
it('advances elapsed and uptime as time passes, without new props', async () => {
  vi.useFakeTimers()
  const now = 1_700_000_000_000
  vi.setSystemTime(now)

  const session = {
    ...baseSession,
    statusTimeMs: now - 65_000,
    startedAtMs: now - 5 * 60_000,
  }
  render(<SessionPopover session={session} />)

  expect(screen.getByTestId('popover-state')).toHaveTextContent('1m')

  await act(async () => {
    vi.setSystemTime(now + 60_000)
    vi.advanceTimersByTime(1_000)
  })

  expect(screen.getByTestId('popover-state')).toHaveTextContent('2m')
  vi.useRealTimers()
})
```

If the file has no `baseSession` helper, add one above the tests:

```tsx
const baseSession = {
  pid: 4242,
  sessionId: 'session-a',
  name: 'api-service-55',
  cwd: '/Users/n/Code/api-service',
  entrypoint: 'cli',
  state: 'waiting' as const,
  detail: 'input needed',
  elapsedMs: 0,
  uptimeMs: 0,
  statusTimeMs: 0,
  startedAtMs: 0,
  background: false,
}
```

- [ ] **Step 8: Run it to make sure it fails**

Run: `npm test -- SessionPopover`
Expected: FAIL — the text stays at `1m` because the component reads the frozen `elapsedMs`.

- [ ] **Step 9: Tick the popover's own clock**

In `src/views/dotRow/SessionPopover.tsx`, add below the existing `EMPTY` constant:

```tsx
/** How often the popover recomputes its ages. */
const TICK_MS = 1000

/**
 * Wall-clock now, refreshed on an interval.
 *
 * The watcher deliberately does not re-emit for the passage of time — its
 * change fingerprint ignores clock-derived fields so the row does not re-render
 * twice a second. That means `elapsedMs` on a snapshot is the age at the moment
 * state last changed, which for anything sitting still is wrong and stays
 * wrong. The clock therefore lives here, where the value is displayed, and only
 * the open popover re-renders.
 */
function useNow(): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), TICK_MS)
    return () => clearInterval(timer)
  }, [])
  return now
}
```

Inside the component, add `const now = useNow()` below the `detail`/`error` state, and replace the two lines that read the snapshot's ages:

```tsx
  const elapsedMs = Math.max(0, now - session.statusTimeMs)
  const uptimeMs = Math.max(0, now - session.startedAtMs)
  const stateLine = `${session.detail ?? session.state} · ${formatElapsed(elapsedMs)}`
```

and in the `proc` field, replace `formatElapsed(session.uptimeMs)` with `formatElapsed(uptimeMs)`.

- [ ] **Step 10: Run the frontend suite**

Run: `npm test`
Expected: PASS. Fix any other popover test that asserted a frozen elapsed value by giving its fixture a `statusTimeMs` consistent with the expected text.

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/watcher/state.rs src-tauri/src/watcher/alerts.rs src/types.ts src/views/dotRow/SessionPopover.tsx src/views/dotRow/SessionPopover.test.tsx
git commit -m "fix: derive popover elapsed time from an absolute timestamp"
```

---

### Task 2: Dead retention measured from first observed death

**Files:**
- Modify: `src-tauri/src/watcher/state.rs`
- Modify: `src-tauri/src/watcher/watch.rs`
- Test: `src-tauri/src/watcher/state.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `SessionSnapshot` with `status_time_ms` / `started_at_ms` from Task 1.
- Produces: `snapshot()` takes a seventh parameter `first_seen_dead: &HashMap<String, i64>` and returns `SnapshotResult { sessions: Vec<SessionSnapshot>, dead_now: Vec<String> }`. Tasks 6 and 7 call `snapshot(...).sessions`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src-tauri/src/watcher/state.rs`:

```rust
    #[test]
    fn a_long_idle_session_that_dies_is_shown_and_not_swallowed() {
        // Regression: retention was measured from statusUpdatedAt, so a session
        // quiet for longer than the retention window was filtered out on the
        // very tick it was first seen dead — no red dot, and no died alert,
        // because diff_alerts only sees what survives this filter.
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 12 * 60_000);

        let out = snapshot(
            &[f], &FakeLiveness::new(), &NoActivity, NOW,
            PAUSED_THRESHOLD_MS, true, &HashMap::new(),
        );

        assert_eq!(out.sessions.len(), 1);
        assert_eq!(out.sessions[0].state, SessionState::Dead);
        assert_eq!(out.dead_now, vec!["session-1".to_string()]);
    }

    #[test]
    fn a_session_dead_longer_than_the_retention_window_drops_off() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);
        let seen = HashMap::from([("session-1".to_string(), NOW - DEAD_RETENTION_MS - 1)]);

        let out = snapshot(
            &[f], &FakeLiveness::new(), &NoActivity, NOW,
            PAUSED_THRESHOLD_MS, true, &seen,
        );

        assert!(out.sessions.is_empty());
    }

    #[test]
    fn a_session_dropped_by_retention_is_still_reported_as_dead_this_tick() {
        // dead_now must list every session observed dead, including those the
        // retention filter removed. Reporting only the survivors would drop the
        // map entry, making the same session look newly dead on the next tick
        // and resurrecting it forever.
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);
        let seen = HashMap::from([("session-1".to_string(), NOW - DEAD_RETENTION_MS - 1)]);

        let out = snapshot(
            &[f], &FakeLiveness::new(), &NoActivity, NOW,
            PAUSED_THRESHOLD_MS, true, &seen,
        );

        assert_eq!(out.dead_now, vec!["session-1".to_string()]);
    }

    #[test]
    fn a_live_session_is_not_reported_dead() {
        let out = snapshot(
            &[file(1, "cli")], &alive(1), &NoActivity, NOW,
            PAUSED_THRESHOLD_MS, true, &HashMap::new(),
        );
        assert!(out.dead_now.is_empty());
    }
```

- [ ] **Step 2: Run it to make sure it fails**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test a_long_idle_session_that_dies -- --test-threads=1`
Expected: FAIL — `snapshot` takes 6 arguments, not 7.

- [ ] **Step 3: Change the signature and the filter**

In `src-tauri/src/watcher/state.rs`, add at the top of the file:

```rust
use std::collections::HashMap;
```

Add above `pub fn snapshot`:

```rust
/// One derivation pass.
///
/// `dead_now` lists every session observed dead this tick, *including* those
/// the retention filter removed — the caller keys its first-seen-dead map off
/// this, and dropping the entry for a session that is still dead would make it
/// look newly dead again on the next tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapshotResult {
    pub sessions: Vec<SessionSnapshot>,
    pub dead_now: Vec<String>,
}
```

Change the signature and body. Replace everything from `pub fn snapshot(` down to the closing `group_jobs_with_parents(out)` with:

```rust
pub fn snapshot(
    files: &[RegistryFile],
    liveness: &dyn PidLiveness,
    activity: &dyn ActivityProbe,
    now_ms: i64,
    paused_threshold_ms: i64,
    include_background: bool,
    first_seen_dead: &HashMap<String, i64>,
) -> SnapshotResult {
    let derived: Vec<SessionSnapshot> = files
        .iter()
        .filter(|f| {
            f.entrypoint
                .as_deref()
                .is_some_and(|e| ALLOWED_ENTRYPOINTS.contains(&e))
                && is_shown(f.kind.as_deref(), f.job_id.as_deref(), include_background)
        })
        .map(|f| {
            // Only `cli` sessions report status. For the rest, transcript
            // activity is the only evidence that anything is happening.
            let reported_status = f.status_updated_at.is_some();
            let last_activity = if reported_status {
                None
            } else {
                activity.last_activity_ms(&f.cwd, &f.session_id)
            };
            let status_time = f
                .status_updated_at
                .or(last_activity)
                .unwrap_or(f.started_at);
            let elapsed_ms = age(now_ms, status_time);
            let alive = liveness.is_alive(f.pid, Some(f.started_at), now_ms);

            let state = if !alive {
                SessionState::Dead
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
                    _ => SessionState::Idle,
                }
            };

            SessionSnapshot {
                pid: f.pid,
                session_id: f.session_id.clone(),
                name: display_name(f),
                cwd: f.cwd.clone(),
                entrypoint: f.entrypoint.clone().unwrap_or_default(),
                state,
                detail: match state {
                    SessionState::Waiting => f.waiting_for.clone(),
                    _ => None,
                },
                elapsed_ms,
                uptime_ms: age(now_ms, f.started_at),
                status_time_ms: status_time,
                started_at_ms: f.started_at,
                background: is_background_job(f.kind.as_deref(), f.job_id.as_deref()),
            }
        })
        .collect();

    // Every session seen dead, recorded before retention can remove any of them.
    let dead_now: Vec<String> = derived
        .iter()
        .filter(|s| s.state == SessionState::Dead)
        .map(|s| s.session_id.clone())
        .collect();

    // A crash is worth showing once, not forever. Measured from when death was
    // first observed: `statusUpdatedAt` is the age of the last status write,
    // which for a session that had been quiet a while is already past the
    // window, so it would be filtered out before it could ever be shown.
    let mut out: Vec<SessionSnapshot> = derived
        .into_iter()
        .filter(|s| {
            if s.state != SessionState::Dead {
                return true;
            }
            let since = first_seen_dead
                .get(&s.session_id)
                .copied()
                .unwrap_or(now_ms);
            age(now_ms, since) <= DEAD_RETENTION_MS
        })
        .collect();

    out.sort_by(|a, b| {
        a.state
            .rank()
            .cmp(&b.state.rank())
            .then(b.uptime_ms.cmp(&a.uptime_ms))
            .then(a.pid.cmp(&b.pid))
    });

    SnapshotResult {
        sessions: group_jobs_with_parents(out),
        dead_now,
    }
}
```

- [ ] **Step 4: Update every existing call in the test module**

Every existing test in `state.rs` calls `snapshot(...)` with six arguments and indexes the result directly. Add `, &HashMap::new()` as the seventh argument to each, and replace `snapshot(...)` with `snapshot(...).sessions` wherever the result is indexed, `.len()`-ed or `.is_empty()`-ed.

Two existing tests assert the old retention behaviour and must be rewritten, because it was the bug:

```rust
    #[test]
    fn a_recently_dead_session_is_retained() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);
        let seen = HashMap::from([("session-1".to_string(), NOW - 60_000)]);
        let out = snapshot(&[f], &FakeLiveness::new(), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true, &seen);
        assert_eq!(out.sessions[0].state, SessionState::Dead);
    }

    #[test]
    fn a_long_dead_session_drops_off_the_list() {
        // Claude Code prunes stale registry files itself; claude-buddy never
        // unlinks them, so it stops showing them instead.
        let f = file(1, "cli");
        let seen = HashMap::from([("session-1".to_string(), NOW - DEAD_RETENTION_MS - 1)]);
        assert!(snapshot(&[f], &FakeLiveness::new(), &NoActivity, NOW, PAUSED_THRESHOLD_MS, true, &seen)
            .sessions
            .is_empty());
    }
```

Delete `fn dead_retention_uses_started_at_when_no_status_time_exists` — retention no longer derives from any registry timestamp.

- [ ] **Step 5: Thread the map through the watcher**

In `src-tauri/src/watcher/watch.rs`, add `use std::collections::HashMap;` to the imports. Inside the `spawn_watcher` thread closure, add above `let mut previous`:

```rust
        // Session id to the timestamp of the first tick on which it read as
        // dead. Rebuilt each tick from what is still dead, so it cannot grow.
        let mut first_seen_dead: HashMap<String, i64> = HashMap::new();
```

Replace the body of the `while` loop's derivation block:

```rust
            let settings = crate::config::cached();
            let now = now_ms();
            let result = snapshot(
                &read_registry_dir(&dir),
                liveness.as_ref(),
                activity.as_ref(),
                now,
                settings.paused_threshold_ms,
                settings.show_background_jobs,
                &first_seen_dead,
            );

            first_seen_dead = result
                .dead_now
                .iter()
                .map(|id| {
                    let since = first_seen_dead.get(id).copied().unwrap_or(now);
                    (id.clone(), since)
                })
                .collect();

            let sessions = result.sessions;
```

The rest of the loop is unchanged.

- [ ] **Step 6: Update the watcher-loop tests**

Any test in `watch.rs` calling `snapshot` directly needs the same seventh argument and `.sessions`. Run the suite to find them.

- [ ] **Step 7: Run the Rust suite**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test -- --test-threads=1`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/watcher/state.rs src-tauri/src/watcher/watch.rs
git commit -m "fix: measure dead retention from when death was first observed"
```

---

### Task 3: Move the two blocking commands off the main thread

**Files:**
- Modify: `src-tauri/src/bridge/raise.rs:55`
- Modify: `src-tauri/src/bridge/transcript.rs:126`

**Interfaces:**
- Consumes: nothing.
- Produces: no signature change visible to the frontend — `invoke` is already promise-based.

- [ ] **Step 1: Make `raise_session` async**

In `src-tauri/src/bridge/raise.rs`, replace the command:

```rust
/// Bring the window running a session to the front. Returns the bundle
/// identifier that was activated, for display in the popover.
///
/// `async` deliberately: Tauri runs non-async commands on the main thread, and
/// this one spawns `ps` and then `open` and waits on both. On the main thread
/// that stalls the event loop mid-animation.
#[tauri::command]
pub async fn raise_session(pid: i32) -> Result<String, String> {
    raise(
        &PsProcTree::snapshot(),
        &OpenActivator,
        &|path| bundle_identifier(path),
        pid,
    )
}
```

- [ ] **Step 2: Make `session_detail` async**

In `src-tauri/src/bridge/transcript.rs`, replace the command:

```rust
/// Transcript-only fields for one session.
///
/// Returns an all-`None` detail rather than an error when the transcript is
/// missing or unreadable: the popover must still open and show its
/// registry-sourced fields.
///
/// `async` deliberately: non-async commands run on the main thread, and this
/// opens a file and may scan every project directory. It fires on every hover.
#[tauri::command]
pub async fn session_detail(cwd: String, session_id: String) -> TranscriptDetail {
    find_transcript(&projects_dir(), &cwd, &session_id)
        .and_then(|path| read_tail(&path, TAIL_BYTES).ok())
        .map(|bytes| detail_from_tail(&bytes))
        .unwrap_or_default()
}
```

- [ ] **Step 3: Build and run the suite**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test -- --test-threads=1`
Expected: PASS. Both commands are covered by tests of their inner pure functions, which are unchanged.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/bridge/raise.rs src-tauri/src/bridge/transcript.rs
git commit -m "perf: run the process-walking and transcript commands off the main thread"
```

---

### Task 4: Deliver notifications directly, and raise on click

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/notify.rs`
- Modify: `src-tauri/src/watcher/alerts.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/default.json`
- Test: `src-tauri/src/notify.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `SessionSnapshot` from Task 1.
- Produces: `Alert.pid: i32`. `notify::should_wait_for_click(outstanding: usize) -> bool`.

- [ ] **Step 1: Swap the dependency**

In `src-tauri/Cargo.toml`, remove the `tauri-plugin-notification = "2"` line and add:

```toml
mac-notification-sys = "0.6"
```

- [ ] **Step 2: Write the failing tests**

In `src-tauri/src/watcher/alerts.rs`, add to `mod tests`:

```rust
    #[test]
    fn an_alert_carries_the_pid_so_it_can_be_raised() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Waiting)];
        assert_eq!(diff_alerts(Some(&prev), &next)[0].pid, 1);
    }
```

In `src-tauri/src/notify.rs`, add to `mod tests`:

```rust
    #[test]
    fn a_click_waiter_is_attached_while_there_is_budget() {
        assert!(should_wait_for_click(0));
        assert!(should_wait_for_click(MAX_CLICK_WAITERS - 1));
    }

    #[test]
    fn the_waiter_budget_is_a_hard_cap() {
        // A notification nobody touches parks its thread until macOS resolves
        // it, which may be never. Past the cap, alerts are still delivered —
        // they just cannot be clicked through.
        assert!(!should_wait_for_click(MAX_CLICK_WAITERS));
        assert!(!should_wait_for_click(MAX_CLICK_WAITERS + 1));
    }
```

- [ ] **Step 3: Run to make sure they fail**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test should_wait_for_click -- --test-threads=1`
Expected: FAIL — `cannot find function should_wait_for_click`.

- [ ] **Step 4: Add `pid` to `Alert`**

In `src-tauri/src/watcher/alerts.rs`, add to `struct Alert` after `pub session_id: String,`:

```rust
    /// The session's process, so a clicked notification knows what to raise.
    pub pid: i32,
```

In `diff_alerts`, add `pid: s.pid,` to the `Alert { ... }` literal. In `mod tests`, add `pid: 1,` to the `Alert { ... }` literal inside `fn alert`.

- [ ] **Step 5: Rewrite `notify.rs`**

Replace the whole non-test portion of `src-tauri/src/notify.rs` with:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;

use mac_notification_sys::{Notification, NotificationResponse};
use tauri::Emitter;

use crate::config::{self, Config};
use crate::watcher::alerts::{Alert, AlertKind};
use crate::watcher::watch::now_ms;

/// Emitted when a notification could not be shown, so the widget can flash
/// instead of the alert being lost.
pub const FLASH_EVENT: &str = "ui://flash";

/// How many notifications may be waiting on a click at once.
///
/// Waiting for a click blocks the sending thread until macOS resolves the
/// notification, and a notification the user simply ignores may never resolve.
/// Past this many outstanding waiters, alerts are delivered without waiting:
/// still shown, just not clickable through to a session. Eight unanswered
/// alerts means the user is not reading them anyway.
pub const MAX_CLICK_WAITERS: usize = 8;

static OUTSTANDING: AtomicUsize = AtomicUsize::new(0);
static APPLICATION: Once = Once::new();

/// Whether a new notification can afford to wait for a click.
pub fn should_wait_for_click(outstanding: usize) -> bool {
    outstanding < MAX_CLICK_WAITERS
}

/// Whether this alert reaches the user, given their settings.
pub fn should_deliver(alert: &Alert, config: &Config, now_ms: i64) -> bool {
    if config.alerts_muted(now_ms) {
        return false;
    }
    match alert.kind {
        AlertKind::NeedsInput => config.alert_needs_input,
        AlertKind::Died => config.alert_died,
    }
}

pub fn alert_text(alert: &Alert) -> (String, String) {
    match alert.kind {
        AlertKind::NeedsInput => (
            format!("{} needs you", alert.name),
            alert
                .detail
                .clone()
                .unwrap_or_else(|| "waiting for input".to_string()),
        ),
        AlertKind::Died => (
            format!("{} died", alert.name),
            "the session's process is gone".to_string(),
        ),
    }
}

/// Point the notification centre at this app, once per process.
///
/// Under `tauri dev` the binary is not inside a bundle, so there is no
/// identifier to register; borrowing Terminal's is what the Tauri notification
/// plugin did and it keeps notifications working in development.
fn ensure_application(identifier: &str) {
    APPLICATION.call_once(|| {
        let id = if tauri::is_dev() {
            "com.apple.Terminal"
        } else {
            identifier
        };
        let _ = mac_notification_sys::set_application(id);
    });
}

/// Deliver alerts as native notifications.
///
/// Deliberately not `tauri-plugin-notification`: its desktop path spawns
/// `notify_rust::Notification::show()` and discards the result, so it can
/// neither report a delivery failure nor tell us that the user clicked. Both
/// matter here — the flash fallback depends on the first, and click-to-raise on
/// the second.
///
/// Settings are re-read per batch rather than cached, so toggling an alert or
/// muting takes effect immediately without restarting the watcher.
pub fn deliver(app: &tauri::AppHandle, alerts: &[Alert]) {
    if alerts.is_empty() {
        return;
    }

    let config = config::cached();
    let now = now_ms();
    ensure_application(&app.config().identifier);

    for alert in alerts {
        if !should_deliver(alert, &config, now) {
            continue;
        }
        let (title, body) = alert_text(alert);
        let wait = should_wait_for_click(OUTSTANDING.load(Ordering::Relaxed));
        if wait {
            OUTSTANDING.fetch_add(1, Ordering::Relaxed);
        }

        let handle = app.clone();
        let alert = alert.clone();
        let sound = config.sound;

        // One thread per notification: sending blocks until the user resolves
        // it when we are waiting for a click.
        std::thread::spawn(move || {
            let mut options = Notification::new();
            options.wait_for_click(wait);
            if sound {
                options.default_sound();
            }

            let result = mac_notification_sys::send_notification(&title, None, &body, Some(&options));

            if wait {
                OUTSTANDING.fetch_sub(1, Ordering::Relaxed);
            }

            match result {
                Ok(NotificationResponse::Click) | Ok(NotificationResponse::ActionButton(_)) => {
                    let _ = crate::bridge::raise::raise_pid(alert.pid);
                }
                Ok(_) => {}
                // The usual cause is denied permission, so fall back to
                // flashing the widget — otherwise a user who declined the
                // prompt gets no signal at all.
                Err(_) => {
                    let _ = handle.emit(FLASH_EVENT, &alert);
                }
            }
        });
    }
}
```

- [ ] **Step 6: Expose a non-command raise**

`raise_session` is a Tauri command and cannot be called from a plain thread. In `src-tauri/src/bridge/raise.rs`, add above `raise_session`:

```rust
/// Raise the app hosting `pid`, callable outside the command layer.
///
/// The notification waiter runs on its own thread with no `AppHandle`
/// available, so it needs a plain function rather than the command.
pub fn raise_pid(pid: i32) -> Result<String, String> {
    raise(
        &PsProcTree::snapshot(),
        &OpenActivator,
        &|path| bundle_identifier(path),
        pid,
    )
}
```

and make the command delegate to it:

```rust
#[tauri::command]
pub async fn raise_session(pid: i32) -> Result<String, String> {
    raise_pid(pid)
}
```

- [ ] **Step 7: Drop the plugin from the app**

In `src-tauri/src/lib.rs`, delete the line `.plugin(tauri_plugin_notification::init())`.

In `src-tauri/capabilities/default.json`, remove `"notification:default"` from `permissions`.

- [ ] **Step 8: Run the Rust suite**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test -- --test-threads=1`
Expected: PASS.

- [ ] **Step 9: Verify the app still builds and launches**

Run: `npm run tauri build 2>&1 | tail -5`
Expected: a bundled `claude-buddy.app`. Launch it and confirm the widget appears.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/notify.rs src-tauri/src/watcher/alerts.rs src-tauri/src/bridge/raise.rs src-tauri/src/lib.rs src-tauri/capabilities/default.json
git commit -m "feat: raise a session by clicking its notification"
```

---

### Task 5: Activity detail in the popover

**Files:**
- Modify: `src-tauri/src/bridge/transcript.rs`
- Modify: `src/types.ts`
- Modify: `src/views/dotRow/SessionPopover.tsx`
- Modify: `src/views/dotRow/dotRow.css`
- Test: `src-tauri/src/bridge/transcript.rs` (inline `mod tests`)
- Test: `src/views/dotRow/SessionPopover.test.tsx`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `transcript::latest_activity(bytes: &[u8]) -> Option<String>`, `transcript::latest_assistant_text(bytes: &[u8]) -> Option<String>` (used by Task 6), and `TranscriptDetail.activity: Option<String>`.

- [ ] **Step 1: Write the failing Rust tests**

Add to `mod tests` in `src-tauri/src/bridge/transcript.rs`:

```rust
    const TOOL_TAIL: &str = concat!(
        r#"{"type":"assistant","message":{"model":"claude-opus-5","content":[{"type":"text","text":"Let me check the config."}]}}"#,
        "\n",
        r#"{"type":"assistant","message":{"model":"claude-opus-5","content":[{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#,
        "\n",
    );

    #[test]
    fn latest_activity_reports_the_newest_tool_use() {
        assert_eq!(latest_activity(TOOL_TAIL.as_bytes()).as_deref(), Some("Bash"));
    }

    #[test]
    fn latest_activity_falls_back_to_assistant_text() {
        let tail = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Shall I delete the branch?"}]}}"#;
        assert_eq!(
            latest_activity(tail.as_bytes()).as_deref(),
            Some("Shall I delete the branch?")
        );
    }

    #[test]
    fn latest_activity_truncates_a_long_line() {
        let long = "x".repeat(400);
        let tail = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{long}"}}]}}}}"#
        );
        let out = latest_activity(tail.as_bytes()).unwrap();
        assert!(out.len() <= ACTIVITY_MAX_CHARS + 1, "got {} chars", out.len());
        assert!(out.ends_with('\u{2026}'));
    }

    #[test]
    fn latest_activity_skips_a_truncated_leading_line() {
        let tail = format!("{}\n{}", r#"{"type":"assis"#, TOOL_TAIL.trim_end());
        assert_eq!(latest_activity(tail.as_bytes()).as_deref(), Some("Bash"));
    }

    #[test]
    fn latest_activity_reports_nothing_for_a_transcript_with_neither() {
        let tail = r#"{"type":"user","message":{"role":"user"}}"#;
        assert_eq!(latest_activity(tail.as_bytes()), None);
    }

    #[test]
    fn latest_assistant_text_ignores_tool_uses() {
        assert_eq!(
            latest_assistant_text(TOOL_TAIL.as_bytes()).as_deref(),
            Some("Let me check the config.")
        );
    }

    #[test]
    fn detail_from_tail_includes_activity() {
        assert_eq!(
            detail_from_tail(TOOL_TAIL.as_bytes()).activity.as_deref(),
            Some("Bash")
        );
    }
```

- [ ] **Step 2: Run to make sure they fail**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test latest_activity -- --test-threads=1`
Expected: FAIL — `cannot find function latest_activity`.

- [ ] **Step 3: Implement the scanners**

In `src-tauri/src/bridge/transcript.rs`, add `pub activity: Option<String>,` to `struct TranscriptDetail`, and add below `TAIL_BYTES`:

```rust
/// Longest activity string the popover will show on one line.
pub const ACTIVITY_MAX_CHARS: usize = 64;

/// Shorten to fit, on a character boundary, with an ellipsis.
fn clip(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= ACTIVITY_MAX_CHARS {
        return text;
    }
    let head: String = text.chars().take(ACTIVITY_MAX_CHARS).collect();
    format!("{head}\u{2026}")
}

/// The content blocks of an assistant record, if this is one.
fn assistant_content(record: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    record
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
}

/// What the session is doing, newest first: the most recent tool use by name,
/// or failing that the most recent thing the assistant said.
///
/// Records are scanned in reverse for the same reason `detail_from_tail` scans
/// in reverse — the tail begins mid-record, and the newest information is at
/// the end.
pub fn latest_activity(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut fallback: Option<String> = None;

    for line in text.lines().rev() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(content) = assistant_content(&record) else {
            continue;
        };

        for block in content.iter().rev() {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("tool_use") => {
                    if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                        return Some(clip(name));
                    }
                }
                Some("text") => {
                    if fallback.is_none() {
                        if let Some(said) = block.get("text").and_then(|t| t.as_str()) {
                            if !said.trim().is_empty() {
                                fallback = Some(clip(said));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fallback
}

/// The most recent thing the assistant actually said, ignoring tool uses.
///
/// This is what a waiting session is asking. `latest_activity` prefers the tool
/// name because "Bash" describes what is happening; a pending question is the
/// opposite case, where the prose is the whole point.
pub fn latest_assistant_text(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);

    for line in text.lines().rev() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(content) = assistant_content(&record) else {
            continue;
        };
        for block in content.iter().rev() {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(said) = block.get("text").and_then(|t| t.as_str()) {
                    if !said.trim().is_empty() {
                        return Some(clip(said));
                    }
                }
            }
        }
    }

    None
}
```

In `detail_from_tail`, set the new field before returning. Change the final line from `detail` to:

```rust
    detail.activity = latest_activity(bytes);
    detail
```

and remove `activity` from the `complete()` check by leaving `complete()` as it is — it gates the early break for the three registry-ish fields only, and `latest_activity` does its own pass.

- [ ] **Step 4: Run the Rust suite**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test -- --test-threads=1`
Expected: PASS.

- [ ] **Step 5: Write the failing frontend test**

Add to `src/views/dotRow/SessionPopover.test.tsx`:

```tsx
it('shows the transcript activity line', async () => {
  mockInvoke({ branch: 'main', model: 'claude-opus-5', effort: 'high', activity: 'Bash' })
  render(<SessionPopover session={baseSession} />)
  expect(await screen.findByTestId('popover-activity')).toHaveTextContent('Bash')
})

it('dashes the activity line when the transcript has nothing', async () => {
  mockInvoke({ branch: null, model: null, effort: null, activity: null })
  render(<SessionPopover session={baseSession} />)
  expect(await screen.findByTestId('popover-activity')).toHaveTextContent('—')
})
```

Use whatever `invoke` mocking helper the file already has; if it mocks `@tauri-apps/api/core` inline, follow that pattern rather than adding `mockInvoke`.

- [ ] **Step 6: Run to make sure it fails**

Run: `npm test -- SessionPopover`
Expected: FAIL — no element with `data-testid="popover-activity"`.

- [ ] **Step 7: Render the activity line**

In `src/types.ts`, add to `interface TranscriptDetail`:

```ts
  /** What the session is doing: the newest tool use, or what it last said. */
  activity: string | null
```

In `src/views/dotRow/SessionPopover.tsx`, change `EMPTY` to `{ branch: null, model: null, effort: null, activity: null }`, and add a field to the `<dl>` immediately after the `state` row:

```tsx
        <dt>doing</dt>
        <dd className="popover-activity" data-testid="popover-activity">
          {dash(detail.activity)}
        </dd>
```

In `src/views/dotRow/dotRow.css`, add beside the other popover rules:

```css
/* One line only: the popover's width is fixed and reserved in the window size,
   so a wrapping activity string would change the height the row was sized for. */
.popover-activity {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
```

- [ ] **Step 8: Run the frontend suite**

Run: `npm test`
Expected: PASS.

- [ ] **Step 9: Check it visually**

Run: `npm run tauri build 2>&1 | tail -3 && open src-tauri/target/release/bundle/macos/claude-buddy.app`
Hover a session and confirm the popover shows a `doing` line. Confirm the popover has not changed width and the row has not shifted.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/bridge/transcript.rs src/types.ts src/views/dotRow/SessionPopover.tsx src/views/dotRow/SessionPopover.test.tsx src/views/dotRow/dotRow.css
git commit -m "feat: show what a session is doing in its popover"
```

---

### Task 6: Put the real question in a waiting notification

**Files:**
- Create: `src-tauri/src/watcher/question.rs`
- Modify: `src-tauri/src/watcher/mod.rs`
- Modify: `src-tauri/src/watcher/watch.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/watcher/question.rs` (inline `mod tests`)
- Test: `src-tauri/src/watcher/watch.rs` (inline `mod tests`)

**Interfaces:**
- Consumes: `transcript::latest_assistant_text` from Task 5; `Alert.pid` from Task 4; `SnapshotResult` from Task 2.
- Produces: `watcher::question::QuestionProbe` trait with `fn pending_question(&self, cwd: &str, session_id: &str) -> Option<String>`, plus `TranscriptQuestion`, `NoQuestion` and `FakeQuestion` implementations. `spawn_watcher` gains a fourth parameter of type `Arc<dyn QuestionProbe + Send + Sync>` before the callback.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/watcher/question.rs`:

```rust
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
        use crate::bridge::transcript::{find_transcript, latest_assistant_text, read_tail, TAIL_BYTES};

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
        Self { answers: HashMap::new() }
    }

    pub fn with(mut self, session_id: &str, question: &str) -> Self {
        self.answers.insert(session_id.to_string(), question.to_string());
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
    alerts: &mut [crate::watcher::alerts::Alert],
    sessions: &[crate::watcher::state::SessionSnapshot],
    probe: &dyn QuestionProbe,
) {
    use crate::watcher::alerts::AlertKind;

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
    use crate::watcher::alerts::{Alert, AlertKind};
    use crate::watcher::state::{SessionSnapshot, SessionState};

    fn session(id: &str) -> SessionSnapshot {
        SessionSnapshot {
            pid: 7,
            session_id: id.to_string(),
            name: format!("name-{id}"),
            cwd: "/Users/n/Code/x".into(),
            entrypoint: "cli".into(),
            state: SessionState::Waiting,
            detail: Some("input needed".into()),
            elapsed_ms: 0,
            uptime_ms: 0,
            status_time_ms: 0,
            started_at_ms: 0,
            background: false,
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

        assert_eq!(alerts[0].detail.as_deref(), Some("Shall I delete the branch?"));
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
```

- [ ] **Step 2: Register the module**

In `src-tauri/src/watcher/mod.rs`, add `pub mod question;` alongside the existing module declarations.

- [ ] **Step 3: Run to make sure it fails, then passes**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test question:: -- --test-threads=1`
Expected: PASS once the module compiles. If `AlertKind` does not derive `PartialEq` for the `!=` comparison, it already does — confirm before adding anything.

- [ ] **Step 4: Call it from the watcher**

In `src-tauri/src/watcher/watch.rs`, add `use crate::watcher::question::QuestionProbe;` to the imports, add a parameter to `spawn_watcher` after `activity`:

```rust
    question: Arc<dyn QuestionProbe + Send + Sync>,
```

and inside the `if changed` block, replace the two lines with:

```rust
            if changed {
                let mut alerts = diff_alerts(previous.as_deref(), &sessions);
                crate::watcher::question::enrich_alerts(&mut alerts, &sessions, question.as_ref());
                on_update(Update { sessions: sessions.clone(), alerts });
                previous = Some(sessions);
            }
```

- [ ] **Step 5: Update the watcher-loop tests and the app wiring**

In `src-tauri/src/watcher/watch.rs`'s `mod tests`, add `use crate::watcher::question::NoQuestion;` and pass `Arc::new(NoQuestion)` as the new argument to every `spawn_watcher` call.

In `src-tauri/src/lib.rs`, pass the real probe:

```rust
                Arc::new(crate::watcher::question::TranscriptQuestion::new(
                    crate::bridge::transcript::projects_dir(),
                )),
```

immediately after the `TranscriptActivity` argument.

- [ ] **Step 6: Add a watcher-loop test**

Add to `mod tests` in `src-tauri/src/watcher/watch.rs`, following the shape of the existing loop tests:

```rust
    #[test]
    fn a_needs_input_alert_carries_the_transcript_question() {
        use crate::watcher::question::FakeQuestion;

        let dir = TempDir::new("question");
        let (tx, rx) = mpsc::channel::<Update>();

        // Start busy so the first snapshot is a baseline, then flip to waiting.
        write_session(&dir.0, 1, "busy");
        let watcher = spawn_watcher(
            dir.0.clone(),
            Arc::new(FakeLiveness::new().with_alive_any_start(1)),
            Arc::new(NoActivity),
            Arc::new(FakeQuestion::new().with("session-1", "Shall I delete the branch?")),
            move |update| {
                let _ = tx.send(update);
            },
        );

        let _baseline = rx.recv_timeout(WAIT).expect("baseline snapshot");
        write_session(&dir.0, 1, "waiting");

        let update = rx.recv_timeout(WAIT).expect("waiting snapshot");
        watcher.stop();

        assert_eq!(update.alerts.len(), 1);
        assert_eq!(
            update.alerts[0].detail.as_deref(),
            Some("Shall I delete the branch?")
        );
    }
```

Use whatever session-writing helper the existing tests use; if it is named differently from `write_session`, follow the existing name and signature exactly.

- [ ] **Step 7: Run the Rust suite**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test -- --test-threads=1`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/watcher/question.rs src-tauri/src/watcher/mod.rs src-tauri/src/watcher/watch.rs src-tauri/src/lib.rs
git commit -m "feat: put the pending question in a needs-input notification"
```

---

### Task 7: Opt-in turn-finished alert

**Files:**
- Modify: `src-tauri/src/watcher/alerts.rs`
- Modify: `src-tauri/src/notify.rs`
- Modify: `src-tauri/src/config.rs`
- Modify: `src/types.ts`
- Modify: `src/settings/SettingsPanel.tsx`
- Test: `src-tauri/src/watcher/alerts.rs`, `src-tauri/src/notify.rs`, `src-tauri/src/config.rs` (inline), `src/settings/SettingsPanel.test.tsx`

**Interfaces:**
- Consumes: `Alert.pid` from Task 4.
- Produces: `AlertKind::Finished`, `Config.alert_finished: bool` serialized as `alertFinished`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src-tauri/src/watcher/alerts.rs`:

```rust
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
```

Add to `mod tests` in `src-tauri/src/notify.rs`:

```rust
    #[test]
    fn finished_is_off_by_default() {
        let mut a = alert(AlertKind::NeedsInput);
        a.kind = AlertKind::Finished;
        assert!(!should_deliver(&a, &Config::default(), 0));
    }

    #[test]
    fn enabling_finished_delivers_it() {
        let mut config = Config::default();
        config.alert_finished = true;
        let mut a = alert(AlertKind::NeedsInput);
        a.kind = AlertKind::Finished;
        assert!(should_deliver(&a, &config, 0));
    }

    #[test]
    fn finished_text_says_the_turn_is_done() {
        let mut a = alert(AlertKind::NeedsInput);
        a.kind = AlertKind::Finished;
        a.detail = None;
        let (title, body) = alert_text(&a);
        assert_eq!(title, "api-service-55 finished");
        assert_eq!(body, "the session is idle again");
    }
```

Add to `mod tests` in `src-tauri/src/config.rs`, inside `fn defaults_are_sane`:

```rust
        assert!(!c.alert_finished);
```

- [ ] **Step 2: Run to make sure they fail**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test finishing_a_turn -- --test-threads=1`
Expected: FAIL — no variant `Finished`.

- [ ] **Step 3: Make the alert kind a function of the transition**

In `src-tauri/src/watcher/alerts.rs`, add `Finished,` to `enum AlertKind`, and replace `fn alert_kind` and the body of `diff_alerts`:

```rust
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
                name: s.name.clone(),
                kind,
                detail: s.detail.clone(),
            })
        })
        .collect()
}
```

- [ ] **Step 4: Add the setting and the copy**

In `src-tauri/src/config.rs`, add to `struct Config` after `pub alert_died: bool,`:

```rust
    /// Whether finishing a turn interrupts you. Off by default: a finished turn
    /// is the common case, and alerting on it is the noisy choice.
    pub alert_finished: bool,
```

and `alert_finished: false,` to `Default`.

In `src-tauri/src/notify.rs`, add to `should_deliver`'s match:

```rust
        AlertKind::Finished => config.alert_finished,
```

and to `alert_text`'s match:

```rust
        AlertKind::Finished => (
            format!("{} finished", alert.name),
            "the session is idle again".to_string(),
        ),
```

- [ ] **Step 5: Run the Rust suite**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test -- --test-threads=1`
Expected: PASS.

- [ ] **Step 6: Write the failing settings test**

Add to `src/settings/SettingsPanel.test.tsx`, following the file's existing render and mock helpers:

```tsx
it('toggles the finished alert', async () => {
  const user = userEvent.setup()
  render(<SettingsPanel onClose={() => {}} />)

  const box = await screen.findByLabelText('Alert when a session finishes its turn')
  expect(box).not.toBeChecked()
  await user.click(box)

  expect(setConfigCalls().at(-1)).toMatchObject({ alertFinished: true })
})
```

Use the file's existing way of asserting on `set_config` invocations rather than inventing `setConfigCalls` if one already exists.

- [ ] **Step 7: Run to make sure it fails**

Run: `npm test -- SettingsPanel`
Expected: FAIL — no such label.

- [ ] **Step 8: Add the checkbox**

In `src/types.ts`, add `alertFinished: boolean` to `interface AppConfig` after `alertDied`.

In `src/settings/SettingsPanel.tsx`, add after the "Alert when a session dies" label block:

```tsx
      <label>
        <input
          type="checkbox"
          checked={config.alertFinished}
          onChange={(e) => update({ alertFinished: e.target.checked })}
        />
        Alert when a session finishes its turn
      </label>
```

- [ ] **Step 9: Run the frontend suite**

Run: `npm test`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/watcher/alerts.rs src-tauri/src/notify.rs src-tauri/src/config.rs src/types.ts src/settings/SettingsPanel.tsx src/settings/SettingsPanel.test.tsx
git commit -m "feat: optional alert when a session finishes its turn"
```

---

### Task 8: Shape per state

**Files:**
- Modify: `src/views/dotRow/dotRow.css`
- Test: `src/views/dotRow/NamedDotRow.test.tsx`

**Interfaces:**
- Consumes: nothing.
- Produces: nothing consumed by later tasks. The `dot-<state>` class names are unchanged.

- [ ] **Step 1: Write the failing test**

Add to `src/views/dotRow/NamedDotRow.test.tsx`:

```tsx
it('gives every state its own dot class so shape can differ, not just hue', () => {
  const states = ['waiting', 'busy', 'idle', 'paused', 'dead'] as const
  const sessions = states.map((state, i) => ({
    ...baseSession,
    sessionId: `s-${i}`,
    name: `project-${i}`,
    state,
  }))

  const { container } = render(
    <NamedDotRow sessions={sessions} hoveredSessionId={null} onHoverSession={() => {}} />,
  )

  for (const state of states) {
    expect(container.querySelector(`.dot-${state}`)).not.toBeNull()
  }
})
```

Reuse the file's existing session fixture helper instead of `baseSession` if it has one.

- [ ] **Step 2: Run it**

Run: `npm test -- NamedDotRow`
Expected: PASS — the classes already exist. This test is a guard so the CSS work below has something asserting the contract it hangs off.

- [ ] **Step 3: Give each state a silhouette**

In `src/views/dotRow/dotRow.css`, replace the five `.dot-*` colour rules with:

```css
/* State is carried by shape as well as hue. Colour alone is unreadable to a
   red-green colourblind user, and these five dots are the widget's entire
   vocabulary. The box stays 11px in every case, so pill metrics do not move. */
.dot {
  box-sizing: border-box;
}

/* Waiting: a triangle, the one shape that reads as "attend to me". */
.dot-waiting {
  background: var(--waiting);
  border-radius: 2px;
  clip-path: polygon(50% 0%, 100% 100%, 0% 100%);
  box-shadow: none;
}

/* Busy: solid. */
.dot-busy {
  background: var(--busy);
  box-shadow: 0 0 0 4px rgba(63, 185, 80, 0.2);
}

/* Idle: hollow. Present, doing nothing. */
.dot-idle {
  background: transparent;
  border: 2px solid var(--idle);
}

/* Paused: hollow and broken, so it reads as fainter than idle at a glance. */
.dot-paused {
  background: transparent;
  border: 2px dashed var(--paused);
}

/* Dead: a cross drawn from two bars, since there is no border style for it. */
.dot-dead {
  background: transparent;
  box-shadow: none;
}

.dot-dead::before,
.dot-dead::after {
  content: '';
  position: absolute;
  top: 50%;
  left: 0;
  width: 100%;
  height: 2px;
  margin-top: -1px;
  background: var(--dead);
  border-radius: 1px;
}

.dot-dead::before {
  transform: rotate(45deg);
}

.dot-dead::after {
  transform: rotate(-45deg);
}
```

The `.dot-waiting::after` ring rule further down the file stays as it is — a circular pulse around a triangle still reads as a pulse. If `clip-path` on `.dot-waiting` clips the ring, move the ring to the parent `.entry` instead by changing its selector to `.entry[data-state='waiting'] .dot::after` and confirming visually.

- [ ] **Step 4: Run the frontend suite**

Run: `npm test`
Expected: PASS.

- [ ] **Step 5: Check it visually — this is the point of the task**

Run: `npm run tauri build 2>&1 | tail -3 && open src-tauri/target/release/bundle/macos/claude-buddy.app`

Drive it from fixtures so all five states are on screen at once:

```bash
CLAUDE_BUDDY_REGISTRY_DIR=/path/to/fixture/sessions \
CLAUDE_BUDDY_PROJECTS_DIR=/path/to/fixture/projects \
  src-tauri/target/release/bundle/macos/claude-buddy.app/Contents/MacOS/claude-buddy
```

Confirm: five distinguishable silhouettes; the pill width has not changed; the waiting ring still animates; the demoted background-job dots still render smaller without losing their shape.

- [ ] **Step 6: Commit**

```bash
git add src/views/dotRow/dotRow.css src/views/dotRow/NamedDotRow.test.tsx
git commit -m "feat: distinguish session state by shape, not only colour"
```

---

### Task 9: Hide when quiet, and remove the view modes

**Files:**
- Create: `src-tauri/src/visibility.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/window.rs`
- Modify: `src/types.ts`
- Modify: `src/settings/SettingsPanel.tsx`
- Test: `src-tauri/src/visibility.rs` (inline), `src-tauri/src/commands.rs` (inline), `src/settings/SettingsPanel.test.tsx`

**Interfaces:**
- Consumes: `SessionSnapshot` from Task 1.
- Produces: `visibility::should_hide(sessions: &[SessionSnapshot], hide_when: &str) -> bool`, `visibility::HIDE_MODES: [&str; 3]`, `Config.hide_when: String` serialized as `hideWhen`.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/visibility.rs`:

```rust
use crate::watcher::state::{SessionSnapshot, SessionState};

/// Accepted values for the `hideWhen` setting.
pub const HIDE_MODES: [&str; 3] = ["never", "noSessions", "nothingActive"];

/// Whether the widget should be off screen right now.
///
/// Pure, so the policy is tested without a window server. The caller owns the
/// panel; this only decides.
pub fn should_hide(sessions: &[SessionSnapshot], hide_when: &str) -> bool {
    match hide_when {
        "noSessions" => sessions.is_empty(),
        "nothingActive" => !sessions.iter().any(|s| {
            matches!(s.state, SessionState::Waiting | SessionState::Busy)
        }),
        // "never", and anything unrecognised: showing is the safe failure.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(state: SessionState) -> SessionSnapshot {
        SessionSnapshot {
            pid: 1,
            session_id: "a".into(),
            name: "api-service".into(),
            cwd: "/Users/n/Code/api-service".into(),
            entrypoint: "cli".into(),
            state,
            detail: None,
            elapsed_ms: 0,
            uptime_ms: 0,
            status_time_ms: 0,
            started_at_ms: 0,
            background: false,
        }
    }

    #[test]
    fn never_always_shows() {
        assert!(!should_hide(&[], "never"));
        assert!(!should_hide(&[session(SessionState::Idle)], "never"));
        assert!(!should_hide(&[session(SessionState::Waiting)], "never"));
    }

    #[test]
    fn no_sessions_hides_only_an_empty_list() {
        assert!(should_hide(&[], "noSessions"));
        assert!(!should_hide(&[session(SessionState::Idle)], "noSessions"));
        assert!(!should_hide(&[session(SessionState::Paused)], "noSessions"));
    }

    #[test]
    fn nothing_active_hides_a_quiet_list() {
        assert!(should_hide(&[], "nothingActive"));
        assert!(should_hide(&[session(SessionState::Idle)], "nothingActive"));
        assert!(should_hide(&[session(SessionState::Paused)], "nothingActive"));
        assert!(should_hide(&[session(SessionState::Dead)], "nothingActive"));
    }

    #[test]
    fn nothing_active_shows_for_waiting_or_busy() {
        assert!(!should_hide(&[session(SessionState::Waiting)], "nothingActive"));
        assert!(!should_hide(&[session(SessionState::Busy)], "nothingActive"));
    }

    #[test]
    fn an_unrecognised_mode_shows_rather_than_hiding() {
        // A hand-edited config must not be able to make the widget vanish with
        // no way to reason about why.
        assert!(!should_hide(&[], "hologram"));
    }
}
```

In `src-tauri/src/lib.rs`, add `pub mod visibility;` to the module list.

- [ ] **Step 2: Run it**

Run: `cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test visibility:: -- --test-threads=1`
Expected: PASS.

- [ ] **Step 3: Add the setting**

In `src-tauri/src/config.rs`, add to `struct Config` after `pub show_background_jobs: bool,`:

```rust
    /// When the widget takes itself off screen: `never`, `noSessions` or
    /// `nothingActive`. The tray icon always remains, so a hidden widget is
    /// never unreachable.
    pub hide_when: String,
```

and `hide_when: "noSessions".into(),` to `Default`. Add to `fn defaults_are_sane`:

```rust
        assert_eq!(c.hide_when, "noSessions");
```

- [ ] **Step 4: Replace view-mode validation with hide-mode validation**

In `src-tauri/src/commands.rs`, delete `pub const VIEW_MODES` and rewrite `validate`:

```rust
/// Reject settings that would break the widget rather than writing them.
/// A zero paused threshold would mark every session paused instantly.
pub fn validate(config: &Config) -> Result<(), String> {
    if config.paused_threshold_ms <= 0 {
        return Err("paused threshold must be greater than zero".into());
    }
    if !crate::visibility::HIDE_MODES.contains(&config.hide_when.as_str()) {
        return Err(format!("unknown hide mode: {}", config.hide_when));
    }
    Ok(())
}
```

Replace the two view-mode tests in `mod tests` with:

```rust
    #[test]
    fn rejects_an_unknown_hide_mode() {
        let mut config = Config::default();
        config.hide_when = "sometimes".into();
        assert!(validate(&config).is_err());
    }

    #[test]
    fn accepts_every_hide_mode() {
        for mode in crate::visibility::HIDE_MODES {
            let mut config = Config::default();
            config.hide_when = mode.into();
            assert!(validate(&config).is_ok(), "{mode} should be valid");
        }
    }
```

- [ ] **Step 5: Apply visibility from the watcher callback**

In `src-tauri/src/window.rs`, add:

```rust
/// Put the widget on or off screen.
///
/// `order_out` rather than closing: the panel keeps its configuration, its
/// level and its collection behaviour, so coming back is a single call rather
/// than a rebuild. Re-showing restores the saved position, because a widget
/// that reappears somewhere else reads as a bug.
pub fn set_widget_visible(app: &AppHandle, visible: bool) {
    let Ok(panel) = app.get_webview_panel("widget") else {
        return;
    };
    if visible {
        if !panel.is_visible() {
            panel.show();
            if let Some(widget) = app.get_webview_window("widget") {
                restore_position(&widget);
            }
        }
    } else if panel.is_visible() {
        panel.order_out(None);
    }
}
```

In `src-tauri/src/lib.rs`, inside the `spawn_watcher` callback, after the store is set and before the emit:

```rust
                    let hide = crate::visibility::should_hide(
                        &update.sessions,
                        &crate::config::cached().hide_when,
                    );
                    let visibility_handle = handle.clone();
                    // Panel calls must run on the main thread; the watcher is
                    // its own thread.
                    let _ = handle.run_on_main_thread(move || {
                        crate::window::set_widget_visible(&visibility_handle, !hide);
                    });
```

- [ ] **Step 6: Remove the view-mode menu**

In `src-tauri/src/window.rs`, in `build_tray_menu`, delete the five `MenuItem` bindings for the view modes and the `views` submenu, remove `&views` and its adjacent separator from `Menu::with_items`, and delete the `id if id.starts_with("view:")` arm from `on_menu_event`.

- [ ] **Step 7: Write the failing settings test**

Add to `src/settings/SettingsPanel.test.tsx`:

```tsx
it('has no view mode control', async () => {
  render(<SettingsPanel onClose={() => {}} />)
  await screen.findByTestId('settings')
  expect(screen.queryByLabelText('View mode')).toBeNull()
})

it('changes when the widget hides', async () => {
  const user = userEvent.setup()
  render(<SettingsPanel onClose={() => {}} />)

  const select = await screen.findByLabelText('Hide the widget')
  await user.selectOptions(select, 'nothingActive')

  expect(setConfigCalls().at(-1)).toMatchObject({ hideWhen: 'nothingActive' })
})
```

- [ ] **Step 8: Run to make sure it fails**

Run: `npm test -- SettingsPanel`
Expected: FAIL — the view-mode label still exists and there is no hide control.

- [ ] **Step 9: Swap the control**

In `src/types.ts`, delete `VIEW_MODES`, remove `viewMode` from `AppConfig`, and add:

```ts
  hideWhen: string
```

plus:

```ts
/** Mirrors visibility::HIDE_MODES in Rust. */
export const HIDE_MODES = [
  { id: 'never', label: 'Never' },
  { id: 'noSessions', label: 'When there are no sessions' },
  { id: 'nothingActive', label: 'When nothing is waiting or working' },
] as const
```

In `src/settings/SettingsPanel.tsx`, replace the view-mode `<label>`/`<select>` pair with:

```tsx
      <label htmlFor="hide-when">Hide the widget</label>
      <select
        id="hide-when"
        value={config.hideWhen}
        onChange={(e) => update({ hideWhen: e.target.value })}
      >
        {HIDE_MODES.map((mode) => (
          <option key={mode.id} value={mode.id}>
            {mode.label}
          </option>
        ))}
      </select>
```

and update the import from `../types` to bring in `HIDE_MODES` instead of `VIEW_MODES`.

- [ ] **Step 10: Run both suites**

Run: `npm test && cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test -- --test-threads=1`
Expected: PASS.

- [ ] **Step 11: Check it visually**

Run: `npm run tauri build 2>&1 | tail -3 && open src-tauri/target/release/bundle/macos/claude-buddy.app`

With no Claude Code sessions running, confirm the widget is absent and the tray icon is present. Start a session and confirm it reappears at its saved position. Open Settings and confirm the view-mode dropdown is gone and the tray menu has no View mode submenu.

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src/visibility.rs src-tauri/src/lib.rs src-tauri/src/config.rs src-tauri/src/commands.rs src-tauri/src/window.rs src/types.ts src/settings/SettingsPanel.tsx src/settings/SettingsPanel.test.tsx
git commit -m "feat: hide the widget when there is nothing to report"
```

---

### Task 10: Auto-update

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `package.json`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/update.rs`
- Modify: `src-tauri/src/window.rs`
- Modify: the release workflow
- Modify: `README.md`

**Interfaces:**
- Consumes: `window::build_tray_menu` from Task 9's edits.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Add the plugin**

In `src-tauri/Cargo.toml`:

```toml
tauri-plugin-updater = "2"
```

In `package.json` dependencies:

```json
    "@tauri-apps/plugin-updater": "^2.0.0"
```

Run: `npm install`

- [ ] **Step 2: Configure the bundle and the endpoint**

In `src-tauri/tauri.conf.json`, add `"createUpdaterArtifacts": true` inside `bundle`, and add a `plugins` block at the top level:

```json
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/norbertsuski/claude-buddy/releases/latest/download/latest.json"
      ],
      "pubkey": ""
    }
  }
```

The endpoint is the `latest.json` asset attached to the newest release, so it is a fixed URL and needs no editing once the owner and repository are right. Leave `pubkey` empty until the keypair exists — the check then fails closed rather than accepting an unverified update.

In `src-tauri/capabilities/default.json`, add `"updater:default"` to `permissions`.

- [ ] **Step 3: Check for an update on launch**

Create `src-tauri/src/update.rs`:

```rust
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

/// Whether an update was found, exposed so the tray item can appear.
pub static AVAILABLE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Check once, in the background, and tell the user if there is something newer.
///
/// Deliberately not automatic: replacing a running menu-bar app under the user
/// without asking is the kind of surprise this widget exists to avoid. The
/// check only sets a flag and notifies; installing is a menu item.
pub fn check_on_launch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Ok(updater) = app.updater() else { return };
        match updater.check().await {
            Ok(Some(update)) => {
                AVAILABLE.store(true, std::sync::atomic::Ordering::Relaxed);
                let version = update.version.clone();
                let _ = std::thread::spawn(move || {
                    let mut options = mac_notification_sys::Notification::new();
                    options.wait_for_click(false);
                    let _ = mac_notification_sys::send_notification(
                        "claude-buddy update available",
                        None,
                        &format!("version {version} — install it from the tray menu"),
                        Some(&options),
                    );
                });
            }
            // No update, or no reachable manifest. Neither is worth surfacing:
            // a widget that nags about its own update server is worse than one
            // that quietly stays on the version you installed.
            _ => {}
        }
    });
}

/// Download and install, then restart.
pub fn install(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Ok(updater) = app.updater() else { return };
        if let Ok(Some(update)) = updater.check().await {
            if update.download_and_install(|_, _| {}, || {}).await.is_ok() {
                app.restart();
            }
        }
    });
}
```

Add `pub mod update;` to `src-tauri/src/lib.rs`, and call it at the end of `setup`, just before `Ok(())`:

```rust
            crate::update::check_on_launch(app.handle().clone());
```

- [ ] **Step 4: Add the tray item**

In `src-tauri/src/window.rs`, in `build_tray_menu`, add above `quit`:

```rust
    let update = MenuItem::with_id(app, "update", "Install update", true, None::<&str>)?;
```

Include `&update` in `Menu::with_items` between the mute item and the separator, and add to `on_menu_event`:

```rust
            "update" => crate::update::install(app.clone()),
```

- [ ] **Step 5: Publish the manifest from CI**

In the release workflow, after the DMG upload, attach the updater's own
artefacts to the same release:

```yaml
      # The updater consumes a tarball and its signature, not the DMG.
      - name: Publish the update manifest
        run: |
          BUNDLE=src-tauri/target/release/bundle/macos
          TARBALL=$(ls "$BUNDLE"/*.app.tar.gz)
          SIG=$(cat "$BUNDLE"/*.app.tar.gz.sig)
          TAG="$GITHUB_REF_NAME"
          REPO="$GITHUB_REPOSITORY"
          cat > latest.json <<EOF
          {
            "version": "${TAG#v}",
            "notes": "claude-buddy $TAG",
            "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
            "platforms": {
              "darwin-aarch64": {
                "signature": "$SIG",
                "url": "https://github.com/$REPO/releases/download/$TAG/$(basename "$TARBALL")"
              }
            }
          }
          EOF
          # Attached to the release itself, which is what makes
          # /releases/latest/download/latest.json a stable endpoint.
          gh release upload "$TAG" "$TARBALL" "$TARBALL.sig" latest.json
```

- [ ] **Step 6: Document the signing step**

In `README.md`, under `## Releasing`, add:

```markdown
### Signing updates

The updater refuses anything it cannot verify, so a release needs a minisign
keypair. This is separate from Apple code signing — it secures the update
channel, not Gatekeeper.

```bash
npm run tauri signer generate -- -w ~/.tauri/claude-buddy.key
```

Put the printed public key in `src-tauri/tauri.conf.json` under
`plugins.updater.pubkey`, and add the private key and its password to the
repository under *Settings → Secrets and variables → Actions*, as secrets:

- `TAURI_SIGNING_PRIVATE_KEY` — the contents of `~/.tauri/claude-buddy.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the password you chose

Until the public key is set the updater is inert: the check fails closed and
the app stays on the installed version. Point `plugins.updater.endpoints` at
your own repository if you are running a fork.
```

- [ ] **Step 7: Build and confirm the artifacts exist**

Run: `npm run tauri build 2>&1 | tail -5 && ls src-tauri/target/release/bundle/macos/`
Expected: `claude-buddy.app`, `claude-buddy.app.tar.gz`, and — once the signing key is configured — `claude-buddy.app.tar.gz.sig`. Without a key the tarball is produced and the `.sig` is not; that is the expected inert state.

- [ ] **Step 8: Run both suites**

Run: `npm test && cd src-tauri && PATH="$HOME/.cargo/bin:$PATH" cargo test -- --test-threads=1`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock package.json package-lock.json src-tauri/tauri.conf.json src-tauri/capabilities/default.json src-tauri/src/update.rs src-tauri/src/lib.rs src-tauri/src/window.rs .github/workflows/ci.yml README.md
git commit -m "feat: check for and install updates from the tag pipeline"
```

---

### Task 11: Bring the README in line

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: nothing.

- [ ] **Step 1: Update the settings file example**

Replace the JSON block under `### Settings file` with:

```json
{
  "pausedThresholdMs": 600000,
  "alertNeedsInput": true,
  "alertDied": true,
  "alertFinished": false,
  "sound": false,
  "muteUntilMs": 0,
  "launchAtLogin": false,
  "showBackgroundJobs": true,
  "hideWhen": "noSessions",
  "preferredDisplay": null,
  "positions": {}
}
```

- [ ] **Step 2: Correct the tray and alert sections**

- Under `## Using it`, remove **View mode** from the tray menu list and add **Install update** and the hide setting to the Settings line.
- Under `### Alerts`, replace "Only two transitions interrupt you" with three, describing the opt-in finished alert, and state that a needs-input notification carries the session's actual question and that clicking one raises that session.
- Under `## Limitations`, delete the "One view mode" bullet. Replace the "Unsigned" bullet's implication that there is no update path with a note that updates are delivered in-app once a signing key is configured.
- Under `## What you see`, add a line to the colour table noting that each state also has its own shape.

- [ ] **Step 3: Correct the test counts**

Run both suites, read the totals, and update the two numbers in `## Tests`.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: bring the README in line with v2"
```
