# clawde-buddy — design

**Date:** 2026-08-25
**Status:** approved, ready for implementation planning

## Problem

Multiple Claude Code sessions run concurrently across projects. When one blocks on input — a question, a permission prompt — nothing surfaces it outside that session's own window. Blocked sessions sit idle for minutes while attention is elsewhere. A menu-bar indicator is insufficient: it is only visible when the menu bar is, and it collapses all sessions into one glyph.

## Solution

A floating always-on-top macOS widget showing live state for every Claude Code session the user drives. Alerts on the two transitions that matter (needs input, died). Click a session to raise the window running it.

## Data source

`~/.claude/sessions/<pid>.json` — the live session registry Claude Code maintains. One file per running session, rewritten on state change.

Observed schema (verified against Claude Code 2.1.237):

```json
{
  "pid": 7952,
  "sessionId": "a1b2c3d4-0000-4000-8000-000000000001",
  "cwd": "/Users/dev/Documents/Code/api-service",
  "startedAt": 1787637231465,
  "procStart": "Tue Aug 25 05:53:49 2026",
  "version": "2.1.234",
  "kind": "interactive",
  "entrypoint": "cli",
  "messagingSocketPath": "/tmp/cc-socks/7952.sock",
  "name": "api-service-55",
  "nameSource": "derived",
  "status": "waiting",
  "updatedAt": 1787662267409,
  "statusUpdatedAt": 1787662267409,
  "waitingFor": "input needed"
}
```

`status` is one of `busy`, `waiting`, or absent/idle. `waitingFor` is free text describing what the session is blocked on. Claude Code itself maps these to a tempo of active / blocked / idle.

Fields `status`, `statusUpdatedAt` and `waitingFor` are absent until first set — absence is not an error and must not be treated as a distinct state.

**Secondary source:** `~/.claude/projects/<slug>/<sessionId>.jsonl`, the session transcript. Its last record carries `gitBranch`, `model`, `effort` and `version` — not available in the registry. Read lazily, on hover only.

**Hard constraint:** the app is strictly read-only against `~/.claude`. Claude Code owns that directory and prunes it itself. Never unlink, never rewrite, never create files there.

## Architecture

Three layers with enforced boundaries. Tauri v2, Rust backend, React/TypeScript frontend.

### Layer 1 — `watcher` (Rust)

Owns the registry directory. FSEvents subscription for instant transitions, plus a 2s reconcile tick. Both are required: FSEvents cannot report that a process died without its file changing, and FSEvents coalesces or drops events under load.

Emits a complete `SessionSnapshot[]` on every change. No deltas — the UI never reconstructs state from a stream.

The core is a pure function:

```
fn snapshot(
    registry: &[RegistryFile],
    alive: &dyn PidLiveness,
    now: Instant,
) -> Vec<SessionSnapshot>
```

All I/O is injected. The entire state machine is testable against fixture JSON with a fake clock and a fake liveness oracle.

Session filtering happens here, before anything downstream: only `entrypoint ∈ {cli, claude-desktop}` passes. This excludes `sdk-cli` sessions — plugin machinery such as claude-mem observer sessions, which the user cannot answer and must never be alerted about.

### Layer 2 — `bridge` (Rust)

Two operations the frontend cannot perform. Nothing else belongs here.

1. `raise_session(pid) -> Result<RaiseOutcome>`
2. `session_detail(session_id) -> Result<TranscriptDetail>`

Both external effects (process-tree lookup, app activation) sit behind traits so tests never launch an application.

### Layer 3 — `ui` (React/TypeScript)

Receives snapshots over a Tauri event channel and renders. Holds no derived state: `paused`, elapsed time and rollup counts all arrive precomputed from Layer 1.

The renderer is a single swappable component behind a `SessionView` interface. Four view modes are planned; the dot row ships first and the rest drop in without touching Layers 1–2.

## State model

Six states, all derived in Layer 1.

