---
name: ferrite-e2e
description: End-to-end testing of the Ferrite browser (Rust + CEF/Chromium, this repository) in a headless container with Xvfb, openbox, Playwright-over-CDP and xdotool. Use when verifying a Ferrite change in the real browser, taking screenshots, driving tabs/address bar, or debugging UI behaviour.
---

# Ferrite end-to-end testing

Ferrite runs for real under a virtual display. The UI strip is a web page (`ferrite://app/chrome.html`)
that exposes `api(cmd, args)`; Playwright attaches over CDP to call it and to evaluate JS in web tabs,
and xdotool sends real keyboard/mouse events. Scripts are in `scripts/` (all have `--help`).

## Quick start

```sh
cd <repo root> && CEF_PATH=$HOME/.local/share/cef cargo build      # debug binary
S=.claude/skills/ferrite-e2e/scripts   # from the repo root
$S/smoke.sh                         # full sanity run, display :97, port 9444; prints SMOKE PASS
$S/ferrite-up.sh --display 97 --port 9444 https://example.com   # start (waits until api() is ready)
$S/ui.js "api('state')"             # UI strip: tabs, active, nav, bookmarks, downloads...
$S/tab.js example.com "document.title"   # JS inside a web tab
$S/shot.sh /tmp/a.png               # then view it with the Read tool
$S/ferrite-down.sh                  # kills only the recorded PIDs (SIGTERM, then SIGKILL)
```

`ferrite-up.sh` records PIDs/port/display in a state file (`$FERRITE_E2E_STATE`, default
`$TMPDIR/ferrite-e2e.state`); the other scripts read it. Use a distinct `--display`/`--port` per run
(other people's instances may be on :98/:99, 9222/9333). Setup it does: Xvfb + openbox (a WM is
needed, otherwise xdotool focus fails), isolated `FERRITE_DATA_DIR` (fresh temp dir unless `--data`),
`FERRITE_UI_DIR=ui` (UI edits need no rebuild, only reload), `--no-sandbox` (root in containers).
Rust changes need `cargo build` first; UI-only changes (`ui/`) do not.

## Recon, then act, then assert

1. Recon: `ui.js "api('state')"` (tabs, active id, `nav.url`, back/forward, find, zoom, security) and
   a screenshot. Do not guess ids or coordinates.
2. Act with one input method (see below).
3. Assert on `api('state')` or tab DOM, not on pixels. Poll until true (see flakes); screenshots are
   evidence for the report and for visual changes.

## UI API cheat-sheet (`ui.js "api('name', {args})"`, defined in `app/src/browser/api.rs`)

- `state` window snapshot; `navigate {input, tab?, new_tab?}`; `open_url {url, new_tab?, background?}`
- Tabs: `new_tab {url?, background?}`, `activate_tab {tab}`, `close_tab {tab?}`, `move_tab {tab,index}`,
  `pin_tab {tab,pinned}`, `duplicate_tab {tab?}`
- `command {name}` runs a keyboard command; name = snake_case of `ferrite_core::commands::Command`:
  `new_tab new_window new_private_window close_tab reopen_closed_tab focus_address_bar reload hard_reload
  stop back forward home next_tab prev_tab select_last_tab find find_next find_prev bookmark_page
  toggle_bookmarks_bar downloads history bookmarks settings zoom_in zoom_out zoom_reset fullscreen
  dev_tools view_source duplicate_tab pin_tab quit` (`select_tab` takes an index: check commands.rs)
- `find`/`find_close`, `focus_content`, `suggest {text}`, `permission`, `toggle_bookmark`
- Data: `history_search {q}`, `history_delete*`, `history_clear`, `bookmarks_tree`, `bookmark_add`,
  `bookmark_update/move/delete`, `downloads_list`, `permissions_list`, `top_sites`, `clear_browsing_data`
- Settings: `settings_get`, `settings_set {patch}`; `about`
- Several windows (e.g. a private one) = several chrome.html pages: pick with `FERRITE_WIN=<n> ui.js ...`
  and read `state.private`. Read api.rs for exact args when in doubt.

## Input: CDP/api vs xdotool

- Use `api(...)`/CDP for setup, navigation and state: deterministic, no focus or geometry issues.
- Use xdotool when the thing under test is real input: shortcuts (`xdotool key ctrl+t`, `ctrl+l`),
  typing into the address bar, clicks, context menus, key->command mapping in `commands.rs`.
- Click into the window first so it has focus: `xdotool mousemove 640 500 click 1`, then keys
  (`DISPLAY=:97 xdotool ...`; display number is in the state file).
- Geometry: window has a 20 px openbox title bar; content starts at y = 20 + UI strip height
  (`ui.js "document.body.offsetHeight"`, ~108 with the bookmarks bar). Screen default 1400x900.
- Mixed pattern: xdotool types a URL + Return, then poll `api('state')` for `nav.url`.

## Screenshots

`shot.sh out.png` = `import -window root` of the whole screen (strip + content + title bar); it
exits non-zero if the image is blank. Open with the Read tool and say what you checked. Popups,
native menus and the file chooser appear only in this X screenshot, not via CDP.

## Flake handling

- Never fixed sleeps for state: poll `api('state')` every 0.5 s with a 15-30 s cap (see `poll` in
  `scripts/smoke.sh`). A short `sleep 0.3` after a click before sending keys is fine.
- `ui.js`/`tab.js` themselves wait for the page; `tab.js` retries until a tab URL matches.
- Lazy background tabs have no browser until activated: `activate_tab` first, then `tab.js`.
- On failure grab `shot.sh`, `ferrite.log` (`$DATA/ferrite.log`; raise logging with
  `FERRITE_RUST_LOG=info,ferrite=trace`) and `curl 127.0.0.1:PORT/json/list` before retrying.

## Pitfalls

- Never `pkill ferrite`/`pkill Xvfb`: others run instances. Use `ferrite-down.sh` (PIDs only).
- Reusing a `--data` dir with a live instance fails on the profile lock (SingletonLock); use a fresh
  dir or stop the other run. State persists in a reused data dir (history, session restore).
- Root needs `--no-sandbox` (the script passes it). Chromium dbus errors in the log are harmless.
- No GPU: no WebGL. YouTube shows Google's `/sorry` page from datacenter IPs; use example.com or a
  local `python3 -m http.server` for deterministic pages.
- If `ferrite-up.sh` says display/port in use, pick others; a stale `/tmp/.X11-unix/X<N>` from a
  crashed Xvfb is removed by `ferrite-down.sh`.
- Playwright is global here (`require('playwright')`); `cdp-lib.js` falls back to `npm root -g`.
