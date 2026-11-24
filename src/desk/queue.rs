use super::*;

impl UploadDesk {
    pub(super) fn refresh_jobs(&mut self, cx: &mut Context<Self>) {
        let storage = self.storage.clone();
        let provider = self.account.config.provider().to_string();
        let owner = self.account.config.owner_id().to_string();
        let before = self.queue.job_cursors.last().copied().flatten();
        self.queue.jobs_loading = true;
        self.queue.jobs_generation += 1;
        let generation = self.queue.jobs_generation;
        self.queue.jobs_task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { storage.jobs(&provider, &owner, before) }).await;
            _ = this.update(cx, |view, cx| {
                if generation != view.queue.jobs_generation { return; }
                view.queue.jobs_loading = false;
                match result {
                    Ok(mut jobs) => {
                        view.queue.jobs_more = jobs.len() > 50;
                        jobs.truncate(50);
                        view.queue.jobs = jobs;
                        view.queue.jobs_error = None;
                    }
                    Err(e) => { view.queue.jobs_error = Some(e.to_string()); view.queue.jobs.clear(); view.queue.jobs_more = false; }
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn retry_one(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.backup_busy || self.queue.busy || self.account.account_busy { return; }
        match self.storage.retry_job(self.account.config.provider(), self.account.config.owner_id(), id) {
            Ok(Some(job)) => self.run_jobs(vec![job], cx),
            Ok(None) => { self.status = "该任务已完成或不属于当前账号".into(); cx.notify(); }
            Err(e) => { self.status = e.to_string(); cx.notify(); }
        }
    }

    pub(super) fn acknowledge_job(&mut self, id:i64,cx:&mut Context<Self>) {
        if self.backup_busy || self.queue.busy || self.account.account_busy {return;}
        match self.storage.acknowledge_unknown(self.account.config.provider(),self.account.config.owner_id(),id) {
            Ok(()) => {self.status="已记录核实结果；如需重新上传，请单独重试".into();
                self.queue.recovery_count=self.storage.recoverable(self.account.config.provider(),self.account.config.owner_id()).map(|j|j.len()).unwrap_or(0);
                self.refresh_jobs(cx);}
            Err(e)=>self.status=e.to_string()
        }
        cx.notify();
    }

    pub(super) fn paste_files(&mut self, cx: &mut Context<Self>) {
        if self.backup_busy || self.queue.busy || self.account.account_busy { return; }
        let Some(item) = cx.read_from_clipboard() else {self.status="剪贴板没有可读取内容".into();cx.notify();return;};
        self.paste_item(item,cx);
    }
    pub(super) fn paste_item(&mut self,item:ClipboardItem,cx:&mut Context<Self>){
        if self.backup_busy || self.queue.busy || self.account.account_busy {return;}
        let mut files = Vec::new();
        for entry in item.entries() {
            match entry {
                ClipboardEntry::ExternalPaths(paths) => files.extend_from_slice(paths.paths()),
                ClipboardEntry::Image(image) => {
                    if image.bytes().len() as u64 > self.account.config.max_file_mb * 1024 * 1024 { continue; }
                    let result = (|| -> Result<PathBuf> {
                        use sha2::{Digest, Sha256};
                        let dir = self.storage.data_dir().join("clipboard");
                        std::fs::create_dir_all(&dir)?;
                        let path = dir.join(format!("{}.{}", hex::encode(Sha256::digest(image.bytes())), image.format().extension()));
                        // Keep a stable local source for Demo links and interrupted tasks.
                        if !path.exists() {
                            let temp=path.with_extension(format!("{}.tmp",std::process::id()));
                            let result=(||->Result<()>{
                                let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
                                use std::io::Write;
                                file.write_all(image.bytes())?;file.sync_all()?;drop(file);
                                std::fs::rename(&temp,&path)?;Ok(())
                            })();
                            if result.is_err(){let _=std::fs::remove_file(&temp);}
                            result?;
                        }
                        Ok(path)
                    })();
                    match result {
                        Ok(path) => files.push(path),
                        Err(_) => { self.status = "保存剪贴板图片失败".into(); cx.notify(); return; }
                    }
                }
                _ => {}
            }
        }
        if files.is_empty() { self.status = "剪贴板没有可上传的文件或图片".into(); cx.notify(); }
        else { self.start_upload(files, cx); }
    }

    pub(super) fn resume_jobs(&mut self, cx: &mut Context<Self>) {
        if self.backup_busy || self.queue.busy || self.account.account_busy { return; }
        match self.storage.recoverable(self.account.config.provider(), self.account.config.owner_id()) {
            Ok(jobs) => self.run_jobs(jobs, cx),
            Err(error) => { self.status = format!("读取任务失败：{error}"); cx.notify(); }
        }
    }

    pub(super) fn select_files(&mut self, cx: &mut Context<Self>) {
        if self.backup_busy || self.queue.busy || self.account.account_busy {
            self.status = "正在上传，请等待当前队列完成".to_string();
            cx.notify();
            return;
        }
        if self.account.config.mode == Mode::Yuque && self.account.config.creds.is_none() {
            self.status = "请先配置语雀会话再上传".into();
            cx.notify();
            return;
        }
        let Some(files) = rfd::FileDialog::new()
            .set_title("选择图片或附件")
            .pick_files() else { return; };
        self.start_upload(files, cx);
    }

    pub(super) fn start_upload(&mut self, files: Vec<PathBuf>, cx: &mut Context<Self>) {
        if self.backup_busy || self.queue.busy || self.account.account_busy || files.is_empty() { return; }
        if self.account.config.mode == Mode::Yuque && self.account.config.creds.is_none() {
            self.status = "请先配置语雀会话再上传".into(); cx.notify(); return;
        }
        match self.storage.enqueue(self.account.config.provider(), self.account.config.owner_id(), &files) {
            Ok(jobs) => self.run_jobs(jobs, cx),
            Err(error) => { self.status = format!("保存任务失败：{error}"); cx.notify(); }
        }
    }

    pub(super) fn run_jobs(&mut self, jobs: Vec<(i64, PathBuf)>, cx: &mut Context<Self>) {
        if self.backup_busy || self.queue.busy || jobs.is_empty() || self.account.account_busy { return; }
        if self.account.config.mode == Mode::Yuque && self.account.config.creds.is_none() { return; }
        let control = Arc::new(AtomicU8::new(0));
        self.queue.queue_control = control.clone();
        let config = self.account.config.clone();
        let storage = self.storage.clone();
        self.queue.busy = true;
        self.queue.job_cursors = vec![None];
        self.refresh_jobs(cx);
        self.queue.transfer_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(120)).await;
                let result = this.update(cx, |view, cx| { cx.notify(); view.queue.busy });
                if !matches!(result, Ok(true)) { break; }
            }
        }));
        self.queue.queue_count = jobs.len();
        self.queue.queue_total = jobs.len();
        self.queue.succeeded = 0;
        self.queue.duplicates = 0;
        self.queue.failed.clear();
        self.status = format!("已加入 {} 个文件", jobs.len());
        cx.notify();

        // The view owns the task. File hashing, uploads and SQLite writes
        // run off the UI thread. Work is sequential to bound peak memory.
        self.queue.upload_task = Some(cx.spawn(async move |this, cx| {
            let total = jobs.len();
            let interval_ms = std::env::var("YUQUE_UPLOAD_INTERVAL_MS").ok().and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(350).min(30_000);
            for (index, (job_id, path)) in jobs.into_iter().enumerate() {
                if index > 0 && config.mode == Mode::Yuque && control.load(Ordering::Relaxed) == 0 {
                    cx.background_executor().timer(std::time::Duration::from_millis(interval_ms)).await;
                }
                let command = control.load(Ordering::Relaxed);
                if command != 0 {
                    let storage = storage.clone();
                    let state = if command == 2 { "cancelled" } else { "paused" };
                    let result = cx.background_spawn(async move { storage.job_state(job_id, state, "") }).await;
                    _ = this.update(cx, |view, cx| {
                        view.queue.queue_count = total - index - 1;
                        view.refresh_jobs(cx);
                        if result.is_err() {
                            view.status = "任务状态保存失败，请在任务中心核实".into();
                        }
                        if view.queue.queue_count == 0 {
                            view.queue.active_job = None;
                            view.queue.busy = false;
                            view.queue.recovery_count = view.storage.recoverable(view.account.config.provider(), view.account.config.owner_id()).map(|jobs| jobs.len()).unwrap_or(0);
                            view.status = if result.is_err() { "任务状态保存失败，请检查恢复队列".into() }
                                else if command == 2 { "已取消尚未开始的任务".into() } else { "队列已暂停，可手动恢复".into() };
                        }
                        cx.notify();
                    });
                    continue;
                }
                let config = config.clone();
                let storage = storage.clone();
                let upload_path = path.clone();
                let progress = Arc::new(uploader::TransferProgress::default());
                _ = this.update(cx, |view, cx| {
                    view.queue.active_job = Some(job_id);
                    view.queue.transfer = progress.clone();
                    cx.notify();
                });
                let result = cx.background_spawn(async move {
                    storage.job_state(job_id, "running", "").map_err(|error| (error.to_string(),None))?;
                    let result = uploader::upload_with_progress(&upload_path, &config, &storage, progress.clone())
                        .map(|out| (out.duplicate, out.item.provider))
                        .map_err(|error| error.to_string());
                    let (state, error) = match &result {
                        Ok(_) => ("done", ""),
                        Err(error) => (uploader::failure_state(&progress), error.as_str()),
                    };
                    storage.job_state(job_id, state, error).map_err(|_| ("远端结果待核实：任务结果保存失败".to_string(),Some(uploader::QueueStop::Unknown)))?;
                    if state == "unknown" {
                        return Err((format!("远端结果待核实：{error}"),Some(uploader::QueueStop::Unknown)));
                    }
                    result.map_err(|error|(error,uploader::queue_stop(&progress)))
                }).await;
                let stop=result.as_ref().err().and_then(|(_,reason)|*reason);
                if stop.is_some() {
                    let _ = control.compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed);
                }
                _ = this.update(cx, |view, cx| {
                    view.queue.queue_count = total - index - 1;
                    let filename = path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default();
                    let message = match result {
                        Ok((true, _)) => {
                            view.queue.duplicates += 1;
                            "复用历史链接".to_string()
                        }
                        Ok((false, provider)) => {
                            view.queue.succeeded += 1;
                            if provider == "demo" { "已保存本地记录".into() } else { "上传成功".into() }
                        }
                        Err((error,reason)) => {
                            if reason==Some(uploader::QueueStop::Authorization){
                                view.account.session_status="上传未获授权：请在设置检查会话及 CSRF Token，再手动恢复任务".into();
                            }
                            view.queue.failed.push((path.clone(), error.clone()));
                            format!("失败：{error}")
                        }
                    };
                    view.status = format!("[{}/{}] {filename} · {message}", index + 1, total);
                    if view.queue.queue_count == 0 {
                        view.queue.busy = false;
                        view.queue.recovery_count = view.storage.recoverable(view.account.config.provider(), view.account.config.owner_id()).map(|jobs| jobs.len()).unwrap_or(0);
                        view.status = format!("本批完成：新增 {} · 复用 {} · 失败 {}",
                            view.queue.succeeded, view.queue.duplicates, view.queue.failed.len());
                    }
                    if view.queue.queue_count==0 && view.desktop_enabled {
                        cx.show_system_notification(SystemNotification{tag:"yuque-upload-batch".into(),title:"Yuque Asset Desk".into(),body:format!("本批结束：新增 {} · 复用 {} · 失败 {}",view.queue.succeeded,view.queue.duplicates,view.queue.failed.len()).into(),actions:vec![]});
                    }
                    view.queue.active_job = None;
                    view.refresh_jobs(cx);
                    view.reload(cx);
                    cx.notify();
                });
            }
        }));
    }
}
