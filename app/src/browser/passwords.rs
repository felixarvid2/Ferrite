//! Proton Pass through the official `pass-cli` (the browser extension cannot run
//! in CEF). The user logs in once in a terminal; Ferrite only runs the tool.
//!
//! - The CLI always runs on a background thread (argv only, no shell, no stdin,
//!   20 s timeout, output size limit). Results come back to the UI thread.
//! - The cache holds logins without passwords. A password is fetched only when
//!   the user picks a login from the menu, is checked against the page again on
//!   the UI thread, and goes straight into the page's main frame; it is never
//!   stored, logged or sent to a Ferrite UI page.
//!
//! Saving a login (the user signed in on a site): `inject.rs` captures the
//! submitted username and password in an isolated world and reports them
//! here. The password stays in Rust memory in a [`SavePrompt`] until the user
//! answers (or 2 minutes pass, or the tab closes); the UI strip only gets
//! `{id, site, username}`. It is written with `pass-cli item create login
//! --from-template -`, the JSON going through stdin (argv is world-readable
//! in /proc). Not offered in private windows or for sites on the never list.
//!
//! Updating a password: when the cached login for the site and username
//! already exists, its stored password is fetched in the background (as for a
//! fill) and compared in Rust; the same password offers nothing, a different
//! one shows "Update password for <username> on <site>?". `item update` only
//! takes values in argv (visible in /proc), so Update recreates the item: it
//! reads `item view --output json`, creates a copy with the new password via
//! stdin, and only then moves the old item to the trash (the item id changes).
//! Items with data the create template cannot carry (note, attachments,
//! passkeys, custom sections) are never touched; the user is told to use the
//! Proton Pass app.
//!
//! 2FA: for logins with a TOTP secret the menu has an extra "2FA code" item;
//! the code (not the secret) is fetched with `pass-cli item totp`, checked
//! against the page again, and typed into the first OTP-looking input or,
//! failing that, copied to the clipboard.

use super::{later, State, TabId, WinId};
use crate::engine::{self, cef_str, from_userfree};
use cef::*;
use ferrite_core::passcli::{self, Login, Vault};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(5 * 60);
const CLI_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_STDOUT: usize = 32 * 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;
const MAX_MENU_ENTRIES: usize = 30;
/// Menu command id of the first "2FA code" item (password items are 1..).
const TOTP_BASE: i32 = 1000;
/// How long an unanswered "save password?" prompt (and its password) lives.
const PROMPT_TTL_MS: i64 = 2 * 60 * 1000;
const MAX_PROMPTS: usize = 8;
/// The clipboard copy of a 2FA code is cleared after this long (if unchanged).
const CLIPBOARD_CLEAR_MS: i64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Disabled,
    NotInstalled,
    Loading,
    Error(String),
    Ready,
}

#[derive(Default)]
struct CountMemo {
    url: String,
    version: u64,
    count: usize,
}

/// What a [`SavePrompt`] asks.
enum PromptKind {
    /// "Save password for <site>?" (no cached login for the username).
    Save,
    /// A cached login has this username: its password is being fetched to see
    /// whether the new one differs. Not shown.
    Checking,
    /// "Update password for <username> on <site>?": the password differs.
    Update(Login),
    /// The login has data Ferrite cannot copy; only an OK button. The
    /// password is already dropped.
    Blocked,
}

/// A login captured on a page, waiting for the user's answer. Holds the
/// password: never log it, never send it to a page or UI.
struct SavePrompt {
    kind: PromptKind,
    id: String,
    tab: TabId,
    /// Site key (host without "www.") the login was submitted on.
    site: String,
    /// Origin of the page (`https://host[:port]`), saved as the item's URL.
    origin: String,
    username: String,
    password: String,
}

pub struct Store {
    logins: Vec<Login>,
    vaults: Vec<Vault>,
    save_prompts: Vec<SavePrompt>,
    prompt_seq: u64,
    status: Status,
    /// When the last refresh finished (successfully or not).
    fetched: Option<Instant>,
    refreshing: bool,
    /// Bumped on every settings change so results of older runs are dropped.
    generation: u64,
    /// Bumped whenever `logins` is replaced (invalidates the count memo).
    version: u64,
    memo: RefCell<CountMemo>,
}

impl Default for Store {
    fn default() -> Self {
        Store {
            logins: Vec::new(),
            vaults: Vec::new(),
            save_prompts: Vec::new(),
            prompt_seq: 0,
            status: Status::Disabled,
            fetched: None,
            refreshing: false,
            generation: 0,
            version: 1,
            memo: RefCell::new(CountMemo::default()),
        }
    }
}

impl Store {
    /// Number of cached logins that apply to `url` (0 when disabled).
    pub fn count_for(&self, url: &str) -> usize {
        if self.status != Status::Ready || self.logins.is_empty() {
            return 0;
        }
        let Ok(mut memo) = self.memo.try_borrow_mut() else {
            return 0;
        };
        if memo.version != self.version || memo.url != url {
            memo.count = self
                .logins
                .iter()
                .filter(|l| passcli::matches(l, url))
                .count();
            memo.url = url.to_string();
            memo.version = self.version;
        }
        memo.count
    }

