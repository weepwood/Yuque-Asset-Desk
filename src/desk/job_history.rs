//! Account-scoped task history and pagination. Owns the asynchronous read lifecycle.
use super::*;
pub(super) struct JobHistory{
    pub jobs:Vec<Job>,
    pub cursors:Vec<Option<i64>>,
    pub more:bool,
    pub loading:bool,
    pub error:Option<String>,
    pub recovery_count:usize,
    scope:Option<(String,String)>,
    generation:u64,
    task:Option<Task<()>>,
}
impl Default for JobHistory{
    fn default()->Self{Self{jobs:vec![],cursors:vec![None],more:false,loading:false,error:None,recovery_count:0,scope:None,generation:0,task:None}}
}
impl JobHistory{
    pub fn first(&mut self){self.cursors=vec![None];self.jobs.clear();self.more=false;}
    pub fn move_page(&mut self,forward:bool)->bool{
        if self.loading{return false;}
        if forward{
            if !self.more{return false;}
            let Some(last)=self.jobs.last()else{return false;};
            self.cursors.push(Some(last.id));
        }else{
            if self.cursors.len()<=1{return false;}
            self.cursors.pop();
        }
        self.jobs.clear();self.more=false;true
    }
    pub fn refresh(&mut self,storage:Storage,provider:String,owner:String,cx:&mut Context<Self>){
        if self.scope.as_ref().is_none_or(|(p,o)|p!=&provider||o!=&owner){
            self.first();self.recovery_count=0;self.error=None;
            self.scope=Some((provider.clone(),owner.clone()));
        }
        let before=self.cursors.last().copied().flatten();
        self.loading=true;self.generation=self.generation.wrapping_add(1);
        let generation=self.generation;
        self.task=Some(cx.spawn(async move|this,cx|{
            let result=cx.background_spawn(async move{
                Ok::<_,anyhow::Error>((storage.jobs(&provider,&owner,before)?,storage.recoverable_count(&provider,&owner)?))
            }).await;
            let _=this.update(cx,|v,cx|{
                if generation!=v.generation{return;}
                v.loading=false;
                match result{
                    Ok((mut jobs,count))=>{v.recovery_count=count;v.more=jobs.len()>50;jobs.truncate(50);v.jobs=jobs;v.error=None;}
                    Err(e)=>{v.error=Some(e.to_string());v.jobs.clear();v.more=false;}
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

#[cfg(all(test,feature="qa"))]
#[::core::prelude::v1::test]
fn scope_change_rejects_previous_page_and_preserves_pagination(){
    use qa_gpui::TestAppContext;
    let root=std::env::temp_dir().join(format!("yuque-job-history-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    db.enqueue("demo","local",&[root.join("old.txt")]).unwrap();
    let paths=(0..51).map(|i|root.join(format!("new-{i}.txt"))).collect::<Vec<_>>();
    db.enqueue("yuque","123456",&paths).unwrap();
    let app=TestAppContext::single();
    let model=app.update(|cx|cx.new(|_|JobHistory::default()));
    app.update(|cx|model.update(cx,|v,cx|{
        v.refresh(db.clone(),"demo".into(),"local".into(),cx);
        v.refresh(db.clone(),"yuque".into(),"123456".into(),cx);
    }));
    app.run_until_parked();
    app.read(|cx|{let v=model.read(cx);assert_eq!(v.recovery_count,51);assert_eq!(v.jobs.len(),50);assert!(v.more);assert!(v.jobs.iter().all(|j|j.path.file_name().unwrap().to_string_lossy().starts_with("new-")));});
    app.update(|cx|model.update(cx,|v,cx|{assert!(v.move_page(true));v.refresh(db.clone(),"yuque".into(),"123456".into(),cx);}));
    app.run_until_parked();
    app.read(|cx|{let v=model.read(cx);assert_eq!(v.jobs.len(),1);assert!(!v.more);assert_eq!(v.cursors.len(),2);});
    app.update(|cx|model.update(cx,|v,cx|v.refresh(db.clone(),"demo".into(),"local".into(),cx)));
    app.run_until_parked();
    app.read(|cx|{let v=model.read(cx);assert_eq!(v.recovery_count,1);assert_eq!(v.cursors,vec![None]);assert_eq!(v.jobs[0].path,root.join("old.txt"));});
    drop(model);drop(app);drop(db);let _=std::fs::remove_dir_all(root);
}
