//! Address-bar input handling: URL vs. search resolution and URL helpers.

use std::net::{Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};
use url::Url;

/// Schemes accepted as typed. `javascript:` is deliberately absent.
const KNOWN_SCHEMES: &[&str] = &[
    "http",
    "https",
    "file",
    "ferrite",
    "about",
    "chrome",
    "data",
    "view-source",
    "blob",
    "ftp",
];

/// Schemes that must be written with `://`.
const AUTHORITY_SCHEMES: &[&str] = &["http", "https", "file", "ferrite", "chrome", "ftp"];

/// Non-IANA "TLDs" that are still treated as domains.
const PSEUDO_TLDS: &[&str] = &[
    "localhost",
    "local",
    "internal",
    "test",
    "lan",
    "home",
    "corp",
    "invalid",
    "example",
    "onion",
];

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityLevel {
    Secure,
    Insecure,
    Internal,
    Local,
    None,
}

/// Percent-encode `query` (form style: space -> `+`) into `search_template`'s `%s`.
pub fn search_url(query: &str, search_template: &str) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    search_template.replace("%s", &encoded)
}

/// Turn address-bar text into a URL (or a search URL).
pub fn resolve_input(input: &str, search_template: &str) -> String {
    let input = input.trim();
    if input.is_empty() {
        return String::new();
    }
    if let Some(q) = input.strip_prefix('?') {
        return search_url(q.trim(), search_template);
    }
    if has_known_scheme(input) && Url::parse(input).is_ok() {
        return input.to_string();
    }
    if !input.chars().any(char::is_whitespace) {
        if let Some(prefix) = implied_scheme(input) {
            return format!("{prefix}{input}");
        }
    }
    search_url(input, search_template)
}

fn has_known_scheme(input: &str) -> bool {
    let Some((scheme, rest)) = input.split_once(':') else {
        return false;
    };
    let scheme = scheme.to_ascii_lowercase();
    if !KNOWN_SCHEMES.contains(&scheme.as_str()) {
        return false;
    }
    !AUTHORITY_SCHEMES.contains(&scheme.as_str()) || rest.starts_with("//")
}

/// `http://` for localhost/IPs, `https://` for domain-like text, `None` for search.
fn implied_scheme(input: &str) -> Option<&'static str> {
    let end = input.find(['/', '?', '#']).unwrap_or(input.len());
    let authority = &input[..end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let close = inner.find(']')?;
        let ip = &inner[..close];
        let after = &inner[close + 1..];
        ip.parse::<Ipv6Addr>().ok()?;
        if !after.is_empty() && !valid_port(after.strip_prefix(':')?) {
            return None;
        }
        return Some("http://");
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };
    if let Some(p) = port {
        if !valid_port(p) {
            return None;
        }
    }
    if host.eq_ignore_ascii_case("localhost") || host.parse::<Ipv4Addr>().is_ok() {
        return Some("http://");
    }
    looks_like_domain(host).then_some("https://")
}

fn valid_port(p: &str) -> bool {
    !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit())
}

