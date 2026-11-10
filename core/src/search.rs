//! Built-in search engines and template selection.

use serde::{Deserialize, Serialize};

use crate::settings::Settings;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SearchEngine {
    pub id: String,
    pub name: String,
    pub url_template: String,
    pub suggest_template: Option<String>,
}

pub fn builtin_engines() -> Vec<SearchEngine> {
    let e = |id: &str, name: &str, tpl: &str| SearchEngine {
        id: id.into(),
        name: name.into(),
        url_template: tpl.into(),
        suggest_template: None,
    };
    vec![
        e("google", "Google", "https://www.google.com/search?q=%s"),
        e("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q=%s"),
        e("bing", "Bing", "https://www.bing.com/search?q=%s"),
        e("brave", "Brave", "https://search.brave.com/search?q=%s"),
        e(
            "startpage",
            "Startpage",
            "https://www.startpage.com/do/search?q=%s",
        ),
        e("ecosia", "Ecosia", "https://www.ecosia.org/search?q=%s"),
        e(
            "wikipedia",
            "Wikipedia",
            "https://en.wikipedia.org/wiki/Special:Search?search=%s",
        ),
    ]
}

/// The URL template to use for the current settings (falls back to Google).
pub fn engine_template(settings: &Settings) -> String {
    if settings.search_engine == "custom" && settings.custom_search_url.contains("%s") {
        return settings.custom_search_url.clone();
    }
    let engines = builtin_engines();
    engines
        .iter()
        .find(|e| e.id == settings.search_engine)
        .unwrap_or(&engines[0])
        .url_template
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates() {
        let mut s = Settings::default();
        assert!(engine_template(&s).contains("google.com"));
        s.search_engine = "duckduckgo".into();
        assert!(engine_template(&s).contains("duckduckgo.com"));
        s.search_engine = "nope".into();
        assert!(engine_template(&s).contains("google.com"));
        s.search_engine = "custom".into();
        s.custom_search_url = "https://x.test/?q=%s".into();
        assert_eq!(engine_template(&s), "https://x.test/?q=%s");
        s.custom_search_url = "https://x.test/".into();
        assert!(engine_template(&s).contains("google.com"));
        assert_eq!(builtin_engines().len(), 7);
    }
}
