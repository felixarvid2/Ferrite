//! Tab lifecycle: creation (including popups that become tabs), activation,
//! closing, reordering, pinning and navigation.

use super::{later, next_id, scheme, window, BrowserRef, FindState, State, Tab, TabId, WinId};
use crate::engine::cef_str;
use cef::*;
use ferrite_core::closed_tabs::ClosedTab;
use std::cell::Cell;

#[derive(Default, Clone)]
pub struct NewTab {
    pub background: bool,
    pub index: Option<usize>,
    pub pinned: bool,
    /// Do not create the browser until the tab is first shown.
    pub lazy: bool,
    pub title: String,
    pub opener: Option<TabId>,
}

impl NewTab {
    pub fn foreground() -> Self {
        Self::default()
    }
    pub fn background() -> Self {
        Self {
            background: true,
            ..Self::default()
        }
    }
}

thread_local! {
    /// Set by `on_before_popup`, read by the popup view callbacks that follow
    /// on the same stack (CEF calls them in that order on the UI thread).
    pub static PENDING_POPUP_DISPOSITION: Cell<Option<WindowOpenDisposition>> = const { Cell::new(None) };
    static PENDING_POPUP_TAB: Cell<Option<TabId>> = const { Cell::new(None) };
}

fn empty_tab(id: TabId, win: WinId, url: &str, opts: &NewTab) -> Tab {
    Tab {
        id,
        win,
        view: None,
        browser: None,
        url: url.to_string(),
        title: opts.title.clone(),
        favicon: None,
        loading: false,
        progress: 0.0,
        can_back: false,
        can_forward: false,
        pinned: opts.pinned,
        crashed: false,
        closing: false,
        zoom_percent: 100,
        find: FindState::default(),
        media_video: false,
        media_audio: false,
        cert_error: false,
    }
}

/// Where a new tab goes in the strip.
fn insert_index(s: &State, win: WinId, opts: &NewTab) -> usize {
    let w = &s.windows[&win];
    let pinned_count = w
        .tabs
        .iter()
        .filter(|t| s.tabs.get(t).is_some_and(|t| t.pinned))
        .count();
    let idx = if let Some(i) = opts.index {
        i
    } else if let Some(pos) = opts
        .opener
        .and_then(|o| w.tabs.iter().position(|t| *t == o))
    {
        pos + 1
    } else {
        w.tabs.len()
    };
    let idx = idx.min(w.tabs.len());
    if opts.pinned {
        idx.min(pinned_count)
    } else {
        idx.max(pinned_count)
    }
}

pub fn new_tab(s: &mut State, win: WinId, url: &str, opts: NewTab) -> Option<TabId> {
    if !s.windows.contains_key(&win) {
        return None;
    }
    let id = next_id();
    let url = if url.is_empty() {
        super::NEW_TAB_URL
    } else {
        url
    };
    s.tabs.insert(id, empty_tab(id, win, url, &opts));
    let index = insert_index(s, win, &opts);
    s.windows.get_mut(&win)?.tabs.insert(index, id);
    if !opts.lazy {
        create_view(s, id);
    }
    if !opts.background {
        activate(s, win, id);
    }
    s.mark_dirty(win);
    s.schedule_session_save();
    Some(id)
}

/// Create the BrowserView (and so the browser) for a tab.
pub fn create_view(s: &mut State, tab: TabId) {
    let Some(t) = s.tabs.get(&tab) else { return };
    if t.view.is_some() {
        return;
    }
    let (win, url) = (t.win, t.url.clone());
    let private = s.is_private(win);
    let mut ctx = s.context_for(private);
    let settings = window::browser_settings(s);
    let mut client = s.content_client.clone();
    let mut delegate = TabViewDelegate::new(tab);
    scheme::allow_internal(&url);
    let Some(view) = browser_view_create(
        Some(&mut client),
        Some(&cef_str(&url)),
        Some(&settings),
        None,
        ctx.as_mut(),
        Some(&mut delegate),
    ) else {
        tracing::error!("browser_view_create failed for {url}");
        return;
    };
    attach_view(s, tab, view);
}

fn attach_view(s: &mut State, tab: TabId, view: BrowserView) {
    let Some(t) = s.tabs.get_mut(&tab) else {
        return;
    };
    let win = t.win;
    view.set_visible(0);
    t.view = Some(view.clone());
    if let Some(w) = s.windows.get(&win) {
        let mut v = View::from(&view);
        w.content.add_child_view(Some(&mut v));
    }
}

/// A popup opened by a page in a tab disposition becomes a tab of the
/// opener's window (keeping `window.opener`).
fn adopt_popup(
    s: &mut State,
    opener: TabId,
    popup_tab: TabId,
    view: BrowserView,
    background: bool,
) {
    let Some(win) = s.tabs.get(&opener).map(|t| t.win) else {
        return;
    };
    let opts = NewTab {
        background,
        opener: Some(opener),
        ..Default::default()
    };
    s.tabs
        .insert(popup_tab, empty_tab(popup_tab, win, "", &opts));
    let index = insert_index(s, win, &opts);
    if let Some(w) = s.windows.get_mut(&win) {
        w.tabs.insert(index, popup_tab);
    }
    attach_view(s, popup_tab, view);
    if !background {
        activate(s, win, popup_tab);
    }
    s.mark_dirty(win);
}

