//! Proton Pass through the official `pass-cli` (the browser extension cannot run
//! in CEF). The user logs in once in a terminal; Ferrite only runs the tool.
//!
//! - The CLI always runs on a background thread (argv only, no shell, no stdin,
//!   20 s timeout, output size limit). Results come back to the UI thread.
//! - The cache holds logins without passwords. A password is fetched only when
//!   the user picks a login from the menu, is checked against the page again on
//!   the UI thread, and goes straight into the page's main frame; it is never
//!   stored, logged or sent to a Ferrite UI page.

use super::{later, State, TabId, WinId};
use crate::engine::{self, cef_str, from_userfree};
use cef::*;
use ferrite_core::passcli::{self, Login};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(5 * 60);
const CLI_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_STDOUT: usize = 32 * 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;
const MAX_MENU_ENTRIES: usize = 30;

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

pub struct Store {
    logins: Vec<Login>,
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
    let mut child = Command::new(cli)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start pass-cli: {e}"))?;
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

fn fetch_logins(cli: &PathBuf) -> Result<Vec<Login>, String> {
    let vaults = run_cli(cli, &["vault", "list", "--output", "json"])?;
    let shares = passcli::parse_vaults(&vaults);
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
        _ => Ok(logins),
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

fn finish_refresh(s: &mut State, generation: u64, result: Result<Vec<Login>, String>) {
    let store = &mut s.passwords;
    if store.generation != generation {
        return;
    }
    store.refreshing = false;
    store.fetched = Some(Instant::now());
    match result {
        Ok(logins) => {
            store.logins = logins;
            store.version += 1;
            store.status = Status::Ready;
        }
        Err(e) => {
            store.logins.clear();
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
            let Some(login) = usize::try_from(command_id - 1).ok().and_then(|i| self.entries.get(i)).cloned() else {
                return;
            };
            let (win, tab) = (self.win, self.tab);
            later(move |s| start_fill(s, win, tab, login));
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

fn start_fill(s: &mut State, win: WinId, tab: TabId, login: Login) {
    if !s.settings.proton_pass
        || !passcli::valid_id(&login.share_id)
        || !passcli::valid_id(&login.item_id)
    {
        return;
    }
    let Some((browser_id, url)) = tab_target(s, tab) else {
        return;
    };
    if s.tabs.get(&tab).map(|t| t.win) != Some(win) || !passcli::matches(&login, &url) {
        toast(s, win, "This login does not match the page");
        return;
    }
    let Some(cli) = resolve_cli(&s.settings.proton_pass_cli) else {
        toast(s, win, "pass-cli not found");
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
        };
        assert_eq!(menu_label(&l), "bob \u{2014} A&&B");
    }
}
