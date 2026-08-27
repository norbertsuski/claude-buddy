# Crazy mode

An opt-in setting that lets the widget dramatise what it is already showing:
the pill catches fire as sessions go busy, shakes as one waits, fractures as
the five-hour limit runs down, and crumbles when a session dies.

## Problem

The widget's whole vocabulary is five dots and a summary line. That is enough
to answer "is anything waiting on me" at a glance, and deliberately so — it
lives in the menu bar and must not shout.

But it is also the only register it has. Three agents working flat out and one
agent idling look the same apart from the dot colours, and a five-hour limit at
94% looks the same as one at 12% apart from a 34px bar. Nothing about the
widget conveys *intensity*, and intensity is real information: it is the
difference between a quiet afternoon and four sessions competing for the same
rate limit.

Crazy mode is that register. It is off by default and always will be — the
calm widget is the correct default, and this is for people who want the machine
to look like it is working as hard as it is.

## What it does

One new setting, **Crazy mode**, a `<select>` in the settings panel. `Off` on a
fresh install and after upgrade. When set to `Ember`, four effect families
switch on, each driven by a signal the widget already receives:

| Family | Driven by | What it does |
| --- | --- | --- |
| Fire | count of busy foreground sessions | glow, flames along the pill's bottom edge, sparks |
| Jitter | longest wait among waiting sessions | the pill shakes, harder the longer the wait |
| Strain | how much of the five-hour limit is gone | the pill fractures; the meter goes molten |
| Ash | a session dying | the dead cross breaks apart and flakes fall, once |

They are separate on purpose. A single blended "heat" number would say the
widget is agitated without saying why, which is less than the dots already
tell you. Keeping one visual language per signal means crazy mode *adds*
information rather than trading it for spectacle.

## Design

### The dial is a ceiling, not a driver

Load decides how intense the widget looks. The dial decides how far that
intensity is allowed to reach:

- **`off`** — today's widget, byte for byte. No extra DOM is mounted.
- **`ember`** — every effect painted inside the pill. Ships in this work.
- **`blaze`** — effects escape the pill into the window's shadow padding.
  Designed below, not built.
- **`inferno`** — a screen-wide overlay. Designed below, not built.

`CRAZY_LEVELS` in `src/types.ts` lists only the levels that exist, so the
`<select>` never offers a dead option. It gains entries as levels land.

### Heat — new `src/views/dotRow/heat.ts`

Everything crazy mode needs is already in the `Update` payload. Nothing in
`src-tauri/src/watcher/` changes; `snapshot()` is untouched.

```ts
export interface Heat {
  /** Busy foreground sessions, capped at 3. */
  fire: 0 | 1 | 2 | 3
  /** 0 at 30s of waiting, 1 at five minutes, linear between. */
  jitter: number
  /** 0 normal, 1 warn, 2 critical. */
  strain: 0 | 1 | 2
  /** Sessions that died in this update, from `died` alerts. */
  ash: readonly string[]
}

export function deriveHeat(
  sessions: readonly SessionSnapshot[],
  usage: Usage | null,
  alerts: readonly Alert[],
): Heat
```

Pure, clock-free, no DOM — the same shape as `visibility::should_hide` and
`watcher::state::snapshot`, and tested the same way. This is where every
future "should it burn when X?" question gets answered, and where the tests
for it belong.

**Background jobs do not feed the fire.** They are already demoted to 0.55
opacity because they are work you did not start; setting the widget alight for
a subagent would be the same mistake in a louder voice. `fire` counts only
sessions with `background === false`.

**Ash is keyed off alerts, not state.** A dead session stays `dead` for as
long as it is listed, which could be hours, but dying happens once. The `died`
alert *is* that moment, and it already arrives exactly once. `ash` carries the
session ids from this update's `died` alerts and is empty on every other tick.

### Fire

`fire = min(3, busy foreground sessions)`. Nothing mounts at 0.

| | glow | border | flames | sparks | breathe |
| --- | --- | --- | --- | --- | --- |
| 1 | `rgba(255,140,50,.15)` | `rgba(255,150,70,.24)` | opacity .5, 18px | — | — |
| 2 | `rgba(255,120,30,.30)` | `rgba(255,140,50,.44)` | opacity .85, 20px | 2 @ 3.0s | 2.6s |
| 3 | `rgba(255,96,16,.52)` | `rgba(255,130,40,.70)` | opacity 1, 23px | 4 @ 1.6s | 1.5s |