    fn matching(&self, url: &str) -> Vec<Login> {
        self.logins
            .iter()
            .filter(|l| passcli::matches(l, url))
            .take(MAX_MENU_ENTRIES)
            .cloned()
            .collect()
    }
}

/// `pass-cli` to run: the configured path/name, else PATH, else ~/.local/bin.
pub fn resolve_cli(setting: &str) -> Option<PathBuf> {
    let setting = setting.trim();
    let in_path = |name: &str| {
        std::env::var_os("PATH").and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join(name))
                .find(|c| c.is_file())
        })
    };
    if !setting.is_empty() {
        return if setting.contains('/') {
            let p = PathBuf::from(setting);
            p.is_file().then_some(p)
        } else {
            in_path(setting)
        };
    }
    in_path("pass-cli").or_else(|| {
        let p = PathBuf::from(std::env::var_os("HOME")?).join(".local/bin/pass-cli");
        p.is_file().then_some(p)
    })
}

// ------------------------------------------------------------ running the CLI

fn read_limited<R: Read + Send + 'static>(
    r: R,
    limit: usize,
) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.take(limit as u64 + 1).read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    rx
}

/// Run `cli args…` and return stdout. Errors carry the first stderr line.
fn run_cli(cli: &PathBuf, args: &[&str]) -> Result<String, String> {
    run_cli_with_stdin(cli, args, None)
}

/// Like [`run_cli`], feeding `input` to stdin (written on its own thread so a
/// CLI that does not read it cannot block us; the timeout still applies).
fn run_cli_with_stdin(
    cli: &PathBuf,
    args: &[&str],
    input: Option<Vec<u8>>,
) -> Result<String, String> {
    let mut child = Command::new(cli)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start pass-cli: {e}"))?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
            // Dropping stdin closes it: the CLI sees end of input.
        });
    }
    let (Some(out), Some(err)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("cannot read pass-cli output".into());
    };
    let out_rx = read_limited(out, MAX_STDOUT);
    let err_rx = read_limited(err, MAX_STDERR);
    let deadline = Instant::now() + CLI_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("pass-cli timed out".into());
            }
            Err(e) => return Err(format!("pass-cli failed: {e}")),
        }
    };
    let stdout = out_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default();
    let stderr = err_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default();
    if !status.success() {
        let line = String::from_utf8_lossy(&stderr)
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(|l| l.chars().take(200).collect::<String>())
            .unwrap_or_else(|| format!("pass-cli exited with {status}"));
        return Err(line);
    }
    if stdout.len() > MAX_STDOUT {
        return Err("pass-cli output too large".into());
    }
    String::from_utf8(stdout).map_err(|_| "pass-cli output is not UTF-8".to_string())
}

type Fetched = (Vec<Login>, Vec<Vault>);

fn fetch_logins(cli: &PathBuf) -> Result<Fetched, String> {
    let vaults = run_cli(cli, &["vault", "list", "--output", "json"])?;
    let vault_infos = passcli::parse_vault_infos(&vaults);
    let shares: Vec<&str> = vault_infos.iter().map(|v| v.share_id.as_str()).collect();
    let mut logins = Vec::new();
    let mut first_err = None;
    for share in &shares {
        match run_cli(
            cli,
            &[
                "item",
                "list",
                "--share-id",
                share,
                "--filter-type",
                "login",
                "--output",
                "json",
                "--show-secrets",
            ],
        ) {
            // The raw output holds passwords: parse, then drop it at once.
            Ok(raw) => logins.extend(passcli::parse_logins(&raw)),
            Err(e) => first_err = first_err.or(Some(e)),
        }
    }
    match first_err {
        Some(e) if logins.is_empty() => Err(e),
        _ => Ok((logins, vault_infos)),
    }
}

fn fetch_password(cli: &PathBuf, login: &Login) -> Result<String, String> {
    let uri = format!("pass://{}/{}/password", login.share_id, login.item_id);
    let mut out = run_cli(cli, &["item", "view", &uri, "--output", "human"])?;
    if out.ends_with('\n') {
        out.pop();
        if out.ends_with('\r') {
            out.pop();
        }
    }
    if out.is_empty() {
        return Err("empty password".into());
    }
    Ok(out)
}

// ------------------------------------------------------------ cache refresh

/// Start a background refresh unless one is running (or the cache is fresh and
/// `force` is false).
pub fn refresh(s: &mut State, force: bool) {
    if !s.settings.proton_pass {
        return;
    }
    let store = &mut s.passwords;
    if store.refreshing {
        return;
    }
    if !force && store.fetched.is_some_and(|t| t.elapsed() < TTL) {
        return;
    }
    let Some(cli) = resolve_cli(&s.settings.proton_pass_cli) else {
        store.status = Status::NotInstalled;
        store.logins.clear();
        store.version += 1;
        store.fetched = Some(Instant::now());
        s.mark_all_dirty();
        return;
    };
    store.refreshing = true;
    if store.logins.is_empty() {
        store.status = Status::Loading;
    }
    let generation = store.generation;
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| fetch_logins(&cli))
            .unwrap_or_else(|_| Err("internal error".into()));
        engine::post_ui(move || {
            super::with_state(|s| finish_refresh(s, generation, result));
        });
    });
    s.mark_all_dirty();
}

