# clawde-buddy v2 — correctness, reach and distribution

Ten changes to the shipped v1 widget: three correctness fixes, five additions,
and two pieces of cleanup. Grouped here because several of them touch the same
files and one of them (the notification rewrite) subsumes a bug found while
verifying another.

## Problem

v1 answers "which of my sessions needs me" well enough to be useful, and gets
three things wrong that undercut it:

- The popover reports the wrong elapsed time — always a few seconds, however
  long a session has been blocked. The one number that answers "how long has
  this been stuck?" is decorative.
- A session that has been quiet for a while and then dies vanishes silently,
  with no red dot and no alert, indistinguishable from a clean exit.
- Two of the commands do blocking work on the main thread, in an app whose
  distinguishing feature is animation smoothness.

Beyond the fixes, the widget tells you *that* a session wants you but never
*what* it wants, the alert is not actionable, state is encoded in colour alone,
the widget occupies screen space when there is nothing to report, three of the
four advertised view modes do not exist, and an installed copy can never update
itself.

## Changes

### 1. Elapsed time is computed from an absolute timestamp

`SessionSnapshot` gains two absolute epoch fields, `status_time_ms` and
`started_at_ms`, serialized as `statusTimeMs` and `startedAtMs`. The derived
`elapsed_ms` and `uptime_ms` remain, so nothing that reads them breaks.

`fingerprint` in `watcher::watch` is unchanged: it still excludes clock-derived
fields, so the watcher still emits only when state actually changes and the
widget still does not re-render twice a second.

`SessionPopover` runs a one-second interval while it is open and derives elapsed
and uptime from the absolute timestamps against `Date.now()`. Only the open
popover re-renders; the row does not.

**Why not emit every tick.** Emitting on every reconcile would re-render the
whole row twice a second, which is what `fingerprint` exists to prevent. The
clock belongs where the value is displayed.

### 2. Dead retention is measured from when death was first observed

`DEAD_RETENTION_MS` is currently applied to `elapsed_ms`, which is the age of
`statusUpdatedAt`. A session idle for longer than the retention window is
therefore filtered out on the same tick it is first seen dead — before it can be
rendered red and before `diff_alerts` can see it. Confirmed by test: a session
quiet for twelve minutes whose process is gone yields a snapshot of length 0 and
zero alerts.

The watcher keeps `first_seen_dead: HashMap<String, i64>`, session id to the
timestamp of the first tick on which that session read as dead. Retention is
measured from that value. Entries are removed when a session is no longer
present in the registry, so the map cannot grow without bound.

`snapshot()` stays pure. The map is passed in and a list of newly-dead session
ids is returned alongside the snapshot, so the caller owns all mutation. The
signature becomes:

```rust
pub fn snapshot(
    files: &[RegistryFile],
    liveness: &dyn PidLiveness,
    activity: &dyn ActivityProbe,
    now_ms: i64,
    paused_threshold_ms: i64,
    include_background: bool,
    first_seen_dead: &HashMap<String, i64>,
) -> SnapshotResult
```

where `SnapshotResult` carries the sessions and the set of ids observed dead
this tick.

### 3. Blocking commands move off the main thread

Tauri runs non-async commands on the main thread. `raise_session` spawns `ps`
and `open` and waits on both; `session_detail` opens a file and may scan every
project directory; `set_config` writes to disk; `list_displays` queries the
window server.

`raise_session` and `session_detail` become `async fn`. Both do only process
and file work, and both sit on the hover and click path.

`set_config` and `list_displays` stay synchronous. Both touch AppKit — monitor
enumeration and window repositioning — and neither is on the animation path, so
moving them off the main thread would trade a real risk for no gain.
`get_sessions` (a mutex read) and `resize_widget` (on the animation path, where
a thread hop would add latency) stay synchronous for the original reasons.

### 4. Notifications are delivered directly, and clicking one raises the session

`tauri-plugin-notification` cannot do this. Its desktop path spawns
`notify_rust::Notification::show()` onto the async runtime and discards the
result, and `onAction` / `registerActionTypes` are documented mobile-only.

Two consequences. First, click-to-raise is impossible through the plugin.
Second — and this is a live bug in v1 — `builder().show()` always returns
`Ok(())`, so the `is_err()` branch in `notify::deliver` never runs and the
amber-flash fallback for denied notification permission is dead code. The README
promises that fallback.

`tauri-plugin-notification` is dropped in favour of `mac-notification-sys`,
already present as a transitive dependency, which returns a real
`NotificationResult<NotificationResponse>` and reports
`NotificationResponse::Click`.

Each delivered alert sets `wait_for_click(true)` and is sent from a waiter
thread that blocks on the response and calls `bridge::raise::raise` for that
session's pid on `NotificationResponse::Click`. `wait_for_click` rather than a
`MainButton`: both return `Click`, but the button variant adds a visible control
to the notification, and the whole point is that the notification body itself is
the target. Delivery failure emits `FLASH_EVENT` as before, and now actually
can.

`Alert` gains a `pid` field so the waiter knows what to raise.

