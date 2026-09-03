# Changelog

One section per tag, newest first. `scripts/release-notes.sh <tag>` reads the
section out of this file, and the release workflow uses it as the body of the
GitHub release and as the notes in the in-app update dialog — so a section that
is missing here leaves that tag with nothing but the download boilerplate.

## v0.12.0 — 2026-09-03

- **Finished subagents no longer sit in the task list forever.** A session that
  had run subagents listed every one of them as still running, for as long as
  the session lived — one real session showed fifty, the oldest four hours old,
  and all fifty had finished. The widget read a subagent's *result* as its
  launch and then waited for a completion notification that Claude Code only
  writes for background tasks, so a foreground subagent could never retire. It
  is now started by its call and ended by its result, which also means a
  subagent that is genuinely working shows while it works and holds its session
  in `tasking` — where before the state was arriving hours late and never
  leaving.

- **The task list stops at five, and counts the rest.** A session that fanned
  out to a dozen subagents filled the whole panel with task rows and pushed
  every other field off it. Five rows now, then `N more…`.

## v0.11.0 — 2026-09-03

- **A background subagent counts as a task.** A session that launched agents in
  the background read `idle` and said nothing about them: the launch record
  names the agent with a field the widget was not reading, so the agents never
  entered the session's task list, the pill never counted them, and the
  notification that should have announced each one finishing had no task to
  report against. Background shells and watches were unaffected.
- **A task you kill stops counting.** Killing a background task left the
  session reading `tasking` for as long as it lived, with the killed task still
  listed and still ageing. A task that exits announces itself, however it exits
  — but a task killed from Claude Code's own task list is recorded nowhere in
  the transcript at all: the only trace is a `[killed]` line appended to the
  task's own output file. That file is now read for any task still believed to
  be running, which also settles a task whose ending the widget was not running
  to see. The ending is dated to when the marker was written rather than when
  the widget noticed, and a missing output file leaves the task alone, since
  absence is not an ending.
- **Notch mode's row detail says what the popover says.** Hovering a row in the
  slab used to give four unlabelled lines. It now carries the popover's own
  fields — what the session is doing, the background tasks it is waiting on,
  its name, working directory, branch, model and process — omitting only the
  three the row and the footer already say. The task list is the same one the
  popover draws, so it names each task and its age, and it is right whether or
  not background jobs are given rows of their own; the count it replaces was
  blind to subagents and reported zero whenever that setting was off.

## v0.10.0 — 2026-09-02

- **A session waiting on a background task says so.** A session that started a
  background test run, a dev server, a watch or a background subagent goes
  quiet, and after ten minutes it read `paused` — the same as a session nobody
  was driving. There is now a sixth state, `tasking`, drawn as a hollow ring
  with a turning arc, and the popover lists every task the session is waiting
  on: what kind it is, what it is, and how long it has been going. Registry
  background jobs count as tasks on the session they belong to, so a parent
  reads the same way whether or not the job is shown as its own row. The
  collapsed pill counts it too, with a new chip between the working count and
  the died count. The widget stays on screen and the display stays awake while
  a task runs, and in crazy mode a task you launched stokes the fire the way a
  working session does — a registry job still does not, for the same reason a
  background job never has. A job is the one kind of task that never raises
  the finished notification either: it stops being a task the moment its
  process is gone, rather than ending with a status to report.
- **A notification when a background task finishes.** It names the task and how
  it ended, so a failure does not read like a success. On by default, and
  switched off with the new "when a background task finishes" checkbox in
  Settings.

## v0.9.0 — 2026-08-28

- **Sessions are named by what they are about.** The row used to label every
  session with the folder it was running in, which made three sessions in one
  repository three copies of the same word. It now shows the title Claude Code
  gives a session — read from the session's own transcript — and falls back to
  the folder name for a session that has not been titled yet. Long titles are
  clipped so the pill cannot grow across the screen. The popover is headed by
  the full title and carries the folder-derived name and the working directory
  underneath it, so nothing that used to be visible has gone away, and
  notifications name a session the same way the row does. A session that has
  been running for hours is named correctly too: Claude Code writes the title
  once, near the start of the transcript, so the widget reads the whole file
  once when the end of it has nothing to say.

## v0.8.0 — 2026-08-28

