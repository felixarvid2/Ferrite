//! Import bookmarks and history from other browsers (Chrome family, Firefox).
//!
//! Everything here is pure: detection looks only below the given home
//! directory, the readers return plain data, and [`apply`] writes into a
//! [`Database`]. SQLite sources are copied (with their `-wal`) to a temp
//! directory first, because the source browser may be running and holds locks.
//! Passwords are deliberately not imported (the source browser encrypts them).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;

use crate::bookmarks::{BookmarkKind, BOOKMARKS_BAR};
use crate::db::Database;

/// History: newest entries only.
pub const HISTORY_MAX_ENTRIES: usize = 20_000;
pub const HISTORY_MAX_DAYS: i64 = 365;
const MAX_URL_LEN: usize = 8192;
const MAX_JSON_BYTES: u64 = 256 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
/// Milliseconds between 1601-01-01 (WebKit epoch) and 1970-01-01.
const WEBKIT_EPOCH_OFFSET_MS: i64 = 11_644_473_600_000;
const DAY_MS: i64 = 86_400_000;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Stable, opaque (hash of browser and path). The API takes this, never a path.
    pub id: String,
    pub browser: String,
    pub profile_name: String,
    #[serde(skip)]
    pub path: PathBuf,
    pub has_bookmarks: bool,
    pub has_history: bool,
}

/// A bookmark tree node read from another browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Folder { title: String, children: Vec<Node> },
    Url { title: String, url: String },
}

/// One history entry; `at` is Unix milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryItem {
    pub url: String,
    pub title: String,
    pub at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ImportData {
    pub bookmarks: Vec<Node>,
    pub history: Vec<HistoryItem>,
}

#[derive(Serialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub bookmarks: usize,
    pub folders: usize,
    pub history: usize,
    pub skipped: usize,
}

// ---- detection --------------------------------------------------------------

/// (browser name, config dirs relative to home)
const CHROME_DIRS: &[(&str, &str)] = &[
    ("Chrome", ".config/google-chrome"),
    ("Chrome", ".var/app/com.google.Chrome/config/google-chrome"),
    ("Chromium", ".config/chromium"),
    ("Chromium", "snap/chromium/common/chromium"),
    ("Chromium", ".var/app/org.chromium.Chromium/config/chromium"),
    ("Brave", ".config/BraveSoftware/Brave-Browser"),
    (
        "Brave",
        ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
    ),
    ("Edge", ".config/microsoft-edge"),
    ("Vivaldi", ".config/vivaldi"),
];

const FIREFOX_DIRS: &[&str] = &[
    ".mozilla/firefox",
    "snap/firefox/common/.mozilla/firefox",
    ".var/app/org.mozilla.firefox/.mozilla/firefox",
];

