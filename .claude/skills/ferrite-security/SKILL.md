---
name: ferrite-security
description: Use when reviewing a Ferrite change for security, or touching the ferrite:// scheme, the JSON API, navigation, popups, downloads, permissions, private mode, the clipboard, ad blocking or dependencies. Encodes the threat model, invariants with code references, a yes/no review checklist, tooling and abuse tests.
---

# Ferrite security review

Paths relative to `app/src` (from the repository root) unless noted. Companion skill: `ferrite-cef` (threads, panics).

## Threat model
- Untrusted: all web content (any http/https/file/data page, its frames, popups, redirects, downloads, permission
  requests, window titles/URLs it controls, console output).
- Privileged: `ferrite://` pages (`chrome.html` strip, settings, history, bookmarks, downloads, about, newtab) and the JSON
  API they call (`browser/api.rs`). It can read/clear history, change settings, open files (`download_open`, `api.rs:364`),
  drive tabs. Goal: web content can never reach it, directly or by confusing the navigation guard.
- Semi-trusted: an **AI client** driving the browser through MCP (`browser/mcp.rs`, `mcp/`). It is not the
  user and not web content: it may be prompt-injected by the pages it reads, so it gets only http(s)
  pages and tabs, never `ferrite://` pages, the JSON API, settings, downloads, file opening or Proton
  Pass. Any local process of the same user can also speak to its socket, so the browser (not the
  `ferrite-mcp` bridge) enforces everything. Other users: socket is 0600 in a 0700 directory and
  `SO_PEERCRED` is checked.
- Local attacker/other users are out of scope except: private windows must leave no traces, files written by downloads
  must not escape the download dir, Ferrite must not run unsandboxed silently.

## Invariants (with code)
1. **API gate** (`browser/scheme.rs:111-117`): served only if request is a main frame (`:99`) whose committed URL starts
   with `ferrite:` (`:109`), resource type `XHR` and method `POST`. Else `None` (blocked). Do not weaken any of the three.
2. **UI-only commands** (`api.rs:104-125`): `set_height, menu, tab_menu, activate_tab, move_tab, pin_tab, permission`.
   Anything that mutates windows/tabs structurally or grants permissions must be added here unless internal content pages
   legitimately need it. Caller identity comes from the browser id, never from args (`api.rs:115-122`).
3. **Navigation guard** (`scheme.rs:52-63`, called from `handlers.rs:418`): a non-internal target is always fine; an
   internal target needs main frame AND one of: browser-initiated (`allow_internal`, single use, removed on match,
   `scheme.rs:29,61`), current URL internal, or fresh tab (`""`/`about:blank`). Subframes never. Every code path that
   loads a user/Rust-chosen internal URL must call `allow_internal` (`tabs.rs:125,459`). UI strip may only show its own
   page (`handlers.rs:410-417`).
4. **No internal popups/links** (`handlers.rs:263` `on_before_popup`, `:435` `on_open_urlfrom_tab`): `is_internal` -> blocked.
5. **No iframes** of internal pages (`scheme.rs:99`) plus CSP `frame-ancestors 'none'` (`:248`).
6. **CSP on internal HTML** (`scheme.rs:244-251`): `default-src 'self' ferrite://app`, no inline script, `nosniff`,
   `no-store`. No `unsafe-inline` for scripts; `style-src` allows inline styles and `img-src *` (needed for favicons).
   New pages must work without inline `<script>`/`onclick=`. Never build HTML/JS from untrusted strings:
   `State::ui_call` splices JSON into JS (`mod.rs:394`), so only pass `serde_json::Value`, and method names are ours.
   Untrusted titles/URLs must reach the DOM via `textContent`, not `innerHTML`.
7. **Static-asset exception** (`scheme.rs:143-148`): non-navigation static files are also served when the frame URL is
   still empty (CEF reports empty frame URL/referrer during first load). Acceptable because the files are embedded,
   public UI code with no data, requests need ferrite scheme, and the API path above is strict. Anything data-bearing must
   go through the API, never through the static path.
8. **Certificates**: `on_certificate_error` returns 0 -> Chromium error page, never `callback.cont()` (`handlers.rs:475`).
9. **Sandbox on** unless `--no-sandbox` is passed; refuse to start as root without it (`main.rs:128-137`).
10. **Private windows**: in-memory `RequestContext` (`mod.rs:418-437`), `private_db` in memory (`mod.rs:215`, reset at
    `window.rs:206-211`), `db_for(win)` selects the DB (`mod.rs:317`), history not recorded (`handlers.rs:353`), session
    skips private windows (`session.rs:13`), popups inherit privacy (`tabs.rs:177`, `handlers.rs:445`).
