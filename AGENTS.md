# Working on claude-buddy

A macOS menu-bar widget that watches local Claude Code sessions. Rust
(Tauri v2) in `src-tauri/`, React 19 and TypeScript in `src/`. macOS only —
there is no other platform hiding behind a feature flag.

[CONTRIBUTING.md](CONTRIBUTING.md) is the full guide and has a section written
for agents specifically. Read it before anything non-trivial. What follows is
the short list of things that cause damage when you do not know them.

## The tree may be shared

More than one agent session can be running against this checkout at once, and
they cannot see each other.

- Run `git status` before you start and again before you stage.
- A file you did not touch showing as modified means someone else is mid-edit.
  Leave it alone. Do not revert it, and do not "fix" a compile error that
  belongs to work in flight — say what you found instead.
- Stage explicit paths. Never `git add -A`, never `git commit -a`. In a shared
  tree those commit someone else's half-finished work under your message.

## Never commit real session data

The registry and transcripts this app reads contain whatever their user was
actually working on — private repository names, branches, conversation prose,
and an account's real spend. `fixtures/` exists so none of that has to leave
the machine. `scripts/dev-fixtures.sh` runs the widget against it and is the
only acceptable source for a screenshot.

## Before you commit

```bash
npm run typecheck && npm test
cd src-tauri && cargo fmt && cargo test -- --test-threads=1
```

`--test-threads=1` is not optional: the watcher-loop tests use real files and
real wall-clock time and interfere with each other in parallel.

Do not reformat files you are not changing. A tree-wide `cargo fmt` is a
legitimate change, but it belongs in its own commit when nothing else is in
flight. The frontend has no formatter configured, so match the surrounding file
rather than imposing one.

## What a change owes

- **README.md** — if the app now behaves differently for a user. CONTRIBUTING
  has a table mapping the kind of change to the section it lands in. This is
  the step that gets skipped, because nothing fails when it is.
- **CHANGELOG.md** — if the change is user-visible. Release notes and the
  in-app update dialog are both lifted straight out of it.
- Neither, if the work is internal: a refactor, a test, tooling, docs.

## Conventions

Conventional commit subjects (`feat:`, `fix:`, `docs:`, `chore:`, `ci:`,
`refactor:`, `perf:`), no scopes, and a body explaining the reasoning. Keep the
`Co-Authored-By:` trailer — attribution should be accurate about how the code
was written.

Comments explain *why*, never *what*: the constraint that is not visible from
the code, the bug a line exists to prevent, the approach that was tried and did
not work.

## Where to look first

Since the split, the widget's provider-agnostic half lives in
[buddy-core](https://github.com/norbertsuski/buddy-core) and its shared React
surface in [buddy-ui](https://github.com/norbertsuski/buddy-ui), both consumed
as tag-pinned git dependencies. What stays here is what knows about Claude Code
specifically: the registry reader, the transcript parsing, the title and task
probes, the usage meter, and the watcher loop that wires them together.

So the file below is in buddy-core now, not this repo — clone it as a sibling
and run `scripts/dev-core.sh` to work against it. `buddy-core/src/watcher/state.rs`
— `snapshot()` derives every session state and
transition. It is pure, with the clock, pid liveness and transcript activity
injected as traits that each have a fake beside the real one, so almost every
behaviour question resolves there and almost every new test belongs there.
CONTRIBUTING.md has a fuller orientation map.