fn source_id(browser: &str, path: &Path) -> String {
    // FNV-1a: stable across runs (unlike DefaultHasher), no extra dependency.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in browser
        .bytes()
        .chain([0])
        .chain(path.to_string_lossy().bytes())
    {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn make_source(browser: &str, profile_name: String, path: PathBuf, bm: bool, hist: bool) -> Source {
    Source {
        id: source_id(browser, &path),
        browser: browser.to_string(),
        profile_name,
        path,
        has_bookmarks: bm,
        has_history: hist,
    }
}

/// Browser profiles below `home` that have something to import.
pub fn detect_sources(home: &Path) -> Vec<Source> {
    let mut out = Vec::new();
    for (browser, rel) in CHROME_DIRS {
        detect_chrome(browser, &home.join(rel), &mut out);
    }
    for rel in FIREFOX_DIRS {
        detect_firefox(&home.join(rel), &mut out);
    }
    out
}

fn detect_chrome(browser: &str, root: &Path, out: &mut Vec<Source>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    let names = chrome_profile_names(root);
    let mut dirs: Vec<String> = rd
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_chrome_profile_dir(n))
        .collect();
    dirs.sort_by_key(|n| (n != "Default", profile_number(n), n.clone()));
    for dir in dirs {
        let path = root.join(&dir);
        let bm = path.join("Bookmarks").is_file();
        let hist = path.join("History").is_file();
        if bm || hist {
            let name = names.get(&dir).cloned().unwrap_or_else(|| dir.clone());
            out.push(make_source(browser, name, path, bm, hist));
        }
    }
}

fn is_chrome_profile_dir(n: &str) -> bool {
    n == "Default"
        || n.strip_prefix("Profile ")
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

fn profile_number(n: &str) -> u64 {
    n.strip_prefix("Profile ")
        .and_then(|d| d.parse().ok())
        .unwrap_or(0)
}

/// `Local State` → `profile.info_cache.<dir>.name`.
fn chrome_profile_names(root: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(v) = read_json(&root.join("Local State")) else {
        return map;
    };
    if let Some(cache) = v.pointer("/profile/info_cache").and_then(Value::as_object) {
        for (dir, info) in cache {
            if let Some(n) = info.get("name").and_then(Value::as_str) {
                if !n.is_empty() {
                    map.insert(dir.clone(), n.to_string());
                }
            }
        }
    }
    map
}

fn detect_firefox(root: &Path, out: &mut Vec<Source>) {
    let Ok(ini) = std::fs::read_to_string(root.join("profiles.ini")) else {
        return;
    };
    for (name, path, relative) in parse_profiles_ini(&ini) {
        let dir = if relative {
            root.join(&path)
        } else {
            PathBuf::from(&path)
        };
        if dir.join("places.sqlite").is_file() {
            let name = if name.is_empty() { path.clone() } else { name };
            out.push(make_source("Firefox", name, dir, true, true));
        }
    }
}

/// `(Name, Path, IsRelative)` of every `[ProfileN]` section.
fn parse_profiles_ini(text: &str) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    let mut cur: Option<(String, String, bool)> = None;
    let flush = |cur: &mut Option<(String, String, bool)>, out: &mut Vec<_>| {
        if let Some(c) = cur.take() {
            if !c.1.is_empty() {
                out.push(c);
            }
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            flush(&mut cur, &mut out);
            if line.to_ascii_lowercase().starts_with("[profile") {
                cur = Some((String::new(), String::new(), true));
            }
        } else if let (Some(c), Some((k, v))) = (cur.as_mut(), line.split_once('=')) {
            match k.trim().to_ascii_lowercase().as_str() {
                "name" => c.0 = v.trim().to_string(),
                "path" => c.1 = v.trim().to_string(),
                "isrelative" => c.2 = v.trim() != "0",
                _ => {}
            }
        }
    }
    flush(&mut cur, &mut out);
    out
}

// ---- readers ----------------------------------------------------------------

/// Read what `source` offers. Missing or unreadable parts yield empty data.
pub fn read_source(source: &Source, bookmarks: bool, history: bool, now_ms: i64) -> ImportData {
    let firefox = source.browser == "Firefox";
    let mut data = ImportData::default();
    if bookmarks {
        let r = if firefox {
            read_firefox_bookmarks(&source.path.join("places.sqlite"))
        } else {
            read_chrome_bookmarks(&source.path.join("Bookmarks"))
        };
        match r {
            Ok(b) => data.bookmarks = b,
            Err(e) => tracing::warn!("import: reading bookmarks failed: {e:#}"),
        }
    }
    if history {
        let r = if firefox {
            read_firefox_history(&source.path.join("places.sqlite"), now_ms)
        } else {
            read_chrome_history(&source.path.join("History"), now_ms)
        };
        match r {
            Ok(h) => data.history = h,
            Err(e) => tracing::warn!("import: reading history failed: {e:#}"),
        }
    }
    data
}

fn read_json(path: &Path) -> Option<Value> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_JSON_BYTES {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Only http and https URLs are imported (no `place:`, `javascript:`, `chrome://`, ...).
fn clean_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() || url.len() > MAX_URL_LEN {
        return None;
    }
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    let parsed = url::Url::parse(url).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| url.to_string())
}

pub fn read_chrome_bookmarks(path: &Path) -> anyhow::Result<Vec<Node>> {
    let v = read_json(path).context("Bookmarks file unreadable")?;
    let roots = v.get("roots").context("no roots")?;
    let mut out = Vec::new();
    for (key, fallback) in [
        ("bookmark_bar", "Bookmarks bar"),
        ("other", "Other bookmarks"),
        ("synced", "Mobile bookmarks"),
    ] {
        let Some(root) = roots.get(key) else { continue };
        let children = chrome_children(root, 0);
        if !children.is_empty() {
            let title = root
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())
                .unwrap_or(fallback)
                .to_string();
            out.push(Node::Folder { title, children });
        }
    }
    Ok(out)
}

