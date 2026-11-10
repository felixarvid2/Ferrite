//! Saving and restoring open windows/tabs (also used for crash recovery: the
//! session is written continuously with `clean_exit = false`).

use super::{tabs, window, State, NEW_TAB_URL};
use crate::LaunchOptions;
use cef::*;
use ferrite_core::session::{Session, TabSnapshot, WindowSnapshot};
use ferrite_core::settings::StartupMode;

pub fn snapshot(s: &State) -> Session {
    let mut windows = Vec::new();
    for w in s.windows.values() {
        if w.private || w.closing {
            continue;
        }
        let tabs: Vec<TabSnapshot> = w
            .tabs
            .iter()
            .filter_map(|t| s.tabs.get(t))
            .filter(|t| !t.closing)
            .map(|t| TabSnapshot {
                url: t.url.clone(),
                title: t.title.clone(),
                pinned: t.pinned,
            })
            .collect();
        if tabs.is_empty() {
            continue;
        }
        let active = w
            .active
            .and_then(|a| w.tabs.iter().position(|t| *t == a))
            .unwrap_or(0);
        let (mut x, mut y, mut width, mut height, mut maximized) = (0, 0, 1280, 800, false);
        if let Some(win) = &w.window {
            let b = win.bounds();
            (x, y, width, height) = (b.x, b.y, b.width, b.height);
            maximized = win.is_maximized() != 0;
        }
        windows.push(WindowSnapshot {
            x,
            y,
            width,
            height,
            maximized,
            tabs,
            active,
        });
    }
    Session {
        windows,
        clean_exit: false,
        saved_at: ferrite_core::now_ms(),
    }
}

/// Write the session (unless frozen by closing the last window).
pub fn save(s: &mut State, force: bool) {
    if s.session_frozen && !force {
        return;
    }
    let session = snapshot(s);
    if let Err(err) = session.save(&s.paths.session) {
        tracing::warn!("saving session failed: {err:#}");
    }
}

pub fn mark_clean_exit(s: &mut State) {
    let mut session = Session::load(&s.paths.session).unwrap_or_default();
    session.clean_exit = true;
    if let Err(err) = session.save(&s.paths.session) {
        tracing::warn!("saving session failed: {err:#}");
    }
}

fn startup_page(s: &State) -> String {
    match s.settings.startup {
        StartupMode::Homepage if !s.settings.homepage.is_empty() => s.settings.homepage.clone(),
        _ => NEW_TAB_URL.to_string(),
    }
}

pub fn restore_or_open(s: &mut State, launch: &LaunchOptions) {
    let urls: Vec<String> = launch.urls.iter().map(|u| s.resolve_input(u)).collect();
    if launch.private {
        window::open_window(s, window::WindowInit::private(urls));
        return;
    }

    let previous = Session::load(&s.paths.session);
    let crashed = previous
        .as_ref()
        .is_some_and(|p| !p.clean_exit && !p.is_empty());
    if crashed {
        tracing::warn!("previous session did not exit cleanly");
    }
    let restore = s.settings.startup == StartupMode::RestoreSession
        || (crashed && s.settings.restore_after_crash);

    let mut first_window = None;
    if let (true, Some(previous)) = (restore, previous) {
        for snap in previous.windows {
            let init = window::WindowInit {
                private: false,
                tabs: snap
                    .tabs
                    .into_iter()
                    .map(|t| window::TabInit {
                        url: t.url,
                        title: t.title,
                        pinned: t.pinned,
                    })
                    .collect(),
                active: snap.active,
                bounds: Some(Rect {
                    x: snap.x,
                    y: snap.y,
                    width: snap.width,
                    height: snap.height,
                }),
                maximized: snap.maximized,
            };
            if let Some(id) = window::open_window(s, init) {
                first_window.get_or_insert(id);
            }
        }
    }

    match first_window {
        Some(win) => {
            for url in urls {
                tabs::new_tab(s, win, &url, tabs::NewTab::foreground());
            }
        }
        None => {
            let urls = if urls.is_empty() {
                vec![startup_page(s)]
            } else {
                urls
            };
            window::open_window(s, window::WindowInit::normal(urls));
        }
    }
    // From now on an unclean exit is detectable.
    save(s, true);
}
