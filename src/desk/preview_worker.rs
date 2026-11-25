//! Durable, serial preview lane independent of upload transfer and gallery viewport.
use super::*;
use std::sync::atomic::AtomicBool;
#[derive(Default)]
pub(super) struct PreviewWorker{
    pub busy:bool,wake:bool,task:Option<Task<()>>,
    pub audit:Option<((String,String),preview::AuditReport)>,
    pub audit_running:bool,cancel:Option<Arc<AtomicBool>>,
}
pub(super) enum PreviewEvent{
    Completed(preview::PreviewCompletion),Error(String),Scheduled(usize),
    RepairFinished{processed:usize,failed:usize},Idle,
}
impl EventEmitter<PreviewEvent> for PreviewWorker{}
impl PreviewWorker{
    pub fn kick(&mut self,storage:Storage,cx:&mut Context<Self>){
        if self.busy{self.wake=true;return;}
        self.start(storage,None,None,cx);
    }
    pub fn repair(&mut self,storage:Storage,provider:String,owner:String,ids:Vec<i64>,cx:&mut Context<Self>){
        if self.busy||ids.is_empty(){return;}
        self.start(storage,Some((provider,owner,ids)),None,cx);
    }
    pub fn scan(&mut self,storage:Storage,provider:String,owner:String,repair:bool,cx:&mut Context<Self>){
        if self.busy{return;}
        let cancel=Arc::new(AtomicBool::new(false));
        self.audit=Some(((provider.clone(),owner.clone()),preview::AuditReport::default()));
        self.audit_running=true;self.cancel=Some(cancel.clone());
        self.start(storage,None,Some((provider,owner,repair,cancel)),cx);
    }
    pub fn stop_scan(&mut self,cx:&mut Context<Self>){
        if let Some(cancel)=&self.cancel{cancel.store(true,Ordering::Relaxed);}
        cx.notify();
    }
    fn start(&mut self,storage:Storage,repair:Option<(String,String,Vec<i64>)>,scan:Option<(String,String,bool,Arc<AtomicBool>)>,cx:&mut Context<Self>){
        self.busy=true;self.wake=false;cx.notify();
        self.task=Some(cx.spawn(async move|this,cx|{
            let explicit=repair.is_some()||scan.as_ref().is_some_and(|s|s.2);
            let (mut processed,mut failed)=(0usize,0usize);
            let mut scan=scan;
            let mut cursor=0;
            let mut upper=0;
            let mut report=preview::AuditReport::default();
            if let Some((provider,owner,_,_))=&scan{
                let db=storage.clone();let p=provider.clone();let o=owner.clone();
                match cx.background_spawn(async move{db.preview_audit_bounds(&p,&o)}).await{
                    Ok((max,total))=>{upper=max;report.total=total;},
                    Err(e)=>{let _=this.update(cx,|v,cx|{v.busy=false;v.audit_running=false;v.cancel=None;if let Some((_,r))=&mut v.audit{r.interrupted=true;}cx.emit(PreviewEvent::Error(e.to_string()));cx.notify();});return;}
                }
            }
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
                if let Some((provider,owner,repair,cancel))=&scan{
                    let db=storage.clone();let p=provider.clone();let o=owner.clone();let c=cancel.clone();let repair=*repair;
                    let previous=report.clone();
                    match cx.background_spawn(async move{preview::audit_page(&db,&p,&o,cursor,upper,previous,&c,repair)}).await{
                        Ok((next,updated,done))=>{
                            cursor=next;report=updated;
                            let scope=(provider.clone(),owner.clone());
                            let _=this.update(cx,|v,cx|{v.audit=Some((scope,report.clone()));if done{v.audit_running=false;v.cancel=None;}cx.notify();});
                            if done{scan=None;}
                        }
                        Err(e)=>{
                            let _=this.update(cx,|v,cx|{v.audit_running=false;v.cancel=None;if let Some((_,r))=&mut v.audit{r.interrupted=true;}cx.emit(PreviewEvent::Error(e.to_string()));cx.notify();});
                            scan=None;
                        }
                    }
                }
                // Alternate bounded scan pages with durable work. A full audit
                // cannot starve previews committed by a concurrent upload.
                let mut empty=false;
                for _ in 0..64{
                    let db=storage.clone();
                    let result=cx.background_spawn(async move{preview::process_next(&db)}).await;
                    let again=this.update(cx,|v,cx|match result{
                        Ok(Some(item))=>{if item.settled{processed+=1;if item.found.is_none(){failed+=1;}}cx.emit(PreviewEvent::Completed(item));true},
                        Ok(None)=>{empty=true;false},
                        Err(e)=>{v.busy=false;v.wake=false;if v.audit_running{if let Some((_,r))=&mut v.audit{r.interrupted=true;}}v.audit_running=false;v.cancel=None;cx.emit(PreviewEvent::Error(e.to_string()));cx.notify();false}
                    }).unwrap_or(false);
                    if !again{if !empty{return;}break;}
                }
                if scan.is_some(){continue;}
                if empty{
                    let again=this.update(cx,|v,cx|{
                        if v.wake{v.wake=false;true}else{v.busy=false;cx.emit(PreviewEvent::Idle);if explicit{cx.emit(PreviewEvent::RepairFinished{processed,failed});}cx.notify();false}
                    }).unwrap_or(false);
                    if !again{break;}
                }
            }
        }));
    }
}
