//! Durable, serial preview lane independent of upload transfer and gallery viewport.
use super::*;
#[derive(Default)]
pub(super) struct PreviewWorker{pub busy:bool,wake:bool,task:Option<Task<()>>}
pub(super) enum PreviewEvent{Completed(preview::PreviewCompletion),Error(String),Scheduled(usize),RepairFinished{processed:usize,failed:usize}}
impl EventEmitter<PreviewEvent> for PreviewWorker{}
impl PreviewWorker{
    pub fn kick(&mut self,storage:Storage,cx:&mut Context<Self>){
        if self.busy{self.wake=true;return;}
        self.start(storage,None,cx);
    }
    pub fn repair(&mut self,storage:Storage,provider:String,owner:String,ids:Vec<i64>,cx:&mut Context<Self>){
        if self.busy||ids.is_empty(){return;}
        self.start(storage,Some((provider,owner,ids)),cx);
    }
    fn start(&mut self,storage:Storage,repair:Option<(String,String,Vec<i64>)>,cx:&mut Context<Self>){
        self.busy=true;self.wake=false;cx.notify();
        self.task=Some(cx.spawn(async move|this,cx|{
            let explicit=repair.is_some();
            let (mut processed,mut failed)=(0usize,0usize);
            if let Some((provider,owner,ids))=repair{
                let db=storage.clone();
                let result=cx.background_spawn(async move{db.schedule_previews(&provider,&owner,&ids)}).await;
                let ok=result.is_ok();
                let _=this.update(cx,|v,cx|match result{
                    Ok(n)=>cx.emit(PreviewEvent::Scheduled(n)),
                    Err(e)=>{v.busy=false;cx.emit(PreviewEvent::Error(e.to_string()));cx.notify();}
                });
                if !ok{return;}
            }
            loop{
                let db=storage.clone();
                let result=cx.background_spawn(async move{preview::process_next(&db)}).await;
                let again=this.update(cx,|v,cx|{
                    match result{
                        Ok(Some(item))=>{if item.settled{processed+=1;if item.found.is_none(){failed+=1;}}cx.emit(PreviewEvent::Completed(item));true},
                        Ok(None) if v.wake=>{v.wake=false;true},
                        Ok(None)=>{v.busy=false;if explicit{cx.emit(PreviewEvent::RepairFinished{processed,failed});}cx.notify();false},
                        Err(e)=>{v.busy=false;v.wake=false;cx.emit(PreviewEvent::Error(e.to_string()));cx.notify();false}
                    }
                }).unwrap_or(false);
                if !again{break;}
            }
        }));
    }
}
