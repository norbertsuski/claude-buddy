# Changelog

One section per tag, newest first. `scripts/release-notes.sh <tag>` reads the
section out of this file, and the release workflow uses it as the body of the
GitHub release and as the notes in the in-app update dialog — so a section that
is missing here leaves that tag with nothing but the download boilerplate.

## Unreleased

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
