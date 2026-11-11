use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::resources::{PermissionMask, Resource};
use adblock::Engine;

/// Selectors per generated CSS rule; one invalid selector only drops its chunk.
const CSS_CHUNK: usize = 100;

pub struct Blocker {
    engine: Engine,
}

impl Blocker {
    /// `lists` are (text, trusted); trusted lists (uBO's own) may use trusted
    /// scriptlets (permission bit 0, as in Brave). `resources_json` is a JSON
    /// array of adblock `Resource`s (uBO scriptlets); invalid JSON only
    /// disables scriptlets.
    pub fn new(lists: &[(String, bool)], resources_json: Option<&str>) -> Blocker {
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
            set.add_filter_list(text.clone(), opts);
        }
        let mut engine = Engine::new_with_filter_set(set);
        if let Some(json) = resources_json {
            if let Ok(resources) = serde_json::from_str::<Vec<Resource>>(json) {
                engine.use_resources(resources);
            }
        }
        Blocker { engine }
    }

    /// Should a subresource request for `url`, made by a page at
    /// `source_url`, be blocked? `request_type` uses the adblock names
    /// (script, image, stylesheet, xmlhttprequest, sub_frame, media, font,
    /// websocket, ping, other). Documents are never blocked.
    pub fn should_block(&self, url: &str, source_url: &str, request_type: &str) -> bool {
        if request_type == "document" {
            return false;
        }
        match Request::new(url, source_url, request_type, "GET") {
            Ok(request) => self.engine.check_network_request(&request).should_block(),
            Err(_) => false,
        }
    }

    /// The script to register for document start on a page at `url` (http/https
    /// only): uBO scriptlets plus cosmetic hiding as a constructed stylesheet,
    /// guarded by `location.hostname` so a stale registration does nothing on
    /// another host. None when there is nothing to inject.
    pub fn cosmetic(&self, url: &str) -> Option<String> {
        let host = http_host(url)?;
        let res = self.engine.url_cosmetic_resources(url);
        // uBO scriptlets expect a free `scriptletGlobals` object.
        let mut js = if res.injected_script.is_empty() {
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
        if !css.is_empty() {
            // A constructed stylesheet needs no DOM node, so it works at
            // document start; invalid rules are dropped one by one.
            let css = serde_json::Value::String(css);
            js.push_str(&format!(
                "\ntry{{const s=new CSSStyleSheet();s.replaceSync({css});\
                 document.adoptedStyleSheets=[...document.adoptedStyleSheets,s];}}catch(e){{}}"
            ));
        }
        if js.is_empty() {
            return None;
        }
        let host = serde_json::Value::String(host);
        Some(format!("if(location.hostname==={host}){{{js}\n}}"))
    }
}

fn http_host(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    matches!(u.scheme(), "http" | "https").then(|| u.host_str().map(str::to_ascii_lowercase))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lists(text: &str, trusted: bool) -> Vec<(String, bool)> {
        vec![(text.to_string(), trusted)]
    }

    fn blocker() -> Blocker {
        Blocker::new(
            &lists(
                "[Adblock Plus 2.0]\n||ads.example.net^\n||tracker.example.org^$third-party\n@@||ads.example.net/allowed.js\n",
                false,
            ),
            None,
        )
    }

    const RESOURCES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/adblock/ubo-scriptlets.json"
    );

    fn with_resources(list: &str, trusted: bool) -> Blocker {
        let json = std::fs::read_to_string(RESOURCES).unwrap();
        Blocker::new(&lists(list, trusted), Some(&json))
    }

    #[test]
    fn blocks_listed_hosts_and_honours_exceptions() {
        let b = blocker();
        let src = "https://news.example.com/";
        assert!(b.should_block("https://ads.example.net/banner.js", src, "script"));
        assert!(!b.should_block("https://cdn.example.com/app.js", src, "script"));
        assert!(!b.should_block("https://ads.example.net/allowed.js", src, "script"));
        assert!(!b.should_block(
            "https://tracker.example.org/x.js",
            "https://tracker.example.org/",
            "script"
        ));
        assert!(b.should_block("https://tracker.example.org/x.js", src, "script"));
        assert!(!b.should_block("https://ads.example.net/", "", "document"));
        assert!(!b.should_block("not a url", src, "script"));
    }

    #[test]
    fn trusted_scriptlet_needs_trusted_list() {
        let rule = "example.com##+js(trusted-rpfr, '\"adPlacements\"', '\"no_ads\"', player?)\n";
        let s = with_resources(rule, true)
            .cosmetic("https://example.com/")
            .unwrap();
        assert!(s.starts_with("if(location.hostname===\"example.com\"){(function(){const scriptletGlobals={};try{"), "{s}");
        assert!(with_resources(rule, false)
            .cosmetic("https://example.com/")
            .is_none());
        assert!(with_resources(rule, true)
            .cosmetic("https://other.org/")
            .is_none());
    }

    #[test]
    fn scriptlet_aliases_resolve() {
        let b = with_resources(
            "example.com##+js(set, ytInitialPlayerResponse.adPlacements, undefined)\n",
            false,
        );
        assert!(b
            .cosmetic("https://example.com/")
            .unwrap()
            .contains("ytInitialPlayerResponse.adPlacements"));
    }

    #[test]
    fn cosmetic_css_is_guarded_and_chunked() {
        let list: String = (0..250).map(|i| format!("example.com##.ad{i}\n")).collect();
        let b = Blocker::new(&lists(&list, false), None);
        let js = b.cosmetic("https://example.com/").unwrap();
        assert!(js.starts_with("if(location.hostname===\"example.com\"){"));
        assert!(js.contains("adoptedStyleSheets"));
        assert!(js.contains("display:none!important"));
        assert_eq!(js.matches("display:none!important}").count(), 3);
        assert!(b.cosmetic("https://other.org/").is_none());
        assert!(b.cosmetic("ftp://example.com/").is_none());
        assert!(b.cosmetic("about:blank").is_none());
    }

    #[test]
    fn ip_and_localhost_hosts_work_with_ports() {
        // The emulator test serves its page from these.
        let b = with_resources(
            "127.0.0.1##+js(set-constant, ferriteMarker, ok)\nlocalhost##+js(set-constant, ferriteMarker, ok)\n/ferrite-blocked.js\n",
            false,
        );
        for url in ["http://127.0.0.1:8123/", "http://localhost:8123/"] {
            let js = b.cosmetic(url).unwrap_or_default();
            assert!(js.contains("ferriteMarker"), "{url}: {js}");
            assert!(b.should_block(&format!("{url}ferrite-blocked.js"), url, "script"));
            assert!(!b.should_block(&format!("{url}ok.js"), url, "script"));
        }
    }

    #[test]
    fn bad_resources_keep_blocking() {
        let b = Blocker::new(&lists("||ads.example.net^\n", false), Some("not json"));
        assert!(b.should_block("https://ads.example.net/a.js", "https://x.com/", "script"));
    }
}
