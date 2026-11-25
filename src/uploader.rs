use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::Duration;
use std::sync::{Arc, atomic::{AtomicU64, AtomicBool, Ordering}};
use anyhow::{bail, Context, Result};
use reqwest::{blocking::{Client, multipart}, header::{ACCEPT, COOKIE, ORIGIN, REFERER, HeaderMap, HeaderValue}};
use serde_json::Value;
use sha2::{Digest, Sha256};
use crate::config::{Config, Mode};
use crate::storage::{Asset, NewAsset, Storage};

pub struct UploadResult {
    pub item: Asset,
    pub duplicate: bool,
    pub preview_error: Option<String>,
}

/// Bytes pulled by the HTTP body reader, not acknowledged by the remote server.
#[derive(Default)]
pub struct TransferProgress {
    pub bytes: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
    pub remote_started: AtomicBool,
    pub rejected: AtomicBool,
    pub http_status: std::sync::atomic::AtomicU16,
}

struct MeteredReader<R> {
    inner: R,
    progress: Arc<TransferProgress>,
    bytes_per_second: u64,
    started: std::time::Instant,
}
impl<R: Read> Read for MeteredReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.progress.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("upload cancelled"));
        }
        if self.progress.bytes.load(Ordering::Relaxed) == 0 { self.started=std::time::Instant::now(); }
        let limit = if self.bytes_per_second == 0 { buf.len() } else {
            buf.len().min((self.bytes_per_second / 20).max(1) as usize)
        };
        let n = self.inner.read(&mut buf[..limit])?;
        if n > 0 && self.bytes_per_second > 0 {
            let target = Duration::from_secs_f64((self.progress.bytes.load(Ordering::Relaxed) + n as u64) as f64 / self.bytes_per_second as f64);
            while let Some(wait) = target.checked_sub(self.started.elapsed()) {
                if self.progress.cancel.load(Ordering::Relaxed) {
                    return Err(std::io::Error::other("upload cancelled"));
                }
                std::thread::sleep(wait.min(Duration::from_millis(20)));
            }
        }
        self.progress.bytes.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

/// Queue decisions use structured transport evidence, never localized error strings.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum QueueStop { Authorization, RateLimit, Unknown }
pub fn queue_stop(progress:&TransferProgress)->Option<QueueStop>{
    match progress.http_status.load(Ordering::Acquire){
        401|403=>Some(QueueStop::Authorization),
        429=>Some(QueueStop::RateLimit),
        _ if failure_state(progress)=="unknown"=>Some(QueueStop::Unknown),
        _=>None,
    }
}

pub fn failure_state(progress: &TransferProgress) -> &'static str {
    if progress.remote_started.load(Ordering::Acquire) && !progress.rejected.load(Ordering::Acquire) { "unknown" }
    else if progress.cancel.load(Ordering::Relaxed) { "cancelled" }
    else { "failed" }
}

#[cfg(test)]
pub fn upload(path: &Path, cfg: &Config, storage: &Storage) -> Result<UploadResult> {
    upload_with_progress(path, cfg, storage, Arc::new(TransferProgress::default()),None)
}

