# Status

Ferrite is an early-stage browser. This page lists what has been checked, what
has not, and what is missing. It was last updated for the Chromium 156 build
(CEF 156.0.26, `cef` 156.4.0).

Test environment for the "verified" list: Ubuntu 24.04 container, Xvfb with
openbox, no GPU, run as root with `--no-sandbox`. Input was sent with real X
events (xdotool) and results were read through the Chrome DevTools Protocol.
Network traffic went through an HTTPS-intercepting proxy.

## Android port

`android/` holds Ferrite for Android (Kotlin, Android System WebView, adblock-rust over JNI); see
[android/README.md](../android/README.md). Verified: host tests of the Rust crate, Kotlin unit tests, and
(in CI, `ferrite-android.yml`) the release build and an emulator smoke test of blocking plus document-start
scriptlet timing. Not verified: real-world sites (YouTube ads), real devices, ABIs other than x86_64 at runtime,
low-memory phones. No Proton Pass, no extensions.

## Ad and tracker blocking

- Implemented in `ferrite-core` (`blocker.rs`) with the `adblock` crate (0.13).
  Unit-tested in core.
- Lists: EasyList and EasyPrivacy (network rules), plus uBlock Origin's own
  `filters.txt`, `quick-fixes.txt` and `unbreak.txt` from uBlockOrigin/uAssets,
  loaded as trusted lists.
- Filter lists are cached in the profile's `filter-lists/` directory. They are
  refreshed when older than 1 day, using `ureq` with the OS certificate verifier.
- Scriptlets: 149 uBO scriptlets (GPL-3.0) in `resources/adblock/ubo-scriptlets.json`,
  converted statically by `scripts/build-adblock-resources.py`. Regenerate with
  `scripts/update-adblock-resources.sh`; see `resources/adblock/README.md`.
- Requests are checked in a CEF `ResourceRequestHandler` on the IO thread.
  Top-level navigations are never blocked by network rules.
- Scriptlets and element-hiding CSS are injected per main-frame navigation
  (`app/src/browser/inject.rs`); see ARCHITECTURE.md.
- On by default for new profiles. Toggle: Settings > Privacy, "Block ads and
  trackers" (`adblock_enabled`).
- Verified live with the earlier EasyList/EasyPrivacy-only build: both lists
  downloaded (2.0 MB + 1.5 MB) and the engine loaded; on cnn.com ad and tracking
  requests were cancelled (Adobe Launch, Optimizely, Google IMA SDK, tracking
  pings) while the page loaded normally.
- Per-site off switch: shield button in the address bar (blocked-request count badge, menu "Turn off blocking on this site"); sites are stored in `adblock_exceptions` (listed in Settings) and skip network blocking and scriptlet/CSS injection, subdomains included. The counter resets on each main-frame navigation.
- Not yet: procedural cosmetic filters
  (`:has-text()` etc.), generic class/id hiding, a blocked-request counter in the
  toolbar.

## Verified

### Browsing

- Window opens; Chromium initialises.
- Sites render and run JavaScript: Google Search (results and AI overview),
  Wikipedia, GitHub, Reddit (r/rust), Discord login (React SPA), Google sign-in
  page, BBC News, rust-lang.org, example.com.
- Address bar: typing a URL with `Ctrl+L` and Enter; search from the address bar
  (Google).
- Back and forward (`Alt+Left`, `Alt+Right`).
- Tabs: `Ctrl+T`, `Ctrl+W`, `Ctrl+Shift+T`, `Ctrl+Tab`.
- Find in page: 29 matches on GitHub.
- Zoom: 110% and 125%.

### Windows, popups and dialogs

- `target=_blank` opens a tab.
- `window.open` popup opens its own Ferrite window; `window.opener` is `true`.
- JavaScript `alert` dialog.
- GTK file chooser opens.

### Permissions

- Geolocation prompt bar. "Block" with "Remember" is stored and applied on the
  next visit without a new prompt.

### Bookmarks and downloads

- `Ctrl+D` bookmarks a page; bookmarks bar shows it.
- A 1.8 MB `.deb` download from GitHub completes; the downloads page shows the
  dangerous-file warning.

### Import from other browsers

