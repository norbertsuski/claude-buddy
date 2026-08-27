#!/usr/bin/env bash
# Publish a locally built DMG to a GitHub release.
#
# For when the project has no macOS runner: build here, create the release, and
# attach the artefact to it so people have a download link.
#
# curl and python3 only, deliberately. The `gh` CLI would collapse this to two
# lines, but it is one more thing every maintainer has to install before they
# can cut a release, and python3 is already here — the tag workflow builds its
# JSON the same way, for the same reason.
#
# Needs GITHUB_TOKEN with write access to the repository's contents: the `repo`
# scope on a classic token, or "Contents: read and write" on a fine-grained one.
# Usage: scripts/publish-release.sh v0.1.0
set -euo pipefail

TAG="${1:?usage: publish-release.sh <tag>}"
: "${GITHUB_TOKEN:?set GITHUB_TOKEN to a token with write access to repository contents}"

REPO="norbertsuski/claude-buddy"
API="https://api.github.com/repos/$REPO"
# Asset uploads do not go to api.github.com. They go to their own host, and the
# path is keyed on the release's numeric id rather than on the tag, so the
# release has to exist before the DMG can be attached to it. That is the reverse
# of the GitLab flow this replaces, where the package was uploaded first and the
# release merely linked to the URL it landed at.
UPLOADS="https://uploads.github.com/repos/$REPO"

DMG="$(ls dist-dmg/*.dmg | head -1)"
NAME="$(basename "$DMG")"

# The same notes the tag workflow would have used, so a locally published
# release does not read differently from a CI one.
NOTES="$(dirname "$0")/release-notes.sh"
sh "$NOTES" "$TAG" > release-notes.md

echo "creating release $TAG"
# Built by python3, not a heredoc: the notes are multi-line prose and have to be
# JSON-escaped rather than pasted.
#
# `--fail-with-body` rather than the plain `--fail` used before, because the
# response is no longer disposable — the release id has to be read back out of
# it. The flag also keeps the error case readable: an already-released tag comes
# back as a 422 whose body is the only place GitHub says so, and `--fail` throws
# exactly that away in favour of "the requested URL returned error: 422".
if ! RESPONSE="$(TAG="$TAG" python3 -c 'import json, os; print(json.dumps({
  "tag_name": os.environ["TAG"],
  "name": os.environ["TAG"],
  "body": open("release-notes.md").read().strip(),
}))' | curl --fail-with-body --silent --show-error --request POST \
  --header "Authorization: Bearer $GITHUB_TOKEN" \
  --header 'Accept: application/vnd.github+json' \
  --header 'X-GitHub-Api-Version: 2022-11-28' \
  --header 'Content-Type: application/json' \
  --data @- "$API/releases")"; then
  printf '%s\n' "$RESPONSE" >&2
  exit 1
fi

RELEASE_ID="$(printf '%s' "$RESPONSE" |
  python3 -c 'import json, sys; print(json.load(sys.stdin)["id"])')"

echo "uploading $NAME"
# `--data-binary`, not `--upload-file`: the latter is a PUT and this endpoint
# only answers POST. The DMG is a few tens of megabytes, which curl is content
# to hold in memory.
#
# The Content-Type header is not decoration. GitHub records whatever the request
# claimed as the asset's own content type and serves every later download with
# it, and `--data-binary` would otherwise have curl announce
# `application/x-www-form-urlencoded` — so the DMG would go out to everyone
# labelled a form body. The asset filename rides in the query string, which is
# safe unescaped: make-dmg.sh builds it out of the package name, the version and
# `uname -m`.
if ! UPLOADED="$(curl --fail-with-body --silent --show-error --request POST \
  --header "Authorization: Bearer $GITHUB_TOKEN" \
  --header 'Accept: application/vnd.github+json' \
  --header 'X-GitHub-Api-Version: 2022-11-28' \
  --header 'Content-Type: application/x-apple-diskimage' \
  --data-binary "@$DMG" \
  "$UPLOADS/releases/$RELEASE_ID/assets?name=$NAME")"; then
  printf '%s\n' "$UPLOADED" >&2
  # The release itself is already up at this point, and deleting it to get back
  # to a clean slate would take the tag's notes with it. Leaving it and saying
  # so is the kinder failure: a re-run after fixing the upload only needs the
  # asset, which can also be attached from the release page by hand.
  echo "release $TAG exists but has no DMG attached" >&2
  exit 1
fi

echo "done: https://github.com/$REPO/releases/tag/$TAG"
