//! User settings, persisted as JSON.

use std::path::Path;

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
    System,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StartupMode {
    RestoreSession,
    NewTab,
    Homepage,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub search_engine: String,
    pub custom_search_url: String,
    pub homepage: String,
    pub startup: StartupMode,
    pub download_dir: String,
    pub ask_download_location: bool,
    pub show_bookmarks_bar: bool,
    pub tracking_protection: bool,
    pub adblock_enabled: bool,
    /// Sites (lowercase hosts, no "www.") where ad blocking is switched off.
    pub adblock_exceptions: Vec<String>,
    pub send_do_not_track: bool,
    pub restore_after_crash: bool,
    pub default_zoom_percent: u32,
    /// Unload background tabs unused for this many minutes (0 = never).
    pub tab_sleep_minutes: u32,
    /// Use the desktop's title bar instead of Ferrite's tab strip.
    pub system_titlebar: bool,
    /// Show tabs in a sidebar on the left (default) instead of above the toolbar.
    pub vertical_tabs: bool,
    /// Offer logins from Proton Pass through the `pass-cli` tool.
    pub proton_pass: bool,
    /// Path of `pass-cli`; empty = look in PATH, then ~/.local/bin.
    pub proton_pass_cli: String,
    /// Offer to save a login to Proton Pass after signing in.
    pub proton_pass_offer_save: bool,
    /// Share id of the vault new logins go to; empty = the first vault.
    pub proton_pass_vault: String,
    /// Sites (site keys like `adblock_exceptions`) never offered for saving.
    pub proton_pass_never_save: Vec<String>,
    /// Ask api.github.com once a day whether a newer Ferrite release exists.
    pub check_for_updates: bool,
    /// Let an AI client (MCP, through the `ferrite-mcp` bridge) control tabs.
    pub mcp_enabled: bool,
    /// Also offer the MCP `evaluate` tool (run JavaScript in a page).
    pub mcp_allow_evaluate: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let download_dir = dirs::download_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
            .unwrap_or_else(|| "Downloads".into());
        Settings {
            theme: Theme::Dark,
            search_engine: "google".into(),
            custom_search_url: String::new(),
            homepage: "ferrite://newtab/".into(),
            startup: StartupMode::RestoreSession,
            download_dir: download_dir.to_string_lossy().into_owned(),
            ask_download_location: false,
            show_bookmarks_bar: true,
            tracking_protection: false,
            adblock_enabled: true,
            adblock_exceptions: Vec::new(),
            send_do_not_track: false,
            restore_after_crash: true,
            default_zoom_percent: 100,
            tab_sleep_minutes: 60,
            system_titlebar: false,
            vertical_tabs: true,
            proton_pass: false,
            proton_pass_cli: String::new(),
            proton_pass_offer_save: true,
            proton_pass_vault: String::new(),
            proton_pass_never_save: Vec::new(),
            check_for_updates: true,
            mcp_enabled: false,
            mcp_allow_evaluate: false,
        }
    }
}

