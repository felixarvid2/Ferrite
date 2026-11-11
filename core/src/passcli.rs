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

/// The cached login for `page_url` with this username (case-insensitive).
pub fn existing_login<'a>(
    logins: &'a [Login],
    page_url: &str,
    username: &str,
) -> Option<&'a Login> {
    logins
        .iter()
        .find(|l| matches(l, page_url) && l.username.eq_ignore_ascii_case(username))
}

/// Is there a cached login for `page_url` with this username (case-insensitive)?
pub fn has_login_for(logins: &[Login], page_url: &str, username: &str) -> bool {
    existing_login(logins, page_url, username).is_some()
}

// ---------------------------------------------------------------- replacing

/// Everything of a login item that `item create login --from-template` can
/// carry, read from `item view --output json`. Holds secrets (the TOTP seed),
/// so it has no `Debug`; the old password is deliberately not read.
#[derive(Clone)]
pub struct LoginCopy {
    title: String,
    username: Option<String>,
    email: Option<String>,
    totp_uri: Option<String>,
    urls: Vec<String>,
    /// `(kind, title, value)`, kind is `text`, `hidden` or `totp`.
    extra_fields: Vec<(&'static str, String, String)>,
    folder_id: Option<String>,
}

pub enum Replacement {
    /// The item can be recreated without losing data.
    Copy(LoginCopy),
    /// The item has data the template cannot carry (note, attachments,
    /// passkeys, custom sections, other field kinds): do not touch it.
    Uncopyable,
}

impl LoginCopy {
    /// Folder of the original item (for `--folder-id`), if it had one.
    pub fn folder_id(&self) -> Option<&str> {
        self.folder_id.as_deref()
    }

    /// Template JSON (one line, for stdin) with `password` in place of the old one.
    pub fn template(&self, password: &str) -> String {
        let extra: Vec<Value> = self
            .extra_fields
            .iter()
            .map(|(kind, title, value)| {
                serde_json::json!({ "kind": kind, "title": title, "value": value })
            })
            .collect();
        serde_json::json!({
            "title": self.title,
            "username": self.username,
            "email": self.email,
            "password": password,
            "totp_uri": self.totp_uri,
            "urls": self.urls,
            "extra_fields": extra,
        })
        .to_string()
    }
}

fn filled(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.trim().is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Number(_) => true,
    }
}

fn opt_string(m: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    non_empty_str(m, key).map(str::to_string)
}

