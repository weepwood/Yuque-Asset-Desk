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

#[derive(Clone, Debug)]
pub struct Job {
    pub id: i64,
    pub path: PathBuf,
    pub state: String,
    pub error: String,
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
        // Journal mode is database-wide; set it once at initialization, not on each read.
        conn.pragma_update(None, "journal_mode", "WAL")?;
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
             CREATE INDEX IF NOT EXISTS idx_asset_recent ON assets(id DESC);
             CREATE INDEX IF NOT EXISTS idx_asset_scope_recent ON assets(provider, owner_id, id DESC);
             CREATE TABLE IF NOT EXISTS upload_jobs (
                id INTEGER PRIMARY KEY, provider TEXT NOT NULL, owner_id TEXT NOT NULL,
                path TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'pending', error TEXT NOT NULL DEFAULT ''
             );
             CREATE INDEX IF NOT EXISTS idx_upload_jobs_scope ON upload_jobs(provider, owner_id, state, id);
             CREATE TABLE IF NOT EXISTS saved_accounts (owner_id TEXT PRIMARY KEY NOT NULL);
             CREATE TABLE IF NOT EXISTS asset_tags (
                 asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
                 tag TEXT NOT NULL, PRIMARY KEY(asset_id, tag)
             );"
        )?;
        Ok(storage)
    }

    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).with_context(|| format!("无法打开 SQLite: {}", self.path.display()))?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(conn)
    }

    /// Stable keyset pagination; scope is explicit so Demo and different accounts
    /// never appear in the active account's history by default.
    #[cfg(test)]
    pub fn page(
        &self, limit: usize, query: &str, image_only: bool,
        scope: Option<(&str, &str)>, before: Option<i64>,
    ) -> Result<(Vec<Asset>, bool, i64)> {
        self.page_sorted(limit, query, image_only, scope, before, false)
    }

    #[cfg(test)]
    pub fn page_sorted(
        &self, limit: usize, query: &str, image_only: bool,
        scope: Option<(&str, &str)>, cursor: Option<i64>, oldest: bool,
    ) -> Result<(Vec<Asset>, bool, i64)> {
        self.page_category(limit, query, if image_only { "image" } else { "" }, scope, cursor, oldest)
    }

    pub fn page_category(
        &self, limit: usize, query: &str, category: &str,
        scope: Option<(&str, &str)>, cursor: Option<i64>, oldest: bool,
    ) -> Result<(Vec<Asset>, bool, i64)> {
        let conn = self.open()?;
        let (provider, owner) = match scope {
            Some((provider, owner)) => (Some(provider), Some(owner)),
            None => (None, None),
        };
        let limit = limit.clamp(1, 150);
        // Use fixed SQL fragments, not user input, so scoped/cursor queries
        // can use the composite index instead of nullable OR predicates.
        let scope_filter = if scope.is_some() {
            "AND provider = ?3 AND owner_id = ?4"
        } else {
            "AND ?3 IS NULL AND ?4 IS NULL"
        };
        let cursor_filter = match (cursor, oldest) {
            (Some(_), true) => "AND id > ?5",
            (Some(_), false) => "AND id < ?5",
            (None, _) => "AND ?5 IS NULL",
        };
        let order = if oldest { "ASC" } else { "DESC" };
        let sql = format!(
            "SELECT id,name,bytes,category,provider,url,sha256,owner_id,filekey,created_at
             FROM assets
             WHERE (?1 = '' OR instr(lower(name), lower(?1)) > 0
                 OR EXISTS (SELECT 1 FROM asset_tags WHERE asset_id = assets.id AND instr(lower(tag), lower(?1)) > 0))
               AND (?2 = '' OR category = ?2)
               {scope_filter} {cursor_filter}
             ORDER BY id {order} LIMIT ?6"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![query, category,
            provider, owner, cursor, (limit + 1) as i64], map_row)?;
        let mut items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let total = match scope {
            Some((provider, owner)) => conn.query_row(
                "SELECT count(*) FROM assets WHERE provider = ?1 AND owner_id = ?2",
                params![provider, owner], |r| r.get(0),
            )?,
            None => conn.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?,
        };
        Ok((items, has_more, total))
    }

    #[cfg(test)]
    pub fn recent_filtered(&self, limit: usize, query: &str, image_only: bool) -> Result<Vec<Asset>> {
        Ok(self.page(limit, query, image_only, None, None)?.0)
    }

    #[cfg(test)]
    pub fn count(&self) -> Result<i64> {
        Ok(self.page(1, "", false, None, None)?.2)
    }

    pub fn tags(&self, id: i64) -> Result<Vec<String>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare("SELECT tag FROM asset_tags WHERE asset_id=?1 ORDER BY tag")?;
        let rows = stmt.query_map([id], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn set_tags(&self, id: i64, text: &str) -> Result<()> {
        let tags: std::collections::BTreeSet<_> = text.split([',', '，'])
            .map(str::trim).filter(|s| !s.is_empty()).collect();
        anyhow::ensure!(tags.len() <= 20 && tags.iter().all(|s| s.chars().count() <= 40), "最多 20 个标签，每个不超过 40 字");
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM asset_tags WHERE asset_id=?1", [id])?;
        for tag in tags {
            tx.execute("INSERT INTO asset_tags(asset_id,tag) VALUES(?1,?2)", params![id, tag])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn forget_account(&self, owner: &str) -> Result<()> {
        self.open()?.execute("DELETE FROM saved_accounts WHERE owner_id=?1", [owner])?;
        Ok(())
    }

    pub fn save_account(&self, owner: &str) -> Result<()> {
        self.open()?.execute("INSERT OR IGNORE INTO saved_accounts(owner_id) VALUES(?1)", [owner])?;
        Ok(())
    }

    pub fn accounts(&self) -> Result<Vec<String>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare("SELECT owner_id FROM saved_accounts ORDER BY owner_id")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn enqueue(&self, provider: &str, owner: &str, files: &[PathBuf]) -> Result<Vec<(i64, PathBuf)>> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        let mut jobs = Vec::new();
        for path in files {
            let text = path.to_str().context("任务路径不是有效 UTF-8")?;
            tx.execute("INSERT INTO upload_jobs(provider,owner_id,path) VALUES(?1,?2,?3)",
                params![provider, owner, text])?;
            jobs.push((tx.last_insert_rowid(), path.clone()));
        }
        tx.commit()?;
        Ok(jobs)
    }

    pub fn jobs(&self, provider: &str, owner: &str, before: Option<i64>) -> Result<Vec<Job>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare("SELECT id,path,state,error FROM upload_jobs
            WHERE provider=?1 AND owner_id=?2 AND (?3 IS NULL OR id < ?3)
            ORDER BY id DESC LIMIT 51")?;
        let rows = stmt.query_map(params![provider, owner, before], |r| Ok(Job {
            id: r.get(0)?, path: PathBuf::from(r.get::<_,String>(1)?),
            state: r.get(2)?, error: r.get(3)?,
        }))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn retry_job(&self, provider: &str, owner: &str, id: i64) -> Result<Option<(i64, PathBuf)>> {
        let conn = self.open()?;
        Ok(conn.query_row("SELECT id,path FROM upload_jobs WHERE provider=?1 AND owner_id=?2 AND id=?3
            AND state IN ('pending','paused','failed','running')", params![provider, owner, id],
            |r| Ok((r.get(0)?, PathBuf::from(r.get::<_,String>(1)?)))).optional()?)
    }

    pub fn recoverable(&self, provider: &str, owner: &str) -> Result<Vec<(i64, PathBuf)>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare("SELECT id,path FROM upload_jobs WHERE provider=?1 AND owner_id=?2
            AND state IN ('pending','paused','failed','running') ORDER BY id")?;
        let rows = stmt.query_map(params![provider, owner], |r| {
            Ok((r.get(0)?, PathBuf::from(r.get::<_, String>(1)?)))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn job_state(&self, id: i64, state: &str, error: &str) -> Result<()> {
        self.open()?.execute("UPDATE upload_jobs SET state=?2,error=?3 WHERE id=?1",
            params![id, state, error])?;
        Ok(())
    }

    pub fn find(&self, provider: &str, owner_id: &str, digest: &str) -> Result<Option<Asset>> {
        let conn = self.open()?;
        Ok(conn.query_row(
            "SELECT id,name,bytes,category,provider,url,sha256,owner_id,filekey,created_at
             FROM assets WHERE provider=?1 AND owner_id=?2 AND sha256=?3",
            params![provider, owner_id, digest], map_row,
        ).optional()?)
    }

    pub fn refresh_demo_link(&self, id: i64, name: &str, url: &str) -> Result<()> {
        self.open()?.execute("UPDATE assets SET name=?2,url=?3 WHERE id=?1 AND provider='demo' AND owner_id='local'",
            params![id, name, url])?;
        Ok(())
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
    #[test]
    fn filters_in_sql_before_result_limit() {
        let base = std::env::temp_dir().join(format!("yuque-filter-{}-{}", std::process::id(), rand_seed()));
        let db = Storage::new(base.join("assets.db")).unwrap();
        for index in 0..14 {
            let is_first = index == 0;
            db.insert_or_get(&NewAsset {
                name: if is_first { "needle-image.png".into() } else { format!("recent-{index}.zip") },
                bytes: 42,
                category: if is_first { "image" } else { "attachment" }.into(),
                provider: "demo".into(),
                owner_id: "local".into(),
                url: format!("file:///demo/{index}"),
                sha256: format!("sha-{index}"),
                filekey: "".into(),
            }).unwrap();
        }
        assert_eq!(db.recent_filtered(5, "", false).unwrap().len(), 5);
        let older = db.recent_filtered(5, "needle", false).unwrap();
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].name, "needle-image.png");
        let images = db.recent_filtered(5, "", true).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].category, "image");
        assert_eq!(db.count().unwrap(), 14);
        drop(db);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn scoped_pages_do_not_mix_accounts_or_repeat_rows() {
        let base = std::env::temp_dir().join(format!("yuque-pages-{}-{}", std::process::id(), rand_seed()));
        let db = Storage::new(base.join("assets.db")).unwrap();
        for (provider, owner, digest) in [
            ("demo", "local", "same"), ("yuque", "1", "same"),
            ("yuque", "2", "same"), ("yuque", "1", "b"), ("yuque", "1", "c"),
        ] {
            db.insert_or_get(&NewAsset {
                name: format!("{digest}.png"), bytes: 1, category: "image".into(),
                provider: provider.into(), owner_id: owner.into(), url: "https://example.org/a".into(),
                sha256: digest.into(), filekey: "".into(),
            }).unwrap();
        }
        let scope = Some(("yuque", "1"));
        let (first, more, total) = db.page(2, "", false, scope, None).unwrap();
        assert_eq!(total, 3);
        assert!(more);
        assert!(first.iter().all(|a| a.owner_id == "1" && a.provider == "yuque"));
        let (second, more, _) = db.page(2, "", false, scope, Some(first.last().unwrap().id)).unwrap();
        assert_eq!(second.len(), 1);
        assert!(!more);
        assert!(second[0].id < first.last().unwrap().id);
        assert_eq!(db.page(10, "", false, None, None).unwrap().2, 5);
        assert_eq!(db.page(10, "same", true, scope, None).unwrap().0.len(), 1);
        drop(db);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn persisted_jobs_are_scoped_and_recoverable() {
        let base = std::env::temp_dir().join(format!("yuque-jobs-{}-{}", std::process::id(), rand_seed()));
        let path = base.join("assets.db");
        let db = Storage::new(path.clone()).unwrap();
        let jobs = db.enqueue("yuque", "1", &[PathBuf::from("a.png"), PathBuf::from("b.png")]).unwrap();
        db.job_state(jobs[0].0, "done", "").unwrap();
        db.job_state(jobs[1].0, "running", "").unwrap();
        db.save_account("1").unwrap();
        drop(db);
        let db = Storage::new(path).unwrap();
        assert_eq!(db.recoverable("yuque", "1").unwrap(), vec![jobs[1].clone()]);
        assert!(db.recoverable("yuque", "2").unwrap().is_empty());
        db.job_state(jobs[1].0, "failed", "fixture failure").unwrap();
        assert_eq!(db.jobs("yuque", "1", None).unwrap()[0].error, "fixture failure");
        assert!(db.jobs("yuque", "2", None).unwrap().is_empty());
        assert!(db.retry_job("yuque", "2", jobs[1].0).unwrap().is_none());
        assert!(db.retry_job("yuque", "1", jobs[0].0).unwrap().is_none());
        assert_eq!(db.retry_job("yuque", "1", jobs[1].0).unwrap(), Some(jobs[1].clone()));
        assert_eq!(db.accounts().unwrap(), vec!["1".to_string()]);
        db.job_state(jobs[1].0, "cancelled", "").unwrap();
        assert!(db.recoverable("yuque", "1").unwrap().is_empty());
        drop(db);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn tags_search_and_ascending_cursor() {
        let base = std::env::temp_dir().join(format!("yuque-tags-{}-{}", std::process::id(), rand_seed()));
        let db = Storage::new(base.join("db")).unwrap();
        let ids: Vec<_> = (0..3).map(|n| db.insert_or_get(&NewAsset {
            name: format!("image-{n}"), bytes: 1, category: "image".into(),
            provider: "demo".into(), owner_id: "local".into(), url: "file:///x".into(),
            sha256: n.to_string(), filekey: "".into(),
        }).unwrap().id).collect();
        db.set_tags(ids[1], "设计, 摄影，设计").unwrap();
        assert_eq!(db.tags(ids[1]).unwrap().len(), 2);
        assert!(db.page_category(10, "", "video", None, None, false).unwrap().0.is_empty());
        assert_eq!(db.page_category(10, "", "image", None, None, false).unwrap().0.len(), 3);
        assert_eq!(db.page_sorted(10, "摄影", false, None, None, false).unwrap().0[0].id, ids[1]);
        let (first, more, _) = db.page_sorted(2, "", false, None, None, true).unwrap();
        assert!(more);
        assert_eq!(first.iter().map(|a| a.id).collect::<Vec<_>>(), ids[..2]);
        let (next, more, _) = db.page_sorted(2, "", false, None, Some(ids[1]), true).unwrap();
        assert!(!more);
        assert_eq!(next[0].id, ids[2]);
        drop(db);
        let _ = std::fs::remove_dir_all(base);
    }

    fn rand_seed() -> u128 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    }
}
