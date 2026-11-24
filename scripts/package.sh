#!/usr/bin/env bash
# Build installable Linux packages from the release bundle:
#   dist/ferrite_<version>_<arch>.deb        (installs to /opt/ferrite; /usr/bin/ferrite and /usr/bin/ferrite-mcp)
#   dist/Ferrite-<version>-<arch>.AppImage   (single file, no install)
#
# <arch> is the build machine's: amd64/x86_64 or arm64/aarch64 (build on an
# arm64 machine for arm64; CEF is downloaded for the host).
#
#   scripts/package.sh [--deb-only|--appimage-only]
#
# EDITION=mcp packages Ferrite-mcp, the edition for AI clients only:
#   dist/ferrite-mcp_<version>_<arch>.deb       (installs to /opt/ferrite-mcp;
#        /usr/bin/ferrite-mcp-server is the MCP server for AI clients and starts
#        the browser by itself, /usr/bin/ferrite-mcp-browser opens it by hand)
#   dist/Ferrite-mcp-<version>-<arch>.AppImage  (`--mcp-server` runs the MCP server)
# It installs next to Ferrite (own profile, app id and AppArmor profile) and
# is not registered as a web browser.
#
# Uses CEF_PATH like the other scripts (set it to a codec build to package
# one; see docs/CODECS.md for the licensing of redistributing that).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
WANT_DEB=1; WANT_APPIMAGE=1
case "${1:-}" in
  --deb-only) WANT_APPIMAGE=0 ;;
  --appimage-only) WANT_DEB=0 ;;
  "") ;;
  *) echo "usage: $0 [--deb-only|--appimage-only]" >&2; exit 2 ;;
esac

VERSION="$(awk -F'"' '/^\[workspace.package\]/ {p=1} p && /^version/ {print $2; exit}' Cargo.toml)"
EDITION="${EDITION:-standard}"
case "$EDITION" in
  standard) PKG=ferrite; NAME=Ferrite; DESKTOP=packaging/ferrite.desktop ;;
  mcp) PKG=ferrite-mcp; NAME=Ferrite-mcp; DESKTOP=packaging/ferrite-mcp.desktop ;;
  *) echo "unknown EDITION=$EDITION (standard or mcp)" >&2; exit 2 ;;
esac
export EDITION
PREFIX="/opt/$PKG"
DEB_ARCH="$(dpkg --print-architecture)"
case "$DEB_ARCH" in
  amd64) APPIMAGE_ARCH=x86_64 ;;
  arm64) APPIMAGE_ARCH=aarch64 ;;
  *) echo "unsupported architecture $DEB_ARCH (amd64 or arm64)" >&2; exit 2 ;;
esac
echo "Packaging $NAME $VERSION"

scripts/bundle.sh
BUNDLE="$ROOT/dist/ferrite"
WORK="$ROOT/dist/pkg"
rm -rf "$WORK" && mkdir -p "$WORK"

# Icons rendered from the SVG with rsvg-convert (apt install librsvg2-bin) or
# ImageMagick. Without either (e.g. CI runners whose apt mirror is down) the
# pre-rendered PNGs in packaging/icons are used; scripts/render-icons.sh
# refreshes them after ui/icon.svg changes.
ICONS="$WORK/icons"
mkdir -p "$ICONS"
for size in 16 32 48 64 128 256 512; do
  if command -v rsvg-convert >/dev/null; then
    rsvg-convert -w "$size" -h "$size" ui/icon.svg -o "$ICONS/ferrite-$size.png"
  elif command -v convert >/dev/null; then
    convert -background none -density 512 ui/icon.svg -resize "${size}x${size}" "$ICONS/ferrite-$size.png"
  else
    cp "packaging/icons/ferrite-$size.png" "$ICONS/"
  fi
done
if ! command -v rsvg-convert >/dev/null && ! command -v convert >/dev/null \
   && [ "$(sha256sum ui/icon.svg | cut -d' ' -f1)" != "$(cat packaging/icons/icon.svg.sha256)" ]; then
  echo "error: packaging/icons is out of date with ui/icon.svg; run scripts/render-icons.sh" >&2
  exit 1
fi

