//! JSON API for the UI strip and internal pages (`ferrite://…/api/<cmd>`).
//! Only reachable from ferrite:// documents (see `scheme`).

use super::{
    commands, downloads, menu, permissions, tabs, window, BrowserRef, State, WinId, NEW_TAB_URL,
};
use anyhow::{anyhow, bail, Context, Result};
use cef::*;
use ferrite_core::commands::Command;
use ferrite_core::omnibox::{display_url, security_level};
use serde_json::{json, Value};

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string argument '{key}'"))
}
fn arg_i64(args: &Value, key: &str) -> Result<i64> {
    args.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("missing integer argument '{key}'"))
}
fn arg_bool(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn window_snapshot(s: &State, win: WinId) -> Value {
    let Some(w) = s.windows.get(&win) else {
        return Value::Null;
    };
    let tabs: Vec<Value> = w
        .tabs
        .iter()
        .filter_map(|id| s.tabs.get(id))
        .map(|t| {
            json!({
                "id": t.id,
                "title": t.title,
                "url": t.url,
                "favicon": t.favicon,
                "loading": t.loading,
                "pinned": t.pinned,
                "crashed": t.crashed,
                "closing": t.closing,
                "media": t.media_video || t.media_audio,
            })
        })
        .collect();
    let active = w.active.and_then(|a| s.tabs.get(&a));
    let nav = active.map(|t| {
        let bookmarked = !w.private
            && !t.url.is_empty()
            && s.db
                .find_bookmarks_by_url(&t.url)
                .map(|b| !b.is_empty())
                .unwrap_or(false);
        json!({
            "tab": t.id,
            "url": t.url,
            "display_url": display_url(&t.url),
            "security": if t.cert_error || t.secure == Some(false) {
                json!("error")
            } else {
                serde_json::to_value(security_level(&t.url)).unwrap_or_default()
            },
            "loading": t.loading,
            "progress": t.progress,
            "can_back": t.can_back,
            "can_forward": t.can_forward,
            "zoom": t.zoom_percent,
            "bookmarked": bookmarked,
            "find": t.find,
            "crashed": t.crashed,
        })
    });
    let bookmarks_bar = if s.settings.show_bookmarks_bar {
        s.db.bookmark_children(ferrite_core::bookmarks::BOOKMARKS_BAR)
            .map(|v| serde_json::to_value(v).unwrap_or_default())
            .unwrap_or_default()
    } else {
        Value::Null
    };
    let prompts: Vec<Value> = w
        .prompts
        .iter()
        .filter(|p| Some(p.tab) == w.active)
        .map(|p| json!({ "id": p.id, "origin": p.origin, "permissions": p.permissions }))
        .collect();
    json!({
        "win": win,
        "private": w.private,
        "theme": s.settings.theme,
        "tabs": tabs,
        "active": w.active,
        "nav": nav,
        "bookmarks_bar": bookmarks_bar,
        "prompts": prompts,
        "downloads": { "active": s.downloads.active_count(), "progress": s.downloads.progress() },
        "closed_tabs": w.closed_tabs.len(),
    })
}

/// Commands only the UI strip may send.
const UI_ONLY: &[&str] = &[
    "set_height",
    "menu",
    "tab_menu",
    "activate_tab",
    "move_tab",
    "pin_tab",
    "permission",
];

pub fn handle(s: &mut State, browser_id: i32, command: &str, args: Value) -> Result<Value> {
    let caller = *s
        .browsers
        .get(&browser_id)
        .ok_or_else(|| anyhow!("unknown caller"))?;
    let win = s
        .win_of_browser(browser_id)
        .ok_or_else(|| anyhow!("caller has no window"))?;
    let is_ui = matches!(caller, BrowserRef::Ui(_));
    if UI_ONLY.contains(&command) && !is_ui {
        bail!("'{command}' is only available to the browser UI");
    }
    let private = s.is_private(win);
    let active = s.active_tab(win);
    let tab_arg =
        |args: &Value| -> Option<u32> { args.get("tab").and_then(Value::as_u64).map(|t| t as u32) };

    Ok(match command {
        // ---- UI strip -------------------------------------------------
        "ready" => {
            if let Some(w) = s.windows.get_mut(&win) {
                w.ui_ready = true;
            }
            window_snapshot(s, win)
        }
        "state" => window_snapshot(s, win),
        "set_height" => {
            window::set_ui_height(s, win, arg_i64(&args, "height")? as i32);
            Value::Null
        }
        "navigate" => {
            let input = arg_str(&args, "input")?.to_string();
            let tab = tab_arg(&args).or(active).context("no tab")?;
            if arg_bool(&args, "new_tab") {
                let url = s.resolve_input(&input);
                tabs::new_tab(s, win, &url, tabs::NewTab::foreground());
            } else {
                tabs::navigate(s, tab, &input);
            }
            Value::Null
        }
        "open_url" => {
            let url = s.resolve_input(arg_str(&args, "url")?);
            if arg_bool(&args, "new_tab") || arg_bool(&args, "background") {
                let opts = tabs::NewTab {
                    background: arg_bool(&args, "background"),
                    opener: active,
                    ..Default::default()
                };
                tabs::new_tab(s, win, &url, opts);
            } else if let Some(tab) = match caller {
                BrowserRef::Tab(t) => Some(t),
                BrowserRef::Ui(_) => active,
            } {
                tabs::load_url(s, tab, &url);
            }
            Value::Null
        }
        "new_tab" => {
            let url = args
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or(NEW_TAB_URL)
                .to_string();
            let url = if url == NEW_TAB_URL {
                url
            } else {
                s.resolve_input(&url)
            };
            let opts = tabs::NewTab {
                background: arg_bool(&args, "background"),
                ..Default::default()
            };
            json!(tabs::new_tab(s, win, &url, opts))
        }
        "activate_tab" => {
            tabs::activate(s, win, arg_i64(&args, "tab")? as u32);
            Value::Null
        }
        "close_tab" => {
            let tab = tab_arg(&args).or(match caller {
                BrowserRef::Tab(t) => Some(t),
                BrowserRef::Ui(_) => active,
            });
            if let Some(tab) = tab {
                tabs::close_tab(s, tab);
            }
            Value::Null
        }
        "move_tab" => {
            tabs::move_tab(
                s,
                arg_i64(&args, "tab")? as u32,
                arg_i64(&args, "index")?.max(0) as usize,
            );
            Value::Null
        }
        "pin_tab" => {
            tabs::set_pinned(s, arg_i64(&args, "tab")? as u32, arg_bool(&args, "pinned"));
            Value::Null
        }
        "duplicate_tab" => {
            if let Some(tab) = tab_arg(&args).or(active) {
                tabs::duplicate(s, tab);
            }
            Value::Null
        }
        "command" => {
            let cmd: Command =
                serde_json::from_value(args.get("name").cloned().unwrap_or_default())
                    .context("unknown command")?;
            commands::exec(s, win, cmd);
            Value::Null
        }
        "menu" => {
            menu::show(
                s,
                win,
                arg_i64(&args, "x")? as i32,
                arg_i64(&args, "y")? as i32,
            );
            Value::Null
        }
        "tab_menu" => {
            menu::show_tab_menu(
                s,
                win,
                arg_i64(&args, "tab")? as u32,
                arg_i64(&args, "x")? as i32,
                arg_i64(&args, "y")? as i32,
            );
            Value::Null
        }
        "find" => {
            let tab = active.context("no tab")?;
            commands::find(
                s,
                tab,
                arg_str(&args, "text")?,
                !arg_bool(&args, "backward"),
                arg_bool(&args, "next"),
            );
            Value::Null
        }
        "find_close" => {
            if let Some(tab) = active {
                commands::close_find(s, tab);
            }
            Value::Null
        }
        "focus_content" => {
            if let Some(view) = active
                .and_then(|t| s.tabs.get(&t))
                .and_then(|t| t.view.clone())
            {
                view.request_focus();
            }
            Value::Null
        }
        "suggest" => suggest(s, win, arg_str(&args, "text")?),
        "permission" => {
            permissions::decide(
                s,
                win,
                arg_i64(&args, "id")? as u64,
                arg_bool(&args, "allow"),
                arg_bool(&args, "remember"),
            );
            Value::Null
        }
        "toggle_bookmark" => json!(commands::toggle_bookmark(s, win)),

        // ---- history --------------------------------------------------
        "history_search" => {
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(300) as usize;
            serde_json::to_value(
                s.db_for(win)
                    .search_history(args.get("q").and_then(Value::as_str).unwrap_or(""), limit)?,
            )?
        }
        "history_delete" => {
            s.db_for(win).delete_visit(arg_i64(&args, "id")?)?;
            Value::Null
        }
        "history_delete_url" => {
            s.db_for(win).delete_url(arg_str(&args, "url")?)?;
            Value::Null
        }
        "history_clear" => {
            let now = ferrite_core::now_ms();
            let hour = 3_600_000;
            let from = match args.get("range").and_then(Value::as_str).unwrap_or("all") {
                "hour" => now - hour,
                "day" => now - 24 * hour,
                "week" => now - 7 * 24 * hour,
                "month" => now - 30 * 24 * hour,
                _ => 0,
            };
            if from == 0 {
                s.db_for(win).clear_history()?;
            } else {
                s.db_for(win).delete_history_range(from, now + 1)?;
            }
            Value::Null
        }

        // ---- bookmarks ------------------------------------------------
        "bookmarks_tree" => serde_json::to_value(s.db.bookmark_tree()?)?,
        "bookmark_add" => {
            let id = s.db.add_bookmark(
                arg_i64(&args, "parent")?,
                arg_str(&args, "title")?,
                arg_str(&args, "url")?,
            )?;
            s.mark_all_dirty();
            json!(id)
        }
        "bookmark_folder_add" => {
            let id =
                s.db.add_folder(arg_i64(&args, "parent")?, arg_str(&args, "title")?)?;
            s.mark_all_dirty();
            json!(id)
        }
        "bookmark_update" => {
            let url = args.get("url").and_then(Value::as_str);
            s.db.update_bookmark(arg_i64(&args, "id")?, arg_str(&args, "title")?, url)?;
            s.mark_all_dirty();
            Value::Null
        }
        "bookmark_move" => {
            s.db.move_bookmark(
                arg_i64(&args, "id")?,
                arg_i64(&args, "parent")?,
                arg_i64(&args, "index")?.max(0) as usize,
            )?;
            s.mark_all_dirty();
            Value::Null
        }
        "bookmark_delete" => {
            s.db.delete_bookmark(arg_i64(&args, "id")?)?;
            s.mark_all_dirty();
            Value::Null
        }

        // ---- downloads ------------------------------------------------
        "downloads_list" => downloads::list(s, private),
        "download_cancel" => {
            downloads::cancel(s, arg_i64(&args, "live_id")? as u32);
            Value::Null
        }
        "download_open" => {
            downloads::open_path(arg_str(&args, "path")?, arg_bool(&args, "folder"))?;
            Value::Null
        }
        "download_remove" => {
            let db = if private { &s.private_db } else { &s.db };
            db.remove_download(arg_i64(&args, "id")?)?;
            Value::Null
        }
        "downloads_clear" => {
            let db = if private { &s.private_db } else { &s.db };
            db.clear_finished_downloads()?;
            Value::Null
        }

        // ---- settings & privacy -----------------------------------------
        "settings_get" => json!({
            "settings": s.settings,
            "engines": ferrite_core::search::builtin_engines(),
            "profile": s.launch.profile,
            "profile_dir": s.paths.root,
            "private": private,
        }),
        "settings_set" => {
            let before_theme = s.settings.theme;
            s.settings
                .apply_patch(args.get("patch").unwrap_or(&Value::Null))?;
            s.save_settings();
            super::handlers::SEND_DNT.store(
                s.settings.send_do_not_track,
                std::sync::atomic::Ordering::Relaxed,
            );
            if s.settings.theme != before_theme {
                super::theme::apply_to_context(s, None);
                if let Some(ctx) = s.private_context.clone() {
                    super::theme::apply_to_context(s, Some(&ctx));
                }
            }
            s.mark_all_dirty();
            serde_json::to_value(&s.settings)?
        }
        "permissions_list" => serde_json::to_value(s.db.list_permissions()?)?,
        "permission_remove" => {
            s.db.remove_permission(arg_str(&args, "origin")?, arg_str(&args, "permission")?)?;
            Value::Null
        }
        "permissions_clear" => {
            s.db.clear_permissions()?;
            Value::Null
        }
        "clear_browsing_data" => clear_browsing_data(s, win, &args)?,
        "top_sites" => {
            let db = s.db_for(win);
            json!({
                "sites": serde_json::to_value(db.suggest("", 12).unwrap_or_default())?,
                "bookmarks": serde_json::to_value(s.db.bookmark_children(ferrite_core::bookmarks::BOOKMARKS_BAR).unwrap_or_default())?,
                "private": private,
            })
        }
        "about" => json!({
            "ferrite": env!("CARGO_PKG_VERSION"),
            "cef": format!("{}.{}.{}", sys::CEF_VERSION_MAJOR, sys::CEF_VERSION_MINOR, sys::CEF_VERSION_PATCH),
            "chromium": format!("{}.{}.{}.{}", sys::CHROME_VERSION_MAJOR, sys::CHROME_VERSION_MINOR, sys::CHROME_VERSION_BUILD, sys::CHROME_VERSION_PATCH),
            "profile_dir": s.paths.root,
            "sandbox": !s.launch.no_sandbox,
        }),
        other => bail!("unknown command '{other}'"),
    })
}

fn suggest(s: &State, win: WinId, text: &str) -> Value {
    let text = text.trim();
    if text.is_empty() {
        return json!({ "inline": null, "items": [] });
    }
    let db = s.db_for(win);
    let history = db.suggest(text, 8).unwrap_or_default();
    let inline = history
        .iter()
        .find_map(|h| ferrite_core::history::inline_completion(text, &h.url));
    let mut items: Vec<Value> = Vec::new();
    for b in s.db.search_bookmarks(text, 4).unwrap_or_default() {
        items.push(json!({ "kind": "bookmark", "title": b.title, "url": b.url }));
    }
    for h in history {
        if items.iter().any(|i| i["url"] == json!(h.url)) {
            continue;
        }
        items.push(json!({ "kind": "history", "title": h.title, "url": h.url }));
    }
    items.truncate(8);
    let search = s.resolve_input(&format!("?{text}"));
    items.push(json!({ "kind": "search", "title": format!("Search for “{text}”"), "url": search }));
    json!({ "inline": inline, "items": items })
}

fn clear_browsing_data(s: &mut State, win: WinId, args: &Value) -> Result<Value> {
    let ctx = if s.is_private(win) {
        s.private_context.clone()
    } else {
        request_context_get_global_context()
    };
    let ctx = ctx.context("no request context")?;
    let mut done = Vec::new();
    if arg_bool(args, "history") {
        s.db_for(win).clear_history()?;
        done.push("history");
    }
    if arg_bool(args, "cookies") {
        if let Some(cm) = ctx.cookie_manager(None) {
            cm.delete_cookies(None, None, None);
        }
        done.push("cookies");
    }
    if arg_bool(args, "cache") {
        ctx.clear_http_cache(None);
        done.push("cache");
    }
    if arg_bool(args, "site_data") {
        // Chromium offers no in-process "clear all storage" call to
        // embedders; schedule removal before the next start instead.
        let _ = std::fs::write(s.paths.root.join("clear-site-data"), b"1");
        done.push("site_data (on next start)");
    }
    if arg_bool(args, "downloads") {
        s.db_for(win).clear_finished_downloads()?;
        done.push("downloads");
    }
    if arg_bool(args, "permissions") {
        s.db_for(win).clear_permissions()?;
        done.push("permissions");
    }
    Ok(json!({ "cleared": done }))
}