fn finish_refresh(s: &mut State, generation: u64, result: Result<Fetched, String>) {
    let store = &mut s.passwords;
    if store.generation != generation {
        return;
    }
    store.refreshing = false;
    store.fetched = Some(Instant::now());
    match result {
        Ok((logins, vaults)) => {
            store.logins = logins;
            store.vaults = vaults;
            store.version += 1;
            store.status = Status::Ready;
        }
        Err(e) => {
            store.logins.clear();
            store.vaults.clear();
            store.version += 1;
            store.status = Status::Error(e);
        }
    }
    s.mark_all_dirty();
}

/// Housekeeping: keep the cache fresh while enabled.
pub fn tick(s: &mut State) {
    refresh(s, false);
}

/// Settings were saved: apply an enable/disable or path change.
pub fn settings_changed(s: &mut State, was_enabled: bool, old_cli: &str) {
    apply_capture_flag(s);
    let enabled = s.settings.proton_pass;
    if enabled == was_enabled && old_cli == s.settings.proton_pass_cli {
        return;
    }
    let store = &mut s.passwords;
    store.generation += 1;
    store.refreshing = false;
    store.fetched = None;
    store.logins.clear();
    store.version += 1;
    store.status = if enabled {
        Status::Loading
    } else {
        Status::Disabled
    };
    if enabled {
        refresh(s, true);
    }
    s.mark_all_dirty();
}

/// Login capture runs while Proton Pass is on and "offer to save" is on.
/// Switching it off removes the page script and drops pending prompts.
pub fn apply_capture_flag(s: &mut State) {
    use std::sync::atomic::Ordering::Relaxed;
    let want = s.settings.proton_pass && s.settings.proton_pass_offer_save;
    let was = super::inject::CAPTURE.swap(want, Relaxed);
    if want && !was {
        // Cover the pages tabs are showing now: register on every browser.
        let browsers: Vec<Browser> = s.tabs.values().filter_map(|t| t.browser.clone()).collect();
        later(move |_| {
            for mut b in browsers {
                super::inject::capture_ensure(&mut b);
            }
        });
    }
    if !want {
        s.passwords.save_prompts.clear();
        if was {
            let browsers: HashMap<i32, Browser> = s
                .tabs
                .values()
                .filter_map(|t| t.browser.clone())
                .map(|b| (b.identifier(), b))
                .collect();
            super::inject::capture_clear_all(|id| browsers.get(&id).cloned());
        }
        s.mark_all_dirty();
    }
}

/// Vaults for the settings page (filled by the last refresh).
pub fn vaults_json(s: &State) -> Value {
    json!(s.passwords.vaults)
}

pub fn status_json(s: &State) -> Value {
    let enabled = s.settings.proton_pass;
    let path = enabled
        .then(|| resolve_cli(&s.settings.proton_pass_cli))
        .flatten()
        .map(|p| p.to_string_lossy().into_owned());
    let (status, message) = match &s.passwords.status {
        Status::Disabled => ("disabled", String::new()),
        Status::NotInstalled => ("not_installed", String::new()),
        Status::Loading => ("loading", String::new()),
        Status::Error(e) => ("error", e.clone()),
        Status::Ready => ("ready", String::new()),
    };
    json!({
        "enabled": enabled,
        "path": path,
        "status": status,
        "message": message,
        "count": s.passwords.logins.len(),
    })
}

// ------------------------------------------------------------ menu and fill

fn menu_label(l: &Login) -> String {
    let text = match (l.username.is_empty(), l.title.is_empty()) {
        (false, false) => format!("{} \u{2014} {}", l.username, l.title),
        (false, true) => l.username.clone(),
        (true, false) => l.title.clone(),
        (true, true) => l.urls.first().cloned().unwrap_or_default(),
    };
    let text: String = text.chars().filter(|c| !c.is_control()).take(80).collect();
    // '&' marks a mnemonic in native menus.
    text.replace('&', "&&")
}

fn totp_label(l: &Login) -> String {
    let who = if l.username.is_empty() {
        &l.title
    } else {
        &l.username
    };
    let who: String = who.chars().filter(|c| !c.is_control()).take(60).collect();
    format!("2FA code \u{2014} {}", who.replace('&', "&&"))
}

pub fn show_menu(s: &State, win: WinId, x: i32, y: i32) {
    if !s.settings.proton_pass {
        return;
    }
    let Some(w) = s.windows.get(&win) else { return };
    let Some(window) = w.window.clone() else {
        return;
    };
    let Some(tab) = w.active else { return };
    let Some(url) = s.tabs.get(&tab).map(|t| t.url.clone()) else {
        return;
    };
    let entries = s.passwords.matching(&url);
    if entries.is_empty() {
        return;
    }
    let mut delegate = PassMenuDelegate::new(win, tab, entries.clone());
    let Some(mut model) = menu_model_create(Some(&mut delegate)) else {
        return;
    };
    for (i, l) in entries.iter().enumerate() {
        model.add_item(i as i32 + 1, Some(&cef_str(&menu_label(l))));
    }
    let mut separated = false;
    for (i, l) in entries.iter().enumerate().filter(|(_, l)| l.has_totp) {
        if !std::mem::replace(&mut separated, true) {
            model.add_separator();
        }
        model.add_item(TOTP_BASE + i as i32, Some(&cef_str(&totp_label(l))));
    }
    let mut point = Point { x, y };
    w.ui_view.convert_point_to_screen(Some(&mut point));
    window.show_menu(Some(&mut model), Some(&point), MenuAnchorPosition::TOPRIGHT);
}

