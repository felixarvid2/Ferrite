//! Ad and tracker blocking with Brave's `adblock` engine and the EasyList /
//! EasyPrivacy filter lists (Adblock Plus syntax, network rules only).
//!
//! Lists are cached in the profile (`filter-lists/`) and refreshed when older
//! than [`MAX_AGE`]. Blocking never applies to top-level navigations.

use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::Engine;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Filter lists: (file name, download URL).
pub const LISTS: &[(&str, &str)] = &[
    ("easylist.txt", "https://easylist.to/easylist/easylist.txt"),
    (
        "easyprivacy.txt",
        "https://easylist.to/easylist/easyprivacy.txt",
    ),
];

pub const MAX_AGE: Duration = Duration::from_secs(4 * 24 * 3600);

pub struct Blocker {
    engine: Engine,
    pub rule_lists: usize,
}

impl Blocker {
    pub fn from_lists<S: AsRef<str>>(lists: &[S]) -> Blocker {
        let mut set = FilterSet::new(false);
        for list in lists {
            set.add_filter_list(list.as_ref().to_string(), ParseOptions::default());
        }
        Blocker {
            engine: Engine::new_with_filter_set(set),
            rule_lists: lists.len(),
        }
    }

    /// Load the cached lists from `dir` (None if none are present).
    pub fn load_dir(dir: &Path) -> Option<Blocker> {
        let lists: Vec<String> = LISTS
            .iter()
            .filter_map(|(name, _)| std::fs::read_to_string(dir.join(name)).ok())
            .collect();
        (!lists.is_empty()).then(|| Blocker::from_lists(&lists))
    }

    /// Should a subresource request for `url`, made by a page at
    /// `source_url`, be blocked? `request_type` uses the adblock names
    /// (script, image, stylesheet, xmlhttprequest, sub_frame, media, font,
    /// websocket, ping, other).
    pub fn should_block(&self, url: &str, source_url: &str, request_type: &str) -> bool {
        if request_type == "document" {
            return false;
        }
        match Request::new(url, source_url, request_type, "GET") {
            Ok(request) => self.engine.check_network_request(&request).should_block(),
            Err(_) => false,
        }
    }
}

fn is_stale(path: &Path) -> bool {
    let modified = std::fs::metadata(path).and_then(|m| m.modified());
    match modified {
        Ok(t) => SystemTime::now().duration_since(t).unwrap_or_default() > MAX_AGE,
        Err(_) => true,
    }
}

/// Download missing or stale lists into `dir`. Returns how many lists were
/// updated. Uses the operating system's certificate verifier.
pub fn update_lists(dir: &Path) -> anyhow::Result<usize> {
    use ureq::tls::{RootCerts, TlsConfig};
    std::fs::create_dir_all(dir)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();
    let mut updated = 0;
    for (name, url) in LISTS {
        let path = dir.join(name);
        if !is_stale(&path) {
            continue;
        }
        let body = agent.get(*url).call().and_then(|mut r| {
            r.body_mut()
                .with_config()
                .limit(20 * 1024 * 1024)
                .read_to_string()
        });
        match body {
            // Sanity check: real lists are large and start with a header.
            Ok(text) if text.len() > 10_000 && text.starts_with('[') => {
                crate::write_atomic(&path, text.as_bytes())?;
                updated += 1;
            }
            Ok(_) => tracing::warn!("filter list {url} looks invalid; keeping the old copy"),
            Err(err) => tracing::warn!("downloading filter list {url} failed: {err}"),
        }
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocker() -> Blocker {
        Blocker::from_lists(&["[Adblock Plus 2.0]\n||ads.example.net^\n||tracker.example.org^$third-party\n@@||ads.example.net/allowed.js\n"])
    }

    #[test]
    fn blocks_listed_hosts() {
        let b = blocker();
        assert!(b.should_block(
            "https://ads.example.net/banner.js",
            "https://news.example.com/",
            "script"
        ));
        assert!(!b.should_block(
            "https://cdn.example.com/app.js",
            "https://news.example.com/",
            "script"
        ));
    }

    #[test]
    fn exceptions_and_third_party() {
        let b = blocker();
        assert!(!b.should_block(
            "https://ads.example.net/allowed.js",
            "https://news.example.com/",
            "script"
        ));
        // First-party use of tracker.example.org is allowed by $third-party.
        assert!(!b.should_block(
            "https://tracker.example.org/x.js",
            "https://tracker.example.org/",
            "script"
        ));
        assert!(b.should_block(
            "https://tracker.example.org/x.js",
            "https://shop.example.com/",
            "script"
        ));
    }

    #[test]
    fn never_blocks_documents() {
        assert!(!blocker().should_block("https://ads.example.net/", "", "document"));
    }

    #[test]
    fn load_dir_without_lists_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Blocker::load_dir(dir.path()).is_none());
    }
}
