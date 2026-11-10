//! The `ferrite://` scheme: serves the UI and internal pages, and the JSON
//! API they use (`ferrite://<host>/api/<command>`, POST).
//!
//! Security model:
//! * Web content can never use the API: requests are only answered for the
//!   main frame of a browser whose current document is itself a ferrite://
//!   page, and only for fetch/XHR requests.
//! * Web pages cannot navigate to ferrite:// pages (see [`navigation_allowed`]),
//!   embed them in iframes, or load their files as subresources.

use super::{api, assets, with_state};
use crate::engine::{self, cef_str, from_userfree, SendBox, SCHEME};
use cef::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

thread_local! {
    /// ferrite:// URLs the browser itself is about to load (typed by the
    /// user, opened from menus, ...). Consumed by the navigation guard.
    static ALLOWED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

pub fn is_internal(url: &str) -> bool {
    url.starts_with("ferrite:")
}

/// Mark a browser-initiated navigation to an internal page as allowed.
pub fn allow_internal(url: &str) {
    if is_internal(url) {
        ALLOWED.with(|a| a.borrow_mut().insert(normalize(url)));
    }
}

/// Chromium treats ferrite:// as a standard scheme, so `ferrite://history`
/// arrives as `ferrite://history/`; the `url` crate does not add the slash
/// for non-special schemes, so do it here.
fn normalize(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut u) => {
            if u.path().is_empty() {
                u.set_path("/");
            }
            u.to_string()
        }
        Err(_) => url.to_string(),
    }
}

/// Called from `on_before_browse`: may `frame` (currently showing
/// `current_url`) navigate to `target`?
pub fn navigation_allowed(current_url: &str, target: &str, is_main: bool) -> bool {
    if !is_internal(target) {
        return true;
    }
    if !is_main {
        return false;
    }
    let from_internal = is_internal(current_url);
    let fresh = current_url.is_empty() || current_url == "about:blank";
    let browser_initiated = ALLOWED.with(|a| a.borrow_mut().remove(&normalize(target)));
    browser_initiated || from_internal || fresh
}

pub fn register(ctx: Option<&RequestContext>) {
    let mut factory = FerriteSchemeFactory::new();
    let scheme = cef_str(SCHEME);
    match ctx {
        Some(ctx) => {
            ctx.register_scheme_handler_factory(Some(&scheme), None, Some(&mut factory));
        }
        None => {
            register_scheme_handler_factory(Some(&scheme), None, Some(&mut factory));
        }
    }
}

#[derive(Default)]
struct Reply {
    status: i32,
    mime: String,
    body: Vec<u8>,
    offset: usize,
}

wrap_scheme_handler_factory! {
    struct FerriteSchemeFactory;

    impl SchemeHandlerFactory {
        fn create(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            // Runs on the IO thread.
            let (browser, frame, request) = (browser?, frame?, request?);
            if frame.is_main() == 0 {
                return None;
            }
            let url = from_userfree(&request.url());
            let parsed = url::Url::parse(&url).ok()?;
            let host = parsed.host_str().unwrap_or("").to_string();
            let path = parsed.path().to_string();
            let frame_url = from_userfree(&frame.url());
            let resource_type = request.resource_type();
            let is_navigation = resource_type == ResourceType::MAIN_FRAME;
            let from_internal = frame_url.starts_with("ferrite:");

            if let Some(command) = path.strip_prefix("/api/") {
                let is_fetch = resource_type == ResourceType::XHR;
                let method = from_userfree(&request.method());
                if !from_internal || !is_fetch || method != "POST" {
                    tracing::warn!("rejected ferrite API request {url} from {frame_url}");
                    return None;
                }
                // Arguments travel in the query (`?a=<json>`): fetch() request
                // bodies reach scheme handlers as data pipes CEF cannot read.
                let mut body = parsed
                    .query_pairs()
                    .find(|(k, _)| k == "a")
                    .map(|(_, v)| v.into_owned().into_bytes())
                    .unwrap_or_default();
                if body.is_empty() {
                    body = read_post_body(request);
                }
                return Some(FerriteResource::new(
                    Arc::new(Mutex::new(Reply::default())),
                    Some(ApiCall {
                        browser_id: browser.identifier(),
                        command: command.to_string(),
                        body,
                    }),
                ));
            }

            // Static files: only for an internal page's own document or
            // subresources of internal pages.
            // While a document's first subresources load, CEF may report an
            // empty frame URL and no referrer. Static UI files hold no data,
            // so they are served then too; the API above stays strict.
            let referrer = from_userfree(&request.referrer_url());
            let loading_first_document = frame_url.is_empty();
            if !is_navigation && !from_internal && !referrer.starts_with("ferrite:") && !loading_first_document {
                tracing::warn!("rejected {url} (frame {frame_url:?}, referrer {referrer:?})");
                return None;
            }
            let name = if path == "/" || path.is_empty() {
                assets::page_for_host(&host)?.to_string()
            } else {
                path.trim_start_matches('/').to_string()
            };
            let reply = match assets::get(&name) {
                Some(body) => Reply { status: 200, mime: assets::mime_for(&name).into(), body, offset: 0 },
                None => Reply { status: 404, mime: "text/plain".into(), body: b"not found".to_vec(), offset: 0 },
            };
            Some(FerriteResource::new(Arc::new(Mutex::new(reply)), None))
        }
    }
}

