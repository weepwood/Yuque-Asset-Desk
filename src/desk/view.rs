use super::*;
use gpui_kit::component::IconName;

impl UploadDesk {
    pub(super) fn color(&self, dark: u32, light: u32) -> Rgba {
        rgb(if self.dark { dark } else { light })
    }

    pub(super) fn status_panel(&self) -> impl IntoElement {
        div().flex().flex_col().gap_2().rounded_lg().p_3()
            .bg(self.color(0x25272a, 0xe8eaed))
            .child("当前上传目标")
            .child(format!("{} / {}", self.config.provider(), self.config.owner_id()))
            .child(format!("待处理：{}", self.queue_count))
    }

    fn task_center(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().id("jobs-list").flex_1().min_h(px(0.)).flex().flex_col().gap_2();
        for job in &self.jobs {
            let id = job.id;
            let active = self.active_job == Some(id);
            let state = if active { "处理中" } else { match job.state.as_str() {
                "done" => "完成", "failed" => "失败", "pending" => "待处理",
                "paused" => "暂停", "cancelled" => "取消", "running" => "中断／待核实", _ => "未知",
            }};
            let retry = matches!(job.state.as_str(), "pending" | "paused" | "failed" | "running");
            let total = self.transfer.total.load(Ordering::Relaxed);
            let bytes = self.transfer.bytes.load(Ordering::Relaxed).min(total);
            let mut row = div().flex().flex_col().gap_2().p_3().rounded_lg()
                .border_1().border_color(self.color(0x34363a,0xdfe2e6))
                .bg(self.color(0x242629,0xffffff))
                .child(div().flex().items_center().gap_2()
                    .child(div().flex_1().min_w(px(0.)).font_semibold().truncate().child(job.path.file_name().unwrap_or_default().to_string_lossy().into_owned()))
                    .child(div().text_sm().child(state))
                    .when(retry, |d| d.child(Button::new(("retry-job",id as usize))
                        .label("重试（先核实远端）").disabled(self.busy || self.account_busy)
                        .on_click(cx.listener(move |view, _, _, cx| view.retry_one(id,cx))))));
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
                .child(Button::new("resume-jobs").label("恢复任务").disabled(self.busy || self.account_busy || self.recovery_count == 0)
                    .on_click(cx.listener(|v,_,_,cx| v.resume_jobs(cx)))))
            .child(div().text_sm().child(format!("当前账号：{} / {} · 未完成 {}",self.config.provider(),self.config.owner_id(),self.recovery_count)))
            .when(self.busy, |d| d.child(div().flex().gap_2()
                .child(Button::new("jobs-pause").label("文件结束后暂停").on_click(cx.listener(|v,_,_,cx| { let _ = v.queue_control.compare_exchange(0,1,Ordering::Relaxed,Ordering::Relaxed); cx.notify(); })))
                .child(Button::new("jobs-cancel").label("取消剩余").on_click(cx.listener(|v,_,_,cx| { v.queue_control.store(2,Ordering::Relaxed); cx.notify(); })))))
            .when(self.jobs_loading, |d| d.child("正在读取任务…"))
            .when(self.jobs.is_empty() && !self.jobs_loading && self.jobs_error.is_none(), |d| d.child("暂无任务记录"))
            .when(self.jobs_error.is_some(), |d| d.child(self.jobs_error.clone().unwrap_or_default()))
            .child(list.overflow_y_scrollbar())
            .child(div().flex().items_center().gap_2()
                .child(Button::new("jobs-prev").label("上一页").disabled(self.jobs_loading || self.job_cursors.len() <= 1)
                    .on_click(cx.listener(|v,_,_,cx| { if v.job_cursors.len()>1 {v.job_cursors.pop(); v.jobs.clear(); v.jobs_more=false; v.refresh_jobs(cx); cx.notify();} })))
                .child(format!("第 {} 页", self.job_cursors.len()))
                .child(Button::new("jobs-next").label("下一页").disabled(self.jobs_loading || !self.jobs_more)
                    .on_click(cx.listener(|v,_,_,cx| { if v.jobs_more { if let Some(last)=v.jobs.last() {v.job_cursors.push(Some(last.id)); v.jobs.clear(); v.jobs_more=false; v.refresh_jobs(cx); cx.notify();} } }))))
            .child(div().text_sm().child(self.status.clone()))
    }

