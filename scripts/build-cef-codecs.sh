#!/usr/bin/env bash
# Build CEF with proprietary codecs (H.264, AAC, MP3-in-MP4) and install it
# where Ferrite's build can use it.
#
# Standard CEF downloads (cef-builds.spotifycdn.com) are built without
# proprietary codecs. This script builds the SAME CEF version that the pinned
# `cef` crate expects (see app/Cargo.toml), with
#   proprietary_codecs=true ffmpeg_branding=Chrome
# using CEF's official automate-git.py, then installs the result in the flat
# layout cef-rs expects (like scripts/setup-cef.sh does for the standard
# build).
#
# Requirements (Chromium's, not ours): Linux x86_64 (Ubuntu 22.04/24.04
# recommended), ~120 GB free disk, 16 GB+ RAM (32 GB recommended), and many
# hours: roughly 4-8 h on 16+ cores, a day or more on 4 cores.
#
# Licensing: H.264/AAC are patent-encumbered. Building them for your own
# personal use is the usual case; redistributing binaries with them may need
# patent licences. Read docs/CODECS.md before sharing a build.
#
# Usage:
#   scripts/build-cef-codecs.sh [--work DIR] [--out DIR] [--jobs N]
#   scripts/build-cef-codecs.sh --dry-run      (show automate-git's plan only)
#   scripts/build-cef-codecs.sh --install-only DIST_DIR [--out DIR]
#       (install an already built cef_binary_*_linux64_minimal directory)
#
# Afterwards:
#   CEF_PATH=<out> cargo build --release && CEF_PATH=<out> scripts/bundle.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${WORK:-$HOME/cef-build}"
OUT="${OUT:-$HOME/.local/share/cef-codecs}"
JOBS=""
INSTALL_ONLY=""
DRY=""

while [ $# -gt 0 ]; do
  case "$1" in
    --work) WORK="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    --install-only) INSTALL_ONLY="$2"; shift 2 ;;
    --dry-run) DRY="--dry-run"; shift ;;  # print automate-git's commands only
    -h|--help) sed -n '2,32p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

# The pinned crate version, e.g. 156.4.0+156.0.26 -> CEF 156.0.26.
# (Cargo.lock has the full version with the CEF build after the "+".)
CRATE_VERSION="$(awk '/^name = "cef"$/ {getline; gsub(/version = |"/, ""); print; exit}' "$ROOT/Cargo.lock")"
CEF_VERSION="${CRATE_VERSION#*+}"
echo "Ferrite pins cef crate $CRATE_VERSION -> CEF $CEF_VERSION"

# Copy a CEF binary distribution into the flat layout used by cef-rs
# (export-cef-dir does the same for the standard download): Release/* and
# Resources/* at the top, plus headers, wrapper sources and CMake files.
install_dist() {
  local dist="$1"
  [ -f "$dist/Release/libcef.so" ] || { echo "not a CEF binary distribution: $dist" >&2; exit 1; }
  # The version comes from the headers (directory names vary between
  # builders), e.g. 156.0.26+g8ea0c73+chromium-156.0.8078.88.
  local version; version="$(grep -oP '#define CEF_VERSION "\K[^"]+' "$dist/include/cef_version.h")"
  [ -n "$version" ] || { echo "cannot read CEF_VERSION from $dist/include/cef_version.h" >&2; exit 1; }
  local name="cef_binary_${version}_linux64_minimal"
  case "$version" in
    "$CEF_VERSION"+*) ;;
    *) echo "warning: this is CEF $version, Ferrite pins $CEF_VERSION; the cef crate accepts" >&2
       echo "         older builds of the same API, but they lack later Chromium security fixes" >&2 ;;
  esac
  echo "Installing $name into $OUT"
  rm -rf "$OUT.tmp" && mkdir -p "$OUT.tmp"
  cp -a "$dist/Release/." "$OUT.tmp/"
  cp -a "$dist/Resources/." "$OUT.tmp/"
  cp -a "$dist/include" "$dist/libcef_dll" "$dist/cmake" "$dist/CMakeLists.txt" "$OUT.tmp/"
  [ -f "$dist/CREDITS.html" ] && cp "$dist/CREDITS.html" "$OUT.tmp/"
  # cef-dll-sys checks this file to match the crate's CEF version.
  printf '{\n  "type": "minimal",\n  "name": "%s.tar.bz2",\n  "sha1": ""\n}\n' "$name" > "$OUT.tmp/archive.json"
  rm -rf "$OUT" && mv "$OUT.tmp" "$OUT"
  echo
  echo "Done. Build Ferrite against it:"
  echo "  cd $ROOT && CEF_PATH=$OUT cargo build --release"
  echo "  CEF_PATH=$OUT scripts/bundle.sh"
  echo "Then open ferrite://about/ — it lists H.264/AAC support."
}

