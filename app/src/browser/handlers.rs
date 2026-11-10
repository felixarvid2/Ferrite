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
                if http_status_code < 400 && !s.is_private(win) {
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
            _browser: Option<&mut Browser>,
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
                    window::open_window(s, window::WindowInit { private, ..window::WindowInit::normal(vec![url]) });
                });
                return 1;
            } else {
                return 0;
            };
            with_tab(id_of(&browser), move |s, opener| {
                let Some(win) = s.tabs.get(&opener).map(|t| t.win) else { return };
                tabs::new_tab(s, win, &url, tabs::NewTab { background, opener: Some(opener), ..Default::default() });
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
            (self.kind == ClientKind::Content && SEND_DNT.load(Ordering::Relaxed)).then(PrivacyHeaders::new)
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
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            if let Some(request) = request {
                for (name, value) in [("DNT", "1"), ("Sec-GPC", "1")] {
                    request.set_header_by_name(
                        Some(&crate::engine::cef_str(name)),
                        Some(&crate::engine::cef_str(value)),
                        1,
                    );
                }
            }
            ReturnValue::CONTINUE
        }
    }
}