install_icons() { # <prefix>
  for size in 16 32 48 64 128 256 512; do
    install -Dm644 "$ICONS/ferrite-$size.png" "$1/usr/share/icons/hicolor/${size}x${size}/apps/$PKG.png"
  done
  install -Dm644 ui/icon.svg "$1/usr/share/icons/hicolor/scalable/apps/$PKG.svg"
}

# ---------------------------------------------------------------- .deb
if [ "$WANT_DEB" = 1 ]; then
  DEB="$WORK/deb"
  mkdir -p "$DEB/opt" "$DEB/DEBIAN"
  cp -a "$BUNDLE" "$DEB$PREFIX"
  mkdir -p "$DEB/usr/bin"
  if [ "$EDITION" = mcp ]; then
    # The MCP server is what AI clients run; it starts the browser itself.
    ln -s "$PREFIX/ferrite-mcp" "$DEB/usr/bin/ferrite-mcp-server"
    printf '#!/bin/sh\n# /usr/bin/ferrite-mcp-browser: open Ferrite-mcp by hand.\nexec %s/ferrite "$@"\n' "$PREFIX" > "$WORK/launcher"
    install -Dm755 "$WORK/launcher" "$DEB/usr/bin/ferrite-mcp-browser"
    sed "s#/opt/ferrite/ferrite#$PREFIX/ferrite#; s#^profile ferrite #profile $PKG #; s#<local/ferrite>#<local/$PKG>#" \
      packaging/apparmor-ferrite > "$WORK/apparmor"
    install -Dm644 "$WORK/apparmor" "$DEB/etc/apparmor.d/$PKG"
    DESCRIPTION="Ferrite-mcp: a web browser for AI clients (MCP)
 Ferrite (Rust on Chromium/CEF) built for AI clients: MCP control is on
 from the start and every web tab can be driven without handing it over.
 Point your MCP client at /usr/bin/ferrite-mcp-server."
    PROVIDES=""
  else
    install -Dm755 packaging/ferrite-launcher.sh "$DEB/usr/bin/ferrite"
    # MCP bridge for AI clients (see README): /usr/bin/ferrite-mcp
    ln -s /opt/ferrite/ferrite-mcp "$DEB/usr/bin/ferrite-mcp"
    install -Dm644 packaging/apparmor-ferrite "$DEB/etc/apparmor.d/ferrite"
    DESCRIPTION="Ferrite web browser
 A desktop web browser written in Rust on top of Chromium (CEF)."
    PROVIDES="Provides: www-browser
"
  fi
  install -Dm644 "$DESKTOP" "$DEB/usr/share/applications/$PKG.desktop"
  install -Dm644 LICENSE "$DEB/usr/share/doc/$PKG/copyright"
  install_icons "$DEB"
  SIZE_KB="$(du -sk "$DEB" | cut -f1)"
  cat > "$DEB/DEBIAN/control" <<EOF
Package: $PKG
Version: $VERSION
Section: web
Priority: optional
Architecture: $DEB_ARCH
Maintainer: Ferrite <noreply@localhost>
Installed-Size: $SIZE_KB
Depends: libasound2t64 | libasound2, libatk-bridge2.0-0t64 | libatk-bridge2.0-0, libatk1.0-0t64 | libatk1.0-0, libatspi2.0-0t64 | libatspi2.0-0, libc6 (>= 2.31), libcairo2, libcups2t64 | libcups2, libdbus-1-3, libdrm2, libexpat1, libgbm1, libglib2.0-0t64 | libglib2.0-0, libgtk-3-0t64 | libgtk-3-0, libnspr4, libnss3, libpango-1.0-0, libudev1, libx11-6, libxcb1, libxcomposite1, libxdamage1, libxext6, libxfixes3, libxkbcommon0, libxrandr2, ca-certificates, xdg-utils
Recommends: fonts-liberation
${PROVIDES}Description: $DESCRIPTION
EOF
  echo "/etc/apparmor.d/$PKG" > "$DEB/DEBIAN/conffiles"
  # The maintainer scripts below are Ferrite's; sed adapts them for the edition.
  cat > "$DEB/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
# Chromium's setuid sandbox helper (used where user namespaces are not
# available) must be owned by root with the setuid bit.
chown root:root /opt/ferrite/chrome-sandbox
chmod 4755 /opt/ferrite/chrome-sandbox
if command -v apparmor_parser >/dev/null 2>&1 && [ -d /sys/kernel/security/apparmor ]; then
  apparmor_parser -r -T -W /etc/apparmor.d/ferrite || true