wrap_menu_model_delegate! {
    struct PassMenuDelegate {
        win: WinId,
        tab: TabId,
        entries: Vec<Login>,
    }

    impl MenuModelDelegate {
        fn execute_command(&self, _menu_model: Option<&mut MenuModel>, command_id: ::std::os::raw::c_int, _event_flags: EventFlags) {
            let totp = command_id >= TOTP_BASE;
            let index = if totp { command_id - TOTP_BASE } else { command_id - 1 };
            let Some(login) = usize::try_from(index).ok().and_then(|i| self.entries.get(i)).cloned() else {
                return;
            };
            let (win, tab) = (self.win, self.tab);
            if totp {
                later(move |s| start_totp(s, win, tab, login));
            } else {
                later(move |s| start_fill(s, win, tab, login));
            }
        }
    }
}

fn toast(s: &State, win: WinId, msg: &str) {
    s.ui_call(win, "toast", &json!(msg));
}

/// The tab's browser id and main-frame URL, if it has a live main frame.
fn tab_target(s: &State, tab: TabId) -> Option<(i32, String)> {
    let t = s.tabs.get(&tab)?;
    let b = t.browser.as_ref()?;
    Some((b.identifier(), from_userfree(&b.main_frame()?.url())))
}

/// Checks shared by fill and 2FA: feature on, ids sane, the login still
/// matches the page of `tab` (in `win`). Returns the CLI and the browser id.
fn precheck(s: &State, win: WinId, tab: TabId, login: &Login) -> Option<(PathBuf, i32)> {
    if !s.settings.proton_pass
        || !passcli::valid_id(&login.share_id)
        || !passcli::valid_id(&login.item_id)
    {
        return None;
    }
    let (browser_id, url) = tab_target(s, tab)?;
    if s.tabs.get(&tab).map(|t| t.win) != Some(win) || !passcli::matches(login, &url) {
        toast(s, win, "This login does not match the page");
        return None;
    }
    let Some(cli) = resolve_cli(&s.settings.proton_pass_cli) else {
        toast(s, win, "pass-cli not found");
        return None;
    };
    Some((cli, browser_id))
}

fn start_fill(s: &mut State, win: WinId, tab: TabId, login: Login) {
    let Some((cli, browser_id)) = precheck(s, win, tab, &login) else {
        return;
    };
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| fetch_password(&cli, &login))
            .unwrap_or_else(|_| Err("internal error".into()));
        engine::post_ui(move || {
            super::with_state(|s| finish_fill(s, win, tab, browser_id, login, result));
        });
    });
}

fn finish_fill(
    s: &mut State,
    win: WinId,
    tab: TabId,
    browser_id: i32,
    login: Login,
    result: Result<String, String>,
) {
    let password = match result {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Proton Pass: fetching a password failed: {e}");
            toast(s, win, "Proton Pass: could not get the password");
            return;
        }
    };
    // The page may have navigated while pass-cli ran: check again.
    let frame = match tab_target(s, tab) {
        Some((id, url)) if id == browser_id && passcli::matches(&login, &url) => s
            .tabs
            .get(&tab)
            .and_then(|t| t.browser.as_ref())
            .and_then(|b| b.main_frame()),
        _ => None,
    };
    let Some(frame) = frame else {
        toast(s, win, "Page changed; password not filled");
        return;
    };
    let script = fill_script(&login.username, &password);
    drop(password);
    // Outside the state borrow (CEF may call back synchronously).
    later(move |_| {
        frame.execute_java_script(Some(&cef_str(&script)), Some(&cef_str("")), 0);
    });
}

/// Page script that fills the first visible password field and its username
/// field. Values are spliced as JSON string literals only.
pub fn fill_script(username: &str, password: &str) -> String {
    const BODY: &str = r#"(function (user, pass) {
  function vis(e) {
    if (!e || e.disabled || e.readOnly) return false;
    var r = e.getBoundingClientRect(), cs = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none';
  }
  function setv(e, v) {
    var d = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
    d.set.call(e, v);
    e.dispatchEvent(new Event('input', { bubbles: true }));
    e.dispatchEvent(new Event('change', { bubbles: true }));
  }
  var all = [].slice.call(document.querySelectorAll('input'));
  var pw = all.filter(function (e) { return e.type === 'password'; }).find(vis);
  if (!pw) return;
  var scope = pw.form ? [].slice.call(pw.form.querySelectorAll('input')) : all;
  var inputs = scope.filter(vis);
  var text = ['text', 'email', 'tel'];
  var ue = inputs.find(function (e) {
    var a = (e.getAttribute('autocomplete') || '').toLowerCase().split(/\s+/);
    return e !== pw && text.indexOf((e.type || 'text').toLowerCase()) >= 0 && (a.indexOf('username') >= 0 || a.indexOf('email') >= 0);
  });
  if (!ue) {
    for (var i = inputs.indexOf(pw) - 1; i >= 0; i--) {
      if (text.indexOf((inputs[i].type || 'text').toLowerCase()) >= 0) { ue = inputs[i]; break; }
    }
  }
  if (user && ue) setv(ue, user);
  setv(pw, pass);
})"#;
    format!(
        "{BODY}({}, {});",
        serde_json::to_string(username).unwrap_or_else(|_| "\"\"".into()),
        serde_json::to_string(password).unwrap_or_else(|_| "\"\"".into())
    )
}

