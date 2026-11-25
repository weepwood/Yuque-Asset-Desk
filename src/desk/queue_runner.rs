//! Owns serial queue execution and progress; emits typed shell events.
use super::*;
use queue_prepare::{Prepare,Prepared};
pub(super) enum QueueEvent { Message(String),Started,Refresh,ResourceChanged,Authorization,Finished(String) }
pub(super) struct QueueRunner {
    pub(super) queue_control: Arc<AtomicU8>,
    pub(super) transfer: Arc<uploader::TransferProgress>,
    pub(super) transfer_task: Option<Task<()>>,
    pub(super) active_job: Option<i64>,
    pub(super) busy: bool,
    pub(super) queue_count: usize,
    pub(super) queue_total: usize,
    pub(super) succeeded: usize,
    pub(super) duplicates: usize,
    pub(super) failed: Vec<(PathBuf, String)>,
    pub(super) upload_task: Option<Task<()>>,
    pub(super) preparing:bool,
    pub(super) prepare_task:Option<Task<()>>,
}

impl EventEmitter<QueueEvent> for QueueRunner {}
impl Default for QueueRunner{fn default()->Self{Self{
                queue_control: Arc::new(AtomicU8::new(0)),
                transfer: Arc::new(uploader::TransferProgress::default()),
                transfer_task: None,
                active_job: None,
                busy: false,
                queue_count: 0,
                queue_total: 0,
                succeeded: 0,
                duplicates: 0,
                failed: Vec::new(),
                upload_task: None,
                preparing:false,prepare_task:None,
            }}}
