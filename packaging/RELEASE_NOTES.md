Ferrite is a desktop web browser written in Rust on Chromium (CEF 156). Linux x86_64.

## Downloads

- **`ferrite_*_amd64.deb`** (Ubuntu 24.04 / Debian). Recommended: it installs
  Chromium's sandbox helper and an AppArmor profile.
  `sudo apt install ./ferrite_*_amd64.deb`, then start "Ferrite".
- **`Ferrite-*-x86_64.AppImage`**: a single file, no install.
  `chmod +x Ferrite-*.AppImage && ./Ferrite-*.AppImage`. On Ubuntu 23.10+ its
  sandbox cannot start (user namespaces are restricted); use the .deb there.
- `SHA256SUMS`: checksums.

## Highlights

- Tabs (vertical by default), private windows, history, bookmarks, downloads,
  permissions, session restore, tab sleeping.
- Ad and tracker blocking with EasyList, EasyPrivacy and uBlock Origin's
  lists and scriptlets, including YouTube ad blocking. On by default.
- Proton Pass: fill logins through Proton's `pass-cli`
  (Settings > Passwords; run `pass-cli login` once).
- Widevine DRM (downloaded by Chromium on first run, active after a restart).

## Known limitations

- No H.264/AAC in this build (standard CEF). YouTube plays (VP9/AV1). See
  `docs/CODECS.md` for a codec build.
- YouTube ad blocking and Proton Pass were tested against local mock pages,
  not against the real services.
- See `browser/docs/STATUS.md` for everything verified and not verified.

This is an early release (0.1.1: new app icon).
