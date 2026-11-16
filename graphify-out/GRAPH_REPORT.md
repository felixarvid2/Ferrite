# Graph Report - ferrite  (2026-11-16)

## Corpus Check
- 134 files · ~103,261 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 41 file(s) not represented in the graph (top: .xml 28, (none) 5, .properties 2)
- WARNING: 41 of 183 files seen (22%) have no supported extractor and are missing from this graph entirely. It may look complete while representing only part of the codebase.

## Summary
- 2134 nodes · 4325 edges · 143 communities (82 shown, 61 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 147 edges (avg confidence: 0.81)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `de101653`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- .conn
- passwords.rs
- importer.rs
- core/src/blocker.rs
- mcp-e2e.py
- rpc.rs
- src/downloads.rs
- TabId
- passcli.rs
- WebClients.kt
- WinId
- handlers.rs
- State
- mcp.rs
- Command
- Call
- steps
- inject.rs
- steps
- Urls
- ToolOutput
- BrowserActivity
- Tab
- update.rs
- chrome.js
- Db
- src/commands.rs
- LibraryActivity.kt
- wire.rs
- policy.rs
- cef
- omnibox.rs
- AdBlockSmokeTest.kt
- SuggestAdapter
- handle
- smoke.sh
- settings.rs
- Verified
- jni_api.rs
- scheme.rs
- context_menu.rs
- updates.rs
- with
- common.js
- UpdateCheckerTest.kt
- Architecture
- SettingsActivity.kt
- cdp.rs
- release
- app/src/main.rs
- serialize
- Ferrite
- AboutActivity.kt
- Settings
- Win
- task.rs
- path
- AdBlocker.kt
- Adapter
- browser/mod.rs
- CEF in Ferrite (cef-rs 156.4, app/src)
- cdp-lib.js
- android
- NewTabView.kt
- src/session.rs
- Ident
- AdBlocker
- .onCreate
- publish
- settings.js
- cef_str
- README.md
- Ferrite for Android
- Ferrite end-to-end testing
- steps
- ExternalLinks.kt
- Native
- BrowsingData.kt
- Known limitations
- rust
- env
- FerriteApp
- Engine updates
- Status
- navigation_ok
- Status
- WindowInit
- match_shortcut
- SecurityLevel
- search.rs
- H.264 / AAC (proprietary codecs)
- linux_window_properties_c
- build-cef-codecs.sh
- about.js
- downloads.js
- ListUpdater
- StartupMode
- Proton Pass
- packaging/RELEASE_NOTES.md
- run.sh
- ui
- Working in this repository
- ferrite
- bundle.sh
- package.sh
- node
- GATE_GEN
- hold
- from_userfree
- ferrite-launcher.sh
- render-icons.sh
- setup-cef.sh
- update-adblock-resources.sh
- load
- ferrite-android

## God Nodes (most connected - your core abstractions)
1. `State` - 159 edges
2. `TabId` - 73 edges
3. `WinId` - 69 edges
4. `BrowserActivity` - 65 edges
5. `handle()` - 48 edges
6. `Command` - 42 edges
7. `cef_str()` - 27 edges
8. `Call` - 27 edges
9. `Login` - 25 edges
10. `Db` - 24 edges

## Surprising Connections (you probably didn't know these)
- `AI control (MCP)` --references--> `check()`  [INFERRED]
  docs/ARCHITECTURE.md → .claude/skills/ferrite-e2e/scripts/mcp-e2e.py
- `start()` --calls--> `now_ms()`  [INFERRED]
  app/src/browser/updates.rs → core/src/lib.rs
- `Invariants (with code)` --references--> `check()`  [INFERRED]
  .claude/skills/ferrite-security/SKILL.md → .claude/skills/ferrite-e2e/scripts/mcp-e2e.py
- `Review checklist (answer yes/no; any "no" needs a reason)` --references--> `check()`  [INFERRED]
  .claude/skills/ferrite-security/SKILL.md → .claude/skills/ferrite-e2e/scripts/mcp-e2e.py
- `newest_update()` --calls--> `find()`  [INFERRED]
  core/src/update.rs → app/src/browser/commands.rs

## Import Cycles
- 2-file cycle: `app/src/browser/cdp.rs -> app/src/browser/inject.rs -> app/src/browser/cdp.rs`
- 2-file cycle: `app/src/browser/tabs.rs -> app/src/browser/window.rs -> app/src/browser/tabs.rs`

## Communities (143 total, 61 thin omitted)

### Community 0 - ".conn"
Cohesion: 0.06
Nodes (38): add_and_order(), Bookmark, BookmarkKind, Folder, Url, BookmarkNode, COLS, Database (+30 more)

### Community 1 - "passwords.rs"
Cohesion: 0.06
Nodes (67): later(), apply_capture_flag(), capture(), CLI_TIMEOUT, CLIPBOARD_CLEAR_MS, CountMemo, expire_prompt(), fetch_logins() (+59 more)

### Community 2 - "importer.rs"
Cohesion: 0.06
Nodes (65): home(), run(), set(), sources(), Status, status(), STATUS, Done (+57 more)

### Community 3 - "core/src/blocker.rs"
Cohesion: 0.06
Nodes (42): bad_resources_keep_blocking(), Blocker, blocker(), blocks_listed_hosts_and_honours_exceptions(), cosmetic_css_is_guarded_and_chunked(), CSS_CHUNK, http_host(), ip_and_localhost_hosts_work_with_ports() (+34 more)

### Community 4 - "mcp-e2e.py"
Cohesion: 0.06
Nodes (23): State and re-entrancy, ai_win(), check(), Client, main(), poll(), Quiet, ref_of() (+15 more)

### Community 5 - "rpc.rs"
Cohesion: 0.07
Nodes (40): bad_calls_never_reach_the_backend(), error(), framing_errors(), INSTRUCTIONS, INVALID_PARAMS, INVALID_REQUEST, legacy_handshake(), legacy_tools_have_no_result_type() (+32 more)

### Community 6 - "src/downloads.rs"
Cohesion: 0.06
Nodes (31): before_download(), dirs_download(), download_updated(), Downloads, list(), Live, make_handler(), open_path() (+23 more)

### Community 7 - "TabId"
Cohesion: 0.10
Nodes (40): close_find(), close_find_if_open(), exec(), find(), open_internal(), toggle_bookmark(), zoom(), zoom_percent() (+32 more)

### Community 8 - "passcli.rs"
Cohesion: 0.09
Nodes (42): copy_of(), existing_login(), existing_login_detection(), extra_field(), filled(), find_urls_object(), has_login_for(), has_totp() (+34 more)

### Community 10 - "WinId"
Cohesion: 0.07
Nodes (28): tab_changed(), ui_loaded(), BrowserRef, Tab, Ui, relaunch(), WinId, focus_omnibox() (+20 more)

### Community 11 - "handlers.rs"
Cohesion: 0.06
Nodes (24): ADBLOCK, adblock_type(), apply_privacy_headers(), BADGE_PENDING, BLOCKED, blocked_count(), BLOCKER, ClientKind (+16 more)

### Community 12 - "State"
Cohesion: 0.09
Nodes (21): apply_exceptions(), grant(), own_all_tabs(), revoke(), sync_tab(), toast(), toggle(), toggle_site_blocking() (+13 more)

### Community 13 - "mcp.rs"
Cohesion: 0.09
Nodes (28): accept_loop(), apply_settings(), bind(), bind_refuses_foreign_files_and_live_sockets(), CDP_TIMEOUT_MS, CONNECTIONS, handle_line(), IDLE_TIMEOUT (+20 more)

### Community 14 - "Command"
Cohesion: 0.05
Nodes (37): ITEMS, Command, Back, BookmarkPage, Bookmarks, CloseTab, CloseWindow, DevTools (+29 more)

### Community 15 - "Call"
Cohesion: 0.07
Nodes (21): CALL_TIMEOUT, HELP, SocketBackend, to_wire(), Backend, Fake, Call, Back (+13 more)

### Community 16 - "steps"
Cohesion: 0.06
Nodes (33): env, CARGO_TERM_COLOR, publish, jobs, release, name, on, push (+25 more)

### Community 17 - "inject.rs"
Cohesion: 0.12
Nodes (25): CdpResult, add_done(), attach(), CAPTURE, CAPTURE_BINDING, capture_clear_all(), capture_done(), capture_ensure() (+17 more)

### Community 18 - "steps"
Cohesion: 0.06
Nodes (31): runs-on, steps, timeout-minutes, concurrency, cancel-in-progress, group, env, CARGO_TERM_COLOR (+23 more)

### Community 19 - "Urls"
Cohesion: 0.11
Nodes (8): SearchEngine, BING, DUCKDUCKGO, GOOGLE, STARTPAGE, Urls, Version, UrlsTest

### Community 21 - "ToolOutput"
Cohesion: 0.21
Nodes (27): activate(), after_action(), cdp_call(), check(), click(), describe(), element_js(), evaluate() (+19 more)

### Community 24 - "update.rs"
Cohesion: 0.13
Nodes (24): Asset, check(), due_rules(), fetch(), INTERVAL_MS, is_due(), live_github(), MAX_BODY (+16 more)

### Community 25 - "chrome.js"
Cohesion: 0.18
Nodes (23): address, api(), clearDrop(), cmd(), el(), favicon(), fetchSuggest(), findInput (+15 more)

### Community 27 - "src/commands.rs"
Cohesion: 0.08
Nodes (23): ALT, COMMAND, CONTROL, PRIMARY, SHIFT, VK_ADD, VK_ESCAPE, VK_F11 (+15 more)

### Community 28 - "LibraryActivity.kt"
Cohesion: 0.13
Nodes (4): LibraryActivity, Adapter, ViewHolder, TextWatcher

### Community 29 - "wire.rs"
Cohesion: 0.13
Nodes (18): bounded_lines(), error_line(), finish(), Line, Eof, Text, TooLong, lines() (+10 more)

### Community 30 - "policy.rs"
Cohesion: 0.15
Nodes (22): set(), check_url(), Control, None, Own, Shared, facts(), live_pages() (+14 more)

### Community 31 - "cef"
Cohesion: 0.12
Nodes (12): Prompt, decide(), dismissed(), make_handler(), MEDIA_NAMES, MEDIA_PROMPT_IDS, PERMISSION_NAMES, PromptReply (+4 more)

### Community 32 - "omnibox.rs"
Cohesion: 0.15
Nodes (18): AUTHORITY_SCHEMES, display_host_origin(), display_url(), has_known_scheme(), host_of(), implied_scheme(), KNOWN_SCHEMES, looks_like_domain() (+10 more)

### Community 35 - "handle"
Cohesion: 0.18
Nodes (12): arg_bool(), arg_i64(), arg_str(), cef_version_info(), clear_browsing_data(), handle(), runtime_version(), suggest() (+4 more)

### Community 36 - "smoke.sh"
Cohesion: 0.16
Nodes (14): ferrite-down.sh script, fail(), ferrite-up.sh script, usage(), load_state(), lib.sh script, shot.sh script, cleanup() (+6 more)

### Community 37 - "settings.rs"
Cohesion: 0.18
Nodes (12): adblock_exceptions_normalised_and_defaulted(), defaults(), invalid_patch_rejected(), load_missing_corrupt_partial(), never_save_sites_and_vault(), patch(), save_load_roundtrip(), Settings (+4 more)

### Community 38 - "Verified"
Cohesion: 0.11
Nodes (19): Ad blocking (scriptlets and YouTube rules), AI control (MCP), Bookmarks and downloads, Browsing, Import from other browsers, Internal pages, Look and window frame, Media playback (+11 more)

### Community 39 - "jni_api.rs"
Cohesion: 0.29
Nodes (6): build(), Java_se_ferrite_browser_adblock_Native_cosmetic(), Java_se_ferrite_browser_adblock_Native_create(), Java_se_ferrite_browser_adblock_Native_free(), Java_se_ferrite_browser_adblock_Native_shouldBlock(), string()

### Community 40 - "scheme.rs"
Cohesion: 0.15
Nodes (10): ui_files!, allow_internal(), ApiCall, is_internal(), navigation_allowed(), normalize(), read_post_body(), Reply (+2 more)

### Community 41 - "context_menu.rs"
Cohesion: 0.11
Nodes (14): clear_text_if(), COPY_IMAGE_URL, COPY_LINK, copy_text(), FIRST, INSPECT, make_handler(), OPEN_IMAGE_TAB (+6 more)

### Community 42 - "updates.rs"
Cohesion: 0.20
Nodes (12): available(), check_now(), Inner, INNER, start(), STARTUP_DELAY_MS, status(), tick() (+4 more)

### Community 43 - "with"
Cohesion: 0.11
Nodes (18): with, api-level, arch, components, disable-animations, distribution, emulator-options, if-no-files-found (+10 more)

### Community 44 - "common.js"
Cohesion: 0.16
Nodes (11): avatarFor(), drawHeaderField(), el(), emptyState(), faviconFor(), favImg(), hostOf(), icon() (+3 more)

### Community 45 - "UpdateCheckerTest.kt"
Cohesion: 0.20
Nodes (4): UpdateChecker, Release, UpdateCheckerTest, Known cef-rs quirks

### Community 46 - "Architecture"
Cohesion: 0.12
Nodes (17): Ad and tracker blocking, Architecture, Engine, Internal pages and IPC, Modules (`app/src`), Popups and windows, Profiles, Rust to UI (+9 more)

### Community 48 - "cdp.rs"
Cohesion: 0.28
Nodes (13): closed(), DEFAULT_TIMEOUT_MS, ensure(), fail(), fail_browser(), MAX_RESULT_BYTES, next_id(), on_result() (+5 more)

### Community 49 - "release"
Cohesion: 0.13
Nodes (15): concurrency, cancel-in-progress, group, env, CARGO_TERM_COLOR, jobs, release, name (+7 more)

### Community 50 - "app/src/main.rs"
Cohesion: 0.22
Nodes (11): shutdown(), before_shutdown(), chromium_switches(), preload(), clear_site_data_if_requested(), HELP, is_root(), LAUNCH (+3 more)

### Community 51 - "serialize"
Cohesion: 0.21
Nodes (5): CAPACITY, ClosedTab, ClosedTabs, lifo_and_bounded(), tab()

### Community 52 - "Ferrite"
Cohesion: 0.13
Nodes (15): Android, Controlling Ferrite from an AI (MCP), Development, Ferrite, Install & build, Keyboard shortcuts, License & third-party, Overview (+7 more)

### Community 54 - "Settings"
Cohesion: 0.16
Nodes (4): DocumentStart, Settings, Session, Saved

### Community 55 - "Win"
Cohesion: 0.18
Nodes (3): FindState, Tab, Win

### Community 56 - "task.rs"
Cohesion: 0.24
Nodes (7): Job, make_task(), on_ui_thread(), post_local(), post_local_delayed(), post_ui_delayed(), SendBox<T>

### Community 57 - "path"
Cohesion: 0.19
Nodes (4): now_ms(), ensure_dirs_creates(), layout(), ProfilePaths

### Community 59 - "Adapter"
Cohesion: 0.26
Nodes (3): TabSwitcher, Adapter, Holder

### Community 60 - "browser/mod.rs"
Cohesion: 0.24
Nodes (7): set_exceptions(), NEXT_ID, schedule_housekeeping(), start(), state_dnt(), with_state(), init()

### Community 61 - "CEF in Ferrite (cef-rs 156.4, app/src)"
Cohesion: 0.15
Nodes (12): Build and check, CEF in Ferrite (cef-rs 156.4, app/src), Checklist: adding a handler/callback, Default return values are not CEF's defaults, DevTools protocol and other threads, Finding exact signatures, Known risks (panics in callback paths; not fixed), Panic safety (investigated) (+4 more)

### Community 62 - "cdp-lib.js"
Cohesion: 0.27
Nodes (9): connect(), fs, out(), pagesOf(), port(), req(), state(), [sub, js] (+1 more)

### Community 63 - "android"
Cohesion: 0.17
Nodes (8): TextWatcher, name, outputs, runs-on, timeout-minutes, android, generated_key, version

### Community 65 - "src/session.rs"
Cohesion: 0.27
Nodes (6): defaults_and_empty(), roundtrip(), sample(), Session, TabSnapshot, WindowSnapshot

### Community 66 - "Ident"
Cohesion: 0.24
Nodes (7): Ident, Num, Text, is_newer(), parse_version(), semver_ordering(), Version

### Community 69 - "publish"
Cohesion: 0.18
Nodes (11): publish, on, pull_request, push, workflow_dispatch, default, description, type (+3 more)

### Community 70 - "settings.js"
Cohesion: 0.35
Nodes (10): fmtNum(), load(), loadImport(), loadMcp(), loadPass(), loadPerms(), loadVaults(), pollImport() (+2 more)

### Community 71 - "cef_str"
Cohesion: 0.27
Nodes (8): menu_state(), show(), show_blocking_menu(), show_tab_menu(), TAB_ITEMS, UPDATE_ITEM, register(), cef_str()

### Community 72 - "README.md"
Cohesion: 0.25
Nodes (4): Adopted, Notes: study of Plume, Pitfalls to avoid, Worth adopting later

### Community 73 - "Ferrite for Android"
Cohesion: 0.22
Nodes (9): Ad blocking, Build, CI and releases, Features, Ferrite for Android, Licences, Limits (differences from desktop Ferrite), Security choices (+1 more)

### Community 74 - "Ferrite end-to-end testing"
Cohesion: 0.22
Nodes (8): Ferrite end-to-end testing, Flake handling, Input: CDP/api vs xdotool, Pitfalls, Quick start, Recon, then act, then assert, Screenshots, UI API cheat-sheet (`ui.js "api('name', {args})"`, defined in `app/src/browser/api.rs`)

### Community 75 - "steps"
Cohesion: 0.31
Nodes (9): steps, steps, steps, id, if, name, run, uses (+1 more)

### Community 79 - "Known limitations"
Cohesion: 0.25
Nodes (8): Ad blocking, AI control (MCP), Behaviour, Known limitations, Media and DRM, Not implemented, Platforms, Proton Pass

### Community 80 - "rust"
Cohesion: 0.25
Nodes (8): defaults, run, rust, working-directory, defaults, name, runs-on, timeout-minutes

### Community 81 - "env"
Cohesion: 0.25
Nodes (8): GENERATED_KEY, GH_TOKEN, KS_ALIAS, KS_B64, KS_KEY_PASS, KS_PASS, VERSION, env

### Community 83 - "Engine updates"
Cohesion: 0.29
Nodes (6): Engine updates, Known cef-rs quirk, Procedure, Smoke checklist, Version check and mismatches, When to update

### Community 84 - "Status"
Cohesion: 0.29
Nodes (7): Ad and tracker blocking, Android port, How to resume work, Not verified, Project skills for Claude Code, Roadmap, Status

### Community 85 - "navigation_ok"
Cohesion: 0.53
Nodes (6): ai_owned_browser_cannot_leave_the_web(), browser_gone(), controlled_browser(), handed_over_browser_may_browse_the_web(), navigation_ok(), uncontrolled_browsers_are_not_restricted_here()

### Community 86 - "Status"
Cohesion: 0.33
Nodes (6): Status, Disabled, Error, Loading, NotInstalled, Ready

### Community 88 - "match_shortcut"
Cohesion: 0.47
Nodes (6): SelectTab, k(), match_shortcut(), plain_typing_is_not_a_shortcut(), required_shortcuts(), tab_numbers_and_misc()

### Community 89 - "SecurityLevel"
Cohesion: 0.33
Nodes (6): SecurityLevel, Insecure, Internal, Local, None, Secure

### Community 90 - "search.rs"
Cohesion: 0.67
Nodes (4): builtin_engines(), engine_template(), SearchEngine, templates()

### Community 91 - "H.264 / AAC (proprietary codecs)"
Cohesion: 0.33
Nodes (5): H.264 / AAC (proprietary codecs), Licensing, Option A: build CEF yourself (recommended), Option B: a third-party prebuilt (not recommended for daily use), Widevine (DRM)

### Community 93 - "build-cef-codecs.sh"
Cohesion: 0.50
Nodes (4): CEF_ARCHIVE_FORMAT, GN_DEFINES, install_dist(), build-cef-codecs.sh script

### Community 94 - "about.js"
Cohesion: 0.60
Nodes (3): checkNow(), link(), renderUpdate()

### Community 95 - "downloads.js"
Cohesion: 0.60
Nodes (4): chip(), EXT, kindOf(), load()

### Community 98 - "StartupMode"
Cohesion: 0.50
Nodes (4): StartupMode, Homepage, NewTab, RestoreSession

### Community 99 - "Proton Pass"
Cohesion: 0.50
Nodes (4): 2FA codes, Proton Pass, Saving logins ("Save password to Proton Pass?"), Updating a password ("Update password for <username> on <site>?")

### Community 100 - "packaging/RELEASE_NOTES.md"
Cohesion: 0.50
Nodes (3): Downloads, Highlights, Known limitations

### Community 101 - "run.sh"
Cohesion: 0.50
Nodes (3): CEF_PATH, FERRITE_UI_DIR, run.sh script

### Community 104 - "ferrite"
Cohesion: 0.67
Nodes (3): ferrite, ferrite-core, ferrite-mcp

## Knowledge Gaps
- **112 isolated node(s):** `FERRITE_E2E_STATE`, `DISPLAY`, `GOOGLE`, `DUCKDUCKGO`, `BING` (+107 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 704 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **61 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `BrowserActivity` connect `BrowserActivity` to `NewTabView.kt`, `.onCreate`, `WebClients.kt`, `SettingsActivity.kt`, `BrowserActivity.kt`, `Settings`, `Tab`, `Db`, `Adapter`?**
  _High betweenness centrality (0.123) - this node is a cross-community bridge._
- **Are the 6 inferred relationships involving `State` (e.g. with `show()` and `show_blocking_menu()`) actually correct?**
  _`State` has 6 INFERRED edges - model-reasoned connections that need verification._
- **What connects `FERRITE_E2E_STATE`, `DISPLAY`, `GOOGLE` to the rest of the system?**
  _112 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `.conn` be split into smaller, more focused modules?**
  _Cohesion score 0.055600106923282544 - nodes in this community are weakly interconnected._
- **Why does `State` connect `State` to `.conn`, `passwords.rs`, `handle`, `src/downloads.rs`, `TabId`, `cef_str`, `WinId`, `handlers.rs`, `updates.rs`, `mcp.rs`, `app/src/main.rs`, `ToolOutput`, `Win`, `path`, `browser/mod.rs`, `cef`?**
  _High betweenness centrality (0.114) - this node is a cross-community bridge._
- **Are the 5 inferred relationships involving `TabId` (e.g. with `tab_changed()` and `with_tab()`) actually correct?**
  _`TabId` has 5 INFERRED edges - model-reasoned connections that need verification._
- **Should `passwords.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.0625694187338023 - nodes in this community are weakly interconnected._