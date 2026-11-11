//! Update check: finds out whether a newer Ferrite release exists on GitHub.
//!
//! Pure parsing and comparison live here (tested with fixture JSON); the app
//! runs [`fetch`] on a background thread. Nothing is downloaded or installed.

use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

/// Releases of the Ferrite repository, newest first (GitHub's order).
pub const RELEASES_URL: &str =
    "https://api.github.com/repos/felixarvid2/Ferrite/releases?per_page=20";
/// Release tags look like `ferrite-v0.1.1`; other tags in the repo are ignored.
const TAG_PREFIX: &str = "ferrite-v";
/// Responses larger than this are rejected.
const MAX_BODY: u64 = 1024 * 1024;

/// Never check more often than this (automatic checks).
pub const MIN_GAP_MS: i64 = 6 * 3600 * 1000;
/// Automatic re-check interval while the browser keeps running.
pub const INTERVAL_MS: i64 = 24 * 3600 * 1000;

/// A release newer than the running version.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    /// The release page (`html_url`).
    pub url: String,
    pub deb_url: Option<String>,
    pub appimage_url: Option<String>,
}

/// What is remembered between runs (`update.json` in the profile).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default)]
pub struct UpdateState {
    /// Unix ms of the last attempt (successful or not), 0 = never.
    pub last_update_check_ms: i64,
    /// Unix ms of the last attempt that got an answer, 0 = never.
    pub last_ok_ms: i64,
    pub update: Option<Update>,
}

impl UpdateState {
    /// Missing or corrupt file gives the empty state. An update that is not
    /// newer than `current` (the browser was updated since) is dropped.
    pub fn load(path: &Path, current: &str) -> UpdateState {
        let mut st: UpdateState = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if let Some(u) = &st.update {
            if !is_newer(&u.version, current) {
                st.update = None;
            }
        }
        st
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        crate::write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }
}

/// True if a check is allowed: at least `gap_ms` since the last one. A clock
/// that went backwards counts as due.
pub fn is_due(last_ms: i64, now_ms: i64, gap_ms: i64) -> bool {
    last_ms <= 0 || now_ms < last_ms || now_ms - last_ms >= gap_ms
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    /// Dot-separated pre-release identifiers (empty = a normal release).
    pre: Vec<Ident>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ident {
    Num(u64),
    Text(String),
}

impl PartialOrd for Ident {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Ident {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        match (self, o) {
            (Ident::Num(a), Ident::Num(b)) => a.cmp(b),
            (Ident::Num(_), Ident::Text(_)) => std::cmp::Ordering::Less,
            (Ident::Text(_), Ident::Num(_)) => std::cmp::Ordering::Greater,
            (Ident::Text(a), Ident::Text(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Version {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        (self.major, self.minor, self.patch)
            .cmp(&(o.major, o.minor, o.patch))
            .then_with(|| match (self.pre.is_empty(), o.pre.is_empty()) {
                (true, true) => Equal,
                // A release is newer than its pre-releases.
                (true, false) => Greater,
                (false, true) => Less,
                (false, false) => self.pre.cmp(&o.pre),
            })
    }
}

/// Parse `MAJOR.MINOR.PATCH[-pre][+build]` (build metadata is ignored).
fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim();
    let s = s.split('+').next()?;
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (s, None),
    };
    let mut parts = core.split('.');
    let mut num = || {
        let p = parts.next()?;
        // No empty parts, no signs, no leading "+".
        (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse::<u64>().ok())
            .flatten()
    };
    let (major, minor, patch) = (num()?, num()?, num()?);
    if parts.next().is_some() {
        return None;
    }
    let pre = match pre {
        None => Vec::new(),
        Some(p) => p
            .split('.')
            .map(|i| {
                if i.is_empty() || !i.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    None
                } else if i.bytes().all(|b| b.is_ascii_digit()) {
                    i.parse().ok().map(Ident::Num)
                } else {
                    Some(Ident::Text(i.to_string()))
                }
            })
            .collect::<Option<Vec<_>>>()?,
    };
    Some(Version {
        major,
        minor,
        patch,
        pre,
    })
}

/// True if `candidate` is a valid version greater than the valid `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(c), Some(cur)) => c > cur,
        _ => false,
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Only plain web links reach the UI (never `javascript:`, `ferrite:` or `file:`).
fn web_url(u: &str) -> Option<String> {
    let parsed = url::Url::parse(u).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| parsed.to_string())
}

/// Parse the GitHub releases list and return the newest published
/// `ferrite-v*` release if it is newer than `current`.
pub fn newest_update(json: &[u8], current: &str) -> anyhow::Result<Option<Update>> {
    let current_v = parse_version(current).context("running version is not a semver")?;
    let releases: Vec<Release> =
        serde_json::from_slice(json).context("unexpected releases response")?;
    let mut best: Option<(Version, &Release)> = None;
    for r in &releases {
        if r.draft || r.prerelease {
            continue;
        }
        let Some(v) = r.tag_name.strip_prefix(TAG_PREFIX).and_then(parse_version) else {
            continue;
        };
        if best.as_ref().is_none_or(|(b, _)| v > *b) {
            best = Some((v, r));
        }
    }
    let Some((v, r)) = best else { return Ok(None) };
    if v <= current_v {
        return Ok(None);
    }
    let Some(url) = web_url(&r.html_url) else {
        return Ok(None);
    };
    let version = r.tag_name[TAG_PREFIX.len()..].to_string();
    let find = |name: String| {
        r.assets
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| web_url(&a.browser_download_url))
    };
    Ok(Some(Update {
        deb_url: find(format!("ferrite_{version}_amd64.deb")),
        appimage_url: find(format!("Ferrite-{version}-x86_64.AppImage")),
        version,
        url,
    }))
}