11. **Downloads**: names go through `unique_path` -> `sanitize_file_name` (`downloads.rs:88`, core `downloads.rs:27,80`):
    strips path separators/control chars, so no traversal. `is_dangerous` flags risky extensions (`downloads.rs:218`) but
    does not block; the UI must warn. `download_open` opens any existing path with `xdg-open` (`downloads.rs:238`): keep
    it reachable only from internal pages and consider restricting to paths in the downloads table.
12. **Permissions** (`permissions.rs`): decision keyed by origin (`:105`) and permission name; stored only when the user
    ticks remember (`:131`, UI-only command); private windows read/write `private_db` (`:80,137`); the UI strip client
    always denies (`:174,197`); the prompt only shows for the active tab (`api.rs:86`).
13. **Clipboard**: Ferrite writes the clipboard only on explicit context-menu commands (`context_menu.rs:127-128`) via
    arboard; web clipboard-read goes through the permission prompt ("clipboard"). Never write clipboard from page events.
14. **Ad block** runs on the IO thread with atomics/RwLock only (`handlers.rs:12-16,610-650`); never blocks main-frame
    documents (`:624`); filter lists are fetched with `ureq` + OS certificate verifier. Lists are data, not code, but
    regexes can be slow: keep matching off the UI thread.

15. **AI control (MCP)** (`browser/mcp.rs`, `browser/cdp.rs`, crate `mcp/`): off by default
    (`mcp_enabled`; `evaluate` needs `mcp_allow_evaluate` too). Unix socket only, no TCP/HTTP (pages can reach
    localhost), mode 0600, same-uid peer check, at most 8 connections, request lines <= 1 MiB.
    Every call is re-validated in the browser (`ferrite_mcp::tools::parse_call`: http(s)/`about:blank`
    only) and re-checked on the UI thread (`mcp::check` -> `policy::may_control`: MCP on, tab controlled
    (`Tab::ai`), not a private window, Own tab in the AI window / Shared tab outside it, live main-frame
    URL is a web page), and again before the answer is returned, so revoke (Stop, tab menu, MCP off,
    navigation to an internal page) takes effect on calls in flight. `on_before_browse` calls
    `mcp::navigation_ok` (AI-owned tabs cannot leave http(s); handed-over tabs that do lose control) from a
    thread-local, not `State`. Own tabs live in the AI window (`Win::ai`, in-memory `State::ai_context`,
    `is_ephemeral`: in-memory DB, no history, skipped by the session); handed-over tabs are only offered for
    normal windows (`mcp::menu_state`). Controlled tabs: downloads refused (`can_download`), permission
    prompts denied (`permissions.rs`), Proton Pass off (`passwords::tab_target`, `capture`), never put to
    sleep. Page reading/clicking runs in an isolated world, results are wrapped in
    `<untrusted_web_content id=NONCE>` markers (random per response), password values are never read.
    Navigation goes through `tabs::load_url`, never `Page.navigate`.

## Known weak spots to verify (not fixed)
- `fresh` accepts `about:blank` (`scheme.rs:60`): a web page can `w = open('about:blank'); w.location = 'ferrite://settings'`
  (opener may navigate its blank popup). Test it; if it loads, tighten (only truly empty URL and no opener).
- Context menu "Open link/image in new tab/window" passes the page-supplied URL to `new_tab`/`open_window`
  (`context_menu.rs:112-124`), and `create_view` calls `allow_internal` on whatever URL (`tabs.rs:125`), so a
  `ferrite://` link in a page can be opened (only on explicit user action, but inconsistent with `handlers.rs:435`).
  Also `javascript:`/`data:` image or link URLs are not filtered there.
- Bookmarks always use `s.db`, also from private windows (`commands.rs:100-108`, `api.rs:322-356`), and
  `clear_browsing_data` drops a marker file from private windows (`api.rs:492`). Decide whether that is intended.
- API commands take tab ids from args without checking they belong to the caller's window (`api.rs:146,194,206,212`):
  fine while only internal pages call, but a private strip can touch normal-window tabs.

- MCP: a handed-over tab is logged in as the user, so a prompt-injected AI can act as the user on that
  site (and, with `evaluate` on, read everything on the page). That is the feature; the user decides.
  Popups of a handed-over tab are not handed over. Snapshots do not include iframes.

