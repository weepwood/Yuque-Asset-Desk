use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Demo,
    Yuque,
}

#[derive(Clone)]
pub struct YuqueCredentials {
    pub user_id: String,
    pub csrf: String,
    pub cookie: String,
}

#[derive(Clone)]
pub struct Config {
    pub mode: Mode,
    pub creds: Option<YuqueCredentials>,
    pub max_file_mb: u64,
}

impl YuqueCredentials {
    pub fn validated(user_id: String, csrf: String, cookie: String) -> anyhow::Result<Self> {
        anyhow::ensure!(!user_id.is_empty() && user_id.len() <= 32 && user_id.bytes().all(|b| b.is_ascii_digit()), "用户 ID 必须是数字");
        anyhow::ensure!(!csrf.is_empty() && csrf.len() <= 4096 && reqwest::header::HeaderValue::from_str(&csrf).is_ok(), "CSRF Token 格式错误");
        anyhow::ensure!(!cookie.is_empty() && cookie.len() <= 16384 && reqwest::header::HeaderValue::from_str(&cookie).is_ok(), "Cookie 格式错误");
        Ok(Self { user_id, csrf, cookie })
    }

    pub fn vault_key(owner: &str) -> String {
        format!("https://yuque.com/yuque-asset-desk/account/{owner}")
    }

    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"csrf": self.csrf, "cookie": self.cookie})).unwrap_or_default()
    }

    pub fn decode(owner: String, data: &[u8]) -> anyhow::Result<Self> {
        let value: serde_json::Value = serde_json::from_slice(data)
            .map_err(|_| anyhow::anyhow!("凭据数据格式错误"))?;
        Self::validated(owner, value["csrf"].as_str().unwrap_or_default().into(),
            value["cookie"].as_str().unwrap_or_default().into())
    }
}

/// A read-only, opt-in probe of the unofficial Web API. Never logs secrets or body.
pub fn probe_session(creds: &YuqueCredentials) -> anyhow::Result<String> {
    use std::io::Read;
    use reqwest::header::{HeaderValue, COOKIE, ACCEPT};
    let mut cookie = HeaderValue::from_str(&creds.cookie).map_err(|_| anyhow::anyhow!("Cookie 格式错误"))?;
    cookie.set_sensitive(true);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none()).build()?;
    let response = client.get("https://www.yuque.com/api/mine")
        .header(COOKIE, cookie).header(ACCEPT, "application/json")
        .send().map_err(|_| anyhow::anyhow!("网络检查失败；会话状态未知"))?;
    let status = response.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        anyhow::bail!("会话未获授权（HTTP {status}）");
    }
    anyhow::ensure!(status.is_success(), "会话检查未确认（HTTP {status}）");
    let mut bytes = Vec::new();
    response.take(256*1024+1).read_to_end(&mut bytes).map_err(|_| anyhow::anyhow!("会话响应读取失败"))?;
    anyhow::ensure!(bytes.len() <= 256*1024, "会话响应过大");
    let data: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("会话响应非预期 JSON；状态未知"))?;
    verify_session_payload(&data, &creds.user_id)
}

fn verify_session_payload(data: &serde_json::Value, expected: &str) -> anyhow::Result<String> {
    anyhow::ensure!(data.get("success").and_then(|v| v.as_bool()) != Some(false), "会话检查未获确认");
    let user = data.get("data").unwrap_or(data);
    let user = user.get("user").unwrap_or(user);
    let id = user.get("id").and_then(|v| v.as_u64().map(|id| id.to_string()).or_else(|| v.as_str().map(str::to_owned)))
        .ok_or_else(|| anyhow::anyhow!("会话响应未包含可核实的账号 ID"))?;
    anyhow::ensure!(id == expected, "当前登录账号与配置的用户 ID 不一致");
    Ok("会话有效（账号 ID 一致；上传权限尚未验证）".into())
}

impl Config {
    pub fn from_env() -> Self {
        let mode = if env::var("YUQUE_MODE").unwrap_or_default().eq_ignore_ascii_case("yuque") {
            Mode::Yuque
        } else {
            Mode::Demo
        };
        let user_id = env::var("YUQUE_USER_ID").unwrap_or_default();
        let csrf = env::var("YUQUE_CTOKEN").unwrap_or_default();
        let provided_cookie = env::var("YUQUE_COOKIE").unwrap_or_default();
        let session = env::var("YUQUE_SESSION").unwrap_or_default();
        let cookie = if !provided_cookie.is_empty() {
            provided_cookie
        } else if !session.is_empty() && !csrf.is_empty() {
            format!("_yuque_session={session}; yuque_ctoken={csrf}")
        } else {
            String::new()
        };
        let creds = YuqueCredentials::validated(user_id, csrf, cookie).ok();
        let max_file_mb = env::var("YUQUE_MAX_FILE_MB")
            .ok().and_then(|x| x.parse::<u64>().ok())
            .filter(|n| (1..=1024).contains(n)).unwrap_or(25);
        Self { mode, creds, max_file_mb }
    }

    pub fn owner_id(&self) -> &str {
        match (&self.mode, &self.creds) {
            (Mode::Yuque, Some(c)) => &c.user_id,
            (Mode::Yuque, None) => "unconfigured",
            (Mode::Demo, _) => "local",
        }
    }

    pub fn provider(&self) -> &'static str {
        match self.mode { Mode::Demo => "demo", Mode::Yuque => "yuque" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_probe_requires_matching_identity() {
        assert!(verify_session_payload(&serde_json::json!({"data":{"id":123}}), "123").is_ok());
        assert!(verify_session_payload(&serde_json::json!({"data":{"id":124}}), "123").is_err());
        assert!(verify_session_payload(&serde_json::json!({"success":false,"data":{"id":123}}), "123").is_err());
        assert!(verify_session_payload(&serde_json::json!({"success":true}), "123").is_err());
    }
    #[test]
    fn vault_payload_roundtrip_and_validation() {
        let creds = YuqueCredentials::validated("123".into(), "test-csrf".into(), "test-cookie=example".into()).unwrap();
        let recovered = YuqueCredentials::decode("123".into(), &creds.encode()).unwrap();
        assert_eq!(recovered.user_id, "123");
        assert_eq!(recovered.csrf, "test-csrf");
        assert!(YuqueCredentials::validated("../invalid".into(), "x".into(), "y".into()).is_err());
        assert!(YuqueCredentials::validated("123".into(), "x\r\n".into(), "y".into()).is_err());
        assert!(YuqueCredentials::decode("123".into(), b"invalid").is_err());
        assert_ne!(YuqueCredentials::vault_key("123"), YuqueCredentials::vault_key("456"));
    }

    #[test]
    fn demo_has_stable_owner() {
        let config = Config { mode: Mode::Demo, creds: None, max_file_mb: 25 };
        assert_eq!(config.owner_id(), "local");
    }
}
