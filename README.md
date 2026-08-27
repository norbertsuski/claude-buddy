# clawde-buddy

A floating always-on-top macOS widget that shows what every local Claude Code session is doing, and tells you when one is waiting on you.

![The cursor reaches the resting pill, which morphs into a named row of sessions, and a popover opens beneath the one under the cursor](docs/media/hover.gif)

*All screenshots on this page use mocked session data.*

## Why

Run more than one Claude Code session and you lose track of them. A session that finishes, or blocks on a question or a permission prompt, does so silently in a window you are not looking at — and sits there until you happen to check. A menu-bar dot is no help: it is only visible when the menu bar is, and it collapses every session into one glyph.

clawde-buddy reads the session registry Claude Code already maintains and puts it somewhere you cannot miss.

## What you see

At rest, a small pill with counts. Each coloured chip carries the dot of the state it counts, and a chip is absent entirely when nothing is in that state — no amber when nothing needs you, no red when nothing has died. What is merely sitting there stays as quiet grey text.

![The collapsed pill reading "1 needs you", "1 working", "1 died", "2 idle", "1 job", then a progress bar and 64%](docs/media/collapsed.png)

Hover it and the pill morphs into a named row, one dot per session.

![The expanded row: api-service waiting behind an amber triangle, its background job migrate-schemas demoted behind an arrow, web-app working behind a green circle, design-system idle behind a hollow ring, docs-site paused behind a two-bar glyph, infra-tools dead behind a red cross, and the limit bar reading 2h40m](docs/media/expanded.png)

Hover a name and a popover opens centred beneath it, with the state and how long it has held, what the session is *doing* — the newest tool it reached for, or failing that the last thing it said — plus the working directory, git branch, model, effort, entrypoint, pid and uptime. Click it to bring that session's editor to the front.

![A popover open under web-app, reading: state busy for 1m, doing Grep, cwd /Users/n/Code/web-app, branch fix/checkout-totals, model claude-opus-5 at high effort, cli with pid 927 up 47m, and 36% of the five-hour limit used with 2h40m to the reset](docs/media/popover.png)

### The five-hour limit

The end of the row is Claude Code's own five-hour usage window: a bar of how much is left, and the share as a number. The bar warms to amber and then red as the window fills, and hovering it opens a popover with the reset time. Turn it off with `showUsage`.

The figure comes from the API, and only from there: the widget asks for it every five minutes with the same `GET /api/oauth/usage` Claude Code makes, using the OAuth token Claude Code already holds — read from `CLAUDE_CODE_OAUTH_TOKEN`, `~/.claude/.credentials.json` or the login Keychain. `showUsage` governs both halves; hide the meter and the requests stop with it.

There is no fallback to Claude Code's own cache of the figure in `~/.claude.json`. The widget used to read it and nothing else, which is why this changed: that cache is refreshed only when Claude Code fetches usage itself — in practice when someone opens its `/usage` panel — so it ran hours behind. Measured mid-session it said 5% where the API said 13%. A stale figure shown as though it were current is worse than no meter, so a failed call leaves no meter: the endpoint is private and undocumented, and every step of it is allowed to fail quietly. It also means the meter takes a few seconds to appear after launch, and never appears if no token can be read.

The token is borrowed, never managed: an expired one is a reason to skip a poll, not to run the refresh flow behind Claude Code's back. Nothing is written anywhere, and the first Keychain read may raise the system's own permission dialog. The meter disappears when the answer is not usable — a window that has already reset, a shape the response does not have any more, no token to ask with.

Every state carries a shape as well as a hue, because colour alone is unreadable to a red-green colourblind user and these five dots are the widget's whole vocabulary. The box stays 11px in each case, so nothing shifts as a session changes state.

| Colour | Shape | State |
|---|---|---|
| amber | triangle, inside a pulsing ring | waiting on you — carries the reason, e.g. `input needed` |
| green | filled circle with a glow | working |
| grey | hollow ring | idle |
| dim grey | two-bar pause glyph | paused — quiet for ten minutes |
| red | cross | died — the process is gone |

### Sessions, subagents and jobs

Background jobs and subagents appear **demoted** — dimmer and smaller — directly after the session that owns them, behind an arrow rather than a divider:

```
● api-service  →  ● migrate-schemas  |  ● web-app  |  ● design-system
```

They are matched to their parent by working directory, which is the only link the registry offers. They never count toward the session chips; they are summarised separately as `N jobs`. Turn them off entirely in Settings.

`sdk` entries — library callers such as plugin machinery — never appear, since you cannot answer them.

## Notch mode

On a MacBook with a notch, the widget can live *in* the menu bar instead of floating below it. Settings → **Sit in the menu bar beside the notch**. The control is disabled on any other Mac.

