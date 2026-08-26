# Notch mode — status counts flanking the MacBook notch

An optional placement for the widget on notched MacBook displays. Instead of
floating over the desktop where the user dragged it, the widget splits into two
chips that sit in the menu bar, each flush against one edge of the notch, and
expand outward on hover.

## Problem

The widget's default placement is top-centre of the primary display with a 12pt
margin, which puts it just below the menu bar — close to the notch already, but
unaligned with it and occupying desktop the user might want. On a notched
MacBook the two strips of menu bar either side of the notch are the emptiest
real estate on the screen, and the app already has everything needed to draw
there: `PANEL_LEVEL` is 26, one above `NSStatusWindowLevel`, and the panel joins
all Spaces with `FullScreenAuxiliary`.

The space immediately beside the notch is also the *least contested* part of the
menu bar. App menu titles fill the left flank from the left edge inward; menu
bar extras fill the right flank from the right edge inward. Both grow away from
the notch, so the notch edges are what is left over.

## What it looks like

At rest, two chips. The left chip carries the counts of sessions that want
something from the user, the right chip carries the counts of sessions that are
merely running. Each chip's inner edge is flush with the notch.

Hovering either chip expands both outward, away from the notch, replacing counts
with session names. Hovering a name opens the existing popover, which drops out
of the menu bar onto the desktop below because 335 × ~100pt does not fit in a
menu bar of any height.

## Decisions

### Placement is a config field, not a display preference

`Config` gains `placement: String`, `"free"` by default, with `"notch"` as the
only other value. Serde's existing `default` attribute means old config files
parse unchanged.

In notch mode `preferred_display` and the saved `positions` entries are ignored
rather than overwritten, so turning notch mode off restores the widget to
wherever the user last dragged it.

### Notch mode is hard-gated on a notched built-in display

Detection is `NSScreen.safeAreaInsets.top > 0`, which is true only on a notched
built-in panel. No `CGDisplayIsBuiltin` check is needed.

The settings toggle is disabled with a reason label when no notched display is
attached. If the config says `notch` and no notch is present — clamshell, lid
closed, external-only — the widget falls back to free placement and **leaves the
config value alone**, so reopening the lid restores notch mode without the user
re-enabling it.

**Why not generalise to any menu bar.** The tuck-and-flank behaviour would work
under a 24pt menu bar on an external display too, but the flank widths have no
notch to derive from, so the geometry would need a second rule and the test
matrix roughly doubles. Deferred.

### Geometry is derived from AppKit, never hardcoded

```rust
pub struct NotchGeometry {
    pub screen_origin: (f64, f64),
    pub notch_x: f64,
    pub notch_width: f64,
    pub bar_height: f64,
    pub left_flank: f64,
    pub right_flank: f64,
}
```

`bar_height` is `safeAreaInsets.top` — measured at 32pt on a 13" M4 Air at
1470x956, not the 37pt this design assumed from the 14"/16" Pro. It is read
rather than hardcoded, so the assumption never reached the code, but no number
in this document should be treated as fixed. `left_flank` and
`right_flank` come from `auxiliaryTopLeftArea.width` and
`auxiliaryTopRightArea.width`; `notch_width` is the screen width minus both.
`notch_x` is `left_flank`, so an off-centre notch is handled without assuming
symmetry.

`probe()` converts NSScreen's bottom-left, y-up frame into the top-left, y-down
space Tauri positions windows in, so nothing downstream deals with two
coordinate systems.

### The window spans the notch plus both budgets, and no more

```
W      = notch_width + 2 * FLANK_BUDGET
origin = screen_origin + (notch_x + notch_width / 2 - W / 2, 0)
height = bar_height + POPOVER_GAP + POPOVER_ALLOWANCE + shadow_pad_bottom
```

`FLANK_BUDGET` is 240pt, giving a window about 670pt wide centred on the notch.
It therefore never covers the Apple menu or the clock at all.

