#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod format;
mod storage;
mod uploader;
mod preview;

use anyhow::{Context as AnyhowContext, Result};
use config::{Config, Mode, YuqueCredentials};
use gpui_kit::component::theme::{Theme, ThemeMode};
use std::sync::{Arc, atomic::{AtomicU8, Ordering}};
use directories::ProjectDirs;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
};
use gpui_kit::*;
use gpui_kit::base::{Disableable, Selectable, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use std::path::PathBuf;
use storage::{Asset, Storage};

fn db_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("YUQUE_DATA_DIR") {
        return Ok(PathBuf::from(path).join("assets.db"));
    }
    let dirs = ProjectDirs::from("com", "Weepwood", "YuqueAssetDesk")
        .context("无法找到应用数据目录")?;
    Ok(dirs.data_local_dir().join("assets.db"))
}

fn file_size(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else if b >= 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else {
        format!("{b:.0} B")
    }
}

const PAGE_SIZE: usize = 50;

struct UploadDesk {
    config: Config,
    storage: Storage,
    assets: Vec<Asset>,
    search: String,
    dark: bool,
    grid_view: bool,
    selected_asset: Option<Asset>,
    oldest: bool,
    selected_ids: std::collections::HashSet<i64>,
    tag_input: Entity<InputState>,
    thumbnail_path: Option<PathBuf>,
    cached_thumbnails: std::collections::HashMap<i64, PathBuf>,
    thumbnails_task: Option<Task<()>>,
    preview_task: Option<Task<()>>,
    preview_loading: bool,
    preview_error: Option<String>,
    account_panel: bool,
    account_id: Entity<InputState>,
    account_csrf: Entity<InputState>,
    account_cookie: Entity<InputState>,
    account_busy: bool,
    saved_accounts: Vec<String>,
    account_task: Option<Task<()>>,
    queue_control: Arc<AtomicU8>,
    recovery_count: usize,
    search_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    image_only: bool,
    all_accounts: bool,
    page_cursors: Vec<Option<i64>>,
    has_more: bool,
    loading_history: bool,
    history_error: Option<String>,
    history_generation: u64,
    history_task: Option<Task<()>>,
    busy: bool,
    queue_count: usize,
    queue_total: usize,
    succeeded: usize,
    duplicates: usize,
    failed: Vec<(PathBuf, String)>,
    total_count: i64,
    status: String,
    upload_task: Option<Task<()>>,
}

