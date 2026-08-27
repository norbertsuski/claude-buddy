# About in the tray menu, and an update item that says what it will do

A standard macOS About panel reachable from the menu bar, plus the existing
update item relabelled to name the version it is about to install.

## Problem

With `LSUIElement` there is no Dock icon, no app-switcher entry, and no app
menu — so there is nowhere the app states its own name, version, or author. The
only way to find out which version is running is to open Settings, which does
not say either.

The update item has a related problem in the other direction. `check_on_launch`
notifies "version 0.7.0 — install it from the menu bar", and the menu the user
then opens says "Check for updates…" — as though the check had not already
happened. The item that installs the update does not admit there is one.

## What it does

```
Hide widget              ✓
Keep screen awake        ✓
Mute alerts              ▸
Show background jobs     ✓
─────────────────
Settings…
About claude-buddy
Install update 0.7.0…      ← "Check for updates…" when nothing is known
Quit claude-buddy
```

"About claude-buddy" opens the standard AppKit about panel: app icon, name,
version, author, licence. The item below it names the waiting version when a
check has found one.

## Design

### About item

`PredefinedMenuItem::about` was the first attempt and does not work here. It is
handled entirely inside muda, which calls
`orderFrontStandardAboutPanelWithOptions:` and returns *without* sending a
`MenuEvent` — only its non-predefined branch does that. So `on_menu_event`
never hears the click and there is nowhere to hook.

The hook is essential, because AppKit builds the panel at
`NSNormalWindowLevel`, and macOS draws an inactive application's normal-level
windows behind the frontmost app. In a menu-bar app that never activates the
panel opens *underneath* whatever the user is looking at: fully realised,
`isVisible` true, opaque, on the active Space — and invisible. Clicking About
looks exactly like clicking a dead menu item.

So the item is an ordinary `MenuItem` with id `about`, and a new
`src-tauri/src/about.rs` does the AppKit work: build the options dictionary,
call `orderFrontStandardAboutPanelWithOptions:`, then
`activateIgnoringOtherApps(true)` to bring the panel forward. `ignoringOtherApps`
rather than a plain activate because the user clicked a menu-bar item belonging
to an app that is not, and does not become, frontmost.

Only the keys macOS honours are set. `objc2-app-kit` exposes constants for
`ApplicationName`, `ApplicationVersion`, `Version`, `Credits` and
`ApplicationIcon`; there has never been an exported `Copyright` key, so that one
goes in by its literal name, as muda also does it.

- name: `"claude-buddy"`
- application version: `app.package_info().version` — which reads
  `tauri.conf.json`, the same single source `chore: bump` moves.
- version: deliberately blank. AppKit renders this one in parentheses after the
  version and both resolve to the same number, so left alone the panel reads
  "Version 0.6.0 (0.6.0)".
- credits: `"Created by Norbert Suski"` and
  `"github.com/norbertsuski/claude-buddy"` on a second line, as an
  `NSAttributedString` — which is what the key documents. Plain text: the panel
  will not linkify a URL, and there is no honoured field that would.
- copyright: `"MIT licensed"`. The panel has no licence field of its own.
- icon: omitted on purpose. With nothing specified AppKit uses
  `NSApplicationIcon` from the bundle, which is the app's own logo and one fewer
  thing to keep in step. An unbundled `tauri dev` run gets the generic icon
  instead — verified against a real `tauri build` bundle, which shows the
  logo.

Present unconditionally. Unlike the update item, it depends on nothing being
configured.

### The update item goes live

`update.rs` gains a `Mutex<Option<String>>` holding the version the last
successful check found newer, `None` meaning "nothing known". Both existing
checks write it:

- `check_on_launch` sets it when `check()` returns `Some`.
- `check_and_install` sets it on `Some` and clears it on `None` — being told
  you are up to date is itself news, and the label must stop advertising a
  version that is now installed.

Each then hops to the main thread to call `tray::refresh`, since every other
menu rebuild does.

The label is a pure function, so it is testable without a menu:

```rust
pub fn update_label(available: Option<&str>) -> String
```

`None` → `"Check for updates…"`, `Some("0.7.0")` → `"Install update 0.7.0…"`.

The menu id and the handler are unchanged. `check_and_install` already installs
when there is an update and reports "up to date" when there is not, so the
label is not promising new behaviour — it is describing which of the two
branches is about to run. No second code path, and nothing to keep in step.

The `is_configured` gate stays exactly as it is: with no signing key the item is
omitted entirely rather than greyed out. That is the 0.6.0 fix for an item that
panicked on a spawned task and looked like a no-op, and it must not regress.

This is also the honest form of the "Install update" item removed in 0.6.0. It
was removed for claiming an action it could not perform; here that wording
appears only once a check has actually found something to install.

## Testing

- `update_label` both ways: `None` gives the check wording, `Some(v)` names the
  version.
- The available-version slot: set, read back, clear.

Neither the panel nor the menu is unit-tested — both are AppKit, and the bug
that shaped this design was invisible to every layer above AppKit: the panel
object existed and reported itself visible throughout. It was found by
breakpointing `-[NSApplication orderFrontStandardAboutPanelWithOptions:]` to
confirm the call arrived, then reading the panel's own `frame`, `level`,
`alphaValue` and `isOnActiveSpace` — `level = 0` was the answer. Note that the
accessibility window list does *not* report the panel, so its absence there is
not evidence that no panel exists; that false signal cost one wrong hypothesis.

Verified manually against a release bundle: the panel shows the app logo,
`claude-buddy`, `Version 0.6.0`, the credits and the licence, and comes to the
front. The menu reads "Install update …" after a launch check finds one and
reverts to "Check for updates…" after installing.

## What this owes

- **README** — "Using it" tray menu list gains About, and notes the update
  item's two labels.
- **CHANGELOG** — user-visible.
