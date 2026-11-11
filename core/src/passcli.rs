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
    /// The item has a TOTP (2FA) secret. Only the fact is kept, never the secret.
    pub has_totp: bool,
}

/// A vault to save new logins into.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Vault {
    pub share_id: String,
    pub name: String,
}

/// Ids end up in a `pass://` URI and in argv: allow only what real ids contain.
pub fn valid_id(id: &str) -> bool {
    (1..=200).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'=' | b'-'))
}

/// Vaults from `pass-cli vault list --output json`.
pub fn parse_vault_infos(json: &str) -> Vec<Vault> {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let list = v.get("vaults").unwrap_or(&v);
    let Some(list) = list.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|e| {
            let share_id = e.get("share_id").and_then(Value::as_str)?;
            if !valid_id(share_id) {
                return None;
            }
            let name = e
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_control())
                .take(100)
                .collect();
            Some(Vault {
                share_id: share_id.to_string(),
                name,
            })
        })
        .collect()
}

/// Share ids from `pass-cli vault list --output json`.
pub fn parse_vaults(json: &str) -> Vec<String> {
    parse_vault_infos(json)
        .into_iter()
        .map(|v| v.share_id)
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
        has_totp: has_totp(content),
    })
}

/// Does the item content hold a TOTP secret? Two shapes are known: the
/// login's `totp_uri` string and an extra field of kind totp (externally
/// tagged `{"Totp": "..."}` or `{"kind":"totp","value":"..."}`). The secret
/// itself is looked at, never returned.
fn has_totp(v: &Value) -> bool {
    match v {
        Value::Object(m) => {
            let filled = |k: &str| {
                m.get(k)
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty())
            };
            if filled("totp_uri") || filled("Totp") || filled("totp") {
                return true;
            }
            let is_totp_kind = ["kind", "type"].iter().any(|k| {
                m.get(*k)
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.eq_ignore_ascii_case("totp"))
            });
            if is_totp_kind && filled("value") {
                return true;
            }
            m.values().any(has_totp)
        }
        Value::Array(a) => a.iter().any(has_totp),
        _ => false,
    }
}

/// The code from `pass-cli item totp … --output json`: an object mapping
/// field names to codes (a login's own secret is "totp"). Only plausible
/// codes (4 to 12 letters or digits) are accepted.
pub fn parse_totp(json: &str) -> Option<String> {
    let v = serde_json::from_str::<Value>(json).ok()?;
    let m = v.as_object()?;
    let ok = |s: &str| (4..=12).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric());
    let pick = m
        .get("totp")
        .and_then(Value::as_str)
        .filter(|s| ok(s))
        .or_else(|| {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            keys.into_iter()
                .filter_map(|k| m.get(k).and_then(Value::as_str))
                .find(|s| ok(s))
        })?;
    Some(pick.to_string())
}

fn looks_like_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.contains('@')
        && !s.contains(char::is_whitespace)
}

/// JSON for `pass-cli item create login --from-template -` (read from stdin,
/// so the password never appears in argv). Proton keeps `email` and
/// `username` apart: an email-looking name goes to `email`.
pub fn login_template(title: &str, username: &str, password: &str, urls: &[String]) -> String {
    let (user, email) = if looks_like_email(username) {
        (Value::Null, Value::from(username))
    } else if username.is_empty() {
        (Value::Null, Value::Null)
    } else {
        (Value::from(username), Value::Null)
    };
    serde_json::json!({
        "title": title,
        "username": user,
        "email": email,
        "password": password,
        "totp_uri": null,
        "urls": urls,
        "extra_fields": [],
    })
    .to_string()
}