Eight flame elements and four spark elements are mounted whenever `fire >= 1`
and stay mounted, with opacity, height and animation duration carrying the
ramp. A fixed element count means changing level never remounts DOM, and the
worst case is bounded at twelve animated nodes.

Flames are clipped to the pill by the `overflow: hidden` already on `.pill`,
which is there so the collapsed/expanded variants reveal as the box grows.

### Jitter

```
jitter = clamp((longestWaitMs - 30_000) / 270_000, 0, 1)
```

Zero for the first thirty seconds — a session that has just asked a question
does not need the widget to panic — then ramping to full at five minutes.

The whole pill shakes, not just the waiting dot. Amplitude is
`--amp: jitter * 1.4`, and the keyframes translate by at most `±amp` on each
axis, so peak displacement is **1.4px**. `--shadow-pad` is 30px, so the shake
stays far inside the window and `useWidgetSize` needs no change: no resize, no
clipping, no morph interaction.

**The shake stops while the pointer is over the widget.** Entries have hover
states and open popovers; a pill shaking under the cursor makes hovering a
moving target. By the time you are pointing at it you have already noticed the
thing that is waiting, so the shake has done its job.

### Strain

`strain` maps `usage.severity`: `normal` → 0, `warn` → 1, `critical` → 2. When
`usage` is null — which is most of the time, since the cache Claude Code writes
is refreshed only when it actually fetches usage — `strain` is 0.

Two effects, deliberately overlapping:

**Cracks on the pill**, at `strain >= 1`. Five fracture paths across the pill,
group opacity .35 at strain 1 and .8 at strain 2. Each path is drawn **twice** —
a `rgba(20,6,2,.75)` underlay at 2.2px, then a `rgba(255,236,208,.75)`
hairline at 0.8px on top. One stroke is not enough: a light crack disappears
against the flames and a dark one disappears against the pill.

Cracks render **above the fire layers but below the dots and summary text**.
The mockup drew them above everything and read well at the sizes tested, but
the pill's width varies with content and a crack landing across a glyph run at
a narrow width would cost legibility for decoration. Under the content they
still read as damage to the pill's surface.

At `strain === 2` the pill also shudders: `±0.6px` on a 3s cycle, so it reads
as strain rather than the fast agitation of jitter.

**A molten meter**, at `strain === 2`, and only when `showUsage` is on. The
fill becomes a `#ff6a10 → #fff0c9` gradient with a pulsing glow, and a droplet
falls from its leading edge every 2.2s.

The overlap is the point. `showUsage` is a setting, and it is off for plenty of
people — if strain lived only on the meter it would silently do nothing for
them. Cracks carry the signal either way; the molten meter is the detail you
get when the meter is there to strain.

### Ash

When a session id appears in `heat.ash`, its dot plays a single 1.4s sequence:
the two bars of the dead cross rotate apart and fall, three grey flakes drift
down, and the dot settles back to the ordinary dead cross it draws today. No
residue, no loop.

`animation-iteration-count: 1`. A dead session can sit in the list for hours,
and an animation running that whole time for something you have already seen
and cannot act on is noise, not signal.

### Interaction rules

The families are independent, but they share pixels. Three rules keep them
from competing:

1. **`fire >= 2` drops crack opacity to 45% of its value.** Fire owns the pill
   at that point; the cracks stay legible without fighting it.
2. **`strain >= 1` shortens flames by 4px.** Cracks read better with less
   blur behind them.
3. **Hovering the widget stops both the shake and the shudder.** As above.

Three CSS selectors and two multipliers. No new machinery.

### Where the animations live

`.pill` already owns an `animation` — `flash-attention`, via
`.dot-row[data-flashing='true']`, when a session needs input. That flash
outranks anything here and must keep its element, and the CSS `animation`
shorthand does not compose across rules.

So the transforms get their own wrappers, one animation each:

```
.dot-row[data-flashing]        flash-attention (box-shadow) — unchanged
  └── .crazy-shake             jitter        (transform)
      └── .crazy-shudder       strain 2      (transform)
          └── .pill            morph + flash — unchanged
              ├── .crazy-heat  glow + breathe          z 1
              ├── .crazy-flames                        z 2
              ├── .crazy-spark ×4                      z 3
              ├── .crazy-cracks                        z 4
              └── entries, summary, usage              z 5
```

Both wrappers are plain inline-blocks, mounted whenever `crazy !== 'off'` and
carrying an animation only while their effect is live. Mounting them once
rather than per-effect keeps the pill from being remounted as jitter or strain
comes and goes, which would restart the box morph mid-flight. At `off` neither
exists and the pill sits where it always did.

