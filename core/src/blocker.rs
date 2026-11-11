//! Ad and tracker blocking with Brave's `adblock` engine: EasyList /
//! EasyPrivacy plus uBlock Origin's filter lists and scriptlets (the way Brave
//! blocks YouTube ads).
//!
//! Lists are cached in the profile (`filter-lists/`) and refreshed when older
//! than [`MAX_AGE`]. Blocking never applies to top-level navigations.
//! [`Blocker::cosmetic`] returns the script and CSS the app injects into pages.

use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::resources::{PermissionMask, Resource};
use adblock::Engine;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// A filter list. `trusted` lists may use trusted scriptlets (permission bit 0,
/// as in Brave); only uBO's own lists are trusted.
pub struct List {
    pub name: &'static str,
    pub url: &'static str,
    pub trusted: bool,
}

pub const LISTS: &[List] = &[
    List {
        name: "easylist.txt",
        url: "https://easylist.to/easylist/easylist.txt",
        trusted: false,
    },
    List {
        name: "easyprivacy.txt",
        url: "https://easylist.to/easylist/easyprivacy.txt",
        trusted: false,
    },
    List {
        name: "ubo-filters.txt",
        url: "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/filters.txt",
        trusted: true,
    },
    List {
        name: "ubo-quick-fixes.txt",
        url:
            "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/quick-fixes.txt",
        trusted: true,
    },
    List {
        name: "ubo-unbreak.txt",
        url: "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/unbreak.txt",
        trusted: true,
    },
];

/// uBO's quick-fixes list changes often.
pub const MAX_AGE: Duration = Duration::from_secs(24 * 3600);

/// Selectors per generated CSS rule; one invalid selector only drops its chunk.
const CSS_CHUNK: usize = 100;

/// What the app injects into a page: `script` runs in the page's main world at
/// document start, `css` goes into a style element.
pub struct Cosmetic {
    pub script: String,
    pub css: String,
}

pub struct Blocker {
    engine: Engine,
    pub rule_lists: usize,
}

impl Blocker {
    /// Untrusted lists, no scriptlet resources.
    pub fn from_lists<S: AsRef<str>>(lists: &[S]) -> Blocker {
        let lists: Vec<(&str, bool)> = lists.iter().map(|l| (l.as_ref(), false)).collect();
        Blocker::from_lists_with(&lists, None)
    }

    /// `lists` are (text, trusted); `resources_json` is a JSON array of
    /// adblock `Resource`s (uBO scriptlets).
    pub fn from_lists_with<S: AsRef<str>>(
        lists: &[(S, bool)],
        resources_json: Option<&str>,
    ) -> Blocker {
        let mut set = FilterSet::new(false);
        for (text, trusted) in lists {
            let opts = if *trusted {
                ParseOptions {
                    permissions: PermissionMask::from_bits(1),
                    ..Default::default()
                }
            } else {
                ParseOptions::default()
            };
            set.add_filter_list(text.as_ref().to_string(), opts);
        }
        let mut engine = Engine::new_with_filter_set(set);
        if let Some(json) = resources_json {
            match serde_json::from_str::<Vec<Resource>>(json) {
                Ok(resources) => engine.use_resources(resources),
                Err(err) => tracing::warn!("invalid scriptlet resources: {err}"),
            }
        }
        Blocker {
            engine,
            rule_lists: lists.len(),
        }
    }

