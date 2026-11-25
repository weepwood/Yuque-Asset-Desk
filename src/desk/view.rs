use super::*;
use gpui_kit::component::{IconName, Sizable, empty::{Empty,EmptyHeader,EmptyMedia,EmptyMediaVariant,EmptyTitle,EmptyDescription}, menu::ContextMenuExt, sidebar::{SidebarItem,SidebarMenuItem}};

impl UploadDesk {
    pub(super) fn spacing(&self)->design::Spacing{design::Spacing::new(self.prefs.comfortable)}
    pub(super) fn palette(&self)->design::Palette{design::Palette::new(self.dark)}

    pub(super) fn tag_tree(&self,w:&mut Window,cx:&mut Context<Self>) -> impl IntoElement {
        let mut nodes=std::collections::BTreeSet::new();
        for tag in &self.resource.tag_catalog {
            let mut path=String::new();
            for part in tag.split('/') {
                if !path.is_empty(){path.push('/');}
                path.push_str(part);
                nodes.insert(path.clone());
            }
        }
        let mut tree=div().flex().flex_col().gap(px(self.spacing().nav))
            .child(SidebarMenuItem::new("标签").icon(if self.resource.tags_expanded{IconName::ChevronDown}else{IconName::ChevronRight})
                .on_click(cx.listener(|v,_,_,cx|{v.resource.tags_expanded=!v.resource.tags_expanded;cx.notify();})).render("tags-toggle",w,cx));
        if self.resource.tags_expanded {
            tree=tree.child(SidebarMenuItem::new("全部标签").active(self.resource.tag_filter.is_none())
                .on_click(cx.listener(|v,_,_,cx|{v.navigate_page(Page::Gallery,cx);v.resource.tag_filter=None;v.reset_history(cx);})).render("tag-all",w,cx));
            for (i,path) in nodes.into_iter().enumerate() {
                let depth=path.matches('/').count().min(5);
                let title=path.rsplit('/').next().unwrap_or(&path).to_string();
                let selected=self.resource.tag_filter.as_ref()==Some(&path);
                tree=tree.child(div().pl(px(depth as f32*10.+8.)).child(SidebarMenuItem::new(title).icon(IconName::FolderOpen).active(selected)
                    .on_click(cx.listener(move |v,_,_,cx|{v.navigate_page(Page::Gallery,cx);v.resource.similar_to=None;v.resource.tag_filter=Some(path.clone());v.reset_history(cx);})).render(("tag-node",i),w,cx)));
            }
        }
        tree
    }

    pub(super) fn task_center(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().id("jobs-list").flex_1().min_h(px(0.)).flex().flex_col();
        for job in &self.queue.jobs {
            let id = job.id;
            let active = self.queue.active_job == Some(id);
            let state = if active { "处理中" } else { match job.state.as_str() {
                "done" => "完成", "failed" => "失败", "pending" => "待处理",
                "paused" => "暂停", "cancelled" => "取消", "running" | "unknown" => "结果待核实", _ => "未知",
            }};
            let retry = matches!(job.state.as_str(), "pending" | "paused" | "failed");
            let total = self.queue.transfer.total.load(Ordering::Relaxed);
            let bytes = self.queue.transfer.bytes.load(Ordering::Relaxed).min(total);
            let mut row = div().flex().flex_col().gap_2().p(px(self.spacing().row))
                .border_b_1().border_color(self.palette().line)
                .child(div().flex().items_center().gap_2()
                    .child(div().flex_1().min_w(px(0.)).font_semibold().truncate().child(job.path.file_name().unwrap_or_default().to_string_lossy().into_owned()))
                    .child(div().text_xs().text_color(if job.state=="failed"||job.state=="unknown"{self.palette().error}else if job.state=="done"{self.palette().accent}else{self.palette().muted}).child(state))
                    .when(retry, |d| d.child(Button::new(("retry-job",id as usize)).small().ghost()
                        .label(if job.state=="failed"{"重试"}else{"继续"}).tooltip("重试前请确认远端结果").disabled(self.queue.busy || self.account.account_busy)
                        .on_click(cx.listener(move |view, _, _, cx| view.retry_one(id,cx)))))
                    .when(matches!(job.state.as_str(),"unknown"|"running") && !active, |d| d.child(Button::new(("ack-job",id as usize)).small()
                        .label("已确认远端无文件").disabled(self.queue.busy || self.account.account_busy)
                        .on_click(cx.listener(move |v,_,_,cx| v.acknowledge_job(id,cx))))));
            if active {
                row = row.child(if total == 0 { "检查文件／去重中".into() }
                    else { format!("已读取请求体 {} / {}，不代表服务端已确认",file_size(bytes as i64),file_size(total as i64)) });
                if total > 0 {
                    row = row.child(gpui_kit::component::progress::Progress::new(("upload-progress",id as usize)).xsmall().value(bytes as f32 / total as f32 * 100.).accessibility_label("请求体读取进度"));
                }
            }
            if !job.error.is_empty() { row = row.child(div().text_sm().text_color(self.palette().error).child(job.error.clone())); }
            list = list.child(row);
        }
        div().flex_1().min_w(px(0.)).flex().flex_col().gap_3().p(px(self.spacing().pad))
            .child(div().flex().items_center().gap_2()
                .child(div().flex_1().text_lg().font_semibold().child("上传任务"))
                .child(Button::new("refresh-jobs").small().ghost().label("刷新").on_click(cx.listener(|v,_,_,cx| v.refresh_jobs(cx))))
                .child(Button::new("resume-jobs").small().label("恢复任务").disabled(self.queue.busy || self.account.account_busy || self.queue.recovery_count == 0)
                    .on_click(cx.listener(|v,_,_,cx| v.resume_jobs(cx)))))
            .child(div().text_sm().child(format!("当前账号：{} / {} · 未完成 {}",self.account.config.provider(),self.account.config.owner_id(),self.queue.recovery_count)))
            .when(self.queue.busy, |d| d.child(div().flex().gap_2()
                .child(Button::new("jobs-pause").label("文件结束后暂停").on_click(cx.listener(|v,_,_,cx| { let _ = v.queue.queue_control.compare_exchange(0,1,Ordering::Relaxed,Ordering::Relaxed); cx.notify(); })))
                .child(Button::new("jobs-cancel").label("取消剩余").on_click(cx.listener(|v,_,_,cx| { v.queue.queue_control.store(2,Ordering::Relaxed); cx.notify(); })))
                .child(Button::new("cancel-active").label("中止当前读取")
                    .disabled(self.queue.active_job.is_none())
                    .on_click(cx.listener(|v,_,_,cx| {v.queue.queue_control.store(1,Ordering::Relaxed);v.queue.transfer.cancel.store(true,Ordering::Relaxed);v.status="已请求中止，远端结果可能未知".into();cx.notify();})))))
            .when(self.queue.jobs_loading, |d| d.child("正在读取任务…"))
            .when(self.queue.jobs.is_empty() && !self.queue.jobs_loading && self.queue.jobs_error.is_none(), |d| d.child("暂无任务记录"))
            .when(self.queue.jobs_error.is_some(), |d| d.child(self.queue.jobs_error.clone().unwrap_or_default()))
            .child(list.overflow_y_scrollbar())
            .child(div().flex().items_center().gap_2()
                .child(Button::new("jobs-prev").label("上一页").disabled(self.queue.jobs_loading || self.queue.job_cursors.len() <= 1)
                    .on_click(cx.listener(|v,_,_,cx| { if v.queue.job_cursors.len()>1 {v.queue.job_cursors.pop(); v.queue.jobs.clear(); v.queue.jobs_more=false; v.refresh_jobs(cx); cx.notify();} })))
                .child(format!("第 {} 页", self.queue.job_cursors.len()))
                .child(Button::new("jobs-next").label("下一页").disabled(self.queue.jobs_loading || !self.queue.jobs_more)
                    .on_click(cx.listener(|v,_,_,cx| { if v.queue.jobs_more { if let Some(last)=v.queue.jobs.last() {v.queue.job_cursors.push(Some(last.id)); v.queue.jobs.clear(); v.queue.jobs_more=false; v.refresh_jobs(cx); cx.notify();} } }))))
            .child(div().text_sm().child(self.status.clone()))
    }

