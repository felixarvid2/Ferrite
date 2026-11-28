Ferrite is a desktop web browser written in Rust on Chromium (CEF 158,
Chromium 158.0.8127.89). Linux x86_64 and arm64, plus an Android APK.

## New in 0.5.0

- A new look in the style of Firefox: tabs above the toolbar (existing
  profiles switch once; the sidebar is still in Settings > Vertical tabs),
  floating tabs, a flat address bar, the system font, Firefox colours on the
  internal pages and a simpler New Tab page.
- Security: Chromium 156 -> 158 (CEF 158.0.6) with its security fixes, and the
  Rust crates `url` (RUSTSEC-2026-0592) and `cookie` (RUSTSEC-2026-0594).
- arm64 (aarch64) .deb packages.

## Downloads

- **`ferrite_*_amd64.deb`** (Ubuntu 24.04 / Debian). Recommended: it installs
  Chromium's sandbox helper and an AppArmor profile.
  `sudo apt install ./ferrite_*_amd64.deb`, then start "Ferrite".
- **`Ferrite-*-x86_64.AppImage`**: a single file, no install.
  `chmod +x Ferrite-*.AppImage && ./Ferrite-*.AppImage`. On Ubuntu 23.10+ its
  sandbox cannot start (user namespaces are restricted); use the .deb there.
- **`ferrite_*_arm64.deb`**: the same for arm64 (Raspberry Pi 5, ARM laptops
  and servers); checksums in `SHA256SUMS-arm64`.
- **`Ferrite-*.apk`**: Ferrite for Android (+ `.apk.sha256`).
- `SHA256SUMS`: checksums of the x86_64 packages.

## Highlights

- Tabs (above the toolbar, or in a sidebar), private windows, history, bookmarks, downloads,
  permissions, session restore, tab sleeping.
- Ad and tracker blocking with EasyList, EasyPrivacy and uBlock Origin's
  lists and scriptlets, including YouTube ad blocking. On by default.
- Proton Pass through Proton's `pass-cli` (Settings > Passwords; run
  `pass-cli login` once): fill logins, save new ones, update changed
  passwords, fill 2FA (TOTP) codes.
- Import bookmarks and history from Chrome, Chromium, Brave, Edge, Vivaldi
  and Firefox (Settings > Import).
- Shield button: blocked-request count and "turn off blocking on this site".
- Update check: "Update available" in the menu and on ferrite://about/
  (once a day, can be turned off).
- AI control over MCP (new in 0.3.0, off by default): Settings > AI control
  (MCP), then add `/usr/bin/ferrite-mcp` as an MCP server in Claude Desktop,
  Claude Code or another client. The AI works in its own isolated AI window,
  or in a tab you hand over from the tab menu; a bar with a Stop button shows
  when it is in control. It can never reach ferrite:// pages, settings,
  downloads, Proton Pass or private windows. `evaluate` (custom JavaScript) is
  refused unless enabled separately.
- Widevine DRM (downloaded by Chromium on first run, active after a restart).

## Known limitations

- No H.264/AAC in this build (standard CEF). YouTube plays (VP9/AV1). The
  prebuilt codec CEF is still 156, so there is no H.264 build of 0.5.0 yet
  (see `docs/CODECS.md`).
- YouTube ad blocking and Proton Pass were tested against local mock pages,
  not against the real services.
- MCP control was tested with a scripted MCP client, not yet with Claude
  Desktop or Claude Code. Linux only (Unix socket).
- Installed 0.2.0 and older check for updates in the old repository
  (felixarvid2/test) and will not see this release; install it manually once.
- See `docs/STATUS.md` for everything verified and not verified.

This is an early release. Ferrite now lives in https://github.com/felixarvid2/Ferrite.
