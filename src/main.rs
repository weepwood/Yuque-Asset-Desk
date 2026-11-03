#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod format;
mod storage;
mod uploader;

use anyhow::{Context as AnyhowContext, Result};
use config::{Config, Mode};
use directories::ProjectDirs;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
};
use gpui_kit::*;
use std::path::PathBuf;
use storage::{Asset, Storage};

fn db_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "Weepwood", "YuqueAssetDesk")
        .context("无法找到应用数据目录")?;
    Ok(dirs.data_local_dir().join("assets.db"))
}

fn file_size(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else if b >= 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else {
        format!("{b:.0} B")
    }
}

struct UploadDesk {
    config: Config,
    storage: Storage,
    assets: Vec<Asset>,
    search: String,
    search_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    image_only: bool,
    busy: bool,
    queue_count: usize,
    status: String,
    upload_task: Option<Task<()>>,
}

impl UploadDesk {
    fn new(window: &mut Window, cx: &mut Context<Self>, config: Config, storage: Storage) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索文件名..."));
        let subscription = cx.subscribe_in(
            &search_input,
            window,
            |this, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.search = input.read(cx).value().to_string();
                    cx.notify();
                }
            },
        );
        let assets = storage.recent(300, "").unwrap_or_default();
        let status = match (&config.mode, &config.creds) {
            (Mode::Demo, _) => "本地演示模式：不会上传至网络".to_string(),
            (Mode::Yuque, None) => "语雀未配置：请设置用户 ID、Cookie 和 CSRF Token".to_string(),
            (Mode::Yuque, Some(_)) => "语雀会话已配置，等待上传文件".to_string(),
        };
        Self {
            config,
            storage,
            assets,
            search: String::new(),
            search_input,
            _subscriptions: vec![subscription],
            image_only: false,
            busy: false,
            queue_count: 0,
            status,
            upload_task: None,
        }
    }

    fn reload(&mut self) {
        match self.storage.recent(300, "") {
            Ok(items) => self.assets = items,
            Err(error) => self.status = format!("读取上传历史失败：{error}"),
        }
    }

    fn select_files(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            self.status = "正在上传，请等待当前队列完成".to_string();
            cx.notify();
            return;
        }
        let Some(files) = rfd::FileDialog::new()
            .set_title("选择图片或附件")
            .pick_files() else { return; };
        if files.is_empty() { return; }
        let config = self.config.clone();
        let storage = self.storage.clone();
        self.busy = true;
        self.queue_count = files.len();
        self.status = format!("已加入 {} 个文件", files.len());
        cx.notify();

        // The view owns the task. File hashing, uploads and SQLite writes
        // run off the UI thread. Work is sequential to bound peak memory.
        self.upload_task = Some(cx.spawn(async move |this, cx| {
            let total = files.len();
            for (index, path) in files.into_iter().enumerate() {
                let config = config.clone();
                let storage = storage.clone();
                let result = cx.background_spawn(async move {
                    uploader::upload(&path, &config, &storage)
                        .map(|out| (out.duplicate, out.item.provider))
                        .map_err(|error| error.to_string())
                }).await;
                _ = this.update(cx, |view, cx| {
                    view.queue_count = total - index - 1;
                    view.status = match result {
                        Ok((true, _)) => "发现相同文件，复用历史链接".to_string(),
                        Ok((false, provider)) if provider == "demo" =>
                            "已保存本地记录（非公网链接）".to_string(),
                        Ok(_) => "上传成功，已保存资源链接".to_string(),
                        Err(error) => format!("上传失败：{error}"),
                    };
                    view.reload();
                    if view.queue_count == 0 {
                        view.busy = false;
                    }
                    cx.notify();
                });
            }
        }));
    }

    fn copy_asset(&mut self, asset: &Asset, as_markdown: bool, cx: &mut Context<Self>) {
        let content = if as_markdown {
            format::markdown(&asset.name, &asset.url, asset.category == "image")
        } else {
            asset.url.clone()
        };
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(content)) {
            Ok(_) => self.status = if as_markdown {
                "Markdown 引用已复制".to_string()
            } else {
                "资源链接已复制".to_string()
            },
            Err(error) => self.status = format!("写入剪贴板失败：{error}"),
        }
        cx.notify();
    }

    fn status_panel(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .rounded_xl()
            .p_4()
            .bg(rgb(0x182b43))
            .child("当前上传目标")
            .child(match self.config.mode {
                Mode::Demo => "本地演示 · file://",
                Mode::Yuque => "语雀 Web Session",
            })
            .child(format!("任务队列：{}", self.queue_count))
    }
}

