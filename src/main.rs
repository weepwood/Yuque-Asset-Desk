#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod format;
mod storage;
mod uploader;
mod preview;

mod desk;

use anyhow::{Context as AnyhowContext, Result};
use config::Config;
use storage::Storage;
use desk::{UploadDesk, db_path};
use gpui_kit::*;
use gpui_kit::component::theme::{Theme, ThemeMode};

fn main() -> Result<()> {
    let config = Config::from_env();
    let path = db_path()?;
    let parent = path.parent().context("缺少数据目录")?;
    std::fs::create_dir_all(parent)?;
    let instance = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(parent.join("instance.lock"))?;
    instance.try_lock().context("已有 Yuque Asset Desk 实例使用此数据目录")?;
    let storage = Storage::new(path)?;
    application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            let config = config.clone();
            let storage = storage.clone();
            let bounds = Bounds::centered(None, size(px(1160.), px(800.)), cx);
            open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), window_min_size: Some(size(px(720.),px(600.))), ..Default::default() }, cx, move |window, cx| {
                window.set_window_title("Yuque Asset Desk");
                cx.new(|cx| UploadDesk::new(window, cx, config, storage))
            })
            .expect("无法创建 GPUI 桌面窗口");
        });
    Ok(())
}