/// One extra field in either known shape: `{"name", "content": {"Hidden": v}}`
/// or `{"name", "kind": "hidden", "value": v}`. `Err(())` = a kind the
/// template cannot carry (or an unreadable field). `Ok(None)` = empty TOTP.
#[allow(clippy::type_complexity)]
fn extra_field(f: &Value) -> Result<Option<(&'static str, String, String)>, ()> {
    let m = f.as_object().ok_or(())?;
    let title = ["name", "title", "field_name", "label"]
        .iter()
        .find_map(|k| m.get(*k).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    let (kind, value) = if let Some(c) = m.get("content").and_then(Value::as_object) {
        let mut it = c.iter();
        match (it.next(), it.next()) {
            (Some((k, v)), None) => (k.to_ascii_lowercase(), v.as_str().ok_or(())?.to_string()),
            _ => return Err(()),
        }
    } else {
        let k = ["kind", "type"]
            .iter()
            .find_map(|k| m.get(*k).and_then(Value::as_str))
            .ok_or(())?;
        let v = m.get("value").map_or(Some(""), Value::as_str).ok_or(())?;
        (k.to_ascii_lowercase(), v.to_string())
    };
    let kind = match kind.as_str() {
        "text" => "text",
        "hidden" => "hidden",
        "totp" if value.trim().is_empty() => return Ok(None),
        "totp" => "totp",
        _ => return Err(()),
    };
    Ok(Some((kind, title, value)))
}

/// Read the output of `pass-cli item view --share-id S --item-id I --output
/// json` and decide whether the login can be recreated. `None` = not
/// readable, or not the item asked for. Both item shapes seen in
/// [`parse_logins`] are accepted, and anything unknown that holds data counts
/// as uncopyable (better to refuse than to lose it).
pub fn plan_replacement(json: &str, share_id: &str, item_id: &str) -> Option<Replacement> {
    let root: Value = serde_json::from_str(json).ok()?;
    let item = root.get("item").filter(|i| i.is_object()).unwrap_or(&root);
    for (key, want) in [("id", item_id), ("share_id", share_id)] {
        if item
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|v| v != want)
        {
            return None;
        }
    }
    let content = item.get("content")?.as_object()?;
    let attachments = [root.get("attachments"), item.get("attachments")];
    if attachments.iter().flatten().any(|a| filled(a)) {
        return Some(Replacement::Uncopyable);
    }
    // Nested (`content.content.Login`) or flat (`content` is the login).
    let inner = content.get("content").unwrap_or(&Value::Null);
    let (login, nested) = match inner.get("Login") {
        Some(l) => (l.as_object()?, true),
        None if inner.is_null() => (content, false),
        None => return Some(Replacement::Uncopyable), // another item type
    };
    const LOGIN_KEYS: [&str; 7] = [
        "username",
        "email",
        "password",
        "urls",
        "totp_uri",
        "passkeys",
        "extra_fields",
    ];
    const CONTENT_KEYS: [&str; 5] = ["title", "note", "item_uuid", "content", "extra_fields"];
    let note = content.get("note").or_else(|| item.get("note"));
    let unknown_data = |m: &serde_json::Map<String, Value>, ok: &[&str], also: &[&str]| {
        m.iter()
            .any(|(k, v)| !ok.contains(&k.as_str()) && !also.contains(&k.as_str()) && filled(v))
    };
    let flat_extra: &[&str] = if nested { &[] } else { &LOGIN_KEYS };
    if note.is_some_and(filled)
        || unknown_data(content, &CONTENT_KEYS, flat_extra)
        || unknown_data(login, &LOGIN_KEYS, if nested { &[] } else { &CONTENT_KEYS })
        || login.get("passkeys").is_some_and(filled)
    {
        return Some(Replacement::Uncopyable);
    }
    let mut extra_fields = Vec::new();
    let lists = [
        content.get("extra_fields"),
        nested.then(|| login.get("extra_fields")).flatten(),
    ];
    for list in lists.into_iter().flatten() {
        let Some(list) = list.as_array() else {
            return Some(Replacement::Uncopyable);
        };
        for f in list {
            match extra_field(f) {
                Ok(Some(e)) => extra_fields.push(e),
                Ok(None) => {}
                Err(()) => return Some(Replacement::Uncopyable),
            }
        }
    }
    let urls = login
        .get("urls")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_string)
        .collect();
    let title = content
        .get("title")
        .or_else(|| item.get("title"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some(Replacement::Copy(LoginCopy {
        title,
        username: opt_string(login, "username"),
        email: opt_string(login, "email"),
        totp_uri: opt_string(login, "totp_uri"),
        urls,
        extra_fields,
        folder_id: item
            .get("folder_id")
            .and_then(Value::as_str)
            .filter(|f| valid_id(f))
            .map(str::to_string),
    }))
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

    fn tpl(c: &LoginCopy) -> Value {
        serde_json::from_str(&c.template("NEW\"pw")).unwrap()
    }

    fn copy_of(j: &str) -> LoginCopy {
        match plan_replacement(j, "S", "I") {
            Some(Replacement::Copy(c)) => c,
            Some(Replacement::Uncopyable) => panic!("uncopyable"),
            None => panic!("unreadable"),
        }
    }

    fn uncopyable(j: &str) -> bool {
        matches!(plan_replacement(j, "S", "I"), Some(Replacement::Uncopyable))
    }

    #[test]
    fn replacement_nested_shape() {
        let j = r#"{"item":{"id":"I","share_id":"S","folder_id":"F1","state":"Active",
          "content":{"title":"Example","note":"","item_uuid":"u","content":{"Login":{"email":"a@b.co","username":"alice",
          "password":"OLD","urls":["https://example.com","https://x.org"],"totp_uri":"otpauth://totp/x?secret=ABC","passkeys":[]}},
          "extra_fields":[{"name":"Recovery","content":{"Hidden":"rk"}},{"name":"Pin","content":{"Text":"1234"}},
          {"name":"2FA","content":{"Totp":"otpauth://totp/y?secret=DEF"}},{"name":"empty","content":{"Totp":""}}]}},
          "attachments":[]}"#;
        let c = copy_of(j);
        let t = tpl(&c);
        assert_eq!(t["title"], "Example");
        assert_eq!(t["username"], "alice");
        assert_eq!(t["email"], "a@b.co");
        assert_eq!(t["password"], "NEW\"pw");
        assert_eq!(t["totp_uri"], "otpauth://totp/x?secret=ABC");
        assert_eq!(
            t["urls"],
            serde_json::json!(["https://example.com", "https://x.org"])
        );
        assert_eq!(
            t["extra_fields"],
            serde_json::json!([
                {"kind":"hidden","title":"Recovery","value":"rk"},
                {"kind":"text","title":"Pin","value":"1234"},
                {"kind":"totp","title":"2FA","value":"otpauth://totp/y?secret=DEF"}
            ])
        );
        assert_eq!(c.folder_id(), Some("F1"));
        assert!(!c.template("p").contains("OLD"), "old password not copied");
        assert!(!c.template("p").contains('\n'));
    }

    #[test]
    fn replacement_flat_shape_and_kind_value_fields() {
        let j = r#"{"id":"I","share_id":"S","title":"Flat","content":{"username":"","email":"a@b.c","password":"OLD",
          "urls":["example.org"],"totp_uri":"",
          "extra_fields":[{"name":"t","kind":"totp","value":"otpauth://totp/z?secret=GHI"},{"name":"h","kind":"Hidden","value":"v"}]}}"#;
        let c = copy_of(j);
        let t = tpl(&c);
        assert_eq!(t["title"], "Flat");
        assert!(t["username"].is_null() && t["totp_uri"].is_null());
        assert_eq!(t["email"], "a@b.c");
        assert_eq!(
            t["extra_fields"].as_array().unwrap().len(),
            2,
            "no duplicates"
        );
        assert_eq!(t["extra_fields"][0]["kind"], "totp");
        assert_eq!(t["extra_fields"][1]["kind"], "hidden");
        assert_eq!(c.folder_id(), None);
    }

    #[test]
    fn replacement_detects_uncopyable_data() {
        let base = |login: &str, content_extra: &str, item_extra: &str| {
            format!(
                r#"{{"item":{{"id":"I","share_id":"S"{item_extra},"content":{{"title":"t","note":"" {content_extra},
                "content":{{"Login":{login}}}}}}}}}"#
            )
        };
        let ok = r#"{"username":"u","password":"p","urls":["https://a.com"],"totp_uri":null,"passkeys":[]}"#;
        assert!(!uncopyable(&base(ok, "", "")));
        // A note (the template has no note field).
        assert!(uncopyable(
            &base(ok, "", "").replace(r#""note":"""#, r#""note":"keep me""#)
        ));
        // Attachments, at the root or in the item.
        assert!(uncopyable(&format!(
            r#"{{"attachments":[{{"id":"a"}}],{}"#,
            &base(ok, "", "")[1..]
        )));
        assert!(uncopyable(&base(ok, "", r#","attachments":[{"id":"a"}]"#)));
        // Passkeys.
        let pk = r#"{"username":"u","password":"p","urls":["https://a.com"],"passkeys":[{"key_id":"k"}]}"#;
        assert!(uncopyable(&base(pk, "", "")));
        // Custom sections / unknown content.
        assert!(uncopyable(&base(
            ok,
            r#","sections":[{"title":"s","fields":[]}]"#,
            ""
        )));
        let unk = r#"{"username":"u","urls":["https://a.com"],"something_new":"x"}"#;
        assert!(uncopyable(&base(unk, "", "")));
        // Field kinds the template rejects.
        assert!(uncopyable(&base(
            ok,
            r#","extra_fields":[{"name":"d","content":{"Timestamp":"1"}}]"#,
            ""
        )));
        // Not a login.
        assert!(uncopyable(
            r#"{"item":{"id":"I","share_id":"S","content":{"title":"n","content":{"Note":{}}}}}"#
        ));
        // Empty values of unknown keys are harmless.
        let empty = r#"{"username":"u","urls":["https://a.com"],"something_new":"","other":[]}"#;
        assert!(!uncopyable(&base(empty, "", "")));
    }

    #[test]
    fn replacement_rejects_wrong_or_broken_item() {
        let j = r#"{"item":{"id":"OTHER","share_id":"S","content":{"title":"t","content":{"Login":{"urls":["https://a.com"]}}}}}"#;
        assert!(plan_replacement(j, "S", "I").is_none());
        assert!(plan_replacement("garbage", "S", "I").is_none());
        assert!(plan_replacement("{}", "S", "I").is_none());
    }
}