- **Crazy mode**, off by default, in Settings. Turned on, the widget stops
  being subtle about what it is already telling you: the pill catches fire as
  sessions go busy — warm at one, properly alight at three — trembles while a
  session has been waiting on you for more than thirty seconds, fractures as
  the five-hour limit runs down until the limit bar itself goes molten, and
  breaks the dot apart once when a session dies. Each of the four responds to
  its own signal, so the widget still says *which* thing is intense rather than
  only that something is. Nothing animates while nothing is happening, and if
  your Mac is set to reduce motion the colours and cracks still ramp while
  everything that moves stays still. Not drawn in notch mode.
- **The settings window looks like a macOS window.** Its background is AppKit's
  own material rather than a colour painted by the page, so it takes the tint of
  what is behind it and goes flat when the window is not frontmost. Labels are
  flush right against the controls they name, the controls are the system's own
  rather than styled rectangles, and the whole form follows light and dark
  appearance instead of being permanently dark.

## v0.7.1 — 2026-08-27

- **The tray menu is in three groups.** What the app is — *About* and the
  update item — sits at the top, where a Mac app menu would put it. The
  mid-task toggles are in the middle, which is what the menu is opened for,
  with *Show background jobs* beside the other ticks and *Mute alerts* last
  since it is the one submenu among them. *Settings…* and *Quit* are together
  at the bottom. Previously the toggles led and the identity items were mixed
  in with *Settings…*.

## v0.7.0 — 2026-08-27

- **Keep screen awake while agents work** in the tray menu. A tick that holds
  the display on for as long as a session is working or waiting on you, and
  releases it the moment nothing is. Off unless you turn it on. A long run is
  exactly when nobody is touching the keyboard, so the display sleeps on its
  idle timer and the answer — or the permission question that stopped the run —
  ends up behind a dark, locked screen. The label names the condition rather
  than just the effect, and flips to *Keeping screen awake now* while the hold
  is real: a tick on its own reads as "always", and this setting does nothing
  whatsoever on an idle machine. Note that while it is holding, the Mac will
  not auto-lock. It is an IOKit power assertion, visible as *claude-buddy: agent
  working* in `pmset -g assertions`, released by the kernel if the app exits.
  Subagents follow **Show background jobs**: ones you have chosen not to see
  cannot hold the display on either.
- **About claude-buddy** in the tray menu — the standard macOS panel, with the
  icon, the version you are running, the author and the licence. With no Dock
  icon and no app menu there was nowhere the app said which version it was.
- **The update item names the version it will install.** Once a check has found
  something newer it reads *Install update 0.8.0…* instead of *Check for
  updates…*. The launch check would tell you "version 0.8.0 — install it from
  the menu bar" and the menu you then opened still offered to check, as though
  nothing had. Same item and same action either way: it installs when there is
  something to install and says you are up to date when there is not.

## v0.6.0 — 2026-08-27

**The tray menu now carries the three things you toggle mid-task.** It had two
items and a broken third; what belongs there turns out not to be "the settings
that fit" but the ones you reach for while something else is happening.

- **Hide widget** puts the widget away and keeps it away. It outranks *Hide the
  widget* in Settings rather than being another of its modes — including
  **Never** — because the setting answers "is there anything worth showing" and
  this answers "not now". A session waking up mid-screen-share will not undo it.
  Untick it, or set `hidden` back to `false`, to get the widget back.
- **Mute alerts** is a submenu: **For 1 hour**, **For 8 hours**, or **Until I
  unmute**. The old *Mute alerts 1h* was write-only — nothing told you a mute was
  running and nothing could lift one early, so clicking it was indistinguishable
  from clicking nothing. While a mute is in effect the item reads *Alerts muted*
  and **Unmute now** inside it is live; it is greyed out the rest of the time, so
  the menu answers "am I muted" without your having to remember.
- **Show background jobs** is the Settings checkbox where you need it. One run
  spawning six subagents is the moment you want them out of the row, and that
  moment does not wait for a settings window. The menu and the form write the
  same field, so a change in either shows up in the other immediately.

Placement, the display, launch at login and the three alert events stay in
Settings, where a longer list costs nothing.

**The update item says what happened.** It is **Check for updates…** now, and it
reports every outcome: already up to date, downloading, or why it could not.
Before, installing on success was the only branch that said anything — already
being on the newest version, an unreachable manifest and a failed download were
all silence. The first of those is the common case, which is what made a working
menu item look broken. The item is still absent entirely when no signing key is
configured.

