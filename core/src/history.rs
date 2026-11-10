//! Browsing history.

use anyhow::Context;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::{like_contains, Database};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visit: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub visited_at: i64,
}

/// `url` without its scheme and a leading `www.`.
fn strip_scheme_www(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    match rest.get(..4) {
        Some(p) if p.eq_ignore_ascii_case("www.") => &rest[4..],
        _ => rest,
    }
}

/// The text to append to what the user typed so it becomes `url` (scheme and `www.` ignored).
/// `None` for an empty prefix, no match, or nothing left to complete.
pub fn inline_completion(prefix: &str, url: &str) -> Option<String> {
    if prefix.is_empty() {
        return None;
    }
    let tail = |s: &str| -> Option<String> {
        let head = s.get(..prefix.len())?;
        if !head.eq_ignore_ascii_case(prefix) {
            return None;
        }
        let rest = &s[prefix.len()..];
        (!rest.is_empty()).then(|| rest.to_string())
    };
    tail(strip_scheme_www(url)).or_else(|| tail(url))
}

impl Database {
    /// Record a visit. Only http, https and file URLs are stored.
    pub fn record_visit(&self, url: &str, title: &str, at: i64) -> anyhow::Result<()> {
        let scheme_ok = url::Url::parse(url)
            .map(|u| matches!(u.scheme(), "http" | "https" | "file"))
            .unwrap_or(false);
        if !scheme_ok {
            return Ok(());
        }
        let tx = self.conn().unchecked_transaction()?;
        tx.execute(
            "INSERT INTO history_urls (url, title, visit_count, last_visit) VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(url) DO UPDATE SET
                visit_count = visit_count + 1,
                last_visit = excluded.last_visit,
                title = CASE WHEN excluded.title <> '' THEN excluded.title ELSE title END",
            params![url, title, at],
        )?;
        let url_id: i64 =
            tx.query_row("SELECT id FROM history_urls WHERE url = ?1", [url], |r| {
                r.get(0)
            })?;
        tx.execute(
            "INSERT INTO history_visits (url_id, visited_at) VALUES (?1, ?2)",
            params![url_id, at],
        )?;
        tx.commit().context("recording visit")
    }

    pub fn update_title(&self, url: &str, title: &str) -> anyhow::Result<()> {
        self.conn().execute(
            "UPDATE history_urls SET title = ?2 WHERE url = ?1",
            params![url, title],
        )?;
        Ok(())
    }

