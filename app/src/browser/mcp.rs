//! AI control through MCP (Model Context Protocol).
//!
//! An MCP client launches the small `ferrite-mcp` bridge, which forwards
//! tool calls over a Unix socket in the profile directory (mode 0600, only
//! the same user may connect, no TCP/HTTP listener) to the server here.
//!
//! Threads: the accept loop and one thread per connection (at most
//! [`MAX_CONNECTIONS`]) are plain `std` threads. They never touch CEF or the
//! browser state: every step that needs either is a closure posted to the UI
//! thread ([`ui`], [`ui_state`]) whose result comes back through a channel.
//! The UI thread never waits for a connection thread, and every wait has a
//! timeout. UI-side closures cannot panic out of the CEF trampoline
//! (`catch_unwind` in [`ui`]).
//!
//! Policy (the bridge is a convenience, this file is the authority):
//! - Only tabs with `Tab::ai != None` are visible to the tools: the AI's own
//!   tabs (isolated AI window with an in-memory request context, no
//!   profile cookies, no history, not in the session) and tabs the user
//!   handed over from the tab menu. Private windows never.
//! - Every tool call re-checks, on the UI thread, that MCP is enabled, that
//!   the tab is still controlled and that its *live* main-frame URL is a web
//!   page (see `ferrite_mcp::policy::may_control`). The answer is checked
//!   again before it is returned, so Stop takes effect on calls in flight.
//! - Navigation uses `tabs::load_url`, never `Page.navigate`, so content
//!   blocking, the request gate and the about:blank deferral still apply.
//!   `on_before_browse` asks [`navigation_ok`]: an AI-owned tab cannot leave
//!   http(s); a handed-over tab that does (the user typed an internal URL)
//!   loses its control instead.
//! - Page reading and element lookup run in an isolated world, so a page
//!   cannot tamper with our helper code. Element references are per
//!   snapshot and die with the document. Page content in results is marked
//!   as untrusted with a random per-response boundary.
//! - Downloads, permission prompts and Proton Pass are off for controlled
//!   tabs (see `downloads.rs`, `permissions.rs`, `passwords.rs`).
//! - `evaluate` runs page JavaScript in the main world and exists only
//!   while `mcp_allow_evaluate` is on.

use super::{cdp, tabs, window, with_state, State, Tab, TabId, WinId};
use crate::engine::{from_userfree, post_ui};
use cef::*;
use ferrite_mcp::policy::{self, Control, TabFacts};
use ferrite_mcp::rpc::ToolOutput;
use ferrite_mcp::tools::{self, Call};
use ferrite_mcp::wire::{self, Line};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// Concurrent socket connections served (the bridge uses one per call).
const MAX_CONNECTIONS: usize = 8;
/// Tabs the AI may have open at once.
const MAX_AI_TABS: usize = 20;
/// How long a connection may stay silent.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a thread waits for the UI thread.
const UI_WAIT: Duration = Duration::from_secs(20);
/// Per DevTools call.
const CDP_TIMEOUT_MS: i64 = 15_000;
/// How long navigations are awaited.
const LOAD_WAIT: Duration = Duration::from_secs(20);
/// Page text returned by `snapshot` (characters).
const MAX_SNAPSHOT_CHARS: usize = 150_000;
/// `evaluate` result size (characters).
const MAX_EVAL_CHARS: usize = 100_000;
/// Largest screenshot returned (base64 characters).
const MAX_SCREENSHOT_B64: usize = 16 * 1024 * 1024;
/// Name of the isolated world the helper scripts run in.
const WORLD: &str = "ferrite-mcp";

// ---------------------------------------------------------------- state ----

thread_local! {
    /// Control state per CEF browser id, for callbacks that must not touch
    /// the borrowed `State` (`on_before_browse`, `can_download`).
    static AI_BROWSERS: RefCell<HashMap<i32, Control>> = RefCell::new(HashMap::new());
    static SERVER: RefCell<Option<ServerHandle>> = const { RefCell::new(None) };
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

struct ServerHandle {
    stop: Arc<AtomicBool>,
    path: PathBuf,
}

/// Re-publish the control state of the tab's browser (call whenever
/// `Tab::ai` or `Tab::browser` changes).
pub fn sync_tab(s: &State, tab: TabId) {
    let Some(t) = s.tabs.get(&tab) else { return };
    let Some(b) = &t.browser else { return };
    let id = b.identifier();
    AI_BROWSERS.with(|m| {
        let mut m = m.borrow_mut();
        if t.ai == Control::None {
            m.remove(&id);
        } else {
            m.insert(id, t.ai);
        }
    });
}

/// The browser closed.
pub fn browser_gone(browser_id: i32) {
    AI_BROWSERS.with(|m| m.borrow_mut().remove(&browser_id));
}

/// Is this browser's tab under AI control? (UI thread, no `State` access.)
pub fn controlled_browser(browser_id: i32) -> bool {
    AI_BROWSERS.with(|m| m.borrow().contains_key(&browser_id))
}

/// `on_before_browse` (main frame): may this browser go to `target`?
/// Returns false to block. A handed-over tab that leaves the web loses its
/// control (the user did that, so the navigation itself goes ahead).
pub fn navigation_ok(browser_id: i32, target: &str) -> bool {
    let control = AI_BROWSERS.with(|m| m.borrow().get(&browser_id).copied());
    match control {
        None | Some(Control::None) => true,
        Some(_) if policy::page_allowed(target) => true,
        Some(Control::Own) => false,
        Some(Control::Shared) => {
            AI_BROWSERS.with(|m| m.borrow_mut().remove(&browser_id));
            super::later(move |s| {
                if let Some(tab) = s.tab_of_browser(browser_id) {
                    revoke(s, tab);
                }
            });
            true
        }
    }
}

// ------------------------------------------------------ control (UI thread) --

fn toast(s: &State, win: WinId, msg: &str) {
    s.ui_call(win, "toast", &json!(msg));
}

/// The tab's live main-frame URL (falls back to the stored one).
fn live_url(t: &Tab) -> String {
    t.main_frame()
        .map(|f| from_userfree(&f.url()))
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| t.url.clone())
}

