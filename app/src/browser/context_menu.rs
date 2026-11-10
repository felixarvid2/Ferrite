//! Context menu for web content. Alloy-style browsers only get CEF's basic
//! menu (back/forward/reload/print/view source, editing commands), so the
//! everyday entries are added here.

use super::{later, tabs, window};
use crate::engine::{cef_str, from_userfree};
use cef::*;
use std::cell::RefCell;

const FIRST: i32 = 26500; // MENU_ID_USER_FIRST
const OPEN_LINK_TAB: i32 = FIRST;
const OPEN_LINK_WINDOW: i32 = FIRST + 1;
const OPEN_LINK_PRIVATE: i32 = FIRST + 2;
const COPY_LINK: i32 = FIRST + 3;
const OPEN_IMAGE_TAB: i32 = FIRST + 4;
const SAVE_IMAGE: i32 = FIRST + 5;
const COPY_IMAGE_URL: i32 = FIRST + 6;
const SEARCH_SELECTION: i32 = FIRST + 7;
const SAVE_LINK: i32 = FIRST + 8;
const INSPECT: i32 = FIRST + 9;

thread_local! {
    // The X11/Wayland clipboard is owned by the process that set it; keep
    // the handle alive so pasting into other apps keeps working.
    static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
}

pub fn copy_text(text: &str) {
    CLIPBOARD.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = arboard::Clipboard::new().ok();
        }
        match c.as_mut().map(|c| c.set_text(text.to_string())) {
            Some(Ok(())) => {}
            other => tracing::warn!("clipboard write failed: {:?}", other.map(|r| r.err())),
        }
    });
}

pub fn make_handler() -> ContextMenuHandler {
    ContentContextMenu::new()
}

/// Links and images from a page may only open as ordinary web URLs (never
/// ferrite://, javascript:, data: or other schemes).
fn is_web_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https" | "ftp"))
}

fn short(text: &str) -> String {
    let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > 32 {
        format!("{}…", t.chars().take(32).collect::<String>())
    } else {
        t
    }
}

wrap_context_menu_handler! {
    struct ContentContextMenu;

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            let (Some(params), Some(model)) = (params, model) else { return };
            let link = from_userfree(&params.link_url());
            let selection = from_userfree(&params.selection_text());
            let is_image = params.media_type() == ContextMenuMediaType::IMAGE;
            let mut index = 0;
            let mut insert = |id: i32, label: &str| {
                model.insert_item_at(index, id, Some(&cef_str(label)));
                index += 1;
            };
            if is_web_url(&link) {
                insert(OPEN_LINK_TAB, "Open link in new tab");
                insert(OPEN_LINK_WINDOW, "Open link in new window");
                insert(OPEN_LINK_PRIVATE, "Open link in private window");
                insert(SAVE_LINK, "Save link as…");
                insert(COPY_LINK, "Copy link address");
            }
            if is_image && is_web_url(&from_userfree(&params.source_url())) {
                insert(OPEN_IMAGE_TAB, "Open image in new tab");
                insert(SAVE_IMAGE, "Save image as…");
                insert(COPY_IMAGE_URL, "Copy image address");
            }
            if !selection.trim().is_empty() && params.is_editable() == 0 {
                insert(SEARCH_SELECTION, &format!("Search the web for “{}”", short(&selection)));
            }
            if index > 0 && model.count() > index {
                model.insert_separator_at(index);
            }
            model.add_separator();
            model.add_item(INSPECT, Some(&cef_str("Inspect")));
        }

        fn on_context_menu_command(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            command_id: ::std::os::raw::c_int,
            _event_flags: EventFlags,
        ) -> ::std::os::raw::c_int {
            let (Some(browser), Some(params)) = (browser, params) else { return 0 };
            let browser_id = browser.identifier();
            let link = from_userfree(&params.link_url());
            let src = from_userfree(&params.source_url());
            let selection = from_userfree(&params.selection_text());
            let (x, y) = (params.xcoord(), params.ycoord());
            let needs_web = matches!(command_id, OPEN_LINK_TAB | OPEN_LINK_WINDOW | OPEN_LINK_PRIVATE | COPY_LINK | SAVE_LINK | OPEN_IMAGE_TAB | SAVE_IMAGE | COPY_IMAGE_URL);
            let target = if matches!(command_id, OPEN_IMAGE_TAB | SAVE_IMAGE | COPY_IMAGE_URL) { &src } else { &link };
            if needs_web && !is_web_url(target) {
                return 1;
            }
            match command_id {
                OPEN_LINK_TAB | OPEN_IMAGE_TAB => {
                    let url = if command_id == OPEN_LINK_TAB { link } else { src };
                    later(move |s| {
                        let Some(opener) = s.tab_of_browser(browser_id) else { return };
                        let Some(win) = s.tabs.get(&opener).map(|t| t.win) else { return };
                        tabs::new_tab(s, win, &url, tabs::NewTab { background: true, opener: Some(opener), ..Default::default() });
                    });
                }
                OPEN_LINK_WINDOW | OPEN_LINK_PRIVATE => {
                    let private_req = command_id == OPEN_LINK_PRIVATE;
                    later(move |s| {
                        let private = private_req
                            || s.win_of_browser(browser_id).is_some_and(|w| s.is_private(w));
                        window::open_window(s, window::WindowInit { private, ..window::WindowInit::normal(vec![link]) });
                    });
                }
                COPY_LINK => copy_text(&link),
                COPY_IMAGE_URL => copy_text(&src),
                SAVE_LINK | SAVE_IMAGE => {
                    let url = if command_id == SAVE_LINK { link } else { src };
                    if let Some(host) = browser.host() {
                        host.start_download(Some(&cef_str(&url)));
                    }
                }
                SEARCH_SELECTION => {
                    later(move |s| {
                        let Some(opener) = s.tab_of_browser(browser_id) else { return };
                        let Some(win) = s.tabs.get(&opener).map(|t| t.win) else { return };
                        let url = s.resolve_input(&format!("?{}", selection.trim()));
                        tabs::new_tab(s, win, &url, tabs::NewTab { opener: Some(opener), ..Default::default() });
                    });
                }
                INSPECT => {
                    if let Some(host) = browser.host() {
                        let point = Point { x, y };
                        host.show_dev_tools(Some(&WindowInfo::default()), None, None, Some(&point));
                    }
                }
                _ => return 0,
            }
            1
        }
    }
}
