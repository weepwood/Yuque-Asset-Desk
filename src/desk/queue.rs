use super::*;
use queue_prepare::{Prepare,Source};

impl UploadDesk {
    pub(super) fn refresh_jobs(&mut self,cx:&mut Context<Self>){
        let storage=self.storage.clone();
        let provider=self.config.provider().to_owned();
        let owner=self.config.owner_id().to_owned();
        self.job_history.update(cx,|v,cx|v.refresh(storage,provider,owner,cx));
    }
    pub(super) fn page_jobs(&mut self,forward:bool,cx:&mut Context<Self>){
        if self.job_history.update(cx,|v,_|v.move_page(forward)){self.refresh_jobs(cx);}
    }
    pub(super) fn first_jobs(&mut self,cx:&mut Context<Self>){
        self.job_history.update(cx,|v,_|v.first());self.refresh_jobs(cx);
    }

    pub(super) fn prepare_queue(&mut self,request:Prepare,cx:&mut Context<Self>){
        if self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.read(cx).busy||self.account_busy(cx)||self.resetting{return;}
        if self.config.mode==Mode::Yuque&&self.config.creds.is_none(){
            self.status="请先配置语雀会话再上传".into();cx.notify();return;
        }
        let storage=self.storage.clone();let config=self.config.clone();
        self.queue.update(cx,|v,cx|v.prepare(request,config,storage,cx));
    }
    pub(super) fn retry_one(&mut self,id:i64,cx:&mut Context<Self>){self.prepare_queue(Prepare::Retry(id),cx);}
    pub(super) fn acknowledge_job(&mut self,id:i64,w:&mut Window,cx:&mut Context<Self>){
        if self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.read(cx).busy||self.account_busy(cx)||self.resetting{return;}
        let Some(job)=self.job_history.read(cx).jobs.iter().find(|j|j.id==id&&matches!(j.state.as_str(),"unknown"|"running"))else{return;};
        let name=job.path.file_name().unwrap_or_default().to_string_lossy();
        let detail=format!("请先在语雀网页端核实「{name}」未上传成功。确认只会将任务改为可手动重试，不会立即上传；如果远端已有文件，重试可能产生重复资源。");
        self.maintenance_busy=true;cx.notify();
        let answer=w.prompt(PromptLevel::Warning,"确认远端没有该文件？",Some(&detail),&["取消","确认核实结果"],cx);
        self.job_ack_task=Some(cx.spawn(async move|this,cx|{
            let accepted=matches!(answer.await,Ok(1));
            let _=this.update(cx,|v,cx|{
                v.maintenance_busy=false;
                if accepted{v.prepare_queue(Prepare::Acknowledge(id),cx);}else{v.status="已取消核实，任务状态未修改".into();}
                cx.notify();
            });
        }));
    }

    pub(super) fn paste_files(&mut self, cx: &mut Context<Self>) {
        if self.resource_write_busy || self.picker_pending || self.backup_busy || self.queue.read(cx).busy || self.account_busy(cx) { return; }
        let Some(item) = cx.read_from_clipboard() else {self.status="剪贴板没有可读取内容".into();cx.notify();return;};
        self.paste_item(item,cx);
    }
    pub(super) fn paste_item(&mut self,item:ClipboardItem,cx:&mut Context<Self>){
        if self.resource_write_busy || self.picker_pending || self.backup_busy || self.queue.read(cx).busy || self.account_busy(cx) {return;}
        let mut sources=Vec::new();
        for entry in item.entries(){
            match entry{
                ClipboardEntry::ExternalPaths(paths)=>sources.extend(paths.paths().iter().cloned().map(Source::Path)),
                ClipboardEntry::Image(image) if image.bytes().len() as u64<=self.config.max_file_mb*1024*1024=>{
                    sources.push(Source::Image(image.bytes().to_vec(),image.format().extension().to_owned()));
                }
                _=>{}
            }
        }
        if sources.is_empty(){self.status="剪贴板没有可上传的文件或图片".into();cx.notify();}
        else{self.review_sources(sources,cx);}
    }

    pub(super) fn resume_jobs(&mut self,cx:&mut Context<Self>){self.prepare_queue(Prepare::Resume,cx);}

    pub(super) fn select_files(&mut self, cx: &mut Context<Self>) {
        if self.upload_review.is_some(){self.navigate_page(Page::Jobs,cx);return;}
        if self.resource_write_busy || self.picker_pending || self.backup_busy || self.queue.read(cx).busy || self.account_busy(cx) {
            return;
        }
        if self.config.mode == Mode::Yuque && self.config.creds.is_none() {
            self.status = "请先配置语雀会话再上传".into();cx.notify();return;
        }
        self.picker_pending=true;
        self.status="请选择图片或附件…".into();cx.notify();
        let selection=cx.prompt_for_paths(PathPromptOptions{files:true,directories:false,multiple:true,prompt:Some("选择文件".into())});
        self.picker_task=Some(cx.spawn(async move|this,cx|{
            let result=selection.await;
            let _=this.update(cx,|v,cx|{
                v.picker_pending=false;
                match result{
                    Ok(Ok(Some(files))) if !files.is_empty()=>v.review_files(files,cx),
                    Ok(Ok(_))=>v.status="已取消文件选择".into(),
                    Ok(Err(_))|Err(_)=>v.status="无法打开文件选择窗口，请重试，或将文件拖入相册".into(),
                }
                cx.notify();
            });
        }));
    }

    #[cfg(all(test,feature="qa"))]
    pub(super) fn start_upload(&mut self, files: Vec<PathBuf>, cx: &mut Context<Self>) {
        if files.is_empty(){return;}
        if self.resource_write_busy || self.picker_pending || self.backup_busy || self.queue.read(cx).busy || self.account_busy(cx) {
            self.status="当前任务处理中，本次拖入的文件未加入队列，请稍后重试".into();cx.notify();return;
        }
        if self.config.mode == Mode::Yuque && self.config.creds.is_none() {
            self.status = "请先配置语雀会话再上传".into(); cx.notify(); return;
        }
        self.prepare_queue(Prepare::Enqueue(files.into_iter().map(Source::Path).collect()),cx);
    }

}