    pub(super) fn inspector(&self, narrow: bool, active:bool, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div().id("inspector").h_full().min_w(px(0.)).min_h(px(0.)).flex().flex_col().gap(px(self.spacing().section)).p(px(self.spacing().pad)).when(!narrow, |d| d.w(px(self.spacing().inspector)).flex_shrink_0()).when(narrow, |d| d.w_full()).bg(self.palette().sidebar).border_l_1().border_color(self.palette().line);
        if self.resource.selected_ids.len()>1{
            panel=panel.child(div().flex().items_center().justify_between()
                .child(div().font_semibold().child("批量选择"))
                .child(Button::new("close-inspector-multi").disabled(!active).ghost().small().icon(IconName::PanelRightClose).tooltip("关闭详情").on_click(cx.listener(|v,_,w,cx|{v.inspector_open=false;v.resource.gallery_focus.focus(w,cx);cx.notify();}))))
                .child(div().text_lg().font_medium().child(format!("{} 项资源",self.resource.selected_ids.len())))
                .child(Button::new("multi-copy").disabled(!active).small().label("复制链接").on_click(cx.listener(|v,_,_,cx|v.copy_selected(false,cx))))
                .child(Button::new("multi-md").disabled(!active).small().ghost().label("复制 Markdown").on_click(cx.listener(|v,_,_,cx|v.copy_selected(true,cx))))
                .child(div().text_xs().text_color(self.palette().muted).child("批量标签请在图库选择工具栏操作。"));
        }else if let Some(asset) = &self.resource.selected_asset {
            let url_asset = asset.clone();
            let md_asset = asset.clone();
            panel = panel.child(div().flex().items_center().justify_between().child(div().text_size(px(self.spacing().title)).font_semibold().child("详情"))
                .child(Button::new("close-inspector").disabled(!active).ghost().small().icon(IconName::PanelRightClose).tooltip("关闭详情").on_click(cx.listener(|v,_,w,cx|{v.inspector_open=false;v.resource.gallery_focus.focus(w,cx);cx.notify();}))));
            if asset.category=="image"{
                panel=panel.child(div().h(px(if self.prefs.comfortable{208.}else{184.})).w_full().rounded(px(design::CONTROL_RADIUS)).overflow_hidden().bg(self.palette().subtle).flex().items_center().justify_center()
                    .child(if let Some(path)=&self.resource.thumbnail_path{img(path.clone()).image_cache(&self.resource.image_cache).size_full().object_fit(ObjectFit::Contain).into_any_element()}else{div().text_xs().text_color(self.palette().muted).child(if self.resource.preview_loading{"读取本地预览…"}else{"无本地预览"}).into_any_element()}))
                    .child(Button::new("large-preview").small().ghost().icon(IconName::Maximize).label("沉浸式预览").disabled(!active||self.resource.preview_loading).on_click(cx.listener(|v,_,w,cx|v.open_large_preview(w,cx))));
                if self.resource.thumbnail_path.is_none()&&!self.resource.preview_loading{
                    panel=panel.child(Button::new("preview-image").disabled(!active).small().label("重新读取本地预览").on_click(cx.listener(|v,_,_,cx|v.load_preview(cx))));
                    if let Some(error)=&self.resource.preview_error{panel=panel.child(div().text_xs().text_color(self.palette().error).child(error.clone()));}
                }
            }
            panel=panel.child(div().flex().flex_col().gap(px(if self.prefs.comfortable{6.}else{self.spacing().section}))
                .child(div().font_semibold().child(asset.name.clone()))
                .child(div().text_xs().text_color(self.palette().muted).child(format!("{} · {}",asset.category,file_size(asset.bytes)))))
                .child(Button::new("detail-url").disabled(!active).small().icon(IconName::Copy).label("复制链接")
                    .on_click(cx.listener(move |view, _, w, cx| view.copy_asset(&url_asset, false,w,cx))))
                .child(div().flex().flex_wrap().gap(px(self.spacing().control))
                    .child(Button::new("detail-open").disabled(!active).small().ghost().label(if asset.provider=="demo" {"打开文件"} else {"浏览器打开"}).on_click(cx.listener(|v,_,_,cx|v.open_selected_resource(cx))))
                    .child(Button::new("detail-md").disabled(!active).small().ghost().label("Markdown").on_click(cx.listener(move |view, _, w, cx| view.copy_asset(&md_asset, true,w,cx)))))
                .child(div().h(px(1.)).bg(self.palette().line))
                .child(div().text_xs().font_medium().text_color(self.palette().muted).child("标签"));
            panel = panel.child(Input::new(&self.inspector_model.read(cx).input).disabled(!active||!self.inspector_model.read(cx).ready||self.resource_write_busy))
                .child(Button::new("save-tags").small().label("保存标签").disabled(!active||!self.inspector_model.read(cx).ready||self.resource_write_busy)
                    .on_click(cx.listener(|v,_,_,cx|v.save_selected_tags(cx))));
            if let Some(error)=&self.inspector_model.read(cx).error{
                panel=panel.child(div().text_xs().text_color(self.palette().error).child(error.clone()))
                    .child(Button::new("retry-tags").disabled(!active).small().ghost().label("重试读取标签").on_click(cx.listener(|v,_,w,cx|{if let Some(a)=v.resource.selected_asset.clone(){let storage=v.storage.clone();v.inspector_model.update(cx,|m,cx|m.select(a.id,storage,w,cx));}})));
            }
            let favorite_asset=asset.clone();
            let similar_asset=asset.clone();
            panel=panel.child(div().flex().flex_wrap().items_center().gap(px(self.spacing().control))
                .child(Button::new("detail-favorite").disabled(!active).small().ghost().icon(if self.resource.favorites.contains(&asset.id){IconName::StarFill}else{IconName::Star}).label(if self.resource.favorites.contains(&asset.id){"取消收藏"}else{"收藏资源"})
                    .on_click(cx.listener(move |v,_,_,cx|v.toggle_favorite(&favorite_asset,cx))))
                .when(asset.category=="image",|d|d.child(Button::new("detail-similar").disabled(!active).small().ghost().icon(IconName::Search).label("相似图片")
                    .on_click(cx.listener(move |v,_,_,cx|v.find_similar(similar_asset.clone(),cx))))));
            panel=panel.child(div().h(px(1.)).bg(self.palette().line))
                .child(div().text_xs().font_medium().text_color(self.palette().muted).child("文件属性"))
                .child(div().flex().items_center().justify_between().gap_2().text_xs()
                    .child(div().text_color(self.palette().muted).child("大小")).child(file_size(asset.bytes)))
                .child(div().flex().items_center().justify_between().gap_2().text_xs()
                    .child(div().text_color(self.palette().muted).child("来源")).child(div().truncate().child(format!("{} / {}",asset.provider,asset.owner_id))))
                .child(div().flex().items_center().justify_between().gap_2().text_xs()
                    .child(div().text_color(self.palette().muted).child("上传时间")).child(asset.created_at.get(..16).unwrap_or(&asset.created_at).replace("T"," ")))
                .child(div().text_xs().text_color(self.palette().muted).truncate().child(format!("SHA-256: {}",asset.sha256)))
                .child(div().text_xs().text_color(self.palette().muted).truncate().child(format!("File key: {}",asset.filekey)))
                .child(div().text_xs().text_color(self.palette().muted).truncate().child(asset.url.clone()));
        } else {
            panel = panel.child(div().flex().justify_between().items_center().child("资源详情").child(Button::new("close-inspector-empty").disabled(!active).ghost().small().icon(IconName::PanelRightClose).tooltip("关闭详情").on_click(cx.listener(|v,_,w,cx|{v.inspector_open=false;v.resource.gallery_focus.focus(w,cx);cx.notify();}))))
                .child(div().text_sm().child("选择列表或网格中的资源，查看属性及复制链接。"));
        }
        panel.overflow_y_scrollbar()
    }
}