| State | Derivation | Colour |
|---|---|---|
| `waiting` | `status == "waiting"`, carries `waitingFor` as detail | amber |
| `busy` | `status == "busy"` | blue |
| `idle` | status absent or idle/running, `statusUpdatedAt` newer than the paused threshold | grey |
| `paused` | as `idle`, but `statusUpdatedAt` at or beyond the paused threshold (default 10 min) | dim grey |
| `dead` | registry file present, process not alive | red |
| *gone* | registry file removed — clean exit; drops off the list entirely | — |

`paused` is a derived convenience, not a Claude Code concept. It exists to surface sessions left open in projects the user has moved on from.

Liveness is `kill(pid, 0)` **plus** a `procStart` match against the process's real start time. The pid alone is insufficient: a recycled pid otherwise reads as a living session.

## Alerts

Edge-triggered on transitions, never on states. Computed by diffing consecutive snapshots in Layer 1.

| Transition | Alert |
|---|---|
| `* → waiting` | yes |
| `* → dead` | yes |
| `* → idle` (turn finished) | no — visual only |
| session appeared | no — visual only |

Channel: native macOS notification, optional sound, per-event toggle in settings.

**Cold-start suppression:** the first snapshot after launch establishes a baseline and fires nothing. Without this, every launch produces a burst of alerts for sessions that were already waiting.

If notification permission is denied, fall back to flashing the pill until acknowledged.

## UI

### Collapsed (resting state)

A summary pill: an amber chip reading `N needs you` when any session is waiting, plus a muted `N working` count. Constant width regardless of session count. The amber chip is absent entirely when nothing needs input.

### Hover stage 1 — pill morphs

The pill widens in place into a named-dot row: one coloured dot plus short project name per session, separated by hairlines. One surface; nothing overlays the user's work.

At eight or more sessions the row caps and overflows to `+N more`.

### Hover stage 2 — per-session popover

Hovering one name opens a popover anchored beneath it. The pill's own height never changes, so the hovered row cannot slide out from under the cursor.

Popover contents:

| Field | Source |
|---|---|
| session name | registry `name` |
| state + elapsed | derived state, `statusUpdatedAt` |
| cwd | registry `cwd` |
| branch | transcript `gitBranch` |
| model + effort | transcript `model`, `effort` |
| entrypoint, pid, uptime | registry |

Branch matters disproportionately: it is often the only way to distinguish two sessions in the same repository.

The popover needs a grace delay before opening, and must flip its anchor near screen edges.

### View modes

Four, user-selectable in settings. **Dot row ships first**; the state model must be proven against one renderer before the others are built.

1. **Dot row** — as specified above.
2. **Card stack** — one always-readable row per session; no hover needed.
3. **Character buddy** — animated critter reflecting the worst state across sessions; click to expand the real list.
4. **Invisible until needed** — nothing on screen unless a session enters `waiting`.

## Jump to session

Clicking a session raises the window running it. Implemented as a fallback ladder; each rung is independent of the ones below.

**Rung 1 — raise the app.** Walk the ppid chain to the first executable inside a `.app` bundle, read `CFBundleIdentifier` from its `Info.plist`, then `open -b <bundleid>`.

Verified against both real entrypoint shapes:

```
cli session in Cursor's integrated terminal:
  7952 claude (ttys003) → 7951 claude → 7447 zsh
       → 6323 Cursor Helper: terminal pty-host
       → 5524 /Applications/Cursor.app          ← raise target

claude-desktop session:
  99215 claude → 99213 Claude.app/Contents/Helpers/disclaimer
        → 51954 /Applications/Claude.app        ← raise target
```

`open -b` requires **neither Accessibility nor Automation permission**. This is why rung 1 uses it rather than an AppleScript `activate`: a v1 install with no permission prompt is worth protecting.

**Rung 2 — select the tab.** Best-effort, scriptable hosts only. `ps -o tty= -p <pid>` yields the tty; Terminal.app and iTerm2 both expose AppleScript to select the session owning a given tty. Costs one Automation prompt per host app on first use.