    /// Visits, newest first. Empty query returns everything.
    pub fn search_history(&self, query: &str, limit: usize) -> anyhow::Result<Vec<Visit>> {
        let pattern = like_contains(query);
        let mut stmt = self.conn().prepare(
            "SELECT v.id, u.url, u.title, v.visited_at
             FROM history_visits v JOIN history_urls u ON u.id = v.url_id
             WHERE ?1 = '' OR u.url LIKE ?2 ESCAPE '\\' OR u.title LIKE ?2 ESCAPE '\\'
             ORDER BY v.visited_at DESC, v.id DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![query, pattern, limit as i64], |r| {
            Ok(Visit {
                id: r.get(0)?,
                url: r.get(1)?,
                title: r.get(2)?,
                visited_at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// URLs whose scheme-less, `www.`-less form starts with `prefix`, or whose title contains it.
    pub fn suggest(&self, prefix: &str, limit: usize) -> anyhow::Result<Vec<HistoryEntry>> {
        let pattern = like_contains(prefix);
        let mut stmt = self.conn().prepare(
            "SELECT id, url, title, visit_count, last_visit FROM history_urls
             WHERE ?1 = '' OR url LIKE ?2 ESCAPE '\\' OR title LIKE ?2 ESCAPE '\\'
             ORDER BY visit_count DESC, last_visit DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![prefix, pattern], |r| {
            Ok(HistoryEntry {
                id: r.get(0)?,
                url: r.get(1)?,
                title: r.get(2)?,
                visit_count: r.get(3)?,
                last_visit: r.get(4)?,
            })
        })?;
        let needle = prefix.to_lowercase();
        let mut out = Vec::new();
        for row in rows {
            let e = row?;
            let url_match = strip_scheme_www(&e.url).to_lowercase().starts_with(&needle);
            if url_match || e.title.to_lowercase().contains(&needle) {
                out.push(e);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    pub fn delete_visit(&self, visit_id: i64) -> anyhow::Result<()> {
        self.conn()
            .execute("DELETE FROM history_visits WHERE id = ?1", [visit_id])?;
        self.refresh_history_urls()
    }

    pub fn delete_url(&self, url: &str) -> anyhow::Result<()> {
        self.conn()
            .execute("DELETE FROM history_urls WHERE url = ?1", [url])?;
        Ok(())
    }

    /// Delete visits with `from <= visited_at < to`; returns the number of visits removed.
    pub fn delete_history_range(&self, from: i64, to: i64) -> anyhow::Result<usize> {
        let n = self.conn().execute(
            "DELETE FROM history_visits WHERE visited_at >= ?1 AND visited_at < ?2",
            params![from, to],
        )?;
        self.refresh_history_urls()?;
        Ok(n)
    }

    pub fn clear_history(&self) -> anyhow::Result<()> {
        self.conn().execute("DELETE FROM history_urls", [])?;
        Ok(())
    }

    /// Drop URLs without visits and recompute counters of the rest.
    fn refresh_history_urls(&self) -> anyhow::Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        tx.execute(
            "DELETE FROM history_urls WHERE id NOT IN (SELECT url_id FROM history_visits)",
            [],
        )?;
        tx.execute(
            "UPDATE history_urls SET
                visit_count = (SELECT COUNT(*) FROM history_visits WHERE url_id = history_urls.id),
                last_visit = (SELECT MAX(visited_at) FROM history_visits WHERE url_id = history_urls.id)",
            [],
        )?;
        Ok(tx.commit()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline() {
        assert_eq!(
            inline_completion("git", "https://github.com/").as_deref(),
            Some("hub.com/")
        );
        assert_eq!(
            inline_completion("GIT", "https://www.github.com/x").as_deref(),
            Some("hub.com/x")
        );
        assert_eq!(
            inline_completion("www.git", "https://www.github.com/").as_deref(),
            None
        );
        assert_eq!(
            inline_completion("https://gi", "https://github.com/").as_deref(),
            Some("thub.com/")
        );
        assert_eq!(inline_completion("", "https://a.com"), None);
        assert_eq!(inline_completion("x", "https://a.com"), None);
        assert_eq!(inline_completion("a.com", "https://a.com"), None);
        assert_eq!(inline_completion("é", "https://a.com"), None);
    }

    #[test]
    fn record_and_search() {
        let db = Database::open_in_memory().unwrap();
        db.record_visit("https://example.com/", "", 100).unwrap();
        db.record_visit("https://example.com/", "Example", 200)
            .unwrap();
        db.record_visit("https://example.com/", "", 300).unwrap();
        db.record_visit("https://rust-lang.org/", "Rust", 150)
            .unwrap();
        db.record_visit("ferrite://settings", "S", 400).unwrap();
        db.record_visit("about:blank", "", 400).unwrap();
        db.record_visit("nonsense", "", 400).unwrap();
        db.record_visit("file:///tmp/a.html", "Local", 50).unwrap();

        let all = db.search_history("", 100).unwrap();
        assert_eq!(all.len(), 5);
        assert_eq!(all[0].visited_at, 300);
        assert_eq!(all[0].title, "Example");
        assert_eq!(db.search_history("", 2).unwrap().len(), 2);
        assert_eq!(db.search_history("RUST", 10).unwrap().len(), 1);
        assert_eq!(db.search_history("exam", 10).unwrap().len(), 3);
        assert_eq!(db.search_history("100%", 10).unwrap().len(), 0);

        let sug = db.suggest("exa", 10).unwrap();
        assert_eq!(sug.len(), 1);
        assert_eq!(sug[0].visit_count, 3);
        assert_eq!(sug[0].last_visit, 300);

        db.update_title("https://rust-lang.org/", "Rust Language")
            .unwrap();
        assert_eq!(db.search_history("language", 10).unwrap().len(), 1);
    }

    #[test]
    fn suggest_rules() {
        let db = Database::open_in_memory().unwrap();
        db.record_visit("https://www.github.com/", "GitHub", 1)
            .unwrap();
        db.record_visit("https://gitlab.com/", "GitLab", 2).unwrap();
        db.record_visit("https://gitlab.com/", "GitLab", 3).unwrap();
        db.record_visit("https://news.site/", "About git", 4)
            .unwrap();
        let s = db.suggest("git", 10).unwrap();
        let urls: Vec<_> = s.iter().map(|e| e.url.as_str()).collect();
        // gitlab has more visits; news.site matches by title only.
        assert_eq!(
            urls,
            [
                "https://gitlab.com/",
                "https://news.site/",
                "https://www.github.com/"
            ]
        );
        assert_eq!(db.suggest("git", 1).unwrap().len(), 1);
        assert_eq!(db.suggest("https", 10).unwrap().len(), 0);
        assert_eq!(db.suggest("hub", 10).unwrap().len(), 1); // title "GitHub" contains it
    }

    #[test]
    fn delete_range_recomputes() {
        let db = Database::open_in_memory().unwrap();
        for t in [100, 200, 300] {
            db.record_visit("https://a.com/", "A", t).unwrap();
        }
        db.record_visit("https://b.com/", "B", 150).unwrap();
        let n = db.delete_history_range(100, 300).unwrap();
        assert_eq!(n, 3);
        let all = db.search_history("", 10).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].url, "https://a.com/");
        let e = &db.suggest("a.com", 5).unwrap()[0];
        assert_eq!((e.visit_count, e.last_visit), (1, 300));
        assert!(db.suggest("b.com", 5).unwrap().is_empty());
    }

    #[test]
    fn delete_visit_url_clear() {
        let db = Database::open_in_memory().unwrap();
        db.record_visit("https://a.com/", "A", 1).unwrap();
        db.record_visit("https://a.com/", "A", 2).unwrap();
        db.record_visit("https://b.com/", "B", 3).unwrap();
        let newest = db.search_history("a.com", 1).unwrap()[0].id;
        db.delete_visit(newest).unwrap();
        let e = &db.suggest("a.com", 5).unwrap()[0];
        assert_eq!((e.visit_count, e.last_visit), (1, 1));
        db.delete_url("https://a.com/").unwrap();
        assert_eq!(db.search_history("", 10).unwrap().len(), 1);
        db.clear_history().unwrap();
        assert!(db.search_history("", 10).unwrap().is_empty());
    }
}
