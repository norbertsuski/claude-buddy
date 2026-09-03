# Multi-provider split: `buddy-core`, `buddy-ui`, and one app per provider

Date: 2026-09-03

## The problem

claude-buddy watches local Claude Code sessions. The same widget would be
useful for Cursor and for Codex. The question is not whether the code can be
reused — most of it can — but where the seams go and how three apps share
nine thousand lines of it without stepping on each other.

## Decisions

**One app per provider, not one widget showing all three.**

Usage limits have no common shape. `usage_api.rs` asks
`https://api.anthropic.com/api/oauth/usage` for a five-hour window, using an
OAuth token Claude Code already holds. Cursor bills per request against a
monthly quota. Codex rides a ChatGPT plan or a raw API key. Different endpoint,
different auth, different *semantics* — a single `Usage` type would be a lie in
two of three cases. Each app also carries its provider's visual identity, which
a merged dot row cannot.

**Separate repos, not a Cargo workspace.**

A workspace would satisfy both reasons above at lower cost — independent bundle
IDs, themes, usage modules and release workflows all fit in one tree, with path
dependencies instead of version bumps. Separate repos were chosen anyway, for
independence of the three products. The accepted cost is cross-repo churn: a
core change is one PR plus a tag plus a bump PR in each consumer.

**Shared code as two repos, consumed as tag-pinned git dependencies.**

- `buddy-core` — Rust crate. https://github.com/norbertsuski/buddy-core
- `buddy-ui` — React package, `@buddy/ui`. https://github.com/norbertsuski/buddy-ui

Both public. Neither is published to a registry: Cargo takes
`{ git = "…", tag = "v0.3.0" }` and npm takes
`"github:norbertsuski/buddy-ui#v0.3.0"`, so both halves pin the same way and
neither needs an account. `buddy-ui` therefore needs a `prepare` script that
builds, because a git install delivers source rather than `dist/`.

Registry publishing stays available as a later upgrade, if outside contributors
ever want to build providers.

**claude-buddy keeps its name, repo, README, stars and releases.** Nothing is
renamed. Its updater endpoint — `/releases/latest/download/latest.json` —
remains correct, because that repo releases only the Claude app.

## What moves and what stays

Provider-specific, stays in the app repo (3,797 lines):

| File | Lines | Why it cannot move |
| --- | --- | --- |
| `watcher/registry.rs` | 173 | `~/.claude/sessions/<pid>.json` schema |
| `bridge/transcript.rs` | 847 | Claude Code's JSONL record shape |
| `watcher/title.rs` | 403 | title events inside that transcript |
| `watcher/tasks.rs` | 1618 | subagent tasks are a Claude Code concept |
| `usage.rs`, `usage_api.rs` | 492 | the quota argument above |
| parts of `bridge/proc_tree.rs` | 264 | matches Claude Code process shapes |

Provider-agnostic, moves to `buddy-core` (6,423 lines): `window.rs` (730,
NSPanel and positioning), `notch.rs` (577, screen geometry), `cursor.rs` (597,
CGEvent hover polling), `tray.rs`, `visibility.rs`, `awake.rs`, `autostart.rs`,
`notify.rs`, `update.rs`, `about.rs`, `rfc3339.rs`, and `config.rs` minus its
`BUNDLE_ID` const.

Moves to `buddy-ui`: the whole `views/dotRow/*` tree, `useNotch`, `useCursor`,
`useWidgetSize`, `format.ts`, `heat.ts` — 2,283 lines of source, tests aside.

`watcher/state.rs` (2344) moves to core, but **not as-is** — an earlier draft
of this document claimed otherwise and was wrong. It is pure, and its clock
and liveness inputs are already injected as traits. Its activity input is
injected as a trait too, but the trait is declared in a file whose only real
implementation reads Claude Code transcripts, so injection alone does not
make it movable — and `state.rs:8-9` imports `watcher::registry::RegistryFile`
and `watcher::tasks::{Task, TaskKind, TaskProbe, TaskStatus}` directly. Two of
its input *types* are provider-shaped, so the seam has to be neutralized
before the file can move.