if [ -n "$INSTALL_ONLY" ]; then
  install_dist "$(cd "$INSTALL_ONLY" && pwd)"
  exit 0
fi

# --- preflight -------------------------------------------------------------
[ "$(uname -s)-$(uname -m)" = "Linux-x86_64" ] || { echo "Linux x86_64 only" >&2; exit 1; }
mkdir -p "$WORK"
free_gb=$(( $(df -Pk "$WORK" | awk 'NR==2 {print $4}') / 1024 / 1024 ))
mem_gb=$(( $(awk '/MemTotal/ {print $2}' /proc/meminfo) / 1024 / 1024 ))
echo "Work dir $WORK: ${free_gb} GB free, ${mem_gb} GB RAM, $(nproc) cores"
if [ "$free_gb" -lt 120 ] && [ -z "$DRY" ]; then
  echo "Need about 120 GB free in $WORK (have ${free_gb} GB). Use --work on a bigger disk." >&2
  exit 1
fi
[ "$mem_gb" -lt 16 ] && echo "warning: less than 16 GB RAM; linking may fail or swap heavily" >&2
for tool in git python3 curl; do
  command -v "$tool" >/dev/null || { echo "missing $tool" >&2; exit 1; }
done

# CEF version 156.0.26 = CEF branch for Chromium 156.0.8078 (branch 8078)
# at commit g<hash> in the distribution name; read both from the version the
# crate ships with (the standard download's archive.json if present).
CHROMIUM_BRANCH="${CHROMIUM_BRANCH:-}"
CEF_COMMIT="${CEF_COMMIT:-}"
std_json="${CEF_PATH:-$HOME/.local/share/cef}/archive.json"
if [ -f "$std_json" ]; then
  std_name="$(grep -oP '"name": "\K[^"]+' "$std_json")"
  CEF_COMMIT="${CEF_COMMIT:-$(echo "$std_name" | grep -oP '\+g\K[0-9a-f]+')}"
  CHROMIUM_BRANCH="${CHROMIUM_BRANCH:-$(echo "$std_name" | grep -oP 'chromium-\d+\.\d+\.\K\d+')}"
fi
if [ -z "$CHROMIUM_BRANCH" ] || [ -z "$CEF_COMMIT" ]; then
  echo "Run scripts/setup-cef.sh first (to learn the exact CEF commit), or set" >&2
  echo "CHROMIUM_BRANCH and CEF_COMMIT (e.g. CHROMIUM_BRANCH=8078 CEF_COMMIT=8ea0c73)." >&2
  exit 1
fi
echo "Building CEF branch $CHROMIUM_BRANCH at commit $CEF_COMMIT with proprietary codecs"

# --- build -----------------------------------------------------------------
cd "$WORK"
curl -fsSLo automate-git.py \
  "https://raw.githubusercontent.com/chromiumembedded/cef/$CHROMIUM_BRANCH/tools/automate/automate-git.py"

export GN_DEFINES="is_official_build=true proprietary_codecs=true ffmpeg_branding=Chrome use_sysroot=true symbol_level=0 chrome_pgo_phase=0"
export CEF_ARCHIVE_FORMAT=tar.bz2
[ -n "$JOBS" ] && export GN_JOBS="$JOBS"

# First run downloads ~30 GB of sources. Chromium's build dependencies
# (compilers, headers) are installed by its own script once the checkout
# exists; automate-git prints the path if they are missing:
#   sudo chromium_git/chromium/src/build/install-build-deps.sh --no-arm --no-nacl
python3 automate-git.py \
  --download-dir="$WORK/chromium_git" \
  --depot-tools-dir="$WORK/depot_tools" \
  --branch="$CHROMIUM_BRANCH" \
  --checkout="$CEF_COMMIT" \
  --minimal-distrib-only \
  --no-distrib-docs \
  --no-distrib-symbols \
  --no-chromium-history \
  --no-debug-build \
  --x64-build \
  --build-target=cefsimple \
  $DRY

[ -n "$DRY" ] && { echo "(dry run: nothing built)"; exit 0; }
dist="$(ls -d "$WORK"/chromium_git/chromium/src/cef/binary_distrib/cef_binary_*_linux64_minimal 2>/dev/null | tail -1)"
[ -n "$dist" ] || { echo "build finished but no minimal distribution was found" >&2; exit 1; }
install_dist "$dist"
