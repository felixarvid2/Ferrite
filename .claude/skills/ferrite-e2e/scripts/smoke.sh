#!/usr/bin/env bash
# End-to-end smoke: up -> example.com title -> Ctrl+T via xdotool -> 2 tabs -> screenshot -> down.
# Exits non-zero on any failure. Usage: smoke.sh [--display N] [--port P] [--shot out.png]
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; . "$HERE/lib.sh"
D=97; P=9444; SHOT="${TMPDIR:-/tmp}/ferrite-smoke.png"
while [ $# -gt 0 ]; do case "$1" in
  -h|--help) echo "Usage: smoke.sh [--display N] [--port P] [--shot out.png]"; exit 0;;
  --display) D="$2"; shift 2;; --port) P="$2"; shift 2;; --shot) SHOT="$2"; shift 2;; *) echo "bad arg $1" >&2; exit 2;;
esac; done
export FERRITE_E2E_STATE="${FERRITE_E2E_STATE:-${TMPDIR:-/tmp}/ferrite-smoke-$P.state}"
rc=1
cleanup() { "$HERE/ferrite-down.sh" >/dev/null 2>&1; [ $rc = 0 ] && echo "SMOKE PASS" || echo "SMOKE FAIL" >&2; exit $rc; }
trap cleanup EXIT
step() { echo "-- $*"; }
die() { echo "FAIL: $*" >&2; "$HERE/shot.sh" "${SHOT%.png}-fail.png" >&2 2>/dev/null; exit 1; }
poll() { # poll <seconds> <js expr returning truthy>
  local end=$((SECONDS + $1)) r
  while [ $SECONDS -lt $end ]; do r=$(node "$HERE/ui.js" "$2" 2>/dev/null) && [ "$r" = true ] && return 0; sleep 0.5; done; return 1; }
step "start"; "$HERE/ferrite-up.sh" --display "$D" --port "$P" || exit 1
export DISPLAY=":$D"
step "navigate example.com"
node "$HERE/ui.js" "api('navigate',{input:'https://example.com'})" >/dev/null || die "navigate call failed"
poll 30 "api('state').then(s=>{const t=s.tabs.find(t=>t.id===s.active);return !!t&&/Example Domain/.test(t.title)})" \
  || die "title never became 'Example Domain': $(node "$HERE/ui.js" "api('state').then(s=>s.tabs.map(t=>t.title))" 2>&1)"
echo "title ok"
step "Ctrl+T via xdotool"
H=$(node "$HERE/ui.js" "document.body.offsetHeight" 2>/dev/null || echo 108)
xdotool mousemove 640 $((20 + ${H:-108} + 150)) click 1; sleep 0.3
xdotool key ctrl+t
poll 15 "api('state').then(s=>s.tabs.length===2)" || die "expected 2 tabs after Ctrl+T: $(node "$HERE/ui.js" "api('state').then(s=>s.tabs.length)" 2>&1)"
echo "2 tabs ok"
step "screenshot"; sleep 1; "$HERE/shot.sh" "$SHOT" || die "screenshot"
rc=0
