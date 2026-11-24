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

/// Identity only, deliberately contains no access token or remote avatar URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountIdentity {
    pub id: String,
    pub name: String,
    pub login: String,
}
const OPEN_USER: &str = "https://www.yuque.com/api/v2/user";
const WEB_USER: &str = "https://www.yuque.com/api/mine";

fn parse_identity(data: &serde_json::Value, open_api: bool) -> anyhow::Result<AccountIdentity> {
    anyhow::ensure!(data.get("success").and_then(|v| v.as_bool()) != Some(false), "账号查询未获确认");
    let user = if open_api {
        data.get("data").ok_or_else(|| anyhow::anyhow!("开放 API 缺少用户信息"))?
    } else {
        let user = data.get("data").unwrap_or(data);
        user.get("user").unwrap_or(user)
    };
    // OpenAPI supports team tokens too; never treat their IDs as personal IDs.
    if open_api {
        anyhow::ensure!(user["type"].as_str()==Some("User"),"请使用个人访问 Token，不能使用团队 Token 或未知账号类型");
    } else {
        anyhow::ensure!(!matches!(user["type"].as_str(),Some("Group"|"Team")),"网页会话不是个人账号");
    }
    let id = user.get("id").and_then(|v| v.as_u64().map(|id|id.to_string()).or_else(||v.as_str().map(str::to_owned)))
        .filter(|id|!id.is_empty() && id.len()<=32 && id.bytes().all(|b|b.is_ascii_digit()) && id.bytes().any(|b|b!=b'0'))
        .ok_or_else(||anyhow::anyhow!("响应未包含有效个人账号 ID"))?;
    let display=|key:&str|user[key].as_str().unwrap_or_default().chars().filter(|c|!c.is_control()).take(100).collect();
    Ok(AccountIdentity{id,name:display("name"),login:display("login")})
}

