use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::Duration;
use anyhow::{bail, Context, Result};
use reqwest::{blocking::{Client, multipart}, header::{ACCEPT, COOKIE, ORIGIN, REFERER, HeaderMap, HeaderValue}};
use serde_json::Value;
use sha2::{Digest, Sha256};
use crate::config::{Config, Mode};
use crate::storage::{Asset, NewAsset, Storage};

pub struct UploadResult {
    pub item: Asset,
    pub duplicate: bool,
}

pub fn upload(path: &Path, cfg: &Config, storage: &Storage) -> Result<UploadResult> {
    // Fail before hashing large files when the account is not configured.
    if cfg.mode == Mode::Yuque && cfg.creds.is_none() {
        bail!("未配置语雀会话，请设置 YUQUE_USER_ID、YUQUE_CTOKEN 和 YUQUE_COOKIE");
    }
    let metadata = std::fs::metadata(path).context("无法读取文件元数据")?;
    if !metadata.is_file() { bail!("只能上传普通文件"); }
    if metadata.len() == 0 { bail!("不支持空文件"); }
    if metadata.len() > cfg.max_file_mb * 1024 * 1024 {
        bail!("文件超过 {} MB 上限", cfg.max_file_mb);
    }
    let file_name = path.file_name().and_then(|v| v.to_str()).context("文件名不是有效 UTF-8")?.to_string();
    let category = asset_category(path);
    let digest = sha256_file(path)?;
    if let Some(existing) = storage.find(cfg.provider(), cfg.owner_id(), &digest)? {
        return Ok(UploadResult { item: existing, duplicate: true });
    }
    let (url, filekey) = match cfg.mode {
        Mode::Demo => {
            // Demo mode never pretends to have obtained a public CDN URL.
            let local = url::Url::from_file_path(path.canonicalize()?)
                .map_err(|_| anyhow::anyhow!("无法生成本机文件 URL"))?;
            (local.to_string(), String::new())
        }
        Mode::Yuque => upload_yuque(path, &file_name, &category, cfg)?,
    };
    let item = storage.insert_or_get(&NewAsset {
        name: file_name, bytes: metadata.len() as i64, category,
        provider: cfg.provider().to_string(), owner_id: cfg.owner_id().to_string(),
        url, filekey, sha256: digest,
    })?;
    Ok(UploadResult { item, duplicate: false })
}

pub fn asset_category(path: &Path) -> String {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "svg") {
        "image".into()
    } else if matches!(ext.as_str(), "mp4" | "mov" | "webm") { "video".into() }
    else { "attachment".into() }
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut reader = BufReader::with_capacity(64 * 1024, File::open(path)?);
    let mut hash = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 { break; }
        hash.update(&chunk[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}

fn upload_yuque(path: &Path, filename: &str, category: &str, cfg: &Config) -> Result<(String, String)> {
    let auth = cfg.creds.as_ref().context("缺少语雀账号凭据，请设置环境变量 YUQUE_USER_ID / YUQUE_CTOKEN / YUQUE_COOKIE")?;
    let url = reqwest::Url::parse_with_params("https://www.yuque.com/api/upload/attach", &[
        ("attachable_type", "User"),
        ("attachable_id", &auth.user_id),
        ("type", category),
        ("ctoken", &auth.csrf),
    ])?;
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(&auth.cookie).context("语雀 Cookie 格式错误")?);
    headers.insert("x-csrf-token", HeaderValue::from_str(&auth.csrf).context("CSRF Token 格式错误")?);
    headers.insert(ORIGIN, HeaderValue::from_static("https://www.yuque.com"));
    headers.insert(REFERER, HeaderValue::from_static("https://www.yuque.com/"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert("x-requested-with", HeaderValue::from_static("XMLHttpRequest"));

    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let part = multipart::Part::file(path)
        .context("打开待上传文件失败")?
        .file_name(filename.to_string())
        .mime_str(mime.essence_str())?;
    let form = multipart::Form::new().part("file", part);
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client.post(url).headers(headers).multipart(form).send()
        .map_err(|_| anyhow::anyhow!("语雀上传请求失败：网络超时、代理异常或接口无法访问"))?;
    let status = response.status();
    if !status.is_success() {
        if status.as_u16() == 401 || status.as_u16() == 403 {
            bail!("语雀返回 HTTP {status}：请检查账号授权、Cookie 和 CSRF Token");
        }
        if status.as_u16() == 429 { bail!("语雀限制了上传频率（HTTP 429），请稍后手动重试"); }
        bail!("语雀返回 HTTP {status}：请检查接口是否变更或文件是否符合限制");
    }
    if response.content_length().unwrap_or_default() > 1024 * 1024 {
        bail!("语雀上传响应异常：内容过大");
    }
    // Bound the response even if the server does not provide a Content-Length header.
    let mut limited_body = Vec::new();
    response.take(1024 * 1024 + 1).read_to_end(&mut limited_body)?;
    if limited_body.len() > 1024 * 1024 { bail!("语雀上传响应异常：内容超过 1 MB"); }
    let payload: Value = serde_json::from_slice(&limited_body)
        .context("语雀响应不是有效 JSON；可能需要重新登录")?;
    if payload.get("success").and_then(Value::as_bool) == Some(false) {
        bail!("语雀明确返回上传失败（具体错误可在网页端开发者工具核实）");
    }
    if let Some(code) = payload.get("code") {
        if !(code == 0 || code == 200 || code == "0" || code == "200") {
            bail!("语雀返回非成功业务状态码");
        }
    }
    let asset_url = find_url(&payload, 0).context("上传响应中没有 HTTPS 文件地址；语雀可能修改了返回格式")?;
    let parsed = url::Url::parse(&asset_url)?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() { bail!("语雀返回的文件地址并非有效 HTTPS URL"); }
    let filekey = find_string(&payload, &["filekey", "fileKey"], 0).unwrap_or_default();
    Ok((asset_url, filekey))
}

fn find_url(node: &Value, depth: usize) -> Option<String> {
    if depth > 5 { return None; }
    let object = node.as_object()?;
    for key in ["url", "download_url", "downloadUrl", "src"] {
        if let Some(url) = object.get(key).and_then(Value::as_str) {
            if url.starts_with("https://") { return Some(url.to_string()); }
        }
    }
    for key in ["data", "result", "attachment", "file"] {
        if let Some(inner) = object.get(key) {
            if let Some(link) = find_url(inner, depth + 1) { return Some(link); }
        }
    }
    None
}

fn find_string(node: &Value, keys: &[&str], depth: usize) -> Option<String> {
    if depth > 5 { return None; }
    let object = node.as_object()?;
    for key in keys {
        if let Some(value) = object.get(*key).and_then(Value::as_str) { return Some(value.to_string()); }
    }
    for key in ["data", "result", "attachment", "file"] {
        if let Some(inner) = object.get(key) {
            if let Some(v) = find_string(inner, keys, depth + 1) { return Some(v); }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn payload_extracts_nested_https_only() {
        let data = serde_json::json!({"data": {"url": "https://cdn.example.org/photo.png", "filekey":"foo"}});
        assert_eq!(find_url(&data, 0).as_deref(), Some("https://cdn.example.org/photo.png"));
        assert_eq!(find_string(&data, &["filekey"], 0).as_deref(), Some("foo"));
        assert!(find_url(&serde_json::json!({"url":"http://untrusted"}), 0).is_none());
    }
    #[test]
    fn category_is_extension_based() {
        assert_eq!(asset_category(Path::new("photo.webp")), "image");
        assert_eq!(asset_category(Path::new("backup.zip")), "attachment");
    }
}
