use std::path::PathBuf;
use std::time::Duration;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone)]
pub struct Asset {
    pub id: i64,
    pub name: String,
    pub bytes: i64,
    pub category: String,
    pub provider: String,
    pub url: String,
    pub sha256: String,
    pub owner_id: String,
    pub filekey: String,
    pub created_at: String,
}

#[derive(Clone)]
pub struct Storage {
    path: PathBuf,
}

impl Storage {
    pub fn new(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("无法创建应用数据目录: {}", parent.display()))?;
        }
        let storage = Self { path };
        let conn = storage.open()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS assets (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               name TEXT NOT NULL,
               bytes INTEGER NOT NULL,
               category TEXT NOT NULL,
               provider TEXT NOT NULL,
               owner_id TEXT NOT NULL,
               url TEXT NOT NULL,
               sha256 TEXT NOT NULL,
               filekey TEXT NOT NULL DEFAULT '',
               created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
               UNIQUE(provider, owner_id, sha256)
             );
             CREATE INDEX IF NOT EXISTS idx_asset_recent ON assets(id DESC);"
        )?;
        Ok(storage)
    }

    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).with_context(|| format!("无法打开 SQLite: {}", self.path.display()))?;
        conn.busy_timeout(Duration::from_secs(5))?;
        // WAL allows a future read-only UI connection during a writer transaction.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Ok(conn)
    }

    pub fn recent_filtered(&self, limit: usize, query: &str, image_only: bool) -> Result<Vec<Asset>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id,name,bytes,category,provider,url,sha256,owner_id,filekey,created_at
             FROM assets
             WHERE (?1 = '' OR instr(lower(name), lower(?1)) > 0)
               AND (?3 = 0 OR category = 'image')
             ORDER BY id DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![query, limit as i64, i64::from(image_only)], map_row)?;
        let mut items = Vec::new();
        for row in rows { items.push(row?); }
        Ok(items)
    }

    pub fn count(&self) -> Result<i64> {
        Ok(self.open()?.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?)
    }

    pub fn find(&self, provider: &str, owner_id: &str, digest: &str) -> Result<Option<Asset>> {
        let conn = self.open()?;
        Ok(conn.query_row(
            "SELECT id,name,bytes,category,provider,url,sha256,owner_id,filekey,created_at
             FROM assets WHERE provider=?1 AND owner_id=?2 AND sha256=?3",
            params![provider, owner_id, digest], map_row,
        ).optional()?)
    }

    pub fn insert_or_get(&self, entry: &NewAsset) -> Result<Asset> {
        let conn = self.open()?;
        conn.execute(
            "INSERT INTO assets(name,bytes,category,provider,owner_id,url,sha256,filekey)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(provider,owner_id,sha256) DO NOTHING",
            params![entry.name, entry.bytes, entry.category, entry.provider,
                    entry.owner_id, entry.url, entry.sha256, entry.filekey],
        )?;
        drop(conn);
        self.find(&entry.provider, &entry.owner_id, &entry.sha256)?
            .context("SQLite 写入后找不到资源")
    }
}

pub struct NewAsset {
    pub name: String,
    pub bytes: i64,
    pub category: String,
    pub provider: String,
    pub owner_id: String,
    pub url: String,
    pub sha256: String,
    pub filekey: String,
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset {
        id: row.get(0)?, name: row.get(1)?, bytes: row.get(2)?,
        category: row.get(3)?, provider: row.get(4)?, url: row.get(5)?,
        sha256: row.get(6)?, owner_id: row.get(7)?, filekey: row.get(8)?,
        created_at: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sqlite_deduplicates_by_provider_and_owner() {
        let base = std::env::temp_dir().join(format!("yuque-desk-test-{}-{}", std::process::id(), rand_seed()));
        let path = base.join("test.db");
        let storage = Storage::new(path).unwrap();
        let a = NewAsset { name: "example.png".into(), bytes: 4, category: "image".into(),
            provider: "demo".into(), owner_id: "local".into(), url: "file:///a".into(), sha256: "abcd".into(), filekey: "".into() };
        let first = storage.insert_or_get(&a).unwrap();
        let again = storage.insert_or_get(&a).unwrap();
        assert_eq!(first.id, again.id);
        assert_eq!(storage.count().unwrap(), 1);
        drop(storage);
        let _ = std::fs::remove_dir_all(base);
    }
    fn rand_seed() -> u128 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    }
}
