#!/usr/bin/env bash
# Install the CEF (Chromium) binaries Ferrite builds against. Chromium is NOT
# compiled locally.
#
#   scripts/setup-cef.sh            standard build -> ~/.local/share/cef
#   scripts/setup-cef.sh --codecs   prebuilt build WITH H.264/AAC
#                                   -> ~/.local/share/cef-codecs
#   CEF_PATH=/opt/cef scripts/setup-cef.sh
#
# Widevine (DRM) works with both: Chromium downloads the Widevine CDM into the
# profile on first run (see docs/CODECS.md).
#
# --codecs uses a third-party prebuilt (obs-rust/cef, CEF branch 8078 built
# with proprietary_codecs=true ffmpeg_branding=Chrome), pinned by sha256. It
# is an OLDER Chromium patch release than the standard build, so it lacks the
# security fixes in between; building it yourself is the safer option
# (scripts/build-cef-codecs.sh). Read docs/CODECS.md, including licensing.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [ "${1:-}" = "--codecs" ]; then
  PIN="$(grep -oP 'cef = \{ version = "=\K[^"]+' "$ROOT/app/Cargo.toml")"
  if [ "${PIN%%.*}" != 156 ]; then
    echo "The prebuilt codec CEF is CEF 156, but app/Cargo.toml pins cef $PIN." >&2
    echo "They cannot be mixed: build Ferrite <= 0.4.x with it, or build a matching" >&2
    echo "codec CEF yourself (scripts/build-cef-codecs.sh, see docs/CODECS.md)." >&2
    exit 1
  fi
  OUT="${CEF_PATH:-$HOME/.local/share/cef-codecs}"
  URL="https://github.com/obs-rust/cef/releases/download/8078/cef_binary_8078_linux_x86_64.tar.xz"
  SHA256="fb36df11bc8280c9cb666732becc1fe267f833bb964dbb13d69cad33488171dc"
  TMP="$(mktemp -d)"
  trap 'rm -rf "$TMP"' EXIT
  echo "Downloading codec-enabled CEF (third party, pinned sha256)..."
  curl -fSL --progress-bar -o "$TMP/cef.tar.xz" "$URL"
  echo "$SHA256  $TMP/cef.tar.xz" | sha256sum -c - >/dev/null || { echo "sha256 mismatch: refusing to install" >&2; exit 1; }
  mkdir "$TMP/x" && tar -xJf "$TMP/cef.tar.xz" -C "$TMP/x"
  "$ROOT/scripts/build-cef-codecs.sh" --install-only "$TMP/x/cef_binary_8078_linux_x86_64" --out "$OUT"
  exit 0
fi

CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef}"
VERSION="$(grep -oP 'cef = \{ version = "=\K[^"]+' "$ROOT/app/Cargo.toml")"
echo "Installing CEF for cef crate $VERSION into $CEF_PATH"
# The installed version comes from cargo's list (`export-cef-dir --version`
# takes an argument). --force replaces an older one, e.g. from a CI cache.
if ! command -v export-cef-dir >/dev/null || ! cargo install --list | grep -q "^export-cef-dir v${VERSION}[+:]"; then
  cargo install --locked --force export-cef-dir --version "$VERSION"
fi
export-cef-dir --force "$CEF_PATH"
echo
echo "Done. Build with:  CEF_PATH=$CEF_PATH cargo build --release"
