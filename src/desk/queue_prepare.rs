//! Blocking preparation operations; never execute these on the UI thread.
use super::*;
pub(super) enum Source {
    Path(PathBuf),
    Image(Vec<u8>,String),
}
pub(super) enum Prepare {
    Enqueue(Vec<Source>),
    Retry(i64),
    Resume,
    Acknowledge(i64),
}
pub(super) enum Prepared {
    Jobs(Vec<(i64,PathBuf)>),
    Acknowledged,
}
impl Prepare {
    pub fn execute(self,storage:&Storage,provider:&str,owner:&str)->Result<Prepared>{
        match self {
            Self::Retry(id)=>Ok(Prepared::Jobs(storage.retry_job(provider,owner,id)?.into_iter().collect())),
            Self::Resume=>Ok(Prepared::Jobs(storage.recoverable(provider,owner)?)),
            Self::Acknowledge(id)=>{storage.acknowledge_unknown(provider,owner,id)?;Ok(Prepared::Acknowledged)}
            Self::Enqueue(sources)=>{
                let mut paths=Vec::new();
                for source in sources{
                    match source{
                        Source::Path(path)=>paths.push(path),
                        Source::Image(bytes,ext)=>paths.push(stage_image(storage,&bytes,&ext)?),
                    }
                }
                Ok(Prepared::Jobs(storage.enqueue(provider,owner,&paths)?))
            }
        }
    }
}
fn stage_image(storage:&Storage,bytes:&[u8],ext:&str)->Result<PathBuf>{
    use sha2::{Digest,Sha256};
    use std::io::Write;
    anyhow::ensure!(ext.bytes().all(|b|b.is_ascii_alphanumeric()),"剪贴板图片格式无效");
    let dir=storage.data_dir().join("clipboard");
    std::fs::create_dir_all(&dir)?;
    let hash=hex::encode(Sha256::digest(bytes));
    let path=dir.join(format!("{hash}.{ext}"));
    // Never overwrite an existing source, including a symlink.
    match std::fs::symlink_metadata(&path){
        Ok(meta)=>{
            anyhow::ensure!(meta.file_type().is_file()&&meta.len()==bytes.len() as u64,"剪贴板暂存文件异常");
            let existing=std::fs::read(&path)?;
            anyhow::ensure!(Sha256::digest(&existing).as_slice()==Sha256::digest(bytes).as_slice(),"剪贴板暂存文件校验失败");
            return Ok(path);
        }
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},
        Err(e)=>return Err(e.into()),
    }
    let temp=path.with_extension(format!("{}.tmp",std::process::id()));
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
    let result=(||->Result<()>{
        file.write_all(bytes)?;file.sync_all()?;drop(file);
        // Publish without overwriting a concurrently created destination.
        std::fs::hard_link(&temp,&path)?;
        Ok(())
    })();
    let _=std::fs::remove_file(&temp);
    result?;
    Ok(path)
}

#[cfg(test)]
#[::core::prelude::v1::test]
fn clipboard_reuses_only_verified_file_and_never_replaces_existing(){
    let root=std::env::temp_dir().join(format!("yuque-stage-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let path=stage_image(&db,b"test-image","png").unwrap();
    assert_eq!(stage_image(&db,b"test-image","png").unwrap(),path);
    std::fs::write(&path,b"bad--image").unwrap();
    assert!(stage_image(&db,b"test-image","png").is_err());
    assert_eq!(std::fs::read(&path).unwrap(),b"bad--image");
    drop(db);let _=std::fs::remove_dir_all(root);
}