/// Hand a user tab over to the AI ("Let AI control this tab").
pub fn grant(s: &mut State, tab: TabId) -> Result<(), &'static str> {
    if !s.settings.mcp_enabled {
        return Err("AI control is turned off in Settings");
    }
    let Some(t) = s.tabs.get(&tab) else {
        return Err("no such tab");
    };
    let win = t.win;
    if s.windows
        .get(&win)
        .is_none_or(|w| w.private || w.ai || w.closing)
    {
        return Err("this window cannot be handed over");
    }
    if t.closing || !policy::page_allowed(&live_url(t)) {
        return Err("only web pages can be handed over");
    }
    if let Some(t) = s.tabs.get_mut(&tab) {
        t.ai = Control::Shared;
    }
    sync_tab(s, tab);
    s.mark_dirty(win);
    toast(s, win, "The AI can now control this tab");
    Ok(())
}

/// Take control away from the AI (Stop button, menu, navigation away,
/// MCP switched off). Calls in flight on the tab fail.
pub fn revoke(s: &mut State, tab: TabId) {
    let Some(t) = s.tabs.get_mut(&tab) else {
        return;
    };
    if t.ai == Control::None {
        return;
    }
    t.ai = Control::None;
    let (win, browser_id) = (t.win, t.browser.as_ref().map(|b| b.identifier()));
    if let Some(id) = browser_id {
        AI_BROWSERS.with(|m| m.borrow_mut().remove(&id));
        cdp::fail_browser(id, "AI control of the tab was stopped");
    }
    s.mark_dirty(win);
    toast(s, win, "AI control stopped");
}

pub fn revoke_all(s: &mut State) {
    let tabs: Vec<TabId> = s
        .tabs
        .values()
        .filter(|t| t.ai != Control::None)
        .map(|t| t.id)
        .collect();
    for tab in tabs {
        revoke(s, tab);
    }
}

/// Every tab of an AI window belongs to the AI (a window it opened).
pub fn own_all_tabs(s: &mut State, win: WinId) {
    let ids: Vec<TabId> = s
        .windows
        .get(&win)
        .map(|w| w.tabs.clone())
        .unwrap_or_default();
    for id in ids {
        if let Some(t) = s.tabs.get_mut(&id) {
            t.ai = Control::Own;
        }
        sync_tab(s, id);
    }
    s.mark_dirty(win);
}

/// Tab menu item: hand over, or take back.
pub fn toggle(s: &mut State, tab: TabId) {
    if s.ai_control(tab) != Control::None {
        revoke(s, tab);
    } else if let Err(msg) = grant(s, tab) {
        if let Some(win) = s.tabs.get(&tab).map(|t| t.win) {
            toast(s, win, msg);
        }
    }
}

/// Should the tab menu offer "Let AI control this tab" / "Take back control"?
pub fn menu_state(s: &State, tab: TabId) -> Option<bool> {
    if !s.settings.mcp_enabled {
        return None;
    }
    let t = s.tabs.get(&tab)?;
    match t.ai {
        Control::None => {
            let ok = s
                .windows
                .get(&t.win)
                .is_some_and(|w| !w.private && !w.ai && !w.closing)
                && !t.closing
                && policy::page_allowed(&live_url(t));
            ok.then_some(false)
        }
        _ => Some(true),
    }
}

// ----------------------------------------------------------- settings ----

fn socket_path(s: &State) -> PathBuf {
    match std::env::var_os("FERRITE_MCP_SOCKET").filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => s.paths.mcp_socket.clone(),
    }
}

/// Start or stop the server to match the settings (UI thread).
pub fn apply_settings(s: &mut State) {
    if s.settings.mcp_enabled {
        let path = socket_path(s);
        match start_server(&path) {
            Ok(()) => LAST_ERROR.with(|e| *e.borrow_mut() = None),
            Err(err) => {
                tracing::error!("MCP server: {err}");
                LAST_ERROR.with(|e| *e.borrow_mut() = Some(err));
            }
        }
    } else {
        stop_server();
        LAST_ERROR.with(|e| *e.borrow_mut() = None);
        revoke_all(s);
    }
}

/// Settings page: where to point the client, and whether it is running.
pub fn status(s: &State) -> Value {
    let path = socket_path(s);
    let listening = SERVER.with(|h| h.borrow().is_some());
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("ferrite-mcp")))
        .filter(|p| p.is_file());
    let controlled = s.tabs.values().filter(|t| t.ai != Control::None).count();
    json!({
        "enabled": s.settings.mcp_enabled,
        "allow_evaluate": s.settings.mcp_allow_evaluate,
        "listening": listening,
        "socket": path,
        "error": LAST_ERROR.with(|e| e.borrow().clone()),
        "command": exe.map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "ferrite-mcp".into()),
        "profile": s.launch.profile,
        "controlled_tabs": controlled,
    })
}

// ----------------------------------------------------------- the server ----

/// The browser shuts down: remove the socket.
pub fn shutdown() {
    stop_server();
}

