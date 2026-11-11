//! Proton Pass integration through the official `pass-cli`: parsing of its JSON
//! output and URL matching. Pure logic; running the CLI lives in the app crate.
//!
//! The parsers are deliberately tolerant (the item schema comes from an external
//! crate) and keep only non-secret fields: passwords never leave the raw output.

use serde::Serialize;
use serde_json::Value;

/// A saved login without its password.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Login {
    pub share_id: String,
    pub item_id: String,
    pub title: String,
    pub username: String,
    pub urls: Vec<String>,
}

/// Ids end up in a `pass://` URI and in argv: allow only what real ids contain.
pub fn valid_id(id: &str) -> bool {
    (1..=200).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'=' | b'-'))
}

/// Share ids from `pass-cli vault list --output json`.
pub fn parse_vaults(json: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let list = v.get("vaults").unwrap_or(&v);
    let Some(list) = list.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|e| e.get("share_id").and_then(Value::as_str))
        .filter(|id| valid_id(id))
        .map(str::to_string)
        .collect()
}

/// First object (depth-first) that has a `urls` array.
fn find_urls_object(v: &Value) -> Option<&serde_json::Map<String, Value>> {
    match v {
        Value::Object(m) => {
            if m.get("urls").is_some_and(Value::is_array) {
                return Some(m);
            }
            m.values().find_map(find_urls_object)
        }
        Value::Array(a) => a.iter().find_map(find_urls_object),
        _ => None,
    }
}

fn non_empty_str<'a>(m: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    m.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Logins with at least one URL from `pass-cli item list --output json --show-secrets`.
pub fn parse_logins(json: &str) -> Vec<Login> {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let items = v.get("items").unwrap_or(&v);
    let Some(items) = items.as_array() else {
        return Vec::new();
    };
    items.iter().filter_map(parse_item).collect()
}

fn parse_item(item: &Value) -> Option<Login> {
    if item
        .get("state")
        .is_some_and(|s| s.to_string().to_ascii_lowercase().contains("trash"))
    {
        return None;
    }
    let share_id = item.get("share_id").and_then(Value::as_str)?;
    let item_id = item.get("id").and_then(Value::as_str)?;
    if !valid_id(share_id) || !valid_id(item_id) {
        return None;
    }
    let content = item.get("content")?;
    let login = find_urls_object(content)?;
    let urls: Vec<String> = login
        .get("urls")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_string)
        .collect();
    if urls.is_empty() {
        return None;
    }
    let title = content
        .get("title")
        .and_then(Value::as_str)
        .or_else(|| item.get("title").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    let username = non_empty_str(login, "username")
        .or_else(|| non_empty_str(login, "email"))
        .unwrap_or_default()
        .to_string();
    Some(Login {
        share_id: share_id.to_string(),
        item_id: item_id.to_string(),
        title,
        username,
        urls,
    })
}

/// Lowercased host without a leading `www.`; the scheme defaults to https.
fn norm(url: &str) -> Option<(url::Url, String)> {
    let u = if url.contains("://") {
        url::Url::parse(url).ok()?
    } else {
        url::Url::parse(&format!("https://{url}")).ok()?
    };
    let host = u.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    if host.is_empty() {
        return None;
    }
    Some((u, host))
}

