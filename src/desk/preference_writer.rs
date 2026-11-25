//! Serialize settings snapshots across the background worker and final shutdown.
//! A delayed older snapshot must never overwrite a newer committed one.
use super::*;
#[derive(Clone,Default)]
pub(super) struct PreferenceWriter(Arc<std::sync::Mutex<Option<u64>>>);
impl PreferenceWriter{
    pub fn write(&self,storage:&Storage,revision:u64,value:&serde_json::Value)->Result<()>{
        let mut last=self.0.lock().map_err(|_|anyhow::anyhow!("设置保存锁异常"))?;
        if last.is_some_and(|n|n>=revision){return Ok(());}
        storage.save_preferences(value)?;
        *last=Some(revision);
        Ok(())
    }
}
#[cfg(test)]
#[::core::prelude::v1::test]
fn older_snapshot_cannot_overwrite_final_snapshot(){
    let root=std::env::temp_dir().join(format!("yuque-pref-order-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let writer=PreferenceWriter::default();
    writer.write(&db,2,&serde_json::json!({"tile":200})).unwrap();
    writer.write(&db,1,&serde_json::json!({"tile":100})).unwrap();
    assert_eq!(db.preferences().unwrap()["tile"],200);
    drop(db);let _=std::fs::remove_dir_all(root);
}