pub fn activate(s: &mut State, win: WinId, tab: TabId) {
    let Some(w) = s.windows.get(&win) else { return };
    if !w.tabs.contains(&tab) {
        return;
    }
    let previous = w.active;
    if let Some(prev) = previous.filter(|p| *p != tab) {
        if let Some(view) = s.tabs.get(&prev).and_then(|t| t.view.clone()) {
            view.set_visible(0);
        }
    }
    create_view(s, tab);
    let Some(w) = s.windows.get_mut(&win) else {
        return;
    };
    w.active = Some(tab);
    let content = w.content.clone();
    let t = &s.tabs[&tab];
    if let Some(view) = &t.view {
        view.set_visible(1);
        content.layout();
        if t.url == super::NEW_TAB_URL || t.url.is_empty() {
            focus_omnibox(s, win);
        } else {
            view.request_focus();
        }
    }
    window::update_title(s, win);
    s.mark_dirty(win);
    s.schedule_session_save();
}

pub fn focus_omnibox(s: &mut State, win: WinId) {
    let Some(w) = s.windows.get_mut(&win) else {
        return;
    };
    if !w.ui_ready {
        w.pending_focus_omnibox = true;
        return;
    }
    w.ui_view.request_focus();
    if let Some(b) = w.ui_browser.as_ref().and_then(|b| b.host()) {
        b.set_focus(1);
    }
    s.ui_call(win, "focusOmnibox", &serde_json::Value::Null);
}

pub fn close_tab(s: &mut State, tab: TabId) {
    let Some(t) = s.tabs.get_mut(&tab) else {
        return;
    };
    if t.closing {
        return;
    }
    let win = t.win;
    match t.host() {
        Some(host) => {
            t.closing = true;
            // Runs beforeunload handlers; on_before_close finishes the job.
            host.close_browser(0);
        }
        None => tab_closed(s, tab),
    }
    s.mark_dirty(win);
}

/// The tab's browser is gone (or never existed): drop it from the window.
/// Alloy-style close step (from `do_close`): destroying the view finishes
/// closing the browser, after which `on_before_close` calls [`tab_closed`].
pub fn detach_view(s: &mut State, tab: TabId) {
    let Some(t) = s.tabs.get_mut(&tab) else {
        return;
    };
    let Some(view) = t.view.take() else { return };
    t.closing = true;
    if let Some(w) = s.windows.get(&t.win) {
        let mut v = View::from(&view);
        w.content.remove_child_view(Some(&mut v));
    }
}

pub fn tab_closed(s: &mut State, tab: TabId) {
    let Some(t) = s.tabs.remove(&tab) else { return };
    let win = t.win;
    let Some(w) = s.windows.get_mut(&win) else {
        return;
    };
    let index = w.tabs.iter().position(|x| *x == tab).unwrap_or(0);
    w.tabs.retain(|x| *x != tab);
    if let Some(view) = &t.view {
        let mut v = View::from(view);
        w.content.remove_child_view(Some(&mut v));
    }
    if t.url != super::NEW_TAB_URL && !t.url.is_empty() {
        w.closed_tabs.push(ClosedTab {
            url: t.url.clone(),
            title: t.title.clone(),
            pinned: t.pinned,
            index,
        });
    }
    let was_active = w.active == Some(tab);
    let closing = w.closing;
    if w.tabs.is_empty() {
        w.active = None;
        if !closing {
            window::close_window(s, win);
        }
        return;
    }
    if was_active {
        let next = w.tabs[index.min(w.tabs.len() - 1)];
        w.active = None;
        activate(s, win, next);
    }
    s.mark_dirty(win);
    s.schedule_session_save();
}

pub fn move_tab(s: &mut State, tab: TabId, to: usize) {
    let Some(win) = s.tabs.get(&tab).map(|t| t.win) else {
        return;
    };
    let pinned = s.tabs[&tab].pinned;
    let pinned_count = s.windows[&win]
        .tabs
        .iter()
        .filter(|t| **t != tab && s.tabs.get(t).is_some_and(|t| t.pinned))
        .count();
    let Some(w) = s.windows.get_mut(&win) else {
        return;
    };
    w.tabs.retain(|t| *t != tab);
    let to = to.min(w.tabs.len());
    let to = if pinned {
        to.min(pinned_count)
    } else {
        to.max(pinned_count)
    };
    w.tabs.insert(to, tab);
    s.mark_dirty(win);
    s.schedule_session_save();
}

pub fn set_pinned(s: &mut State, tab: TabId, pinned: bool) {
    let Some(t) = s.tabs.get_mut(&tab) else {
        return;
    };
    t.pinned = pinned;
    let win = t.win;
    // Re-insert at the edge of the pinned group.
    let pinned_count = s.windows[&win]
        .tabs
        .iter()
        .filter(|x| **x != tab && s.tabs.get(x).is_some_and(|t| t.pinned))
        .count();
    move_tab(s, tab, pinned_count);
}

