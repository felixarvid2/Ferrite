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

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design.

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
Cargo.toml            workspace (members: core, app)
core/                 ferrite-core: pure Rust, no CEF
  src/db.rs           SQLite access (rusqlite, bundled)
  src/history.rs, bookmarks.rs, permissions.rs, downloads.rs
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
