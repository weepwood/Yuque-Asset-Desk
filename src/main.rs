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
            let light = std::env::var("YUQUE_QA_LIGHT").as_deref() == Ok("1") && std::env::var_os("YUQUE_DATA_DIR").is_some() && config.mode == config::Mode::Demo;
            Theme::change(if light {ThemeMode::Light} else {ThemeMode::Dark}, None, cx);
            let config = config.clone();
            let storage = storage.clone();
            let narrow = std::env::var("YUQUE_QA_NARROW").as_deref() == Ok("1") && std::env::var_os("YUQUE_DATA_DIR").is_some() && config.mode == config::Mode::Demo;
            let bounds = Bounds::centered(None, size(px(if narrow {780.} else {1160.}), px(if narrow {640.} else {800.})), cx);
            open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), window_min_size: Some(size(px(720.),px(600.))), ..Default::default() }, cx, move |window, cx| {
                window.set_window_title("Yuque Asset Desk");
                #[cfg(feature="qa")]
                if config.mode == config::Mode::Demo && std::env::var_os("YUQUE_DATA_DIR").is_some() {
                    if let Some(path) = std::env::var_os("YUQUE_QA_CAPTURE") {
                        // Capture the renderer's own frame, not the user's desktop.
                        window.spawn(cx, async move |cx| {
                            cx.background_executor().timer(std::time::Duration::from_millis(650)).await;
                            // First draw initiates asynchronous image decoding. Allow
                            // the loader to complete before recording the final frame.
                            for _ in 0..3 {
                                let _=cx.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
                                cx.background_executor().timer(std::time::Duration::from_millis(250)).await;
                            }
                            let result = cx.update(|window,cx| {
                                window.refresh();
                                window.draw(cx).clear(cx);
                                let result = window.render_to_image().and_then(|image| image.save(std::path::PathBuf::from(path)).map_err(Into::into));
                                eprintln!("QA_CAPTURE {:?}", result);
                                cx.quit();
                            });
                            if let Err(e)=result { eprintln!("QA_CAPTURE window error: {e}"); }
                        }).detach();
                    }
                }
                cx.new(|cx| UploadDesk::new(window, cx, config, storage))
            })
            .expect("无法创建 GPUI 桌面窗口");
        });
    Ok(())
}
