//! The main ("⋮") menu, drawn natively by CEF Views.

use super::{commands, later, WinId};
use crate::engine::cef_str;
use cef::*;
use ferrite_core::commands::Command;

const ITEMS: &[(i32, &str, Option<Command>)] = &[
    (1, "New tab\tCtrl+T", Some(Command::NewTab)),
    (2, "New window\tCtrl+N", Some(Command::NewWindow)),
    (
        3,
        "New private window\tCtrl+Shift+N",
        Some(Command::NewPrivateWindow),
    ),
    (0, "", None),
    (4, "History\tCtrl+H", Some(Command::History)),
    (5, "Downloads\tCtrl+J", Some(Command::Downloads)),
    (6, "Bookmarks\tCtrl+Shift+O", Some(Command::Bookmarks)),
    (
        7,
        "Show bookmarks bar\tCtrl+Shift+B",
        Some(Command::ToggleBookmarksBar),
    ),
    (0, "", None),
    (8, "Zoom in\tCtrl++", Some(Command::ZoomIn)),
    (9, "Zoom out\tCtrl+-", Some(Command::ZoomOut)),
    (10, "Reset zoom\tCtrl+0", Some(Command::ZoomReset)),
    (11, "Full screen\tF11", Some(Command::Fullscreen)),
    (0, "", None),
    (12, "Find in page…\tCtrl+F", Some(Command::Find)),
    (13, "Print…\tCtrl+P", Some(Command::Print)),
    (14, "View page source\tCtrl+U", Some(Command::ViewSource)),
    (15, "Developer tools\tF12", Some(Command::DevTools)),
    (0, "", None),
    (16, "Settings", Some(Command::Settings)),
    (17, "Quit\tCtrl+Shift+Q", Some(Command::Quit)),
];

/// Menu id of the "Update available" item (opens the release page).
const UPDATE_ITEM: i32 = 18;

pub fn show(s: &super::State, win: WinId, x: i32, y: i32) {
    let Some(w) = s.windows.get(&win) else { return };
    let Some(window) = w.window.clone() else {
        return;
    };
    let update = super::updates::available(s);
    let mut delegate = MainMenuDelegate::new(win, update.as_ref().map(|u| u.url.clone()));
    let Some(model) = menu_model_create(Some(&mut delegate)) else {
        return;
    };
    if let Some(u) = &update {
        model.add_item(
            UPDATE_ITEM,
            Some(&cef_str(&format!("Update available: {}", u.version))),
        );
        model.add_separator();
    }
    for (id, label, _) in ITEMS {
        if *id == 0 {
            model.add_separator();
        } else {
            model.add_item(*id, Some(&cef_str(label)));
        }
    }
    if s.settings.show_bookmarks_bar {
        model.set_label(7, Some(&cef_str("Hide bookmarks bar\tCtrl+Shift+B")));
    }
    // (x, y) is relative to the UI view; menus take screen coordinates.
    let mut point = Point { x, y };
    w.ui_view.convert_point_to_screen(Some(&mut point));
    let mut model = model;
    window.show_menu(Some(&mut model), Some(&point), MenuAnchorPosition::TOPRIGHT);
}

wrap_menu_model_delegate! {
    struct MainMenuDelegate {
        win: WinId,
        update_url: Option<String>,
    }

    impl MenuModelDelegate {
        fn execute_command(&self, _menu_model: Option<&mut MenuModel>, command_id: ::std::os::raw::c_int, _event_flags: EventFlags) {
            if command_id == UPDATE_ITEM {
                if let Some(url) = self.update_url.clone() {
                    let win = self.win;
                    later(move |s| {
                        super::tabs::new_tab(s, win, &url, super::tabs::NewTab::foreground());
                    });
                }
                return;
            }
            let Some(command) = ITEMS.iter().find(|(id, _, _)| *id == command_id).and_then(|(_, _, c)| *c) else {
                return;
            };
            let win = self.win;
            later(move |s| commands::exec(s, win, command));
        }
    }
}

const TAB_ITEMS: &[(i32, &str)] = &[
    (101, "Reload"),
    (102, "Duplicate"),
    (103, "Pin tab"),
    (0, ""),
    (104, "Close tab"),
    (105, "Close other tabs"),
    (106, "Close tabs to the right"),
    (0, ""),
    (107, "Reopen closed tab"),
];

