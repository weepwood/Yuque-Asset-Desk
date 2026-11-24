//! Product composition: native sidebar items, menus and compact toolbars.
use super::*;
use gpui_kit::component::{IconName,Sizable,menu::{DropdownMenu,PopupMenu,PopupMenuItem},sidebar::{SidebarItem,SidebarMenuItem}};
type DeskAction=fn(&mut UploadDesk,&mut Window,&mut Context<UploadDesk>);
fn action(weak:WeakEntity<UploadDesk>,label:&'static str,checked:bool,f:DeskAction)->PopupMenuItem{
    PopupMenuItem::new(label).checked(checked).on_click(move|_,w,cx|{let _=weak.update(cx,|v,cx|f(v,w,cx));})
}
pub(super) fn asset_menu(menu:PopupMenu,weak:WeakEntity<UploadDesk>,asset:Asset,starred:bool,checked:bool)->PopupMenu {
    let item=|label:&str,icon:IconName,f:fn(&mut UploadDesk,&Asset,&mut Window,&mut Context<UploadDesk>)|{
        let weak=weak.clone();let a=asset.clone();
        PopupMenuItem::new(label.to_owned()).icon(icon).on_click(move|_,w,cx|{let _=weak.update(cx,|v,cx|f(v,&a,w,cx));})
    };
    menu.item(item("复制链接",IconName::Copy,|v,a,_,cx|v.copy_asset(a,false,cx)))
        .item(item("复制 Markdown",IconName::Copy,|v,a,_,cx|v.copy_asset(a,true,cx)))
        .separator()
        .item(item("查看详情",IconName::PanelRight,|v,a,w,cx|{v.immersive=false;v.select_asset(a.clone(),w,cx)}))
        .item(item(if starred{"取消收藏"}else{"收藏"},IconName::Star,|v,a,_,cx|v.toggle_favorite(a,cx)))
        .item(item(if checked{"取消选择"}else{"选择"},IconName::Check,|v,a,_,cx|v.toggle_album_selection(a.id,cx)))
        .when(asset.category=="image",|m|m.item(item("查找相似图片",IconName::File,|v,a,_,cx|v.find_similar(a.clone(),cx))))
}
impl UploadDesk {
    pub(super) fn toggle_album_selection(&mut self,id:i64,cx:&mut Context<Self>){
        if !self.resource.selected_ids.remove(&id){
            if self.resource.selected_ids.len()<150{self.resource.selected_ids.insert(id);}else{self.status="单批最多选择 150 项".into();}
        }
        cx.notify();
    }
    fn navigate(&mut self,category:&'static str,favorites:bool,cx:&mut Context<Self>){
        self.jobs_page=false;self.account_panel=false;self.inspector_open=false;self.immersive=false;
        self.resource.similar_to=None;self.resource.tag_filter=None;self.resource.favorites_only=favorites;self.resource.category=category;
        self.persist_preferences(cx);self.reset_history(cx);
    }
    pub(super) fn navigation(&self,narrow:bool,w:&mut Window,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();
        let section=|label:&str|div().px_2().pt_3().pb_1().text_xs().font_medium().text_color(p.muted).child(label.to_owned());
        let rows=[("nav-all","全部资源","",false,IconName::FolderOpen),("nav-images","图片","image",false,IconName::File),("nav-favorites","收藏","",true,IconName::Star),("nav-videos","视频","video",false,IconName::File),("nav-attachments","附件","attachment",false,IconName::File)];
        let mut nav=div().flex().flex_col().gap_1().child(section("资源库"));
        for (id,label,cat,fav,icon) in rows{
            let active=!self.jobs_page&&!self.account_panel&&self.resource.category==cat&&self.resource.favorites_only==fav&&self.resource.tag_filter.is_none();
            nav=nav.child(div().debug_selector(move||id.into()).child(SidebarMenuItem::new(label).icon(icon).active(active)
                .on_click(cx.listener(move|v,_,_,cx|v.navigate(cat,fav,cx))).render(id,w,cx)));
        }
        div().id("navigation-sidebar").w(px(if narrow{design::SIDEBAR_NARROW}else{design::SIDEBAR_W})).flex_shrink_0().h_full()
            .flex().flex_col().p_2().bg(p.sidebar).border_r_1().border_color(p.line)
            .child(div().flex().items_center().gap_2().px_2().py_3()
                .child(div().size(px(26.)).rounded(px(7.)).bg(p.accent_soft).text_color(p.accent).flex().items_center().justify_center().child(IconName::FolderOpen))
                .child(div().flex().flex_col().min_w(px(0.)).child(div().font_semibold().truncate().child("Yuque Desk"))
                    .child(div().text_xs().text_color(p.muted).child("资源工作台"))))
            .child(div().flex_1().min_h(px(0.)).flex().flex_col().gap_1().child(nav).child(section("分类")).child(self.tag_tree(w,cx)).overflow_y_scrollbar())
            .child(div().h(px(1.)).bg(p.line))
            .child(div().debug_selector(||"nav-jobs".into()).child(SidebarMenuItem::new(if self.queue.queue_count>0{format!("上传任务 · {}",self.queue.queue_count)}else{"上传任务".into()})
                .icon(IconName::File).active(self.jobs_page&&!self.account_panel).on_click(cx.listener(|v,_,_,cx|{v.jobs_page=true;v.account_panel=false;v.inspector_open=false;v.refresh_jobs(cx);cx.notify();})).render("nav-jobs",w,cx)))
            .child(div().debug_selector(||"nav-settings".into()).child(SidebarMenuItem::new("设置").icon(IconName::Settings).active(self.account_panel)
                .on_click(cx.listener(|v,_,_,cx|{v.account_panel=true;v.jobs_page=false;v.inspector_open=false;cx.notify();})).render("nav-settings",w,cx)))
            .child(div().px_2().pt_2().text_xs().text_color(p.muted).truncate().child(if self.account.config.mode==Mode::Demo{"本地演示 · 离线"}else{"语雀账号 · 离线预览"}))
    }
    pub(super) fn settings_preferences(&self,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();
        div().flex().flex_col().gap_3()
            .child(div().text_sm().font_medium().child("外观与快捷操作"))
            .child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().child("界面主题"))
                .child(div().debug_selector(||"settings-theme".into()).child(Button::new("theme-toggle").small().icon(if self.dark{IconName::Sun}else{IconName::Moon}).label(if self.dark{"深色 · 切换浅色"}else{"浅色 · 切换深色"})
                    .on_click(cx.listener(|v,_,w,cx|{v.dark=!v.dark;design::apply(v.dark,Some(w),cx);v.persist_preferences(cx);cx.notify();})))))
            .child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().child("全局快捷键").child(div().text_xs().text_color(p.muted).child("切换后重启生效")))
                .child(Button::new("shortcut-choice").small().label(format!("{}",if self.prefs.shortcut=="off"{"禁用"}else{self.prefs.shortcut.as_str()}))
                    .on_click(cx.listener(|v,_,_,cx|{v.prefs.shortcut=match v.prefs.shortcut.as_str(){"U"=>"I","I"=>"off",_=>"U"}.into();v.persist_preferences(cx);v.status="快捷键设置已更改，重启生效".into();cx.notify();}))))
            .child(div().h(px(1.)).bg(p.line))
            .child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().child("相册备份").child(div().text_xs().text_color(p.muted).child("保存本地资源索引与微缩图，不含凭据")))
                .child(Button::new("album-backup").small().label(if self.backup_busy{"备份中…"}else{"导出备份"}).disabled(self.backup_busy||self.queue.busy||self.account.account_busy)
                    .on_click(cx.listener(|v,_,_,cx|v.export_album(cx)))))
    }
    pub(super) fn gallery_toolbar(&self,narrow:bool,total:i64,is_demo:bool,w:&Window,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();let weak=cx.weak_entity();
        let (sort,dates,all,fav)=(self.resource.oldest,self.resource.group_dates,self.resource.all_accounts,self.resource.favorites_only);
        let filter=Button::new("filter-menu").ghost().small().icon(IconName::SortDescending).label("筛选")
            .dropdown_menu(move|m,_,_|m
                .item(action(weak.clone(),"最早优先",sort,|v,_,cx|{v.resource.oldest=true;v.persist_preferences(cx);v.reset_history(cx);}))
                .item(action(weak.clone(),"最新优先",!sort,|v,_,cx|{v.resource.oldest=false;v.persist_preferences(cx);v.reset_history(cx);}))
                .separator()
                .item(action(weak.clone(),"日期分组",dates,|v,_,cx|{v.resource.group_dates=!v.resource.group_dates;v.persist_preferences(cx);cx.notify();}))
                .item(action(weak.clone(),"仅收藏",fav,|v,_,cx|{v.resource.similar_to=None;v.resource.favorites_only=!v.resource.favorites_only;v.persist_preferences(cx);v.reset_history(cx);}))
                .item(action(weak.clone(),"全部账号",all,|v,_,cx|{v.resource.similar_to=None;v.resource.all_accounts=!v.resource.all_accounts;v.resource.tag_filter=None;v.reset_history(cx);}))
                .separator()
                .item(action(weak.clone(),"选择已加载（最多150项）",false,|v,_,cx|{v.resource.selected_ids=v.resource.assets.iter().take(150).map(|a|a.id).collect();cx.notify();})));
        let weak=cx.weak_entity();
        let mode=if !self.resource.grid_view{3}else if self.resource.masonry{2}else if self.resource.justified{1}else{0};
        let layout=Button::new("layout-menu").ghost().small().icon(IconName::PanelRight).label(match mode{1=>"等高",2=>"瀑布流",3=>"列表",_=>"网格"})
            .dropdown_menu(move|m,_,_|m
                .item(action(weak.clone(),"等宽网格",mode==0,|v,_,cx|v.set_layout(0,cx)))
                .item(action(weak.clone(),"等高拼图",mode==1,|v,_,cx|v.set_layout(1,cx)))
                .item(action(weak.clone(),"瀑布流",mode==2,|v,_,cx|v.set_layout(2,cx)))
                .item(action(weak.clone(),"列表",mode==3,|v,_,cx|v.set_layout(3,cx)))
                .separator()
                .item(action(weak.clone(),"显示 / 隐藏详情",false,|v,_,cx|{v.immersive=false;v.inspector_open=!v.inspector_open;cx.notify();})));
        let controls=div().flex().items_center().gap_1()
            .child(div().debug_selector(||"filter-trigger".into()).child(filter)).child(div().debug_selector(||"layout-trigger".into()).child(layout))
            .child(Button::new("tile-smaller").small().ghost().icon(IconName::Minus).tooltip("缩小图片").disabled(self.resource.tile_size<=96.||!self.resource.grid_view).on_click(cx.listener(|v,_,_,cx|{v.resource.tile_size=(v.resource.tile_size-16.).max(96.);v.persist_preferences(cx);cx.notify();})))
            .child(div().debug_selector(||"album-density-increase".into()).child(Button::new("tile-larger").small().ghost().icon(IconName::Plus).tooltip("放大图片").disabled(self.resource.tile_size>=240.||!self.resource.grid_view).on_click(cx.listener(|v,_,_,cx|{v.resource.tile_size=(v.resource.tile_size+16.).min(240.);v.persist_preferences(cx);cx.notify();}))))
            .child(div().debug_selector(||"immersive-toggle".into()).child(Button::new("immersive-toggle").small().ghost().icon(if self.immersive{IconName::Minimize}else{IconName::Maximize}).tooltip(if self.immersive{"退出图片墙"}else{"沉浸图片墙"}).on_click(cx.listener(|v,_,_,cx|{v.immersive=!v.immersive;cx.notify();}))))
            .when(self.immersive,|d|d.child(Button::new("native-fullscreen").small().ghost().icon(if w.is_fullscreen(){IconName::Minimize}else{IconName::Maximize}).tooltip("系统全屏").on_click(cx.listener(|_,_,w,_|w.toggle_fullscreen()))))
            .child(Button::new("paste").small().ghost().icon(IconName::Copy).tooltip("粘贴上传").disabled(self.queue.busy||self.account.account_busy).on_click(cx.listener(|v,_,_,cx|v.paste_files(cx))))
            .child(Button::new("choose-main").small().primary().icon(IconName::Plus).label("上传").tooltip("选择文件").disabled(self.queue.busy||self.account.account_busy).on_click(cx.listener(|v,_,_,cx|v.select_files(cx))));
        let title=if self.resource.favorites_only{"收藏"}else{match self.resource.category{"image"=>"图片","video"=>"视频","attachment"=>"附件",_=>"全部资源"}};
        div().id("drop-zone").flex().flex_col().gap_2().pb_2()
            .on_drop(cx.listener(|v,paths:&ExternalPaths,_,cx|v.start_upload(paths.paths().to_vec(),cx)))
            .child(div().flex().items_center().gap_3()
                .child(div().flex_1().min_w(px(0.)).child(div().text_lg().font_semibold().child(title))
                    .child(div().text_xs().text_color(p.muted).child(format!("{} 项 · {}",total,if is_demo{"本地演示"}else{"离线预览"}))))
                .when(!narrow,|d|d.child(div().w(px(180.)).child(Input::new(&self.resource.search_input))))
                .child(controls))
            .when(narrow,|d|d.child(Input::new(&self.resource.search_input)))
    }
    fn set_layout(&mut self,mode:u8,cx:&mut Context<Self>){
        self.resource.grid_view=mode!=3;self.resource.justified=mode==1;self.resource.masonry=mode==2;self.persist_preferences(cx);cx.notify();
    }
}
