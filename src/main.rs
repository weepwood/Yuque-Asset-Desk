#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod format;
mod storage;
mod uploader;

use std::collections::VecDeque;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use directories::ProjectDirs;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, h_flex, v_flex,
    button::{Button, ButtonVariants as _},
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
};
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use config::{Config, Mode};
use storage::{Asset, Storage};

const MAX_QUEUE: usize = 128;
const RECENT_LIMIT: usize = 300;

fn data_path() -> Result<PathBuf> {
    // Retain the same application identity and database location as the Slint build.
    let dirs = ProjectDirs::from("com", "Weepwood", "YuqueAssetDesk")
        .context("找不到当前系统的应用数据目录")?;
    Ok(dirs.data_local_dir().join("assets.db"))
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

struct Dashboard {
    config: Config,
    storage: Storage,
    assets: Vec<Asset>,
    total: i64,
    status: String,
    images_only: bool,
    search_query: String,
    search: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    queue: VecDeque<PathBuf>,
    working: bool,
    current_task: Option<Task<()>>,
}

impl Dashboard {
    fn new(window: &mut Window, cx: &mut Context<Self>, config: Config, storage: Storage) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索上传历史..."));
        let sub = cx.subscribe_in(&search, window, |this, input, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.search_query = input.read(cx).value().to_string();
                cx.notify();
            }
        });
        let (assets, total) = Self::load_records(&storage);
        let status = match (&config.mode, &config.creds) {
            (Mode::Demo, _) => "演示模式：只记录本机文件链接，不向网络上传".to_owned(),
            (Mode::Yuque, Some(_)) => "语雀会话已加载，非公开上传接口仍需账号实测".to_owned(),
            (Mode::Yuque, None) => "语雀模式未配置 Cookie / CSRF / 用户 ID".to_owned(),
        };
        Self {
            config, storage, assets, total, status, images_only: false,
            search_query: String::new(), search,
            _subscriptions: vec![sub], queue: VecDeque::new(),
            working: false, current_task: None,
        }
    }

    fn load_records(storage: &Storage) -> (Vec<Asset>, i64) {
        (storage.recent(RECENT_LIMIT, "").unwrap_or_default(), storage.count().unwrap_or(0))
    }

    fn reload(&mut self) {
        let (assets, total) = Self::load_records(&self.storage);
        self.assets = assets;
        self.total = total;
    }

    fn pick_files(&mut self, cx: &mut Context<Self>) {
        // Native modal picker: upload work never runs on the UI event thread.
        let Some(files) = rfd::FileDialog::new()
            .set_title("选择要上传的图片和附件")
            .pick_files()
        else { return; };
        let available = MAX_QUEUE.saturating_sub(self.queue.len() + usize::from(self.working));
        let selected = files.len().min(available);
        self.queue.extend(files.into_iter().take(available));
        self.status = if selected == 0 {
            "队列已满，请在当前任务完成后重新选择".into()
        } else {
            format!("已添加 {selected} 个文件，上传队列依次处理")
        };
        self.run_next(cx);
        cx.notify();
    }

    fn run_next(&mut self, cx: &mut Context<Self>) {
        if self.working { return; }
        let Some(path) = self.queue.pop_front() else { return; };
        self.working = true;
        self.status = format!("正在处理：{}", path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default());
        let config = self.config.clone();
        let storage = self.storage.clone();
        self.current_task = Some(cx.spawn(async move |this, cx| {
            // Synchronous SHA-256 and HTTP file streaming execute only on background workers.
            let result = cx.background_spawn(async move {
                uploader::upload(&path, &config, &storage)
            }).await;
            let _ = this.update(cx, |dashboard, cx| {
                dashboard.working = false;
                dashboard.status = match result {
                    Ok(output) if output.duplicate => "已复用相同文件的历史链接".into(),
                    Ok(output) if output.item.provider == "demo" => {
                        "本地演示已记录，file:// 链接无法用于公开文章".into()
                    },
                    Ok(_) => "语雀返回了资源 URL，历史已保存".into(),
                    Err(error) => format!("上传失败：{error}"),
                };
                dashboard.reload();
                dashboard.run_next(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn copy(&mut self, url: String, name: String, kind: String, markdown: bool, cx: &mut Context<Self>) {
        let value = if markdown {
            format::markdown(&name, &url, kind == "image")
        } else { url };
        self.status = match arboard::Clipboard::new().and_then(|mut c| c.set_text(value)) {
            Ok(()) => if markdown { "已复制 Markdown 引用".into() } else { "已复制文件链接".into() },
            Err(error) => format!("无法访问剪贴板：{error}"),
        };
        cx.notify();
    }

    fn matches(&self, item: &Asset) -> bool {
        (!self.images_only || item.category == "image")
            && (self.search_query.is_empty()
                || item.name.to_lowercase().contains(&self.search_query.to_lowercase()))
    }

    fn mode_title(&self) -> &'static str {
        match self.config.mode { Mode::Demo => "本地演示", Mode::Yuque => "语雀上传" }
    }

    fn row(&self, asset: &Asset, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let item_id = asset.id;
        let url = asset.url.clone();
        let label = asset.name.clone();
        let kind = asset.category.clone();
        let url_md = url.clone();
        let label_md = label.clone();
        let kind_md = kind.clone();
        h_flex()
            .items_center()
            .gap_3()
            .p_3()
            .rounded_lg()
            .bg(cx.theme().secondary)
            .child(
                div()
                    .w(px(38.))
                    .h(px(38.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(cx.theme().muted)
                    .child(if kind == "image" { "▧" } else { "▤" })
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_ellipsis().child(label.clone()))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child(format!("{}  ·  {}  ·  {}", human_size(asset.bytes as u64), asset.provider, asset.created_at)))
            )
            .child(Button::new(format!("url-{item_id}")).label("复制 URL")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.copy(url.clone(), label.clone(), kind.clone(), false, cx);
                })))
            .child(Button::new(format!("markdown-{item_id}")).label("Markdown")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.copy(url_md.clone(), label_md.clone(), kind_md.clone(), true, cx);
                })))
    }
}

