//! Main-thread native desktop integration. No clipboard monitoring.
use gpui_kit::*;
use crate::desk::UploadDesk;
use tray_icon::{TrayIconBuilder,Icon,menu::{Menu,MenuItem,MenuEvent}};
use global_hotkey::{GlobalHotKeyManager,GlobalHotKeyEvent,HotKeyState,hotkey::{HotKey,Modifiers as HotkeyModifiers,Code}};

fn icon()->anyhow::Result<Icon>{
    let bytes=if cfg!(target_os="macos"){include_bytes!("../assets/branding/tray-template.png").as_slice()}else{include_bytes!("../assets/branding/tray-color.png").as_slice()};
    let image=image::load_from_memory_with_format(bytes,image::ImageFormat::Png)?.into_rgba8();
    let (w,h)=image.dimensions();
    Ok(Icon::from_rgba(image.into_raw(),w,h)?)
}
pub fn install(window:AnyWindowHandle,desk:Entity<UploadDesk>,cx:&mut App){
    let menu=Menu::new();
    let show=MenuItem::new("打开相册",true,None);
    let paste=MenuItem::new("上传剪贴板图片 / 文件",true,None);
    let quit=MenuItem::new("退出",true,None);
    let tray=(||->anyhow::Result<_>{
        menu.append(&show)?;menu.append(&paste)?;menu.append(&quit)?;
        let builder=TrayIconBuilder::new();
        #[cfg(target_os="macos")] let builder=builder.with_icon_templated(icon()?);
        #[cfg(target_os="windows")] let builder=builder.with_icon(icon()?);
        Ok(builder.with_tooltip("Yuque Asset Desk").with_menu(Box::new(menu)).build()?)
    })();
    let shortcut=desk.read(cx).shortcut().to_owned();
    let hotkey=HotKey::new(Some(if cfg!(target_os="macos"){HotkeyModifiers::META|HotkeyModifiers::SHIFT}else{HotkeyModifiers::CONTROL|HotkeyModifiers::SHIFT}),if shortcut=="I"{Code::KeyI}else{Code::KeyU});
    let manager=if shortcut=="off"{None}else{Some(GlobalHotKeyManager::new().and_then(|manager|{manager.register(hotkey)?;Ok(manager)}))};
    let tray_ok=tray.is_ok();let hotkey_ok=manager.as_ref().is_some_and(|m|m.is_ok());
    if let Err(e)=&tray{eprintln!("DESKTOP tray unavailable: {e}");}
    if let Some(Err(e))=&manager{eprintln!("DESKTOP shortcut unavailable: {e}");}
    eprintln!("DESKTOP tray={tray_ok} hotkey={hotkey_ok}");
    desk.update(cx,|v,cx|v.desktop_status(tray_ok||hotkey_ok,format!("托盘：{} · 全局快捷键：{}",if tray_ok{"已启用"}else{"不可用"},if shortcut=="off"{"已禁用".to_string()}else if hotkey_ok{format!("{}{}",if cfg!(target_os="macos"){"⌘⇧"}else{"Ctrl+Shift+"},shortcut)}else{"注册失败，请切换 U/I 并重启".into()}),cx));
    let close_desk=desk.downgrade();
    let _=window.update(cx,|_,window,cx|window.on_window_should_close(cx,move|w,cx|{
        if let Some(desk)=close_desk.upgrade(){
            desk.read(cx).flush_preferences();
            if tray_ok{w.minimize_window();return false;}
            return desk.update(cx,|v,cx|v.desktop_can_quit(cx));
        }
        true
    }));
    let show_id=show.id().clone();let paste_id=paste.id().clone();let quit_id=quit.id().clone();
    let weak=desk.downgrade();
    cx.spawn(async move|cx|{
        // Keep native objects alive on the GPUI main thread.
        let _tray=tray.ok();let manager=manager.and_then(Result::ok);
        loop{
            cx.background_executor().timer(std::time::Duration::from_millis(150)).await;
            let mut activate=false;let mut upload=false;let mut exit=false;
            while let Ok(event)=MenuEvent::receiver().try_recv(){
                if event.id==show_id{activate=true;}
                if event.id==paste_id{activate=true;upload=true;}
                if event.id==quit_id{exit=true;}
            }
            while let Ok(event)=GlobalHotKeyEvent::receiver().try_recv(){
                if hotkey_ok && event.id==hotkey.id()&&event.state==HotKeyState::Pressed{activate=true;}
            }
            if !activate&&!exit{continue;}
            let keep=cx.update(|cx|{
                let Some(desk)=weak.upgrade()else{return false;};
                if exit{
                    if desk.update(cx,|v,cx|v.desktop_can_quit(cx)){desk.read(cx).flush_preferences();cx.quit();return false;}
                    activate=true;
                }
                if activate{
                    cx.activate(true);
                    if window.update(cx,|_,w,_|w.activate_window()).is_err(){return false;}
                    if upload{desk.update(cx,|v,cx|v.desktop_quick_upload(cx));}
                }
                true
            });
            if !keep{break;}
        }
        if let Some(manager)=manager{let _=manager.unregister(hotkey);}
    }).detach();
}
