#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod format;
mod storage;
mod uploader;

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use anyhow::{Context, Result};
use directories::ProjectDirs;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use config::{Config, Mode};
use storage::{Asset, Storage};

slint::include_modules!();

fn db_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "Weepwood", "YuqueAssetDesk")
        .context("无法找到当前系统的用户数据目录")?;
    Ok(dirs.data_local_dir().join("assets.db"))
}

fn update_assets(ui: &MainWindow, storage: &Storage) -> Result<()> {
    let search = ui.get_search_text();
    let records = storage.recent(300, search.as_str())?;
    let rows = records.iter().map(to_row).collect::<Vec<_>>();
    ui.set_assets(ModelRc::new(VecModel::from(rows)));
    ui.set_total_count(storage.count()?.min(i32::MAX as i64) as i32);
    Ok(())
}

fn to_row(item: &Asset) -> AssetRow {
    AssetRow {
        name: item.name.clone().into(),
        detail: format!("{} · {} · {}", size_label(item.bytes as u64), item.provider, item.created_at).into(),
        url: item.url.clone().into(),
        kind: item.category.clone().into(),
        provider: item.provider.clone().into(),
    }
}

fn size_label(bytes: u64) -> String {
    if bytes >= 1024 * 1024 { format!("{:.1} MB", bytes as f64 / (1024 * 1024) as f64) }
    else if bytes >= 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
    else { format!("{bytes} B") }
}

fn set_status(ui: &MainWindow, message: impl AsRef<str>) {
    ui.set_status_text(message.as_ref().into());
}

fn select_clipboard(row: AssetRow, markdown: bool) -> Result<()> {
    let text = if markdown {
        format::markdown(row.name.as_str(), row.url.as_str(), row.kind.as_str() == "image")
    } else { row.url.to_string() };
    arboard::Clipboard::new().context("无法访问系统剪贴板")?
        .set_text(text).context("剪贴板写入失败")?;
    Ok(())
}

