//! Bookmark tree: folders and URLs under two fixed roots.

use anyhow::{bail, Context};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::{like_contains, Database};

pub const BOOKMARKS_BAR: i64 = 1;
pub const OTHER_BOOKMARKS: i64 = 2;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BookmarkKind {
    Folder,
    Url,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub kind: BookmarkKind,
    pub title: String,
    pub url: Option<String>,
    pub position: i64,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct BookmarkNode {
    #[serde(flatten)]
    pub bookmark: Bookmark,
    pub children: Vec<BookmarkNode>,
}

const COLS: &str = "id, parent_id, kind, title, url, position, created_at";

fn from_row(r: &Row) -> rusqlite::Result<Bookmark> {
    let kind: String = r.get(2)?;
    Ok(Bookmark {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        kind: if kind == "folder" {
            BookmarkKind::Folder
        } else {
            BookmarkKind::Url
        },
        title: r.get(3)?,
        url: r.get(4)?,
        position: r.get(5)?,
        created_at: r.get(6)?,
    })
}

impl Database {
    fn get_bookmark(&self, id: i64) -> anyhow::Result<Bookmark> {
        self.conn()
            .query_row(
                &format!("SELECT {COLS} FROM bookmarks WHERE id = ?1"),
                [id],
                from_row,
            )
            .optional()?
            .with_context(|| format!("bookmark {id} not found"))
    }

    fn require_folder(&self, id: i64) -> anyhow::Result<()> {
        if self.get_bookmark(id)?.kind != BookmarkKind::Folder {
            bail!("bookmark {id} is not a folder");
        }
        Ok(())
    }

    fn insert_bookmark(
        &self,
        parent_id: i64,
        kind: &str,
        title: &str,
        url: Option<&str>,
    ) -> anyhow::Result<i64> {
        self.require_folder(parent_id)?;
        self.conn().execute(
            "INSERT INTO bookmarks (parent_id, kind, title, url, position, created_at)
             VALUES (?1, ?2, ?3, ?4,
                     (SELECT COALESCE(MAX(position) + 1, 0) FROM bookmarks WHERE parent_id = ?1), ?5)",
            params![parent_id, kind, title, url, crate::now_ms()],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    pub fn add_bookmark(&self, parent_id: i64, title: &str, url: &str) -> anyhow::Result<i64> {
        self.insert_bookmark(parent_id, "url", title, Some(url))
    }

    pub fn add_folder(&self, parent_id: i64, title: &str) -> anyhow::Result<i64> {
        self.insert_bookmark(parent_id, "folder", title, None)
    }

    /// Change the title and, for URL bookmarks, the URL (when given).
    pub fn update_bookmark(&self, id: i64, title: &str, url: Option<&str>) -> anyhow::Result<()> {
        let b = self.get_bookmark(id)?;
        let url = match b.kind {
            BookmarkKind::Url => url.map(str::to_string).or(b.url),
            BookmarkKind::Folder => None,
        };
        self.conn().execute(
            "UPDATE bookmarks SET title = ?2, url = ?3 WHERE id = ?1",
            params![id, title, url],
        )?;
        Ok(())
    }

    /// Ids of the children of `parent` in order, optionally leaving out `skip`.
    fn child_ids(&self, parent: i64, skip: Option<i64>) -> anyhow::Result<Vec<i64>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id FROM bookmarks WHERE parent_id = ?1 ORDER BY position, id")?;
        let ids = stmt
            .query_map([parent], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids.into_iter().filter(|i| Some(*i) != skip).collect())
    }

    fn renumber(&self, parent: i64, ids: &[i64]) -> anyhow::Result<()> {
        for (pos, id) in ids.iter().enumerate() {
            self.conn().execute(
                "UPDATE bookmarks SET parent_id = ?2, position = ?3 WHERE id = ?1",
                params![id, parent, pos as i64],
            )?;
        }
        Ok(())
    }

    /// Move to `new_parent` at `index` (clamped). Roots cannot move; a folder cannot move
    /// into itself or a descendant.
    pub fn move_bookmark(&self, id: i64, new_parent: i64, index: usize) -> anyhow::Result<()> {
        let b = self.get_bookmark(id)?;
        let Some(old_parent) = b.parent_id else {
            bail!("root folders cannot be moved");
        };
        self.require_folder(new_parent)?;
        let mut cur = Some(new_parent);
        while let Some(c) = cur {
            if c == id {
                bail!("cannot move a folder into itself or its descendant");
            }
            cur = self.get_bookmark(c)?.parent_id;
        }
        let tx = self.conn().unchecked_transaction()?;
        let mut dest = self.child_ids(new_parent, Some(id))?;
        dest.insert(index.min(dest.len()), id);
        self.renumber(new_parent, &dest)?;
        if old_parent != new_parent {
            let src = self.child_ids(old_parent, Some(id))?;
            self.renumber(old_parent, &src)?;
        }
        Ok(tx.commit()?)
    }

    /// Delete a bookmark; folders are deleted with everything inside. Roots cannot be deleted.
    pub fn delete_bookmark(&self, id: i64) -> anyhow::Result<()> {
        let b = self.get_bookmark(id)?;
        let Some(parent) = b.parent_id else {
            bail!("root folders cannot be deleted");
        };
        let tx = self.conn().unchecked_transaction()?;
        // Children go with it through ON DELETE CASCADE.
        self.conn()
            .execute("DELETE FROM bookmarks WHERE id = ?1", [id])?;
        let rest = self.child_ids(parent, None)?;
        self.renumber(parent, &rest)?;
        Ok(tx.commit()?)
    }

    pub fn bookmark_children(&self, parent_id: i64) -> anyhow::Result<Vec<Bookmark>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLS} FROM bookmarks WHERE parent_id = ?1 ORDER BY position, id"
        ))?;
        let rows = stmt.query_map([parent_id], from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Both root folders with full subtrees.
    pub fn bookmark_tree(&self) -> anyhow::Result<Vec<BookmarkNode>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLS} FROM bookmarks WHERE parent_id IS NULL ORDER BY position, id"
        ))?;
        let roots = stmt
            .query_map([], from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        roots.into_iter().map(|b| self.build_node(b)).collect()
    }

    fn build_node(&self, bookmark: Bookmark) -> anyhow::Result<BookmarkNode> {
        let children = if bookmark.kind == BookmarkKind::Folder {
            self.bookmark_children(bookmark.id)?
                .into_iter()
                .map(|c| self.build_node(c))
                .collect::<anyhow::Result<_>>()?
        } else {
            Vec::new()
        };
        Ok(BookmarkNode { bookmark, children })
    }

    pub fn find_bookmarks_by_url(&self, url: &str) -> anyhow::Result<Vec<Bookmark>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLS} FROM bookmarks WHERE kind = 'url' AND url = ?1 ORDER BY id"
        ))?;
        let rows = stmt.query_map([url], from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// URL bookmarks whose title or URL contains `query` (case-insensitive).
    pub fn search_bookmarks(&self, query: &str, limit: usize) -> anyhow::Result<Vec<Bookmark>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLS} FROM bookmarks
             WHERE kind = 'url' AND (title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\')
             ORDER BY created_at DESC, id DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![like_contains(query), limit as i64], from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(v: &[Bookmark]) -> Vec<&str> {
        v.iter().map(|b| b.title.as_str()).collect()
    }

    #[test]
    fn add_and_order() {
        let db = Database::open_in_memory().unwrap();
        let a = db
            .add_bookmark(BOOKMARKS_BAR, "A", "https://a.com/")
            .unwrap();
        db.add_bookmark(BOOKMARKS_BAR, "B", "https://b.com/")
            .unwrap();
        let f = db.add_folder(BOOKMARKS_BAR, "F").unwrap();
        let kids = db.bookmark_children(BOOKMARKS_BAR).unwrap();
        assert_eq!(titles(&kids), ["A", "B", "F"]);
        assert_eq!(
            kids.iter().map(|b| b.position).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(kids[2].kind, BookmarkKind::Folder);
        assert!(kids[2].url.is_none());
        // parent must be a folder
        assert!(db.add_bookmark(a, "x", "https://x.com/").is_err());
        assert!(db.add_bookmark(999, "x", "https://x.com/").is_err());
        assert!(db.add_folder(f, "sub").is_ok());
    }

    #[test]
    fn update() {
        let db = Database::open_in_memory().unwrap();
        let a = db
            .add_bookmark(BOOKMARKS_BAR, "A", "https://a.com/")
            .unwrap();
        db.update_bookmark(a, "A2", None).unwrap();
        db.update_bookmark(a, "A3", Some("https://c.com/")).unwrap();
        let b = &db.bookmark_children(BOOKMARKS_BAR).unwrap()[0];
        assert_eq!(
            (b.title.as_str(), b.url.as_deref()),
            ("A3", Some("https://c.com/"))
        );
        db.update_bookmark(a, "A4", None).unwrap();
        assert_eq!(
            db.bookmark_children(BOOKMARKS_BAR).unwrap()[0]
                .url
                .as_deref(),
            Some("https://c.com/")
        );
        assert!(db.update_bookmark(999, "x", None).is_err());
    }

    #[test]
    fn move_renumbers() {
        let db = Database::open_in_memory().unwrap();
        let a = db
            .add_bookmark(BOOKMARKS_BAR, "A", "https://a.com/")
            .unwrap();
        let b = db
            .add_bookmark(BOOKMARKS_BAR, "B", "https://b.com/")
            .unwrap();
        let c = db
            .add_bookmark(BOOKMARKS_BAR, "C", "https://c.com/")
            .unwrap();
        let f = db.add_folder(OTHER_BOOKMARKS, "F").unwrap();
        db.add_bookmark(f, "X", "https://x.com/").unwrap();

        db.move_bookmark(c, BOOKMARKS_BAR, 0).unwrap();
        assert_eq!(
            titles(&db.bookmark_children(BOOKMARKS_BAR).unwrap()),
            ["C", "A", "B"]
        );
        db.move_bookmark(a, BOOKMARKS_BAR, 99).unwrap();
        assert_eq!(
            titles(&db.bookmark_children(BOOKMARKS_BAR).unwrap()),
            ["C", "B", "A"]
        );

        db.move_bookmark(b, f, 0).unwrap();
        assert_eq!(
            titles(&db.bookmark_children(BOOKMARKS_BAR).unwrap()),
            ["C", "A"]
        );
        let src = db.bookmark_children(BOOKMARKS_BAR).unwrap();
        assert_eq!(src.iter().map(|b| b.position).collect::<Vec<_>>(), [0, 1]);
        let dst = db.bookmark_children(f).unwrap();
        assert_eq!(titles(&dst), ["B", "X"]);
        assert_eq!(dst.iter().map(|b| b.position).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(dst[0].parent_id, Some(f));
    }

    #[test]
    fn move_rejections() {
        let db = Database::open_in_memory().unwrap();
        let f = db.add_folder(BOOKMARKS_BAR, "F").unwrap();
        let g = db.add_folder(f, "G").unwrap();
        let h = db.add_folder(g, "H").unwrap();
        let u = db
            .add_bookmark(BOOKMARKS_BAR, "U", "https://u.com/")
            .unwrap();
        assert!(db.move_bookmark(f, f, 0).is_err());
        assert!(db.move_bookmark(f, g, 0).is_err());
        assert!(db.move_bookmark(f, h, 0).is_err());
        assert!(db.move_bookmark(BOOKMARKS_BAR, OTHER_BOOKMARKS, 0).is_err());
        assert!(db.move_bookmark(OTHER_BOOKMARKS, f, 0).is_err());
        assert!(db.move_bookmark(f, u, 0).is_err()); // not a folder
        assert!(db.move_bookmark(h, f, 0).is_ok());
    }

    #[test]
    fn delete_and_roots() {
        let db = Database::open_in_memory().unwrap();
        let a = db
            .add_bookmark(BOOKMARKS_BAR, "A", "https://a.com/")
            .unwrap();
        let f = db.add_folder(BOOKMARKS_BAR, "F").unwrap();
        let g = db.add_folder(f, "G").unwrap();
        db.add_bookmark(g, "deep", "https://d.com/").unwrap();
        let z = db
            .add_bookmark(BOOKMARKS_BAR, "Z", "https://z.com/")
            .unwrap();
        db.delete_bookmark(f).unwrap();
        let kids = db.bookmark_children(BOOKMARKS_BAR).unwrap();
        assert_eq!(titles(&kids), ["A", "Z"]);
        assert_eq!(kids.iter().map(|b| b.position).collect::<Vec<_>>(), [0, 1]);
        assert!(db
            .find_bookmarks_by_url("https://d.com/")
            .unwrap()
            .is_empty());
        assert!(db.delete_bookmark(BOOKMARKS_BAR).is_err());
        assert!(db.delete_bookmark(OTHER_BOOKMARKS).is_err());
        assert!(db.delete_bookmark(999).is_err());
        db.delete_bookmark(a).unwrap();
        db.delete_bookmark(z).unwrap();
        assert_eq!(db.bookmark_tree().unwrap().len(), 2);
    }

    #[test]
    fn tree_find_search() {
        let db = Database::open_in_memory().unwrap();
        let f = db.add_folder(OTHER_BOOKMARKS, "Dev").unwrap();
        db.add_bookmark(f, "Rust Lang", "https://rust-lang.org/")
            .unwrap();
        db.add_bookmark(BOOKMARKS_BAR, "Rust again", "https://rust-lang.org/")
            .unwrap();
        db.add_bookmark(BOOKMARKS_BAR, "Other", "https://o.com/")
            .unwrap();
        let tree = db.bookmark_tree().unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].bookmark.id, BOOKMARKS_BAR);
        assert_eq!(tree[0].children.len(), 2);
        assert_eq!(tree[1].children[0].children[0].bookmark.title, "Rust Lang");
        assert_eq!(
            db.find_bookmarks_by_url("https://rust-lang.org/")
                .unwrap()
                .len(),
            2
        );
        assert_eq!(db.search_bookmarks("RUST", 10).unwrap().len(), 2);
        assert_eq!(db.search_bookmarks("o.com", 10).unwrap().len(), 1);
        assert_eq!(db.search_bookmarks("Dev", 10).unwrap().len(), 0); // folders excluded
        assert_eq!(db.search_bookmarks("rust", 1).unwrap().len(), 1);

        let json = serde_json::to_value(&tree[0]).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["kind"], "folder");
        assert!(json["children"].is_array());
    }
}
