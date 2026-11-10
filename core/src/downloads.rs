//! Download records and file-name helpers.

use std::path::{Path, PathBuf};

use anyhow::bail;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};

use crate::db::Database;

const MAX_NAME_BYTES: usize = 200;

const DANGEROUS_EXTENSIONS: &[&str] = &[
    "exe", "msi", "bat", "cmd", "com", "scr", "ps1", "vbs", "js", "jse", "wsf", "sh", "bash",
    "run", "bin", "appimage", "deb", "rpm", "jar", "apk", "dmg", "pkg", "app", "command",
    "desktop", "py", "pl",
];

const MULTI_EXTENSIONS: &[&str] = &[".tar.gz", ".tar.xz", ".tar.bz2"];

/// Make a server-supplied name safe as a single file name.
///
/// Any directory part (before the last `/` or `\`) is dropped, NUL and control characters
/// are removed, the Windows-reserved `<>:"|?*` become `_`, leading/trailing dots and spaces
/// are trimmed and the result is capped at 200 bytes keeping the extension. Empty gives
/// `download`.
pub fn sanitize_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| if "<>:\"|?*".contains(c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim_matches(|c| c == '.' || c == ' ');
    if trimmed.is_empty() {
        return "download".into();
    }
    cap_length(trimmed)
}

fn cap_length(name: &str) -> String {
    if name.len() <= MAX_NAME_BYTES {
        return name.to_string();
    }
    let (stem, ext) = split_extension(name);
    // An absurdly long extension is not worth preserving.
    let (stem, ext) = if ext.len() > 20 {
        (name, "")
    } else {
        (stem, ext)
    };
    let mut end = MAX_NAME_BYTES - ext.len();
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    let out = format!("{}{}", stem[..end].trim_end_matches([' ', '.']), ext);
    if out.is_empty() {
        "download".into()
    } else {
        out
    }
}