/// Does `login` apply to the page at `page_url`? Same host or a subdomain of an
/// item URL's host; ports and paths are ignored; https items never match http pages.
pub fn matches(login: &Login, page_url: &str) -> bool {
    let Some((page, page_host)) = norm(page_url) else {
        return false;
    };
    if !page_url.contains("://") || !matches!(page.scheme(), "http" | "https") {
        return false;
    }
    login.urls.iter().any(|u| {
        let Some((item, host)) = norm(u) else {
            return false;
        };
        if !matches!(item.scheme(), "http" | "https") {
            return false;
        }
        if item.scheme() == "https" && page.scheme() == "http" {
            return false;
        }
        page_host == host || page_host.ends_with(&format!(".{host}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login(urls: &[&str]) -> Login {
        Login {
            share_id: "S".into(),
            item_id: "I".into(),
            title: "t".into(),
            username: "u".into(),
            urls: urls.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn ids() {
        assert!(valid_id("abc_DEF-123=="));
        assert!(!valid_id(""));
        assert!(!valid_id("a/b"));
        assert!(!valid_id("a b"));
        assert!(!valid_id("a?x=1"));
        assert!(!valid_id("pass://x"));
        assert!(!valid_id("ä"));
        assert!(!valid_id(&"a".repeat(201)));
        assert!(valid_id(&"a".repeat(200)));
    }

    #[test]
    fn vaults() {
        let j = r#"{"vaults":[{"name":"Personal","vault_id":"v1","share_id":"s1"},{"name":"x","vault_id":"v","share_id":"bad/id"},{"share_id":"s2"}]}"#;
        assert_eq!(parse_vaults(j), vec!["s1", "s2"]);
        assert_eq!(parse_vaults(r#"[{"share_id":"s3"}]"#), vec!["s3"]);
        assert!(parse_vaults("nope").is_empty());
        assert!(parse_vaults("{}").is_empty());
    }

    #[test]
    fn logins_externally_tagged() {
        let j = r#"{"items":[{"id":"i1","share_id":"s1","vault_id":"v","state":"Active",
          "content":{"title":"Example","note":"","content":{"Login":{"email":"","username":"alice",
          "password":"hunter2","urls":["https://example.com/login"],"totp_uri":""}}}}]}"#;
        let l = parse_logins(j);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].title, "Example");
        assert_eq!(l[0].username, "alice");
        assert_eq!(l[0].urls, vec!["https://example.com/login"]);
        assert_eq!(
            (l[0].share_id.as_str(), l[0].item_id.as_str()),
            ("s1", "i1")
        );
        assert!(!format!("{:?}", l[0]).contains("hunter2"));
    }

    #[test]
    fn logins_flat_title_and_email_fallback() {
        let j = r#"{"items":[{"id":"i1","share_id":"s1","title":"Flat","state":"Active",
          "content":{"username":"","email":"a@b.c","urls":["example.org"]}}]}"#;
        let l = parse_logins(j);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].title, "Flat");
        assert_eq!(l[0].username, "a@b.c");
    }

    #[test]
    fn logins_skipped() {
        let j = r#"{"items":[
          {"id":"i1","share_id":"s1","state":"Trashed","content":{"title":"a","content":{"Login":{"urls":["https://a.com"]}}}},
          {"id":"i2","share_id":"s1","state":"Active","content":{"title":"n","content":{"Note":{}}}},
          {"id":"i3","share_id":"s1","state":"Active","content":{"title":"nourl","content":{"Login":{"urls":[]}}}},
          {"id":"bad id","share_id":"s1","state":"Active","content":{"title":"x","content":{"Login":{"urls":["https://a.com"]}}}},
          {"id":"i5","share_id":"s1","state":"active","content":{"title":"ok","content":{"Login":{"urls":["https://a.com"]}}}}]}"#;
        let l = parse_logins(j);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].item_id, "i5");
        assert!(parse_logins("garbage").is_empty());
    }

    #[test]
    fn matching() {
        let l = login(&["https://example.com/login"]);
        assert!(matches(&l, "https://example.com/"));
        assert!(matches(&l, "https://www.example.com/x?y=1"));
        assert!(matches(&l, "https://accounts.example.com/"));
        assert!(matches(&l, "https://EXAMPLE.com:8443/"));
        assert!(!matches(&l, "https://evil-example.com/"));
        assert!(!matches(&l, "https://example.com.evil.com/"));
        assert!(!matches(&l, "http://example.com/"));
        assert!(!matches(&l, "ftp://example.com/"));
        assert!(!matches(&l, "ferrite://newtab/"));
        assert!(!matches(&l, "example.com"));
        assert!(!matches(&l, ""));
        let www = login(&["www.example.com"]);
        assert!(matches(&www, "https://example.com/"));
        assert!(!matches(&www, "http://example.com/"));
        let local = login(&["http://localhost:8768"]);
        assert!(matches(&local, "http://localhost:9999/a"));
        assert!(matches(&local, "https://localhost/"));
        assert!(!matches(&local, "http://localhost.evil.com/"));
        assert!(!matches(&login(&[]), "https://example.com/"));
    }
}