pub fn upload_with_progress(path: &Path, cfg: &Config, storage: &Storage, progress: Arc<TransferProgress>,job:Option<i64>) -> Result<UploadResult> {
    upload_with_transport(path,cfg,storage,progress,job,upload_yuque)
}
fn upload_with_transport(path:&Path,cfg:&Config,storage:&Storage,progress:Arc<TransferProgress>,job:Option<i64>,
    transport:impl FnOnce(&Path,&str,&str,&Config,Arc<TransferProgress>)->Result<(String,String)>)->Result<UploadResult>{
    // Fail before hashing large files when the account is not configured.
    if cfg.mode == Mode::Yuque && cfg.creds.is_none() {
        bail!("未配置语雀会话，请设置 YUQUE_USER_ID、YUQUE_CTOKEN 和 YUQUE_COOKIE");
    }
    anyhow::ensure!(!progress.cancel.load(Ordering::Relaxed), "上传已取消");
    let metadata = std::fs::metadata(path).context("无法读取文件元数据")?;
    if !metadata.is_file() { bail!("只能上传普通文件"); }
    if metadata.len() == 0 { bail!("不支持空文件"); }
    if metadata.len() > cfg.max_file_mb * 1024 * 1024 {
        bail!("文件超过 {} MB 上限", cfg.max_file_mb);
    }
    let file_name = path.file_name().and_then(|v| v.to_str()).context("文件名不是有效 UTF-8")?.to_string();
    let snapshot=UploadSnapshot::create(storage,path,metadata.len(),&progress)?;
    let category = detected_category(&snapshot.path,path);
    let digest = snapshot.digest.clone();
    anyhow::ensure!(!progress.cancel.load(Ordering::Relaxed), "上传已取消");
    if let Some(mut existing) = storage.find(cfg.provider(), cfg.owner_id(), &digest)? {
        // Demo links refer to real local files. Repair stale links when the same
        // content is imported from another location, retaining its ID and tags.
        if cfg.mode == Mode::Demo {
            let old_path = url::Url::parse(&existing.url).ok().and_then(|u| u.to_file_path().ok());
            let usable = old_path.as_ref().is_some_and(|p| {
                std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() == metadata.len())
                    && sha256_file(p).is_ok_and(|hash| hash == digest)
            });
            if !usable {
                let local = url::Url::from_file_path(path.canonicalize()?)
                    .map_err(|_| anyhow::anyhow!("无法生成本机文件 URL"))?.to_string();
                storage.refresh_demo_link(existing.id, &file_name, &local)?;
                existing.name = file_name;
                existing.url = local;
            }
        }
        let entry=NewAsset{name:existing.name.clone(),bytes:existing.bytes,category:existing.category.clone(),provider:existing.provider.clone(),owner_id:existing.owner_id.clone(),url:existing.url.clone(),filekey:existing.filekey.clone(),sha256:existing.sha256.clone()};
        let existing=storage.commit_asset(&entry,Some(path),job)?;
        let preview_error=crate::preview::persist_upload(storage,&existing,&snapshot.path);
        return Ok(UploadResult { item: existing, duplicate: true,preview_error });
    }
    let (url, filekey) = match cfg.mode {
        Mode::Demo => {
            // Demo mode never pretends to have obtained a public CDN URL.
            let local = url::Url::from_file_path(path.canonicalize()?)
                .map_err(|_| anyhow::anyhow!("无法生成本机文件 URL"))?;
            (local.to_string(), String::new())
        }
        Mode::Yuque => transport(&snapshot.path, &file_name, &category, cfg, progress.clone())?,
    };
    let entry=NewAsset {
        name: file_name, bytes: metadata.len() as i64, category,
        provider: cfg.provider().to_string(), owner_id: cfg.owner_id().to_string(),
        url, filekey, sha256: digest,
    };
    if cfg.mode==Mode::Yuque {
        if let Some(job)=job{storage.record_receipt(&entry,path,job).context("远端已返回结果，但回执保存失败；请核实远端，勿直接重传")?;}
    }
    let item=storage.commit_asset(&entry,Some(path),job).context("上传结果已返回，但本地资源提交失败；请核实远端，勿直接重传")?;
    let preview_error=crate::preview::persist_upload(storage,&item,&snapshot.path);
    Ok(UploadResult { item, duplicate: false,preview_error })
}

