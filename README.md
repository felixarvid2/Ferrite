# Ferrite

Ferrite is a desktop web browser written in Rust. It uses Chromium through the
Chromium Embedded Framework (CEF) and adds its own browser UI: tabs, address
bar, history, bookmarks, downloads, settings, sessions and permission prompts.
Chromium supplies the rendering, JavaScript and network stack; Ferrite does not
compile Chromium.

Ferrite is at an early stage and is only tested on Linux/X11. See
[docs/STATUS.md](docs/STATUS.md) for what has been verified and what has not.

## Overview

- Engine: Chromium 156.0.8078.88 via CEF 156.0.26, through the Rust bindings
  crate `cef` 156.4.0 (pinned exactly in `app/Cargo.toml`).
- Browser state (history, bookmarks, permissions, downloads list) is stored in
  SQLite. Settings and session are JSON files.
- The browser UI is HTML, CSS and JavaScript in `ui/`, embedded in the binary.
- Private windows use an in-memory profile that is discarded when the last
  private window closes.
- Tab sleeping unloads idle background tabs to save memory.
- Proton Pass logins can be filled, saved after you sign in, and 2FA codes
  fetched, through Proton's `pass-cli` (Settings > Passwords).
- Bookmarks and history can be imported from Chrome, Chromium, Brave, Edge, Vivaldi
  and Firefox profiles (Settings > Import). Passwords are not imported.
- Ad and tracker blocking uses EasyList, EasyPrivacy and uBlock Origin's lists
  and scriptlets. It is on by default for new profiles (Settings > Privacy).
  See [docs/STATUS.md](docs/STATUS.md) for limits.
- Update check: Ferrite asks `api.github.com` once a day whether a newer release
  exists and offers a link in the menu and on `ferrite://about/`. It sends one
  anonymous request with the user agent `Ferrite/<version>` and installs
  nothing. Turn it off in Settings > Privacy and security (Check for updates).

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design.

## Controlling Ferrite from an AI (MCP)

Ferrite can be driven by an AI client (Claude Desktop, Claude Code, any MCP client) through the Model
Context Protocol. It is off by default.

1. In Ferrite, open Settings > AI control (MCP) and turn on **Allow AI control**. Settings shows the
   socket path and a ready-to-paste client configuration.
2. Add the `ferrite-mcp` program to the client. It sits next to `ferrite` (`/usr/bin/ferrite-mcp` in the
   `.deb`, `dist/ferrite/ferrite-mcp` in a bundle, `target/release/ferrite-mcp` in a source build).
   Claude Desktop (`claude_desktop_config.json`) or a Claude Code `.mcp.json`:

   ```json
   { "mcpServers": { "ferrite": { "command": "/usr/bin/ferrite-mcp", "args": [] } } }
   ```

   Claude Code: `claude mcp add ferrite -- /usr/bin/ferrite-mcp`. For another profile add
   `"--profile=NAME"` to `args`. Ferrite must be running; the client may start before it (tool calls then
   say Ferrite is unreachable).

How it works and what it can do:

- **Tools**: `list_tabs`, `open_tab`, `navigate`, `close_tab`, `back`, `forward`, `reload`, `snapshot`
  (page text with element references), `screenshot` (PNG), `click`, `type`, `wait_for`, and `evaluate`.
  Tab ids come from `open_tab`/`list_tabs`; references like `e12` come from the latest `snapshot`.
- **Isolated by default**: `open_tab` opens tabs in a separate **AI window** (teal, badge "AI window").
  It has its own in-memory browser context: no cookies, logins or storage from your profile, nothing is
  kept after the last AI window closes, no history, not in the saved session. It is not a private
  window and it is separate from them.
