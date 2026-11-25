//! Independent settings presentation. One section switcher, no duplicate links.
use super::*;
use gpui_kit::component::{Sizable,scroll::ScrollableElement};
impl UploadDesk{
    pub(super) fn settings_view(&self,cx:&mut Context<Self>)->impl IntoElement{
        let tabs=div().flex().items_center().gap_1().border_b_1().border_color(self.palette().line).pb_2()
            .child(Button::new("settings-tab-settings").small().ghost().selected(self.page==Page::Settings).label("常规").on_click(cx.listener(|v,_,_,cx|v.navigate_page(Page::Settings,cx))))
            .child(div().debug_selector(||"open-account-settings".into()).child(Button::new("settings-tab-account").small().ghost().selected(self.page==Page::Account).label("账号").on_click(cx.listener(|v,_,_,cx|v.navigate_page(Page::Account,cx)))))
            .child(div().debug_selector(||"open-data-settings".into()).child(Button::new("settings-tab-data").small().ghost().selected(self.page==Page::Data).label("存储").on_click(cx.listener(|v,_,_,cx|v.navigate_page(Page::Data,cx)))));
        let title=match self.page{Page::Account=>"语雀账号",Page::Data=>"存储与数据",_=>"常规设置"};
        let mut content=div().flex().flex_col().gap(px(self.spacing().section)).min_w(px(0.))
            .child(div().flex().flex_col().gap_1().pb_2().child(div().text_lg().font_semibold().child(title))
                .child(div().text_xs().text_color(self.palette().muted).child(match self.page{Page::Account=>"管理上传身份与会话验证",Page::Data=>"本地索引、离线预览与备份",_=>"个性化工作区、快捷操作与更新"})));
        match self.page{
            Page::Account=>content=content.child(self.account_form(cx)),
            Page::Data=>content=content.child(self.data_panel(cx)),
            _=>content=content.child(self.settings_preferences(cx)).child(self.update_panel(cx)),
        }
        div().id("settings-page").h_full().min_w(px(0.)).min_h(px(0.)).w_full().flex().flex_col().gap(px(self.spacing().section)).p(px(self.spacing().pad)).bg(self.palette().canvas)
            .child(tabs.flex_shrink_0())
            .child(content.flex_1().min_h(px(0.)).overflow_y_scrollbar())
    }
}