`state.rs` reads all twelve `RegistryFile` fields — `pid`, `session_id`, `cwd`,
`started_at`, `proc_start`, `entrypoint`, `kind`, `job_id`, `name`, `status`,
`status_updated_at`, `waiting_for` — so the neutral record core needs is the
whole struct, minus the serde rename attributes that encode Claude Code's JSON
spelling. Core owns `RawSession` as twelve plain fields; the app keeps
`RegistryFile` as the deserialization of one provider's file format, plus a
`From<RegistryFile> for RawSession`. A Cursor hook script supplies the same
twelve.

Three further couplings are one-liners, not redesigns: `tray.rs:265` reads a
`crate::commands::CONFIG_EVENT` const, `window.rs:90` reads `watch.rs`'s
`SnapshotStore`, and `notify.rs` uses `alerts::{Alert, AlertKind}` plus
`raise_pid` — the latter already behind an `Activator` trait.

## The provider seam

Core needs three things from an app, and one of them is allowed to be absent:

```rust
pub trait Provider {
    fn sessions(&self) -> Vec<RawSession>;
    fn detail(&self, s: &RawSession) -> Detail;
    fn usage(&self) -> Option<Usage>;
}
```

`usage()` returning `None` is a first-class answer: the meter simply does not
appear, the way `usage_api.rs` already handles a failed fetch.

Theming is CSS custom properties only. `@buddy/ui` components never carry
colour literals; each app's `theme.css` supplies the palette. Shared layout,
separate skin.

## Where the other providers' signals come from

claude-buddy works because Claude Code publishes session state to disk —
`status`, `waitingFor`, a pid. Neither Cursor nor Codex does.

Cursor has a hooks system. Verified present on this machine in
`~/.cursor/hooks.json`: `beforeSubmitPrompt`, `stop`, `afterShellExecution`,
`afterFileEdit`, `afterMCPExecution`. `beforeSubmitPrompt` and `stop` are
exactly the working/idle pair the state machine needs, so `cursor-buddy` ships
a small hook script that writes a registry its own adapter reads. Cursor's
`~/.cursor/projects/<slug>/` holds only canvases, terminals and MCP state; chat
state lives in `state.vscdb`, which is undocumented and version-fragile and is
deliberately not a data source here.

Codex is **not installed on this machine and its signals are unverified.** It
is out of scope until they are.

## Local development across four clones

A thin `buddy-dev` repo is the only thing cloned by hand. Bootstrap clones the
rest as siblings and wires up two untracked overrides.

```
~/Code/buddy/
├─ buddy-dev/          # repos.txt, justfile, scripts/bootstrap.sh
├─ buddy-core/
├─ buddy-ui/
├─ claude-buddy/
└─ cursor-buddy/
```

**Rust.** Each app commits a clean tag-pinned dependency. Bootstrap writes a
gitignored `.cargo/config.toml` beside it:

```toml
[patch."https://github.com/norbertsuski/buddy-core"]
buddy-core = { path = "../../buddy-core" }
```

Verified: with that file present the sibling clone is used; remove it and the
pinned tag returns.

**JavaScript.** A Vite alias gated on an env var, rather than `npm link`:

```ts
resolve: {
  alias: process.env.BUDDY_UI_LOCAL
    ? { '@buddy/ui': path.resolve(__dirname, '../buddy-ui/src') }
    : {},
}
```

This beats a symlink twice over: HMR crosses the boundary because Vite treats
aliased source as source, and Vitest inherits the alias from the same config.
It also survives `npm ci`, which wipes `node_modules` and any links in it.

The matching step is TypeScript: `npm run typecheck` would otherwise resolve
`@buddy/ui` to the published `.d.ts` while Vite resolves it to local source, and
the two would disagree silently. A `tsconfig.local.json` extending the base with
`paths`, used by a `typecheck:local` script, keeps them honest.

**Fan-out.** A `justfile` in `buddy-dev`: `just status`, `just pull`,
`just test`, `just dev claude`, `just release core`. `gita`, `mu-repo` and
`meta` are off-the-shelf alternatives if hand-rolling it stops being worth it.

## Two hazards found while testing this

**`Cargo.lock` is modified by the patch.** Confirmed: while patched, the
`buddy-core` entry loses its `source =` line; unpatched, it comes back. A local
build therefore dirties a committed lockfile. Guard it with `cargo build
--locked` in CI — a fresh clone has no patch, so a patched lock fails there
instead of shipping. Given the shared-tree warning in CLAUDE.md, `buddy-dev`
should also carry a `just precommit` that unpatches, rebuilds and checks the
lock.

