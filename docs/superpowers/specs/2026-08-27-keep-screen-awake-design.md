# Keep screen awake while an agent works

An opt-in tray toggle that holds the display on for as long as any session the
widget is showing is busy or waiting on the user.

## Problem

A long agent run is exactly the situation where nobody is touching the
keyboard. The display sleeps on its idle timer, and with it the screen locks,
so the run finishes — or stops to ask a permission question — behind a dark
screen. The widget already knows the answer to "is anything working right now";
it derives it every tick. Nothing acts on that answer beyond drawing it.

The existing workaround is `caffeinate` in a spare terminal, which the user has
to remember to start and, worse, remember to stop.

## What it does

One checkbox in the tray menu, "Keep screen awake", directly beneath "Hide
widget". Off on a fresh install. While it is ticked *and* at least one shown
session is `Busy` or `Waiting`, macOS is asked not to idle-sleep the display.
The moment nothing is in either state, the request is dropped and normal sleep
resumes.

## Design

### Setting

`Config` gains `keep_awake: bool`, serialised `keepAwake`, defaulting to
`false`. `Config` is `#[serde(default)]` throughout, so an existing settings
file loads unchanged and is not rewritten. Mirrored in `src/types.ts`.

Off by default deliberately: preventing display sleep and auto-lock changes how
the machine behaves, and that must not happen because someone installed a
status widget.

### Policy — new `src-tauri/src/awake.rs`

```rust
pub fn should_stay_awake(sessions: &[SessionSnapshot], keep_awake: bool) -> bool {
    keep_awake
        && sessions.iter().any(|s| {
            matches!(s.state, SessionState::Waiting | SessionState::Busy)
        })
}
```

Pure, in the shape of `visibility::should_hide`, so the policy is tested with
no window server and no IOKit.

`Waiting` is included on purpose: a session blocked on a permission prompt is
precisely the case where a sleeping display costs the user the most.

Background jobs need no clause. `snapshot()` already filters the session list
by `show_background_jobs`, so a subagent the user has chosen not to see is not
in `sessions` and cannot hold the display on. "Follows the Show background jobs
setting" is a property of the input, not a rule in this function.

### Holder — same module

```rust
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: CFStringRef,
        level: u32,
        name: CFStringRef,
        id: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(id: u32) -> i32;
}
```

`kIOPMAssertionTypePreventUserIdleDisplaySleep` at
`kIOPMAssertionLevelOn`, named "claude-buddy: agent working" — that string is
what `pmset -g assertions` prints, so it has to say who is holding the display
and why.

A `Mutex<Option<u32>>` holds the live assertion id. `apply(want: bool)` is
idempotent: it creates only on `false → true`, releases only on `true → false`,
and does nothing otherwise. That is what makes it safe to call on every tick.

In-process FFI rather than spawning `caffeinate`: no child process to leak, no
process churn on every busy/idle flip, and the kernel drops the assertion if
claude-buddy dies — which is also why there is no release-on-quit code.

One new dependency, `core-foundation = "0.10"`, for `CFString`. That is the
version `core-graphics` 0.24 already resolves to.
`IOPMAssertionCreateWithName` takes `CFStringRef` for both the type and the
name and has no C-string variant, so Core Foundation cannot be avoided.

### Wiring — two callers

**`lib.rs`, in `on_update`:**

```rust
awake::apply(awake::should_stay_awake(
    &update.sessions,
    config::cached().keep_awake,
));
```

`on_update` fires only when the snapshot changes, which is exactly when this
answer can change. Unlike `apply_visibility`, this stays on the watcher thread:
power assertions are thread-safe and there is no AppKit call to marshal.

**`tray.rs`, in the menu handler:** ticking the box re-applies against
`SnapshotStore::get()`. Without this, enabling it mid-run does nothing until the
next session state change — the same reason `hide` calls `apply_visibility`
directly.

### Tray

A `CheckMenuItem` with id `keepawake`, label "Keep screen awake", placed after
"Hide widget" — both are decisions about what the machine does while you are
looking elsewhere. It goes through the existing `edit()` helper, so it
load-modify-saves, emits `CONFIG_EVENT`, and rebuilds the menu like every other
item.

Tray only, no Settings row. This is a mid-task decision — "this run matters,
don't sleep on it" — which is the line the tray menu's own doc comment draws.

## No upper bound, and why

`Busy` self-heals: a session whose transcript goes quiet for
`PAUSED_THRESHOLD_MS` (10 minutes) becomes `Paused`, which is not in the awake
set, so a wedged run cannot hold the display indefinitely.

`Waiting` has no such decay, and this is accepted. A session that asked a
question stays `Waiting` until answered, so walking away from a prompt leaves
the display lit and unlocked until the user comes back. Capping it was
considered and rejected: a timeout would put the display to sleep at precisely
the moment the feature exists to prevent, and the user can untick the box.

The security consequence is real and belongs in the README: while this is on
and an agent is working, the Mac will not auto-lock.

## Testing

`awake.rs` unit tests, in the style of `visibility.rs`:

- off ⇒ false, whatever the sessions
- `Busy` ⇒ true
- `Waiting` ⇒ true
- `Idle`, `Paused`, `Dead` ⇒ false
- empty list ⇒ false
- a mixed list containing one `Busy` ⇒ true

The FFI holder is not unit-tested — it needs real IOKit and a real display.
Verified manually with `pmset -g assertions` showing the named assertion appear
while a session is busy and disappear when it finishes.

Frontend: `keepAwake: false` added to the `SettingsPanel.test.tsx` config
fixture, which TypeScript requires once the field exists on `Config`.

## What this owes

- **README** — "Using it" tray menu list gains the item; the settings file
  section gains `keepAwake`; the auto-lock consequence is stated.
- **CHANGELOG** — user-visible feature.
