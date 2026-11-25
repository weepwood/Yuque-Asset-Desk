//! Persistent, local-only image contact sheet. Never requests an asset URL.
use crate::storage::Asset;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{io::{Cursor, Read}, path::{Path, PathBuf}};

const MAX_INPUT: u64 = 32 * 1024 * 1024;
const EDGE: u32 = 160;
static CACHE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn cached_path(root: &Path, asset: &Asset) -> PathBuf {
    cached_path_for(root,&asset.provider,&asset.owner_id,&asset.sha256)
}
pub(crate) fn cached_path_for(root:&Path,provider:&str,owner:&str,sha:&str)->PathBuf{
    let key = format!("v2:{provider}:{owner}:{sha}");
    root.join("album-previews").join(format!("{}.jpg", hex::encode(Sha256::digest(key.as_bytes()))))
}

fn legacy_path(root:&Path, asset:&Asset) -> PathBuf {
    let key=format!("v1:{}:{}:{}:{}",asset.provider,asset.owner_id,asset.sha256,asset.url);
    root.join("thumbnails").join(format!("{}.png",hex::encode(Sha256::digest(key.as_bytes()))))
}

fn read_bounded(path:&Path) -> Result<Vec<u8>> {
    let meta=std::fs::symlink_metadata(path)?;
    ensure!(meta.file_type().is_file(),"预览来源不是普通文件");
    ensure!(meta.len()>0&&meta.len()<=MAX_INPUT,"图片超过本地预览输入限制或为空");
    let mut bytes=Vec::new();
    std::fs::File::open(path)?.take(MAX_INPUT+1).read_to_end(&mut bytes)?;
    ensure!(!bytes.is_empty() && bytes.len() as u64<=MAX_INPUT,"图片超过本地预览输入限制");
    Ok(bytes)
}

