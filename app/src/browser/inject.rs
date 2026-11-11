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
//! Login capture for "save to Proton Pass" (see [`capture_ensure`]) uses the
//! same DevTools session: an isolated-world script that reports submitted
//! logins through a `Runtime.addBinding` function that exists only in that
//! world. [`evaluate`] runs one expression in the page for the 2FA fill.
//!
//! Everything here runs on the CEF UI thread, except [`hold`] (IO thread).

use crate::engine::task::{post_ui_delayed, SendBox};
use cef::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

/// Login capture is wanted (Proton Pass enabled and "offer to save" on).
/// Set from settings on the UI thread, read by the tab code (deferral).
pub static CAPTURE: AtomicBool = AtomicBool::new(false);

/// Name of the binding and of the isolated world that owns it.
const CAPTURE_BINDING: &str = "__ferritePassSave";
const CAPTURE_WORLD: &str = "ferrite-pass";

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
    /// Capture script and binding were sent for this browser.
    capture: bool,
    /// Message id of the capture script's add call still waiting for its identifier.
    capture_pending: Option<i32>,
    capture_identifier: Option<String>,
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

pub fn next_id() -> i32 {
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

/// Page script run in the isolated world `ferrite-pass`. It shares the DOM
/// with the page but not its JavaScript objects, so the page can neither see
/// the binding nor tamper with the code. It reports a submitted login
/// (username, password) from the top frame only, at most once per 2 s, and
/// only for real user input (`isTrusted`) except the `submit` event itself.
const CAPTURE_JS: &str = r#"(function () {
  if (window !== window.top) return;
  var last = 0;
  var TEXT = ['text', 'email', 'tel'];
  function vis(e) {
    if (!e || e.disabled) return false;
    var r = e.getBoundingClientRect(), cs = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none';
  }
  function pwOf(scope) {
    return [].slice.call(scope.querySelectorAll('input[type=password]')).filter(function (e) { return vis(e) && e.value; });
  }
  function collect(scope) {
    var pws = pwOf(scope);
    if (!pws.length) return null;
    // Sign-up (equal values) or change-password (different): the last one.
    var pw = pws[pws.length - 1];
    var inputs = [].slice.call(scope.querySelectorAll('input')).filter(vis);
    var ue = inputs.find(function (e) {
      var a = (e.getAttribute('autocomplete') || '').toLowerCase().split(/\s+/);
      return e !== pw && e.value && TEXT.indexOf((e.type || 'text').toLowerCase()) >= 0 && (a.indexOf('username') >= 0 || a.indexOf('email') >= 0);
    });
    if (!ue) {
      for (var i = inputs.indexOf(pw) - 1; i >= 0; i--) {
        if (inputs[i].value && TEXT.indexOf((inputs[i].type || 'text').toLowerCase()) >= 0) { ue = inputs[i]; break; }
      }
    }
    var user = ue ? ue.value : '';
    if (pw.value.length > 1000 || user.length > 500) return null;
    return { username: user, password: pw.value };
  }
  function report(scope) {
    var now = Date.now();
    if (now - last < 2000) return;
    var d = collect(scope);
    var f = window.__ferritePassSave;
    if (!d || typeof f !== 'function') return;
    last = now;
    try { f(JSON.stringify(d)); } catch (_) {}
  }
  function scopeOf(el) {
    if (el.form) return el.form;
    var f = el.closest && el.closest('form');
    if (f) return f;
    for (var n = el, i = 0; n && i < 6; n = n.parentElement, i++) {
      if (n.querySelector && n.querySelector('input[type=password]')) return n;
    }
    return null;
  }
  document.addEventListener('submit', function (e) {
    if (e.target && e.target.tagName === 'FORM') report(e.target);
  }, true);
  document.addEventListener('keydown', function (e) {
    var t = e.target;
    if (!e.isTrusted || e.key !== 'Enter' || !t || t.tagName !== 'INPUT') return;
    var s = scopeOf(t);
    if (s) report(s);
  }, true);
  document.addEventListener('click', function (e) {
    if (!e.isTrusted || !e.target || !e.target.closest) return;
    var b = e.target.closest('button, input[type=submit], input[type=button], [role=button], a');
    if (!b) return;
    var label = (b.type === 'submit' ? 'submit ' : '') + (b.innerText || b.value || b.getAttribute('aria-label') || '');
    if (!/submit|log\s?in|sign\s?in|continue|next|enter|verify|register|sign\s?up|create|logga|anm.l/i.test(label)) return;
    var s = scopeOf(b);
    if (s) report(s);
  }, true);
})();"#;

