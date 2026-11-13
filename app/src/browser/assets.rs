//! UI files compiled into the binary (from `ui/`). Set
//! `FERRITE_UI_DIR` to serve them from disk instead while developing.

macro_rules! ui_files {
    ($($name:literal),* $(,)?) => {
        const FILES: &[(&str, &[u8])] = &[
            $(($name, include_bytes!(concat!("../../../ui/", $name))),)*
        ];
    };
}

ui_files!(
    "chrome.html",
    "chrome.css",
    "chrome.js",
    "common.css",
    "common.js",
    "newtab.html",
    "history.html",
    "bookmarks.html",
    "downloads.html",
    "settings.html",
    "about.html",
    "newtab.js",
    "history.js",
    "bookmarks.js",
    "downloads.js",
    "settings.js",
    "about.js",
    "icon.svg",
    "fonts/IBMPlexSans-Regular.ttf",
    "fonts/IBMPlexSans-Medium.ttf",
    "fonts/IBMPlexSans-SemiBold.ttf",
    "fonts/IBMPlexMono-Regular.ttf",
    "fonts/IBMPlexMono-Medium.ttf",
);

pub fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "html" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ttf" => "font/ttf",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

/// Look up a UI file by name (no directories allowed, except `fonts/`).
pub fn get(name: &str) -> Option<Vec<u8>> {
    // Flat namespace, except for the fonts under `fonts/`.
    let flat = name.strip_prefix("fonts/").unwrap_or(name);
    if flat.is_empty() || flat.contains('/') || name.contains("..") || name.contains('\\') {
        return None;
    }
    if let Ok(dir) = std::env::var("FERRITE_UI_DIR") {
        if let Ok(bytes) = std::fs::read(std::path::Path::new(&dir).join(name)) {
            return Some(bytes);
        }
    }
    FILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, bytes)| bytes.to_vec())
}

/// Internal page hosts (`ferrite://<host>/`) and their HTML file.
pub fn page_for_host(host: &str) -> Option<&'static str> {
    Some(match host {
        "app" => "chrome.html",
        "newtab" => "newtab.html",
        "history" => "history.html",
        "bookmarks" => "bookmarks.html",
        "downloads" => "downloads.html",
        "settings" => "settings.html",
        "about" => "about.html",
        _ => return None,
    })
}
