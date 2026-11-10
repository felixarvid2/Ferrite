#!/usr/bin/env bash
# Download the prebuilt CEF binary distribution that matches the pinned `cef`
# crate (Chromium is NOT compiled locally) into $CEF_PATH.
#   scripts/setup-cef.sh            -> ~/.local/share/cef
#   CEF_PATH=/opt/cef scripts/setup-cef.sh
set -euo pipefail
CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef}"
VERSION="$(grep -oP 'cef = \{ version = "=\K[^"]+' "$(dirname "$0")/../app/Cargo.toml")"
echo "Installing CEF for cef crate $VERSION into $CEF_PATH"
if ! command -v export-cef-dir >/dev/null || ! export-cef-dir --version 2>/dev/null | grep -q "$VERSION"; then
  cargo install --locked export-cef-dir --version "$VERSION"
fi
export-cef-dir --force "$CEF_PATH"
echo
echo "Done. Build with:  CEF_PATH=$CEF_PATH cargo build --release"
