use super::*;

impl UploadDesk {
    pub(super) fn reset_history(&mut self, cx: &mut Context<Self>) {
        self.page_cursors = vec![None];
        self.selected_asset = None;
        self.selected_ids.clear();
        self.large_preview_open = false;
        self.large_preview_path = None;
        self.thumbnail_path = None;
        self.cached_thumbnails.clear();
        self.thumbnails_task = None;
        self.preview_task = None;
        self.preview_loading = false;
        self.preview_error = None;
        self.assets.clear();
        self.has_more = false;
        self.reload(cx);
    }

    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        let storage = self.storage.clone();
        let query = self.search.clone();
        let category = self.category;
        let oldest = self.oldest;
        let scope = (!self.all_accounts).then(|| (
            self.config.provider().to_string(), self.config.owner_id().to_string()
        ));
        let before = self.page_cursors.last().copied().flatten();
        self.history_generation += 1;
        let generation = self.history_generation;
        self.loading_history = true;
        self.history_error = None;
        self.history_task = Some(cx.spawn(async move |this, cx| {
            if !query.is_empty() { cx.background_executor().timer(std::time::Duration::from_millis(100)).await; }
            let result = cx.background_spawn(async move {
                storage.page_category(PAGE_SIZE, &query, category,
                    scope.as_ref().map(|(provider, owner)| (provider.as_str(), owner.as_str())), before, oldest)
            }).await;
            _ = this.update(cx, |view, cx| {
                if generation != view.history_generation { return; }
                view.loading_history = false;
                match result {
                    Ok((items, more, total)) => {
                        view.selected_ids.retain(|id| items.iter().any(|a| a.id == *id));
                        view.assets = items;
                        view.has_more = more;
                        view.total_count = total;
                        view.refresh_thumbnails(cx);
                    }
                    Err(error) => {
                        view.assets.clear();
                        view.has_more = false;
                        view.history_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn refresh_thumbnails(&mut self, cx: &mut Context<Self>) {
        if self.cache_busy { return; }
        self.cached_thumbnails.clear();
        let generation = self.history_generation;
        let items = self.assets.clone();
        let Ok(path) = db_path() else { return; };
        let Some(parent) = path.parent() else { return; };
        let cache = parent.join("thumbnails");
        let generate_local = self.grid_view;
        self.thumbnails_task = Some(cx.spawn(async move |this, cx| {
            for asset in items.into_iter().filter(|a| a.category == "image") {
                let id = asset.id;
                let cache = cache.clone();
                let path = cx.background_spawn(async move {
                    let path = preview::cached_path(&cache, &asset);
                    if path.is_file() { Some(path) }
                    else if generate_local && asset.provider == "demo" {
                        preview::thumbnail(&cache, &asset).ok()
                    } else { None }
                }).await;
                let result = this.update(cx, |view, cx| {
                    if generation != view.history_generation { return false; }
                    if let Some(path) = path { view.cached_thumbnails.insert(id, path); cx.notify(); }
                    true
                });
                if !matches!(result, Ok(true)) { break; }
            }
        }));
    }

    pub(super) fn select_asset(&mut self, asset: Asset, window: &mut Window, cx: &mut Context<Self>) {
        let tags = self.storage.tags(asset.id).unwrap_or_default().join(", ");
        self.tag_input.update(cx, |input, cx| input.set_value(tags, window, cx));
        self.large_preview_open = false;
        self.large_preview_path = None;
        self.thumbnail_path = None;
        self.preview_task = None;
        self.preview_loading = false;
        self.preview_error = None;
        self.selected_asset = Some(asset.clone());
        self.account_panel = false;
        self.inspector_open = true;
        if asset.category == "image" && asset.provider == "demo" { self.load_preview(cx); }
        cx.notify();
    }

    pub(super) fn load_preview(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.selected_asset.clone() else { return; };
        if asset.category != "image" || self.preview_loading || self.cache_busy { return; }
        let Ok(path) = db_path() else { return; };
        let Some(parent) = path.parent() else { return; };
        let cache = parent.join("thumbnails");
        let id = asset.id;
        self.preview_loading = true;
        self.preview_error = None;
        self.preview_task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move {
                preview::thumbnail(&cache, &asset).map_err(|e| e.to_string())
            }).await;
            _ = this.update(cx, |view, cx| {
                if view.selected_asset.as_ref().is_none_or(|a| a.id != id) { return; }
                view.preview_loading = false;
                match result {
                    Ok(path) => {
                        view.cached_thumbnails.insert(id, path.clone());
                        view.thumbnail_path = Some(path);
                    },
                    Err(error) => view.preview_error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn open_large_preview(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.selected_asset.clone() else { return; };
        if asset.category != "image" || self.preview_loading || self.cache_busy { return; }
        let Ok(path) = db_path() else { return; };
        let Some(parent) = path.parent() else { return; };
        let cache = parent.join("thumbnails");
        let id = asset.id;
        self.large_preview_open = true;
        self.large_preview_path = None;
        self.preview_loading = true;
        self.preview_error = None;
        self.preview_task = Some(cx.spawn(async move |this,cx| {
            let result = cx.background_spawn(async move { preview::large_preview(&cache,&asset) }).await;
            _ = this.update(cx, |v,cx| {
                if v.selected_asset.as_ref().is_none_or(|a|a.id!=id) { return; }
                v.preview_loading = false;
                match result { Ok(path)=>v.large_preview_path=Some(path), Err(e)=>v.preview_error=Some(e.to_string()) }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn clear_image_cache(&mut self, cx: &mut Context<Self>) {
        if self.cache_busy || self.preview_loading { return; }
        let Ok(path) = db_path() else { return; };
        let Some(parent) = path.parent() else { return; };
        let cache = parent.join("thumbnails");
        self.cache_busy = true;
        self.thumbnails_task = None;
        self.preview_task = None;
        self.cached_thumbnails.clear();
        self.thumbnail_path = None;
        self.large_preview_open = false;
        self.large_preview_path = None;
        self.cache_task = Some(cx.spawn(async move |this,cx| {
            let result = cx.background_spawn(async move { preview::clear_cache(&cache) }).await;
            _ = this.update(cx, |v,cx| {
                v.cache_busy=false;
                v.status=match result {Ok(n)=>format!("已清理 {n} 项预览缓存，原文件未修改"),Err(e)=>e.to_string()};
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn copy_selected(&mut self, markdown: bool, cx: &mut Context<Self>) {
        let links = self.assets.iter().filter(|a| self.selected_ids.contains(&a.id))
            .map(|a| if markdown { format::markdown(&a.name, &a.url, a.category == "image") } else { a.url.clone() }).collect::<Vec<_>>();
        if links.is_empty() { return; }
        cx.write_to_clipboard(ClipboardItem::new_string(links.join("\n")));
        self.status = format!("已复制 {} 条 {}", links.len(), if markdown { "Markdown 引用" } else { "URL" });
        cx.notify();
    }

    pub(super) fn copy_asset(&mut self, asset: &Asset, as_markdown: bool, cx: &mut Context<Self>) {
        let content = if as_markdown {
            format::markdown(&asset.name, &asset.url, asset.category == "image")
        } else {
            asset.url.clone()
        };
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(content)) {
            Ok(_) => self.status = if as_markdown {
                "Markdown 引用已复制".to_string()
            } else {
                "资源链接已复制".to_string()
            },
            Err(error) => self.status = format!("写入剪贴板失败：{error}"),
        }
        cx.notify();
    }
}
