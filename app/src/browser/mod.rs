//! Browser application state: windows, tabs and the services behind them.
//!
//! All state lives on the CEF UI thread in a thread-local [`State`]. CEF
//! callbacks never touch it directly while CEF may be calling back into us;
//! instead they post closures ([`later`]) that run on a clean stack. This
//! avoids re-entrancy panics when a CEF call synchronously triggers another
//! callback.

pub mod api;
pub mod assets;
pub mod cdp;
pub mod commands;
pub mod context_menu;
pub mod downloads;
pub mod handlers;
pub mod import;
pub mod inject;
pub mod mcp;
pub mod menu;
pub mod passwords;
pub mod permissions;
pub mod scheme;
pub mod session;
pub mod tabs;
pub mod theme;
pub mod updates;
pub mod window;

use crate::engine::{self, cef_str};
use crate::LaunchOptions;
use cef::*;
use ferrite_core::{
    closed_tabs::ClosedTabs, db::Database, profile::ProfilePaths, settings::Settings,
};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

pub type WinId = u32;
pub type TabId = u32;

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

pub fn next_id() -> u32 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub const NEW_TAB_URL: &str = "ferrite://newtab/";

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FindState {
    pub open: bool,
    pub text: String,
    pub count: i32,
    pub active: i32,
}

pub struct Tab {
    pub id: TabId,
    pub win: WinId,
    /// Created lazily: restored background tabs get a view on first activation.
    pub view: Option<BrowserView>,
    pub browser: Option<Browser>,
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    pub loading: bool,
    pub progress: f64,
    pub can_back: bool,
    pub can_forward: bool,
    pub pinned: bool,
    pub crashed: bool,
    pub closing: bool,
    pub zoom_percent: u32,
    pub find: FindState,
    pub media_video: bool,
    pub media_audio: bool,
    pub cert_error: bool,
    /// Last time the tab was shown (ms); used for tab sleeping.
    pub last_active: i64,
    /// The browser is being closed to free memory; the tab stays.
    pub discarding: bool,
    /// Result of the TLS check for the current page (https only).
    pub secure: Option<bool>,
    /// Under AI control (MCP)? See `mcp.rs`.
    pub ai: ferrite_mcp::policy::Control,
    /// Counts loads started in this tab (the MCP tools wait on it).
    pub load_seq: u64,
}

impl Tab {
    pub fn host(&self) -> Option<BrowserHost> {
        self.browser.as_ref().and_then(|b| b.host())
    }
    pub fn main_frame(&self) -> Option<Frame> {
        self.browser.as_ref().and_then(|b| b.main_frame())
    }
}

/// A permission prompt waiting for the user (shown in the window's UI bar).
pub struct Prompt {
    pub id: u64,
    pub tab: TabId,
    pub origin: String,
    pub permissions: Vec<String>,
    pub reply: permissions::PromptReply,
}

pub struct Win {
    pub id: WinId,
    pub window: Option<Window>,
    pub ui_view: BrowserView,
    pub ui_browser: Option<Browser>,
    pub ui_height: Rc<Cell<i32>>,
    pub ui_ready: bool,
    /// Vertical-tabs sidebar (a second UI view) when enabled.
    pub side_view: Option<BrowserView>,
    pub side_browser: Option<Browser>,
    pub content: Panel,
    pub tabs: Vec<TabId>,
    pub active: Option<TabId>,
    pub private: bool,
    /// The AI window: isolated in-memory context, leaves no traces.
    pub ai: bool,
    pub closing: bool,
    pub closed_tabs: ClosedTabs,
    pub html_fullscreen: bool,
    pub prompts: Vec<Prompt>,
    /// Ask the UI to focus the address bar once it is ready.
    pub pending_focus_omnibox: bool,
}

impl Win {
    pub fn ui_frame(&self) -> Option<Frame> {
        self.ui_browser.as_ref().and_then(|b| b.main_frame())
    }
}

/// What a CEF browser id refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserRef {
    Tab(TabId),
    Ui(WinId),
}