fn chrome_children(node: &Value, depth: usize) -> Vec<Node> {
    let Some(list) = node.get("children").and_then(Value::as_array) else {
        return Vec::new();
    };
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let mut out = Vec::new();
    for c in list {
        let title = c
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match c.get("type").and_then(Value::as_str) {
            Some("url") => {
                if let Some(url) = c.get("url").and_then(Value::as_str).and_then(clean_url) {
                    let title = if title.is_empty() { url.clone() } else { title };
                    out.push(Node::Url { title, url });
                }
            }
            Some("folder") => out.push(Node::Folder {
                title,
                children: chrome_children(c, depth + 1),
            }),
            _ => {}
        }
    }
    out
}

/// Copy `db` (and `-wal`) into `dir` and open the copy. It is opened read-write
/// so SQLite can recover the copied WAL, then made query-only.
fn open_copy(db: &Path, dir: &Path) -> anyhow::Result<Connection> {
    let name = db.file_name().context("no file name")?;
    let dest = dir.join(name);
    std::fs::copy(db, &dest).with_context(|| format!("copying {}", db.display()))?;
    let mut wal = db.as_os_str().to_os_string();
    wal.push("-wal");
    let wal = PathBuf::from(wal);
    if wal.is_file() {
        let mut dest_wal = dest.as_os_str().to_os_string();
        dest_wal.push("-wal");
        let _ = std::fs::copy(&wal, PathBuf::from(dest_wal));
    }
    let conn = Connection::open(&dest).context("opening copy")?;
    conn.pragma_update(None, "query_only", true)?;
    Ok(conn)
}

fn history_cutoff_ms(now_ms: i64) -> i64 {
    now_ms.saturating_sub(HISTORY_MAX_DAYS * DAY_MS)
}

pub fn read_chrome_history(path: &Path, now_ms: i64) -> anyhow::Result<Vec<HistoryItem>> {
    let tmp = tempfile::tempdir()?;
    let conn = open_copy(path, tmp.path())?;
    let cutoff = (history_cutoff_ms(now_ms) + WEBKIT_EPOCH_OFFSET_MS) * 1000;
    let mut stmt = conn.prepare(
        "SELECT url, title, last_visit_time FROM urls
         WHERE last_visit_time > ?1 AND (url LIKE 'http://%' OR url LIKE 'https://%')
         ORDER BY last_visit_time DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![cutoff, HISTORY_MAX_ENTRIES as i64], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for (url, title, t) in rows.filter_map(Result::ok) {
        if let Some(url) = clean_url(&url) {
            out.push(HistoryItem {
                url,
                title: title.unwrap_or_default(),
                at: t / 1000 - WEBKIT_EPOCH_OFFSET_MS,
            });
        }
    }
    Ok(out)
}

pub fn read_firefox_history(path: &Path, now_ms: i64) -> anyhow::Result<Vec<HistoryItem>> {
    let tmp = tempfile::tempdir()?;
    let conn = open_copy(path, tmp.path())?;
    let cutoff = history_cutoff_ms(now_ms) * 1000;
    let mut stmt = conn.prepare(
        "SELECT url, title, last_visit_date FROM moz_places
         WHERE last_visit_date > ?1 AND visit_count > 0 AND hidden = 0
           AND (url LIKE 'http://%' OR url LIKE 'https://%')
         ORDER BY last_visit_date DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![cutoff, HISTORY_MAX_ENTRIES as i64], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for (url, title, t) in rows.filter_map(Result::ok) {
        if let Some(url) = clean_url(&url) {
            out.push(HistoryItem {
                url,
                title: title.unwrap_or_default(),
                at: t / 1000,
            });
        }
    }
    Ok(out)
}

struct FfRow {
    kind: i64,
    title: String,
    url: Option<String>,
    guid: String,
}

pub fn read_firefox_bookmarks(path: &Path) -> anyhow::Result<Vec<Node>> {
    let tmp = tempfile::tempdir()?;
    let conn = open_copy(path, tmp.path())?;
    let mut stmt = conn.prepare(
        "SELECT b.id, b.parent, b.type, b.title, p.url, b.guid
         FROM moz_bookmarks b LEFT JOIN moz_places p ON p.id = b.fk
         ORDER BY b.parent, b.position, b.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            FfRow {
                kind: r.get(2)?,
                title: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                url: r.get::<_, Option<String>>(4)?,
                guid: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            },
        ))
    })?;
    let mut by_parent: HashMap<i64, Vec<(i64, FfRow)>> = HashMap::new();
    let mut root_id = None;
    for (id, parent, row) in rows.filter_map(Result::ok) {
        if row.guid == "root________" {
            root_id = Some(id);
        }
        by_parent.entry(parent).or_default().push((id, row));
    }
    let root_id = root_id.unwrap_or(1);
    let mut out = Vec::new();
    for (id, row) in by_parent.get(&root_id).map(Vec::as_slice).unwrap_or(&[]) {
        let title = match row.guid.as_str() {
            "toolbar_____" => "Bookmarks toolbar",
            "menu________" => "Bookmarks menu",
            "unfiled_____" => "Other bookmarks",
            "mobile______" => "Mobile bookmarks",
            _ => continue, // tags, smart-bookmark queries
        };
        let mut seen = HashSet::new();
        let children = ff_children(*id, &by_parent, &mut seen, 0);
        if !children.is_empty() {
            out.push(Node::Folder {
                title: title.to_string(),
                children,
            });
        }
    }
    Ok(out)
}

