//! CEF client handlers. Each callback extracts what it needs and posts the
//! work to the browser state with [`later`]; only decisions CEF needs
//! synchronously are made inline.

use super::{commands, downloads, later, permissions, scheme, tabs, window, BrowserRef, State};
use crate::engine::{from_userfree, opt_str};
use cef::*;
use ferrite_core::commands::{is_escape, match_shortcut};
use std::sync::atomic::{AtomicBool, Ordering};

/// Send `DNT: 1` and `Sec-GPC: 1` with every request (privacy setting).
pub static SEND_DNT: AtomicBool = AtomicBool::new(false);
/// Ad/tracker blocking switch and the loaded filter engine (IO thread reads).
pub static ADBLOCK: AtomicBool = AtomicBool::new(false);
pub static BLOCKER: std::sync::RwLock<Option<std::sync::Arc<ferrite_core::blocker::Blocker>>> =
    std::sync::RwLock::new(None);

/// Sites where ad blocking is off (see `Settings::adblock_exceptions`).
/// Replaced as a whole whenever the setting changes; the IO thread clones the Arc.
static EXCEPTIONS: std::sync::RwLock<Option<std::sync::Arc<Vec<String>>>> =
    std::sync::RwLock::new(None);
/// Requests cancelled by the blocker since the last main-frame navigation, per browser.
static BLOCKED: std::sync::Mutex<Option<std::collections::HashMap<i32, u32>>> =
    std::sync::Mutex::new(None);
/// A delayed UI refresh for the blocked-count badge is already queued.
static BADGE_PENDING: AtomicBool = AtomicBool::new(false);

pub fn set_exceptions(list: Vec<String>) {
    if let Ok(mut slot) = EXCEPTIONS.write() {
        *slot = Some(std::sync::Arc::new(list));
    }
}

/// Is blocking switched off for the site of `url`? (any thread)
pub fn is_excepted(url: &str) -> bool {
    let list = EXCEPTIONS.read().ok().and_then(|l| l.clone());
    list.is_some_and(|l| ferrite_core::blocker::is_exception(&l, url))
}

/// Requests blocked on the browser's current page. Never blocks: if the
/// lock is busy (IO thread counting) it reports 0 and the next refresh fixes it.
pub fn blocked_count(browser_id: i32) -> u32 {
    BLOCKED
        .try_lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|m| m.get(&browser_id).copied()))
        .unwrap_or(0)
}

fn reset_blocked(browser_id: i32) {
    let had = BLOCKED.lock().ok().is_some_and(|mut g| {
        g.as_mut()
            .and_then(|m| m.remove(&browser_id))
            .is_some_and(|n| n > 0)
    });
    if had {
        later(move |s| {
            if let Some(win) = s.win_of_browser(browser_id) {
                s.mark_dirty(win);
            }
        });
    }
}

/// IO thread: one more request cancelled. Refreshes the UI at most every 250 ms.
fn count_blocked(browser_id: i32) {
    if let Ok(mut g) = BLOCKED.lock() {
        let n = g
            .get_or_insert_with(Default::default)
            .entry(browser_id)
            .or_insert(0);
        *n = n.saturating_add(1);
    }
    if !BADGE_PENDING.swap(true, Ordering::Relaxed) {
        crate::engine::post_ui_delayed(250, move || {
            BADGE_PENDING.store(false, Ordering::Relaxed);
            later(|s| s.mark_all_dirty());
        });
    }
}

/// The exception list changed: publish it and drop the injected scripts of
/// every tab (they are registered again, or not, on the next navigation).
pub fn apply_exceptions(s: &State) {
    set_exceptions(s.settings.adblock_exceptions.clone());
    let browsers: std::collections::HashMap<i32, Browser> = s
        .tabs
        .values()
        .filter_map(|t| t.browser.clone())
        .map(|b| (b.identifier(), b))
        .collect();
    super::inject::clear_all(|id| browsers.get(&id).cloned());
}

/// uBlock Origin's scriptlets (GPL-3.0 data file, see resources/adblock/):
/// next to the executable in a bundle, else in the source tree (dev builds).
pub fn scriptlets_path() -> Option<std::path::PathBuf> {
    let name = "adblock/ubo-scriptlets.json";
    let bundled = std::env::current_exe().ok()?.parent()?.join(name);
    if bundled.is_file() {
        return Some(bundled);
    }
    let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../resources")
        .join(name);
    dev.is_file().then_some(dev)
}

