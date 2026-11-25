//! UI-only preflight. No network or SQLite writes before the user confirms.
use super::*;
use super::queue_prepare::{Prepare,Source};
use gpui_kit::component::{IconName,Sizable};
use std::io::Read;
use std::sync::atomic::AtomicBool;

const PAGE_SIZE:usize=24;
const INPUT_LIMIT:u64=32*1024*1024;

pub(super) struct DraftRow{
    pub name:String,pub bytes:u64,pub picture:Option<Arc<RenderImage>>,pub warning:Option<String>,pub valid:bool,
}
pub(super) struct Review{
    scope:(String,String),sources:Vec<Source>,rows:Vec<DraftRow>,loading:bool,selected:usize,
    preview_loading:bool,preview_cancel:Option<Arc<AtomicBool>>,preview_done:usize,
}
impl Drop for Review{fn drop(&mut self){if let Some(cancel)=&self.preview_cancel{cancel.store(true,Ordering::Relaxed);}}}
fn inspect(source:&Source,max:u64,show:bool)->DraftRow{
    let (name,bytes,content)=match source{
        Source::Path(path)=>{
            let name=path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            match std::fs::metadata(path){
                Ok(m) if m.is_file()&&m.len()>0&&m.len()<=max=>{
                    let bytes=if show&&m.len()<=INPUT_LIMIT{std::fs::File::open(path).and_then(|f|{let mut b=Vec::new();f.take(INPUT_LIMIT+1).read_to_end(&mut b)?;Ok(b)}).ok()}else{None};
                    (name,m.len(),bytes)
                }
                _=>return DraftRow{name,bytes:0,picture:None,warning:Some("文件不可读、为空或超过上传限制".into()),valid:false}
            }
        }
        Source::Image(bytes,ext)=>{
            let len=bytes.len() as u64;
            if len==0||len>max{return DraftRow{name:format!("剪贴板.{ext}"),bytes:len,picture:None,warning:Some("剪贴板内容为空或超过限制".into()),valid:false};}
            (format!("剪贴板.{ext}"),len,(show&&len<=INPUT_LIMIT).then(||bytes.as_ref().clone()))
        }
    };
    let category=uploader::asset_category(std::path::Path::new(&name));
    let picture=content.and_then(|b|preview::draft_image(b).ok());
    let warning=if show&&category=="image"&&picture.is_none(){Some("无法生成预览；确认后仍可尝试上传".into())}else{None};
    DraftRow{name,bytes,picture,warning,valid:true}
}
impl UploadDesk{
    pub(super) fn review_files(&mut self,files:Vec<PathBuf>,cx:&mut Context<Self>){
        self.review_sources(files.into_iter().map(Source::Path).collect(),cx);
    }
    pub(super) fn review_sources(&mut self,sources:Vec<Source>,cx:&mut Context<Self>){
        if sources.is_empty(){return;}
        if self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.read(cx).busy||self.account_busy(cx)||self.resetting{
            self.status="当前操作尚未结束，本次文件未加入".into();cx.notify();return;
        }
        if self.config.mode==Mode::Yuque&&self.config.creds.is_none(){self.status="请先配置语雀会话再上传".into();cx.notify();return;}
        let scope=(self.config.provider().to_owned(),self.config.owner_id().to_owned());
        if self.upload_review.as_ref().is_some_and(|r|r.loading){
            self.status="正在检查上一批文件，请稍后添加".into();cx.notify();return;
        }
        if self.upload_review.as_ref().is_some_and(|r|r.scope!=scope){self.cancel_upload_review(cx);}
        self.review_generation=self.review_generation.wrapping_add(1);
        let generation=self.review_generation;
        self.stop_review_preview(cx);
        let review=self.upload_review.get_or_insert_with(||Review{scope,sources:vec![],rows:vec![],loading:false,selected:0,preview_loading:false,preview_cancel:None,preview_done:0});
        review.loading=true;
        let max=self.config.max_file_mb*1024*1024;
        self.navigate_page(Page::Jobs,cx);
        self.review_task=Some(cx.spawn(async move|this,cx|{
            let (sources,rows)=cx.background_spawn(async move{
                let rows=sources.iter().map(|s|inspect(s,max,false)).collect::<Vec<_>>();
                (sources,rows)
            }).await;
            let _=this.update(cx,|v,cx|{
                if v.review_generation!=generation{return;}
                if let Some(r)=v.upload_review.as_mut(){
                    let offset=r.rows.len();r.sources.extend(sources);r.rows.extend(rows);r.loading=false;
                    r.selected=offset.min(r.rows.len().saturating_sub(1));
                }
                v.load_review_page(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }
    // Each page owns at most PAGE_SIZE decoded previews. Old workers check
    // cancellation between files; stale results never replace the current page.
    fn stop_review_preview(&mut self,cx:&mut Context<Self>){
        self.review_preview_task=None;
        if let Some(r)=self.upload_review.as_mut(){
            if let Some(cancel)=r.preview_cancel.take(){cancel.store(true,Ordering::Relaxed);}
            r.preview_loading=false;r.preview_done=0;
            image_cache::release_images(r.rows.iter_mut().filter_map(|row|row.picture.take()).collect(),cx);
        }
    }
    fn load_review_page(&mut self,cx:&mut Context<Self>){
        self.stop_review_preview(cx);
        self.review_generation=self.review_generation.wrapping_add(1);
        let generation=self.review_generation;
        let Some(r)=self.upload_review.as_mut()else{return;};
        if r.loading||r.rows.is_empty(){return;}
        let start=r.selected/PAGE_SIZE*PAGE_SIZE;
        let end=(start+PAGE_SIZE).min(r.sources.len());
        let selected=r.selected;
        // Selected image first, then small batches. One decode at a time,
        // no unbounded producer queue or waiting for the slowest file.
        let mut order=vec![selected];
        order.extend((start..end).filter(|i|*i!=selected));
        let mut sources=order.into_iter().map(|i|(i,r.sources[i].clone())).collect::<std::collections::VecDeque<_>>();
        let max=self.config.max_file_mb*1024*1024;
        let cancel=Arc::new(AtomicBool::new(false));
        r.preview_cancel=Some(cancel.clone());r.preview_loading=true;
        self.review_preview_task=Some(cx.spawn(async move|this,cx|{
            let mut first=true;
            while !sources.is_empty()&&!cancel.load(Ordering::Relaxed){
                let n=if first{1}else{3};first=false;
                let batch=(0..n).filter_map(|_|sources.pop_front()).collect::<Vec<_>>();
                let token=cancel.clone();
                let rows=cx.background_spawn(async move{
                    batch.into_iter().take_while(|_|!token.load(Ordering::Relaxed)).map(|(i,s)|(i,inspect(&s,max,true))).collect::<Vec<_>>()
                }).await;
                if cancel.load(Ordering::Relaxed){break;}
                let finished=sources.is_empty();
                let applied=this.update(cx,|v,cx|{
                    if v.review_generation!=generation{return false;}
                    let Some(r)=v.upload_review.as_mut()else{return false;};
                    r.preview_done+=rows.len();
                    for (i,row) in rows{if let Some(slot)=r.rows.get_mut(i){*slot=row;}}
                    if finished{r.preview_loading=false;r.preview_cancel=None;}
                    cx.notify();true
                }).unwrap_or(false);
                if !applied{break;}
            }
        }));
        cx.notify();
    }
    fn page_review(&mut self,forward:bool,cx:&mut Context<Self>){
        let Some(r)=self.upload_review.as_mut()else{return;};
        if r.loading||r.rows.is_empty(){return;}
        let page=r.selected/PAGE_SIZE;
        let next=if forward{page+1}else{page.saturating_sub(1)};
        if next==page||next*PAGE_SIZE>=r.rows.len(){return;}
        r.selected=next*PAGE_SIZE;
        self.load_review_page(cx);
    }
    #[cfg(all(test,feature="qa"))]
    pub(super) fn review_snapshot(&self)->Option<(usize,usize,usize,bool)>{
        self.upload_review.as_ref().map(|r|(r.rows.len(),r.selected,r.rows.iter().filter(|row|row.picture.is_some()).count(),r.preview_loading))
    }
    pub(super) fn cancel_upload_review(&mut self,cx:&mut Context<Self>){
        self.review_generation=self.review_generation.wrapping_add(1);self.review_task=None;self.stop_review_preview(cx);
        if let Some(mut review)=self.upload_review.take(){
            let images=review.rows.iter_mut().filter_map(|r|r.picture.take()).collect();
            image_cache::release_images(images,cx);
        }
        cx.notify();
    }
    pub(super) fn confirm_upload_review(&mut self,cx:&mut Context<Self>){
        if self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.read(cx).busy||self.account_busy(cx)||self.resetting{return;}
        let Some(review)=self.upload_review.as_ref()else{return;};
        if review.loading||!review.rows.iter().any(|r|r.valid){return;}
        if review.scope!=(self.config.provider().to_owned(),self.config.owner_id().to_owned()){
            self.cancel_upload_review(cx);self.status="账号已变化，请重新选择文件".into();return;
        }
        self.review_generation=self.review_generation.wrapping_add(1);self.stop_review_preview(cx);
        let mut review=self.upload_review.take().unwrap();
        let valid=review.rows.iter().map(|r|r.valid).collect::<Vec<_>>();
        let sources=std::mem::take(&mut review.sources).into_iter().zip(valid).filter_map(|(s,v)|v.then_some(s)).collect();
        image_cache::release_images(review.rows.iter_mut().filter_map(|r|r.picture.take()).collect(),cx);
        self.prepare_queue(Prepare::Enqueue(sources),cx);
    }
    pub(super) fn review_loading(&self)->bool{self.upload_review.as_ref().is_some_and(|r|r.loading)}
    fn remove_review_row(&mut self,index:usize,cx:&mut Context<Self>){
        let Some(r)=self.upload_review.as_mut()else{return;};
        if r.loading||index>=r.rows.len(){return;}
        r.sources.remove(index);
        let row=r.rows.remove(index);
        image_cache::release_images(row.picture.into_iter().collect(),cx);
        r.selected=if index<r.selected{r.selected-1}else{r.selected.min(r.rows.len().saturating_sub(1))};
        self.load_review_page(cx);
        cx.notify();
    }
    pub(super) fn upload_review_panel(&self,wide:bool,cx:&mut Context<Self>)->AnyElement{
        let Some(review)=&self.upload_review else{return div().into_any_element();};
        let p=self.palette();
        let ready=review.rows.iter().filter(|r|r.valid).count();
        let bytes=review.rows.iter().filter(|r|r.valid).map(|r|r.bytes).fold(0u64,u64::saturating_add);
        let page=review.selected/PAGE_SIZE;
        let start=page*PAGE_SIZE;let end=(start+PAGE_SIZE).min(review.rows.len());
        let pages=review.rows.len().div_ceil(PAGE_SIZE).max(1);
        let mut rows=div().id(("review-scroll",page)).w_full().h_full().min_h(px(0.)).flex().flex_col().gap_1();
        for (i,r) in review.rows.iter().enumerate().skip(start).take(PAGE_SIZE){
            let image=div().size(px(42.)).flex_shrink_0().rounded(px(5.)).overflow_hidden().bg(p.subtle).flex().items_center().justify_center()
                .child(if let Some(image)=&r.picture{img(image.clone()).size_full().object_fit(ObjectFit::Contain).into_any_element()}else{div().text_color(p.muted).child(IconName::File).into_any_element()});
            rows=rows.child(div().id(("review-row",i)).debug_selector(move||format!("review-row-{i}")).flex().flex_shrink_0().items_center().gap_2().px_2().py_1().rounded(px(6.))
                .bg(if review.selected==i{p.accent_soft}else{p.canvas}).hover(|d|d.bg(p.surface))
                .on_click(cx.listener(move|v,_,_,cx|{if let Some(r)=v.upload_review.as_mut(){if i<r.rows.len(){r.selected=i;cx.notify();}}}))
                .child(image).child(div().flex_1().min_w(px(0.)).flex().flex_col().gap_1()
                    .child(div().font_medium().truncate().child(r.name.clone()))
                    .child(div().text_xs().text_color(p.muted).child(file_size(r.bytes as i64)))
                    .when(r.warning.is_some(),|d|d.child(div().text_xs().text_color(if r.valid{p.muted}else{p.error}).child(r.warning.clone().unwrap_or_default()))))
                .child(div().text_xs().text_color(if r.valid{p.accent}else{p.error}).child(if r.valid{"待确认"}else{"无效"}))
                .child(div().debug_selector(move||format!("review-remove-{i}")).child(Button::new(("review-remove",i)).xsmall().ghost().icon(IconName::Minus).tooltip("从本批移除").disabled(review.loading).on_click(cx.listener(move|v,_,_,cx|{cx.stop_propagation();v.remove_review_row(i,cx)})))));
        }
        let selected=review.rows.get(review.selected);
        let preview_panel=div().flex().flex_col().gap_2().min_w(px(0.)).rounded(px(7.)).border_1().border_color(p.line).bg(p.surface).p_3()
            .when(wide,|d|d.w(px(300.)).flex_shrink_0()).when(!wide,|d|d.w_full())
            .child(div().flex().justify_between().text_xs().text_color(p.muted).child("本地预览").child(format!("{} / {}",if selected.is_some(){review.selected+1}else{0},review.rows.len())))
            .child(div().id("review-preview").debug_selector(||"review-preview".into()).w_full().min_h(px(0.)).when(wide,|d|d.flex_1()).when(!wide,|d|d.h(px(84.))).bg(p.stage).rounded(px(5.)).overflow_hidden().flex().items_center().justify_center()
                .child(if let Some(image)=selected.and_then(|r|r.picture.as_ref()){img(image.clone()).size_full().object_fit(ObjectFit::Contain).into_any_element()}else{div().text_xs().text_color(p.muted).child(if review.preview_loading{"正在读取本页预览…"}else{"无可用预览"}).into_any_element()}))
            .when(selected.is_some(),|d|d.child(div().text_sm().font_medium().truncate().child(selected.unwrap().name.clone())).child(div().text_xs().text_color(p.muted).child(format!("{} · 仅用于上传前确认",file_size(selected.unwrap().bytes as i64)))));
        let list=div().id("review-list-viewport").debug_selector(||"review-list-viewport".into()).relative().flex_1().min_h(px(0.)).min_w(px(0.)).overflow_hidden()
            .child(div().absolute().top_0().bottom_0().left_0().right_0().overflow_hidden().child(rows.overflow_y_scrollbar().id(("review-scrollbar",page))));
        let body=if wide{div().flex_1().min_h(px(0.)).overflow_hidden().flex().gap_3().child(list).child(preview_panel)}
            else{div().flex_1().min_h(px(0.)).overflow_hidden().flex().flex_col().gap_2().child(preview_panel.flex_shrink_0()).child(list)};
        div().id("upload-review").debug_selector(||"upload-review".into()).flex_1().min_h(px(0.)).flex().flex_col().gap_3()
            .drag_over::<ExternalPaths>(move|style,_,_,_|style.bg(p.accent_soft))
            .on_drop(cx.listener(|v,paths:&ExternalPaths,_,cx|v.review_files(paths.paths().to_vec(),cx)))
            .child(div().flex().items_center().gap_2().child(div().flex_1().min_w(px(0.)).text_lg().font_semibold().child("确认上传"))
                .child(Button::new("review-add").small().icon(IconName::Plus).label("添加").disabled(review.loading||self.picker_pending).on_click(cx.listener(|v,_,_,cx|v.select_files(cx))))
                .child(Button::new("review-cancel").small().ghost().label("取消").on_click(cx.listener(|v,_,_,cx|v.cancel_upload_review(cx)))))
            .child(div().text_xs().text_color(p.muted).child(if review.scope.0=="demo"{"本地演示 · 不会发送至语雀".into()}else{format!("语雀账号 {} · 确认后才发送",review.scope.1)}))
            .child(div().flex().items_center().justify_between().gap_2().child(div().text_sm().font_medium().child(format!("{} 项可上传 / {} 项 · {}",ready,review.rows.len(),file_size(bytes.min(i64::MAX as u64) as i64))))
                .child(div().flex().items_center().gap_2().text_xs().text_color(p.muted)
                    .when(wide&&!review.loading&&!review.rows.is_empty(),|d|d.child(div().w(px(64.)).h(px(3.)).rounded_full().bg(p.subtle).child(div().h_full().rounded_full().bg(p.accent).w(relative((review.preview_done as f32/(end-start).max(1) as f32).clamp(0.,1.))))))
                    .child(if review.loading{"检查中…".into()}else if review.rows.is_empty(){"可继续拖入文件".into()}else{format!("本页预览 {} / {}",review.preview_done,end-start)})))
            .when(pages>1,|d|d.child(div().flex_shrink_0().flex().items_center().gap_2().text_xs().text_color(p.muted)
                .child(div().flex_1().min_w(px(0.)).truncate().child(format!("第 {}–{} 项 / {} · {}",start+1,end,review.rows.len(),if review.preview_loading{"读取预览中"}else{"按页预览"})))
                .child(div().debug_selector(||"review-prev-page".into()).child(Button::new("review-prev-page").xsmall().ghost().icon(IconName::ChevronLeft).tooltip("上一页").disabled(review.loading||page==0).on_click(cx.listener(|v,_,_,cx|v.page_review(false,cx)))))
                .child(format!("{} / {}",page+1,pages))
                .child(div().debug_selector(||"review-next-page".into()).child(Button::new("review-next-page").xsmall().ghost().icon(IconName::ChevronRight).tooltip("下一页").disabled(review.loading||page+1>=pages).on_click(cx.listener(|v,_,_,cx|v.page_review(true,cx)))))))
            .child(body)
            .child(div().id("review-footer").occlude().bg(p.canvas).flex_shrink_0().flex().items_center().justify_end().gap_2().border_t_1().border_color(p.line).pt_3()
                .child(div().flex_1().min_w(px(0.)).text_xs().text_color(p.muted).child(if pages>1{"确认全部页面；无效项跳过。"}else{"不修改原文件；无效项目会跳过。"}))
                .child(div().debug_selector(||"review-confirm".into()).child(Button::new("review-confirm").small().primary().label(format!("确认上传 {ready} 项")).disabled(review.loading||ready==0||self.queue.read(cx).busy||self.account_busy(cx)||self.picker_pending).on_click(cx.listener(|v,_,_,cx|v.confirm_upload_review(cx))))))
            .into_any_element()
    }
}
