#!/usr/bin/env bash
# Publish a locally built DMG to a GitLab release.
#
# For when the project has no macOS runner: build here, upload the artefact, and
# attach it to a release so people have a download link.
#
# Needs GITLAB_TOKEN with api scope. Usage: scripts/publish-release.sh v0.1.0
set -euo pipefail

TAG="${1:?usage: publish-release.sh <tag>}"
: "${GITLAB_TOKEN:?set GITLAB_TOKEN to a token with api scope}"

PROJECT="norbert.suski%2Fclawde-buddy"
API="https://gitlab.com/api/v4/projects/$PROJECT"
DMG="$(ls dist-dmg/*.dmg | head -1)"
NAME="$(basename "$DMG")"

echo "uploading $NAME"
curl --fail --silent --show-error \
  --header "PRIVATE-TOKEN: $GITLAB_TOKEN" \
  --upload-file "$DMG" \
  "$API/packages/generic/clawde-buddy/$TAG/$NAME" >/dev/null

# The same notes the tag pipeline would have used, so a locally published
# release does not read differently from a CI one.
NOTES="$(dirname "$0")/release-notes.sh"
sh "$NOTES" "$TAG" > release-notes.md

echo "creating release $TAG"
# Built by python3, not a heredoc: the notes are multi-line prose and have to be
# JSON-escaped rather than pasted.
TAG="$TAG" NAME="$NAME" \
URL="$API/packages/generic/clawde-buddy/$TAG/$NAME" \
python3 -c 'import json, os; print(json.dumps({
  "name": os.environ["TAG"],
  "tag_name": os.environ["TAG"],
  "description": open("release-notes.md").read().strip(),
  "assets": {"links": [{
    "name": os.environ["NAME"] + " (Apple Silicon)",
    "url": os.environ["URL"],
    "link_type": "package",
  }]},
}))' | curl --fail --silent --show-error --request POST \
  --header "PRIVATE-TOKEN: $GITLAB_TOKEN" \
  --header 'Content-Type: application/json' \
  --data @- "$API/releases" >/dev/null

echo "done: https://gitlab.com/norbert.suski/clawde-buddy/-/releases/$TAG"
