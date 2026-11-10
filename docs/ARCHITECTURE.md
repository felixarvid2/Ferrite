# Architecture

Ferrite is a Cargo workspace with two crates:

- `core` (`ferrite-core`): pure Rust, no CEF. Storage, omnibox logic, settings,
  sessions, shortcuts. It can be tested without a browser engine.
- `app` (binary `ferrite`): CEF integration, windows, tabs, handlers and the
  UI bridge.

The browser UI is HTML, CSS and JavaScript in `ui/`. It is embedded into the
binary with `include_bytes` (`app/src/browser/assets.rs`). Setting
`FERRITE_UI_DIR` serves the files from disk instead.

## Engine

- Chromium 156.0.8078.88 through CEF 156.0.26. The CEF minimal prebuilt
  distribution is downloaded from the Spotify CDN; Chromium is never compiled
  locally.
- Rust bindings: crate `cef` 156.4.0 (tauri-apps/cef-rs), pinned exactly in
  `app/Cargo.toml`. The bindings are generated from the CEF headers.
- Rust stable 1.97.
- Storage: rusqlite with the bundled SQLite.

## Views and windows

Each window is a CEF Views window with a vertical BoxLayout:

1. Top: a privileged BrowserView for `ferrite://app/chrome.html`. It contains
   the tab strip, toolbar, address bar with suggestions dropdown, bookmarks bar,
   find bar, permission prompt bar, crash bar and toasts. The page reports its
   own height with a ResizeObserver, and Rust resizes the strip to match.
2. Below: a content Panel with a FillLayout. It holds one BrowserView per tab.
   Only the active tab's view is visible.

All browser views use the CEF Alloy runtime style.

### Why Alloy and not Chrome style

CEF refuses to add more than one Chrome-style BrowserView to a window ("Cannot
add multiple Chrome style BrowserViews",
`libcef/browser/views/browser_view_impl.cc:321`). A Chrome-style view is
therefore not usable for the tab content, and the window would have only one
tab. Alloy uses the same Blink, V8 and network stack, so web content behaves
the same. The features that Alloy does not provide are implemented in Ferrite:

- Context menu entries: open link in new tab, window or private window; save
  link; copy link address (through the `arboard` clipboard crate); open, save
  and copy image; search selection; Inspect.
- Permission prompts (UI).
- Popup windows.

JavaScript dialogs (alert, confirm, prompt) and the file chooser (GTK) come from
CEF and were verified to work.

Trade-offs of Alloy: no Chrome extensions, no Chrome password manager or
autofill UI, no Chrome print preview (the system print dialog is used).

## Threading and state

- All browser state lives in a `thread_local` `State` (`app/src/browser/mod.rs`)
  on the CEF UI thread.
- CEF callbacks that arrive while state is borrowed do not touch the state
  directly. They post a closure with `later` and run it later on the UI thread.
  This avoids re-entrant borrows.
- Task posting helpers are in `app/src/engine/task.rs`.

## Modules (`app/src`)

| Path | Role |
|---|---|
| `main.rs` | Command line parsing and startup |
| `engine/` | `App`, process handler, custom scheme registration, task helpers |
| `browser/mod.rs` | `State` |
| `browser/window.rs` | Window creation and layout |
| `browser/tabs.rs` | Tab lifecycle, popups as tabs/windows, reorder, pinning, tab sleeping |
| `browser/handlers.rs` | CEF client handlers (display, load, life-span, request, keyboard, find); content blocking and DNT/GPC headers |
| `browser/scheme.rs` | `ferrite://` scheme and navigation guard |
| `browser/api.rs` | JSON API used by the UI |
| `browser/commands.rs` | Executes browser commands from shortcuts, menus and the UI |
| `browser/menu.rs` | Native CEF Views menus: main menu and tab context menu |
| `browser/context_menu.rs` | Web page context menu |
| `browser/downloads.rs` | Download handling |
| `browser/permissions.rs` | Permission requests and decisions |
| `browser/session.rs` | Session save and restore |
| `browser/theme.rs` | Dark, light and system theme |
| `browser/assets.rs` | Embedded UI files |

