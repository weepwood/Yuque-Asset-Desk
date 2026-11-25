//! UI-only preflight. No network or SQLite writes before the user confirms.
use super::*;
use super::queue_prepare::{Prepare,Source};
use gpui_kit::component::{IconName,Sizable};
use std::io::Read;

const PREVIEW_LIMIT:usize=24;
const INPUT_LIMIT:u64=32*1024*1024;

pub(super) struct DraftRow{
    pub name:String,pub bytes:u64,pub picture:Option<Arc<RenderImage>>,pub warning:Option<String>,pub valid:bool,
}
pub(super) struct Review{
    scope:(String,String),sources:Vec<Source>,rows:Vec<DraftRow>,loading:bool,
}
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
            (format!("剪贴板.{ext}"),len,(show&&len<=INPUT_LIMIT).then(||bytes.clone()))
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
        if self.upload_review.is_some(){self.status="请先确认或取消当前待上传批次".into();self.navigate_page(Page::Jobs,cx);return;}
        let scope=(self.config.provider().to_owned(),self.config.owner_id().to_owned());
        self.review_generation=self.review_generation.wrapping_add(1);
        let generation=self.review_generation;
        self.upload_review=Some(Review{scope:scope.clone(),sources:vec![],rows:vec![],loading:true});
        let max=self.config.max_file_mb*1024*1024;
        self.navigate_page(Page::Jobs,cx);
        self.review_task=Some(cx.spawn(async move|this,cx|{
            let (sources,rows)=cx.background_spawn(async move{
                let rows=sources.iter().enumerate().map(|(i,s)|inspect(s,max,i<PREVIEW_LIMIT)).collect();
                (sources,rows)
            }).await;
            let _=this.update(cx,|v,cx|{
                if v.review_generation!=generation||v.upload_review.is_none(){return;}
                v.upload_review=Some(Review{scope,sources,rows,loading:false});cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn cancel_upload_review(&mut self,cx:&mut Context<Self>){
        self.review_generation=self.review_generation.wrapping_add(1);self.review_task=None;
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
        let mut review=self.upload_review.take().unwrap();
        let valid=review.rows.iter().map(|r|r.valid).collect::<Vec<_>>();
        let sources=std::mem::take(&mut review.sources).into_iter().zip(valid).filter_map(|(s,v)|v.then_some(s)).collect();
        image_cache::release_images(review.rows.iter_mut().filter_map(|r|r.picture.take()).collect(),cx);
        self.prepare_queue(Prepare::Enqueue(sources),cx);
    }
    pub(super) fn upload_review_panel(&self,cx:&mut Context<Self>)->AnyElement{
        let Some(review)=&self.upload_review else{return div().into_any_element();};
        let p=self.palette();
        let ready=review.rows.iter().filter(|r|r.valid).count();
        let bytes=review.rows.iter().filter(|r|r.valid).map(|r|r.bytes).fold(0u64,u64::saturating_add);
        let mut rows=div().id("review-scroll").flex_1().min_h(px(0.)).flex().flex_col().gap_1();
        for (i,r) in review.rows.iter().take(100).enumerate(){
            let image=div().size(px(54.)).flex_shrink_0().rounded(px(5.)).overflow_hidden().bg(p.subtle).flex().items_center().justify_center()
                .child(if let Some(image)=&r.picture{img(image.clone()).size_full().object_fit(ObjectFit::Contain).into_any_element()}else{div().text_color(p.muted).child(IconName::File).into_any_element()});
            rows=rows.child(div().id(("review-row",i)).flex().items_center().gap_3().py_2().border_b_1().border_color(p.line)
                .child(image).child(div().flex_1().min_w(px(0.)).flex().flex_col().gap_1()
                    .child(div().font_medium().truncate().child(r.name.clone()))
                    .child(div().text_xs().text_color(p.muted).child(file_size(r.bytes as i64)))
                    .when(r.warning.is_some(),|d|d.child(div().text_xs().text_color(if r.valid{p.muted}else{p.error}).child(r.warning.clone().unwrap_or_default()))))
                .child(div().text_xs().text_color(if r.valid{p.accent}else{p.error}).child(if r.valid{"待确认"}else{"无法上传"})));
        }
        div().id("upload-review").debug_selector(||"upload-review".into()).flex_1().min_h(px(0.)).flex().flex_col().gap_3()
            .child(div().flex().items_center().gap_2().child(div().flex_1().min_w(px(0.)).text_lg().font_semibold().child("确认上传"))
                .child(Button::new("review-cancel").small().ghost().label("取消").on_click(cx.listener(|v,_,_,cx|v.cancel_upload_review(cx)))))
            .child(div().text_xs().text_color(p.muted).child(if review.scope.0=="demo"{"本地演示 · 不会发送至语雀".into()}else{format!("语雀账号 {} · 确认后才发送",review.scope.1)}))
            .when(review.loading,|d|d.child(div().text_sm().text_color(p.muted).child("正在检查文件并生成本地预览…")))
            .when(!review.loading,|d|d.child(div().text_sm().child(format!("{} 项可上传 / {} 项 · {}",ready,review.rows.len(),file_size(bytes.min(i64::MAX as u64) as i64)))))
            .when(review.rows.len()>PREVIEW_LIMIT,|d|d.child(div().text_xs().text_color(p.muted).child("仅预览前 24 项，其余文件仍会按队列处理。")))
            .when(review.rows.len()>100,|d|d.child(div().text_xs().text_color(p.muted).child("仅显示前100项；确认数量包含其余有效文件。")))
            .child(rows.overflow_y_scrollbar())
            .child(div().flex_shrink_0().flex().items_center().justify_end().gap_2().border_t_1().border_color(p.line).pt_3()
                .child(div().flex_1().min_w(px(0.)).text_xs().text_color(p.muted).child("不修改原文件；无法上传的项目将跳过。"))
                .child(div().debug_selector(||"review-confirm".into()).child(Button::new("review-confirm").small().primary().label(format!("确认上传 {ready} 项")).disabled(review.loading||ready==0||self.queue.read(cx).busy||self.account_busy(cx)).on_click(cx.listener(|v,_,_,cx|v.confirm_upload_review(cx))))))
            .into_any_element()
    }
}
