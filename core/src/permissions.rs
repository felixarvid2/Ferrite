//! Per-site permission decisions.

use anyhow::{bail, Context};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Database;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Block,
}

impl Decision {
    fn as_str(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Block => "block",
        }
    }

    fn parse(s: &str) -> anyhow::Result<Decision> {
        match s {
            "allow" => Ok(Decision::Allow),
            "block" => Ok(Decision::Block),
            other => bail!("unknown decision {other:?}"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SitePermission {
    pub origin: String,
    pub permission: String,
    pub decision: Decision,
    pub updated_at: i64,
}

impl Database {
    pub fn get_permission(
        &self,
        origin: &str,
        permission: &str,
    ) -> anyhow::Result<Option<Decision>> {
        let s: Option<String> = self
            .conn()
            .query_row(
                "SELECT decision FROM permissions WHERE origin = ?1 AND permission = ?2",
                params![origin, permission],
                |r| r.get(0),
            )
            .optional()?;
        s.as_deref().map(Decision::parse).transpose()
    }

    pub fn set_permission(
        &self,
        origin: &str,
        permission: &str,
        decision: Decision,
        at: i64,
    ) -> anyhow::Result<()> {
        self.conn().execute(
            "INSERT INTO permissions (origin, permission, decision, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(origin, permission) DO UPDATE SET
                decision = excluded.decision, updated_at = excluded.updated_at",
            params![origin, permission, decision.as_str(), at],
        )?;
        Ok(())
    }

    pub fn remove_permission(&self, origin: &str, permission: &str) -> anyhow::Result<()> {
        self.conn().execute(
            "DELETE FROM permissions WHERE origin = ?1 AND permission = ?2",
            params![origin, permission],
        )?;
        Ok(())
    }

    pub fn list_permissions(&self) -> anyhow::Result<Vec<SitePermission>> {
        let mut stmt = self.conn().prepare(
            "SELECT origin, permission, decision, updated_at FROM permissions
             ORDER BY origin, permission",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (origin, permission, decision, updated_at) = row?;
            Ok(SitePermission {
                origin,
                permission,
                decision: Decision::parse(&decision).context("reading permission")?,
                updated_at,
            })
        })
        .collect()
    }

    pub fn clear_permissions(&self) -> anyhow::Result<()> {
        self.conn().execute("DELETE FROM permissions", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let db = Database::open_in_memory().unwrap();
        let o = "https://a.com";
        assert_eq!(db.get_permission(o, "camera").unwrap(), None);
        db.set_permission(o, "camera", Decision::Allow, 10).unwrap();
        db.set_permission(o, "geolocation", Decision::Block, 11)
            .unwrap();
        assert_eq!(
            db.get_permission(o, "camera").unwrap(),
            Some(Decision::Allow)
        );
        db.set_permission(o, "camera", Decision::Block, 20).unwrap();
        assert_eq!(
            db.get_permission(o, "camera").unwrap(),
            Some(Decision::Block)
        );
        let list = db.list_permissions().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].permission, "camera");
        assert_eq!(list[0].updated_at, 20);
        db.remove_permission(o, "camera").unwrap();
        assert_eq!(db.get_permission(o, "camera").unwrap(), None);
        db.clear_permissions().unwrap();
        assert!(db.list_permissions().unwrap().is_empty());
        assert_eq!(
            serde_json::to_string(&Decision::Allow).unwrap(),
            "\"allow\""
        );
    }
}