pub struct State {
    pub paths: ProfilePaths,
    pub db: Database,
    /// In-memory database used for private windows' history-like data.
    pub private_db: Database,
    pub settings: Settings,
    pub launch: LaunchOptions,
    pub windows: BTreeMap<WinId, Win>,
    pub tabs: HashMap<TabId, Tab>,
    pub browsers: HashMap<i32, BrowserRef>,
    pub content_client: Client,
    pub ui_client: Client,
    pub private_context: Option<RequestContext>,
    /// In-memory context shared by the AI windows (not the user's profile).
    pub ai_context: Option<RequestContext>,
    pub downloads: downloads::Downloads,
    /// Windows whose UI needs a state push.
    pub dirty: HashSet<WinId>,
    pub flush_scheduled: bool,
    pub session_save_scheduled: bool,
    /// Set when the last normal window closes, so that the saved session
    /// keeps that window's tabs for the next start.
    pub session_frozen: bool,
    pub last_window_focus: Option<WinId>,
    /// Live CEF browsers (tabs + UI); the app quits when this and the
    /// window list are both empty.
    pub live_browsers: usize,
    /// Proton Pass login cache (no passwords).
    pub passwords: passwords::Store,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static PRELOAD: RefCell<Option<(ProfilePaths, Settings, LaunchOptions)>> = const { RefCell::new(None) };
}

/// Run `f` with the state. Returns `None` (and logs) if the state is not
/// initialised or already borrowed further up the stack.
pub fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|cell| match cell.try_borrow_mut() {
        Ok(mut guard) => guard.as_mut().map(f),
        Err(_) => {
            tracing::error!("browser state re-entered; dropping call");
            None
        }
    })
}

/// Run `f` with the state on a fresh UI-thread stack.
pub fn later(f: impl FnOnce(&mut State) + 'static) {
    engine::post_local(move || {
        with_state(f);
    });
}

/// Called in `main` before CEF initialises.
pub fn preload(paths: ProfilePaths, settings: Settings, launch: LaunchOptions) {
    PRELOAD.with(|p| *p.borrow_mut() = Some((paths, settings, launch)));
}

/// Chromium switches derived from settings, applied to the browser process.
pub fn chromium_switches(settings: &Settings) -> Vec<(String, Option<String>)> {
    use ferrite_core::settings::Theme;
    let mut switches = Vec::new();
    if settings.theme == Theme::Dark {
        // Web content sees `prefers-color-scheme: dark`, Chromium UI is dark.
        switches.push(("force-dark-mode".to_string(), None));
    }
    if settings.send_do_not_track {
        // Handled per request in the resource request handler; nothing here.
    }
    switches
}

/// Called from `on_context_initialized` on the UI thread.
pub fn start() {
    let Some((paths, settings, launch)) = PRELOAD.with(|p| p.borrow_mut().take()) else {
        tracing::error!("browser::start called without preload");
        return;
    };
    let db = match Database::open(&paths.database) {
        Ok(db) => db,
        Err(err) => {
            tracing::error!("cannot open profile database ({err:#}); using a temporary one");
            match Database::open_in_memory() {
                Ok(db) => db,
                Err(err) => {
                    tracing::error!("cannot open a temporary database either: {err:#}");
                    return;
                }
            }
        }
    };
    let private_db = match Database::open_in_memory() {
        Ok(db) => db,
        Err(err) => {
            tracing::error!("cannot open the private-mode database: {err:#}");
            return;
        }
    };

    scheme::register(None);

    let state = State {
        paths,
        db,
        private_db,
        settings,
        launch: launch.clone(),
        windows: BTreeMap::new(),
        tabs: HashMap::new(),
        browsers: HashMap::new(),
        content_client: handlers::make_client(handlers::ClientKind::Content),
        ui_client: handlers::make_client(handlers::ClientKind::Ui),
        private_context: None,
        ai_context: None,
        downloads: downloads::Downloads::default(),
        dirty: HashSet::new(),
        flush_scheduled: false,
        session_save_scheduled: false,
        session_frozen: false,
        last_window_focus: None,
        live_browsers: 0,
        passwords: passwords::Store::default(),
    };
    STATE.with(|cell| *cell.borrow_mut() = Some(state));

    if let Some(list) = with_state(|s| s.settings.adblock_exceptions.clone()) {
        handlers::set_exceptions(list);
    }
    handlers::SEND_DNT.store(state_dnt(), std::sync::atomic::Ordering::Relaxed);
    if with_state(|s| s.settings.adblock_enabled).unwrap_or(false) {
        handlers::ADBLOCK.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(dir) = with_state(|s| s.paths.filter_lists.clone()) {
            handlers::start_blocker(dir);
        }
    }
    schedule_housekeeping();
    with_state(|s| {
        updates::init(s);
        passwords::apply_capture_flag(s);
        if s.settings.proton_pass {
            passwords::tick(s);
        }
        theme::apply_to_context(s, None);
        session::restore_or_open(s, &launch);
        mcp::apply_settings(s);
    });
}

