use super::*;
use gpui_kit::component::Sizable;

impl UploadDesk {
    pub(super) fn activate_account(&mut self, creds: Option<YuqueCredentials>, cx: &mut Context<Self>) {
        if self.resource_write_busy || self.picker_pending || self.queue.busy || self.resetting { return; }
        self.account.config.mode = if creds.is_some() { Mode::Yuque } else { Mode::Demo };
        self.account.config.creds = creds;
        self.account.identity=None; self.account.identity_status.clear();
        self.account.session_status = "尚未验证会话".into();
        self.resource.all_accounts = false;
        self.resource.similar_to=None;
        self.resource.favorites_only=false;
        self.resource.favorites.clear();
        self.resource.tag_filter=None;
        self.resource.tag_catalog.clear();
        self.resource.total_count = 0;
        self.queue.failed.clear();
        self.queue.queue_total = 0;
        self.queue.queue_count = 0;
        self.queue.succeeded = 0;
        self.queue.duplicates = 0;
        self.queue.recovery_count = self.storage.recoverable(self.account.config.provider(), self.account.config.owner_id()).map(|jobs| jobs.len()).unwrap_or(0);
        self.status = if self.account.config.mode == Mode::Demo { "已切换到本地演示".into() }
            else { "凭据已加载，实际登录状态尚未验证".into() };
        self.queue.jobs.clear();
        self.queue.job_cursors = vec![None];
        self.refresh_jobs(cx);
        self.reset_history(cx);
    }