/// Load cached filter lists, then refresh stale ones, off the UI thread.
pub fn start_blocker(lists_dir: std::path::PathBuf) {
    std::thread::spawn(move || {
        use ferrite_core::blocker::{update_lists, Blocker};
        let install = |b: Option<Blocker>| {
            if let (Some(b), Ok(mut slot)) = (b, BLOCKER.write()) {
                tracing::info!("content blocker ready ({} lists)", b.rule_lists);
                *slot = Some(std::sync::Arc::new(b));
            }
        };
        let resources = scriptlets_path();
        install(Blocker::load_dir(&lists_dir, resources.as_deref()));
        match update_lists(&lists_dir) {
            Ok(0) => {}
            Ok(n) => {
                tracing::info!("updated {n} filter lists");
                install(Blocker::load_dir(&lists_dir, resources.as_deref()));
            }
            Err(err) => tracing::warn!("filter list update failed: {err:#}"),
        }
    });
}

fn adblock_type(t: ResourceType) -> &'static str {
    match t {
        t if t == ResourceType::MAIN_FRAME => "document",
        t if t == ResourceType::SUB_FRAME => "sub_frame",
        t if t == ResourceType::STYLESHEET => "stylesheet",
        t if t == ResourceType::SCRIPT
            || t == ResourceType::WORKER
            || t == ResourceType::SHARED_WORKER
            || t == ResourceType::SERVICE_WORKER =>
        {
            "script"
        }
        t if t == ResourceType::IMAGE || t == ResourceType::FAVICON => "image",
        t if t == ResourceType::FONT_RESOURCE => "font",
        t if t == ResourceType::MEDIA => "media",
        t if t == ResourceType::XHR => "xmlhttprequest",
        t if t == ResourceType::PING => "ping",
        t if t == ResourceType::OBJECT || t == ResourceType::PLUGIN_RESOURCE => "object",
        _ => "other",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    /// Web content in tabs (and popups/DevTools created from them).
    Content,
    /// The privileged browser UI strip.
    Ui,
}

pub fn make_client(kind: ClientKind) -> Client {
    FerriteClient::new(kind)
}

fn id_of(browser: &Option<&mut Browser>) -> i32 {
    browser.as_ref().map(|b| b.identifier()).unwrap_or(-1)
}

/// Run `f` for the tab showing `browser_id` (if it is one of our tabs).
fn with_tab(browser_id: i32, f: impl FnOnce(&mut State, super::TabId) + 'static) {
    later(move |s| {
        if let Some(tab) = s.tab_of_browser(browser_id) {
            f(s, tab);
        }
    });
}

fn tab_changed(s: &mut State, tab: super::TabId) {
    if let Some(win) = s.tabs.get(&tab).map(|t| t.win) {
        s.mark_dirty(win);
        if s.active_tab(win) == Some(tab) {
            window::update_title(s, win);
        }
    }
}

wrap_client! {
    pub struct FerriteClient {
        kind: ClientKind,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(FerriteDisplay::new(self.kind))
        }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(FerriteLifeSpan::new(self.kind))
        }
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(FerriteLoad::new(self.kind))
        }
        fn request_handler(&self) -> Option<RequestHandler> {
            Some(FerriteRequest::new(self.kind))
        }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(FerriteKeyboard::new(self.kind))
        }
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            match self.kind {
                ClientKind::Ui => Some(UiContextMenu::new()),
                ClientKind::Content => Some(super::context_menu::make_handler()),
            }
        }
        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(downloads::make_handler())
        }
        fn find_handler(&self) -> Option<FindHandler> {
            Some(FerriteFind::new())
        }
        fn drag_handler(&self) -> Option<DragHandler> {
            (self.kind == ClientKind::Ui).then(UiDrag::new)
        }
        fn permission_handler(&self) -> Option<PermissionHandler> {
            Some(permissions::make_handler(self.kind == ClientKind::Ui))
        }
    }
}

