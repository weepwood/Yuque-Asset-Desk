use super::*;

impl UploadDesk {
    pub(super) fn reset_history(&mut self, cx: &mut Context<Self>) {
        self.clear_quality(cx);
        self.resource.preview_reload_pending=false;
        self.resource.pending_preview=None;
        self.resource.selection_anchor=None;self.resource.batch_tag_open=false;
        self.inspector_model.update(cx,|model,cx|model.reset(cx));
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
                            if view.resource.selected_asset.as_ref().is_some_and(|a|!items.iter().any(|x|x.id==a.id)){
                                view.resource.selected_asset=None;view.inspector_model.update(cx,|m,cx|m.reset(cx));
                            }
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
        let candidates:Vec<&Asset>=if self.resource.large_preview_open{
            self.preview_strip()
        }else{
            self.resource.assets[mid..hi].iter().chain(self.resource.assets[lo..mid].iter()).collect()
        };
        let items=candidates.into_iter().filter(|a|a.category=="image"&&!self.resource.preview_attempted.contains(&a.id)&&!self.resource.cached_thumbnails.contains_key(&a.id)).take(12).cloned().collect::<Vec<_>>();
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
        if self.picker_pending||self.backup_busy||self.queue.busy||self.account.account_busy{return;}
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
        if asset.category=="image"{self.load_preview(cx);}
        cx.notify();
    }
    pub(super) fn show_asset_details(&mut self,asset:Asset,window:&mut Window,cx:&mut Context<Self>){
        self.select_asset(asset,window,cx);self.immersive=false;self.inspector_open=true;cx.notify();
    }
    pub(super) fn click_tile(&mut self,asset:Asset,count:usize,modifiers:Modifiers,window:&mut Window,cx:&mut Context<Self>){
        let order=self.resource.assets.iter().map(|a|a.id).collect::<Vec<_>>();
        selection::click(&mut self.resource.selected_ids,&order,&mut self.resource.selection_anchor,asset.id,modifiers.shift,modifiers.platform||modifiers.control);
        if self.resource.selected_ids.is_empty(){self.resource.batch_tag_open=false;}
        if self.resource.selected_ids.contains(&asset.id){self.select_asset(asset.clone(),window,cx);}else if let Some(a)=self.resource.assets.iter().find(|a|self.resource.selected_ids.contains(&a.id)).cloned(){self.select_asset(a,window,cx);}else{self.resource.selected_asset=None;self.inspector_model.update(cx,|m,cx|m.reset(cx));}
        self.resource.gallery_focus.focus(window,cx);
        if count>=2&&!modifiers.shift&&!modifiers.platform&&!modifiers.control&&asset.category=="image"{self.open_large_preview(window,cx);}
        cx.notify();
    }
    pub(super) fn gallery_key(&mut self,e:&KeyDownEvent,w:&mut Window,cx:&mut Context<Self>){
        if !self.resource.gallery_focus.is_focused(w){return;}
        let m=e.keystroke.modifiers;
        let key=e.keystroke.key.as_str();
        if m.platform||m.control{
            match key{"a"=>self.command(Command::SelectAll,w,cx),"i"=>self.command(Command::ToggleInspector,w,cx),_=>return}
        }else if !m.alt{
            match key{
                "space"=>self.command(Command::Preview,w,cx),
                "escape"=>self.command(Command::ClearSelection,w,cx),
                "left"|"right"|"up"|"down"=>{
                    let at=self.resource.selected_asset.as_ref().and_then(|a|self.resource.assets.iter().position(|x|x.id==a.id));
                    let next=match at{Some(i) if key=="left"||key=="up"=>i.saturating_sub(1),Some(i)=>(i+1).min(self.resource.assets.len().saturating_sub(1)),None=>0};
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
        if !model.ready||self.account.account_busy{return;}
        let Some(id)=self.resource.selected_asset.as_ref().map(|a|a.id)else{return;};
        let text=model.input.read(cx).value().to_string();
        // Keep this bounded existing write synchronous until mutation/cleanup
        // serialization is separated from the shell.
        self.status=match self.storage.set_tags(id,&text){Ok(())=>"标签已保存".into(),Err(_)=>"标签保存失败，请重试".into()};
        self.reload(cx);cx.notify();
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

    pub(super) fn open_large_preview(&mut self, window:&mut Window,cx: &mut Context<Self>) {
        let Some(asset) = self.resource.selected_asset.clone() else { return; };
        if asset.category != "image" { return; }
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
                        if v.resource.ratios.insert(id,ratio)!=Some(ratio){v.resource.ratio_revision+=1;}
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
        let scope=(!self.resource.all_accounts).then(||(self.account.config.provider(),self.account.config.owner_id()));
        self.status=match self.storage.batch_tags(&ids,scope,&text,remove) {
            Ok(())=>format!("已更新 {} 项资源标签",ids.len()), Err(e)=>e.to_string()
        };
        self.reload(cx);
        cx.notify();
    }

    pub(super) fn preview_size(&self,window:&Window)->(f32,f32,f32){
        let ratio=self.resource.quality.as_ref().map(|q|q.width as f32/q.height as f32).or_else(||self.resource.selected_asset.as_ref().and_then(|a|self.resource.ratios.get(&a.id)).copied()).filter(|r|r.is_finite()&&*r>0.).unwrap_or(1.);
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
        }else{self.resource.quality_status=if self.resource.history_error.is_some(){"下一页加载失败，请返回相册重试".into()}else if self.resource.has_more{"本次已检查多页，可继续点击下一张".into()}else{"已到最后一张图片".into()};}
        cx.notify();
    }
    pub(super) fn clear_quality(&mut self,cx:&mut Context<Self>){
        self.resource.quality_generation=self.resource.quality_generation.wrapping_add(1);
        self.resource.quality_task=None;
        self.resource.quality_busy=false;self.resource.quality_status.clear();
        if let Some(q)=self.resource.quality.take(){cx.drop_image(q.image,None);}
    }
    pub(super) fn load_quality(&mut self,remote:bool,cx:&mut Context<Self>){
        if !self.resource.large_preview_open||self.resource.quality_busy{return;}
        let Some(asset)=self.resource.selected_asset.clone()else{return;};
        if self.resource.quality.as_ref().is_some_and(|q|q.remote==remote){return;}
        self.resource.quality_generation=self.resource.quality_generation.wrapping_add(1);
        self.resource.quality_task=None;
        self.resource.quality_busy=true;self.resource.quality_status=if remote{"正在读取在线原图…".into()}else{"正在读取本地原图…".into()};
        let generation=self.resource.quality_generation;let storage=self.storage.clone();let id=asset.id;
        self.resource.quality_task=Some(cx.spawn(async move|this,cx|{
            let result=cx.background_spawn(async move{crate::high_preview::load(&storage,&asset,remote)}).await;
            let _=this.update(cx,|v,cx|{
                if !v.resource.large_preview_open||v.resource.quality_generation!=generation||v.resource.selected_asset.as_ref().is_none_or(|a|a.id!=id){return;}
                v.resource.quality_busy=false;
                match result{
                    Ok(q)=>{
                        v.resource.quality_status.clear();
                        if let Some(old)=v.resource.quality.replace(q){cx.drop_image(old.image,None);}
                        v.resource.zoom=1.;v.resource.pan=point(px(0.),px(0.));v.resource.zoom_channel=v.resource.zoom_channel.wrapping_add(1);
                    }
                    Err(e)=>v.resource.quality_status=e.to_string(),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn close_preview(&mut self,window:&mut Window,cx:&mut Context<Self>){
        self.clear_quality(cx);
        self.resource.pending_preview=None;
        self.resource.large_preview_open=false;
        self.resource.drag_at=None;
        self.resource.gallery_focus.focus(window,cx);
        if let Some(a)=&self.resource.selected_asset{self.resource.selected_ids.clear();self.resource.selected_ids.insert(a.id);self.resource.selection_anchor=Some(a.id);}
        if self.resource.preview_reload_pending{self.reload(cx);}else{self.refresh_thumbnails(cx);}
        cx.notify();
    }
    pub(super) fn zoom_by(&mut self,delta:f32,cx:&mut Context<Self>){
        self.resource.zoom=(self.resource.zoom+delta).clamp(1.,4.);
        if self.resource.zoom==1.{self.resource.pan=point(px(0.),px(0.));}
        cx.notify();
    }
    pub(super) fn zoom_at(&mut self,delta:f32,position:Point<Pixels>,window:&Window,cx:&mut Context<Self>){
        let (w,h,ratio)=self.preview_size(window);
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
            let d=position-prev;let (w,h,ratio)=self.preview_size(window);
            let pan=album::clamp_pan((w,h),album::fit_preview(w,h,ratio,self.resource.displayed_zoom),(f32::from(self.resource.pan.x+d.x),f32::from(self.resource.pan.y+d.y)));
            self.resource.pan=point(px(pan.0),px(pan.1));
            self.resource.drag_at=Some(position);cx.notify();
        }
    }
    pub(super) fn open_selected_resource(&mut self,cx:&mut Context<Self>) {
        let Some(asset)=self.resource.selected_asset.as_ref() else{return;};
        let Ok(url)=url::Url::parse(&asset.url) else {self.status="资源链接无效".into();if self.resource.large_preview_open{self.resource.quality_status=self.status.clone();}cx.notify();return;};
        if asset.provider=="demo" && url.scheme()=="file" {
            if let Ok(path)=url.to_file_path() {
                if path.is_file(){cx.open_with_system(&path);return;}
            }
        } else if url.scheme()=="https" && url.username().is_empty() && url.password().is_none() {
            cx.open_url(url.as_str());return;
        }
        self.status="资源不存在或链接不受支持".into();
        if self.resource.large_preview_open{self.resource.quality_status=self.status.clone();}
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
        use gpui_kit::component::{notification::Notification,WindowExt};
        struct CopyFeedback;
        let notification=match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(content)) {
            Ok(_) => {
                self.status=if as_markdown{"Markdown 引用已复制"}else{"资源链接已复制"}.into();
                Notification::success(self.status.clone())
            }
            Err(error) => {self.status=format!("写入剪贴板失败：{error}");Notification::error(self.status.clone())}
        };
        window.push_notification(notification.id::<CopyFeedback>(),cx);
        cx.notify();
    }
}
