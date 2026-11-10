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

pub fn show(s: &super::State, win: WinId, x: i32, y: i32) {
    let Some(w) = s.windows.get(&win) else { return };
    let Some(window) = w.window.clone() else {
        return;
    };
    let mut delegate = MainMenuDelegate::new(win);
    let Some(model) = menu_model_create(Some(&mut delegate)) else {
        return;
    };
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
    }

    impl MenuModelDelegate {
        fn execute_command(&self, _menu_model: Option<&mut MenuModel>, command_id: ::std::os::raw::c_int, _event_flags: EventFlags) {
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

pub fn show_tab_menu(s: &super::State, win: WinId, tab: super::TabId, x: i32, y: i32) {
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
    w.ui_view.convert_point_to_screen(Some(&mut point));
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
