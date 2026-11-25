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
    fn data_idle(&self,cx:&App)->bool{!self.prefs_writing&&!self.resource_write_busy&&!self.picker_pending&&!self.backup_busy&&!self.queue.read(cx).busy&&!self.account_busy(cx)&&!self.data_scan_busy&&!self.resetting}
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
        if let Some(u)=&self.data_usage{
            let mut stats=div().rounded_md().bg(p.surface).p_3().flex().flex_col().gap_2();
            for (label,n) in [("数据库与日志",u.database),("永久离线微缩图",u.previews),("旧版预览",u.legacy),("剪贴板暂存",u.clipboard)]{
                stats=stats.child(div().flex().justify_between().items_center().child(div().text_xs().text_color(p.muted).child(label)).child(div().text_sm().child(bytes(n))));
            }
            panel=panel.child(stats.child(div().h(px(1.)).bg(p.line))
                .child(div().flex().justify_between().child("已识别文件合计").child(bytes(u.database.saturating_add(u.previews).saturating_add(u.legacy).saturating_add(u.clipboard))))
                .child(div().flex().justify_between().child("可安全清理").child(format!("{} · {} 项",bytes(u.reclaimable),u.candidates)))
                .child(div().text_xs().text_color(p.muted).child(format!("未完成任务 {} · 已保存账号 {}",u.unfinished,u.accounts))));
        }
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
