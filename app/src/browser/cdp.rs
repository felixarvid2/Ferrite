//! One shared layer over the DevTools protocol (`send_dev_tools_message`).
//!
//! Several features talk CDP to the same browser: content blocking and login
//! capture ([`super::inject`]), the Proton Pass 2FA fill ([`super::passwords`])
//! and the MCP tools ([`super::mcp`]). They all go through here so that
//!
//! - message ids come from one counter (never reused, so an answer can only
//!   belong to the call that was sent with its id),
//! - every call that wants its answer registers a reply closure that is
//!   routed by id, with a timeout, and failed when the browser closes,
//! - there is exactly one DevTools observer per browser.
//!
//! Results nobody registered for (fire-and-forget calls such as
//! `Page.enable`) are dropped. Events go to [`super::inject::on_event`].
//!
//! UI thread only. A reply closure runs inside CEF's observer callback: it
//! must not panic and must not touch the browser state directly (use
//! [`super::later`], or just send on a channel).

use super::inject;
use crate::engine::post_local_delayed;
use cef::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// The answer to a call: the protocol's `result` object, or an error text
/// (protocol error, timeout, browser closed).
pub type CdpResult = Result<Value, String>;

/// Runs once, on the UI thread. The browser is `None` for timeouts and when
/// the browser is gone.
pub type Reply = Box<dyn FnOnce(Option<&mut Browser>, CdpResult)>;

/// Longest a reply is awaited unless the caller says otherwise.
pub const DEFAULT_TIMEOUT_MS: i64 = 10_000;

/// Largest answer parsed (a full-page PNG as base64 is a few MB).
const MAX_RESULT_BYTES: usize = 32 * 1024 * 1024;

struct Pending {
    browser_id: i32,
    reply: Reply,
}

thread_local! {
    static NEXT_ID: Cell<i32> = const { Cell::new(1_000_000) };
    static PENDING: RefCell<HashMap<i32, Pending>> = RefCell::new(HashMap::new());
    /// Keeps each browser's observer registered while the browser lives.
    static OBSERVERS: RefCell<HashMap<i32, Registration>> = RefCell::new(HashMap::new());
}

/// A fresh message id (>= 1_000_000, wraps before overflowing).
pub fn next_id() -> i32 {
    NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id.wrapping_add(1).max(1_000_000));
        id
    })
}

/// Make sure the browser has our observer (and the Page domain enabled).
pub fn ensure(browser: &mut Browser) {
    let browser_id = browser.identifier();
    if OBSERVERS.with(|o| o.borrow().contains_key(&browser_id)) {
        return;
    }
    let Some(host) = browser.host() else { return };
    let mut observer = CdpObserver::new(browser_id);
    let Some(reg) = host.add_dev_tools_message_observer(Some(&mut observer)) else {
        return;
    };
    OBSERVERS.with(|o| o.borrow_mut().insert(browser_id, reg));
    send(browser, "Page.enable", serde_json::json!({}));
}

/// Fire and forget; the answer is ignored. Returns the message id.
pub fn send(browser: &mut Browser, method: &str, params: Value) -> Option<i32> {
    let id = next_id();
    send_with_id(browser, id, method, params).then_some(id)
}

fn send_with_id(browser: &mut Browser, id: i32, method: &str, params: Value) -> bool {
    let Some(host) = browser.host() else {
        return false;
    };
    let msg = serde_json::json!({ "id": id, "method": method, "params": params });
    host.send_dev_tools_message(Some(msg.to_string().as_bytes())) != 0
}

/// Send a call and route its answer to `reply` (once). Returns the message
/// id, or `None` if the call could not be sent (then `reply` is dropped
/// without running: the caller reports the failure itself).
pub fn request(
    browser: &mut Browser,
    method: &str,
    params: Value,
    timeout_ms: i64,
    reply: Reply,
) -> Option<i32> {
    ensure(browser);
    let id = next_id();
    let browser_id = browser.identifier();
    // Registered before sending: the answer may come back synchronously.
    PENDING.with(|p| p.borrow_mut().insert(id, Pending { browser_id, reply }));
    if !send_with_id(browser, id, method, params) {
        PENDING.with(|p| p.borrow_mut().remove(&id));
        return None;
    }
    post_local_delayed(timeout_ms.max(1), move || {
        fail(id, "DevTools call timed out");
    });
    Some(id)
}

/// Fail one pending call (no-op if it was answered meanwhile).
fn fail(id: i32, message: &str) {
    if let Some(p) = PENDING.with(|p| p.borrow_mut().remove(&id)) {
        (p.reply)(None, Err(message.to_string()));
    }
}

/// Fail every pending call of a browser (it closed, or control was revoked).
pub fn fail_browser(browser_id: i32, message: &str) {
    let ids: Vec<i32> = PENDING.with(|p| {
        p.borrow()
            .iter()
            .filter(|(_, v)| v.browser_id == browser_id)
            .map(|(k, _)| *k)
            .collect()
    });
    for id in ids {
        fail(id, message);
    }
}

/// The browser closed: fail its calls and drop its observer.
pub fn closed(browser_id: i32) {
    fail_browser(browser_id, "the tab was closed");
    OBSERVERS.with(|o| o.borrow_mut().remove(&browser_id));
}

fn on_result(browser: Option<&mut Browser>, message_id: i32, success: bool, result: &[u8]) {
    let Some(p) = PENDING.with(|p| p.borrow_mut().remove(&message_id)) else {
        if !success {
            tracing::debug!(
                "devtools call {message_id} failed: {}",
                String::from_utf8_lossy(&result[..result.len().min(300)])
            );
        }
        return;
    };
    let outcome = if result.len() > MAX_RESULT_BYTES {
        Err("DevTools answer too large".to_string())
    } else {
        let value: Value = serde_json::from_slice(result).unwrap_or(Value::Null);
        if success {
            Ok(value)
        } else {
            Err(value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("DevTools call failed")
                .to_string())
        }
    };
    (p.reply)(browser, outcome);
}

wrap_dev_tools_message_observer! {
    struct CdpObserver {
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
            let _ = self.browser_id;
            on_result(browser, message_id, success != 0, result.unwrap_or_default());
        }

        fn on_dev_tools_event(
            &self,
            browser: Option<&mut Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            inject::on_event(browser, &crate::engine::opt_str(method), params.unwrap_or_default());
        }
    }
}
