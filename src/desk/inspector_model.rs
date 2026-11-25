//! Retained inspector metadata, owned separately from the shell and gallery.
use super::*;
pub(super) struct InspectorModel {
    pub input:Entity<InputState>,
    pub ready:bool,
    asset:Option<i64>,
    generation:u64,
    task:Option<Task<()>>,
}
impl InspectorModel{
    pub fn new(window:&mut Window,cx:&mut Context<Self>)->Self{
        Self{input:cx.new(|cx|InputState::new(window,cx).placeholder("标签，用逗号分隔")),ready:false,asset:None,generation:0,task:None}
    }
    pub fn reset(&mut self,cx:&mut Context<Self>){
        self.asset=None;self.ready=false;self.generation=self.generation.wrapping_add(1);self.task=None;cx.notify();
    }
    pub fn select(&mut self,id:i64,storage:Storage,window:&mut Window,cx:&mut Context<Self>){
        self.reset(cx);self.asset=Some(id);
        self.input.update(cx,|input,cx|input.set_value("",window,cx));
        let generation=self.generation;
        self.task=Some(cx.spawn_in(window,async move|this,cx|{
            let result=cx.background_spawn(async move{storage.tags(id)}).await;
            let _=this.update_in(cx,|v,w,cx|{
                if v.asset!=Some(id)||v.generation!=generation{return;}
                if let Ok(tags)=result{v.input.update(cx,|input,cx|input.set_value(tags.join(", "),w,cx));v.ready=true;}
                cx.notify();
            });
        }));
        cx.notify();
    }
}
