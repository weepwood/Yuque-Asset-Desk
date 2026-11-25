//! Explicitly requested, single-image preview. No persistent high-resolution cache.
use crate::storage::{Asset,Storage};
use anyhow::{Context,Result,ensure};
use sha2::{Digest,Sha256};
use std::{io::{Cursor,Read},path::Path, sync::Mutex,time::Duration};
const INPUT:u64=32*1024*1024;
const PIXELS:u64=24_000_000;
const EDGE:u32=2560;
static WORK:Mutex<()>=Mutex::new(());
#[cfg(test)]
pub(crate) static TEST_LOCK:Mutex<()>=Mutex::new(());

pub struct QualityImage{
    pub image:std::sync::Arc<gpui_kit::RenderImage>,
    pub width:u32,pub height:u32,pub shown_width:u32,pub shown_height:u32,pub remote:bool,
}
fn read_bounded(r:impl Read)->Result<Vec<u8>>{
    let mut data=Vec::new();r.take(INPUT+1).read_to_end(&mut data)?;
    ensure!(!data.is_empty()&&data.len() as u64<=INPUT,"图片为空或超过 32 MiB");
    Ok(data)
}
fn verified(bytes:&[u8],asset:&Asset)->Result<()>{
    ensure!(hex::encode(Sha256::digest(bytes)).eq_ignore_ascii_case(&asset.sha256),"图片内容与上传记录不一致");
    Ok(())
}
fn decode(bytes:Vec<u8>,asset:&Asset,remote:bool)->Result<QualityImage>{
    if !remote{verified(&bytes,asset)?;}
    if crate::preview::is_svg(&bytes){
        let (decoded,width,height)=crate::preview::raster_svg(&bytes,EDGE)?;
        let mut rgba=decoded.into_rgba8();let (shown_width,shown_height)=rgba.dimensions();
        for p in rgba.pixels_mut(){p.0.swap(0,2);}
        return Ok(QualityImage{image:std::sync::Arc::new(gpui_kit::RenderImage::new(vec![image::Frame::new(rgba)])),width,height,shown_width,shown_height,remote});
    }
    let reader=image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
    let format=reader.format().context("无法识别图片格式")?;
    ensure!(matches!(format,image::ImageFormat::Png|image::ImageFormat::Jpeg|image::ImageFormat::WebP|image::ImageFormat::Gif|image::ImageFormat::Bmp),"不支持该预览格式");
    let (width,height)=reader.into_dimensions()?;
    ensure!(width>0&&height>0&&width<=16384&&height<=16384&&u64::from(width)*u64::from(height)<=PIXELS,"图片超过安全解码限制（2400 万像素 / 单边 16384）");
    let mut reader=image::ImageReader::with_format(Cursor::new(bytes),format);
    let mut limits=image::Limits::default();limits.max_image_width=Some(16384);limits.max_image_height=Some(16384);limits.max_alloc=Some(128*1024*1024);reader.limits(limits);
    let decoded=reader.decode()?;
    let decoded=if width>EDGE||height>EDGE{decoded.resize(EDGE,EDGE,image::imageops::FilterType::Triangle)}else{decoded};
    let mut rgba=decoded.into_rgba8();
    let (shown_width,shown_height)=rgba.dimensions();
    // GPUI RenderImage expects BGRA, not RGBA.
    for p in rgba.pixels_mut(){p.0.swap(0,2);}
    Ok(QualityImage{image:std::sync::Arc::new(gpui_kit::RenderImage::new(vec![image::Frame::new(rgba)])),width,height,shown_width,shown_height,remote})
}
fn read_local(path:&Path,asset:&Asset)->Result<Vec<u8>>{
    let meta=std::fs::symlink_metadata(path)?;
    ensure!(meta.is_file()&&!meta.file_type().is_symlink()&&meta.len()<=INPUT,"非普通图片文件或过大");
    let bytes=read_bounded(std::fs::File::open(path)?)?;verified(&bytes,asset)?;Ok(bytes)
}
fn remote_url(raw:&str)->Result<url::Url>{
    let mut u=url::Url::parse(raw).map_err(|_|anyhow::anyhow!("资源链接无效"))?;
    ensure!(u.scheme()=="https"&&u.host_str()==Some("cdn.nlark.com")&&u.port_or_known_default()==Some(443)&&u.username().is_empty()&&u.password().is_none()&&u.path().starts_with("/yuque/"),"仅支持语雀 CDN HTTPS 原图，请在浏览器打开其他链接");
    u.set_fragment(None);Ok(u)
}
fn client_builder()->reqwest::blocking::ClientBuilder{
    // Fixed trusted HTTPS endpoint; normal system/proxy routing works without
    // bypassing TLS checks. Never attach session headers or follow redirects.
    reqwest::blocking::Client::builder().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(20))
}
fn fetch(client:&reqwest::blocking::Client,url:url::Url)->Result<Vec<u8>>{
    let r=client.get(url).header("Accept","image/*").send().map_err(|_|anyhow::anyhow!("原图请求失败，请尝试浏览器打开"))?;
    ensure!(r.status().as_u16()==200,"原图请求失败（HTTP {}）",r.status().as_u16());
    ensure!(r.content_length().is_none_or(|n|n<=INPUT),"原图超过 32 MiB");
    read_bounded(r).map_err(|_|anyhow::anyhow!("原图读取失败或超过 32 MiB"))
}
pub(crate) fn download(asset:&Asset)->Result<Vec<u8>>{
    let url=remote_url(&asset.url)?;
    let client=client_builder().build().map_err(|_|anyhow::anyhow!("原图请求初始化失败"))?;
    fetch(&client,url)
}
pub fn load(storage:&Storage,asset:&Asset,remote:bool)->Result<QualityImage>{
    let _work=WORK.try_lock().map_err(|_|anyhow::anyhow!("已有高清请求正在处理，请稍后重试"))?;
    ensure!(asset.category=="image","非图片资源");
    if remote{
        ensure!(asset.provider=="yuque","本地资源不能在线加载");
        return decode(download(asset)?,asset,true);
    }
    let mut paths=Vec::new();
    if asset.provider=="demo"{
        if let Ok(u)=url::Url::parse(&asset.url){if let Ok(p)=u.to_file_path(){paths.push(p);}}
    }
    paths.extend(storage.preview_sources(asset)?);
    for p in paths{
        if let Ok(bytes)=read_local(&p,asset){return decode(bytes,asset,false);}
    }
    anyhow::bail!("未找到匹配的本地原图，可主动加载在线原图或在浏览器打开")
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn quality_local_hash_and_bgra(){
        let _serial=TEST_LOCK.lock().unwrap();
        let root=std::env::temp_dir().join(format!("yuque-hq-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let db=Storage::new(root.join("assets.db")).unwrap();let p=root.join("source.png");
        image::RgbImage::from_pixel(640,480,image::Rgb([19,82,201])).save(&p).unwrap();
        let bytes=std::fs::read(&p).unwrap();
        let a=db.insert_or_get(&crate::storage::NewAsset{name:"source.png".into(),bytes:bytes.len() as i64,category:"image".into(),provider:"demo".into(),owner_id:"local".into(),url:url::Url::from_file_path(&p).unwrap().to_string(),sha256:hex::encode(Sha256::digest(&bytes)),filekey:String::new()}).unwrap();
        let q=load(&db,&a,false).unwrap();assert_eq!((q.width,q.height,q.shown_width,q.shown_height),(640,480,640,480));assert_eq!(&q.image.as_bytes(0).unwrap()[..4],&[201,82,19,255]);
        assert!(!q.remote);assert!(!crate::preview::cached_path(&root,&a).exists());
        std::fs::write(&p,b"changed").unwrap();assert!(load(&db,&a,false).is_err());assert_eq!(std::fs::read(&p).unwrap(),b"changed");
        assert!(load(&db,&a,true).is_err());std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn online_http_is_explicit_and_bounded(){
        use std::io::{Read,Write};
        for (status,expected) in [("200 OK",true),("302 Found",false),("403 Forbidden",false),("200 OK",false)]{
            let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url=url::Url::parse(&format!("http://{}/fixture",listener.local_addr().unwrap())).unwrap();
            let oversized=status=="200 OK"&&!expected;
            let server=std::thread::spawn(move||{
                let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut req=Vec::new();let mut b=[0u8;1];
                while !req.ends_with(b"\r\n\r\n")&&req.len()<8192{if stream.read(&mut b).unwrap()==0{break;}req.push(b[0]);}
                let req=String::from_utf8_lossy(&req).to_lowercase();
                assert!(req.starts_with("get /fixture "));
                assert!(req.contains("accept: image/*"));
                for header in ["cookie:","authorization:","x-auth-token:","x-csrf-token:","referer:"]{assert!(!req.contains(header),"{header}");}
                let len=if oversized{INPUT+1}else{7};
                write!(stream,"HTTP/1.1 {status}\r\nContent-Length: {len}\r\nLocation: http://127.0.0.1:1/never\r\nConnection: close\r\n\r\nfixture").unwrap();
            });
            let result=fetch(&client_builder().no_proxy().build().unwrap(),url);
            server.join().unwrap();assert_eq!(result.is_ok(),expected);
            if expected{assert_eq!(result.unwrap(),b"fixture");}
        }
    }
    #[test]
    fn remote_scope_and_limits(){
        // The limit is checked before decoding or allocating pixel buffers.
        let mut bytes=Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(16385,1)).write_to(&mut Cursor::new(&mut bytes),image::ImageFormat::Png).unwrap();
        let a=Asset{id:1,name:"wide.png".into(),bytes:bytes.len() as i64,category:"image".into(),provider:"demo".into(),owner_id:"local".into(),url:String::new(),sha256:hex::encode(Sha256::digest(&bytes)),filekey:String::new(),created_at:String::new()};
        assert!(decode(bytes,&a,false).is_err());
        let mut bytes=Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(3000,1000)).write_to(&mut Cursor::new(&mut bytes),image::ImageFormat::Png).unwrap();
        let a=a;
        assert!(decode(bytes.clone(),&a,false).is_err());
        let q=decode(bytes,&a,true).unwrap();assert_eq!((q.width,q.height,q.shown_width,q.shown_height),(3000,1000,2560,853));

        assert!(remote_url("https://cdn.nlark.com/yuque/0/a.png#x").is_ok());
        for u in ["http://cdn.nlark.com/yuque/a","https://cdn.nlark.com.evil.test/yuque/a","https://user@cdn.nlark.com/yuque/a","https://cdn.nlark.com:444/yuque/a","https://cdn.nlark.com/other/a","https://127.0.0.1/yuque/a"]{assert!(remote_url(u).is_err(),"{u}");}
        assert!(read_bounded(std::io::repeat(0).take(INPUT+1)).is_err());
    }
}
