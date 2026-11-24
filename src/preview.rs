//! Persistent, local-only image contact sheet. Never requests an asset URL.
use crate::storage::Asset;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{io::{Cursor, Read}, path::{Path, PathBuf}};

const MAX_INPUT: u64 = 32 * 1024 * 1024;
const EDGE: u32 = 160;
static CACHE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn cached_path(root: &Path, asset: &Asset) -> PathBuf {
    let key = format!("v2:{}:{}:{}", asset.provider, asset.owner_id, asset.sha256);
    root.join("album-previews").join(format!("{}.jpg", hex::encode(Sha256::digest(key.as_bytes()))))
}

fn legacy_path(root:&Path, asset:&Asset) -> PathBuf {
    let key=format!("v1:{}:{}:{}:{}",asset.provider,asset.owner_id,asset.sha256,asset.url);
    root.join("thumbnails").join(format!("{}.png",hex::encode(Sha256::digest(key.as_bytes()))))
}

fn read_bounded(path:&Path) -> Result<Vec<u8>> {
    let mut bytes=Vec::new();
    std::fs::File::open(path)?.take(MAX_INPUT+1).read_to_end(&mut bytes)?;
    ensure!(!bytes.is_empty() && bytes.len() as u64<=MAX_INPUT,"图片超过本地预览输入限制");
    Ok(bytes)
}

fn write_small(dest:&Path,bytes:Vec<u8>) -> Result<PathBuf> {
    let mut reader=image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(8192);
    limits.max_image_height=Some(8192);
    limits.max_alloc=Some(96*1024*1024);
    reader.limits(limits);
    let decoded=reader.decode().context("图片格式不支持或超出预览限制")?;
    let small=if decoded.width()>EDGE || decoded.height()>EDGE {decoded.thumbnail(EDGE,EDGE)}else{decoded};
    // JPEG is intentionally lossy; composite transparency onto a neutral light background.
    let rgba=small.to_rgba8();
    let rgb=image::RgbImage::from_fn(rgba.width(),rgba.height(),|x,y|{
        let p=rgba.get_pixel(x,y).0;
        let a=p[3] as u16;
        image::Rgb(std::array::from_fn(|i|((p[i] as u16*a+240*(255-a))/255) as u8))
    });
    std::fs::create_dir_all(dest.parent().context("预览目录无效")?)?;
    let tmp=dest.with_extension(format!("{}.{}.tmp",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()));
    let result=(|| -> Result<()> {
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file,45).encode_image(&rgb)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp,dest)?;
        Ok(())
    })();
    if result.is_err(){let _=std::fs::remove_file(&tmp);}
    result?;
    Ok(dest.to_owned())
}

/// Called with the actual upload source before it is discarded. A preview failure
/// never changes the successful upload result or the recorded remote URL.
pub fn persist_source(root:&Path,asset:&Asset,source:&Path) -> Result<PathBuf> {
    let _guard=CACHE_LOCK.lock().map_err(|_|anyhow::anyhow!("预览锁异常"))?;
    ensure!(asset.category=="image","非图片资源");
    let dest=cached_path(root,asset);
    if dest.is_file(){return Ok(dest);}
    let bytes=read_bounded(source)?;
    ensure!(hex::encode(Sha256::digest(&bytes))==asset.sha256,"本地文件内容已变化");
    write_small(&dest,bytes)
}

/// Existing rows: reuse an old local preview, or a verified Demo source.
/// A missing remote thumbnail stays missing; no HTTP fallback.
pub fn thumbnail(root:&Path,asset:&Asset) -> Result<PathBuf> {
    ensure!(asset.category=="image","非图片资源");
    let dest=cached_path(root,asset);
    if dest.is_file(){return Ok(dest);}
    {
        let _guard=CACHE_LOCK.lock().map_err(|_|anyhow::anyhow!("预览锁异常"))?;
        if dest.is_file(){return Ok(dest);}
        let legacy=legacy_path(root,asset);
        let large=legacy.with_file_name(format!("large-{}",legacy.file_name().unwrap_or_default().to_string_lossy()));
        for source in [legacy,large] {
            if source.is_file() {
                if let Ok(bytes)=read_bounded(&source) {
                    if let Ok(path)=write_small(&dest,bytes){return Ok(path);}
                }
            }
        }
    }
    if asset.provider=="demo" {
        let url=url::Url::parse(&asset.url)?;
        if url.scheme()=="file" {
            let path=url.to_file_path().map_err(|_|anyhow::anyhow!("无效本地路径"))?;
            return persist_source(root,asset,&path);
        }
    }
    anyhow::bail!("没有本地缩略图；仍可复制原始链接")
}

pub fn visual_hash(path:&Path) -> Result<(u64,f32)> {
    let bytes=read_bounded(path)?;
    let mut reader=image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(160);limits.max_image_height=Some(160);limits.max_alloc=Some(1024*1024);
    reader.limits(limits);
    let image=reader.decode()?;
    let ratio=image.width() as f32/image.height().max(1) as f32;
    let gray=image.resize_exact(9,8,image::imageops::FilterType::Triangle).to_luma8();
    let mut hash=0u64;
    for y in 0..8 {for x in 0..8 {hash=(hash<<1)|u64::from(gray.get_pixel(x,y)[0]>gray.get_pixel(x+1,y)[0]);}}
    Ok((hash,ratio))
}

