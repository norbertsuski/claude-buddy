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

echo "creating release $TAG"
curl --fail --silent --show-error --request POST \
  --header "PRIVATE-TOKEN: $GITLAB_TOKEN" \
  --header 'Content-Type: application/json' \
  --data @- "$API/releases" <<JSON >/dev/null
{
  "name": "$TAG",
  "tag_name": "$TAG",
  "description": "macOS build of clawde-buddy $TAG.\n\nThe app is unsigned: after downloading, right-click it in Finder and choose Open, then confirm.",
  "assets": {
    "links": [{
      "name": "$NAME (Apple Silicon)",
      "url": "$API/packages/generic/clawde-buddy/$TAG/$NAME",
      "link_type": "package"
    }]
  }
}
JSON

echo "done: https://gitlab.com/norbert.suski/clawde-buddy/-/releases/$TAG"