    /// Load the cached lists from `dir` (None if none are present) and the
    /// scriptlet resources from `resources`. A missing or invalid resources
    /// file only disables scriptlets.
    pub fn load_dir(dir: &Path, resources: Option<&Path>) -> Option<Blocker> {
        let lists: Vec<(String, bool)> = LISTS
            .iter()
            .filter_map(|l| Some((std::fs::read_to_string(dir.join(l.name)).ok()?, l.trusted)))
            .collect();
        if lists.is_empty() {
            return None;
        }
        let json = resources.and_then(|path| match std::fs::read_to_string(path) {
            Ok(json) => Some(json),
            Err(err) => {
                tracing::warn!("cannot read scriptlet resources {}: {err}", path.display());
                None
            }
        });
        Some(Blocker::from_lists_with(&lists, json.as_deref()))
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

    /// Scriptlets and element hiding for a page at `url` (http/https only).
    /// Procedural cosmetic filters and generic class/id hiding are not
    /// supported and are ignored. None when there is nothing to inject.
    pub fn cosmetic(&self, url: &str) -> Option<Cosmetic> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return None;
        }
        let res = self.engine.url_cosmetic_resources(url);
        // uBO scriptlets expect a free `scriptletGlobals` object.
        let script = if res.injected_script.is_empty() {
            String::new()
        } else {
            format!(
                "(function(){{const scriptletGlobals={{}};try{{{}}}catch(e){{}}}})();",
                res.injected_script
            )
        };
        let mut selectors: Vec<&str> = res.hide_selectors.iter().map(String::as_str).collect();
        selectors.sort_unstable();
        let css: String = selectors
            .chunks(CSS_CHUNK)
            .map(|chunk| format!("{}{{display:none!important}}", chunk.join(",")))
            .collect::<Vec<_>>()
            .join("\n");
        (!script.is_empty() || !css.is_empty()).then_some(Cosmetic { script, css })
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
    for List { name, url, .. } in LISTS {
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
            // Sanity check: real lists are large and start with a header
            // (`[Adblock Plus 2.0]` for EasyList, `! Title:` for uBO).
            Ok(text) if text.len() > 1000 && (text.starts_with('[') || text.starts_with('!')) => {
                crate::write_atomic(&path, text.as_bytes())?;
                updated += 1;
            }
            Ok(_) => tracing::warn!("filter list {url} looks invalid; keeping the old copy"),
            Err(err) => tracing::warn!("downloading filter list {url} failed: {err}"),
        }
    }
    Ok(updated)
}

/// Site key of an http(s) URL for the per-site blocking exceptions: the
/// lowercase host without a leading "www." (None for other schemes).
pub fn site_key(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let host = u.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    (!host.is_empty()).then(|| host.to_string())
}

/// Normalise a user-supplied exception ("WWW.Example.com", a full URL) to a
/// site key; None if it names no host.
pub fn normalize_site(input: &str) -> Option<String> {
    let t = input.trim();
    if t.contains("://") {
        return site_key(t);
    }
    site_key(&format!("https://{t}")).filter(|k| !t.contains('/') || k.len() > 1)
}