wrap_display_handler! {
    struct FerriteDisplay {
        kind: ClientKind,
    }

    impl DisplayHandler {
        fn on_address_change(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            if self.kind != ClientKind::Content || !frame.is_some_and(|f| f.is_main() != 0) {
                return;
            }
            let url = opt_str(url);
            if url == "about:blank" && tabs::is_deferred(id_of(&browser)) {
                return; // placeholder before the real first page, keep the tab's URL
            }
            if let Some(mut b) = browser.as_deref().cloned() {
                super::inject::committed(&mut b, &url);
            }
            with_tab(id_of(&browser), move |s, tab| {
                if let Some(t) = s.tabs.get_mut(&tab) {
                    if t.url != url {
                        t.favicon = None;
                        t.cert_error = false;
                        t.secure = None;
                    }
                    t.url = url;
                }
                tab_changed(s, tab);
                s.schedule_session_save();
            });
        }

        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            if self.kind != ClientKind::Content {
                return;
            }
            let title = opt_str(title);
            if tabs::is_deferred(id_of(&browser)) {
                return;
            }
            with_tab(id_of(&browser), move |s, tab| {
                let Some(t) = s.tabs.get_mut(&tab) else { return };
                t.title = title.clone();
                let (url, win) = (t.url.clone(), t.win);
                tab_changed(s, tab);
                if !title.is_empty() {
                    let _ = s.db_for(win).update_title(&url, &title);
                }
            });
        }

        fn on_favicon_urlchange(&self, browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            if self.kind != ClientKind::Content {
                return;
            }
            let icon = icon_urls.and_then(|l| crate::engine::read_string_list(l).into_iter().next());
            tracing::trace!("favicon {icon:?}");
            with_tab(id_of(&browser), move |s, tab| {
                if let Some(t) = s.tabs.get_mut(&tab) {
                    t.favicon = icon;
                }
                tab_changed(s, tab);
            });
        }

        fn on_fullscreen_mode_change(&self, browser: Option<&mut Browser>, fullscreen: ::std::os::raw::c_int) {
            if self.kind != ClientKind::Content {
                return;
            }
            let browser_id = id_of(&browser);
            later(move |s| {
                if let Some(win) = s.win_of_browser(browser_id) {
                    window::set_html_fullscreen(s, win, fullscreen != 0);
                }
            });
        }

        fn on_loading_progress_change(&self, browser: Option<&mut Browser>, progress: f64) {
            if self.kind != ClientKind::Content {
                return;
            }
            with_tab(id_of(&browser), move |s, tab| {
                if let Some(t) = s.tabs.get_mut(&tab) {
                    t.progress = progress;
                }
                tab_changed(s, tab);
            });
        }

        fn on_media_access_change(&self, browser: Option<&mut Browser>, has_video_access: ::std::os::raw::c_int, has_audio_access: ::std::os::raw::c_int) {
            with_tab(id_of(&browser), move |s, tab| {
                if let Some(t) = s.tabs.get_mut(&tab) {
                    t.media_video = has_video_access != 0;
                    t.media_audio = has_audio_access != 0;
                }
                tab_changed(s, tab);
            });
        }

        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            if self.kind == ClientKind::Ui {
                tracing::debug!(target: "ferrite::ui", "{} ({}:{line})", opt_str(message), opt_str(source));
            }
            0
        }
    }
}

wrap_life_span_handler! {
    struct FerriteLifeSpan {
        kind: ClientKind,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // The UI never opens popups; pages may not pop up internal pages.
            if self.kind == ClientKind::Ui || scheme::is_internal(&opt_str(target_url)) {
                return 1;
            }
            tabs::PENDING_POPUP_DISPOSITION.with(|p| p.set(Some(target_disposition)));
            let size = popup_features
                .filter(|f| f.width_set != 0 && f.height_set != 0)
                .map(|f| (f.width, f.height));
            tabs::PENDING_POPUP_SIZE.with(|p| p.set(size));
            0
        }

        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            later(|s| s.live_browsers += 1);
        }

        fn do_close(&self, browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            // Closing a tab must not close the window: take the BrowserView
            // out of the hierarchy, which completes the browser close.
            let browser_id = id_of(&browser);
            later(move |s| {
                if let Some(tab) = s.tab_of_browser(browser_id) {
                    tabs::detach_view(s, tab);
                }
            });
            1
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            let browser_id = id_of(&browser);
            super::inject::closed(browser_id);
            super::mcp::browser_gone(browser_id);
            if let Ok(mut g) = BLOCKED.lock() {
                if let Some(m) = g.as_mut() {
                    m.remove(&browser_id);
                }
            }
            later(move |s| {
                if let Some(BrowserRef::Tab(tab)) = s.browsers.remove(&browser_id) {
                    if s.tabs.get(&tab).is_some_and(|t| t.discarding) {
                        tabs::tab_slept(s, tab);
                    } else {
                        tabs::tab_closed(s, tab);
                    }
                }
                s.browser_closed();
            });
        }
    }
}

