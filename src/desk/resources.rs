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
        self.resource.ratios.clear();
        self.resource.hover_asset=None;
        self.resource.album_list.reset(0);
        self.resource.masonry_scroll.set_offset(point(px(0.),px(0.)));
        self.resource.masonry_plan=Default::default();
        self.resource.masonry_signature=0;
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
        let favorites_only=self.resource.favorites_only;
        let similar=self.resource.similar_to.clone();
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
                let page=if let Some(asset)=similar {
                    let path=preview::restore_local(&storage,&asset)?;
                    let (hash,_)=preview::visual_hash(&path)?;
                    storage.store_visual_hash(asset.id,hash)?;
                    let mut cursor=None;
                    loop {
                        let (page,more,_)=storage.page_album(150,"","image",Some((&asset.provider,&asset.owner_id)),cursor,false,None,false)?;
                        for candidate in &page {
                            if storage.visual_hash(candidate.id)?.is_none(){
                                let path=preview::cached_path(storage.data_dir(),candidate);
                                if path.is_file(){if let Ok((hash,_))=preview::visual_hash(&path){storage.store_visual_hash(candidate.id,hash)?;}}
                            }
                        }
                        if !more {break;}
                        cursor=page.last().map(|a|a.id);
                        if cursor.is_none(){break;}
                    }
                    let items=storage.similar_assets(&asset,hash)?;
                    let total=items.len() as i64;
                    (items,false,total)
                }else{storage.page_album(PAGE_SIZE,&query,category,scope,before,oldest,tag.as_deref(),favorites_only)?};
                let tags=storage.tag_catalog(scope)?;
                let favorites=storage.favorite_ids(scope)?;
                Ok::<_,anyhow::Error>((page,tags,favorites))
            }).await;
            _ = this.update(cx, |view, cx| {
                if generation != view.resource.history_generation { return; }
                view.resource.loading_history = false;
                match result {
                    Ok(((items, more, total), tags,favorites)) => {
                        view.resource.favorites=favorites;
                        view.resource.tag_catalog=tags;
                        view.resource.selected_ids.retain(|id| items.iter().any(|a| a.id == *id));
                        view.resource.assets = items;
                        #[cfg(feature="qa")]
                        if std::env::var_os("YUQUE_QA_PERF").is_some(){eprintln!("PERF page_ready_ms={} count={}",view.resource.perf_start.elapsed().as_millis(),view.resource.assets.len());}
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

    pub(super) fn refresh_thumbnails(&mut self,cx:&mut Context<Self>){
        let generation=self.resource.history_generation;
        let items=self.resource.assets.iter().filter(|a|a.category=="image").cloned().collect::<Vec<_>>();
        let ids=items.iter().map(|a|a.id).collect::<std::collections::HashSet<_>>();
        self.resource.cached_thumbnails.retain(|id,_|ids.contains(id));
        self.resource.ratios.retain(|id,_|ids.contains(id));
        let storage=self.storage.clone();
        self.resource.thumbnails_task=Some(cx.spawn(async move|this,cx|{
            for batch in items.chunks(12){
                let batch=batch.to_vec();let storage=storage.clone();
                let found=cx.background_spawn(async move{
                    batch.into_iter().filter_map(|a|preview::index_local(&storage,&a).ok().map(|(p,r)|(a.id,p,r))).collect::<Vec<_>>()
                }).await;
                let result=this.update(cx,|v,cx|{
                    if generation!=v.resource.history_generation{return false;}
                    for (id,p,r) in found{
                        v.resource.cached_thumbnails.insert(id,p);
                        if v.resource.ratios.insert(id,r)!=Some(r){v.resource.ratio_revision+=1;}
                    }
                    cx.notify();true
                });
                if !matches!(result,Ok(true)){break;}
            }
            #[cfg(feature="qa")]
            if std::env::var_os("YUQUE_QA_PERF").is_some(){
                _=this.update(cx,|v,_|eprintln!("PERF previews_ready_ms={} count={}",v.resource.perf_start.elapsed().as_millis(),v.resource.cached_thumbnails.len()));
            }
        }));
    }

    pub(super) fn export_album(&mut self,cx:&mut Context<Self>){
        if self.backup_busy || self.queue.busy || self.account.account_busy{return;}
        let Some(parent)=rfd::FileDialog::new().set_title("选择相册备份位置").pick_folder() else{return;};
        self.backup_busy=true;
        let storage=self.storage.clone();
        self.status="正在备份链接数据库与本地微缩图…".into();
        self.backup_task=Some(cx.spawn(async move |this,cx|{
            let result=cx.background_spawn(async move{storage.export_album(&parent)}).await;
            _=this.update(cx,|v,cx|{
                v.backup_busy=false;
                v.status=match result{Ok(path)=>format!("相册备份成功：{}",path.display()),Err(e)=>format!("备份失败：{e}")};
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn toggle_favorite(&mut self,asset:&Asset,cx:&mut Context<Self>) {
        match self.storage.toggle_favorite(asset) {
            Ok(true)=>{self.resource.favorites.insert(asset.id);self.status="已收藏".into();}
            Ok(false)=>{self.resource.favorites.remove(&asset.id);self.status="已取消收藏".into();}
            Err(e)=>{self.status=e.to_string();}
        }
        if self.resource.favorites_only {self.reload(cx);}
        cx.notify();
    }

    pub(super) fn find_similar(&mut self,asset:Asset,cx:&mut Context<Self>) {
        self.resource.similar_to=Some(asset);
        self.resource.favorites_only=false;
        self.resource.group_dates=false;
        self.inspector_open=false;
        self.reset_history(cx);
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
