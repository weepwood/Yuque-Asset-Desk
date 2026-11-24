//! Only application-owned files; never recursively delete a data directory.
use anyhow::{Context,Result,ensure};
use std::{collections::HashSet,fs,path::{Path,PathBuf},io::{Read,Write},time::{Duration,Instant}};
use sha2::{Digest,Sha256};
use crate::storage::Storage;

pub const REQUEST:&str=".reset-request";
const DB_FILES:[&str;4]=["assets.db","assets.db-wal","assets.db-shm","assets.db-journal"];
const DIRS:[&str;3]=["album-previews","thumbnails","clipboard"];
#[derive(Clone,Debug,Default)]
pub struct Usage{
    pub database:u64,pub previews:u64,pub legacy:u64,pub clipboard:u64,pub reclaimable:u64,pub candidates:usize,pub unfinished:u64,pub accounts:usize,
}
#[derive(Debug,Default)]
pub struct Cleanup{pub count:usize,pub bytes:u64,pub failed:usize}
fn hash_name(s:&str)->bool{s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit())}
fn ordinary(m:&fs::Metadata)->bool{
    if m.file_type().is_symlink(){return false;}
    #[cfg(windows)] {use std::os::windows::fs::MetadataExt;if m.file_attributes()&0x400!=0{return false;}}
    true
}
fn meta(path:&Path)->Result<Option<fs::Metadata>>{
    match fs::symlink_metadata(path){Ok(m)=>Ok(Some(m)),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(None),Err(e)=>Err(e.into())}
}
fn root(storage:&Storage)->Result<PathBuf>{
    let path=storage.data_dir().canonicalize()?;
    ensure!(path.parent().is_some(),"不能操作文件系统根目录");
    Ok(path)
}
fn entries(root:&Path,folder:&str)->Result<Vec<PathBuf>>{
    let dir=root.join(folder);
    let Some(m)=meta(&dir)? else{return Ok(vec![])};
    ensure!(ordinary(&m)&&m.is_dir(),"数据子目录不是普通目录：{folder}");
    let mut out=vec![];
    for item in fs::read_dir(dir)?{
        let p=item?.path();
        if let Some(m)=meta(&p)?{if ordinary(&m)&&m.is_file(){out.push(p);}}
    }
    Ok(out)
}
fn file_bytes(p:&Path)->Result<u64>{
    Ok(meta(p)?.filter(|m|ordinary(m)&&m.is_file()).map_or(0,|m|m.len()))
}
fn recognized(folder:&str,p:&Path)->bool{
    let Some(name)=p.file_name().and_then(|s|s.to_str())else{return false};
    match folder{
        "album-previews"=>name.strip_suffix(".jpg").is_some_and(hash_name),
        "thumbnails"=>name.strip_suffix(".png").is_some_and(|s|hash_name(s.strip_prefix("large-").unwrap_or(s))),
        "clipboard"=>{
            let Some((stem,ext))=name.split_once('.')else{return false};
            hash_name(stem)&&(matches!(ext,"png"|"jpg"|"jpeg"|"webp"|"gif"|"svg"|"bmp"|"tiff"|"tif"|"ico"|"pnm"|"ppm"|"pgm"|"pbm")||
                ext.strip_suffix(".tmp").is_some_and(|pid|!pid.is_empty()&&pid.bytes().all(|b|b.is_ascii_digit())))
        }
        _=>false
    }
}
fn clipboard_owned(p:&Path)->Result<bool>{
    if !recognized("clipboard",p){return Ok(false);}
    let name=p.file_name().and_then(|s|s.to_str()).unwrap_or_default();
    let expected=name.split('.').next().unwrap_or_default();
    let mut file=fs::File::open(p)?;let mut hash=Sha256::new();let mut buf=[0u8;65536];
    loop{let n=file.read(&mut buf)?;if n==0{break;}hash.update(&buf[..n]);}
    Ok(hex::encode(hash.finalize()).eq_ignore_ascii_case(expected))
}
fn path_key(p:&Path)->String{
    let key=p.to_string_lossy().into_owned();
    if cfg!(windows){key.to_lowercase()}else{key}
}
fn candidates(storage:&Storage,root:&Path)->Result<Vec<PathBuf>>{
    let mut refs=HashSet::new();
    let mut names=HashSet::new();
    for p in storage.local_references()?{
        // Conservative protection also covers historical relative paths, case
        // variants, and references whose source is temporarily unavailable.
        if let Some(name)=p.file_name().and_then(|s|s.to_str()){names.insert(name.to_ascii_lowercase());}
        match p.canonicalize(){
            Ok(p)=>{refs.insert(path_key(&p));}
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},
            Err(e)=>return Err(e).context("无法确认本地引用，已停止清理"),
        }
    }
    let mut result=vec![];
    for p in entries(root,"clipboard")?{
        let name=p.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
        if !names.contains(&name)&&!refs.contains(&path_key(&p.canonicalize()?))&&clipboard_owned(&p)?{result.push(p);}
    }
    Ok(result)
}
pub fn usage(storage:&Storage)->Result<Usage>{
    let root=root(storage)?;
    let mut u=Usage::default();
    for name in DB_FILES{u.database=u.database.saturating_add(file_bytes(&root.join(name))?);}
    for (folder,total) in [("album-previews",&mut u.previews),("thumbnails",&mut u.legacy),("clipboard",&mut u.clipboard)]{
        for p in entries(&root,folder)?{*total=total.saturating_add(file_bytes(&p)?);}
    }
    for p in candidates(storage,&root)?{u.candidates+=1;u.reclaimable=u.reclaimable.saturating_add(file_bytes(&p)?);}
    u.unfinished=storage.unfinished_count()?;
    u.accounts=storage.accounts()?.len();
    Ok(u)
}
pub fn clean(storage:&Storage)->Result<Cleanup>{
    let root=root(storage)?;
    let mut r=Cleanup::default();
    for p in candidates(storage,&root)?{
        let n=file_bytes(&p)?;
        match fs::remove_file(&p){Ok(())=>{r.count+=1;r.bytes=r.bytes.saturating_add(n);},Err(_)=>r.failed+=1}
    }
    Ok(r)
}
/// Revalidate before any deletion. Unknown files, reparse points and folders are not removed.
pub fn reset_owned(storage:&Storage)->Result<Cleanup>{
    ensure!(storage.unfinished_count()?==0,"存在未完成或待核实任务，不能重置");
    ensure!(storage.accounts()?.is_empty(),"仍有保存的账号凭据索引，不能重置");
    let root=root(storage)?;
    let mut paths=vec![];
    for folder in DIRS{
        for p in entries(&root,folder)?{
            if recognized(folder,&p)&&(folder!="clipboard"||clipboard_owned(&p)?){paths.push(p);}
        }
    }
    for name in DB_FILES{
        let p=root.join(name);
        if let Some(m)=meta(&p)?{ensure!(ordinary(&m)&&m.is_file(),"数据库文件不是普通文件，停止重置");paths.push(p);}
    }
    // Delete DB last so a partial cleanup keeps the original records.
    let mut r=Cleanup::default();
    for p in paths{
        let n=file_bytes(&p)?;
        if let Err(e)=fs::remove_file(&p){return Err(e).context(format!("已删除 {} 项；清理中止，剩余数据仍保留",r.count));}
        r.count+=1;r.bytes=r.bytes.saturating_add(n);
    }
    for folder in DIRS{let _=fs::remove_dir(root.join(folder));}
    Ok(r)
}
pub fn schedule_reset(storage:&Storage)->Result<()>{
    ensure!(storage.unfinished_count()?==0&&storage.accounts()?.is_empty(),"仍有未完成任务或账号索引");
    let root=root(storage)?;
    let nonce=format!("{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos());
    let exe=std::env::current_exe()?;
    let marker=root.join(REQUEST);
    let mut f=fs::OpenOptions::new().write(true).create_new(true).open(&marker)?;
    if let Err(e)=f.write_all(nonce.as_bytes()).and_then(|_|f.sync_all()){drop(f);let _=fs::remove_file(&marker);return Err(e.into());}
    drop(f);
    let child=std::process::Command::new(exe).arg("--reset-after-exit").arg(&nonce)
        .env("YUQUE_DATA_DIR",&root).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    if let Err(e)=child{let _=fs::remove_file(marker);return Err(e).context("无法启动退出清理进程");}
    Ok(())
}
pub fn reset_helper(path:PathBuf,nonce:&str)->Result<()>{
    let root=path.parent().context("缺少数据目录")?.canonicalize()?;
    let marker=root.join(REQUEST);
    ensure!(meta(&marker)?.is_some_and(|m|ordinary(&m)&&m.is_file()&&m.len()<128),"没有有效的重置请求");
    ensure!(fs::read_to_string(&marker)?==nonce,"重置请求不匹配");
    let result=(||->Result<Cleanup>{
        let lock=root.join("instance.lock");
        ensure!(meta(&lock)?.is_some_and(|m|ordinary(&m)&&m.is_file()),"实例锁不合法");
        let file=fs::OpenOptions::new().read(true).write(true).open(lock)?;
        let start=Instant::now();
        loop{
            if file.try_lock().is_ok(){break;}
            ensure!(start.elapsed()<Duration::from_secs(20),"应用未退出，取消重置");
            std::thread::sleep(Duration::from_millis(100));
        }
        ensure!(fs::read_to_string(&marker)?==nonce,"重置请求已取消");
        for name in DB_FILES{
            if let Some(m)=meta(&root.join(name))?{ensure!(ordinary(&m)&&m.is_file(),"数据库文件不是普通文件");}
        }
        let result=reset_owned(&Storage::existing(path)?);
        if result.is_ok(){fs::remove_file(&marker)?;}
        // Lock remains held until the marker has been removed.
        result
    })();
    match result{
        Ok(_)=>Ok(()),
        Err(e)=>{
            // Preserve a report without touching any unrelated existing file.
            if let Ok(mut f)=fs::OpenOptions::new().write(true).create_new(true).open(root.join("reset-error.txt")){let _=writeln!(f,"{e:#}");}
            if fs::read_to_string(&marker).ok().as_deref()==Some(nonce){let _=fs::remove_file(&marker);}
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn fixture()->(PathBuf,Storage){
        static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
        let p=std::env::temp_dir().join(format!("yuque-clean-{}-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db=Storage::new(p.join("assets.db")).unwrap();(p,db)
    }
    fn clip(root:&Path,b:&[u8])->PathBuf{
        let dir=root.join("clipboard");fs::create_dir_all(&dir).unwrap();
        let p=dir.join(format!("{}.png",hex::encode(Sha256::digest(b))));fs::write(&p,b).unwrap();p
    }
    #[test]
    fn cleanup_respects_cross_account_refs_demo_links_and_unknown_files(){
        let (root,db)=fixture();
        let orphan=clip(&root,b"orphan");let job=clip(&root,b"job");let demo=clip(&root,b"demo");
        let original=root.join("clipboard/original.png");fs::write(&original,b"user file").unwrap();
        let preview=root.join("album-previews");fs::create_dir(&preview).unwrap();fs::write(preview.join(format!("{}.jpg","a".repeat(64))),b"preview").unwrap();
        db.enqueue("yuque","123",&[job.clone()]).unwrap();
        db.insert_or_get(&crate::storage::NewAsset{name:"demo.png".into(),bytes:4,category:"image".into(),provider:"demo".into(),owner_id:"local".into(),url:url::Url::from_file_path(&demo).unwrap().to_string(),sha256:"demo".into(),filekey:String::new()}).unwrap();
        let u=usage(&db).unwrap();assert_eq!((u.candidates,u.reclaimable,u.unfinished),(1,6,1));
        let r=clean(&db).unwrap();assert_eq!((r.count,r.bytes,r.failed),(1,6,0));
        assert!(!orphan.exists()&&job.exists()&&demo.exists()&&original.exists()&&preview.exists());
        assert!(reset_owned(&db).is_err());assert!(root.join("assets.db").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn reset_blocks_accounts_and_leaves_unknown_and_external_files(){
        let (root,db)=fixture();let clipboard=clip(&root,b"temporary");
        let external=root.join("original.jpg");fs::write(&external,b"original").unwrap();
        db.save_account("123").unwrap();assert!(reset_owned(&db).is_err());
        db.forget_account("123").unwrap();
        let r=reset_owned(&db).unwrap();assert!(r.count>=2);
        assert!(!clipboard.exists()&&!root.join("assets.db").exists()&&external.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn linked_directory_is_rejected_before_deletion(){
        let (root,db)=fixture();let external=root.join("originals");fs::create_dir(&external).unwrap();
        let file=external.join(format!("{}.png","a".repeat(64)));fs::write(&file,b"external").unwrap();
        std::os::unix::fs::symlink(&external,root.join("clipboard")).unwrap();
        assert!(clean(&db).is_err());assert!(reset_owned(&db).is_err());assert!(file.exists()&&root.join("assets.db").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
