//! Update check: asks GitHub once a day whether a newer release exists.
//!
//! Flow: `init` (startup) loads `update.json` and schedules the first check
//! 30 s later; the housekeeping tick re-checks after 24 h. A check runs on a
//! background thread and stores its result here; the ⋮ menu and
//! ferrite://about/ read it (the page polls `update_status`). Nothing is
//! downloaded or installed.
//!
//! `FERRITE_UPDATE_URL` (read once at startup) replaces the GitHub URL. It
//! exists for the end-to-end tests only; loopback `http://` or `https://`.

use super::{with_state, State};
use crate::engine;
use ferrite_core::now_ms;
use ferrite_core::update::{self, Update, UpdateState};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;

const STARTUP_DELAY_MS: i64 = 30_000;

struct Inner {
    url: String,
    path: PathBuf,
    state: UpdateState,
    checking: bool,
    /// The last attempt failed (details are in the log).
    failed: bool,
}

static INNER: Mutex<Option<Inner>> = Mutex::new(None);

fn with_inner<R>(f: impl FnOnce(&mut Inner) -> R) -> Option<R> {
    INNER.lock().ok().and_then(|mut g| g.as_mut().map(f))
}

/// Called once from `browser::start`.
pub fn init(s: &State) {
    let url = match std::env::var("FERRITE_UPDATE_URL") {
        Ok(u) if update::url_allowed(&u) => {
            tracing::info!("update check uses FERRITE_UPDATE_URL (testing)");
            u
        }
        Ok(_) => {
            tracing::warn!("ignoring FERRITE_UPDATE_URL: not https or loopback http");
            update::RELEASES_URL.to_string()
        }
        Err(_) => update::RELEASES_URL.to_string(),
    };
    let path = s.paths.update_state.clone();
    let state = UpdateState::load(&path, env!("CARGO_PKG_VERSION"));
    if let Ok(mut g) = INNER.lock() {
        *g = Some(Inner {
            url,
            path,
            state,
            checking: false,
            failed: false,
        });
    }
    engine::post_local_delayed(STARTUP_DELAY_MS, || {
        with_state(|s| {
            if s.settings.check_for_updates {
                start(s, update::MIN_GAP_MS);
            }
        });
    });
}

/// Housekeeping: re-check after 24 h while the browser keeps running.
pub fn tick(s: &mut State) {
    if s.settings.check_for_updates {
        start(s, update::INTERVAL_MS);
    }
}

/// Start a check on a background thread unless one is running or the last
/// attempt is younger than `gap_ms`.
fn start(_s: &State, gap_ms: i64) -> bool {
    let job = with_inner(|i| {
        if i.checking || !update::is_due(i.state.last_update_check_ms, now_ms(), gap_ms) {
            return None;
        }
        i.checking = true;
        Some(i.url.clone())
    })
    .flatten();
    let Some(url) = job else { return false };
    std::thread::spawn(move || {
        let current = env!("CARGO_PKG_VERSION");
        let result = update::check(&url, current);
        if let Err(err) = &result {
            tracing::warn!("update check failed: {err:#}");
        }
        with_inner(|i| {
            let now = now_ms();
            i.state.last_update_check_ms = now;
            i.checking = false;
            i.failed = result.is_err();
            if let Ok(found) = result {
                i.state.last_ok_ms = now;
                i.state.update = found;
            }
            if let Err(err) = i.state.save(&i.path) {
                tracing::warn!("saving update state failed: {err:#}");
            }
        });
    });
    true
}

/// "Check now": runs regardless of the 6 h rule (but not twice at once).
pub fn check_now(s: &State) -> Value {
    start(s, 0);
    status(s)
}

/// The update to offer, if known and the check is enabled.
pub fn available(s: &State) -> Option<Update> {
    if !s.settings.check_for_updates {
        return None;
    }
    with_inner(|i| i.state.update.clone()).flatten()
}

pub fn status(s: &State) -> Value {
    let snap = with_inner(|i| (i.state.clone(), i.checking, i.failed));
    let (state, checking, failed) = snap.unwrap_or_else(|| (UpdateState::default(), false, false));
    json!({
        "current": env!("CARGO_PKG_VERSION"),
        "enabled": s.settings.check_for_updates,
        "checking": checking,
        "failed": failed,
        "last_checked_ms": state.last_ok_ms,
        "update": state.update,
    })
}
