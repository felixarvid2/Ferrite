---
name: ferrite-cef
description: Use when writing or reviewing code that touches CEF in Ferrite (handlers, Views/BrowserViews, posted tasks, the ferrite:// scheme handler, or adding any new CEF callback). Covers cef-rs wrap_* macros, UI vs IO thread rules, the State/later() re-entrancy rule, panic safety, popups and known cef-rs quirks.
---

# CEF in Ferrite (cef-rs 156.4, app/src)

Paths below are relative to `app/src` (from the repository root). Design background: `docs/ARCHITECTURE.md`.

## wrap_* macros
- `wrap_xxx! { struct Name { field: T } impl Xxx { fn method(&self, ...) -> R {..} } }` generates the struct,
  `Name::new(fields) -> Xxx` (the CEF wrapper type, not `Name`), and `Clone` that calls `add_ref` and `.clone()`s every
  field. So **every field must be `Clone`** (use `Rc<Cell<_>>`, `ClientKind: Copy`, CEF handles). `&self` only: use
  `Cell`/`RefCell`/`Arc<Mutex>` for mutation (`engine/task.rs:161`, `browser/scheme.rs:190`).
- Arguments are copied from the C trampoline signature: pointers become `Option<&mut T>` / `Option<&T>`,
  `*mut` out-params stay `Option<&mut c_int>`, bool-like returns/args are `::std::os::raw::c_int` (1/0), strings are
  `Option<&CefString>` (`engine::opt_str`), userfree strings go through `engine::from_userfree`. Always destructure with
  let-else; a null arg must not panic (`handlers.rs:406`, `scheme.rs:98`).
- Out-params are written through the option (`*handle_request = 1`, `scheme.rs:203`). To keep a callback past the call,
  `callback.cloned()` (add_ref) then move it into a closure (`downloads.rs:274`).
- Delegate chains need every super-trait block, even empty: `impl ViewDelegate {}` / `impl PanelDelegate {}`
  (`window.rs:295,304`, `tabs.rs:549`). Runtime style overrides are `browser_runtime_style` / `window_runtime_style`.
- Enums are newtypes with consts: `WindowOpenDisposition::NEW_POPUP`, `ResourceType::XHR`. They are not `match`-able
  patterns; compare with `==` or guard arms (`handlers.rs:40-59`, `tabs.rs:586`). Raw sys enums/consts: `cef::sys::...`
  (`engine/app.rs:39`).

## Finding exact signatures
`rg -n "pub trait Impl<Name>\b" ~/.cargo/registry/src/*/cef-156.4.0*/src/bindings/x86_64_unknown_linux_gnu.rs`, then read
the trait: it is the method list you may override (defaults exist for the rest). Scratchpad helper idea: a shell function
`cefsig() { rg -n -A60 "pub trait Impl$1\b" <that file>; }`. The `wrap_<snake_name>!` macro is defined in the same file.

## Panic safety (investigated)
- Finding: cef-rs 156.4.0 has **no `catch_unwind`** anywhere (`rg catch_unwind` over the crate: zero hits) and the
  generated trampolines are plain `extern "C" fn` (e.g. `bindings/x86_64_unknown_linux_gnu.rs:3668`), not `C-unwind`.
  Neither Cargo.toml sets `panic = ...`. With Rust 1.97, a panic reaching an `extern "C"` boundary prints the message and
  then **aborts the whole browser process** (all windows, unsaved session). Task closures (`post_ui`, `later`) run inside
  `FnTask::execute`, also a trampoline, so the same applies.
- Rule: **never `unwrap()`, `expect()`, `[]` indexing on maps/slices, unchecked arithmetic or `unreachable!` in any
  callback or closure posted with `later`/`post_*`.** Use let-else / `get` / `?` into a logged early return. Return a safe
  default from callbacks (`0`, `None`).
- Also avoid in callbacks: `RefCell::borrow_mut` on anything CEF can re-enter, `debug_assert!`s that can fire
  (`task.rs:195`), long blocking work (IO thread stalls network, UI thread stalls the window).

## Threads
- Callbacks run on the UI thread unless listed: **IO thread** = scheme handler factory `create`, `ResourceHandler`
  (`open/read/response_headers`), `RequestHandler::resource_request_handler` and the returned `ResourceRequestHandler`
  (`handlers.rs:459,610`). Everything else (life span, load, display, download, permission, context menu, Views
  delegates) is UI. Check the header comment of the CEF class if unsure.
- IO-thread code may only touch `Send + Sync` statics (`handlers.rs:12-16` atomics/RwLock) and CEF objects it was given.
  It must not call `with_state`/touch thread-locals (they are UI-thread `thread_local!`s: `browser/mod.rs:159`,
  `scheme.rs:18`). To use state, hop: `engine::post_ui` (needs `Send`) as in `scheme.rs:213`.
- `SendBox<T>` (`engine/task.rs:144`) asserts `Send` for a value. Legitimate only for CEF ref-counted handles (thread
  safe) or closures that will run solely on the UI thread (`post_local`, `task.rs:194`). Never for `Rc`/`RefCell` data
  that is used from two threads. Always move it in and `into_inner()` on the target thread.

## State and re-entrancy
- All browser state is the UI-thread `thread_local STATE` (`mod.rs:159`). `with_state` (`mod.rs:166`) uses `try_borrow_mut`:
  if state is already borrowed up the stack it logs and returns `None` instead of panicking.
- Many CEF calls made while holding `&mut State` call back synchronously (`browser_view_create`, `close_browser`,
  `Window::close`, `add_child_view`, `execute_java_script`). A callback that then did `with_state` would be dropped (or,
  worse, if code used `borrow_mut`, panic -> abort). **Rule: callbacks extract plain data (ids, owned Strings, cloned CEF
  handles) and run `later(move |s| ...)` (`mod.rs:177`)**, which posts to the UI queue so it runs on a clean stack. Inline
  work is only for answers CEF needs synchronously (`on_before_browse`, `do_close`'s return value).
- Closures given to `later` must be `'static`; do not hold `&mut State` or borrowed args across it. Re-look up by id
  inside (`s.tab_of_browser(id)`), because the tab may be gone by then.
- Sole deliberate exception: `can_close` calls `with_state` directly and accepts `None` (`window.rs:334`).
- Helper pattern for tab callbacks: `with_tab` (`handlers.rs:79`).

## Views specifics
- One Chrome-style BrowserView per window is the CEF limit, so every BrowserView/Window uses `RuntimeStyle::ALLOY`
  (`tabs.rs:595`, `window.rs:361,403`). Do not add a Chrome-style view.
- Closing: `do_close` must return 1 and detach the BrowserView from its parent (`handlers.rs:278`, `tabs.rs:296`);
  that completes the close. Returning 0 would close the top-level Window. `on_before_close` finishes: drop the
  browser id, tab, count (`handlers.rs:290`). `close_browser(0)` runs unload handlers, `(1)` forces (`tabs.rs:286,520`).
- Popups: `on_before_popup` (`handlers.rs:246`) stashes disposition/size in thread-locals and returns 0;
  `delegate_for_popup_browser_view` (`tabs.rs:558`) allocates the new tab id and returns its delegate;
  `on_popup_browser_view_created` (`tabs.rs:570`) consumes the thread-locals and `later()`s adoption. They run in that
  order on one UI stack, so the thread-local hand-off is safe; always `take()` them. DevTools popups return 0 (CEF window).
- Browser creation is async: the `Browser` arrives via `on_browser_created` -> `later(browser_created)` (`tabs.rs:552`).

## Known cef-rs quirks
- `CefStringList::clone` drops borrowed lists. Read lent lists with `engine::read_string_list` (`engine/mod.rs:111`,
  used at `app.rs:75`, `handlers.rs:180`).
- `fetch()` bodies reach scheme handlers as unreadable data pipes: send API args in the query `?a=<json>`
  (`scheme.rs:118`; `read_post_body` is only a fallback).
- Scheme must be registered in `on_register_custom_schemes` (all processes) and the factory per RequestContext
  (`scheme.rs:65`, called for private contexts at `mod.rs:431`). New per-context setup must be mirrored for both.
- Chromium normalises `ferrite://history` to `ferrite://history/` (`scheme.rs:35`).

## Known risks (panics in callback paths; not fixed)
Only `[]`/`expect` hits in `app/src` (no `.unwrap()` exists). Each would abort the browser if its assumption breaks:
- `browser/tabs.rs:244` `&s.tabs[&tab]` in `activate` (guarded only by `w.tabs.contains`); `:351,:352,:379`
  `s.tabs[&tab]`, `s.windows[&win]` in `move_tab`/`set_pinned` (reachable from API with arbitrary ids); `:65`
  `s.windows[&win]` in `insert_index`; `:537` `.expect("tab")`.
- `browser/permissions.rs:104` and `browser/context_menu.rs:115,138`: `s.tabs[&tab]` right after `tab_of_browser`
  (browser map and tab map can diverge during close).
- `browser/window.rs:88,91` `expect` on `browser_view_create`/`panel_create` in `open_window` (called from posted closures).
- `browser/mod.rs:212,215` `expect` on in-memory DB in `start()` (inside `on_context_initialized`).
- `engine/task.rs:195,206` `debug_assert!(on_ui_thread())` (debug builds only).
- Stale `PENDING_POPUP_DISPOSITION/SIZE` (`tabs.rs:30`): set in `on_before_popup`, only cleared when a popup view is
  created; a cancelled popup leaks them into the next one.

## Checklist: adding a handler/callback
1. Find the trait (`pub trait Impl<Name>`), copy the exact signature, note which thread it runs on.
2. Put `wrap_<name>!` in `browser/handlers.rs` (or the feature module); fields `Clone`; add the getter to the client
   (`handlers.rs:101`) for `ClientKind::Content` and/or `Ui`. Decide behaviour per `ClientKind` (UI strip is privileged
   and must deny what web content may do, e.g. `permissions.rs:174`).
3. Destructure all `Option` args with let-else; no unwrap/index; default return value is the "do nothing" one.
4. Extract owned data, then `later(...)`; look entities up by id inside; no CEF re-entrant call while borrowing State.
5. IO-thread callback? Only atomics/`Send` data; hop with `post_ui` + `SendBox` for CEF handles.
6. Hold callbacks via `.cloned()`; make sure every path eventually calls `cont()/cancel()` (leaked callbacks hang requests).
7. Private windows: use `s.db_for(win)`/`is_private`; never write to `s.db` for private data.
8. Add `UI_ONLY`/guards if it adds an API command (see ferrite-security). Mark dirty (`mark_dirty`) when UI-visible.

## Build and check
```
cd <repo root>
export CEF_PATH=$HOME/.local/share/cef     # scripts/setup-cef.sh installs it
cargo build                                 # add --release for dist
cargo clippy --all-targets
cargo test                                  # core unit tests + scheme guard test (app test binary needs libcef via rpath)
```
UI-only edits (`ui/`): set `FERRITE_UI_DIR=ui` to serve from disk; embedded copy is `browser/assets.rs`.
Verify behaviour changes in a real window (`scripts/run.sh`); unit tests cannot exercise CEF callbacks.

## Default return values are not CEF's defaults

cef-rs trait methods default to `Default::default()` (0/false/None), not to the
C++ CEF defaults. Example: `WindowDelegate::can_resize/can_maximize/can_minimize`
return 0 unless overridden, which made windows non-resizable
(`browser/window.rs`, MainWindowDelegate). When adding a delegate, check every
method whose C++ default is `true` and override it.

## Strings written into out-parameter structs are lost

`impl From<CefStringUtf16> for _cef_string_utf16_t` only passes *borrowed*
strings through; an owned `CefString` becomes an empty C string. So setting
string fields of an out struct in a callback (e.g.
`WindowDelegate::linux_window_properties` → `wm_class_class`) silently has no
effect. Use another route (Ferrite sets WM_CLASS directly with Xlib in
`browser::window::set_wm_class`; Chromium's `--class` switch does not reach
Views windows).
