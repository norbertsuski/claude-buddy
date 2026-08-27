#!/usr/bin/env bash
# Run the widget against the committed fixtures instead of your own machine.
#
# Without this there is nothing to look at: the widget shows whatever Claude
# Code sessions happen to be running, so a contributor with none sees an empty
# pill, and nobody can reproduce the screenshots in the README. The fixtures are
# a fixed cast — one session waiting, one working, one idle, one paused, one
# dead, and a background job under the first — and this points the three
# override variables at them.
#
# `CLAWDE_BUDDY_USAGE_FILE` also suppresses the live `GET /api/oauth/usage`
# call, which is the point of it: without the override the widget would fetch
# the real figure with the real token and put your own account's spend back on
# screen, in the screenshot you were about to publish.
#
# Usage:
#   scripts/dev-fixtures.sh              # npm run tauri dev
#   scripts/dev-fixtures.sh app          # the release bundle, already built
#   scripts/dev-fixtures.sh <path>       # any binary, e.g. a debug build
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURES="$ROOT/fixtures"
BUNDLE="$ROOT/src-tauri/target/release/bundle/macos/clawde-buddy.app/Contents/MacOS/clawde-buddy"

# Fresh every run. Session states are derived from elapsed time and from process
# liveness, so a registry stamped an hour ago reads as a row of paused, dead
# sessions — see fixtures/generate.sh.
"$FIXTURES/generate.sh"

export CLAWDE_BUDDY_REGISTRY_DIR="$FIXTURES/sessions"
export CLAWDE_BUDDY_PROJECTS_DIR="$FIXTURES/projects"
export CLAWDE_BUDDY_USAGE_FILE="$FIXTURES/usage.json"

# infra-tools is only shown for five minutes after the widget first sees it dead
# (`DEAD_RETENTION_MS`): a crash is worth showing once, not forever. Take the
# screenshot that needs the red cross in it early.
echo "fixtures live; the dead session ages off the row five minutes from now"

TARGET="${1:-dev}"
case "$TARGET" in
  dev)
    # The env vars are read by the Rust process, which `tauri dev` runs as a
    # child of this shell, so exporting them here is enough.
    exec npm --prefix "$ROOT" run tauri dev
    ;;
  app)
    [ -x "$BUNDLE" ] || {
      echo "no bundle at $BUNDLE — run 'npm run tauri build' first" >&2
      exit 1
    }
    # The binary inside the bundle, deliberately, not `open -a`: launchd starts
    # an opened app with its own environment and none of the overrides above
    # would reach it, leaving the widget on live data with no sign anything
    # went wrong.
    exec "$BUNDLE"
    ;;
  *)
    [ -x "$TARGET" ] || {
      echo "not an executable: $TARGET" >&2
      echo "usage: dev-fixtures.sh [dev|app|<path-to-binary>]" >&2
      exit 1
    }
    exec "$TARGET"
    ;;
esac