Effects live in a new `src/views/dotRow/crazy.css`, imported alongside
`dotRow.css`. The base file is heavily commented load-bearing layout — window
sizing, morph timing, the notch anchor — and burying fire keyframes in it would
make both harder to read.

### Reduced motion

`prefers-reduced-motion: reduce` caps crazy mode rather than killing it. The
ramp still runs 0→3 and still says what it says; only the moving half goes.

| Kept | Dropped |
| --- | --- |
| glow, border warmth, background ramp | flames, sparks, breathe |
| cracks, still dimmed by rule 1 | shake, shudder |
| molten gradient on the meter | meter glow pulse, the drip |
| the settled dead cross | the crumble sequence |

Flames are `display: none`, not frozen: a static blurred smear reads as a
rendering fault rather than a design. Same for sparks and the drip.

### Settings

`Config` gains `crazy: String`, serialised `crazy`, defaulting to `"off"`,
mirroring `hide_when`'s string-id pattern. `Config` is `#[serde(default)]`
throughout, so an existing settings file loads unchanged. Mirrored in
`src/types.ts` with a `CRAZY_LEVELS` const for the `<select>`.

The control sits in `SettingsPanel` immediately after "Show the 5h limit at the
end of the row" — it is about how the row looks, and belongs with the other
appearance toggles.

**No tray item.** The tray was recently curated into three groups by purpose,
and a level submenu would undo that. This is a set-and-forget preference, not a
quick toggle like mute or keep-awake.

## Cost, and why it is small

Nothing animates on an idle machine. Every effect element mounts only when its
family is live, so a widget showing two idle sessions with crazy mode on runs
exactly as many animations as one with it off: none.

Everything animated is `transform`, `opacity` or `filter`, which composite
without layout. The one exception is `breathe`, which uses
`filter: brightness()` and repaints its layer. It is confined to the
`.crazy-heat` element — not the pill — so the repaint is one small layer, and
if it still shows up it becomes an opacity crossfade between two static glow
layers instead.

The worst case is twelve fire nodes, ten crack strokes and two transform
wrappers, all CSS-driven. No `requestAnimationFrame`, no canvas, no React
re-render per frame.

## What is designed but not built

`blaze` and `inferno` are specified here so the heat number and the dial are
built to carry them, not so they ship now.

**`blaze`** lets embers drift up past the pill and a heat halo bleed past the
border. `useWidgetSize` sizes the window from `getBoundingClientRect`, so a
larger `--shadow-pad` is close to a token change — but anything painted beyond
the pad is clipped square at the window edge and reads as a grey box, not a
glow. The real obstacle is placement: in notch mode the pill is anchored
against the menu bar with nothing above it, so blaze must send its embers
downward there or sit that placement out. That decision belongs to whoever
builds it.

**`inferno`** puts fire along the whole menu bar, scorches the top of the
screen and casts warm light down over the desktop. It needs a second
borderless, click-through, always-on-top window spanning the display, at a
window level above the menu bar, with its own lifecycle and display targeting.
It repaints screen-width for as long as anything is busy, and its battery cost
is a different order from everything above. It is bigger than `ember` and
`blaze` combined.

## Testing

`heat.test.ts` carries the behaviour, and is where a change to any of this
should be pinned first:

- `fire` counts busy sessions and caps at 3
- `fire` ignores `background === true` sessions entirely, including when they
  are the only busy ones
- `jitter` is 0 below 30s, 1 at and beyond five minutes, and linear between
- `jitter` reads the *longest* wait when several sessions are waiting
- `strain` maps each severity, and is 0 when `usage` is null
- `ash` lists ids from `died` alerts only, and is empty when a session is
  merely in the `dead` state without a fresh alert

`DotRow.test.tsx`:

- `crazy: 'off'` mounts no `.crazy-*` element and no wrapper
- `crazy: 'ember'` mounts the wrappers and layers the heat calls for
- the hover state that suppresses the shake is applied where expected

`SettingsPanel.test.tsx`: the select renders every level in `CRAZY_LEVELS` and
saves the chosen one.

Rust: `config.rs` round-trips `crazy`, and a settings file written before this
change loads with `crazy == "off"`.

The *look* is not unit-testable. It is verified by eye through
`scripts/dev-fixtures.sh`, which is also the only acceptable source for a
screenshot — the real registry contains private repository names, branches,
conversation prose and an account's real spend.

## What this owes

- **README.md** — the widget behaves differently for a user who turns this on.
  A new setting and what each level does.
- **CHANGELOG.md** — user-visible. Release notes and the in-app update dialog
  are lifted straight from it.