    pub(super) fn check_session(&mut self, cx: &mut Context<Self>) {
        if self.resource_write_busy || self.picker_pending || self.queue.busy || self.account.account_busy || self.account.config.mode != Mode::Yuque { return; }
        let Some(creds) = self.account.config.creds.clone() else { return; };
        self.account.account_busy = true;
        self.account.session_status = "正在执行只读会话检查…".into();
        self.account.account_task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { crate::config::probe_session(&creds) }).await;
            _ = this.update(cx, |view,cx| {
                view.account.account_busy = false;
                view.account.session_status = result.unwrap_or_else(|e| e.to_string());
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
        self.account.account_task = Some(cx.spawn(async move |this,cx| {
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

    fn identify_account(&mut self,cx:&mut Context<Self>){
        if self.resource_write_busy || self.picker_pending || self.queue.busy||self.account.account_busy||self.backup_busy{return;}
        let token=self.account.account_token.read(cx).value().trim().to_string();
        let cookie=self.account.account_cookie.read(cx).value().trim().to_string();
        if token.is_empty()&&cookie.is_empty(){self.account.identity_status="请先填写个人 API Token 或网页 Cookie".into();cx.notify();return;}
        self.account.identity=None;
        self.account.identity_status="正在识别账号…".into();
        self.account.account_busy=true;cx.notify();
        self.account.account_task=Some(cx.spawn(async move|this,cx|{
            let result=cx.background_spawn(async move{
                if token.is_empty(){crate::config::identify_web(&cookie)}else{crate::config::identify_open(&token)}
            }).await;
            let _=this.update(cx,|v,cx|{
                v.account.account_busy=false;
                match result{
                    Ok(info)=>{v.account.identity=Some(info);v.account.identity_status="上次识别结果；保存时将重新查询当前填写的认证信息".into();}
                    Err(e)=>v.account.identity_status=e.to_string(),
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn save_credentials(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.resource_write_busy || self.picker_pending || self.queue.busy || self.account.account_busy || self.backup_busy { return; }
        let token=self.account.account_token.read(cx).value().trim().to_string();
        let cookie=self.account.account_cookie.read(cx).value().trim().to_string();
        let csrf=self.account.account_csrf.read(cx).value().trim().to_string();
        if cookie.is_empty(){self.account.identity_status="上传需要网页 Cookie；API Token 仅用于账号识别".into();cx.notify();return;}
        self.account.identity=None;
        self.account.account_busy=true;
        self.account.identity_status="正在核对个人账号与网页会话…".into();
        cx.notify();
        self.account.account_task=Some(cx.spawn_in(window,async move|this,cx|{
            let result=cx.background_spawn(async move{crate::config::resolve_account(&token,cookie,csrf)}).await;
            let (creds,identity)=match result{
                Ok(r)=>r,
                Err(e)=>{let _=this.update_in(cx,|v,_,cx|{v.account.account_busy=false;v.account.identity_status=e.to_string();cx.notify();});return;}
            };
            let task=match this.update_in(cx,|v,_,cx|{
                if v.storage.save_account(&creds.user_id).is_err(){
                    v.account.account_busy=false;v.account.identity_status="账号索引保存失败，未写入系统凭据".into();cx.notify();return None;
                }
                v.account.saved_accounts=v.storage.accounts().unwrap_or_default();
                Some(cx.write_credentials(&YuqueCredentials::vault_key(&creds.user_id),&creds.user_id,&creds.encode()))
            }){
                Ok(Some(task))=>task,_=>return
            };
            let result=task.await;
            let _=this.update_in(cx,|v,w,cx|{
                v.account.account_busy=false;
                match result{
                    Ok(())=>{
                        v.account.account_token.update(cx,|input,cx|input.set_value("",w,cx));
                        v.account.account_csrf.update(cx,|input,cx|input.set_value("",w,cx));
                        v.account.account_cookie.update(cx,|input,cx|input.set_value("",w,cx));
                        v.account.saved_accounts=v.storage.accounts().unwrap_or_default();
                        v.activate_account(Some(creds),cx);
                        v.account.identity=Some(identity);
                        v.account.identity_status="已保存并切换账号".into();
                        v.account.session_status="网页会话账号一致；上传权限尚未验证".into();
                    }
                    Err(_)=>v.account.identity_status="系统密钥库写入失败；已保留索引，未明文保存".into(),
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn account_form(&self,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();
        let busy=self.resource_write_busy||self.picker_pending||self.account.account_busy||self.queue.busy||self.backup_busy;
        let mut form=div().flex().flex_col().gap(px(self.spacing().section))
            .child(div().text_xs().text_color(p.muted).child("无需手动填写数字 ID。个人 API Token 仅用于识别账号，不保存；网页 Cookie 用于上传。"))
            .child(div().flex().items_center().justify_between()
                .child(div().font_medium().child("个人访问 Token（可选）"))
                .child(Button::new("open-token-settings").small().ghost().label("获取 Token").on_click(cx.listener(|_,_,_,cx|cx.open_url("https://www.yuque.com/settings/tokens")))))
            .child(Input::new(&self.account.account_token).disabled(busy))
            .child(div().debug_selector(||"identify-account".into()).child(Button::new("identify-account").small().label(if self.account.account_busy{"处理中…"}else{"识别账号"}).disabled(busy).on_click(cx.listener(|v,_,_,cx|v.identify_account(cx)))))
            .when_some(self.account.identity.clone(),|d,info|d.child(div().p_3().rounded_md().bg(p.surface).flex().flex_col().gap_1()
                .child(div().font_medium().child(if info.name.is_empty(){"已识别个人账号".into()}else{info.name}))
                .child(div().text_xs().text_color(p.muted).child(format!("{} · ID {}",info.login,info.id)))))
            .when(!self.account.identity_status.is_empty(),|d|d.child(div().text_sm().child(self.account.identity_status.clone())))
            .child(div().h(px(1.)).bg(p.line))
            .child(div().font_medium().child("网页上传会话"))
            .child(div().flex().flex_col().gap(px(if self.prefs.comfortable{8.}else{self.spacing().section})).child(div().text_xs().text_color(p.muted).child("Cookie（必填）")).child(Input::new(&self.account.account_cookie).disabled(busy)))
            .child(div().flex().flex_col().gap(px(if self.prefs.comfortable{8.}else{self.spacing().section})).child(div().text_xs().text_color(p.muted).child("CSRF Token（Cookie 含 yuque_ctoken 时自动提取）")).child(Input::new(&self.account.account_csrf).disabled(busy)))
            .child(div().debug_selector(||"save-account".into()).child(Button::new("save-account").primary().label("验证并保存账号").disabled(busy).on_click(cx.listener(|v,_,w,cx|v.save_credentials(w,cx)))))
            .child(div().h(px(1.)).bg(p.line))
            .child(div().font_medium().child("当前账号"))
            .child(div().text_sm().child(if self.account.config.mode==Mode::Demo{"本地演示".into()}else{format!("语雀账号 {}",self.account.config.owner_id())}))
            .child(div().text_xs().text_color(p.muted).child(self.account.session_status.clone()))
            .when(self.account.config.mode==Mode::Yuque&&self.account.config.creds.is_some(),|d|d.child(Button::new("check-session").small().label("检查网页会话").disabled(busy).on_click(cx.listener(|v,_,_,cx|v.check_session(cx)))))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("demo-account").small().ghost().label("切换本地演示").disabled(busy).on_click(cx.listener(|v,_,_,cx|v.activate_account(None,cx))))
                .when(self.account.config.mode==Mode::Yuque,|d|d.child(Button::new("forget-account").small().ghost().label("删除当前保存凭据").disabled(busy).on_click(cx.listener(|v,_,w,cx|v.confirm_privacy(false,w,cx))))))
            .child(div().font_medium().child("已保存账号"))
            .when(self.account.saved_accounts.is_empty(),|d|d.child(div().text_xs().text_color(p.muted).child("暂无已保存账号")));
        for owner in &self.account.saved_accounts{
            let id=owner.clone();
            form=form.child(Button::new(SharedString::from(format!("account-{owner}"))).small().label(format!("账号 {owner}")).disabled(busy).on_click(cx.listener(move|v,_,_,cx|v.load_credentials(id.clone(),cx))));
        }
        form.child(div().text_xs().text_color(p.muted).child(self.status.clone()))
    }

    pub(super) fn load_credentials(&mut self, owner: String, cx: &mut Context<Self>) {
        if self.resource_write_busy || self.picker_pending || self.queue.busy || self.account.account_busy { return; }
        let task = cx.read_credentials(&YuqueCredentials::vault_key(&owner));
        self.account.account_busy = true;
        self.account.account_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |view, cx| {
                view.account.account_busy = false;
                match result {
                    Ok(Some((username, data))) if username == owner => {
                        match YuqueCredentials::decode(owner, &data) {
                            Ok(creds) => view.activate_account(Some(creds), cx),
                            Err(error) => view.status = error.to_string(),
                        }
                    }
                    _ => view.status = "未找到可用的系统密钥库凭据".into(),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

}
