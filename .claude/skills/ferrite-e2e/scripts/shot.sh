#!/usr/bin/env bash
# Screenshot the whole X screen (UI strip + content) to a PNG; fails if the image looks blank.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; . "$HERE/lib.sh"
case "${1:-}" in ""|-h|--help) echo "Usage: shot.sh <out.png>   (display from \$DISPLAY_NUM / state file)"; exit 0;; esac
load_state; D="${DISPLAY_NUM:-97}"
mkdir -p "$(dirname "$1")"
DISPLAY=":$D" import -window root "$1" || { echo "import failed" >&2; exit 1; }
# Blank check: a real page has more than a handful of distinct colours.
N=$(convert "$1" -format %k info: 2>/dev/null || identify -format %k "$1" 2>/dev/null || echo 99)
[ "${N:-0}" -lt 4 ] && { echo "screenshot looks blank ($N colours): $1" >&2; exit 1; }
echo "$1 ($N colours)"