At rest it is a black band the height of the menu bar, hugging the notch: session counts on the left of it, the five-hour limit's progress bar on the right. The notch is part of the band rather than a hole in it, so the three read as one shape.

![The menu bar with a black band across the notch: amber, red, green, grey and paused dots each with a count on the left, and the limit's bar on the right](docs/media/notch-rest.png)

Hover anywhere on the black and that same element grows — down and out to a third of the display's width — into a list of every session with its status and elapsed time, and the detail of the row under the cursor opens beneath it. There is no popover in this mode; the slab is wide enough to say everything the popover said. Leave it and it collapses back into the menu bar.

![The slab open below the notch: api-service needing input and hovered, its detail showing AskUserQuestion, one background agent, branch and model, and cwd; then web-app working, design-system idle, docs-site paused and infra-tools died; a footer reading 64% of the 5h limit left](docs/media/notch-open.png)

Background agents are counted into the detail of the session that owns them rather than listed beside it — four agents rendered as four more rows buried the three sessions they belonged to.

The open width is a third of the display, bounded to 260–560pt. It does reach across the menu bar extras while open, which is deliberate: the resting band hugs its content and stays clear of them, so nothing sits over your clock unless the cursor is on the widget.

Notch placement takes the display choice out of your hands — the notch decides — so *Show on display* is disabled while it is on, and the widget cannot be dragged. Turn it off and the pill goes back where it was.

## Requirements

- macOS 13 or later
- Xcode Command Line Tools — `xcode-select --install`
- Node 20 or later
- Rust, via [rustup](https://rustup.rs)

## Install

Build it and copy it into Applications:

```bash
npm install
npm run tauri build
cp -R src-tauri/target/release/bundle/macos/clawde-buddy.app /Applications/
```

The build is unsigned, so Gatekeeper blocks the first launch. Try to open it, dismiss the warning, then go to **System Settings → Privacy & Security** and press **Open Anyway** in the block that has just appeared there. Once only. On macOS 14 and earlier, right-clicking the app and choosing **Open** does the same thing in one step. Opening it from the terminal will not get past either prompt.

For a distributable image instead, `npm run dmg` writes `dist-dmg/clawde-buddy_<version>_<arch>.dmg`.

### Development

```bash
npm run tauri dev
```

The frontend hot-reloads; Rust changes trigger a rebuild.

## Using it

There is **no Dock icon and no Cmd-Tab entry** — it is a menu-bar app. The tray icon is the only way in and the only way out:

- **Settings…** — opens a normal window: when to hide the widget, which display to use, whether to sit in the notch, the sound and its three alert events, the 5h limit, background jobs, launch at login
- **Mute alerts 1h**
- **Install update** — installs a newer release if one is available and the updater is configured; otherwise it does nothing
- **Quit clawde-buddy**

It starts at the top centre of the primary display. Pick a different screen under Settings → *Show on display*, or drag the pill anywhere; positions are remembered per display, so docking and undocking a monitor puts it back where you left it rather than off-screen.

The widget floats above fullscreen apps and follows you across Spaces, and clicking it never takes focus from your editor.

Settings → *Hide the widget* takes it off screen when there is nothing to watch: **Never**, **When there are no sessions** (the default), or **When nothing is waiting or working**. The tray icon stays either way, so a hidden widget is never unreachable.

### Alerts

macOS asks for notification permission on the first alert. Decline it and the pill flashes amber until you look at it instead — the signal is not lost.

**Alerts fire on transitions, not states.** A session that is already waiting when the widget starts stays silent, because the first reading is a baseline. Without that, every launch would open with a burst of alerts about things you already knew.

*Play a sound* is the switch for all of this, and the three transitions below sit under it in Settings. An alert is a notification with a sound, so silence means no alert: turning it off writes all three off and greys them out, and turning it back on restores the defaults. `notify::should_deliver` gates on it too, so a config file hand-edited to arm an event under a silent parent still delivers nothing.

Three transitions can interrupt you:

- **A session starts waiting for input.** The notification carries the session's actual pending question, read from its transcript — not just `input needed`. The registry's own reason stands in when the transcript yields nothing.
- **A session dies.** Its process is gone.
- **A session finishes its turn** — busy to idle. Off by default, since a finished turn is the common case and alerting on it is the noisy choice; enable it in Settings. Only that edge counts: answering a question and going quiet is not a finished turn, and a session first seen idle has finished nothing.

Clicking any notification raises that session's window, the same as clicking its popover.

### Settings file

`~/Library/Application Support/com.clawde.buddy/config.json` — plain JSON, hand-editable, every key optional:

```json
{
  "viewMode": "dotRow",
  "placement": "free",
  "alertNeedsInput": true,
  "alertDied": true,
  "alertFinished": false,
  "sound": true,
  "muteUntilMs": 0,
  "launchAtLogin": false,
  "showBackgroundJobs": true,
  "showUsage": true,
  "hideWhen": "noSessions",
  "preferredDisplay": null,
  "positions": {}
}
```

`hideWhen` is one of `never`, `noSessions` or `nothingActive`; anything else falls back to showing the widget. `placement` is `free` or `notch`; anything else reads as `free`, deliberately, since a hand-edited typo must not strand the widget in a placement it cannot be dragged out of. `viewMode` is vestigial — the view modes are gone, and the field is still parsed only so an existing config file keeps loading.

A corrupt or half-written file falls back to defaults rather than refusing to start.

## How it works

Three layers with enforced boundaries: a Rust watcher that owns the data, a Rust bridge for the two things a webview cannot do, and a React frontend that renders precomputed snapshots and derives nothing.

The data source is `~/.claude/sessions/<pid>.json`, the registry Claude Code maintains, plus the session transcript for fields the registry does not carry. clawde-buddy is **strictly read-only** against `~/.claude` — it never writes, moves or deletes anything there.

A few details that are less obvious than they look:

- **Liveness is `kill(pid, 0)` plus a process-start comparison**, because pid numbers get recycled. The comparison is one-sided: only a process *newer* than its registry entry indicates reuse. Claude Code adopts pre-forked spares, so an entry's `startedAt` can be hours after its process began.
- **Process start comes from elapsed time** (`ps -o etime=`), not from the registry's `procStart` string. That string is written in a different timezone than `ps -o lstart=` prints — two hours apart on a CEST machine — so comparing them marks every live session dead.
- **Only `cli` sessions report a status.** A `claude-desktop` entry has no `status` field at all, so for those the transcript's modification time stands in: touched within 30 seconds reads as working, and quiet time ages into idle and then paused.
- **FSEvents plus a 2-second reconcile tick.** FSEvents cannot report that a process died without its file changing, and it coalesces under load.
- **All mouse input is handled in Rust.** The widget is a non-activating `NSPanel`, which never becomes the key window, and WKWebView installs its mouse tracking as `activeInKeyWindow` — so the page receives no `mousemove`, no `:hover` and no `click`. Neither `-webkit-app-region` (Chromium-only) nor Tauri's `data-tauri-drag-region` works here. Instead the cursor and button state are sampled every 60ms and pushed to the page as window-local coordinates, which it hit-tests itself. Hover, the popover, click-to-raise and dragging all run through that.
- **The window never resizes while you interact with it.** It is sized to the widest state and reserves room for a popover, because resizing a transparent panel shows one unpainted frame — which, landing on the start of an animation, was the single largest source of visible stutter. The surrounding transparent margin is click-through so it cannot swallow a click meant for the app behind it.

- **The notch's geometry comes from AppKit, once per change.** `safeAreaInsets.top` identifies a notched built-in display and `auxiliaryTopLeftArea` gives the usable flank; both are main-thread only, so the probe is cached and re-run by a 2-second watcher rather than answered live from a command. Closing the lid takes the notched display away entirely, and the widget has to stop drawing against a screen that is gone.

Jumping to a session walks the process tree to the first executable inside a `.app`, reads its `CFBundleIdentifier`, and runs `open -b`. That needs neither Accessibility nor Automation permission, which is why a fresh install raises windows without prompting for anything.

## Limitations

- **App-level raise only.** Clicking a session brings its editor to the front, not the specific tab. VS Code-family editors expose no tab-targeting API.
- **Unsigned.** The DMG carries an ad-hoc signature, which is what keeps Gatekeeper from calling the app *damaged*, but it is not a notarised one — so Gatekeeper still prompts once per install, and getting rid of that prompt needs an Apple Developer ID to sign and notarize. Update *delivery* is a separate matter and does work: configure a minisign key and the app updates itself in place from the tray menu — see [Signing updates](#signing-updates). With no key, as shipped, it never checks and never updates.
- **A `claude-desktop` session inside a long tool call writes nothing** to its transcript, so it can read as idle until the result lands. It will not reach paused, which needs ten minutes of quiet.
- **Multi-display placement follows the primary display** by default; pick another in Settings if that is not the one you watch.
- **Notch mode assumes the menu bar is where the menu bar is.** A fullscreen app, or *automatically hide and show the menu bar*, leaves the slab at the top edge over the app's own content. It also needs the notched built-in display: close the lid and the widget has nothing to sit in.

## Releasing

Tag and push; the pipeline builds the DMG, uploads it and creates the release.

The release description is the tag's own section of [CHANGELOG.md](CHANGELOG.md),
extracted by `scripts/release-notes.sh` — the same notes go into the updater's
`latest.json`, so the in-app update dialog says what changed too. Write the
section before tagging: a tag with no section still releases, but with nothing
but the download boilerplate in it.

```bash
scripts/release-notes.sh v0.4.0
```

## Tests

```bash
npm test
```

```bash
cd src-tauri && cargo test -- --test-threads=1
```

262 Rust tests and 191 frontend tests. The Rust suite is weighted toward `watcher::state`, where every session state and transition is derived — that function is pure, with the clock, pid liveness and transcript activity all injected, so the whole state machine is tested without touching a filesystem. The watcher-loop tests use real files and real time, hence `--test-threads=1`.

Three environment variables point the widget at fixtures instead of live data, which is how the screenshots on this page were made — the third because the real usage file carries what the account has actually spent, which is neither reproducible nor anyone else's business. Setting `CLAWDE_BUDDY_USAGE_FILE` also stops the live call, which would otherwise put the real figure straight back on screen:

```bash
CLAWDE_BUDDY_REGISTRY_DIR=/path/to/sessions CLAWDE_BUDDY_PROJECTS_DIR=/path/to/projects CLAWDE_BUDDY_USAGE_FILE=/path/to/usage.json src-tauri/target/release/bundle/macos/clawde-buddy.app/Contents/MacOS/clawde-buddy
```

## Releasing

`npm run dmg` builds the installer image with `hdiutil`, deliberately not through Tauri's `dmg` bundler — that one drives Finder over AppleScript to arrange the window and times out without Automation permission, locally and in CI alike. The result installs identically, just without a custom background.

`.gitlab-ci.yml` builds and publishes on a tag: it runs the build, uploads the DMG to the Generic Packages registry and attaches it to a Release. A `.app` cannot be cross-compiled from Linux, so that job needs a macOS runner, and there are two ways to have one:

- **Your own Mac as a project runner** — free, and it already has the toolchain. This is what `MACOS_RUNNER_TAG: macos` in the shipped config expects:

  ```bash
  brew install gitlab-runner
  gitlab-runner register --url https://gitlab.com --executor shell --tag-list macos
  brew services start gitlab-runner
  ```

  Create the runner first under *Settings → CI/CD → Runners → New project runner* with the tag `macos`, then register with the token it gives you. A shell executor ignores `MACOS_IMAGE`, and the job puts Volta, Cargo and Homebrew on `PATH` itself, because a shell executor inherits almost none.

- **GitLab's hosted macOS runners** — Premium or Ultimate only; there is no free tier. Set `MACOS_RUNNER_TAG` to `saas-macos-medium-m1` and keep `MACOS_IMAGE`.

Without any runner, build locally and run `GITLAB_TOKEN=... scripts/publish-release.sh v0.1.0`, which performs the same upload and release creation over the API.

### Signing updates

The app checks for a newer release on launch and only tells you about it;
*Install update* in the tray menu does the install. The updater refuses
anything it cannot verify, so a release needs a minisign keypair. This is
separate from Apple code signing — it secures the update channel, not
Gatekeeper.

```bash
npm run tauri signer generate -- -w ~/.tauri/clawde-buddy.key
```

Put the printed public key in `src-tauri/tauri.conf.json` under
`plugins.updater.pubkey`, which ships empty, and add the private key and its
password to GitLab under *Settings → CI/CD → Variables*, both masked:

- `TAURI_SIGNING_PRIVATE_KEY` — the contents of `~/.tauri/clawde-buddy.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the password you chose

`plugins.updater.endpoints` already points at this project's package registry,
addressed by path rather than by numeric id, so it needs no editing.

**No key is committed, so as shipped the updater is switched off**: with an
empty `pubkey` the plugin is never registered, so the launch check and the
tray item both return without making any network call, and the app stays on
the version you installed.

That keylessness is also why `bundle.createUpdaterArtifacts` is `false` here.
`tauri build` refuses to bundle an update tarball for a public key it cannot
also sign, so leaving it on would break `npm run tauri build` for anyone
without the private key. The tag pipeline turns it on for itself when
`TAURI_SIGNING_PRIVATE_KEY` is set, publishing `clawde-buddy.app.tar.gz`, its
`.sig` and a `latest.json` manifest alongside the DMG; without the variables it
skips all three and releases the DMG alone. To bundle a signed tarball by hand:

```bash
TAURI_SIGNING_PRIVATE_KEY=$(cat ~/.tauri/clawde-buddy.key) \
  npm run tauri build -- --config '{"bundle":{"createUpdaterArtifacts":true}}'
```

## Design documents

The spec and implementation plans this was built from are kept in the repo:

- [Design spec](docs/superpowers/specs/2026-08-25-clawde-buddy-design.md)
- [Implementation plan, v1](docs/superpowers/plans/2026-08-25-clawde-buddy-v1.md)
- [Implementation plan, v2](docs/superpowers/plans/2026-08-25-clawde-buddy-v2.md)