// The same immutable snapshot is hashed, transmitted and decoded. It is
// private transient data, never an original, and removed on every normal exit.
struct UploadSnapshot{path:std::path::PathBuf,digest:String}
impl Drop for UploadSnapshot{fn drop(&mut self){let _=std::fs::remove_file(&self.path);}}
impl UploadSnapshot{
    fn create(storage:&Storage,source:&Path,expected:u64,progress:&TransferProgress)->Result<Self>{
        use std::io::Write;
        let dir=storage.data_dir().join("upload-staging");std::fs::create_dir_all(&dir)?;
        anyhow::ensure!(std::fs::symlink_metadata(&dir)?.file_type().is_dir(),"暂存目录不是普通目录");
        let nonce=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
        let path=dir.join(format!("{}-{nonce}.snapshot",std::process::id()));
        let mut options=std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
        let output=options.open(&path)?;
        let mut snapshot=Self{path,digest:String::new()};
        let mut output=output;
        let mut input=File::open(source)?.take(expected+1);
        let mut hash=Sha256::new();let mut total=0;let mut buf=[0u8;65536];
        loop{anyhow::ensure!(!progress.cancel.load(Ordering::Relaxed),"上传已取消");
            let n=input.read(&mut buf)?;if n==0{break;}
            total+=n as u64;anyhow::ensure!(total<=expected,"文件在读取期间变大");
            output.write_all(&buf[..n])?;hash.update(&buf[..n]);
        }
        anyhow::ensure!(total==expected,"文件在读取期间发生变化");
        output.flush()?;drop(output);snapshot.digest=hex::encode(hash.finalize());Ok(snapshot)
    }
}
fn detected_category(snapshot:&Path,original:&Path)->String{
    let mut prefix=Vec::new();
    if File::open(snapshot).and_then(|f|f.take(64*1024).read_to_end(&mut prefix)).is_ok()&&crate::preview::is_svg(&prefix){return "image".into();}
    if image::ImageReader::open(snapshot).and_then(|r|r.with_guessed_format()).ok().and_then(|r|r.format()).is_some(){return "image".into();}
    asset_category(original)
}
pub fn asset_category(path: &Path) -> String {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "svg") {
        "image".into()
    } else if matches!(ext.as_str(), "mp4" | "mov" | "webm") { "video".into() }
    else { "attachment".into() }
}

pub fn sha256_file(path: &Path) -> Result<String> {
    sha256_file_controlled(path, None)
}