pub fn show_tab_menu(
    s: &super::State,
    win: WinId,
    tab: super::TabId,
    x: i32,
    y: i32,
    from_side: bool,
) {
    let Some(w) = s.windows.get(&win) else { return };
    let Some(window) = w.window.clone() else {
        return;
    };
    let pinned = s.tabs.get(&tab).is_some_and(|t| t.pinned);
    let mut delegate = TabMenuDelegate::new(win, tab);
    let Some(mut model) = menu_model_create(Some(&mut delegate)) else {
        return;
    };
    for (id, label) in TAB_ITEMS {
        if *id == 0 {
            model.add_separator();
        } else {
            let label = if *id == 103 && pinned {
                "Unpin tab"
            } else {
                label
            };
            model.add_item(*id, Some(&cef_str(label)));
        }
    }
    model.set_enabled(107, (!w.closed_tabs.is_empty()) as i32);
    let mut point = Point { x, y };
    match (from_side, &w.side_view) {
        (true, Some(side)) => side.convert_point_to_screen(Some(&mut point)),
        _ => w.ui_view.convert_point_to_screen(Some(&mut point)),
    };
    window.show_menu(Some(&mut model), Some(&point), MenuAnchorPosition::TOPLEFT);
}

fn tab_menu_command(s: &mut super::State, win: WinId, tab: super::TabId, id: i32) {
    use super::tabs;
    match id {
        101 => {
            if let Some(b) = s.tabs.get(&tab).and_then(|t| t.browser.clone()) {
                b.reload();
            }
        }
        102 => tabs::duplicate(s, tab),
        103 => {
            let pinned = s.tabs.get(&tab).is_some_and(|t| t.pinned);
            tabs::set_pinned(s, tab, !pinned);
        }
        104 => tabs::close_tab(s, tab),
        105 | 106 => {
            let Some(w) = s.windows.get(&win) else { return };
            let pos = w.tabs.iter().position(|t| *t == tab).unwrap_or(0);
            let victims: Vec<_> = w
                .tabs
                .iter()
                .enumerate()
                .filter(|(i, t)| **t != tab && (id == 105 || *i > pos))
                .filter(|(_, t)| !s.tabs.get(t).is_some_and(|t| t.pinned))
                .map(|(_, t)| *t)
                .collect();
            for t in victims {
                tabs::close_tab(s, t);
            }
        }
        107 => tabs::reopen_closed(s, win),
        _ => {}
    }
}

wrap_menu_model_delegate! {
    struct TabMenuDelegate {
        win: WinId,
        tab: super::TabId,
    }

    impl MenuModelDelegate {
        fn execute_command(&self, _menu_model: Option<&mut MenuModel>, command_id: ::std::os::raw::c_int, _event_flags: EventFlags) {
            let (win, tab) = (self.win, self.tab);
            later(move |s| tab_menu_command(s, win, tab, command_id));
        }
    }
}

/// Per-site ad blocking menu (shield button): a disabled count line and the
/// switch for the active tab's site.
pub fn show_blocking_menu(s: &super::State, win: WinId, x: i32, y: i32) {
    let Some(w) = s.windows.get(&win) else { return };
    let Some(window) = w.window.clone() else {
        return;
    };
    let Some(tab) = w.active else { return };
    let Some(t) = s.tabs.get(&tab) else { return };
    let Some(site) = ferrite_core::blocker::site_key(&t.url) else {
        return;
    };
    let off = ferrite_core::blocker::is_exception(&s.settings.adblock_exceptions, &t.url);
    let count = t
        .browser
        .as_ref()
        .map_or(0, |b| super::handlers::blocked_count(b.identifier()));
    let mut delegate = BlockingMenuDelegate::new(win, tab, site.clone());
    let Some(mut model) = menu_model_create(Some(&mut delegate)) else {
        return;
    };
    model.add_item(201, Some(&cef_str(&format!("{count} blocked on {site}"))));
    model.set_enabled(201, 0);
    model.add_separator();
    let label = if off {
        "Turn on blocking on this site"
    } else {
        "Turn off blocking on this site"
    };
    model.add_item(202, Some(&cef_str(label)));
    let mut point = Point { x, y };
    w.ui_view.convert_point_to_screen(Some(&mut point));
    window.show_menu(Some(&mut model), Some(&point), MenuAnchorPosition::TOPRIGHT);
}

/// Switch blocking off/on for `site`, save, and reload the tab.
fn toggle_site_blocking(s: &mut super::State, tab: super::TabId, site: &str) {
    let list = &mut s.settings.adblock_exceptions;
    if let Some(i) = list.iter().position(|e| e == site) {
        list.remove(i);
    } else {
        list.push(site.to_string());
        list.sort();
    }
    s.save_settings();
    super::handlers::apply_exceptions(s);
    if let Some(b) = s.tabs.get(&tab).and_then(|t| t.browser.clone()) {
        b.reload();
    }
    s.mark_all_dirty();
}

wrap_menu_model_delegate! {
    struct BlockingMenuDelegate {
        win: WinId,
        tab: super::TabId,
        site: String,
    }

    impl MenuModelDelegate {
        fn execute_command(&self, _menu_model: Option<&mut MenuModel>, command_id: ::std::os::raw::c_int, _event_flags: EventFlags) {
            if command_id != 202 {
                return;
            }
            let (tab, site) = (self.tab, self.site.clone());
            let _ = self.win;
            later(move |s| toggle_site_blocking(s, tab, &site));
        }
    }
}