fn looks_like_domain(host: &str) -> bool {
    if !host.contains('.') {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.iter().any(|l| {
        l.is_empty()
            || !l
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    }) {
        return false;
    }
    let tld = labels[labels.len() - 1].to_ascii_lowercase();
    let alphabetic = (2..=63).contains(&tld.len()) && tld.chars().all(|c| c.is_alphabetic());
    alphabetic || tld.starts_with("xn--") || PSEUDO_TLDS.contains(&tld.as_str())
}

pub fn security_level(url: &str) -> SecurityLevel {
    let Ok(u) = Url::parse(url) else {
        return SecurityLevel::None;
    };
    match u.scheme() {
        "https" | "wss" => SecurityLevel::Secure,
        "http" | "ws" => SecurityLevel::Insecure,
        "ferrite" | "chrome" | "about" | "devtools" => SecurityLevel::Internal,
        "file" | "data" | "blob" => SecurityLevel::Local,
        _ => SecurityLevel::None,
    }
}

/// Text to show in the address bar (the new-tab page shows an empty box).
pub fn display_url(url: &str) -> String {
    match url {
        "ferrite://newtab/" | "ferrite://newtab" => String::new(),
        _ => url.to_string(),
    }
}

pub fn host_of(url: &str) -> Option<String> {
    Url::parse(url).ok()?.host_str().map(str::to_string)
}

/// `scheme://host[:port]` for tuple origins; `None` for opaque ones (file, data, ...).
pub fn origin_of(url: &str) -> Option<String> {
    let origin = Url::parse(url).ok()?.origin();
    origin.is_tuple().then(|| origin.ascii_serialization())
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "https://s.test/?q=%s";

    #[test]
    fn resolution_table() {
        let cases = [
            ("", ""),
            ("   ", ""),
            ("example.com", "https://example.com"),
            ("  example.com  ", "https://example.com"),
            (
                "en.wikipedia.org/wiki/Rust",
                "https://en.wikipedia.org/wiki/Rust",
            ),
            ("github.com:443", "https://github.com:443"),
            ("example.com/a?b=c#d", "https://example.com/a?b=c#d"),
            ("localhost", "http://localhost"),
            ("localhost:8080", "http://localhost:8080"),
            ("localhost:3000/x", "http://localhost:3000/x"),
            ("192.168.0.1/admin", "http://192.168.0.1/admin"),
            ("192.168.1.1:8080", "http://192.168.1.1:8080"),
            ("[::1]:8080", "http://[::1]:8080"),
            ("hello world", "https://s.test/?q=hello+world"),
            ("rust", "https://s.test/?q=rust"),
            ("what is 1.5", "https://s.test/?q=what+is+1.5"),
            ("1.5", "https://s.test/?q=1.5"),
            ("https://x.org", "https://x.org"),
            ("http://x.org/a b", "http://x.org/a b"),
            ("ferrite://settings", "ferrite://settings"),
            ("about:blank", "about:blank"),
            ("file:///tmp/a.html", "file:///tmp/a.html"),
            (
                "javascript:alert(1)",
                "https://s.test/?q=javascript%3Aalert%281%29",
            ),
            ("?example.com", "https://s.test/?q=example.com"),
            ("? a&b", "https://s.test/?q=a%26b"),
            ("user@example.com", "https://s.test/?q=user%40example.com"),
            ("foo.bar1", "https://s.test/?q=foo.bar1"),
            ("example.com:abc", "https://s.test/?q=example.com%3Aabc"),
            ("a..com", "https://s.test/?q=a..com"),
            ("http:/broken", "https://s.test/?q=http%3A%2Fbroken"),
            ("printer.local", "https://printer.local"),
        ];
        for (input, want) in cases {
            assert_eq!(resolve_input(input, T), want, "input {input:?}");
        }
    }

    #[test]
    fn search_encoding() {
        assert_eq!(
            search_url("a b&c=d/é", T),
            "https://s.test/?q=a+b%26c%3Dd%2F%C3%A9"
        );
    }

    #[test]
    fn security() {
        use SecurityLevel::*;
        for (u, want) in [
            ("https://a.com", Secure),
            ("wss://a.com", Secure),
            ("http://a.com", Insecure),
            ("ferrite://settings", Internal),
            ("chrome://version", Internal),
            ("about:blank", Internal),
            ("devtools://devtools/x", Internal),
            ("file:///a", Local),
            ("data:text/plain,hi", Local),
            ("ftp://a.com", None),
            ("garbage", None),
        ] {
            assert_eq!(security_level(u), want, "{u}");
        }
        assert_eq!(serde_json::to_string(&Secure).unwrap(), "\"secure\"");
    }

    #[test]
    fn display_host_origin() {
        assert_eq!(display_url("ferrite://newtab/"), "");
        assert_eq!(display_url("ferrite://newtab"), "");
        assert_eq!(display_url("https://a.com/"), "https://a.com/");
        assert_eq!(
            host_of("https://www.a.com:8080/x").as_deref(),
            Some("www.a.com")
        );
        assert_eq!(host_of("nonsense"), None);
        assert_eq!(
            origin_of("https://example.com/a/b").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            origin_of("https://example.com:443/").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            origin_of("http://example.com:8080/").as_deref(),
            Some("http://example.com:8080")
        );
        assert_eq!(origin_of("file:///a"), None);
    }
}
