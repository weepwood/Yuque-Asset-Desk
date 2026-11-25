use super::*;
use gpui_kit::component::Sizable;
impl UploadDesk{
    pub(super) fn activate_account(&mut self, creds: Option<YuqueCredentials>, cx: &mut Context<Self>) {
        if self.resource_write_busy || self.picker_pending || self.backup_busy || self.queue.read(cx).busy || self.account_busy(cx) || self.resetting { return; }
        self.account.update(cx,|a,cx|a.reset_identity(cx));
        self.apply_account(creds,cx);
    }
    pub(super) fn apply_account(&mut self,creds:Option<YuqueCredentials>,cx:&mut Context<Self>){
        self.cancel_upload_review(cx);
        self.audit_export_feedback.clear();
        self.preview_worker.update(cx,|v,cx|v.stop_scan(cx));
        self.resource.image_cache.update(cx,|cache,cx|cache.clear(cx));
        self.config.mode = if creds.is_some() { Mode::Yuque } else { Mode::Demo };
        self.config.creds = creds;
        self.resource.all_accounts = false;
        self.resource.similar_to=None;
        self.resource.favorites_only=false;
        self.resource.favorites.clear();
        self.resource.tag_filter=None;
        self.resource.tag_catalog.clear();
        self.resource.total_count = 0;
        self.queue.update(cx,|v,cx|v.reset_summary(cx));
        self.status = if self.config.mode == Mode::Demo { "已切换到本地演示".into() }
            else { "凭据已加载，实际登录状态尚未验证".into() };
        self.first_jobs(cx);
        self.reset_history(cx);
    }

