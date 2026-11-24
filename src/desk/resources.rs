use super::*;

impl UploadDesk {
    pub(super) fn reset_history(&mut self, cx: &mut Context<Self>) {
        self.resource.page_cursors = vec![None];
        self.resource.selected_asset = None;
        self.resource.selected_ids.clear();
        self.resource.large_preview_open = false;
        self.resource.large_preview_path = None;
        self.resource.thumbnail_path = None;
        self.resource.cached_thumbnails.clear();
        self.resource.thumbnails_task = None;
        self.resource.preview_task = None;
        self.resource.preview_loading = false;
        self.resource.preview_error = None;
        self.resource.assets.clear();
        self.resource.has_more = false;
        self.reload(cx);
    }

    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        let storage = self.storage.clone();
        let query = self.resource.search.clone();
        let category = self.resource.category;
        let oldest = self.resource.oldest;
        let tag = self.resource.tag_filter.clone();
        let scope = (!self.resource.all_accounts).then(|| (
            self.account.config.provider().to_string(), self.account.config.owner_id().to_string()
        ));
        let before = self.resource.page_cursors.last().copied().flatten();
        self.resource.history_generation += 1;
        let generation = self.resource.history_generation;
        self.resource.loading_history = true;
        self.resource.history_error = None;
        self.resource.history_task = Some(cx.spawn(async move |this, cx| {
            if !query.is_empty() { cx.background_executor().timer(std::time::Duration::from_millis(100)).await; }
            let result = cx.background_spawn(async move {
                let scope=scope.as_ref().map(|(provider,owner)|(provider.as_str(),owner.as_str()));
                let page=storage.page_tagged(PAGE_SIZE,&query,category,scope,before,oldest,tag.as_deref())?;
                let tags=storage.tag_catalog(scope)?;
                Ok::<_,anyhow::Error>((page,tags))
            }).await;
            _ = this.update(cx, |view, cx| {
                if generation != view.resource.history_generation { return; }
                view.resource.loading_history = false;
                match result {
                    Ok(((items, more, total), tags)) => {
                        view.resource.tag_catalog=tags;
                        view.resource.selected_ids.retain(|id| items.iter().any(|a| a.id == *id));
                        view.resource.assets = items;
                        view.resource.has_more = more;
                        view.resource.total_count = total;
                        view.refresh_thumbnails(cx);
                    }
                    Err(error) => {
                        view.resource.assets.clear();
                        view.resource.has_more = false;
                        view.resource.history_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn refresh_thumbnails(&mut self, cx: &mut Context<Self>) {
        self.resource.cached_thumbnails.clear();
        let generation = self.resource.history_generation;
        let items = self.resource.assets.clone();
        let storage = self.storage.clone();
        self.resource.thumbnails_task = Some(cx.spawn(async move |this, cx| {
            for asset in items.into_iter().filter(|a| a.category == "image") {
                let id = asset.id;
                let storage = storage.clone();
                let path = cx.background_spawn(async move {
                    preview::restore_local(&storage,&asset).ok()
                }).await;
                let result = this.update(cx, |view, cx| {
                    if generation != view.resource.history_generation { return false; }
                    if let Some(path) = path { view.resource.cached_thumbnails.insert(id, path); cx.notify(); }
                    true
                });
                if !matches!(result, Ok(true)) { break; }
            }
        }));
    }

    pub(super) fn select_asset(&mut self, asset: Asset, window: &mut Window, cx: &mut Context<Self>) {
        let tags = self.storage.tags(asset.id).unwrap_or_default().join(", ");
        self.resource.tag_input.update(cx, |input, cx| input.set_value(tags, window, cx));
        self.resource.large_preview_open = false;
        self.resource.large_preview_path = None;
        self.resource.thumbnail_path = None;
        self.resource.preview_task = None;
        self.resource.preview_loading = false;
        self.resource.preview_error = None;
        self.resource.selected_asset = Some(asset.clone());
        self.account_panel = false;
        self.inspector_open = true;
        if asset.category == "image" { self.load_preview(cx); }
        cx.notify();
    }

    pub(super) fn load_preview(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.resource.selected_asset.clone() else { return; };
        if asset.category != "image" || self.resource.preview_loading { return; }
        let storage = self.storage.clone();
        let id = asset.id;
        self.resource.preview_loading = true;
        self.resource.preview_error = None;
        self.resource.preview_task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move {
                preview::restore_local(&storage,&asset).map_err(|e| e.to_string())
            }).await;
            _ = this.update(cx, |view, cx| {
                if view.resource.selected_asset.as_ref().is_none_or(|a| a.id != id) { return; }
                view.resource.preview_loading = false;
                match result {
                    Ok(path) => {
                        view.resource.cached_thumbnails.insert(id, path.clone());
                        view.resource.thumbnail_path = Some(path);
                    },
                    Err(error) => view.resource.preview_error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn open_large_preview(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.resource.selected_asset.clone() else { return; };
        if asset.category != "image" || self.resource.preview_loading { return; }
        let storage = self.storage.clone();
        let id = asset.id;
        self.resource.large_preview_open = true;
        self.resource.zoom=1.; self.resource.pan=point(px(0.),px(0.)); self.resource.drag_at=None;
        self.resource.large_preview_path = None;
        self.resource.preview_loading = true;
        self.resource.preview_error = None;
        self.resource.preview_task = Some(cx.spawn(async move |this,cx| {
            let result = cx.background_spawn(async move { preview::restore_local(&storage,&asset) }).await;
            _ = this.update(cx, |v,cx| {
                if v.resource.selected_asset.as_ref().is_none_or(|a|a.id!=id) { return; }
                v.resource.preview_loading = false;
                match result { Ok(path)=>v.resource.large_preview_path=Some(path), Err(e)=>v.resource.preview_error=Some(e.to_string()) }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn apply_batch_tags(&mut self, remove:bool,cx:&mut Context<Self>) {
        let ids=self.resource.assets.iter().filter(|a|self.resource.selected_ids.contains(&a.id)).map(|a|a.id).collect::<Vec<_>>();
        let text=self.resource.batch_tag_input.read(cx).value().to_string();
        let scope=(!self.resource.all_accounts).then(||(self.account.config.provider(),self.account.config.owner_id()));
        self.status=match self.storage.batch_tags(&ids,scope,&text,remove) {
            Ok(())=>format!("已更新 {} 项资源标签",ids.len()), Err(e)=>e.to_string()
        };
        self.reload(cx);
        cx.notify();
    }

    pub(super) fn zoom_by(&mut self, delta:f32,cx:&mut Context<Self>) {
        self.resource.zoom=(self.resource.zoom+delta).clamp(1.,4.);
        if self.resource.zoom==1. {self.resource.pan=point(px(0.),px(0.));}
        cx.notify();
    }

    pub(super) fn open_selected_resource(&mut self,cx:&mut Context<Self>) {
        let Some(asset)=self.resource.selected_asset.as_ref() else{return;};
        let Ok(url)=url::Url::parse(&asset.url) else {self.status="资源链接无效".into();cx.notify();return;};
        if asset.provider=="demo" && url.scheme()=="file" {
            if let Ok(path)=url.to_file_path() {
                if path.is_file(){cx.open_with_system(&path);return;}
            }
        } else if url.scheme()=="https" && url.username().is_empty() && url.password().is_none() {
            cx.open_url(url.as_str());return;
        }
        self.status="资源不存在或链接不受支持".into();cx.notify();
    }

    pub(super) fn copy_selected(&mut self, markdown: bool, cx: &mut Context<Self>) {
        let links = self.resource.assets.iter().filter(|a| self.resource.selected_ids.contains(&a.id))
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
