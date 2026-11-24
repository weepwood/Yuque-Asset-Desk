use super::*;
use gpui_kit::component::{IconName, Sizable};

impl UploadDesk {
    pub(super) fn color(&self, dark: u32, light: u32) -> Rgba {
        rgb(if self.dark { dark } else { light })
    }

    fn tag_tree(&self,cx:&mut Context<Self>) -> impl IntoElement {
        let mut nodes=std::collections::BTreeSet::new();
        for tag in &self.resource.tag_catalog {
            let mut path=String::new();
            for part in tag.split('/') {
                if !path.is_empty(){path.push('/');}
                path.push_str(part);
                nodes.insert(path.clone());
            }
        }
        let mut tree=div().flex().flex_col().gap_1()
            .child(Button::new("tags-toggle").ghost().small().label(if self.resource.tags_expanded {"▾ 标签分类"} else {"▸ 标签分类"})
                .on_click(cx.listener(|v,_,_,cx|{v.resource.tags_expanded=!v.resource.tags_expanded;cx.notify();})));
        if self.resource.tags_expanded {
            tree=tree.child(Button::new("tag-all").ghost().small().label("全部标签").selected(self.resource.tag_filter.is_none())
                .on_click(cx.listener(|v,_,_,cx|{v.resource.tag_filter=None;v.reset_history(cx);})));
            for (i,path) in nodes.into_iter().enumerate() {
                let depth=path.matches('/').count().min(5);
                let title=path.rsplit('/').next().unwrap_or(&path).to_string();
                let selected=self.resource.tag_filter.as_ref()==Some(&path);
                tree=tree.child(div().pl(px(depth as f32*8.)).child(Button::new(("tag-node",i)).ghost().small().label(title).selected(selected)
                    .on_click(cx.listener(move |v,_,_,cx|{v.jobs_page=false;v.account_panel=false;v.inspector_open=false;v.resource.similar_to=None;v.resource.tag_filter=Some(path.clone());v.reset_history(cx);}))));
            }
        }
        tree
    }

    pub(super) fn status_panel(&self) -> impl IntoElement {
        div().flex().flex_col().gap_1().p_2().rounded_md().text_xs()
            .bg(self.color(0x25272a,0xe1e3e6))
            .child(div().font_semibold().child(if self.account.config.mode==Mode::Demo {"本地演示"} else {"语雀账号"}))
            .child(format!("{} · 待处理 {}",self.account.config.owner_id(),self.queue.queue_count))
    }