## Review checklist (answer yes/no; any "no" needs a reason)
1. Does any new API command check caller kind where needed, and is it in `UI_ONLY` if structural?
2. Can web content, an iframe, a popup, a redirect or an `about:blank` window reach the new behaviour?
3. Does every new internal navigation use `allow_internal`, and does nothing else widen `navigation_allowed`?
4. Is untrusted text (title, URL, origin, file name) escaped/`textContent` in UI and JSON only (no HTML/JS splicing)?
5. Do new internal pages run with no inline script and under the existing CSP header?
6. Does new data written to disk/DB have a private-mode path (`db_for`, in-memory) and is it excluded from sessions?
7. Does a new RequestContext/setting get applied to both the global and the private context (`mod.rs:431-432`)?
8. Do new file paths from the network or page get sanitised and confined to a known directory?
9. Are permission/prompt decisions per origin, shown only to the owning tab, and remembered only on user choice?
10. Is certificate/TLS handling still fail-closed, and is the sandbox flag untouched?
11. Is IO-thread code limited to atomics/`Send` data, with no blocking or panics (`unwrap`/index) in callbacks?
12. Are errors returned to the page free of paths, secrets or internal state?
13. Are new dependencies necessary, pinned, licence-compatible, and clean in `cargo audit`/`cargo deny`?
14. Does new `unsafe` have a SAFETY comment stating who guarantees validity and thread?
15. Is there a test (unit for pure logic in `core`, abuse test below for guard changes)?
16. MCP: does a new tool go through `parse_call` (validated args) and `mcp::check`, return page text only inside
    `wrap_untrusted`, and avoid anything that reaches `ferrite://`, the API, downloads or Proton Pass? Does
    anything new that creates tabs/windows from an AI tab keep the AI/Own marking (or none)? Does a new
    DevTools call use `cdp::request` (never a raw id)?

## Tooling
- `cargo audit` (RustSec advisories) and `cargo deny check` at the repository root; run before bumping any crate and
  on each engine update (`docs/ENGINE_UPDATES.md`). `cargo clippy --all-targets`, `cargo test`.
- Suggested minimal `deny.toml` (not created yet):
```toml
[advisories]
yanked = "deny"
[licenses]
allow = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "MPL-2.0", "Unicode-3.0", "Zlib", "CC0-1.0"]
[bans]
multiple-versions = "warn"
wildcards = "deny"
[sources]
unknown-registry = "deny"
unknown-git = "deny"
```
  (`cef` is pinned `=156.4.0`; bumping it is a Chromium update: check Chromium security releases for the new version.)
- Methodology to adapt (write our own prompts, do not copy): Trail of Bits' skills repo, https://github.com/trailofbits/skills
  (CC BY-SA 4.0). Ideas worth borrowing in our own words: a Rust review pass that hunts unsafe blocks, FFI boundaries,
  panics across them and integer casts (`rust-review`); an API-misuse pass asking which easy-to-hold-wrong function or
  default invites a bug (`sharp-edges`, e.g. `allow_internal` being callable with any URL, `with_state` returning `None`);
  and a diff-focused pass that reads each changed line against the invariants above and asks what guarantee the old code
  gave that the new code loses (`differential-review`).

## Abuse tests (run in a real window; see skill `ferrite-e2e` for automation)
1. On an https page console: `fetch('ferrite://app/api/state',{method:'POST'})` must fail/CORS-block, no JSON.
2. Page with `<iframe src="ferrite://settings">` renders blank (non-main frame rejected).
3. `window.open('ferrite://settings')` does nothing; also `open('about:blank').location='ferrite://settings'` (see weak spot).
4. Page link `<a href="ferrite://settings">` click, middle-click, ctrl-click: no navigation / no tab.
5. `location='ferrite://history'`, meta refresh and a 302 redirect to `ferrite://settings` from http: blocked.
6. Private window: set a cookie/localStorage on a site, open the site in a normal window: not visible; close the last
   private window, reopen: gone; `history`, downloads list and permissions empty; `session.json` lacks it.
7. Download `a/../../x.sh` style names (via `Content-Disposition`): saved inside the download dir, sanitised.
8. https site with an expired/self-signed cert: error page, no "proceed" bypass.
9. Permission prompt for camera from a normal page in tab A while tab B is active: prompt shows only in A; remember
   unticked leaves no row in Settings > Permissions.
10. Start as root without `--no-sandbox`: refuses.
11. MCP: `scripts/mcp-e2e.py` (ferrite-e2e skill) covers: raw socket calls with `ferrite://`/`file:` URLs and
    oversize lines, `location='ferrite://settings'` / `window.open` / the `about:blank` popup trick from an AI
    tab, private-window tabs invisible, Stop mid-call, navigation of a handed-over tab to `ferrite://`,
    isolation of cookies, download refusal, marker forgery. Run it for any change to `mcp.rs`, `cdp.rs`,
    `on_before_browse` or tab/window creation.