// ------------------------------------------------------------ 2FA codes

fn fetch_totp(cli: &PathBuf, login: &Login) -> Result<String, String> {
    let out = run_cli(
        cli,
        &[
            "item",
            "totp",
            "--share-id",
            &login.share_id,
            "--item-id",
            &login.item_id,
            "--output",
            "json",
        ],
    )?;
    passcli::parse_totp(&out).ok_or_else(|| "no 2FA code".to_string())
}

fn start_totp(s: &mut State, win: WinId, tab: TabId, login: Login) {
    let Some((cli, browser_id)) = precheck(s, win, tab, &login) else {
        return;
    };
    if !login.has_totp {
        return;
    }
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| fetch_totp(&cli, &login))
            .unwrap_or_else(|_| Err("internal error".into()));
        engine::post_ui(move || {
            super::with_state(|s| finish_totp(s, win, tab, browser_id, login, result));
        });
    });
}

fn finish_totp(
    s: &mut State,
    win: WinId,
    tab: TabId,
    browser_id: i32,
    login: Login,
    result: Result<String, String>,
) {
    let code = match result {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Proton Pass: fetching a 2FA code failed: {e}");
            toast(s, win, "Proton Pass: could not get the 2FA code");
            return;
        }
    };
    let browser = match tab_target(s, tab) {
        Some((id, url)) if id == browser_id && passcli::matches(&login, &url) => {
            s.tabs.get(&tab).and_then(|t| t.browser.clone())
        }
        _ => None,
    };
    let Some(browser) = browser else {
        toast(s, win, "Page changed; 2FA code not filled");
        return;
    };
    // Outside the state borrow (DevTools calls may call back synchronously).
    later(move |_| {
        let mut browser = browser;
        let job = OtpJob {
            win,
            code: code.clone(),
        };
        match super::inject::evaluate(&mut browser, &otp_script(&code)) {
            Some(id) => {
                EVALS.with(|e| e.borrow_mut().insert(id, job));
                // No answer from DevTools: treat as "not filled".
                engine::post_local_delayed(5000, move || {
                    if let Some(job) = EVALS.with(|e| e.borrow_mut().remove(&id)) {
                        later(move |s| otp_done(s, job, false));
                    }
                });
            }
            None => later(move |s| otp_done(s, job, false)),
        }
    });
}

/// A 2FA code waiting for the page script's verdict.
struct OtpJob {
    win: WinId,
    code: String,
}

thread_local! {
    /// DevTools message id of the OTP script -> job.
    static EVALS: RefCell<HashMap<i32, OtpJob>> = RefCell::new(HashMap::new());
}

/// A DevTools method result arrived: if it answers an OTP script, handle it
/// and return true.
pub fn eval_result(message_id: i32, success: bool, result: &[u8]) -> bool {
    let Some(job) = EVALS.with(|e| e.borrow_mut().remove(&message_id)) else {
        return false;
    };
    let filled = success
        && serde_json::from_slice::<Value>(result)
            .ok()
            .and_then(|v| v.pointer("/result/value").and_then(Value::as_bool))
            == Some(true);
    later(move |s| otp_done(s, job, filled));
    true
}

fn otp_done(s: &mut State, job: OtpJob, filled: bool) {
    if filled {
        toast(s, job.win, "2FA code filled");
        return;
    }
    // No OTP input found: the user asked for the code, so copy it. It is
    // removed from the clipboard again after a while if still there.
    super::context_menu::copy_text(&job.code);
    let code = job.code;
    engine::post_local_delayed(CLIPBOARD_CLEAR_MS, move || {
        super::context_menu::clear_text_if(&code);
    });
    toast(s, job.win, "2FA code copied");
}

/// Page expression (main world) that types `code` into the first visible
/// input that looks like a one-time-code field and returns whether it did.
/// The code is validated alphanumeric and spliced as a JSON string literal.
pub fn otp_script(code: &str) -> String {
    const BODY: &str = r#"(function (code) {
  function vis(e) {
    var r = e.getBoundingClientRect(), cs = getComputedStyle(e);
    return !e.disabled && !e.readOnly && r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none';
  }
  function setv(e, v) {
    var d = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
    d.set.call(e, v);
    e.dispatchEvent(new Event('input', { bubbles: true }));
    e.dispatchEvent(new Event('change', { bubbles: true }));
  }
  function ml(e) { var m = parseInt(e.getAttribute('maxlength') || '', 10); return isNaN(m) ? 0 : m; }
  var types = ['text', 'tel', 'number', ''];
  var cand = [].slice.call(document.querySelectorAll('input')).filter(function (e) {
    return types.indexOf((e.getAttribute('type') || '').toLowerCase()) >= 0 && vis(e) && (ml(e) === 0 || ml(e) >= code.length);
  });
  var name = /otp|totp|2fa|mfa|one.?time|verif|security.?code|auth.?code|authenticator|\bcode\b|token/i;
  var not = /zip|post|promo|coupon|country|area|search|captcha/i;
  var pick = cand.find(function (e) { return (e.getAttribute('autocomplete') || '').toLowerCase().split(/\s+/).indexOf('one-time-code') >= 0; })
    || cand.find(function (e) {
      var t = [e.name, e.id, e.getAttribute('aria-label'), e.placeholder].join(' ');
      return name.test(t) && !not.test(t);
    })
    || cand.find(function (e) {
      var im = (e.getAttribute('inputmode') || '').toLowerCase(), m = ml(e);
      return (im === 'numeric' || im === 'decimal') && m >= 6 && m <= 8;
    });
  if (!pick) return false;
  pick.focus();
  setv(pick, code);
  return true;
})"#;
    let code = if code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        code
    } else {
        ""
    };
    format!(
        "{BODY}({});",
        serde_json::to_string(code).unwrap_or_else(|_| "\"\"".into())
    )
}

