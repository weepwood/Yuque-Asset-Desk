use super::*;
use gpui_kit::component::Sizable;

use crate::update::{self, State};

impl UploadDesk {
    fn check_update(&mut self,cx:&mut Context<Self>){
        if self.update_state==State::Checking{return;}
        self.update_state=State::Checking;
        cx.notify();
        self.update_task=Some(cx.spawn(async move|this,cx|{
            let state=cx.background_spawn(async{update::check()}).await;
            let _=this.update(cx,|view,cx|{view.update_state=state;cx.notify();});
        }));
    }
    pub(super) fn update_panel(&self,cx:&mut Context<Self>)->impl IntoElement{
        let p=self.palette();
        let mut section=div().flex().flex_col().gap_2().border_b_1().border_color(p.line).pb_4()
            .child(div().font_medium().child("关于与更新"))
            .child(div().flex().items_center().justify_between().gap_2()
                .child(div().flex_1().min_w(px(0.)).child(format!("当前版本 v{}",update::CURRENT))
                    .child(div().text_xs().text_color(p.muted).child("仅手动检查 GitHub 正式版本")))
                .child(Button::new("check-update").small().label(if self.update_state==State::Checking{"检查中…"}else{"检查更新"})
                    .disabled(self.update_state==State::Checking)
                    .on_click(cx.listener(|v,_,_,cx|v.check_update(cx)))));
        match &self.update_state{
            State::Idle=>{},
            State::Checking=>{section=section.child(div().text_xs().text_color(p.muted).child("正在读取发布信息…"));},
            State::Current(latest)=>{section=section.child(div().text_sm().text_color(p.accent).child(format!("没有可用更新（最新正式版本 v{latest}）")));},
            State::Failed(error)=>{section=section.child(div().text_sm().text_color(p.error).child(error.clone()));},
            State::Newer(info)=>{
                let page=info.page.clone();
                section=section.child(div().rounded_md().bg(p.surface).p_3().flex().flex_col().gap_2()
                    .child(div().font_medium().text_color(p.accent).child(format!("发现新版本 v{}",info.version)))
                    .child(div().text_xs().text_color(p.muted).child(if info.package.is_some(){"下载后核对 SHA256，退出应用再手动安装或替换。"}else{"暂无适配当前平台且附校验文件的安装包，请查看发布页。"}))
                    .child(div().flex().flex_wrap().items_center().gap_2()
                        .when_some(info.package.clone(),|d,(zip,sum)|d
                            .child(Button::new("download-update").small().primary().label("下载安装包").on_click(cx.listener(move|_,_,_,cx|cx.open_url(&zip))))
                            .child(Button::new("checksum-update").small().ghost().label("SHA256").on_click(cx.listener(move|_,_,_,cx|cx.open_url(&sum)))))
                        .child(Button::new("update-notes").small().ghost().label("版本说明").on_click(cx.listener(move|_,_,_,cx|cx.open_url(&page))))));
            }
        }
        section.child(Button::new("all-releases").small().ghost().label("查看历史版本").on_click(cx.listener(|_,_,_,cx|cx.open_url(update::RELEASES))))
    }
}