**New settings key `hidden`.** `true` hides the widget whatever `hideWhen` says.
`muteUntilMs` also accepts `9223372036854775807`, which is what *Until I unmute*
writes; an indefinite mute is a sentinel rather than a date far in the future,
so it cannot quietly expire.

## v0.5.0 — 2026-08-27

**The app is called `claude-buddy` now, and it lives on GitHub.** The binary,
the app bundle and the DMG all change name with it: `claude-buddy.app` and
`claude-buddy_<version>_<arch>.dmg`. Releases, issues and the source are at
<https://github.com/norbertsuski/claude-buddy>, and the old project is not
being maintained alongside it.

- **The bundle identifier is now `com.claude.buddy`**, which moves the settings
  file to `~/Library/Application Support/com.claude.buddy/config.json`. You do
  not have to move it yourself: on first launch, if there is nothing at the new
  location, the app reads the old one and copies the file across — once, and
  leaving the original where it is. Every setting you had comes with it.
- **Notification permission has to be granted again.** macOS files permissions
  under the bundle identifier, so a changed identifier is, as far as the system
  is concerned, an application it has never met. The first alert raises the
  permission dialog again; until you allow it the pill flashes amber instead,
  which is what it has always done when permission is refused. If your alerts
  go quiet after this update, that is why — it is worth checking **System
  Settings → Notifications** rather than assuming the alerts broke.
- **Launch at login repairs itself.** The login item macOS was holding is filed
  under the old name and points at the old app, and nothing in the settings
  file would have told you: the checkbox reads from your settings, so it would
  have gone on showing as on while nothing was registered. On first launch the
  stale login item is removed and, if you had the setting on, a new one is
  registered under the new name. The old item is removed either way — left in
  place it would keep launching the old app beside the new one for anyone who
  has not thrown it away yet.
- **The three environment overrides are renamed** to `CLAUDE_BUDDY_REGISTRY_DIR`,
  `CLAUDE_BUDDY_PROJECTS_DIR` and `CLAUDE_BUDDY_USAGE_FILE`. The old spellings
  are not read any more, so a script that sets them will silently aim the widget
  at your live sessions instead of your fixtures.

**Upgrading means installing the new app by hand.** There is no path across
this rename for the in-app updater, and the honest thing is to say so rather
than let anyone wait for a notification that is not coming:

- Every release up to and including 0.4.0 shipped with an empty signing key, so
  the updater plugin was never registered in those builds. They have never
  checked for an update and are not going to start now.
- Even a build with a key has its update endpoint compiled into it, and 0.4.0's
  points at the old host's package registry, which is going away with the rest
  of it.
- The mechanics would not survive the rename in any case. The macOS updater
  unpacks a new bundle over the *running* one, keeping the path and the name it
  already has — so a successful install would leave the new app inside a bundle
  still named for the old one, and the restart that follows would look for an
  executable that the new bundle no longer contains.

So: download the DMG from the releases page, drag it in, and drag the old app
to the Trash. Settings come across on their own; notification permission does
not.

**This is the last install you have to do by hand.** The update channel is
switched on from this release: a signing key is committed, releases carry a
signed update alongside the DMG, and the endpoint is a URL that does not move
as versions come and go. From here the app tells you when there is something
newer and *Install update* in the tray menu does the rest.

**A session running a long tool no longer reads as idle.** Claude Desktop
sessions write no `status`, so the widget fell back to transcript modification
time with a 30-second busy window — but a transcript is only appended when a
message or a tool result lands. One long build, test run or subagent holds it
silent for minutes. Measured on a live session, 58% of a twelve-minute working
stretch sat inside gaps longer than that window, all of it displayed as idle.

- The watcher now reads the transcript tail for an assistant `tool_use` with no
  `tool_result` for its id, which is direct evidence the session is mid-turn.
  `AskUserQuestion` and `ExitPlanMode` are excluded: those wait on a human, and
  the amber "needs you" state still wins.
- Bounded by the paused threshold, so an interrupted turn — which leaves its
  call unanswered for good — settles into `paused` instead of reading busy
  until the session exits.
- Sessions that report their own status are untouched: what they say still
  beats what the transcript implies.

## v0.4.0 — 2026-08-27

**The five-hour limit is live now.** The meter used to read Claude Code's cache
of the figure, which Claude Code only refreshes when it fetches usage itself —
in practice when someone opens its `/usage` panel — so it sat hours behind, or
vanished entirely on a window that had already lapsed. Measured mid-session it
said 5% where the API said 13%. The widget now makes the same request Claude
Code makes, `GET /api/oauth/usage`, every five minutes, with the OAuth token
Claude Code already holds.