// ------------------------------------------------------------ saving logins

fn random_id(seq: u64) -> String {
    use std::hash::{BuildHasher, Hasher};
    // RandomState is seeded randomly per process: unguessable ids.
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(seq);
    h.write_i64(ferrite_core::now_ms());
    format!("{:016x}", h.finish())
}

fn expire_prompt(id: String) {
    engine::post_local_delayed(PROMPT_TTL_MS, move || {
        later(move |s| {
            let before = s.passwords.save_prompts.len();
            s.passwords.save_prompts.retain(|p| p.id != id);
            if s.passwords.save_prompts.len() != before {
                s.mark_all_dirty();
            }
        });
    });
}

/// A login was submitted in the main frame of `browser_id` whose URL was
/// `url` (read by the Rust side, not taken from the page). `payload` is the
/// untrusted JSON from the capture script.
pub fn capture(s: &mut State, browser_id: i32, url: String, payload: String) {
    if !s.settings.proton_pass
        || !s.settings.proton_pass_offer_save
        || s.passwords.status != Status::Ready
    {
        tracing::debug!("login capture ignored: Proton Pass off or not ready");
        return;
    }
    let Some(tab) = s.tab_of_browser(browser_id) else {
        tracing::debug!("login capture ignored: not a tab");
        return;
    };
    let Some(win) = s.tabs.get(&tab).map(|t| t.win) else {
        return;
    };
    // Private windows never offer to save.
    if s.is_private(win) {
        tracing::debug!("login capture ignored: private window");
        return;
    }
    let Some(site) = ferrite_core::blocker::site_key(&url) else {
        return; // not http(s)
    };
    if ferrite_core::blocker::is_exception(&s.settings.proton_pass_never_save, &url) {
        tracing::debug!("login capture ignored: site is on the never list");
        return;
    }
    #[derive(serde::Deserialize)]
    struct Payload {
        username: String,
        password: String,
    }
    let Ok(p) = serde_json::from_str::<Payload>(&payload) else {
        return;
    };
    drop(payload);
    let username = p.username.trim().to_string();
    if p.password.is_empty()
        || p.password.chars().count() > 1000
        || username.chars().count() > 500
        || username.chars().any(char::is_control)
    {
        tracing::debug!("login capture ignored: empty or oversized values");
        return;
    }
    let existing = passcli::existing_login(&s.passwords.logins, &url, &username).cloned();
    let Ok(parsed) = url::Url::parse(&url) else {
        return;
    };
    let origin = parsed.origin().ascii_serialization();
    let store = &mut s.passwords;
    // One prompt per tab: a newer submit replaces the older.
    store.save_prompts.retain(|x| x.tab != tab);
    while store.save_prompts.len() >= MAX_PROMPTS {
        store.save_prompts.remove(0);
    }
    store.prompt_seq += 1;
    let id = random_id(store.prompt_seq);
    let password = p.password;
    let compare = existing.is_some().then(|| password.clone());
    store.save_prompts.push(SavePrompt {
        kind: if existing.is_some() {
            PromptKind::Checking
        } else {
            PromptKind::Save
        },
        id: id.clone(),
        tab,
        site,
        origin,
        username,
        password,
    });
    expire_prompt(id.clone());
    if let (Some(login), Some(new_password)) = (existing, compare) {
        start_check(s, id, login, new_password);
    }
    s.mark_dirty(win);
}

/// A cached login matches the submitted site and username: fetch its stored
/// password in the background and compare. Passwords are not cached, so this
/// is the only way to know whether an update is needed. Only the verdict
/// comes back to the UI thread.
fn start_check(s: &mut State, id: String, login: Login, new_password: String) {
    let cli = (passcli::valid_id(&login.share_id) && passcli::valid_id(&login.item_id))
        .then(|| resolve_cli(&s.settings.proton_pass_cli))
        .flatten();
    let Some(cli) = cli else {
        s.passwords.save_prompts.retain(|p| p.id != id);
        return;
    };
    std::thread::spawn(move || {
        let differs = std::panic::catch_unwind(|| {
            fetch_password(&cli, &login).map(|stored| stored != new_password)
        })
        .unwrap_or_else(|_| Err("internal error".into()));
        drop(new_password);
        engine::post_ui(move || {
            super::with_state(|s| finish_check(s, &id, login, differs));
        });
    });
}