/// Is there a cached login for `page_url` with this username (case-insensitive)?
pub fn has_login_for(logins: &[Login], page_url: &str, username: &str) -> bool {
    logins
        .iter()
        .any(|l| matches(l, page_url) && l.username.eq_ignore_ascii_case(username))
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
            has_totp: false,
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

    #[test]
    fn totp_detection_both_shapes() {
        let j = r#"{"items":[
          {"id":"i1","share_id":"s1","state":"Active","content":{"title":"a","content":{"Login":{"username":"u","password":"p","urls":["https://a.com"],"totp_uri":"otpauth://totp/x?secret=ABC"}}}},
          {"id":"i2","share_id":"s1","state":"Active","content":{"title":"b","content":{"Login":{"urls":["https://b.com"],"totp_uri":""}},"extra_fields":[{"name":"2FA","content":{"Totp":"JBSWY3DPEHPK3PXP"}}]}},
          {"id":"i3","share_id":"s1","state":"Active","content":{"title":"c","content":{"Login":{"urls":["https://c.com"],"totp_uri":""}},"extra_fields":[{"name":"t","kind":"totp","value":"JBSWY3DPEHPK3PXP"}]}},
          {"id":"i4","share_id":"s1","state":"Active","content":{"title":"d","content":{"Login":{"urls":["https://d.com"],"totp_uri":""}},"extra_fields":[{"name":"t","content":{"Totp":""}},{"name":"h","content":{"Hidden":"x"}}]}}]}"#;
        let l = parse_logins(j);
        let t: Vec<bool> = l.iter().map(|x| x.has_totp).collect();
        assert_eq!(t, vec![true, true, true, false]);
        // The secret is not kept anywhere in the struct.
        assert!(!format!("{l:?}").contains("JBSWY3DPEHPK3PXP"));
        assert!(!format!("{l:?}").contains("secret=ABC"));
    }

    #[test]
    fn totp_output() {
        assert_eq!(
            parse_totp(r#"{"totp":"123456"}"#).as_deref(),
            Some("123456")
        );
        assert_eq!(
            parse_totp(r#"{"totp":"123456","totp_uri":"123456"}"#).as_deref(),
            Some("123456")
        );
        assert_eq!(
            parse_totp(r#"{"B":"222222","A":"111111"}"#).as_deref(),
            Some("111111")
        );
        assert_eq!(parse_totp(r#"{"totp":"12 3456"}"#), None);
        assert_eq!(parse_totp(r#"{"totp":"1"}"#), None);
        assert_eq!(parse_totp("totp: 123456"), None);
        assert_eq!(parse_totp("{}"), None);
    }

    #[test]
    fn vault_infos() {
        let j = r#"{"vaults":[{"name":"Per\nsonal","vault_id":"v1","share_id":"s1"},{"name":"x","share_id":"bad/id"},{"share_id":"s2"}]}"#;
        let v = parse_vault_infos(j);
        assert_eq!(v.len(), 2);
        assert_eq!(
            (v[0].share_id.as_str(), v[0].name.as_str()),
            ("s1", "Personal")
        );
        assert_eq!(v[1].name, "");
    }

    #[test]
    fn template_json_escapes_and_splits_email() {
        let urls = vec!["https://example.com".to_string()];
        let t = login_template("ex\"ample", "bob", "p\"w\\\n</x>\u{2028}", &urls);
        let v: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["title"], "ex\"ample");
        assert_eq!(v["password"], "p\"w\\\n</x>\u{2028}");
        assert_eq!(v["username"], "bob");
        assert!(v["email"].is_null());
        assert_eq!(v["urls"][0], "https://example.com");
        assert!(!t.contains('\n'), "single line");
        let v: Value = serde_json::from_str(&login_template("t", "a@b.co", "p", &urls)).unwrap();
        assert_eq!(v["email"], "a@b.co");
        assert!(v["username"].is_null());
        let v: Value = serde_json::from_str(&login_template("t", "", "p", &urls)).unwrap();
        assert!(v["email"].is_null() && v["username"].is_null());
        // "@" alone or spaces are not emails.
        let v: Value = serde_json::from_str(&login_template("t", "a b@c.d", "p", &urls)).unwrap();
        assert_eq!(v["username"], "a b@c.d");
    }

    #[test]
    fn existing_login_detection() {
        let mut l = login(&["https://example.com"]);
        l.username = "Alice".into();
        let list = vec![l];
        assert!(has_login_for(&list, "https://example.com/login", "alice"));
        assert!(!has_login_for(&list, "https://example.com/", "bob"));
        assert!(!has_login_for(&list, "https://other.com/", "alice"));
    }
}
