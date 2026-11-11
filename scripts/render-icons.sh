#!/usr/bin/env bash
# Re-render packaging/icons/*.png from ui/icon.svg (needs rsvg-convert).
set -euo pipefail
cd "$(dirname "$0")/.."
for size in 16 32 48 64 128 256 512; do
  rsvg-convert -w "$size" -h "$size" ui/icon.svg -o "packaging/icons/ferrite-$size.png"
done
sha256sum ui/icon.svg | cut -d' ' -f1 > packaging/icons/icon.svg.sha256
echo "packaging/icons updated"
