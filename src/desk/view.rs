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
            tree=tree.child(SidebarMenuItem::new("不限标签").active(self.page==Page::Gallery&&self.resource.similar_to.is_none()&&self.resource.tag_filter.is_none())
                .on_click(cx.listener(|v,_,w,cx|{v.navigate_page(Page::Gallery,cx);v.resource.similar_to=None;v.resource.tag_filter=None;v.reset_history(cx);v.focus_gallery_destination(w,cx);})).render("tag-all",w,cx));
            for (i,path) in nodes.into_iter().enumerate() {
                let depth=path.matches('/').count().min(5);
                let title=path.rsplit('/').next().unwrap_or(&path).to_string();
                let selected=self.page==Page::Gallery&&self.resource.similar_to.is_none()&&self.resource.tag_filter.as_ref()==Some(&path);
                tree=tree.child(div().pl(px(depth as f32*10.+8.)).child(SidebarMenuItem::new(title).icon(IconName::FolderOpen).active(selected)
                    .on_click(cx.listener(move |v,_,w,cx|{v.navigate_page(Page::Gallery,cx);v.resource.similar_to=None;v.resource.tag_filter=Some(path.clone());v.reset_history(cx);v.focus_gallery_destination(w,cx);})).render(("tag-node",i),w,cx)));
            }
        }
        tree
    }

    pub(super) fn task_center(&self, wide:bool,cx: &mut Context<Self>) -> impl IntoElement {
        if self.upload_review.is_some(){
            return div().flex_1().min_w(px(0.)).h_full().flex().flex_col().p(px(self.spacing().pad)).child(self.upload_review_panel(wide,cx)).into_any_element();
        }
        let history=self.job_history.read(cx);
        let queue=self.queue.read(cx);
        let command=queue.queue_control.load(Ordering::Relaxed);
        let mut list = div().id("jobs-list").flex_1().min_h(px(0.)).flex().flex_col();
        for job in &history.jobs {
            let id = job.id;
            let active = self.queue.read(cx).active_job == Some(id);
            let state = if active { if queue.transfer.cancel.load(Ordering::Relaxed){"中止请求中"}else{"处理中"} } else { match job.state.as_str() {
                "done" => "完成", "failed" => "失败", "pending" => "待处理",
                "paused" => "暂停", "cancelled" => "取消", "running" | "unknown" => "结果待核实", _ => "未知",
            }};
            let retry = !active&&matches!(job.state.as_str(), "pending" | "paused" | "failed");
            let result_url=job.result_url.clone();let result_asset_id=job.result_asset_id;
            let total = self.queue.read(cx).transfer.total.load(Ordering::Relaxed);
            let bytes = self.queue.read(cx).transfer.bytes.load(Ordering::Relaxed).min(total);
            let mut row = div().flex().flex_col().gap_1().py_3().px_2()
                .border_b_1().border_color(self.palette().line)
                .child(div().flex().items_center().gap_2()
                    .child(div().size(px(42.)).flex_shrink_0().rounded(px(5.)).overflow_hidden().bg(self.palette().subtle).flex().items_center().justify_center()
                        .child(if let Some(path)=&job.thumbnail{img(path.clone()).image_cache(&self.resource.image_cache).size_full().object_fit(ObjectFit::Contain).into_any_element()}
                            else{div().text_color(self.palette().muted).child(if uploader::asset_category(&job.path)=="image"{IconName::GalleryVerticalEnd}else{IconName::File}).into_any_element()}))
                    .child(div().flex_1().min_w(px(0.)).flex().flex_col().gap_1()
                        .child(div().font_medium().truncate().child(job.path.file_name().unwrap_or_default().to_string_lossy().into_owned()))
                        .child(div().text_xs().text_color(self.palette().muted).truncate().child(if job.state=="done"{"资源已记录"}else if active{"正在处理"}else{"本地任务"})))

                    .child(div().w(px(82.)).flex_shrink_0().text_xs().text_color(if job.state=="failed"||job.state=="unknown"{self.palette().error}else if job.state=="done"{self.palette().accent}else{self.palette().muted}).child(state))
                    .child(div().w(px(128.)).flex_shrink_0().flex().justify_end()
                    .when(job.state=="done"&&result_asset_id.is_some(),|d|d.child(div().debug_selector(move||format!("locate-job-{id}")).child(Button::new(("locate-job",id as usize)).small().ghost().label("定位").tooltip("在当前账号图库中定位资源").disabled(self.account_busy(cx)).on_click(cx.listener(move|v,_,w,cx|{if let Some(asset_id)=result_asset_id{v.locate_job_asset(asset_id,w,cx);}})))))
                    .when(job.state=="done"&&result_url.is_some(),|d|d.child(Button::new(("copy-job-result",id as usize)).small().ghost().icon(IconName::Copy).tooltip("复制资源链接").on_click(cx.listener(move|v,_,w,cx|{if let Some(url)=&result_url{v.copy_result_text(url.clone(),false,w,cx);}}))))
                    .when(retry, |d| d.child(Button::new(("retry-job",id as usize)).small().ghost()
                        .label(if job.state=="failed"{"重试"}else{"继续"}).tooltip("重试前请确认远端结果").disabled(self.queue.read(cx).busy || self.account_busy(cx))
                        .on_click(cx.listener(move |view, _, _, cx| view.retry_one(id,cx)))))
                    .when(matches!(job.state.as_str(),"unknown"|"running") && !active, |d| d.child(Button::new(("ack-job",id as usize)).small()
                        .label("核实结果…").tooltip("先核实远端是否已存在文件").disabled(self.queue.read(cx).busy || self.account_busy(cx))
                        .on_click(cx.listener(move |v,_,w,cx| v.acknowledge_job(id,w,cx)))))));
            if active {
                row = row.child(div().text_xs().text_color(self.palette().muted).child(queue.transfer_label()));
                if total > 0 {
                    row = row.child(gpui_kit::component::progress::Progress::new(("upload-progress",id as usize)).xsmall().value(bytes as f32 / total as f32 * 100.).accessibility_label("请求体读取进度"));
                }
            }
            if let Some((state,error))=&job.preview{
                if state=="failed"{row=row.child(div().flex().items_center().gap_2().child(div().flex_1().min_w(px(0.)).text_xs().text_color(self.palette().error).child(format!("{}；预览失败：{error}",if self.config.mode==Mode::Demo{"已记录"}else{"已上传"})))
                    .when_some(result_asset_id,|d,asset_id|d.child(Button::new(("retry-preview",id as usize)).xsmall().ghost().label("重试预览").disabled(self.preview_worker.read(cx).busy||self.account_busy(cx)).on_click(cx.listener(move|v,_,_,cx|v.repair_local_previews(vec![asset_id],cx))))));}
                else if state=="pending"{row=row.child(div().flex().items_center().gap_2().child(div().flex_1().text_xs().text_color(self.palette().muted).child("资源已入库 · 等待后台生成缩略图"))
                    .when(!self.preview_worker.read(cx).busy,|d|d.child(Button::new(("resume-preview",id as usize)).xsmall().ghost().label("继续预览").disabled(self.account_busy(cx)).on_click(cx.listener(|v,_,_,cx|v.kick_previews(cx))))));}
                else if state=="ready"||state=="remote_ready"{row=row.child(div().text_xs().text_color(self.palette().muted).child("本地缩略图已就绪"));}
            }
            if !job.error.is_empty() { row = row.child(div().text_sm().text_color(self.palette().error).child(job.error.clone())); }
            list = list.child(row);
        }
        div().flex_1().min_w(px(0.)).flex().flex_col().gap_3().p(px(self.spacing().pad))
            .drag_over::<ExternalPaths>({let p=self.palette();move|style,_,_,_|style.bg(p.accent_soft)})
            .on_drop(cx.listener(|v,paths:&ExternalPaths,_,cx|v.review_files(paths.paths().to_vec(),cx)))
            .child(div().flex().items_center().gap_2()
                .child(div().flex_1().min_w(px(0.)).flex().flex_col().gap_1().child(div().text_lg().font_semibold().child("上传任务"))
                    .child(div().text_xs().text_color(self.palette().muted).child("跟踪上传、预览与恢复状态")))
                .child(Button::new("jobs-add").small().primary().icon(IconName::Plus).label("添加文件").disabled(self.queue.read(cx).busy||self.picker_pending||self.account_busy(cx)).on_click(cx.listener(|v,_,_,cx|v.select_files(cx))))
                .child(Button::new("refresh-jobs").small().ghost().label("刷新").on_click(cx.listener(|v,_,_,cx| v.refresh_jobs(cx))))
                .child(Button::new("resume-jobs").small().label("恢复任务").disabled(self.queue.read(cx).busy || self.account_busy(cx) || history.recovery_count == 0)
                    .on_click(cx.listener(|v,_,_,cx| v.resume_jobs(cx)))))
            .child(div().flex().justify_between().items_center().text_xs().text_color(self.palette().muted)
                .child(if self.config.mode==Mode::Demo{"本地演示".to_owned()}else{format!("语雀账号 {}",self.config.owner_id())})
                .child(format!("{} 项可恢复",history.recovery_count)))
            .when(queue.busy,|d|d.child(div().flex().flex_col().gap_2().px_3().py_2().rounded_md().bg(self.palette().surface)
                .child(div().flex().items_center().gap_2().child(div().flex_1().min_w(px(0.)).text_sm().font_medium().child(queue.activity_label()))
                    .when(!queue.preparing&&queue.queue_total>0,|d|d.child(div().text_xs().text_color(self.palette().muted).child(format!("{}/{}",queue.queue_total.saturating_sub(queue.queue_count),queue.queue_total)))))
                .when(queue.active_job.is_some_and(|id|!history.jobs.iter().any(|j|j.id==id)),|d|d.child(div().text_xs().text_color(self.palette().muted).child(queue.transfer_label())))
                .when(!queue.preparing&&queue.queue_total>0,|d|d.child(gpui_kit::component::progress::Progress::new("task-batch-progress").xsmall().value(queue.queue_total.saturating_sub(queue.queue_count) as f32/queue.queue_total as f32*100.).accessibility_label("批次处理进度")))
                .when(!queue.preparing, |d| d.child(div().flex().flex_wrap().gap_2()
                    .child(Button::new("jobs-pause").small().label(if command==1{"已请求暂停"}else{"文件结束后暂停"}).disabled(command!=0).on_click(cx.listener(|v,_,_,cx|v.queue.update(cx,|q,cx|q.command(1,cx)))))
                    .child(Button::new("jobs-cancel").small().label(if command==2{"已请求取消"}else{"取消剩余"}).disabled(command==2).on_click(cx.listener(|v,_,_,cx|v.queue.update(cx,|q,cx|q.command(2,cx)))))
                    .child(Button::new("cancel-active").small().ghost().label("中止当前读取")
                        .disabled(queue.active_job.is_none()||queue.transfer.cancel.load(Ordering::Relaxed))
                        .on_click(cx.listener(|v,_,_,cx|v.queue.update(cx,|q,cx|q.command(3,cx)))))))))
            .child(div().flex().items_center().px_2().py_2().border_y_1().border_color(self.palette().line).text_xs().text_color(self.palette().muted)
                .child(div().flex_1().child("文件"))
                .gap_2().child(div().w(px(82.)).child("状态"))
                .child(div().w(px(128.)).text_align(TextAlign::Right).child("操作")))
            .when(history.loading, |d| d.child(div().text_xs().text_color(self.palette().muted).child("正在读取任务…")))
            .when(history.jobs.is_empty() && !history.loading && history.error.is_none(), |d| d.child(div().flex().flex_col().items_center().gap_2().py_6().text_color(self.palette().muted)
                .child(IconName::File).child(div().font_medium().text_color(self.palette().text).child("暂无上传任务"))
                .child(div().text_xs().child("添加或拖入文件，确认后开始上传"))))
            .when(history.error.is_some(), |d| d.child(div().text_xs().text_color(self.palette().error).child(history.error.clone().unwrap_or_default())))
            .child(list.overflow_y_scrollbar())
            .child(div().flex().items_center().gap_2()
                .child(Button::new("jobs-prev").small().label("上一页").disabled(history.loading || history.cursors.len() <= 1)
                    .on_click(cx.listener(|v,_,_,cx| { v.page_jobs(false,cx) })))
                .child(format!("第 {} 页", history.cursors.len()))
                .child(Button::new("jobs-next").small().label("下一页").disabled(history.loading || !history.more)
                    .on_click(cx.listener(|v,_,_,cx| { v.page_jobs(true,cx) }))))
            .child(div().border_t_1().border_color(self.palette().line).pt_2().text_xs().text_color(self.palette().muted).child(self.status.clone()))
            .into_any_element()
    }

    pub(super) fn inspector(&self, narrow: bool, active:bool, cx: &mut Context<Self>) -> impl IntoElement {
        let header=div().id("inspector-header").debug_selector(||"inspector-header".into()).flex_shrink_0().flex().items_center().justify_between()
            .child(div().font_semibold().child(if self.resource.selected_ids.len()>1{"批量选择"}else if self.resource.selected_asset.is_some(){"详情"}else{"资源详情"}))
            .child(Button::new("close-inspector").disabled(!active).ghost().small().icon(if narrow{IconName::ArrowLeft}else{IconName::PanelRightClose})
                .label(if narrow{"返回图库"}else{""}).tooltip("关闭详情 · Esc").on_click(cx.listener(|v,_,w,cx|v.set_inspector(false,false,w,cx))));
        let mut panel=div().id("inspector-body").flex_1().min_h(px(0.)).min_w(px(0.)).flex().flex_col().gap(px(if narrow{10.}else{self.spacing().section}));
        if self.resource.selected_ids.len()>1{
            panel=panel
                .child(div().text_lg().font_medium().child(format!("{} 项资源",self.resource.selected_ids.len())))
                .child(Button::new("multi-copy").disabled(!active).small().label("复制链接").on_click(cx.listener(|v,_,_,cx|v.copy_selected(false,cx))))
                .child(Button::new("multi-md").disabled(!active).small().ghost().label("复制 Markdown").on_click(cx.listener(|v,_,_,cx|v.copy_selected(true,cx))))
                .child(div().text_xs().text_color(self.palette().muted).child("批量标签请在图库选择工具栏操作。"));
        }else if let Some(asset) = &self.resource.selected_asset {
            let url_asset = asset.clone();
            let md_asset = asset.clone();
            let type_label=match asset.category.as_str(){"image"=>"图片","video"=>"视频",_=>"附件"};
            panel=panel.child(div().flex().flex_col().gap_1().child(div().font_semibold().child(asset.name.clone()))
                .child(div().flex().items_center().gap_2().text_xs().text_color(self.palette().muted)
                    .child(div().px_2().py_1().rounded(px(4.)).bg(self.palette().surface).child(type_label))
                    .child(file_size(asset.bytes))
                    .when(asset.category=="image",|d|d.child(div().text_color(if self.resource.thumbnail_path.is_some(){self.palette().accent}else{self.palette().muted}).child(if self.resource.thumbnail_path.is_some(){"预览就绪"}else if self.resource.preview_loading{"读取中"}else{"待恢复"})))));
            if asset.category=="image"{
                panel=panel.child(div().h(px(if narrow{112.}else if self.prefs.comfortable{208.}else{184.})).w_full().rounded(px(design::CONTROL_RADIUS)).overflow_hidden().bg(self.palette().subtle).flex().items_center().justify_center()
                    .child(if let Some(path)=&self.resource.thumbnail_path{img(path.clone()).image_cache(&self.resource.image_cache).size_full().object_fit(ObjectFit::Contain).into_any_element()}else{div().text_xs().text_color(self.palette().muted).child(if self.resource.preview_loading{"读取本地预览…"}else{"无本地预览"}).into_any_element()}))
                    .child(div().debug_selector(||"detail-preview".into()).child(Button::new("large-preview").small().ghost().icon(IconName::Maximize).label("查看图片").disabled(!active||self.resource.preview_loading).on_click(cx.listener(|v,_,w,cx|v.open_large_preview(w,cx)))));
                if self.resource.thumbnail_path.is_none()&&!self.resource.preview_loading{
                    panel=panel.child(Button::new("preview-image").disabled(!active).small().label("重新读取本地预览").on_click(cx.listener(|v,_,_,cx|v.load_preview(cx))));
                    if asset.provider=="yuque"{panel=panel.child(Button::new("repair-remote-thumbnail").small().label("从语雀恢复缩略图").disabled(!active).tooltip("主动请求可信语雀 CDN；不重新上传原文件").on_click(cx.listener(|v,_,_,cx|v.repair_preview(true,cx))));}
                    if let Some(error)=&self.resource.preview_error{panel=panel.child(div().text_xs().text_color(self.palette().error).child(error.clone()));}
                }
            }
            panel=panel
                .child(Button::new("detail-url").disabled(!active).small().primary().icon(IconName::Copy).label("复制链接")
                    .on_click(cx.listener(move |view, _, w, cx| view.copy_asset(&url_asset, false,w,cx))))
                .child(div().flex().flex_wrap().gap(px(self.spacing().control))
                    .child(Button::new("detail-open").disabled(!active).small().ghost().label(if asset.provider=="demo" {"打开文件"} else {"浏览器打开"}).on_click(cx.listener(|v,_,_,cx|v.open_selected_resource(cx))))
                    .child(Button::new("detail-md").disabled(!active).small().ghost().label("复制 Markdown").on_click(cx.listener(move |view, _, w, cx| view.copy_asset(&md_asset, true,w,cx)))))
;
            let favorite_asset=asset.clone();
            let similar_asset=asset.clone();
            panel=panel.child(div().flex().flex_wrap().items_center().gap(px(self.spacing().control))
                .child(Button::new("detail-favorite").disabled(!active||self.resource_write_busy).small().ghost().icon(if self.resource.favorites.contains(&asset.id){IconName::StarFill}else{IconName::Star}).label(if self.resource.favorites.contains(&asset.id){"取消收藏"}else{"收藏资源"})
                    .on_click(cx.listener(move |v,_,_,cx|v.toggle_favorite(&favorite_asset,cx))))
                .when(asset.category=="image",|d|d.child(Button::new("detail-similar").disabled(!active).small().ghost().icon(IconName::Search).label("相似图片")
                    .on_click(cx.listener(move |v,_,w,cx|{v.find_similar(similar_asset.clone(),cx);v.focus_gallery_destination(w,cx);})))));
            panel=panel
                .child(div().h(px(1.)).bg(self.palette().line))
                .child(div().text_xs().font_medium().text_color(self.palette().muted).child("标签管理"));
            panel = panel.child(Input::new(&self.inspector_model.read(cx).input).disabled(!active||!self.inspector_model.read(cx).ready||self.resource_write_busy))
                .child(div().flex().justify_end().child(Button::new("save-tags").small().label("保存标签").disabled(!active||!self.inspector_model.read(cx).ready||self.resource_write_busy)
                    .on_click(cx.listener(|v,_,_,cx|v.save_selected_tags(cx)))));
            if let Some(error)=&self.inspector_model.read(cx).error{
                panel=panel.child(div().text_xs().text_color(self.palette().error).child(error.clone()))
                    .child(Button::new("retry-tags").disabled(!active).small().ghost().label("重试读取标签").on_click(cx.listener(|v,_,w,cx|{if let Some(a)=v.resource.selected_asset.clone(){let storage=v.storage.clone();v.inspector_model.update(cx,|m,cx|m.select(a.id,storage,w,cx));}})));
            }
            let metadata=format!("文件名：{}\n资源 ID：{}\n大小：{} 字节\n来源：{} / {}\n上传时间：{}\nSHA-256：{}\nFile key：{}\n链接：{}",asset.name,asset.id,asset.bytes,asset.provider,asset.owner_id,asset.created_at,asset.sha256,asset.filekey,asset.url);
            panel=panel.child(div().h(px(1.)).bg(self.palette().line))
                .child(div().flex().items_center().justify_between().gap_2()
                    .child(div().text_xs().font_medium().text_color(self.palette().muted).child("文件属性"))
                    .child(Button::new("copy-metadata").xsmall().ghost().icon(IconName::Copy).tooltip("复制完整文件属性").disabled(!active).on_click(cx.listener(move|v,_,w,cx|v.copy_text_feedback(metadata.clone(),"文件属性已复制",w,cx)))))
                .child(div().flex().items_center().justify_between().gap_2().text_xs()
                    .child(div().text_color(self.palette().muted).child("扩展名")).child(std::path::Path::new(&asset.name).extension().and_then(|s|s.to_str()).unwrap_or("未知").to_uppercase()))
                .child(div().flex().items_center().justify_between().gap_2().text_xs()
                    .child(div().text_color(self.palette().muted).child("大小")).child(file_size(asset.bytes)))
                .child(div().flex().items_center().justify_between().gap_2().text_xs()
                    .child(div().text_color(self.palette().muted).child("来源")).child(div().truncate().child(if asset.provider=="demo"{"本地演示".to_owned()}else{format!("语雀 / {}",asset.owner_id)})))
                .child(div().flex().items_center().justify_between().gap_2().text_xs()
                    .child(div().text_color(self.palette().muted).child("上传时间")).child(asset.created_at.get(..16).unwrap_or(&asset.created_at).replace("T"," ")))
                .child(div().flex().items_center().justify_between().gap_2().text_xs()
                    .child(div().text_color(self.palette().muted).child("资源 ID")).child(asset.id.to_string()))
                .child(div().flex().flex_col().gap_1().p_2().rounded(px(design::CONTROL_RADIUS)).bg(self.palette().surface)
                    .child(div().text_xs().text_color(self.palette().muted).child("SHA-256"))
                    .child(div().text_xs().truncate().child(asset.sha256.clone()))
                    .when(!asset.filekey.is_empty(),|d|d.child(div().text_xs().text_color(self.palette().muted).child("File key")).child(div().text_xs().truncate().child(asset.filekey.clone())))
                    .child(div().text_xs().text_color(self.palette().muted).child("资源链接"))
                    .child(div().text_xs().truncate().child(asset.url.clone())));
        } else {
            panel = panel.child(div().text_sm().child("选择列表或网格中的资源，查看属性及复制链接。"));
        }
        div().id("inspector").when(active,|d|d.track_focus(&self.resource.inspector_focus).key_context("YuqueInspector"))
            .on_action(cx.listener(|v,_:&shell::InspectorClose,w,cx|{if v.inspector_open{v.set_inspector(false,false,w,cx);}}))
            .h_full().min_w(px(0.)).min_h(px(0.)).flex().flex_col().gap(px(self.spacing().section)).p(px(self.spacing().pad))
            .w_full()
            .bg(self.palette().sidebar).border_l_1().border_color(self.palette().line)
            .child(header).child(panel.overflow_y_scrollbar())
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
                    let ratio=self.resource.ratios.get(&asset.id).copied().filter(|r|r.is_finite()&&*r>0.);
                    let image_size=ratio.map(|r|{
                        let w=tile_width.min(tile_image_height*r);
                        (w,(w/r).min(tile_image_height))
                    });
                    let menu_asset=asset.clone();let preview_asset=asset.clone();
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
                            .rounded(px(design::CONTROL_RADIUS)).bg(if thumbnail.is_some(){palette.surface}else{palette.subtle}).flex().items_center().justify_center()
                            .child(match thumbnail {
                                Some(path)=>{
                                    let image=img(path).image_cache(&self.resource.image_cache).object_fit(ObjectFit::Contain);
                                    if let Some((w,h))=image_size{div().h_full().w_full().flex().justify_center().items_center().child(image.w(px(w)).h(px(h))).into_any_element()}
                                    else{image.size_full().into_any_element()}
                                },
                                None=>div().flex().flex_col().items_center().gap_1().text_color(palette.muted)
                                    .child(if asset.category=="video"{IconName::Play}else if asset.category=="image"{IconName::GalleryVerticalEnd}else{IconName::File})
                                    .child(div().text_xs().child(if asset.category=="image" {if self.thumbnails.read(cx).attempted(asset.id){"预览不可用"}else{"等待预览"}} else if asset.category=="video" {"视频"} else {"附件"})).into_any_element()
                            })
                            .when(checked||selected,|d|d.child(div().absolute().inset_0().rounded(px(design::CONTROL_RADIUS)).border_2().border_color(palette.accent)))
                            .when(revealed,|d|d.child(div().debug_selector(move||format!("grid-select-{index}")).absolute().top(px(5.)).left(px(5.)).rounded(px(5.)).bg(palette.surface).opacity(0.85+0.15*hover_alpha)
                                .child(Button::new(("grid-select",index)).xsmall().selected(checked).icon(if checked{IconName::Check}else{IconName::Square}).tooltip("多选")
                                    .on_click(cx.listener(move|v,_,w,cx|{cx.stop_propagation();v.toggle_album_selection(asset_id,w,cx);})))))
                            .when(starred,|d|d.child(div().absolute().top(px(6.)).right(px(6.)).p_1().rounded_sm().bg(palette.surface).text_color(palette.accent).child(IconName::StarFill)))
                            .when(revealed,|d|d.child(div().absolute().bottom(px(6.)).right(px(6.)).rounded(px(5.)).bg(palette.surface).opacity(hover_alpha.max(0.7))
                                .child(div().flex().items_center().gap_1()
                                    .when(asset.category=="image",|d|d.child(div().debug_selector(move||format!("tile-preview-{index}")).child(Button::new(("tile-preview",index)).xsmall().ghost().icon(IconName::Maximize).tooltip("查看图片").on_click(cx.listener(move|v,_,w,cx|{cx.stop_propagation();v.click_tile(preview_asset.clone(),2,Modifiers::default(),w,cx);})))))
                                    .child(Button::new(("tile-copy",index)).xsmall().ghost().icon(IconName::Copy).tooltip("复制链接").on_click(cx.listener(move|v,_,w,cx|{cx.stop_propagation();v.copy_asset(&url_asset,false,w,cx);})))))))
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
                        .on_click(cx.listener(move |view, _, w, cx|{cx.stop_propagation();view.toggle_album_selection(asset_id,w,cx);})))
                    .child(Button::new(("detail", index)).small().selected(selected).label("详情")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            cx.stop_propagation();view.show_asset_details(detail_asset.clone(), window, cx);
                        })))
                    .when(!narrow, |d| d.child(
                        Button::new(("url", index)).small()
                            .label("复制 URL")
                            .on_click(cx.listener(move |view, _, w, cx| {
                                cx.stop_propagation();view.copy_asset(&url_asset, false,w,cx);
                            })),
                    ))
                    .when(!narrow, |d| d.child(
                        Button::new(("markdown", index)).small()
                            .label("Markdown")
                            .on_click(cx.listener(move |view, _, w, cx| {
                                cx.stop_propagation();view.copy_asset(&markdown_asset, true,w,cx);
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
                    .child(div().flex_1().min_w(px(0.)).flex().items_center().gap_2().child(div().flex_1().min_w(px(0.)).font_semibold().truncate().child(self.resource.selected_asset.as_ref().map(|a|a.name.clone()).unwrap_or_default())))
                    .child(Button::new("preview-prev").ghost().small().icon(IconName::ChevronLeft).tooltip("上一张 · ←").disabled(!previous).on_click(cx.listener(|v,_,w,cx|v.step_preview(false,w,cx))))
                    .child(Button::new("preview-next").ghost().small().icon(IconName::ChevronRight).tooltip("下一张 · →").disabled(!next).on_click(cx.listener(|v,_,w,cx|v.step_preview(true,w,cx))))
                    .child(Button::new("zoom-out").ghost().small().icon(IconName::Minus).tooltip("缩小 · -").on_click(cx.listener(|v,_,_,cx|v.zoom_by(-0.25,cx))))
                    .child(div().text_xs().text_color(self.palette().muted).child(format!("适应 {:.0}%",zoom*100.)))
                    .child(Button::new("zoom-in").ghost().small().icon(IconName::Plus).tooltip("放大 · +").on_click(cx.listener(|v,_,_,cx|v.zoom_by(0.25,cx))))

                    .child(Button::new("zoom-reset").ghost().small().label("适应窗口").on_click(cx.listener(|v,_,_,cx|{v.resource.zoom=1.;v.resource.pan=point(px(0.),px(0.));cx.notify();}))))
                .child({
                    let panel=div().id("preview-canvas").bg(self.palette().stage).rounded(px(design::CONTROL_RADIUS)).relative().flex_1().min_h(px(0.)).overflow_hidden().flex().items_center().justify_center()
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
                    else { panel.child(div().flex().flex_col().items_center().gap_2()
                .child(div().text_sm().text_color(self.palette().muted).child(self.resource.preview_error.clone().unwrap_or_else(|| if self.resource.preview_loading{"正在读取本地预览…".into()}else{"没有可显示的本地预览，请按需读取原图。".into()})))
                .when(!self.resource.preview_loading,|d|d.child(Button::new("viewer-retry-local").small().label("重新读取本地").on_click(cx.listener(|v,_,_,cx|v.repair_preview(false,cx)))))
                .when(!self.resource.preview_loading&&self.resource.selected_asset.as_ref().is_some_and(|a|a.provider=="yuque"),|d|d.child(Button::new("viewer-repair-remote").small().label("从语雀恢复缩略图").on_click(cx.listener(|v,_,_,cx|v.repair_preview(true,cx))))) ) }
                }).child(self.preview_footer(cx)).into_any_element()

    }
    #[inline(never)]
    fn preview_footer(&self,cx:&mut Context<Self>)->AnyElement{
        let p=self.palette();
        let current=self.resource.selected_asset.as_ref().map(|a|a.id);
        let online=self.resource.selected_asset.as_ref().is_some_and(|a|a.provider=="yuque");
        let position=self.resource.assets.iter().filter(|a|a.category=="image").position(|a|Some(a.id)==current).map(|i|i+1).unwrap_or(0);
        let loaded=self.resource.assets.iter().filter(|a|a.category=="image").count();
        let position_label=format!("{position} / {loaded}{}",if self.resource.has_more{"+"}else{""});
        let quality=self.viewer_model.read(cx).quality.as_ref();
        let mode=if let Some(q)=quality{
            let source=if q.remote{"在线图片（可能经 CDN 转码）"}else{"本地原图"};
            if (q.width,q.height)==(q.shown_width,q.shown_height){format!("{source} · {}×{}",q.width,q.height)}else{format!("{source} · {}×{} · 预览 {}×{}",q.width,q.height,q.shown_width,q.shown_height)}
        }else if self.resource.large_preview_path.is_some(){"离线微缩图".into()}else{"暂无图像".into()};
        let message=if self.resource.pending_preview.is_some(){"正在加载下一页…"}else{self.viewer_model.read(cx).status.as_str()};
        let mut strip=div().h(px(48.)).flex().justify_center().items_center().gap_2().overflow_hidden().rounded(px(6.)).bg(p.surface);
        for a in self.preview_strip(){
            let selected=Some(a.id)==current;
            let asset=a.clone();let id=a.id;
            let mut tile=div().id(("preview-strip",a.id as usize)).debug_selector(move||format!("preview-strip-{id}")).w(px(56.)).h(px(44.)).flex_shrink_0().rounded_md().overflow_hidden().border_1().border_color(if selected{p.accent}else{p.line}).when(selected,|d|d.bg(p.accent_soft).p_1()).when(!selected,|d|d.bg(p.sidebar)).cursor_pointer().flex().items_center().justify_center()
                .on_click(cx.listener(move|v,_,w,cx|{if v.resource.selected_asset.as_ref().is_none_or(|a|a.id!=asset.id){v.select_asset(asset.clone(),w,cx);v.open_large_preview(w,cx);}}));
            if let Some(path)=self.resource.cached_thumbnails.get(&a.id){tile=tile.child(img(path.clone()).image_cache(&self.resource.image_cache).size_full().object_fit(ObjectFit::Contain));}
            else{tile=tile.child(IconName::GalleryVerticalEnd);}
            strip=strip.child(tile);
        }
        div().h(px(112.)).flex_shrink_0().flex().flex_col().gap_2()
            .child(div().h(px(28.)).flex().items_center().gap_2()
                .child(div().flex_1().min_w(px(0.)).truncate().text_xs().text_color(p.muted).child(mode))
                .when(self.viewer_model.read(cx).busy,|d|d.child(div().debug_selector(||"cancel-quality".into()).child(Button::new("cancel-quality").small().ghost().label("停止等待").tooltip("保留当前图像；后台读取可能短暂继续").on_click(cx.listener(|v,_,_,cx|v.viewer_model.update(cx,|m,cx|m.cancel_load(cx)))))))
                .when(quality.is_some()&&!self.viewer_model.read(cx).busy,|d|d.child(div().debug_selector(||"offline-quality".into()).child(Button::new("offline-quality").small().ghost().label("离线预览").tooltip("释放高清图，返回本地微缩图").on_click(cx.listener(|v,_,_,cx|{v.clear_quality(cx);cx.notify();})))))
                .child(div().debug_selector(||"local-quality".into()).child(Button::new("local-quality").small().label("本地原图").selected(quality.is_some_and(|q|!q.remote)).disabled(self.viewer_model.read(cx).busy||quality.is_some_and(|q|!q.remote)).on_click(cx.listener(|v,_,_,cx|v.load_quality(false,cx)))))
                .when(online,|d|d.child(Button::new("remote-quality").small().label("在线图片").tooltip("可信 CDN 图片，可能经过转码").selected(quality.is_some_and(|q|q.remote)).disabled(self.viewer_model.read(cx).busy||quality.is_some_and(|q|q.remote)).on_click(cx.listener(|v,_,_,cx|v.load_quality(true,cx)))))
                .child(Button::new("preview-browser").ghost().small().icon(IconName::ExternalLink).tooltip("使用系统或浏览器打开资源").on_click(cx.listener(|v,_,_,cx|v.open_selected_resource(cx)))))
            .child(div().h(px(20.)).flex().items_center().justify_center().text_xs().text_color(p.muted).gap_2().child(div().flex_shrink_0().child(position_label)).child(div().flex_1().min_w(px(0.)).truncate().child(if message.is_empty(){if self.resource.preview_reload_pending{"相册有更新，返回后刷新".to_string()}else{"左右键切图 · 原图按需加载".to_string()}}else{message.to_owned()})))
            .child(strip).into_any_element()
    }
}

impl UploadDesk{
    #[inline(never)]
    fn gallery_content(&mut self,available:f32,narrow:bool,window:&mut Window,cx:&mut Context<Self>)->AnyElement{
        use std::hash::{Hash,Hasher};
        let masonry_mode=self.resource.grid_view && self.resource.masonry;
        if masonry_mode{
            let dates=self.resource.group_dates&&self.resource.similar_to.is_none();
            let spacing=self.spacing();
            let signature=(available.to_bits(),self.resource.tile_size.to_bits(),dates,self.prefs.comfortable);
            let mut h=std::collections::hash_map::DefaultHasher::new();signature.hash(&mut h);let sig=h.finish();
            if self.resource.masonry_signature!=sig || self.resource.masonry_plan.tiles.len()>self.resource.assets.len(){
                let old_top=(-f32::from(self.resource.masonry_scroll.offset().y)).max(0.);
                let anchor=self.resource.masonry_plan.anchor(old_top);
                let plan=album::masonry(&self.resource.assets,&self.resource.ratios,available,self.resource.tile_size,dates,spacing);
                if let Some(top)=plan.restore(anchor){
                    self.resource.masonry_scroll.set_offset(point(px(0.),px(-top)));
                }
                self.resource.masonry_plan=plan;self.resource.masonry_signature=sig;
            }else if self.resource.masonry_plan.tiles.len()<self.resource.assets.len(){
                self.resource.masonry_plan.extend(&self.resource.assets,&self.resource.ratios,available,self.resource.tile_size,dates,spacing);
            }
            let handle=self.resource.masonry_scroll.clone();
            let top=(-f32::from(handle.offset().y)).max(0.).min((self.resource.masonry_plan.height-f32::from(handle.bounds().size.height)).max(0.));
            let measured=f32::from(handle.bounds().size.height);
            let viewport=if measured>1.{measured}else{f32::from(window.viewport_size().height)};
            let lo=(top-160.).max(0.);let hi=top+viewport+160.;
            let plan=&self.resource.masonry_plan;
            let mut wall=div().relative().w_full().h(px(plan.height)).flex_shrink_0();
            let day_start=plan.days.partition_point(|(_,y)|*y+spacing.day<lo);
            for (day,y) in &plan.days[day_start..]{if *y>hi{break;}
                wall=wall.child(div().absolute().top(px(*y)).left(px(0.)).h(px(self.spacing().day)).flex().items_center().text_size(px(self.spacing().caption)).font_medium().text_color(self.palette().muted).child(day.clone()));
            }
            let mut first=usize::MAX;let mut last=0;
            for t in plan.visible_tiles(lo,hi){
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
            let dates=self.resource.group_dates&&self.resource.similar_to.is_none();
            let spacing=self.spacing();
            let mut hash=std::collections::hash_map::DefaultHasher::new();
            (if self.resource.justified{self.resource.ratio_revision}else{0},available.to_bits(),self.resource.tile_size.to_bits(),
             self.resource.grid_view,self.resource.justified,dates,self.prefs.comfortable).hash(&mut hash);
            let signature=hash.finish();
            let planned_count=album::row_asset_count(&self.resource.row_plan);
            let same_geometry=self.resource.layout_signature==signature;
            if same_geometry && planned_count<self.resource.assets.len() && self.resource.album_list.item_count()==self.resource.row_plan.len() {
                if let Some((start,suffix))=album::append_rows(&self.resource.row_plan,&self.resource.assets,&self.resource.ratios,available,self.resource.tile_size,self.resource.grid_view,self.resource.justified,dates,spacing){
                    let old_len=self.resource.row_plan.len();
                    let prefix=start+self.resource.row_plan[start..].iter().zip(&suffix).take_while(|(a,b)|a==b).count();
                    self.resource.album_list.splice(prefix..old_len,start+suffix.len()-prefix);
                    let rows=std::rc::Rc::make_mut(&mut self.resource.row_plan);
                    rows.truncate(start);
                    rows.extend(suffix);
                }else{self.resource.layout_signature=0;}
            }
            if self.resource.layout_signature!=signature || album::row_asset_count(&self.resource.row_plan)!=self.resource.assets.len() {
                let old=self.resource.album_list.logical_scroll_top();
                let planned=album::plan_rows(&self.resource.assets,&self.resource.ratios,available,self.resource.tile_size,self.resource.grid_view,self.resource.justified,dates,spacing);
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
            let event_plan=std::rc::Rc::downgrade(&planned);let event_entity=entity.clone();let generation=self.resource.history_generation;
            self.resource.album_list.set_scroll_handler(move|event,window,app|{
                let span=event.visible_range.len().max(4);
                let weak=event_entity.clone();let plan=event_plan.clone();
                window.defer(app,move|_,app|{let _=weak.update(app,|v,cx|{
                    if v.resource.history_generation!=generation{return;}
                    let Some(plan)=plan.upgrade()else{return;};
                    if !std::rc::Rc::ptr_eq(&plan,&v.resource.row_plan){return;}
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
                entity.update(app,|v,cx|v.resource.row_plan.get(i).map(|row|v.album_row(row,narrow,window,cx)).unwrap_or_else(||div().into_any_element()))
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
            if self.queue.read(cx).busy { 1.0f32 } else { 0.82f32 },
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
        let nav_progress=design::tween("nav-reveal",if self.sidebar_collapsed{0.}else{1.},design::SURFACE_MS,window,cx);
        let nav_visibility=design::tween("nav-visible",if self.immersive{0.}else{1.},design::SURFACE_MS,window,cx);
        let nav_reserve=!self.immersive||nav_visibility>0.001;
        let nav_preferred=if narrow||self.prefs.sidebar_width<=0.{self.spacing().sidebar_width(narrow)}else{self.prefs.sidebar_width.clamp(168.,300.)};
        let nav_width=if !nav_reserve{0.}else if self.sidebar_collapsed&&nav_progress<=0.001{56.}else{nav_preferred};
        let preferred=if self.prefs.inspector_width>0.{self.prefs.inspector_width}else{self.spacing().inspector};
        let right_width=preferred.clamp(240.,420.).min((f32::from(window.viewport_size().width)-nav_width-460.).max(240.));
        let available = f32::from(window.viewport_size().width) - nav_width - if !narrow && reserve_inspector {right_width} else {0.} - 2.*self.spacing().pad;
        let gallery=if self.page==Page::Gallery&&show_center{self.gallery_content(available,narrow,window,cx)}else{div().into_any_element()};
        // Never access SQLite from the hot render path.
        let total = self.resource.total_count;
        let is_demo = self.config.mode == Mode::Demo;
        let detail_width=if narrow{(f32::from(window.viewport_size().width)-nav_width).max(0.)}else{right_width};
        div()
            .flex()
            .size_full()
            .text_size(px(self.spacing().body))
            .bg(self.palette().canvas)
            .text_color(self.palette().text)
            .on_mouse_move(cx.listener(|v,e:&MouseMoveEvent,_,cx|{
                if let Some((start,width))=v.sidebar_drag{
                    if e.dragging(){v.prefs.sidebar_width=(width+f32::from(e.position.x)-start).clamp(168.,300.);cx.notify();}
                    else{v.sidebar_drag=None;v.persist_preferences(cx);}
                }
                if let Some((start,width))=v.inspector_drag{
                    if e.dragging(){v.prefs.inspector_width=(width+start-f32::from(e.position.x)).clamp(240.,420.);cx.notify();}
                    else{v.inspector_drag=None;v.persist_preferences(cx);}
                }
            }))
            .on_mouse_up(MouseButton::Left,cx.listener(|v,_,_,cx|{let changed=v.inspector_drag.take().is_some()|v.sidebar_drag.take().is_some();if changed{v.persist_preferences(cx);cx.notify();}}))
            .on_mouse_up_out(MouseButton::Left,cx.listener(|v,_,_,cx|{let changed=v.inspector_drag.take().is_some()|v.sidebar_drag.take().is_some();if changed{v.persist_preferences(cx);cx.notify();}}))
             .when(nav_reserve,|root|root.child(div().id("nav-slot").debug_selector(||"nav-slot".into()).relative().w(px(nav_width)).h_full().flex_shrink_0().overflow_hidden().bg(self.palette().sidebar)
                .child(div().absolute().top_0().bottom_0().left(px(-22.*(1.-nav_visibility)-if self.sidebar_collapsed{0.}else{12.*(1.-nav_progress)})).w(px(if self.sidebar_collapsed{56.}else{nav_width})).opacity(nav_visibility*if self.sidebar_collapsed{1.}else{0.75+0.25*nav_progress})
                    .when(self.immersive,|d|d.occlude().capture_any_mouse_down(|_,_,cx|cx.stop_propagation()).capture_any_mouse_up(|_,_,cx|cx.stop_propagation()).capture_key_down(|_,_,cx|cx.stop_propagation()))
                    .child(if self.sidebar_collapsed{self.navigation_rail(cx).into_any_element()}else{self.navigation(narrow,nav_width,window,cx).into_any_element()}))
                .when(!narrow&&!self.sidebar_collapsed&&!self.immersive,|d|d.child(div().id("sidebar-resize").debug_selector(||"sidebar-resize".into()).absolute().right_0().top_0().bottom_0().w(px(6.)).cursor(CursorStyle::ResizeLeftRight)
                    .hover(|s|s.bg(self.palette().accent_soft))
                    .on_mouse_down(MouseButton::Left,cx.listener(move|v,e:&MouseDownEvent,_,cx|{v.sidebar_drag=Some((f32::from(e.position.x),nav_width));cx.stop_propagation();}))))))
            .when(self.page.settings(),|d|d.child(div().flex_1().min_w(px(0.)).h_full().flex().justify_center().opacity(surface_alpha).child(div().w_full().max_w(px(680.)).h_full().child(self.settings_view(cx)))))
            .when(show_center && self.page==Page::Jobs, |d| d.child(div().flex_1().min_w(px(0.)).h_full().flex().opacity(surface_alpha).child(self.task_center(f32::from(window.viewport_size().width)-nav_width>=720.,cx))))
            .when(show_center && self.page==Page::Gallery, |d| d.child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col().opacity(surface_alpha)
                    .gap_2().p(px(self.spacing().pad))
                    .child(self.gallery_toolbar(available<760.,total,is_demo,window,cx))
                    .child(self.selection_toolbar(cx))
                    .when(self.resource.similar_to.is_some()||self.resource.all_accounts||self.resource.tag_filter.is_some()||!self.resource.search.is_empty()||(self.resource.favorites_only&&!self.resource.category.is_empty()),|d|d.child(self.browse_context(cx)))
                    .when(self.resource.batch_tag_open&&!self.resource.selected_ids.is_empty(),|d|d.child(div().flex().gap_2().items_center()
                        .child(div().flex_1().min_w(px(0.)).child(Input::new(&self.resource.batch_tag_input)))
                        .child(Button::new("batch-add-tags").small().label("添加标签").disabled(self.resource_write_busy).on_click(cx.listener(|v,_,_,cx|v.apply_batch_tags(false,cx))))
                        .child(Button::new("batch-remove-tags").small().label("移除标签").disabled(self.resource_write_busy).on_click(cx.listener(|v,_,_,cx|v.apply_batch_tags(true,cx))))
                        .child(Button::new("batch-close").small().ghost().label("关闭").on_click(cx.listener(|v,_,_,cx|{v.resource.batch_tag_open=false;cx.notify();})))))
                    .child(
                        div()
                            .id("asset-history").debug_selector(||"asset-history".into()).track_focus(&self.resource.gallery_focus).key_context("YuqueGallery")
                            .on_action(cx.listener(|v,_:&shell::GalleryPreview,w,cx|v.command(Command::Preview,w,cx)))
                            .on_action(cx.listener(|v,_:&shell::GalleryClear,w,cx|v.command(Command::ClearSelection,w,cx)))
                            .on_action(cx.listener(|v,_:&shell::GallerySelectAll,w,cx|v.command(Command::SelectAll,w,cx)))
                            .on_action(cx.listener(|v,_:&shell::GalleryInspector,w,cx|v.command(Command::ToggleInspector,w,cx)))
                            .on_key_down(cx.listener(|v,e:&KeyDownEvent,w,cx|v.gallery_key(e,w,cx)))
                            .drag_over::<ExternalPaths>({let p=self.palette();move|style,_,_,_|style.bg(p.accent_soft)})
                            .on_drop(cx.listener(|v,paths:&ExternalPaths,_,cx|v.review_files(paths.paths().to_vec(),cx)))
                            .flex_1()
                            .min_h(px(0.))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .when(!self.resource.assets.is_empty()||self.resource.loading_history,|d|d.child(gallery))
                            .when(self.resource.loading_history&&self.resource.assets.is_empty(), |container| container.child(div().text_sm().text_color(self.palette().muted).child("正在读取资源…")))
                            .when(self.resource.history_error.is_some()&&self.resource.assets.is_empty(), |container| {
                                container.child(div().text_sm().text_color(self.palette().error).child(format!("历史读取失败：{}", self.resource.history_error.as_deref().unwrap_or(""))))
                            })
                            .when(self.resource.assets.is_empty() && !self.resource.loading_history && self.resource.history_error.is_none(), |container| {
                                let initial=!self.has_browse_filters()&&self.resource.similar_to.is_none();
                                container.child(Empty::new().header(EmptyHeader::new()
                                    .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(IconName::FolderOpen))
                                    .title(EmptyTitle::new().child(if initial{"还没有资源"}else{"没有符合条件的资源"}))
                                    .description(EmptyDescription::new().child(if initial{"拖入文件或点击上传，即可离线浏览、复制链接。"}else{"尝试调整搜索词或筛选条件。"})))
                                    .when(initial,|e|e.child(Button::new("empty-upload").small().primary().icon(IconName::Plus).label("上传文件")
                                        .disabled(self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.read(cx).busy||self.account_busy(cx)).on_click(cx.listener(|v,_,_,cx|v.select_files(cx)))))
                                    .when(!initial,|e|e.child(div().debug_selector(||"empty-clear".into()).child(Button::new("empty-clear").small().label(if self.resource.similar_to.is_some(){"返回图库"}else{"清除条件"}).on_click(cx.listener(|v,_,w,cx|{if v.resource.similar_to.is_some(){v.exit_similar(cx);}else{v.clear_browse_filters(w,cx);}v.focus_gallery_destination(w,cx);}))))))
                            })
                            .overflow_hidden(),
                    )
                    .child(div().flex().flex_col().gap(px(if self.prefs.comfortable{6.}else{self.spacing().section}))
                      .child(
                        div().id("gallery-status").debug_selector(||"gallery-status".into()).h(px(30.)).flex_shrink_0().border_t_1().border_color(self.palette().line).flex().items_center().justify_between().gap_2().text_xs().text_color(self.palette().muted)
                            .child(div().flex_1().min_w(px(0.)).truncate().opacity(feedback_opacity).child(if self.resource.loading_history&&!self.resource.assets.is_empty(){"正在加载更多资源…".into()}else if self.resource.history_error.is_some(){"读取资源失败".into()}else{self.status.clone()}))
                            .when(self.resource.history_error.is_none()&&!self.queue.read(cx).busy && self.job_history.read(cx).recovery_count>0,|d|d.child(Button::new("view-recovery-jobs").small().ghost().label(format!("待恢复 {}",self.job_history.read(cx).recovery_count)).on_click(cx.listener(|v,_,_,cx|{v.navigate_page(Page::Jobs,cx);}))))
                            .when(self.resource.history_error.is_none()&&self.resource.grid_view,|d|d.child(self.density_control()))
                            .when(self.resource.history_error.is_some(),|d|d.child(div().debug_selector(||"refresh-history".into()).child(Button::new("refresh-history").small().label("重试读取")
                                .tooltip(self.resource.history_error.clone().unwrap_or_default())
                                .on_click(cx.listener(|v,_,_,cx|{v.resource.history_error=None;if v.resource.assets.is_empty(){v.reload(cx);}else{v.load_more(cx);}})))))
                            .when(self.resource.history_error.is_none()&&(self.resource.has_more || self.resource.loading_history),|d|d.child(div().debug_selector(||"next-page".into()).child(Button::new("next-page").ghost().small().label(if self.resource.loading_history{"加载中…"}else{"加载更多"})
                                .disabled(self.resource.loading_history).on_click(cx.listener(|v,_,_,cx|v.load_more(cx))))))
                    )
                    .when(self.queue.read(cx).busy||self.upload_review.is_some()||self.queue.read(cx).queue_total>0,|d|d.child(self.upload_summary(cx)))
),
            ))
            .when(reserve_inspector, |d| d.child(div().id("inspector-slot").debug_selector(||"inspector-slot".into()).relative().w(px(detail_width)).flex_shrink_0().h_full().overflow_hidden().bg(self.palette().canvas)
                .child(div().absolute().top_0().bottom_0().w(px(detail_width)).left(px(22.*(1.-inspector_opacity))).opacity(inspector_opacity)
                    .when(!show_inspector,|d|d.occlude().capture_any_mouse_down(|_,_,cx|cx.stop_propagation()).capture_any_mouse_up(|_,_,cx|cx.stop_propagation()).capture_key_down(|_,_,cx|cx.stop_propagation()))
                    .child(self.inspector(narrow,show_inspector,cx)))
                .when(!narrow&&show_inspector,|d|d.child(div().id("inspector-resize").debug_selector(||"inspector-resize".into()).absolute().left_0().top_0().bottom_0().w(px(6.)).cursor(CursorStyle::ResizeLeftRight)
                    .hover(|s|s.bg(self.palette().accent_soft))
                    .on_mouse_down(MouseButton::Left,cx.listener(move|v,e:&MouseDownEvent,_,cx|{v.inspector_drag=Some((f32::from(e.position.x),right_width));cx.stop_propagation();}))))))

            .into_any_element()
    }
}