    fn task_center(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().id("jobs-list").flex_1().min_h(px(0.)).flex().flex_col().gap_2();
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
            let mut row = div().flex().flex_col().gap_2().p_3().rounded_lg()
                .border_1().border_color(self.color(0x34363a,0xdfe2e6))
                .bg(self.color(0x242629,0xffffff))
                .child(div().flex().items_center().gap_2()
                    .child(div().flex_1().min_w(px(0.)).font_semibold().truncate().child(job.path.file_name().unwrap_or_default().to_string_lossy().into_owned()))
                    .child(div().text_sm().child(state))
                    .when(retry, |d| d.child(Button::new(("retry-job",id as usize))
                        .label("重试（先核实远端）").disabled(self.queue.busy || self.account.account_busy)
                        .on_click(cx.listener(move |view, _, _, cx| view.retry_one(id,cx)))))
                    .when(matches!(job.state.as_str(),"unknown"|"running") && !active, |d| d.child(Button::new(("ack-job",id as usize))
                        .label("已确认远端无文件").disabled(self.queue.busy || self.account.account_busy)
                        .on_click(cx.listener(move |v,_,_,cx| v.acknowledge_job(id,cx))))));
            if active {
                row = row.child(if total == 0 { "检查文件／去重中".into() }
                    else { format!("已读取请求体 {} / {}，不代表服务端已确认",file_size(bytes as i64),file_size(total as i64)) });
                if total > 0 {
                    row = row.child(div().h(px(4.)).rounded_md().bg(self.color(0x36383c,0xdfe2e6))
                        .child(div().h_full().rounded_md().bg(rgb(0x4c8ce8)).w(relative(bytes as f32 / total as f32))));
                }
            }
            if !job.error.is_empty() { row = row.child(div().text_sm().text_color(self.color(0xf5b4a5,0xa53222)).child(job.error.clone())); }
            list = list.child(row);
        }
        div().flex_1().min_w(px(0.)).flex().flex_col().gap_3().p_4()
            .child(div().flex().items_center().gap_2()
                .child(div().flex_1().text_lg().font_semibold().child("上传任务"))
                .child(Button::new("refresh-jobs").label("刷新").on_click(cx.listener(|v,_,_,cx| v.refresh_jobs(cx))))
                .child(Button::new("resume-jobs").label("恢复任务").disabled(self.queue.busy || self.account.account_busy || self.queue.recovery_count == 0)
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

    pub(super) fn inspector(&self, narrow: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div().id("inspector").h_full().min_h(px(0.)).flex().flex_col().gap_3().p_4().when(!narrow, |d| d.w(px(260.)).flex_shrink_0()).when(narrow, |d| d.w_full())
            .bg(self.color(0x1c1e21, 0xf7f8fa))
            .border_l_1().border_color(self.color(0x34363a, 0xdfe2e6));
        if self.account_panel {
            panel = panel.child(div().font_semibold().child("账号管理"))
                .child(div().text_sm().child("仅保存到系统密钥库。凭据格式有效不代表已登录。"))
                .child(div().text_sm().child(self.account.session_status.clone()))
                .when(self.account.config.mode == Mode::Yuque && self.account.config.creds.is_some(), |d| d.child(
                    Button::new("check-session").label("检查会话（只读）").disabled(self.account.account_busy || self.queue.busy)
                        .on_click(cx.listener(|view,_,_,cx| view.check_session(cx)))))
                .child(Input::new(&self.account.account_id).disabled(self.account.account_busy || self.queue.busy))
                .child(Input::new(&self.account.account_csrf).disabled(self.account.account_busy || self.queue.busy))
                .child(Input::new(&self.account.account_cookie).disabled(self.account.account_busy || self.queue.busy))
                .child(Button::new("save-account").primary().label("保存并切换")
                    .disabled(self.account.account_busy || self.queue.busy)
                    .on_click(cx.listener(|view, _, window, cx| view.save_credentials(window, cx))))
                .child(Button::new("demo-account").label("本地演示")
                    .disabled(self.account.account_busy || self.queue.busy)
                    .on_click(cx.listener(|view, _, _, cx| view.activate_account(None, cx))))
                .when(self.account.config.mode == Mode::Yuque, |d| d.child(
                    Button::new("forget-account").label("删除当前保存凭据")
                        .disabled(self.account.account_busy || self.queue.busy)
                        .on_click(cx.listener(|view, _, _, cx| view.forget_account(cx)))))
                .child(div().font_semibold().child("已保存账号"));
            for owner in &self.account.saved_accounts {
                let id = owner.clone();
                panel = panel.child(Button::new(SharedString::from(format!("account-{owner}")))
                    .label(format!("账号 {owner}"))
                    .disabled(self.account.account_busy || self.queue.busy)
                    .on_click(cx.listener(move |view, _, _, cx| view.load_credentials(id.clone(), cx))));
            }
        } else if let Some(asset) = &self.resource.selected_asset {
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
                .child(Button::new("detail-open").small().label(if asset.provider=="demo" {"打开本地文件"} else {"浏览器打开链接"})
                    .on_click(cx.listener(|v,_,_,cx|v.open_selected_resource(cx))))
                .child(Button::new("detail-md").label("复制 Markdown")
                    .on_click(cx.listener(move |view, _, _, cx| view.copy_asset(&md_asset, true, cx))));
            panel = panel.child(Input::new(&self.resource.tag_input))
                .child(Button::new("save-tags").label("保存标签")
                    .on_click(cx.listener(|view, _, _, cx| {
                        if let Some(asset) = &view.resource.selected_asset {
                            let text = view.resource.tag_input.read(cx).value().to_string();
                            view.status = match view.storage.set_tags(asset.id, &text) {
                                Ok(()) => "标签已保存".into(), Err(e) => e.to_string(),
                            };
                            view.reload(cx);
                        }
                        cx.notify();
                    })));
            let favorite_asset=asset.clone();
            panel=panel.child(Button::new("detail-favorite").label(if self.resource.favorites.contains(&asset.id){"取消收藏"}else{"收藏资源"})
                .on_click(cx.listener(move |v,_,_,cx|v.toggle_favorite(&favorite_asset,cx))));
            if asset.category == "image" {
                let similar_asset=asset.clone();
                panel=panel.child(Button::new("detail-similar").label("查找相似图片")
                    .on_click(cx.listener(move |v,_,_,cx|v.find_similar(similar_asset.clone(),cx))));
                panel = panel.child(Button::new("large-preview").label("放大本地缩略图")
                    .disabled(self.resource.preview_loading)
                    .on_click(cx.listener(|v,_,_,cx| v.open_large_preview(cx))));
                if let Some(path) = &self.resource.thumbnail_path {
                    panel = panel.child(img(path.clone()).image_cache(&self.resource.image_cache).w_full().h(px(160.)).object_fit(ObjectFit::Contain));
                } else {
                    panel = panel.child(Button::new("preview-image")
                        .label(if self.resource.preview_loading { "读取缩略图…" } else { "读取本地缩略图" })
                        .disabled(self.resource.preview_loading)
                        .on_click(cx.listener(|view, _, _, cx| view.load_preview(cx))));
                    if let Some(error) = &self.resource.preview_error { panel = panel.child(div().text_sm().child(error.clone())); }
                }
            }
        } else {
            panel = panel.child(div().font_semibold().child("资源详情"))
                .child(div().text_sm().child("选择列表或网格中的资源，查看属性及复制链接。"));
        }
        panel.overflow_y_scrollbar()
    }
}

impl UploadDesk {
    fn album_item(&self,asset:Asset,tile_width:f32,tile_image_height:f32,narrow:bool,cx:&mut Context<Self>)->AnyElement {
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
                    let favorite_asset=asset.clone();
                    let similar_asset=asset.clone();
                    let thumbnail = self.resource.cached_thumbnails.get(&asset.id).cloned();
                    return div().id(("album-tile",index)).relative().flex().flex_col().gap_1()
                        .w(px(tile_width)).h(px(tile_image_height+album::TILE_FOOTER)).flex_shrink_0().gap_0().rounded_md()
                        .border_1().border_color(if checked || selected {rgb(0x528bd8)} else {self.color(0x17191b,0xf3f4f6)})
                        .bg(self.color(0x17191b,0xf3f4f6))
                        .hover(|s|s.bg(self.color(0x2d3035,0xe8edf4)))
                        .on_hover(cx.listener(move |v,hovered:&bool,_,cx|{
                            let old=v.resource.hover_asset;
                            if *hovered {v.resource.hover_asset=Some(asset_id);}
                            else if old==Some(asset_id){v.resource.hover_asset=None;}
                            if old!=v.resource.hover_asset{cx.notify();}
                        }))
                        .child(div().id(("album-image",index)).relative().h(px(tile_image_height)).w_full().overflow_hidden()
                            .rounded_md().bg(self.color(0x303338,0xeef0f3)).flex().items_center().justify_center()
                            .cursor_pointer()
                            .on_click(cx.listener(move |v,_,window,cx|v.select_asset(detail_asset.clone(),window,cx)))
                            .child(match thumbnail {
                                Some(path)=>img(path).image_cache(&self.resource.image_cache).size_full().object_fit(ObjectFit::Contain).into_any_element(),
                                None=>div().flex().flex_col().items_center().gap_1()
                                    .text_color(self.color(0xa6a9b0,0x646a73))
                                    .child(IconName::File)
                                    .child(div().text_xs().child(if asset.category=="image" {"无本地缩略图"} else if asset.category=="video" {"视频"} else {"附件"})).into_any_element()
                            }))
                        .when(hovered || selected,|d|d.child(div().absolute().top(px(6.)).left(px(6.)).right(px(6.)).flex().items_center().justify_end().gap_1()
                            .child(Button::new(("grid-md",index)).xsmall().label("MD").tooltip("复制 Markdown")
                                .on_click(cx.listener(move |v,_,_,cx|v.copy_asset(&markdown_asset,true,cx))))
                            .when(asset.category=="image" && tile_width>=112.,|d|d.child(Button::new(("grid-similar",index)).xsmall().label("相似")
                                .on_click(cx.listener(move |v,_,_,cx|v.find_similar(similar_asset.clone(),cx)))))))
                        .child(div().h(px(19.)).text_xs().font_medium().truncate().px_1().child(asset.name.clone()))
                        .child(div().h(px(22.)).flex().items_center().justify_between().gap_1()
                            .child(Button::new(("grid-select",index)).ghost().xsmall().selected(checked)
                                .label(if checked {"✓"} else {"□"}).tooltip("选择")
                                .on_click(cx.listener(move |v,_,_,cx|{if !v.resource.selected_ids.remove(&asset_id){if v.resource.selected_ids.len()<150{v.resource.selected_ids.insert(asset_id);}else{v.status="单批最多选择 150 项".into();}}cx.notify();})))
                            .child(Button::new(("grid-fav",index)).ghost().xsmall()
                                .label(if starred {"★"} else {"☆"}).tooltip("收藏")
                                .on_click(cx.listener(move |v,_,_,cx|v.toggle_favorite(&favorite_asset,cx))))
                            .child(Button::new(("grid-copy",index)).xsmall().label(if tile_width<115.{"↗"}else{"URL"}).tooltip("复制原始链接")
                                .on_click(cx.listener(move |v,_,_,cx|v.copy_asset(&url_asset,false,cx)))))
                        .into_any_element();
                }
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .p_2()
                    .rounded_md()
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
                                "{} · {} · {}",
                                file_size(asset.bytes), if self.resource.all_accounts {format!("{} / {}",asset.provider,asset.owner_id)} else {asset.category.clone()},
                                asset.created_at.get(..16).unwrap_or(&asset.created_at).replace("T"," ")
                            ))),
                    )
                    .child(Button::new(("row-select", index)).small().selected(checked)
                        .label(if checked { "已选" } else { "选择" })
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if !view.resource.selected_ids.remove(&asset_id) { if view.resource.selected_ids.len()<150{view.resource.selected_ids.insert(asset_id);}else{view.status="单批最多选择 150 项".into();} } cx.notify();
                        })))
                    .child(Button::new(("detail", index)).small().selected(selected).label("详情")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.select_asset(detail_asset.clone(), window, cx);
                        })))
                    .when(!narrow, |d| d.child(
                        Button::new(("url", index)).small()
                            .label("复制 URL")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.copy_asset(&url_asset, false, cx);
                            })),
                    ))
                    .when(!narrow, |d| d.child(
                        Button::new(("markdown", index)).small()
                            .label("Markdown")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.copy_asset(&markdown_asset, true, cx);
                            })),
                    ))
                    .into_any_element()
    }

    fn album_row(&self,row:&album::AlbumRow,narrow:bool,cx:&mut Context<Self>)->AnyElement {
        match row {
            album::AlbumRow::Day(day)=>div().h(px(32.)).flex().items_center().text_sm().font_semibold().text_color(self.color(0xa6a9b0,0x646a73)).child(day.clone()).into_any_element(),
            album::AlbumRow::Item(i)=>div().pb_2().child(self.album_item(self.resource.assets[*i].clone(),0.,0.,narrow,cx)).into_any_element(),
            album::AlbumRow::Tiles{items,height}=>div().w_full().flex().items_start().gap_2().pb_2()
                .children(items.iter().map(|(i,w)|self.album_item(self.resource.assets[*i].clone(),*w,*height,narrow,cx)).collect::<Vec<_>>()).into_any_element(),
        }
    }
}

