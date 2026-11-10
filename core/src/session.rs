//! Session snapshot (open windows/tabs) for restore after restart or crash.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default)]
pub struct TabSnapshot {
    pub url: String,
    pub title: String,
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(default)]
pub struct WindowSnapshot {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
    pub tabs: Vec<TabSnapshot>,
    pub active: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Session {
    pub windows: Vec<WindowSnapshot>,
    pub clean_exit: bool,
    pub saved_at: i64,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            windows: Vec::new(),
            clean_exit: true,
            saved_at: 0,
        }
    }
}

impl Session {
    /// `None` if the file is missing or corrupt.
    pub fn load(path: &Path) -> Option<Session> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        crate::write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }

    /// True when there is no tab in any window.
    pub fn is_empty(&self) -> bool {
        self.windows.iter().all(|w| w.tabs.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Session {
        Session {
            windows: vec![WindowSnapshot {
                x: 10,
                y: 20,
                width: 800,
                height: 600,
                maximized: true,
                tabs: vec![TabSnapshot {
                    url: "https://a.com/".into(),
                    title: "A".into(),
                    pinned: true,
                }],
                active: 0,
            }],
            clean_exit: false,
            saved_at: 42,
        }
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/session.json");
        let s = sample();
        s.save(&p).unwrap();
        assert!(!dir.path().join("sub/session.json.tmp").exists());
        assert_eq!(Session::load(&p), Some(s));
    }

    #[test]
    fn missing_and_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("session.json");
        assert!(Session::load(&p).is_none());
        std::fs::write(&p, "{not json").unwrap();
        assert!(Session::load(&p).is_none());
    }

    #[test]
    fn defaults_and_empty() {
        let s = Session::default();
        assert!(s.clean_exit && s.is_empty());
        assert!(!sample().is_empty());
        let mut w = sample();
        w.windows[0].tabs.clear();
        assert!(w.is_empty());
    }
}
