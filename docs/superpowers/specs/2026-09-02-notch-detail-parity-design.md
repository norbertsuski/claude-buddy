# Notch row detail: parity with the free-mode popover

## The problem

Notch mode's row detail says four unlabelled things — the current activity, a
count of background agents, `branch · model`, and the working directory. The
free-mode popover says nine, including the list of background tasks a session is
waiting on, which is the field v0.10.0 was built around. Someone using notch
mode gets a strictly poorer answer to "what is this session doing", and there is
no reason for it beyond the two surfaces having been written separately.

The agent count is worse than merely thin. It is derived from
`backgroundCounts`, which counts *rows* in the snapshot list, so it reports zero
whenever "Show background jobs" is off — even though the jobs are still there,
folded into the session's own task list by `state::snapshot` regardless of that
setting (`state.rs`, "that setting governs whether a job gets a row of its
own"). It also never counted background subagents, only registry jobs.

## What the detail shows

Seven labelled fields, in the popover's own order:

```
doing    <the current activity>
tasks    agent · B3 unapplied field guard · 4m
         shell · Run the suite · 12s
session  portal-ui-bridge-99
cwd      /Users/…/portal-ui-bridge
branch   feature/brickworks-personalisations
model    claude-opus-5 · high
proc     claude-desktop · pid 17383 · 2h 14m
```

Running tasks only, the same filter the popover applies: finished tasks stay in
a snapshot for a minute so the alert diff can see them end, and the detail is
about what is happening now. The `tasks` field is absent when there are none.

Under the fields, the hint `click → raise this window`. The behaviour already
exists in notch mode — `NotchFlanks` listens for `ui://click` and raises the
hovered row's session — it has simply never been advertised.

Three popover fields are deliberately not repeated. The head is the row's own
name, the state and its age are the row's own status and elapsed columns, and
the 5h limit has its own footer row. The detail is what the row cannot already
say.

## Structure

`SessionPopover` currently owns the field list, the `session_detail` fetch, the
one-second clock, and the `dash`/`TASK_KIND_LABEL` helpers. All of that becomes
`SessionFields.tsx`:

- `useSessionDetail(session)` — the per-hover transcript fetch, unchanged in
  behaviour. Both surfaces already do this identically.
- `useNow()` — the one-second clock, moved verbatim. The watcher does not
  re-emit for the passage of time, so ages have to tick where they are drawn.
- `SessionFields` — the `dt`/`dd` pairs, rendering a canonical order filtered by
  a `fields` prop. The popover passes all nine; the row detail passes seven.

Both surfaces render the same markup and the same `data-testid`s. The ids keep
their `popover-` prefix: they name the field rather than the surface, and
renaming them would churn a 300-line test file that is not otherwise changing.

Styling stays per surface. `.popover-fields` is unchanged; a new `.notch-fields`
sets the notch's smaller type scale and a narrower label column, and keeps the
34px left inset that lines the detail up under the row's dot.

`backgroundCounts` and the `agents` prop go away with the line they fed.

## Window height

Eight rows, the usage row and the bar come to roughly 344pt, which leaves about
56pt of the 400pt reserve for an open detail. Today's detail is already ~80pt
and the parity one is ~150pt.

`notch::POPOVER_ALLOWANCE` therefore goes to 560. The free-mode constant in
`useWidgetSize.ts` stays at 400: it sizes a different window, and the popover it
reserves for has not grown. The doc comment that claims the two mirror each
other is rewritten to say why they no longer do.

The reserved area is transparent and click-through, so a taller notch window
costs nothing while the slab is closed.

## Testing

Vitest:

- each field renders from a snapshot, and the two surfaces render different sets
- the task list shows running tasks only and drops finished ones
- a session with no running tasks omits the field entirely
- the hint line is present

Rust: the notch window frame reserves 560 below the bar.

`backgroundCounts`'s tests go with it.
