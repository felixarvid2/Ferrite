#!/usr/bin/env bash
# Build (debug) and run Ferrite from the source tree.
#   scripts/run.sh [ferrite args...]      e.g. scripts/run.sh https://example.com
set -euo pipefail
cd "$(dirname "$0")/.."
export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef}"
cargo build
# Serve UI files from disk so ui/ edits show up after a reload (Ctrl+R in the
# internal page, or restart for the toolbar).
export FERRITE_UI_DIR="$PWD/ui"
exec target/debug/ferrite "$@"