**Thread budget.** A notification the user never touches parks its waiter until
macOS resolves it, which may be never. Outstanding waiters are capped at 8; past
that, alerts are delivered with `wait_for_click(false)`, which does not block and
therefore cannot be clicked through to a session. Eight unanswered alerts means
the user is not reading them anyway.

**Bundle identity.** `mac_notification_sys::set_application` must be called once
before the first send, with the real bundle identifier when bundled and
`com.apple.Terminal` under `tauri dev`, mirroring what the plugin did.

### 5. Activity detail

`bridge::transcript` gains:

```rust
pub fn latest_activity(bytes: &[u8]) -> Option<String>
```

scanning tail records newest-first for the most recent tool use
(`message.content[]` with `type == "tool_use"`, taking `name`) and falling back
to the leading text of the most recent assistant message. Truncated to fit one
popover line.

Two consumers, deliberately different:

- **Popover** — `session_detail` returns it alongside branch, model and effort.
  Lazy, one tail per hover, exactly as today.
- **Notification body** — when the watcher observes a session *transition into*
  `Waiting`, it tails that one transcript once and uses the result as the
  alert's `detail`, so the notification says what is actually being asked rather
  than "input needed". One tail per transition, not per tick, not per session.

If the tail yields nothing the existing `waitingFor` text stands.

### 6. Turn-finished alert

`AlertKind::Finished`, fired on a `Busy → Idle` transition. Config key
`alert_finished`, **default false**, with a settings checkbox. Off by default
because a finished turn is the common case and interrupting on it is the noisy
choice; the users who want it want it badly.

`alert_kind` currently maps state to kind, which cannot express a transition.
It becomes a function of the previous and next state.

### 7. State is encoded in shape as well as colour

The expanded row and the popover head distinguish five states by hue alone.
Within the same 8px box and with no change to pill metrics, each state gets a
silhouette:

| State | Shape |
|---|---|
| waiting | filled triangle |
| busy | filled circle |
| idle | hollow circle |
| paused | hollow circle, dashed border |
| dead | × |

Implemented in `dotRow.css` off the existing `dot-<state>` classes. Colours are
retained; the shape is additive.

### 8. Hide when there is nothing to report

New config key:

```
hideWhen: "never" | "noSessions" | "nothingActive"
```

Default `noSessions`. `nothingActive` also hides when every session is idle,
paused or dead — that is, when nothing is waiting or busy.

The decision is a pure function over the snapshot and the setting, tested
directly:

```rust
pub fn should_hide(sessions: &[SessionSnapshot], hide_when: &str) -> bool
```

Hiding calls `order_out` on the panel; showing calls `show` followed by
`restore_position`, so a widget that reappears lands where the user left it.
The tray icon remains, so a hidden widget is never unreachable.

### 9. View modes are removed

Three of the four modes in the tray menu and the settings dropdown are disabled
placeholders, and the fourth — "invisible until needed" — is what `hideWhen`
now provides. The submenu and the dropdown both go. The tray menu keeps
Settings, Mute alerts 1h and Quit.

`view_mode` stays in `Config` and is still accepted on load, so an existing
config file continues to parse; it is no longer read, written or validated.
`commands::validate` drops its view-mode branch.

### 10. Auto-update

`tauri-plugin-updater`, with `createUpdaterArtifacts` enabled in
`tauri.conf.json`. The tag pipeline publishes `.app.tar.gz`, its `.sig`, and a
generated `latest.json` to the same generic-packages path that already carries
the DMG. The DMG remains the first-install route.

The app checks once on launch. When a newer version exists it delivers a
notification and adds an "Install update" tray item; the install itself is
user-initiated, never automatic.

Signing is the user's step, documented in the README: generate a minisign
keypair with `npm run tauri signer generate`, add the private key and its
password to GitLab CI/CD variables as `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and put the public key in
`tauri.conf.json`. Until that is done the updater is inert rather than broken —
no key means no manifest to verify against, and the check fails closed.

## Testing

Rust:

- `first_seen_dead` retention: newly dead within window renders and alerts; the
  same session past the window drops; map entries are pruned when the registry
  entry disappears. Includes the regression case from the probe — a session
  paused past the threshold that then dies.
- `AlertKind::Finished` edges: `Busy → Idle` fires, `Idle → Idle` does not,
  cold start does not, and the config toggle gates delivery.
- `latest_activity`: a tool-use record, an assistant-text fallback, a truncated
  first line, and a transcript with neither.
- `should_hide`: every combination of the three settings against empty, quiet
  and active session lists.
- Notification thread budget: the waiter cap is a pure counter check, tested
  without sending anything.

Frontend:

- Popover elapsed against a fake clock: advancing time re-renders the popover
  and leaves the row untouched.
- Shape-per-state rendering for all five states.
- Settings: the hide dropdown and the finished-alert checkbox round-trip; the
  view-mode control is gone.

Manual only:

- Notification click actually raising a session — needs a real notification
  centre.
- The updater end to end — needs a real signed release.

## Out of scope

Repeat and escalation alerts, transcript activity in the collapsed row,
signing and notarization for Gatekeeper, a CI test stage, ESLint, a CSP, and
config-file watching. All were considered and deferred; several were explicitly
dropped from the request.
