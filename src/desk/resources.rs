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
        self.resource.preview_attempted.clear();
        self.resource.preview_loading_batch=false;
        self.resource.visible_range=(0,48);
        self.resource.ratios.clear();
        self.resource.hover_asset=None;
        self.resource.album_list.reset(0);
        self.resource.row_plan=Default::default();
        self.resource.layout_signature=0;
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
        self.resource.page_cursors=vec![None];
        self.resource.visible_range=(0,48);
        self.resource.album_list.reset(0);
        self.resource.row_plan=Default::default();
        self.resource.layout_signature=0;
        self.resource.masonry_scroll.set_offset(point(px(0.),px(0.)));
        self.resource.masonry_plan=Default::default();
        self.resource.preview_attempted.clear();
        self.resource.preview_loading_batch=false;
        self.resource.thumbnails_task=None;
        self.load_history(false,cx);
    }
    pub(super) fn load_more(&mut self,cx:&mut Context<Self>){
        if self.resource.loading_history || !self.resource.has_more || self.resource.history_error.is_some() || self.resource.similar_to.is_some(){return;}
        if let Some(last)=self.resource.assets.last(){self.resource.page_cursors.push(Some(last.id));self.load_history(true,cx);}
    }
    fn load_history(&mut self,append:bool,cx:&mut Context<Self>){
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
        self.resource.thumbnails_task=None;self.resource.preview_loading_batch=false;
        self.resource.preview_attempted.retain(|id|self.resource.cached_thumbnails.contains_key(id));
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
                let ratios=page.0.iter().filter(|a|a.category=="image").filter_map(|a|{
                    let path=preview::cached_path(storage.data_dir(),a);
                    image::image_dimensions(&path).ok().map(|(w,h)|(a.id,w as f32/h.max(1) as f32,path))
                }).collect::<Vec<_>>();
                let tags=storage.tag_catalog(scope)?;
                let favorites=storage.favorite_ids(scope)?;
                Ok::<_,anyhow::Error>((page,tags,favorites,ratios))
            }).await;
            _ = this.update(cx, |view, cx| {
                if generation != view.resource.history_generation { return; }
                view.resource.loading_history = false;
                match result {
                    Ok(((items, more, total), tags,favorites,ratios)) => {
                        view.resource.favorites=favorites;
                        view.resource.tag_catalog=tags;
                        if append {
                            let existing=view.resource.assets.iter().map(|a|a.id).collect::<std::collections::HashSet<_>>();
                            view.resource.assets.extend(items.into_iter().filter(|a|!existing.contains(&a.id)));
                        } else {
                            view.resource.selected_ids.retain(|id| items.iter().any(|a| a.id == *id));
                            view.resource.assets=items;
                            view.resource.cached_thumbnails.clear();view.resource.ratios.clear();
                        }
                        for (id,ratio,path) in ratios{
                            view.resource.ratios.insert(id,ratio);view.resource.cached_thumbnails.insert(id,path);
                        }
                        #[cfg(feature="qa")]
                        if view.isolated_qa() && std::env::var("YUQUE_QA_PAGE").as_deref()==Ok("hover"){
                            view.resource.hover_asset=view.resource.assets.first().map(|a|a.id);
                        }
                        view.resource.ratio_revision+=1;
                        view.resource.preview_loading_batch=false;view.resource.thumbnails_task=None;
                        #[cfg(feature="qa")]
                        if std::env::var_os("YUQUE_QA_PERF").is_some(){eprintln!("PERF page_ready_ms={} count={}",view.resource.perf_start.elapsed().as_millis(),view.resource.assets.len());}
                        view.resource.has_more = more;
                        view.resource.total_count = total;
                        view.refresh_thumbnails(cx);
                    }
                    Err(error) => {
                        if append{view.resource.page_cursors.pop();}else{view.resource.assets.clear();view.resource.has_more=false;}
                        view.resource.history_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn update_visible(&mut self,start:usize,end:usize,cx:&mut Context<Self>){
        self.resource.visible_range=(start,end);
        if end.saturating_add(24)>=self.resource.assets.len(){self.load_more(cx);}
        self.refresh_thumbnails(cx);
    }
    pub(super) fn refresh_thumbnails(&mut self,cx:&mut Context<Self>){
        if self.resource.preview_loading_batch || self.resource.loading_history{return;}
        let generation=self.resource.history_generation;
        let (start,end)=self.resource.visible_range;
        let lo=start.saturating_sub(24).min(self.resource.assets.len());
        let hi=end.saturating_add(48).min(self.resource.assets.len());
        let mid=start.min(self.resource.assets.len());
        let items=self.resource.assets[mid..hi].iter().chain(self.resource.assets[lo..mid].iter()).filter(|a|a.category=="image"&&!self.resource.preview_attempted.contains(&a.id)&&!self.resource.cached_thumbnails.contains_key(&a.id)).take(12).cloned().collect::<Vec<_>>();
        if items.is_empty(){
            #[cfg(feature="qa")]
            if std::env::var_os("YUQUE_QA_PERF").is_some() && self.resource.perf_preview_count!=self.resource.cached_thumbnails.len(){
                self.resource.perf_preview_count=self.resource.cached_thumbnails.len();
                eprintln!("PERF previews_ready_ms={} count={}",self.resource.perf_start.elapsed().as_millis(),self.resource.cached_thumbnails.len());
            }
            return;
        }
        self.resource.preview_loading_batch=true;
        for a in &items{self.resource.preview_attempted.insert(a.id);}
        let storage=self.storage.clone();
        self.resource.thumbnails_task=Some(cx.spawn(async move|this,cx|{
            let found=cx.background_spawn(async move{items.into_iter().filter_map(|a|preview::index_local(&storage,&a).ok().map(|(p,r)|(a.id,p,r))).collect::<Vec<_>>()}).await;
            _=this.update(cx,|v,cx|{
                if generation!=v.resource.history_generation{return;}
                v.resource.preview_loading_batch=false;
                for (id,p,r) in found{v.resource.cached_thumbnails.insert(id,p);if v.resource.ratios.insert(id,r)!=Some(r){v.resource.ratio_revision+=1;}}
                v.refresh_thumbnails(cx);cx.notify();
            });
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