fn ff_children(
    parent: i64,
    by_parent: &HashMap<i64, Vec<(i64, FfRow)>>,
    seen: &mut HashSet<i64>,
    depth: usize,
) -> Vec<Node> {
    let mut out = Vec::new();
    if depth > MAX_DEPTH || !seen.insert(parent) {
        return out;
    }
    for (id, row) in by_parent.get(&parent).map(Vec::as_slice).unwrap_or(&[]) {
        match row.kind {
            1 => {
                if let Some(url) = row.url.as_deref().and_then(clean_url) {
                    let title = if row.title.is_empty() {
                        url.clone()
                    } else {
                        row.title.clone()
                    };
                    out.push(Node::Url { title, url });
                }
            }
            2 => out.push(Node::Folder {
                title: row.title.clone(),
                children: ff_children(*id, by_parent, seen, depth + 1),
            }),
            _ => {}
        }
    }
    out
}

// ---- applying -----------------------------------------------------------------

/// Write `data` into `db`. Bookmarks go into "Imported from <browser>" on the
/// bookmarks bar (found again on re-import); URLs already bookmarked anywhere
/// are skipped. History entries with the same URL and time are skipped.
pub fn apply(db: &Database, browser: &str, data: &ImportData) -> anyhow::Result<Counts> {
    let mut counts = Counts::default();
    let tx = db.conn().unchecked_transaction()?;
    if !data.bookmarks.is_empty() {
        apply_bookmarks(db, browser, &data.bookmarks, &mut counts)?;
    }
    apply_history(db, &data.history, &mut counts)?;
    tx.commit()?;
    Ok(counts)
}

/// Drop URLs already known (or repeated within the import) and folders left empty.
fn prune(
    nodes: &[Node],
    known: &mut HashSet<String>,
    db: &Database,
    skipped: &mut usize,
) -> Vec<Node> {
    let mut out = Vec::new();
    for n in nodes {
        match n {
            Node::Url { url, .. } => {
                let exists = known.contains(url)
                    || db
                        .find_bookmarks_by_url(url)
                        .map(|b| !b.is_empty())
                        .unwrap_or(false);
                if exists {
                    *skipped += 1;
                } else {
                    known.insert(url.clone());
                    out.push(n.clone());
                }
            }
            Node::Folder { title, children } => {
                let kept = prune(children, known, db, skipped);
                if !kept.is_empty() {
                    out.push(Node::Folder {
                        title: title.clone(),
                        children: kept,
                    });
                }
            }
        }
    }
    out
}

fn apply_bookmarks(
    db: &Database,
    browser: &str,
    nodes: &[Node],
    counts: &mut Counts,
) -> anyhow::Result<()> {
    let mut known = HashSet::new();
    let kept = prune(nodes, &mut known, db, &mut counts.skipped);
    if kept.is_empty() {
        return Ok(());
    }
    let title = format!("Imported from {browser}");
    let existing = db
        .bookmark_children(BOOKMARKS_BAR)?
        .into_iter()
        .find(|b| b.kind == BookmarkKind::Folder && b.title == title);
    let root = match existing {
        Some(b) => b.id,
        None => {
            counts.folders += 1;
            db.add_folder(BOOKMARKS_BAR, &title)?
        }
    };
    add_nodes(db, root, &kept, counts, 0)
}

