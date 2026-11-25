mod account;
mod account_model;
pub(crate) mod design;
mod workspace;
mod updates;
mod data_panel;
mod album;
mod image_cache;
mod thumbnail_model;
pub(crate) mod preferences;
mod preference_writer;
mod queue;
mod queue_prepare;
mod queue_runner;
mod job_history;
mod resources;
mod gallery_query;
mod view;
mod shell;
mod selection;
mod inspector_model;
mod mutations;
mod settings_view;
mod viewer_model;
use shell::{Page,Command};

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
    density:Entity<gpui_kit::component::slider::SliderState>,
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

    image_cache:Entity<image_cache::AlbumImageCache>,
    selected_asset: Option<Asset>,
    oldest: bool,
    selected_ids: std::collections::HashSet<i64>,
    batch_tag_input: Entity<InputState>,
    tag_filter: Option<String>,
    tag_catalog: Vec<String>,
    tags_expanded: bool,
    zoom: f32,
    pan: Point<Pixels>,
    drag_at: Option<Point<Pixels>>,
    thumbnail_path: Option<PathBuf>,
    cached_thumbnails: std::collections::HashMap<i64, PathBuf>,
    preview_task: Option<Task<()>>,
    pending_preview:Option<(i64,u8)>,
    preview_reload_pending:bool,
    viewer_return:Option<(Asset,std::collections::HashSet<i64>,Option<i64>)>,
    preview_loading: bool,
    large_preview_path: Option<PathBuf>,
    large_preview_open: bool,
    selection_anchor:Option<i64>,
    batch_tag_open:bool,
    gallery_focus:FocusHandle,
    inspector_focus:FocusHandle,
    preview_focus:FocusHandle,
    zoom_channel:usize,
    displayed_zoom:f32,
    preview_error: Option<String>,
    search_input: Entity<InputState>,
    category: &'static str,
    all_accounts: bool,
    page_cursors: Vec<Option<i64>>,
    has_more: bool,
    loading_history: bool,
    history_error: Option<String>,
    history_generation: u64,
    total_count: i64,
}

pub(crate) struct UploadDesk {
    resource: ResourceState,
    gallery_query:Entity<gallery_query::GalleryQuery>,
    thumbnails:Entity<thumbnail_model::ThumbnailModel>,
    resource_write_busy:bool,
    resource_write_task:Option<Task<()>>,
    inspector_model:Entity<inspector_model::InspectorModel>,
    viewer_model:Entity<viewer_model::ViewerModel>,
    viewer_revision:u64,
    queue: Entity<queue_runner::QueueRunner>,
    job_history:Entity<job_history::JobHistory>,
    account: Entity<account_model::AccountModel>,
    config:Config,
    maintenance_busy:bool,
    job_ack_task:Option<Task<()>>,
    storage: Storage,
    dark: bool,
    page: Page,
    _subscriptions: Vec<Subscription>,
    inspector_open: bool,
    immersive:bool,
    sidebar_collapsed:bool,
    surface_motion:design::SurfaceMotion,
    status: String,
    picker_pending: bool,
    picker_task: Option<Task<()>>,
    backup_busy: bool,
    desktop_enabled:bool,
    prefs:preferences::Preferences,
    prefs_task:Option<Task<()>>,
    prefs_writer:preference_writer::PreferenceWriter,
    prefs_revision:u64,
    prefs_saved_revision:u64,
    prefs_dirty:bool,
    prefs_writing:bool,
    backup_task: Option<Task<()>>,
    update_state:crate::update::State,
    update_task:Option<Task<()>>,
    data_usage:Option<crate::data_management::Usage>,
    data_scan_busy:bool,
    data_task:Option<Task<()>>,
    data_feedback:String,
    reset_armed:bool,
    reset_pending:bool,
    resetting:bool,

}

