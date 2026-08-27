# Contributing

Thank you for looking. clawde-buddy is a small project with one maintainer, so
the rules below are short — but they are written out with their reasons rather
than as a list, because a contributor who knows *why* a rule exists will get the
cases it did not anticipate right too.

If you are an AI agent, there is [a section for you](#if-you-are-an-ai-agent) further down. Read the rest of this file too — it is the same standard — but that
one covers the failure modes that are specific to working here.

## The one structural thing to know first

The app is macOS-only, and not incidentally so. It reads a registry Claude Code
maintains, draws itself as a non-activating `NSPanel` above fullscreen windows,
asks AppKit where the notch is, and raises editors through `open -b`. There is
no Linux or Windows path hiding behind a feature flag, and there is no plan to
grow one. If you are on another platform you can read the code and reason about
it, but you cannot run it or its Rust suite.

## Getting set up

- **macOS 13 or later.**
- **Xcode Command Line Tools** — `xcode-select --install`. The Rust build links
  against the macOS SDK and the Tauri bundler needs the toolchain.
- **Node 20 or later.**
- **Rust 1.77 or later**, via [rustup](https://rustup.rs). That floor is
  `rust-version` in `src-tauri/Cargo.toml`; it is the real minimum rather than a
  guess, so if you raise it in a merge request, raise it there too.

Then:

```bash
npm install
npm run tauri dev
```

The frontend hot-reloads. Rust changes trigger a rebuild, which is slow the
first time and tolerable after that.

### Running against fixtures

`npm run tauri dev` shows you your own live Claude Code sessions, which is
useful for the happy path and useless for everything else — you cannot
conveniently arrange for a session to be dead, another to be waiting, and a
third to have a background job, all at once, just because you changed a
stylesheet.

```bash
scripts/dev-fixtures.sh
```

That points `CLAWDE_BUDDY_REGISTRY_DIR`, `CLAWDE_BUDDY_PROJECTS_DIR` and
`CLAWDE_BUDDY_USAGE_FILE` at the committed data under `fixtures/` and launches
the built app against it, so `npm run tauri build` has to have run at least
once. The fixtures reproduce the six sessions in the README screenshots, which
is how those screenshots were made, so a UI change can be eyeballed against
exactly the arrangement the README documents. Set the three variables by hand
if you would rather aim the widget at a registry of your own.

Setting the usage variable is not incidental: it stands in for the whole
five-hour meter, the API call included. Without it the widget would fetch the
real figure and paint the maintainer's actual account spend over your fixture
run — which is neither reproducible nor anyone else's business. If you are
adding a screenshot to the README, take it from a fixture run.

## Tests

```bash
npm test
```

```bash
cd src-tauri && cargo test -- --test-threads=1
```

At the time of writing that is 191 frontend tests and 277 Rust tests, and both
suites are green on `main`. `npm run typecheck` is the third thing CI will look
at and costs a couple of seconds.

`--test-threads=1` is not optional and not superstition. Most of the Rust suite
is pure and would run in parallel happily, but the watcher-loop tests spin up a
real `notify` watcher over a real temporary directory and wait on real
wall-clock time for the reconcile tick. Run those concurrently and they compete
for FSEvents delivery and for the timing margins they assert on, and you get
failures that have nothing to do with your change. If you add a test that
touches the filesystem or the clock, assume the same constraint applies.

### Where to put a new test

The Rust suite is weighted heavily toward `watcher::state`, and that is
deliberate. `state::snapshot` derives every session's state and every transition
between them, and it is a pure function: the clock arrives as a `now_ms`
argument, and pid liveness, transcript activity and the blocked-on-a-question
check all arrive as trait objects (`PidLiveness`, `ActivityProbe`,
`BlockedProbe`), each of which has a `Fake*` implementation next to the real
one. The whole state machine is therefore testable without a filesystem, without
a process to kill, and without waiting for anything.

That purity is the single most important property in the codebase, and it is
easy to destroy by accident. If you find yourself wanting to call
`SystemTime::now()` or read a file from inside the derivation, that is the
signal to add another injected dependency instead — there are already eight, and
`#[allow(clippy::too_many_arguments)]` sits on the function acknowledging the
trade as the price of the thing being worth testing. The same pattern shows up
in `visibility` (pure policy, no window server) and `notch` (one AppKit probe,
pure arithmetic on top), for the same reason.

## What CI will and will not do for you

Stated plainly, because finding this out from a green pipeline that did not
actually test your change is worse.

A merge request runs a `test` stage on GitLab's free Linux runners:
`test:frontend` (`npm run typecheck` and `npm test`) and `test:rustfmt`
(`cargo fmt --check`). Those run for forks. The format check runs on Linux even
though the crate does not build there, because rustfmt only parses and re-prints
the source and never touches the dependency graph.

`test:rust` runs `cargo clippy` and `cargo test`, and that one needs macOS — the
crate pulls in AppKit and Core Graphics and does not compile anywhere else.
Clippy is not currently blocking there: `main` carries about twenty warnings, of
which seven are deprecations inside a re-exported `cocoa` binding that is not
ours to fix. Compile errors still fail the job. See the note under Code style
about what is expected of a merge request in the meantime.

The project's macOS runner is the maintainer's own laptop, registered as a
project runner with the **shell executor**. A shell executor runs job commands
unsandboxed, as the logged-in user, directly on that machine. Running it for a
fork merge request would mean executing a stranger's code — including whatever a
`build.rs` or an `npm` lifecycle script felt like doing — on a personal computer
with the maintainer's own credentials on it. So it does not run for forks, and
that is not going to change without a disposable runner to change it to.

What this means for you: **run both suites locally before you open the merge
request.** That is the only thing that closes the gap. The maintainer runs the
Rust suite and clippy before merging, and a failure found there is a round trip
that a `cargo test` on your machine would have saved.

## Commits

Conventional-commits style, no scopes. The types actually in use across the
history are `feat`, `fix`, `docs`, `chore`, `ci`, `refactor` and `perf` — that
is the whole vocabulary, and there is no need to reach past it.

The subject line is lowercase after the type, imperative, and describes the
change in plain words rather than naming the files it touched:

```
feat: retract the chips into the notch and drop a list out of it
fix: stop the slab closing itself on the way to the second row
```

Bodies are where the reasoning goes, and on anything non-trivial there should be
one. Write prose, in paragraphs, saying what was wrong and why this is the fix —
the commit log is the only place some of these decisions are recorded, and
"fixed the bug" records nothing. Look at `git log` for the house style; several
of the larger commits are effectively short design notes.

## Merge requests

One coherent change per merge request. Describe what changed and why in the
description; if the reasoning is already in the commit bodies, saying so is
enough.

If your change is user-visible, include the changelog prose in the description —
see below — and bring the README along with it, which has a section of its own
because it is the part people forget.

Security vulnerabilities do not go in a merge request or a public issue. See
[SECURITY.md](SECURITY.md).

## CHANGELOG.md

`CHANGELOG.md` has one section per tag, and `scripts/release-notes.sh <tag>`
extracts that tag's section verbatim. The tag pipeline uses the result as the
GitLab release description *and* as the `notes` field in the updater manifest,
so it is what the in-app update dialog shows people. A tag with no section still
releases — the script deliberately does not fail over a forgotten entry — but it
releases with nothing in it except the download boilerplate.

So: **if your change is visible to someone using the app, it needs changelog
prose.** New behaviour, changed behaviour, a fixed bug someone could have hit, a
setting added or removed, a changed default. Purely internal work — a refactor
that leaves behaviour identical, a test added, a dependency bumped with no
observable effect, a CI or tooling change — does not.

The borderline cases, so you do not have to guess:

- **A performance fix** is user-visible if anyone would notice it, and most of
  them are. Write the entry.
- **A change to the settings file format** is user-visible even when the UI is
  unchanged, because `config.json` is documented as hand-editable and someone
  has hand-edited it.
- **A refactor that also fixes a latent bug** splits: the refactor is silent,
  the fix gets an entry.
- **README and docs changes** are not user-visible in this sense. `docs:`
  commits do not need entries.

Sections are named for a tag that does not exist yet when you open a merge
request, so unless the top section is for a version that has not been tagged,
put the prose in your merge request description under a `Changelog` heading and
the maintainer will fold it into the section for whichever release carries it.
Write it as it should appear: the existing entries are prose that explains the
change to someone who does not know the code, not a restatement of the commit
subject.

## Keeping the README in step

`CHANGELOG.md` says what changed in a release. `README.md` says what the app
*is*, right now, to someone who has never run it — and unlike a changelog it
goes stale silently. Nothing fails when a setting exists that the README does
not mention; it just quietly stops being true, and the next person to read it
learns something wrong.

So a change that alters what the app does to a user updates the README in the
same merge request. Not afterwards, and not in a follow-up nobody opens. The
mapping is mechanical enough to write down:

| If you change | Update |
|---|---|
| A session state, a dot colour or a dot shape | the state table under *What you see* |
| Anything about the five-hour meter — when it polls, where the token comes from, when it hides | *The five-hour limit* |
| How subagents or background jobs are matched or displayed | *Sessions, subagents and jobs* |
| Notch geometry, or when the slab shows | *Notch mode* |
| A field in `config.json`, including a changed default | the JSON block **and** the prose under *Settings file* |
| A tray menu item, or what one does | *Using it* |
| When an alert fires, or what it says | *Alerts* |
| A minimum toolchain version | *Requirements*, and `rust-version` in `Cargo.toml` if it is that one |
| An environment override | *Tests* |
| Something the app now can — or still cannot — do | *Limitations* |

Two of those deserve emphasis. The **settings file** is documented as
hand-editable, so its prose is a contract with people who have hand-edited it;
adding a key and leaving the block alone leaves them with a file the README says
is complete and is not. And **Limitations** is the section people actually read
before filing an issue — a limitation you removed should leave it, and one you
introduced should arrive in it rather than in a bug report six weeks later.

If the change alters what a screenshot shows, regenerate the screenshot. Run
`scripts/dev-fixtures.sh`, capture the same state the existing image shows, and
overwrite the file in `docs/media/` under its existing name so nothing else has
to move. Fixture runs are the only acceptable source: a capture from your own
machine puts real project names, real branches and a real account's usage figure
into a public repository, which is why the fixtures exist at all. The alt text
on those images is unusually long and describes the picture in detail — keep
that up, it is what a screen reader gets.

What does *not* belong in the README is contributor detail. It is the
user-facing document, and it has a Development section three lines long that
points here on purpose. Setup, conventions, architecture and test strategy go in
this file; resist growing a second copy of it inside the README.

## Code style

On the Rust side **rustfmt is the arbiter** — `cargo fmt` applies it, CI runs
`cargo fmt --check`, and there is nothing to argue about. Run it before you
commit. `src-tauri/rustfmt.toml` is worth a look once: every key in it is
stable rustfmt's own default, written down rather than changed, because
defaults have shifted between editions before and a contributor whose toolchain
quietly disagrees with CI's finds out by having the pipeline reformat their
diff.

`cargo clippy` is a softer gate, honestly stated: the build is not
warning-free today. Some of the noise is a dependency's deprecated Cocoa
constants, which is not yours to fix, and some is real. The rule is therefore
**do not add new warnings** rather than "get to zero". Where a lint is knowingly
allowed there is an `#[allow(...)]` with a comment beside it saying why — see
`state::snapshot` and its eight arguments — and that is the pattern to follow
rather than silencing a lint crate-wide.

`.editorconfig` at the root covers the basics for everything else: two spaces on
the frontend, four in Rust, LF endings, a final newline. It is advisory, not a
gate, and it was read off the committed files rather than invented — turning it
on changes nothing about how the existing code looks. VS Code and Zed need a
plugin for it.

Beyond that, be honest about the state of things: **the frontend has no
automated formatter configured.** No Prettier, no ESLint. The standing
instruction is therefore to match the surrounding file. Read what is already
there and write like it; do not reformat a file you are editing for one line,
and do not introduce a formatter in the same merge request as a behaviour
change.

The comments deserve a word of their own. This codebase comments the *why*, not
the *what* — the non-obvious constraint, the bug a line exists to prevent, the
approach that was tried and did not work. `// increment the counter` has no
place; `// Plain glob, not ls | head: the shell executor runs with set -eo
pipefail` does. If a piece of code needed a paragraph of reasoning to arrive at,
leave the paragraph.

## If you are an AI agent

A good deal of this repository was written with Claude Code, and contributions
made that way are welcome on the same terms as any other. What follows is not a
different standard — it is the set of mistakes an agent makes here specifically,
written down so you do not have to make them first.

**The working tree may be shared.** More than one agent session can be running
against this single checkout at once, and they do not see each other. Run
`git status` before you start and again before you stage. A file you did not
touch showing up modified means someone else is mid-edit in it: leave it alone,
and say so rather than reverting it or "fixing" a compile error that belongs to
work in flight. This is not hypothetical — the rustfmt adoption in this repo
landed across a refactor that was being written at the same moment, and only a
status check caught it.

**Stage explicit paths.** Never `git add -A`, never `git commit -a`. In a shared
tree those sweep up whatever another session happens to have open, and the
result is unrecoverable in the sense that matters: their half-finished change is
now in your commit, under your message.

**Do not reformat what you are not changing.** A tree-wide `cargo fmt` is a
legitimate change and it belongs in its own commit, on its own, when nothing
else is in flight. Folding it into a feature makes the diff unreviewable.

**Verify before you assert.** Anything you write into a document — a test count,
a file path, a claim about what a function does — should come from having run
the command or read the source, not from what the surrounding prose implies. The
suites are cheap: run them.

**Never commit real session data.** The registry and the transcripts this app
reads contain whatever their user was actually working on: private repository
names, branch names, prose from their conversations, and an account's real
spend. That is what `fixtures/` is for, and it is the only thing that should
ever end up in a commit, an issue, or a screenshot.

**Keep the scope you were given.** If you notice something else wrong — a
clippy warning, a stale comment, a missing test — mention it rather than fixing
it in passing. One coherent change per merge request is not a style preference
here; it is what makes a one-maintainer review tractable.

**Read before you design.** `docs/superpowers/specs/` holds the design documents
for the features that already exist, and the reasoning in them is usually still
load-bearing. The orientation map below is the fastest way into the code, and
`watcher::state::snapshot` is the function to understand first — nearly every
behaviour question in this app resolves there.

**Keep the trailer.** Commits made with an assistant carry a
`Co-Authored-By:` line, and the convention here is to keep it. Attribution
should be accurate about how the code was written.

## Design documents

Specs and implementation plans live in the repository:

- `docs/superpowers/specs/` — what is being built and why, decided before any
  code
- `docs/superpowers/plans/` — the task-by-task implementation plan derived from
  a spec

A bug fix or a small feature does not need either. A large change — a new
placement mode, a new data source, anything that touches the boundary between
the watcher, the bridge and the frontend — is expected to arrive with a spec, or
at least to start as a discussion in an issue that produces one. That is not
ceremony: the three existing specs are the reason the state derivation is a pure
function and the frontend derives nothing, and both of those properties would
have been lost by the third feature had they been decided one commit at a time.

Open an issue before starting anything large. It is a small project and a
rejected merge request is a waste of your afternoon, not just the maintainer's.

## Orientation map

Enough to know which file to open. Not exhaustive.

**Rust — `src-tauri/src/`**

| Path | What lives there |
|---|---|
| `lib.rs` | Wiring: module list, the `invoke_handler` surface, watcher spawn, tray menu, activation policy |
| `watcher/state.rs` | **Start here.** `snapshot()` — the pure derivation of every session's state, with the clock and all probes injected |
| `watcher/watch.rs` | The loop: FSEvents plus a 2-second reconcile tick, change filtering, and the snapshot store the frontend can fetch from |
| `watcher/registry.rs` | Parsing `~/.claude/sessions/<pid>.json`; only modelled fields, unknown keys ignored |
| `watcher/liveness.rs` | `kill(pid, 0)` plus `ps -o etime=`, behind the `PidLiveness` trait with a fake beside it |
| `watcher/activity.rs`, `blocked.rs`, `question.rs` | Transcript-derived probes — last touch, blocked-on-a-question, the question's prose — each a trait with a fake |
| `watcher/alerts.rs` | Transition diffing: alerts fire on edges between snapshots, never on states |
| `commands.rs` | The `#[tauri::command]` surface plus config validation and persistence |
| `config.rs` | `config.json` load, save and defaults; every field optional, a corrupt file falls back |
| `notify.rs` | macOS notifications and the `should_deliver` gate that the sound setting rides on |
| `window.rs` | The `NSPanel`: window level, Spaces behaviour, sizing, per-display positions, the settings window |
| `notch.rs` | One main-thread AppKit probe for the notch, and pure placement arithmetic over the result |
| `visibility.rs` | The `hideWhen` policy. Pure — it decides, the caller moves the panel |
| `cursor.rs` | 60ms cursor sampling, hover rects, click and drag — all mouse input, since the webview receives none |
| `usage.rs` / `usage_api.rs` | The shape and rules of the five-hour figure / fetching it from the API and finding a token |
| `bridge/raise.rs`, `proc_tree.rs`, `transcript.rs` | Raising a session's editor via `open -b`, walking the process tree to find it, and reading transcript tails |
| `update.rs` | The signing-key gate; with no key the updater is never registered |

**Frontend — `src/`**

| Path | What lives there |
|---|---|
| `App.tsx` | Widget or settings, chosen by the URL fragment; both windows load this bundle |
| `types.ts` | Mirrors the Rust serialization. Change a serialized shape and you change both, together |
| `useSessions.ts`, `useConfig.ts` | Subscribe to watcher and config events, with a fetch on mount so a quiet start is not an empty widget |
| `useCursor.ts`, `useNotch.ts`, `useWidgetSize.ts` | Cursor pushed from Rust and hit-tested here; notch geometry in window-local px; window sizing and morph timing |
| `views/dotRow/DotRow.tsx` | The widget. Orchestrates the collapsed pill, the named row, the popovers and the resize dance |
| `views/dotRow/` (rest) | `CollapsedPill`, `NamedDotRow`, `SessionPopover`, `UsageMeter`/`UsagePopover`, and `NotchFlanks`/`NotchPanel`/`RowDetail` for notch mode |
| `settings/SettingsPanel.tsx` | The settings window, including the sound-parent behaviour |
| `format.ts` | Elapsed times, short names, state counts |

The boundary between the two is enforced on purpose: Rust computes, the frontend
renders. If you find yourself deriving session state in TypeScript, the change
belongs in `watcher/state.rs` instead.

## License

clawde-buddy is MIT licensed — see [LICENSE](LICENSE). Contributions are
accepted under the same license; opening a merge request means you are offering
your work on those terms.
