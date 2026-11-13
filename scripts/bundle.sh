#!/usr/bin/env bash
# Build a self-contained release directory: dist/ferrite/ with the ferrite
# executable, libcef.so and Chromium's resources. Copy that directory
# anywhere and run ./ferrite. ferrite-mcp (the MCP bridge for AI clients) is
# shipped next to it.
set -euo pipefail
cd "$(dirname "$0")/.."
export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef}"
cargo build --release
OUT=dist/ferrite
rm -rf "$OUT" && mkdir -p "$OUT"
cp target/release/ferrite target/release/ferrite-mcp "$OUT/"
# Runtime files of the CEF minimal distribution (no headers/cmake/wrapper).
for f in libcef.so libvk_swiftshader.so libvulkan.so.1 vk_swiftshader_icd.json \
         icudtl.dat v8_context_snapshot.bin resources.pak chrome_100_percent.pak \
         chrome_200_percent.pak chrome-sandbox; do
  [ -e "$CEF_PATH/$f" ] && cp "$CEF_PATH/$f" "$OUT/"
done
for f in libEGL.so libGLESv2.so snapshot_blob.bin; do
  [ -e "$CEF_PATH/$f" ] && cp "$CEF_PATH/$f" "$OUT/"
done
cp -r "$CEF_PATH/locales" "$OUT/"
cp "$CEF_PATH/CREDITS.html" "$OUT/CHROMIUM-CREDITS.html"
cp ui/fonts/OFL.txt "$OUT/LICENSE-IBM-Plex-OFL.txt"
mkdir -p "$OUT/adblock" && cp resources/adblock/ubo-scriptlets.json resources/adblock/LICENSE-uBlock-Origin.txt resources/adblock/README.md "$OUT/adblock/"
# Debug symbols make libcef.so ~1.4 GB; stripped it is ~265 MB.
strip --strip-debug "$OUT/ferrite" "$OUT/ferrite-mcp" "$OUT/libcef.so" 2>/dev/null || true
cat <<MSG
Bundle ready in $OUT ($(du -sh "$OUT" | cut -f1)).

Chromium's sandbox on Linux needs either unprivileged user namespaces or the
setuid helper. If ferrite reports a sandbox error, run once:
  sudo chown root:root $OUT/chrome-sandbox && sudo chmod 4755 $OUT/chrome-sandbox
MSG