fn state_dnt() -> bool {
    with_state(|s| s.settings.send_do_not_track).unwrap_or(false)
}

/// Periodic work: tab sleeping, password cache, update check.
fn schedule_housekeeping() {
    engine::post_local_delayed(60_000, || {
        with_state(tabs::sleep_idle_tabs);
        with_state(passwords::tick);
        with_state(updates::tick);
        schedule_housekeeping();
    });
}

/// Another `ferrite` process was started for this profile.
pub fn relaunch(urls: Vec<String>, private: bool) {
    later(move |s| {
        let urls: Vec<String> = urls.iter().map(|u| s.resolve_input(u)).collect();
        if private {
            window::open_window(s, window::WindowInit::private(urls));
            return;
        }
        let target = s
            .last_window_focus
            .filter(|w| {
                s.windows
                    .get(w)
                    .is_some_and(|w| !w.private && !w.ai && !w.closing)
            })
            .or_else(|| {
                s.windows
                    .values()
                    .find(|w| !w.private && !w.ai && !w.closing)
                    .map(|w| w.id)
            });
        match (target, urls.is_empty()) {
            (Some(win), false) => {
                for url in urls {
                    tabs::new_tab(s, win, &url, tabs::NewTab::foreground());
                }
                s.focus_window(win);
            }
            (Some(win), true) => {
                window::open_window(s, window::WindowInit::normal(vec![]));
                let _ = win;
            }
            (None, _) => {
                window::open_window(s, window::WindowInit::normal(urls));
            }
        }
    });
}

/// Called after the message loop ends, before `shutdown()`.
pub fn before_shutdown() {
    mcp::shutdown();
    with_state(|s| {
        s.downloads.cancel_all();
        session::mark_clean_exit(s);
    });
}

impl State {
    pub fn resolve_input(&self, input: &str) -> String {
        let template = ferrite_core::search::engine_template(&self.settings);
        ferrite_core::omnibox::resolve_input(input, &template)
    }

    /// The database for the window: private and AI windows never touch disk.
    pub fn db_for(&self, win: WinId) -> &Database {
        if self.is_ephemeral(win) {
            &self.private_db
        } else {
            &self.db
        }
    }

    pub fn is_private(&self, win: WinId) -> bool {
        self.windows.get(&win).is_some_and(|w| w.private)
    }

    /// Private or AI window: nothing about it is written to the profile.
    pub fn is_ephemeral(&self, win: WinId) -> bool {
        self.windows.get(&win).is_some_and(|w| w.private || w.ai)
    }

    /// The AI control state of a tab (None for unknown tabs).
    pub fn ai_control(&self, tab: TabId) -> ferrite_mcp::policy::Control {
        self.tabs.get(&tab).map(|t| t.ai).unwrap_or_default()
    }

    pub fn tab_of_browser(&self, browser_id: i32) -> Option<TabId> {
        match self.browsers.get(&browser_id) {
            Some(BrowserRef::Tab(t)) => Some(*t),
            _ => None,
        }
    }

    /// Window for any browser: a tab's window or the window whose UI it is.
    pub fn win_of_browser(&self, browser_id: i32) -> Option<WinId> {
        match self.browsers.get(&browser_id)? {
            BrowserRef::Tab(t) => self.tabs.get(t).map(|t| t.win),
            BrowserRef::Ui(w) => Some(*w),
        }
    }

