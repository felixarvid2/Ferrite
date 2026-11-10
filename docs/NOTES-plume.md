# Notes: study of Plume

Plume is a Rust browser built on WebView2 through a vendored, patched wry 0.53.5
and tao. It targets Windows and stores its data as JSON in `%LOCALAPPDATA%\Plume`.

- Repository: https://github.com/shxmacrypto/plume
- License: MIT, Copyright 2026 shxmacrypto

This file records what Ferrite took from Plume, what is worth adopting later, and
what to avoid. No code was copied. Plume's license requires the copyright notice
to be kept with any copied code; that does not apply to the ideas below.

## Adopted

- **Tab sleeping.** Plume discards idle background tabs (`discard_idle` in
  `main.rs`). It skips the active tab, tabs playing audio, tabs in
  picture-in-picture, tabs downloading, and whitelisted tabs. On battery it waits
  2 minutes. Ferrite implements this as `tabs::sleep_idle_tabs`, with a setting
  of 0, 15, 30, 60 or 240 minutes (default 60).

## Worth adopting later

- **Host blocklist ad blocking.** Plume uses a HashSet of about 3,600 domains
  from Peter Lowe's list. It walks parent domains of each request and intercepts
  `WebResourceRequested`. Ferrite plans `adblock-rust` with full ABP syntax
  instead, in `OnBeforeResourceLoad`. Check the terms of Peter Lowe's list before
  reusing it.
- **Password vault design.** Argon2id for key derivation, AES-256-GCM for
  encryption, atomic writes, and an automatic lock after 15 minutes. It is pure
  Rust and portable. Ferrite prefers the OS secret store (Secret Service through
  `keyring`); this design is a reasonable fallback. Do not write homemade
  cryptography without review.
- **Password audit with HIBP.** Use k-anonymity: send only the first five
  characters of the SHA-1 hash of the password.
- **Browser import.** Detect Chrome and Firefox profiles. Copy `History` or
  `places.sqlite` (with its WAL file) to a temporary location before reading it.
  Read passwords only through a CSV export.
- **Clear on exit with a keep-list.** Clear selected data when the browser quits,
  except for sites on the keep-list.
- **Containers as profiles.** Map each container to a separate profile. In CEF,
  that means one RequestContext per container.
- **Update check.** Follow the GitHub `releases/latest` redirect to find a new
  version. Plume's update integrity check uses a digest from the same release,
  which only detects corruption. Use a detached signature instead.

## Pitfalls to avoid

- **IPC exposed to web pages.** Plume exposes its IPC object (`window.ipc`) to all
  web pages. Page-triggerable actions include config writes and closing, creating
  and reloading tabs (`feats07.rs`, `tabs07.rs`). The `peek` feature opens a
  page-chosen URL in a devtools view. Ferrite restricts its API to `ferrite://`
  main frames.
- **Autofill host matching without the public suffix list.** Matching by simple
  host suffix can offer credentials to the wrong site. Use the public suffix list.
- **Installing extensions without a signature check.** Plume installs CRX files
  without checking a signature and slices ZIP data without bounds checks. Do not
  install extensions without verifying them.
- **`--allow-insecure-localhost` for all tabs.** It disables a security check for
  every tab, not only for localhost.
- **Editing the engine's Local State file** to enable secure DNS. It is fragile
  and can break on engine updates.
- **Exposing per-site custom JavaScript or allow-lists to every page** through a
  global object. Keep such settings in the privileged UI only.