/// Register login capture for this browser (once per browser session, not
/// per navigation): the binding exists only in the isolated world, the script
/// runs in every new document in that world. Call it as early as possible
/// (browser creation, or when the feature is switched on): a session whose
/// first DevTools message is sent while a navigation is starting does not
/// get the script in time for that page, and holding the request (see
/// [`hold`]) does not help, DevTools only answers after the request goes on.
pub fn capture_ensure(browser: &mut Browser) {
    if !CAPTURE.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let browser_id = browser.identifier();
    let done = ENTRIES.with(|e| e.borrow().get(&browser_id).is_some_and(|en| en.capture));
    if done {
        return;
    }
    tracing::debug!("login capture: registering for browser {browser_id}");
    ensure_observer(browser);
    // bindingCalled events are only delivered while Runtime is enabled.
    send(browser, next_id(), "Runtime.enable", serde_json::json!({}));
    send(
        browser,
        next_id(),
        "Runtime.addBinding",
        serde_json::json!({ "name": CAPTURE_BINDING, "executionContextName": CAPTURE_WORLD }),
    );
    let id = next_id();
    let ok = send(
        browser,
        id,
        "Page.addScriptToEvaluateOnNewDocument",
        serde_json::json!({ "source": CAPTURE_JS, "worldName": CAPTURE_WORLD }),
    );
    if !ok {
        return;
    }
    ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        let en = e.entry(browser_id).or_default();
        en.capture = true;
        en.capture_pending = Some(id);
    });
}

/// Is the capture registration of this browser still waiting for DevTools'
/// answer? (A deferred first navigation waits for it, see `tabs.rs`.)
pub fn capture_waiting(browser_id: i32) -> bool {
    CAPTURE.load(std::sync::atomic::Ordering::Relaxed)
        && ENTRIES.with(|e| {
            e.borrow()
                .get(&browser_id)
                .is_some_and(|en| en.capture && en.capture_pending.is_some())
        })
}

/// Capture was switched off: remove script and binding everywhere.
pub fn capture_clear_all(mut browser_of: impl FnMut(i32) -> Option<Browser>) {
    let all: Vec<(i32, Option<String>)> = ENTRIES.with(|e| {
        e.borrow_mut()
            .iter_mut()
            .filter(|(_, en)| en.capture)
            .map(|(id, en)| {
                en.capture = false;
                en.capture_pending = None;
                (*id, en.capture_identifier.take())
            })
            .collect()
    });
    for (id, identifier) in all {
        let Some(mut b) = browser_of(id) else {
            continue;
        };
        if let Some(identifier) = identifier {
            remove(&mut b, &identifier);
        }
        send(
            &mut b,
            next_id(),
            "Runtime.removeBinding",
            serde_json::json!({ "name": CAPTURE_BINDING }),
        );
        send(&mut b, next_id(), "Runtime.disable", serde_json::json!({}));
    }
}

/// Run `expression` in the page's main world (main frame). The result
/// arrives through `passwords::eval_result` under the returned message id.
pub fn evaluate(browser: &mut Browser, expression: &str) -> Option<i32> {
    ensure_observer(browser);
    let id = next_id();
    send(
        browser,
        id,
        "Runtime.evaluate",
        serde_json::json!({ "expression": expression, "returnByValue": true }),
    )
    .then_some(id)
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
    // The entry may not exist yet (capture on a browser that was not attached
    // at creation): dropping the registration would unregister the observer.
    ENTRIES.with(|e| e.borrow_mut().entry(browser_id).or_default()._observer = reg);
}

fn on_result(
    browser: Option<&mut Browser>,
    browser_id: i32,
    message_id: i32,
    success: bool,
    result: &[u8],
) {
    if super::passwords::eval_result(message_id, success, result) {
        return;
    }
    let identifier = serde_json::from_slice::<serde_json::Value>(result)
        .ok()
        .and_then(|v| {
            v.get("identifier")
                .and_then(|i| i.as_str())
                .map(str::to_string)
        });
    // The capture script's registration: remember its identifier. It never
    // holds requests, so the content-blocking gate is not involved.
    let capture = ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        let en = e.get_mut(&browser_id)?;
        if en.capture_pending != Some(message_id) {
            return None;
        }
        en.capture_pending = None;
        en.capture_identifier = identifier.clone().filter(|_| en.capture);
        Some(())
    });
    if capture.is_some() {
        tracing::debug!("login capture: confirmed for browser {browser_id}");
        if !CAPTURE.load(std::sync::atomic::Ordering::Relaxed) {
            // Switched off while the call was in flight.
            if let (Some(identifier), Some(b)) = (identifier, browser) {
                remove(b, &identifier);
            }
        }
        return;
    }
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

/// `Runtime.bindingCalled`: only our binding, only the payload string. The
/// binding exists only in the isolated world, so the page cannot call it.
fn on_event(browser: Option<&mut Browser>, method: &str, params: &[u8]) {
    if method != "Runtime.bindingCalled" || params.len() > 16 * 1024 {
        return;
    }
    let Some(browser) = browser else { return };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(params) else {
        return;
    };
    if v.get("name").and_then(|n| n.as_str()) != Some(CAPTURE_BINDING) {
        return;
    }
    let Some(payload) = v
        .get("payload")
        .and_then(|p| p.as_str())
        .map(str::to_string)
    else {
        return;
    };
    // The site is the main frame's current URL, never anything from the payload.
    let url = browser
        .main_frame()
        .map(|f| crate::engine::from_userfree(&f.url()))
        .unwrap_or_default();
    let browser_id = browser.identifier();
    tracing::debug!("login capture reported by browser {browser_id}");
    super::later(move |s| super::passwords::capture(s, browser_id, url, payload));
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

        fn on_dev_tools_event(
            &self,
            browser: Option<&mut Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            on_event(browser, &crate::engine::opt_str(method), params.unwrap_or_default());
        }
    }
}
