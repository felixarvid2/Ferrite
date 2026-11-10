//! Downloads: Chromium does the transfer; we pick a safe target path, track
//! progress, persist the list and let the user cancel or open files.

use super::{later, State};
use crate::engine::{cef_str, from_userfree, opt_str};
use cef::*;
use ferrite_core::downloads::{is_dangerous, unique_path, DownloadRecord, DownloadState};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Live {
    pub db_id: i64,
    pub private: bool,
    pub file_name: String,
    pub path: String,
    pub url: String,
    pub received: i64,
    pub total: i64,
    pub speed: i64,
    pub percent: i32,
    pub state: DownloadState,
    pub dangerous: bool,
    pub callback: Option<DownloadItemCallback>,
}

#[derive(Default)]
pub struct Downloads {
    pub live: HashMap<u32, Live>,
}

impl Downloads {
    pub fn active_count(&self) -> usize {
        self.live
            .values()
            .filter(|d| d.state == DownloadState::InProgress)
            .count()
    }

    /// Combined progress 0..=100 of running downloads (None if unknown).
    pub fn progress(&self) -> Option<i32> {
        let running: Vec<&Live> = self
            .live
            .values()
            .filter(|d| d.state == DownloadState::InProgress && d.total > 0)
            .collect();
        if running.is_empty() {
            return None;
        }
        let (r, t) = running
            .iter()
            .fold((0i64, 0i64), |(r, t), d| (r + d.received, t + d.total));
        Some(((r * 100) / t.max(1)) as i32)
    }

    pub fn cancel_all(&mut self) {
        for d in self.live.values_mut() {
            if d.state == DownloadState::InProgress {
                if let Some(cb) = &d.callback {
                    cb.cancel();
                }
            }
        }
    }
}

pub fn make_handler() -> DownloadHandler {
    FerriteDownloads::new()
}

fn before_download(
    s: &mut State,
    browser_id: i32,
    id: u32,
    url: String,
    suggested: String,
    callback: BeforeDownloadCallback,
) {
    let private = s
        .win_of_browser(browser_id)
        .is_some_and(|w| s.is_private(w));
    let dir = PathBuf::from(&s.settings.download_dir);
    let dir = if s.settings.download_dir.is_empty() {
        dirs_download()
    } else {
        dir
    };
    if let Err(err) = std::fs::create_dir_all(&dir) {
        tracing::warn!("cannot create download dir {}: {err}", dir.display());
    }
    let path = unique_path(&dir, &suggested);
    let path_str = path.to_string_lossy().to_string();
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".into());
    let record = DownloadRecord {
        id: 0,
        url: url.clone(),
        path: path_str.clone(),
        file_name: file_name.clone(),
        mime_type: String::new(),
        total_bytes: 0,
        received_bytes: 0,
        state: DownloadState::InProgress,
        started_at: ferrite_core::now_ms(),
        finished_at: None,
    };
    let db = if private { &s.private_db } else { &s.db };
    let db_id = db.insert_download(&record).unwrap_or(-1);
    let dangerous = is_dangerous(&file_name);
    s.downloads.live.insert(
        id,
        Live {
            db_id,
            private,
            file_name,
            path: path_str.clone(),
            url,
            received: 0,
            total: 0,
            speed: 0,
            percent: 0,
            state: DownloadState::InProgress,
            dangerous,
            callback: None,
        },
    );
    callback.cont(
        Some(&cef_str(&path_str)),
        s.settings.ask_download_location as i32,
    );
    s.mark_all_dirty();
}

fn dirs_download() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Downloads"))
        .unwrap_or_else(std::env::temp_dir)
}

struct Update {
    id: u32,
    full_path: String,
    received: i64,
    total: i64,
    speed: i64,
    percent: i32,
    complete: bool,
    canceled: bool,
    interrupted: bool,
}

