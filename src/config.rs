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