wrap_load_handler! {
    struct FerriteLoad {
        kind: ClientKind,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            if self.kind != ClientKind::Content {
                return;
            }
            with_tab(id_of(&browser), move |s, tab| {
                if let Some(t) = s.tabs.get_mut(&tab) {
                    if is_loading != 0 && !t.loading {
                        t.load_seq += 1;
                    }
                    t.loading = is_loading != 0;
                    t.can_back = can_go_back != 0;
                    t.can_forward = can_go_forward != 0;
                    if is_loading == 0 {
                        t.progress = 1.0;
                    }
                }
                tab_changed(s, tab);
            });
        }

        fn on_load_end(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, http_status_code: ::std::os::raw::c_int) {
            let Some(frame) = frame else { return };
            if frame.is_main() == 0 {
                return;
            }
            if self.kind == ClientKind::Content {
                if let Some(mut b) = browser.as_deref().cloned() {
                    tabs::deferred_start(&mut b);
                }
            }
            if self.kind == ClientKind::Ui {
                let browser_id = id_of(&browser);
                later(move |s| {
                    if let Some(win) = s.win_of_browser(browser_id) {
                        ui_loaded(s, win);
                    }
                });
                return;
            }
            let url = from_userfree(&frame.url());
            with_tab(id_of(&browser), move |s, tab| {
                let Some(t) = s.tabs.get(&tab) else { return };
                let (win, title) = (t.win, t.title.clone());
                if http_status_code < 400 && !s.is_ephemeral(win) {
                    if let Err(err) = s.db.record_visit(&url, &title, ferrite_core::now_ms()) {
                        tracing::warn!("recording history failed: {err:#}");
                    }
                }
                let secure = s
                    .tabs
                    .get(&tab)
                    .and_then(|t| t.host())
                    .and_then(|h| h.visible_navigation_entry())
                    .and_then(|e| e.sslstatus())
                    .map(|ssl| ssl.is_secure_connection() != 0);
                if let Some(t) = s.tabs.get_mut(&tab) {
                    t.secure = if t.url.starts_with("https:") { secure } else { None };
                }
                if let Some(zoom) = s.tabs.get(&tab).and_then(|t| t.host()).map(|h| h.zoom_level()) {
                    if let Some(t) = s.tabs.get_mut(&tab) {
                        t.zoom_percent = commands::zoom_percent(zoom);
                    }
                }
                tab_changed(s, tab);
            });
        }
    }
}

/// The UI page finished loading: push state, run deferred focus requests.
fn ui_loaded(s: &mut State, win: super::WinId) {
    let Some(w) = s.windows.get_mut(&win) else {
        return;
    };
    w.ui_ready = true;
    let focus = std::mem::take(&mut w.pending_focus_omnibox);
    s.mark_dirty(win);
    if focus {
        tabs::focus_omnibox(s, win);
    }
}

