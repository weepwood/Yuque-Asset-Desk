#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod data_management;
mod format;
mod storage;
mod uploader;
mod update;
mod preview;
mod high_preview;

mod desk;
#[cfg(any(target_os="macos",target_os="windows"))]
mod desktop;

use anyhow::{Context as AnyhowContext, Result};
use config::Config;
use storage::Storage;
use desk::{UploadDesk, db_path};
use gpui_kit::*;


fn main() -> Result<()> {
    let path = db_path()?;
    let mut args=std::env::args().skip(1);
    if args.next().as_deref()==Some("--reset-after-exit"){
        let token=args.next().context("缺少重置授权")?;
        anyhow::ensure!(args.next().is_none(),"重置参数无效");
        return data_management::reset_helper(path,&token);
    }
    let config = Config::from_env();
    let parent = path.parent().context("缺少数据目录")?;
    std::fs::create_dir_all(parent)?;
    let instance = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(parent.join("instance.lock"))?;
    instance.try_lock().context("已有 Yuque Asset Desk 实例使用此数据目录")?;
    anyhow::ensure!(!parent.join(data_management::REQUEST).exists(),"应用重置尚未完成；请等待清理结束");
    let storage = Storage::new(path)?;
    application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            init(cx);
            let prefs=desk::preferences::Preferences::decode(storage.preferences().unwrap_or_default());
            let light = std::env::var("YUQUE_QA_LIGHT").as_deref() == Ok("1") && std::env::var_os("YUQUE_DATA_DIR").is_some() && config.mode == config::Mode::Demo;
            desk::design::apply(!light && prefs.dark,None,cx);
            let config = config.clone();
            let storage = storage.clone();
            let narrow = std::env::var("YUQUE_QA_NARROW").as_deref() == Ok("1") && std::env::var_os("YUQUE_DATA_DIR").is_some() && config.mode == config::Mode::Demo;
            let minimum=cfg!(feature="qa")&&std::env::var_os("YUQUE_QA_MINIMUM").is_some()&&std::env::var_os("YUQUE_DATA_DIR").is_some()&&config.mode==config::Mode::Demo;
            let bounds = Bounds::centered(None, size(px(if minimum{720.}else if narrow {780.} else {prefs.width}), px(if minimum{600.}else if narrow {640.} else {prefs.height})), cx);
            let (window_handle,desk)=open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), window_min_size: Some(size(px(720.),px(600.))), ..Default::default() }, cx, move |window, cx| {
                window.set_window_title("Yuque Asset Desk");
                #[cfg(feature="qa")]
                if config.mode == config::Mode::Demo && std::env::var_os("YUQUE_DATA_DIR").is_some() {
                    if std::env::var_os("YUQUE_QA_PERF").is_some(){
                        window.spawn(cx,async move|cx|{
                            cx.background_executor().timer(std::time::Duration::from_millis(1800)).await;
                            let mut times=Vec::new();
                            for _ in 0..120{
                                let time=cx.update(|window,cx|{
                                    let start=std::time::Instant::now();
                                    window.dispatch_event(PlatformInput::Scroll(ScrollEvent{position:point(px(600.),px(420.)),delta:ScrollDelta::Pixels(point(px(0.),px(-72.))),..Default::default()}),cx);
                                    window.refresh();window.draw(cx).clear(cx);
                                    start.elapsed().as_secs_f64()*1000.
                                }).unwrap_or(0.);
                                times.push(time);
                                cx.background_executor().timer(std::time::Duration::from_millis(16)).await;
                            }
                            times.sort_by(|a,b|a.total_cmp(b));
                            eprintln!("PERF render_cpu_ms p50={:.3} p95={:.3} max={:.3} samples={}",times[60],times[114],times[119],times.len());
                            let samples=std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                            let shared=samples.clone();
                            _=cx.update(|window,_|perf_native_frames(window,shared,std::time::Instant::now()));
                            cx.background_executor().timer(std::time::Duration::from_secs(6)).await;
                            if samples.borrow().len()<120{
                                eprintln!("PERF native_callback_unavailable samples={} (display frame source not delivering enough callbacks)",samples.borrow().len());
                                _=cx.update(|_,cx|cx.quit());
                            }
                        }).detach();
                    }
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
            #[cfg(feature="qa")]
            if std::env::var_os("YUQUE_DATA_DIR").is_some() && (std::env::var_os("YUQUE_QA_PERF").is_some() || std::env::var("YUQUE_DESKTOP_SMOKE").as_deref()==Ok("2")) {
                // An inactive CLI-launched test window may be occluded: activate
                // the isolated app before attempting platform frame sampling.
                cx.activate(true);
                let _=window_handle.update(cx,|_,w,_|w.activate_window());
            }
            #[cfg(feature="qa")]
            if let Some(out)=std::env::var_os("YUQUE_QA_PATH").filter(|_|std::env::var_os("YUQUE_DATA_DIR").is_some()&&std::env::var("YUQUE_MODE").as_deref()==Ok("demo")){
                let root=std::path::PathBuf::from(out);
                let view=desk.clone();
                cx.spawn(async move|cx|{
                    cx.background_executor().timer(std::time::Duration::from_millis(1800)).await;
                    // Same isolated 780×640 fixture and coordinates as the pointer regression.
                    for (name,mouse,key,expected) in [
                        ("00-gallery",None,None,(0,None,false,false)),
                        ("01-select",Some((263.,216.,Modifiers::default())),None,(1,Some(3),false,false)),
                        ("02-range",Some((465.,216.,Modifiers{shift:true,..Default::default()})),None,(2,Some(2),false,false)),
                        ("03-toggle",Some((465.,216.,Modifiers{control:true,..Default::default()})),None,(1,Some(3),false,false)),
                        ("04-preview",None,Some("space"),(1,Some(3),true,false)),
                        ("05-next",None,Some("right"),(1,Some(2),true,false)),
                        ("06-return",None,Some("escape"),(1,Some(2),false,false)),
                        ("07-inspector",None,Some("ctrl-i"),(1,Some(2),false,true)),
                    ]{
                        let result=cx.update(|cx|window_handle.update(cx,|_,w,cx|{
                            w.refresh();w.draw(cx).clear(cx);
                            if let Some((x,y,modifiers))=mouse{
                                let position=point(px(x),px(y));
                                w.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{position,button:MouseButton::Left,click_count:1,modifiers,..Default::default()}),cx);
                                w.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{position,button:MouseButton::Left,click_count:1,modifiers,..Default::default()}),cx);
                            }
                            if let Some(key)=key{w.dispatch_keystroke(Keystroke::parse(key).expect("valid QA key"),cx);}
                        }));
                        if result.is_err(){eprintln!("PATH_CAPTURE window unavailable");break;}
                        cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                        let result=cx.update(|cx|window_handle.update(cx,|_,w,cx|{
                            w.refresh();w.draw(cx).clear(cx);
                            let state=view.read(cx).qa_path_state();
                            let capture=w.render_to_image().and_then(|image|image.save(root.join(format!("{name}.png"))).map_err(Into::into));
                            eprintln!("PATH_CAPTURE {name} state={state:?} expected={expected:?} capture={capture:?}");
                            state==expected&&capture.is_ok()
                        }));
                        if !matches!(result,Ok(true)){eprintln!("PATH_CAPTURE FAILED");break;}
                    }
                    let _=cx.update(|cx|cx.quit());
                }).detach();
            }
            let close_desk=desk.downgrade();
            let _=window_handle.update(cx,|_,w,cx|w.on_window_should_close(cx,move|_,cx|{
                if let Some(desk)=close_desk.upgrade(){desk.read(cx).flush_preferences();desk.update(cx,|v,cx|v.desktop_can_quit(cx))}else{true}
            }));
            let quit_desk=desk.downgrade();
            cx.on_app_quit(move|cx|{
                if let Some(desk)=quit_desk.upgrade(){
                    desk.read(cx).flush_preferences();
                    #[cfg(feature="qa")] desk.read(cx).report_perf(cx);
                }
                async {}
            }).detach();
            #[cfg(any(target_os="macos",target_os="windows"))]
            if std::env::var("YUQUE_DESKTOP_INTEGRATION").as_deref()!=Ok("0")
                && (std::env::var_os("YUQUE_DATA_DIR").is_none() || matches!(std::env::var("YUQUE_DESKTOP_SMOKE").as_deref(),Ok("1"|"2")))
                && std::env::var_os("YUQUE_QA_CAPTURE").is_none() {
                desktop::install(window_handle,desk,cx);
                #[cfg(feature="qa")]
                if std::env::var("YUQUE_DESKTOP_SMOKE").as_deref()==Ok("2") && std::env::var_os("YUQUE_DATA_DIR").is_some(){
                    cx.spawn(async move|cx|{
                        cx.background_executor().timer(std::time::Duration::from_millis(600)).await;
                        cx.update(|cx|{let _=window_handle.update(cx,|_,w,_|{eprintln!("DESKTOP visibility_before={:?}",w.visibility());w.minimize_window();});});
                        cx.background_executor().timer(std::time::Duration::from_millis(600)).await;
                        cx.update(|cx|{let _=window_handle.update(cx,|_,w,cx|{eprintln!("DESKTOP visibility_minimized={:?}",w.visibility());cx.activate(true);w.activate_window();});});
                        cx.background_executor().timer(std::time::Duration::from_millis(900)).await;
                        cx.update(|cx|{let _=window_handle.update(cx,|_,w,_|eprintln!("DESKTOP visibility_restored={:?}",w.visibility()));cx.quit();});
                    }).detach();
                }
            }
        });
    Ok(())
}


#[cfg(feature="qa")]
fn perf_native_frames(window:&Window,intervals:std::rc::Rc<std::cell::RefCell<Vec<f64>>>,last:std::time::Instant){
    window.on_next_frame(move|window,cx|{
        let now=std::time::Instant::now();
        intervals.borrow_mut().push(now.duration_since(last).as_secs_f64()*1000.);
        if intervals.borrow().len()>=120 {
            let mut values=intervals.borrow_mut();
            // First callback interval includes startup scheduling.
            values.remove(0);values.sort_by(|a,b|a.total_cmp(b));
            eprintln!("PERF native_callback_ms p50={:.3} p95={:.3} samples={}",values[59],values[112],values.len());
            cx.quit();
        }else{
            window.dispatch_event(PlatformInput::Scroll(ScrollEvent{position:point(px(600.),px(420.)),delta:ScrollDelta::Pixels(point(px(0.),px(-72.))),..Default::default()}),cx);
            window.refresh();
            perf_native_frames(window,intervals,now);
        }
    });
}
