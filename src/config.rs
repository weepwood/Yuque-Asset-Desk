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
        let creds = if !cookie.is_empty() && !csrf.is_empty() && user_id.bytes().all(|b| b.is_ascii_digit()) && !user_id.is_empty() {
            Some(YuqueCredentials { user_id, csrf, cookie })
        } else {
            None
        };
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
    fn demo_has_stable_owner() {
        let config = Config { mode: Mode::Demo, creds: None, max_file_mb: 25 };
        assert_eq!(config.owner_id(), "local");
    }
}