wrap_request_handler! {
    struct FerriteRequest {
        kind: ClientKind,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: ::std::os::raw::c_int,
            _is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let (Some(frame), Some(request)) = (frame, request) else { return 1 };
            let target = from_userfree(&request.url());
            let current = from_userfree(&frame.url());
            let is_main = frame.is_main() != 0;
            if self.kind == ClientKind::Ui {
                // The UI strip only ever shows its own page.
                let ok = target.starts_with(window::UI_URL);
                if !ok {
                    tracing::warn!("blocked UI navigation to {target}");
                }
                return (!ok) as i32;
            }
            if !scheme::navigation_allowed(&current, &target, is_main) {
                tracing::warn!("blocked navigation from {current:?} to internal page {target}");
                return 1;
            }
            if is_main && !super::mcp::navigation_ok(id_of(&browser), &target) {
                tracing::warn!("blocked navigation of an AI-controlled tab to {target}");
                return 1;
            }
            if let (true, Some(browser)) = (is_main, browser) {
                reset_blocked(browser.identifier());
                if target.starts_with("http") {
                    super::inject::capture_ensure(browser); // backstop: normally done at creation
                }
                super::inject::navigation(browser, &target, super::inject::blocking_script);
            }
            0
        }

        fn on_open_urlfrom_tab(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            // Middle/ctrl-click on links.
            let url = opt_str(target_url);
            if self.kind == ClientKind::Ui || scheme::is_internal(&url) {
                return 1;
            }
            let background = if target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB {
                true
            } else if target_disposition == WindowOpenDisposition::NEW_FOREGROUND_TAB {
                false
            } else if target_disposition == WindowOpenDisposition::NEW_WINDOW {
                let browser_id = id_of(&browser);
                later(move |s| {
                    let private = s.win_of_browser(browser_id).is_some_and(|w| s.is_private(w));
                    let owned = s.tab_of_browser(browser_id).is_some_and(|t| s.ai_control(t) == ferrite_mcp::policy::Control::Own);
                    let ai = owned && s.win_of_browser(browser_id).is_some_and(|w| s.windows.get(&w).is_some_and(|w| w.ai));
                    let win = window::open_window(s, window::WindowInit { private, ai, ..window::WindowInit::normal(vec![url]) });
                    if let (true, Some(win)) = (ai, win) {
                        super::mcp::own_all_tabs(s, win);
                    }
                });
                return 1;
            } else {
                return 0;
            };
            with_tab(id_of(&browser), move |s, opener| {
                let Some(win) = s.tabs.get(&opener).map(|t| t.win) else { return };
                let ai = s.ai_control(opener) == ferrite_mcp::policy::Control::Own;
                tabs::new_tab(s, win, &url, tabs::NewTab { background, opener: Some(opener), ai, ..Default::default() });
            });
            1
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            // Only installed when needed, so normal loads pay nothing.
            (self.kind == ClientKind::Content
                && (SEND_DNT.load(Ordering::Relaxed) || ADBLOCK.load(Ordering::Relaxed)))
                .then(PrivacyHeaders::new)
        }

        fn on_certificate_error(
            &self,
            browser: Option<&mut Browser>,
            _cert_error: Errorcode,
            request_url: Option<&CefString>,
            _ssl_info: Option<&mut Sslinfo>,
            _callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            // Never bypass certificate errors; Chromium shows its error page.
            tracing::warn!("certificate error for {}", opt_str(request_url));
            with_tab(id_of(&browser), |s, tab| {
                if let Some(t) = s.tabs.get_mut(&tab) {
                    t.cert_error = true;
                }
                tab_changed(s, tab);
            });
            0
        }

        fn on_render_process_terminated(
            &self,
            browser: Option<&mut Browser>,
            status: TerminationStatus,
            error_code: ::std::os::raw::c_int,
            _error_string: Option<&CefString>,
        ) {
            tracing::warn!("render process terminated: {status:?} ({error_code})");
            let browser_id = id_of(&browser);
            later(move |s| match s.browsers.get(&browser_id).copied() {
                Some(BrowserRef::Tab(tab)) => {
                    if let Some(t) = s.tabs.get_mut(&tab) {
                        t.crashed = true;
                        t.loading = false;
                    }
                    tab_changed(s, tab);
                }
                Some(BrowserRef::Ui(win)) => {
                    // Reload the UI strip; it is stateless.
                    if let Some(frame) = s.windows.get(&win).and_then(|w| w.ui_frame()) {
                        frame.load_url(Some(&crate::engine::cef_str(window::UI_URL)));
                    }
                }
                None => {}
            });
        }
    }
}

wrap_keyboard_handler! {
    struct FerriteKeyboard {
        kind: ClientKind,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: Option<&mut sys::XEvent>,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let Some(event) = event else { return 0 };
            if event.type_ != KeyEventType::RAWKEYDOWN {
                return 0;
            }
            let browser_id = id_of(&browser);
            tracing::trace!("key code={} mods={:#x}", event.windows_key_code, event.modifiers);
            if is_escape(event.windows_key_code, event.modifiers) {
                // Close the find bar if it is open; otherwise the page gets Esc.
                later(move |s| {
                    if let Some(win) = s.win_of_browser(browser_id) {
                        commands::close_find_if_open(s, win);
                    }
                });
                return 0;
            }
            let Some(command) = match_shortcut(event.windows_key_code, event.modifiers) else {
                return 0;
            };
            let _ = self.kind;
            later(move |s| {
                if let Some(win) = s.win_of_browser(browser_id) {
                    commands::exec(s, win, command);
                }
            });
            1
        }
    }
}