// No cookie jar, no redirects, no logging body or reqwest errors containing credentials.
fn identity_request(endpoint:&str, header:&str, secret:&str, open_api:bool)->anyhow::Result<AccountIdentity>{
    use std::io::Read;
    let max=if open_api{4096}else{16384};
    anyhow::ensure!(!secret.is_empty()&&secret.len()<=max,"认证信息为空或过长");
    let mut value=reqwest::header::HeaderValue::from_str(secret).map_err(|_|anyhow::anyhow!("认证信息格式错误"))?;
    value.set_sensitive(true);
    let client=reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(12))
        .connect_timeout(std::time::Duration::from_secs(5)).redirect(reqwest::redirect::Policy::none())
        .build().map_err(|_|anyhow::anyhow!("无法初始化账号查询"))?;
    let response=client.get(endpoint).header(header,value).header("Accept","application/json")
        .header("User-Agent",concat!("Yuque-Asset-Desk/",env!("CARGO_PKG_VERSION")))
        .send().map_err(|_|anyhow::anyhow!("网络检查失败；请检查连接后重试"))?;
    match response.status().as_u16(){
        200=>{},
        401|403=>anyhow::bail!(if open_api{"个人 API Token 无效或无权限"}else{"网页会话未获授权，请更新 Cookie"}),
        429=>anyhow::bail!("账号查询频率受限，请稍后重试"),
        n=>anyhow::bail!("账号查询失败（HTTP {n}）"),
    }
    let mut bytes=Vec::new();
    response.take(256*1024+1).read_to_end(&mut bytes).map_err(|_|anyhow::anyhow!("账号响应读取失败"))?;
    anyhow::ensure!(bytes.len()<=256*1024,"账号响应过大");
    let data=serde_json::from_slice(&bytes).map_err(|_|anyhow::anyhow!("账号响应非预期 JSON"))?;
    parse_identity(&data,open_api)
}
pub fn identify_open(token:&str)->anyhow::Result<AccountIdentity>{
    identity_request(OPEN_USER,"x-auth-token",token,true)
}
pub fn identify_web(cookie:&str)->anyhow::Result<AccountIdentity>{
    identity_request(WEB_USER,"cookie",cookie,false)
}
fn matching_identity(api:Option<AccountIdentity>, web:AccountIdentity)->anyhow::Result<AccountIdentity>{
    if let Some(api)=api{
        anyhow::ensure!(api.id==web.id,"个人 API Token 与网页 Cookie 属于不同账号，未保存");
        Ok(api)
    }else{Ok(web)}
}
fn resolve_csrf(csrf:&str,cookie:&str)->anyhow::Result<String>{
    let mut found=None;
    for pair in cookie.split(';'){
        if let Some((key,value))=pair.trim().split_once('='){
            if key.trim()=="yuque_ctoken"{
                let value=value.trim();
                anyhow::ensure!(found.is_none()||found==Some(value),"Cookie 包含冲突的 yuque_ctoken");
                found=Some(value);
            }
        }
    }
    if let Some(value)=found{
        anyhow::ensure!(csrf.is_empty()||csrf==value,"手填 CSRF Token 与 Cookie 中的 yuque_ctoken 不一致");
        if !value.is_empty(){return Ok(value.into());}
    }
    anyhow::ensure!(!csrf.is_empty(),"Cookie 未包含 yuque_ctoken，请填写 CSRF Token");
    Ok(csrf.into())
}
/// Validate both inputs before saving; no caller-provided numeric ID or stale preview.
pub fn resolve_account(token:&str,cookie:String,csrf:String)->anyhow::Result<(YuqueCredentials,AccountIdentity)>{
    resolve_account_with(token,cookie,csrf,identify_open,identify_web)
}
fn resolve_account_with(token:&str,cookie:String,csrf:String,lookup_open:impl FnOnce(&str)->anyhow::Result<AccountIdentity>,lookup_web:impl FnOnce(&str)->anyhow::Result<AccountIdentity>)->anyhow::Result<(YuqueCredentials,AccountIdentity)>{
    let csrf=resolve_csrf(&csrf,&cookie)?;
    // Validate formats before network IO.
    YuqueCredentials::validated("1".into(),csrf.clone(),cookie.clone())?;
    let api=if token.is_empty(){None}else{Some(lookup_open(token)?)};
    let web=lookup_web(&cookie)?;
    let identity=matching_identity(api,web)?;
    let creds=YuqueCredentials::validated(identity.id.clone(),csrf,cookie)?;
    Ok((creds,identity))
}
pub fn probe_session(creds:&YuqueCredentials)->anyhow::Result<String>{
    let identity=identify_web(&creds.cookie)?;
    anyhow::ensure!(identity.id==creds.user_id,"当前登录账号与配置的用户 ID 不一致");
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
    fn identity_contract_and_account_mismatch(){
        use serde_json::json;
        let api=parse_identity(&json!({"data":{"id":123,"type":"User","name":"示例","login":"demo"}}),true).unwrap();
        assert_eq!((api.id.as_str(),api.name.as_str(),api.login.as_str()),("123","示例","demo"));
        let web=parse_identity(&json!({"data":{"user":{"id":"123"}}}),false).unwrap();
        assert_eq!(matching_identity(Some(api.clone()),web.clone()).unwrap(),api);
        assert_eq!(matching_identity(None,web.clone()).unwrap(),web);
        let other=parse_identity(&json!({"data":{"id":124}}),false).unwrap();
        assert!(matching_identity(Some(api),other).is_err());
        for value in [json!({"data":{"id":123,"type":"Group"}}),json!({"data":{"id":123}}),json!({"data":{"id":0,"type":"User"}}),json!({"data":{"id":-1,"type":"User"}}),json!({"success":false,"data":{"id":123,"type":"User"}}),json!({})]{
            assert!(parse_identity(&value,true).is_err());
        }
        assert!(parse_identity(&json!({"data":{"id":"../x"}}),false).is_err());
    }
    #[test]
    fn csrf_from_cookie_or_manual(){
        assert_eq!(resolve_csrf("","session=fixture; yuque_ctoken=abc=def").unwrap(),"abc=def");
        assert_eq!(resolve_csrf("manual","session=fixture").unwrap(),"manual");
        assert!(resolve_csrf("other","yuque_ctoken=fixture").is_err());
        assert!(resolve_csrf("","yuque_ctoken=a; yuque_ctoken=b").is_err());
        assert!(resolve_csrf("","session=fixture").is_err());
    }
    #[test]
    fn save_uses_fresh_identity_and_never_serializes_api_token(){
        let profile=|id:&str|AccountIdentity{id:id.into(),name:"Example".into(),login:"fixture".into()};
        let cookie="session=fixture; yuque_ctoken=csrf".to_string();
        let (creds,identity)=resolve_account_with("secret-api-token",cookie.clone(),String::new(),
            |token|{assert_eq!(token,"secret-api-token");Ok(profile("123"))},
            |value|{assert_eq!(value,cookie);Ok(profile("123"))}).unwrap();
        assert_eq!(creds.user_id,"123");
        assert_eq!(identity.id,"123");
        assert_eq!(creds.csrf,"csrf");
        assert!(!String::from_utf8(creds.encode()).unwrap().contains("secret-api-token"));
        assert!(resolve_account_with("api",cookie.clone(),String::new(),|_|Ok(profile("123")),|_|Ok(profile("456"))).is_err());
        assert!(resolve_account_with("api",cookie.clone(),String::new(),|_|anyhow::bail!("invalid token"),|_|panic!("must not fall back after token failure")).is_err());
        let (creds,_)=resolve_account_with("",cookie,String::new(),|_|panic!("optional token must not be sent"),|_|Ok(profile("456"))).unwrap();
        assert_eq!(creds.user_id,"456");
    }
    #[test]
    fn account_lookup_uses_separate_headers_and_rejects_redirect(){
        use std::io::{Read,Write};
        for (open,header) in [(true,"x-auth-token"),(false,"cookie")]{
            for code in [200,302,401,429]{
                let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let endpoint=format!("http://{}/user",listener.local_addr().unwrap());
                let worker=std::thread::spawn(move||{
                    let (mut stream,_)=listener.accept().unwrap();
                    stream.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
                    let mut bytes=Vec::new();
                    while !bytes.ends_with(b"\r\n\r\n"){
                        let mut b=[0u8;1];if stream.read(&mut b).unwrap()==0{break;}bytes.push(b[0]);
                    }
                    let req=String::from_utf8(bytes).unwrap().to_lowercase();
                    assert!(req.contains(&format!("{header}: fixture")));
                    assert!(!req.contains(if open{"cookie:"}else{"x-auth-token:"}));
                    let body=r#"{"data":{"id":123,"type":"User","name":"Fixture"}}"#;
                    write!(stream,"HTTP/1.1 {code} Test\r\nContent-Length: {}\r\nLocation: http://127.0.0.1:1/never\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                });
                let result=identity_request(&endpoint,header,"fixture",open);
                worker.join().unwrap();assert_eq!(result.is_ok(),code==200);
            }
        }
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