pub fn index_local(storage:&crate::storage::Storage,asset:&Asset) -> Result<(PathBuf,f32)> {
    let path=restore_local(storage,asset)?;
    if storage.visual_hash(asset.id)?.is_some(){
        let (w,h)=image::image_dimensions(&path)?;
        return Ok((path,w as f32/h.max(1) as f32));
    }
    let (hash,ratio)=visual_hash(&path)?;
    storage.store_visual_hash(asset.id,hash)?;
    Ok((path,ratio))
}

pub fn restore_local(storage:&crate::storage::Storage,asset:&Asset) -> Result<PathBuf> {
    if let Ok(path)=thumbnail(storage.data_dir(),asset){return Ok(path);}
    for source in storage.preview_sources(asset)? {
        if let Ok(path)=persist_source(storage.data_dir(),asset,&source){return Ok(path);}
    }
    anyhow::bail!("没有本地缩略图；仍可复制原始链接")
}

// Only the old transient cache is cleared; persistent album thumbnails are not evicted.
#[cfg(test)]
pub fn clear_cache(root:&Path) -> Result<usize> {
    let _guard=CACHE_LOCK.lock().map_err(|_|anyhow::anyhow!("预览锁异常"))?;
    let cache=root.join("thumbnails");
    if !cache.exists(){return Ok(0);}
    let mut removed=0;
    for entry in std::fs::read_dir(cache)? {
        let entry=entry?;
        let name=entry.file_name();
        let Some(name)=name.to_str() else{continue;};
        let Some(stem)=name.strip_suffix(".png") else{continue;};
        let stem=stem.strip_prefix("large-").unwrap_or(stem);
        if stem.len()==64 && stem.bytes().all(|b|b.is_ascii_hexdigit()) && entry.file_type()?.is_file(){
            std::fs::remove_file(entry.path())?;removed+=1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upload_persists_thumbnail_and_restores_prior_remote_source() {
        let root=std::env::temp_dir().join(format!("yuque-album-upload-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db=crate::storage::Storage::new(root.join("assets.db")).unwrap();
        let source=root.join("photo.png");
        image::RgbImage::new(640,480).save(&source).unwrap();
        let config=crate::config::Config{mode:crate::config::Mode::Demo,creds:None,max_file_mb:1};
        let result=crate::uploader::upload(&source,&config,&db).unwrap();
        assert!(cached_path(&root,&result.item).is_file());
        let remote=db.insert_or_get(&crate::storage::NewAsset{name:"photo.png".into(),bytes:result.item.bytes,category:"image".into(),provider:"yuque".into(),owner_id:"1".into(),url:"https://example.invalid/photo.png".into(),sha256:result.item.sha256.clone(),filekey:"".into()}).unwrap();
        assert!(restore_local(&db,&remote).is_err());
        let jobs=db.enqueue("yuque","1",&[source.clone()]).unwrap();
        db.job_state(jobs[0].0,"running","").unwrap();
        db.job_state(jobs[0].0,"done","").unwrap();
        let preview=restore_local(&db,&remote).unwrap();
        assert_eq!(image::image_dimensions(&preview).unwrap(),(160,120));
        std::fs::remove_file(&source).unwrap();
        drop(db);
        let db=crate::storage::Storage::new(root.join("assets.db")).unwrap();
        assert_eq!(restore_local(&db,&remote).unwrap(),preview);
        assert!(restore_local(&db,&result.item).is_ok());
        let _=std::fs::remove_dir_all(root);
    }
    #[test]
    fn local_preview_survives_source_removal_and_legacy_cleanup() {
        let root=std::env::temp_dir().join(format!("yuque-album-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&root).unwrap();
        let source=root.join("source.png");
        image::RgbImage::from_fn(640,480,|x,y|image::Rgb([(x%256) as u8,(y%256) as u8,90])).save(&source).unwrap();
        let bytes=std::fs::read(&source).unwrap();
        let asset=Asset{id:1,name:"source.png".into(),bytes:bytes.len() as i64,category:"image".into(),provider:"yuque".into(),owner_id:"1".into(),url:"https://example.invalid/a.png".into(),sha256:hex::encode(Sha256::digest(&bytes)),filekey:"".into(),created_at:"".into()};
        assert!(thumbnail(&root,&asset).is_err()); // no remote fallback
        let thumb=persist_source(&root,&asset,&source).unwrap();
        assert_eq!(image::image_dimensions(&thumb).unwrap(),(160,120));
        assert!(std::fs::metadata(&thumb).unwrap().len()<30*1024);
        std::fs::remove_file(&source).unwrap();
        assert_eq!(thumbnail(&root,&asset).unwrap(),thumb);
        let mut other=asset.clone();other.owner_id="2".into();
        assert_ne!(cached_path(&root,&other),thumb);
        assert!(thumbnail(&root,&other).is_err());
        let old=legacy_path(&root,&other);
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        image::RgbImage::new(320,240).save(&old).unwrap();
        let migrated=thumbnail(&root,&other).unwrap();
        assert_eq!(image::image_dimensions(&migrated).unwrap(),(160,120));
        assert_eq!(clear_cache(&root).unwrap(),1);
        assert!(thumb.exists()&&migrated.exists());
        let _=std::fs::remove_dir_all(root);
    }
}
