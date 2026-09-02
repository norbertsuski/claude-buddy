# Fixtures

The widget shows what Claude Code is doing on *your* machine, which is a problem
the moment someone else wants to look at it. A contributor with no sessions
running sees an empty pill, a contributor with three sees their own three, and
neither can reproduce a screenshot or judge whether a change to the paused glyph
looks right. The three environment variables in the root README exist for that,
and this directory is what they point at.

```bash
scripts/dev-fixtures.sh
```

That regenerates the fixtures, exports the three variables and starts
`npm run tauri dev` against them. `scripts/dev-fixtures.sh app` runs the built
bundle instead, and any path to a binary works in place of either.

## The cast

Seven entries, chosen to put every state the widget can draw on screen at once —
the same six the screenshots in the root README show, in the same order.

| | State | Why |
|---|---|---|
| `api-service` | waiting | Reports `waiting` with `input needed`, and its transcript is sitting on an unanswered `AskUserQuestion`, so the popover says what it is waiting for. |
| `migrate-schemas` | working, demoted | A `bg` entry with a `jobId`, which is what makes it a job rather than a session. It repeats `api-service`'s working directory exactly: that is the only link the registry offers between a job and its parent, and without it the arrow and the indent do not happen. |
| `web-app` | working | Reports `busy`, one minute in, on `fix/checkout-totals` with `claude-opus-5` at high effort, mid-`Grep`. |
| `design-system` | idle | Reports `idle`, three minutes quiet — short of the pause threshold. |
| `docs-site` | paused | Twenty-six minutes quiet, which is past the ten the threshold wants. Its transcript ends on prose rather than a tool call, so the popover shows the last thing it said. |
| `test-runner` | tasking | Twenty-six minutes quiet, which would be `paused`, except that its transcript leaves two background shells and a CI watch running. A third task is left finished, which is what shows that the popover lists only what is still going. |
| `infra-tools` | died | Still claims to be `busy`; its pid is 999999, which is above macOS's PID\_MAX and so cannot be anything. Death outranks whatever the registry says. |

Between them they also cover the fields the popover reads out of a transcript
rather than the registry — two models, three effort levels, five branches, a
tool name as the activity and a sentence of prose as the fallback.

Every transcript but one ends on a `custom-title` record, which is where the
name the row shows comes from. `infra-tools` deliberately has none, so the cast
also shows what an untitled session looks like: it falls back to the
folder-derived name in the registry. `docs-site`'s title is longer than the row
will draw, so the clipping is on screen too rather than only in a test.

### The hot cast

`CB_FIXTURE_HOT=1 scripts/dev-fixtures.sh` adds two more working sessions —
`payments-api` and `search-index` — and spends the five-hour limit down to 94%.
It exists because crazy mode has a ramp: fire reaches its top step at three
working sessions and the pill only fractures once the limit is `critical`, and
the default cast has one working session and 36% spent, so it shows the first
rung of an effect and nothing else. The crazy-mode images in the root README are
taken in this state.

It is additive and opt-in. The default cast is byte-for-byte what it was, so
every screenshot taken before it still shows what its alt text says it does.

## What is committed and what is not

`projects/` is committed. It is the transcripts, laid out the way Claude Code
lays them out: one directory per working directory, named after it with every
`/` and `.` flattened to `-`, holding one `<sessionId>.jsonl` per session. The
parser reads four things out of a transcript — `gitBranch`, `message.model`,
`effort` and `message.content` — and none of them is time-sensitive, so these
files can sit in git unchanged.

`sessions/` and `usage.json` are **not** committed. `generate.sh` writes them,
and `scripts/dev-fixtures.sh` runs it every time, because almost everything the
widget derives is relative to now:

- Paused is ten minutes of quiet, busy-without-a-status is thirty seconds of
  transcript writes, and the popover counts the elapsed time out loud. A
  timestamp frozen into a committed file is correct on the day it is written and
  a museum piece a week later — every session would read as paused, or dead.
- Liveness is `kill(pid, 0)` plus a one-sided process-start comparison, so a pid
  has to belong to a process that is genuinely running *and* that started no
  later than the session claims to have. A pid cannot be committed at all: it
  means something different on every machine and on every boot. `generate.sh`
  borrows the oldest processes actually running — old enough to back-date a
  session by hours, stable enough to outlast a screenshot session — and clamps
  each fixture's uptime to the age of the process it borrowed.
- The five-hour meter counts down to an absolute reset instant, and a window
  that has already reset is dropped rather than shown, so a committed
  `usage.json` would show no meter at all.

One transcript is generated rather than committed: `test-runner`'s. Everything
else under `projects/` is committed because none of the fields read out of a
transcript is time-sensitive, and task records are the exception — a task start
is only believed when it is stamped no earlier than the session's own
`startedAt`, which is what stops a resumed session inheriting a dead process's
tasks. A committed timestamp would fall the wrong side of that boundary and the
session would read `paused`.

So the fixture data proper is the cast table at the bottom of `generate.sh`:
one line per session, in relative terms — how long it has held its state, how
long it has been up — and the script turns those into a registry the app will
accept.

## Two things that will look like bugs

The dead session disappears about five minutes in. That is `DEAD_RETENTION_MS`:
a crash is worth showing once rather than forever, measured from when the widget
first saw it. Take the screenshot that needs a red cross early, or restart.

The background job only appears if your own config has `showBackgroundJobs` on,
which is the default. The fixtures cannot override that — the settings file is
read from `~/Library/Application Support`, and no environment variable redirects
it.

## Adding one

Add the transcript first, under `projects/<flattened-cwd>/<sessionId>.jsonl`,
with the record shapes the existing ones use. Then add a line to the cast table
in `generate.sh`: a slot (a number borrows the nth-oldest process's pid, `gone`
gets the impossible one), the session id you just used, a display name, the
working directory the slug was built from, `interactive` or `bg`, a job id when
it is `bg`, the status word and reason, and the two ages in seconds. Run
`./fixtures/generate.sh` on its own to see it stamped without launching
anything.

A session that needs `tasking` is the exception: its task records are only
believed when stamped no earlier than `startedAt`, which is stamped at run
time, so the transcript cannot be a committed file. Write it with a function
like `emit_task_transcript` instead, called from `generate.sh` before the cast
line that needs it, and add its output directory to `.gitignore` alongside
`test-runner`'s.

The states are worth reading out of `src-tauri/src/watcher/state.rs` rather than
guessed at. A session that reports a status is believed; only a statusless one —
a `claude-desktop` entry — falls back to its transcript's modification time,
which `generate.sh` sets alongside the registry file so that path works too.