fn add_nodes(
    db: &Database,
    parent: i64,
    nodes: &[Node],
    counts: &mut Counts,
    depth: usize,
) -> anyhow::Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    for n in nodes {
        match n {
            Node::Url { title, url } => {
                db.add_bookmark(parent, title, url)?;
                counts.bookmarks += 1;
            }
            Node::Folder { title, children } => {
                let id = db.add_folder(parent, title)?;
                counts.folders += 1;
                add_nodes(db, id, children, counts, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn apply_history(db: &Database, items: &[HistoryItem], counts: &mut Counts) -> anyhow::Result<()> {
    let conn = db.conn();
    let mut find = conn.prepare(
        "SELECT 1 FROM history_visits v JOIN history_urls u ON u.id = v.url_id
         WHERE u.url = ?1 AND v.visited_at = ?2",
    )?;
    let mut upsert = conn.prepare(
        "INSERT INTO history_urls (url, title, visit_count, last_visit) VALUES (?1, ?2, 1, ?3)
         ON CONFLICT(url) DO UPDATE SET
            visit_count = visit_count + 1,
            last_visit = MAX(last_visit, excluded.last_visit),
            title = CASE WHEN title = '' THEN excluded.title ELSE title END",
    )?;
    let mut url_id = conn.prepare("SELECT id FROM history_urls WHERE url = ?1")?;
    let mut visit =
        conn.prepare("INSERT INTO history_visits (url_id, visited_at) VALUES (?1, ?2)")?;
    for it in items {
        if find.exists(params![it.url, it.at])? {
            counts.skipped += 1;
            continue;
        }
        upsert.execute(params![it.url, it.title, it.at])?;
        let id: i64 = url_id.query_row([&it.url], |r| r.get(0))?;
        visit.execute(params![id, it.at])?;
        counts.history += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    pub const NOW: i64 = 1_800_000_000_000; // ms

    fn webkit(ms: i64) -> i64 {
        (ms + WEBKIT_EPOCH_OFFSET_MS) * 1000
    }

    pub fn write_chrome_bookmarks(path: &Path) {
        let json = serde_json::json!({
            "roots": {
                "bookmark_bar": {"name": "Bookmarks bar", "type": "folder", "children": [
                    {"type": "url", "name": "Rust", "url": "https://www.rust-lang.org/"},
                    {"type": "folder", "name": "News", "children": [
                        {"type": "url", "name": "HN", "url": "https://news.ycombinator.com/"},
                        {"type": "url", "name": "Bad", "url": "javascript:alert(1)"},
                        {"type": "url", "name": "Internal", "url": "chrome://settings"}
                    ]}
                ]},
                "other": {"name": "Other bookmarks", "type": "folder", "children": [
                    {"type": "url", "name": "", "url": "http://example.com/a"}
                ]},
                "synced": {"name": "Mobile bookmarks", "type": "folder", "children": []}
            }
        });
        fs::write(path, json.to_string()).unwrap();
    }

    pub fn write_chrome_history(path: &Path) {
        let c = Connection::open(path).unwrap();
        c.execute_batch(
            "CREATE TABLE urls (id INTEGER PRIMARY KEY, url LONGVARCHAR, title LONGVARCHAR,
               visit_count INTEGER DEFAULT 0 NOT NULL, last_visit_time INTEGER NOT NULL);",
        )
        .unwrap();
        let rows = [
            ("https://a.example/", "A", webkit(NOW - 1000)),
            ("https://b.example/", "B", webkit(NOW - 2000)),
            ("https://old.example/", "Old", webkit(NOW - 400 * DAY_MS)),
            ("chrome://version/", "V", webkit(NOW - 3000)),
            ("file:///etc/passwd", "F", webkit(NOW - 3000)),
        ];
        for (u, t, w) in rows {
            c.execute(
                "INSERT INTO urls (url, title, visit_count, last_visit_time) VALUES (?1, ?2, 1, ?3)",
                params![u, t, w],
            )
            .unwrap();
        }
    }

    pub fn write_firefox_places(path: &Path) {
        let c = Connection::open(path).unwrap();
        c.execute_batch(
            "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url LONGVARCHAR, title LONGVARCHAR,
               visit_count INTEGER DEFAULT 0, hidden INTEGER DEFAULT 0 NOT NULL, last_visit_date INTEGER);
             CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER DEFAULT NULL,
               parent INTEGER, position INTEGER, title LONGVARCHAR, guid TEXT);
             INSERT INTO moz_places (id,url,title,visit_count,hidden,last_visit_date) VALUES
               (1,'https://mozilla.org/','Mozilla',3,0,%A%),
               (2,'https://hidden.example/','Hidden',1,1,%A%),
               (3,'place:type=6&sort=14','Recent',0,0,NULL),
               (4,'https://unvisited.example/','U',0,0,%A%),
               (5,'https://old.example/','Old',2,0,%OLD%),
               (6,'https://fx.example/','FX',1,0,%B%);
             INSERT INTO moz_bookmarks (id,type,fk,parent,position,title,guid) VALUES
               (1,2,NULL,0,0,'','root________'),
               (2,2,NULL,1,0,'menu','menu________'),
               (3,2,NULL,1,1,'Bookmarks Toolbar','toolbar_____'),
               (4,2,NULL,1,2,'Other','unfiled_____'),
               (5,2,NULL,1,3,'mobile','mobile______'),
               (6,2,NULL,1,4,'','tags________'),
               (7,1,1,3,0,'Mozilla','g7'),
               (8,2,NULL,3,1,'Dev','g8'),
               (9,1,6,8,0,NULL,'g9'),
               (10,1,3,4,0,'Query','g10'),
               (11,3,NULL,3,2,NULL,'g11');"
                .replace("%A%", &((NOW - 5000) * 1000).to_string())
                .replace("%B%", &((NOW - 6000) * 1000).to_string())
                .replace("%OLD%", &((NOW - 500 * DAY_MS) * 1000).to_string())
                .as_str(),
        )
        .unwrap();
    }

    #[test]
    fn detects_chrome_and_firefox() {
        let home = tempfile::tempdir().unwrap();
        let h = home.path();
        let chrome = h.join(".config/google-chrome");
        fs::create_dir_all(chrome.join("Default")).unwrap();
        fs::create_dir_all(chrome.join("Profile 2")).unwrap();
        fs::create_dir_all(chrome.join("Profile 3")).unwrap(); // empty: ignored
        fs::create_dir_all(chrome.join("System Profile")).unwrap();
        fs::write(chrome.join("Default/Bookmarks"), "{}").unwrap();
        fs::write(chrome.join("Profile 2/History"), "").unwrap();
        fs::write(
            chrome.join("Local State"),
            r#"{"profile":{"info_cache":{"Profile 2":{"name":"Work"}}}}"#,
        )
        .unwrap();
        let snap = h.join("snap/chromium/common/chromium/Default");
        fs::create_dir_all(&snap).unwrap();
        fs::write(snap.join("History"), "").unwrap();

        let ff = h.join(".mozilla/firefox");
        fs::create_dir_all(ff.join("abc.default-release")).unwrap();
        fs::create_dir_all(ff.join("nodb.default")).unwrap();
        fs::write(ff.join("abc.default-release/places.sqlite"), "").unwrap();
        fs::write(
            ff.join("profiles.ini"),
            "[Install4F96]\nDefault=abc.default-release\n\n[Profile1]\nName=old\nIsRelative=1\nPath=nodb.default\n\n[Profile0]\nName=default-release\nIsRelative=1\nPath=abc.default-release\nDefault=1\n\n[General]\nVersion=2\n",
        )
        .unwrap();

        let s = detect_sources(h);
        let got: Vec<_> = s
            .iter()
            .map(|s| {
                (
                    s.browser.as_str(),
                    s.profile_name.as_str(),
                    s.has_bookmarks,
                    s.has_history,
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("Chrome", "Default", true, false),
                ("Chrome", "Work", false, true),
                ("Chromium", "Default", false, true),
                ("Firefox", "default-release", true, true),
            ]
        );
        // ids are stable and unique
        let again = detect_sources(h);
        assert_eq!(s, again);
        let ids: HashSet<_> = s.iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids.len(), 4);
        assert!(detect_sources(&h.join("nothing")).is_empty());
    }

    #[test]
    fn reads_chrome_bookmarks() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Bookmarks");
        write_chrome_bookmarks(&p);
        let n = read_chrome_bookmarks(&p).unwrap();
        assert_eq!(n.len(), 2); // empty synced root dropped
        let Node::Folder { title, children } = &n[0] else {
            panic!()
        };
        assert_eq!(title, "Bookmarks bar");
        assert_eq!(children.len(), 2);
        let Node::Folder { children: news, .. } = &children[1] else {
            panic!()
        };
        assert_eq!(news.len(), 1); // javascript:/chrome:// dropped
        let Node::Folder {
            children: other, ..
        } = &n[1]
        else {
            panic!()
        };
        assert_eq!(
            other[0],
            Node::Url {
                title: "http://example.com/a".into(),
                url: "http://example.com/a".into()
            }
        );
        assert!(read_chrome_bookmarks(&d.path().join("missing")).is_err());
        fs::write(d.path().join("junk"), "not json").unwrap();
        assert!(read_chrome_bookmarks(&d.path().join("junk")).is_err());
    }

    #[test]
    fn reads_chrome_history() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("History");
        write_chrome_history(&p);
        let h = read_chrome_history(&p, NOW).unwrap();
        let got: Vec<_> = h.iter().map(|i| (i.url.as_str(), i.at)).collect();
        assert_eq!(
            got,
            vec![
                ("https://a.example/", NOW - 1000),
                ("https://b.example/", NOW - 2000)
            ]
        );
        assert_eq!(h[0].title, "A");
    }

    #[test]
    fn reads_firefox() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("places.sqlite");
        write_firefox_places(&p);
        let h = read_firefox_history(&p, NOW).unwrap();
        let got: Vec<_> = h.iter().map(|i| (i.url.as_str(), i.at)).collect();
        assert_eq!(
            got,
            vec![
                ("https://mozilla.org/", NOW - 5000),
                ("https://fx.example/", NOW - 6000)
            ]
        );
        let b = read_firefox_bookmarks(&p).unwrap();
        // menu, mobile empty; "Other" holds only a place: query; tags skipped
        assert_eq!(b.len(), 1, "{b:?}");
        let Node::Folder { title, children } = &b[0] else {
            panic!()
        };
        assert_eq!(title, "Bookmarks toolbar");
        assert_eq!(children.len(), 2);
        let Node::Folder { children: dev, .. } = &children[1] else {
            panic!()
        };
        assert_eq!(
            dev,
            &vec![Node::Url {
                title: "https://fx.example/".into(),
                url: "https://fx.example/".into()
            }]
        );
    }

    #[test]
    fn copy_includes_wal() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("History");
        let c = Connection::open(&p).unwrap();
        c.pragma_update(None, "journal_mode", "WAL").unwrap();
        c.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        c.execute_batch("CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, last_visit_time INTEGER);").unwrap();
        c.execute(
            "INSERT INTO urls (url,title,visit_count,last_visit_time) VALUES ('https://wal.example/','W',1,?1)",
            [webkit(NOW - 10)],
        )
        .unwrap();
        // source connection still open (browser running): data only in the WAL
        let h = read_chrome_history(&p, NOW).unwrap();
        assert_eq!(h.len(), 1);
        drop(c);
    }

    #[test]
    fn apply_is_idempotent() {
        let d = tempfile::tempdir().unwrap();
        write_chrome_bookmarks(&d.path().join("Bookmarks"));
        write_chrome_history(&d.path().join("History"));
        let src = make_source(
            "Chrome",
            "Default".into(),
            d.path().to_path_buf(),
            true,
            true,
        );
        let data = read_source(&src, true, true, NOW);
        let db = Database::open_in_memory().unwrap();
        db.add_bookmark(
            crate::bookmarks::OTHER_BOOKMARKS,
            "mine",
            "https://www.rust-lang.org/",
        )
        .unwrap();
        let c = apply(&db, "Chrome", &data).unwrap();
        assert_eq!(
            c,
            Counts {
                bookmarks: 2,
                folders: 4,
                history: 2,
                skipped: 1
            }
        );
        let tree = db.bookmark_tree().unwrap();
        let bar = &tree[0];
        let imp = bar
            .children
            .iter()
            .find(|n| n.bookmark.title == "Imported from Chrome")
            .unwrap();
        assert_eq!(imp.children.len(), 2);
        assert_eq!(db.search_history("", 100).unwrap().len(), 2);

        let c2 = apply(&db, "Chrome", &data).unwrap();
        assert_eq!((c2.bookmarks, c2.folders, c2.history), (0, 0, 0));
        assert_eq!(c2.skipped, 3 + 2);
        assert_eq!(db.search_history("", 100).unwrap().len(), 2);
        let imps = db
            .bookmark_children(BOOKMARKS_BAR)
            .unwrap()
            .into_iter()
            .filter(|b| b.title == "Imported from Chrome")
            .count();
        assert_eq!(imps, 1);
    }
}