fn download_updated(s: &mut State, u: Update, callback: Option<DownloadItemCallback>) {
    let Some(d) = s.downloads.live.get_mut(&u.id) else {
        return;
    };
    if callback.is_some() {
        d.callback = callback;
    }
    if !u.full_path.is_empty() {
        d.path = u.full_path.clone();
        if let Some(name) = PathBuf::from(&u.full_path).file_name() {
            d.file_name = name.to_string_lossy().to_string();
        }
    }
    d.received = u.received;
    d.total = u.total;
    d.speed = u.speed;
    d.percent = u.percent;
    let new_state = if u.complete {
        DownloadState::Complete
    } else if u.canceled {
        DownloadState::Cancelled
    } else if u.interrupted {
        DownloadState::Interrupted
    } else {
        DownloadState::InProgress
    };
    let finished = new_state != DownloadState::InProgress && d.state == DownloadState::InProgress;
    d.state = new_state;
    if finished {
        let db = if d.private { &s.private_db } else { &s.db };
        let _ = db.update_download(
            d.db_id,
            &d.path,
            d.received,
            d.total,
            new_state,
            Some(ferrite_core::now_ms()),
        );
        let msg = match new_state {
            DownloadState::Complete => format!("Downloaded {}", d.file_name),
            DownloadState::Cancelled => format!("Cancelled {}", d.file_name),
            _ => format!("Download failed: {}", d.file_name),
        };
        let wins: Vec<_> = s.windows.keys().copied().collect();
        for w in wins {
            s.ui_call(w, "toast", &serde_json::json!(msg));
        }
    }
    s.mark_all_dirty();
}

/// The download list for internal pages: persisted records plus live data.
pub fn list(s: &State, private: bool) -> serde_json::Value {
    let db = if private { &s.private_db } else { &s.db };
    let records = db.list_downloads(200).unwrap_or_default();
    let items: Vec<serde_json::Value> = records
        .into_iter()
        .map(|r| {
            let live = s.downloads.live.iter().find(|(_, d)| d.db_id == r.id);
            let mut v = serde_json::to_value(&r).unwrap_or_default();
            if let Some((cef_id, d)) = live {
                v["live_id"] = serde_json::json!(cef_id);
                v["received_bytes"] = serde_json::json!(d.received);
                v["total_bytes"] = serde_json::json!(d.total);
                v["speed"] = serde_json::json!(d.speed);
                v["percent"] = serde_json::json!(d.percent);
                v["state"] = serde_json::to_value(d.state).unwrap_or_default();
                v["path"] = serde_json::json!(d.path);
                v["file_name"] = serde_json::json!(d.file_name);
            }
            v["dangerous"] = serde_json::json!(is_dangerous(&r.file_name));
            v["exists"] = serde_json::json!(std::path::Path::new(&r.path).exists());
            v
        })
        .collect();
    serde_json::Value::Array(items)
}

pub fn cancel(s: &mut State, live_id: u32) {
    if let Some(cb) = s
        .downloads
        .live
        .get(&live_id)
        .and_then(|d| d.callback.clone())
    {
        cb.cancel();
    }
}

/// Open a finished download (or its folder) with the desktop's default app.
pub fn open_path(path: &str, folder: bool) -> anyhow::Result<()> {
    let p = PathBuf::from(path);
    let target = if folder {
        p.parent().map(|p| p.to_path_buf()).unwrap_or(p)
    } else {
        p
    };
    if !target.exists() {
        anyhow::bail!("file no longer exists");
    }
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(&target).spawn()?;
    Ok(())
}

wrap_download_handler! {
    struct FerriteDownloads;

    impl DownloadHandler {
        fn can_download(&self, _browser: Option<&mut Browser>, _url: Option<&CefString>, _method: Option<&CefString>) -> ::std::os::raw::c_int {
            1
        }

        fn on_before_download(
            &self,
            browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> ::std::os::raw::c_int {
            let (Some(item), Some(callback)) = (download_item, callback.cloned()) else { return 0 };
            let browser_id = browser.map(|b| b.identifier()).unwrap_or(-1);
            let id = item.id();
            let url = from_userfree(&item.url());
            let suggested = opt_str(suggested_name);
            later(move |s| before_download(s, browser_id, id, url, suggested, callback));
            1
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            callback: Option<&mut DownloadItemCallback>,
        ) {
            let Some(item) = download_item else { return };
            let update = Update {
                id: item.id(),
                full_path: from_userfree(&item.full_path()),
                received: item.received_bytes(),
                total: item.total_bytes(),
                speed: item.current_speed(),
                percent: item.percent_complete(),
                complete: item.is_complete() != 0,
                canceled: item.is_canceled() != 0,
                interrupted: item.is_interrupted() != 0,
            };
            let callback = callback.cloned();
            later(move |s| download_updated(s, update, callback));
        }
    }
}