impl UploadDesk {
    fn album_item(&self,asset:Asset,tile_width:f32,tile_image_height:f32,narrow:bool,window:&mut Window,cx:&mut Context<Self>)->AnyElement {
                let index = asset.id as usize;
                let url_asset = asset.clone();
                let markdown_asset = asset.clone();
                let detail_asset = asset.clone();
                let selected = self.resource.selected_asset.as_ref().is_some_and(|a| a.id == asset.id);
                let checked = self.resource.selected_ids.contains(&asset.id);
                let asset_id = asset.id;
                if self.resource.grid_view {
                    let starred=self.resource.favorites.contains(&asset.id);
                    let hovered=self.resource.hover_asset==Some(asset.id);
                    let revealed=hovered||checked||selected;
                    let hover_alpha=design::tween(ElementId::from(("album-hover",index)),if revealed{1.}else{0.},design::HOVER_MS,window,cx);
                    let thumbnail = self.resource.cached_thumbnails.get(&asset.id).cloned();
                    let palette=self.palette();
                    let menu_asset=asset.clone();
                    let weak=cx.weak_entity();
                    return div().id(("album-tile",index)).debug_selector(move||format!("tile-{index}")).relative().flex().flex_col()
                        .w(px(tile_width)).h(px(tile_image_height+self.spacing().footer)).flex_shrink_0()
                            .on_click(cx.listener(move |v,e:&ClickEvent,window,cx|v.click_tile(detail_asset.clone(),e.click_count(),e.modifiers(),window,cx)))
                        .on_hover(cx.listener(move |v,hovered:&bool,_,cx|{
                            let old=v.resource.hover_asset;
                            if *hovered {v.resource.hover_asset=Some(asset_id);}
                            else if old==Some(asset_id){v.resource.hover_asset=None;}
                            if old!=v.resource.hover_asset{cx.notify();}
                        }))
                        .child(div().id(("album-image",index)).relative().h(px(tile_image_height)).w_full().overflow_hidden()
                            .rounded(px(design::CONTROL_RADIUS)).bg(if thumbnail.is_some(){palette.canvas}else{palette.subtle}).flex().items_center().justify_center()
                            .child(match thumbnail {
                                Some(path)=>img(path).image_cache(&self.resource.image_cache).size_full().object_fit(ObjectFit::Contain).into_any_element(),
                                None=>div().flex().flex_col().items_center().gap_1().text_color(palette.muted)
                                    .child(if asset.category=="video"{IconName::Play}else if asset.category=="image"{IconName::GalleryVerticalEnd}else{IconName::File})
                                    .child(div().text_xs().child(if asset.category=="image" {"无本地预览"} else if asset.category=="video" {"视频"} else {"附件"})).into_any_element()
                            })
                            .when(checked||selected,|d|d.child(div().absolute().inset_0().rounded(px(design::CONTROL_RADIUS)).border_2().border_color(palette.accent)))
                            .when(revealed,|d|d.child(div().absolute().top(px(5.)).left(px(5.)).opacity(0.72+0.28*hover_alpha)
                                .child(Button::new(("grid-select",index)).xsmall().selected(checked).icon(if checked{IconName::Check}else{IconName::Square}).tooltip("多选")
                                    .on_click(cx.listener(move|v,_,w,cx|v.toggle_album_selection(asset_id,w,cx))))))
                            .when(starred,|d|d.child(div().absolute().top(px(6.)).right(px(6.)).p_1().rounded_sm().bg(palette.surface).text_color(palette.accent).child(IconName::StarFill))))
                        .child(div().h(px(self.spacing().footer)).flex().flex_col().justify_center().items_center().gap_0p5()
                            .child(div().max_w_full().truncate().text_size(px(self.spacing().caption)).text_color(if selected||checked{palette.accent}else{palette.text}).child(asset.name.clone()))
                            .child(div().text_xs().text_color(palette.muted).child(file_size(asset.bytes))))
                        .context_menu(move|menu,_,_|super::workspace::asset_menu(menu,weak.clone(),menu_asset.clone(),starred,checked))
                        .into_any_element();
                }
                let row_asset=asset.clone();let menu_asset=asset.clone();let weak=cx.weak_entity();
                let starred=self.resource.favorites.contains(&asset.id);
                div().id(("album-row",index)).debug_selector(move||format!("tile-{index}"))
                    .on_click(cx.listener(move|v,e:&ClickEvent,w,cx|v.click_tile(row_asset.clone(),e.click_count(),e.modifiers(),w,cx)))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .p(px(self.spacing().row))
                    .rounded_md()
                    .bg(if checked||selected{self.palette().accent_soft}else{self.palette().canvas})
                    .hover(|style| style.bg(self.palette().subtle))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .flex_1()
                            .child(div().font_semibold().text_sm().truncate().child(asset.name.clone()))
                            .child(div().text_sm().text_color(self.palette().muted).truncate().child(format!(
                                "{} · {} · {}",
                                file_size(asset.bytes), if self.resource.all_accounts {format!("{} / {}",asset.provider,asset.owner_id)} else {asset.category.clone()},
                                asset.created_at.get(..16).unwrap_or(&asset.created_at).replace("T"," ")
                            ))),
                    )
                    .child(Button::new(("row-select", index)).small().selected(checked)
                        .label(if checked { "已选" } else { "选择" })
                        .on_click(cx.listener(move |view, _, w, cx|view.toggle_album_selection(asset_id,w,cx))))
                    .child(Button::new(("detail", index)).small().selected(selected).label("详情")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.show_asset_details(detail_asset.clone(), window, cx);
                        })))
                    .when(!narrow, |d| d.child(
                        Button::new(("url", index)).small()
                            .label("复制 URL")
                            .on_click(cx.listener(move |view, _, w, cx| {
                                view.copy_asset(&url_asset, false,w,cx);
                            })),
                    ))
                    .when(!narrow, |d| d.child(
                        Button::new(("markdown", index)).small()
                            .label("Markdown")
                            .on_click(cx.listener(move |view, _, w, cx| {
                                view.copy_asset(&markdown_asset, true,w,cx);
                            })),
                    ))
                    .context_menu(move|menu,_,_|super::workspace::asset_menu(menu,weak.clone(),menu_asset.clone(),starred,checked))
                    .into_any_element()
    }

    fn album_row(&self,row:&album::AlbumRow,narrow:bool,window:&mut Window,cx:&mut Context<Self>)->AnyElement {
        match row {
            album::AlbumRow::Day(day)=>div().h(px(self.spacing().day)).flex().items_center().text_size(px(self.spacing().caption)).font_medium().text_color(self.palette().muted).child(day.clone()).into_any_element(),
            album::AlbumRow::Item(i)=>div().pb_2().child(self.album_item(self.resource.assets[*i].clone(),0.,0.,narrow,window,cx)).into_any_element(),
            album::AlbumRow::Tiles{items,height}=>div().w_full().flex().items_start().gap(px(self.spacing().gap)).pb(px(self.spacing().gap))
                .children(items.iter().map(|(i,w)|self.album_item(self.resource.assets[*i].clone(),*w,*height,narrow,window,cx)).collect::<Vec<_>>()).into_any_element(),
        }
    }
}

