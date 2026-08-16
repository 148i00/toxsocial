#!/usr/bin/env bash
# Non-interactive GitHub release upload.
# Uses the already-stored Windows Git credential (no browser prompt when cached).
#
# Usage:
#   bash scripts/upload-release.sh v0.1.1 "ToxSocial v0.1.1" "Release notes" \
#     target/release/bundle/msi/ToxSocial_0.1.0_x64_en-US.msi \
#     target/release/bundle/nsis/ToxSocial_0.1.0_x64-setup.exe

set -euo pipefail

export GIT_TERMINAL_PROMPT=0
export HTTP_PROXY="${HTTP_PROXY:-http://127.0.0.1:7897}"
export HTTPS_PROXY="${HTTPS_PROXY:-http://127.0.0.1:7897}"
export http_proxy="$HTTP_PROXY"
export https_proxy="$HTTPS_PROXY"

TAG="${1:?tag required}"
NAME="${2:?release name required}"
BODY="${3:?body required}"
shift 3

if [ -n "${GITHUB_TOKEN:-}" ]; then
  TOKEN="$GITHUB_TOKEN"
elif [ -f "$HOME/.toxsocial_gh_token" ]; then
  TOKEN=$(cat "$HOME/.toxsocial_gh_token")
else
  echo "No GitHub token found. Set GITHUB_TOKEN or create ~/.toxsocial_gh_token" >&2
  exit 1
fi

RELEASE_JSON=$(curl -sS -m 30 -X POST "https://api.github.com/repos/148i00/toxsocial/releases" \
  -H "Authorization: token $TOKEN" \
  -H "Accept: application/vnd.github+json" \
  -d "{\"tag_name\":\"$TAG\",\"name\":\"$NAME\",\"body\":$(python -c "import json,sys; print(json.dumps(sys.argv[1]))" "$BODY"),\"draft\":false,\"prerelease\":false}")

RELEASE_ID=$(echo "$RELEASE_JSON" | python -c "import sys,json; print(json.load(sys.stdin)['id'])")
echo "Release created: https://github.com/148i00/toxsocial/releases/tag/$TAG"

for FILE in "$@"; do
  BASENAME=$(basename "$FILE")
  echo "Uploading $BASENAME ..."
  curl -sS -m 120 -X POST "https://uploads.github.com/repos/148i00/toxsocial/releases/$RELEASE_ID/assets?name=$BASENAME" \
    -H "Authorization: token $TOKEN" \
    -H "Content-Type: application/octet-stream" \
    --data-binary "@$FILE" | python -c "import sys,json; print('  ', json.load(sys.stdin).get('browser_download_url'))"
done