impl UploadDesk {
    fn account_busy(&self,cx:&App)->bool{self.maintenance_busy||self.account.read(cx).account_busy}
    pub(crate) fn persist_preferences(&mut self,cx:&mut Context<Self>){
        self.prefs.dark=self.dark;
        self.prefs.layout=if !self.resource.grid_view{"list"}else if self.resource.masonry{"masonry"}else if self.resource.justified{"justified"}else{"grid"}.into();
        self.prefs.tile=self.resource.tile_size;self.prefs.oldest=self.resource.oldest;self.prefs.favorites=self.resource.favorites_only;self.prefs.dates=self.resource.group_dates;
        self.prefs_revision=self.prefs_revision.saturating_add(1);
        if self.resetting||self.reset_armed||self.reset_pending||self.isolated_qa(){return;}
        self.prefs_dirty=true;
        if self.prefs_writing{return;}
        self.prefs_writing=true;
        let writer=self.prefs_writer.clone();
        let storage=self.storage.clone();
        self.prefs_task=Some(cx.spawn(async move|this,cx|{
            loop{
                cx.background_executor().timer(std::time::Duration::from_millis(180)).await;
                let snapshot=this.update(cx,|v,_|{
                    if v.resetting||v.reset_armed||v.reset_pending{v.prefs_dirty=false;v.prefs_writing=false;return None;}
                    v.prefs_dirty=false;
                    Some((v.prefs_revision,v.prefs.encode()))
                });
                let Ok(Some((revision,value)))=snapshot else{break;};
                let writer=writer.clone();let storage=storage.clone();
                let result=cx.background_spawn(async move{writer.write(&storage,revision,&value)}).await;
                let next=this.update(cx,|v,cx|{
                    if result.is_err(){v.status="保存个性化设置失败".into();}else{v.prefs_saved_revision=v.prefs_saved_revision.max(revision);}
                    let next=v.prefs_dirty&&!v.resetting&&!v.reset_armed&&!v.reset_pending;
                    if !next{v.prefs_writing=false;}
                    cx.notify();next
                });
                if !matches!(next,Ok(true)){break;}
            }
        }));
    }
    fn isolated_qa(&self)->bool{
        cfg!(feature="qa")&&self.config.mode==Mode::Demo&&std::env::var_os("YUQUE_DATA_DIR").is_some()&&(std::env::var_os("YUQUE_QA_CAPTURE").is_some()||std::env::var_os("YUQUE_QA_PERF").is_some()||std::env::var_os("YUQUE_QA_PATH").is_some())
    }
    #[cfg(all(test,feature="qa"))]
    pub(crate) fn flush_preferences(&self){if !self.resetting && !self.isolated_qa(){let _=self.prefs_writer.write(&self.storage,self.prefs_revision,&self.prefs.encode());}}
    // GPUI native quit observers have a 200ms shutdown budget. Normal close
    // verifies the latest revision before quit; external/native quit is best effort.
    pub(crate) fn final_preferences_task(&self,cx:&App)->Option<Task<()>>{
        if self.resetting||self.reset_pending||self.isolated_qa()||self.prefs_saved_revision>=self.prefs_revision{return None;}
        let writer=self.prefs_writer.clone();let storage=self.storage.clone();
        let revision=self.prefs_revision;let value=self.prefs.encode();
        Some(cx.background_executor().spawn(async move{let _=writer.write(&storage,revision,&value);}))
    }
    #[cfg(feature="qa")]
    pub(crate) fn report_perf(&self,cx:&App){
        if std::env::var_os("YUQUE_QA_PERF").is_some(){eprintln!("PERF final loaded={} decoded_cache={} bound={}",self.resource.assets.len(),self.resource.image_cache.read(cx).len(),image_cache::MAX_IMAGES);}
    }
    #[cfg(feature="qa")]
    pub(crate) fn qa_memory_phase(&mut self,phase:&str,cx:&mut Context<Self>){
        if !self.isolated_qa(){return;}
        match phase{
            "leave"=>self.navigate_page(Page::Settings,cx),
            "return"=>self.navigate_page(Page::Gallery,cx),
            "reset"=>self.apply_account(None,cx),
            _=>{}
        }
    }
    #[cfg(feature="qa")]
    pub(crate) fn qa_memory_checkpoint(&self,phase:&str,cx:&App){
        let r=&self.resource;
        eprintln!("MEMORY phase={phase} loaded={} cache={} bound={} paths={} ratios={} rows={} tiles={} loading={}",
            r.assets.len(),r.image_cache.read(cx).len(),image_cache::MAX_IMAGES,r.cached_thumbnails.len(),r.ratios.len(),
            r.row_plan.len(),r.masonry_plan.tiles.len(),r.loading_history);
    }
    pub(crate) fn shortcut(&self)->&str{&self.prefs.shortcut}