## Internal pages and IPC

Internal pages use the `ferrite://` scheme. Examples: `ferrite://app/chrome.html`,
`ferrite://settings`, `ferrite://history`, `ferrite://bookmarks`,
`ferrite://downloads`, `ferrite://about`, and the new tab page.

### UI to Rust

The UI calls Rust with:

```js
fetch('/api/<command>?a=<json>', { method: 'POST' })
```

The request goes to the page's own `ferrite://` origin. Arguments are in the
query string because the request body reaches CEF scheme handlers as a data pipe
that cannot be read.

The scheme handler factory runs on the IO thread and answers on the UI thread.
The API is served only when all of these hold:

- The request comes from a main frame whose current URL is `ferrite://`.
- The resource type is XHR or fetch.
- The method is POST.

Some commands are accepted only from the UI strip. These are: `set_height`,
menu commands, tab activation, move and pin, and permission decisions.

### Rust to UI

Rust calls `frame.execute_java_script("window.ferrite.onState(<json>)")` with a
full snapshot of the window state. Updates are debounced.

Static UI files without data may be served while the frame URL is still empty
during the first load. CEF reports an empty frame URL and referrer at that
time.

## Security

- Web pages cannot navigate to `ferrite://`. The `on_before_browse` guard allows
  only browser-initiated loads, loads from internal pages, and fresh tabs.
- Web pages cannot open `ferrite://` URLs in popups.
- Web pages cannot embed `ferrite://` pages in iframes. Non-main frames are
  rejected.
- Internal pages send a Content Security Policy: no inline scripts and
  `frame-ancestors none`.
- Certificate errors are never bypassed. `on_certificate_error` returns false,
  and Chromium shows its error page.
- The lock icon uses the page's real TLS status from the NavigationEntry.
- The Chromium sandbox stays enabled. Ferrite refuses to start as root unless
  `--no-sandbox` is passed.
- Private windows use an in-memory RequestContext with an empty cache path. It
  is shared by private windows and dropped when the last one closes. Private
  history, downloads and permissions go to an in-memory database.

## Popups and windows

- `target=_blank` and tab dispositions open a tab next to the opener. The popup
  BrowserView is adopted into the window.
- `window.open` with features (for example OAuth or login windows) opens its own
  Ferrite window with the address bar visible. `window.opener` is preserved.
- Middle-click and Ctrl-click open background tabs through `on_open_urlfrom_tab`.
- DevTools open in a CEF default window.

## Ad and tracker blocking

`ferrite-core` (`blocker.rs`) uses the `adblock` crate with EasyList and
EasyPrivacy. The lists are cached in the profile's `filter-lists/` directory and
refreshed when older than 4 days (`ureq`, OS certificate verifier). The app
checks each sub-resource request in a CEF `ResourceRequestHandler` on the IO
thread. Top-level navigations are never blocked. The feature is off by default
and is switched on in Settings > Privacy. Unit-tested in core and verified live (see STATUS.md).

## Sessions

- The session is written continuously to `session.json`, with atomic writes.
  It is written with `clean_exit=false`, so a crash can be recovered.
- On start, all normal windows are restored with their tabs, pinned state,
  active tab, bounds and maximized state. Background tabs are restored lazily.
- Settings choose between continuing where you left off, a new tab page, or the
  homepage. Restore after a crash can be turned on or off.

## Tab sleeping

Idle background tabs are unloaded after a configurable time (settings: 0, 15,
30, 60 or 240 minutes; default 60). Pinned tabs, tabs playing media, loading
tabs and internal pages are skipped. A sleeping tab reloads when selected.

The idea comes from the Plume browser. See [NOTES-plume.md](NOTES-plume.md).

## Single instance

A second launch for the same profile is meant to forward its URLs to the
running instance through CEF's process singleton (`on_already_running_app_relaunch`).
This is implemented but not tested.

## Profiles

Profiles are selected with `--profile=NAME` and stored under
`<data dir>/profiles/NAME/`. See the README for the directory layout.