/// `http://` is accepted only for loopback (test override); everything else must be https.
pub fn url_allowed(u: &str) -> bool {
    let Ok(p) = url::Url::parse(u) else {
        return false;
    };
    match p.scheme() {
        "https" => true,
        "http" => matches!(p.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
        _ => false,
    }
}

/// GET `url` (OS certificate verifier, timeouts, 1 MB limit) and return the body.
/// Blocking: call from a background thread. Sends only a `Ferrite/<version>`
/// User-Agent.
pub fn fetch(url: &str, version: &str) -> anyhow::Result<Vec<u8>> {
    use ureq::tls::{RootCerts, TlsConfig};
    if !url_allowed(url) {
        bail!("refusing update URL {url}");
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent(format!("Ferrite/{version}"))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();
    let body = agent
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .call()?
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_vec()?;
    Ok(body)
}

/// Fetch and evaluate in one step.
pub fn check(url: &str, current: &str) -> anyhow::Result<Option<Update>> {
    newest_update(&fetch(url, current)?, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, draft: bool, pre: bool) -> String {
        let v = tag.trim_start_matches("ferrite-v");
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://github.com/felixarvid2/test/releases/tag/{tag}",
            "draft":{draft},"prerelease":{pre},"published_at":"2026-01-01T00:00:00Z","assets":[
            {{"name":"SHA256SUMS","browser_download_url":"https://github.com/x/SHA256SUMS"}},
            {{"name":"ferrite_{v}_amd64.deb","browser_download_url":"https://github.com/x/ferrite_{v}_amd64.deb"}},
            {{"name":"Ferrite-{v}-x86_64.AppImage","browser_download_url":"https://github.com/x/Ferrite-{v}-x86_64.AppImage"}}]}}"#
        )
    }
    fn list(items: &[String]) -> Vec<u8> {
        format!("[{}]", items.join(",")).into_bytes()
    }

    #[test]
    fn semver_ordering() {
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.1.1", "0.1.1"));
        assert!(!is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("1.0.0", "1.0.0-rc.1"));
        assert!(!is_newer("1.0.0-rc.1", "1.0.0"));
        assert!(is_newer("1.0.0-rc.2", "1.0.0-rc.1"));
        assert!(is_newer("1.0.0-rc.10", "1.0.0-rc.9"));
        assert!(is_newer("1.0.0-beta", "1.0.0-alpha"));
        assert!(!is_newer("1.0.0+b2", "1.0.0+b1"));
        for bad in [
            "", "1", "1.2", "1.2.3.4", "a.b.c", "1.2.-3", "1..3", "1.2.3-", "+1.2.3",
        ] {
            assert!(parse_version(bad).is_none(), "{bad}");
            assert!(!is_newer(bad, "0.0.1"), "{bad}");
        }
    }

    #[test]
    fn picks_newest_by_semver_not_order() {
        let json = list(&[
            release("ferrite-v0.9.0", false, false),
            release("ferrite-v0.10.0", false, false),
            release("ferrite-v0.2.0", false, false),
        ]);
        let u = newest_update(&json, "0.1.1").unwrap().unwrap();
        assert_eq!(u.version, "0.10.0");
        assert_eq!(
            u.url,
            "https://github.com/felixarvid2/test/releases/tag/ferrite-v0.10.0"
        );
        assert_eq!(
            u.deb_url.as_deref(),
            Some("https://github.com/x/ferrite_0.10.0_amd64.deb")
        );
        assert_eq!(
            u.appimage_url.as_deref(),
            Some("https://github.com/x/Ferrite-0.10.0-x86_64.AppImage")
        );
    }

    #[test]
    fn skips_drafts_prereleases_and_other_tags() {
        let json = list(&[
            release("ferrite-v9.0.0", true, false),
            release("ferrite-v8.0.0", false, true),
            release("assets-large-v1", false, false),
            release("ferrite-vbogus", false, false),
            release("ferrite-v0.3.0", false, false),
        ]);
        let u = newest_update(&json, "0.1.1").unwrap().unwrap();
        assert_eq!(u.version, "0.3.0");
    }

    #[test]
    fn none_when_current_or_older() {
        let json = list(&[
            release("ferrite-v0.1.1", false, false),
            release("ferrite-v0.1.0", false, false),
            release("assets-large-v1", false, false),
        ]);
        assert_eq!(newest_update(&json, "0.1.1").unwrap(), None);
        assert_eq!(newest_update(&json, "0.2.0").unwrap(), None);
        assert!(newest_update(&json, "0.1.0").unwrap().is_some());
        assert_eq!(newest_update(b"[]", "0.1.0").unwrap(), None);
    }

    #[test]
    fn missing_assets_and_unsafe_links() {
        let json = br#"[{"tag_name":"ferrite-v0.5.0","html_url":"https://example.org/r","assets":[
            {"name":"ferrite_0.5.0_amd64.deb","browser_download_url":"javascript:alert(1)"}]}]"#;
        let u = newest_update(json, "0.1.0").unwrap().unwrap();
        assert_eq!(u.deb_url, None);
        assert_eq!(u.appimage_url, None);
        let json =
            br#"[{"tag_name":"ferrite-v0.5.0","html_url":"ferrite://settings/","assets":[]}]"#;
        assert_eq!(newest_update(json, "0.1.0").unwrap(), None);
    }

    /// Real response of the releases endpoint (trimmed to the fields we read):
    /// ferrite-v0.1.1, ferrite-v0.1.0 and the unrelated assets-large-v1.
    const REAL: &[u8] = include_bytes!("../testdata/github_releases.json");

    #[test]
    fn real_github_response() {
        let u = newest_update(REAL, "0.1.0").unwrap().unwrap();
        assert_eq!(u.version, "0.1.1");
        assert_eq!(
            u.url,
            "https://github.com/felixarvid2/test/releases/tag/ferrite-v0.1.1"
        );
        assert!(u.deb_url.unwrap().ends_with("/ferrite_0.1.1_amd64.deb"));
        assert!(u
            .appimage_url
            .unwrap()
            .ends_with("/Ferrite-0.1.1-x86_64.AppImage"));
        assert_eq!(newest_update(REAL, "0.1.1").unwrap(), None);
    }

    /// Live request; run with `cargo test -p ferrite-core -- --ignored live_github`.
    #[test]
    #[ignore]
    fn live_github() {
        let u = check(RELEASES_URL, "0.0.1").unwrap().unwrap();
        assert!(is_newer(&u.version, "0.0.1"));
        println!("{u:?}");
    }

    #[test]
    fn rejects_garbage() {
        assert!(newest_update(b"{\"message\":\"rate limited\"}", "0.1.0").is_err());
        assert!(newest_update(b"nope", "0.1.0").is_err());
        assert!(newest_update(b"[]", "x").is_err());
    }

    #[test]
    fn due_rules() {
        let h = 3600 * 1000;
        assert!(is_due(0, 1000, MIN_GAP_MS));
        assert!(!is_due(10 * h, 12 * h, MIN_GAP_MS));
        assert!(is_due(10 * h, 16 * h, MIN_GAP_MS));
        assert!(!is_due(10 * h, 20 * h, INTERVAL_MS));
        assert!(is_due(10 * h, 34 * h, INTERVAL_MS));
        assert!(is_due(10 * h, 5 * h, INTERVAL_MS), "clock went backwards");
    }

    #[test]
    fn url_policy() {
        assert!(url_allowed(RELEASES_URL));
        assert!(url_allowed("http://127.0.0.1:8770/r.json"));
        assert!(url_allowed("http://localhost:8770/r.json"));
        assert!(!url_allowed("http://example.com/r.json"));
        assert!(!url_allowed("file:///etc/passwd"));
        assert!(!url_allowed("nonsense"));
    }

    #[test]
    fn state_roundtrip_and_stale_update_dropped() {
        let dir = std::env::temp_dir().join(format!("ferrite-upd-{}", std::process::id()));
        let path = dir.join("update.json");
        let st = UpdateState {
            last_update_check_ms: 5,
            last_ok_ms: 5,
            update: Some(Update {
                version: "0.2.0".into(),
                url: "https://x/r".into(),
                deb_url: None,
                appimage_url: None,
            }),
        };
        st.save(&path).unwrap();
        assert_eq!(UpdateState::load(&path, "0.1.0"), st);
        // Browser was upgraded meanwhile: the remembered update is gone.
        assert_eq!(UpdateState::load(&path, "0.2.0").update, None);
        assert_eq!(
            UpdateState::load(&dir.join("none.json"), "0.1.0"),
            UpdateState::default()
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