fn start_server(path: &Path) -> Result<(), String> {
    if SERVER.with(|h| h.borrow().as_ref().is_some_and(|h| h.path == path)) {
        return Ok(());
    }
    stop_server();
    if path.as_os_str().len() > wire::MAX_SOCKET_PATH {
        return Err(format!(
            "the socket path is longer than {} bytes; set FERRITE_MCP_SOCKET to a shorter one",
            wire::MAX_SOCKET_PATH
        ));
    }
    let listener = bind(path).map_err(|e| format!("cannot listen on {}: {e}", path.display()))?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::Builder::new()
        .name("mcp-accept".into())
        .spawn(move || accept_loop(listener, flag))
        .map_err(|e| format!("cannot start the MCP thread: {e}"))?;
    tracing::info!("MCP: listening on {}", path.display());
    SERVER.with(|h| {
        *h.borrow_mut() = Some(ServerHandle {
            stop,
            path: path.to_path_buf(),
        })
    });
    Ok(())
}

fn stop_server() {
    if let Some(h) = SERVER.with(|h| h.borrow_mut().take()) {
        h.stop.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&h.path);
        tracing::info!("MCP: stopped");
    }
}

/// Bind the socket, removing a stale file but never another instance's.
fn bind(path: &Path) -> std::io::Result<UnixListener> {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if !meta.file_type().is_socket() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "a file that is not a socket is in the way",
            ));
        }
        if UnixStream::connect(path).is_ok() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "another Ferrite is listening there",
            ));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    use std::os::unix::fs::PermissionsExt;
    // The profile directory is 0700 already; the socket itself is 0600 too.
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        let _ = std::fs::remove_file(path);
        return Err(e);
    }
    listener.set_nonblocking(true)?;
    Ok(listener)
}

static CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

fn accept_loop(listener: UnixListener, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                if CONNECTIONS.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
                    CONNECTIONS.fetch_sub(1, Ordering::AcqRel);
                    reject(stream, "too many connections");
                    continue;
                }
                let stop = stop.clone();
                let spawned =
                    std::thread::Builder::new()
                        .name("mcp-conn".into())
                        .spawn(move || {
                            let _ = catch_unwind(AssertUnwindSafe(|| serve(stream, &stop)));
                            CONNECTIONS.fetch_sub(1, Ordering::AcqRel);
                        });
                if spawned.is_err() {
                    CONNECTIONS.fetch_sub(1, Ordering::AcqRel);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                tracing::warn!("MCP accept failed: {e}");
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

fn reject(mut stream: UnixStream, why: &str) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let _ = writeln!(stream, "{}", wire::error_line(&Value::Null, why));
}

/// The connecting process must run as the same user.
fn peer_is_me(stream: &UnixStream) -> bool {
    let mut cred = libc::ucred {
        pid: 0,
        uid: u32::MAX,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: the fd is a valid connected socket owned by `stream`; `cred`
    // and `len` are valid for the size passed.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    // SAFETY: geteuid has no preconditions.
    rc == 0 && cred.uid == unsafe { libc::geteuid() }
}

fn serve(stream: UnixStream, stop: &AtomicBool) {
    if !peer_is_me(&stream) {
        reject(stream, "not allowed");
        return;
    }
    let _ = stream.set_read_timeout(Some(IDLE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_half);
    let mut out = stream;
    while !stop.load(Ordering::Relaxed) {
        let reply = match wire::read_line_bounded(&mut reader, wire::MAX_REQUEST_LINE) {
            Ok(Line::Text(line)) => handle_line(&line),
            Ok(Line::TooLong) => wire::error_line(&Value::Null, "request too large"),
            Ok(Line::Eof) | Err(_) => return,
        };
        if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
            return;
        }
    }
}

/// One request line to one reply line, whatever happens.
fn handle_line(line: &str) -> String {
    let Ok(msg) = serde_json::from_str::<Value>(line) else {
        return wire::error_line(&Value::Null, "unreadable request");
    };
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let Some(tool) = msg.get("tool").and_then(Value::as_str) else {
        return wire::error_line(&id, "missing tool");
    };
    let empty = json!({});
    let args = msg.get("args").unwrap_or(&empty);
    let out = match tools::parse_call(tool, args) {
        Err(e) => ToolOutput::error(format!("Invalid arguments: {e}")),
        Ok(call) => catch_unwind(AssertUnwindSafe(|| run(call)))
            .unwrap_or_else(|_| ToolOutput::error("internal error")),
    };
    wire::ok_line(&id, &out.content, out.is_error)
}

// ------------------------------------------------- bridging to the UI thread --

/// Run `f` on the UI thread and wait for it. Never call from the UI thread.
fn ui<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> Result<R, String> {
    let (tx, rx) = mpsc::channel();
    post_ui(move || {
        let r = catch_unwind(AssertUnwindSafe(f)).map_err(|_| "internal error".to_string());
        let _ = tx.send(r);
    });
    rx.recv_timeout(UI_WAIT)
        .map_err(|_| "Ferrite is busy; try again".to_string())?
}

fn ui_state<R: Send + 'static>(
    f: impl FnOnce(&mut State) -> R + Send + 'static,
) -> Result<R, String> {
    ui(move || with_state(f))?.ok_or_else(|| "Ferrite is busy; try again".to_string())
}

/// May the tools act on `tab` right now?
fn check(s: &State, tab: TabId) -> Result<(), String> {
    if !s.settings.mcp_enabled {
        return Err("AI control is turned off in Ferrite's Settings".into());
    }
    let t = s.tabs.get(&tab).ok_or("no such tab")?;
    let win = s.windows.get(&t.win);
    let url = live_url(t);
    policy::may_control(&TabFacts {
        control: t.ai,
        private_window: win.is_none_or(|w| w.private),
        closing: t.closing,
        url: &url,
    })?;
    // Own tabs live in the AI window, handed-over ones in the user's.
    let in_ai_window = win.is_some_and(|w| w.ai);
    if (t.ai == Control::Own) != in_ai_window {
        return Err("no such tab".into());
    }
    Ok(())
}

