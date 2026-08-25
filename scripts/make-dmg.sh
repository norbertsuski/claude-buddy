#!/usr/bin/env bash
# Build a distributable DMG from the bundled .app.
#
# Tauri's own dmg target drives Finder over AppleScript to arrange the window,
# which needs Automation permission and times out without it — including in CI.
# hdiutil alone produces a plain drag-to-Applications image with no such
# dependency. The result is a functional installer, just without a custom
# background image or icon placement.
set -euo pipefail

APP="src-tauri/target/release/bundle/macos/clawde-buddy.app"
VERSION="$(node -p "require('./package.json').version")"
ARCH="$(uname -m)"
OUT="dist-dmg/clawde-buddy_${VERSION}_${ARCH}.dmg"

[ -d "$APP" ] || { echo "no app bundle at $APP — run 'npm run tauri build' first" >&2; exit 1; }

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

mkdir -p dist-dmg
rm -f "$OUT"
hdiutil create \
  -volname "clawde-buddy" \
  -srcfolder "$STAGE" \
  -ov -format UDZO \
  "$OUT" >/dev/null

echo "$OUT"