fn finish_check(s: &mut State, id: &str, login: Login, differs: Result<bool, String>) {
    let store = &mut s.passwords;
    let Some(i) = store
        .save_prompts
        .iter()
        .position(|p| p.id == id && matches!(p.kind, PromptKind::Checking))
    else {
        return; // answered, expired or replaced meanwhile
    };
    match differs {
        Ok(true) => {
            store.save_prompts[i].kind = PromptKind::Update(login);
            if let Some(win) = s.tabs.get(&s.passwords.save_prompts[i].tab).map(|t| t.win) {
                s.mark_dirty(win);
            }
        }
        Ok(false) => {
            tracing::debug!("login capture: password unchanged");
            store.save_prompts.remove(i);
        }
        Err(e) => {
            tracing::debug!("login capture: could not compare passwords: {e}");
            store.save_prompts.remove(i);
        }
    }
}

/// The prompt to show in the UI strip for `tab` (no password).
pub fn prompt_json(s: &State, tab: Option<TabId>) -> Value {
    let Some(tab) = tab else { return Value::Null };
    let Some(p) = s.passwords.save_prompts.iter().find(|p| p.tab == tab) else {
        return Value::Null;
    };
    let kind = match p.kind {
        PromptKind::Save => "save",
        PromptKind::Update(_) => "update",
        PromptKind::Blocked => "blocked",
        PromptKind::Checking => return Value::Null,
    };
    // The stored login's spelling of the name, not what was typed.
    let username = match &p.kind {
        PromptKind::Update(l) => &l.username,
        _ => &p.username,
    };
    json!({ "id": p.id, "kind": kind, "site": p.site, "username": username })
}

pub fn tab_closed(s: &mut State, tab: TabId) {
    s.passwords.save_prompts.retain(|p| p.tab != tab);
}

/// The user answered a prompt from window `win`: `save`, `never` or `dismiss`
/// for a save prompt, `update` or `dismiss` for an update prompt, `dismiss`
/// for the "cannot copy" notice.
pub fn save_answer(s: &mut State, win: WinId, id: &str, action: &str) -> anyhow::Result<()> {
    let Some(i) = s
        .passwords
        .save_prompts
        .iter()
        .position(|p| p.id == id && s.tabs.get(&p.tab).map(|t| t.win) == Some(win))
    else {
        return Ok(()); // expired or not ours
    };
    let allowed: &[&str] = match s.passwords.save_prompts[i].kind {
        PromptKind::Save => &["save", "never", "dismiss"],
        PromptKind::Update(_) => &["update", "dismiss"],
        PromptKind::Blocked => &["dismiss"],
        PromptKind::Checking => return Ok(()), // not shown yet
    };
    if !allowed.contains(&action) {
        anyhow::bail!("unknown action");
    }
    // The prompt (and its password) is dropped on every answer.
    let prompt = s.passwords.save_prompts.remove(i);
    s.mark_dirty(win);
    match action {
        "dismiss" => {}
        "never" => {
            let list = &mut s.settings.proton_pass_never_save;
            if !list.contains(&prompt.site) {
                list.push(prompt.site.clone());
                list.sort();
            }
            s.save_settings();
            let site = prompt.site.clone();
            s.passwords.save_prompts.retain(|p| p.site != site);
            toast(
                s,
                win,
                &format!("Will not ask to save passwords for {site}"),
            );
        }
        "save" => start_save(s, win, prompt),
        "update" => {
            if let PromptKind::Update(login) = prompt.kind {
                start_update(s, win, prompt.tab, login, prompt.password);
            }
        }
        _ => {}
    }
    Ok(())
}

fn start_save(s: &mut State, win: WinId, prompt: SavePrompt) {
    if !s.settings.proton_pass {
        return;
    }
    let Some(cli) = resolve_cli(&s.settings.proton_pass_cli) else {
        toast(s, win, "pass-cli not found");
        return;
    };
    let vault = s.settings.proton_pass_vault.clone();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| save_login(&cli, &vault, &prompt))
            .unwrap_or_else(|_| Err("internal error".into()));
        engine::post_ui(move || {
            super::with_state(|s| finish_save(s, win, result));
        });
    });
}

fn save_login(cli: &PathBuf, vault: &str, p: &SavePrompt) -> Result<(), String> {
    let share = if vault.is_empty() {
        let out = run_cli(cli, &["vault", "list", "--output", "json"])?;
        passcli::parse_vaults(&out)
            .into_iter()
            .next()
            .ok_or("no vault found")?
    } else {
        vault.to_string()
    };
    let template = passcli::login_template(
        &p.site,
        &p.username,
        &p.password,
        std::slice::from_ref(&p.origin),
    );
    run_cli_with_stdin(
        cli,
        &[
            "item",
            "create",
            "login",
            "--share-id",
            &share,
            "--from-template",
            "-",
        ],
        Some(template.into_bytes()),
    )
    .map(|_| ())
    // Defence in depth: a message must never repeat the password.
    .map_err(|e| e.replace(&p.password, "***"))
}

fn finish_save(s: &mut State, win: WinId, result: Result<(), String>) {
    match result {
        Ok(()) => {
            toast(s, win, "Saved to Proton Pass");
            // Reload the cache; results of a refresh already running are stale.
            s.passwords.generation += 1;
            s.passwords.refreshing = false;
            refresh(s, true);
        }
        Err(e) => {
            tracing::warn!("Proton Pass: saving a login failed: {e}");
            toast(s, win, "Proton Pass: could not save the login");
        }
    }
}

