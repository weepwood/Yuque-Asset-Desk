//! Main-thread native desktop integration. No clipboard monitoring.
use gpui_kit::*;
use crate::desk::UploadDesk;
use tray_icon::{TrayIconBuilder,Icon,menu::{Menu,MenuItem,MenuEvent}};
use global_hotkey::{GlobalHotKeyManager,GlobalHotKeyEvent,HotKeyState,hotkey::{HotKey,Modifiers as HotkeyModifiers,Code}};

fn icon()->anyhow::Result<Icon>{
    let mut rgba=vec![0u8;24*24*4];
    for y in 0..24 {for x in 0..24{
        let frame=(3..21).contains(&x)&&(4..20).contains(&y)&&(x==3||x==20||y==4||y==19);
        let peak=(7..17).contains(&x)&&y==16-(x as i32-12).abs() as usize;
        let sun=(14..17).contains(&x)&&(7..10).contains(&y);
        if frame||peak||sun{let i=(y*24+x)*4;rgba[i..i+4].copy_from_slice(&[65,145,105,255]);}
    }}
    Ok(Icon::from_rgba(rgba,24,24)?)
}
pub fn install(window:AnyWindowHandle,desk:Entity<UploadDesk>,cx:&mut App){
    let menu=Menu::new();
    let show=MenuItem::new("打开相册",true,None);
    let paste=MenuItem::new("上传剪贴板图片 / 文件",true,None);
    let quit=MenuItem::new("退出",true,None);
    let tray=(||->anyhow::Result<_>{
        menu.append(&show)?;menu.append(&paste)?;menu.append(&quit)?;
        Ok(TrayIconBuilder::new().with_icon(icon()?).with_tooltip("Yuque Asset Desk").with_menu(Box::new(menu)).build()?)
    })();
    let hotkey=HotKey::new(Some(if cfg!(target_os="macos"){HotkeyModifiers::META|HotkeyModifiers::SHIFT}else{HotkeyModifiers::CONTROL|HotkeyModifiers::SHIFT}),Code::KeyU);
    let manager=GlobalHotKeyManager::new().and_then(|manager|{manager.register(hotkey)?;Ok(manager)});
    let tray_ok=tray.is_ok();let hotkey_ok=manager.is_ok();
    if let Err(e)=&tray{eprintln!("DESKTOP tray unavailable: {e}");}
    if let Err(e)=&manager{eprintln!("DESKTOP shortcut unavailable: {e}");}
    eprintln!("DESKTOP tray={tray_ok} hotkey={hotkey_ok}");
    desk.update(cx,|v,cx|v.desktop_status(tray_ok||hotkey_ok,format!("托盘：{} · 全局快捷键：{}",if tray_ok{"已启用"}else{"不可用"},if hotkey_ok{if cfg!(target_os="macos"){"⌘⇧U"}else{"Ctrl+Shift+U"}}else{"注册失败"}),cx));
    if tray_ok {
        let _=window.update(cx,|_,window,cx|window.on_window_should_close(cx,|w,_|{w.minimize_window();false}));
    }
    let show_id=show.id().clone();let paste_id=paste.id().clone();let quit_id=quit.id().clone();
    let weak=desk.downgrade();
    cx.spawn(async move|cx|{
        // Keep native objects alive on the GPUI main thread.
        let _tray=tray.ok();let manager=manager.ok();
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
                    if desk.update(cx,|v,cx|v.desktop_can_quit(cx)){cx.quit();return false;}
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