/// One DevTools call on a controlled tab, and its answer. The tab must still
/// be controlled when the answer arrives.
fn cdp_call(tab: TabId, method: &'static str, params: Value) -> Result<Value, String> {
    let (tx, rx) = mpsc::channel::<cdp::CdpResult>();
    ui(move || -> Result<(), String> {
        let browser = with_state(|s| {
            check(s, tab)?;
            s.tabs
                .get(&tab)
                .and_then(|t| t.browser.clone())
                .ok_or_else(|| "the tab is not loaded yet".to_string())
        })
        .ok_or_else(|| "Ferrite is busy; try again".to_string())??;
        // Outside the state borrow: DevTools calls may call back synchronously.
        let mut browser = browser;
        cdp::request(
            &mut browser,
            method,
            params,
            CDP_TIMEOUT_MS,
            Box::new(move |_, res| {
                let _ = tx.send(res);
            }),
        )
        .map(|_| ())
        .ok_or_else(|| "could not reach the tab".to_string())
    })??;
    let res = rx
        .recv_timeout(Duration::from_millis(CDP_TIMEOUT_MS as u64 + 3000))
        .map_err(|_| "the page did not answer".to_string())??;
    ui_state(move |s| check(s, tab))??;
    Ok(res)
}

/// Run `expression` in the isolated world of the tab's main frame.
fn isolated_eval(tab: TabId, expression: String) -> Result<Value, String> {
    let tree = cdp_call(tab, "Page.getFrameTree", json!({}))?;
    let frame_id = tree
        .pointer("/frameTree/frame/id")
        .and_then(Value::as_str)
        .ok_or("the page has no main frame")?
        .to_string();
    let world = cdp_call(
        tab,
        "Page.createIsolatedWorld",
        json!({ "frameId": frame_id, "worldName": WORLD }),
    )?;
    let context = world
        .get("executionContextId")
        .and_then(Value::as_i64)
        .ok_or("could not create the script context")?;
    let r = cdp_call(
        tab,
        "Runtime.evaluate",
        json!({ "expression": expression, "contextId": context, "returnByValue": true }),
    )?;
    if r.get("exceptionDetails").is_some() {
        return Err("the helper script failed on this page".into());
    }
    Ok(r.pointer("/result/value").cloned().unwrap_or(Value::Null))
}

// ------------------------------------------------------------ tab state ----

#[derive(Debug, Clone)]
struct TabInfo {
    browser: bool,
    deferred: bool,
    loading: bool,
    load_seq: u64,
    url: String,
    title: String,
}

fn tab_info(tab: TabId) -> Result<TabInfo, String> {
    ui_state(move |s| {
        check(s, tab)?;
        let t = s.tabs.get(&tab).ok_or("no such tab")?;
        Ok::<_, String>(TabInfo {
            browser: t.browser.is_some(),
            deferred: t
                .browser
                .as_ref()
                .is_some_and(|b| tabs::is_deferred(b.identifier())),
            loading: t.loading,
            load_seq: t.load_seq,
            url: live_url(t),
            title: t.title.clone(),
        })
    })?
}

