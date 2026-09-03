#!/usr/bin/env bash
# Point buddy-core and @buddy/ui at sibling clones, or back at their pinned tags.
#
# The committed dependencies are tag-pinned on purpose: a core change is a tag
# there and a bump here, so this app never moves under its own feet. That is the
# wrong loop while actually developing core, hence this.
#
# The Rust side is redirected by a `[patch]` in an untracked
# `src-tauri/.cargo/config.toml`. Cargo honours it transparently — but it also
# drops the `source` line from Cargo.lock, so do NOT commit a lockfile written
# while patched. CI runs `cargo test --locked` to catch exactly that.
#
# The JS side is an env var rather than a file, read by vite.config.ts. An alias
# beats `npm link` twice over: HMR crosses the package boundary because Vite
# treats the aliased path as source, and it survives `npm ci`, which wipes
# node_modules and any symlink in it.
#
# Usage:
#   scripts/dev-core.sh on     # patch to ../buddy-core and ../buddy-ui
#   scripts/dev-core.sh off    # back to the pinned tags
#   scripts/dev-core.sh dev    # `on`, then run the widget against fixtures
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PATCH="$ROOT/src-tauri/.cargo/config.toml"
CORE="$(cd "$ROOT/.." && pwd)/buddy-core"
UI="$(cd "$ROOT/.." && pwd)/buddy-ui"

case "${1:-}" in
  on|dev)
    for d in "$CORE" "$UI"; do
      if [ ! -d "$d" ]; then
        echo "missing sibling clone: $d" >&2
        echo "  git clone https://github.com/norbertsuski/$(basename "$d") $d" >&2
        exit 1
      fi
    done
    mkdir -p "$(dirname "$PATCH")"
    cat > "$PATCH" <<'PATCHEOF'
# Written by scripts/dev-core.sh. Untracked, and safe to delete.
[patch."https://github.com/norbertsuski/buddy-core"]
buddy-core = { path = "../../buddy-core" }
PATCHEOF
    echo "buddy-core -> $CORE"
    echo "@buddy/ui  -> $UI  (via BUDDY_UI_LOCAL)"
    if [ "$1" = "dev" ]; then
      BUDDY_UI_LOCAL=1 exec "$ROOT/scripts/dev-fixtures.sh"
    fi
    echo
    echo "For the JS half, export it yourself or use \`npm run dev:local\`:"
    echo "  export BUDDY_UI_LOCAL=1"
    ;;
  off)
    rm -f "$PATCH"
    echo "back to the pinned tags. Check Cargo.lock is not still patched:"
    echo "  git diff --stat src-tauri/Cargo.lock"
    ;;
  *)
    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
    exit 1
    ;;
esac