impl Render for UploadDesk {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Event-driven, short transition. Respects the OS reduced-motion preference.
        let feedback_opacity = gpui_kit::base::transition(
            ("upload-feedback", "opacity"),
            if self.queue.busy { 1.0f32 } else { 0.82f32 },
            gpui_kit::base::Transition::new(std::time::Duration::from_millis(160)),
            window,
            cx,
        );
        let narrow = window.viewport_size().width < px(1000.);
        let available = f32::from(window.viewport_size().width) - if narrow {150.} else {184.} - if !narrow && self.inspector_open {260.} else {0.} - 40.;
        use std::hash::{Hash,Hasher};
        let masonry_mode=self.resource.grid_view && self.resource.masonry;
        let gallery=if masonry_mode{
            let signature=(self.resource.history_generation,self.resource.assets.len(),self.resource.assets.first().map(|a|a.id),self.resource.ratio_revision,available.to_bits(),self.resource.tile_size.to_bits(),self.resource.group_dates);
            let mut h=std::collections::hash_map::DefaultHasher::new();signature.hash(&mut h);let sig=h.finish();
            if self.resource.masonry_signature!=sig{
                let old_top=(-f32::from(self.resource.masonry_scroll.offset().y)).max(0.);
                let anchor=self.resource.masonry_plan.anchor(old_top);
                let plan=album::masonry(&self.resource.assets,&self.resource.ratios,available,self.resource.tile_size,self.resource.group_dates);
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
            for (day,y) in &plan.days{if *y+32.>=lo && *y<=hi{
                wall=wall.child(div().absolute().top(px(*y)).left(px(0.)).h(px(32.)).text_sm().font_semibold().child(day.clone()));
            }}
            let mut first=usize::MAX;let mut last=0;
            // Tile start positions are monotonic; skip all earlier tiles except
            // the longest possible overlapping card.
            let max_height=self.resource.tile_size.max(240.)*5.+album::TILE_FOOTER;
            let from=plan.tiles.partition_point(|t|t.y<lo-max_height);
            for t in &plan.tiles[from..]{
                if t.y>hi{break;}
                if t.y+t.height+album::TILE_FOOTER<lo{continue;}
                first=first.min(t.index);last=last.max(t.index+1);
                if let Some(asset)=self.resource.assets.get(t.index){
                    wall=wall.child(div().absolute().left(px(t.x)).top(px(t.y)).w(px(t.width))
                        .child(self.album_item(asset.clone(),t.width,t.height,narrow,cx)));
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
             self.resource.grid_view,self.resource.justified,self.resource.group_dates).hash(&mut hash);
            let signature=hash.finish();
            if self.resource.layout_signature!=signature {
                let old=self.resource.album_list.logical_scroll_top();
                let planned=album::plan_rows(&self.resource.assets,&self.resource.ratios,available,self.resource.tile_size,self.resource.grid_view,self.resource.justified,self.resource.group_dates);
                let prefix=self.resource.row_plan.iter().zip(&planned).take_while(|(a,b)|a==b).count();
                if self.resource.album_list.item_count()==self.resource.row_plan.len() && prefix>0 {
                    self.resource.album_list.splice(prefix..self.resource.row_plan.len(),planned.len()-prefix);
                }else{
                    self.resource.album_list.reset(planned.len());
                    if !planned.is_empty() && old.item_ix>0{
                        self.resource.album_list.scroll_to(ListOffset{item_ix:old.item_ix.min(planned.len()-1),offset_in_item:old.offset_in_item});
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
            list(self.resource.album_list.clone(),move|i,_,app|{
                entity.update(app,|v,cx|planned.get(i).map(|row|v.album_row(row,narrow,cx)).unwrap_or_else(||div().into_any_element()))
                    .unwrap_or_else(|_|div().into_any_element())
            }).flex_1().min_h(px(0.)).w_full().into_any_element()
        };
        // Never access SQLite from the hot render path.
        let total = self.resource.total_count;
        let is_demo = self.account.config.mode == Mode::Demo;
        if self.resource.large_preview_open {
            return div().size_full().flex().flex_col().p_4().gap_3()
                .bg(self.color(0x17191b,0xf3f4f6)).text_color(self.color(0xebedf0,0x24272c))
                .child(div().flex().items_center().gap_2()
                    .child(Button::new("close-preview").label("返回资源").on_click(cx.listener(|v,_,_,cx| {v.resource.large_preview_open=false; cx.notify();})))
                    .child(div().flex_1().font_semibold().truncate().child(self.resource.selected_asset.as_ref().map(|a|a.name.clone()).unwrap_or_default()))
                    .child(Button::new("zoom-out").label("−").on_click(cx.listener(|v,_,_,cx|v.zoom_by(-0.25,cx))))
                    .child(format!("{:.0}%",self.resource.zoom*100.))
                    .child(Button::new("zoom-in").label("+").on_click(cx.listener(|v,_,_,cx|v.zoom_by(0.25,cx))))
                    .child(Button::new("zoom-reset").label("适应窗口").on_click(cx.listener(|v,_,_,cx|{v.resource.zoom=1.;v.resource.pan=point(px(0.),px(0.));cx.notify();}))))
                .child({
                    let panel=div().id("preview-canvas").relative().flex_1().min_h(px(0.)).overflow_hidden().flex().items_center().justify_center()
                        .on_mouse_down(MouseButton::Left,cx.listener(|v,e:&MouseDownEvent,_,_|{v.resource.drag_at=Some(e.position);}))
                        .on_mouse_move(cx.listener(|v,e:&MouseMoveEvent,_,cx|{
                            if e.dragging() && v.resource.zoom>1. {
                                if let Some(prev)=v.resource.drag_at {let d=e.position-prev;
                                    v.resource.pan=point((v.resource.pan.x+d.x).clamp(px(-1500.),px(1500.)),(v.resource.pan.y+d.y).clamp(px(-1500.),px(1500.)));cx.notify();}
                                v.resource.drag_at=Some(e.position);
                            }else{v.resource.drag_at=None;}
                        }))
                        .on_mouse_up(MouseButton::Left,cx.listener(|v,_,_,_|v.resource.drag_at=None));
                    if let Some(path)=&self.resource.large_preview_path { panel.child(div().absolute().left(relative((1.-self.resource.zoom)/2.)).top(relative((1.-self.resource.zoom)/2.))
                        .w(relative(self.resource.zoom)).h(relative(self.resource.zoom))
                        .child(div().relative().left(self.resource.pan.x).top(self.resource.pan.y).size_full()
                            .child(img(path.clone()).image_cache(&self.resource.image_cache).size_full().object_fit(ObjectFit::Contain)))) }
                    else { panel.child(self.resource.preview_error.clone().unwrap_or_else(|| "正在加载大图…".into())) }
                }).into_any_element();
        }
        let show_inspector = self.inspector_open && (!narrow || self.account_panel || self.resource.selected_asset.is_some());
        let show_center = !narrow || !show_inspector;
        div()
            .flex()
            .size_full()
            .text_sm()
            .bg(self.color(0x17191b, 0xf3f4f6))
            .text_color(self.color(0xebedf0, 0x24272c))
            .child(
                div()
                    .id("navigation-sidebar").w(px(if narrow { 150. } else { 184. })).flex_shrink_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .border_r_1().border_color(self.color(0x303237,0xdde0e4))
                    .bg(self.color(0x1d1f22, 0xe9ebee))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_2().child(div().font_semibold().truncate().child(if narrow {"Yuque Desk"} else {"Yuque Asset Desk"}))
                            .child(div().text_xs().text_color(self.color(0xa6a9b0, 0x666c75)).child("资源相册")),
                    )
                    .child(
                        Button::new("upload-navigation").ghost().small()
                            .selected(self.resource.category.is_empty() && !self.jobs_page).icon(IconName::FolderOpen)
                            .label("上传工作台")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.jobs_page = false;
                                view.resource.similar_to=None;view.resource.favorites_only=false;view.resource.category = ""; view.account_panel=false; view.inspector_open=false;
                                view.reset_history(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("images-navigation").ghost().small()
                            .selected(self.resource.category == "image" && !self.jobs_page).icon(IconName::File)
                            .label("图片资源")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.jobs_page = false;
                                view.resource.similar_to=None;view.resource.favorites_only=false;view.resource.category = "image"; view.resource.grid_view=true; view.account_panel=false; view.inspector_open=false;
                                view.reset_history(cx);
                                cx.notify();
                            })),
                    )
                    .child(Button::new("video-nav").ghost().small().label("视频").icon(IconName::File).selected(self.resource.category=="video" && !self.jobs_page)
                        .on_click(cx.listener(|v,_,_,cx| {v.jobs_page=false; v.resource.similar_to=None;v.resource.favorites_only=false;v.resource.category="video"; v.account_panel=false; v.inspector_open=false; v.reset_history(cx); })))
                    .child(Button::new("attachment-nav").ghost().small().label("附件").icon(IconName::File).selected(self.resource.category=="attachment" && !self.jobs_page)
                        .on_click(cx.listener(|v,_,_,cx| {v.jobs_page=false; v.resource.similar_to=None;v.resource.favorites_only=false;v.resource.category="attachment"; v.account_panel=false; v.inspector_open=false; v.reset_history(cx); })))
                    .child(self.tag_tree(cx))
                    .child(Button::new("jobs-nav").ghost().small().selected(self.jobs_page).label("上传任务")
                        .on_click(cx.listener(|view,_,_,cx| {view.jobs_page=true; view.account_panel=false; view.inspector_open=false; view.refresh_jobs(cx); cx.notify();})))
                    .child(Button::new("account-nav").ghost().small().icon(IconName::Settings).selected(self.account_panel).label("账号管理")
                        .on_click(cx.listener(|view, _, _, cx| { view.account_panel = true; view.inspector_open = true; cx.notify(); })))
                    .child(Button::new("inspector-toggle").ghost().small().icon(IconName::PanelRight).label(if self.inspector_open {"隐藏详情"} else {"显示详情"})
                        .on_click(cx.listener(|v,_,_,cx| {v.inspector_open = !v.inspector_open; cx.notify();})))
                    .child(Button::new("theme-toggle").ghost().small().icon(if self.dark {IconName::Sun} else {IconName::Moon}).label(if self.dark { "浅色主题" } else { "深色主题" })
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.dark = !view.dark;
                            Theme::change(if view.dark { ThemeMode::Dark } else { ThemeMode::Light }, Some(window), cx);
                            view.persist_preferences(cx);cx.notify();
                        })))
                    .child(Button::new("shortcut-choice").ghost().small().label(format!("快捷键：{}",if self.prefs.shortcut=="off"{"关闭"}else{self.prefs.shortcut.as_str()}))
                        .tooltip("切换 U / I / 禁用，重启生效")
                        .on_click(cx.listener(|v,_,_,cx|{v.prefs.shortcut=match v.prefs.shortcut.as_str(){"U"=>"I","I"=>"off",_=>"U"}.into();v.persist_preferences(cx);v.status="快捷键设置已更改，重启生效".into();cx.notify();})))
                    .child(Button::new("album-backup").ghost().small().label(if self.backup_busy{"备份中…"}else{"备份相册"})
                        .disabled(self.backup_busy || self.queue.busy || self.account.account_busy)
                        .on_click(cx.listener(|v,_,_,cx|v.export_album(cx))))
                    .child(div().flex_1())
                    .child(self.status_panel()).overflow_y_scrollbar(),
            )
            .when(show_center && self.jobs_page, |d| d.child(self.task_center(cx)))
            .when(show_center && !self.jobs_page, |d| d.child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap_2().p_3()
                    .child(div().id("drop-zone").flex().flex_wrap().items_center().gap_2()
                        .on_drop(cx.listener(|v,paths:&ExternalPaths,_,cx|v.start_upload(paths.paths().to_vec(),cx)))
                        .child(div().flex_1().min_w(px(120.)).child(div().text_lg().font_semibold().child(match self.resource.category{"image"=>"图片相册","video"=>"视频","attachment"=>"附件",_=>"资源相册"}))
                            .child(div().text_xs().text_color(self.color(0xa6a9b0,0x646a73)).child(format!("{} 项 · {}",total,if is_demo{"本地演示"}else{"离线预览"}))))
                        .child(Button::new("paste").small().label("粘贴").disabled(self.queue.busy || self.account.account_busy)
                            .on_click(cx.listener(|v,_,_,cx|v.paste_files(cx))))
                        .child(Button::new("choose-main").small().primary().label("＋ 导入").tooltip("选择或拖放文件").disabled(self.queue.busy || self.account.account_busy)
                            .on_click(cx.listener(|v,_,_,cx|v.select_files(cx)))))
                    .child(div().flex().items_center().gap_2()
                        .child(div().flex_1().min_w(px(0.)).child(Input::new(&self.resource.search_input)))
                        .child(Button::new("grid-toggle").small().label(if self.resource.grid_view{"列表"}else{"相册"})
                            .on_click(cx.listener(|v,_,_,cx|{v.resource.grid_view=!v.resource.grid_view;v.persist_preferences(cx);cx.notify();})))
                        .child(Button::new("account-scope").small().label(if self.resource.all_accounts{"全部账号"}else{"当前账号"})
                            .on_click(cx.listener(|v,_,_,cx|{v.resource.similar_to=None;v.resource.all_accounts=!v.resource.all_accounts;v.resource.tag_filter=None;v.reset_history(cx);}))))
                    .when(self.resource.similar_to.is_some(),|d|d.child(div().flex().items_center().gap_2()
                        .child(div().text_xs().child(format!("相似图片 · {} · 同账号本地微缩图",self.resource.similar_to.as_ref().map(|a|a.name.as_str()).unwrap_or(""))))
                        .child(Button::new("exit-similar").small().label("退出").on_click(cx.listener(|v,_,_,cx|{v.resource.similar_to=None;v.reset_history(cx);})))) )
                    .child(div().flex().flex_wrap().items_center().gap_1()
                        .child(Button::new("sort-time").small().label(if self.resource.oldest { "最早优先" } else { "最新优先" })
                            .on_click(cx.listener(|v,_,_,cx|{v.resource.oldest=!v.resource.oldest;v.persist_preferences(cx);v.reset_history(cx);})))
                        .child(Button::new("favorites").small().selected(self.resource.favorites_only).label("收藏").on_click(cx.listener(|v,_,_,cx|{v.resource.similar_to=None;v.resource.favorites_only=!v.resource.favorites_only;v.persist_preferences(cx);v.reset_history(cx);})))
                        .child(Button::new("date-groups").small().selected(self.resource.group_dates).label("日期分组").on_click(cx.listener(|v,_,_,cx|{v.resource.group_dates=!v.resource.group_dates;v.persist_preferences(cx);cx.notify();})))
                        .when(self.resource.grid_view,|d|d
                            .child(Button::new("justified").small().selected(self.resource.justified).label(if self.resource.masonry {"瀑布流"}else if self.resource.justified {"等高拼图"}else{"等宽网格"}).on_click(cx.listener(|v,_,_,cx|{if v.resource.masonry{v.resource.masonry=false;v.resource.justified=false;}else if v.resource.justified{v.resource.justified=false;v.resource.masonry=true;}else{v.resource.justified=true;}v.persist_preferences(cx);cx.notify();})))
                            .child(Button::new("tile-smaller").small().label("−").disabled(self.resource.tile_size<=96.).on_click(cx.listener(|v,_,_,cx|{v.resource.tile_size=(v.resource.tile_size-16.).max(96.);v.persist_preferences(cx);cx.notify();})))
                            .child(div().text_xs().child(format!("{:.0}px",self.resource.tile_size)))
                            .child(div().debug_selector(||"album-density-increase".into()).child(Button::new("tile-larger").small().label("+").disabled(self.resource.tile_size>=240.).on_click(cx.listener(|v,_,_,cx|{v.resource.tile_size=(v.resource.tile_size+16.).min(240.);v.persist_preferences(cx);cx.notify();}))))))
                    .when(self.resource.tag_filter.is_some(),|d|d.child(div().flex().items_center().gap_2()
                        .child(format!("标签：{}",self.resource.tag_filter.as_deref().unwrap_or_default()))
                        .child(Button::new("clear-tag").small().label("清除").on_click(cx.listener(|v,_,_,cx|{v.resource.tag_filter=None;v.reset_history(cx);})))) )
                    .when(!self.resource.selected_ids.is_empty(),|d|d.child(div().flex().gap_2().items_center()
                        .child(div().flex_1().min_w(px(0.)).child(Input::new(&self.resource.batch_tag_input)))
                        .child(Button::new("batch-add-tags").small().label("添加标签").on_click(cx.listener(|v,_,_,cx|v.apply_batch_tags(false,cx))))
                        .child(Button::new("batch-remove-tags").small().label("移除标签").on_click(cx.listener(|v,_,_,cx|v.apply_batch_tags(true,cx))))))
                    .child(div().flex().flex_wrap().gap_1().items_center()
                        .child(Button::new("select-page").small().label("选择已加载（最多150）")
                            .on_click(cx.listener(|view, _, _, cx| { view.resource.selected_ids = view.resource.assets.iter().take(150).map(|a| a.id).collect(); cx.notify(); })))
                        .when(!self.resource.selected_ids.is_empty(),|d|d.child(Button::new("batch-copy").small().label(format!("复制已选 ({})", self.resource.selected_ids.len()))
                            .disabled(self.resource.selected_ids.is_empty())
                            .on_click(cx.listener(|view, _, _, cx| view.copy_selected(true,cx)))))
                        .when(!self.resource.selected_ids.is_empty(),|d|d.child(Button::new("batch-url").small().label("复制 URL").disabled(self.resource.selected_ids.is_empty())
                            .on_click(cx.listener(|v,_,_,cx| v.copy_selected(false,cx)))))
                        .when(!self.resource.selected_ids.is_empty(),|d|d.child(Button::new("clear-selection").small().label("取消选择").disabled(self.resource.selected_ids.is_empty())
                            .on_click(cx.listener(|v,_,_,cx| {v.resource.selected_ids.clear();cx.notify();})))))
                    .child(
                        div()
                            .id("asset-history")
                            .flex_1()
                            .min_h(px(0.))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(gallery)
                            .when(self.resource.loading_history, |container| container.child("正在读取资源…"))
                            .when(self.resource.history_error.is_some(), |container| {
                                container.child(format!("历史读取失败：{}", self.resource.history_error.as_deref().unwrap_or("")))
                            })
                            .when(self.resource.assets.is_empty() && !self.resource.loading_history && self.resource.history_error.is_none(), |container| {
                                container.child(if self.resource.search.is_empty() && self.resource.category.is_empty() {
                                    "暂无上传记录，选择图片或附件开始"
                                } else {
                                    "没有找到符合条件的资源"
                                })
                            })
                            .overflow_hidden(),
                    )
                    .child(
                        div().flex().items_center().justify_between().gap_2()
                            .child(format!("已加载 {} / {} 项",self.resource.assets.len(),self.resource.total_count))
                            .when(self.resource.history_error.is_some(),|d|d.child(Button::new("refresh-history").small().label("重试读取")
                                .on_click(cx.listener(|v,_,_,cx|{v.resource.history_error=None;if v.resource.assets.is_empty(){v.reload(cx);}else{v.load_more(cx);}}))))
                            .child(Button::new("next-page").small().label(if self.resource.loading_history{"加载中…"}else if self.resource.has_more{"加载更多"}else{"已到底"})
                                .disabled(self.resource.loading_history || !self.resource.has_more)
                                .on_click(cx.listener(|v,_,_,cx|v.load_more(cx))))
                    )
                    .child(
                        div()
                            .px_1().text_xs()
                            .opacity(feedback_opacity)
                            .child(self.status.clone()),
                    ),
            ))
            .when(show_inspector, |d| d.child(div().when(narrow, |d| d.flex_1()).child(self.inspector(narrow,cx))))
            .into_any_element()
    }
}