Strictly additive — implement after rung 1 ships, and only if the app-level jump proves insufficient.

**No rung 3.** VS Code-family hosts, including Cursor, expose no tab-targeting API. Rung 1 is the answer there; the popover has already told the user which project they are jumping to.

If a walk terminates without reaching a `.app` bundle, surface a quiet error in the popover. Do not crash, do not retry.

## Window behaviour

- `LSUIElement = true` — no Dock icon, no app-switcher entry.
- Panel level above fullscreen apps; `canJoinAllSpaces` so it follows across Spaces rather than living on one.
- Non-activating panel: hover and click never pull focus to the widget. Clicking a row does move focus, but to the user's terminal, which is the intent.
- Draggable anywhere. Position persisted **per display**, keyed by display ID — without this the widget lands off-screen on every dock/undock cycle.
- Right-click menu: Settings, view mode, Mute alerts 1h, Launch at login, Quit. With no Dock icon this menu is the only route to quitting, so it ships in v1.

## Settings

`~/Library/Application Support/com.clawde.buddy/config.json`. Plain JSON, hand-editable.

Keys: view mode, paused threshold (default 10 min), per-event alert toggles, sound on/off, positions keyed by display ID, launch at login, `muteUntil` timestamp backing the Mute alerts 1h menu item.

## Failure modes

| Case | Handling |
|---|---|
| Partial JSON read mid-write | Skip that entry this tick, retain last known good. Registry writes are not atomic; truncated reads are normal. |
| pid reuse | `procStart` match alongside `kill(pid, 0)`. |
| FSEvents coalescing or drops | 2s reconcile tick is the backstop. |
| Stale file, dead process | Show as `dead`, alert once, drop from the list after 5 min. Never unlink. |
| Huge transcripts | Tail the last 64KB, parse the last complete line. Never read the whole file — observed sizes reach 3.4MB per session, 44MB per project directory. |
| `statusUpdatedAt` in the future | Clock skew — clamp elapsed to 0. |
| Notification permission denied | Flash the pill until acknowledged. |
| Registry directory absent | Empty state, not an error. |
| Transcript unreadable or absent | Popover renders registry-sourced fields normally and shows `—` for branch, model and effort. Never blocks the popover from opening. |
| Saved position references a display no longer attached | Fall back to the primary display's default corner. |
| 8+ sessions | Cap the morphed row, overflow to `+N more`. |
| `open -b` fails | Quiet error in the popover. |

## Testing

Weighted toward Layer 1, where the logic lives.

- **State derivation** — table tests over fixture registry directories with injected clock and injected liveness. All six states, and every transition edge between them. The bulk of the suite.
- **Alert edge-triggering** — snapshot-diff tests, explicitly including cold-start suppression.
- **Session filtering** — `sdk-cli` entries excluded; `cli` and `claude-desktop` retained.
- **Ancestry walk** — synthetic process trees behind an injected ppid lookup: both verified real shapes, plus an orphan whose chain never reaches a `.app`.
- **Transcript tail** — fixtures with a truncated final line, an oversized file, and a session with no assistant message yet.
- **Renderers** — Vitest + React Testing Library against snapshot fixtures.
- **Manual only** — floating above fullscreen apps, multi-monitor position restore, a real jump into Cursor. Not worth automating.

## Out of scope

Inline reply to sessions, spawning sessions, history and analytics, remote or cross-machine sessions. No network calls. No telemetry.

## Build order

1. Layer 1 watcher plus its test suite — state model proven against fixtures before any UI exists.
2. Tauri shell: floating non-activating panel, `LSUIElement`, per-display position persistence, right-click menu with Quit.
3. Dot row renderer: collapsed pill, morph on hover.
4. Popover, including lazy transcript tail for branch and model.
5. Jump to session, rung 1.
6. Alerts with cold-start suppression.
7. Settings file and settings UI.
8. Remaining three view modes.

Steps 1–7 constitute a shippable v1 and belong in one implementation plan. Step 8 is additive view work against a proven state model and should be planned separately.
