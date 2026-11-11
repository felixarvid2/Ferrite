//! Document-start script injection for content blocking (uBlock Origin
//! scriptlets and cosmetic CSS), the way Brave applies them.
//!
//! For each main-frame navigation of a content browser, the script for the
//! target host is registered with the DevTools protocol method
//! `Page.addScriptToEvaluateOnNewDocument`, which runs it in the page's main
//! world before any of the page's own scripts. The previous registration is
//! removed with `Page.removeScriptToEvaluateOnNewDocument` once its
//! identifier is known. Every script also checks `location.hostname`, so a
//! registration that has not been removed yet does nothing on another host.
//!
//! A new browser's first navigation can commit before the registration
//! reaches the renderer, so the main-frame request is held (see [`hold`])
//! until CDP has confirmed the registration, for at most [`GATE_MAX_MS`].
//!
//! Limits: only the main frame's host is covered (same-process iframes of
//! another host are skipped by the hostname check; cross-process iframes
//! are separate DevTools targets and get nothing).
//!
//! Everything here runs on the CEF UI thread, except [`hold`] (IO thread).

use crate::engine::task::{post_ui_delayed, SendBox};
use cef::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

/// Longest a main-frame request waits for the script registration.
const GATE_MAX_MS: i64 = 1000;

/// Closed gates: browser id -> (generation, held requests).
type Gates = HashMap<i32, (u64, Vec<SendBox<Callback>>)>;
static GATES: Mutex<Option<Gates>> = Mutex::new(None);
static GATE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn close_gate(browser_id: i32) {
    let generation = GATE_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut g) = GATES.lock() {
        let held = g
            .get_or_insert_with(HashMap::new)
            .insert(browser_id, (generation, Vec::new()));
        // Requests held by an older gate go on now.
        for cb in held.map(|(_, v)| v).unwrap_or_default() {
            cb.into_inner().cont();
        }
    }
    post_ui_delayed(GATE_MAX_MS, move || open_gate(browser_id, Some(generation)));
}

/// Open the gate (only if it is still `generation`, when given).
fn open_gate(browser_id: i32, generation: Option<u64>) {
    let held = GATES.lock().ok().and_then(|mut g| {
        let gates = g.as_mut()?;
        let current = gates.get(&browser_id)?.0;
        if generation.is_some_and(|gen| gen != current) {
            return None;
        }
        if generation.is_some() {
            tracing::debug!("script registration timed out for browser {browser_id}");
        }
        gates.remove(&browser_id).map(|(_, v)| v)
    });
    for cb in held.unwrap_or_default() {
        cb.into_inner().cont();
    }
}

/// IO thread, main-frame request: true if the request must wait (the
/// callback is continued once the script is registered).
pub fn hold(browser_id: i32, callback: &mut Callback) -> bool {
    let Ok(mut g) = GATES.lock() else {
        return false;
    };
    match g.as_mut().and_then(|gates| gates.get_mut(&browser_id)) {
        Some((_, held)) => {
            held.push(SendBox::new(callback.clone()));
            true
        }
        None => false,
    }
}

#[derive(Default)]
struct Entry {
    /// Host the current registration is for ("" = none).
    host: String,
    /// Identifier of the registered script, once CDP has answered.
    identifier: Option<String>,
    /// Message id of the add call still waiting for its identifier.
    pending: Option<i32>,
    /// Keeps the DevTools observer registered while the browser lives.
    _observer: Option<Registration>,
    /// Drop the about:blank history entry once the first page commits.
    reset_history: bool,
}

/// The main frame's URL changed (navigation committed).
pub fn committed(browser: &mut Browser, url: &str) {
    if url.is_empty() || url == "about:blank" {
        return;
    }
    let reset = ENTRIES.with(|e| {
        e.borrow_mut()
            .get_mut(&browser.identifier())
            .is_some_and(|en| std::mem::take(&mut en.reset_history))
    });
    if reset {
        send(
            browser,
            next_id(),
            "Page.resetNavigationHistory",
            serde_json::json!({}),
        );
    }
}

thread_local! {
    static ENTRIES: RefCell<HashMap<i32, Entry>> = RefCell::new(HashMap::new());
    static NEXT_ID: std::cell::Cell<i32> = const { std::cell::Cell::new(1_000_000) };
}

fn next_id() -> i32 {
    NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id.wrapping_add(1).max(1_000_000));
        id
    })
}

fn send(browser: &mut Browser, id: i32, method: &str, params: serde_json::Value) -> bool {
    let Some(host) = browser.host() else {
        return false;
    };
    let msg = serde_json::json!({ "id": id, "method": method, "params": params });
    host.send_dev_tools_message(Some(msg.to_string().as_bytes())) != 0
}

fn remove(browser: &mut Browser, identifier: &str) {
    send(
        browser,
        next_id(),
        "Page.removeScriptToEvaluateOnNewDocument",
        serde_json::json!({ "identifier": identifier }),
    );
}

/// Host of an http(s) URL, lowercase.
fn http_host(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    matches!(u.scheme(), "http" | "https").then(|| u.host_str().map(str::to_ascii_lowercase))?
}

/// Wrap `body` so it only runs on `host`.
fn guarded(host: &str, body: &str) -> String {
    let host = serde_json::Value::String(host.to_string());
    format!("if(location.hostname==={host}){{{body}\n}}")
}