    fn account_idle(&self,cx:&App)->bool{
        !self.resource_write_busy&&!self.picker_pending&&!self.backup_busy&&!self.queue.read(cx).busy&&!self.account_busy(cx)&&!self.resetting
    }
    pub(super) fn check_session(&mut self,cx:&mut Context<Self>){
        if !self.account_idle(cx){return;}
        if let Some(creds)=self.config.creds.clone(){self.account.update(cx,|a,cx|a.check_session(creds,cx));}
    }
    pub(super) fn keychain_smoke(&mut self,cx:&mut Context<Self>){self.account.update(cx,|a,cx|a.keychain_smoke(cx));}
    fn identify_account(&mut self,cx:&mut Context<Self>){
        if self.account_idle(cx){self.account.update(cx,|a,cx|a.identify_account(cx));}
    }
    pub(super) fn save_credentials(&mut self,w:&mut Window,cx:&mut Context<Self>){
        if self.account_idle(cx){self.account.update(cx,|a,cx|a.save_credentials(self.storage.clone(),w,cx));}
    }
    pub(super) fn load_credentials(&mut self,owner:String,cx:&mut Context<Self>){
        if self.account_idle(cx){self.account.update(cx,|a,cx|a.load_credentials(owner,cx));}
    }
    pub(super) fn account_form(&self,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();
        let busy=self.resource_write_busy||self.picker_pending||self.account_busy(cx)||self.queue.read(cx).busy||self.backup_busy;
        let model=self.account.read(cx);
        let current=div().flex().items_center().gap_3().pb_3().border_b_1().border_color(p.line)
            .child(div().flex_1().min_w(px(0.)).flex().flex_col().gap_1()
                .child(div().text_sm().font_medium().child(if self.config.mode==Mode::Demo{"本地演示".into()}else{format!("语雀账号 {}",self.config.owner_id())}))
                .child(div().text_xs().text_color(p.muted).child(if self.config.mode==Mode::Demo{"仅本地记录，不上传至语雀".into()}else{model.session_status.clone()})))
            .when(self.config.mode==Mode::Yuque,|d|d.child(div().flex().flex_wrap().gap_1()
                .child(Button::new("check-session").small().ghost().label("检查会话").disabled(busy||self.config.creds.is_none()).on_click(cx.listener(|v,_,_,cx|v.check_session(cx))))
                .child(Button::new("demo-account").small().ghost().label("切换演示").disabled(busy).on_click(cx.listener(|v,_,_,cx|v.activate_account(None,cx))))
                .child(Button::new("forget-account").small().ghost().label("删除凭据").disabled(busy).on_click(cx.listener(|v,_,w,cx|v.confirm_privacy(false,w,cx))))));
        let mut form=div().flex().flex_col().gap_3()
            .child(current)
            .child(div().font_medium().child("添加或更新账号"))
            .child(div().text_xs().text_color(p.muted).child("Token 仅用于识别账号，不保存；网页 Cookie 用于上传。"))
            .child(div().flex().flex_col().gap_1()
                .child(div().flex().items_center().justify_between()
                    .child(div().text_sm().child("个人访问 Token（可选）"))
                    .child(Button::new("open-token-settings").small().ghost().label("获取 Token").on_click(cx.listener(|_,_,_,cx|cx.open_url("https://www.yuque.com/settings/tokens")))))
                .child(div().flex().items_center().gap_2()
                    .child(div().flex_1().min_w(px(0.)).child(Input::new(&model.account_token).disabled(busy)))
                    .child(div().debug_selector(||"identify-account".into()).child(Button::new("identify-account").small().label(if self.account_busy(cx){"处理中…"}else{"识别账号"}).disabled(busy).on_click(cx.listener(|v,_,_,cx|v.identify_account(cx)))))))
            .when_some(model.identity.clone(),|d,info|d.child(div().px_3().py_2().rounded_md().bg(p.surface).flex().items_center().gap_3()
                .child(div().flex_1().truncate().font_medium().child(if info.name.is_empty(){"已识别个人账号".into()}else{info.name}))
                .child(div().text_xs().text_color(p.muted).child(format!("{} · ID {}",info.login,info.id)))))
            .when(!model.identity_status.is_empty(),|d|d.child(div().text_xs().text_color(p.muted).child(model.identity_status.clone())))
            .child(div().h(px(1.)).bg(p.line))
            .child(div().font_medium().child("网页上传会话"))
            .child(div().flex().flex_col().gap_1().child(div().text_xs().text_color(p.muted).child("Cookie（必填）")).child(Input::new(&model.account_cookie).disabled(busy)))
            .child(div().flex().flex_col().gap_1().child(div().text_xs().text_color(p.muted).child("CSRF Token（Cookie 含 yuque_ctoken 时自动提取）")).child(Input::new(&model.account_csrf).disabled(busy)))
            .child(div().flex().justify_end().child(div().debug_selector(||"save-account".into()).child(Button::new("save-account").small().primary().label("验证并保存账号").disabled(busy).on_click(cx.listener(|v,_,w,cx|v.save_credentials(w,cx))))))
            .child(div().h(px(1.)).bg(p.line))
            .child(div().font_medium().child("已保存账号"))
            .when(model.index_loading,|d|d.child(div().text_xs().text_color(p.muted).child("正在读取账号索引…")))
            .when(model.index_error,|d|d.child(Button::new("retry-account-index").small().ghost().label("账号索引读取失败 · 重试").on_click(cx.listener(|v,_,_,cx|v.account.update(cx,|a,cx|a.refresh_index(v.storage.clone(),cx))))))
            .when(!model.index_loading&&!model.index_error&&model.saved_accounts.is_empty(),|d|d.child(div().text_xs().text_color(p.muted).child("暂无已保存账号")));
        for owner in &self.account.read(cx).saved_accounts{
            let id=owner.clone();
            form=form.child(Button::new(SharedString::from(format!("account-{owner}"))).small().label(format!("账号 {owner}")).disabled(busy).on_click(cx.listener(move|v,_,_,cx|v.load_credentials(id.clone(),cx))));
        }
        form.child(div().text_xs().text_color(p.muted).child(self.status.clone()))
    }


}