pub(crate) fn valid_cache(path:&Path)->bool{
    if !std::fs::symlink_metadata(path).is_ok_and(|m|m.file_type().is_file()&&m.len()>0&&m.len()<=1024*1024){return false;}
    visual_hash(path).is_ok()
}
pub(crate) fn is_svg(bytes:&[u8])->bool{
    let Ok(text)=std::str::from_utf8(bytes)else{return false;};
    let text=text.trim_start_matches(char::from_u32(0xfeff).unwrap()).trim_start();
    text.starts_with("<svg")||((text.starts_with("<?xml")||text.starts_with("<!--"))&&text.contains("<svg"))
}
/// Recognize explicit ISO BMFF image brands, not generic MP4/mif1 containers.
pub(crate) fn modern_image_mime(bytes:&[u8])->Option<&'static str>{
    if bytes.len()<16||&bytes[4..8]!=b"ftyp"{return None;}
    let size=u32::from_be_bytes(bytes[..4].try_into().ok()?) as usize;
    if size<16||size>bytes.len(){return None;}
    let brands=std::iter::once(&bytes[8..12]).chain(bytes[16..size].chunks_exact(4));
    let mut heic=false;
    for brand in brands{
        if matches!(brand,b"avif"|b"avis"){return Some("image/avif");}
        if matches!(brand,b"heic"|b"heix"|b"hevc"|b"hevx"|b"heim"|b"heis"|b"hevm"|b"hevs"){heic=true;}
    }
    heic.then_some("image/heic")
}
pub(crate) fn unsupported_modern_format(bytes:&[u8])->Result<()>{
    match modern_image_mime(bytes){
        Some("image/avif")=>anyhow::bail!("已识别 AVIF；当前版本未集成跨平台 AVIF 解码器，可尝试在线预览或转换为 PNG/JPEG"),
        Some("image/heic")=>anyhow::bail!("已识别 HEIC；当前版本未集成跨平台 HEIC 解码器，可尝试在线预览或转换为 PNG/JPEG"),
        _=>Ok(())
    }
}
pub(crate) fn raster_svg(bytes:&[u8],edge:u32)->Result<(image::DynamicImage,u32,u32)>{
    ensure!(bytes.len()<=1024*1024,"SVG 超过 1 MiB 安全限制");
    let text=std::str::from_utf8(bytes)?;
    ensure!(!text.contains("<!DOCTYPE")&&!text.contains("<!ENTITY"),"SVG 不允许外部实体");
    let mut options=resvg::usvg::Options::default();
    options.image_href_resolver.resolve_string=Box::new(|_,_|None);
    options.image_href_resolver.resolve_data=Box::new(|_,_,_|None);
    let tree=resvg::usvg::Tree::from_data(bytes,&options).context("SVG 解析失败")?;
    let w=tree.size().width();let h=tree.size().height();
    ensure!(w.is_finite()&&h.is_finite()&&w>0.&&h>0.&&w<=100_000.&&h<=100_000.,"SVG 尺寸不安全");
    let scale=(edge as f32/w.max(h)).min(1.);
    let rw=(w*scale).ceil().max(1.) as u32;let rh=(h*scale).ceil().max(1.) as u32;
    let mut pixmap=resvg::tiny_skia::Pixmap::new(rw,rh).context("SVG 预览分配失败")?;
    resvg::render(&tree,resvg::tiny_skia::Transform::from_scale(scale,scale),&mut pixmap.as_mut());
    let pixels=pixmap.pixels().iter().map(|p|p.demultiply());
    let rgba=image::RgbaImage::from_raw(rw,rh,pixels.flat_map(|p|[p.red(),p.green(),p.blue(),p.alpha()]).collect()).context("SVG 像素格式无效")?;
    Ok((image::DynamicImage::ImageRgba8(rgba),w.ceil() as u32,h.ceil() as u32))
}
/// Shared bounded decode path for draft, offline thumbnail and quality viewer.
/// Serializes expensive decodes across lanes without holding the upload worker.
static DECODE_LOCK:std::sync::Mutex<()>=std::sync::Mutex::new(());
pub(crate) fn decode_bounded(bytes:Vec<u8>,edge:u32)->Result<(image::DynamicImage,u32,u32)>{
    use image::ImageDecoder;
    ensure!(!bytes.is_empty()&&bytes.len() as u64<=MAX_INPUT,"图片超过预览输入限制");
    let _guard=DECODE_LOCK.lock().map_err(|_|anyhow::anyhow!("图片解码锁异常"))?;
    unsupported_modern_format(&bytes)?;
    if is_svg(&bytes){return raster_svg(&bytes,edge);}
    let mut reader=image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let format=reader.format().context("无法识别图片格式")?;
    ensure!(matches!(format,image::ImageFormat::Png|image::ImageFormat::Jpeg|image::ImageFormat::WebP|image::ImageFormat::Gif|image::ImageFormat::Bmp|image::ImageFormat::Tiff|image::ImageFormat::Ico),"不支持该预览格式");
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(16384);limits.max_image_height=Some(16384);limits.max_alloc=Some(128*1024*1024);
    reader.limits(limits.clone());
    let mut decoder=reader.into_decoder().context("图片格式不支持、内容损坏或尺寸超限")?;
    let (w,h)=decoder.dimensions();
    ensure!(w>0&&h>0&&w<=16384&&h<=16384&&u64::from(w)*u64::from(h)<=24_000_000,"图片超过安全解码限制（2400万像素 / 单边16384）");
    limits.reserve(decoder.total_bytes())?;
    decoder.set_limits(limits)?;
    let decoded=image::DynamicImage::from_decoder(decoder).context("图片内容损坏或超出安全解码限制")?;
    let small=if w>edge||h>edge{decoded.thumbnail(edge,edge)}else{decoded};
    Ok((small,w,h))
}
fn decode_small(bytes:Vec<u8>,edge:u32)->Result<image::DynamicImage>{Ok(decode_bounded(bytes,edge)?.0)}
pub(crate) fn draft_image(bytes:Vec<u8>)->Result<std::sync::Arc<gpui_kit::RenderImage>>{
    let mut rgba=decode_small(bytes,384)?.into_rgba8();
    for p in rgba.pixels_mut(){p.0.swap(0,2);}
    Ok(std::sync::Arc::new(gpui_kit::RenderImage::new(vec![image::Frame::new(rgba)])))
}
fn write_small(dest:&Path,bytes:Vec<u8>) -> Result<PathBuf> {
    let small=decode_small(bytes,EDGE)?;
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
    if valid_cache(&dest){return Ok(dest);}
    let bytes=read_bounded(source)?;
    ensure!(hex::encode(Sha256::digest(&bytes))==asset.sha256,"本地文件内容已变化");
    write_small(&dest,bytes)
}