#[derive(Clone)]
struct ApiCall {
    browser_id: i32,
    command: String,
    body: Vec<u8>,
}

fn read_post_body(request: &mut Request) -> Vec<u8> {
    let mut out = Vec::new();
    let Some(post) = request.post_data() else {
        return out;
    };
    let mut elements = Vec::new();
    post.elements(Some(&mut elements));
    for element in elements.into_iter().flatten() {
        let n = element.bytes_count();
        if n == 0 {
            continue;
        }
        let mut buf = vec![0u8; n];
        let got = element.bytes(n, buf.as_mut_ptr());
        buf.truncate(got);
        out.extend_from_slice(&buf);
    }
    out
}

wrap_resource_handler! {
    struct FerriteResource {
        reply: Arc<Mutex<Reply>>,
        call: Option<ApiCall>,
    }

    impl ResourceHandler {
        fn open(
            &self,
            _request: Option<&mut Request>,
            handle_request: Option<&mut ::std::os::raw::c_int>,
            callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            let Some(handle_request) = handle_request else { return 0 };
            let Some(call) = self.call.clone() else {
                *handle_request = 1;
                return 1;
            };
            // API call: answer from the UI thread, then continue.
            *handle_request = 0;
            let Some(callback) = callback.cloned() else { return 0 };
            let reply = self.reply.clone();
            let callback = SendBox::new(callback);
            engine::post_ui(move || {
                let callback = callback.into_inner();
                let args: serde_json::Value = serde_json::from_slice(&call.body).unwrap_or(serde_json::Value::Null);
                let result = with_state(|s| api::handle(s, call.browser_id, &call.command, args))
                    .unwrap_or_else(|| Err(anyhow::anyhow!("browser busy")));
                let (status, value) = match result {
                    Ok(value) => (200, value),
                    Err(err) => (400, serde_json::json!({ "error": format!("{err:#}") })),
                };
                if let Ok(mut r) = reply.lock() {
                    r.status = status;
                    r.mime = "application/json".into();
                    r.body = serde_json::to_vec(&value).unwrap_or_default();
                }
                callback.cont();
            });
            1
        }

        fn response_headers(
            &self,
            response: Option<&mut Response>,
            response_length: Option<&mut i64>,
            _redirect_url: Option<&mut CefString>,
        ) {
            let Ok(r) = self.reply.lock() else { return };
            if let Some(response) = response {
                response.set_status(r.status);
                response.set_mime_type(Some(&cef_str(&r.mime)));
                response.set_header_by_name(Some(&cef_str("Cache-Control")), Some(&cef_str("no-store")), 1);
                response.set_header_by_name(Some(&cef_str("X-Content-Type-Options")), Some(&cef_str("nosniff")), 1);
                if r.mime == "text/html" {
                    // Internal pages run only our own scripts.
                    response.set_header_by_name(
                        Some(&cef_str("Content-Security-Policy")),
                        Some(&cef_str("default-src 'self' ferrite://app; img-src * data:; style-src 'self' ferrite://app 'unsafe-inline'; frame-ancestors 'none'")),
                        1,
                    );
                }
            }
            if let Some(len) = response_length {
                *len = r.body.len() as i64;
            }
        }

        fn read(
            &self,
            data_out: *mut u8,
            bytes_to_read: ::std::os::raw::c_int,
            bytes_read: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut ResourceReadCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(bytes_read) = bytes_read else { return 0 };
            let Ok(mut r) = self.reply.lock() else { return 0 };
            let remaining = r.body.len().saturating_sub(r.offset);
            let n = remaining.min(bytes_to_read.max(0) as usize);
            if n == 0 {
                *bytes_read = 0;
                return 0;
            }
            // SAFETY: CEF guarantees data_out has room for bytes_to_read bytes.
            unsafe {
                std::ptr::copy_nonoverlapping(r.body.as_ptr().add(r.offset), data_out, n);
            }
            r.offset += n;
            *bytes_read = n as i32;
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_pages_cannot_open_internal_pages() {
        assert!(!navigation_allowed(
            "https://evil.example/",
            "ferrite://settings/",
            true
        ));
        assert!(navigation_allowed(
            "ferrite://newtab/",
            "ferrite://settings/",
            true
        ));
        assert!(navigation_allowed("", "ferrite://newtab/", true));
        assert!(!navigation_allowed(
            "ferrite://newtab/",
            "ferrite://settings/",
            false
        ));
        allow_internal("ferrite://history");
        assert!(navigation_allowed(
            "https://a.example/",
            "ferrite://history/",
            true
        ));
        // Single use.
        assert!(!navigation_allowed(
            "https://a.example/",
            "ferrite://history/",
            true
        ));
        assert!(navigation_allowed(
            "https://a.example/",
            "https://b.example/",
            true
        ));
    }
}
