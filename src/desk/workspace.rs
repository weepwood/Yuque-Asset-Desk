//! Product composition: native sidebar items, menus and compact toolbars.
use super::*;
use gpui_kit::component::{IconName,Sizable,switch::Switch,slider::Slider,menu::{DropdownMenu,PopupMenu,PopupMenuItem},sidebar::{SidebarItem,SidebarMenuItem}};
type DeskAction=fn(&mut UploadDesk,&mut Window,&mut Context<UploadDesk>);
fn action(weak:WeakEntity<UploadDesk>,label:&'static str,checked:bool,f:DeskAction)->PopupMenuItem{
    PopupMenuItem::new(label).checked(checked).on_click(move|_,w,cx|{let _=weak.update(cx,|v,cx|f(v,w,cx));})
}
pub(super) fn asset_menu(menu:PopupMenu,weak:WeakEntity<UploadDesk>,asset:Asset,starred:bool,checked:bool)->PopupMenu {
    let item=|label:&str,icon:IconName,f:fn(&mut UploadDesk,&Asset,&mut Window,&mut Context<UploadDesk>)|{
        let weak=weak.clone();let a=asset.clone();
        PopupMenuItem::new(label.to_owned()).icon(icon).on_click(move|_,w,cx|{let _=weak.update(cx,|v,cx|f(v,&a,w,cx));})
    };
    menu.item(item("复制链接",IconName::Copy,|v,a,w,cx|v.copy_asset(a,false,w,cx)))
        .item(item("复制 Markdown",IconName::Copy,|v,a,w,cx|v.copy_asset(a,true,w,cx)))
        .separator()
        .item(item("查看详情",IconName::PanelRight,|v,a,w,cx|{v.show_asset_details(a.clone(),w,cx)}))
        .item(item(if starred{"取消收藏"}else{"收藏"},IconName::Star,|v,a,_,cx|v.toggle_favorite(a,cx)))
        .item(item(if checked{"取消选择"}else{"选择"},IconName::Check,|v,a,w,cx|v.toggle_album_selection(a.id,w,cx)))
        .when(asset.category=="image",|m|m.item(item("查找相似图片",IconName::File,|v,a,_,cx|v.find_similar(a.clone(),cx))))
}
impl UploadDesk {
    pub(super) fn toggle_album_selection(&mut self,id:i64,w:&mut Window,cx:&mut Context<Self>){
        if let Some(asset)=self.resource.assets.iter().find(|a|a.id==id).cloned(){self.click_tile(asset,1,Modifiers{control:true,..Default::default()},w,cx);}
    }
    pub(super) fn set_comfortable(&mut self,value:bool,cx:&mut Context<Self>){
        if self.prefs.comfortable==value{return;}
        self.prefs.comfortable=value;
        self.resource.layout_signature=0;
        self.resource.masonry_signature=0;
        self.persist_preferences(cx);cx.notify();
    }
    pub(super) fn upload_summary(&self,cx:&mut Context<Self>)->impl IntoElement{
        let total=self.queue.queue_total;
        let finished=total.saturating_sub(self.queue.queue_count);
        let p=self.palette();
        let bytes=self.queue.transfer.bytes.load(Ordering::Relaxed);
        let size=self.queue.transfer.total.load(Ordering::Relaxed);
        div().flex().items_center().gap_2().px_3().py_2().rounded_md().bg(p.surface)
            .child(div().flex_1().min_w(px(0.)).flex().flex_col().gap_1()
                .child(div().text_xs().text_color(p.text).child(if self.queue.preparing{"正在准备任务…".into()}else if self.queue.busy{format!("正在处理 · {finished}/{total} 项")}else{format!("{} 项可恢复任务",self.job_history.read(cx).recovery_count)}))
                .when(size>0&&self.queue.active_job.is_some(),|d|d.child(div().text_xs().text_color(p.muted).child(format!("当前文件已读取 {} / {} · 等待服务端确认",file_size((bytes.min(size)) as i64),file_size(size as i64)))))
                .when(self.queue.busy&&total>0,|d|d.child(gpui_kit::component::progress::Progress::new("batch-progress").xsmall().value(finished as f32/total as f32*100.).accessibility_label("批次处理进度"))))
            .child(Button::new("view-upload-jobs").small().ghost().label("查看任务").on_click(cx.listener(|v,_,w,cx|{v.command(Command::Navigate(Page::Jobs),w,cx);})))
    }
    fn navigate(&mut self,category:&'static str,favorites:bool,cx:&mut Context<Self>){
        self.navigate_page(Page::Gallery,cx);
        self.resource.similar_to=None;self.resource.tag_filter=None;self.resource.favorites_only=favorites;self.resource.category=category;
        self.persist_preferences(cx);self.reset_history(cx);
    }
    pub(super) fn navigation_rail(&self,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();
        let mut rail=div().size_full().flex().flex_col().items_center().gap_2().p_2().bg(p.sidebar).border_r_1().border_color(p.line)
            .child(div().debug_selector(||"expand-sidebar".into()).child(Button::new("expand-sidebar").ghost().icon(IconName::PanelLeftOpen).tooltip("展开侧栏").on_click(cx.listener(|v,_,_,cx|{v.sidebar_collapsed=false;cx.notify();}))));
        for (label,category,fav,icon) in [("全部资源","",false,IconName::FolderOpen),("图片","image",false,IconName::GalleryVerticalEnd),("收藏","",true,IconName::Star),("视频","video",false,IconName::Play),("附件","attachment",false,IconName::File)]{
            rail=rail.child(Button::new(label).ghost().icon(icon).tooltip(label).selected(self.page==Page::Gallery&&self.resource.category==category&&self.resource.favorites_only==fav&&self.resource.tag_filter.is_none()).on_click(cx.listener(move|v,_,_,cx|v.navigate(category,fav,cx))));
        }
        rail.child(div().flex_1())
            .child(Button::new("rail-jobs").ghost().icon(IconName::File).tooltip("上传任务").selected(self.page==Page::Jobs).on_click(cx.listener(|v,_,w,cx|{v.command(Command::Navigate(Page::Jobs),w,cx);})))
            .child(Button::new("rail-settings").ghost().icon(IconName::Settings).tooltip("设置").selected(self.page.settings()).on_click(cx.listener(|v,_,w,cx|{v.command(Command::Navigate(Page::Settings),w,cx);})))
    }
    pub(super) fn navigation(&self,narrow:bool,w:&mut Window,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();
        let section=|label:&str|div().px_2().pt_3().pb_1().text_xs().font_medium().text_color(p.muted).child(label.to_owned());
        let rows=[("nav-all","全部资源","",false,IconName::FolderOpen),("nav-images","图片","image",false,IconName::GalleryVerticalEnd),("nav-favorites","收藏","",true,IconName::Star),("nav-videos","视频","video",false,IconName::Play),("nav-attachments","附件","attachment",false,IconName::File)];
        let mut nav=div().flex().flex_col().gap(px(self.spacing().nav)).child(section("资源库"));
        for (id,label,cat,fav,icon) in rows{
            let active=self.page==Page::Gallery&&self.resource.category==cat&&self.resource.favorites_only==fav&&self.resource.tag_filter.is_none();
            nav=nav.child(div().debug_selector(move||id.into()).child(SidebarMenuItem::new(label).icon(icon).active(active)
                .on_click(cx.listener(move|v,_,_,cx|v.navigate(cat,fav,cx))).render(id,w,cx)));
        }
        div().id("navigation-sidebar").w(px(self.spacing().sidebar_width(narrow))).flex_shrink_0().h_full()
            .flex().flex_col().p(px(self.spacing().nav_pad)).bg(p.sidebar).border_r_1().border_color(p.line)
            .child(div().flex().items_center().gap(px(self.spacing().nav)).px_2().py_2()
                .child(div().size(px(26.)).rounded(px(7.)).bg(p.accent_soft).text_color(p.accent).flex().items_center().justify_center().child(IconName::FolderOpen))
                .child(div().flex().flex_col().min_w(px(0.)).child(div().font_semibold().truncate().child(if narrow{"语雀"}else{"Yuque Desk"}))
                    )
                .child(div().debug_selector(||"collapse-sidebar".into()).child(Button::new("collapse-sidebar").xsmall().ghost().icon(IconName::PanelLeftClose).tooltip("收起侧栏").on_click(cx.listener(|v,_,_,cx|{v.sidebar_collapsed=true;cx.notify();})))))
            .child(div().px_1().pb_3().child(Button::new("nav-account").small().ghost().icon(IconName::User).w_full()
                .label(if self.account.config.mode==Mode::Demo{"本地演示".to_owned()}else{format!("账号 {}",self.account.config.owner_id())})
                .selected(self.page==Page::Account).tooltip("账号管理").on_click(cx.listener(|v,_,w,cx|v.command(Command::Navigate(Page::Account),w,cx)))))
            .child(div().flex_1().min_h(px(0.)).flex().flex_col().gap_1().child(nav).child(section("分类")).child(self.tag_tree(w,cx)).overflow_y_scrollbar())
            .child(div().h(px(1.)).bg(p.line))
            .when(self.prefs.comfortable,|d|d.child(div().h(px(8.))))
            .child(div().debug_selector(||"nav-jobs".into()).child(SidebarMenuItem::new(if self.queue.queue_count>0{format!("上传任务 · {}",self.queue.queue_count)}else{"上传任务".into()})
                .icon(IconName::File).active(self.page==Page::Jobs).on_click(cx.listener(|v,_,w,cx|{v.command(Command::Navigate(Page::Jobs),w,cx);})).render("nav-jobs",w,cx)))
            .when(self.prefs.comfortable,|d|d.child(div().h(px(8.))))
            .child(div().debug_selector(||"nav-settings".into()).child(SidebarMenuItem::new("设置").icon(IconName::Settings).active(self.page.settings())
                .on_click(cx.listener(|v,_,w,cx|{v.command(Command::Navigate(Page::Settings),w,cx);})).render("nav-settings",w,cx)))

    }
    pub(super) fn settings_preferences(&self,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();
        div().flex().flex_col().gap(px(self.spacing().section)).border_b_1().border_color(p.line).pb_4()
            .child(div().text_sm().font_medium().child("外观与快捷操作"))
            .child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().child("深色主题").child(div().text_xs().text_color(p.muted).child("切换应用的整体配色")))
                .child(div().debug_selector(||"settings-theme".into()).child(Switch::new("theme-toggle").checked(self.dark).small().color(p.accent).accessibility_label("深色主题").on_change(cx.listener(|v,checked:&bool,w,cx|{v.dark=*checked;design::apply(v.dark,Some(w),cx);v.persist_preferences(cx);cx.notify();})))))
            .child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().child("舒适模式").child(div().text_xs().text_color(p.muted).child("增加图片、导航与表单留白；关闭后为紧凑模式")))
                .child(div().debug_selector(||"settings-comfortable".into()).child(Switch::new("comfortable-toggle").checked(self.prefs.comfortable).small().color(p.accent).accessibility_label("舒适模式").on_change(cx.listener(|v,checked:&bool,_,cx|v.set_comfortable(*checked,cx))))))
            .child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().child("全局快捷键").child(div().text_xs().text_color(p.muted).child("切换后重启生效")))
                .child(Button::new("shortcut-choice").small().label(if self.prefs.shortcut=="off"{"禁用"}else{self.prefs.shortcut.as_str()})
                    .dropdown_menu({
                        let weak=cx.weak_entity();let current=self.prefs.shortcut.clone();
                        move|menu,_,_|{
                            let mut menu=menu;
                            for (label,key) in [("快捷键 U","U"),("快捷键 I","I"),("禁用","off")]{
                                let weak=weak.clone();
                                menu=menu.item(PopupMenuItem::new(label).checked(current==key).on_click(move|_,_,cx|{let _=weak.update(cx,|v,cx|{v.prefs.shortcut=key.into();v.persist_preferences(cx);v.status="快捷键设置已更改，重启生效".into();cx.notify();});}));
                            }
                            menu
                        }
                    })))
            .child(div().h(px(1.)).bg(p.line))
            .child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().child("相册备份").child(div().text_xs().text_color(p.muted).child("保存本地资源索引与微缩图，不含凭据")))
                .child(Button::new("album-backup").small().label(if self.backup_busy{"备份中…"}else{"导出备份"}).disabled(self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.busy||self.account.account_busy)
                    .on_click(cx.listener(|v,_,_,cx|v.export_album(cx)))))
    }
    pub(super) fn gallery_toolbar(&self,narrow:bool,total:i64,_is_demo:bool,w:&Window,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();let weak=cx.weak_entity();
        let (sort,dates,all,fav)=(self.resource.oldest,self.resource.group_dates,self.resource.all_accounts,self.resource.favorites_only);
        let filter=Button::new("filter-menu").small().ghost().icon(IconName::Settings2).selected(all||fav).label("筛选")
            .dropdown_menu(move|m,_,_|m
                .item(action(weak.clone(),"仅收藏",fav,|v,_,cx|{v.resource.similar_to=None;v.resource.favorites_only=!v.resource.favorites_only;v.persist_preferences(cx);v.reset_history(cx);}))
                .item(action(weak.clone(),"全部账号",all,|v,_,cx|{v.resource.similar_to=None;v.resource.all_accounts=!v.resource.all_accounts;v.resource.tag_filter=None;v.reset_history(cx);})));
        let weak=cx.weak_entity();
        let mode=if !self.resource.grid_view{3}else if self.resource.masonry{2}else if self.resource.justified{1}else{0};
        let comfortable=self.prefs.comfortable;
        let layout=Button::new("layout-menu").small().ghost().icon(IconName::LayoutDashboard).label("视图")
            .dropdown_menu(move|m,_,_|m
                .item(action(weak.clone(),"等宽网格",mode==0,|v,_,cx|v.set_layout(0,cx)))
                .item(action(weak.clone(),"等高拼图",mode==1,|v,_,cx|v.set_layout(1,cx)))
                .item(action(weak.clone(),"瀑布流",mode==2,|v,_,cx|v.set_layout(2,cx)))
                .item(action(weak.clone(),"列表",mode==3,|v,_,cx|v.set_layout(3,cx)))
                .separator()
                .item(action(weak.clone(),"最新优先",!sort,|v,_,cx|{v.resource.oldest=false;v.persist_preferences(cx);v.reset_history(cx);}))
                .item(action(weak.clone(),"最早优先",sort,|v,_,cx|{v.resource.oldest=true;v.persist_preferences(cx);v.reset_history(cx);}))
                .item(action(weak.clone(),"日期分组",dates,|v,_,cx|{v.resource.group_dates=!v.resource.group_dates;v.persist_preferences(cx);cx.notify();}))
                .separator()
                .item(action(weak.clone(),"舒适模式",comfortable,|v,_,cx|v.set_comfortable(true,cx)))
                .item(action(weak.clone(),"紧凑模式",!comfortable,|v,_,cx|v.set_comfortable(false,cx))));
        let controls=div().flex().items_center().gap_1().flex_shrink_0()
            .child(div().debug_selector(||"filter-trigger".into()).child(filter))
            .child(div().debug_selector(||"layout-trigger".into()).child(layout))
            .child(div().h(px(20.)).w(px(1.)).bg(p.line))
            .child(div().debug_selector(||"toggle-inspector".into()).child(Button::new("toggle-inspector").small().ghost().icon(IconName::PanelRight).selected(self.inspector_open&&!self.immersive).tooltip("详情 · Cmd/Ctrl+I").on_click(cx.listener(|v,_,w,cx|v.command(Command::ToggleInspector,w,cx)))))
            .child(div().debug_selector(||"immersive-toggle".into()).child(Button::new("immersive-toggle").small().ghost().selected(self.immersive).icon(if self.immersive{IconName::Minimize}else{IconName::Maximize}).tooltip(if self.immersive{"退出沉浸"}else{"沉浸浏览"}).on_click(cx.listener(|v,_,w,cx|v.command(Command::ToggleImmersive,w,cx)))))
            .when(self.immersive,|d|d.child(Button::new("native-fullscreen").small().ghost().icon(if w.is_fullscreen(){IconName::Minimize}else{IconName::Maximize}).tooltip("系统全屏").on_click(cx.listener(|_,_,w,_|w.toggle_fullscreen()))))
            .child(Button::new("paste").small().ghost().icon(IconName::Copy).tooltip("粘贴上传").disabled(self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.busy||self.account.account_busy).on_click(cx.listener(|v,_,_,cx|v.paste_files(cx))))
            .child(div().debug_selector(||"choose-main".into()).child(Button::new("choose-main").small().primary().icon(IconName::Plus).label(if self.picker_pending{"选择中…"}else{"上传"}).tooltip("选择文件").disabled(self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.busy||self.account.account_busy).on_click(cx.listener(|v,_,_,cx|v.select_files(cx)))));
        let title=if fav{"收藏"}else{match self.resource.category{"image"=>"图片","video"=>"视频","attachment"=>"附件",_=>"全部资源"}};
        let heading=div().flex_1().min_w(px(0.)).flex().items_center().gap_2()
            .child(div().text_size(px(self.spacing().title)).font_semibold().truncate().child(title))
            .child(div().text_xs().text_color(p.muted).child(format!("{total} 项")));
        let base=div().id("drop-zone").flex().flex_col().gap_2().pb_3().border_b_1().border_color(p.line)
            .on_drop(cx.listener(|v,paths:&ExternalPaths,_,cx|v.start_upload(paths.paths().to_vec(),cx)));
        if narrow{
            base.child(div().flex().items_center().gap_2().child(heading).child(controls))
                .child(Input::new(&self.resource.search_input))
        }else{
            base.child(div().flex().items_center().gap_3().child(heading)
                .child(div().w(px(180.)).child(Input::new(&self.resource.search_input))).child(controls))
        }
    }
    pub(super) fn selection_toolbar(&self,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();let n=self.resource.selected_ids.len();let weak=cx.weak_entity();
        div().h(px(30.)).flex_shrink_0().flex().items_center().gap_2()
            .child(div().flex_1().min_w(px(0.)).truncate().text_xs().text_color(p.muted).child(if n>0{format!("已选择 {n} 项")}else if self.resource.all_accounts{"全部账号".into()}else{"Space 预览 · Shift 多选".into()}))
            .when(n>0,|d|d
                .child(Button::new("selection-copy").small().ghost().icon(IconName::Copy).label("复制链接").on_click(cx.listener(|v,_,_,cx|v.copy_selected(false,cx))))
                .child(Button::new("selection-more").small().ghost().label("更多").dropdown_menu(move|m,_,_|m
                    .item(action(weak.clone(),"复制 Markdown",false,|v,_,cx|v.copy_selected(true,cx)))
                    .item(action(weak.clone(),"批量标签",false,|v,_,cx|{v.resource.batch_tag_open=!v.resource.batch_tag_open;cx.notify();}))
                    .item(action(weak.clone(),"选择已加载（最多150）",false,|v,w,cx|v.command(Command::SelectAll,w,cx)))))
                .child(Button::new("clear-selection").small().ghost().label("取消").on_click(cx.listener(|v,_,w,cx|v.command(Command::ClearSelection,w,cx)))))
    }
    pub(super) fn density_control(&self)->impl IntoElement{
        div().flex().items_center().gap_2().text_xs().text_color(self.palette().muted)
            .child(format!("{:.0} px",self.resource.tile_size))
            .child(div().debug_selector(||"density-slider".into()).w(px(100.)).child(Slider::new(&self.resource.density).accessibility_label("图片尺寸").disabled(!self.resource.grid_view)))
    }
    fn set_layout(&mut self,mode:u8,cx:&mut Context<Self>){
        self.resource.grid_view=mode!=3;self.resource.justified=mode==1;self.resource.masonry=mode==2;self.persist_preferences(cx);cx.notify();
    }
}
