# Status

Ferrite is an early-stage browser. This page lists what has been checked, what
has not, and what is missing. It was last updated for the Chromium 156 build
(CEF 156.0.26, `cef` 156.4.0).

Test environment for the "verified" list: Ubuntu 24.04 container, Xvfb with
openbox, no GPU, run as root with `--no-sandbox`. Input was sent with real X
events (xdotool) and results were read through the Chrome DevTools Protocol.
Network traffic went through an HTTPS-intercepting proxy.

## Ad and tracker blocking

- Implemented in `ferrite-core` (`blocker.rs`) with the `adblock` crate (0.13),
  using EasyList and EasyPrivacy. Unit-tested in core.
- Filter lists are cached in the profile's `filter-lists/` directory. They are
  refreshed when older than 4 days, using `ureq` with the OS certificate verifier.
- Requests are checked in a CEF `ResourceRequestHandler` on the IO thread.
  Top-level navigations are never blocked.
- Off by default. Toggle: Settings > Privacy, "Block ads and trackers".
- Verified live: both lists downloaded (2.0 MB + 1.5 MB) and the engine loaded;
  on cnn.com ad and tracking requests were cancelled (Adobe Launch, Optimizely,
  Google IMA SDK, tracking pings) while the page loaded normally.
- Not yet: per-site "disable blocking on this site", cosmetic (element hiding)
  filters, a blocked-request counter in the toolbar.

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

### Privacy

- Private window isolation: cookies and localStorage from the private window are
  not visible in a normal window; no history entry is written.
- A web page link to `ferrite://settings` is blocked.

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

- No H.264 or AAC. The standard CEF builds do not include proprietary codecs
  (`canPlayType` returns "" for `avc1` and `mp4a`). Sites that serve only H.264
  (some news video players, X/Twitter video, some streaming) will not play.
  YouTube normally serves VP9 or AV1 with Opus, so it should play.
  Fixing this needs a CEF build with `proprietary_codecs=true
  ffmpeg_branding=Chrome`, which has license implications.
- No Widevine DRM. Netflix, the Spotify web player and Disney+ will not play.

### Not implemented

- Password manager.
- Import from Chrome or Firefox.
- Extensions.
- Spell-check UI.
- Reader mode.
- Per-site zoom memory (Chromium may keep it).
- Automatic updates.
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

1. Per-site "disable blocking on this site" and cosmetic filtering.
2. Password manager on the OS secret store (Secret Service through `keyring`).
   No homemade cryptography.
3. Overlay suggestions dropdown (CEF overlay view) and a download shelf.
4. Document a proprietary-codec CEF build option.
5. Import bookmarks and history from Chrome and Firefox: copy the database files
   (including WAL), then read the copies with rusqlite.
6. Packaging: `.deb` or AppImage, desktop file, and registration as the default
   browser through xdg.
7. macOS and Windows ports.
8. Per-site settings UI (zoom, JavaScript, cookies).
9. `beforeunload` handling when a window closes or a tab sleeps.