/// How replacing a login ended.
enum Replaced {
    Done,
    /// The new item exists but the old one could not be moved to the trash.
    OldKept,
    /// The item has data the template cannot carry: nothing was changed.
    Uncopyable,
}

fn start_update(s: &mut State, win: WinId, tab: TabId, login: Login, password: String) {
    if !s.settings.proton_pass
        || !passcli::valid_id(&login.share_id)
        || !passcli::valid_id(&login.item_id)
    {
        return;
    }
    let Some(cli) = resolve_cli(&s.settings.proton_pass_cli) else {
        toast(s, win, "pass-cli not found");
        return;
    };
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| replace_login(&cli, &login, &password))
            .unwrap_or_else(|_| Err("internal error".into()));
        drop(password);
        engine::post_ui(move || {
            super::with_state(|s| finish_update(s, win, tab, login, result));
        });
    });
}

/// `item update` takes values in argv (visible in /proc), so the login is
/// recreated instead: read the item, create a copy with the new password
/// through stdin, and only after that move the old item to the trash (never
/// deleted). The item id changes.
fn replace_login(cli: &PathBuf, login: &Login, password: &str) -> Result<Replaced, String> {
    let redact = |e: String| e.replace(password, "***");
    let raw = run_cli(
        cli,
        &[
            "item",
            "view",
            "--share-id",
            &login.share_id,
            "--item-id",
            &login.item_id,
            "--output",
            "json",
        ],
    )
    .map_err(redact)?;
    // The raw output holds secrets: parse, then drop it at once.
    let plan = passcli::plan_replacement(&raw, &login.share_id, &login.item_id);
    drop(raw);
    let copy = match plan {
        Some(passcli::Replacement::Copy(c)) => c,
        Some(passcli::Replacement::Uncopyable) => return Ok(Replaced::Uncopyable),
        None => return Err("cannot read the item".into()),
    };
    let template = copy.template(password);
    let mut args = vec![
        "item",
        "create",
        "login",
        "--share-id",
        &login.share_id,
        "--from-template",
        "-",
    ];
    if let Some(folder) = copy.folder_id() {
        args.extend(["--folder-id", folder]);
    }
    run_cli_with_stdin(cli, &args, Some(template.into_bytes())).map_err(redact)?;
    let trashed = run_cli(
        cli,
        &[
            "item",
            "trash",
            "--share-id",
            &login.share_id,
            "--item-id",
            &login.item_id,
        ],
    );
    Ok(match trashed {
        Ok(_) => Replaced::Done,
        Err(e) => {
            tracing::warn!("Proton Pass: could not trash the old item: {}", redact(e));
            Replaced::OldKept
        }
    })
}

fn finish_update(
    s: &mut State,
    win: WinId,
    tab: TabId,
    login: Login,
    result: Result<Replaced, String>,
) {
    match result {
        Ok(Replaced::Done | Replaced::OldKept) => {
            let kept = matches!(result, Ok(Replaced::OldKept));
            toast(
                s,
                win,
                if kept {
                    "Password updated in Proton Pass (the old entry could not be moved to the trash)"
                } else {
                    "Password updated in Proton Pass"
                },
            );
            // The item id changed: reload the cache.
            s.passwords.generation += 1;
            s.passwords.refreshing = false;
            refresh(s, true);
        }
        Ok(Replaced::Uncopyable) => {
            if s.tabs.get(&tab).map(|t| t.win) != Some(win) {
                return;
            }
            let store = &mut s.passwords;
            store.save_prompts.retain(|x| x.tab != tab);
            store.prompt_seq += 1;
            let id = random_id(store.prompt_seq);
            store.save_prompts.push(SavePrompt {
                kind: PromptKind::Blocked,
                id: id.clone(),
                tab,
                site: String::new(),
                origin: String::new(),
                username: login.username,
                password: String::new(),
            });
            expire_prompt(id);
            s.mark_dirty(win);
        }
        Err(e) => {
            tracing::warn!("Proton Pass: updating a login failed: {e}");
            toast(s, win, "Proton Pass: could not update the password");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_escapes_values() {
        let s = fill_script("a\"b</script>", "p'\\\n\u{2028}x");
        assert!(s.ends_with("(\"a\\\"b</script>\", \"p'\\\\\\n\u{2028}x\");"));
        assert!(!s.contains("p'\\\n"));
    }

    #[test]
    fn labels() {
        let l = Login {
            share_id: "s".into(),
            item_id: "i".into(),
            title: "A&B".into(),
            username: "bob".into(),
            urls: vec![],
            has_totp: true,
        };
        assert_eq!(menu_label(&l), "bob \u{2014} A&&B");
        assert_eq!(totp_label(&l), "2FA code \u{2014} bob");
    }

    #[test]
    fn otp_script_splices_only_safe_codes() {
        assert!(otp_script("123456").ends_with("(\"123456\");"));
        assert!(otp_script("1\");alert(1);//").ends_with("(\"\");"));
    }

    #[test]
    fn ids_are_distinct() {
        assert_ne!(random_id(1), random_id(2));
        assert_eq!(random_id(1).len(), 16);
    }
}
