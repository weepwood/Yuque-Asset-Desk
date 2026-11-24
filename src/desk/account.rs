use super::*;

impl UploadDesk {
    pub(super) fn activate_account(&mut self, creds: Option<YuqueCredentials>, cx: &mut Context<Self>) {
        if self.queue.busy { return; }
        self.account.config.mode = if creds.is_some() { Mode::Yuque } else { Mode::Demo };
        self.account.config.creds = creds;
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
        if self.queue.busy || self.account.account_busy || self.account.config.mode != Mode::Yuque { return; }
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

    pub(super) fn save_credentials(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.queue.busy || self.account.account_busy { return; }
        let result = YuqueCredentials::validated(
            self.account.account_id.read(cx).value().trim().to_string(),
            self.account.account_csrf.read(cx).value().trim().to_string(),
            self.account.account_cookie.read(cx).value().trim().to_string());
        let creds = match result {
            Ok(creds) => creds,
            Err(error) => { self.status = error.to_string(); cx.notify(); return; }
        };
        let key = YuqueCredentials::vault_key(&creds.user_id);
        let task = cx.write_credentials(&key, &creds.user_id, &creds.encode());
        self.account.account_csrf.update(cx, |input, cx| input.set_value("", window, cx));
        self.account.account_cookie.update(cx, |input, cx| input.set_value("", window, cx));
        self.account.account_busy = true;
        self.account.account_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |view, cx| {
                view.account.account_busy = false;
                match result {
                    Ok(()) => {
                        match view.storage.save_account(&creds.user_id) {
                            Ok(()) => {
                                view.account.saved_accounts = view.storage.accounts().unwrap_or_default();
                                view.activate_account(Some(creds), cx);
                            }
                            Err(_) => view.status = "凭据已保存在系统密钥库，但账号索引写入失败".into(),
                        }
                    }
                    Err(_) => view.status = "系统密钥库写入失败；未使用明文存储".into(),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn load_credentials(&mut self, owner: String, cx: &mut Context<Self>) {
        if self.queue.busy || self.account.account_busy { return; }
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

    pub(super) fn forget_account(&mut self, cx: &mut Context<Self>) {
        if self.queue.busy || self.account.account_busy || self.account.config.mode != Mode::Yuque { return; }
        let owner = self.account.config.owner_id().to_string();
        let task = cx.delete_credentials(&YuqueCredentials::vault_key(&owner));
        self.account.account_busy = true;
        self.account.account_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |view, cx| {
                view.account.account_busy = false;
                if result.is_ok() {
                    if view.storage.forget_account(&owner).is_ok() {
                        view.account.saved_accounts = view.storage.accounts().unwrap_or_default();
                    }
                    view.activate_account(None, cx);
                    view.status = "已删除系统凭据，资源历史仍保留".into();
                } else { view.status = "系统凭据删除失败".into(); }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
