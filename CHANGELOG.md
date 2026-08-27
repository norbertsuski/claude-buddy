# Changelog

One section per tag, newest first. `scripts/release-notes.sh <tag>` reads the
section out of this file, and the tag pipeline uses it as the release
description — so a section that is missing here leaves that tag with nothing but
the download boilerplate.

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