pub fn duplicate(s: &mut State, tab: TabId) {
    let Some(t) = s.tabs.get(&tab) else { return };
    let (win, url) = (t.win, t.url.clone());
    new_tab(
        s,
        win,
        &url,
        NewTab {
            opener: Some(tab),
            ..Default::default()
        },
    );
}

pub fn reopen_closed(s: &mut State, win: WinId) {
    let Some(closed) = s.windows.get_mut(&win).and_then(|w| w.closed_tabs.pop()) else {
        return;
    };
    new_tab(
        s,
        win,
        &closed.url,
        NewTab {
            index: Some(closed.index),
            pinned: closed.pinned,
            title: closed.title,
            ..Default::default()
        },
    );
}

pub fn cycle(s: &mut State, win: WinId, delta: isize) {
    let Some(w) = s.windows.get(&win) else { return };
    if w.tabs.is_empty() {
        return;
    }
    let len = w.tabs.len() as isize;
    let cur = w
        .active
        .and_then(|a| w.tabs.iter().position(|t| *t == a))
        .unwrap_or(0) as isize;
    let next = w.tabs[((cur + delta).rem_euclid(len)) as usize];
    activate(s, win, next);
}

pub fn select_index(s: &mut State, win: WinId, index: Option<usize>) {
    let Some(w) = s.windows.get(&win) else { return };
    let tab = match index {
        Some(i) => w.tabs.get(i).copied(),
        None => w.tabs.last().copied(),
    };
    if let Some(tab) = tab {
        activate(s, win, tab);
    }
}

/// Navigate a tab to user input (URL or search terms).
pub fn navigate(s: &mut State, tab: TabId, input: &str) {
    let url = s.resolve_input(input);
    if url.is_empty() {
        return;
    }
    load_url(s, tab, &url);
}

pub fn load_url(s: &mut State, tab: TabId, url: &str) {
    let Some(t) = s.tabs.get_mut(&tab) else {
        return;
    };
    t.url = url.to_string();
    t.crashed = false;
    let win = t.win;
    scheme::allow_internal(url);
    match t.main_frame() {
        Some(frame) => frame.load_url(Some(&cef_str(url))),
        None => create_view(s, tab),
    }
    if let Some(view) = s.tabs.get(&tab).and_then(|t| t.view.clone()) {
        if s.active_tab(win) == Some(tab) {
            view.request_focus();
        }
    }
    s.mark_dirty(win);
}

pub fn browser_created(s: &mut State, tab: TabId, browser: Browser) {
    s.browsers
        .insert(browser.identifier(), BrowserRef::Tab(tab));
    let Some(t) = s.tabs.get_mut(&tab) else {
        // Popup windows/devtools made by CEF itself: not our tabs.
        s.browsers.remove(&browser.identifier());
        return;
    };
    t.browser = Some(browser);
    let win = t.win;
    s.mark_dirty(win);
}

wrap_browser_view_delegate! {
    pub struct TabViewDelegate {
        tab_id: TabId,
    }

    impl ViewDelegate {}

    impl BrowserViewDelegate {
        fn on_browser_created(&self, _view: Option<&mut BrowserView>, browser: Option<&mut Browser>) {
            let Some(browser) = browser.cloned() else { return };
            let tab = self.tab_id;
            later(move |s| browser_created(s, tab, browser));
        }

        fn delegate_for_popup_browser_view(
            &self,
            _view: Option<&mut BrowserView>,
            _settings: Option<&BrowserSettings>,
            _client: Option<&mut Client>,
            _is_devtools: ::std::os::raw::c_int,
        ) -> Option<BrowserViewDelegate> {
            let id = next_id();
            PENDING_POPUP_TAB.with(|p| p.set(Some(id)));
            Some(TabViewDelegate::new(id))
        }

        fn on_popup_browser_view_created(
            &self,
            _view: Option<&mut BrowserView>,
            popup: Option<&mut BrowserView>,
            is_devtools: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let popup_tab = PENDING_POPUP_TAB.with(|p| p.take());
            let disposition = PENDING_POPUP_DISPOSITION.with(|p| p.take());
            let (Some(popup), Some(popup_tab)) = (popup.cloned(), popup_tab) else { return 0 };
            if is_devtools != 0 {
                // CEF creates a default top-level window for DevTools.
                return 0;
            }
            let background = match disposition {
                Some(d) if d == WindowOpenDisposition::NEW_FOREGROUND_TAB => false,
                Some(d) if d == WindowOpenDisposition::NEW_BACKGROUND_TAB => true,
                // Real popups (window.open with features, e.g. OAuth/login
                // windows): let CEF give them their own window.
                _ => return 0,
            };
            let opener = self.tab_id;
            later(move |s| adopt_popup(s, opener, popup_tab, popup, background));
            1
        }

        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}
