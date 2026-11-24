mod account;
mod queue;
mod resources;
mod view;

use anyhow::{Context as AnyhowContext, Result};
use crate::config::{Config, Mode, YuqueCredentials};
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
use crate::storage::{Asset, Storage, Job};

use crate::{format, preview, uploader};

pub(crate) fn db_path() -> Result<PathBuf> {
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

pub(crate) struct UploadDesk {
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
    large_preview_path: Option<PathBuf>,
    large_preview_open: bool,
    cache_busy: bool,
    cache_task: Option<Task<()>>,
    preview_error: Option<String>,
    account_panel: bool,
    account_id: Entity<InputState>,
    account_csrf: Entity<InputState>,
    account_cookie: Entity<InputState>,
    account_busy: bool,
    session_status: String,
    saved_accounts: Vec<String>,
    account_task: Option<Task<()>>,
    queue_control: Arc<AtomicU8>,
    recovery_count: usize,
    search_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    category: &'static str,
    all_accounts: bool,
    page_cursors: Vec<Option<i64>>,
    has_more: bool,
    loading_history: bool,
    history_error: Option<String>,
    history_generation: u64,
    history_task: Option<Task<()>>,
    jobs_page: bool,
    jobs: Vec<Job>,
    job_cursors: Vec<Option<i64>>,
    jobs_more: bool,
    jobs_loading: bool,
    jobs_task: Option<Task<()>>,
    jobs_generation: u64,
    jobs_error: Option<String>,
    transfer: Arc<uploader::TransferProgress>,
    transfer_task: Option<Task<()>>,
    active_job: Option<i64>,
    inspector_open: bool,
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
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>, config: Config, storage: Storage) -> Self {
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
            large_preview_path: None,
            large_preview_open: false,
            cache_busy: false,
            cache_task: None,
            preview_error: None,
            account_panel: false,
            account_id: cx.new(|cx| InputState::new(window, cx).placeholder("语雀数字用户 ID")),
            account_csrf: cx.new(|cx| InputState::new(window, cx).placeholder("CSRF Token").masked(true)),
            account_cookie: cx.new(|cx| InputState::new(window, cx).placeholder("Cookie").masked(true)),
            account_busy: false,
            session_status: "尚未验证会话".into(),
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
            category: "",
            all_accounts: false,
            page_cursors: vec![None],
            has_more: false,
            loading_history: false,
            history_error: None,
            history_generation: 0,
            history_task: None,
            jobs_page: false,
            jobs: Vec::new(),
            job_cursors: vec![None],
            jobs_more: false,
            jobs_loading: false,
            jobs_task: None,
            jobs_generation: 0,
            jobs_error: None,
            transfer: Arc::new(uploader::TransferProgress::default()),
            transfer_task: None,
            active_job: None,
            inspector_open: true,
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
        if std::env::var("YUQUE_KEYCHAIN_SMOKE").as_deref() == Ok("1") && std::env::var_os("YUQUE_DATA_DIR").is_some() && view.config.mode == Mode::Demo {
            view.keychain_smoke(cx);
        }
        view
    }

}