- **Hand over a tab**: right-click a tab > **Let AI control this tab**. The tab keeps your profile
  (cookies, logins), so the AI can act as you on that site. Tabs you did not hand over are invisible to
  the AI. **Take back control from AI** in the same menu, or the **Stop** button of the bar
  &ldquo;AI is controlling this tab&rdquo;, revokes control immediately (calls in flight fail).
  Controlled tabs also carry an AI badge in the tab strip. Private windows can never be handed over.
- **What the AI cannot do**: open `ferrite://` pages, the settings, downloads or file opening, use
  Proton Pass, or navigate to anything but `http(s)` (and `about:blank`). Downloads and permission
  prompts (camera, location, ...) from a controlled tab are refused. A handed-over tab that you (or a
  link) take to an internal page leaves AI control.
- **`evaluate`** (run JavaScript in the page) is a second switch, **Allow running JavaScript**, off by
  default. A script can read everything on the page, including text you filled in.
- **Untrusted content**: text from pages is returned between `<untrusted_web_content id="...">` markers
  with a random id per response, and password field values are never included. Clients and models
  should treat it as data, not instructions (prompt injection is the main risk of letting an AI browse).
- **Transport**: MCP on stdio to `ferrite-mcp`, which talks to the browser over a Unix socket in the
  profile directory (`mcp.sock`, mode 0600, same user only). There is no TCP or HTTP listener, so web
  pages cannot reach it. Set `FERRITE_MCP_SOCKET` (for both programs) to use another path if the default
  is too long for a Unix socket. The AppImage runs from a temporary mount: use the `.deb` or a bundle
  for a stable `ferrite-mcp` path.
- **Protocol**: current MCP revision 2026-07-28 (stateless, `server/discover`) and the handshake-based
  revisions 2025-11-25 down to 2024-11-05. Tools only.

## Android

[android/](android/README.md) is Ferrite for Android: a browser on Android System WebView with the
same ad blocking (adblock-rust, EasyList/EasyPrivacy, uBlock Origin scriptlets), tabs, bookmarks, history
and downloads. One APK, no Proton Pass, no extensions. CI: `.github/workflows/ferrite-android.yml`.

## Status

See [docs/STATUS.md](docs/STATUS.md) for verified features, untested areas,
known limitations and the roadmap.

## Requirements

- Ubuntu 24.04 (other Linux distributions are untested).
- Rust stable 1.97 (`rustup`).
- Build tools: `cmake`, `ninja-build`, `g++`, `pkg-config`. The `cef-dll-sys`
  crate builds `libcef_dll_wrapper` with cmake and ninja.
- Runtime: an X11 desktop session (Wayland through XWayland is untested) and
  GTK 3 for the file chooser.
- The release bundle (`dist/ferrite/`) is about 563 MB.

## Install & build

```sh
# 1. Download the prebuilt CEF distribution matching the cef crate into ~/.local/share/cef
scripts/setup-cef.sh

# 2. Build
export CEF_PATH=$HOME/.local/share/cef
cargo build --release

# 3. Create a standalone bundle in dist/ferrite/
scripts/bundle.sh
```

`scripts/setup-cef.sh` installs `export-cef-dir` at the version matching the
pinned `cef` crate, then exports the CEF distribution to `$CEF_PATH`.

`scripts/bundle.sh` copies the binary, `libcef.so` (stripped from about 1.4 GB
to about 265 MB), Chromium resources and `CHROMIUM-CREDITS.html` into
`dist/ferrite/`.

### Sandbox setup

Ubuntu 23.10 and later restrict unprivileged user namespaces with AppArmor, so
Chromium's sandbox may fail to start. Install the setuid helper once:

```sh
sudo chown root:root dist/ferrite/chrome-sandbox && sudo chmod 4755 dist/ferrite/chrome-sandbox
```

`scripts/bundle.sh` prints this command. Ferrite refuses to run as root unless
`--no-sandbox` is passed explicitly, because Chromium cannot sandbox as root.

## Run

```sh
# Development build, from the source tree
scripts/run.sh [URL]

# Release bundle
dist/ferrite/ferrite [--profile=NAME] [--private] [--no-sandbox] [URL...]
```

