//! One retained write lane for user-initiated resource changes.
//! The shell must refuse account switches, backups, cleanup and quit while busy.
use super::*;
pub(super) enum Mutation {
    Favorite(Asset),
    Tags(i64,String),
    Batch(Vec<i64>,Option<(String,String)>,String,bool),
}
pub(super) enum Outcome { Favorite(i64,bool), Tags, Batch(usize) }
impl Mutation {
    fn run(self,storage:&Storage)->anyhow::Result<Outcome>{
        match self {
            Self::Favorite(a)=>Ok(Outcome::Favorite(a.id,storage.toggle_favorite(&a)?)),
            Self::Tags(id,text)=>{storage.set_tags(id,&text)?;Ok(Outcome::Tags)},
            Self::Batch(ids,scope,text,remove)=>{
                storage.batch_tags(&ids,scope.as_ref().map(|(p,o)|(p.as_str(),o.as_str())),&text,remove)?;
                Ok(Outcome::Batch(ids.len()))
            }
        }
    }
}
impl UploadDesk{
    pub(super) fn mutate_resource(&mut self,mutation:Mutation,cx:&mut Context<Self>){
        if self.resource_write_busy||self.account.account_busy||self.resetting||self.picker_pending||self.backup_busy{
            self.status="已有操作正在处理，请稍后重试".into();cx.notify();return;
        }
        self.resource_write_busy=true;
        self.status="正在保存资源修改…".into();
        let storage=self.storage.clone();
        self.resource_write_task=Some(cx.spawn(async move|this,cx|{
            let result=cx.background_spawn(async move{mutation.run(&storage)}).await;
            let _=this.update(cx,|v,cx|{
                v.resource_write_busy=false;
                match result {
                    Ok(Outcome::Favorite(id,starred))=>{
                        if starred{v.resource.favorites.insert(id);}else{v.resource.favorites.remove(&id);}
                        v.status=if starred{"已收藏"}else{"已取消收藏"}.into();
                        if v.resource.favorites_only{v.reload(cx);}
                    }
                    Ok(Outcome::Tags)=>{v.status="标签已保存".into();v.reload(cx);}
                    Ok(Outcome::Batch(n))=>{v.status=format!("已更新 {n} 项资源标签");v.reload(cx);}
                    Err(_)=>v.status="资源修改失败，请重试".into(),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