impl Render for UploadDesk {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible: Vec<Asset> = self
            .assets
            .iter()
            .filter(|asset| !self.image_only || asset.category == "image")
            .filter(|asset| self.search.is_empty() || asset.name.to_lowercase().contains(&self.search.to_lowercase()))
            .take(150)
            .cloned()
            .collect();
        let rows: Vec<AnyElement> = visible
            .into_iter()
            .enumerate()
            .map(|(index, asset)| {
                let url_asset = asset.clone();
                let markdown_asset = asset.clone();
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(0x1a2b42))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .flex_1()
                            .child(asset.name.clone())
                            .child(format!(
                                "{} · {} · {}",
                                file_size(asset.bytes), asset.provider, asset.created_at
                            )),
                    )
                    .child(
                        Button::new(("url", index))
                            .label("复制 URL")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.copy_asset(&url_asset, false, cx);
                            })),
                    )
                    .child(
                        Button::new(("markdown", index))
                            .label("Markdown")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.copy_asset(&markdown_asset, true, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        let total = self.storage.count().unwrap_or(self.assets.len() as i64);
        let is_demo = self.config.mode == Mode::Demo;

        div()
            .flex()
            .size_full()
            .bg(rgb(0x0b1422))
            .text_color(rgb(0xf2f6ff))
            .child(
                div()
                    .w(px(232.0))
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_5()
                    .bg(rgb(0x111f31))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child("YUQUE")
                            .child("ASSET DESK"),
                    )
                    .child(
                        Button::new("upload-navigation")
                            .primary()
                            .label("上传工作台")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.image_only = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("images-navigation")
                            .label("图片资源")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.image_only = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("choose-sidebar")
                            .label("选择本地文件")
                            .on_click(cx.listener(|view, _, _, cx| view.select_files(cx))),
                    )
                    .child(div().flex_1())
                    .child(self.status_panel()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(480.0))
                    .flex()
                    .flex_col()
                    .gap_5()
                    .p_6()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child("PERSONAL WORKSPACE / FILE UPLOADER")
                            .child("上传与管理"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .p_6()
                            .rounded_xl()
                            .bg(rgb(0x1c3860))
                            .child("轻松上传，专注创作")
                            .child("将图片和附件上传后，一键复制可用于文章的 Markdown 引用")
                            .child(
                                Button::new("choose-main")
                                    .primary()
                                    .label("＋ 选择文件上传")
                                    .on_click(cx.listener(|view, _, _, cx| view.select_files(cx))),
                            )
                            .child(if is_demo {
                                "演示模式只返回本地 file:// 链接，不能公开访问"
                            } else {
                                "语雀上传采用非公开网页接口，实际可用性需账号验证"
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .p_4()
                                    .rounded_lg()
                                    .bg(rgb(0x172a40))
                                    .child(format!("已记录资源   {total}")),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .p_4()
                                    .rounded_lg()
                                    .bg(rgb(0x172a40))
                                    .child(format!("待处理任务   {}", self.queue_count)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(if self.image_only { "图片资源" } else { "最近上传" })
                            .child(Input::new(&self.search_input))
                            .child(
                                Button::new("toggle-image-filter")
                                    .label(if self.image_only { "显示全部" } else { "仅看图片" })
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.image_only = !view.image_only;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .overflow_y_scroll()
                            .children(rows),
                    )
                    .child(
                        div()
                            .p_3()
                            .rounded_lg()
                            .bg(rgb(0x142b3e))
                            .child(self.status.clone()),
                    ),
            )
    }
}

fn main() -> Result<()> {
    let config = Config::from_env();
    let storage = Storage::new(db_path()?)?;
    application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            init(cx);
            let config = config.clone();
            let storage = storage.clone();
            open_window(WindowOptions::default(), cx, move |window, cx| {
                window.set_window_title("Yuque Asset Desk — GPUI Kit");
                cx.new(|cx| UploadDesk::new(window, cx, config, storage))
            })
            .expect("无法创建 GPUI 桌面窗口");
        });
    Ok(())
}
