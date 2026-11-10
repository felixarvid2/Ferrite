//! On-disk layout of a browser profile.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilePaths {
    pub root: PathBuf,
    pub cef_cache: PathBuf,
    pub database: PathBuf,
    pub settings: PathBuf,
    pub session: PathBuf,
    pub filter_lists: PathBuf,
}

impl ProfilePaths {
    /// Base is `FERRITE_DATA_DIR` if set, else the platform data dir joined with `ferrite`.
    pub fn for_profile(name: &str) -> anyhow::Result<ProfilePaths> {
        let base = match std::env::var_os("FERRITE_DATA_DIR") {
            Some(d) if !d.is_empty() => PathBuf::from(d),
            _ => dirs::data_dir()
                .context("no platform data directory")?
                .join("ferrite"),
        };
        Self::for_profile_in(&base, name)
    }

    pub fn for_profile_in(base: &Path, name: &str) -> anyhow::Result<ProfilePaths> {
        let valid = !name.is_empty()
            && name.len() <= 64
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if !valid {
            bail!("invalid profile name {name:?}: use 1-64 characters from A-Z a-z 0-9 _ -");
        }
        let root = base.join("profiles").join(name);
        Ok(ProfilePaths {
            cef_cache: root.join("cef"),
            database: root.join("ferrite.sqlite"),
            settings: root.join("settings.json"),
            session: root.join("session.json"),
            filter_lists: root.join("filter-lists"),
            root,
        })
    }

    pub fn ensure_dirs(&self) -> anyhow::Result<()> {
        for d in [&self.root, &self.cef_cache, &self.filter_lists] {
            std::fs::create_dir_all(d).with_context(|| format!("creating {}", d.display()))?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))
                .with_context(|| format!("chmod {}", self.root.display()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        let base = Path::new("/data");
        let p = ProfilePaths::for_profile_in(base, "default").unwrap();
        assert_eq!(p.root, base.join("profiles/default"));
        assert_eq!(p.cef_cache, base.join("profiles/default/cef"));
        assert_eq!(p.database, base.join("profiles/default/ferrite.sqlite"));
        assert_eq!(p.settings, base.join("profiles/default/settings.json"));
        assert_eq!(p.session, base.join("profiles/default/session.json"));
        assert_eq!(p.filter_lists, base.join("profiles/default/filter-lists"));
    }

    #[test]
    fn name_validation() {
        let base = Path::new("/data");
        for ok in ["a", "Work_1-x", &"a".repeat(64)] {
            assert!(ProfilePaths::for_profile_in(base, ok).is_ok(), "{ok}");
        }
        for bad in ["", "..", "a/b", "a b", "é", "a.b", &"a".repeat(65), "a\\b"] {
            assert!(ProfilePaths::for_profile_in(base, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ensure_dirs_creates() {
        let dir = tempfile::tempdir().unwrap();
        let p = ProfilePaths::for_profile_in(dir.path(), "t").unwrap();
        p.ensure_dirs().unwrap();
        p.ensure_dirs().unwrap();
        assert!(p.cef_cache.is_dir() && p.filter_lists.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p.root).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }
}
