# Ferrite for Android

An Android browser in the Ferrite project. One public APK (minSdk 26, targetSdk 35,
package `se.ferrite.browser`), built on **Android System WebView** (Chromium). H.264 and other
codecs come from the device, so there are no codec variants.

## Features

- Tabs (switcher with titles and favicons, restored on restart), address bar with search
  (Google default; DuckDuckGo, Bing, Startpage), suggestions from bookmarks and history,
  back/forward/reload/stop, progress bar, pull to refresh, share, find in page, desktop-site
  toggle, downloads (DownloadManager; storage permission asked on Android 9 and older), file upload,
  fullscreen video, per-site location/camera/microphone prompts that are remembered.
- Bookmarks and history (SQLite) with search, a native new-tab page with top sites, dark theme
  following the system (WebView algorithmic darkening).
- Ad and tracker blocking like the desktop browser (see below).
- Update check: once a day (setting, default on) the app asks the GitHub releases API for the
  newest `ferrite-android-v*` release and shows "Update available" in the menu. It links to the APK;
  nothing is installed silently.

## Security choices

- No `addJavascriptInterface` and no JS bridges at all. File and content access are off, universal/file
  URL access off, mixed content `NEVER_ALLOW`, Safe Browsing on.
- TLS errors are never bypassed: `onReceivedSslError` cancels and shows an error page.
- Top-level navigation only to http, https, `about:blank` and `data:` (the latter only from the address bar).
  `intent:`, `tel:`, `mailto:`, `market:` etc. need a user gesture and a confirmation dialog; `intent:` URIs
  are sanitised (no explicit component or selector). Everything else is dropped. Pop-ups need a user gesture.

## Ad blocking

Same design as the desktop (`core/src/blocker.rs`, `app/src/browser/inject.rs`):

- `rust/` (crate `ferrite-android`, cdylib, JNI via the `jni` crate) wraps Brave's `adblock` 0.13 with the
  same features as the desktop core. It does not depend on `ferrite-core`. Entry points
  (`create`, `shouldBlock`, `cosmetic`, `free`) catch panics. The crate is excluded from the desktop
  workspace (`browser/Cargo.toml`).
- Lists (same URLs as the desktop: EasyList, EasyPrivacy, uBO filters, quick-fixes, unbreak) are downloaded in
  Kotlin with `HttpURLConnection` into app storage, refreshed when older than a day (on start/resume),
  and the engine is rebuilt on a background thread. uBO lists are trusted (permission bit 0). The scriptlet
  resources `resources/adblock/ubo-scriptlets.json` and the uBO licence are packaged as APK assets straight
  from `browser/resources/adblock/`.
- Network: `shouldInterceptRequest` asks the engine for every subresource; blocked ones get a 403 response with
  an empty body (so scripts and images fire `error`). First party is the tab's current top URL. The shield
  button shows the count and "Turn off blocking on this site" (exceptions match the host and its subdomains,
  `www.` stripped).
- Scriptlets/CSS: `WebViewCompat.addDocumentStartJavaScript` (feature `DOCUMENT_START_SCRIPT`) registers the
  host-guarded script for the target origin before each navigation (own loads, `shouldOverrideUrlLoading`,
  page start); the previous handler is removed. The emulator test checks that it runs before the page's inline scripts.

## Limits (differences from desktop Ferrite)

- WebView, not CEF: the Chromium version is whatever the device's WebView provider has; no extensions,
  no Chromium UI features (no password manager, no sync), no Proton Pass.
- `WebResourceRequest` does not give the resource type; it is guessed from the file extension and `Accept`
  header (unknown = `other`), so rules that need `$xhr`/`$script` on extension-less URLs may not match.
- No `$removeparam`, redirect rules, procedural cosmetic filters or generic class/id hiding (as on the desktop).
- Only the main frame's host is covered by document-start scripts for scriptlets (same-origin frames included).
- Building the engine from the full lists takes some seconds and tens of MB of RAM on a phone; blocking starts
  once it is built (shield menu says "Filter lists are still loading").
- Desktop-site mode changes the user agent; sites with a mobile viewport meta tag may still lay out as mobile.
- A recreation of the activity (system theme change) reloads the tabs from their saved URLs.

## Build

Requirements: JDK 17, Android SDK (platform 35, build-tools 35), Rust with the Android targets and
[cargo-ndk](https://github.com/bbqsrc/cargo-ndk) plus an NDK for the native library.

```sh
cd browser/android/rust
cargo test                                  # host tests
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 --platform 26 -o ../app/src/main/jniLibs build --release
cd .. && ./gradlew testDebugUnitTest assembleRelease
```

Without the `.so` files the app still builds and runs, with ad blocking reported as unavailable.
`cargo check --target aarch64-linux-android` in `rust/` type-checks the library without an NDK.
Emulator test: `./gradlew connectedDebugAndroidTest` (needs a device/emulator).

## CI and releases

`.github/workflows/ferrite-android.yml` runs on pushes and pull requests touching `browser/android/**`,
`browser/resources/adblock/**` or the workflow: Rust fmt/clippy/tests, the NDK build, Kotlin unit tests, the
release APK, and an emulator smoke test (API 34, x86_64). **It publishes only** when started manually
(Actions > ferrite-android > Run workflow) with `publish` ticked: it creates the GitHub release
`ferrite-android-v<ferriteVersion>` (version in `gradle.properties`) with `Ferrite-<version>.apk`, `SHA256SUMS`
and the notes from `RELEASE_NOTES.md`. Bump `ferriteVersion` and edit `RELEASE_NOTES.md` first; the version
code is derived from it.

## Signing

Android only installs an update if it is signed with the same key as the installed app. Add your key once:

```sh
keytool -genkeypair -keystore ferrite-release.jks -alias ferrite -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 ferrite-release.jks      # copy the output
```

In GitHub: repository Settings > Secrets and variables > Actions > New repository secret, create
`ANDROID_KEYSTORE_BASE64` (the base64 text), `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` (`ferrite`) and
`ANDROID_KEY_PASSWORD`. Keep the `.jks` file and passwords safe; losing them means users must uninstall to update.
Without the secrets the workflow signs with a key generated in the job and prints a warning in the job summary
(and in the release notes if published): such APKs cannot update each other.

## Licences

Ferrite: MIT. adblock-rust: MPL-2.0. uBlock Origin scriptlets: GPL-3.0 (shipped as a data asset with
`LICENSE-uBlock-Origin.txt`, shown under Settings > About). EasyList/EasyPrivacy are downloaded at runtime.
