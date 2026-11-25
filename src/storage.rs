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
    pub preview:Option<(String,String)>,
    pub thumbnail:Option<PathBuf>,
    pub result_url:Option<String>,
    pub result_asset_id:Option<i64>,
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
             CREATE TABLE IF NOT EXISTS desk_preferences (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS saved_accounts (owner_id TEXT PRIMARY KEY NOT NULL);
             CREATE TABLE IF NOT EXISTS asset_favorites (
                 asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS asset_sources (
                 asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
                 path TEXT NOT NULL, PRIMARY KEY(asset_id,path)
             );
             CREATE TABLE IF NOT EXISTS asset_preview_state (
                 asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
                 state TEXT NOT NULL, error TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE IF NOT EXISTS upload_job_assets (
                 job_id INTEGER PRIMARY KEY REFERENCES upload_jobs(id) ON DELETE CASCADE,
                 asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS upload_receipts (
                 job_id INTEGER PRIMARY KEY REFERENCES upload_jobs(id) ON DELETE CASCADE,
                 name TEXT NOT NULL, bytes INTEGER NOT NULL, category TEXT NOT NULL,
                 provider TEXT NOT NULL, owner_id TEXT NOT NULL, url TEXT NOT NULL,
                 filekey TEXT NOT NULL, sha256 TEXT NOT NULL, source TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS asset_visual (
                 asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
                 hash INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS asset_tags (
                 asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
                 tag TEXT NOT NULL, PRIMARY KEY(asset_id, tag)
             );"
        )?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS desk_migrations (name TEXT PRIMARY KEY);
            BEGIN IMMEDIATE;
            UPDATE upload_jobs SET state='unknown' WHERE provider='yuque' AND state='failed'
                AND NOT EXISTS (SELECT 1 FROM desk_migrations WHERE name='safe-job-recovery-v1');
            INSERT OR IGNORE INTO desk_migrations(name) VALUES('safe-job-recovery-v1');
            UPDATE upload_jobs SET state='unknown',error='上次请求结果未知，请先核实远端' WHERE provider='yuque' AND state='running';
            UPDATE upload_jobs SET state='paused' WHERE provider='demo' AND state='running';
            COMMIT;")?;
        drop(conn);
        storage.recover_receipts()?;
        Ok(storage)
    }

    pub fn preferences(&self)->Result<serde_json::Value>{
        let raw:Option<String>=self.open()?.query_row("SELECT value FROM desk_preferences WHERE id=1",[],|r|r.get(0)).optional()?;
        Ok(raw.and_then(|v|serde_json::from_str(&v).ok()).unwrap_or_default())
    }
    pub fn save_preferences(&self,value:&serde_json::Value)->Result<()>{
        self.open()?.execute("INSERT INTO desk_preferences(id,value) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET value=excluded.value",[value.to_string()])?;Ok(())
    }

    /// Open without migrations/recovery so an exit helper cannot mutate task state.
    pub fn existing(path:PathBuf)->Result<Self>{
        let m=std::fs::symlink_metadata(&path)?;
        anyhow::ensure!(m.is_file()&&!m.file_type().is_symlink(),"数据库不是普通文件");
        Ok(Self{path})
    }
    pub fn unfinished_count(&self)->Result<u64>{
        Ok(self.open()?.query_row("SELECT count(*) FROM upload_jobs WHERE state NOT IN ('done','cancelled')",[],|r|r.get(0))?)
    }
    pub fn local_references(&self)->Result<Vec<PathBuf>>{
        let conn=self.open()?;
        let mut stmt=conn.prepare("SELECT path FROM upload_jobs")?;
        let mut refs=stmt.query_map([],|r|r.get::<_,String>(0))?.map(|r|r.map(PathBuf::from)).collect::<rusqlite::Result<Vec<_>>>()?;
        let mut stmt=conn.prepare("SELECT path FROM asset_sources")?;
        refs.extend(stmt.query_map([],|r|r.get::<_,String>(0))?.map(|r|r.map(PathBuf::from)).collect::<rusqlite::Result<Vec<_>>>()?);
        let mut stmt=conn.prepare("SELECT url FROM assets WHERE provider='demo'")?;
        for row in stmt.query_map([],|r|r.get::<_,String>(0))?{
            let raw=row?;
            let url=url::Url::parse(&raw).context("本地资源链接格式不合法")?;
            if url.scheme()=="file"{refs.push(url.to_file_path().map_err(|_|anyhow::anyhow!("无法解析本地资源路径"))?);}
        }
        Ok(refs)
    }

    pub fn data_dir(&self) -> &std::path::Path {
        self.path.parent().unwrap_or_else(|| std::path::Path::new("."))
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

    #[cfg(test)]
    pub fn page_category(
        &self, limit: usize, query: &str, category: &str,
        scope: Option<(&str, &str)>, cursor: Option<i64>, oldest: bool,
    ) -> Result<(Vec<Asset>, bool, i64)> {
        self.page_tagged(limit,query,category,scope,cursor,oldest,None)
    }

    #[cfg(test)]
    pub fn page_tagged(&self, limit:usize, query:&str, category:&str, scope:Option<(&str,&str)>, cursor:Option<i64>, oldest:bool, tag:Option<&str>) -> Result<(Vec<Asset>,bool,i64)> {
        self.page_album(limit,query,category,scope,cursor,oldest,tag,false)
    }

    pub fn page_album(&self, limit:usize, query:&str, category:&str, scope:Option<(&str,&str)>, cursor:Option<i64>, oldest:bool, tag:Option<&str>, favorites:bool) -> Result<(Vec<Asset>,bool,i64)> {
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
        let exact_id=query.strip_prefix("id:").filter(|s|s.parse::<i64>().is_ok_and(|id|id>0));
        let query_filter=if exact_id.is_some(){"id = ?1"}else{"(?1 = '' OR instr(lower(name), lower(?1)) > 0 OR EXISTS (SELECT 1 FROM asset_tags WHERE asset_id = assets.id AND instr(lower(tag), lower(?1)) > 0))"};
        let sql = format!(
            "SELECT id,name,bytes,category,provider,url,sha256,owner_id,filekey,created_at
             FROM assets
             WHERE {query_filter}
               AND (?2 = '' OR category = ?2)
               AND (?7 IS NULL OR EXISTS (SELECT 1 FROM asset_tags WHERE asset_id=assets.id
                   AND (tag=?7 OR substr(tag,1,length(?7)+1)=?7||'/')))
               AND (?8=0 OR EXISTS(SELECT 1 FROM asset_favorites f WHERE f.asset_id=assets.id))
               {scope_filter} {cursor_filter}
             ORDER BY id {order} LIMIT ?6"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![exact_id.unwrap_or(query), category,
            provider, owner, cursor, (limit + 1) as i64, tag, favorites], map_row)?;
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

    /// Export a consistent SQLite snapshot plus local derivatives only. Never
    /// copies the OS credential vault or original clipboard/upload source files.
    pub fn export_album(&self,parent:&std::path::Path) -> Result<PathBuf> {
        let parent=parent.canonicalize()?;
        let source=self.data_dir().canonicalize()?;
        anyhow::ensure!(!parent.starts_with(&source),"请选择应用数据目录以外的备份位置");
        let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
        let target=parent.join(format!("YuqueAlbum-{stamp}"));
        std::fs::create_dir(&target)?;
        let result=(||->Result<()>{
            let path=target.join("assets.db");
            self.open()?.execute("VACUUM INTO ?1",[path.to_str().context("备份路径不是 UTF-8")?])?;
            let check=Connection::open(&path)?;
            let integrity:String=check.query_row("PRAGMA quick_check",[],|r|r.get(0))?;
            anyhow::ensure!(integrity=="ok","备份数据库校验失败");
            for folder in ["album-previews","thumbnails"] {
                let from=source.join(folder);
                if !from.is_dir(){continue;}
                let dest=target.join(folder);
                std::fs::create_dir(&dest)?;
                for item in std::fs::read_dir(from)? {
                    let item=item?;
                    if !item.file_type()?.is_file(){continue;}
                    let name=item.file_name();
                    let Some(name_str)=name.to_str() else{continue;};
                    let key=if folder=="album-previews"{name_str.strip_suffix(".jpg")}
                        else{name_str.strip_suffix(".png").map(|s|s.strip_prefix("large-").unwrap_or(s))};
                    if key.is_some_and(|s|s.len()==64 && s.bytes().all(|b|b.is_ascii_hexdigit())){
                        std::fs::copy(item.path(),dest.join(name))?;
                    }
                }
            }
            std::fs::write(target.join("BACKUP.txt"),"Yuque Asset Desk album backup v1
Includes SQLite links and thumbnails. No credentials or original upload files.
Restore into a NEW data directory only; see README.
")?;
            Ok(())
        })();
        if let Err(e)=result {
            let _=std::fs::remove_dir_all(&target);
            return Err(e);
        }
        Ok(target)
    }

    pub fn favorite_ids(&self,scope:Option<(&str,&str)>) -> Result<std::collections::HashSet<i64>> {
        let conn=self.open()?;
        let (provider,owner)=scope.map_or((None,None),|(p,o)|(Some(p),Some(o)));
        let mut stmt=conn.prepare("SELECT f.asset_id FROM asset_favorites f JOIN assets a ON a.id=f.asset_id WHERE (?1 IS NULL OR (a.provider=?1 AND a.owner_id=?2))")?;
        let rows=stmt.query_map(params![provider,owner],|r|r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn toggle_favorite(&self,asset:&Asset) -> Result<bool> {
        let mut conn=self.open()?;
        let tx=conn.transaction()?;
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND provider=?2 AND owner_id=?3)",params![asset.id,asset.provider,asset.owner_id],|r|r.get(0))?;
        anyhow::ensure!(exists,"资源已变化");
        let removed=tx.execute("DELETE FROM asset_favorites WHERE asset_id=?1",[asset.id])?;
        if removed==0 {tx.execute("INSERT INTO asset_favorites(asset_id) VALUES(?1)",[asset.id])?;}
        tx.commit()?;
        Ok(removed==0)
    }

    pub fn visual_hash(&self,id:i64) -> Result<Option<u64>> {
        Ok(self.open()?.query_row("SELECT hash FROM asset_visual WHERE asset_id=?1",[id],|r|r.get::<_,i64>(0)).optional()?.map(|v|v as u64))
    }

    pub fn store_visual_hash(&self,id:i64,hash:u64) -> Result<()> {
        self.open()?.execute("INSERT INTO asset_visual(asset_id,hash) VALUES(?1,?2) ON CONFLICT(asset_id) DO UPDATE SET hash=excluded.hash",params![id,hash as i64])?;
        Ok(())
    }

    /// Only unindexed images, in the source account; no repeated COUNT or per-row SQL lookup.
    pub fn unindexed_images(&self,asset:&Asset,before:Option<i64>) -> Result<Vec<Asset>> {
        let conn=self.open()?;
        let cursor=if before.is_some(){"AND a.id < ?3"}else{"AND ?3 IS NULL"};
        let sql=format!("SELECT a.id,a.name,a.bytes,a.category,a.provider,a.url,a.sha256,a.owner_id,a.filekey,a.created_at
            FROM assets a WHERE a.provider=?1 AND a.owner_id=?2 AND a.category='image'
            AND NOT EXISTS(SELECT 1 FROM asset_visual v WHERE v.asset_id=a.id) {cursor}
            ORDER BY a.id DESC LIMIT 150");
        let mut stmt=conn.prepare(&sql)?;
        let rows=stmt.query_map(params![asset.provider,asset.owner_id,before],map_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    #[cfg(test)]
    pub fn similar_assets(&self,asset:&Asset,hash:u64) -> Result<Vec<Asset>> {
        self.similar_assets_until(asset,hash,||false)
    }

    pub fn similar_assets_until(&self,asset:&Asset,hash:u64,cancelled:impl Fn()->bool) -> Result<Vec<Asset>> {
        let conn=self.open()?;
        let mut stmt=conn.prepare("SELECT a.id,a.name,a.bytes,a.category,a.provider,a.url,a.sha256,a.owner_id,a.filekey,a.created_at,v.hash
            FROM assets a JOIN asset_visual v ON a.id=v.asset_id WHERE a.provider=?1 AND a.owner_id=?2 AND a.id<>?3 AND a.category='image'")?;
        let rows=stmt.query_map(params![asset.provider,asset.owner_id,asset.id],|r|Ok((map_row(r)?,r.get::<_,i64>(10)? as u64)))?;
        // Keep the best 150 throughout, ordered by distance and newest ID.
        let mut matches=std::collections::BTreeMap::new();
        for row in rows {
            anyhow::ensure!(!cancelled(),"查询已取消");
            let (a,h)=row?;
            let distance=(hash^h).count_ones();
            if distance<=10 {
                matches.insert((distance,std::cmp::Reverse(a.id)),a);
                if matches.len()>150{matches.pop_last();}
            }
        }
        Ok(matches.into_values().collect())
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
        anyhow::ensure!(tags.len() <= 20 && tags.iter().all(|s| s.chars().count() <= 40 && s.split('/').all(|part|!part.trim().is_empty())), "最多 20 个标签，每个不超过 40 字");
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM asset_tags WHERE asset_id=?1", [id])?;
        for tag in tags {
            tx.execute("INSERT INTO asset_tags(asset_id,tag) VALUES(?1,?2)", params![id, tag])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn tag_catalog(&self, scope:Option<(&str,&str)>) -> Result<Vec<String>> {
        let conn=self.open()?;
        let (provider,owner)=scope.map_or((None,None),|(p,o)|(Some(p),Some(o)));
        let mut stmt=conn.prepare("SELECT DISTINCT t.tag FROM asset_tags t JOIN assets a ON a.id=t.asset_id
            WHERE (?1 IS NULL OR (a.provider=?1 AND a.owner_id=?2)) ORDER BY t.tag LIMIT 500")?;
        let rows=stmt.query_map(params![provider,owner],|r|r.get::<_,String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn batch_tags(&self, ids:&[i64], scope:Option<(&str,&str)>, text:&str, remove:bool) -> Result<()> {
        let tags:std::collections::BTreeSet<_>=text.split([',','，']).map(str::trim).filter(|s|!s.is_empty()).collect();
        anyhow::ensure!(!ids.is_empty() && ids.len()<=150 && !tags.is_empty() && tags.len()<=20 && tags.iter().all(|t|t.chars().count()<=40 && t.split('/').all(|s|!s.trim().is_empty())), "标签或选中资源不符合限制");
        let mut conn=self.open()?;
        let tx=conn.transaction()?;
        let (provider,owner)=scope.map_or((None,None),|(p,o)|(Some(p),Some(o)));
        for id in ids {
            let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND (?2 IS NULL OR (provider=?2 AND owner_id=?3)))",params![id,provider,owner],|r|r.get(0))?;
            anyhow::ensure!(exists,"资源已变化或不属于当前账号");
            for tag in &tags {
                if remove { tx.execute("DELETE FROM asset_tags WHERE asset_id=?1 AND tag=?2",params![id,tag])?; }
                else { tx.execute("INSERT OR IGNORE INTO asset_tags(asset_id,tag) VALUES(?1,?2)",params![id,tag])?; }
            }
            let n:i64=tx.query_row("SELECT count(*) FROM asset_tags WHERE asset_id=?1",[id],|r|r.get(0))?;
            anyhow::ensure!(n<=20,"每项资源最多 20 个标签");
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
            let unresolved:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM upload_jobs WHERE provider=?1 AND owner_id=?2 AND path=?3 AND state IN ('unknown','running'))",params![provider,owner,text],|r|r.get(0))?;
            anyhow::ensure!(!unresolved,"同一路径存在待核实任务，请先到任务中心处理");
            tx.execute("INSERT INTO upload_jobs(provider,owner_id,path) VALUES(?1,?2,?3)",
                params![provider, owner, text])?;
            jobs.push((tx.last_insert_rowid(), path.clone()));
        }
        tx.commit()?;
        Ok(jobs)
    }

    pub fn jobs(&self, provider: &str, owner: &str, before: Option<i64>) -> Result<Vec<Job>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare("SELECT j.id,j.path,j.state,j.error,p.state,p.error,CASE WHEN r.category='image' THEN r.sha256 END,r.url,r.id FROM upload_jobs j
            LEFT JOIN upload_job_assets a ON a.job_id=j.id LEFT JOIN assets r ON r.id=a.asset_id AND r.provider=j.provider AND r.owner_id=j.owner_id
            LEFT JOIN asset_preview_state p ON p.asset_id=r.id
            WHERE j.provider=?1 AND j.owner_id=?2 AND (?3 IS NULL OR j.id < ?3)
            ORDER BY j.id DESC LIMIT 51")?;
        let rows = stmt.query_map(params![provider, owner, before], |r| Ok(Job {
            id: r.get(0)?, path: PathBuf::from(r.get::<_,String>(1)?),
            state: r.get(2)?, error: r.get(3)?,
            preview:r.get::<_,Option<String>>(4)?.zip(r.get::<_,Option<String>>(5)?),
            thumbnail:r.get::<_,Option<String>>(6)?.map(|sha|crate::preview::cached_path_for(self.data_dir(),provider,owner,&sha)).filter(|p|crate::preview::valid_cache(p)),
            result_url:r.get(7)?,result_asset_id:r.get(8)?,
        }))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn retry_job(&self, provider: &str, owner: &str, id: i64) -> Result<Option<(i64, PathBuf)>> {
        let conn = self.open()?;
        Ok(conn.query_row("SELECT id,path FROM upload_jobs WHERE provider=?1 AND owner_id=?2 AND id=?3
            AND state IN ('pending','paused','failed')", params![provider, owner, id],
            |r| Ok((r.get(0)?, PathBuf::from(r.get::<_,String>(1)?)))).optional()?)
    }

    pub fn recoverable_count(&self,provider:&str,owner:&str)->Result<usize>{
        let count:i64=self.open()?.query_row("SELECT COUNT(*) FROM upload_jobs WHERE provider=?1 AND owner_id=?2 AND state IN ('pending','paused','failed')",params![provider,owner],|r|r.get(0))?;
        Ok(count.try_into()?)
    }

    pub fn recoverable(&self, provider: &str, owner: &str) -> Result<Vec<(i64, PathBuf)>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare("SELECT id,path FROM upload_jobs WHERE provider=?1 AND owner_id=?2
            AND state IN ('pending','paused','failed') ORDER BY id")?;
        let rows = stmt.query_map(params![provider, owner], |r| {
            Ok((r.get(0)?, PathBuf::from(r.get::<_, String>(1)?)))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn job_state(&self, id: i64, state: &str, error: &str) -> Result<()> {
        let changed = self.open()?.execute("UPDATE upload_jobs SET state=?2,error=?3 WHERE id=?1 AND (
            (state IN ('pending','paused','failed') AND ?2 IN ('running','paused','cancelled')) OR
            (state='running' AND ?2 IN ('done','failed','unknown','cancelled'))
        )", params![id, state, error])?;
        anyhow::ensure!(changed == 1, "任务状态转换不允许");
        Ok(())
    }

    pub fn acknowledge_unknown(&self, provider:&str, owner:&str, id:i64) -> Result<()> {
        let changed = self.open()?.execute("UPDATE upload_jobs SET state='failed',error='用户已核实远端，允许手动重试'
            WHERE provider=?1 AND owner_id=?2 AND id=?3 AND state IN ('unknown','running')", params![provider,owner,id])?;
        anyhow::ensure!(changed==1,"任务不属于当前账号或状态已变化");
        Ok(())
    }

    pub fn set_preview_state(&self,id:i64,state:&str,error:&str)->Result<()>{
        self.open()?.execute("INSERT INTO asset_preview_state(asset_id,state,error) VALUES(?1,?2,?3) ON CONFLICT(asset_id) DO UPDATE SET state=excluded.state,error=excluded.error",params![id,state,error])?;Ok(())
    }
    pub fn preview_state(&self,id:i64)->Result<Option<(String,String)>>{
        Ok(self.open()?.query_row("SELECT state,error FROM asset_preview_state WHERE asset_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?)
    }
    pub fn preview_sources(&self,asset:&Asset) -> Result<Vec<PathBuf>> {
        let conn=self.open()?;
        let mut exact=conn.prepare("SELECT path FROM asset_sources WHERE asset_id=?1 ORDER BY rowid DESC LIMIT 32")?;
        let mut paths=exact.query_map([asset.id],|r|Ok(PathBuf::from(r.get::<_,String>(0)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut stmt=conn.prepare("SELECT path FROM upload_jobs WHERE provider=?1 AND owner_id=?2 AND state='done'
            AND (substr(path,-length(?3)-1)='/'||?3 OR substr(path,-length(?3)-1)=char(92)||?3 OR path=?3)
            ORDER BY id DESC LIMIT 4")?;
        let rows=stmt.query_map(params![asset.provider,asset.owner_id,asset.name],|r|Ok(PathBuf::from(r.get::<_,String>(0)?)))?;
        for path in rows{let path=path?;if !paths.contains(&path){paths.push(path);}}
        Ok(paths)
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

    /// Persist a known successful remote result before the asset transaction.
    /// Only a matching running job can accept a receipt; it contains no session credentials.
    pub fn record_receipt(&self,entry:&NewAsset,source:&std::path::Path,job:i64)->Result<()>{
        let path=source.to_str().context("来源路径不是有效 UTF-8")?;
        let n=self.open()?.execute("INSERT INTO upload_receipts(job_id,name,bytes,category,provider,owner_id,url,filekey,sha256,source)
            SELECT id,?2,?3,?4,?5,?6,?7,?8,?9,?10 FROM upload_jobs WHERE id=?1 AND provider=?5 AND owner_id=?6 AND state='running' AND path=?10",
            params![job,entry.name,entry.bytes,entry.category,entry.provider,entry.owner_id,entry.url,entry.filekey,entry.sha256,path])?;
        anyhow::ensure!(n==1,"已返回的上传结果无法保存至对应任务");Ok(())
    }
    fn recover_receipts(&self)->Result<()>{
        let conn=self.open()?;
        let mut stmt=conn.prepare("SELECT r.job_id,r.name,r.bytes,r.category,r.provider,r.owner_id,r.url,r.filekey,r.sha256,r.source
            FROM upload_receipts r JOIN upload_jobs j ON j.id=r.job_id AND j.provider=r.provider AND j.owner_id=r.owner_id AND j.path=r.source
            WHERE j.state='unknown' AND r.provider='yuque' ORDER BY r.job_id")?;
        let receipts=stmt.query_map([],|r|Ok((r.get::<_,i64>(0)?,NewAsset{name:r.get(1)?,bytes:r.get(2)?,category:r.get(3)?,provider:r.get(4)?,owner_id:r.get(5)?,url:r.get(6)?,filekey:r.get(7)?,sha256:r.get(8)?},PathBuf::from(r.get::<_,String>(9)?))))?.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);drop(conn);
        for (job,entry,source) in receipts{
            // No upload and no network. The normal commit transaction validates
            // the matching receipt before accepting an unknown job.
            if self.commit_asset(&entry,Some(&source),Some(job)).is_err(){
                eprintln!("上传回执恢复未完成，任务仍待核实");
            }
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn insert_or_get(&self, entry: &NewAsset) -> Result<Asset> {
        self.commit_asset(entry,None,None)
    }
    pub fn commit_asset(&self,entry:&NewAsset,source:Option<&std::path::Path>,job:Option<i64>)->Result<Asset>{
        let mut conn=self.open()?;let tx=conn.transaction()?;
        tx.execute(
            "INSERT INTO assets(name,bytes,category,provider,owner_id,url,sha256,filekey)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(provider,owner_id,sha256) DO NOTHING",
            params![entry.name, entry.bytes, entry.category, entry.provider,
                    entry.owner_id, entry.url, entry.sha256, entry.filekey],
        )?;
        let asset=tx.query_row("SELECT id,name,bytes,category,provider,url,sha256,owner_id,filekey,created_at FROM assets WHERE provider=?1 AND owner_id=?2 AND sha256=?3",params![entry.provider,entry.owner_id,entry.sha256],map_row)?;
        if let Some(source)=source{
            tx.execute("INSERT OR IGNORE INTO asset_sources(asset_id,path) VALUES(?1,?2)",params![asset.id,source.to_str().context("来源路径不是有效 UTF-8")?])?;
        }
        if let Some(job)=job{
            let n=tx.execute("UPDATE upload_jobs SET state='done',error='' WHERE id=?1 AND provider=?2 AND owner_id=?3 AND (state='running' OR (state='unknown' AND EXISTS(SELECT 1 FROM upload_receipts r WHERE r.job_id=?1 AND r.provider=?2 AND r.owner_id=?3 AND r.sha256=?4 AND r.url=?5 AND r.source=upload_jobs.path)))",params![job,entry.provider,entry.owner_id,entry.sha256,entry.url])?;
            anyhow::ensure!(n==1,"任务与资源账号不一致或状态已变化");
            tx.execute("INSERT INTO upload_job_assets(job_id,asset_id) VALUES(?1,?2) ON CONFLICT(job_id) DO UPDATE SET asset_id=excluded.asset_id",params![job,asset.id])?;
            tx.execute("DELETE FROM upload_receipts WHERE job_id=?1",[job])?;
        }
        tx.commit()?;Ok(asset)
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
    fn album_favorites_similarity_backup_and_large_pagination() {
        let root=std::env::temp_dir().join(format!("yuque-album-storage-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db=Storage::new(root.join("data/assets.db")).unwrap();
        let mut conn=db.open().unwrap();
        let tx=conn.transaction().unwrap();
        for i in 0..10000 {
            tx.execute("INSERT INTO assets(name,bytes,category,provider,owner_id,url,sha256) VALUES(?1,10,'image','yuque','1','https://example.invalid/x',?1)",[format!("image-{i}")]).unwrap();
        }
        tx.execute("INSERT INTO assets(name,bytes,category,provider,owner_id,url,sha256) VALUES('other',10,'image','yuque','2','https://example.invalid/x','other')",[]).unwrap();
        tx.commit().unwrap();
        let mut before=None;
        let mut count=0;
        let mut first=None;
        let mut second=None;
        loop {
            let (items,more,total)=db.page_album(150,"","image",Some(("yuque","1")),before,false,None,false).unwrap();
            assert_eq!(total,10000);
            if first.is_none(){first=Some(items[0].clone());second=Some(items[1].clone());}
            for item in &items{assert!(before.is_none_or(|v|item.id<v));before=Some(item.id);}
            count+=items.len();
            if !more{break;}
        }
        assert_eq!(count,10000);
        let a=first.unwrap();let b=second.unwrap();
        assert!(db.toggle_favorite(&a).unwrap());
        assert!(db.favorite_ids(Some(("yuque","2"))).unwrap().is_empty());
        assert_eq!(db.page_album(150,"","",Some(("yuque","1")),None,false,None,true).unwrap().0[0].id,a.id);
        db.store_visual_hash(a.id,0).unwrap();db.store_visual_hash(b.id,1).unwrap();db.store_visual_hash(10001,0).unwrap();
        assert_eq!(db.similar_assets(&a,0).unwrap().iter().map(|a|a.id).collect::<Vec<_>>(),vec![b.id]);
        let preview=db.data_dir().join("album-previews");
        std::fs::create_dir_all(&preview).unwrap();
        let name=format!("{}.jpg","a".repeat(64));
        std::fs::write(preview.join(&name),b"fixture").unwrap();
        std::fs::write(db.data_dir().join("secret.txt"),b"do not copy").unwrap();
        assert!(db.export_album(db.data_dir()).is_err());
        let backup=db.export_album(&root).unwrap();
        assert!(backup.join("album-previews").join(name).is_file());
        assert!(!backup.join("secret.txt").exists());
        let restored=Storage::new(backup.join("assets.db")).unwrap();
        assert!(restored.favorite_ids(Some(("yuque","1"))).unwrap().contains(&a.id));
        assert_eq!(restored.similar_assets(&a,0).unwrap().len(),1);
        assert!(!db.toggle_favorite(&a).unwrap());
        drop(restored);drop(conn);drop(db);
        let _=std::fs::remove_dir_all(root);
    }

    #[test]
    fn similarity_index_pages_and_bounded_rank_are_account_scoped(){
        let root=std::env::temp_dir().join(format!("yuque-similarity-index-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db=Storage::new(root.join("assets.db")).unwrap();
        let mut conn=db.open().unwrap();
        let tx=conn.transaction().unwrap();
        for i in 1..=500{
            tx.execute("INSERT INTO assets(name,bytes,category,provider,owner_id,url,sha256) VALUES(?1,10,'image','yuque','1','https://example.invalid/x',?1)",[format!("image-{i}")]).unwrap();
        }
        tx.execute("INSERT INTO assets(name,bytes,category,provider,owner_id,url,sha256) VALUES('other',10,'image','yuque','2','https://example.invalid/x','other')",[]).unwrap();
        tx.execute("INSERT INTO asset_visual(asset_id,hash) SELECT id, CASE WHEN id <= 50 THEN 0 ELSE 1 END FROM assets WHERE id<=200 OR id=501",[]).unwrap();
        tx.commit().unwrap();
        let a=db.page_album(1,"","image",Some(("yuque","1")),None,true,None,false).unwrap().0.remove(0);
        let first=db.unindexed_images(&a,None).unwrap();
        assert_eq!(first.len(),150);assert_eq!(first[0].id,500);assert_eq!(first[149].id,351);
        let second=db.unindexed_images(&a,Some(first[149].id)).unwrap();
        assert_eq!(second.len(),150);assert_eq!(second[149].id,201);
        assert!(db.unindexed_images(&a,Some(201)).unwrap().is_empty());
        let ranked=db.similar_assets(&a,0).unwrap();
        assert_eq!(ranked.len(),150);
        let expected=(2..=50).rev().chain((100..=200).rev()).collect::<Vec<i64>>();
        assert_eq!(ranked.iter().map(|a|a.id).collect::<Vec<_>>(),expected);
        let checks=std::cell::Cell::new(0);
        let result=db.similar_assets_until(&a,0,||{checks.set(checks.get()+1);checks.get()>5});
        assert!(result.is_err());assert_eq!(checks.get(),6);
        drop(conn);drop(db);let _=std::fs::remove_dir_all(root);
    }

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
        assert!(db.job_state(jobs[0].0, "done", "").is_err());
        db.job_state(jobs[0].0, "running", "").unwrap();
        db.job_state(jobs[0].0, "done", "").unwrap();
        db.job_state(jobs[1].0, "running", "").unwrap();
        db.save_account("1").unwrap();
        drop(db);
        let db = Storage::new(path).unwrap();
        assert!(db.recoverable("yuque", "1").unwrap().is_empty());
        assert!(db.recoverable("yuque", "2").unwrap().is_empty());
        assert_eq!(db.jobs("yuque", "1", None).unwrap()[0].state, "unknown");
        assert!(db.retry_job("yuque", "1", jobs[1].0).unwrap().is_none());
        assert!(db.enqueue("yuque","1",&[PathBuf::from("b.png")]).is_err());
        assert!(db.acknowledge_unknown("yuque","2",jobs[1].0).is_err());
        db.acknowledge_unknown("yuque","1",jobs[1].0).unwrap();
        db.job_state(jobs[1].0, "running", "").unwrap();
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
        db.batch_tags(&ids[..2],Some(("demo","local")),"设计/UI",false).unwrap();
        assert_eq!(db.page_tagged(10,"","",Some(("demo","local")),None,false,Some("设计")).unwrap().0.len(),2);
        assert!(db.batch_tags(&ids[..2],Some(("yuque","1")),"other",false).is_err());
        assert!(db.tag_catalog(Some(("yuque","1"))).unwrap().is_empty());
        db.batch_tags(&ids[..2],Some(("demo","local")),"设计/UI",true).unwrap();
        assert_eq!(db.tag_catalog(None).unwrap(),vec!["摄影".to_string(),"设计".to_string()]);
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