    pub fn active_tab(&self, win: WinId) -> Option<TabId> {
        self.windows.get(&win).and_then(|w| w.active)
    }

    pub fn focus_window(&mut self, win: WinId) {
        if let Some(window) = self.windows.get(&win).and_then(|w| w.window.clone()) {
            window.show();
            window.activate();
        }
    }

    /// Queue a UI state push for `win`.
    pub fn mark_dirty(&mut self, win: WinId) {
        self.dirty.insert(win);
        if !self.flush_scheduled {
            self.flush_scheduled = true;
            later(|s| {
                s.flush_scheduled = false;
                s.flush_ui();
            });
        }
    }

    pub fn mark_all_dirty(&mut self) {
        let ids: Vec<WinId> = self.windows.keys().copied().collect();
        for id in ids {
            self.mark_dirty(id);
        }
    }

    fn flush_ui(&mut self) {
        let dirty: Vec<WinId> = self.dirty.drain().collect();
        for win in dirty {
            let Some(w) = self.windows.get(&win) else {
                continue;
            };
            if !w.ui_ready {
                continue;
            }
            let snapshot = api::window_snapshot(self, win);
            self.ui_call(win, "onState", &snapshot);
        }
    }

    /// Call `window.ferrite.<method>(arg)` in the window's UI page.
    pub fn ui_call(&self, win: WinId, method: &str, arg: &serde_json::Value) {
        let Some(frame) = self.windows.get(&win).and_then(|w| w.ui_frame()) else {
            return;
        };
        // JSON is a valid JavaScript expression; method names are ours.
        let code = format!("window.ferrite && window.ferrite.{method}({arg});");
        frame.execute_java_script(Some(&cef_str(&code)), Some(&cef_str("")), 0);
        if let Some(side) = self
            .windows
            .get(&win)
            .and_then(|w| w.side_browser.as_ref())
            .and_then(|b| b.main_frame())
        {
            side.execute_java_script(Some(&cef_str(&code)), Some(&cef_str("")), 0);
        }
    }

    pub fn schedule_session_save(&mut self) {
        if self.session_save_scheduled {
            return;
        }
        self.session_save_scheduled = true;
        engine::post_local_delayed(1000, || {
            with_state(|s| {
                s.session_save_scheduled = false;
                session::save(s, false);
            });
        });
    }

    pub fn save_settings(&mut self) {
        if let Err(err) = self.settings.save(&self.paths.settings) {
            tracing::warn!("saving settings failed: {err:#}");
        }
    }

    /// Request context for new browsers (None = global/profile).
    /// With the AI window's own in-memory context when `ai` (never shared
    /// with private windows).
    pub fn context_for_kind(&mut self, private: bool, ai: bool) -> Option<RequestContext> {
        if ai && !private {
            if self.ai_context.is_none() {
                let settings = RequestContextSettings {
                    persist_session_cookies: 0,
                    ..Default::default()
                };
                let ctx = request_context_create_context(Some(&settings), None);
                if let Some(ctx) = &ctx {
                    scheme::register(Some(ctx));
                    theme::apply_to_context(self, Some(ctx));
                }
                self.ai_context = ctx;
            }
            return self.ai_context.clone();
        }
        if !private {
            return None;
        }
        if self.private_context.is_none() {
            // Empty cache path = in-memory: cookies, storage and cache vanish
            // when the last private browser closes.
            let settings = RequestContextSettings {
                persist_session_cookies: 0,
                ..Default::default()
            };
            let ctx = request_context_create_context(Some(&settings), None);
            if let Some(ctx) = &ctx {
                scheme::register(Some(ctx));
                theme::apply_to_context(self, Some(ctx));
            }
            self.private_context = ctx;
        }
        self.private_context.clone()
    }

    pub fn browser_closed(&mut self) {
        self.live_browsers = self.live_browsers.saturating_sub(1);
        self.maybe_quit();
    }

    pub fn maybe_quit(&mut self) {
        if self.windows.is_empty() && self.live_browsers == 0 {
            tracing::info!("all windows closed; quitting");
            quit_message_loop();
        }
    }
}