impl Settings {
    /// Missing or corrupt file gives defaults (corrupt logs a warning).
    pub fn load(path: &Path) -> Settings {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => return Settings::default(),
        };
        match serde_json::from_slice(&bytes) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("corrupt settings file {}: {e}", path.display());
                Settings::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        crate::write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }

    /// Merge a JSON object of field -> value into the settings. On error nothing changes.
    pub fn apply_patch(&mut self, patch: &serde_json::Value) -> anyhow::Result<()> {
        let Some(patch) = patch.as_object() else {
            bail!("settings patch must be a JSON object");
        };
        let mut current = serde_json::to_value(&*self)?;
        let obj = current.as_object_mut().context("settings not an object")?;
        for (k, v) in patch {
            obj.insert(k.clone(), v.clone());
        }
        let mut next: Settings =
            serde_json::from_value(current).context("invalid settings patch")?;
        // Exceptions are site keys: normalise, drop invalid, sort, dedupe.
        let site_list = |list: &[String]| {
            let mut sites: Vec<String> = list
                .iter()
                .filter_map(|s| crate::blocker::normalize_site(s))
                .collect();
            sites.sort();
            sites.dedup();
            sites
        };
        next.adblock_exceptions = site_list(&next.adblock_exceptions);
        next.proton_pass_never_save = site_list(&next.proton_pass_never_save);
        next.proton_pass_vault = next.proton_pass_vault.trim().to_string();
        if !next.proton_pass_vault.is_empty() && !crate::passcli::valid_id(&next.proton_pass_vault)
        {
            bail!("invalid vault id");
        }
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults() {
        let s = Settings::default();
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.search_engine, "google");
        assert_eq!(s.homepage, "ferrite://newtab/");
        assert!(s.show_bookmarks_bar && s.restore_after_crash);
        assert_eq!(s.default_zoom_percent, 100);
        assert!(!s.proton_pass && s.proton_pass_cli.is_empty());
        assert!(!s.download_dir.is_empty());
        assert!(s.proton_pass_offer_save);
        assert!(s.check_for_updates);
        // AI control is opt-in, and running page scripts is a second opt-in.
        assert!(!s.mcp_enabled && !s.mcp_allow_evaluate);
        assert!(s.proton_pass_vault.is_empty() && s.proton_pass_never_save.is_empty());
    }

    #[test]
    fn never_save_sites_and_vault() {
        let mut s = Settings::default();
        s.apply_patch(&json!({"proton_pass_never_save":["WWW.Example.com","https://b.org/x","example.com","","http://"],
            "proton_pass_vault":" abc_1= "}))
            .unwrap();
        assert_eq!(s.proton_pass_never_save, vec!["b.org", "example.com"]);
        assert_eq!(s.proton_pass_vault, "abc_1=");
        let before = s.clone();
        assert!(s.apply_patch(&json!({"proton_pass_vault":"a/b"})).is_err());
        assert_eq!(s, before);
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a/b/settings.json");
        let s = Settings {
            theme: Theme::Light,
            adblock_enabled: true,
            ..Settings::default()
        };
        s.save(&p).unwrap();
        assert!(!dir.path().join("a/b/settings.json.tmp").exists());
        assert_eq!(Settings::load(&p), s);
    }

    #[test]
    fn old_settings_file_defaults_update_check_on() {
        let s: Settings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
        assert!(s.check_for_updates);
        let s: Settings = serde_json::from_str(r#"{"check_for_updates":false}"#).unwrap();
        assert!(!s.check_for_updates);
    }

    #[test]
    fn load_missing_corrupt_partial() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        assert_eq!(Settings::load(&p), Settings::default());
        std::fs::write(&p, "garbage").unwrap();
        assert_eq!(Settings::load(&p), Settings::default());
        std::fs::write(&p, r#"{"theme":"system"}"#).unwrap();
        let s = Settings::load(&p);
        assert_eq!(s.theme, Theme::System);
        assert_eq!(s.search_engine, "google");
    }

    #[test]
    fn patch() {
        let mut s = Settings::default();
        s.apply_patch(&json!({"theme":"light","default_zoom_percent":125,"startup":"new_tab"}))
            .unwrap();
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.default_zoom_percent, 125);
        assert_eq!(s.startup, StartupMode::NewTab);
    }

    #[test]
    fn invalid_patch_rejected() {
        let mut s = Settings::default();
        let before = s.clone();
        assert!(s.apply_patch(&json!({"theme":"neon"})).is_err());
        assert!(s
            .apply_patch(&json!({"default_zoom_percent":"big"}))
            .is_err());
        assert!(s.apply_patch(&json!([1, 2])).is_err());
        assert_eq!(s, before);
    }

    #[test]
    fn adblock_exceptions_normalised_and_defaulted() {
        let mut s = Settings::default();
        assert!(s.adblock_exceptions.is_empty());
        s.apply_patch(&json!({"adblock_exceptions":["WWW.B.com","a.com","b.com","ferrite://x"]}))
            .unwrap();
        assert_eq!(s.adblock_exceptions, vec!["a.com", "b.com"]);
        // Old files without the field still load.
        let old: Settings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
        assert!(old.adblock_exceptions.is_empty());
    }
}