fi
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
if command -v update-alternatives >/dev/null 2>&1; then
  update-alternatives --install /usr/bin/x-www-browser x-www-browser /usr/bin/ferrite 40 || true
fi
exit 0
EOF
  cat > "$DEB/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = remove ] && command -v update-alternatives >/dev/null 2>&1; then
  update-alternatives --remove x-www-browser /usr/bin/ferrite || true
fi
exit 0
EOF
  cat > "$DEB/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = purge ] && command -v apparmor_parser >/dev/null 2>&1; then
  apparmor_parser -R /etc/apparmor.d/ferrite 2>/dev/null || true
fi
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || true
exit 0
EOF
  if [ "$EDITION" = mcp ]; then
    # Not a default-browser candidate: no x-www-browser alternative.
    sed -i '/update-alternatives/,/^fi$/d' "$DEB/DEBIAN/postinst" "$DEB/DEBIAN/prerm"
    sed -i "s#/opt/ferrite/#$PREFIX/#g; s#/etc/apparmor.d/ferrite#/etc/apparmor.d/$PKG#g" \
      "$DEB/DEBIAN/postinst" "$DEB/DEBIAN/postrm"
  fi
  chmod 755 "$DEB/DEBIAN/postinst" "$DEB/DEBIAN/prerm" "$DEB/DEBIAN/postrm"
  DEBFILE="dist/${PKG}_${VERSION}_${DEB_ARCH}.deb"
  dpkg-deb --root-owner-group -Zxz --build "$DEB" "$DEBFILE" >/dev/null
  echo "Built $DEBFILE ($(du -h "$DEBFILE" | cut -f1))"
fi

# ---------------------------------------------------------------- AppImage
if [ "$WANT_APPIMAGE" = 1 ]; then
  APPDIR="$WORK/$NAME.AppDir"
  mkdir -p "$APPDIR/opt"
  cp -a "$BUNDLE" "$APPDIR/opt/ferrite"
  install_icons "$APPDIR"
  cp "$DESKTOP" "$APPDIR/$PKG.desktop"
  cp "$ICONS/ferrite-256.png" "$APPDIR/$PKG.png"
  cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
# AppImages cannot carry the setuid sandbox helper, so Chromium's sandbox
# needs unprivileged user namespaces. Ubuntu 23.10+ restricts those through
# AppArmor; there the .deb (which installs an AppArmor profile) is the way to
# run Ferrite with its sandbox. We never disable the sandbox automatically.
HERE="$(dirname "$(readlink -f "$0")")"
if [ "$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns 2>/dev/null)" = 1 ]; then
  echo "ferrite: this system restricts user namespaces (Ubuntu 23.10+), so the" >&2
  echo "         sandbox cannot start from an AppImage. Install the .deb instead." >&2
fi
EOF
  if [ "$EDITION" = mcp ]; then
    cat >> "$APPDIR/AppRun" <<'EOF'
# `Ferrite-mcp-*.AppImage --mcp-server` is what an MCP client runs. The server
# starts the browser as a separate run of this AppImage (its own mount).
if [ "${1:-}" = --mcp-server ]; then
  shift
  export FERRITE_MCP_BROWSER="${APPIMAGE:-$0}"
  exec "$HERE/opt/ferrite/ferrite-mcp" "$@"
fi
EOF
  fi
  echo 'exec "$HERE/opt/ferrite/ferrite" "$@"' >> "$APPDIR/AppRun"
  chmod 755 "$APPDIR/AppRun"
  TOOL="$ROOT/dist/.tools/appimagetool-$APPIMAGE_ARCH.AppImage"
  if [ ! -x "$TOOL" ]; then
    mkdir -p "$(dirname "$TOOL")"
    curl -fsSLo "$TOOL" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$APPIMAGE_ARCH.AppImage
    chmod +x "$TOOL"
  fi
  OUT="dist/${NAME}-${VERSION}-${APPIMAGE_ARCH}.AppImage"
  # Extract-and-run: works without FUSE (containers, CI).
  ARCH=$APPIMAGE_ARCH APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "$APPDIR" "$OUT" >/dev/null
  echo "Built $OUT ($(du -h "$OUT" | cut -f1))"
fi

rm -rf "$WORK"
