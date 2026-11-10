//! Executing browser commands (from shortcuts, menus and the UI).

use super::{tabs, window, State, TabId, WinId, NEW_TAB_URL};
use crate::engine::{self, cef_str};
use cef::*;
pub use ferrite_core::commands::Command;

pub fn zoom_percent(level: f64) -> u32 {
    (1.2f64.powf(level) * 100.0).round() as u32
}

/// Open an internal page, reusing a tab in this window that shows it.
pub fn open_internal(s: &mut State, win: WinId, url: &str) {
    let existing = s.windows.get(&win).and_then(|w| {
        w.tabs.iter().copied().find(|t| {
            s.tabs
                .get(t)
                .is_some_and(|t| t.url.trim_end_matches('/') == url.trim_end_matches('/'))
        })
    });
    match existing {
        Some(tab) => tabs::activate(s, win, tab),
        None => {
            tabs::new_tab(s, win, url, tabs::NewTab::foreground());
        }
    }
}

pub fn find(s: &mut State, tab: TabId, text: &str, forward: bool, find_next: bool) {
    let Some(t) = s.tabs.get_mut(&tab) else {
        return;
    };
    t.find.open = true;
    t.find.text = text.to_string();
    let Some(host) = t.host() else { return };
    if text.is_empty() {
        host.stop_finding(1);
        t.find.count = 0;
        t.find.active = 0;
    } else {
        host.find(Some(&cef_str(text)), forward as i32, 0, find_next as i32);
    }
    let win = t.win;
    s.mark_dirty(win);
}

pub fn close_find(s: &mut State, tab: TabId) {
    let Some(t) = s.tabs.get_mut(&tab) else {
        return;
    };
    if let Some(host) = t.host() {
        host.stop_finding(1);
    }
    t.find = Default::default();
    let win = t.win;
    if let Some(view) = &t.view {
        view.request_focus();
    }
    s.mark_dirty(win);
}

pub fn close_find_if_open(s: &mut State, win: WinId) {
    if let Some(tab) = s.active_tab(win) {
        if s.tabs.get(&tab).is_some_and(|t| t.find.open) {
            close_find(s, tab);
        }
    }
}

pub fn zoom(s: &mut State, tab: TabId, command: ZoomCommand) {
    let Some(host) = s.tabs.get(&tab).and_then(|t| t.host()) else {
        return;
    };
    host.zoom(command);
    // The new level is applied asynchronously.
    engine::post_local_delayed(150, move || {
        super::with_state(|s| {
            let Some(t) = s.tabs.get_mut(&tab) else {
                return;
            };
            if let Some(host) = t.host() {
                t.zoom_percent = zoom_percent(host.zoom_level());
            }
            let win = t.win;
            s.mark_dirty(win);
        });
    });
}

/// Bookmark the active page, or remove its bookmarks if it has some.
pub fn toggle_bookmark(s: &mut State, win: WinId) -> Option<bool> {
    let tab = s.active_tab(win)?;
    let (url, title) = {
        let t = s.tabs.get(&tab)?;
        (t.url.clone(), t.title.clone())
    };
    if url.is_empty() || url == NEW_TAB_URL {
        return None;
    }
    let existing = s.db.find_bookmarks_by_url(&url).ok()?;
    let added = if existing.is_empty() {
        let title = if title.is_empty() { url.clone() } else { title };
        s.db.add_bookmark(ferrite_core::bookmarks::BOOKMARKS_BAR, &title, &url)
            .ok()?;
        true
    } else {
        for b in existing {
            let _ = s.db.delete_bookmark(b.id);
        }
        false
    };
    s.mark_all_dirty();
    Some(added)
}

