//! Explicit preview requests only. Never pass account credentials to an asset URL.
use crate::storage::Asset;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{io::{Cursor, Read}, path::{Path, PathBuf}, time::Duration};

const MAX_INPUT: u64 = 8 * 1024 * 1024;
const CACHE_ITEMS: usize = 256;

pub fn cached_path(cache: &Path, asset: &Asset) -> PathBuf {
    let key = format!("v1:{}:{}:{}:{}", asset.provider, asset.owner_id, asset.sha256, asset.url);
    cache.join(format!("{}.png", hex::encode(Sha256::digest(key.as_bytes()))))
}

pub fn thumbnail(cache: &Path, asset: &Asset) -> Result<PathBuf> {
    ensure!(asset.category == "image", "仅支持图片预览");
    let dest = cached_path(cache, asset);
    if dest.is_file() { return Ok(dest); }
    let url = url::Url::parse(&asset.url).context("图片地址格式错误")?;
    let mut bytes = Vec::new();
    if asset.provider == "demo" && url.scheme() == "file" {
        let path = url.to_file_path().map_err(|_| anyhow::anyhow!("无效本地路径"))?;
        let file = std::fs::File::open(path).context("本地文件已移动或不存在")?;
        file.take(MAX_INPUT + 1).read_to_end(&mut bytes)?;
        ensure!(hex::encode(Sha256::digest(&bytes)) == asset.sha256, "本地文件内容已变化，请重新导入");
    } else {
        ensure!(url.scheme() == "https" && url.username().is_empty() && url.password().is_none(), "只允许 HTTPS 图片预览");
        let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none()).build()?;
        let response = client.get(url).send().map_err(|_| anyhow::anyhow!("图片预览请求失败"))?;
        ensure!(response.status().is_success(), "图片预览返回 HTTP {}", response.status());
        ensure!(response.content_length().unwrap_or(0) <= MAX_INPUT, "图片超过预览大小限制");
        response.take(MAX_INPUT + 1).read_to_end(&mut bytes).map_err(|_| anyhow::anyhow!("图片读取失败"))?;
    }
    ensure!(!bytes.is_empty() && bytes.len() as u64 <= MAX_INPUT, "图片超过预览大小限制或为空");
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().context("图片格式不支持或解码超出限制")?;
    let small = image.thumbnail(320, 320);
    std::fs::create_dir_all(cache)?;
    let tmp = dest.with_extension(format!("{}.{}.tmp", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()));
    if let Err(error) = small.save_with_format(&tmp, image::ImageFormat::Png) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error.into());
    }
    if let Err(error) = std::fs::rename(&tmp, &dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error.into());
    }
    // Bound the number of derived images; never delete source assets.
    let mut files: Vec<_> = std::fs::read_dir(cache)?.filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "png"))
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    files.sort_by_key(|(time, _)| *time);
    let excess = files.len().saturating_sub(CACHE_ITEMS);
    for (_, path) in files.into_iter().take(excess) {
        if path != dest { let _ = std::fs::remove_file(path); }
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_preview_generates_bounded_cache() {
        let root = std::env::temp_dir().join(format!("yuque-preview-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let src = root.join("source.png");
        image::RgbImage::new(640, 480).save(&src).unwrap();
        let bytes = std::fs::read(&src).unwrap();
        let asset = Asset { id: 1, name: "source.png".into(), bytes: bytes.len() as i64, category: "image".into(),
            provider: "demo".into(), owner_id: "local".into(), url: url::Url::from_file_path(&src).unwrap().to_string(),
            sha256: hex::encode(Sha256::digest(&bytes)), filekey: "".into(), created_at: "".into() };
        let cache = root.join("cache");
        let output = thumbnail(&cache, &asset).unwrap();
        assert_eq!(image::image_dimensions(&output).unwrap(), (320, 240));
        assert_eq!(thumbnail(&cache, &asset).unwrap(), output);
        let _ = std::fs::remove_dir_all(root);
    }
}