wrap_context_menu_handler! {
    struct UiContextMenu;

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            // The UI strip draws its own menus; keep only text editing menus
            // (address bar, find field).
            let editable = params.is_some_and(|p| p.is_editable() != 0);
            if let (false, Some(model)) = (editable, model) {
                model.clear();
            }
        }
    }
}

wrap_find_handler! {
    struct FerriteFind;

    impl FindHandler {
        fn on_find_result(
            &self,
            browser: Option<&mut Browser>,
            _identifier: ::std::os::raw::c_int,
            count: ::std::os::raw::c_int,
            _selection_rect: Option<&Rect>,
            active_match_ordinal: ::std::os::raw::c_int,
            _final_update: ::std::os::raw::c_int,
        ) {
            with_tab(id_of(&browser), move |s, tab| {
                if let Some(t) = s.tabs.get_mut(&tab) {
                    t.find.count = count;
                    t.find.active = active_match_ordinal;
                }
                tab_changed(s, tab);
            });
        }
    }
}

wrap_resource_request_handler! {
    struct PrivacyHeaders;

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let Some(request) = request else { return ReturnValue::CONTINUE };
            // Wait until the content-blocking script for this page is
            // registered (it must run before the page's own scripts).
            if request.resource_type() == ResourceType::MAIN_FRAME {
                if let (Some(b), Some(cb)) = (browser.as_ref(), callback) {
                    if super::inject::hold(b.identifier(), cb) {
                        apply_privacy_headers(request);
                        return ReturnValue::CONTINUE_ASYNC;
                    }
                }
            }
            if ADBLOCK.load(Ordering::Relaxed) {
                let kind = adblock_type(request.resource_type());
                if kind != "document" {
                    let url = from_userfree(&request.url());
                    let browser_id = browser.as_ref().map(|b| b.identifier());
                    // First party = the tab's top-level page.
                    let source = browser
                        .and_then(|b| b.main_frame())
                        .map(|f| from_userfree(&f.url()))
                        .unwrap_or_default();
                    let blocker = BLOCKER.read().ok().and_then(|b| b.clone());
                    if let Some(blocker) = blocker {
                        if url.starts_with("http")
                            && !is_excepted(&source)
                            && blocker.should_block(&url, &source, kind)
                        {
                            tracing::trace!("blocked {kind} {url}");
                            if let Some(id) = browser_id {
                                count_blocked(id);
                            }
                            return ReturnValue::CANCEL;
                        }
                    }
                }
            }
            apply_privacy_headers(request);
            ReturnValue::CONTINUE
        }
    }
}

fn apply_privacy_headers(request: &mut Request) {
    if SEND_DNT.load(Ordering::Relaxed) {
        for (name, value) in [("DNT", "1"), ("Sec-GPC", "1")] {
            request.set_header_by_name(
                Some(&crate::engine::cef_str(name)),
                Some(&crate::engine::cef_str(value)),
                1,
            );
        }
    }
}

wrap_drag_handler! {
    struct UiDrag;

    impl DragHandler {
        fn on_draggable_regions_changed(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            regions: Option<&[DraggableRegion]>,
        ) {
            // The UI strip sits at the window's origin, so its coordinates
            // are window coordinates.
            let regions: Vec<DraggableRegion> = regions.map(|r| r.to_vec()).unwrap_or_default();
            tracing::debug!("draggable regions: {}", regions.len());
            let browser_id = id_of(&browser);
            later(move |s| {
                // Only the top strip defines the window's drag regions.
                let from_side = s.windows.values().any(|w| w.side_browser.as_ref().is_some_and(|b| b.identifier() == browser_id));
                if from_side {
                    return;
                }
                if let Some(window) = s.win_of_browser(browser_id).and_then(|w| s.windows.get(&w)).and_then(|w| w.window.clone()) {
                    window.set_draggable_regions(Some(&regions));
                }
            });
        }
    }
}
