//! Explicit, read-only stable release discovery. No credentials or auto-install.
use anyhow::{Context, Result, ensure};
use semver::Version;
use serde_json::Value;
use std::{io::Read, time::Duration};

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
pub const RELEASES: &str = "https://github.com/weepwood/Yuque-Asset-Desk/releases";
const API: &str = "https://api.github.com/repos/weepwood/Yuque-Asset-Desk/releases/latest";
const MAX_BODY: u64 = 1024 * 1024;

pub fn target() -> Option<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("aarch64-apple-darwin")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("x86_64-pc-windows-msvc")
    } else { None }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Available {
    pub version: String,
    pub page: String,
    pub package: Option<(String, String)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    Current(String),
    Newer(Available),
    Failed(String),
}

fn parse(body: &[u8], current: &str, target: Option<&str>) -> Result<State> {
    let data: Value = serde_json::from_slice(body).context("版本响应格式无效")?;
    ensure!(data["draft"].as_bool() == Some(false) && data["prerelease"].as_bool() == Some(false), "未获得正式发布版本");
    let tag = data["tag_name"].as_str().context("缺少版本号")?;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).context("版本号无效")?;
    ensure!(version.pre.is_empty() && version.build.is_empty(), "非标准正式版本");
    let installed = Version::parse(current).context("当前版本号无效")?;
    if version <= installed {
        return Ok(State::Current(version.to_string()));
    }
    let page = format!("{RELEASES}/tag/{tag}");
    // Construct fixed, repository-owned links; never open arbitrary response URLs.
    let package = target.and_then(|target| {
        if !matches!(target, "aarch64-apple-darwin" | "x86_64-pc-windows-msvc") { return None; }
        let zip = format!("yuque-asset-desk-{version}-{target}.zip");
        let sum = format!("{zip}.sha256");
        let has = |name: &str| {
            data["assets"].as_array().is_some_and(|assets| assets.iter().any(|a| {
                a["name"].as_str() == Some(name) && a["state"].as_str() == Some("uploaded")
                    && a["size"].as_u64().is_some_and(|size| size > 0)
                    && a["browser_download_url"].as_str() == Some(format!("{RELEASES}/download/{tag}/{name}").as_str())
            }))
        };
        (has(&zip) && has(&sum)).then(|| (format!("{RELEASES}/download/{tag}/{zip}"), format!("{RELEASES}/download/{tag}/{sum}")))
    });
    Ok(State::Newer(Available { version: version.to_string(), page, package }))
}

pub fn check() -> State {
    match fetch() {
        Ok(state) => state,
        Err(error) => State::Failed(error.to_string()),
    }
}
fn fetch() -> Result<State> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(12)).connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none()).build().context("无法初始化更新检查")?;
    let response = client.get(API)
        .header("User-Agent", format!("Yuque-Asset-Desk/{CURRENT}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send().map_err(|_| anyhow::anyhow!("无法连接 GitHub，请检查网络后重试"))?;
    match response.status().as_u16() {
        200 => {},
        403 | 429 => anyhow::bail!("GitHub 请求受限，请稍后重试"),
        404 => anyhow::bail!("未找到可访问的正式发布，请检查项目发布页"),
        n => anyhow::bail!("GitHub 返回 HTTP {n}，请稍后重试"),
    }
    let mut body = Vec::new();
    response.take(MAX_BODY + 1).read_to_end(&mut body).context("读取版本响应失败")?;
    ensure!(body.len() as u64 <= MAX_BODY, "版本响应超出大小限制");
    parse(&body, CURRENT, target())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn release(tag: &str) -> Value {
        let version = tag.trim_start_matches('v');
        let assets = ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"].into_iter().flat_map(|target| {
            let zip = format!("yuque-asset-desk-{version}-{target}.zip");
            [zip.clone(), format!("{zip}.sha256")]
        }).map(|name| json!({"name":name,"size":42,"state":"uploaded","browser_download_url":format!("{RELEASES}/download/{tag}/{name}")})).collect::<Vec<_>>();
        json!({"tag_name":tag,"draft":false,"prerelease":false,"assets":assets})
    }
    fn read(value: Value, current: &str, target: Option<&str>) -> Result<State> { parse(&serde_json::to_vec(&value)?, current, target) }
    #[test]
    fn stable_semver_and_no_downgrade() {
        assert_eq!(read(release("v0.5.0"),"0.5.0",None).unwrap(),State::Current("0.5.0".into()));
        assert_eq!(read(release("v0.4.0"),"0.5.0",None).unwrap(),State::Current("0.4.0".into()));
        assert!(matches!(read(release("v0.10.0"),"0.9.0",None).unwrap(),State::Newer(_)));
        for tag in ["v0.6.0-beta.1","latest","v0.6.0+build","../../x"] { assert!(read(release(tag),"0.5.0",None).is_err()); }
        for key in ["draft","prerelease"] { let mut v=release("v0.6.0");v[key]=json!(true);assert!(read(v,"0.5.0",None).is_err()); }
    }
    #[test]
    fn correct_architecture_and_verified_asset_names() {
        for target in ["aarch64-apple-darwin","x86_64-pc-windows-msvc"] {
            let State::Newer(info)=read(release("v0.6.0"),"0.5.0",Some(target)).unwrap() else { panic!() };
            let (zip, sum)=info.package.unwrap();
            assert!(zip.ends_with(&format!("{target}.zip")));
            assert_eq!(sum,format!("{zip}.sha256"));
            assert_eq!(info.page,format!("{RELEASES}/tag/v0.6.0"));
        }
        let mut v=release("v0.6.0");v["assets"][0]["browser_download_url"]=json!("https://example.invalid/malicious.zip");
        let State::Newer(info)=read(v,"0.5.0",Some("aarch64-apple-darwin")).unwrap() else { panic!() };
        assert!(info.package.is_none());
        let mut v=release("v0.6.0");v["assets"].as_array_mut().unwrap().pop();
        let State::Newer(info)=read(v,"0.5.0",Some("x86_64-pc-windows-msvc")).unwrap() else { panic!() };
        assert!(info.package.is_none());
        let State::Newer(info)=read(release("v0.6.0"),"0.5.0",None).unwrap() else { panic!() };
        assert!(info.package.is_none());
        assert!(parse(b"not json","0.5.0",None).is_err());
    }
    #[test]
    #[ignore = "explicit live public GitHub API smoke; no account credentials"]
    fn public_release_smoke() {
        let state=check();
        println!("{state:?}");
        assert!(matches!(state,State::Current(_) | State::Newer(_)));
    }
}
