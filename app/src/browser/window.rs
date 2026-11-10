//! Top-level browser windows built with CEF Views:
//!
//! ```text
//! Window (vertical BoxLayout)
//! ├── UI BrowserView   ferrite://app/chrome.html  (tabs, toolbar, bars)
//! └── content Panel    (FillLayout; one BrowserView per tab, only the
//!                       active one visible)
//! ```

use super::{later, next_id, tabs, theme, with_state, BrowserRef, State, Win, WinId};
use crate::engine::{self, cef_str};
use cef::*;
use ferrite_core::closed_tabs::ClosedTabs;
use std::cell::Cell;
use std::rc::Rc;

/// Width (px) of the resize frame around frameless windows.
const RESIZE_BORDER: i32 = 4;

pub const UI_URL: &str = "ferrite://app/chrome.html";
/// The same UI page in sidebar (vertical tabs) mode.
pub const SIDE_URL: &str = "ferrite://app/chrome.html#side";
/// Width of the vertical-tabs sidebar.
const SIDE_WIDTH: i32 = 240;
/// Height of the UI strip before the page reports its real height.
const INITIAL_UI_HEIGHT: i32 = 78;

pub struct TabInit {
    pub url: String,
    pub title: String,
    pub pinned: bool,
}

pub struct WindowInit {
    pub private: bool,
    pub tabs: Vec<TabInit>,
    pub active: usize,
    pub bounds: Option<Rect>,
    pub maximized: bool,
}

impl WindowInit {
    fn with_urls(private: bool, urls: Vec<String>) -> Self {
        let urls = if urls.is_empty() {
            vec![super::NEW_TAB_URL.to_string()]
        } else {
            urls
        };
        WindowInit {
            private,
            tabs: urls
                .into_iter()
                .map(|url| TabInit {
                    url,
                    title: String::new(),
                    pinned: false,
                })
                .collect(),
            active: 0,
            bounds: None,
            maximized: false,
        }
    }
    pub fn normal(urls: Vec<String>) -> Self {
        Self::with_urls(false, urls)
    }
    pub fn private(urls: Vec<String>) -> Self {
        Self::with_urls(true, urls)
    }
}

pub fn browser_settings(s: &State) -> BrowserSettings {
    BrowserSettings {
        background_color: theme::window_background(&s.settings),
        ..Default::default()
    }
}

pub fn open_window(s: &mut State, init: WindowInit) -> Option<WinId> {
    let id = next_id();
    let height = Rc::new(Cell::new(INITIAL_UI_HEIGHT));
    let mut ctx = s.context_for(init.private);
    let settings = browser_settings(s);

    let mut ui_client = s.ui_client.clone();
    let mut ui_delegate = UiViewDelegate::new(id, height.clone(), false);
    let Some(ui_view) = browser_view_create(
        Some(&mut ui_client),
        Some(&cef_str(UI_URL)),
        Some(&settings),
        None,
        ctx.as_mut(),
        Some(&mut ui_delegate),
    ) else {
        tracing::error!("could not create the UI browser view");
        return None;
    };
    ui_view.set_background_color(theme::window_background(&s.settings));

    let Some(content) = panel_create(None) else {
        tracing::error!("could not create the content panel");
        return None;
    };
    content.set_to_fill_layout();
    content.set_background_color(theme::window_background(&s.settings));

    // Vertical tabs: [sidebar | content] in a horizontal box below the toolbar.
    let mut side_view = None;
    let mut body = content.clone();
    if s.settings.vertical_tabs {
        let mut side_client = s.ui_client.clone();
        let mut side_delegate = UiViewDelegate::new(id, Rc::new(Cell::new(0)), true);
        let side = browser_view_create(
            Some(&mut side_client),
            Some(&cef_str(SIDE_URL)),
            Some(&settings),
            None,
            ctx.as_mut(),
            Some(&mut side_delegate),
        );
        if let (Some(side), Some(row)) = (side, panel_create(None)) {
            side.set_background_color(theme::frame_color(&s.settings, init.private));
            let layout = row.set_to_box_layout(Some(&BoxLayoutSettings {
                horizontal: 1,
                cross_axis_alignment: AxisAlignment::STRETCH,
                ..Default::default()
            }));
            let mut sv = View::from(&side);
            let mut cv = View::from(&content);
            row.add_child_view(Some(&mut sv));
            row.add_child_view(Some(&mut cv));
            if let Some(layout) = layout {
                layout.set_flex_for_view(Some(&mut cv), 1);
            }
            side_view = Some(side);
            body = row;
        }
    }

    s.windows.insert(
        id,
        Win {
            id,
            window: None,
            ui_view: ui_view.clone(),
            ui_browser: None,
            ui_height: height,
            ui_ready: false,
            side_view,
            side_browser: None,
            content: content.clone(),
            tabs: Vec::new(),
            active: None,
            private: init.private,
            closing: false,
            closed_tabs: ClosedTabs::new(),
            html_fullscreen: false,
            prompts: Vec::new(),
            pending_focus_omnibox: false,
        },
    );

    let count = init.tabs.len();
    // (Popup windows start empty and adopt the popup's view right after.)
    let active = init.active.min(count.saturating_sub(1));
    let mut active_id = None;
    for (i, t) in init.tabs.into_iter().enumerate() {
        let opts = tabs::NewTab {
            background: true,
            lazy: i != active,
            pinned: t.pinned,
            title: t.title,
            ..Default::default()
        };
        let tab = tabs::new_tab(s, id, &t.url, opts);
        if i == active {
            active_id = tab;
        }
    }
    if let Some(tab) = active_id {
        tabs::activate(s, id, tab);
    }

    let mut delegate = MainWindowDelegate::new(
        id,
        ui_view,
        body,
        init.bounds.unwrap_or(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }),
        init.maximized,
        init.private,
        (!s.settings.system_titlebar).then(|| theme::frame_color(&s.settings, init.private)),
    );
    let window = window_create_top_level(Some(&mut delegate));
    if let Some(w) = s.windows.get_mut(&id) {
        w.window = window;
    }
    s.last_window_focus = Some(id);
    s.schedule_session_save();
    Some(id)
}