impl QueueRunner{
    pub fn prepare(&mut self,request:Prepare,config:Config,storage:Storage,cx:&mut Context<Self>){
        if self.busy{return;}
        let provider=config.provider().to_owned();let owner=config.owner_id().to_owned();
        self.busy=true;self.preparing=true;self.queue_count=0;self.queue_total=0;
        cx.emit(QueueEvent::Message("正在准备并保存任务…".into()));cx.notify();
        let db=storage.clone();
        self.prepare_task=Some(cx.spawn(async move|this,cx|{
            let result=cx.background_spawn(async move{request.execute(&db,&provider,&owner)}).await;
            let _=this.update(cx,|v,cx|{
                v.preparing=false;v.busy=false;
                match result{
                    Ok(Prepared::Jobs(jobs)) if !jobs.is_empty()=>v.run_jobs(jobs,config,storage,cx),
                    Ok(Prepared::Jobs(_))=>{cx.emit(QueueEvent::Message("没有可恢复或可上传的任务".into()));cx.emit(QueueEvent::Refresh);},
                    Ok(Prepared::Acknowledged)=>{cx.emit(QueueEvent::Message("已记录核实结果；如需重新上传，请单独重试".into()));cx.emit(QueueEvent::Refresh);},
                    Err(e)=>{cx.emit(QueueEvent::Message(format!("任务准备失败：{e}")));cx.emit(QueueEvent::Refresh);},
                }
                cx.notify();
            });
        }));
    }
    pub fn reset_summary(&mut self,cx:&mut Context<Self>){
        self.failed.clear();self.queue_total=0;self.queue_count=0;self.succeeded=0;self.duplicates=0;cx.notify();
    }
    pub fn command(&mut self,command:u8,cx:&mut Context<Self>){
        if !self.busy||self.preparing{return;}
        match command{
            1=>{let _=self.queue_control.compare_exchange(0,1,Ordering::Relaxed,Ordering::Relaxed);}
            2=>self.queue_control.store(2,Ordering::Relaxed),
            3 if self.active_job.is_some()=>{self.queue_control.store(1,Ordering::Relaxed);self.transfer.cancel.store(true,Ordering::Relaxed);cx.emit(QueueEvent::Message("已请求中止，远端结果可能未知".into()));}
            _=>{}
        }
        cx.notify();
    }
    fn run_jobs(&mut self,jobs:Vec<(i64,PathBuf)>,config:Config,storage:Storage,cx:&mut Context<Self>) {
        if jobs.is_empty(){return;}
        let control=Arc::new(AtomicU8::new(0));
        self.queue_control=control.clone();
        self.busy = true;
        cx.emit(QueueEvent::Started);
        self.transfer_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(120)).await;
                let result = this.update(cx, |view, cx| { cx.notify(); view.busy&&!view.preparing });
                if !matches!(result, Ok(true)) { break; }
            }
        }));
        self.queue_count = jobs.len();
        self.queue_total = jobs.len();
        self.succeeded = 0;
        self.duplicates = 0;
        self.failed.clear();
        cx.emit(QueueEvent::Message(format!("已加入 {} 个文件",jobs.len())));
        cx.notify();

        // The runner owns the task. File hashing, uploads and SQLite writes
        // run off the UI thread. Work is sequential to bound peak memory.
        self.upload_task = Some(cx.spawn(async move |this, cx| {
            let total = jobs.len();
            let interval_ms = std::env::var("YUQUE_UPLOAD_INTERVAL_MS").ok().and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(350).min(30_000);
            let mut pause_reason=None;
            let mut state_error=false;
            for (index, (job_id, path)) in jobs.into_iter().enumerate() {
                if index > 0 && config.mode == Mode::Yuque && control.load(Ordering::Relaxed) == 0 {
                    cx.background_executor().timer(std::time::Duration::from_millis(interval_ms)).await;
                }
                let command = control.load(Ordering::Relaxed);
                if command != 0 {
                    let storage = storage.clone();
                    let state = if command == 2 { "cancelled" } else { "paused" };
                    let result = cx.background_spawn(async move { storage.job_state(job_id, state, "") }).await;
                    let reason=pause_reason;
                    state_error|=result.is_err();
                    _ = this.update(cx, |view, cx| {
                        view.queue_count = total - index - 1;
                        cx.emit(QueueEvent::Refresh);
                        if result.is_err() {
                            cx.emit(QueueEvent::Message("任务状态保存失败，请在任务中心核实".into()));
                        }
                        if view.queue_count == 0 {
                            view.active_job = None;
                            view.busy = false;

                            let message = if state_error { "任务状态保存失败，请检查恢复队列".into() }
                                else if command == 2 { "已取消尚未开始的任务".into() } else { match reason {
                                    Some(uploader::QueueStop::Authorization)=>"上传未获授权，请检查会话后恢复任务".into(),
                                    Some(uploader::QueueStop::RateLimit)=>"上传受限（HTTP 429），稍后手动恢复".into(),
                                    Some(uploader::QueueStop::Unknown)=>"远端结果待核实，请先检查语雀网页端".into(),
                                    None=>"队列已暂停，可手动恢复".into(),
                                } };
                            cx.emit(QueueEvent::Message(message));
                            cx.emit(QueueEvent::ResourceChanged);
                        }
                        cx.notify();
                    });
                    continue;
                }
                let config = config.clone();
                let storage = storage.clone();
                let upload_path = path.clone();
                let progress = Arc::new(uploader::TransferProgress::default());
                _ = this.update(cx, |view, cx| {
                    view.active_job = Some(job_id);
                    view.transfer = progress.clone();
                    cx.notify();
                });
                let result = cx.background_spawn(async move {
                    storage.job_state(job_id, "running", "").map_err(|error| (error.to_string(),None))?;
                    let result = uploader::upload_with_progress(&upload_path, &config, &storage, progress.clone())
                        .map(|out| (out.duplicate, out.item.provider))
                        .map_err(|error| error.to_string());
                    let (state, error) = match &result {
                        Ok(_) => ("done", ""),
                        Err(error) => (uploader::failure_state(&progress), error.as_str()),
                    };
                    storage.job_state(job_id, state, error).map_err(|_| ("远端结果待核实：任务结果保存失败".to_string(),Some(uploader::QueueStop::Unknown)))?;
                    if state == "unknown" {
                        return Err((format!("远端结果待核实：{error}"),Some(uploader::QueueStop::Unknown)));
                    }
                    result.map_err(|error|(error,uploader::queue_stop(&progress)))
                }).await;
                let stop=result.as_ref().err().and_then(|(_,reason)|*reason);
                if stop.is_some() {
                    pause_reason=stop;
                    let _ = control.compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed);
                }
                _ = this.update(cx, |view, cx| {
                    view.queue_count = total - index - 1;
                    let filename = path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default();
                    let message = match result {
                        Ok((true, _)) => {
                            view.duplicates += 1;
                            "复用历史链接".to_string()
                        }
                        Ok((false, provider)) => {
                            view.succeeded += 1;
                            if provider == "demo" { "已保存本地记录".into() } else { "上传成功".into() }
                        }
                        Err((error,reason)) => {
                            if reason==Some(uploader::QueueStop::Authorization){
                                cx.emit(QueueEvent::Authorization);
                            }
                            view.failed.push((path.clone(), error.clone()));
                            format!("失败：{error}")
                        }
                    };
                    let mut status = format!("[{}/{}] {filename} · {message}", index + 1, total);
                    if view.queue_count == 0 {
                        view.busy = false;

                        status = format!("本批结束：新增 {} · 复用 {} · 失败或待核实 {}",
                            view.succeeded, view.duplicates, view.failed.len());
                        if let Some(reason)=stop{
                            status=match reason{
                                uploader::QueueStop::Authorization=>"上传未获授权，请检查会话后恢复任务".into(),
                                uploader::QueueStop::RateLimit=>"上传受限（HTTP 429），稍后手动恢复".into(),
                                uploader::QueueStop::Unknown=>"远端结果待核实，请先检查语雀网页端".into(),
                            };
                        }
                    }
                    cx.emit(QueueEvent::Message(status));
                    if view.queue_count==0{cx.emit(QueueEvent::Finished(format!("本批结束：新增 {} · 复用 {} · 失败 {}",view.succeeded,view.duplicates,view.failed.len())));}
                    view.active_job = None;
                    cx.emit(QueueEvent::Refresh);
                    if view.queue_count==0{cx.emit(QueueEvent::ResourceChanged);}
                    cx.notify();
                });
            }
        }));
    }
}