Options:

| Option | Meaning |
|---|---|
| `--profile=NAME` | Use profile `NAME` (default: `default`). |
| `--private` | Open a private window. |
| `--no-sandbox` | Disable Chromium's sandbox. Needed only as root. |
| `--switch[=value]` | Any other `--` switch is passed to Chromium. |
| `URL...` | URLs to open. |

Environment variables:

| Variable | Meaning |
|---|---|
| `FERRITE_DATA_DIR` | Base data directory (default `~/.local/share/ferrite`). |
| `RUST_LOG` | Log filter for `tracing` output, e.g. `RUST_LOG=ferrite=debug`. |
| `FERRITE_UI_DIR` | Serve the UI files from this directory instead of the embedded copy. Useful while editing `ui/`. |
| `FERRITE_UPDATE_URL` | For tests only: replaces the GitHub releases URL of the update check (https, or http on localhost). |

## Keyboard shortcuts

On Linux and Windows, `Ctrl` is the modifier. On macOS, `Cmd` is used.

| Action | Shortcut |
|---|---|
| New tab | `Ctrl+T` |
| New window | `Ctrl+N` |
| New private window | `Ctrl+Shift+N` |
| Close tab | `Ctrl+W`, `Ctrl+F4` |
| Close window | `Ctrl+Shift+W` |
| Reopen closed tab | `Ctrl+Shift+T` |
| Focus address bar | `Ctrl+L`, `Alt+D`, `F6` |
| Open address in new tab | `Alt+Enter` (in address bar) |
| Back / forward | `Alt+Left` / `Alt+Right` |
| Home | `Alt+Home` |
| Reload | `Ctrl+R`, `F5` |
| Hard reload | `Ctrl+Shift+R`, `Shift+F5` |
| Next tab | `Ctrl+Tab`, `Ctrl+PgDn` |
| Previous tab | `Ctrl+Shift+Tab`, `Ctrl+PgUp` |
| Select tab 1-8 | `Ctrl+1` ... `Ctrl+8` |
| Last tab | `Ctrl+9` |
| Find in page | `Ctrl+F` |
| Next / previous match | `Ctrl+G`, `F3` / `Ctrl+Shift+G`, `Shift+F3` |
| Close find bar | `Esc` |
| Bookmark page | `Ctrl+D` |
| Toggle bookmarks bar | `Ctrl+Shift+B` |
| Bookmarks manager | `Ctrl+Shift+O` |
| Downloads | `Ctrl+J` |
| History | `Ctrl+H` |
| Zoom in / out / reset | `Ctrl+Plus` / `Ctrl+Minus` / `Ctrl+0` |
| Fullscreen | `F11` |
| Developer tools | `F12`, `Ctrl+Shift+I` |
| View source | `Ctrl+U` |
| Print | `Ctrl+P` |
| Quit | `Ctrl+Shift+Q` |

## Packages (.deb / AppImage)

```sh
sudo apt install librsvg2-bin          # icon rendering for the packager
scripts/package.sh                     # both, or --deb-only / --appimage-only
sudo apt install ./dist/ferrite_0.1.0_amd64.deb
```

- **.deb** installs to `/opt/ferrite` with `/usr/bin/ferrite`, a desktop entry
  (handles http/https/HTML, "New Window" and "New Private Window" actions),
  icons, the setuid `chrome-sandbox` helper and an AppArmor profile that lets
  the sandbox use user namespaces on Ubuntu 23.10+. It also registers
  `x-www-browser`. To make it the desktop's default browser:
  `xdg-settings set default-web-browser ferrite.desktop`.
- **AppImage** is a single file with no install step. It cannot carry the
  setuid sandbox helper, so on Ubuntu 23.10+ (which restricts user namespaces)
  use the .deb. The AppImage prints a warning there and never disables the
  sandbox itself. The packager downloads `appimagetool` from the AppImage
  project's GitHub releases. Without FUSE, run it with
  `APPIMAGE_EXTRACT_AND_RUN=1`.