/// Begin closing a window (from our own code, e.g. its last tab closed).
pub fn close_window(s: &mut State, win: WinId) {
    prepare_close(s, win);
    if let Some(window) = s.windows.get(&win).and_then(|w| w.window.clone()) {
        engine::post_local(move || window.close());
    }
}

/// Runs before a window goes away: keeps the session if it is the last
/// normal window, so the next start can restore it.
fn prepare_close(s: &mut State, win: WinId) {
    let Some(w) = s.windows.get(&win) else { return };
    if w.closing {
        return;
    }
    let private = w.private;
    let others = s
        .windows
        .values()
        .filter(|o| o.id != win && !o.private && !o.closing)
        .count();
    if !private && others == 0 {
        super::session::save(s, true);
        s.session_frozen = true;
    }
    if let Some(w) = s.windows.get_mut(&win) {
        w.closing = true;
    }
    s.schedule_session_save();
}

fn window_destroyed(s: &mut State, win: WinId) {
    let Some(w) = s.windows.remove(&win) else {
        return;
    };
    for tab in w.tabs {
        if let Some(t) = s.tabs.remove(&tab) {
            if let Some(b) = &t.browser {
                // The browser closes with the view hierarchy; keep the id
                // mapped to nothing so late callbacks are ignored.
                s.browsers.remove(&b.identifier());
            }
        }
    }
    if let Some(b) = &w.ui_browser {
        s.browsers.remove(&b.identifier());
    }
    if !s.windows.values().any(|w| w.private) {
        s.private_context = None;
        // Forget private browsing traces kept in memory.
        if let Ok(db) = ferrite_core::db::Database::open_in_memory() {
            s.private_db = db;
        }
    }
    if s.last_window_focus == Some(win) {
        s.last_window_focus = s.windows.keys().next().copied();
    }
    s.schedule_session_save();
    s.maybe_quit();
}

/// Update the native window title from the active tab.
pub fn update_title(s: &State, win: WinId) {
    let Some(w) = s.windows.get(&win) else { return };
    let title = w
        .active
        .and_then(|t| s.tabs.get(&t))
        .map(|t| {
            if t.title.is_empty() {
                t.url.clone()
            } else {
                t.title.clone()
            }
        })
        .unwrap_or_default();
    let suffix = if w.private {
        "Ferrite (Private)"
    } else {
        "Ferrite"
    };
    let full = if title.is_empty() {
        suffix.to_string()
    } else {
        format!("{title} — {suffix}")
    };
    if let Some(window) = &w.window {
        window.set_title(Some(&cef_str(&full)));
    }
}

/// Resize the UI strip (the page reports its content height).
pub fn set_ui_height(s: &mut State, win: WinId, height: i32) {
    let Some(w) = s.windows.get(&win) else { return };
    let height = height.clamp(30, 2000);
    if w.ui_height.get() == height {
        return;
    }
    w.ui_height.set(height);
    w.ui_view.invalidate_layout();
    if let Some(window) = &w.window {
        window.layout();
    }
}

/// Hide the UI strip while a page is in HTML fullscreen (video etc.).
pub fn set_html_fullscreen(s: &mut State, win: WinId, fullscreen: bool) {
    let Some(w) = s.windows.get_mut(&win) else {
        return;
    };
    w.html_fullscreen = fullscreen;
    w.ui_view.set_visible(!fullscreen as i32);
    if let Some(side) = &w.side_view {
        side.set_visible(!fullscreen as i32);
    }
    if let Some(window) = &w.window {
        if window.is_fullscreen() != fullscreen as i32 {
            window.set_fullscreen(fullscreen as i32);
        }
        window.layout();
    }
}

pub fn toggle_fullscreen(s: &mut State, win: WinId) {
    if let Some(window) = s.windows.get(&win).and_then(|w| w.window.clone()) {
        let fs = window.is_fullscreen() == 0;
        window.set_fullscreen(fs as i32);
    }
}