/// A main-frame navigation to `url` is starting in a content browser.
/// `script_for` returns the script body for a URL (None = nothing to run).
pub fn navigation(
    browser: &mut Browser,
    url: &str,
    script_for: impl FnOnce(&str) -> Option<String>,
) {
    let browser_id = browser.identifier();
    let host = http_host(url).unwrap_or_default();
    let same = ENTRIES.with(|e| {
        e.borrow()
            .get(&browser_id)
            .is_some_and(|en| en.host == host)
    });
    if same {
        return;
    }
    // Drop the old registration (or forget a pending one: its answer is
    // removed when it arrives, see on_result).
    let old = ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        let en = e.entry(browser_id).or_default();
        en.host.clear();
        en.pending = None;
        en.identifier.take()
    });
    if let Some(old) = old {
        remove(browser, &old);
    }
    if host.is_empty() {
        return;
    }
    let Some(body) = script_for(url) else { return };
    ensure_observer(browser);
    let id = next_id();
    let ok = send(
        browser,
        id,
        "Page.addScriptToEvaluateOnNewDocument",
        serde_json::json!({ "source": guarded(&host, &body) }),
    );
    if ok {
        close_gate(browser_id);
        ENTRIES.with(|e| {
            if let Some(en) = e.borrow_mut().get_mut(&browser_id) {
                en.host = host;
                en.pending = Some(id);
            }
        });
    }
}

/// The script for a page at `url`: uBlock Origin scriptlets plus element
/// hiding CSS from the content blocker (None when blocking is off or the site is an exception, the
/// filter lists are not loaded yet, or nothing applies).
pub fn blocking_script(url: &str) -> Option<String> {
    use super::handlers::{ADBLOCK, BLOCKER};
    if !ADBLOCK.load(std::sync::atomic::Ordering::Relaxed) || super::handlers::is_excepted(url) {
        return None;
    }
    let blocker = BLOCKER.read().ok().and_then(|b| b.clone())?;
    let cosmetic = blocker.cosmetic(url)?;
    let mut js = cosmetic.script;
    if !cosmetic.css.is_empty() {
        // A constructed stylesheet needs no DOM node, so it works at
        // document start; invalid rules are dropped one by one.
        let css = serde_json::Value::String(cosmetic.css);
        js.push_str(&format!(
            "\ntry{{const s=new CSSStyleSheet();s.replaceSync({css});\
             document.adoptedStyleSheets=[...document.adoptedStyleSheets,s];}}catch(e){{}}"
        ));
    }
    Some(js)
}

/// Content blocking was switched off: remove every registration.
pub fn clear_all(mut browser_of: impl FnMut(i32) -> Option<Browser>) {
    let all: Vec<(i32, Option<String>)> = ENTRIES.with(|e| {
        e.borrow_mut()
            .iter_mut()
            .map(|(id, en)| {
                en.host.clear();
                en.pending = None;
                (*id, en.identifier.take())
            })
            .collect()
    });
    for (id, identifier) in all {
        if let (Some(identifier), Some(mut b)) = (identifier, browser_of(id)) {
            remove(&mut b, &identifier);
        }
    }
}

/// The browser closed.
pub fn closed(browser_id: i32) {
    ENTRIES.with(|e| e.borrow_mut().remove(&browser_id));
    open_gate(browser_id, None);
}

pub fn attach(browser: &mut Browser) {
    ENTRIES.with(|e| {
        e.borrow_mut()
            .entry(browser.identifier())
            .or_default()
            .reset_history = true;
    });
    ensure_observer(browser);
}

fn ensure_observer(browser: &mut Browser) {
    let browser_id = browser.identifier();
    let has = ENTRIES.with(|e| {
        e.borrow()
            .get(&browser_id)
            .is_some_and(|en| en._observer.is_some())
    });
    if has {
        return;
    }
    let Some(host) = browser.host() else { return };
    let mut observer = InjectObserver::new(browser_id);
    let reg = host.add_dev_tools_message_observer(Some(&mut observer));
    send(browser, next_id(), "Page.enable", serde_json::json!({}));
    ENTRIES.with(|e| {
        if let Some(en) = e.borrow_mut().get_mut(&browser_id) {
            en._observer = reg;
        }
    });
}

fn on_result(
    browser: Option<&mut Browser>,
    browser_id: i32,
    message_id: i32,
    success: bool,
    result: &[u8],
) {
    let identifier = serde_json::from_slice::<serde_json::Value>(result)
        .ok()
        .and_then(|v| {
            v.get("identifier")
                .and_then(|i| i.as_str())
                .map(str::to_string)
        });
    let was_pending = ENTRIES.with(|e| {
        e.borrow()
            .get(&browser_id)
            .is_some_and(|en| en.pending == Some(message_id))
    });
    if was_pending {
        open_gate(browser_id, None);
    }
    if !success {
        tracing::debug!(
            "devtools call {message_id} failed: {}",
            String::from_utf8_lossy(result)
        );
    }
    let Some(identifier) = identifier else { return };
    // Ours and still current: remember it. Otherwise (superseded add) remove.
    let current = ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        match e.get_mut(&browser_id) {
            Some(en) if en.pending == Some(message_id) => {
                en.pending = None;
                en.identifier = Some(identifier.clone());
                true
            }
            _ => false,
        }
    });
    if !current && message_id >= 1_000_000 {
        if let Some(b) = browser {
            remove(b, &identifier);
        }
    }
}

wrap_dev_tools_message_observer! {
    struct InjectObserver {
        browser_id: i32,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            browser: Option<&mut Browser>,
            message_id: ::std::os::raw::c_int,
            success: ::std::os::raw::c_int,
            result: Option<&[u8]>,
        ) {
            on_result(browser, self.browser_id, message_id, success != 0, result.unwrap_or_default());
        }
    }
}
