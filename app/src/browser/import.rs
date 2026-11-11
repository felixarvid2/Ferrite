//! Import bookmarks and history from other browsers (settings page).
//!
//! Flow: `import_run` resolves the source id against a fresh detection under
//! the user's home (the page never supplies a path), reads and parses on a
//! background thread, then applies the plain data to the database on the UI
//! thread. The page polls `import_status`.

use super::with_state;
use crate::engine;
use anyhow::{bail, Result};
use ferrite_core::importer::{self, Counts};
use serde_json::{json, Value};
use std::sync::Mutex;

#[derive(Clone)]
enum Status {
    Idle,
    Running { browser: String },
    Done { browser: String, counts: Counts },
    Failed { message: String },
}

static STATUS: Mutex<Status> = Mutex::new(Status::Idle);

fn set(status: Status) {
    if let Ok(mut g) = STATUS.lock() {
        *g = status;
    }
}

fn home() -> Result<std::path::PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow::anyhow!("home directory unknown"))
}

pub fn sources() -> Result<Value> {
    let list: Vec<Value> = importer::detect_sources(&home()?)
        .into_iter()
        .map(|s| {
            json!({
                "id": s.id,
                "browser": s.browser,
                "profile_name": s.profile_name,
                "has_bookmarks": s.has_bookmarks,
                "has_history": s.has_history,
            })
        })
        .collect();
    Ok(Value::Array(list))
}

pub fn status() -> Value {
    let st = STATUS.lock().map(|g| g.clone()).unwrap_or(Status::Idle);
    match st {
        Status::Idle => json!({ "state": "idle" }),
        Status::Running { browser } => json!({ "state": "running", "browser": browser }),
        Status::Done { browser, counts } => {
            json!({ "state": "done", "browser": browser, "counts": counts })
        }
        Status::Failed { message } => json!({ "state": "error", "message": message }),
    }
}

/// Start an import. Returns at once; progress is in [`status`].
pub fn run(source_id: &str, bookmarks: bool, history: bool) -> Result<Value> {
    if matches!(
        *STATUS.lock().map_err(|_| anyhow::anyhow!("busy"))?,
        Status::Running { .. }
    ) {
        bail!("an import is already running");
    }
    if !bookmarks && !history {
        bail!("nothing selected");
    }
    let Some(source) = importer::detect_sources(&home()?)
        .into_iter()
        .find(|x| x.id == source_id)
    else {
        bail!("unknown import source");
    };
    let browser = source.browser.clone();
    set(Status::Running {
        browser: browser.clone(),
    });
    std::thread::spawn(move || {
        let now = ferrite_core::now_ms();
        let data =
            std::panic::catch_unwind(|| importer::read_source(&source, bookmarks, history, now));
        engine::post_ui(move || {
            let Ok(data) = data else {
                set(Status::Failed {
                    message: "Reading the profile failed.".into(),
                });
                return;
            };
            let applied = with_state(|s| {
                let r = importer::apply(&s.db, &browser, &data);
                if r.is_ok() {
                    s.mark_all_dirty();
                }
                r
            });
            match applied {
                Some(Ok(counts)) => set(Status::Done { browser, counts }),
                Some(Err(e)) => {
                    tracing::warn!("import failed: {e:#}");
                    set(Status::Failed {
                        message: "Saving the imported data failed.".into(),
                    });
                }
                None => set(Status::Failed {
                    message: "Ferrite was busy; try again.".into(),
                }),
            }
        });
    });
    Ok(json!("started"))
}
