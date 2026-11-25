use super::*;
use gpui_kit::component::Sizable;
use crate::data_management as data;

fn bytes(n:u64)->String{file_size(n.min(i64::MAX as u64) as i64)}
fn valid_owner(owner:&str)->bool{!owner.is_empty()&&owner.len()<=32&&owner.bytes().all(|b|b.is_ascii_digit())}
// A failed vault operation must retain the index for a later retry.
fn record_deleted(storage:&Storage,owner:&str,confirmed_absent:bool)->bool{
    confirmed_absent&&storage.forget_account(owner).is_ok()
}

impl UploadDesk{
    fn data_idle(&self,cx:&App)->bool{!self.prefs_writing&&!self.resource_write_busy&&!self.picker_pending&&!self.backup_busy&&!self.queue.read(cx).busy&&!self.preview_worker.read(cx).busy&&!self.account_busy(cx)&&!self.data_scan_busy&&!self.resetting}
    pub(super) fn refresh_data(&mut self,cx:&mut Context<Self>){
        if self.resource_write_busy||self.picker_pending||self.backup_busy||self.queue.read(cx).busy||self.account_busy(cx)||self.data_scan_busy||self.resetting{return;}
        self.data_scan_busy=true;self.data_feedback="正在统计本地文件…".into();cx.notify();
        let storage=self.storage.clone();
        self.data_task=Some(cx.spawn(async move|this,cx|{
            let result=cx.background_spawn(async move{data::usage(&storage)}).await;
            let _=this.update(cx,|v,cx|{
                v.data_scan_busy=false;
                match result{Ok(u)=>{v.data_usage=Some(u);v.data_feedback="统计完成（文件逻辑大小）".into();},Err(e)=>{v.data_usage=None;v.data_feedback=format!("统计失败：{e}");}}
                cx.notify();
            });
        }));
    }
    fn confirm_cleanup(&mut self,w:&mut Window,cx:&mut Context<Self>){
        if !self.data_idle(cx){return;}
        self.maintenance_busy=true;cx.notify();
        let answer=w.prompt(PromptLevel::Warning,"清理无引用暂存文件？",Some("仅删除内容哈希匹配、没有任何任务或本地链接引用的剪贴板暂存文件。永久微缩图、原始文件和备份不会删除。"),&["取消","确认清理"],cx);
        let storage=self.storage.clone();
        self.data_task=Some(cx.spawn(async move|this,cx|{
            let accepted=matches!(answer.await,Ok(1));
            let result=if accepted{Some(cx.background_spawn(async move{
                let r=data::clean(&storage);
                (r,data::usage(&storage))
            }).await)}else{None};
            let _=this.update(cx,|v,cx|{
                v.maintenance_busy=false;
                if let Some((result,usage))=result{
                    v.data_usage=usage.ok();
                    v.data_feedback=match result{Ok(r)=>format!("已清理 {} 项，释放 {}；失败 {} 项",r.count,bytes(r.bytes),r.failed),Err(e)=>format!("清理停止：{e}")};
                }else{v.data_feedback="已取消清理".into();}
                cx.notify();
            });
        }));
    }
    pub(super) fn confirm_privacy(&mut self,all:bool,w:&mut Window,cx:&mut Context<Self>){
        self.confirm_credentials(all,false,w,cx);
    }
    fn arm_reset(&mut self,cx:&mut Context<Self>){
        if !self.data_idle(cx){return;}
        self.maintenance_busy=true;
        self.reset_armed=false;
        self.data_feedback="正在核对未完成任务…".into();cx.notify();
        let storage=self.storage.clone();
        self.data_task=Some(cx.spawn(async move|this,cx|{
            let result=cx.background_spawn(async move{storage.unfinished_count()}).await;
            let _=this.update(cx,|v,cx|{
                v.maintenance_busy=false;
                if v.page!=Page::Data{cx.notify();return;}
                match result{
                    Ok(0)=>{v.reset_armed=true;v.data_feedback="请先导出备份并阅读重置范围，再确认退出。".into();}
                    Ok(n)=>v.data_feedback=format!("存在 {n} 项未完成、失败或待核实任务；请先处理，不允许重置。"),
                    Err(_)=>v.data_feedback="无法核对任务状态，禁止重置".into(),
                }
                cx.notify();
            });
        }));
    }
    fn confirm_credentials(&mut self,all:bool,reset:bool,w:&mut Window,cx:&mut Context<Self>){
        if !self.data_idle(cx) || (reset&&!self.reset_armed){return;}
        let active=self.config.creds.as_ref().map(|c|c.user_id.clone());
        self.maintenance_busy=true;self.reset_pending=reset;cx.notify();
        let (title,detail,action)=if reset{
            ("完全重置并退出？","将清除本应用已索引的账号凭据、本地链接数据库、设置、永久微缩图及应用生成的剪贴板副本。外部原始文件、备份和语雀云端不删除。备份不含剪贴板原图，请先另存需要的副本。操作不可撤销。","确认重置并退出")
        }else if all{
            ("清除所有已保存账号凭据？","将删除本应用索引的系统凭据并退出当前账号；上传历史、微缩图和任务保留。外部环境变量中的凭据不受影响。","确认清除")
        }else{
            ("删除当前账号凭据？","保留资源历史和上传任务，不删除其他账号。","确认删除")
        };
        let answer=w.prompt(PromptLevel::Warning,title,Some(detail),&["取消",action],cx);
        let storage=self.storage.clone();
        self.data_task=Some(cx.spawn(async move|this,cx|{
            if !matches!(answer.await,Ok(1)){
                let _=this.update(cx,|v,cx|{v.maintenance_busy=false;v.reset_pending=false;v.reset_armed=false;v.data_feedback="操作已取消".into();cx.notify();});return;
            }
            // Recheck after confirmation, before any vault deletion. The maintenance
            // gate stays held through preflight, credential deletion and reset launch.
            let db=storage.clone();
            let preflight=cx.background_spawn(async move{
                if reset{anyhow::ensure!(db.unfinished_count()?==0,"存在未完成或待核实任务，禁止重置");}
                if all{db.accounts()}else{Ok(vec![])}
            }).await;
            let mut owners=match preflight{
                Ok(ids)=>ids,
                Err(_)=>{
                    let _=this.update(cx,|v,cx|{v.maintenance_busy=false;v.reset_pending=false;v.reset_armed=false;v.data_feedback="无法确认任务状态或账号索引，未删除凭据".into();cx.notify();});
                    return;
                }
            };
            if let Some(id)=&active{if !owners.contains(id){owners.push(id.clone());}}
            let mut success=0;let mut failed=0;let mut active_removed=false;
            for owner in owners{
                if !valid_owner(&owner){failed+=1;continue;}
                let key=YuqueCredentials::vault_key(&owner);
                let read=match this.update(cx,|_,cx|cx.read_credentials(&key)){Ok(t)=>t.await,Err(_)=>return};
                let absent=match read{
                    Ok(None)=>true,
                    Ok(Some(_))=>{
                        let delete=match this.update(cx,|_,cx|cx.delete_credentials(&key)){Ok(t)=>t.await,Err(_)=>return};
                        if delete.is_err(){false}else{
                            match this.update(cx,|_,cx|cx.read_credentials(&key)){Ok(t)=>matches!(t.await,Ok(None)),Err(_)=>return}
                        }
                    }
                    Err(_)=>false,
                };
                let db=storage.clone();let id=owner.clone();
                let recorded=cx.background_spawn(async move{record_deleted(&db,&id,absent)}).await;
                if absent && active.as_ref()==Some(&owner){active_removed=true;}
                if recorded{success+=1;}
                else{failed+=1;}
            }
            let accounts=cx.background_spawn({let storage=storage.clone();async move{storage.accounts()}}).await;
            if accounts.is_err(){failed+=1;}
            let launch=if reset&&failed==0{
                Some(cx.background_spawn(async move{data::schedule_reset(&storage)}).await)
            }else{None};
            let _=this.update(cx,|v,cx|{
                v.maintenance_busy=false;v.reset_pending=false;v.reset_armed=false;v.data_usage=None;
                if let Ok(ids)=accounts{v.account.update(cx,|a,cx|a.replace_index(ids,cx));}
                if !matches!(launch,Some(Ok(())))&&(active_removed || (all&&active.is_none())){v.activate_account(None,cx);}
                v.data_feedback=format!("凭据已清除或确认不存在 {success} 项；失败 {failed} 项（保留索引以便重试）");
                v.status=v.data_feedback.clone();
                if reset{
                    if failed>0{v.data_feedback.push_str("；重置已停止。");}
                    else{
                        match launch.expect("reset scheduled after successful preflight"){
                            Ok(())=>{v.resetting=true;v.maintenance_busy=true;v.prefs_task=None;cx.quit();}
                            Err(e)=>v.data_feedback=format!("凭据处理完成，但本地重置未启动：{e}"),
                        }
                    }
                }
                cx.notify();
            });
        }));
    }
    pub(super) fn export_preview_audit(&mut self,cx:&mut Context<Self>){
        if !self.data_idle(cx){return;}
        self.picker_pending=true;self.audit_export_feedback="选择诊断报告保存目录（包含资源名称和 ID，不含链接及凭据）…".into();cx.notify();
        let selection=cx.prompt_for_paths(PathPromptOptions{files:false,directories:true,multiple:false,prompt:Some("选择诊断报告位置".into())});
        let scope=(self.config.provider().to_owned(),self.config.owner_id().to_owned());
        self.picker_task=Some(cx.spawn(async move|this,cx|{
            let result=selection.await;
            let _=this.update(cx,|v,cx|{
                v.picker_pending=false;
                match result{
                    Ok(Ok(Some(mut paths))) if paths.len()==1&&scope.0==v.config.provider()&&scope.1==v.config.owner_id()=>{
                        if !v.data_idle(cx){v.audit_export_feedback="当前有其他任务，请稍后导出".into();cx.notify();return;}
                        v.backup_busy=true;v.audit_export_feedback="正在重新扫描并导出完整诊断报告…".into();
                        let parent=paths.remove(0);let storage=v.storage.clone();
                        v.backup_task=Some(cx.spawn(async move|this,cx|{
                            let result=cx.background_spawn(async move{preview::export_audit(&storage,&scope.0,&scope.1,&parent)}).await;
                            let _=this.update(cx,|v,cx|{v.backup_busy=false;v.audit_export_feedback=match result{Ok((path,n))=>format!("已导出 {n} 项异常或待处理记录：{}",path.display()),Err(e)=>format!("报告导出失败：{e}")};cx.notify();});
                        }));
                    }
                    Ok(Ok(_))=>v.audit_export_feedback="已取消导出或账号已切换".into(),
                    _=>v.audit_export_feedback="无法选择报告目录".into(),
                }
                cx.notify();
            });
        }));
    }
    pub(super) fn data_panel(&self,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();let idle=self.data_idle(cx);
        let mut panel=div().flex().flex_col().gap(px(self.spacing().section))
            .child(div().text_xs().text_color(p.muted).child("管理本地数据和账号隐私。不会删除语雀云端文件。"))
            .child(div().flex().items_center().gap_2()
                .child(div().flex_1().min_w(px(0.)).flex().flex_col()
                    .child(div().text_xs().text_color(p.muted).child("本地数据目录"))
                    .child(div().text_xs().truncate().child(self.storage.data_dir().display().to_string())))
                .child(Button::new("open-data").small().ghost().label("打开目录").on_click(cx.listener(|v,_,_,cx|cx.open_with_system(v.storage.data_dir())))))
            .child(div().flex().items_center().justify_between()
                .child(div().text_xs().text_color(p.muted).child("空间统计 · 不含外部备份"))
                .child(Button::new("scan-data").small().label(if self.data_scan_busy{"统计中…"}else{"统计空间"}).disabled(!idle).on_click(cx.listener(|v,_,_,cx|v.refresh_data(cx)))));
        let mut stats_panel=None;
        if let Some(u)=&self.data_usage{
            let mut stats=div().rounded_md().bg(p.surface).p_3().flex().flex_col().gap_2();
            for (label,n) in [("数据库与日志",u.database),("永久离线微缩图",u.previews),("旧版预览",u.legacy),("剪贴板暂存",u.clipboard),("上传暂存",u.staging)]{
                stats=stats.child(div().flex().justify_between().items_center().child(div().text_xs().text_color(p.muted).child(label)).child(div().text_sm().child(bytes(n))));
            }
            stats_panel=Some(stats.child(div().h(px(1.)).bg(p.line))
                .child(div().flex().justify_between().child("已识别文件合计").child(bytes(u.database.saturating_add(u.previews).saturating_add(u.legacy).saturating_add(u.clipboard).saturating_add(u.staging))))
                .child(div().flex().justify_between().child("可安全清理").child(format!("{} · {} 项",bytes(u.reclaimable),u.candidates)))
                .child(div().text_xs().text_color(p.muted).child(format!("未完成任务 {} · 已保存账号 {}",u.unfinished,u.accounts))));
        }
        let worker=self.preview_worker.read(cx);
        let report=worker.audit.as_ref().filter(|((provider,owner),_)|provider==self.config.provider()&&owner==self.config.owner_id()).map(|(_,r)|r);
        let mut audit=div().rounded_md().bg(p.surface).p_3().flex().flex_col().gap_2()
            .child(div().font_semibold().child("当前账号 · 全库图片诊断"))
            .child(div().text_xs().text_color(p.muted).child("仅诊断检查本地缩略图和链接策略，不读取原图；修复尝试本地来源。均不联网、不重新上传。"))
            .child(div().flex().flex_wrap().gap_2()
                .child(div().debug_selector(||"audit-previews".into()).child(Button::new("audit-previews").small().label("仅诊断").disabled(!idle).on_click(cx.listener(|v,_,_,cx|v.audit_previews(false,cx)))))
                .child(div().debug_selector(||"audit-repair".into()).child(Button::new("audit-repair").small().label("诊断并修复").disabled(!idle).on_click(cx.listener(|v,_,_,cx|v.audit_previews(true,cx)))))
                .child(Button::new("audit-export").small().ghost().label("导出完整报告").disabled(!idle).on_click(cx.listener(|v,_,_,cx|v.export_preview_audit(cx))))
                .when(worker.audit_running,|d|d.child(Button::new("audit-stop").small().ghost().label("停止扫描").on_click(cx.listener(|v,_,_,cx|v.preview_worker.update(cx,|m,cx|m.stop_scan(cx)))))))
            .when(worker.audit_running,|d|d.child(div().text_xs().text_color(p.muted).child("扫描中 · 已入队修复仍会完成")));
        if let Some(report)=report{
            audit=audit.child(div().text_xs().child(report.summary())).child(div().text_xs().text_color(p.muted).child("统计为扫描时状态；修复后可再次诊断。链接检查不代表远端可访问。"));
            if !report.issues.is_empty(){
                let page=worker.audit_page.min((report.issues.len()-1)/8);
                let start=page*8;let end=(start+8).min(report.issues.len());
                audit=audit.child(div().h(px(1.)).bg(p.line)).child(div().text_xs().font_medium().child(format!("异常与待处理 {} 项 · 显示 {}–{} / {}（最多100项）",report.issue_count,start+1,end,report.issues.len())));
                for issue in &report.issues[start..end]{
                    let id=issue.id;let scope=(self.config.provider().to_owned(),self.config.owner_id().to_owned());let repair_scope=scope.clone();
                    audit=audit.child(div().flex().flex_wrap().items_center().gap_2().py_1()
                        .child(div().flex_1().min_w(px(120.)).flex().flex_col().gap_1()
                            .child(div().text_sm().truncate().child(issue.name.clone()))
                            .child(div().text_xs().text_color(p.muted).child(format!("#{} · {}{}",id,issue.status.label(),if issue.unsupported_link{" · 不支持在线恢复"}else{""}))))
                        .child(div().debug_selector(move||format!("audit-locate-{id}").into()).child(Button::new(SharedString::from(format!("audit-locate-{id}"))).small().ghost().label("定位").disabled(!idle).on_click(cx.listener(move|v,_,w,cx|{
                            if v.config.provider()==scope.0&&v.config.owner_id()==scope.1{v.locate_job_asset(id,w,cx);v.status="已定位诊断资源；清空搜索返回全部资源".into();cx.notify();}
                        }))))
                        .when(issue.status.repairable(),|d|d.child(div().debug_selector(move||format!("audit-fix-{id}").into()).child(Button::new(SharedString::from(format!("audit-fix-{id}"))).small().label("修复").disabled(!idle).on_click(cx.listener(move|v,_,_,cx|{
                            if v.config.provider()==repair_scope.0&&v.config.owner_id()==repair_scope.1{v.repair_local_previews(vec![id],cx);}
                        }))))));
                }
                audit=audit.child(div().flex().items_center().gap_2()
                    .child(Button::new("audit-prev").small().ghost().label("上一页").disabled(page==0).on_click(cx.listener(move|v,_,_,cx|v.preview_worker.update(cx,|m,cx|{m.audit_page=page.saturating_sub(1);cx.notify();}))))
                    .child(Button::new("audit-next").small().ghost().label("下一页").disabled(end>=report.issues.len()).on_click(cx.listener(move|v,_,_,cx|v.preview_worker.update(cx,|m,cx|{m.audit_page=page+1;cx.notify();}))))
                    .child(div().text_xs().text_color(p.muted).child("完整报告会重新扫描，不含链接、路径或凭据")));
            }
        }
        panel=panel.child(audit.when(!self.audit_export_feedback.is_empty(),|d|d.child(div().text_xs().text_color(p.muted).child(self.audit_export_feedback.clone()))));
        if let Some(stats)=stats_panel{panel=panel.child(stats);}
        panel.child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().child("清理无引用暂存").child(div().text_xs().text_color(p.muted).child("不删除永久离线微缩图或任何已引用文件")))
                .child(Button::new("clean-data").small().label("清理").disabled(!idle).on_click(cx.listener(|v,_,w,cx|v.confirm_cleanup(w,cx)))))
            .child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().child("隐私清理").child(div().text_xs().text_color(p.muted).child("清除已保存账号凭据，保留相册与任务")))
                .child(Button::new("privacy-data").small().label("清除凭据").disabled(!idle).on_click(cx.listener(|v,_,w,cx|v.confirm_privacy(true,w,cx)))))
            .child(div().h(px(1.)).bg(p.line))
            .child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().child(div().text_color(p.error).child("完全重置")).child(div().text_xs().text_color(p.muted).child("卸载前可选；先备份，未完成任务会阻止重置")))
                .child(div().debug_selector(||"arm-reset".into()).child(Button::new("arm-reset").small().label("重置数据…").disabled(!idle).on_click(cx.listener(|v,_,_,cx|v.arm_reset(cx))))))
            .when(self.reset_armed,|d|d.child(div().p_3().rounded_md().bg(p.surface).flex().flex_col().gap_2()
                .child(div().text_xs().text_color(p.error).child("重置将永久删除本地相册、配置、微缩图和应用生成的剪贴板副本。备份不包含剪贴板原图，请先另存。"))
                .child(div().flex().flex_wrap().gap_2()
                    .child(Button::new("reset-backup").small().label("先导出相册备份").disabled(!idle).on_click(cx.listener(|v,_,_,cx|v.export_album(cx))))
                    .child(Button::new("reset-cancel").small().ghost().label("取消").on_click(cx.listener(|v,_,_,cx|{v.reset_armed=false;cx.notify();})))
                    .child(Button::new("reset-final").small().danger().label("继续重置…").disabled(!idle).on_click(cx.listener(|v,_,w,cx|v.confirm_credentials(true,true,w,cx)))))))
            .child(div().text_xs().text_color(p.muted).child(if cfg!(target_os="macos"){"卸载：退出应用后，将 .app 移入废纸篓。"}else{"卸载：退出应用后，删除便携 EXE 或其解压目录。"}))
            .when(!self.data_feedback.is_empty(),|d|d.child(div().text_xs().text_color(p.muted).child(self.data_feedback.clone())))
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[::core::prelude::v1::test]
    fn failed_credential_deletion_keeps_index(){
        let root=std::env::temp_dir().join(format!("yuque-credential-index-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let storage=Storage::new(root.join("assets.db")).unwrap();
        storage.save_account("123").unwrap();storage.save_account("456").unwrap();
        assert!(!record_deleted(&storage,"123",false));
        assert!(record_deleted(&storage,"456",true));
        assert_eq!(storage.accounts().unwrap(),vec!["123"]);
        assert!(!valid_owner("../other"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