/// Existing rows: reuse an old local preview, or a verified Demo source.
/// A missing remote thumbnail stays missing; no HTTP fallback.
pub fn thumbnail(root:&Path,asset:&Asset) -> Result<PathBuf> {
    ensure!(asset.category=="image","非图片资源");
    let dest=cached_path(root,asset);
    if valid_cache(&dest){return Ok(dest);}
    {
        let _guard=CACHE_LOCK.lock().map_err(|_|anyhow::anyhow!("预览锁异常"))?;
        if valid_cache(&dest){return Ok(dest);}
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
    limits.max_image_width=Some(EDGE);limits.max_image_height=Some(EDGE);limits.max_alloc=Some(2*1024*1024);
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
    if storage.preview_pending(asset.id)?{
        let path=cached_path(storage.data_dir(),asset);
        if valid_cache(&path){return Ok(path);}
        anyhow::bail!("缩略图正在后台生成");
    }
    if let Ok(path)=thumbnail(storage.data_dir(),asset){
        if storage.preview_state(asset.id)?.is_none_or(|(state,_)|state=="failed"){
            storage.set_preview_state(asset.id,"ready","")?;
        }
        return Ok(path);
    }
    let mut reason=storage.preview_state(asset.id)?.map(|(_,error)|error).filter(|e|!e.is_empty()).unwrap_or_else(||"没有可用的本地来源".into());
    for source in storage.preview_sources(asset)? {
        match persist_source(storage.data_dir(),asset,&source){
            Ok(path)=>{storage.set_preview_state(asset.id,"ready","")?;return Ok(path);}
            Err(e)=>reason=e.to_string(),
        }
    }
    storage.set_preview_state(asset.id,"failed",&reason)?;
    anyhow::bail!("本地预览不可用：{reason}")
}
pub fn persist_upload(storage:&crate::storage::Storage,asset:&Asset,source:&Path)->Option<String>{
    if asset.category!="image"{return None;}
    let remote=storage.preview_state(asset.id).ok().flatten().is_some_and(|(s,_)|s=="remote_ready")&&valid_cache(&cached_path(storage.data_dir(),asset));
    let result=persist_source(storage.data_dir(),asset,source).and_then(|p|{
        let (hash,_)=visual_hash(&p)?;
        storage.store_visual_hash(asset.id,hash)?;Ok(())
    });
    let error=result.err().map(|e|e.to_string());
    if let Err(e)=storage.set_preview_state(asset.id,if error.is_some(){"failed"}else if remote{"remote_ready"}else{"ready"},error.as_deref().unwrap_or("")){
        return Some(format!("预览状态保存失败：{e}"));
    }
    error
}
pub fn recover_remote(storage:&crate::storage::Storage,asset:&Asset)->Result<PathBuf>{
    ensure!(asset.provider=="yuque"&&asset.category=="image","仅支持语雀图片");
    ensure!(!storage.preview_pending(asset.id)?,"缩略图正在后台生成，请稍后重试");
    let dest=cached_path(storage.data_dir(),asset);
    if valid_cache(&dest){return Ok(dest);}
    let bytes=crate::high_preview::download(asset)?;
    recover_remote_bytes(storage,asset,bytes)
}
fn recover_remote_bytes(storage:&crate::storage::Storage,asset:&Asset,bytes:Vec<u8>)->Result<PathBuf>{
    ensure!(asset.provider=="yuque"&&asset.category=="image","仅支持语雀图片");
    ensure!(!bytes.is_empty()&&bytes.len() as u64<=MAX_INPUT,"图片超过预览输入限制");
    let _guard=CACHE_LOCK.lock().map_err(|_|anyhow::anyhow!("预览锁异常"))?;
    let dest=cached_path(storage.data_dir(),asset);
    // CDN bytes may be transformed. Never replace an already valid local preview.
    if valid_cache(&dest){return Ok(dest);}
    let path=write_small(&dest,bytes)?;
    let (hash,_)=visual_hash(&path)?;
    storage.store_visual_hash(asset.id,hash)?;
    storage.set_preview_state(asset.id,"remote_ready","")?;
    Ok(path)
}

const AUDIT_SAMPLE_LIMIT:usize=100;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum AuditStatus{Ready,Missing,Corrupt,Pending}
impl AuditStatus{
    pub fn label(self)->&'static str{match self{Self::Ready=>"有效",Self::Missing=>"缺失",Self::Corrupt=>"损坏/不可读",Self::Pending=>"待处理"}}
    fn code(self)->&'static str{match self{Self::Ready=>"ready",Self::Missing=>"missing",Self::Corrupt=>"corrupt",Self::Pending=>"pending"}}
    pub fn repairable(self)->bool{matches!(self,Self::Missing|Self::Corrupt)}
}
#[derive(Clone,Debug)]
pub(crate) struct AuditIssue{
    pub id:i64,pub name:String,pub status:AuditStatus,pub unsupported_link:bool,
}
#[derive(Clone,Default,Debug)]
pub(crate) struct AuditReport{
    pub total:usize,pub scanned:usize,pub ready:usize,pub missing:usize,pub corrupt:usize,
    pub pending:usize,pub unsupported_link:usize,pub scheduled:usize,pub cancelled:bool,pub interrupted:bool,
    pub issue_count:usize,pub issues:Vec<AuditIssue>,
}
impl AuditReport{
    pub fn summary(&self)->String{
        format!("{} {}/{} · 有效 {} · 缺失 {} · 损坏/不可读 {} · 待处理 {} · 不支持在线恢复的链接 {} · 新增修复 {}",
            if self.interrupted{"扫描异常中断"}else if self.cancelled{"已停止"}else{"已检查"},self.scanned,self.total,self.ready,self.missing,self.corrupt,self.pending,self.unsupported_link,self.scheduled)
    }
    fn inspect(&mut self,root:&Path,asset:&Asset,pending:bool)->Option<AuditIssue>{
        self.scanned+=1;
        let unsupported_link=asset.provider=="yuque"&&crate::high_preview::remote_url(&asset.url).is_err();
        if unsupported_link{self.unsupported_link+=1;}
        let status=if pending{self.pending+=1;AuditStatus::Pending}else{
            let path=cached_path(root,asset);
            if valid_cache(&path){self.ready+=1;AuditStatus::Ready}else{
                match std::fs::symlink_metadata(&path){
                    Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{self.missing+=1;AuditStatus::Missing},
                    _=>{self.corrupt+=1;AuditStatus::Corrupt},
                }
            }
        };
        if status==AuditStatus::Ready&&!unsupported_link{return None;}
        self.issue_count+=1;
        // Filenames are sufficient for locating an asset. Do not include URL,
        // credentials, source path or arbitrary database error text in reports.
        let issue=AuditIssue{id:asset.id,name:asset.name.chars().take(255).collect(),status,unsupported_link};
        if self.issues.len()<AUDIT_SAMPLE_LIMIT{self.issues.push(issue.clone());}
        Some(issue)
    }
}
/// Inspect only local cache bytes. Link checks are syntactic/policy checks, not
/// reachability checks. No source files are read or network requests made.
pub(crate) fn audit_page(storage:&crate::storage::Storage,provider:&str,owner:&str,after:i64,upper:i64,mut report:AuditReport,cancel:&std::sync::atomic::AtomicBool,repair:bool)->Result<(i64,AuditReport,bool)>{
    use std::sync::atomic::Ordering;
    let rows=storage.preview_audit_page(provider,owner,after,upper)?;
    let mut cursor=after;let mut ids=Vec::new();
    let done=rows.len()<64;
    for (asset,pending) in rows{
        if cancel.load(Ordering::Relaxed){report.cancelled=true;break;}
        cursor=asset.id;
        if report.inspect(storage.data_dir(),&asset,pending).is_some_and(|i|i.status.repairable()){ids.push(asset.id);}
    }
    if cancel.load(Ordering::Relaxed){report.cancelled=true;}
    if repair&&!report.cancelled{report.scheduled+=storage.schedule_previews(provider,owner,&ids)?;}
    let finished=done||cursor>=upper||report.cancelled;
    Ok((cursor,report,finished))
}
/// Explicit export is a fresh read-only scan, not a replay of a stale sample.
pub(crate) fn export_audit(storage:&crate::storage::Storage,provider:&str,owner:&str,parent:&Path)->Result<(PathBuf,usize)>{
    use std::io::{BufWriter,Write};
    let parent=parent.canonicalize()?;
    ensure!(!parent.starts_with(storage.data_dir().canonicalize()?),"请选择应用数据目录之外的位置");
    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let dir=parent.join(format!("YuquePreviewAudit-{stamp}"));
    std::fs::create_dir(&dir)?;
    let path=dir.join("preview-audit.jsonl");
    let result=(||->Result<usize>{
        let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
        #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
        let mut writer=BufWriter::new(options.open(&path)?);
        let (upper,total)=storage.preview_audit_bounds(provider,owner)?;
        let mut report=AuditReport{total,..Default::default()};
        let mut cursor=0;
        writeln!(writer,"{}",serde_json::json!({"type":"header","schema":1,"provider":provider,"total":total,"scope":"selected_account","link_check":"policy_only"}))?;
        loop{
            let rows=storage.preview_audit_page(provider,owner,cursor,upper)?;
            let done=rows.len()<64;
            for (asset,pending) in rows{
                cursor=asset.id;
                if let Some(issue)=report.inspect(storage.data_dir(),&asset,pending){
                    writeln!(writer,"{}",serde_json::json!({"type":"issue","asset":{"id":issue.id,"name":issue.name,"status":issue.status.code(),"unsupported_link":issue.unsupported_link}}))?;
                }
            }
            if done||cursor>=upper{break;}
        }
        writeln!(writer,"{}",serde_json::json!({"type":"summary","scanned":report.scanned,"total":report.total,"ready":report.ready,"missing":report.missing,"corrupt":report.corrupt,"pending":report.pending,"unsupported_link":report.unsupported_link,"issues":report.issue_count}))?;
        writer.flush()?;writer.get_ref().sync_all()?;
        Ok(report.issue_count)
    })();
    match result{Ok(n)=>Ok((path,n)),Err(e)=>{let _=std::fs::remove_file(&path);let _=std::fs::remove_dir(&dir);Err(e)}}
}

/// One durable task at a time. No network, credentials or upload retries.
pub(crate) struct PreviewCompletion{pub id:i64,pub found:Option<(PathBuf,f32)>,pub changed:bool,pub settled:bool}
pub(crate) fn process_next(storage:&crate::storage::Storage)->Result<Option<PreviewCompletion>>{
    let Some((asset,name))=storage.next_preview()?else{return Ok(None);};
    let dest=cached_path(storage.data_dir(),&asset);
    let initially_valid=valid_cache(&dest);
    let remote=storage.preview_state(asset.id)?.is_some_and(|(s,_)|s=="remote_ready")&&initially_valid;
    let mut reason="没有可用的本地来源".to_string();
    let mut result=if initially_valid{Some(dest.clone())}else{None};
    let staging=storage.data_dir().join("upload-staging");
    let snapshot=if crate::storage::valid_snapshot_name(&name)&&std::fs::symlink_metadata(&staging).is_ok_and(|m|m.file_type().is_dir()){
        Some(staging.join(&name))
    }else{None};
    if result.is_none(){
        if let Some(path)=snapshot.as_ref().filter(|p|std::fs::symlink_metadata(p).is_ok_and(|m|m.file_type().is_file())){
            match persist_source(storage.data_dir(),&asset,path){Ok(p)=>result=Some(p),Err(e)=>reason=e.to_string()}
        }
    }
    if result.is_none(){
        if let Ok(p)=thumbnail(storage.data_dir(),&asset){result=Some(p);}
    }
    if result.is_none(){
        for source in storage.preview_sources(&asset)?{
            match persist_source(storage.data_dir(),&asset,&source){Ok(p)=>{result=Some(p);break;},Err(e)=>{if reason=="没有可用的本地来源"||source.is_file(){reason=e.to_string();}}}
        }
    }
    let (found,hash)=match result{
        Some(path)=>match visual_hash(&path){Ok((hash,ratio))=>(Some((path,ratio)),Some(hash)),Err(e)=>{reason=e.to_string();(None,None)}},
        None=>(None,None)
    };
    let state=if found.is_none(){"failed"}else if remote{"remote_ready"}else{"ready"};
    let error=if found.is_none(){reason.as_str()}else{""};
    // Compare the snapshot name: a concurrent duplicate may have upgraded a
    // source-only task. In that case leave the newer work for the next pass.
    let settled=storage.finish_preview(asset.id,&name,state,error,hash)?;
    if settled{
        if let Some(path)=snapshot{if std::fs::symlink_metadata(&path).is_ok_and(|m|m.file_type().is_file()){let _=std::fs::remove_file(path);}}
    }
    Ok(Some(PreviewCompletion{id:asset.id,found,changed:!initially_valid,settled}))
}
/// Only called with the process-wide instance lock held, before any uploads.
pub(crate) fn cleanup_staging(storage:&crate::storage::Storage)->Result<usize>{
    let dir=storage.data_dir().join("upload-staging");
    if !dir.exists(){return Ok(0);}
    ensure!(std::fs::symlink_metadata(&dir)?.file_type().is_dir(),"暂存目录不是普通目录");
    let mut count=0;
    for entry in std::fs::read_dir(dir)?{
        let entry=entry?;
        if !entry.file_type()?.is_file(){continue;}
        let name=entry.file_name();let Some(name)=name.to_str()else{continue;};
        if crate::storage::valid_snapshot_name(name)&&!storage.snapshot_referenced(name)?{
            std::fs::remove_file(entry.path())?;count+=1;
        }
    }
    Ok(count)
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
    fn shared_decode_supports_real_raster_formats_and_rejects_bad_content(){
        for format in [image::ImageFormat::Png,image::ImageFormat::Jpeg,image::ImageFormat::WebP,image::ImageFormat::Gif,image::ImageFormat::Bmp,image::ImageFormat::Tiff,image::ImageFormat::Ico]{
            let mut bytes=Vec::new();
            let fixture=if format==image::ImageFormat::Ico{image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(64,32,image::Rgba([20,80,150,255])))}else{image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(64,32,image::Rgb([20,80,150])))};
            fixture.write_to(&mut Cursor::new(&mut bytes),format).unwrap();
            let (decoded,w,h)=decode_bounded(bytes.clone(),16).unwrap();
            assert_eq!((w,h,decoded.width(),decoded.height()),(64,32,16,8),"{format:?}");
            assert!(draft_image(bytes).is_ok(),"{format:?}");
        }
        assert!(decode_bounded(vec![1,2,3],160).is_err());
        let mut bytes=Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(16385,1)).write_to(&mut Cursor::new(&mut bytes),image::ImageFormat::Png).unwrap();
        assert!(decode_bounded(bytes,160).is_err());
    }
    #[test]
    fn account_audit_paginates_is_read_only_and_respects_cancel(){
        use std::sync::atomic::{AtomicBool,Ordering};
        let root=std::env::temp_dir().join(format!("yuque-audit-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db=crate::storage::Storage::new(root.join("assets.db")).unwrap();
        let make=|i,owner:&str|db.insert_or_get(&crate::storage::NewAsset{name:format!("{i}.png"),bytes:1,category:"image".into(),provider:"yuque".into(),owner_id:owner.into(),url:if i==2{"https://example.invalid/2.png".into()}else{format!("https://cdn.nlark.com/yuque/0/{i}.png")},filekey:String::new(),sha256:format!("{i:064x}")}).unwrap();
        let assets=(1..=130).map(|i|make(i,"123")).collect::<Vec<_>>();
        let other=make(1,"456");
        let cache=cached_path(&root,&assets[0]);std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        image::RgbImage::new(32,16).save(&cache).unwrap();
        std::fs::write(cached_path(&root,&assets[1]),b"bad").unwrap();
        let (upper,total)=db.preview_audit_bounds("yuque","123").unwrap();assert_eq!(total,130);
        let late=make(131,"123");
        let cancel=AtomicBool::new(false);let mut report=AuditReport{total,..Default::default()};let mut cursor=0;
        loop{let (next,r,done)=audit_page(&db,"yuque","123",cursor,upper,report,&cancel,false).unwrap();cursor=next;report=r;if done{break;}}
        assert_eq!((report.scanned,report.ready,report.corrupt,report.missing,report.unsupported_link,report.scheduled),(130,1,1,128,1,0));
        assert_eq!(db.pending_previews().unwrap(),0);assert_eq!(std::fs::read(cached_path(&root,&assets[1])).unwrap(),b"bad");
        assert_eq!(report.issue_count,129);assert_eq!(report.issues.len(),100);
        assert_eq!(report.issues[0].id,assets[1].id);assert_eq!(report.issues[0].status,AuditStatus::Corrupt);assert!(report.issues[0].unsupported_link);
        let export_root=root.with_extension("exports");std::fs::create_dir(&export_root).unwrap();
        assert!(export_audit(&db,"yuque","123",&root).is_err());
        let (path,n)=export_audit(&db,"yuque","123",&export_root).unwrap();assert_eq!(n,130);
        let text=std::fs::read_to_string(&path).unwrap();
        let lines=text.lines().map(|l|serde_json::from_str::<serde_json::Value>(l).unwrap()).collect::<Vec<_>>();
        assert_eq!(lines.len(),132);assert_eq!(lines[0]["type"],"header");assert_eq!(lines.last().unwrap()["issues"],130);
        assert!(lines.iter().any(|v|v["asset"]["id"]==late.id));
        assert!(!lines.iter().any(|v|v["asset"]["id"]==other.id));
        assert!(!text.contains("cdn.nlark.com")&&!text.contains("example.invalid")&&!text.contains("owner_id")&&!text.contains("sha256"));
        assert_eq!(db.pending_previews().unwrap(),0);
        let (second,_)=export_audit(&db,"yuque","123",&export_root).unwrap();assert_ne!(path,second);assert_eq!(std::fs::read_to_string(&path).unwrap(),text);
        std::fs::remove_dir_all(export_root).unwrap();

        let (cursor,report,done)=audit_page(&db,"yuque","123",0,upper,AuditReport{total,..Default::default()},&cancel,true).unwrap();
        assert!(!done);assert_eq!(report.scheduled,63);assert_eq!(db.pending_previews().unwrap(),63);
        cancel.store(true,Ordering::Relaxed);
        let (_,report,done)=audit_page(&db,"yuque","123",cursor,upper,report,&cancel,true).unwrap();
        assert!(done&&report.cancelled);assert_eq!(report.scanned,64);assert_eq!(db.pending_previews().unwrap(),63);
        assert!(!db.preview_pending(other.id).unwrap());assert!(!db.preview_pending(late.id).unwrap());
        drop(db);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn svg_long_image_and_remote_transformed_cache_recovery(){
        let root=std::env::temp_dir().join(format!("yuque-v09-formats-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let storage=crate::storage::Storage::new(root.join("assets.db")).unwrap();
        let cfg=crate::config::Config{mode:crate::config::Mode::Demo,creds:None,max_file_mb:25};
        let svg=root.join("vector.svg");
        std::fs::write(&svg,br#"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="160"><rect width="320" height="160" fill="red"/></svg>"#).unwrap();
        let out=crate::uploader::upload(&svg,&cfg,&storage).unwrap();
        assert!(out.preview_error.is_none());assert_eq!(image::image_dimensions(cached_path(&root,&out.item)).unwrap(),(160,80));
        let long=root.join("long.png");image::RgbImage::from_pixel(10000,64,image::Rgb([50,60,70])).save(&long).unwrap();
        let out=crate::uploader::upload(&long,&cfg,&storage).unwrap();assert!(out.preview_error.is_none());
        assert_eq!(image::image_dimensions(cached_path(&root,&out.item)).unwrap().0,160);
        let remote=storage.insert_or_get(&crate::storage::NewAsset{name:"remote.png".into(),bytes:1,category:"image".into(),provider:"yuque".into(),owner_id:"1".into(),url:"https://cdn.nlark.com/yuque/0/fixture.png".into(),filekey:String::new(),sha256:"0".repeat(64)}).unwrap();
        let remote_bytes=std::fs::read(&long).unwrap();
        let cache=recover_remote_bytes(&storage,&remote,remote_bytes).unwrap();
        assert_eq!(storage.preview_state(remote.id).unwrap().unwrap().0,"remote_ready");
        let before=std::fs::read(&cache).unwrap();
        assert!(recover_remote_bytes(&storage,&remote,vec![0]).is_ok());assert_eq!(std::fs::read(&cache).unwrap(),before);
        assert_eq!(storage.schedule_previews("yuque","1",&[remote.id]).unwrap(),1);
        let result=process_next(&storage).unwrap().unwrap();assert!(result.found.is_some()&&!result.changed&&result.settled);
        assert_eq!(storage.preview_state(remote.id).unwrap().unwrap().0,"remote_ready");
        assert_eq!(std::fs::read(&cache).unwrap(),before);
        assert!(raster_svg(br#"<!DOCTYPE svg [<!ENTITY x SYSTEM "file:///etc/passwd">]><svg/>"#,160).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
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
