# Graph Report - ferrite  (2026-11-16)

## Corpus Check
- 136 files · ~103,611 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 42 file(s) not represented in the graph (top: .xml 28, (none) 6, .properties 2)
- WARNING: 42 of 186 files seen (23%) have no supported extractor and are missing from this graph entirely. It may look complete while representing only part of the codebase.

## Summary
- 2137 nodes · 4327 edges · 138 communities (79 shown, 59 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 147 edges (avg confidence: 0.82)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `46e036d0`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- .conn
- passwords.rs
- importer.rs
- core/src/blocker.rs
- mcp-e2e.py
- rpc.rs
- browser/downloads.rs
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
- .onCreate
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
- DownloadState
- SuggestAdapter
- handle
- smoke.sh
- settings.rs
- Verified
- src/downloads.rs
- scheme.rs
- context_menu.rs
- browser/commands.rs
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
- AdBlocker.kt
- AdBlocker
- publish
- settings.js
- README.md
- Ferrite for Android
- Ferrite end-to-end testing
- steps
- ExternalLinks.kt
- Native
- BrowsingData.kt
- Internal pages and IPC
- rust
- env
- FerriteApp
- Engine updates
- is_excepted
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
- .claude/CLAUDE.md
- packaging/RELEASE_NOTES.md
- run.sh
- Working in this repository
- ferrite
- bundle.sh
- package.sh
- node
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
7. `Call` - 27 edges
8. `cef_str()` - 27 edges
9. `Login` - 25 edges
10. `Db` - 24 edges

## Surprising Connections (you probably didn't know these)
- `AI control (MCP)` --references--> `check()`  [INFERRED]
  docs/ARCHITECTURE.md → .claude/skills/ferrite-e2e/scripts/mcp-e2e.py
- `Invariants (with code)` --references--> `check()`  [INFERRED]
  .claude/skills/ferrite-security/SKILL.md → .claude/skills/ferrite-e2e/scripts/mcp-e2e.py
- `Review checklist (answer yes/no; any "no" needs a reason)` --references--> `check()`  [INFERRED]
  .claude/skills/ferrite-security/SKILL.md → .claude/skills/ferrite-e2e/scripts/mcp-e2e.py
- `start_blocker()` --calls--> `update_lists()`  [INFERRED]
  app/src/browser/handlers.rs → core/src/blocker.rs
- `newest_update()` --calls--> `find()`  [INFERRED]
  core/src/update.rs → app/src/browser/commands.rs

## Import Cycles
- 2-file cycle: `app/src/browser/cdp.rs -> app/src/browser/inject.rs -> app/src/browser/cdp.rs`
- 2-file cycle: `app/src/browser/tabs.rs -> app/src/browser/window.rs -> app/src/browser/tabs.rs`

## Communities (138 total, 59 thin omitted)

### Community 0 - ".conn"
Cohesion: 0.07
Nodes (33): add_and_order(), Bookmark, BookmarkKind, Folder, Url, BookmarkNode, COLS, Database (+25 more)

### Community 1 - "passwords.rs"
Cohesion: 0.06
Nodes (67): capture(), CLI_TIMEOUT, CLIPBOARD_CLEAR_MS, CountMemo, expire_prompt(), fetch_logins(), fetch_password(), fetch_totp() (+59 more)

### Community 2 - "importer.rs"
Cohesion: 0.05
Nodes (70): home(), run(), set(), sources(), Status, status(), STATUS, Done (+62 more)

### Community 3 - "core/src/blocker.rs"
Cohesion: 0.05
Nodes (48): bad_resources_keep_blocking(), Blocker, blocker(), blocks_listed_hosts_and_honours_exceptions(), cosmetic_css_is_guarded_and_chunked(), CSS_CHUNK, http_host(), ip_and_localhost_hosts_work_with_ports() (+40 more)

### Community 4 - "mcp-e2e.py"
Cohesion: 0.06
Nodes (23): State and re-entrancy, ai_win(), check(), Client, main(), poll(), Quiet, ref_of() (+15 more)

### Community 5 - "rpc.rs"
Cohesion: 0.07
Nodes (40): bad_calls_never_reach_the_backend(), error(), framing_errors(), INSTRUCTIONS, INVALID_PARAMS, INVALID_REQUEST, legacy_handshake(), legacy_tools_have_no_result_type() (+32 more)

### Community 6 - "browser/downloads.rs"
Cohesion: 0.11
Nodes (9): before_download(), dirs_download(), download_updated(), Downloads, list(), Live, make_handler(), open_path() (+1 more)

### Community 7 - "TabId"
Cohesion: 0.13
Nodes (36): exec(), open_internal(), capture_waiting(), tab_menu_command(), TabId, tab_closed(), activate(), adopt_into() (+28 more)

### Community 8 - "passcli.rs"
Cohesion: 0.09
Nodes (41): copy_of(), existing_login(), existing_login_detection(), extra_field(), filled(), find_urls_object(), has_login_for(), has_totp() (+33 more)

### Community 10 - "WinId"
Cohesion: 0.11
Nodes (21): BrowserRef, Tab, Ui, WinId, APP_ID, close_window(), INITIAL_UI_HEIGHT, install_linux_window_properties() (+13 more)

### Community 11 - "handlers.rs"
Cohesion: 0.05
Nodes (37): ADBLOCK, adblock_type(), apply_exceptions(), apply_privacy_headers(), BADGE_PENDING, BLOCKED, blocked_count(), BLOCKER (+29 more)

### Community 12 - "State"
Cohesion: 0.11
Nodes (8): relaunch(), State, apply_capture_flag(), settings_changed(), sleep_idle_tabs(), apply_to_context(), frame_color(), window_background()

### Community 13 - "mcp.rs"
Cohesion: 0.08
Nodes (30): accept_loop(), apply_settings(), bind(), bind_refuses_foreign_files_and_live_sockets(), CDP_TIMEOUT_MS, CONNECTIONS, handle_line(), IDLE_TIMEOUT (+22 more)

### Community 14 - "Command"
Cohesion: 0.05
Nodes (37): ITEMS, Command, Back, BookmarkPage, Bookmarks, CloseTab, CloseWindow, DevTools (+29 more)

### Community 15 - "Call"
Cohesion: 0.08
Nodes (21): CALL_TIMEOUT, HELP, SocketBackend, to_wire(), Backend, Fake, Call, Back (+13 more)

### Community 16 - "steps"
Cohesion: 0.06
Nodes (33): env, CARGO_TERM_COLOR, publish, jobs, release, name, on, push (+25 more)

### Community 17 - "inject.rs"
Cohesion: 0.11
Nodes (26): CdpResult, add_done(), attach(), CAPTURE, CAPTURE_BINDING, capture_clear_all(), capture_done(), capture_ensure() (+18 more)

### Community 18 - "steps"
Cohesion: 0.06
Nodes (31): runs-on, steps, timeout-minutes, concurrency, cancel-in-progress, group, env, CARGO_TERM_COLOR (+23 more)

### Community 19 - "Urls"
Cohesion: 0.06
Nodes (9): AdBlockSmokeTest, SearchEngine, BING, DUCKDUCKGO, GOOGLE, STARTPAGE, Urls, Version (+1 more)

### Community 21 - "ToolOutput"
Cohesion: 0.22
Nodes (26): activate(), after_action(), cdp_call(), check(), click(), describe(), element_js(), evaluate() (+18 more)

### Community 24 - "update.rs"
Cohesion: 0.06
Nodes (44): available(), check_now(), Inner, INNER, start(), STARTUP_DELAY_MS, status(), tick() (+36 more)

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
Cohesion: 0.13
Nodes (12): Prompt, decide(), dismissed(), make_handler(), MEDIA_NAMES, MEDIA_PROMPT_IDS, PERMISSION_NAMES, PromptReply (+4 more)

### Community 32 - "omnibox.rs"
Cohesion: 0.15
Nodes (18): AUTHORITY_SCHEMES, display_host_origin(), display_url(), has_known_scheme(), host_of(), implied_scheme(), KNOWN_SCHEMES, looks_like_domain() (+10 more)

### Community 33 - "DownloadState"
Cohesion: 0.22
Nodes (10): Database, db_lifecycle(), DownloadRecord, DownloadState, Cancelled, Complete, InProgress, Interrupted (+2 more)

### Community 35 - "handle"
Cohesion: 0.18
Nodes (12): arg_bool(), arg_i64(), arg_str(), cef_version_info(), clear_browsing_data(), handle(), runtime_version(), suggest() (+4 more)

### Community 36 - "smoke.sh"
Cohesion: 0.16
Nodes (14): ferrite-down.sh script, fail(), ferrite-up.sh script, usage(), load_state(), lib.sh script, shot.sh script, cleanup() (+6 more)

### Community 37 - "settings.rs"
Cohesion: 0.14
Nodes (16): adblock_exceptions_normalised_and_defaulted(), defaults(), invalid_patch_rejected(), load_missing_corrupt_partial(), never_save_sites_and_vault(), patch(), save_load_roundtrip(), Settings (+8 more)

### Community 38 - "Verified"
Cohesion: 0.05
Nodes (38): 2FA codes, Ad and tracker blocking, Ad blocking, Ad blocking (scriptlets and YouTube rules), AI control (MCP), AI control (MCP), Android port, Behaviour (+30 more)

### Community 39 - "src/downloads.rs"
Cohesion: 0.21
Nodes (12): cap_length(), dangerous(), DANGEROUS_EXTENSIONS, from_row(), is_dangerous(), MAX_NAME_BYTES, MULTI_EXTENSIONS, sanitize() (+4 more)

### Community 40 - "scheme.rs"
Cohesion: 0.19
Nodes (9): ui_files!, allow_internal(), ApiCall, is_internal(), navigation_allowed(), normalize(), read_post_body(), Reply (+1 more)

### Community 41 - "context_menu.rs"
Cohesion: 0.11
Nodes (14): clear_text_if(), COPY_IMAGE_URL, COPY_LINK, copy_text(), FIRST, INSPECT, make_handler(), OPEN_IMAGE_TAB (+6 more)

### Community 42 - "browser/commands.rs"
Cohesion: 0.24
Nodes (7): close_find(), close_find_if_open(), find(), toggle_bookmark(), zoom(), zoom_percent(), NEW_TAB_URL

### Community 43 - "with"
Cohesion: 0.11
Nodes (18): with, api-level, arch, components, disable-animations, distribution, emulator-options, if-no-files-found (+10 more)

### Community 44 - "common.js"
Cohesion: 0.16
Nodes (11): avatarFor(), drawHeaderField(), el(), emptyState(), faviconFor(), favImg(), hostOf(), icon() (+3 more)

### Community 45 - "UpdateCheckerTest.kt"
Cohesion: 0.31
Nodes (4): UpdateChecker, Release, UpdateCheckerTest, Known cef-rs quirks

### Community 46 - "Architecture"
Cohesion: 0.14
Nodes (14): Ad and tracker blocking, Architecture, Engine, Modules (`app/src`), Popups and windows, Profiles, Scriptlet injection (`browser/inject.rs`), Security (+6 more)

### Community 48 - "cdp.rs"
Cohesion: 0.28
Nodes (13): closed(), DEFAULT_TIMEOUT_MS, ensure(), fail(), fail_browser(), MAX_RESULT_BYTES, next_id(), on_result() (+5 more)

### Community 49 - "release"
Cohesion: 0.13
Nodes (15): concurrency, cancel-in-progress, group, env, CARGO_TERM_COLOR, jobs, release, name (+7 more)

### Community 50 - "app/src/main.rs"
Cohesion: 0.25
Nodes (10): chromium_switches(), preload(), clear_site_data_if_requested(), HELP, is_root(), LAUNCH, LaunchOptions, main() (+2 more)

### Community 51 - "serialize"
Cohesion: 0.19
Nodes (5): CAPACITY, ClosedTab, ClosedTabs, lifo_and_bounded(), tab()

### Community 52 - "Ferrite"
Cohesion: 0.13
Nodes (15): Android, Controlling Ferrite from an AI (MCP), Development, Ferrite, Install & build, Keyboard shortcuts, License & third-party, Overview (+7 more)

### Community 54 - "Settings"
Cohesion: 0.12
Nodes (3): Settings, Session, Saved

### Community 55 - "Win"
Cohesion: 0.16
Nodes (3): FindState, Tab, Win

### Community 56 - "task.rs"
Cohesion: 0.21
Nodes (10): close_gate(), Job, make_task(), on_ui_thread(), post_local(), post_local_delayed(), post_ui(), post_ui_delayed() (+2 more)

### Community 59 - "Adapter"
Cohesion: 0.22
Nodes (3): TabSwitcher, Adapter, Holder

### Community 60 - "browser/mod.rs"
Cohesion: 0.11
Nodes (12): before_shutdown(), NEXT_ID, schedule_housekeeping(), start(), state_dnt(), with_state(), mark_clean_exit(), restore_or_open() (+4 more)

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

### Community 69 - "publish"
Cohesion: 0.18
Nodes (11): publish, on, pull_request, push, workflow_dispatch, default, description, type (+3 more)

### Community 70 - "settings.js"
Cohesion: 0.35
Nodes (10): fmtNum(), load(), loadImport(), loadMcp(), loadPass(), loadPerms(), loadVaults(), pollImport() (+2 more)

### Community 72 - "README.md"
Cohesion: 0.33
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

### Community 79 - "Internal pages and IPC"
Cohesion: 0.67
Nodes (3): Internal pages and IPC, Rust to UI, UI to Rust

### Community 80 - "rust"
Cohesion: 0.25
Nodes (8): defaults, run, rust, working-directory, defaults, name, runs-on, timeout-minutes

### Community 81 - "env"
Cohesion: 0.25
Nodes (8): GENERATED_KEY, GH_TOKEN, KS_ALIAS, KS_B64, KS_KEY_PASS, KS_PASS, VERSION, env

### Community 83 - "Engine updates"
Cohesion: 0.33
Nodes (6): Engine updates, Known cef-rs quirk, Procedure, Smoke checklist, Version check and mismatches, When to update

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

### Community 100 - "packaging/RELEASE_NOTES.md"
Cohesion: 0.50
Nodes (3): Downloads, Highlights, Known limitations

### Community 101 - "run.sh"
Cohesion: 0.50
Nodes (3): CEF_PATH, FERRITE_UI_DIR, run.sh script

### Community 103 - "Working in this repository"
Cohesion: 0.50
Nodes (3): graphify, Who does what, Working in this repository

### Community 104 - "ferrite"
Cohesion: 0.67
Nodes (3): ferrite, ferrite-core, ferrite-mcp

### Community 112 - "from_userfree"
Cohesion: 0.24
Nodes (8): grant(), live_url(), own_all_tabs(), revoke(), sync_tab(), toast(), toggle(), from_userfree()

## Knowledge Gaps
- **112 isolated node(s):** `Folder`, `Url`, `Allow`, `Block`, `Blocked` (+107 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 707 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **59 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `BrowserActivity` connect `Tab` to `NewTabView.kt`, `.onGeolocationRequest`, `WebClients.kt`, `SettingsActivity.kt`, `BrowserActivity.kt`, `.onCreate`, `Settings`, `Db`, `Adapter`?**
  _High betweenness centrality (0.122) - this node is a cross-community bridge._
- **Are the 6 inferred relationships involving `State` (e.g. with `show()` and `show_blocking_menu()`) actually correct?**
  _`State` has 6 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Folder`, `Url`, `Allow` to the rest of the system?**
  _112 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `.conn` be split into smaller, more focused modules?**
  _Cohesion score 0.06924882629107981 - nodes in this community are weakly interconnected._
- **Why does `State` connect `State` to `passwords.rs`, `importer.rs`, `handle`, `browser/downloads.rs`, `TabId`, `browser/commands.rs`, `handlers.rs`, `WinId`, `mcp.rs`, `from_userfree`, `app/src/main.rs`, `ToolOutput`, `Win`, `update.rs`, `browser/mod.rs`, `cef`?**
  _High betweenness centrality (0.115) - this node is a cross-community bridge._
- **Are the 5 inferred relationships involving `TabId` (e.g. with `tab_changed()` and `with_tab()`) actually correct?**
  _`TabId` has 5 INFERRED edges - model-reasoned connections that need verification._
- **Should `passwords.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06054054054054054 - nodes in this community are weakly interconnected._