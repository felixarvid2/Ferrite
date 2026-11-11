//! Engine-independent logic for the Ferrite browser. Must not depend on CEF.

pub mod blocker;
pub mod bookmarks;
pub mod closed_tabs;
pub mod commands;
pub mod db;
pub mod downloads;
pub mod history;
pub mod importer;
pub mod omnibox;
pub mod passcli;
pub mod permissions;
pub mod profile;
pub mod search;
pub mod session;
pub mod settings;
pub mod update;

use std::path::Path;

/// Current time as Unix milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Write `bytes` to `path` atomically: write `<path>.tmp` (same directory), then rename.
/// Parent directories are created.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use anyhow::Context;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
    }
    let mut name = path
        .file_name()
        .context("path has no file name")?
        .to_os_string();
    name.push(".tmp");
    let tmp = path.with_file_name(name);
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn now_ms_is_recent() {
        assert!(super::now_ms() > 1_700_000_000_000);
    }
}
