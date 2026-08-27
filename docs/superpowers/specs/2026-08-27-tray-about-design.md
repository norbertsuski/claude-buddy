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

```rust
PredefinedMenuItem::about(app, Some("About claude-buddy"), Some(metadata))
```

`tauri::menu::AboutMetadata::icon` takes a `tauri::image::Image`, so
`app.default_window_icon().cloned()` — the same call that supplies the tray
icon — provides the logo with no new bundled asset.

Only four fields are set, because on macOS only four are honoured. muda 0.19.3
maps `name`, `version`, `short_version`, `copyright`, `icon` and `credits` onto
`NSAboutPanelOption*` keys and passes them to
`orderFrontStandardAboutPanelWithOptions`. `authors`, `comments`, `license` and
`website` exist on the struct and are silently dropped on this platform. The
code carries a comment saying so, so that nobody later "completes" the metadata
by adding `website` and spends an afternoon wondering why it never appears.

- `name`: `"claude-buddy"`
- `version`: `app.package_info().version` — which reads `tauri.conf.json`, the
  same single source `chore: bump` moves. Nothing about the version is
  duplicated here.
- `credits`: `"Created by Norbert Suski"` and
  `"github.com/norbertsuski/claude-buddy"` on a second line. Plain text — the
  panel will not make it clickable, and `website`, which would have, is one of
  the dropped fields.
- `copyright`: `"MIT licensed"`. `license` is dropped on macOS, so `copyright`
  is the only honoured field left to say it in.

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

Neither the panel nor the menu is unit-tested — both are AppKit. Verified
manually: the panel shows icon, name, the version from `tauri.conf.json`, and
the credits; the menu reads "Install update …" after a launch check finds one
and reverts to "Check for updates…" after installing.

## What this owes

- **README** — "Using it" tray menu list gains About, and notes the update
  item's two labels.
- **CHANGELOG** — user-visible.