- Settings > Import lists Chrome, Chromium, Brave, Edge, Vivaldi and Firefox
  profiles found under `$HOME` (including snap and flatpak paths). Verified in a
  real window with fake Chrome and Firefox profiles in a temporary `HOME`
  (`scripts/import-fixtures.py`): bookmarks arrive under "Imported from <browser>" on
  the bookmarks bar with their folders, history keeps its original times, a second
  import adds nothing, and the page shows the counts. Not verified against real,
  running browsers. Passwords are not imported. Only http(s) URLs; history is
  limited to the newest 20 000 entries from the last 365 days. Source databases
  are copied (with `-wal`) before reading. Not available from private windows.

### Privacy

- Private window isolation: cookies and localStorage from the private window are
  not visible in a normal window; no history entry is written.
- A web page link to `ferrite://settings` is blocked.

### Update check

- `core/src/update.rs` parses `https://api.github.com/repos/felixarvid2/Ferrite/releases?per_page=20`
  and picks the newest non-draft, non-prerelease `ferrite-v*` tag by semver
  (other tags such as `assets-large-v1` are ignored). Nothing is downloaded or
  installed; the deb needs root, so the user installs it.
- `app/src/browser/updates.rs` runs it on a background thread (ureq, platform
  verifier, 20 s timeout, 1 MB limit, user agent `Ferrite/<version>`). First
  check 30 s after start, then via the 60 s housekeeping tick every 24 h, never
  within 6 h of the last attempt. The last attempt time and the found update
  are kept in `update.json` in the profile (not in settings). "Check now" is
  not rate limited. Setting `check_for_updates` (default on) stops all
  automatic requests.
- UI: the main menu gets "Update available: x.y.z" (opens the release page in a
  new tab) and ferrite://about/ shows the release and download links or "up to
  date (checked ...)" with Check now (API `update_check`, `update_status`; the
  page polls because API calls are synchronous on the UI thread).
- `FERRITE_UPDATE_URL` (testing only) overrides the URL.
- Verified in the real browser (e2e, local http server): menu item and its click,
  about page links, Check now to "up to date", persisted update after restart
  with no request within 6 h, no request at all with the setting off. The live
  api.github.com response (ferrite-v0.1.1, ferrite-v0.1.0, assets-large-v1) is
  parsed (ignored test `live_github` and a trimmed fixture test).

### Ad blocking (scriptlets and YouTube rules)

- Local mock YouTube watch page (rules rewritten from youtube.com to
  youtube.localhost): inline `ytInitialPlayerResponse.adPlacements` became
  `undefined`; `JSON.parse` output lost `adPlacements` and `playerAds`; the XHR
  player response lost `adPlacements` through `adSlots`; the fetch response had
  `adPlacements`/`adSlots` renamed to `no_ads`. With blocking off, all of these
  were unchanged.
- Scriptlet injection timing checked for: startup tab, new tabs, same-site and
  cross-site navigation, server redirect to another host, and reload.
- Real sites load with blocking on: CNN, Reddit, GitHub, BBC, Wikipedia, Vimeo.
- Unit tests: trusted vs untrusted lists, aliases, json-prune, CSS. An ignored
  test checks that the real lists produce all YouTube rule families (script
  about 78 KB for youtube.com watch pages; generic CSS about 17 KB on news sites).

### Sessions and crashes

- Quit with `Ctrl+Shift+Q`, then restart: tabs are restored.
- `kill -9`, then restart: tabs are restored.

### Tab sleeping

- An idle tab frees its browser and wakes when selected.

### Internal pages

- History, downloads, settings and new tab pages render.

### Look and window frame

- Redesigned toolbar: the active tab merges into the toolbar, separators between
  inactive tabs, coloured letter icons for sites without a favicon, the address
  bar shows the site's domain in full ink when not focused, purple tint in
  private windows. Internal pages share a design system with a sidebar, toggle
  switches and day-grouped history; checked in dark and light themes.
- Frameless window by default: the tab strip is the title bar (CSS
  `app-region: drag`, reported through CEF's drag handler and applied with
  `Window::set_draggable_regions`). Dragging the strip moved the window under
  openbox; minimize/maximize/close buttons are drawn by the UI.
  Settings > Appearance > "Use system title bar" (`system_titlebar`) turns the
  desktop's title bar back on (takes effect for new windows).
- Resizing: dragging the bottom-right corner (1280x860 -> 1130x760) and the
  left edge resized the window under openbox. Frameless windows keep a 4 px
  frame because web content is a native child window that would swallow edge
  clicks. (Before this fix windows were not resizable at all: cef-rs defaults
  `WindowDelegate::can_resize/can_maximize/can_minimize` to false.)