The width is constant across every hover state, which is the invariant
`widgetWindowSize` already maintains for the same reason: resizing a transparent
panel shows one unpainted frame, and it lands on the start of the morph.
`POPOVER_ALLOWANCE` is 400pt and already reserved unconditionally, staying
transparent and click-through, so a popover opening resizes nothing.

### Two hover rects, not one

`cursor.rs` holds a single `HOVER_RECT` and `contains` takes one `Rect`. Two
chips flanking a notch need two, and a union would bridge across the notch and
make hovering the notch itself read as hovering the widget.

`HOVER_RECT` becomes a short list, `contains` gains a `contains_any`, and
`set_hover_rect` becomes `set_hover_rects` taking a slice. Each chip contributes
one rect with `top = 0` and `height = bar_height`, its inner edge on the notch
edge and its width measured from content; the open popover contributes a third.

**Why not two panels.** The cursor watcher, the press-gesture state machine,
`resize_widget` and `configure_panel` are all written against one window labelled
`widget`. Two panels means two of each. Extending the rect list is a change to
two functions.

### Clicks pass through, already

`cursor.rs` calls `panel.set_ignore_mouse_events(!next.inside)` on every
inside/outside transition, so the window is transparent to the mouse except when
the cursor is on a reported rect. Menu titles and menu bar extras underneath
stay clickable without further work.

Known residual: the poll is 60ms, so a click that lands within one sample of the
cursor crossing a chip boundary can still be swallowed.

### Left/right split is by urgency

Left chip: `Waiting ∪ Dead`. Right chip: `Busy ∪ Idle ∪ Paused`. Background jobs
sort by their own state and remain subject to `show_background_jobs`.

A side with nothing on it renders no chip at all, so a quiet machine reads as
deliberately asymmetric and a busy one as symmetric.

### Hovering either chip expands both

One `cursor.inside` boolean already exists and drives the whole morph. Per-side
hover would mean two, plus a decision about what happens when the cursor crosses
the notch between them.

### Expansion occludes, within a fixed budget

`auxiliaryTopLeftArea.width` gives the flank's total width, not its free width.
Where the frontmost app's menus end is unobservable without Accessibility
permissions, and changes on every app switch.

So expansion is capped at `FLANK_BUDGET` per side and occludes whatever is under
it for as long as the cursor is there. In a menu-heavy app such as Xcode the left
chip covers the tail of the menu titles while hovered.

**Why not read menu extents.** An `AXUIElement` query on every app switch buys
exact clamping in exchange for an Accessibility permission prompt on first run
and a denied-permission fallback path. Not worth it for an occlusion that lasts
as long as a hover.

### Chips are styled as app chips, not as native menu items

The chips reuse the pill's existing `--bg`, `--border` and radius, scaled down.

**Why not vibrancy.** The macOS menu bar is translucent over the wallpaper, so a
chip that wants to look native needs an `NSVisualEffectView` with the
`.headerView` material behind the webview, positioned to each flank rect and
repositioned whenever those rects change — and its text has to invert on light
wallpapers to stay readable, as the real menu bar's does. An app-styled chip is
correct on any wallpaper with no native work, and it occludes covered menu titles
cleanly because it has a real background.

**Why not solid black.** Correct beside the notch, where the bar reads
near-black; wrong the moment it expands over a light wallpaper, and expansion is
the whole interaction.

### The notch scale hangs off the chip, not off an ancestor

`dotRow.css` states that its sizes are the design's base values scaled by 1.25.
The bar allows about 24pt of content height after padding, against the pill's
current 76pt, so notch mode needs the same tokens at a smaller multiplier —
12px text and 8px dots — rather than a second set of values.

The overrides are scoped to `.flank-chip`, the element `FlankCluster` itself
renders, not to a `.notch-flanks` ancestor. Scoping them to the wrapper made the
scale depend on an ancestor the component neither owns nor requires: rendered
anywhere else it silently fell back to free-mode 11px dots, and the busy dot's
`box-shadow: 0 0 0 4px` glow — 19px across — clipped against the chip.

