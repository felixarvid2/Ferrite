//! Integration with the Chromium Embedded Framework: process setup, custom
//! scheme registration and thread helpers. Browser/tab logic lives in
//! `crate::browser`.

pub mod app;
pub mod task;

pub use task::*;

/// The internal scheme used for the browser UI and internal pages.
pub const SCHEME: &str = "ferrite";

pub fn cef_str(s: &str) -> cef::CefString {
    cef::CefString::from(s)
}

pub fn from_userfree(s: &cef::CefStringUserfree) -> String {
    cef::CefString::from(s).to_string()
}

pub fn opt_str(s: Option<&cef::CefString>) -> String {
    s.map(|s| s.to_string()).unwrap_or_default()
}

/// Read a CEF string list that CEF lends to a callback.
///
/// (`CefStringList::clone` in cef-rs drops borrowed lists, so read it through
/// the C API.)
pub fn read_string_list(list: &mut cef::CefStringList) -> Vec<String> {
    let raw: *mut cef::sys::_cef_string_list_t = list.into();
    if raw.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // SAFETY: `raw` is a valid list for the duration of the callback.
    unsafe {
        let count = cef::sys::cef_string_list_size(raw);
        for i in 0..count {
            let mut value: cef::sys::cef_string_t = std::mem::zeroed();
            if cef::sys::cef_string_list_value(raw, i, &mut value) > 0 {
                out.push(cef::CefString::from(std::ptr::from_ref(&value)).to_string());
                cef::sys::cef_string_utf16_clear(&mut value);
            }
        }
    }
    out
}
