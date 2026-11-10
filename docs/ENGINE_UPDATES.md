# Engine updates

Ferrite's engine consists of two parts that must match:

- the CEF binary distribution (`libcef.so` and resources), from the Spotify CDN;
- the Rust bindings crate `cef` (tauri-apps/cef-rs), pinned exactly in
  `app/Cargo.toml`.

Versions of the `cef` crate have the form `<cef major>.<minor>.<patch>+<cef build>`,
for example `156.4.0+156.0.26`. The part after `+` is the CEF build. The current
pin is `cef = { version = "=156.4.0" }`, which uses CEF 156.0.26 and Chromium
156.0.8078.88.

## When to update

Chromium security fixes arrive every one to four weeks. Update at least monthly.

## Procedure

1. Find the new `cef` crate release on crates.io or in the tauri-apps/cef-rs
   releases. Note its CEF build number.
2. Bump the pin in `app/Cargo.toml`:

   ```toml
   cef = { version = "=X.Y.Z", default-features = false, features = ["sandbox"] }
   ```

3. Download the matching CEF distribution:

   ```sh
   scripts/setup-cef.sh
   ```

   The script reads the pin from `app/Cargo.toml`, installs the matching
   `export-cef-dir` with `cargo install --locked`, and exports CEF to
   `$CEF_PATH` (default `~/.local/share/cef`).

4. Build:

   ```sh
   export CEF_PATH=$HOME/.local/share/cef
   cargo build --release
   ```

5. Fix API changes. The bindings are generated from the CEF headers, so compile
   errors show changed signatures. The client handlers are in
   `app/src/browser/handlers.rs` and the engine code is in `app/src/engine/`.

6. Run the tests and the checks:

   ```sh
   cargo test
   cargo clippy --all-targets
   cargo fmt --all
   ```

7. Run the smoke checklist (see below).

8. Rebuild the bundle: `scripts/bundle.sh`.

## Smoke checklist

- Load a few sites (for example a search page, a news site and a web app).
- Tabs: new, close, switch, reopen closed tab.
- Downloads: complete a download and check the downloads page.
- Popups: a `target=_blank` link opens a tab; `window.open` opens a window.
- Permissions: a geolocation prompt appears, and Block with Remember is kept.
- Private window: no cookies or history are shared with a normal window.
- Session: quit and restart restores tabs.

## Version check and mismatches

- The CEF version is checked at runtime through `api_hash`. A mismatched
  `libcef.so` and `cef` crate fail at start.
- Always ship `libcef.so` from the same CEF build as the crate. `scripts/bundle.sh`
  copies it from `$CEF_PATH`, so rebuild `$CEF_PATH` with `setup-cef.sh` first.

## Known cef-rs quirk

`CefStringList::clone` loses borrowed lists. `engine::read_string_list` therefore
reads lists through the C API. Keep that helper when updating the crate, and
check whether the quirk still exists in the new release.
