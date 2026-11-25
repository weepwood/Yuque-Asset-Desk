use super::*;

impl UploadDesk {
    pub(super) fn clear_resource_selection(&mut self,cx:&mut Context<Self>){
        self.clear_quality(cx);
        self.resource.selected_ids.clear();
        self.resource.selected_asset=None;
        self.resource.selection_anchor=None;
        self.resource.batch_tag_open=false;
        self.resource.viewer_return=None;
        self.resource.pending_preview=None;
        self.resource.large_preview_open=false;
        self.resource.large_preview_path=None;
        self.resource.thumbnail_path=None;
        self.resource.preview_task=None;
        self.resource.preview_loading=false;
        self.resource.preview_error=None;
        self.inspector_model.update(cx,|m,cx|m.reset(cx));
    }

    pub(super) fn reset_history(&mut self, cx: &mut Context<Self>) {
        self.clear_resource_selection(cx);
        self.resource.preview_reload_pending=false;
        self.resource.page_cursors = vec![None];
        self.resource.cached_thumbnails=Default::default();
        self.resource.visible_range=(0,48);
        self.resource.ratios=Default::default();
        self.resource.hover_asset=None;
        self.resource.album_list.set_scroll_handler(|_,_,_|{});
        self.resource.album_list.reset(0);
        self.resource.row_plan=Default::default();
        self.resource.layout_signature=0;
        self.resource.masonry_scroll.set_offset(point(px(0.),px(0.)));
        self.resource.masonry_plan=Default::default();
        self.resource.masonry_signature=0;
        self.resource.preview_task = None;
        self.resource.preview_loading = false;
        self.resource.preview_error = None;
        self.resource.assets=Vec::new();
        self.resource.total_count=0;
        self.resource.has_more = false;
        self.reload(cx);
    }

    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        // Upload completion can refresh the album while the user is deep in a
        // preview. Keep the current cursor chain until they return.
        if self.resource.large_preview_open{
            self.resource.preview_reload_pending=true;
            cx.notify();return;
        }
        self.resource.preview_reload_pending=false;
        self.resource.pending_preview=None;
        self.resource.page_cursors=vec![None];
        self.resource.visible_range=(0,48);
        self.resource.album_list.set_scroll_handler(|_,_,_|{});
        self.resource.album_list.reset(0);
        self.resource.row_plan=Default::default();
        self.resource.layout_signature=0;
        self.resource.masonry_scroll.set_offset(point(px(0.),px(0.)));
        self.resource.masonry_plan=Default::default();
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
            self.config.provider().to_string(), self.config.owner_id().to_string()
        ));
        let before = self.resource.page_cursors.last().copied().flatten();
        if !append{self.thumbnails.update(cx,|v,_|v.stop(true));}
        self.resource.loading_history=true;self.resource.history_error=None;
        let request=gallery_query::GalleryRequest{query,category,oldest,tag,favorites_only,similar,scope,before,append};
        self.resource.history_generation=self.gallery_query.update(cx,|v,cx|v.request(storage,request,cx));
        cx.notify();
    }
    pub(super) fn apply_gallery_result(&mut self,event:&gallery_query::GalleryResult,cx:&mut Context<Self>){
                if event.generation != self.resource.history_generation { return; }
                self.resource.loading_history = false;
                match event.result.clone() {
                    Ok(((items, more, total), tags,favorites,ratios)) => {
                        self.resource.favorites=favorites;
                        self.resource.tag_catalog=tags;
                        if event.append {
                            let existing=self.resource.assets.iter().map(|a|a.id).collect::<std::collections::HashSet<_>>();
                            if ratios.iter().any(|(id,r,_)|existing.contains(id)&&self.resource.ratios.get(id)!=Some(r)){
                                self.resource.masonry_signature=0;
                                self.resource.ratio_revision=self.resource.ratio_revision.wrapping_add(1);
                            }
                            self.resource.assets.extend(items.into_iter().filter(|a|!existing.contains(&a.id)));
                        } else {
                            self.resource.selected_ids.retain(|id| items.iter().any(|a| a.id == *id));
                            if self.resource.selected_asset.as_ref().is_some_and(|a|!self.resource.selected_ids.contains(&a.id)){
                                // A refresh can remove the primary item; do not keep a
                                // disconnected selection or expose actions for an old asset.
                                self.clear_resource_selection(cx);
                            }else{
                                if self.resource.selection_anchor.is_some_and(|id|!items.iter().any(|a|a.id==id)){
                                    self.resource.selection_anchor=self.resource.selected_asset.as_ref().map(|a|a.id);
                                }
                                if self.resource.selected_ids.is_empty(){self.resource.batch_tag_open=false;}
                            }
                            self.resource.assets=items;
                            self.resource.layout_signature=0;
                            self.resource.masonry_signature=0;
                            self.resource.cached_thumbnails=Default::default();self.resource.ratios=Default::default();
                        }
                        for (id,ratio,path) in ratios{
                            self.resource.ratios.insert(id,ratio);self.resource.cached_thumbnails.insert(id,path);
                        }
                        #[cfg(feature="qa")]
                        if self.isolated_qa() && std::env::var("YUQUE_QA_PAGE").as_deref()==Ok("hover"){
                            self.resource.hover_asset=self.resource.assets.first().map(|a|a.id);
                        }
                        #[cfg(feature="qa")]
                        if std::env::var_os("YUQUE_QA_PERF").is_some(){eprintln!("PERF page_ready_ms={} count={}",self.resource.perf_start.elapsed().as_millis(),self.resource.assets.len());}
                        self.resource.has_more = more;
                        self.resource.total_count = total;
                        self.refresh_thumbnails(cx);
                    }
                    Err(error) => {
                        if event.append{self.resource.page_cursors.pop();}else{
                            self.clear_resource_selection(cx);
                            self.resource.layout_signature=0;
                            self.resource.masonry_signature=0;
                            self.resource.assets=Vec::new();self.resource.total_count=0;self.resource.has_more=false;
                            self.resource.cached_thumbnails=Default::default();self.resource.ratios=Default::default();
                        }
                        self.resource.history_error = Some(error.to_string());
                        if event.append{self.refresh_thumbnails(cx);}
                    }
                }
                cx.notify();
    }

    pub(super) fn update_visible(&mut self,start:usize,end:usize,cx:&mut Context<Self>){
        if self.page!=Page::Gallery{return;}
        self.resource.visible_range=(start,end);
        if end.saturating_add(24)>=self.resource.assets.len(){self.load_more(cx);}
        self.refresh_thumbnails(cx);
    }
    pub(super) fn refresh_thumbnails(&mut self,cx:&mut Context<Self>){
        if self.page!=Page::Gallery||self.thumbnails.read(cx).busy||self.resource.loading_history{return;}
        let (start,end)=self.resource.visible_range;
        let lo=start.saturating_sub(24).min(self.resource.assets.len());
        let mid=start.min(self.resource.assets.len());
        let hi=end.max(start).saturating_add(48).min(self.resource.assets.len());
        let candidates:Vec<&Asset>=if self.resource.large_preview_open{
            self.preview_strip()
        }else{
            self.resource.assets[mid..hi].iter().chain(self.resource.assets[lo..mid].iter()).collect()
        };
        let items=candidates.into_iter().filter(|a|a.category=="image"&&!self.thumbnails.read(cx).attempted(a.id)&&!self.resource.cached_thumbnails.contains_key(&a.id)).take(12).cloned().collect::<Vec<_>>();
        if items.is_empty(){
            #[cfg(feature="qa")]
            if std::env::var_os("YUQUE_QA_PERF").is_some() && self.resource.perf_preview_count!=self.resource.cached_thumbnails.len(){
                self.resource.perf_preview_count=self.resource.cached_thumbnails.len();
                eprintln!("PERF previews_ready_ms={} count={}",self.resource.perf_start.elapsed().as_millis(),self.resource.cached_thumbnails.len());
            }
            return;
        }
        let storage=self.storage.clone();
        self.thumbnails.update(cx,|v,cx|v.request(storage,items,cx));
    }

    pub(super) fn kick_previews(&mut self,cx:&mut Context<Self>){
        if self.resetting||self.reset_pending||self.backup_busy{return;}
        let storage=self.storage.clone();self.preview_worker.update(cx,|v,cx|v.kick(storage,cx));
    }
    pub(super) fn repair_visible_previews(&mut self,cx:&mut Context<Self>){
        if self.preview_worker.read(cx).busy{self.status="缩略图后台任务正在运行".into();cx.notify();return;}
        let (start,end)=self.resource.visible_range;
        let lo=start.min(self.resource.assets.len());
        let hi=end.max(start.saturating_add(24)).min(self.resource.assets.len());
        let ids=self.resource.assets[lo..hi].iter().filter(|a|a.category=="image"&&a.provider==self.config.provider()&&a.owner_id==self.config.owner_id()).take(100).map(|a|a.id).collect::<Vec<_>>();
        if ids.is_empty(){self.status="当前可见范围没有本账号图片".into();cx.notify();return;}
        self.repair_local_previews(ids,cx);
    }
    pub(super) fn audit_previews(&mut self,repair:bool,cx:&mut Context<Self>){
        if self.preview_worker.read(cx).busy||self.account_busy(cx)||self.backup_busy||self.resetting||self.reset_pending{return;}
        let storage=self.storage.clone();let provider=self.config.provider().to_owned();let owner=self.config.owner_id().to_owned();
        self.preview_worker.update(cx,|v,cx|v.scan(storage,provider,owner,repair,cx));
    }
    pub(super) fn repair_local_previews(&mut self,ids:Vec<i64>,cx:&mut Context<Self>){
        if self.preview_worker.read(cx).busy||self.account_busy(cx)||self.backup_busy||self.resetting{return;}
        let storage=self.storage.clone();let provider=self.config.provider().to_owned();let owner=self.config.owner_id().to_owned();
        self.preview_worker.update(cx,|v,cx|v.repair(storage,provider,owner,ids,cx));
    }
    pub(super) fn apply_preview_completion(&mut self,item:&preview::PreviewCompletion,cx:&mut Context<Self>){
        if !item.settled{return;}
        let id=item.id;
        if let Some((path,ratio))=&item.found{
            if self.resource.assets.iter().any(|a|a.id==id){
                self.resource.cached_thumbnails.insert(id,path.clone());
                if self.resource.ratios.insert(id,*ratio)!=Some(*ratio){self.resource.ratio_revision+=1;self.resource.masonry_signature=0;}
                if item.changed{self.resource.image_cache.update(cx,|cache,cx|cache.invalidate_path(path,cx));}
            }
            if self.resource.selected_asset.as_ref().is_some_and(|a|a.id==id){
                self.resource.thumbnail_path=Some(path.clone());self.resource.preview_error=None;
                if self.resource.large_preview_open{self.resource.large_preview_path=Some(path.clone());}
            }
        }else{
            if let Some(path)=self.resource.cached_thumbnails.remove(&id){
                self.resource.image_cache.update(cx,|cache,cx|cache.invalidate_path(&path,cx));
                self.resource.ratios.remove(&id);self.resource.ratio_revision+=1;self.resource.masonry_signature=0;
            }
            if self.resource.selected_asset.as_ref().is_some_and(|a|a.id==id){
                self.resource.thumbnail_path=None;self.resource.large_preview_path=None;
                self.resource.preview_error=Some("本地预览生成失败，可在任务中心重试或主动恢复在线缩略图".into());
            }
        }
        // Only refresh visible job history when an affected row is present.
        // The worker emits Idle for a final refresh after the batch.
        if self.page==Page::Jobs&&self.job_history.read(cx).jobs.iter().any(|j|j.result_asset_id==Some(id)){self.refresh_jobs(cx);}
        cx.notify();
    }

    pub(super) fn apply_thumbnail_batch(&mut self,event:&thumbnail_model::ThumbnailBatch,cx:&mut Context<Self>){
        if !self.thumbnails.read(cx).current(event.generation){return;}
        for (id,path,ratio) in &event.found{
            self.resource.cached_thumbnails.insert(*id,path.clone());
            if self.resource.ratios.insert(*id,*ratio)!=Some(*ratio){self.resource.ratio_revision+=1;self.resource.masonry_signature=0;}
        }
        self.refresh_thumbnails(cx);
        cx.notify();
    }

    pub(super) fn export_album(&mut self,cx:&mut Context<Self>){
        if self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.read(cx).busy||self.preview_worker.read(cx).busy||self.account_busy(cx){return;}
        self.picker_pending=true;
        self.status="请选择相册备份位置…".into();cx.notify();
        let selection=cx.prompt_for_paths(PathPromptOptions{files:false,directories:true,multiple:false,prompt:Some("选择备份位置".into())});
        self.picker_task=Some(cx.spawn(async move|this,cx|{
            let result=selection.await;
            let _=this.update(cx,|v,cx|{
                v.picker_pending=false;
                match result{
                    Ok(Ok(Some(mut paths))) if paths.len()==1=>v.export_album_to(paths.remove(0),cx),
                    Ok(Ok(_))=>v.status="已取消备份位置选择".into(),
                    Ok(Err(_))|Err(_)=>v.status="无法打开备份目录选择窗口，请重试".into(),
                }
                cx.notify();
            });
        }));
    }

    fn export_album_to(&mut self,parent:PathBuf,cx:&mut Context<Self>){
        if self.preview_worker.read(cx).busy||self.queue.read(cx).busy{self.status="请等待上传与预览任务完成后再备份".into();cx.notify();return;}
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
        self.mutate_resource(mutations::Mutation::Favorite(asset.clone()),cx);
    }

    pub(super) fn find_similar(&mut self,asset:Asset,cx:&mut Context<Self>) {
        self.resource.similar_to=Some(asset);
        self.inspector_open=false;
        self.reset_history(cx);
    }

    pub(super) fn exit_similar(&mut self,cx:&mut Context<Self>){
        if self.resource.similar_to.take().is_some(){self.reset_history(cx);}
    }

    pub(super) fn clear_browse_filters(&mut self,w:&mut Window,cx:&mut Context<Self>){
        self.resource.similar_to=None;
        self.resource.category="";
        self.resource.favorites_only=false;
        self.resource.all_accounts=false;
        self.resource.tag_filter=None;
        self.resource.search.clear();
        self.resource.search_input.update(cx,|input,cx|input.set_value("",w,cx));
        self.persist_preferences(cx);
        self.reset_history(cx);
    }

    pub(super) fn has_browse_filters(&self)->bool{
        !self.resource.search.is_empty()||!self.resource.category.is_empty()||
        self.resource.favorites_only||self.resource.all_accounts||self.resource.tag_filter.is_some()
    }

    pub(super) fn locate_job_asset(&mut self,id:i64,window:&mut Window,cx:&mut Context<Self>){
        if self.account_busy(cx){return;}
        self.resource.category="";
        self.resource.favorites_only=false;self.resource.all_accounts=false;
        self.resource.tag_filter=None;self.resource.similar_to=None;
        let query=format!("id:{id}");
        self.resource.search=query.clone();
        self.resource.search_input.update(cx,|input,cx|input.set_value(query,window,cx));
        self.navigate_page(Page::Gallery,cx);
        self.reset_history(cx);self.focus_gallery_destination(window,cx);
        self.status="已按任务关联定位资源；清空搜索可返回全部资源".into();
        cx.notify();
    }

    pub(super) fn select_asset(&mut self, asset: Asset, window: &mut Window, cx: &mut Context<Self>) {
        if !self.resource.large_preview_open&&self.resource.selected_asset.as_ref().is_some_and(|a|a.id==asset.id)&&self.inspector_model.read(cx).ready{return;}
        self.clear_quality(cx);
        self.resource.pending_preview=None;
        let storage=self.storage.clone();let id=asset.id;
        self.inspector_model.update(cx,|model,cx|model.select(id,storage,window,cx));
        self.resource.large_preview_open=false;
        self.resource.large_preview_path=None;
        self.resource.thumbnail_path=None;
        self.resource.preview_task=None;
        self.resource.preview_loading=false;
        self.resource.preview_error=None;
        self.resource.selected_asset=Some(asset.clone());
        if asset.category=="image"{
            if let Some(path)=self.resource.cached_thumbnails.get(&asset.id){self.resource.thumbnail_path=Some(path.clone());}
            else{self.load_preview(cx);}
        }
        cx.notify();
    }
    pub(super) fn show_asset_details(&mut self,asset:Asset,window:&mut Window,cx:&mut Context<Self>){
        if !self.resource.selected_ids.contains(&asset.id){self.resource.selected_ids.clear();self.resource.selected_ids.insert(asset.id);self.resource.selection_anchor=Some(asset.id);}self.select_asset(asset,window,cx);self.set_inspector(true,true,window,cx);
    }
    pub(super) fn click_tile(&mut self,asset:Asset,count:usize,modifiers:Modifiers,window:&mut Window,cx:&mut Context<Self>){
        let order=self.resource.assets.iter().map(|a|a.id).collect::<Vec<_>>();
        selection::click(&mut self.resource.selected_ids,&order,&mut self.resource.selection_anchor,asset.id,modifiers.shift,modifiers.platform||modifiers.control);
        if self.resource.selected_ids.is_empty(){self.resource.batch_tag_open=false;}
        if self.resource.selected_ids.contains(&asset.id){self.select_asset(asset.clone(),window,cx);}else if let Some(a)=self.resource.assets.iter().find(|a|self.resource.selected_ids.contains(&a.id)).cloned(){self.select_asset(a,window,cx);}else{self.clear_resource_selection(cx);}
        self.resource.gallery_focus.focus(window,cx);
        if count>=2&&!modifiers.shift&&!modifiers.platform&&!modifiers.control&&asset.category=="image"{self.open_large_preview(window,cx);}
        cx.notify();
    }
    pub(super) fn gallery_key(&mut self,e:&KeyDownEvent,w:&mut Window,cx:&mut Context<Self>){
        if !self.resource.gallery_focus.is_focused(w){return;}
        let m=e.keystroke.modifiers;
        let key=e.keystroke.key.as_str();
        if !m.platform&&!m.control&&!m.alt{
            match key{
                "left"|"right"|"up"|"down"=>{
                    let at=self.resource.selected_asset.as_ref().and_then(|a|self.resource.assets.iter().position(|x|x.id==a.id));
                    let next=if let Some(i)=at{
                        let spacing=self.spacing();
                        if self.resource.masonry&&self.resource.grid_view{
                            album::masonry_neighbor(&self.resource.masonry_plan,i,key).unwrap_or(i)
                        }else{
                            album::row_neighbor(&self.resource.row_plan,i,key,spacing.gap).unwrap_or(i)
                        }
                    }else{0};
                    if let Some(asset)=self.resource.assets.get(next).cloned(){
                        self.click_tile(asset,1,m,w,cx);
                        if self.resource.masonry&&self.resource.grid_view{
                            if let Some(t)=self.resource.masonry_plan.tiles.iter().find(|t|t.index==next){
                                let handle=&self.resource.masonry_scroll;let top=-f32::from(handle.offset().y);
                                let viewport=f32::from(handle.bounds().size.height);
                                let bottom=t.y+t.height+self.spacing().footer;
                                if t.y<top{handle.set_offset(point(px(0.),px(-t.y)));}else if bottom>top+viewport{handle.set_offset(point(px(0.),px(-(bottom-viewport).max(0.))));}
                            }
                        }else if let Some(row)=self.resource.row_plan.iter().position(|r|match r{album::AlbumRow::Tiles{items,..}=>items.iter().any(|(i,_)|*i==next),album::AlbumRow::Item(i)=>*i==next,_=>false}){
                            self.resource.album_list.scroll_to_reveal_item(row);
                        }
                    }
                }
                _=>return,
            }
        }else{return;}
        cx.stop_propagation();
    }
    pub(super) fn save_selected_tags(&mut self,cx:&mut Context<Self>){
        let model=self.inspector_model.read(cx);
        if !model.ready||self.account_busy(cx){return;}
        let Some(id)=self.resource.selected_asset.as_ref().map(|a|a.id)else{return;};
        let text=model.input.read(cx).value().to_string();
        self.mutate_resource(mutations::Mutation::Tags(id,text),cx);
    }

    pub(super) fn load_preview(&mut self, cx: &mut Context<Self>) {self.repair_preview(false,cx);}
    pub(super) fn repair_preview(&mut self,remote:bool,cx:&mut Context<Self>){
        let Some(asset) = self.resource.selected_asset.clone() else { return; };
        if asset.category != "image" || self.resource.preview_loading { return; }
        let storage = self.storage.clone();
        let id = asset.id;
        self.resource.preview_loading = true;
        self.resource.preview_error = None;
        self.resource.preview_task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move {
                let path=if remote{preview::recover_remote(&storage,&asset)?}else{preview::restore_local(&storage,&asset)?};
                let (w,h)=image::image_dimensions(&path)?;
                Ok::<_,anyhow::Error>((path,w as f32/h.max(1) as f32))
            }).await;
            _ = this.update(cx, |view, cx| {
                if view.resource.selected_asset.as_ref().is_none_or(|a| a.id != id) { return; }
                view.resource.preview_loading = false;
                match result {
                    Ok((path,ratio)) => {
                        view.resource.cached_thumbnails.insert(id, path.clone());
                        if view.resource.ratios.insert(id,ratio)!=Some(ratio){view.resource.ratio_revision+=1;view.resource.masonry_signature=0;}
                        view.resource.image_cache.update(cx,|cache,cx|cache.clear(cx));
                        if view.resource.large_preview_open{view.resource.large_preview_path=Some(path.clone());}
                        view.resource.thumbnail_path = Some(path);
                        view.refresh_jobs(cx);
                    },
                    Err(error) => view.resource.preview_error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn open_large_preview(&mut self, window:&mut Window,cx: &mut Context<Self>) {
        let Some(asset) = self.resource.selected_asset.clone() else { return; };
        if asset.category != "image" { return; }
        if self.resource.viewer_return.is_none()&&self.resource.selected_ids.len()>1{
            self.resource.viewer_return=Some((asset.clone(),self.resource.selected_ids.clone(),self.resource.selection_anchor));
        }
        self.clear_quality(cx);
        self.resource.pending_preview=None;
        self.resource.preview_task=None;
        let storage = self.storage.clone();
        let id = asset.id;
        self.resource.large_preview_open = true;
        self.resource.preview_focus.focus(window,cx);

        self.resource.zoom_channel=self.resource.zoom_channel.wrapping_add(1);
        self.resource.displayed_zoom=1.;
        self.resource.zoom=1.; self.resource.pan=point(px(0.),px(0.)); self.resource.drag_at=None;
        self.resource.large_preview_path = self.resource.cached_thumbnails.get(&id).cloned();
        self.refresh_thumbnails(cx);
        if self.resource.large_preview_path.is_some(){self.resource.preview_loading=false;self.resource.preview_error=None;self.resource.thumbnail_path=self.resource.large_preview_path.clone();cx.notify();return;}
        self.resource.preview_loading = true;
        self.resource.preview_error = None;
        self.resource.preview_task = Some(cx.spawn(async move |this,cx| {
            let result = cx.background_spawn(async move {
                let path=preview::restore_local(&storage,&asset)?;
                let (w,h)=image::image_dimensions(&path)?;
                Ok::<_,anyhow::Error>((path,w as f32/h.max(1) as f32))
            }).await;
            _ = this.update(cx, |v,cx| {
                if v.resource.selected_asset.as_ref().is_none_or(|a|a.id!=id) { return; }
                v.resource.preview_loading = false;
                match result {
                    Ok((path,ratio))=>{
                        v.resource.thumbnail_path=Some(path.clone());
                        v.resource.cached_thumbnails.insert(id,path.clone());
                        if v.resource.ratios.insert(id,ratio)!=Some(ratio){v.resource.ratio_revision+=1;v.resource.masonry_signature=0;}
                        v.resource.large_preview_path=Some(path);
                    }
                    Err(e)=>v.resource.preview_error=Some(e.to_string())
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn apply_batch_tags(&mut self, remove:bool,cx:&mut Context<Self>) {
        let ids=self.resource.assets.iter().filter(|a|self.resource.selected_ids.contains(&a.id)).map(|a|a.id).collect::<Vec<_>>();
        let text=self.resource.batch_tag_input.read(cx).value().to_string();
        let scope=self.resource.similar_to.as_ref().map(|a|(a.provider.clone(),a.owner_id.clone()))
            .or_else(||(!self.resource.all_accounts).then(||(self.config.provider().to_owned(),self.config.owner_id().to_owned())));
        if !ids.is_empty(){self.mutate_resource(mutations::Mutation::Batch(ids,scope,text,remove),cx);}
    }

    pub(super) fn preview_size(&self,window:&Window,cx:&App)->(f32,f32,f32){
        let ratio=self.viewer_model.read(cx).quality.as_ref().map(|q|q.width as f32/q.height as f32).or_else(||self.resource.selected_asset.as_ref().and_then(|a|self.resource.ratios.get(&a.id)).copied()).filter(|r|r.is_finite()&&*r>0.).unwrap_or(1.);
        ((f32::from(window.viewport_size().width)-32.).max(1.),(f32::from(window.viewport_size().height)-200.).max(1.),ratio)
    }
    pub(super) fn preview_strip(&self)->Vec<&Asset>{
        let Some(id)=self.resource.selected_asset.as_ref().map(|a|a.id) else{return vec![];};
        let Some(at)=self.resource.assets.iter().position(|a|a.id==id)else{return vec![];};
        let mut after=self.resource.assets[at..].iter().filter(|a|a.category=="image").take(4).collect::<Vec<_>>();
        let mut before=self.resource.assets[..at].iter().rev().filter(|a|a.category=="image").take(7-after.len()).collect::<Vec<_>>();
        if before.len()+after.len()<7{
            after.extend(self.resource.assets[at..].iter().filter(|a|a.category=="image").skip(after.len()).take(7-before.len()-after.len()));
        }
        before.reverse();before.extend(after);before
    }
    pub(super) fn preview_neighbor(&self,forward:bool)->Option<Asset>{
        let id=self.resource.selected_asset.as_ref()?.id;
        let at=self.resource.assets.iter().position(|a|a.id==id)?;
        if forward{self.resource.assets[at+1..].iter().find(|a|a.category=="image").cloned()}
        else{self.resource.assets[..at].iter().rev().find(|a|a.category=="image").cloned()}
    }
    pub(super) fn step_preview(&mut self,forward:bool,window:&mut Window,cx:&mut Context<Self>){
        if forward&&self.resource.pending_preview.is_some(){return;}
        self.resource.pending_preview=None;
        if let Some(asset)=self.preview_neighbor(forward){
            self.select_asset(asset,window,cx);
            self.open_large_preview(window,cx);
        }else if forward&&self.resource.has_more&&self.resource.history_error.is_none(){
            if let Some(id)=self.resource.selected_asset.as_ref().map(|a|a.id){
                self.resource.pending_preview=Some((id,8));
                self.load_more(cx);cx.notify();
            }
        }
    }
    pub(super) fn continue_preview_page(&mut self,id:i64,left:u8,window:&mut Window,cx:&mut Context<Self>){
        if !self.resource.large_preview_open||self.resource.selected_asset.as_ref().is_none_or(|a|a.id!=id){return;}
        if let Some(a)=self.preview_neighbor(true){
            self.select_asset(a,window,cx);self.open_large_preview(window,cx);
        }else if self.resource.has_more&&self.resource.history_error.is_none()&&left>0{
            self.resource.pending_preview=Some((id,left-1));self.load_more(cx);
        }else{
            let status=if self.resource.history_error.is_some(){"下一页加载失败，请返回相册重试".into()}else if self.resource.has_more{"本次已检查多页，可继续点击下一张".into()}else{"已到最后一张图片".into()};
            self.viewer_model.update(cx,|v,cx|v.feedback(status,cx));
        }
        cx.notify();
    }
    pub(super) fn clear_quality(&mut self,cx:&mut Context<Self>){
        self.viewer_model.update(cx,|v,cx|v.clear(cx));
    }
    pub(super) fn load_quality(&mut self,remote:bool,cx:&mut Context<Self>){
        if !self.resource.large_preview_open{return;}
        let Some(asset)=self.resource.selected_asset.clone()else{return;};
        let storage=self.storage.clone();
        self.viewer_model.update(cx,|v,cx|v.load(asset,storage,remote,cx));
    }
    pub(super) fn close_preview(&mut self,window:&mut Window,cx:&mut Context<Self>){
        self.clear_quality(cx);
        self.resource.pending_preview=None;
        self.resource.large_preview_open=false;
        self.resource.drag_at=None;
        if self.inspector_open&&!self.immersive&&window.viewport_size().width<px(1000.){
            self.resource.inspector_focus.focus(window,cx);
        }else{self.resource.gallery_focus.focus(window,cx);}
        if let Some((a,ids,anchor))=self.resource.viewer_return.take(){
            self.select_asset(a,window,cx);self.resource.selected_ids=ids;self.resource.selection_anchor=anchor;
        }else if let Some(a)=&self.resource.selected_asset{self.resource.selected_ids.clear();self.resource.selected_ids.insert(a.id);self.resource.selection_anchor=Some(a.id);}
        if self.resource.preview_reload_pending{self.reload(cx);}else{self.refresh_thumbnails(cx);}
        cx.notify();
    }
    pub(super) fn zoom_by(&mut self,delta:f32,cx:&mut Context<Self>){
        self.resource.zoom=(self.resource.zoom+delta).clamp(1.,4.);
        if self.resource.zoom==1.{self.resource.pan=point(px(0.),px(0.));}
        cx.notify();
    }
    pub(super) fn zoom_at(&mut self,delta:f32,position:Point<Pixels>,window:&Window,cx:&mut Context<Self>){
        let (w,h,ratio)=self.preview_size(window,cx);
        let old=self.resource.displayed_zoom;
        let new=(old+delta).clamp(1.,4.);
        let anchor=(f32::from(position.x)-16.-w*0.5,f32::from(position.y)-60.-h*0.5);
        let current=album::clamp_pan((w,h),album::fit_preview(w,h,ratio,old),(f32::from(self.resource.pan.x),f32::from(self.resource.pan.y)));
        let pan=album::anchor_pan(current,anchor,old,new);
        let p=album::clamp_pan((w,h),album::fit_preview(w,h,ratio,new),pan);
        self.resource.zoom=new;self.resource.displayed_zoom=new;
        self.resource.zoom_channel=self.resource.zoom_channel.wrapping_add(1);
        self.resource.pan=point(px(p.0),px(p.1));cx.notify();
    }
    pub(super) fn drag_preview(&mut self,position:Point<Pixels>,window:&Window,cx:&mut Context<Self>){
        if let Some(prev)=self.resource.drag_at{
            let d=position-prev;let (w,h,ratio)=self.preview_size(window,cx);
            let pan=album::clamp_pan((w,h),album::fit_preview(w,h,ratio,self.resource.displayed_zoom),(f32::from(self.resource.pan.x+d.x),f32::from(self.resource.pan.y+d.y)));
            self.resource.pan=point(px(pan.0),px(pan.1));
            self.resource.drag_at=Some(position);cx.notify();
        }
    }
    pub(super) fn open_selected_resource(&mut self,cx:&mut Context<Self>) {
        let Some(asset)=self.resource.selected_asset.as_ref() else{return;};
        let Ok(url)=url::Url::parse(&asset.url) else {self.status="资源链接无效".into();if self.resource.large_preview_open{self.viewer_model.update(cx,|v,cx|v.feedback(self.status.clone(),cx));}cx.notify();return;};
        if asset.provider=="demo" && url.scheme()=="file" {
            if let Ok(path)=url.to_file_path() {
                if path.is_file(){cx.open_with_system(&path);return;}
            }
        } else if url.scheme()=="https" && url.username().is_empty() && url.password().is_none() {
            cx.open_url(url.as_str());return;
        }
        self.status="资源不存在或链接不受支持".into();
        if self.resource.large_preview_open{self.viewer_model.update(cx,|v,cx|v.feedback(self.status.clone(),cx));}
        cx.notify();
    }

    pub(super) fn copy_selected(&mut self, markdown: bool, cx: &mut Context<Self>) {
        let links = self.resource.assets.iter().filter(|a| self.resource.selected_ids.contains(&a.id))
            .map(|a| if markdown { format::markdown(&a.name, &a.url, a.category == "image") } else { a.url.clone() }).collect::<Vec<_>>();
        if links.is_empty() { return; }
        cx.write_to_clipboard(ClipboardItem::new_string(links.join("\n")));
        self.status = format!("已复制 {} 条 {}", links.len(), if markdown { "Markdown 引用" } else { "URL" });
        cx.notify();
    }

    pub(super) fn copy_asset(&mut self, asset: &Asset, as_markdown: bool, window:&mut Window,cx: &mut Context<Self>) {
        let content = if as_markdown {
            format::markdown(&asset.name, &asset.url, asset.category == "image")
        } else {
            asset.url.clone()
        };
        self.copy_result_text(content,as_markdown,window,cx);
    }
    pub(super) fn copy_result_text(&mut self,content:String,as_markdown:bool,window:&mut Window,cx:&mut Context<Self>){
        self.copy_text_feedback(content,if as_markdown{"Markdown 引用已复制"}else{"资源链接已复制"},window,cx);
    }
    pub(super) fn copy_text_feedback(&mut self,content:String,success:&str,window:&mut Window,cx:&mut Context<Self>){
        use gpui_kit::component::{notification::Notification,WindowExt};
        struct CopyFeedback;
        let notification=match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(content)) {
            Ok(_) => {
                self.status=success.into();
                Notification::success(self.status.clone())
            }
            Err(error) => {self.status=format!("写入剪贴板失败：{error}");Notification::error(self.status.clone())}
        };
        window.push_notification(notification.id::<CopyFeedback>(),cx);
        cx.notify();
    }
}
