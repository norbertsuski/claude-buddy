#!/bin/sh
# Print the release description for a tag: its CHANGELOG.md section, followed by
# the download boilerplate every release needs.
#
# Used by the tag pipeline for `release:description` and by
# scripts/publish-release.sh, so a release reads the same either way.
#
# POSIX sh on purpose, and no bashisms: the release job runs on the release-cli
# image, where /bin/sh is busybox and bash does not exist.
#
# Usage: scripts/release-notes.sh v0.4.0
set -eu

TAG="${1:?usage: release-notes.sh <tag>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHANGELOG="$ROOT/CHANGELOG.md"

# The section, from its own heading to the next one, with the blank lines awk
# collects at either end trimmed off.
#
# A missing section is not fatal: a release carrying only boilerplate is worse
# than one with notes, but a pipeline that fails over a forgotten changelog
# entry is worse than both.
section=""
if [ -f "$CHANGELOG" ]; then
  section="$(awk -v tag="$TAG" '
    # Headings look like "## v0.4.0 — 2026-08-27". Matched on the version alone,
    # so the date is free to change or be left out.
    /^## / {
      if (found) exit
      split($0, parts, " ")
      if (parts[2] == tag) { found = 1 }
      next
    }
    found {
      if ($0 ~ /^[[:space:]]*$/) { blank++; next }
      if (started) { for (i = 0; i < blank; i++) print "" }
      blank = 0
      started = 1
      print
    }
  ' "$CHANGELOG")"
fi

if [ -z "$(printf '%s' "$section" | tr -d '[:space:]')" ]; then
  echo "no CHANGELOG.md section for $TAG" >&2
  section="macOS build of clawde-buddy $TAG."
fi

printf '%s\n' "$section"

cat <<'BOILERPLATE'

---

The app is unsigned. After downloading, right-click it in Finder and choose
Open, then confirm — a downloaded copy carries the quarantine flag, so
double-clicking will refuse.
BOILERPLATE