- Maximized and fullscreen windows drop the resize frame (content reaches the
  screen edge; checked by screenshot), and it returns when restored. F11 and
  HTML5 video fullscreen were checked; in HTML5 fullscreen the video fills the
  screen and the UI strip is hidden.
- "Use system title bar": a new window gets the window manager's title bar and
  no Ferrite window buttons (checked under openbox).
- Not checked: GNOME/KDE window managers, Wayland.

### Vertical tabs (default layout)

- Default since the design review scored it 9/10 (top tabs: 8.7/10). Settings >
  Appearance > "Vertical tabs" switches back to tabs above the toolbar (new
  windows; existing profiles keep their saved choice). Tabs
  move to a 240 px sidebar left of the content (a second UI view running
  `chrome.html#side` against the same API); the top strip keeps the window
  buttons and drag area. Checked: layout after restart with restored tabs,
  switching tabs and opening a new tab by mouse in the sidebar.
- Not checked: drag-reordering in the sidebar, many tabs (scrolling).

### Packaging

- `scripts/package.sh --deb-only` built `dist/ferrite_0.1.0_amd64.deb` (134 MB).
  `apt install ./dist/...deb` resolved its dependencies and installed it on
  Ubuntu 24.04. Checks: `/opt/ferrite/chrome-sandbox` is `-rwsr-xr-x root`,
  `x-www-browser` points to `/usr/bin/ferrite`, the desktop file passes
  `desktop-file-validate` and is in the MIME cache for http/https, and the
  icons are installed. `ferrite` started from `/usr/bin` and opened a window
  with `WM_CLASS "ferrite", "Ferrite"` (matching `StartupWMClass`).
- WM_CLASS is set with Xlib: cef-rs loses strings written to
  `LinuxWindowProperties`, and Chromium's `--class` switch does not reach
  Views windows.
- AppImage: `scripts/package.sh --appimage-only` built a 170 MB
  `Ferrite-0.1.0-x86_64.AppImage`; it started and opened a window (run with
  `APPIMAGE_EXTRACT_AND_RUN=1`, since the container has no FUSE).
- Not tested: the AppArmor profile (no AppArmor in the
  container), the sandbox from the installed package (tests run as root with
  `--no-sandbox`), and `xdg-settings` on a real desktop.

### Release bundle

- The release bundle in `dist/` runs standalone with the embedded UI.

### Media playback

- WebM (VP9) video plays: 960x540, `currentTime` advances (MDN's `flower.webm`
  served locally); MP3 audio plays.
- MP4 (H.264/AAC) does not: `DEMUXER_ERROR_NO_SUPPORTED_STREAMS`, as expected
  for CEF builds without proprietary codecs (see Known limitations).

### Web platform probe

- WebAssembly: yes. IndexedDB: yes. localStorage: yes. Service Worker API: yes.
- WebSocket (wss echo): yes. RTCPeerConnection: yes. MediaSource: yes.
- VP9, AV1 and Opus playback: `canPlayType` returns "probably".

## Not verified

- YouTube video: www.youtube.com redirected to Google's "unusual traffic" page
  for the test container's datacenter IP. Through an embed iframe the YouTube
  player loaded and rendered but answered "Video unavailable" (server-side
  refusal for this IP). Opening an `/embed/` URL directly shows "Error 153",
  which YouTube shows in any browser without an embedding page. YouTube serves
  VP9/AV1 + Opus, which play in Ferrite (see Media playback).
- Gmail and Google Docs: only the sign-in page was reached (no account).
- Login and OAuth flows: not tested end to end.
- Hardware acceleration and WebGL on a real GPU. WebGL was unavailable in the
  container (no GPU; the SwiftShader WebGL fallback is off by default).
- Webcam and microphone, WebRTC calls, notifications display, picture-in-picture,
  PDF viewer, printing, drag and drop of files, IME input, HiDPI, Wayland,
  multi-monitor.
