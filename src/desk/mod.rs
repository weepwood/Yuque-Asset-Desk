mod account;
mod album;
mod queue;
mod resources;
mod view;

use anyhow::{Context as AnyhowContext, Result};
use crate::config::{Config, Mode, YuqueCredentials};
use gpui_kit::component::theme::{Theme, ThemeMode, ActiveTheme};
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

const PAGE_SIZE: usize = 150;

struct AccountState {
    config: Config,
    account_id: Entity<InputState>,
    account_csrf: Entity<InputState>,
    account_cookie: Entity<InputState>,
    account_busy: bool,
    session_status: String,
    saved_accounts: Vec<String>,
    account_task: Option<Task<()>>,
}

struct QueueState {
    queue_control: Arc<AtomicU8>,
    recovery_count: usize,
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
    busy: bool,
    queue_count: usize,
    queue_total: usize,
    succeeded: usize,
    duplicates: usize,
    failed: Vec<(PathBuf, String)>,
    upload_task: Option<Task<()>>,
}

struct ResourceState {
    assets: Vec<Asset>,
    search: String,
    grid_view: bool,
    justified: bool,
    tile_size: f32,
    group_dates: bool,
    favorites_only: bool,
    favorites: std::collections::HashSet<i64>,
    ratios: std::collections::HashMap<i64,f32>,
    hover_asset: Option<i64>,
    similar_to: Option<Asset>,
    album_list: ListState,
    layout_signature: u64,
    selected_asset: Option<Asset>,
    oldest: bool,
    selected_ids: std::collections::HashSet<i64>,
    tag_input: Entity<InputState>,
    batch_tag_input: Entity<InputState>,
    tag_filter: Option<String>,
    tag_catalog: Vec<String>,
    tags_expanded: bool,
    zoom: f32,
    pan: Point<Pixels>,
    drag_at: Option<Point<Pixels>>,
    thumbnail_path: Option<PathBuf>,
    cached_thumbnails: std::collections::HashMap<i64, PathBuf>,
    thumbnails_task: Option<Task<()>>,
    preview_task: Option<Task<()>>,
    preview_loading: bool,
    large_preview_path: Option<PathBuf>,
    large_preview_open: bool,
    preview_error: Option<String>,
    search_input: Entity<InputState>,
    category: &'static str,
    all_accounts: bool,
    page_cursors: Vec<Option<i64>>,
    has_more: bool,
    loading_history: bool,
    history_error: Option<String>,
    history_generation: u64,
    history_task: Option<Task<()>>,
    total_count: i64,
}

pub(crate) struct UploadDesk {
    resource: ResourceState,
    queue: QueueState,
    account: AccountState,
    storage: Storage,
    dark: bool,
    account_panel: bool,
    _subscriptions: Vec<Subscription>,
    jobs_page: bool,
    inspector_open: bool,
    status: String,
    backup_busy: bool,
    backup_task: Option<Task<()>>,

}

impl UploadDesk {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>, config: Config, storage: Storage) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索文件名或标签…"));
        let subscription = cx.subscribe_in(
            &search_input,
            window,
            |this, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.resource.search = input.read(cx).value().to_string();
                    this.resource.similar_to=None;
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
            resource: ResourceState {
                assets: Vec::new(),
                search: String::new(),
                grid_view: true,
                justified: false,
                tile_size: 148.,
                group_dates: true,
                favorites_only: false,
                favorites: Default::default(),
                ratios: Default::default(),
                hover_asset: None,
                similar_to: None,
                album_list: ListState::new(0,ListAlignment::Top,px(350.)),
                layout_signature: 0,
                selected_asset: None,
                oldest: false,
                selected_ids: Default::default(),
                tag_input: cx.new(|cx| InputState::new(window, cx).placeholder("标签，用逗号分隔")),
                batch_tag_input: cx.new(|cx| InputState::new(window,cx).placeholder("批量标签，如 设计/UI")),
                tag_filter: None,
                tag_catalog: Vec::new(),
                tags_expanded: false,
                zoom: 1.,
                pan: point(px(0.),px(0.)),
                drag_at: None,
                thumbnail_path: None,
                cached_thumbnails: Default::default(),
                thumbnails_task: None,
                preview_task: None,
                preview_loading: false,
                large_preview_path: None,
                large_preview_open: false,
                preview_error: None,
                search_input,
                category: "",
                all_accounts: false,
                page_cursors: vec![None],
                has_more: false,
                loading_history: false,
                history_error: None,
                history_generation: 0,
                history_task: None,
                total_count: 0,
            },
            queue: QueueState {
                queue_control: Arc::new(AtomicU8::new(0)),
                recovery_count,
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
                busy: false,
                queue_count: 0,
                queue_total: 0,
                succeeded: 0,
                duplicates: 0,
                failed: Vec::new(),
                upload_task: None,
            },
            account: AccountState {
                config,
                account_id: cx.new(|cx| InputState::new(window, cx).placeholder("语雀数字用户 ID")),
                account_csrf: cx.new(|cx| InputState::new(window, cx).placeholder("CSRF Token").masked(true)),
                account_cookie: cx.new(|cx| InputState::new(window, cx).placeholder("Cookie").masked(true)),
                account_busy: false,
                session_status: "尚未验证会话".into(),
                saved_accounts,
                account_task: None,
            },
            dark: cx.theme().mode == ThemeMode::Dark,
            account_panel: false,
            storage,
            _subscriptions: vec![subscription],
            jobs_page: false,
            inspector_open: false,
            status,
            backup_busy:false,
            backup_task:None,

        };
        cx.defer_in(window, |view, _, cx| {
            #[cfg(feature="qa")]
            if view.account.config.mode==Mode::Demo && std::env::var_os("YUQUE_DATA_DIR").is_some() && std::env::var_os("YUQUE_QA_CAPTURE").is_some() {
                match std::env::var("YUQUE_QA_PAGE").as_deref() {
                    Ok("tasks")=>{view.jobs_page=true;view.inspector_open=false;view.refresh_jobs(cx);}
                    Ok("tags")=>{view.resource.tags_expanded=true;}
                    Ok("justified")=>{view.resource.justified=true;}
                    Ok("dense")=>{view.resource.tile_size=96.;}
                    Ok("favorites")=>{view.resource.favorites_only=true;}
                    _=>{}
                }
            }
            view.reload(cx);
        });
        if std::env::var("YUQUE_KEYCHAIN_SMOKE").as_deref() == Ok("1") && std::env::var_os("YUQUE_DATA_DIR").is_some() && view.account.config.mode == Mode::Demo {
            view.keychain_smoke(cx);
        }
        view
    }

}

#[cfg(all(test,feature="qa"))]
mod interaction_tests;
