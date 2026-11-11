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
//! DevTools calls go through the shared layer in [`super::cdp`] (one id
//! counter, one observer per browser, answers routed to the call that sent
//! them).
//!
//! Everything here runs on the CEF UI thread, except [`hold`] (IO thread).

use super::cdp;
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
    /// Token of the add call still waiting for its identifier.
    pending: Option<u64>,
    /// Drop the about:blank history entry once the first page commits.
    reset_history: bool,
    /// Capture script and binding were sent for this browser.
    capture: bool,
    /// Token of the capture script's add call still waiting for its identifier.
    capture_pending: Option<u64>,
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
            "Page.resetNavigationHistory",
            serde_json::json!({}),
        );
    }
}

thread_local! {
    static ENTRIES: RefCell<HashMap<i32, Entry>> = RefCell::new(HashMap::new());
    static TOKEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn send(browser: &mut Browser, method: &str, params: serde_json::Value) -> Option<i32> {
    cdp::send(browser, method, params)
}

fn remove(browser: &mut Browser, identifier: &str) {
    send(
        browser,
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
    // removed when it arrives, see add_done).
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
    // Ids are not known before the call is sent; the reply compares against
    // the entry's `pending` marker, a per-call token.
    let token = TOKEN.with(|t| {
        let n = t.get() + 1;
        t.set(n);
        n
    });
    ENTRIES.with(|e| {
        if let Some(en) = e.borrow_mut().get_mut(&browser_id) {
            en.host = host.clone();
            en.pending = Some(token);
        }
    });
    let sent = cdp::request(
        browser,
        "Page.addScriptToEvaluateOnNewDocument",
        serde_json::json!({ "source": guarded(&host, &body) }),
        cdp::DEFAULT_TIMEOUT_MS,
        Box::new(move |browser, res| add_done(browser_id, token, browser, res)),
    );
    if sent.is_some() {
        close_gate(browser_id);
    } else {
        ENTRIES.with(|e| {
            if let Some(en) = e.borrow_mut().get_mut(&browser_id) {
                en.host.clear();
                en.pending = None;
            }
        });
    }
}

/// The registration of the content-blocking script was answered. `token` is
/// still current: remember the identifier. Otherwise the registration was
/// superseded (another navigation, blocking switched off): remove it.
fn add_done(browser_id: i32, token: u64, browser: Option<&mut Browser>, res: cdp::CdpResult) {
    let current = ENTRIES.with(|e| {
        e.borrow()
            .get(&browser_id)
            .is_some_and(|en| en.pending == Some(token))
    });
    if current {
        open_gate(browser_id, None);
    }
    let identifier = match res {
        Ok(v) => v
            .get("identifier")
            .and_then(|i| i.as_str())
            .map(str::to_string),
        Err(err) => {
            tracing::debug!("script registration failed for browser {browser_id}: {err}");
            None
        }
    };
    let Some(identifier) = identifier else {
        if current {
            ENTRIES.with(|e| {
                if let Some(en) = e.borrow_mut().get_mut(&browser_id) {
                    en.pending = None;
                    en.host.clear();
                }
            });
        }
        return;
    };
    if current {
        ENTRIES.with(|e| {
            if let Some(en) = e.borrow_mut().get_mut(&browser_id) {
                en.pending = None;
                en.identifier = Some(identifier);
            }
        });
    } else if let Some(b) = browser {
        remove(b, &identifier);
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
    cdp::ensure(browser);
    // bindingCalled events are only delivered while Runtime is enabled.
    send(browser, "Runtime.enable", serde_json::json!({}));
    send(
        browser,
        "Runtime.addBinding",
        serde_json::json!({ "name": CAPTURE_BINDING, "executionContextName": CAPTURE_WORLD }),
    );
    let token = TOKEN.with(|t| {
        let n = t.get() + 1;
        t.set(n);
        n
    });
    ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        let en = e.entry(browser_id).or_default();
        en.capture = true;
        en.capture_pending = Some(token);
    });
    let sent = cdp::request(
        browser,
        "Page.addScriptToEvaluateOnNewDocument",
        serde_json::json!({ "source": CAPTURE_JS, "worldName": CAPTURE_WORLD }),
        cdp::DEFAULT_TIMEOUT_MS,
        Box::new(move |browser, res| capture_done(browser_id, token, browser, res)),
    );
    if sent.is_none() {
        ENTRIES.with(|e| {
            if let Some(en) = e.borrow_mut().get_mut(&browser_id) {
                en.capture = false;
                en.capture_pending = None;
            }
        });
    }
}

/// The capture script's registration was answered.
fn capture_done(browser_id: i32, token: u64, browser: Option<&mut Browser>, res: cdp::CdpResult) {
    let identifier = res.ok().and_then(|v| {
        v.get("identifier")
            .and_then(|i| i.as_str())
            .map(str::to_string)
    });
    // Remember it only while the registration is still wanted.
    let keep = ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        let en = e.get_mut(&browser_id)?;
        if en.capture_pending != Some(token) {
            return None;
        }
        en.capture_pending = None;
        let keep = CAPTURE.load(std::sync::atomic::Ordering::Relaxed) && en.capture;
        en.capture_identifier = identifier.clone().filter(|_| keep);
        Some(keep)
    });
    if keep.is_some() {
        tracing::debug!("login capture: confirmed for browser {browser_id}");
    }
    if keep != Some(true) {
        // Switched off, or superseded, while the call was in flight.
        if let (Some(identifier), Some(b)) = (identifier, browser) {
            remove(b, &identifier);
        }
    }
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
            "Runtime.removeBinding",
            serde_json::json!({ "name": CAPTURE_BINDING }),
        );
        send(&mut b, "Runtime.disable", serde_json::json!({}));
    }
}

/// Run `expression` in the page's main world (main frame). `reply` gets the
/// DevTools answer (see [`cdp::request`]).
pub fn evaluate(browser: &mut Browser, expression: &str, reply: cdp::Reply) -> bool {
    cdp::request(
        browser,
        "Runtime.evaluate",
        serde_json::json!({ "expression": expression, "returnByValue": true }),
        5000,
        reply,
    )
    .is_some()
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
    cdp::closed(browser_id);
}

pub fn attach(browser: &mut Browser) {
    ENTRIES.with(|e| {
        e.borrow_mut()
            .entry(browser.identifier())
            .or_default()
            .reset_history = true;
    });
    cdp::ensure(browser);
}

/// `Runtime.bindingCalled`: only our binding, only the payload string. The
/// binding exists only in the isolated world, so the page cannot call it.
pub(super) fn on_event(browser: Option<&mut Browser>, method: &str, params: &[u8]) {
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