- The token is read from `CLAUDE_CODE_OAUTH_TOKEN`, `~/.claude/.credentials.json`
  or the login Keychain, in that order. It is borrowed, never managed: an
  expired token skips a poll rather than triggering a refresh behind Claude
  Code's back, and nothing is ever written back.
- The cache is gone as a source, not kept as a fallback: a stale figure shown
  as though it were current is worse than no meter. So the meter takes a few
  seconds to appear after launch, and does not appear at all when no token can
  be read.
- The endpoint is private and undocumented, so every step of it fails quietly.
  A change on Anthropic's side costs the meter, not the widget.
- `showUsage` governs both halves. Hide the meter and the requests stop with it.
- `CLAWDE_BUDDY_USAGE_FILE` stands in for the whole meter, fetch included, so
  fixture runs stay reproducible — the one remaining reader of a file in
  `~/.claude.json`'s shape.

**Settings lost four controls that did not carry their weight.**

- *Play a sound* is now the parent of the three alert events, which sit indented
  under it. Switching it off writes all three off and greys them out; switching
  it on restores the defaults. An alert here *is* a notification with a sound,
  so `notify::should_deliver` gates on it as well — a config file hand-edited to
  arm an event under a silent parent still delivers nothing. `sound` now
  defaults to on, since with the gate in place the old default would have meant
  no alerts at all.
- *Smooth transitions when a status changes* is gone. Every change is timed to
  the distance it covers, which is what the setting did when it was on, and
  there was no good reason to offer the worse timing.
- *Keep the 5h limit up to date* is gone; see above — `showUsage` covers it.
- *Paused after (minutes)* is gone. The threshold is ten minutes, as it was by
  default, and it is no longer a knob.

`pausedThresholdMs`, `smoothStatusChanges` and `fetchUsage` are no longer read.
An existing config file still loads — unknown keys are ignored — and drops them
the next time the app saves it.

**Fixed:** the launch update check panicked on every start when no signing key
is configured. `app.updater()` does not fail quietly with the plugin
unregistered; it panics on state nobody managed, on a spawned task where nobody
sees it. Both update paths check for a key first now.

## v0.3.1 — 2026-08-26

- *Install update* is hidden when there is no signing key, instead of being a
  menu item whose click panicked invisibly.
- README media refreshed against fixture data, so the screenshots show the
  five-hour meter and notch mode.

## v0.3.0 — 2026-08-26

**Notch mode.** The widget can sit in the menu bar flanking a MacBook's notch:
session counts on the left, the five-hour limit's bar on the right, the notch
itself part of one black band rather than a hole in it. Hovering opens a slab
below it — a third of the display wide, one row per session, with detail under
the hovered row and background-agent counts. Turn it on in Settings, which
offers it only where there is a notch to place against.

- The five-hour limit arrived at the end of the collapsed row, with its own
  popover spelling out the reset time.
- Geometry comes from AppKit and is re-probed on display changes, so closing the
  lid hands the widget back to free placement without rewriting the setting.
- Fixes: the whole resting band hovers rather than only its content; the slab no
  longer closes itself on the way to a second row; the chips read as separate
  from the notch; two window-placement bugs; the crossfade scales with the box
  it follows.

## v0.2.2 — 2026-08-26

- The app bundle is ad-hoc signed, which stops a downloaded DMG from being
  called *damaged* by Gatekeeper.

## v0.2.1 — 2026-08-26

- The pill is sized from its pre-transform layout box, and its padding is even
  on all four sides.
- A Claude Desktop session blocked on a question is detected as waiting.
- A waiting or dead background job surfaces in the collapsed pill.
- A missing updater tarball no longer fails the release job.

## v0.2.0 — 2026-08-26

- Session state reads as a shape as well as a colour, so the five dots are
  legible to a red-green colourblind user.
- The widget can hide itself when there is nothing to report.
- Notifications carry the session's actual pending question, and clicking one
  raises that session.
- Optional alert when a session finishes its turn.
- The popover says what a session is doing.
- Updates can be checked for and installed from the tag pipeline.
- Process-walking and transcript reads moved off the main thread.

## v0.1.0 — 2026-08-25

First release: a floating widget for local Claude Code sessions, built on a
self-hosted macOS runner.