    #[cfg(feature="qa")]
    pub(crate) fn qa_path_state(&self)->(usize,Option<i64>,bool,bool){
        (self.resource.selected_ids.len(),self.resource.selected_asset.as_ref().map(|a|a.id),self.resource.large_preview_open,self.inspector_open)
    }
    pub(crate) fn desktop_status(&mut self,enabled:bool,status:String,cx:&mut Context<Self>){
        self.desktop_enabled=enabled;self.status=status;cx.notify();
    }
    pub(crate) fn desktop_quick_upload(&mut self,cx:&mut Context<Self>){
        if self.resource_write_busy || self.picker_pending || self.queue.read(cx).busy || self.backup_busy || self.account_busy(cx){
            self.status="当前有任务正在处理，请稍后再操作".into();cx.notify();return;
        }
        self.paste_files(cx);
    }
    pub(crate) fn desktop_can_quit(&mut self,cx:&mut Context<Self>)->bool{
        if self.prefs_writing || self.resource_write_busy || self.picker_pending || self.queue.read(cx).busy || self.backup_busy || self.account_busy(cx){
            self.status="请先关闭文件选择窗口或等待任务结束，再退出".into();cx.notify();false
        }else if !self.isolated_qa()&&self.prefs_saved_revision<self.prefs_revision{
            self.reset_armed=false;
            self.persist_preferences(cx);
            self.status="正在保存最新设置，完成后可退出".into();cx.notify();false
        }else{true}
    }

