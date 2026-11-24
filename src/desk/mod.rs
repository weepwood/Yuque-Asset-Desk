mod account;
pub(crate) mod design;
mod workspace;
mod album;
mod image_cache;
pub(crate) mod preferences;
mod queue;
mod resources;
mod view;

use anyhow::{Context as AnyhowContext, Result};
use crate::config::{Config, Mode, YuqueCredentials};
use gpui_kit::component::theme::{ThemeMode, ActiveTheme};
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
    masonry: bool,
    masonry_scroll: ScrollHandle,
    masonry_plan: album::MasonryPlan,
    masonry_signature: u64,
    ratio_revision:u64,
    #[cfg(feature="qa")]
    perf_start:std::time::Instant,
    #[cfg(feature="qa")]
    perf_preview_count:usize,
    tile_size: f32,
    group_dates: bool,
    favorites_only: bool,
    favorites: std::collections::HashSet<i64>,
    ratios: std::collections::HashMap<i64,f32>,
    hover_asset: Option<i64>,
    similar_to: Option<Asset>,
    album_list: ListState,
    layout_signature: u64,
    row_plan: std::rc::Rc<Vec<album::AlbumRow>>,
    visible_range: (usize,usize),
    preview_attempted:std::collections::HashSet<i64>,
    preview_loading_batch:bool,
    image_cache:Entity<image_cache::AlbumImageCache>,
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
    immersive:bool,
    status: String,
    backup_busy: bool,
    desktop_enabled:bool,
    prefs:preferences::Preferences,
    prefs_task:Option<Task<()>>,
    backup_task: Option<Task<()>>,

}

impl UploadDesk {
    pub(crate) fn persist_preferences(&mut self,cx:&mut Context<Self>){
        self.prefs.dark=self.dark;
        self.prefs.layout=if !self.resource.grid_view{"list"}else if self.resource.masonry{"masonry"}else if self.resource.justified{"justified"}else{"grid"}.into();
        self.prefs.tile=self.resource.tile_size;self.prefs.oldest=self.resource.oldest;self.prefs.favorites=self.resource.favorites_only;self.prefs.dates=self.resource.group_dates;
        // Serialize the small SQLite write on the main thread after debounce.
        // A cancelled old task cannot overwrite the latest settings.
        if self.isolated_qa(){return;}
        self.prefs_task=Some(cx.spawn(async move|this,cx|{
            cx.background_executor().timer(std::time::Duration::from_millis(180)).await;
            _=this.update(cx,|v,cx|{if v.storage.save_preferences(&v.prefs.encode()).is_err(){v.status="保存个性化设置失败".into();cx.notify();}});
        }));
    }
    fn isolated_qa(&self)->bool{
        cfg!(feature="qa")&&self.account.config.mode==Mode::Demo&&std::env::var_os("YUQUE_DATA_DIR").is_some()&&(std::env::var_os("YUQUE_QA_CAPTURE").is_some()||std::env::var_os("YUQUE_QA_PERF").is_some())
    }
    pub(crate) fn flush_preferences(&self){if !self.isolated_qa(){let _=self.storage.save_preferences(&self.prefs.encode());}}
    #[cfg(feature="qa")]
    pub(crate) fn report_perf(&self,cx:&App){
        if std::env::var_os("YUQUE_QA_PERF").is_some(){eprintln!("PERF final loaded={} decoded_cache={} bound={}",self.resource.assets.len(),self.resource.image_cache.read(cx).len(),image_cache::MAX_IMAGES);}
    }
    pub(crate) fn shortcut(&self)->&str{&self.prefs.shortcut}

    pub(crate) fn desktop_status(&mut self,enabled:bool,status:String,cx:&mut Context<Self>){
        self.desktop_enabled=enabled;self.status=status;cx.notify();
    }
    pub(crate) fn desktop_quick_upload(&mut self,cx:&mut Context<Self>){
        if self.queue.busy || self.backup_busy || self.account.account_busy{
            self.status="当前有任务正在处理，请稍后再操作".into();cx.notify();return;
        }
        self.paste_files(cx);
    }
    pub(crate) fn desktop_can_quit(&mut self,cx:&mut Context<Self>)->bool{
        if self.queue.busy || self.backup_busy || self.account.account_busy{
            self.status="请先等待上传或备份结束，再退出".into();cx.notify();false
        }else{true}
    }

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
        let prefs=preferences::Preferences::decode(storage.preferences().unwrap_or_default());
        let resize_subscription=cx.observe_window_bounds(window,|v,w,cx|{
            if w.is_maximized() || w.is_fullscreen(){return;}
            let size=w.bounds().size;
            v.prefs.width=f32::from(size.width).clamp(720.,3840.);v.prefs.height=f32::from(size.height).clamp(600.,2160.);
            v.persist_preferences(cx);
        });
        let mut view = Self {
            resource: ResourceState {
                assets: Vec::new(),
                search: String::new(),
                grid_view:prefs.layout!="list",
                justified:prefs.layout=="justified",
                masonry:prefs.layout=="masonry",
                masonry_scroll:ScrollHandle::new(),
                masonry_plan:Default::default(),
                masonry_signature:0,
                ratio_revision:0,
                #[cfg(feature="qa")]
                perf_start:std::time::Instant::now(),
                #[cfg(feature="qa")]
                perf_preview_count:usize::MAX,
                tile_size:prefs.tile,
                group_dates:prefs.dates,
                favorites_only:prefs.favorites,
                favorites: Default::default(),
                ratios: Default::default(),
                hover_asset: None,
                similar_to: None,
                album_list: ListState::new(0,ListAlignment::Top,px(180.)),
                layout_signature: 0,
                row_plan:Default::default(),
                visible_range:(0,48),
                preview_attempted:Default::default(),
                preview_loading_batch:false,
                image_cache:image_cache::AlbumImageCache::new(cx),
                selected_asset: None,
                oldest:prefs.oldest,
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
            _subscriptions: vec![subscription,resize_subscription],
            jobs_page: false,
            inspector_open: false,
            immersive:false,
            status,
            backup_busy:false,
            desktop_enabled:false,
            prefs,
            prefs_task:None,
            backup_task:None,

        };
        cx.defer_in(window, |view, _, cx| {
            #[cfg(feature="qa")]
            if view.account.config.mode==Mode::Demo && std::env::var_os("YUQUE_DATA_DIR").is_some() && (std::env::var_os("YUQUE_QA_CAPTURE").is_some() || std::env::var_os("YUQUE_QA_PERF").is_some()) {
                match std::env::var("YUQUE_QA_PAGE").as_deref() {
                    Ok("settings")=>{view.account_panel=true;view.inspector_open=false;}
                    Ok("tasks")=>{view.jobs_page=true;view.inspector_open=false;view.refresh_jobs(cx);}
                    Ok("tags")=>{view.resource.tags_expanded=true;}
                    Ok("justified")=>{view.resource.justified=true;}
                    Ok("immersive")=>{view.immersive=true;view.resource.masonry=true;}
                    Ok("masonry")=>{view.resource.masonry=true;}
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
