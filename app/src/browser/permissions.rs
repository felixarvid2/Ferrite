//! Site permission prompts (camera, microphone, location, notifications, ...).
//! Decisions can be remembered per origin (in memory for private windows).

use super::{later, Prompt, State, WinId};
use crate::engine::opt_str;
use cef::*;
use ferrite_core::permissions::Decision;

pub enum PromptReply {
    Permission(PermissionPromptCallback),
    Media {
        callback: MediaAccessCallback,
        requested: u32,
    },
}

impl PromptReply {
    fn send(&self, allow: bool) {
        match self {
            PromptReply::Permission(cb) => cb.cont(if allow {
                PermissionRequestResult::ACCEPT
            } else {
                PermissionRequestResult::DENY
            }),
            PromptReply::Media {
                callback,
                requested,
            } => callback.cont(if allow { *requested } else { 0 }),
        }
    }
}

const PERMISSION_NAMES: &[(u32, &str)] = &[
    (1 << 1, "camera_pan_tilt_zoom"),
    (1 << 2, "camera"),
    (1 << 4, "clipboard"),
    (1 << 5, "storage_access"),
    (1 << 7, "local_fonts"),
    (1 << 8, "geolocation"),
    (1 << 10, "identity_provider"),
    (1 << 11, "idle_detection"),
    (1 << 12, "microphone"),
    (1 << 13, "midi_sysex"),
    (1 << 14, "multiple_downloads"),
    (1 << 15, "notifications"),
    (1 << 16, "keyboard_lock"),
    (1 << 17, "pointer_lock"),
    (1 << 18, "protected_media"),
    (1 << 19, "protocol_handler"),
    (1 << 20, "storage_access"),
    (1 << 23, "window_management"),
    (1 << 24, "file_system"),
    (1 << 26, "local_network"),
    (1 << 27, "loopback_network"),
    (1 << 28, "sensors"),
];

const MEDIA_NAMES: &[(u32, &str)] = &[
    (1, "microphone"),
    (2, "camera"),
    (4, "screen"),
    (8, "screen"),
];

fn names(bits: u32, table: &[(u32, &str)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (bit, name) in table {
        if bits & bit != 0 && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    }
    if out.is_empty() && bits != 0 {
        out.push(format!("permission_{bits:#x}"));
    }
    out
}

/// Every requested permission already decided? -> Some(allow).
fn stored_decision(s: &State, win: WinId, origin: &str, perms: &[String]) -> Option<bool> {
    let db = s.db_for(win);
    let mut all_allowed = true;
    for p in perms {
        match db.get_permission(origin, p).ok().flatten() {
            Some(Decision::Block) => return Some(false),
            Some(Decision::Allow) => {}
            None => all_allowed = false,
        }
    }
    all_allowed.then_some(true)
}

fn request(
    s: &mut State,
    browser_id: i32,
    id: u64,
    origin: String,
    perms: Vec<String>,
    reply: PromptReply,
) {
    let Some(tab) = s.tab_of_browser(browser_id) else {
        reply.send(false);
        return;
    };
    let Some(win) = s.tabs.get(&tab).map(|t| t.win) else {
        reply.send(false);
        return;
    };
    let origin = ferrite_core::omnibox::origin_of(&origin).unwrap_or(origin);
    if let Some(allow) = stored_decision(s, win, &origin, &perms) {
        reply.send(allow);
        return;
    }
    if let Some(w) = s.windows.get_mut(&win) {
        w.prompts.push(Prompt {
            id,
            tab,
            origin,
            permissions: perms,
            reply,
        });
    }
    s.mark_dirty(win);
}

/// The user answered a prompt in the UI.
pub fn decide(s: &mut State, win: WinId, id: u64, allow: bool, remember: bool) {
    let Some(w) = s.windows.get_mut(&win) else {
        return;
    };
    let Some(pos) = w.prompts.iter().position(|p| p.id == id) else {
        return;
    };
    let prompt = w.prompts.remove(pos);
    if remember {
        let decision = if allow {
            Decision::Allow
        } else {
            Decision::Block
        };
        let db = s.db_for(win);
        for p in &prompt.permissions {
            let _ = db.set_permission(&prompt.origin, p, decision, ferrite_core::now_ms());
        }
    }
    prompt.reply.send(allow);
    s.mark_dirty(win);
}

fn dismissed(s: &mut State, id: u64) {
    for w in s.windows.values_mut() {
        w.prompts.retain(|p| p.id != id);
    }
    s.mark_all_dirty();
}

static MEDIA_PROMPT_IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1 << 62);

pub fn make_handler(ui: bool) -> PermissionHandler {
    FerritePermissions::new(ui)
}

wrap_permission_handler! {
    struct FerritePermissions {
        ui: bool,
    }

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback.cloned() else { return 0 };
            if self.ui {
                callback.cont(0);
                return 1;
            }
            let browser_id = browser.map(|b| b.identifier()).unwrap_or(-1);
            let origin = opt_str(requesting_origin);
            let id = MEDIA_PROMPT_IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let perms = names(requested_permissions, MEDIA_NAMES);
            later(move |s| {
                request(s, browser_id, id, origin, perms, PromptReply::Media { callback, requested: requested_permissions })
            });
            1
        }

        fn on_show_permission_prompt(
            &self,
            browser: Option<&mut Browser>,
            prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback.cloned() else { return 0 };
            if self.ui {
                callback.cont(PermissionRequestResult::DENY);
                return 1;
            }
            let browser_id = browser.map(|b| b.identifier()).unwrap_or(-1);
            let origin = opt_str(requesting_origin);
            let perms = names(requested_permissions, PERMISSION_NAMES);
            later(move |s| request(s, browser_id, prompt_id, origin, perms, PromptReply::Permission(callback)));
            1
        }

        fn on_dismiss_permission_prompt(&self, _browser: Option<&mut Browser>, prompt_id: u64, _result: PermissionRequestResult) {
            later(move |s| dismissed(s, prompt_id));
        }
    }
}