impl UploadDesk {
    #[inline(never)]
    fn preview_view(&mut self,surface_alpha:f32,window:&mut Window,cx:&mut Context<Self>)->AnyElement{
            // Each preview owns a fresh channel; closing never drives a hidden animation.
            let zoom=design::tween(ElementId::from(("preview-zoom",self.resource.zoom_channel)),self.resource.zoom,design::ZOOM_MS,window,cx);
            self.resource.displayed_zoom=zoom;
            let (canvas_w,canvas_h,ratio)=self.preview_size(window,cx);
            let (image_w,image_h)=album::fit_preview(canvas_w,canvas_h,ratio,zoom);
            let pan=album::clamp_pan((canvas_w,canvas_h),(image_w,image_h),(f32::from(self.resource.pan.x),f32::from(self.resource.pan.y)));
            let previous=self.preview_neighbor(false).is_some();let next=self.resource.pending_preview.is_none()&&(self.preview_neighbor(true).is_some()||(self.resource.has_more&&self.resource.history_error.is_none()));
            if !self.resource.loading_history{
                if let Some((id,left))=self.resource.pending_preview{cx.defer_in(window,move|v,w,cx|{if v.resource.pending_preview==Some((id,left))&&!v.resource.loading_history{v.resource.pending_preview=None;v.continue_preview_page(id,left,w,cx);}});}
            }
            div().id("preview-root").track_focus(&self.resource.preview_focus).size_full().flex().flex_col().p_4().gap_3().opacity(surface_alpha)
                .on_key_down(cx.listener(|v,e:&KeyDownEvent,w,cx|{
                    if e.keystroke.modifiers.control||e.keystroke.modifiers.platform||e.keystroke.modifiers.alt{return;}
                    match e.keystroke.key.as_str(){
                        "escape"=>v.close_preview(w,cx),
                        "left"=>v.step_preview(false,w,cx),
                        "right"=>v.step_preview(true,w,cx),
                        "+"|"="=>v.zoom_by(0.25,cx),
                        "-"=>v.zoom_by(-0.25,cx),
                        "0"=>{v.resource.zoom=1.;v.resource.pan=point(px(0.),px(0.));cx.notify();},
                        _=>return,
                    }
                    cx.stop_propagation();
                }))
                .bg(self.palette().canvas).text_color(self.palette().text)
                .child(div().h(px(32.)).flex_shrink_0().flex().items_center().gap_2()
                    .child(Button::new("close-preview").ghost().small().icon(IconName::ArrowLeft).label("返回").on_click(cx.listener(|v,_,w,cx| {v.close_preview(w,cx);})))
                    .child(div().flex_1().font_semibold().truncate().child(self.resource.selected_asset.as_ref().map(|a|a.name.clone()).unwrap_or_default()))
                    .child(Button::new("preview-prev").ghost().small().icon(IconName::ChevronLeft).tooltip("上一张 · ←").disabled(!previous).on_click(cx.listener(|v,_,w,cx|v.step_preview(false,w,cx))))
                    .child(Button::new("preview-next").ghost().small().icon(IconName::ChevronRight).tooltip("下一张 · →").disabled(!next).on_click(cx.listener(|v,_,w,cx|v.step_preview(true,w,cx))))
                    .child(Button::new("zoom-out").ghost().small().icon(IconName::Minus).on_click(cx.listener(|v,_,_,cx|v.zoom_by(-0.25,cx))))
                    .child(format!("{:.0}%",zoom*100.))
                    .child(Button::new("zoom-in").ghost().small().icon(IconName::Plus).on_click(cx.listener(|v,_,_,cx|v.zoom_by(0.25,cx))))

                    .child(Button::new("zoom-reset").ghost().small().label("适应窗口").on_click(cx.listener(|v,_,_,cx|{v.resource.zoom=1.;v.resource.pan=point(px(0.),px(0.));cx.notify();}))))
                .child({
                    let panel=div().id("preview-canvas").bg(self.palette().sidebar).rounded(px(design::CONTROL_RADIUS)).relative().flex_1().min_h(px(0.)).overflow_hidden().flex().items_center().justify_center()
                        .debug_selector(||"preview-canvas".into())
                        .on_scroll(cx.listener(|v,e:&ScrollEvent,w,cx|{
                            let dy=f32::from(e.delta.pixel_delta(px(20.)).y);
                            if dy.abs()>0.01{v.zoom_at((dy*0.003).clamp(-0.5,0.5),e.position,w,cx);cx.stop_propagation();}
                        }))
                        .on_mouse_down(MouseButton::Left,cx.listener(|v,e:&MouseDownEvent,w,cx|{
                            v.resource.preview_focus.focus(w,cx);
                            v.resource.drag_at=Some(e.position);
                            v.resource.zoom=v.resource.displayed_zoom;
                            v.resource.zoom_channel=v.resource.zoom_channel.wrapping_add(1);
                            cx.notify();
                        }))
                        .on_mouse_move(cx.listener(|v,e:&MouseMoveEvent,w,cx|{
                            if e.dragging(){v.drag_preview(e.position,w,cx);}else{v.resource.drag_at=None;}
                        }))
                        .on_mouse_up(MouseButton::Left,cx.listener(|v,_,_,cx|{v.resource.drag_at=None;cx.notify();}))
                        .on_mouse_up_out(MouseButton::Left,cx.listener(|v,_,_,cx|{v.resource.drag_at=None;cx.notify();}));
                    if let Some(q)=&self.viewer_model.read(cx).quality {panel.child(div().absolute()
                        .left(px((canvas_w-image_w)*0.5)+px(pan.0)).top(px((canvas_h-image_h)*0.5)+px(pan.1)).w(px(image_w)).h(px(image_h))
                        .child(img(q.image.clone()).w(px(image_w)).h(px(image_h)).object_fit(ObjectFit::Contain)))}
                    else if let Some(path)=&self.resource.large_preview_path { panel.child(div().absolute()
                        .left(px((canvas_w-image_w)*0.5)+px(pan.0))
                        .top(px((canvas_h-image_h)*0.5)+px(pan.1))
                        .w(px(image_w)).h(px(image_h))
                        .child(img(path.clone()).image_cache(&self.resource.image_cache).w(px(image_w)).h(px(image_h)).object_fit(ObjectFit::Contain))) }
                    else { panel.child(self.resource.preview_error.clone().unwrap_or_else(|| "正在加载大图…".into())) }
                }).child(self.preview_footer(cx)).into_any_element()

    }
    #[inline(never)]
    fn preview_footer(&self,cx:&mut Context<Self>)->AnyElement{
        let p=self.palette();
        let current=self.resource.selected_asset.as_ref().map(|a|a.id);
        let online=self.resource.selected_asset.as_ref().is_some_and(|a|a.provider=="yuque");
        let quality=self.viewer_model.read(cx).quality.as_ref();
        let mode=if let Some(q)=quality{
            let source=if q.remote{"在线原图"}else{"本地原图"};
            if (q.width,q.height)==(q.shown_width,q.shown_height){format!("{source} · {}×{}",q.width,q.height)}else{format!("{source} · {}×{} · 预览 {}×{}",q.width,q.height,q.shown_width,q.shown_height)}
        }else{"离线微缩图".into()};
        let message=if self.resource.pending_preview.is_some(){"正在加载下一页…"}else{self.viewer_model.read(cx).status.as_str()};
        let mut strip=div().h(px(48.)).flex().justify_center().items_center().gap_2().overflow_hidden();
        for a in self.preview_strip(){
            let selected=Some(a.id)==current;
            let asset=a.clone();let id=a.id;
            let mut tile=div().id(("preview-strip",a.id as usize)).debug_selector(move||format!("preview-strip-{id}")).w(px(56.)).h(px(44.)).flex_shrink_0().rounded_md().overflow_hidden().border_1().border_color(if selected{p.accent}else{p.line}).bg(p.sidebar).cursor_pointer().flex().items_center().justify_center()
                .on_click(cx.listener(move|v,_,w,cx|{if v.resource.selected_asset.as_ref().is_none_or(|a|a.id!=asset.id){v.select_asset(asset.clone(),w,cx);v.open_large_preview(w,cx);}}));
            if let Some(path)=self.resource.cached_thumbnails.get(&a.id){tile=tile.child(img(path.clone()).image_cache(&self.resource.image_cache).size_full().object_fit(ObjectFit::Cover));}
            else{tile=tile.child(IconName::GalleryVerticalEnd);}
            strip=strip.child(tile);
        }
        div().h(px(112.)).flex_shrink_0().flex().flex_col().gap_2()
            .child(div().h(px(28.)).flex().items_center().gap_2()
                .child(div().flex_1().min_w(px(0.)).truncate().text_xs().text_color(p.muted).child(mode))
                .when(quality.is_some()||self.viewer_model.read(cx).busy,|d|d.child(div().debug_selector(||"offline-quality".into()).child(Button::new("offline-quality").small().ghost().label("离线预览").on_click(cx.listener(|v,_,_,cx|{v.clear_quality(cx);cx.notify();})))))
                .child(div().debug_selector(||"local-quality".into()).child(Button::new("local-quality").small().label("本地原图").selected(quality.is_some_and(|q|!q.remote)).disabled(self.viewer_model.read(cx).busy||quality.is_some_and(|q|!q.remote)).on_click(cx.listener(|v,_,_,cx|v.load_quality(false,cx)))))
                .when(online,|d|d.child(Button::new("remote-quality").small().label("在线原图").selected(quality.is_some_and(|q|q.remote)).disabled(self.viewer_model.read(cx).busy||quality.is_some_and(|q|q.remote)).on_click(cx.listener(|v,_,_,cx|v.load_quality(true,cx)))))
                .child(Button::new("preview-browser").ghost().small().label("打开原文件").on_click(cx.listener(|v,_,_,cx|v.open_selected_resource(cx)))))
            .child(div().h(px(20.)).flex().items_center().justify_center().text_xs().text_color(p.muted).truncate().child(if message.is_empty(){if self.resource.preview_reload_pending{"相册有更新，返回后刷新".to_string()}else{"左右键切图 · 原图按需加载".to_string()}}else{message.to_owned()}))
            .child(strip).into_any_element()
    }
}