## Profiles & data locations

Each profile has its own directory. The default data directory is
`~/.local/share/ferrite`; set `FERRITE_DATA_DIR` to change it.

```
<data dir>/profiles/<NAME>/
    cef/              Chromium profile data (cookies, cache, local storage)
    ferrite.sqlite    history, bookmarks, site permissions, downloads list
    settings.json     browser settings
    session.json      open windows and tabs, written continuously
```

- Private windows keep history, downloads and permissions in an in-memory
  database and use an in-memory Chromium request context. The context is dropped
  when the last private window closes.
- "Clear browsing data" on the settings page removes history, cookies, cache,
  site storage or the download list. Site storage and cookies are cleared at the
  next start, before Chromium opens the profile.
- Profiles are selected only with `--profile=NAME`. There is no profile UI yet.

## Project layout

```
Cargo.toml            workspace (members: core, app, mcp)
core/                 ferrite-core: pure Rust, no CEF
  src/db.rs           SQLite access (rusqlite, bundled)
  src/history.rs, bookmarks.rs, permissions.rs, downloads.rs
  src/importer.rs     read and apply bookmarks/history from Chrome-family browsers and Firefox
  src/omnibox.rs      address bar input resolution and inline completion
  src/search.rs       search engine definitions and URL templates
  src/session.rs      session JSON with atomic writes
  src/closed_tabs.rs  closed-tab stack (25 per window)
  src/profile.rs      profile directory layout
  src/blocker.rs      ad and tracker blocking (adblock crate; off by default)
  settings JSON and the keyboard shortcut table are also in core/src
app/                  the `ferrite` binary: CEF integration
  src/main.rs         command line and startup
  src/engine/         CEF app, process handler, scheme registration, task helpers
  src/browser/        windows, tabs, handlers, ferrite:// scheme, JSON API,
                      menus, context menu, downloads, permissions, session, theme
mcp/                  ferrite-mcp: MCP bridge binary (stdio) and the protocol/policy library the
                      browser shares (JSON-RPC, tool catalogue, URL and tab policy)
ui/                   browser UI: chrome.html/css/js, settings, history,
                      bookmarks, downloads, newtab, about pages
scripts/              setup-cef.sh, run.sh, bundle.sh
docs/                 ARCHITECTURE.md, STATUS.md, ENGINE_UPDATES.md, NOTES-plume.md
```

## Development

```sh
export CEF_PATH=$HOME/.local/share/cef
cargo test                       # 41 tests: 40 in core, 1 navigation-guard test in app
cargo fmt --all                  # format
cargo clippy --all-targets       # lint (clean at the time of writing)
FERRITE_UI_DIR=$PWD/ui scripts/run.sh   # edit ui/ and reload without rebuilding
```

The app test binary finds `libcef.so` through an `$ORIGIN/..` rpath.

For the engine update procedure see [docs/ENGINE_UPDATES.md](docs/ENGINE_UPDATES.md).

## License & third-party

- Ferrite is licensed under the MIT License (see `LICENSE`).
- Chromium and CEF are licensed under the BSD-3-Clause license and other
  licenses listed in Chromium's credits. `scripts/bundle.sh` copies CEF's
  `CREDITS.html` into the bundle as `CHROMIUM-CREDITS.html`.
- The `cef` Rust bindings (tauri-apps/cef-rs) are licensed under Apache-2.0 or
  MIT.
- Other Rust dependencies have their own licenses; see `Cargo.lock`.
- uBlock Origin's scriptlets (`resources/adblock/ubo-scriptlets.json`) are GPL-3.0;
  they are a separate data file loaded at runtime, shipped with their licence
  in `adblock/` of the bundle. See `resources/adblock/README.md`.
