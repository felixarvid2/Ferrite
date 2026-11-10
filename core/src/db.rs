//! SQLite storage: connection setup, schema and migrations.

use std::path::Path;

use anyhow::Context;
use rusqlite::Connection;

const SCHEMA_V1: &str = "
CREATE TABLE history_urls (
    id INTEGER PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL DEFAULT '',
    visit_count INTEGER NOT NULL DEFAULT 0,
    last_visit INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_history_urls_last_visit ON history_urls(last_visit);
CREATE TABLE history_visits (
    id INTEGER PRIMARY KEY,
    url_id INTEGER NOT NULL REFERENCES history_urls(id) ON DELETE CASCADE,
    visited_at INTEGER NOT NULL
);
CREATE INDEX idx_history_visits_url ON history_visits(url_id);
CREATE INDEX idx_history_visits_at ON history_visits(visited_at);
CREATE TABLE bookmarks (
    id INTEGER PRIMARY KEY,
    parent_id INTEGER REFERENCES bookmarks(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('folder', 'url')),
    title TEXT NOT NULL DEFAULT '',
    url TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_bookmarks_parent ON bookmarks(parent_id, position);
CREATE TABLE permissions (
    origin TEXT NOT NULL,
    permission TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('allow', 'block')),
    updated_at INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (origin, permission)
);
CREATE TABLE downloads (
    id INTEGER PRIMARY KEY,
    url TEXT NOT NULL,
    path TEXT NOT NULL DEFAULT '',
    file_name TEXT NOT NULL DEFAULT '',
    mime_type TEXT NOT NULL DEFAULT '',
    total_bytes INTEGER NOT NULL DEFAULT 0,
    received_bytes INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL,
    started_at INTEGER NOT NULL DEFAULT 0,
    finished_at INTEGER
);
INSERT INTO bookmarks (id, parent_id, kind, title, position, created_at)
    VALUES (1, NULL, 'folder', 'Bookmarks bar', 0, 0);
INSERT INTO bookmarks (id, parent_id, kind, title, position, created_at)
    VALUES (2, NULL, 'folder', 'Other bookmarks', 1, 0);
";

const LATEST_VERSION: i64 = 1;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> anyhow::Result<Database> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening database {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> anyhow::Result<Database> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> anyhow::Result<Database> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let db = Database { conn };
        db.migrate()?;
        // Downloads still in progress belong to a previous run that did not finish them.
        db.conn.execute(
            "UPDATE downloads SET state = 'interrupted' WHERE state = 'in_progress'",
            [],
        )?;
        Ok(db)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > LATEST_VERSION {
            anyhow::bail!("database version {version} is newer than supported {LATEST_VERSION}");
        }
        if version < 1 {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(SCHEMA_V1)?;
            tx.pragma_update(None, "user_version", 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

/// Escape `%`, `_` and `\` for use in a `LIKE ... ESCAPE '\'` pattern, wrapped in `%...%`.
pub(crate) fn like_contains(query: &str) -> String {
    let mut s = String::with_capacity(query.len() + 2);
    s.push('%');
    for c in query.chars() {
        if matches!(c, '%' | '_' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('%');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_roots_and_version() {
        let db = Database::open_in_memory().unwrap();
        let titles: Vec<String> = db
            .conn()
            .prepare("SELECT title FROM bookmarks ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(titles, ["Bookmarks bar", "Other bookmarks"]);
        let v: i64 = db
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, LATEST_VERSION);
    }

    #[test]
    fn reopen_file_keeps_data_and_does_not_reseed() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x/ferrite.sqlite");
        {
            let db = Database::open(&p).unwrap();
            db.add_bookmark(1, "a", "https://a.com/").unwrap();
            let mode: String = db
                .conn()
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
        }
        let db = Database::open(&p).unwrap();
        assert_eq!(db.bookmark_children(1).unwrap().len(), 1);
        assert_eq!(db.bookmark_tree().unwrap().len(), 2);
    }
}