    pub(super) fn inspector(&self, narrow: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div().id("inspector").h_full().min_h(px(0.)).flex().flex_col().gap_3().p_4().when(!narrow, |d| d.w(px(260.)).flex_shrink_0()).when(narrow, |d| d.w_full())
            .bg(self.color(0x1c1e21, 0xf7f8fa))
            .border_l_1().border_color(self.color(0x34363a, 0xdfe2e6));
        if self.account_panel {
            panel = panel.child(div().font_semibold().child("账号管理"))
                .child(div().text_sm().child("仅保存到系统密钥库。凭据格式有效不代表已登录。"))
                .child(div().text_sm().child(self.session_status.clone()))
                .when(self.config.mode == Mode::Yuque && self.config.creds.is_some(), |d| d.child(
                    Button::new("check-session").label("检查会话（只读）").disabled(self.account_busy || self.busy)
                        .on_click(cx.listener(|view,_,_,cx| view.check_session(cx)))))
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
                panel = panel.child(Button::new("large-preview").label("查看大图")
                    .disabled(self.preview_loading || self.cache_busy)
                    .on_click(cx.listener(|v,_,_,cx| v.open_large_preview(cx))));
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
        panel.overflow_y_scrollbar()
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
        let narrow = window.viewport_size().width < px(1000.);
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
                    .when(!narrow, |d| d.child(
                        Button::new(("url", index))
                            .label("复制 URL")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.copy_asset(&url_asset, false, cx);
                            })),
                    ))
                    .when(!narrow, |d| d.child(
                        Button::new(("markdown", index))
                            .label("Markdown")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.copy_asset(&markdown_asset, true, cx);
                            })),
                    ))
                    .into_any_element()
            })
            .collect();

        // Never access SQLite from the hot render path.
        let total = self.total_count;
        let is_demo = self.config.mode == Mode::Demo;
        if self.large_preview_open {
            return div().size_full().flex().flex_col().p_4().gap_3()
                .bg(self.color(0x17191b,0xf3f4f6)).text_color(self.color(0xebedf0,0x24272c))
                .child(div().flex().items_center().gap_2()
                    .child(Button::new("close-preview").label("返回资源").on_click(cx.listener(|v,_,_,cx| {v.large_preview_open=false; cx.notify();})))
                    .child(div().font_semibold().child(self.selected_asset.as_ref().map(|a|a.name.clone()).unwrap_or_default())))
                .child({
                    let panel=div().flex_1().min_h(px(0.)).flex().items_center().justify_center();
                    if let Some(path)=&self.large_preview_path { panel.child(img(path.clone()).size_full().object_fit(ObjectFit::Contain)) }
                    else { panel.child(self.preview_error.clone().unwrap_or_else(|| "正在加载大图…".into())) }
                }).into_any_element();
        }
        let show_inspector = self.inspector_open && (!narrow || self.account_panel || self.selected_asset.is_some());
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
                    .gap_2()
                    .p_3()
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
                            .selected(self.category.is_empty() && !self.jobs_page).icon(IconName::FolderOpen)
                            .label("上传工作台")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.jobs_page = false;
                                view.category = ""; view.account_panel=false; view.inspector_open=false;
                                view.reset_history(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("images-navigation")
                            .selected(self.category == "image" && !self.jobs_page).icon(IconName::File)
                            .label("图片资源")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.jobs_page = false;
                                view.category = "image"; view.account_panel=false; view.inspector_open=false;
                                view.reset_history(cx);
                                cx.notify();
                            })),
                    )
                    .child(Button::new("video-nav").label("视频").icon(IconName::File).selected(self.category=="video" && !self.jobs_page)
                        .on_click(cx.listener(|v,_,_,cx| {v.jobs_page=false; v.category="video"; v.account_panel=false; v.inspector_open=false; v.reset_history(cx); })))
                    .child(Button::new("attachment-nav").label("附件").icon(IconName::File).selected(self.category=="attachment" && !self.jobs_page)
                        .on_click(cx.listener(|v,_,_,cx| {v.jobs_page=false; v.category="attachment"; v.account_panel=false; v.inspector_open=false; v.reset_history(cx); })))
                    .child(
                        Button::new("choose-sidebar")
                            .disabled(self.busy || self.account_busy)
                            .label("选择本地文件")
                            .on_click(cx.listener(|view, _, _, cx| view.select_files(cx))),
                    )
                    .child(Button::new("jobs-nav").selected(self.jobs_page).label("上传任务")
                        .on_click(cx.listener(|view,_,_,cx| {view.jobs_page=true; view.account_panel=false; view.inspector_open=false; view.refresh_jobs(cx); cx.notify();})))
                    .child(Button::new("account-nav").icon(IconName::Settings).selected(self.account_panel).label("账号管理")
                        .on_click(cx.listener(|view, _, _, cx| { view.account_panel = true; view.inspector_open = true; cx.notify(); })))
                    .child(Button::new("inspector-toggle").icon(IconName::PanelRight).label(if self.inspector_open {"隐藏详情"} else {"显示详情"})
                        .on_click(cx.listener(|v,_,_,cx| {v.inspector_open = !v.inspector_open; cx.notify();})))
                    .child(Button::new("cache-clear").label(if self.cache_busy {"清理中…"} else {"清理预览缓存"})
                        .disabled(self.cache_busy || self.preview_loading)
                        .on_click(cx.listener(|v,_,_,cx| v.clear_image_cache(cx))))
                    .child(Button::new("theme-toggle").icon(if self.dark {IconName::Sun} else {IconName::Moon}).label(if self.dark { "浅色主题" } else { "深色主题" })
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.dark = !view.dark;
                            Theme::change(if view.dark { ThemeMode::Dark } else { ThemeMode::Light }, Some(window), cx);
                            cx.notify();
                        })))
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
                            .child(match self.category {"image"=>"图片资源","video"=>"视频资源","attachment"=>"附件资源",_=>"全部资源"})
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
                                    .label(if self.category.is_empty() { "仅看图片" } else { "显示全部" })
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.category = if view.category.is_empty() { "image" } else { "" };
                                        view.reset_history(cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(Input::new(&self.search_input))
                    .child(div().flex().flex_wrap().gap_2().items_center()
                        .child(Button::new("sort-time").label(if self.oldest { "最早优先" } else { "最新优先" })
                            .on_click(cx.listener(|view, _, _, cx| { view.oldest = !view.oldest; view.reset_history(cx); })))
                        .child(Button::new("select-page").label("选择本页")
                            .on_click(cx.listener(|view, _, _, cx| { view.selected_ids = view.assets.iter().map(|a| a.id).collect(); cx.notify(); })))
                        .child(Button::new("batch-copy").label(format!("复制已选 ({})", self.selected_ids.len()))
                            .disabled(self.selected_ids.is_empty())
                            .on_click(cx.listener(|view, _, _, cx| view.copy_selected(true,cx))))
                        .child(Button::new("batch-url").label("复制 URL").disabled(self.selected_ids.is_empty())
                            .on_click(cx.listener(|v,_,_,cx| v.copy_selected(false,cx))))
                        .child(Button::new("clear-selection").label("取消选择").disabled(self.selected_ids.is_empty())
                            .on_click(cx.listener(|v,_,_,cx| {v.selected_ids.clear();cx.notify();}))))
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
                                container.child(if self.search.is_empty() && self.category.is_empty() {
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
                    .child(
                        div()
                            .p_3()
                            .rounded_lg()
                            .bg(self.color(0x242629, 0xffffff))
                            .opacity(feedback_opacity)
                            .child(self.status.clone()),
                    ),
            ))
            .when(show_inspector, |d| d.child(div().when(narrow, |d| d.flex_1()).child(self.inspector(narrow,cx))))
            .into_any_element()
    }
}