wrap_window_delegate! {
    struct MainWindowDelegate {
        win_id: WinId,
        ui_view: BrowserView,
        content: Panel,
        bounds: Rect,
        maximized: bool,
        private: bool,
        // Some(frame colour) for a frameless window.
        frameless: Option<u32>,
    }

    impl ViewDelegate {
        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 480, height: 320 }
        }
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1280, height: 860 }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            let layout = window.set_to_box_layout(Some(&BoxLayoutSettings {
                horizontal: 0,
                cross_axis_alignment: AxisAlignment::STRETCH,
                ..Default::default()
            }));
            let mut ui = View::from(&self.ui_view);
            let mut content = View::from(&self.content);
            window.add_child_view(Some(&mut ui));
            window.add_child_view(Some(&mut content));
            if let Some(layout) = layout {
                layout.set_flex_for_view(Some(&mut content), 1);
            }
            let title = if self.private { "Ferrite (Private)" } else { "Ferrite" };
            window.set_title(Some(&cef_str(title)));
            if let Some(frame_color) = self.frameless {
                // Web content is a native child window that swallows mouse
                // events, so leave a thin frame for resizing from the edges.
                window.set_insets(Some(&Insets { top: 0, left: RESIZE_BORDER, bottom: RESIZE_BORDER, right: RESIZE_BORDER }));
                window.set_background_color(frame_color);
            }
            window.show();
            if self.maximized {
                window.maximize();
            }
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            let win = self.win_id;
            later(move |s| window_destroyed(s, win));
        }

        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            // When we close the window ourselves the state is borrowed and
            // the preparation already happened.
            let win = self.win_id;
            with_state(|s| prepare_close(s, win));
            1
        }

        fn initial_bounds(&self, _window: Option<&mut Window>) -> Rect {
            if self.bounds.width >= 200 && self.bounds.height >= 200 {
                self.bounds.clone()
            } else {
                Rect { x: 0, y: 0, width: 0, height: 0 }
            }
        }

        fn on_window_activation_changed(&self, _window: Option<&mut Window>, active: ::std::os::raw::c_int) {
            if active != 0 {
                let win = self.win_id;
                later(move |s| s.last_window_focus = Some(win));
            }
        }

        fn on_window_bounds_changed(&self, window: Option<&mut Window>, _bounds: Option<&Rect>) {
            // The resize frame is only useful for a normal (restored) window.
            if let (Some(_), Some(window)) = (self.frameless, window) {
                let edge = window.is_maximized() == 0 && window.is_fullscreen() == 0;
                let border = if edge { RESIZE_BORDER } else { 0 };
                if window.insets().left != border {
                    window.set_insets(Some(&Insets { top: 0, left: border, bottom: border, right: border }));
                    window.layout();
                }
            }
            later(|s| s.schedule_session_save());
        }

        // cef-rs defaults these to 0 (CEF's C++ defaults are true): without
        // them the window cannot be resized, maximized or minimized.
        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn can_maximize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn can_minimize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn is_frameless(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            // The tab strip doubles as the title bar (drag regions come from
            // the UI page's `app-region: drag` CSS).
            self.frameless.is_some() as i32
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

wrap_browser_view_delegate! {
    pub struct UiViewDelegate {
        win_id: WinId,
        height: Rc<Cell<i32>>,
        side: bool,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            if self.side {
                return Size { width: SIDE_WIDTH, height: 100 };
            }
            Size { width: 800, height: self.height.get() }
        }
        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            if self.side {
                return Size { width: SIDE_WIDTH, height: 50 };
            }
            Size { width: 200, height: self.height.get() }
        }
    }

    impl BrowserViewDelegate {
        fn on_browser_created(&self, _view: Option<&mut BrowserView>, browser: Option<&mut Browser>) {
            let Some(browser) = browser.cloned() else { return };
            let (win, side) = (self.win_id, self.side);
            later(move |s| {
                s.browsers.insert(browser.identifier(), BrowserRef::Ui(win));
                if let Some(w) = s.windows.get_mut(&win) {
                    if side {
                        w.side_browser = Some(browser);
                        return;
                    }
                    w.ui_browser = Some(browser);
                }
            });
        }

        fn on_popup_browser_view_created(
            &self,
            _view: Option<&mut BrowserView>,
            _popup: Option<&mut BrowserView>,
            _is_devtools: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            0
        }

        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

/// Window buttons drawn by the UI strip in frameless mode.
pub fn window_action(s: &mut State, win: WinId, action: &str) {
    let Some(window) = s.windows.get(&win).and_then(|w| w.window.clone()) else {
        return;
    };
    match action {
        "minimize" => window.minimize(),
        "maximize" => {
            if window.is_maximized() != 0 {
                window.restore();
            } else {
                window.maximize();
            }
            s.mark_dirty(win);
        }
        "close" => close_window(s, win),
        _ => {}
    }
}
