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

At rest, two chips flush against the notch: session counts on the left, the
five-hour limit on the right as a bare progress bar. All three read as one black
shape.

Hovering either chip opens **the slab** — a single black block of one fixed
width, spanning the menu bar and continuing down into a list of every session
with its status and elapsed time. The notch sits inside the slab and disappears.

**This shape was reached by iterating on hardware, not by design.** Three
earlier shapes were built and rejected: chips expanding sideways over the app's
menu titles; chips retracting into the notch with a notch-width list dropping
out; and a 335pt detail card winging out beside that list. The record of why is
kept below, because each rejection is a constraint on anything built here next.

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

The window is `2 × max(notch/2 + FLANK_BUDGET, SLAB_WIDTH/2)` — about 499pt on a
179pt notch — centred on the notch. The resting chips set it, not the slab:
`SLAB_WIDTH` is 340 and a chip reaches 160 beyond the notch's edge, so the chips
reach further from the centre. Either way the window never comes near the Apple
menu or the clock.

The width is constant across every hover state, which is the invariant
`widgetWindowSize` already maintains for the same reason: resizing a transparent
panel shows one unpainted frame, and it lands on the start of the morph.
`POPOVER_ALLOWANCE` is 400pt and already reserved unconditionally, staying
transparent and click-through, so a popover opening resizes nothing.

### The hover rect is whatever is painted black

`cursor.rs` held a single `HOVER_RECT` and `contains` took one `Rect`. An earlier
design flanked the notch with two chips and needed two rects, so `HOVER_RECT`
became a short list, `contains` gained `contains_any`, and `set_hover_rect`
gained `set_hover_rects`. Free mode still reports one box.

The slab reports one box in both states: the band. Reporting the two resting
halves instead — done originally so that sweeping across the notch would not
open the slab — left the black either side of their content dead to the cursor,
so the band responded only where it had something to say. Now the notch gap is
inside the reported box, which is right: the notch is part of the black.

While the slab is open it is the only rect, because it spans the bar as well as
the list — the cursor that opened it is already inside. An earlier design, where
the list was only as wide as the notch and the chips retracted, needed a third
rect spanning the bar for exactly this reason: report only a panel *below* the
bar and the cursor sitting *in* the bar falls outside every rect, shutting the
panel and reopening it on the next 60ms sample, forever.

The slab is described from the geometry and its measured content height, never
from `getBoundingClientRect`. Its height is animated, so at the moment it opens
its box is 0 tall and `visibleRects` discards it as un-laid-out — which produced
exactly the oscillation above.

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

### Counts left, the limit right — no urgency split

The left chip carries every state's count, most urgent first. The right chip
carries the five-hour limit.

**An urgency split was tried and removed.** Waiting and dead on the left, the
rest on the right, forced two rules that existed only to prop it up: a background
job had to be walked onto its parent's chip to keep its continuation arrow
meaningful, since `SessionSnapshot` has no parent field and parentage is only the
nearest own session earlier in the list; and a chip had to count states its side
did not nominally carry, or a job that crossed the split was counted nowhere.
With one chip neither arises — order is the order it arrived in, and adjacency is
free.

In the bar the limit is the bar and nothing else: no percentage, no countdown.
The label took the chip to roughly 96pt, which reached the first menu bar extra
on a 1470pt panel. The track alone is 34pt, and both figures are spelled out in
the slab's footer.

### A detail opens and closes rather than appearing

The hovered row's detail is wrapped in a box whose height is measured and
written inline, transitioned over 200ms. `auto` cannot be transitioned, and the
activity line arrives from `session_detail` after the row is hovered, so the box
is re-measured with a `ResizeObserver`.

The row being left stays mounted until it has finished collapsing —
`DETAIL_MORPH_MS`, mirrored in the CSS. Unmounting it on the frame the next one
opens snapped every row below up by the old detail's height and then eased them
back down by the new one's, from a cursor that had moved by one row. Its 200ms
is shorter than the band's own `--morph`: the band follows this, and a follower
slower than what it follows reads as lag rather than as one movement.

The highlight moves faster still, at 140ms, because it is what confirms the
cursor moved and must not wait for the box.

### Hovering either chip opens the slab

One `cursor.inside` boolean already exists and drives the whole thing. Per-side
hover would mean two, plus a decision about what happens when the cursor crosses
the notch between them.

The chips do not move. The slab is wider than both and is painted over them, so
nothing has to slide out of the way.

### The slab is one fixed width: a third of the display

`auxiliaryTopLeftArea.width` gives the flank's total width, not its free width.
Where the frontmost app's menus end is unobservable without Accessibility
permissions, and changes on every app switch — so occlusion cannot be avoided by
measuring, only by picking a width and checking it.

The open width is `display_width / 3`, bounded to 260–560pt — 490 on a 1470pt
panel, spanning 490 to 980 about a notch centred at 735.

It does reach under the menu bar extras, which start at logical 910 on the panel
this was measured on. Accepted: the width applies only while the cursor is on
the widget, so the slab is over them for as long as it is open and off them the
moment it closes — the same trade the resting chips already make against the
app's menu titles.

A narrower share was tried first: 4.3, the widest that clears 910, giving 342.
Rejected on hardware. It put the open width within 30pt of the resting band's
313, so the slab appeared to shift sideways rather than grow out of the notch —
too small a change to read as an expansion, large enough to read as a jump. The
two widths have to be visibly different for the animation to say what it means.
It also made a row barely wider than the notch itself, which read as the hover
only working behind the notch.

A share rather than a constant so it travels between displays, bounded because a
third of a large display is far wider than any row needs.

**The resting band is not fixed** — it hugs its content, which keeps it clear of
the extras without needing to know where they are. Only the open width is
constant, so the slab is the same size however much there is to say.

**Why not read menu extents.** An `AXUIElement` query on every app switch buys
exact clamping in exchange for an Accessibility permission prompt on first run
and a denied-permission fallback path. Not worth it for a fixed width that
already clears both sides.

### Chips are styled as app chips, not as native menu items

The chips reuse the pill's existing `--bg`, `--border` and radius, scaled down.

**Revised after testing on hardware.** They were flush against the notch, with
the notch-facing corner squared and its border removed, so the chip read as
growing out of it. `--bg` is near-black, and a near-black chip touching a black
notch with no edge between them is invisible — the same failure that ruled out
solid black, one step removed. The chips now sit 7pt clear of the notch, fully
rounded and bordered all the way round.

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

A row costs height, not width, and 400pt is already reserved below the bar, so
`MAX_ROWS` is 8 before the tail collapses into `+N more`. At 340pt a row holds a
full session name, its status and its elapsed time — `dependencies-path-coverage`
fits without an ellipsis, which a 179pt list could not manage.

**This is why there is no popover in notch mode.** A 335pt card winging out
beside a notch-width list was built and then removed: once the list itself is
340pt, the row carries what the card carried. The name still truncates before the
status and elapsed time, both of which are short and fixed.

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