/// Make sure the tab has a browser (wake a sleeping tab) and wait for it.
fn prepare(tab: TabId) -> Result<TabInfo, String> {
    let info = tab_info(tab)?;
    if info.browser {
        return Ok(info);
    }
    ui_state(move |s| {
        if check(s, tab).is_ok() {
            tabs::create_view(s, tab);
        }
    })?;
    let deadline = Instant::now() + LOAD_WAIT;
    loop {
        let info = tab_info(tab)?;
        if info.browser && !info.deferred {
            return wait_loaded(tab, 0, info.url.as_str() == "about:blank");
        }
        if Instant::now() > deadline {
            return Err("the tab did not wake up".into());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// Wait until a load that started after `since` has finished. Gives up
/// quietly when no load starts within a few seconds (same-document
/// navigation) or after [`LOAD_WAIT`] (slow page: the caller says so).
fn wait_loaded(tab: TabId, since: u64, expect_blank: bool) -> Result<TabInfo, String> {
    let start = Instant::now();
    loop {
        let info = tab_info(tab)?;
        let settled = info.browser
            && !info.deferred
            && !info.loading
            && info.load_seq > since
            && (expect_blank || info.url != "about:blank");
        if settled {
            return Ok(info);
        }
        let waited = start.elapsed();
        if waited > LOAD_WAIT
            || (info.load_seq == since && !info.loading && waited > Duration::from_secs(3))
        {
            return Ok(info);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// Bring the tab to the front of its window (rendering and input need it).
fn activate(tab: TabId) -> Result<(), String> {
    ui_state(move |s| {
        check(s, tab)?;
        let win = s.tabs.get(&tab).map(|t| t.win).ok_or("no such tab")?;
        tabs::activate(s, win, tab);
        Ok::<_, String>(())
    })??;
    std::thread::sleep(Duration::from_millis(200));
    Ok(())
}

fn page_text(nonce: &str, url: &str, body: &str) -> String {
    policy::wrap_untrusted(nonce, url, &policy::sanitize(body))
}

fn describe(nonce: &str, tab: TabId, info: &TabInfo, note: &str) -> ToolOutput {
    let failed = !policy::page_allowed(&info.url);
    let state = if failed {
        "the page did not load (error page)"
    } else if info.loading {
        "still loading"
    } else {
        "loaded"
    };
    let body = format!("title: {}\nstate: {state}", info.title);
    ToolOutput::text(format!(
        "{note}\ntab {tab}\n{}",
        page_text(nonce, &info.url, &body)
    ))
}

// ---------------------------------------------------------------- tools ----

fn run(call: Call) -> ToolOutput {
    let nonce = policy::nonce();
    match run_inner(call, &nonce) {
        Ok(out) => out,
        Err(e) => ToolOutput::error(e),
    }
}

fn run_inner(call: Call, nonce: &str) -> Result<ToolOutput, String> {
    match call {
        Call::ListTabs => list_tabs(nonce),
        Call::OpenTab { url } => open_tab(nonce, url),
        Call::Navigate { tab, url } => {
            let before = prepare(tab)?.load_seq;
            ui_state(move |s| {
                check(s, tab)?;
                tabs::load_url(s, tab, &url);
                Ok::<_, String>(())
            })??;
            let info = wait_loaded(tab, before, false)?;
            Ok(describe(nonce, tab, &info, "Navigated."))
        }
        Call::CloseTab { tab } => {
            let own = ui_state(move |s| {
                check(s, tab)?;
                let own = s.ai_control(tab) == Control::Own;
                if own {
                    tabs::close_tab(s, tab);
                } else {
                    revoke(s, tab);
                }
                Ok::<_, String>(own)
            })??;
            Ok(ToolOutput::text(if own {
                format!("Closed tab {tab}.")
            } else {
                format!("Gave control of tab {tab} back to the user; the tab stays open.")
            }))
        }
        Call::Back { tab } | Call::Forward { tab } | Call::Reload { tab } => {
            let kind = match &call {
                Call::Back { .. } => 0,
                Call::Forward { .. } => 1,
                _ => 2,
            };
            let before = prepare(tab)?.load_seq;
            ui_state(move |s| {
                check(s, tab)?;
                let t = s.tabs.get(&tab).ok_or("no such tab")?;
                let b = t.browser.clone().ok_or("the tab is not loaded yet")?;
                match kind {
                    0 if !t.can_back => return Err("there is no page to go back to".to_string()),
                    0 => b.go_back(),
                    1 if !t.can_forward => {
                        return Err("there is no page to go forward to".to_string())
                    }
                    1 => b.go_forward(),
                    _ => b.reload(),
                }
                Ok(())
            })??;
            let info = wait_loaded(tab, before, false)?;
            Ok(describe(nonce, tab, &info, "Done."))
        }
        Call::Snapshot { tab } => snapshot(nonce, tab),
        Call::Screenshot { tab } => screenshot(tab),
        Call::Click { tab, element } => click(nonce, tab, &element),
        Call::Type {
            tab,
            element,
            text,
            submit,
        } => type_text(nonce, tab, &element, &text, submit),
        Call::WaitFor { tab, text, seconds } => wait_for(nonce, tab, text, seconds),
        Call::Evaluate { tab, js } => evaluate(nonce, tab, js),
    }
}

fn list_tabs(nonce: &str) -> Result<ToolOutput, String> {
    let rows = ui_state(|s| {
        if !s.settings.mcp_enabled {
            return Err("AI control is turned off in Ferrite's Settings".to_string());
        }
        let mut rows = Vec::new();
        for w in s.windows.values().filter(|w| !w.private && !w.closing) {
            for id in &w.tabs {
                if check(s, *id).is_err() {
                    continue;
                }
                let Some(t) = s.tabs.get(id) else { continue };
                let who = if t.ai == Control::Own {
                    "opened by you"
                } else {
                    "handed over by the user"
                };
                let active = if w.active == Some(*id) {
                    ", active in its window"
                } else {
                    ""
                };
                rows.push(format!(
                    "tab {id} ({who}{active}): {} | {}",
                    live_url(t),
                    t.title
                ));
            }
        }
        Ok(rows)
    })??;
    if rows.is_empty() {
        return Ok(ToolOutput::text(
            "No tabs yet. Use open_tab to open a page; tabs the user hands over appear here too.",
        ));
    }
    Ok(ToolOutput::text(policy::wrap_untrusted(
        nonce,
        "ferrite tab list",
        &policy::sanitize(&rows.join("\n")),
    )))
}

fn open_tab(nonce: &str, url: String) -> Result<ToolOutput, String> {
    let tab = ui_state(move |s| {
        if !s.settings.mcp_enabled {
            return Err("AI control is turned off in Ferrite's Settings".to_string());
        }
        if s.tabs.values().filter(|t| t.ai == Control::Own).count() >= MAX_AI_TABS {
            return Err(format!(
                "too many AI tabs open (limit {MAX_AI_TABS}); close some first"
            ));
        }
        let win = match s
            .windows
            .values()
            .find(|w| w.ai && !w.closing)
            .map(|w| w.id)
        {
            Some(w) => w,
            None => {
                let mut init = window::WindowInit::normal(vec![]);
                init.ai = true;
                init.tabs.clear();
                window::open_window(s, init).ok_or("could not open the AI window")?
            }
        };
        let opts = tabs::NewTab {
            ai: true,
            ..Default::default()
        };
        let tab = tabs::new_tab(s, win, &url, opts).ok_or("could not open a tab")?;
        sync_tab(s, tab);
        Ok(tab)
    })??;
    let info = wait_loaded(tab, 0, false)?;
    Ok(describe(
        nonce,
        tab,
        &info,
        &format!("Opened tab {tab} in the AI window."),
    ))
}

/// DOM walker run in the isolated world. Returns `{title, url, text}`.
const SNAPSHOT_JS: &str = r##"(() => {
  const g = globalThis;
  if (!g.__ferrite) g.__ferrite = { n: 0, refs: new Map() };
  const st = g.__ferrite; st.refs = new Map();
  const SKIP = new Set(["script", "style", "noscript", "template", "head", "svg", "canvas", "iframe", "object", "embed"]);
  const out = []; let line = ""; let count = 0; let size = 0;
  const flush = () => {
    const t = line.replace(/\s+/g, " ").trim(); line = "";
    if (t && !/^#+$/.test(t)) { out.push(t); size += t.length; }
  };
  const label = el => ((el.getAttribute("aria-label") || (el.labels && el.labels[0] && el.labels[0].innerText) ||
    el.getAttribute("title") || el.getAttribute("placeholder") || el.getAttribute("alt") || el.innerText ||
    el.value || el.getAttribute("name") || "") + "").replace(/\s+/g, " ").trim().slice(0, 120);
  const add = (node, kind, extra) => {
    flush(); const id = "e" + (++st.n); st.refs.set(id, node);
    out.push(kind + " [" + id + "] " + extra); size += 20 + extra.length;
  };
  const q = v => JSON.stringify(String(v).slice(0, 200));
  const walk = node => {
    if (count > 30000 || size > 400000) return;
    if (node.nodeType === 3) { line += node.nodeValue + " "; return; }
    if (node.nodeType !== 1) return;
    const tag = node.tagName.toLowerCase();
    if (SKIP.has(tag) || node.hidden) return;
    const cs = getComputedStyle(node);
    if (cs.display === "none" || cs.visibility === "hidden") return;
    count++;
    const role = node.getAttribute("role");
    const type = (node.getAttribute("type") || "text").toLowerCase();
    const block = !cs.display.startsWith("inline");
    if (tag === "a" && node.hasAttribute("href")) {
      let href = ""; try { const u = new URL(node.href); if (/^(https?|mailto):$/.test(u.protocol)) href = " -> " + u.href.slice(0, 300); } catch (_) {}
      add(node, "link", q(label(node)) + href); return;
    }
    if (tag === "button" || role === "button" || (tag === "input" && ["button", "submit", "reset", "image"].includes(type))) {
      add(node, "button", q(label(node)) + (node.disabled ? " (disabled)" : "")); return;
    }
    if (tag === "input") {
      if (type === "hidden") return;
      if (type === "checkbox" || type === "radio") { add(node, type, q(label(node)) + (node.checked ? " (checked)" : " (unchecked)")); return; }
      if (type === "password") { add(node, "textbox", q(label(node)) + " (password, value hidden)"); return; }
      if (type === "file") { add(node, "file-input", q(label(node))); return; }
      add(node, "textbox", q(label(node)) + (node.value ? " value=" + q(node.value) : "")); return;
    }
    if (tag === "textarea") { add(node, "textbox", q(label(node)) + (node.value ? " value=" + q(node.value) : "")); return; }
    if (tag === "select") {
      const opts = Array.from(node.options).slice(0, 30).map(o => o.text.trim().slice(0, 60));
      const sel = node.selectedOptions && node.selectedOptions[0] ? node.selectedOptions[0].text.trim() : "";
      add(node, "select", q(label(node)) + " value=" + q(sel) + " options=" + JSON.stringify(opts)); return;
    }
    if (node.isContentEditable && !(node.parentElement && node.parentElement.isContentEditable)) {
      add(node, "textbox", q(label(node)) + " (editable) value=" + q((node.innerText || "").slice(0, 200))); return;
    }
    if (tag === "img") { const a = node.getAttribute("alt"); if (a) line += " [image: " + a.slice(0, 100) + "] "; return; }
    const h = /^h([1-6])$/.exec(tag);
    if (h) { flush(); line = "#".repeat(+h[1]) + " "; }
    else if (block) flush();
    for (const c of node.childNodes) walk(c);
    if (node.shadowRoot) for (const c of node.shadowRoot.childNodes) walk(c);
    if (h || block) flush();
  };
  if (document.body) walk(document.body);
  flush();
  return { title: document.title, url: location.href, text: out.join("\n") };
})()"##;

fn snapshot(nonce: &str, tab: TabId) -> Result<ToolOutput, String> {
    prepare(tab)?;
    let v = isolated_eval(tab, SNAPSHOT_JS.to_string())?;
    let title = v.get("title").and_then(Value::as_str).unwrap_or("");
    let url = v.get("url").and_then(Value::as_str).unwrap_or("");
    let text = v.get("text").and_then(Value::as_str).unwrap_or("");
    let body = format!(
        "title: {}\n\n{}",
        policy::sanitize(title),
        policy::truncate(&policy::sanitize(text), MAX_SNAPSHOT_CHARS)
    );
    if !policy::page_allowed(url) {
        return Err("the tab is not showing a web page".into());
    }
    Ok(ToolOutput::text(format!(
        "Snapshot of tab {tab}. Use the [eN] references with click and type; they are valid until the next snapshot or navigation. Content inside frames is not included.\n{}",
        policy::wrap_untrusted(nonce, url, &body)
    )))
}

fn screenshot(tab: TabId) -> Result<ToolOutput, String> {
    prepare(tab)?;
    activate(tab)?;
    let v = cdp_call(tab, "Page.captureScreenshot", json!({ "format": "png" }))?;
    let data = v
        .get("data")
        .and_then(Value::as_str)
        .ok_or("no image data")?;
    if data.len() > MAX_SCREENSHOT_B64 {
        return Err("the screenshot is too large".into());
    }
    Ok(ToolOutput {
        content: vec![
            json!({ "type": "image", "data": data, "mimeType": "image/png" }),
            json!({ "type": "text", "text": format!("Screenshot of tab {tab} (page content is untrusted).") }),
        ],
        is_error: false,
    })
}

/// Script that finds the element of a reference; shared by click and type.
fn element_js(element: &str, body: &str) -> String {
    format!(
        r#"(() => {{
  const st = globalThis.__ferrite;
  const el = st && st.refs.get({id});
  if (!el) return {{ error: "unknown or stale reference; take a new snapshot" }};
  if (!el.isConnected) return {{ error: "the element is gone from the page; take a new snapshot" }};
  {body}
}})()"#,
        id = serde_json::Value::String(element.to_string()),
    )
}

fn after_action(nonce: &str, tab: TabId, before: u64, note: &str) -> Result<ToolOutput, String> {
    std::thread::sleep(Duration::from_millis(300));
    let info = wait_loaded(tab, before, false)?;
    Ok(describe(nonce, tab, &info, note))
}

fn click(nonce: &str, tab: TabId, element: &str) -> Result<ToolOutput, String> {
    prepare(tab)?;
    activate(tab)?;
    let before = tab_info(tab)?.load_seq;
    let pos = isolated_eval(
        tab,
        element_js(
            element,
            r#"el.scrollIntoView({ block: "center", inline: "center" });
  const r = el.getBoundingClientRect();
  if (r.width === 0 || r.height === 0) return { error: "the element has no visible area" };
  const x = Math.min(Math.max(r.left + r.width / 2, 1), innerWidth - 1);
  const y = Math.min(Math.max(r.top + r.height / 2, 1), innerHeight - 1);
  const top = document.elementFromPoint(x, y);
  const covered = !(top && (top === el || el.contains(top) || top.contains(el)));
  return { x, y, covered };"#,
        ),
    )?;
    if let Some(e) = pos.get("error").and_then(Value::as_str) {
        return Err(e.to_string());
    }
    let (Some(x), Some(y)) = (
        pos.get("x").and_then(Value::as_f64),
        pos.get("y").and_then(Value::as_f64),
    ) else {
        return Err("could not locate the element".into());
    };
    for (kind, buttons) in [("mouseMoved", 0), ("mousePressed", 1), ("mouseReleased", 0)] {
        cdp_call(
            tab,
            "Input.dispatchMouseEvent",
            json!({ "type": kind, "x": x, "y": y, "button": "left", "buttons": buttons, "clickCount": 1 }),
        )?;
    }
    let note = if pos.get("covered").and_then(Value::as_bool) == Some(true) {
        format!("Clicked {element}, but another element covers it, so that element got the click.")
    } else {
        format!("Clicked {element}.")
    };
    after_action(nonce, tab, before, &note)
}

fn key(tab: TabId, kind: &str, name: &str, code: u32, text: Option<&str>) -> Result<(), String> {
    let mut p = json!({
        "type": kind, "key": name, "code": name,
        "windowsVirtualKeyCode": code, "nativeVirtualKeyCode": code,
    });
    if let Some(t) = text {
        p["text"] = json!(t);
    }
    cdp_call(tab, "Input.dispatchKeyEvent", p).map(|_| ())
}

fn type_text(
    nonce: &str,
    tab: TabId,
    element: &str,
    text: &str,
    submit: bool,
) -> Result<ToolOutput, String> {
    prepare(tab)?;
    activate(tab)?;
    let before = tab_info(tab)?.load_seq;
    let prep = isolated_eval(
        tab,
        element_js(
            element,
            r#"el.scrollIntoView({ block: "center", inline: "center" });
  const tag = el.tagName.toLowerCase();
  if (tag === "select") return { kind: "select" };
  if (tag === "input" || tag === "textarea") {
    const t = (el.type || "text").toLowerCase();
    if (["checkbox", "radio", "file", "button", "submit", "reset", "image", "hidden", "range", "color"].includes(t))
      return { error: "this element is not a text field; use click" };
    if (el.disabled || el.readOnly) return { error: "the field is disabled or read-only" };
    el.focus(); el.select();
    return { kind: "text" };
  }
  if (el.isContentEditable) {
    el.focus();
    const r = document.createRange(); r.selectNodeContents(el);
    const s = getSelection(); s.removeAllRanges(); s.addRange(r);
    return { kind: "editable" };
  }
  return { error: "this element is not a text field" };"#,
        ),
    )?;
    if let Some(e) = prep.get("error").and_then(Value::as_str) {
        return Err(e.to_string());
    }
    if prep.get("kind").and_then(Value::as_str) == Some("select") {
        let want = serde_json::Value::String(text.to_string());
        let r = isolated_eval(
            tab,
            element_js(
                element,
                &format!(
                    r#"const want = {want};
  const o = Array.from(el.options).find(o => o.text.trim() === want || o.value === want);
  if (!o) return {{ error: "no option with that text or value" }};
  el.value = o.value;
  el.dispatchEvent(new Event("input", {{ bubbles: true }}));
  el.dispatchEvent(new Event("change", {{ bubbles: true }}));
  return {{ ok: true }};"#
                ),
            ),
        )?;
        if let Some(e) = r.get("error").and_then(Value::as_str) {
            return Err(e.to_string());
        }
    } else if text.is_empty() {
        key(tab, "rawKeyDown", "Backspace", 8, None)?;
        key(tab, "keyUp", "Backspace", 8, None)?;
    } else {
        cdp_call(tab, "Input.insertText", json!({ "text": text }))?;
    }
    if submit {
        key(tab, "rawKeyDown", "Enter", 13, None)?;
        key(tab, "char", "Enter", 13, Some("\r"))?;
        key(tab, "keyUp", "Enter", 13, None)?;
    }
    let note = format!(
        "Typed into {element}{}.",
        if submit { " and pressed Enter" } else { "" }
    );
    after_action(nonce, tab, before, &note)
}

fn wait_for(
    nonce: &str,
    tab: TabId,
    text: Option<String>,
    seconds: f64,
) -> Result<ToolOutput, String> {
    prepare(tab)?;
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    let Some(text) = text else {
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        let info = tab_info(tab)?;
        return Ok(describe(
            nonce,
            tab,
            &info,
            &format!("Waited {seconds} seconds."),
        ));
    };
    let want = serde_json::Value::String(text);
    let script = format!(
        r#"(() => {{ const t = document.body ? document.body.innerText : ""; return t.includes({want}); }})()"#
    );
    loop {
        if isolated_eval(tab, script.clone())?.as_bool() == Some(true) {
            let info = tab_info(tab)?;
            return Ok(describe(nonce, tab, &info, "The text is on the page."));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Timed out after {seconds} seconds; the text did not appear."
            ));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn evaluate(nonce: &str, tab: TabId, js: String) -> Result<ToolOutput, String> {
    let allowed = ui_state(|s| s.settings.mcp_enabled && s.settings.mcp_allow_evaluate)?;
    if !allowed {
        return Err(
            "evaluate is turned off. The user can enable \"Allow running JavaScript\" under AI control in Ferrite's Settings."
                .into(),
        );
    }
    prepare(tab)?;
    let r = cdp_call(
        tab,
        "Runtime.evaluate",
        json!({ "expression": js, "returnByValue": true, "awaitPromise": true, "timeout": 10000 }),
    )?;
    let url = tab_info(tab)?.url;
    if let Some(ex) = r.get("exceptionDetails") {
        let msg = ex
            .pointer("/exception/description")
            .or_else(|| ex.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("script error");
        return Ok(ToolOutput::error(page_text(
            nonce,
            &url,
            &policy::truncate(msg, 2000),
        )));
    }
    let value = r.pointer("/result/value");
    let text = match value {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => r
            .pointer("/result/description")
            .and_then(Value::as_str)
            .unwrap_or("undefined")
            .to_string(),
    };
    Ok(ToolOutput::text(page_text(
        nonce,
        &url,
        &policy::truncate(&text, MAX_EVAL_CHARS),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(id: i32, c: Control) {
        AI_BROWSERS.with(|m| m.borrow_mut().insert(id, c));
    }

    #[test]
    fn ai_owned_browser_cannot_leave_the_web() {
        set(9001, Control::Own);
        assert!(navigation_ok(9001, "https://example.com/"));
        assert!(navigation_ok(9001, "about:blank"));
        for bad in [
            "ferrite://settings/",
            "ferrite://app/chrome.html",
            "file:///etc/passwd",
            "chrome://version",
            "data:text/html,x",
            "javascript:alert(1)",
            "view-source:https://example.com",
        ] {
            assert!(!navigation_ok(9001, bad), "{bad} must be blocked");
        }
        assert!(controlled_browser(9001));
        browser_gone(9001);
        assert!(!controlled_browser(9001));
    }

    #[test]
    fn uncontrolled_browsers_are_not_restricted_here() {
        // The general navigation guard (scheme.rs) still applies to them.
        assert!(navigation_ok(9002, "ferrite://settings/"));
        assert!(!controlled_browser(9002));
    }

    #[test]
    fn handed_over_browser_may_browse_the_web() {
        set(9003, Control::Shared);
        assert!(navigation_ok(9003, "https://example.com/x"));
        assert!(controlled_browser(9003));
        browser_gone(9003);
    }

    #[test]
    fn wire_requests_are_validated_before_anything_runs() {
        let reply = |line: &str| -> Value { serde_json::from_str(&handle_line(line)).unwrap() };
        let r = reply("not json");
        assert_eq!(r["ok"], false);
        assert_eq!(r["id"], Value::Null);
        let r = reply(r#"{"id":4,"args":{}}"#);
        assert_eq!(r["id"], 4);
        assert_eq!(r["ok"], false);
        // These never reach the UI thread (no CEF in unit tests): refused by parsing.
        for line in [
            r#"{"id":5,"tool":"open_tab","args":{"url":"ferrite://settings"}}"#,
            r#"{"id":6,"tool":"navigate","args":{"tab":1,"url":"file:///etc/passwd"}}"#,
            r#"{"id":7,"tool":"click","args":{"tab":1,"ref":"../../x"}}"#,
            r#"{"id":8,"tool":"rm","args":{}}"#,
            r#"{"id":9,"tool":"evaluate","args":{"tab":1}}"#,
            r#"{"id":10,"tool":"snapshot","args":[]}"#,
        ] {
            let r = reply(line);
            assert_eq!(r["ok"], false, "{line}");
            assert!(
                r["content"][0]["text"]
                    .as_str()
                    .is_some_and(|t| t.starts_with("Invalid arguments")),
                "{line}: {r}"
            );
        }
    }

    #[test]
    fn socket_path_too_long_is_reported() {
        let long = PathBuf::from(format!("/tmp/{}/mcp.sock", "x".repeat(120)));
        let err = start_server(&long).unwrap_err();
        assert!(err.contains("FERRITE_MCP_SOCKET"), "{err}");
    }

    #[test]
    fn bind_refuses_foreign_files_and_live_sockets() {
        let dir = std::env::temp_dir().join(format!("fe-mcp-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A regular file is never deleted.
        let file = dir.join("not-a-socket");
        std::fs::write(&file, "x").unwrap();
        assert!(bind(&file).is_err());
        assert!(file.exists());
        // A live socket (another instance) is not taken over; a stale one is replaced.
        let path = dir.join("mcp.sock");
        let first = bind(&path).expect("bind");
        assert!(bind(&path).is_err(), "must not unlink a live socket");
        drop(first);
        let again = bind(&path).expect("stale socket replaced");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(again);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