pub fn exec(s: &mut State, win: WinId, command: Command) {
    use Command::*;
    let active = s.active_tab(win);
    let host = active.and_then(|t| s.tabs.get(&t)).and_then(|t| t.host());
    let browser = active
        .and_then(|t| s.tabs.get(&t))
        .and_then(|t| t.browser.clone());
    match command {
        NewTab => {
            tabs::new_tab(s, win, NEW_TAB_URL, tabs::NewTab::foreground());
        }
        NewWindow => {
            window::open_window(s, window::WindowInit::normal(vec![]));
        }
        NewPrivateWindow => {
            window::open_window(s, window::WindowInit::private(vec![]));
        }
        CloseTab => {
            if let Some(tab) = active {
                tabs::close_tab(s, tab);
            }
        }
        CloseWindow => window::close_window(s, win),
        ReopenClosedTab => tabs::reopen_closed(s, win),
        FocusAddressBar => tabs::focus_omnibox(s, win),
        Reload => {
            if let Some(b) = browser {
                b.reload();
            }
        }
        HardReload => {
            if let Some(b) = browser {
                b.reload_ignore_cache();
            }
        }
        Stop => {
            if let Some(b) = browser {
                b.stop_load();
            }
        }
        Back => {
            if let Some(b) = browser {
                b.go_back();
            }
        }
        Forward => {
            if let Some(b) = browser {
                b.go_forward();
            }
        }
        Home => {
            let home = s.settings.homepage.clone();
            if let Some(tab) = active {
                tabs::load_url(s, tab, if home.is_empty() { NEW_TAB_URL } else { &home });
            }
        }
        NextTab => tabs::cycle(s, win, 1),
        PrevTab => tabs::cycle(s, win, -1),
        SelectTab(i) => tabs::select_index(s, win, Some(i)),
        SelectLastTab => tabs::select_index(s, win, None),
        Find => {
            if let Some(tab) = active {
                if let Some(t) = s.tabs.get_mut(&tab) {
                    t.find.open = true;
                }
                s.mark_dirty(win);
                if let Some(w) = s.windows.get(&win) {
                    w.ui_view.request_focus();
                }
                s.ui_call(win, "focusFind", &serde_json::Value::Null);
            }
        }
        FindNext | FindPrev => {
            if let Some(tab) = active {
                let text = s
                    .tabs
                    .get(&tab)
                    .map(|t| t.find.text.clone())
                    .unwrap_or_default();
                if text.is_empty() {
                    exec(s, win, Find);
                } else {
                    find(s, tab, &text, command == FindNext, true);
                }
            }
        }
        BookmarkPage => {
            if let Some(added) = toggle_bookmark(s, win) {
                let msg = if added {
                    "Bookmark added"
                } else {
                    "Bookmark removed"
                };
                s.ui_call(win, "toast", &serde_json::json!(msg));
            }
        }
        ToggleBookmarksBar => {
            s.settings.show_bookmarks_bar = !s.settings.show_bookmarks_bar;
            s.save_settings();
            s.mark_all_dirty();
        }
        Downloads => open_internal(s, win, "ferrite://downloads/"),
        History => open_internal(s, win, "ferrite://history/"),
        Bookmarks => open_internal(s, win, "ferrite://bookmarks/"),
        Settings => open_internal(s, win, "ferrite://settings/"),
        ZoomIn | ZoomOut | ZoomReset => {
            let cmd = match command {
                ZoomIn => ZoomCommand::IN,
                ZoomOut => ZoomCommand::OUT,
                _ => ZoomCommand::RESET,
            };
            if let Some(tab) = active {
                zoom(s, tab, cmd);
            }
        }
        Fullscreen => window::toggle_fullscreen(s, win),
        DevTools => {
            if let Some(host) = host {
                if host.has_dev_tools() != 0 {
                    host.close_dev_tools();
                } else {
                    host.show_dev_tools(Some(&WindowInfo::default()), None, None, None);
                }
            }
        }
        ViewSource => {
            if let Some(tab) = active {
                let url = s.tabs.get(&tab).map(|t| t.url.clone()).unwrap_or_default();
                if url.starts_with("http") || url.starts_with("file:") {
                    tabs::new_tab(
                        s,
                        win,
                        &format!("view-source:{url}"),
                        tabs::NewTab {
                            opener: Some(tab),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        Print => {
            if let Some(host) = host {
                host.print();
            }
        }
        DuplicateTab => {
            if let Some(tab) = active {
                tabs::duplicate(s, tab);
            }
        }
        PinTab => {
            if let Some(tab) = active {
                let pinned = s.tabs.get(&tab).is_some_and(|t| t.pinned);
                tabs::set_pinned(s, tab, !pinned);
            }
        }
        Quit => {
            let ids: Vec<WinId> = s.windows.keys().copied().collect();
            // Keep every normal window in the session, not just the last.
            super::session::save(s, true);
            s.session_frozen = true;
            for id in ids {
                window::close_window(s, id);
            }
        }
    }
}
