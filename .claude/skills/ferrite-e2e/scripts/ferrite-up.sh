#!/usr/bin/env bash
# Start Xvfb + openbox + Ferrite (isolated profile) and wait until the UI API is ready.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; . "$HERE/lib.sh"
usage() { cat <<'U'
Usage: ferrite-up.sh [--display N] [--port P] [--data DIR] [--size WxH] [URL...]
  --display N   X display number (default 97)
  --port P      remote-debugging port (default 9444)
  --data DIR    FERRITE_DATA_DIR (default: fresh mktemp dir, removed by ferrite-down.sh)
  --size WxH    Xvfb screen size (default 1400x900)
  URL...        passed to ferrite as startup URLs
Env: FERRITE_REPO (default: <skill>/../../..), FERRITE_BIN (default target/debug/ferrite),
     FERRITE_E2E_STATE (state file, default $TMPDIR/ferrite-e2e.state), FERRITE_RUST_LOG.
Prints PIDs into the state file; ferrite-down.sh kills exactly those.
U
}
D=97; P=9444; DATA=""; SIZE=1400x900; URLS=()
while [ $# -gt 0 ]; do case "$1" in
  -h|--help) usage; exit 0;;
  --display) D="$2"; shift 2;; --port) P="$2"; shift 2;;
  --data) DATA="$2"; shift 2;; --size) SIZE="$2"; shift 2;;
  *) URLS+=("$1"); shift;;
esac; done
REPO="${FERRITE_REPO:-$(cd "$HERE/../../../.." && pwd)}"
BIN="${FERRITE_BIN:-$REPO/target/debug/ferrite}"
[ -x "$BIN" ] || { echo "no binary $BIN; build: cd $REPO && CEF_PATH=\$HOME/.local/share/cef cargo build" >&2; exit 2; }
for t in Xvfb openbox xdotool import node; do command -v $t >/dev/null || { echo "missing tool: $t" >&2; exit 2; }; done
if [ -f "$STATE_FILE" ]; then . "$STATE_FILE"
  if kill -0 "${FERRITE_PID:-0}" 2>/dev/null; then echo "already running (state $STATE_FILE); run ferrite-down.sh first" >&2; exit 1; fi
fi
[ -e "/tmp/.X11-unix/X$D" ] && { echo "display :$D already in use; pick another with --display" >&2; exit 1; }
curl -s -m 1 "http://127.0.0.1:$P/json/version" >/dev/null 2>&1 && { echo "port $P already in use; pick another with --port" >&2; exit 1; }
OWN_DATA=0; [ -z "$DATA" ] && { DATA="$(mktemp -d /tmp/ferrite-e2e-data.XXXXXX)"; OWN_DATA=1; }
mkdir -p "$DATA"; LOG="$DATA/ferrite.log"
Xvfb ":$D" -screen 0 "${SIZE}x24" >/dev/null 2>&1 & XVFB_PID=$!
for _ in $(seq 50); do [ -e "/tmp/.X11-unix/X$D" ] && break; sleep 0.1; done
DISPLAY=":$D" openbox >/dev/null 2>&1 & WM_PID=$!
sleep 0.5
DISPLAY=":$D" FERRITE_DATA_DIR="$DATA" FERRITE_UI_DIR="$REPO/ui" RUST_LOG="${FERRITE_RUST_LOG:-info}" \
  nohup "$BIN" --no-sandbox --remote-debugging-port="$P" ${URLS[@]+"${URLS[@]}"} >"$LOG" 2>&1 & FERRITE_PID=$!
{ echo "DISPLAY_NUM=$D"; echo "PORT=$P"; echo "DATA=$DATA"; echo "OWN_DATA=$OWN_DATA"
  echo "XVFB_PID=$XVFB_PID"; echo "WM_PID=$WM_PID"; echo "FERRITE_PID=$FERRITE_PID"; echo "LOG=$LOG"; } >"$STATE_FILE"
fail() { echo "ferrite-up: $1 (log: $LOG)" >&2; tail -15 "$LOG" >&2; "$HERE/ferrite-down.sh" >/dev/null 2>&1; exit 1; }
for _ in $(seq 150); do
  kill -0 $FERRITE_PID 2>/dev/null || fail "ferrite exited early (SingletonLock from a live run on the same --data?)"
  curl -sf -m 1 "http://127.0.0.1:$P/json/version" >/dev/null && break; sleep 0.2
done
curl -sf -m 1 "http://127.0.0.1:$P/json/version" >/dev/null || fail "CDP port never came up"
# Wait for the UI page and its api() function.
if ! FERRITE_PORT=$P node "$HERE/ui.js" --wait >/dev/null 2>&1; then fail "UI page chrome.html / api() not ready"; fi
echo "up: display=:$D port=$P data=$DATA xvfb=$XVFB_PID openbox=$WM_PID ferrite=$FERRITE_PID"
echo "state file: $STATE_FILE"