impl Render for Dashboard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let shown = self.assets.iter().filter(|item| self.matches(item)).count();
        let pending = self.queue.len() + usize::from(self.working);
        let stats = h_flex()
            .gap_3()
            .child(stat_card("资源记录", self.total.to_string(), cx))
            .child(stat_card("任务队列", pending.to_string(), cx))
            .child(stat_card("存储目标", self.mode_title().to_owned(), cx));
        let list = v_flex()
            .id("asset-history")
            .flex_1()
            .min_h_0()
            .gap_2()
            .overflow_y_scrollbar()
            .children(self.assets.iter().filter(|item| self.matches(item))
                .map(|item| self.row(item, cx)));
        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                v_flex()
                    .w(px(228.))
                    .flex_shrink_0()
                    .p_5()
                    .gap_4()
                    .bg(cx.theme().secondary)
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child("YUQUE  /  DESK"))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child("GPUI KIT · NATIVE DESKTOP"))
                    .child(Button::new("tab-all")
                        .when(!self.images_only, |button| button.primary())
                        .label("上传工作台")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.images_only = false;
                            cx.notify();
                        })))
                    .child(Button::new("tab-images")
                        .when(self.images_only, |button| button.primary())
                        .label("图片资源")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.images_only = true;
                            cx.notify();
                        })))
                    .child(div().flex_1())
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("无需 Chromium / WebView"))
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .p_6()
                    .gap_4()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                v_flex().gap_1()
                                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                                        .child("PERSONAL WORKSPACE / FILE UPLOAD"))
                                    .child(div().text_2xl().font_weight(FontWeight::BOLD)
                                        .child("资源上传与管理"))
                            )
                            .child(Button::new("refresh").label("刷新历史")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.reload();
                                    this.status = "历史记录已刷新".into();
                                    cx.notify();
                                })))
                    )
                    .child(
                        v_flex().p_5().gap_3().rounded_xl().bg(cx.theme().secondary)
                            .child(div().text_xl().font_weight(FontWeight::BOLD).child("让图片和文档链接触手可及"))
                            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                                .child("上传图片、PDF 或附件，复制原始地址或 Markdown 引用"))
                            .child(Button::new("upload").primary().label("＋ 选择文件上传")
                                .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))))
                    )
                    .child(stats)
                    .child(
                        h_flex().items_center().justify_between()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(format!("最近资源  ·  {shown}")))
                            .child(div().w(px(246.)).child(Input::new(&self.search).small()))
                    )
                    .child(
                        v_flex().flex_1().min_h_0().p_2().rounded_xl().bg(cx.theme().background)
                            .when(shown == 0, |container| container.child(
                                div().p_6().text_color(cx.theme().muted_foreground)
                                    .child("没有匹配的上传记录。使用上方按钮添加文件。")
                            ))
                            .child(list)
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if self.working { "● 上传中" } else { "● 就绪" })
                            .child(self.status.clone())
                    )
            )
    }
}

fn stat_card(title: &str, value: String, cx: &mut Context<Dashboard>) -> impl IntoElement + use<> {
    v_flex()
        .flex_1()
        .gap_2()
        .p_4()
        .rounded_lg()
        .bg(cx.theme().secondary)
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(title.to_owned()))
        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(value))
}

fn main() -> Result<()> {
    let config = Config::from_env();
    let storage = Storage::new(data_path()?)?;
    application().with_assets(assets::Assets).run(move |cx| {
        init(cx);
        let config = config.clone();
        let storage = storage.clone();
        let _ = open_window(WindowOptions::default(), cx, |window, cx| {
            window.set_window_title("Yuque Asset Desk · GPUI Kit");
            cx.new(|cx| Dashboard::new(window, cx, config, storage))
        }).expect("无法创建 GPUI Kit 主窗口");
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::human_size;
    #[test]
    fn sizes() {
        assert_eq!(human_size(500), "500 B");
        assert_eq!(human_size(1024), "1.0 KB");
    }
}