/// Split into (stem, extension including the dot); ".tar.gz" style pairs count as one.
fn split_extension(name: &str) -> (&str, &str) {
    let lower = name.to_ascii_lowercase();
    for m in MULTI_EXTENSIONS {
        if lower.ends_with(m) && name.len() > m.len() {
            let at = name.len() - m.len();
            return (&name[..at], &name[at..]);
        }
    }
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// `dir/name`, or `dir/name (1).ext`, `(2)`, ... when the file already exists.
pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let name = sanitize_file_name(file_name);
    let first = dir.join(&name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = split_extension(&name);
    (1u32..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("unbounded range")
}

pub fn is_dangerous(file_name: &str) -> bool {
    match file_name.rsplit_once('.') {
        Some((_, ext)) => DANGEROUS_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    InProgress,
    Complete,
    Cancelled,
    Interrupted,
}

impl DownloadState {
    fn as_str(self) -> &'static str {
        match self {
            DownloadState::InProgress => "in_progress",
            DownloadState::Complete => "complete",
            DownloadState::Cancelled => "cancelled",
            DownloadState::Interrupted => "interrupted",
        }
    }

    fn parse(s: &str) -> anyhow::Result<DownloadState> {
        Ok(match s {
            "in_progress" => DownloadState::InProgress,
            "complete" => DownloadState::Complete,
            "cancelled" => DownloadState::Cancelled,
            "interrupted" => DownloadState::Interrupted,
            other => bail!("unknown download state {other:?}"),
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DownloadRecord {
    pub id: i64,
    pub url: String,
    pub path: String,
    pub file_name: String,
    pub mime_type: String,
    pub total_bytes: i64,
    pub received_bytes: i64,
    pub state: DownloadState,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

fn from_row(r: &Row) -> rusqlite::Result<(DownloadRecord, String)> {
    Ok((
        DownloadRecord {
            id: r.get(0)?,
            url: r.get(1)?,
            path: r.get(2)?,
            file_name: r.get(3)?,
            mime_type: r.get(4)?,
            total_bytes: r.get(5)?,
            received_bytes: r.get(6)?,
            state: DownloadState::InProgress,
            started_at: r.get(8)?,
            finished_at: r.get(9)?,
        },
        r.get(7)?,
    ))
}

impl Database {
    /// Insert a record (its `id` is ignored) and return the new id.
    pub fn insert_download(&self, rec: &DownloadRecord) -> anyhow::Result<i64> {
        self.conn().execute(
            "INSERT INTO downloads (url, path, file_name, mime_type, total_bytes, received_bytes,
                                    state, started_at, finished_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                rec.url,
                rec.path,
                rec.file_name,
                rec.mime_type,
                rec.total_bytes,
                rec.received_bytes,
                rec.state.as_str(),
                rec.started_at,
                rec.finished_at
            ],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    pub fn update_download(
        &self,
        id: i64,
        path: &str,
        received: i64,
        total: i64,
        state: DownloadState,
        finished_at: Option<i64>,
    ) -> anyhow::Result<()> {
        self.conn().execute(
            "UPDATE downloads SET path = ?2, received_bytes = ?3, total_bytes = ?4,
                    state = ?5, finished_at = ?6 WHERE id = ?1",
            params![id, path, received, total, state.as_str(), finished_at],
        )?;
        Ok(())
    }

    /// Newest first.
    pub fn list_downloads(&self, limit: usize) -> anyhow::Result<Vec<DownloadRecord>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, url, path, file_name, mime_type, total_bytes, received_bytes, state,
                    started_at, finished_at
             FROM downloads ORDER BY started_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], from_row)?;
        rows.map(|row| {
            let (mut rec, state) = row?;
            rec.state = DownloadState::parse(&state)?;
            Ok(rec)
        })
        .collect()
    }

    pub fn remove_download(&self, id: i64) -> anyhow::Result<()> {
        self.conn()
            .execute("DELETE FROM downloads WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Remove every download that is not in progress.
    pub fn clear_finished_downloads(&self) -> anyhow::Result<()> {
        self.conn()
            .execute("DELETE FROM downloads WHERE state <> 'in_progress'", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_file_name("report.pdf"), "report.pdf");
        assert_eq!(sanitize_file_name(""), "download");
        assert_eq!(sanitize_file_name("..."), "download");
        assert_eq!(sanitize_file_name("  . "), "download");
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_file_name("C:\\Users\\x\\a.txt"), "a.txt");
        assert_eq!(sanitize_file_name("dir/"), "download");
        assert_eq!(
            sanitize_file_name("a<b>c:d\"e|f?g*h.txt"),
            "a_b_c_d_e_f_g_h.txt"
        );
        assert_eq!(sanitize_file_name("a\0b\x07c\n.txt"), "abc.txt");
        assert_eq!(sanitize_file_name(" .hidden. "), "hidden");
        assert_eq!(sanitize_file_name("tab\there.txt"), "tabhere.txt");

        let long = format!("{}.tar.gz", "a".repeat(300));
        let s = sanitize_file_name(&long);
        assert_eq!(s.len(), 200);
        assert!(s.ends_with(".tar.gz"));
        let multi = format!("{}.txt", "é".repeat(150));
        let s = sanitize_file_name(&multi);
        assert!(s.len() <= 200 && s.ends_with(".txt"));
        let weird = "b".repeat(250);
        assert_eq!(sanitize_file_name(&weird).len(), 200);
    }

    #[test]
    fn unique() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        assert_eq!(unique_path(d, "a.txt"), d.join("a.txt"));
        std::fs::write(d.join("a.txt"), "").unwrap();
        assert_eq!(unique_path(d, "a.txt"), d.join("a (1).txt"));
        std::fs::write(d.join("a (1).txt"), "").unwrap();
        assert_eq!(unique_path(d, "a.txt"), d.join("a (2).txt"));

        std::fs::write(d.join("x.tar.gz"), "").unwrap();
        assert_eq!(unique_path(d, "x.tar.gz"), d.join("x (1).tar.gz"));
        std::fs::write(d.join("x (1).tar.gz"), "").unwrap();
        assert_eq!(unique_path(d, "x.tar.gz"), d.join("x (2).tar.gz"));

        std::fs::write(d.join("noext"), "").unwrap();
        assert_eq!(unique_path(d, "noext"), d.join("noext (1)"));
        std::fs::write(d.join(".bashrc"), "").unwrap();
        assert_eq!(unique_path(d, "bashrc"), d.join("bashrc"));
        std::fs::write(d.join("y.zip"), "").unwrap();
        assert_eq!(unique_path(d, "../y.zip"), d.join("y (1).zip"));
    }

    #[test]
    fn dangerous() {
        for n in [
            "a.exe",
            "A.EXE",
            "x.AppImage",
            "s.sh",
            "t.py",
            "i.msi",
            "j.jar",
        ] {
            assert!(is_dangerous(n), "{n}");
        }
        for n in ["a.txt", "exe", "a.exe.txt", "", "photo.png", "x.tar.gz"] {
            assert!(!is_dangerous(n), "{n}");
        }
    }

    fn rec(url: &str, started: i64, state: DownloadState) -> DownloadRecord {
        DownloadRecord {
            id: 777,
            url: url.into(),
            path: "/tmp/f".into(),
            file_name: "f".into(),
            mime_type: "text/plain".into(),
            total_bytes: 10,
            received_bytes: 0,
            state,
            started_at: started,
            finished_at: None,
        }
    }

    #[test]
    fn db_lifecycle() {
        let db = Database::open_in_memory().unwrap();
        let a = db
            .insert_download(&rec("https://a/", 1, DownloadState::InProgress))
            .unwrap();
        let b = db
            .insert_download(&rec("https://b/", 2, DownloadState::InProgress))
            .unwrap();
        assert_ne!(a, 777);
        assert_ne!(a, b);
        db.update_download(a, "/d/a", 10, 10, DownloadState::Complete, Some(5))
            .unwrap();
        let list = db.list_downloads(10).unwrap();
        assert_eq!(list.iter().map(|r| r.id).collect::<Vec<_>>(), [b, a]);
        assert_eq!(list[1].state, DownloadState::Complete);
        assert_eq!(list[1].finished_at, Some(5));
        assert_eq!(list[1].path, "/d/a");
        assert_eq!(db.list_downloads(1).unwrap().len(), 1);
        db.clear_finished_downloads().unwrap();
        let left = db.list_downloads(10).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, b);
        db.remove_download(b).unwrap();
        assert!(db.list_downloads(10).unwrap().is_empty());
    }

    #[test]
    fn in_progress_becomes_interrupted_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.sqlite");
        let id = {
            let db = Database::open(&p).unwrap();
            db.insert_download(&rec("https://a/", 1, DownloadState::InProgress))
                .unwrap()
        };
        let db = Database::open(&p).unwrap();
        let l = db.list_downloads(10).unwrap();
        assert_eq!(l[0].id, id);
        assert_eq!(l[0].state, DownloadState::Interrupted);
    }
}
