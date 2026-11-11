#!/usr/bin/env bash
# Stop the instance recorded in the state file (only those PIDs): SIGTERM, then SIGKILL.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; . "$HERE/lib.sh"
case "${1:-}" in -h|--help) echo "Usage: ferrite-down.sh   (kills PIDs from \$FERRITE_E2E_STATE, default $STATE_FILE; removes auto-created data dir)"; exit 0;; esac
[ -f "$STATE_FILE" ] || { echo "no state file $STATE_FILE; nothing to do"; exit 0; }
. "$STATE_FILE"
PIDS="${FERRITE_PID:-} ${WM_PID:-} ${XVFB_PID:-}"
for p in $PIDS; do kill -TERM "$p" 2>/dev/null; done
for _ in $(seq 40); do
  alive=0; for p in $PIDS; do kill -0 "$p" 2>/dev/null && alive=1; done
  [ $alive = 0 ] && break; sleep 0.25
done
for p in $PIDS; do kill -0 "$p" 2>/dev/null && { kill -KILL "$p" 2>/dev/null; echo "SIGKILL $p"; }; done
# Ferrite's CEF helper processes normally exit with the browser; reap stragglers on our data dir only.
if [ -n "${DATA:-}" ]; then pkill -KILL -f -- "--user-data-dir=.*${DATA}" 2>/dev/null; fi
[ "${OWN_DATA:-0}" = 1 ] && [ -n "${DATA:-}" ] && rm -rf "$DATA"
rm -f "$STATE_FILE" "/tmp/.X11-unix/X${DISPLAY_NUM:-none}" 2>/dev/null
echo "down"