fn main() -> Result<()> {
    let config = Config::from_env();
    let storage = Storage::new(db_path()?)?;
    let main = MainWindow::new()?;
    let tray = AppTray::new()?;
    let about = AboutWindow::new()?;
    main.set_mode_label(match config.mode {
        Mode::Demo => "本地演示 / 不上传网络".into(),
        Mode::Yuque => "语雀 Web Session".into(),
    });
    main.set_account_text(match (&config.mode, &config.creds) {
        (Mode::Demo, _) => "仅生成本地 file:// 链接".into(),
        (Mode::Yuque, Some(auth)) => format!("用户 ID: {}", auth.user_id).into(),
        (Mode::Yuque, None) => "未配置账号凭据".into(),
    });
    set_status(&main, match (&config.mode, &config.creds) {
        (Mode::Demo, _) => "演示模式：不向语雀发送任何文件",
        (Mode::Yuque, Some(_)) => "语雀配置已加载，上传接口尚需账号验证",
        (Mode::Yuque, None) => "请先配置 YUQUE_USER_ID、YUQUE_CTOKEN、YUQUE_COOKIE",
    });
    update_assets(&main, &storage)?;

    // Sequential worker minimizes idle memory and prevents duplicate uploads racing each other.
    let (sender, receiver) = mpsc::channel::<PathBuf>();
    let pending = Arc::new(AtomicUsize::new(0));
    let worker_weak = main.as_weak();
    let worker_storage = storage.clone();
    let worker_config = config.clone();
    let worker_pending = pending.clone();
    thread::spawn(move || {
        while let Ok(file_path) = receiver.recv() {
            let display = file_path.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| "文件".into());
            let weak_before = worker_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak_before.upgrade() { set_status(&ui, format!("正在处理：{display}")); }
            });
            let result = uploader::upload(&file_path, &worker_config, &worker_storage);
            let waiting = worker_pending.fetch_sub(1, Ordering::SeqCst).saturating_sub(1);
            let weak_after = worker_weak.clone();
            let storage_for_refresh = worker_storage.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak_after.upgrade() {
                    match result {
                        Ok(out) => {
                            let label = if out.duplicate { "去重成功，复用历史链接" }
                                else if out.item.provider == "demo" { "已添加本地演示资源（非公网链接）" }
                                else { "语雀返回上传地址，已写入 SQLite" };
                            set_status(&ui, if waiting > 0 { format!("{label} · 队列剩余 {waiting} 项") } else { label.to_string() });
                            if let Err(error) = update_assets(&ui, &storage_for_refresh) {
                                set_status(&ui, format!("资源入库成功，但界面刷新失败：{error}"));
                            }
                        }
                        Err(error) => set_status(&ui, format!("上传失败：{error}（队列剩余 {waiting} 项）")),
                    }
                }
            });
        }
    });

    let send_files = sender.clone();
    let pick_weak = main.as_weak();
    let pick_pending = pending.clone();
    main.on_pick_files(move || {
        // The native modal dialog is intentionally run on the UI thread, not the upload worker.
        if let Some(files) = rfd::FileDialog::new().set_title("选择要上传的图片或附件").pick_files() {
            let count = files.len();
            for file in files {
                pick_pending.fetch_add(1, Ordering::SeqCst);
                if send_files.send(file).is_err() {
                    pick_pending.fetch_sub(1, Ordering::SeqCst);
                }
            }
            if let Some(ui) = pick_weak.upgrade() { set_status(&ui, format!("已加入 {count} 个文件，依次上传")); }
        }
    });

    let search_weak = main.as_weak();
    let search_db = storage.clone();
    main.on_search_changed(move |text| {
        if let Some(ui) = search_weak.upgrade() {
            ui.set_search_text(text);
            if let Err(err) = update_assets(&ui, &search_db) { set_status(&ui, format!("搜索失败：{err}")); }
        }
    });

    let copy_weak = main.as_weak();
    main.on_copy_url(move |index| {
        if let Some(ui) = copy_weak.upgrade() {
            if let Some(asset) = ui.get_assets().row_data(index as usize) {
                match select_clipboard(asset, false) {
                    Ok(()) => set_status(&ui, "已复制 URL（演示模式只适用于本机）"),
                    Err(err) => set_status(&ui, format!("无法复制：{err}")),
                }
            }
        }
    });
    let markdown_weak = main.as_weak();
    main.on_copy_markdown(move |index| {
        if let Some(ui) = markdown_weak.upgrade() {
            if let Some(asset) = ui.get_assets().row_data(index as usize) {
                match select_clipboard(asset, true) {
                    Ok(()) => set_status(&ui, "已复制 Markdown 引用（演示模式只适用于本机）"),
                    Err(err) => set_status(&ui, format!("无法复制：{err}")),
                }
            }
        }
    });

    let hide_weak = main.as_weak();
    main.on_hide_main(move || {
        if let Some(ui) = hide_weak.upgrade() { let _ = ui.hide(); }
    });
    let help_weak = main.as_weak();
    main.on_show_help(move || {
        if let Some(ui) = help_weak.upgrade() {
            set_status(&ui, "配置说明请查看压缩包中的 README.md；不要在聊天中分享 Cookie");
        }
    });

    let about_weak = about.as_weak();
    main.on_show_about(move || {
        if let Some(dialog) = about_weak.upgrade() { let _ = dialog.show(); }
    });
    let about_close = about.as_weak();
    about.on_close_about(move || {
        if let Some(dialog) = about_close.upgrade() { let _ = dialog.hide(); }
    });

    let tray_open_weak = main.as_weak();
    tray.on_open_main(move || {
        if let Some(ui) = tray_open_weak.upgrade() { let _ = ui.show(); }
    });
    let tray_click_weak = main.as_weak();
    tray.on_clicked(move || {
        if let Some(ui) = tray_click_weak.upgrade() { let _ = ui.show(); }
    });
    tray.on_quit_app(|| { let _ = slint::quit_event_loop(); });

    main.show()?;
    tray.show()?;
    slint::run_event_loop()?;
    Ok(())
}
