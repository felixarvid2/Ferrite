# Shared helpers, sourced by the ferrite-e2e scripts.
# State file: key=value lines (DISPLAY_NUM, PORT, DATA, XVFB_PID, WM_PID, FERRITE_PID, LOG).
STATE_FILE="${FERRITE_E2E_STATE:-${TMPDIR:-/tmp}/ferrite-e2e.state}"
export STATE_FILE
load_state() { [ -f "$STATE_FILE" ] && . "$STATE_FILE"; return 0; }