impl UploadDesk {
    fn new(window: &mut Window, cx: &mut Context<Self>, config: Config, storage: Storage) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索文件名或标签…"));
        let subscription = cx.subscribe_in(
            &search_input,
            window,
            |this, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.search = input.read(cx).value().to_string();
                    this.reset_history(cx);
                    cx.notify();
                }
            },
        );
        let status = match (&config.mode, &config.creds) {
            (Mode::Demo, _) => "本地演示模式：不会上传至网络".to_string(),
            (Mode::Yuque, None) => "语雀未配置：请设置用户 ID、Cookie 和 CSRF Token".to_string(),
            (Mode::Yuque, Some(_)) => "语雀会话已配置，等待上传文件".to_string(),
        };
        let saved_accounts = storage.accounts().unwrap_or_default();
        let recovery_count = storage.recoverable(config.provider(), config.owner_id()).map(|jobs| jobs.len()).unwrap_or(0);
        let mut view = Self {
            dark: true,
            grid_view: false,
            selected_asset: None,
            oldest: false,
            selected_ids: Default::default(),
            tag_input: cx.new(|cx| InputState::new(window, cx).placeholder("标签，用逗号分隔")),
            thumbnail_path: None,
            cached_thumbnails: Default::default(),
            thumbnails_task: None,
            preview_task: None,
            preview_loading: false,
            preview_error: None,
            account_panel: false,
            account_id: cx.new(|cx| InputState::new(window, cx).placeholder("语雀数字用户 ID")),
            account_csrf: cx.new(|cx| InputState::new(window, cx).placeholder("CSRF Token").masked(true)),
            account_cookie: cx.new(|cx| InputState::new(window, cx).placeholder("Cookie").masked(true)),
            account_busy: false,
            saved_accounts,
            account_task: None,
            queue_control: Arc::new(AtomicU8::new(0)),
            recovery_count,
            config,
            storage,
            assets: Vec::new(),
            search: String::new(),
            search_input,
            _subscriptions: vec![subscription],
            image_only: false,
            all_accounts: false,
            page_cursors: vec![None],
            has_more: false,
            loading_history: false,
            history_error: None,
            history_generation: 0,
            history_task: None,
            busy: false,
            queue_count: 0,
            queue_total: 0,
            succeeded: 0,
            duplicates: 0,
            failed: Vec::new(),
            total_count: 0,
            status,
            upload_task: None,
        };
        view.reload(cx);
        view
    }

    fn activate_account(&mut self, creds: Option<YuqueCredentials>, cx: &mut Context<Self>) {
        if self.busy { return; }
        self.config.mode = if creds.is_some() { Mode::Yuque } else { Mode::Demo };
        self.config.creds = creds;
        self.all_accounts = false;
        self.total_count = 0;
        self.failed.clear();
        self.queue_total = 0;
        self.queue_count = 0;
        self.succeeded = 0;
        self.duplicates = 0;
        self.recovery_count = self.storage.recoverable(self.config.provider(), self.config.owner_id()).map(|jobs| jobs.len()).unwrap_or(0);
        self.status = if self.config.mode == Mode::Demo { "已切换到本地演示".into() }
            else { "凭据已加载，实际登录状态尚未验证".into() };
        self.reset_history(cx);
    }

    fn save_credentials(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.account_busy { return; }
        let result = YuqueCredentials::validated(
            self.account_id.read(cx).value().trim().to_string(),
            self.account_csrf.read(cx).value().trim().to_string(),
            self.account_cookie.read(cx).value().trim().to_string());
        let creds = match result {
            Ok(creds) => creds,
            Err(error) => { self.status = error.to_string(); cx.notify(); return; }
        };
        let key = YuqueCredentials::vault_key(&creds.user_id);
        let task = cx.write_credentials(&key, &creds.user_id, &creds.encode());
        self.account_csrf.update(cx, |input, cx| input.set_value("", window, cx));
        self.account_cookie.update(cx, |input, cx| input.set_value("", window, cx));
        self.account_busy = true;
        self.account_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |view, cx| {
                view.account_busy = false;
                match result {
                    Ok(()) => {
                        match view.storage.save_account(&creds.user_id) {
                            Ok(()) => {
                                view.saved_accounts = view.storage.accounts().unwrap_or_default();
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

    fn load_credentials(&mut self, owner: String, cx: &mut Context<Self>) {
        if self.busy || self.account_busy { return; }
        let task = cx.read_credentials(&YuqueCredentials::vault_key(&owner));
        self.account_busy = true;
        self.account_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |view, cx| {
                view.account_busy = false;
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

    fn paste_files(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.account_busy { return; }
        let Some(item) = cx.read_from_clipboard() else { return; };
        let mut files = Vec::new();
        for entry in item.entries() {
            match entry {
                ClipboardEntry::ExternalPaths(paths) => files.extend_from_slice(paths.paths()),
                ClipboardEntry::Image(image) => {
                    if image.bytes().len() as u64 > self.config.max_file_mb * 1024 * 1024 { continue; }
                    let result = (|| -> Result<PathBuf> {
                        use sha2::{Digest, Sha256};
                        let dir = db_path()?.parent().context("缺少数据目录")?.join("clipboard");
                        std::fs::create_dir_all(&dir)?;
                        let path = dir.join(format!("{}.{}", hex::encode(Sha256::digest(image.bytes())), image.format().extension()));
                        // Keep a stable local source for Demo links and interrupted tasks.
                        if !path.exists() {
                            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
                            use std::io::Write;
                            file.write_all(image.bytes())?;
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

    fn resume_jobs(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.account_busy { return; }
        match self.storage.recoverable(self.config.provider(), self.config.owner_id()) {
            Ok(jobs) => self.run_jobs(jobs, cx),
            Err(error) => { self.status = format!("读取任务失败：{error}"); cx.notify(); }
        }
    }

    fn reset_history(&mut self, cx: &mut Context<Self>) {
        self.page_cursors = vec![None];
        self.selected_asset = None;
        self.selected_ids.clear();
        self.thumbnail_path = None;
        self.cached_thumbnails.clear();
        self.thumbnails_task = None;
        self.preview_task = None;
        self.preview_loading = false;
        self.preview_error = None;
        self.assets.clear();
        self.has_more = false;
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let storage = self.storage.clone();
        let query = self.search.clone();
        let image_only = self.image_only;
        let oldest = self.oldest;
        let scope = (!self.all_accounts).then(|| (
            self.config.provider().to_string(), self.config.owner_id().to_string()
        ));
        let before = self.page_cursors.last().copied().flatten();
        self.history_generation += 1;
        let generation = self.history_generation;
        self.loading_history = true;
        self.history_error = None;
        self.history_task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move {
                storage.page_sorted(PAGE_SIZE, &query, image_only,
                    scope.as_ref().map(|(provider, owner)| (provider.as_str(), owner.as_str())), before, oldest)
            }).await;
            _ = this.update(cx, |view, cx| {
                if generation != view.history_generation { return; }
                view.loading_history = false;
                match result {
                    Ok((items, more, total)) => {
                        view.selected_ids.retain(|id| items.iter().any(|a| a.id == *id));
                        view.assets = items;
                        view.has_more = more;
                        view.total_count = total;
                        view.refresh_thumbnails(cx);
                    }
                    Err(error) => {
                        view.assets.clear();
                        view.has_more = false;
                        view.history_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn select_files(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.account_busy {
            self.status = "正在上传，请等待当前队列完成".to_string();
            cx.notify();
            return;
        }
        if self.config.mode == Mode::Yuque && self.config.creds.is_none() {
            self.status = "请先配置语雀会话再上传".into();
            cx.notify();
            return;
        }
        let Some(files) = rfd::FileDialog::new()
            .set_title("选择图片或附件")
            .pick_files() else { return; };
        self.start_upload(files, cx);
    }

    fn start_upload(&mut self, files: Vec<PathBuf>, cx: &mut Context<Self>) {
        if self.busy || self.account_busy || files.is_empty() { return; }
        if self.config.mode == Mode::Yuque && self.config.creds.is_none() {
            self.status = "请先配置语雀会话再上传".into(); cx.notify(); return;
        }
        match self.storage.enqueue(self.config.provider(), self.config.owner_id(), &files) {
            Ok(jobs) => self.run_jobs(jobs, cx),
            Err(error) => { self.status = format!("保存任务失败：{error}"); cx.notify(); }
        }
    }

    fn run_jobs(&mut self, jobs: Vec<(i64, PathBuf)>, cx: &mut Context<Self>) {
        if self.busy || jobs.is_empty() || self.account_busy { return; }
        if self.config.mode == Mode::Yuque && self.config.creds.is_none() { return; }
        let control = Arc::new(AtomicU8::new(0));
        self.queue_control = control.clone();
        let config = self.config.clone();
        let storage = self.storage.clone();
        self.busy = true;
        self.queue_count = jobs.len();
        self.queue_total = jobs.len();
        self.succeeded = 0;
        self.duplicates = 0;
        self.failed.clear();
        self.status = format!("已加入 {} 个文件", jobs.len());
        cx.notify();

        // The view owns the task. File hashing, uploads and SQLite writes
        // run off the UI thread. Work is sequential to bound peak memory.
        self.upload_task = Some(cx.spawn(async move |this, cx| {
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
                        view.queue_count = total - index - 1;
                        if view.queue_count == 0 {
                            view.busy = false;
                            view.recovery_count = view.storage.recoverable(view.config.provider(), view.config.owner_id()).map(|jobs| jobs.len()).unwrap_or(0);
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
                let result = cx.background_spawn(async move {
                    storage.job_state(job_id, "running", "").map_err(|error| error.to_string())?;
                    let result = uploader::upload(&upload_path, &config, &storage)
                        .map(|out| (out.duplicate, out.item.provider))
                        .map_err(|error| error.to_string());
                    let (state, error) = match &result {
                        Ok(_) => ("done", ""),
                        Err(error) => ("failed", error.as_str()),
                    };
                    storage.job_state(job_id, state, error).map_err(|_| "任务结果保存失败，请检查资源历史后再恢复".to_string())?;
                    result
                }).await;
                if result.as_ref().err().is_some_and(|e| e.contains("HTTP 429") || e.contains("HTTP 401") || e.contains("HTTP 403")) {
                    let _ = control.compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed);
                }
                _ = this.update(cx, |view, cx| {
                    view.queue_count = total - index - 1;
                    let filename = path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default();
                    let message = match result {
                        Ok((true, _)) => {
                            view.duplicates += 1;
                            "复用历史链接".to_string()
                        }
                        Ok((false, provider)) => {
                            view.succeeded += 1;
                            if provider == "demo" { "已保存本地记录".into() } else { "上传成功".into() }
                        }
                        Err(error) => {
                            view.failed.push((path.clone(), error.clone()));
                            format!("失败：{error}")
                        }
                    };
                    view.status = format!("[{}/{}] {filename} · {message}", index + 1, total);
                    if view.queue_count == 0 {
                        view.busy = false;
                        view.recovery_count = view.storage.recoverable(view.config.provider(), view.config.owner_id()).map(|jobs| jobs.len()).unwrap_or(0);
                        view.status = format!("本批完成：新增 {} · 复用 {} · 失败 {}",
                            view.succeeded, view.duplicates, view.failed.len());
                    }
                    view.reload(cx);
                    cx.notify();
                });
            }
        }));
    }

    fn refresh_thumbnails(&mut self, cx: &mut Context<Self>) {
        self.cached_thumbnails.clear();
        let generation = self.history_generation;
        let items = self.assets.clone();
        let Ok(path) = db_path() else { return; };
        let Some(parent) = path.parent() else { return; };
        let cache = parent.join("thumbnails");
        let generate_local = self.grid_view;
        self.thumbnails_task = Some(cx.spawn(async move |this, cx| {
            for asset in items.into_iter().filter(|a| a.category == "image") {
                let id = asset.id;
                let cache = cache.clone();
                let path = cx.background_spawn(async move {
                    let path = preview::cached_path(&cache, &asset);
                    if path.is_file() { Some(path) }
                    else if generate_local && asset.provider == "demo" {
                        preview::thumbnail(&cache, &asset).ok()
                    } else { None }
                }).await;
                let result = this.update(cx, |view, cx| {
                    if generation != view.history_generation { return false; }
                    if let Some(path) = path { view.cached_thumbnails.insert(id, path); cx.notify(); }
                    true
                });
                if !matches!(result, Ok(true)) { break; }
            }
        }));
    }

    fn select_asset(&mut self, asset: Asset, window: &mut Window, cx: &mut Context<Self>) {
        let tags = self.storage.tags(asset.id).unwrap_or_default().join(", ");
        self.tag_input.update(cx, |input, cx| input.set_value(tags, window, cx));
        self.thumbnail_path = None;
        self.preview_task = None;
        self.preview_loading = false;
        self.preview_error = None;
        self.selected_asset = Some(asset.clone());
        self.account_panel = false;
        if asset.category == "image" && asset.provider == "demo" { self.load_preview(cx); }
        cx.notify();
    }

    fn load_preview(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.selected_asset.clone() else { return; };
        if asset.category != "image" || self.preview_loading { return; }
        let Ok(path) = db_path() else { return; };
        let Some(parent) = path.parent() else { return; };
        let cache = parent.join("thumbnails");
        let id = asset.id;
        self.preview_loading = true;
        self.preview_error = None;
        self.preview_task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move {
                preview::thumbnail(&cache, &asset).map_err(|e| e.to_string())
            }).await;
            _ = this.update(cx, |view, cx| {
                if view.selected_asset.as_ref().is_none_or(|a| a.id != id) { return; }
                view.preview_loading = false;
                match result {
                    Ok(path) => {
                        view.cached_thumbnails.insert(id, path.clone());
                        view.thumbnail_path = Some(path);
                    },
                    Err(error) => view.preview_error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn copy_selected(&mut self, cx: &mut Context<Self>) {
        let links = self.assets.iter().filter(|a| self.selected_ids.contains(&a.id))
            .map(|a| format::markdown(&a.name, &a.url, a.category == "image")).collect::<Vec<_>>();
        if links.is_empty() { return; }
        cx.write_to_clipboard(ClipboardItem::new_string(links.join("\n")));
        self.status = format!("已复制 {} 条 Markdown 引用", links.len());
        cx.notify();
    }

    fn forget_account(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.account_busy || self.config.mode != Mode::Yuque { return; }
        let owner = self.config.owner_id().to_string();
        let task = cx.delete_credentials(&YuqueCredentials::vault_key(&owner));
        self.account_busy = true;
        self.account_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |view, cx| {
                view.account_busy = false;
                if result.is_ok() {
                    if view.storage.forget_account(&owner).is_ok() {
                        view.saved_accounts = view.storage.accounts().unwrap_or_default();
                    }
                    view.activate_account(None, cx);
                    view.status = "已删除系统凭据，资源历史仍保留".into();
                } else { view.status = "系统凭据删除失败".into(); }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn copy_asset(&mut self, asset: &Asset, as_markdown: bool, cx: &mut Context<Self>) {
        let content = if as_markdown {
            format::markdown(&asset.name, &asset.url, asset.category == "image")
        } else {
            asset.url.clone()
        };
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(content)) {
            Ok(_) => self.status = if as_markdown {
                "Markdown 引用已复制".to_string()
            } else {
                "资源链接已复制".to_string()
            },
            Err(error) => self.status = format!("写入剪贴板失败：{error}"),
        }
        cx.notify();
    }

    fn color(&self, dark: u32, light: u32) -> Rgba {
        rgb(if self.dark { dark } else { light })
    }

    fn status_panel(&self) -> impl IntoElement {
        div().flex().flex_col().gap_2().rounded_lg().p_3()
            .bg(self.color(0x25272a, 0xe8eaed))
            .child("当前上传目标")
            .child(format!("{} / {}", self.config.provider(), self.config.owner_id()))
            .child(format!("待处理：{}", self.queue_count))
    }

    fn inspector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div().flex().flex_col().gap_3().p_4().w(px(240.)).flex_shrink_0()
            .bg(self.color(0x1c1e21, 0xf7f8fa))
            .border_l_1().border_color(self.color(0x34363a, 0xdfe2e6));
        if self.account_panel {
            panel = panel.child(div().font_semibold().child("账号管理"))
                .child(div().text_sm().child("仅保存到系统密钥库。凭据格式有效不代表已登录。"))
                .child(Input::new(&self.account_id).disabled(self.account_busy || self.busy))
                .child(Input::new(&self.account_csrf).disabled(self.account_busy || self.busy))
                .child(Input::new(&self.account_cookie).disabled(self.account_busy || self.busy))
                .child(Button::new("save-account").primary().label("保存并切换")
                    .disabled(self.account_busy || self.busy)
                    .on_click(cx.listener(|view, _, window, cx| view.save_credentials(window, cx))))
                .child(Button::new("demo-account").label("本地演示")
                    .disabled(self.account_busy || self.busy)
                    .on_click(cx.listener(|view, _, _, cx| view.activate_account(None, cx))))
                .when(self.config.mode == Mode::Yuque, |d| d.child(
                    Button::new("forget-account").label("删除当前保存凭据")
                        .disabled(self.account_busy || self.busy)
                        .on_click(cx.listener(|view, _, _, cx| view.forget_account(cx)))))
                .child(div().font_semibold().child("已保存账号"));
            for owner in &self.saved_accounts {
                let id = owner.clone();
                panel = panel.child(Button::new(SharedString::from(format!("account-{owner}")))
                    .label(format!("账号 {owner}"))
                    .disabled(self.account_busy || self.busy)
                    .on_click(cx.listener(move |view, _, _, cx| view.load_credentials(id.clone(), cx))));
            }
        } else if let Some(asset) = &self.selected_asset {
            let url_asset = asset.clone();
            let md_asset = asset.clone();
            panel = panel.child(div().font_semibold().child("资源详情"))
                .child(div().font_semibold().child(asset.name.clone()))
                .child(format!("{} · {}", asset.category, file_size(asset.bytes)))
                .child(format!("来源：{} / {}", asset.provider, asset.owner_id))
                .child(div().text_sm().child(asset.created_at.clone()))
                .child(div().text_sm().truncate().child(format!("SHA-256: {}", asset.sha256)))
                .child(div().text_sm().truncate().child(format!("File key: {}", asset.filekey)))
                .child(div().text_sm().truncate().child(asset.url.clone()))
                .child(Button::new("detail-url").primary().label("复制 URL")
                    .on_click(cx.listener(move |view, _, _, cx| view.copy_asset(&url_asset, false, cx))))
                .child(Button::new("detail-md").label("复制 Markdown")
                    .on_click(cx.listener(move |view, _, _, cx| view.copy_asset(&md_asset, true, cx))));
            panel = panel.child(Input::new(&self.tag_input))
                .child(Button::new("save-tags").label("保存标签")
                    .on_click(cx.listener(|view, _, _, cx| {
                        if let Some(asset) = &view.selected_asset {
                            let text = view.tag_input.read(cx).value().to_string();
                            view.status = match view.storage.set_tags(asset.id, &text) {
                                Ok(()) => "标签已保存".into(), Err(e) => e.to_string(),
                            };
                            view.reload(cx);
                        }
                        cx.notify();
                    })));
            if asset.category == "image" {
                if let Some(path) = &self.thumbnail_path {
                    panel = panel.child(img(path.clone()).w_full().h(px(160.)).object_fit(ObjectFit::Contain));
                } else {
                    panel = panel.child(Button::new("preview-image")
                        .label(if self.preview_loading { "正在生成缩略图…" } else { "加载图片预览" })
                        .disabled(self.preview_loading)
                        .on_click(cx.listener(|view, _, _, cx| view.load_preview(cx))));
                    if let Some(error) = &self.preview_error { panel = panel.child(div().text_sm().child(error.clone())); }
                }
            }
        } else {
            panel = panel.child(div().font_semibold().child("资源详情"))
                .child(div().text_sm().child("选择列表或网格中的资源，查看属性及复制链接。"));
        }
        panel
    }

}

impl Render for UploadDesk {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Event-driven, short transition. Respects the OS reduced-motion preference.
        let feedback_opacity = gpui_kit::base::transition(
            ("upload-feedback", "opacity"),
            if self.busy { 1.0f32 } else { 0.82f32 },
            gpui_kit::base::Transition::new(std::time::Duration::from_millis(160)),
            window,
            cx,
        );
        // Filtering happens in SQLite before LIMIT, so older matching files are searchable.
        let visible: Vec<Asset> = self.assets.clone();
        let rows: Vec<AnyElement> = visible
            .into_iter()
            .map(|asset| {
                let index = asset.id as usize;
                let url_asset = asset.clone();
                let markdown_asset = asset.clone();
                let detail_asset = asset.clone();
                let selected = self.selected_asset.as_ref().is_some_and(|a| a.id == asset.id);
                let checked = self.selected_ids.contains(&asset.id);
                let asset_id = asset.id;
                if self.grid_view {
                    let thumbnail = self.cached_thumbnails.get(&asset.id).cloned();
                    return div().flex().flex_col().gap_2().p_3().w(px(176.)).h(px(200.))
                        .rounded_lg().border_1().border_color(self.color(0x34363a, 0xdfe2e6))
                        .bg(self.color(0x25272a, 0xffffff))
                        .child({
                            let d = div().h(px(90.)).flex().items_center().justify_center()
                                .bg(self.color(0x303338, 0xeef0f3)).rounded_md();
                            match thumbnail {
                                Some(path) => d.child(img(path).w_full().h_full().object_fit(ObjectFit::Contain)),
                                None => d.child(if asset.category == "image" { "IMG" } else { "FILE" }),
                            }
                        })
                        .child(Button::new(("grid-select", index)).selected(checked)
                            .label(if checked { "已选" } else { "选择" })
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if !view.selected_ids.remove(&asset_id) { view.selected_ids.insert(asset_id); } cx.notify();
                            })))
                        .child(div().text_sm().truncate().child(asset.name.clone()))
                        .child(Button::new(("grid-detail", index)).selected(selected).label("查看详情")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.select_asset(detail_asset.clone(), window, cx);
                            })))
                        .into_any_element();
                }
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .p_3()
                    .rounded_lg()
                    .bg(self.color(0x242629, 0xffffff))
                    .hover(|style| style.bg(self.color(0x303338, 0xeef0f3)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .flex_1()
                            .child(div().font_semibold().text_sm().truncate().child(asset.name.clone()))
                            .child(div().text_sm().text_color(self.color(0xa6a9b0, 0x646a73)).truncate().child(format!(
                                "{} · {} / {} · {}",
                                file_size(asset.bytes), asset.provider, asset.owner_id, asset.created_at
                            ))),
                    )
                    .child(Button::new(("row-select", index)).selected(checked)
                        .label(if checked { "已选" } else { "选择" })
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if !view.selected_ids.remove(&asset_id) { view.selected_ids.insert(asset_id); } cx.notify();
                        })))
                    .child(Button::new(("detail", index)).selected(selected).label("详情")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.select_asset(detail_asset.clone(), window, cx);
                        })))
                    .child(
                        Button::new(("url", index))
                            .label("复制 URL")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.copy_asset(&url_asset, false, cx);
                            })),
                    )
                    .child(
                        Button::new(("markdown", index))
                            .label("Markdown")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.copy_asset(&markdown_asset, true, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        // Never access SQLite from the hot render path.
        let total = self.total_count;
        let is_demo = self.config.mode == Mode::Demo;
        let failure_rows: Vec<_> = self.failed.iter().take(3).map(|(path, error)| {
            div().text_sm().text_color(self.color(0xf5b4a5, 0xa53222)).truncate().child(format!(
                "{}：{}", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(), error
            ))
        }).collect();

        div()
            .flex()
            .size_full()
            .bg(self.color(0x17191b, 0xf3f4f6))
            .text_color(self.color(0xebedf0, 0x24272c))
            .child(
                div()
                    .w(px(176.0)).flex_shrink_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_5()
                    .bg(self.color(0x1d1f22, 0xe9ebee))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_lg().font_bold().child("Yuque Asset Desk"))
                            .child(div().text_sm().text_color(self.color(0xa6a9b0, 0x666c75)).child("轻量文件上传工作台")),
                    )
                    .child(
                        Button::new("upload-navigation")
                            .selected(!self.image_only)
                            .label("上传工作台")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.image_only = false;
                                view.reset_history(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("images-navigation")
                            .selected(self.image_only)
                            .label("图片资源")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.image_only = true;
                                view.reset_history(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("choose-sidebar")
                            .disabled(self.busy || self.account_busy)
                            .label("选择本地文件")
                            .on_click(cx.listener(|view, _, _, cx| view.select_files(cx))),
                    )
                    .child(Button::new("account-nav").selected(self.account_panel).label("账号管理")
                        .on_click(cx.listener(|view, _, _, cx| { view.account_panel = !view.account_panel; cx.notify(); })))
                    .child(Button::new("theme-toggle").label(if self.dark { "浅色主题" } else { "深色主题" })
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.dark = !view.dark;
                            Theme::change(if view.dark { ThemeMode::Dark } else { ThemeMode::Light }, Some(window), cx);
                            cx.notify();
                        })))
                    .child(div().flex_1())
                    .child(self.status_panel()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(340.0))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_lg().font_semibold().child("资源工作台")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_3()
                            .rounded_lg()
                            .border_1().border_color(self.color(0x34363a, 0xdfe2e6))
                            .bg(self.color(0x23262a, 0xffffff))
                            .id("drop-zone")
                            .child(div().font_semibold().child("拖放文件到此处"))
                            .on_drop(cx.listener(|view, paths: &ExternalPaths, _, cx| view.start_upload(paths.paths().to_vec(), cx)))
                            .child(Button::new("paste").label("粘贴剪贴板图片或文件")
                                .disabled(self.busy || self.account_busy)
                                .on_click(cx.listener(|view, _, _, cx| view.paste_files(cx))))
                            .child(
                                Button::new("choose-main")
                                    .primary()
                                    .disabled(self.busy || self.account_busy)
                                    .label(if self.busy { "正在上传…" } else { "＋ 选择文件上传" })
                                    .on_click(cx.listener(|view, _, _, cx| view.select_files(cx))),
                            )
                            .child(if is_demo {
                                "演示模式只返回本地 file:// 链接，不能公开访问"
                            } else {
                                "语雀上传采用非公开网页接口，实际可用性需账号验证"
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .p_4()
                                    .rounded_lg()
                                    .bg(self.color(0x222428, 0xffffff))
                                    .child(format!("已记录资源   {total}")),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .p_4()
                                    .rounded_lg()
                                    .bg(self.color(0x222428, 0xffffff))
                                    .child(format!("待处理任务   {}", self.queue_count)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(if self.image_only { "图片资源" } else { "最近上传" })
                            .child(div().flex_1())
                            .child(Button::new("grid-toggle").label(if self.grid_view { "列表" } else { "网格" })
                                .on_click(cx.listener(|view, _, _, cx| { view.grid_view = !view.grid_view; view.refresh_thumbnails(cx); cx.notify(); })))
                            .child(Button::new("account-scope")
                                .label(if self.all_accounts { "全部账号" } else { "当前账号" })
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.all_accounts = !view.all_accounts;
                                    view.total_count = 0;
                                    view.reset_history(cx);
                                })))
                            .child(
                                Button::new("toggle-image-filter")
                                    .label(if self.image_only { "显示全部" } else { "仅看图片" })
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.image_only = !view.image_only;
                                        view.reset_history(cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(Input::new(&self.search_input))
                    .child(div().flex().gap_2().items_center()
                        .child(Button::new("sort-time").label(if self.oldest { "最早优先" } else { "最新优先" })
                            .on_click(cx.listener(|view, _, _, cx| { view.oldest = !view.oldest; view.reset_history(cx); })))
                        .child(Button::new("select-page").label("选择本页")
                            .on_click(cx.listener(|view, _, _, cx| { view.selected_ids = view.assets.iter().map(|a| a.id).collect(); cx.notify(); })))
                        .child(Button::new("batch-copy").label(format!("复制已选 ({})", self.selected_ids.len()))
                            .disabled(self.selected_ids.is_empty())
                            .on_click(cx.listener(|view, _, _, cx| view.copy_selected(cx)))))
                    .child(
                        div()
                            .id("asset-history")
                            .flex_1()
                            .min_h(px(0.))
                            .flex()
                            .when(self.grid_view, |d| d.flex_wrap().items_start())
                            .when(!self.grid_view, |d| d.flex_col())
                            .gap_2()
                            .children(rows)
                            .when(self.loading_history, |container| container.child("正在读取资源…"))
                            .when(self.history_error.is_some(), |container| {
                                container.child(format!("历史读取失败：{}", self.history_error.as_deref().unwrap_or("")))
                            })
                            .when(self.assets.is_empty() && !self.loading_history && self.history_error.is_none(), |container| {
                                container.child(if self.search.is_empty() && !self.image_only {
                                    "暂无上传记录，选择图片或附件开始"
                                } else {
                                    "没有找到符合条件的资源"
                                })
                            })
                            .overflow_y_scrollbar(),
                    )
                    .child(
                        div().flex().items_center().justify_between().gap_2()
                            .child(format!("第 {} 页 · 当前 {} 条", self.page_cursors.len(), self.assets.len()))
                            .when(self.history_error.is_some(), |container| container.child(
                                Button::new("refresh-history").label("重新读取")
                                    .on_click(cx.listener(|view, _, _, cx| view.reload(cx)))
                            ))
                            .child(Button::new("previous-page").label("上一页")
                                .disabled(self.loading_history || self.page_cursors.len() <= 1)
                                .on_click(cx.listener(|view, _, _, cx| {
                                    if view.page_cursors.len() > 1 && !view.loading_history {
                                        view.page_cursors.pop();
                                        view.assets.clear();
                                        view.reload(cx);
                                    }
                                })))
                            .child(Button::new("next-page").label("下一页")
                                .disabled(self.loading_history || !self.has_more)
                                .on_click(cx.listener(|view, _, _, cx| {
                                    if view.has_more && !view.loading_history {
                                        if let Some(last) = view.assets.last() {
                                            view.page_cursors.push(Some(last.id));
                                            view.assets.clear();
                                            view.reload(cx);
                                        }
                                    }
                                })))
                    )
                    .when(self.recovery_count > 0 && !self.busy, |container| container.child(
                        Button::new("recover-queue").label(format!("恢复 {} 个任务（先核实远端状态）", self.recovery_count))
                            .on_click(cx.listener(|view, _, _, cx| view.resume_jobs(cx)))
                    ))
                    .when(self.busy, |container| container.child(
                        div().flex().gap_2()
                            .child(Button::new("pause-queue").label("当前文件结束后暂停")
                                .on_click(cx.listener(|view, _, _, cx| { let _ = view.queue_control.compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed); view.status = "等待当前文件结束".into(); cx.notify(); })))
                            .child(Button::new("cancel-queue").label("取消剩余任务")
                                .on_click(cx.listener(|view, _, _, cx| { view.queue_control.store(2, Ordering::Relaxed); view.status = "等待当前文件结束".into(); cx.notify(); })))
                    ))
                    .when(self.queue_total > 0, |container| container.child(
                        div().flex().flex_col().gap_2().p_3().rounded_lg().bg(self.color(0x242629, 0xffffff))
                            .child(format!("本批 {}/{} · 新增 {} · 复用 {} · 失败 {}",
                                self.succeeded + self.duplicates + self.failed.len(), self.queue_total,
                                self.succeeded, self.duplicates, self.failed.len()))
                            .children(failure_rows)
                            .when(self.failed.len() > 3, |container| container.child(format!("另有 {} 个失败项", self.failed.len() - 3)))
                            .when(!self.failed.is_empty(), |container| container.child(
                                Button::new("retry-failed").label("手动恢复未完成任务")
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.resume_jobs(cx);
                                    }))
                            ))
                    ))
                    .child(
                        div()
                            .p_3()
                            .rounded_lg()
                            .bg(self.color(0x242629, 0xffffff))
                            .opacity(feedback_opacity)
                            .child(self.status.clone()),
                    ),
            )
            .child(self.inspector(cx))
    }
}

fn main() -> Result<()> {
    let config = Config::from_env();
    let path = db_path()?;
    let parent = path.parent().context("缺少数据目录")?;
    std::fs::create_dir_all(parent)?;
    let instance = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(parent.join("instance.lock"))?;
    instance.try_lock().context("已有 Yuque Asset Desk 实例使用此数据目录")?;
    let storage = Storage::new(path)?;
    application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            let config = config.clone();
            let storage = storage.clone();
            let bounds = Bounds::centered(None, size(px(1160.), px(800.)), cx);
            open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() }, cx, move |window, cx| {
                window.set_window_title("Yuque Asset Desk — GPUI Kit");
                cx.new(|cx| UploadDesk::new(window, cx, config, storage))
            })
            .expect("无法创建 GPUI 桌面窗口");
        });
    Ok(())
}