**A patched git dependency still has to resolve.** An early test against a
nonexistent repo failed with `failed to authenticate when downloading
repository`. `[patch]` redirects resolution; it does not remove the need for the
original source to exist. So core must be pushed and tagged before any app can
build against it, locally included.

## Migration sequence

**Phase 1 — neutralize the seam in place, inside claude-buddy.** Introduce a
provider-neutral `RawSession`, retarget `snapshot()` at it, split the `Task`
data types away from the transcript scanner that produces them, and undo the
three one-line couplings. No new repo is touched and no file leaves the tree.
The suite stays green at every commit, and the result is a genuine improvement
even if the split never happens.

**Phase 2 — move the files out.**

1. Seed `buddy-core` with the now-agnostic Rust modules; push; tag `v0.1.0`.
2. Seed `buddy-ui` with the agnostic React tree plus its `prepare` build;
   push; tag `v0.1.0`.
3. Create `buddy-dev` with `repos.txt`, `bootstrap.sh` and the `justfile`.
4. One PR in claude-buddy: delete the moved files, add both dependencies, add
   the Vite alias and `tsconfig.local.json`, add `--locked` to CI.
5. Verify against fixtures with `scripts/dev-fixtures.sh`, then release.
6. Only then start `cursor-buddy`.

Phase 1 must complete before phase 2 begins. Moving files first and fixing
imports afterwards would leave `main` uncompilable across a window spanning
three repos.

Within phase 2, steps 1–4 cannot be atomic across repos, so claude-buddy's
`main` has a window where the dependency exists but the deletion has not
landed. Do step 4 as a single PR, and do not start it while another agent
session is mid-edit in the same checkout.

## Not in scope

- Renaming claude-buddy, or a neutral umbrella name. Considered and dropped:
  separate repos make it unnecessary.
- A Cargo workspace monorepo. Considered and rejected above.
- Registry publishing for either shared package.
- `codex-buddy`, until Codex's on-disk signals are verified.

## Open questions

- Whether Cursor exposes a readable quota at all. If not, `cursor-buddy` ships
  without a usage meter, which `Option<Usage>` already allows for.

**Settled during phase 1.** `alerts.rs` imports only `watcher::state` and
`watcher::task`; it derives every alert from `SessionSnapshot` and `Task`
alone, so it moves to `buddy-core` with them. `blocked.rs`, `working.rs` and
`question.rs` each import `bridge::transcript` instead: `TranscriptBlocked`
recognizes an unanswered `AskUserQuestion` in Claude Code's JSONL,
`TranscriptWork` recognizes an in-flight tool call there, and
`TranscriptQuestion` reads the latest assistant message out of it. All three
interpret that transcript directly, so all three stay.

`activity.rs`, `blocked.rs`, `working.rs` and `title.rs` are not clean stays,
though. Each declares a trait (`ActivityProbe`, `BlockedProbe`, `WorkProbe`,
`TitleProbe`) that `state.rs` takes as a parameter, and each trait's only
real implementation — `TranscriptActivity`, `TranscriptBlocked`,
`TranscriptWork`, `TranscriptTitle` — imports `crate::bridge::transcript` to
read Claude Code's JSONL. `state.rs` is pure and headed for core, but it
currently imports all four traits from files whose implementations are
staying behind. The other two injected traits are unaffected: `PidLiveness`
(`liveness.rs`) checks OS pid liveness and imports nothing under `crate::`,
so there is no transcript dependency to strand; `TaskProbe` (`task.rs`) has
that same clean shape only because `f0f02d5` already split it out of
`tasks.rs`, for this exact reason. That commit is the precedent and the
shape of the fix here too: the trait moved to `task.rs` with `state.rs`, and
the transcript-reading implementation stayed behind in `tasks.rs`.
`ActivityProbe`, `BlockedProbe`, `WorkProbe` and `TitleProbe` each need that
same split before `state.rs` can actually leave. Phase 1 has been extended
to perform all four splits on that pattern now, rather than deferring them
to phase 2, so a mistake here surfaces as a local compile error instead of a
broken cross-repo dependency.
