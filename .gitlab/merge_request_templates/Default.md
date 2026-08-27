<!--
Every box below is something to run on your own machine before pressing merge,
rather than something to leave for the pipeline to find. See CONTRIBUTING.md at
the repo root for the setup, the conventions, and what CI does and does not
check on a merge request.
-->

## What this changes

<!-- What it does and why. If it fixes an issue, `Closes #12` here will link and
     close it on merge. -->

## Checks

- [ ] `npm test` passes
- [ ] `cd src-tauri && cargo test -- --test-threads=1` passes
      <!-- `--test-threads=1` is not optional. The watcher-loop tests use real
           files and real wall-clock time, and they interfere with each other
           when run in parallel. -->
- [ ] `npm run typecheck` passes
- [ ] `cargo fmt` has been run in `src-tauri/` and left nothing to reformat
- [ ] `cargo clippy --all-targets` reports nothing new
      <!-- `main` is not clippy-clean, so this is not "no warnings" — it is "no
           warnings you added". Compare against the count on `main` if you are
           unsure which are yours. -->
- [ ] New behaviour has tests, or there is a reason below why it does not

## README

- [ ] The README describes the app as it now behaves, or nothing it describes
      changed

<!-- Nothing fails when the README goes stale, which is exactly why it does.
     CONTRIBUTING.md has a table mapping a kind of change to the section it
     lands in; the two that get missed are `config.json`, whose prose is a
     contract with people who hand-edit it, and Limitations, which is what
     people read before filing an issue. If a screenshot no longer shows what
     the app does, regenerate it from `scripts/dev-fixtures.sh` — never from
     your own sessions. -->

## CHANGELOG

- [ ] User-visible changes are described in the section for the next release in
      [CHANGELOG.md](https://gitlab.com/norbert.suski/clawde-buddy/-/blob/main/CHANGELOG.md), or this change is not user-visible

<!-- Release notes are not generated from commits. `scripts/release-notes.sh`
     lifts the tag's own section straight out of CHANGELOG.md, and that is what
     the GitLab release and the in-app update dialog both show. A change that
     never reaches the file is a change nobody reads about on release day, and a
     tag with no section at all ships with nothing but the download
     boilerplate. -->

## Screenshots

<!-- Required for anything visual, which is most of this app. Before and after,
     of the same state, so the difference is the change and not the session
     data. Screenshots taken against the fixtures are ideal — they are
     reproducible, and they keep your real account's usage figure out of the
     issue tracker:

     scripts/dev-fixtures.sh

     If the change touches notch mode, show both placements: the pill and the
     band behave differently and a fix for one has broken the other before. -->

| Before | After |
|---|---|
|  |  |

## Anything the reviewer should know

<!-- Trade-offs you made, things you tried that did not work, parts you would
     like a second opinion on. -->
