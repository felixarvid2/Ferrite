#!/usr/bin/env bash
# Regenerate resources/adblock/ubo-scriptlets.json from uBlock Origin's
# current sources (see resources/adblock/README.md).
#
#   scripts/update-adblock-resources.sh [git ref, default master]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REF="${1:-master}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
git clone -q --filter=blob:none --no-checkout https://github.com/gorhill/uBlock.git "$TMP/ubo"
git -C "$TMP/ubo" sparse-checkout set --no-cone /src/js/resources/ /src/js/urlskip.js /src/js/jsonpath.js /src/js/arglist-parser.js /LICENSE.txt
git -C "$TMP/ubo" checkout -q "$REF"
COMMIT="$(git -C "$TMP/ubo" rev-parse HEAD)"
python3 -I "$ROOT/scripts/build-adblock-resources.py" "$TMP/ubo" "$ROOT/resources/adblock/ubo-scriptlets.json"
cp "$TMP/ubo/LICENSE.txt" "$ROOT/resources/adblock/LICENSE-uBlock-Origin.txt"
sed -i -E "s/commit \`[0-9a-f]{40}\`/commit \`$COMMIT\`/" "$ROOT/resources/adblock/README.md"
echo "uBlock Origin $COMMIT; update the date in resources/adblock/README.md"