impl UploadDesk{
    #[inline(never)]
    fn gallery_content(&mut self,available:f32,narrow:bool,window:&mut Window,cx:&mut Context<Self>)->AnyElement{
        use std::hash::{Hash,Hasher};
        let masonry_mode=self.resource.grid_view && self.resource.masonry;
        if masonry_mode{
            let signature=(self.resource.history_generation,self.resource.assets.len(),self.resource.assets.first().map(|a|a.id),self.resource.ratio_revision,available.to_bits(),self.resource.tile_size.to_bits(),self.resource.group_dates,self.prefs.comfortable);
            let mut h=std::collections::hash_map::DefaultHasher::new();signature.hash(&mut h);let sig=h.finish();
            if self.resource.masonry_signature!=sig{
                let old_top=(-f32::from(self.resource.masonry_scroll.offset().y)).max(0.);
                let anchor=self.resource.masonry_plan.anchor(old_top);
                let plan=album::masonry(&self.resource.assets,&self.resource.ratios,available,self.resource.tile_size,self.resource.group_dates,self.spacing());
                if let Some(top)=plan.restore(anchor){
                    self.resource.masonry_scroll.set_offset(point(px(0.),px(-top)));
                }
                self.resource.masonry_plan=plan;self.resource.masonry_signature=sig;
            }
            let handle=self.resource.masonry_scroll.clone();
            let top=(-f32::from(handle.offset().y)).max(0.).min((self.resource.masonry_plan.height-f32::from(handle.bounds().size.height)).max(0.));
            let measured=f32::from(handle.bounds().size.height);
            let viewport=if measured>1.{measured}else{f32::from(window.viewport_size().height)};
            let lo=(top-160.).max(0.);let hi=top+viewport+160.;
            let plan=&self.resource.masonry_plan;
            let mut wall=div().relative().w_full().h(px(plan.height)).flex_shrink_0();
            for (day,y) in &plan.days{if *y+self.spacing().day>=lo && *y<=hi{
                wall=wall.child(div().absolute().top(px(*y)).left(px(0.)).h(px(self.spacing().day)).flex().items_center().text_size(px(self.spacing().caption)).font_medium().text_color(self.palette().muted).child(day.clone()));
            }}
            let mut first=usize::MAX;let mut last=0;
            // Tile start positions are monotonic; skip all earlier tiles except
            // the longest possible overlapping card.
            let max_height=self.resource.tile_size.max(240.)*5.+self.spacing().footer;
            let from=plan.tiles.partition_point(|t|t.y<lo-max_height);
            for t in &plan.tiles[from..]{
                if t.y>hi{break;}
                if t.y+t.height+self.spacing().footer<lo{continue;}
                first=first.min(t.index);last=last.max(t.index+1);
                if let Some(asset)=self.resource.assets.get(t.index){
                    wall=wall.child(div().absolute().left(px(t.x)).top(px(t.y)).w(px(t.width))
                        .child(self.album_item(asset.clone(),t.width,t.height,narrow,window,cx)));
                }
            }
            if first!=usize::MAX && self.resource.visible_range!=(first,last){
                let generation=self.resource.history_generation;
                cx.defer_in(window,move|v,_,cx|if v.resource.history_generation==generation{v.update_visible(first,last,cx);});
            }
            div().id("masonry-scroll").flex_1().min_h(px(0.)).w_full().overflow_y_scroll().track_scroll(&handle)
                .child(wall).into_any_element()
        }else{
            let mut hash=std::collections::hash_map::DefaultHasher::new();
            (self.resource.history_generation,self.resource.assets.len(),self.resource.assets.first().map(|a|a.id),
             if self.resource.justified{self.resource.ratio_revision}else{0},available.to_bits(),self.resource.tile_size.to_bits(),
             self.resource.grid_view,self.resource.justified,self.resource.group_dates,self.prefs.comfortable).hash(&mut hash);
            let signature=hash.finish();
            if self.resource.layout_signature!=signature {
                let old=self.resource.album_list.logical_scroll_top();
                let planned=album::plan_rows(&self.resource.assets,&self.resource.ratios,available,self.resource.tile_size,self.resource.grid_view,self.resource.justified,self.resource.group_dates,self.spacing());
                let anchor=self.resource.row_plan.get(old.item_ix).and_then(|row|match row{album::AlbumRow::Tiles{items,..}=>items.first().map(|(i,_)|*i),album::AlbumRow::Item(i)=>Some(*i),_=>None});
                let prefix=self.resource.row_plan.iter().zip(&planned).take_while(|(a,b)|a==b).count();
                if self.resource.layout_signature!=0 && self.resource.album_list.item_count()==self.resource.row_plan.len() && prefix>0 {
                    self.resource.album_list.splice(prefix..self.resource.row_plan.len(),planned.len()-prefix);
                }else{
                    self.resource.album_list.reset(planned.len());
                    if !planned.is_empty() && old.item_ix>0{
                        let item_ix=anchor.and_then(|i|planned.iter().position(|row|match row{album::AlbumRow::Tiles{items,..}=>items.iter().any(|(j,_)|*j==i),album::AlbumRow::Item(j)=>*j==i,_=>false})).unwrap_or(old.item_ix.min(planned.len()-1));
                        self.resource.album_list.scroll_to(ListOffset{item_ix,offset_in_item:px(0.)});
                    }
                }
                self.resource.row_plan=std::rc::Rc::new(planned);self.resource.layout_signature=signature;
            }
            let entity=cx.weak_entity();let planned=self.resource.row_plan.clone();
            let event_plan=planned.clone();let event_entity=entity.clone();let generation=self.resource.history_generation;
            self.resource.album_list.set_scroll_handler(move|event,window,app|{
                let span=event.visible_range.len().max(4);
                let weak=event_entity.clone();let plan=event_plan.clone();
                window.defer(app,move|_,app|{let _=weak.update(app,|v,cx|{
                    if v.resource.history_generation!=generation{return;}
                    let start=v.resource.album_list.logical_scroll_top().item_ix.min(plan.len());
                    let end=(start+span+2).min(plan.len());
                    let mut first=usize::MAX;let mut last=0;
                    for row in &plan[start..end]{
                        match row{
                            album::AlbumRow::Item(i)=>{first=first.min(*i);last=last.max(*i+1);}
                            album::AlbumRow::Tiles{items,..}=>{for (i,_) in items{first=first.min(*i);last=last.max(*i+1);}}
                            _=>{}
                        }
                    }
                    if first!=usize::MAX{v.update_visible(first,last,cx);}
                });});
            });
            list(self.resource.album_list.clone(),move|i,window,app|{
                entity.update(app,|v,cx|planned.get(i).map(|row|v.album_row(row,narrow,window,cx)).unwrap_or_else(||div().into_any_element()))
                    .unwrap_or_else(|_|div().into_any_element())
            }).flex_1().min_h(px(0.)).w_full().into_any_element()
        }
    }
}
impl Render for UploadDesk {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Event-driven, short transition. Respects the OS reduced-motion preference.
        let feedback_opacity = gpui_kit::base::transition(
            ("upload-feedback", "opacity"),
            if self.queue.busy { 1.0f32 } else { 0.82f32 },
            gpui_kit::base::Transition::new(std::time::Duration::from_millis(design::FEEDBACK_MS)),
            window,
            cx,
        );
        if self.resource.density.read(cx).value().start()!=self.resource.tile_size{
            self.resource.density.update(cx,|state,cx|state.set_value(self.resource.tile_size,window,cx));
        }
        #[cfg(feature="qa")]
        if self.isolated_qa(){
            let page=std::env::var("YUQUE_QA_PAGE").unwrap_or_default();
            if matches!(page.as_str(),"detail"|"preview"|"quality") && self.resource.selected_asset.is_none(){
                if let Some(asset)=self.resource.assets.iter().find(|a|a.category=="image").cloned(){
                    cx.defer_in(window,move|v,w,cx|{if v.resource.selected_asset.is_none(){v.resource.selected_ids.insert(asset.id);v.select_asset(asset,w,cx);if page=="detail"{v.inspector_open=true;}}});
                }
            }else if matches!(page.as_str(),"preview"|"quality") && !self.resource.large_preview_open && self.resource.thumbnail_path.is_some(){
                cx.defer_in(window,|v,w,cx|{if !v.resource.large_preview_open{v.open_large_preview(w,cx);}});
            }else if page=="quality" && self.resource.large_preview_open && self.viewer_model.read(cx).quality.is_none() && !self.viewer_model.read(cx).busy && self.viewer_model.read(cx).status.is_empty(){
                cx.defer_in(window,|v,_,cx|{v.load_quality(false,cx);});
            }
        }
        let narrow = window.viewport_size().width < px(1000.);
        let show_inspector=self.page==Page::Gallery&&!self.immersive&&self.inspector_open;
        let inspector_opacity=design::tween("inspector-opacity",if show_inspector&&!self.resource.large_preview_open{1.}else{0.},design::SURFACE_MS,window,cx);
        let reserve_inspector=self.page==Page::Gallery&&!self.immersive&&(show_inspector||inspector_opacity>0.001);
        let show_center=!narrow||!reserve_inspector;
        use design::Surface;
        let surface=if self.resource.large_preview_open{Surface::Preview}else if self.page.settings(){if self.page==Page::Account{Surface::Account}else if self.page==Page::Data{Surface::Data}else{Surface::Settings}}else if self.page==Page::Jobs{Surface::Jobs}else if self.immersive{Surface::Immersive}else{Surface::Gallery};
        let surface_alpha=self.surface_motion.sample(surface,window,cx);
        if self.resource.large_preview_open {return self.preview_view(surface_alpha,window,cx);}
        let nav_width=if self.immersive{0.}else if self.sidebar_collapsed{56.}else{self.spacing().sidebar_width(narrow)};
        let nav_progress=design::tween("nav-reveal",if self.sidebar_collapsed{0.}else{1.},design::SURFACE_MS,window,cx);
        let available = f32::from(window.viewport_size().width) - nav_width - if !narrow && reserve_inspector {self.spacing().inspector} else {0.} - 2.*self.spacing().pad;
        let gallery=if self.page==Page::Gallery&&show_center{self.gallery_content(available,narrow,window,cx)}else{div().into_any_element()};
        // Never access SQLite from the hot render path.
        let total = self.resource.total_count;
        let is_demo = self.account.config.mode == Mode::Demo;
        let detail_width=if narrow{(f32::from(window.viewport_size().width)-nav_width).max(0.)}else{self.spacing().inspector};
        div()
            .flex()
            .size_full()
            .text_size(px(self.spacing().body))
            .bg(self.palette().canvas)
            .text_color(self.palette().text)
             .when(!self.immersive,|root|root.child(div().relative().w(px(nav_width)).h_full().flex_shrink_0().overflow_hidden().bg(self.palette().sidebar)
                .child(div().absolute().top_0().bottom_0().left(px(if self.sidebar_collapsed{-8.*nav_progress}else{-12.*(1.-nav_progress)})).w(px(nav_width))
                    .child(if self.sidebar_collapsed{self.navigation_rail(cx).into_any_element()}else{self.navigation(narrow,window,cx).into_any_element()}))))
            .when(self.page.settings(),|d|d.child(div().flex_1().min_w(px(0.)).h_full().flex().justify_center().opacity(surface_alpha).child(div().w_full().max_w(px(680.)).h_full().child(self.settings_view(cx)))))
            .when(show_center && self.page==Page::Jobs, |d| d.child(div().flex_1().min_w(px(0.)).h_full().flex().opacity(surface_alpha).child(self.task_center(cx))))
            .when(show_center && self.page==Page::Gallery, |d| d.child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col().opacity(surface_alpha)
                    .gap_2().p(px(self.spacing().pad))
                    .child(self.gallery_toolbar(available<760.,total,is_demo,window,cx))
                    .child(self.selection_toolbar(cx))
                    .when(self.resource.similar_to.is_some(),|d|d.child(div().flex().items_center().gap_2()
                        .child(div().text_xs().text_color(self.palette().muted).child(format!("相似图片 · {}",self.resource.similar_to.as_ref().map(|a|a.name.as_str()).unwrap_or(""))))
                        .child(Button::new("exit-similar").small().ghost().label("退出").on_click(cx.listener(|v,_,_,cx|{v.resource.similar_to=None;v.reset_history(cx);})))) )
                    .when(self.resource.tag_filter.is_some(),|d|d.child(div().flex().items_center().gap_2()
                        .child(format!("标签：{}",self.resource.tag_filter.as_deref().unwrap_or_default()))
                        .child(Button::new("clear-tag").small().label("清除").on_click(cx.listener(|v,_,_,cx|{v.resource.tag_filter=None;v.reset_history(cx);})))) )
                    .when(self.resource.batch_tag_open&&!self.resource.selected_ids.is_empty(),|d|d.child(div().flex().gap_2().items_center()
                        .child(div().flex_1().min_w(px(0.)).child(Input::new(&self.resource.batch_tag_input)))
                        .child(Button::new("batch-add-tags").small().label("添加标签").disabled(self.resource_write_busy).on_click(cx.listener(|v,_,_,cx|v.apply_batch_tags(false,cx))))
                        .child(Button::new("batch-remove-tags").small().label("移除标签").disabled(self.resource_write_busy).on_click(cx.listener(|v,_,_,cx|v.apply_batch_tags(true,cx))))
                        .child(Button::new("batch-close").small().ghost().label("关闭").on_click(cx.listener(|v,_,_,cx|{v.resource.batch_tag_open=false;cx.notify();})))))
                    .child(
                        div()
                            .id("asset-history").track_focus(&self.resource.gallery_focus).key_context("YuqueGallery")
                            .on_action(cx.listener(|v,_:&shell::GalleryPreview,w,cx|v.command(Command::Preview,w,cx)))
                            .on_action(cx.listener(|v,_:&shell::GalleryClear,w,cx|v.command(Command::ClearSelection,w,cx)))
                            .on_action(cx.listener(|v,_:&shell::GallerySelectAll,w,cx|v.command(Command::SelectAll,w,cx)))
                            .on_action(cx.listener(|v,_:&shell::GalleryInspector,w,cx|v.command(Command::ToggleInspector,w,cx)))
                            .on_key_down(cx.listener(|v,e:&KeyDownEvent,w,cx|v.gallery_key(e,w,cx)))
                            .drag_over::<ExternalPaths>({let p=self.palette();move|style,_,_,_|style.bg(p.accent_soft)})
                            .on_drop(cx.listener(|v,paths:&ExternalPaths,_,cx|v.start_upload(paths.paths().to_vec(),cx)))
                            .flex_1()
                            .min_h(px(0.))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .when(!self.resource.assets.is_empty()||self.resource.loading_history,|d|d.child(gallery))
                            .when(self.resource.loading_history, |container| container.child("正在读取资源…"))
                            .when(self.resource.history_error.is_some(), |container| {
                                container.child(format!("历史读取失败：{}", self.resource.history_error.as_deref().unwrap_or("")))
                            })
                            .when(self.resource.assets.is_empty() && !self.resource.loading_history && self.resource.history_error.is_none(), |container| {
                                let initial=self.resource.search.is_empty() && self.resource.category.is_empty() && self.resource.tag_filter.is_none() && !self.resource.favorites_only && self.resource.similar_to.is_none();
                                container.child(Empty::new().header(EmptyHeader::new()
                                    .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(IconName::FolderOpen))
                                    .title(EmptyTitle::new().child(if initial{"还没有资源"}else{"没有符合条件的资源"}))
                                    .description(EmptyDescription::new().child(if initial{"拖入文件或点击上传，即可离线浏览、复制链接。"}else{"尝试调整搜索词或筛选条件。"})))
                                    .when(initial,|e|e.child(Button::new("empty-upload").small().primary().icon(IconName::Plus).label("上传文件")
                                        .disabled(self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.busy||self.account.account_busy).on_click(cx.listener(|v,_,_,cx|v.select_files(cx))))))
                            })
                            .overflow_hidden(),
                    )
                    .child(div().flex().flex_col().gap(px(if self.prefs.comfortable{6.}else{self.spacing().section}))
                      .child(
                        div().h(px(30.)).flex_shrink_0().border_t_1().border_color(self.palette().line).flex().items_center().justify_between().gap_2().text_xs().text_color(self.palette().muted)
                            .child(div().flex_1().min_w(px(0.)).truncate().opacity(feedback_opacity).child(self.status.clone()))
                            .when(!self.queue.busy && self.queue.recovery_count>0,|d|d.child(Button::new("view-recovery-jobs").small().ghost().label(format!("待恢复 {}",self.queue.recovery_count)).on_click(cx.listener(|v,_,_,cx|{v.navigate_page(Page::Jobs,cx);}))))
                            .when(self.resource.grid_view,|d|d.child(self.density_control()))
                            .when(self.resource.history_error.is_some(),|d|d.child(Button::new("refresh-history").small().label("重试读取")
                                .on_click(cx.listener(|v,_,_,cx|{v.resource.history_error=None;if v.resource.assets.is_empty(){v.reload(cx);}else{v.load_more(cx);}}))))
                            .when(self.resource.has_more || self.resource.loading_history,|d|d.child(Button::new("next-page").ghost().small().label(if self.resource.loading_history{"加载中…"}else{"加载更多"})
                                .disabled(self.resource.loading_history).on_click(cx.listener(|v,_,_,cx|v.load_more(cx)))))
                    )
                    .when(self.queue.busy,|d|d.child(self.upload_summary(cx)))
),
            ))
            .when(reserve_inspector, |d| d.child(div().id("inspector-slot").debug_selector(||"inspector-slot".into()).relative().w(px(detail_width)).flex_shrink_0().h_full().overflow_hidden().bg(self.palette().canvas)
                .child(div().absolute().top_0().bottom_0().w(px(detail_width)).left(px(22.*(1.-inspector_opacity))).opacity(inspector_opacity)
                    .when(!show_inspector,|d|d.occlude().capture_any_mouse_down(|_,_,cx|cx.stop_propagation()).capture_any_mouse_up(|_,_,cx|cx.stop_propagation()).capture_key_down(|_,_,cx|cx.stop_propagation()))
                    .child(self.inspector(narrow,show_inspector,cx)))))
            .into_any_element()
    }
}
