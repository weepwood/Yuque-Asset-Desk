//! Local thumbnail recovery lane. Query replacement cancels work; pagination does not.
use super::*;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

pub(super) type Thumbnail = (i64,PathBuf,f32);
pub(super) struct ThumbnailBatch{pub generation:u64,pub found:Vec<Thumbnail>}
impl EventEmitter<ThumbnailBatch> for ThumbnailModel {}

#[derive(Default)]
pub(super) struct ThumbnailModel{
    pub busy:bool,
    attempted:HashSet<i64>,
    generation:u64,
    cancel:Arc<AtomicBool>,
    task:Option<Task<()>>,
}
impl Drop for ThumbnailModel{
    fn drop(&mut self){self.cancel.store(true,Ordering::Release);}
}
impl ThumbnailModel{
    pub fn current(&self,generation:u64)->bool{self.generation==generation}
    pub fn attempted(&self,id:i64)->bool{self.attempted.contains(&id)}
    pub fn stop(&mut self,reset:bool){
        self.cancel.store(true,Ordering::Release);
        self.generation=self.generation.wrapping_add(1);
        self.task=None;
        self.busy=false;
        if reset{self.attempted.clear();}
    }
    pub fn request(&mut self,storage:Storage,items:Vec<Asset>,cx:&mut Context<Self>){
        if self.busy||items.is_empty(){return;}
        self.cancel=Arc::new(AtomicBool::new(false));
        self.generation=self.generation.wrapping_add(1);
        self.busy=true;
        let generation=self.generation;
        let cancel=self.cancel.clone();
        self.task=Some(cx.spawn(async move|this,cx|{
            let (attempted,found)=cx.background_spawn(async move{
                let mut attempted=Vec::new();
                let mut found=Vec::new();
                for a in items{
                    if cancel.load(Ordering::Acquire){break;}
                    let result=preview::index_local(&storage,&a);
                    if cancel.load(Ordering::Acquire){break;}
                    attempted.push(a.id);
                    if let Ok((p,r))=result{found.push((a.id,p,r));}
                }
                (attempted,found)
            }).await;
            let _=this.update(cx,|v,cx|{
                if v.generation!=generation{return;}
                v.busy=false;
                v.attempted.extend(attempted);
                cx.emit(ThumbnailBatch{generation,found});
            });
        }));
    }
}
