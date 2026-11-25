//! Retained high-quality viewer lifecycle. Owns the request and RenderImage.
//! Gallery remains the authority for the current filtered resource and low-res cache.
use super::*;
#[derive(Default)]
pub(super) struct ViewerModel{
    pub quality:Option<crate::high_preview::QualityImage>,
    pub busy:bool,
    pub status:String,
    pub revision:u64,
    generation:u64,
    asset:Option<i64>,
    task:Option<Task<()>>,
}
impl ViewerModel{
    pub fn clear(&mut self,cx:&mut Context<Self>){
        self.generation=self.generation.wrapping_add(1);
        self.task=None;self.asset=None;self.busy=false;self.status.clear();
        if let Some(q)=self.quality.take(){cx.drop_image(q.image,None);}
        cx.notify();
    }
    pub fn feedback(&mut self,status:String,cx:&mut Context<Self>){self.status=status;cx.notify();}
    pub fn load(&mut self,asset:Asset,storage:Storage,remote:bool,cx:&mut Context<Self>){
        if self.busy{return;}
        if self.asset!=Some(asset.id){self.clear(cx);}
        if self.quality.as_ref().is_some_and(|q|q.remote==remote){return;}
        self.generation=self.generation.wrapping_add(1);
        self.task=None;self.asset=Some(asset.id);self.busy=true;
        self.status=if remote{"正在读取在线原图…".into()}else{"正在读取本地原图…".into()};
        let generation=self.generation;let id=asset.id;
        self.task=Some(cx.spawn(async move|this,cx|{
            let result=cx.background_spawn(async move{crate::high_preview::load(&storage,&asset,remote)}).await;
            let _=this.update(cx,|v,cx|{
                if v.asset!=Some(id)||v.generation!=generation{return;}
                v.busy=false;
                match result{
                    Ok(q)=>{
                        v.status.clear();
                        if let Some(old)=v.quality.replace(q){cx.drop_image(old.image,None);}
                        v.revision=v.revision.wrapping_add(1);
                    }
                    Err(e)=>v.status=e.to_string(),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