`--shadow-pad` is 30px on all sides of `body` and feeds the window sizing. The
chips must sit flush at `y = 0`, so notch mode zeroes the top padding while
keeping the rest for the popover's shadow.

### Capacity is about three names per side

Measured against the real stylesheet rather than estimated: an expanded entry
group is 72–91pt, so three of them plus the overflow marker wanted 313pt. The
budget is 240pt and each chip shows **two** names before `+N`, which measures at
222pt worst case. Named capacity in notch mode is about 4 across both sides, with
the popover carrying detail.

Two further consequences of measuring, both of which the estimate hid:

- The overflow marker sits at the outer end of the chip, which is the end
  `overflow: hidden` eats first — so the one element that says sessions are
  hidden was the first thing to disappear. It is `flex: none`, and the entries
  beside it shrink instead.
- Nothing bounds a session name; it comes from the user's repo. `.entry-name` is
  capped at 72px with an ellipsis, or one long name pushes the chip past its
  budget however few entries are allowed.

### Drag is suppressed in notch mode

Position is derived, so a drag would move the chips somewhere the next placement
pass would undo — or worse, leave them somewhere with no way back.

## Structure

New Rust module `notch.rs`, one AppKit call and the rest pure:

```rust
pub fn probe() -> Option<NotchGeometry>;
pub fn flank_rects(geo: &NotchGeometry, budget: f64) -> (Rect, Rect);
pub fn window_frame(geo: &NotchGeometry, budget: f64, popover: f64) -> (Point, Size);
```

`flank_rects` and `window_frame` are pure, so the geometry is tested without a
display attached.

Changed Rust:

- `window.rs` — `restore_position` short-circuits to `place_in_notch` under notch
  placement, bypassing `choose_display_key` and `monitor_key` entirely. Matching
  an `NSScreen` to a Tauri `Monitor` by name would have been the most fragile
  part of the feature and is avoided outright.
- `cursor.rs` — rect list, `contains_any`, drag suppression.
- `config.rs` — one field.
- `visibility.rs` — **unchanged**. Rest state is visible counts, so `should_hide`
  already does the only hiding this design has.

New frontend:

- `FlankCluster.tsx` — one chip. Takes a `side`, renders counts collapsed and
  entries expanded, growing away from the notch.
- `NotchFlanks.tsx` — composes two clusters and the popover.
- `useNotch.ts` — geometry and mode from Rust.

Changed frontend: the entry markup lifts out of `NamedDotRow.tsx` into a shared
`SessionEntry` so both modes render entries identically.

## Display changes

Nothing currently observes monitors appearing or disappearing. `restore_position`
runs at startup, on config apply, on show, and on resize. In free mode the next
resize re-clamps; in notch mode closing the lid would park the window on a
display that no longer exists, with drag suppressed and no way to recover it.

Notch mode therefore adds an `NSApplicationDidChangeScreenParametersNotification`
observer that re-probes and re-places. This also closes the same latent gap in
free mode.

## Accepted limitations

Both are worth a follow-up rather than a fix now, and both are consequences of
drawing in the menu bar at all:

- A fullscreen app hides the menu bar. The panel already draws over fullscreen
  apps via `FullScreenAuxiliary`, so the chips remain visible and sit at the top
  edge over app content.
- The "automatically hide and show the menu bar" setting has the same effect
  whenever the bar is retracted.

Neither is new behaviour — the widget floats over fullscreen apps today — but at
`y = 0` it is more intrusive than where the user places the pill themselves.

## Tests

- `notch.rs` — `flank_rects` and `window_frame` as pure functions, including an
  off-centre notch and a zero-width flank.
- `cursor.rs` — `contains_any` across two rects and the gap between them; drag
  suppressed under notch placement.
- `config.rs` — `placement` defaults to `free`; an unrecognised value falls back
  to `free` rather than making the widget unplaceable.
- Frontend — the urgency split, per-side overflow to `+N more`, an empty side
  rendering no chip, and popover clamping at both flank edges.

The existing 225 Rust and 102 TypeScript tests stay green.