/// True if `url`'s site is one of the exceptions or a subdomain of one
/// (dot boundary: "ads.example.com" matches "example.com", "badexample.com"
/// does not).
pub fn is_exception(exceptions: &[String], url: &str) -> bool {
    if exceptions.is_empty() {
        return false;
    }
    let Some(key) = site_key(url) else {
        return false;
    };
    exceptions.iter().any(|e| {
        key == *e
            || key
                .strip_suffix(e.as_str())
                .is_some_and(|rest| rest.ends_with('.'))
    })
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
        assert!(Blocker::load_dir(dir.path(), None).is_none());
    }

    const RESOURCES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../resources/adblock/ubo-scriptlets.json"
    );

    fn with_resources(list: &str, trusted: bool) -> Blocker {
        let json = std::fs::read_to_string(RESOURCES).unwrap();
        Blocker::from_lists_with(&[(list, trusted)], Some(&json))
    }

    fn script(b: &Blocker, url: &str) -> String {
        b.cosmetic(url).map(|c| c.script).unwrap_or_default()
    }

    const TRUSTED_RULE: &str =
        "example.com##+js(trusted-rpfr, '\"adPlacements\"', '\"no_ads\"', player?)\n";

    #[test]
    fn trusted_scriptlet_needs_trusted_list() {
        let s = script(&with_resources(TRUSTED_RULE, true), "https://example.com/");
        assert!(s.contains("scriptletGlobals"), "{s}");
        assert!(s.contains("replace-fetch-response") || s.contains("replaceFetchResponse"));
        assert!(s.starts_with("(function(){const scriptletGlobals={};try{"));
        assert!(s.ends_with("catch(e){}})();"));
        assert!(script(&with_resources(TRUSTED_RULE, false), "https://example.com/").is_empty());
        assert!(script(&with_resources(TRUSTED_RULE, true), "https://other.org/").is_empty());
    }

    #[test]
    fn scriptlet_aliases_resolve() {
        let b = with_resources(
            "example.com##+js(set, ytInitialPlayerResponse.adPlacements, undefined)\n",
            false,
        );
        let s = script(&b, "https://example.com/");
        assert!(s.contains("ytInitialPlayerResponse.adPlacements"), "{s}");
    }

    #[test]
    fn json_prune_scriptlet() {
        let b = with_resources(
            "example.com##+js(json-prune, adPlacements playerAds)\n",
            false,
        );
        let s = script(&b, "https://example.com/watch");
        assert!(s.contains("adPlacements"), "{s}");
    }

    #[test]
    fn cosmetic_hiding_per_host() {
        let b = Blocker::from_lists(&["example.com##.ad-banner\nexample.com##.a-first\n"]);
        let c = b.cosmetic("https://example.com/").unwrap();
        assert!(c.script.is_empty());
        assert!(c.css.contains(".ad-banner"));
        assert!(c.css.contains("display:none!important"));
        assert!(c.css.starts_with(".a-first,.ad-banner{"), "{}", c.css);
        assert!(b.cosmetic("https://other.org/").is_none());
        assert!(b.cosmetic("ftp://example.com/").is_none());
        assert!(b.cosmetic("about:blank").is_none());
    }

    #[test]
    fn css_is_chunked() {
        let list: String = (0..250).map(|i| format!("example.com##.ad{i}\n")).collect();
        let c = Blocker::from_lists(&[list])
            .cosmetic("https://example.com/")
            .unwrap();
        assert_eq!(c.css.lines().count(), 3);
    }

    #[test]
    fn missing_resources_file_keeps_blocking() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("easylist.txt"), "||ads.example.net^\n").unwrap();
        let b = Blocker::load_dir(dir.path(), Some(&dir.path().join("nope.json"))).unwrap();
        assert!(b.should_block("https://ads.example.net/a.js", "https://x.com/", "script"));
    }

    /// Run with `FERRITE_LISTS_DIR=<dir with downloaded lists> cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_lists_youtube() {
        let Ok(dir) = std::env::var("FERRITE_LISTS_DIR") else {
            return;
        };
        let b = Blocker::load_dir(Path::new(&dir), Some(Path::new(RESOURCES))).unwrap();
        let c = b.cosmetic("https://www.youtube.com/watch?v=x").unwrap();
        println!("script {} bytes, css {} bytes", c.script.len(), c.css.len());
        assert!(c.script.contains("adPlacements"));
        // The rule families uBO uses against YouTube ads (scriptlet function
        // names as they appear in the injected code).
        for f in [
            "function replaceFetchResponseFn",
            "function jsonPrune",
            "function setConstant",
            "function trustedReplaceXhrResponse",
            "function editInboundObjectFn",
        ] {
            assert!(c.script.contains(f), "missing {f}");
        }
        for url in [
            "https://edition.cnn.com/",
            "https://www.reddit.com/",
            "https://example.org/",
        ] {
            let size = b.cosmetic(url).map(|c| (c.script.len(), c.css.len()));
            println!("{url}: {size:?}");
        }
    }

    #[test]
    fn site_keys() {
        assert_eq!(
            site_key("https://WWW.Example.com/a?b").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            site_key("http://sub.example.com:8080/").as_deref(),
            Some("sub.example.com")
        );
        assert_eq!(
            site_key("http://localhost:8769/x").as_deref(),
            Some("localhost")
        );
        assert_eq!(site_key("ferrite://settings/"), None);
        assert_eq!(site_key("file:///tmp/a.html"), None);
        assert_eq!(site_key("about:blank"), None);
        assert_eq!(site_key("garbage"), None);
    }

    #[test]
    fn exceptions_match_on_dot_boundary() {
        let ex = vec!["example.com".to_string()];
        assert!(is_exception(&ex, "https://example.com/"));
        assert!(is_exception(&ex, "https://www.example.com/x"));
        assert!(is_exception(&ex, "https://a.b.example.com/"));
        assert!(!is_exception(&ex, "https://badexample.com/"));
        assert!(!is_exception(&ex, "https://example.com.evil.net/"));
        assert!(!is_exception(&ex, "ferrite://example.com/"));
        assert!(!is_exception(&[], "https://example.com/"));
    }

    #[test]
    fn normalize_sites() {
        assert_eq!(
            normalize_site(" WWW.Example.com ").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            normalize_site("https://www.a.org/p").as_deref(),
            Some("a.org")
        );
        assert_eq!(normalize_site("ferrite://x"), None);
        assert_eq!(normalize_site(""), None);
    }
}