fn sha256_file_controlled(path: &Path, progress: Option<&TransferProgress>) -> Result<String> {
    let mut reader = BufReader::with_capacity(64 * 1024, File::open(path)?);
    let mut hash = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        anyhow::ensure!(!progress.is_some_and(|p| p.cancel.load(Ordering::Relaxed)), "上传已取消");
        let n = reader.read(&mut chunk)?;
        if n == 0 { break; }
        hash.update(&chunk[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}

fn upload_yuque(path: &Path, filename: &str, category: &str, cfg: &Config, progress: Arc<TransferProgress>) -> Result<(String, String)> {
    upload_yuque_to("https://www.yuque.com/api/upload/attach",path,filename,category,cfg,progress)
}
fn upload_yuque_to(endpoint:&str,path:&Path,filename:&str,category:&str,cfg:&Config,progress:Arc<TransferProgress>)->Result<(String,String)>{
    let auth = cfg.creds.as_ref().context("缺少语雀账号凭据，请设置环境变量 YUQUE_USER_ID / YUQUE_CTOKEN / YUQUE_COOKIE")?;
    let url = reqwest::Url::parse_with_params(endpoint, &[
        ("attachable_type", "User"),
        ("attachable_id", &auth.user_id),
        ("type", category),
        ("ctoken", &auth.csrf),
    ])?;
    let mut headers = HeaderMap::new();
    let mut cookie = HeaderValue::from_str(&auth.cookie).context("语雀 Cookie 格式错误")?;
    cookie.set_sensitive(true);
    headers.insert(COOKIE, cookie);
    let mut csrf = HeaderValue::from_str(&auth.csrf).context("CSRF Token 格式错误")?;
    csrf.set_sensitive(true);
    headers.insert("x-csrf-token", csrf);
    headers.insert(ORIGIN, HeaderValue::from_static("https://www.yuque.com"));
    headers.insert(REFERER, HeaderValue::from_static("https://www.yuque.com/"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert("x-requested-with", HeaderValue::from_static("XMLHttpRequest"));

    let detected=image::ImageReader::open(path).and_then(|r|r.with_guessed_format()).ok().and_then(|r|r.format());
    let fallback=mime_guess::from_path(filename).first_or_octet_stream();
    let mut prefix=Vec::new();
    let svg=File::open(path).and_then(|f|f.take(64*1024).read_to_end(&mut prefix)).is_ok()&&crate::preview::is_svg(&prefix);
    let mime=if svg{"image/svg+xml"}else{detected.map(|f|f.to_mime_type()).unwrap_or(fallback.essence_str())};
    let file = File::open(path).context("打开待上传文件失败")?;
    let length = file.metadata()?.len();
    anyhow::ensure!(length > 0 && length <= cfg.max_file_mb * 1024 * 1024, "文件大小已变化或超过限制");
    progress.total.store(length, Ordering::Relaxed);
    let bytes_per_second = std::env::var("YUQUE_UPLOAD_KIB_PER_SEC").ok().and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0).min(1024*1024) * 1024;
    let reader = MeteredReader { inner: file.take(length), progress: progress.clone(), bytes_per_second, started: std::time::Instant::now() };
    let part = multipart::Part::reader_with_length(reader, length)
        .file_name(filename.to_string())
        .mime_str(mime)?;
    let form = multipart::Form::new().part("file", part);
    let transfer_seconds = if bytes_per_second == 0 { 0 } else { length.div_ceil(bytes_per_second) };
    anyhow::ensure!(transfer_seconds <= 23*3600, "文件过大或限速过低，预计传输超过 23 小时");
    let builder=Client::builder();
    #[cfg(test)]
    let builder=if url.host_str()==Some("127.0.0.1"){builder.no_proxy()}else{builder};
    let client = builder
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120 + transfer_seconds))
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    send_multipart(client,url,headers,form,&progress)
}

fn send_multipart(client:Client,url:reqwest::Url,headers:HeaderMap,form:multipart::Form,progress:&TransferProgress) -> Result<(String,String)> {
    anyhow::ensure!(!progress.cancel.load(Ordering::Relaxed), "上传已取消");
    progress.remote_started.store(true, Ordering::Release);
    let response = client.post(url).headers(headers).multipart(form).send()
        .map_err(|e| anyhow::anyhow!(if e.is_timeout(){"语雀上传请求超时；远端结果待核实"}else if e.is_connect(){"语雀连接失败（DNS/TLS/代理或网络）；请检查网络"}else if e.is_body(){"上传请求体读取失败；远端结果待核实"}else{"语雀上传请求失败；远端结果待核实"}))?;
    let status = response.status();
    progress.http_status.store(status.as_u16(),Ordering::Release);
    if !status.is_success() {
        if matches!(status.as_u16(), 401 | 403 | 429) {
            progress.rejected.store(true, Ordering::Release);
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            bail!("语雀返回 HTTP {status}：请检查账号授权、Cookie 和 CSRF Token");
        }
        if status.as_u16() == 429 { bail!("语雀限制了上传频率（HTTP 429），请稍后手动重试"); }
        if status.is_redirection(){bail!("语雀返回 HTTP {status} 跳转；可能需要重新登录，未自动跟随或重传");}
        if status.as_u16()==413{bail!("语雀返回 HTTP 413：服务端拒绝该文件大小");}
        if status.as_u16()==415{bail!("语雀返回 HTTP 415：服务端不接受该媒体格式");}
        bail!("语雀返回 HTTP {status}：请检查接口是否变更或文件是否符合限制");
    }
    if response.content_length().unwrap_or_default() > 1024 * 1024 {
        bail!("语雀上传响应异常：内容过大");
    }
    // Bound the response even if the server does not provide a Content-Length header.
    let mut limited_body = Vec::new();
    response.take(1024 * 1024 + 1).read_to_end(&mut limited_body)
        .map_err(|_| anyhow::anyhow!("读取语雀上传响应失败；请先确认网页端是否已上传成功"))?;
    if limited_body.len() > 1024 * 1024 { bail!("语雀上传响应异常：内容超过 1 MB"); }
    parse_upload_payload(&limited_body)
}

fn parse_upload_payload(body:&[u8])->Result<(String,String)>{
    let payload: Value = serde_json::from_slice(body)
        .context("语雀响应不是有效 JSON；可能需要重新登录")?;
    if payload.get("success").and_then(Value::as_bool) == Some(false) {
        bail!("语雀明确返回上传失败（具体错误可在网页端开发者工具核实）");
    }
    if let Some(code) = payload.get("code") {
        if !(code == 0 || code == 200 || code == "0" || code == "200") {
            bail!("语雀返回非成功业务状态码");
        }
    }
    let (asset_url,filekey) = find_asset(&payload, 0).context("上传响应中没有 HTTPS 文件地址；语雀可能修改了返回格式")?;
    let parsed = url::Url::parse(&asset_url)?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() || !parsed.username().is_empty() || parsed.password().is_some() { bail!("语雀返回的文件地址并非有效 HTTPS URL"); }
    Ok((asset_url, filekey))
}

fn find_asset(node:&Value,depth:usize)->Option<(String,String)>{
    if depth>5{return None;}
    let object=node.as_object()?;
    if object.get("success").and_then(Value::as_bool)==Some(false){return None;}
    if let Some(code)=object.get("code"){if !(code==0||code==200||code=="0"||code=="200"){return None;}}
    for key in ["data","result","attachment","file"]{
        if let Some(link)=object.get(key).and_then(|inner|find_asset(inner,depth+1)){return Some(link);}
    }
    for key in ["url","download_url","downloadUrl","src"]{
        if let Some(url)=object.get(key).and_then(Value::as_str).filter(|u|u.starts_with("https://")){
            let filekey=["filekey","fileKey"].iter().find_map(|k|object.get(*k).and_then(Value::as_str)).unwrap_or_default();
            return Some((url.to_string(),filekey.to_string()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_png_mock_http_upload_receipt_preview_and_recovery(){
        use std::io::Write;
        let root=std::env::temp_dir().join(format!("yuque-v09-upload-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db=Storage::new(root.join("assets.db")).unwrap();
        let path=root.join("photo.dat");let mut bytes=Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(640,480,image::Rgb([30,80,120]))).write_to(&mut std::io::Cursor::new(&mut bytes),image::ImageFormat::Png).unwrap();
        std::fs::write(&path,&bytes).unwrap();
        let renamed=root.join("renamed.png");std::fs::write(&renamed,&bytes).unwrap();
        let cfg=Config{mode:Mode::Yuque,creds:Some(crate::config::YuqueCredentials::validated("123".into(),"fixture".into(),"session=fixture".into()).unwrap()),max_file_mb:25};
        let job=db.enqueue("yuque","123",&[path.clone()]).unwrap()[0].0;db.job_state(job,"running","").unwrap();
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let endpoint=format!("http://{}/fixture",listener.local_addr().unwrap());
        let sent=bytes.clone();
        let server=std::thread::spawn(move||{
            let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut request=Vec::new();let mut chunk=[0u8;4096];
            while !request.windows(4).any(|w|w==b"\r\n\r\n"){let n=stream.read(&mut chunk).unwrap();assert!(n>0);request.extend_from_slice(&chunk[..n]);}
            let header_end=request.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;
            let headers=String::from_utf8_lossy(&request[..header_end]);
            assert!(headers.starts_with("POST /fixture?attachable_type=User&attachable_id=123&type=image&ctoken=fixture "));
            let length=headers.lines().find_map(|l|l.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("content-length")).and_then(|(_,v)|v.trim().parse::<usize>().ok())).unwrap();
            while request.len()<header_end+length{let n=stream.read(&mut chunk).unwrap();assert!(n>0);request.extend_from_slice(&chunk[..n]);}
            assert!(request[header_end..].windows(sent.len()).any(|w|w==sent));
            let body=String::from_utf8_lossy(&request[header_end..]);assert!(body.contains("filename=\"photo.dat\""));assert!(body.contains("image/png"));
            let response=r#"{"success":true,"data":{"url":"https://cdn.nlark.com/yuque/0/fixture.png","filekey":"fixture"}}"#;
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        });
        let result=upload_with_transport(&path,&cfg,&db,Arc::new(TransferProgress::default()),Some(job),|snapshot,name,category,cfg,progress|{
            // A user editing the source after preparation must not alter transmitted bytes.
            std::fs::write(&path,b"changed original").unwrap();
            upload_yuque_to(&endpoint,snapshot,name,category,cfg,progress)
        }).unwrap();
        server.join().unwrap();
        assert_eq!(result.item.category,"image");assert_eq!(result.item.sha256,hex::encode(Sha256::digest(&bytes)));
        assert!(result.preview_error.is_none());
        assert_eq!(db.jobs("yuque","123",None).unwrap()[0].state,"done");
        assert_eq!(db.jobs("yuque","123",None).unwrap()[0].preview.as_ref().unwrap().0,"ready");
        assert_eq!(image::image_dimensions(crate::preview::cached_path(&root,&result.item)).unwrap(),(160,120));
        let duplicate=upload_with_transport(&renamed,&cfg,&db,Arc::new(TransferProgress::default()),None,|_,_,_,_,_|panic!("duplicate must not upload")).unwrap();
        assert!(duplicate.duplicate);assert_eq!(duplicate.item.id,result.item.id);
        let cache=crate::preview::cached_path(&root,&result.item);std::fs::write(&cache,b"corrupt").unwrap();
        assert_eq!(crate::preview::restore_local(&db,&result.item).unwrap(),cache);
        assert_eq!(image::image_dimensions(&cache).unwrap(),(160,120));
        std::fs::remove_file(&renamed).unwrap();drop(db);
        let db=Storage::new(root.join("assets.db")).unwrap();
        assert_eq!(crate::preview::restore_local(&db,&result.item).unwrap(),cache);
        assert_eq!(std::fs::read(&path).unwrap(),b"changed original");
        assert_eq!(std::fs::read_dir(root.join("upload-staging")).unwrap().count(),0);
        drop(db);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn receipt_replays_after_restart_and_preview_failure_is_independent(){
        let root=std::env::temp_dir().join(format!("yuque-v09-receipt-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db=Storage::new(root.join("assets.db")).unwrap();
        let path=root.join("unsupported.avif");std::fs::write(&path,b"unsupported fixture").unwrap();
        let job=db.enqueue("yuque","123",&[path.clone()]).unwrap()[0].0;db.job_state(job,"running","").unwrap();
        let entry=NewAsset{name:"unsupported.avif".into(),bytes:19,category:"image".into(),provider:"yuque".into(),owner_id:"123".into(),url:"https://cdn.nlark.com/yuque/0/fixture.avif".into(),filekey:"fixture".into(),sha256:sha256_file(&path).unwrap()};
        db.record_receipt(&entry,&path,job).unwrap();
        assert!(db.find("yuque","123",&entry.sha256).unwrap().is_none());
        drop(db);let db=Storage::new(root.join("assets.db")).unwrap();
        let a=db.find("yuque","123",&entry.sha256).unwrap().unwrap();
        assert_eq!(db.jobs("yuque","123",None).unwrap()[0].state,"done");
        assert!(crate::preview::persist_upload(&db,&a,&path).is_some());
        let job=db.jobs("yuque","123",None).unwrap().remove(0);assert_eq!(job.state,"done");assert_eq!(job.preview.unwrap().0,"failed");
        assert!(db.recoverable("yuque","123").unwrap().is_empty());
        drop(db);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn local_http_failures_do_not_assume_success() {
        use std::io::Write;
        use std::net::TcpListener;
        for (status, body, expected) in [
            ("401 Unauthorized","", "failed"),
            ("403 Forbidden","", "failed"),
            ("429 Too Many Requests","", "failed"),
            ("","", "unknown"),
            ("500 Internal Server Error","", "unknown"),
            ("200 OK","not json", "unknown"),
        ] {
            let listener=TcpListener::bind("127.0.0.1:0").unwrap();
            let addr=listener.local_addr().unwrap();
            let server=std::thread::spawn(move || {
                let (mut stream,_)=listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut request=Vec::new();
                let mut chunk=[0u8;1024];
                while !request.windows(4).any(|w|w==b"\r\n\r\n") {
                    let n=stream.read(&mut chunk).unwrap();if n==0{break;}request.extend_from_slice(&chunk[..n]);
                }
                let header_end=request.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;
                let headers=String::from_utf8_lossy(&request[..header_end]);
                let length=headers.lines().find_map(|l|l.split_once(':').filter(|(k,_)|k.eq_ignore_ascii_case("content-length")).and_then(|(_,v)|v.trim().parse::<usize>().ok())).unwrap_or(0);
                while request.len()<header_end+length {
                    let n=stream.read(&mut chunk).unwrap();if n==0{break;}request.extend_from_slice(&chunk[..n]);
                }
                if status.is_empty(){return;}
                write!(stream,"HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            });
            let progress=TransferProgress::default();
            let client=Client::builder().no_proxy().retry(reqwest::retry::never()).timeout(Duration::from_secs(3)).redirect(reqwest::redirect::Policy::none()).build().unwrap();
            let url=reqwest::Url::parse(&format!("http://{addr}/fixture")).unwrap();
            let result=send_multipart(client,url,HeaderMap::new(),multipart::Form::new().text("fixture","synthetic"),&progress);
            server.join().unwrap();
            assert!(result.is_err());
            assert_eq!(failure_state(&progress),expected);
            assert_eq!(queue_stop(&progress),match status{
                "401 Unauthorized"|"403 Forbidden"=>Some(QueueStop::Authorization),
                "429 Too Many Requests"=>Some(QueueStop::RateLimit),
                _=>Some(QueueStop::Unknown),
            });
        }
    }

    #[test]
    fn upload_response_contract(){
        let ok=br#"{"success":true,"code":200,"data":{"attachment":{"url":"https://example.invalid/p.png","filekey":"fixture"}}}"#;
        assert_eq!(parse_upload_payload(ok).unwrap(),("https://example.invalid/p.png".into(),"fixture".into()));
        for body in [
            r#"{"success":false,"url":"https://example.invalid/p.png"}"#,
            r#"{"code":403,"url":"https://example.invalid/p.png"}"#,
            r#"{"data":{"url":"http://example.invalid/p.png"}}"#,
            r#"{"data":{"url":"https://"}} "#,
            r#"{"data":{}}"#,
            "not json",
        ]{assert!(parse_upload_payload(body.as_bytes()).is_err(),"{body}");}
    }
    #[test]
    fn cancellation_and_unknown_outcome_are_distinct() {
        let p = Arc::new(TransferProgress::default());
        assert_eq!(failure_state(&p), "failed");
        p.cancel.store(true, Ordering::Relaxed);
        assert_eq!(failure_state(&p), "cancelled");
        p.remote_started.store(true, Ordering::Release);
        assert_eq!(failure_state(&p), "unknown");
        p.rejected.store(true, Ordering::Release);
        assert_eq!(failure_state(&p), "cancelled");
        let mut reader = MeteredReader { inner: std::io::Cursor::new(b"abc"), progress:p, bytes_per_second:0, started:std::time::Instant::now() };
        assert!(reader.read(&mut [0;3]).is_err());
    }

    #[test]
    fn metered_reader_enforces_rate() {
        let p = Arc::new(TransferProgress::default());
        let start = std::time::Instant::now();
        let mut reader = MeteredReader { inner: std::io::Cursor::new(vec![0;100]), progress:p, bytes_per_second:1000, started:start };
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        assert_eq!(out.len(),100);
        assert!(start.elapsed() >= Duration::from_millis(95));
    }

    #[test]
    fn metered_reader_counts_actual_bytes() {
        let progress = Arc::new(TransferProgress::default());
        let mut reader = MeteredReader { inner: std::io::Cursor::new(b"123456"), progress: progress.clone(), bytes_per_second: 0, started: std::time::Instant::now() };
        let mut buf = [0; 4];
        assert_eq!(reader.read(&mut buf).unwrap(), 4);
        assert_eq!(progress.bytes.load(Ordering::Relaxed), 4);
        assert_eq!(reader.read(&mut buf).unwrap(), 2);
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
        assert_eq!(progress.bytes.load(Ordering::Relaxed), 6);
    }

    #[test]
    fn unconfigured_account_is_rejected_before_reading_file() {
        let cfg = Config { mode: Mode::Yuque, creds: None, max_file_mb: 25 };
        let base = std::env::temp_dir().join(format!("yuque-auth-{}", std::process::id()));
        let db = Storage::new(base.join("db.sqlite")).unwrap();
        let error = upload(Path::new("this-file-should-not-exist"), &cfg, &db).err().unwrap();
        assert!(error.to_string().contains("未配置语雀会话"));
        drop(db);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn demo_validates_files_and_deduplicates_without_network() {
        let base = std::env::temp_dir().join(format!("yuque-demo-{}-{}",
            std::process::id(), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db = Storage::new(base.join("db.sqlite")).unwrap();
        let cfg = Config { mode: Mode::Demo, creds: None, max_file_mb: 1 };
        let first = base.join("first.png");
        let second = base.join("second.png");
        image::RgbImage::from_pixel(32,24,image::Rgb([15,40,90])).save(&first).unwrap();
        let fixture=std::fs::read(&first).unwrap();std::fs::write(&second,&fixture).unwrap();
        let uploaded = upload(&first, &cfg, &db).unwrap();
        assert!(!uploaded.duplicate);
        assert!(uploaded.item.url.starts_with("file:"));
        let duplicate = upload(&second, &cfg, &db).unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.item.id, uploaded.item.id);
        std::fs::remove_file(&first).unwrap();
        let repaired = upload(&second, &cfg, &db).unwrap();
        assert_eq!(repaired.item.id, uploaded.item.id);
        assert_eq!(repaired.item.name, "second.png");
        // A file URL round trip may remove the Windows extended-length path
        // prefix. Compare canonical filesystem paths, not their URL spelling.
        let repaired_path = url::Url::parse(&repaired.item.url).unwrap().to_file_path().unwrap();
        assert_eq!(repaired_path.canonicalize().unwrap(), second.canonicalize().unwrap());
        assert_eq!(std::fs::read(&repaired_path).unwrap(), fixture);
        assert_eq!(sha256_file(&repaired_path).unwrap(), repaired.item.sha256);
        let empty = base.join("empty.png");
        std::fs::write(&empty, []).unwrap();
        assert!(upload(&empty, &cfg, &db).is_err());
        let large = base.join("large.bin");
        std::fs::write(&large, vec![0u8; 1024 * 1024 + 1]).unwrap();
        assert!(upload(&large, &cfg, &db).is_err());
        assert!(upload(&base, &cfg, &db).is_err());
        drop(db);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn payload_extracts_nested_https_only() {
        let data = serde_json::json!({"data": {"url": "https://cdn.example.org/photo.png", "filekey":"foo"}});
        assert_eq!(find_asset(&data,0),Some(("https://cdn.example.org/photo.png".into(),"foo".into())));
        assert!(find_asset(&serde_json::json!({"url":"http://untrusted"}),0).is_none());
    }
    #[test]
    fn category_is_extension_based() {
        assert_eq!(asset_category(Path::new("photo.webp")), "image");
        assert_eq!(asset_category(Path::new("backup.zip")), "attachment");
    }
}
