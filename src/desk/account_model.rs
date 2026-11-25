//! Owns account form, credential operations and asynchronously loaded index.
use super::*;
pub(super) enum AccountEvent { Activate(YuqueCredentials) }
pub(super) struct AccountModel {
    pub(super) account_token: Entity<InputState>,
    pub(super) identity: Option<crate::config::AccountIdentity>,
    pub(super) identity_status: String,
    pub(super) account_csrf: Entity<InputState>,
    pub(super) account_cookie: Entity<InputState>,
    pub(super) account_busy: bool,
    pub(super) session_status: String,
    pub(super) saved_accounts: Vec<String>,
    account_task: Option<Task<()>>,
    index_task:Option<Task<()>>,
    index_generation:u64,
    pub index_loading:bool,
    pub index_error:bool,
}

impl EventEmitter<AccountEvent> for AccountModel {}
impl AccountModel{
    pub fn new(window:&mut Window,cx:&mut Context<Self>)->Self{Self{
                account_token: cx.new(|cx| InputState::new(window, cx).placeholder("个人 API Token（可选）").masked(true)),
                identity:None, identity_status:String::new(),
                account_csrf: cx.new(|cx| InputState::new(window, cx).placeholder("Cookie 无 yuque_ctoken 时填写").masked(true)),
                account_cookie: cx.new(|cx| InputState::new(window, cx).placeholder("Cookie").masked(true)),
                account_busy: false,
                session_status: "尚未验证会话".into(),
                saved_accounts:vec![],index_task:None,index_generation:0,index_loading:false,index_error:false,
                account_task: None,
            }}
    pub fn reset_identity(&mut self,cx:&mut Context<Self>){
        self.identity=None;self.identity_status.clear();self.session_status="尚未验证会话".into();cx.notify();
    }
    pub fn replace_index(&mut self,ids:Vec<String>,cx:&mut Context<Self>){
        self.index_generation=self.index_generation.wrapping_add(1);
        self.index_task=None;self.index_loading=false;self.index_error=false;self.saved_accounts=ids;cx.notify();
    }
    pub fn refresh_index(&mut self,storage:Storage,cx:&mut Context<Self>){
        self.index_generation=self.index_generation.wrapping_add(1);let generation=self.index_generation;
        self.index_loading=true;self.index_error=false;cx.notify();
        self.index_task=Some(cx.spawn(async move|this,cx|{
            let result=cx.background_spawn(async move{storage.accounts()}).await;
            let _=this.update(cx,|v,cx|{
                if v.index_generation!=generation{return;}
                v.index_loading=false;
                match result{Ok(ids)=>v.saved_accounts=ids,Err(_)=>v.index_error=true}
                cx.notify();
            });
        }));
    }
    pub(super) fn check_session(&mut self,creds:YuqueCredentials,cx:&mut Context<Self>) {
        if self.account_busy{return;}
        self.account_busy = true;
        self.session_status = "正在执行只读会话检查…".into();
        self.account_task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { crate::config::probe_session(&creds) }).await;
            _ = this.update(cx, |view,cx| {
                view.account_busy = false;
                view.session_status = result.unwrap_or_else(|e| e.to_string());
                cx.notify();
            });
        }));
        cx.notify();
    }

    // Explicit diagnostic flag, only enabled with Demo + isolated data directory.
    // Uses a unique, fake entry and never enumerates or touches real accounts.
    pub(super) fn keychain_smoke(&mut self, cx: &mut Context<Self>) {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let key = format!("https://yuque.com/yuque-asset-desk/test/{}-{nonce}", std::process::id());
        let write = cx.write_credentials(&key, "test-only", b"synthetic-credential");
        self.account_task = Some(cx.spawn(async move |this,cx| {
            if write.await.is_err() { eprintln!("KEYCHAIN_SMOKE write failed"); return; }
            let read = match this.update(cx, |_,cx| cx.read_credentials(&key)) {
                Ok(task) => task.await,
                Err(_) => { eprintln!("KEYCHAIN_SMOKE view unavailable before read"); return; }
            };
            let matches = matches!(read, Ok(Some((ref user,ref secret))) if user=="test-only" && secret==b"synthetic-credential");
            let delete = match this.update(cx, |_,cx| cx.delete_credentials(&key)) {
                Ok(task) => task.await.is_ok(), Err(_) => false,
            };
            let absent = match this.update(cx, |_,cx| cx.read_credentials(&key)) {
                Ok(task) => matches!(task.await, Ok(None)), Err(_) => false,
            };
            eprintln!("KEYCHAIN_SMOKE write=true read={matches} delete={delete} absent={absent}");
        }));
    }

    pub fn identify_account(&mut self,cx:&mut Context<Self>){
        if self.account_busy{return;}
        let token=self.account_token.read(cx).value().trim().to_string();
        let cookie=self.account_cookie.read(cx).value().trim().to_string();
        if token.is_empty()&&cookie.is_empty(){self.identity_status="请先填写个人 API Token 或网页 Cookie".into();cx.notify();return;}
        self.identity=None;
        self.identity_status="正在识别账号…".into();
        self.account_busy=true;cx.notify();
        self.account_task=Some(cx.spawn(async move|this,cx|{
            let result=cx.background_spawn(async move{
                if token.is_empty(){crate::config::identify_web(&cookie)}else{crate::config::identify_open(&token)}
            }).await;
            let _=this.update(cx,|v,cx|{
                v.account_busy=false;
                match result{
                    Ok(info)=>{v.identity=Some(info);v.identity_status="上次识别结果；保存时将重新查询当前填写的认证信息".into();}
                    Err(e)=>v.identity_status=e.to_string(),
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn save_credentials(&mut self,storage:Storage,window:&mut Window,cx:&mut Context<Self>) {
        if self.account_busy{return;}
        let token=self.account_token.read(cx).value().trim().to_string();
        let cookie=self.account_cookie.read(cx).value().trim().to_string();
        let csrf=self.account_csrf.read(cx).value().trim().to_string();
        if cookie.is_empty(){self.identity_status="上传需要网页 Cookie；API Token 仅用于账号识别".into();cx.notify();return;}
        self.identity=None;
        self.account_busy=true;
        self.identity_status="正在核对个人账号与网页会话…".into();
        cx.notify();
        self.account_task=Some(cx.spawn_in(window,async move|this,cx|{
            let result=cx.background_spawn(async move{
                let (creds,identity)=crate::config::resolve_account(&token,cookie,csrf)?;
                storage.save_account(&creds.user_id).map_err(|_|anyhow::anyhow!("账号索引保存失败，未写入系统凭据"))?;
                let accounts=storage.accounts().map_err(|_|anyhow::anyhow!("账号索引读取失败，未写入系统凭据"))?;
                Ok::<_,anyhow::Error>((creds,identity,accounts))
            }).await;
            let (creds,identity,accounts)=match result{
                Ok(r)=>r,
                Err(e)=>{let _=this.update_in(cx,|v,_,cx|{v.account_busy=false;v.identity_status=e.to_string();cx.notify();});return;}
            };
            let task=match this.update_in(cx,|v,_,cx|{
                v.replace_index(accounts,cx);
                Some(cx.write_credentials(&YuqueCredentials::vault_key(&creds.user_id),&creds.user_id,&creds.encode()))
            }){
                Ok(Some(task))=>task,_=>return
            };
            let result=task.await;
            let _=this.update_in(cx,|v,w,cx|{
                v.account_busy=false;
                match result{
                    Ok(())=>{
                        v.account_token.update(cx,|input,cx|input.set_value("",w,cx));
                        v.account_csrf.update(cx,|input,cx|input.set_value("",w,cx));
                        v.account_cookie.update(cx,|input,cx|input.set_value("",w,cx));
                        cx.emit(AccountEvent::Activate(creds));
                        v.identity=Some(identity);
                        v.identity_status="已保存并切换账号".into();
                        v.session_status="网页会话账号一致；上传权限尚未验证".into();
                    }
                    Err(_)=>v.identity_status="系统密钥库写入失败；已保留索引，未明文保存".into(),
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn load_credentials(&mut self, owner: String, cx: &mut Context<Self>) {
        if self.account_busy{return;}
        let task = cx.read_credentials(&YuqueCredentials::vault_key(&owner));
        self.account_busy = true;
        self.account_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |view, cx| {
                view.account_busy = false;
                match result {
                    Ok(Some((username, data))) if username == owner => {
                        match YuqueCredentials::decode(owner, &data) {
                            Ok(creds) => {view.reset_identity(cx);cx.emit(AccountEvent::Activate(creds));},
                            Err(error) => view.identity_status = error.to_string(),
                        }
                    }
                    _ => view.identity_status = "未找到可用的系统密钥库凭据".into(),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

}