    #[cfg(all(test,feature="qa"))]
    pub(crate) fn new(window:&mut Window,cx:&mut Context<Self>,config:Config,storage:Storage)->Self{
        let prefs=preferences::Preferences::decode(storage.preferences().unwrap_or_default());
        Self::new_with_preferences(window,cx,config,storage,prefs)
    }
    pub(crate) fn new_with_preferences(window: &mut Window, cx: &mut Context<Self>, config: Config, storage: Storage,prefs:preferences::Preferences) -> Self {
        shell::bind_gallery(cx);
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索文件名或标签…"));
        let subscription = cx.subscribe_in(
            &search_input,
            window,
            |this, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let query=input.read(cx).value().to_string();
                    if this.resource.search==query{return;}
                    this.resource.search=query;
                    this.resource.similar_to=None;
                    this.reset_history(cx);
                    cx.notify();
                }
            },
        );
        let status = match (&config.mode, &config.creds) {
            (Mode::Demo, _) => "本地演示模式：不会上传至网络".to_string(),
            (Mode::Yuque, None) => "语雀未配置：请在设置中识别账号并配置网页会话".to_string(),
            (Mode::Yuque, Some(_)) => "语雀会话已配置，等待上传文件".to_string(),
        };
        use gpui_kit::component::slider::{SliderState,SliderEvent};
        let density=cx.new(|_|SliderState::new().min(96.).max(240.).step(4.).default_value(prefs.tile));
        let density_subscription=cx.subscribe_in(&density,window,|v,_,event:&SliderEvent,_,cx|{
            match event{
                SliderEvent::Change(value)=>{v.resource.tile_size=value.start().clamp(96.,240.);cx.notify();}
                SliderEvent::Release(value)=>{v.resource.tile_size=value.start().clamp(96.,240.);v.persist_preferences(cx);cx.notify();}
            }
        });
        let resize_subscription=cx.observe_window_bounds(window,|v,w,cx|{
            if v.page==Page::Gallery&&v.inspector_open&&!v.immersive&&!v.resource.large_preview_open
                &&w.viewport_size().width<px(1000.)&&!v.resource.inspector_focus.contains_focused(w,cx){
                v.resource.inspector_focus.focus(w,cx);
            }
            if w.is_maximized() || w.is_fullscreen(){return;}
            let size=w.bounds().size;
            v.prefs.width=f32::from(size.width).clamp(720.,3840.);v.prefs.height=f32::from(size.height).clamp(600.,2160.);
            v.persist_preferences(cx);
        });
        let inspector_model=cx.new(|cx|inspector_model::InspectorModel::new(window,cx));
        let inspector_subscription=cx.observe(&inspector_model,|_,_,cx|cx.notify());
        let viewer_model=cx.new(|_|viewer_model::ViewerModel::default());
        let viewer_subscription=cx.observe(&viewer_model,|v,model,cx|{
            let revision=model.read(cx).revision;
            if revision!=v.viewer_revision{
                v.viewer_revision=revision;
                v.resource.zoom=1.;v.resource.pan=point(px(0.),px(0.));v.resource.zoom_channel=v.resource.zoom_channel.wrapping_add(1);
            }
            cx.notify();
        });
        let job_history=cx.new(|_|job_history::JobHistory::default());
        let job_history_subscription=cx.observe(&job_history,|_,_,cx|cx.notify());
        let queue=cx.new(|_|queue_runner::QueueRunner::default());
        let queue_observer=cx.observe(&queue,|_,_,cx|cx.notify());
        let queue_events=cx.subscribe(&queue,|v,_,event:&queue_runner::QueueEvent,cx|{
            use queue_runner::QueueEvent::*;
            match event{
                Message(s)=>v.status=s.clone(),
                Started=>v.first_jobs(cx),
                Refresh=>v.refresh_jobs(cx),
                ResourceChanged=>v.reload(cx),
                ImageCommitted=>{
                    // Refresh promptly at the newest first page; do not jump a
                    // user away from deep browsing or a fullscreen viewer.
                    if v.page==Page::Gallery&&!v.resource.large_preview_open&&!v.resource.loading_history
                        &&!v.resource.oldest&&v.resource.assets.len()<=PAGE_SIZE&&v.resource.visible_range.0<=12
                        &&v.resource.similar_to.is_none(){v.reload(cx);}
                },
                Authorization=>v.account.update(cx,|a,cx|{a.session_status="上传未获授权：请在设置检查会话及 CSRF Token，再手动恢复任务".into();cx.notify();}),
                Finished(body)=>if v.desktop_enabled{cx.show_system_notification(SystemNotification{tag:"yuque-upload-batch".into(),title:"Yuque Asset Desk".into(),body:body.clone().into(),actions:vec![]});},
            }
            cx.notify();
        });
        let gallery_query=cx.new(|_|gallery_query::GalleryQuery::default());
        let gallery_events=cx.subscribe(&gallery_query,|v,_,event:&gallery_query::GalleryResult,cx|v.apply_gallery_result(event,cx));
        let thumbnails=cx.new(|_|thumbnail_model::ThumbnailModel::default());
        let thumbnail_events=cx.subscribe(&thumbnails,|v,_,event:&thumbnail_model::ThumbnailBatch,cx|v.apply_thumbnail_batch(event,cx));
        let account=cx.new(|cx|account_model::AccountModel::new(window,cx));
        let account_observer=cx.observe(&account,|_,_,cx|cx.notify());
        let account_events=cx.subscribe(&account,|v,_,event:&account_model::AccountEvent,cx|{
            match event{account_model::AccountEvent::Activate(creds)=>v.apply_account(Some(creds.clone()),cx)}
        });
        let mut view = Self {
            gallery_query,thumbnails,
            job_history,
            viewer_model,viewer_revision:0,
            inspector_model,
            resource_write_busy:false,resource_write_task:None,
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
                density,
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
                image_cache:image_cache::AlbumImageCache::new(cx),
                selected_asset: None,
                oldest:prefs.oldest,
                selected_ids: Default::default(),
                batch_tag_input: cx.new(|cx| InputState::new(window,cx).placeholder("批量标签，如 设计/UI")),
                tag_filter: None,
                tag_catalog: Vec::new(),
                tags_expanded: false,
                zoom: 1.,
                pan: point(px(0.),px(0.)),
                drag_at: None,
                thumbnail_path: None,
                cached_thumbnails: Default::default(),
                preview_task: None,
                pending_preview:None,preview_reload_pending:false,viewer_return:None,
                preview_loading: false,
                large_preview_path: None,
                large_preview_open: false,
                selection_anchor:None,batch_tag_open:false,gallery_focus:cx.focus_handle().tab_stop(true),inspector_focus:cx.focus_handle().tab_stop(true),preview_focus:cx.focus_handle(),zoom_channel:0,displayed_zoom:1.,
                preview_error: None,
                search_input,
                category: "",
                all_accounts: false,
                page_cursors: vec![None],
                has_more: false,
                loading_history: false,
                history_error: None,
                history_generation: 0,
                total_count: 0,
            },
            queue,
            account,config,maintenance_busy:false,job_ack_task:None,
            dark: cx.theme().mode == ThemeMode::Dark,
            page: Page::Gallery,
            storage,
            _subscriptions: vec![subscription,resize_subscription,density_subscription,inspector_subscription,viewer_subscription,job_history_subscription,queue_observer,queue_events,gallery_events,thumbnail_events,account_observer,account_events],
            inspector_open: false,
            immersive:false,sidebar_collapsed:false,
            surface_motion:Default::default(),
            status,
            picker_pending:false,picker_task:None,
            backup_busy:false,
            desktop_enabled:false,
            prefs,
            prefs_task:None,prefs_writer:Default::default(),prefs_revision:0,prefs_saved_revision:0,prefs_dirty:false,prefs_writing:false,
            backup_task:None,
            update_state:Default::default(),
            update_task:None,
            data_usage:None,data_scan_busy:false,data_task:None,data_feedback:String::new(),
            reset_armed:false,reset_pending:false,resetting:false,

        };
        cx.defer_in(window, |view, _window, cx| {
            #[cfg(feature="qa")]
            if view.config.mode==Mode::Demo && std::env::var_os("YUQUE_DATA_DIR").is_some() && (std::env::var_os("YUQUE_QA_CAPTURE").is_some() || std::env::var_os("YUQUE_QA_PERF").is_some()) {
                match std::env::var("YUQUE_QA_PAGE").as_deref() {
                    Ok("notice")=>{
                        use gpui_kit::component::{notification::Notification,WindowExt};
                        _window.push_notification(Notification::success("演示：资源链接已复制").autohide(false),cx);
                    }
                    Ok("updates")=>{
                        view.page=Page::Settings;view.inspector_open=false;
                        view.update_state=crate::update::State::Newer(crate::update::Available{version:"0.6.0".into(),page:crate::update::RELEASES.into(),package:Some((crate::update::RELEASES.into(),crate::update::RELEASES.into()))});
                    }
                    Ok("data")=>{view.page=Page::Data;view.inspector_open=false;view.refresh_data(cx);}
                    Ok("account")=>{view.page=Page::Account;view.inspector_open=false;}
                    Ok("account-identified")=>{view.page=Page::Account;view.inspector_open=false;view.account.update(cx,|a,cx|{a.identity=Some(crate::config::AccountIdentity{id:"123456".into(),name:"演示账号".into(),login:"fixture-user".into()});a.identity_status="已识别个人账号；保存时仍需核对网页会话".into();cx.notify();});}
                    Ok("settings")=>{view.page=Page::Settings;view.inspector_open=false;}
                    Ok("tasks")=>{view.page=Page::Jobs;view.inspector_open=false;}
                    Ok("tasks-active")=>{
                        view.page=Page::Jobs;view.inspector_open=false;
                        view.queue.update(cx,|q,cx|{
                            q.busy=true;q.queue_total=3;q.queue_count=2;q.active_job=Some(3);
                            q.transfer.total.store(8*1024*1024,Ordering::Relaxed);
                            q.transfer.bytes.store(8*1024*1024,Ordering::Relaxed);
                            q.queue_control.store(1,Ordering::Relaxed);
                            cx.notify();
                        });
                    }
                    Ok("tags")=>{view.resource.tags_expanded=true;}
                    Ok("justified")=>{view.resource.justified=true;}
                    Ok("rail")=>{view.sidebar_collapsed=true;}
                    Ok("immersive")=>{view.immersive=true;view.resource.masonry=true;}
                    Ok("masonry")=>{view.resource.masonry=true;}
                    Ok("dense")=>{view.resource.tile_size=96.;view.prefs.comfortable=false;}
                    Ok("compact")=>{view.prefs.comfortable=false;}
                    Ok("compact-masonry")=>{view.prefs.comfortable=false;view.resource.masonry=true;}
                    Ok("favorites")=>{view.resource.favorites_only=true;}
                    Ok("browse-context")=>{view.resource.category="image";view.resource.favorites_only=true;view.resource.all_accounts=true;view.resource.tag_filter=Some("设计/UI".into());view.resource.tags_expanded=true;}
                    Ok("browse-empty")=>{view.resource.category="video";view.resource.tag_filter=Some("设计/UI".into());view.resource.tags_expanded=true;}
                    _=>{}
                }
            }
            view.account.update(cx,|a,cx|a.refresh_index(view.storage.clone(),cx));
            view.refresh_jobs(cx);
            view.reload(cx);
        });
        #[cfg(feature="qa")]
        if view.config.mode==Mode::Demo&&std::env::var_os("YUQUE_DATA_DIR").is_some(){
            if let Ok(kind)=std::env::var("YUQUE_QA_PICKER"){
                if matches!(kind.as_str(),"upload"|"backup"){
                    cx.spawn(async move|this,cx|{
                        cx.background_executor().timer(std::time::Duration::from_millis(900)).await;
                        let _=this.update(cx,|v,cx|{
                            if kind=="upload"{v.select_files(cx);}else{v.export_album(cx);}
                            eprintln!("PICKER requested={kind} pending={}",v.picker_pending);
                        });
                        cx.background_executor().timer(std::time::Duration::from_secs(4)).await;
                        let _=this.update(cx,|v,_|eprintln!("PICKER alive pending={} status={}",v.picker_pending,v.status));
                    }).detach();
                }
            }
        }
        if std::env::var("YUQUE_KEYCHAIN_SMOKE").as_deref() == Ok("1") && std::env::var_os("YUQUE_DATA_DIR").is_some() && view.config.mode == Mode::Demo {
            view.keychain_smoke(cx);
        }
        view
    }

}

#[cfg(all(test,feature="qa"))]
mod interaction_tests;