- Single-instance relaunch forwarding (implemented, not tested).
- Downloads "ask where" (uses CEF's dialog).
- Multiple profiles: only through `--profile`; no UI.

## Known limitations

### Media and DRM

- No H.264 or AAC in the standard CEF build (`canPlayType` returns "" for
  `avc1` and `mp4a`). Sites that serve only H.264 (some news players,
  X/Twitter video) will not play; YouTube serves VP9/AV1 and plays.
  Without building Chromium: `scripts/setup-cef.sh --codecs` downloads a
  third-party codec build (obs-rust, CEF 156.0.8, sha256-pinned) and installs
  it into `~/.local/share/cef-codecs`; it is an older Chromium patch release.
  Building it yourself: `scripts/build-cef-codecs.sh` (about 120 GB of disk).
  H.264/AAC playback was verified with the third-party build. See
  docs/CODECS.md. `ferrite://about/` shows which codecs a build supports.
- Widevine DRM works in the standard build: Chromium's component updater
  downloads the Widevine CDM into the profile (`<profile>/cef/WidevineCdm/`)
  on first run; it is used after a restart. Verified: Widevine-encrypted VP9
  test content (Shaka demo assets) played. Services that require H.264 with
  Widevine, or hardware-backed DRM (Netflix in HD), need the codec build and
  may still refuse a non-Chrome browser; Netflix/Spotify/Disney+ were not
  tested (no accounts).

### Proton Pass

- Fills logins from Proton Pass through Proton's official `pass-cli`
  (Settings > Passwords; run `pass-cli login` once; needs a Proton Pass plan
  with CLI access). The browser extension cannot run in CEF.
- Verified with a fake `pass-cli` only (no Proton account in the dev
  container): the key button appears on a matching site, the native menu
  lists "username — title", choosing it fills username and password and fires
  input/change events; no fill on a non-matching host, none when the page
  navigated while pass-cli ran; not-logged-in and missing-binary states show
  in Settings; web pages cannot call the pass API.
- Not verified: the real pass-cli JSON (the parsers accept the shapes seen in
  its source and docs; `item totp --output json` is documented as
  `{"<field>": "<code>"}`, a login's own secret is the field `totp`),
  React-heavy login pages, logins inside iframes (only the main frame is
  filled), a real Proton account for saving.
- Matching: same host or a subdomain of the saved URL's host (ports ignored);
  an https login is never filled into an http page. The fill runs in the
  page, so the page can read the password once filled (as with any autofill).

#### Saving logins ("Save password to Proton Pass?")

- After a sign-in, a bar in the UI strip asks "Save password for <site> to
  Proton Pass?" with Save, Never for this site and Not now. It shows the
  username, never the password. Settings > Passwords: "Offer to save
  passwords" (default on), the vault for new logins (default: the first
  vault), and the never-list with remove buttons.
- Capture: per content browser, a script in the isolated world `ferrite-pass`
  (`Page.addScriptToEvaluateOnNewDocument` with `worldName`) listens for form
  `submit`, Enter in a password field and clicks on a submit-like button near
  a password field (SPA logins; trusted events only), and reports
  `{username, password}` through a `Runtime.addBinding` function that exists
  only in that world (`executionContextName`). The page cannot see, call or
  spoof it: verified with `typeof window.__ferritePassSave` and a call from
  the page's own script (undefined / throws, no prompt). It runs in the top
  frame only (`window !== top`), so an iframe's form never prompts; the site
  is the main frame's URL as read by Rust, never part of the payload. It needs
  `Runtime.enable` (bindingCalled is not delivered otherwise).
- Registration happens once per browser session (not per navigation): when
  the tab's browser is created (`tabs.rs` `on_browser_created`), and on every
  existing tab when the setting is switched on. It must come before the first
  navigation: a DevTools session whose first message goes out while a
  navigation starts is not ready in time (measured: the answer only arrives
  after the held request is released, about 1 s), so holding the request does
  not help. Instead the deferred first navigation waits (at most 1 s) until
  the registration is confirmed (`tabs.rs` `deferred_start_after`); without
  that wait 1 of 20 fresh starts missed its first page, with it 0 of 30. The
  `on_before_browse` call is only a backstop for browsers that
  were missed and covers the page after the one being loaded. New web tabs
  also start on about:blank when ad blocking **or** login capture is on
  (`tabs.rs` `deferred`). Switching the offer off removes script and binding
  (and `Runtime.disable`).
- Costs: `Runtime.enable` stays on for every content browser while the feature
  is on (pages can detect it; execution-context events reach the browser
  process and are ignored). The capture script also runs in private windows;
  the payload is dropped in Rust there (logged at debug level). The prompt
  appears on submit, so it also appears after a failed login attempt.
- Not offered: private windows, non-http(s) pages, sites on the never-list,
  empty or oversized values (password over 1000 chars, username over 500).
  When a cached login for the site already has the same username, the update
  flow below applies instead. Two or more filled password fields: the last
  one is used (sign-up forms save the new account; change-password forms the
  new password).
- The password lives only in Rust memory (`SavePrompt`), is dropped on any
  answer, after 2 minutes, when the tab closes or when the feature is turned
  off; the UI gets `{id, kind, site, username}`. One prompt per tab (a newer submit
  replaces it). The `pass_save_answer` command is UI-only and checks that the
  prompt belongs to the caller's window.
- Saving runs `pass-cli item create login --share-id <vault> --from-template -`
  on a background thread with the JSON on stdin (argv is readable in /proc).
  The title is the site, the URL the page origin, and an email-looking name is
  stored as `email`, otherwise `username`. Messages and logs never contain the
  password (failure toast is generic; pass-cli's first stderr line is logged
  with the password redacted). The cache refreshes afterwards.
- Verified with the fake pass-cli (stdin JSON byte-checked with quotes,
  backslash, `</script>`, non-ASCII and shell metacharacters; argv has no
  password or username; failure path; "Never" and the disabled setting
  suppress the prompt). Rust memory is not zeroed on drop (a freed `String`
  may linger); not fixed.

#### Updating a password ("Update password for <username> on <site>?")
- Passwords are not cached, so when a captured sign-in matches a cached login
  (same site, same username, case-insensitive) Ferrite fetches that item's
  password in the background (`item view pass://S/I/password --output human`,
  as the fill does) and compares in Rust; both strings are dropped right after.
  Same password: nothing is shown. Different: the save bar asks "Update
  password for <username> on <site> in Proton Pass?" with [Not now] [Update]
  (the username is the stored login's spelling). A failed fetch shows nothing.
  Same rules as saving: http(s) main frame, not in private windows, needs
  "Offer to save passwords" and respects the never-list, one prompt per tab,
  password only in Rust memory (random prompt id), dropped on answer, after
  2 minutes, on tab close or when the feature is turned off. `pass_save_answer`
  accepts `update`/`dismiss` for an update prompt only.
- `pass-cli item update` only takes values in argv (visible in /proc for about
  a second), so it is not used for passwords. Update **recreates the item**:
  `item view --share-id S --item-id I --output json`, then `item create login
  --share-id S --from-template -` (same vault, same `--folder-id` if the item
  had one, JSON on stdin) with title, username, email, urls, TOTP URI and extra
  fields (text, hidden, totp) copied and only the password replaced; only after
  the create succeeded `item trash --share-id S --item-id I` moves the old item
  to the trash (never deleted permanently). The item id therefore changes, and
  creation/modification dates and pinned state of the original are not copied.
  The cache refreshes afterwards. Toasts: "Password updated in Proton Pass",
  "... (the old entry could not be moved to the trash)" if the trash call failed
  (the item then exists twice), or a generic failure (create failed: the old
  item is untouched and not trashed).
- Blocked, not replaced silently: the create template has no field for a
  **note**, attachments, passkeys, custom sections or field kinds other than
  text/hidden/totp (anything unknown that holds data counts). Then the bar says
  "This login has data Ferrite can't copy (attachments/passkeys). Update it in
  the Proton Pass app." with [OK], and nothing is created or trashed. A login
  with a note is therefore never updated by Ferrite.
- Verified with the fake pass-cli (it logs argv of every call and the stdin of
  create): update prompt and Update (stdin has the new password with quotes,
  backslash, `</script>`, shell characters, and the same title, username, URLs,
  TOTP URI and text/hidden/totp extra fields; argv never holds the password;
  trash after create; no `item update`/`delete`), same password and private
  window and offer off and never-list give no prompt, Not now does nothing,
  a newer submit replaces the prompt, note and passkey/attachment items show
  the blocked bar, create failure gives no trash call, trash failure toast, no
  secrets in the log. Unit tests in `core/src/passcli.rs` cover both item JSON
  shapes, TOTP, URLs and detection of uncopyable data. Not verified: the real
  `item view --output json` shape (parser accepts the shapes of `item list`
  plus the `{"item": ..., "attachments": ...}` wrapper from the source), real
  template acceptance of every kind, and whether the real CLI lists the
  attachments of an item in that output.

#### 2FA codes

- Logins with a TOTP secret (the login's `totp_uri` or an extra field of kind
  totp; only the fact is cached, never the secret) get an extra menu item
  "2FA code - username". It runs `pass-cli item totp --share-id S --item-id I
  --output json`, checks the page URL again like the fill, and types the code
  into the first visible input that looks like an OTP field
  (`autocomplete=one-time-code`, a name/id/label such as otp/2fa/code/token,
  or `inputmode=numeric` with maxlength 6 to 8; split one-digit boxes are not
  handled). Otherwise the code is copied to the clipboard, a toast says "2FA
  code copied", and the clipboard is cleared after 30 s if it still holds the
  code. The clipboard is only written after the user picked that menu item.
- After a password fill nothing else happens: pick the 2FA item when the code
  page shows. A login with several TOTP fields gets the field named `totp`,
  else the first by name.

### Ad blocking (scriptlets and YouTube rules)

- Local mock YouTube watch page (rules rewritten from youtube.com to
  youtube.localhost): inline `ytInitialPlayerResponse.adPlacements` became
  `undefined`; `JSON.parse` output lost `adPlacements` and `playerAds`; the XHR
  player response lost `adPlacements` through `adSlots`; the fetch response had
  `adPlacements`/`adSlots` renamed to `no_ads`. With blocking off, all of these
  were unchanged.
- Scriptlet injection timing checked for: startup tab, new tabs, same-site and
  cross-site navigation, server redirect to another host, and reload.
- Real sites load with blocking on: CNN, Reddit, GitHub, BBC, Wikipedia, Vimeo.
- Unit tests: trusted vs untrusted lists, aliases, json-prune, CSS. An ignored
  test checks that the real lists produce all YouTube rule families (script
  about 78 KB for youtube.com watch pages; generic CSS about 17 KB on news sites).

### Sessions and crashes

- Quit with `Ctrl+Shift+Q`, then restart: tabs are restored.
- `kill -9`, then restart: tabs are restored.

### Tab sleeping

- An idle tab frees its browser and wakes when selected.

### Internal pages

- History, downloads, settings and new tab pages render.

### Look and window frame

- Redesigned toolbar: the active tab merges into the toolbar, separators between
  inactive tabs, coloured letter icons for sites without a favicon, the address
  bar shows the site's domain in full ink when not focused, purple tint in
  private windows. Internal pages share a design system with a sidebar, toggle
  switches and day-grouped history; checked in dark and light themes.
- Frameless window by default: the tab strip is the title bar (CSS
  `app-region: drag`, reported through CEF's drag handler and applied with
  `Window::set_draggable_regions`). Dragging the strip moved the window under
  openbox; minimize/maximize/close buttons are drawn by the UI.
  Settings > Appearance > "Use system title bar" (`system_titlebar`) turns the
  desktop's title bar back on (takes effect for new windows).
- Resizing: dragging the bottom-right corner (1280x860 -> 1130x760) and the
  left edge resized the window under openbox. Frameless windows keep a 4 px
  frame because web content is a native child window that would swallow edge
  clicks. (Before this fix windows were not resizable at all: cef-rs defaults
  `WindowDelegate::can_resize/can_maximize/can_minimize` to false.)
- Maximized and fullscreen windows drop the resize frame (content reaches the
  screen edge; checked by screenshot), and it returns when restored. F11 and
  HTML5 video fullscreen were checked; in HTML5 fullscreen the video fills the
  screen and the UI strip is hidden.
- "Use system title bar": a new window gets the window manager's title bar and
  no Ferrite window buttons (checked under openbox).
- Not checked: GNOME/KDE window managers, Wayland.

### Vertical tabs (default layout)

- Default since the design review scored it 9/10 (top tabs: 8.7/10). Settings >
  Appearance > "Vertical tabs" switches back to tabs above the toolbar (new
  windows; existing profiles keep their saved choice). Tabs
  move to a 240 px sidebar left of the content (a second UI view running
  `chrome.html#side` against the same API); the top strip keeps the window
  buttons and drag area. Checked: layout after restart with restored tabs,
  switching tabs and opening a new tab by mouse in the sidebar.
- Not checked: drag-reordering in the sidebar, many tabs (scrolling).

### Packaging

- `scripts/package.sh --deb-only` built `dist/ferrite_0.1.0_amd64.deb` (134 MB).
  `apt install ./dist/...deb` resolved its dependencies and installed it on
  Ubuntu 24.04. Checks: `/opt/ferrite/chrome-sandbox` is `-rwsr-xr-x root`,
  `x-www-browser` points to `/usr/bin/ferrite`, the desktop file passes
  `desktop-file-validate` and is in the MIME cache for http/https, and the
  icons are installed. `ferrite` started from `/usr/bin` and opened a window
  with `WM_CLASS "ferrite", "Ferrite"` (matching `StartupWMClass`).
- WM_CLASS is set with Xlib: cef-rs loses strings written to
  `LinuxWindowProperties`, and Chromium's `--class` switch does not reach
  Views windows.
- AppImage: `scripts/package.sh --appimage-only` built a 170 MB
  `Ferrite-0.1.0-x86_64.AppImage`; it started and opened a window (run with
  `APPIMAGE_EXTRACT_AND_RUN=1`, since the container has no FUSE).
- Not tested: the AppArmor profile (no AppArmor in the
  container), the sandbox from the installed package (tests run as root with
  `--no-sandbox`), and `xdg-settings` on a real desktop.

### Release bundle

- The release bundle in `dist/` runs standalone with the embedded UI.

### Media playback

- WebM (VP9) video plays: 960x540, `currentTime` advances (MDN's `flower.webm`
  served locally); MP3 audio plays.
- MP4 (H.264/AAC) does not: `DEMUXER_ERROR_NO_SUPPORTED_STREAMS`, as expected
  for CEF builds without proprietary codecs (see Known limitations).

### Web platform probe

- WebAssembly: yes. IndexedDB: yes. localStorage: yes. Service Worker API: yes.
- WebSocket (wss echo): yes. RTCPeerConnection: yes. MediaSource: yes.
- VP9, AV1 and Opus playback: `canPlayType` returns "probably".

## Not verified

- YouTube video: www.youtube.com redirected to Google's "unusual traffic" page
  for the test container's datacenter IP. Through an embed iframe the YouTube
  player loaded and rendered but answered "Video unavailable" (server-side
  refusal for this IP). Opening an `/embed/` URL directly shows "Error 153",
  which YouTube shows in any browser without an embedding page. YouTube serves
  VP9/AV1 + Opus, which play in Ferrite (see Media playback).
- Gmail and Google Docs: only the sign-in page was reached (no account).
- Login and OAuth flows: not tested end to end.
- Hardware acceleration and WebGL on a real GPU. WebGL was unavailable in the
  container (no GPU; the SwiftShader WebGL fallback is off by default).
- Webcam and microphone, WebRTC calls, notifications display, picture-in-picture,
  PDF viewer, printing, drag and drop of files, IME input, HiDPI, Wayland,
  multi-monitor.
- Single-instance relaunch forwarding (implemented, not tested).
- Downloads "ask where" (uses CEF's dialog).
- Multiple profiles: only through `--profile`; no UI.

## Known limitations

### Media and DRM

- No H.264 or AAC in the standard CEF build (`canPlayType` returns "" for
  `avc1` and `mp4a`). Sites that serve only H.264 (some news players,
  X/Twitter video) will not play; YouTube serves VP9/AV1 and plays.
  Without building Chromium: `scripts/setup-cef.sh --codecs` downloads a
  third-party codec build (obs-rust, CEF 156.0.8, sha256-pinned) and installs
  it into `~/.local/share/cef-codecs`; it is an older Chromium patch release.
  Building it yourself: `scripts/build-cef-codecs.sh` (about 120 GB of disk).
  H.264/AAC playback was verified with the third-party build. See
  docs/CODECS.md. `ferrite://about/` shows which codecs a build supports.
- Widevine DRM works in the standard build: Chromium's component updater
  downloads the Widevine CDM into the profile (`<profile>/cef/WidevineCdm/`)
  on first run; it is used after a restart. Verified: Widevine-encrypted VP9
  test content (Shaka demo assets) played. Services that require H.264 with
  Widevine, or hardware-backed DRM (Netflix in HD), need the codec build and
  may still refuse a non-Chrome browser; Netflix/Spotify/Disney+ were not
  tested (no accounts).

### Proton Pass

- Fills logins from Proton Pass through Proton's official `pass-cli`
  (Settings > Passwords; run `pass-cli login` once; needs a Proton Pass plan
  with CLI access). The browser extension cannot run in CEF.
- Verified with a fake `pass-cli` only (no Proton account in the dev
  container): the key button appears on a matching site, the native menu
  lists "username — title", choosing it fills username and password and fires
  input/change events; no fill on a non-matching host, none when the page
  navigated while pass-cli ran; not-logged-in and missing-binary states show
  in Settings; web pages cannot call the pass API.
- Not verified: the real pass-cli JSON (the parser accepts both plausible
  shapes), React-heavy login pages, logins inside iframes (only the main frame
  is filled), TOTP codes (not implemented), saving new logins (not
  implemented).
- Matching: same host or a subdomain of the saved URL's host (ports ignored);
  an https login is never filled into an http page. The fill runs in the
  page, so the page can read the password once filled (as with any autofill).

### Ad blocking

- Real YouTube is not verified: www.youtube.com is blocked from the test
  container's IP (see Not verified). YouTube changes often. uBO fixes arrive
  through `quick-fixes.txt`, which is picked up daily, but a new rule that needs
  a newer scriptlet only works after a Ferrite update.
- Popups opened by pages (`window.open`, `target=_blank` adopted as tabs) get
  scriptlets only from their second page or after a reload.
- Cross-process iframes (for example YouTube embeds on other sites) get no
  scriptlets. Network blocking still applies to them.
- Procedural cosmetic filters (`:has-text()` etc.) and generic class/id hiding
  are not applied.
- uBO `$redirect` rules act as plain blocks; no redirect resources are served.
- Private windows behave the same as normal windows.

### Not implemented

- A built-in password manager. Ferrite fills from and saves to Proton Pass
  only; there is no update/change-password flow yet.
- Extensions.
- Spell-check UI.
- Reader mode.
- Per-site zoom memory (Chromium may keep it).
- Automatic download and installation of updates (only the check exists).
- Overlay suggestions dropdown: the dropdown pushes the page down instead of
  overlaying it.
- Download shelf.

### Behaviour

- `beforeunload` is skipped when a whole window closes or a tab is put to
  sleep. Unsaved form input in such tabs can be lost. Closing a single tab does
  run `beforeunload`.
- Chromium writes harmless D-Bus errors to stderr in containers.

### Platforms

- Linux with X11 is the only tested platform. Wayland sessions work through
  XWayland, which is untested.
- macOS and Windows builds have not been attempted. The code has a `cfg(macos)`
  mapping of `Cmd` for shortcuts only. macOS needs an app bundle and helper
  processes through the cef-rs bundle tooling. Windows needs a sandbox bootstrap.

## Project skills for Claude Code

`/home/user/test/.claude/skills/` (repository root) holds skills written for
this project after a survey of public skills found nothing for CEF, Xvfb or
browser security:

- `ferrite-e2e`: start/stop an isolated headless instance, drive it over CDP
  and xdotool, screenshots, `scripts/smoke.sh` (passes).
- `ferrite-cef`: cef-rs patterns, threading, the re-entrancy rule, panic safety
  (cef-rs has no `catch_unwind`: a panic in a callback aborts the browser).
- `ferrite-security`: threat model, invariants, review checklist, abuse tests.

## How to resume work

1. Install the prerequisites: Rust stable, `cmake`, `ninja-build`, `g++`,
   `pkg-config`.
2. Download CEF: `scripts/setup-cef.sh`.
3. Run a development build: `scripts/run.sh [URL]`. Use `FERRITE_UI_DIR=$PWD/ui`
   to load UI files from disk.
4. Run the checks: `cargo test`, `cargo fmt --all`, `cargo clippy --all-targets`.

For headless testing:

1. Start a virtual display and a window manager: `Xvfb :99 &`, `openbox &`, with
   `DISPLAY=:99`.
2. Start Ferrite with `--remote-debugging-port=9222`.
3. Connect with Playwright: `chromium.connectOverCDP('http://127.0.0.1:9222')`.
   The UI strip is the page `ferrite://app/chrome.html`. It exposes `api('state')`
   and other commands.
4. Use `xdotool` to send real keyboard and mouse input.

## Roadmap

In priority order:

1. Procedural and generic cosmetic filtering.
2. Password manager on the OS secret store (Secret Service through `keyring`).
   No homemade cryptography.
3. Overlay suggestions dropdown (CEF overlay view) and a download shelf.
4. Document a proprietary-codec CEF build option.
5. Packaging: `.deb` or AppImage, desktop file, and registration as the default
   browser through xdg.
6. macOS and Windows ports.
7. Per-site settings UI (zoom, JavaScript, cookies).
8. `beforeunload` handling when a window closes or a tab sleeps.
