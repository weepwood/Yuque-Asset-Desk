use super::*;
use qa_gpui::{TestAppContext, VisualTestContext};

#[::core::prelude::v1::test]
fn native_clicks_switch_resource_and_task_views() {
    let root = std::env::temp_dir().join(format!("yuque-ui-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db = Storage::new(root.join("assets.db")).unwrap();
    let config = Config {mode:Mode::Demo,creds:None,max_file_mb:1};
    db.insert_or_get(&crate::storage::NewAsset {
        name:"fixture.txt".into(),bytes:10,category:"attachment".into(),provider:"demo".into(),
        owner_id:"local".into(),url:"file:///fixture.txt".into(),sha256:"test-only".into(),filekey:"".into()
    }).unwrap();
    let app=TestAppContext::single();
    app.update(gpui_kit::init);
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{
        window_bounds:Some(WindowBounds::Windowed(Bounds {origin:Point::default(),size:size(px(1160.),px(800.))})),
        ..Default::default()
    },cx,move|window,cx|cx.new(|cx|UploadDesk::new(window,cx,config,db))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.len()),1);
    // Native GPUI pointer events in a fixed 1160×800 logical viewport.
    let bounds=ui.debug_bounds("nav-images").unwrap(); ui.simulate_click(bounds.center(),Modifiers::default());
    app.run_until_parked();
    assert_eq!(app.read(|cx|desk.read(cx).resource.category),"image");
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.len()),0);
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("nav-jobs").unwrap(); ui.simulate_click(bounds.center(),Modifiers::default());
    app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).page==Page::Jobs));
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("nav-settings").unwrap(); ui.simulate_click(bounds.center(),Modifiers::default());
    app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).page.settings() && !desk.read(cx).inspector_open));
    let dark=app.read(|cx|desk.read(cx).dark);
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("settings-theme").unwrap(); ui.simulate_click(bounds.center(),Modifiers::default());
    app.run_until_parked();
    assert_ne!(app.read(|cx|desk.read(cx).dark),dark);
    assert!(app.read(|cx|desk.read(cx).prefs.comfortable));
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("settings-comfortable").unwrap();
    ui.simulate_click(bounds.center(),Modifiers::default());app.run_until_parked();
    assert!(!app.read(|cx|desk.read(cx).prefs.comfortable));
    assert_eq!(app.read(|cx|desk.read(cx).spacing().gap),10.);
    app.update(|cx|desk.update(cx,|v,_|v.flush_preferences()));
    assert!(!preferences::Preferences::decode(Storage::new(root.join("assets.db")).unwrap().preferences().unwrap()).comfortable);
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("open-data-settings").unwrap();ui.simulate_click(bounds.center(),Modifiers::default());
    app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).page==Page::Data));
    assert!(app.read(|cx|desk.read(cx).data_usage.is_some()));
    // A pending preference snapshot must finish before destructive operations.
    app.executor().advance_clock(std::time::Duration::from_millis(250));app.run_until_parked();
    // Arm, but do not confirm a destructive operation. No credentials touched.
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("arm-reset").unwrap();ui.simulate_click(bounds.center(),Modifiers::default());app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).reset_armed));
    assert!(root.join("assets.db").exists());
    app.update(|cx|desk.update(cx,|v,cx|{v.navigate_page(Page::Account,cx);}));
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("identify-account").unwrap();
    ui.simulate_click(bounds.center(),Modifiers::default());app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).account.read(cx).identity_status.contains("请先填写")));
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("save-account").unwrap();
    ui.simulate_click(bounds.center(),Modifiers::default());app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).account.read(cx).identity_status.contains("网页 Cookie")));
    assert!(app.read(|cx|desk.read(cx).storage.accounts().unwrap().is_empty()));


    drop(ui);drop(desk);drop(app);
    let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn album_native_scroll_and_density_controls() {
    let root=std::env::temp_dir().join(format!("yuque-gallery-ui-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    for i in 0..150{
        db.insert_or_get(&crate::storage::NewAsset{name:format!("fixture-{i}.txt"),bytes:10,category:"attachment".into(),provider:"demo".into(),owner_id:"local".into(),url:format!("file:///fixture-{i}.txt"),sha256:format!("fixture-{i}"),filekey:String::new()}).unwrap();
    }
    let app=TestAppContext::single();
    app.update(gpui_kit::init);
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{
        window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(1160.),px(800.))})),
        ..Default::default()
    },cx,move|window,cx|cx.new(|cx|UploadDesk::new(window,cx,config,db))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.len()),150);
    let before=app.read(|cx|desk.read(cx).resource.album_list.logical_scroll_top().item_ix);
    ui.simulate_event(ScrollEvent{position:point(px(600.),px(550.)),delta:ScrollDelta::Pixels(point(px(0.),px(-1000.))),..Default::default()});
    app.run_until_parked();
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    assert!(app.read(|cx|desk.read(cx).resource.album_list.logical_scroll_top().item_ix)>before);
    // The compact toolbar is independent of scroll position.
    let slider=ui.debug_bounds("density-slider").expect("native density slider");
    ui.simulate_click(point(slider.left()+slider.size.width*0.8,slider.center().y),Modifiers::default());
    app.run_until_parked();
    let value=app.read(|cx|desk.read(cx).resource.tile_size);
    assert!(value>164. && value<=240.,"slider value {value}");
    assert_eq!(app.read(|cx|desk.read(cx).prefs.tile),value);

    app.update(|cx|desk.update(cx,|v,cx|{v.resource.masonry=true;cx.notify();}));
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    ui.simulate_event(ScrollEvent{position:point(px(600.),px(420.)),delta:ScrollDelta::Pixels(point(px(0.),px(-1000.))),..Default::default()});
    app.run_until_parked();
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    let top=app.read(|cx|-f32::from(desk.read(cx).resource.masonry_scroll.offset().y));
    assert!(top>0.,"masonry scroll must move");
    let anchor=app.read(|cx|desk.read(cx).resource.masonry_plan.anchor(top)).unwrap();
    app.update(|cx|desk.update(cx,|v,cx|{v.resource.tile_size=180.;cx.notify();}));
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    let restored=app.read(|cx|{
        let r=&desk.read(cx).resource;
        let expected=r.masonry_plan.restore(Some(anchor)).unwrap();
        let actual=-f32::from(r.masonry_scroll.offset().y);
        (actual-expected).abs()
    });
    assert!(restored<2.,"masonry anchor drift: {restored}");
    let anchor=app.read(|cx|{let r=&desk.read(cx).resource;r.masonry_plan.anchor(-f32::from(r.masonry_scroll.offset().y))}).unwrap();
    app.update(|cx|desk.update(cx,|v,cx|v.set_comfortable(false,cx)));
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    assert!(app.read(|cx|{let r=&desk.read(cx).resource;(r.masonry_plan.restore(Some(anchor)).unwrap()+f32::from(r.masonry_scroll.offset().y)).abs()<2.}));

    let toggle=ui.debug_bounds("immersive-toggle").expect("immersive control");
    ui.simulate_click(toggle.center(),Modifiers::default());app.run_until_parked();
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    assert!(app.read(|cx|desk.read(cx).immersive));
    let exit=ui.debug_bounds("immersive-toggle").expect("immersive exit");
    ui.simulate_click(exit.center(),Modifiers::default());app.run_until_parked();
    assert!(!app.read(|cx|desk.read(cx).immersive));


    drop(ui);drop(desk);drop(app);
    let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn continuous_loading_keeps_selection_and_preferences(){
    let root=std::env::temp_dir().join(format!("yuque-stream-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    for i in 0..310{
        db.insert_or_get(&crate::storage::NewAsset{name:format!("fixture-{i}.txt"),bytes:10,category:"attachment".into(),provider:"demo".into(),owner_id:"local".into(),url:format!("file:///fixture-{i}.txt"),sha256:format!("fixture-{i}"),filekey:String::new()}).unwrap();
    }
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let db_for_view=db.clone();
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{
        window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(1160.),px(800.))})),..Default::default()
    },cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db_for_view))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.len()),150);
    let id=app.read(|cx|desk.read(cx).resource.assets[0].id);
    app.update(|cx|desk.update(cx,|v,cx|{v.resource.selected_ids.insert(id);cx.notify();}));
    for _ in 0..45{
        ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
        ui.simulate_event(ScrollEvent{position:point(px(600.),px(450.)),delta:ScrollDelta::Pixels(point(px(0.),px(-480.))),..Default::default()});
        app.run_until_parked();
        ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
        app.run_until_parked();
        if app.read(|cx|desk.read(cx).resource.assets.len())>=300{break;}
    }
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.len()),300);
    assert!(app.read(|cx|desk.read(cx).resource.selected_ids.contains(&id)));
    app.update(|cx|desk.update(cx,|v,cx|v.load_more(cx)));app.run_until_parked();
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.len()),310);
    assert!(!app.read(|cx|desk.read(cx).resource.has_more));
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.iter().map(|a|a.id).collect::<std::collections::HashSet<_>>().len()),310);
    app.update(|cx|desk.update(cx,|v,cx|{
        v.dark=false;v.resource.masonry=true;v.resource.oldest=true;v.resource.favorites_only=true;
        v.resource.tile_size=196.;v.resource.group_dates=false;v.prefs.shortcut="I".into();v.prefs.width=920.;v.prefs.height=680.;
        v.persist_preferences(cx);v.flush_preferences();
    }));
    let restored=preferences::Preferences::decode(db.preferences().unwrap());
    assert_eq!(restored.layout,"masonry");assert_eq!(restored.tile,196.);assert_eq!(restored.shortcut,"I");
    assert!(!restored.dark && restored.oldest && restored.favorites && !restored.dates);
    assert_eq!((restored.width,restored.height),(920.,680.));
    drop(ui);drop(desk);drop(app);drop(db);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn isolated_clipboard_image_creates_offline_resource(){
    let root=std::env::temp_dir().join(format!("yuque-clipboard-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let db_for_view=db.clone();
    let (_,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions::default(),cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db_for_view))).unwrap());
    app.run_until_parked();
    let mut png=std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::new(64,48)).write_to(&mut png,image::ImageFormat::Png).unwrap();
    // TestAppContext uses a private clipboard, not the OS pasteboard.
    app.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png,png.into_inner())));
    app.update(|cx|desk.update(cx,|v,cx|v.desktop_quick_upload(cx)));
    app.run_until_parked();
    let (items,_,_)=db.page_album(150,"","image",Some(("demo","local")),None,false,None,false).unwrap();
    assert_eq!(items.len(),1);assert!(preview::cached_path(&root,&items[0]).is_file());
    assert!(root.join("clipboard").is_dir());
    assert!(!app.read(|cx|desk.read(cx).queue.read(cx).busy));
    drop(desk);drop(app);drop(db);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn native_filter_layout_and_card_context_menu(){
    let root=std::env::temp_dir().join(format!("yuque-menu-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let asset=db.insert_or_get(&crate::storage::NewAsset{name:"fixture.txt".into(),bytes:10,category:"attachment".into(),provider:"demo".into(),owner_id:"local".into(),url:"file:///fixture.txt".into(),sha256:"fixture-menu".into(),filekey:String::new()}).unwrap();
    let id=asset.id;
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{
        window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(1160.),px(800.))})),..Default::default()
    },cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let filter=ui.debug_bounds("filter-trigger").unwrap();
    ui.simulate_click(filter.center(),Modifiers::default());app.run_until_parked();
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    ui.simulate_keystrokes("down enter");app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).resource.favorites_only));
    app.update(|cx|desk.update(cx,|v,cx|{v.resource.favorites_only=false;v.reset_history(cx);}));app.run_until_parked();
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let layout=ui.debug_bounds("layout-trigger").unwrap();
    ui.simulate_click(layout.center(),Modifiers::default());app.run_until_parked();
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    ui.simulate_keystrokes("down down down enter");app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).resource.masonry));
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let tile=ui.debug_bounds("tile-1").unwrap();
    ui.simulate_mouse_move(tile.center(),None,Modifiers::default());app.run_until_parked();
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    assert_eq!(app.read(|cx|desk.read(cx).resource.hover_asset),Some(id));
    ui.simulate_mouse_down(tile.center(),MouseButton::Right,Modifiers::default());
    ui.simulate_mouse_up(tile.center(),MouseButton::Right,Modifiers::default());app.run_until_parked();
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    // Keyboard navigate to 收藏 (fourth selectable action); never modify OS clipboard.
    ui.simulate_keystrokes("down down down down enter");app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).resource.favorites.contains(&id)));
    drop(ui);drop(desk);drop(app);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn gallery_external_file_drop_uses_existing_queue(){
    use qa_gpui::{FileDropEvent,InputEvent};
    let root=std::env::temp_dir().join(format!("yuque-drop-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let source=root.join("drop.png");
    image::RgbImage::new(64,48).save(&source).unwrap();
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let db_view=db.clone();
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{
        window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(1160.),px(800.))})),..Default::default()
    },cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db_view))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let pos=point(px(600.),px(420.));
    ui.update(|w,cx|{w.dispatch_event(FileDropEvent::Entered{position:pos,paths:ExternalPaths([source.clone()].into_iter().collect())}.to_platform_input(),cx);w.refresh();w.draw(cx).clear(cx);});
    ui.update(|w,cx|{w.dispatch_event(FileDropEvent::Submit{position:pos}.to_platform_input(),cx);});
    app.run_until_parked();
    let (items,_,_)=db.page_album(150,"","image",Some(("demo","local")),None,false,None,false).unwrap();
    assert_eq!(items.len(),1);
    assert!(preview::cached_path(&root,&items[0]).is_file());
    assert!(!app.read(|cx|desk.read(cx).queue.read(cx).busy));
    app.update(|cx|desk.update(cx,|v,cx|{v.queue.update(cx,|q,_|q.busy=true);v.start_upload(vec![source],cx);assert!(v.status.contains("未加入队列"));v.queue.update(cx,|q,_|q.busy=false);}));
    assert_eq!(db.jobs("demo","local",None).unwrap().len(),1);
    drop(ui);drop(desk);drop(app);drop(db);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn upload_picker_click_cancel_select_and_backup(){
    let root=std::env::temp_dir().join(format!("yuque-picker-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let data=root.join("data");
    let db=Storage::new(data.join("assets.db")).unwrap();
    let source=root.join("fixture.png");
    image::RgbImage::new(64,48).save(&source).unwrap();
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let db_view=db.clone();
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{
        window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(1160.),px(800.))})),..Default::default()
    },cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db_view))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("choose-main").unwrap();
    ui.simulate_click(bounds.center(),Modifiers::default());app.run_until_parked();
    assert!(app.did_prompt_for_paths());
    assert!(app.read(|cx|desk.read(cx).picker_pending));
    app.update(|cx|desk.update(cx,|v,cx|{v.select_files(cx);v.export_album(cx);v.start_upload(vec![source.clone()],cx);}));
    assert!(db.jobs("demo","local",None).unwrap().is_empty());
    app.simulate_path_prompt_response(|options|{assert!(options.files&&options.multiple&&!options.directories);None});
    app.run_until_parked();
    assert!(!app.did_prompt_for_paths());
    assert!(app.read(|cx|!desk.read(cx).picker_pending&&desk.read(cx).status.contains("已取消")));
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("choose-main").unwrap();
    ui.simulate_click(bounds.center(),Modifiers::default());app.run_until_parked();
    app.simulate_path_prompt_response(|_|Some(vec![source.clone()]));
    app.run_until_parked();
    let (items,_,_)=db.page_album(150,"","image",Some(("demo","local")),None,false,None,false).unwrap();
    assert_eq!(items.len(),1);assert!(preview::cached_path(&data,&items[0]).is_file());
    app.update(|cx|desk.update(cx,|v,cx|v.export_album(cx)));
    app.run_until_parked();
    app.simulate_path_prompt_response(|options|{assert!(!options.files&&!options.multiple&&options.directories);None});
    app.run_until_parked();assert!(app.read(|cx|!desk.read(cx).picker_pending&&!desk.read(cx).backup_busy));
    app.update(|cx|desk.update(cx,|v,cx|v.export_album(cx)));app.run_until_parked();
    app.simulate_path_prompt_response(|_|Some(vec![root.clone()]));
    app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).status.contains("相册备份成功")));
    assert!(std::fs::read_dir(&root).unwrap().filter_map(|x|x.ok()).any(|x|x.file_name().to_string_lossy().starts_with("YuqueAlbum-")&&x.path().join("assets.db").exists()));
    drop(ui);drop(desk);drop(app);drop(db);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn comfort_toolbar_fits_minimum_window_and_preserves_density(){
    let root=std::env::temp_dir().join(format!("yuque-comfort-min-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{
        window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(720.),px(600.))})),..Default::default()
    },cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    let density=app.read(|cx|desk.read(cx).resource.tile_size);
    for comfortable in [true,false,true]{
        app.update(|cx|desk.update(cx,|v,cx|v.set_comfortable(comfortable,cx)));
        ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
        let bounds=ui.debug_bounds("choose-main").unwrap();
        assert!(bounds.origin.x>=px(150.)&&bounds.origin.x+bounds.size.width<=px(720.),"{bounds:?}");
        assert!(bounds.origin.y>=px(0.)&&bounds.origin.y+bounds.size.height<=px(600.),"{bounds:?}");
        assert_eq!(app.read(|cx|desk.read(cx).resource.tile_size),density);
        assert_eq!(app.read(|cx|desk.read(cx).spacing().gap),if comfortable{16.}else{10.});
        ui.simulate_click(bounds.center(),Modifiers::default());app.run_until_parked();
        assert!(app.did_prompt_for_paths());
        app.simulate_path_prompt_response(|_|None);app.run_until_parked();
    }
    drop(ui);drop(desk);drop(app);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn motion_samples_retarget_and_respect_reduced_motion(){
    use design::{Surface,SurfaceMotion};
    struct Probe{motion:SurfaceMotion,next:Surface,target:f32,alpha:f32,value:f32}
    impl Render for Probe{
        fn render(&mut self,w:&mut Window,cx:&mut Context<Self>)->impl IntoElement{
            self.alpha=self.motion.sample(self.next,w,cx);
            self.value=design::tween("motion-test",self.target,design::HOVER_MS,w,cx);
            div().size_full().opacity(self.alpha)
        }
    }
    let app=TestAppContext::single();app.update(gpui_kit::init);
    app.update(|cx|cx.set_reduce_motion(false));
    let (window,probe)=app.update(|cx|gpui_kit::open_window(WindowOptions::default(),cx,|_,cx|cx.new(|_|Probe{motion:Default::default(),next:Surface::Gallery,target:0.,alpha:0.,value:0.}))).unwrap();
    let mut ui=VisualTestContext::from_window(window,&app);
    let draw=|ui:&mut VisualTestContext|ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    draw(&mut ui);
    assert_eq!(app.read(|cx|probe.read(cx).alpha),1.);
    app.update(|cx|probe.update(cx,|v,cx|{v.next=Surface::Settings;v.target=1.;cx.notify();}));
    draw(&mut ui);
    let initial=app.read(|cx|probe.read(cx).alpha);
    assert!(initial<1.&&initial>=0.88,"{initial}");
    app.executor().advance_clock(std::time::Duration::from_millis(40));
    draw(&mut ui);
    let (alpha,mid)=app.read(|cx|{let p=probe.read(cx);(p.alpha,p.value)});
    assert!(alpha>initial&&alpha<1.,"{alpha}");
    assert!(mid>0.&&mid<1.,"{mid}");
    app.update(|cx|probe.update(cx,|v,cx|{v.target=0.;cx.notify();}));
    draw(&mut ui);
    let reverse=app.read(|cx|probe.read(cx).value);
    assert!((reverse-mid).abs()<0.05,"{reverse} versus {mid}");
    app.executor().advance_clock(std::time::Duration::from_millis(250));
    draw(&mut ui);
    assert_eq!(app.read(|cx|{let p=probe.read(cx);(p.alpha,p.value)}),(1.,0.));
    draw(&mut ui);
    assert_eq!(app.read(|cx|probe.read(cx).alpha),1.);
    app.update(|cx|{cx.set_reduce_motion(true);probe.update(cx,|v,cx|{v.next=Surface::Preview;v.target=1.;cx.notify();});});
    draw(&mut ui);
    assert_eq!(app.read(|cx|{let p=probe.read(cx);(p.alpha,p.value)}),(1.,1.));
}

#[::core::prelude::v1::test]
fn preview_double_click_keyboard_zoom_and_sidebar(){
    let _serial=crate::high_preview::TEST_LOCK.lock().unwrap();
    let root=std::env::temp_dir().join(format!("yuque-preview-ui-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    for i in 0..3{
        let source=root.join(format!("fixture-{i}.png"));
        image::RgbImage::from_pixel(640,480,image::Rgb([i as u8*50,120,70])).save(&source).unwrap();
        db.insert_or_get(&crate::storage::NewAsset{name:format!("{i}.png"),bytes:100,category:"image".into(),provider:"demo".into(),owner_id:"local".into(),url:url::Url::from_file_path(&source).unwrap().to_string(),sha256:{use sha2::{Digest,Sha256};hex::encode(Sha256::digest(std::fs::read(&source).unwrap()))},filekey:String::new()}).unwrap();
    }
    let app=TestAppContext::single();app.update(gpui_kit::init);
    app.update(|cx|cx.set_reduce_motion(true));
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{
        window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(780.),px(640.))})),..Default::default()
    },cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    let draw=|ui:&mut VisualTestContext|ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    draw(&mut ui);
    let collapse=ui.debug_bounds("collapse-sidebar").unwrap();
    ui.simulate_click(collapse.center(),Modifiers::default());app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).sidebar_collapsed));draw(&mut ui);
    let expand=ui.debug_bounds("expand-sidebar").unwrap();
    ui.simulate_click(expand.center(),Modifiers::default());app.run_until_parked();draw(&mut ui);
    assert!(!app.read(|cx|desk.read(cx).sidebar_collapsed));
    let id=app.read(|cx|desk.read(cx).resource.assets[0].id);
    assert_eq!(id,3);
    let tile=ui.debug_bounds("tile-3").unwrap();
    let pos=point(tile.center().x,tile.top()+px(35.));
    ui.simulate_click(pos,Modifiers::default());app.run_until_parked();draw(&mut ui);
    assert!(!app.read(|cx|desk.read(cx).inspector_open));
    assert_eq!(ui.debug_bounds("tile-3").unwrap(),tile);
    assert_eq!(app.read(|cx|desk.read(cx).resource.selected_ids.len()),1);
    eprintln!("PATH single-select stable tile={tile:?}");
    let second=ui.debug_bounds("tile-2").unwrap();
    ui.simulate_click(point(second.center().x,second.top()+px(35.)),Modifiers{shift:true,..Default::default()});
    app.run_until_parked();draw(&mut ui);
    assert_eq!(app.read(|cx|desk.read(cx).resource.selected_ids.len()),2);
    assert_eq!(ui.debug_bounds("tile-3").unwrap(),tile);
    assert_eq!(ui.debug_bounds("tile-2").unwrap(),second);
    eprintln!("PATH shift-select two items, unchanged bounds");
    ui.simulate_click(point(second.center().x,second.top()+px(35.)),Modifiers{control:true,..Default::default()});
    app.run_until_parked();draw(&mut ui);
    assert_eq!(app.read(|cx|desk.read(cx).resource.selected_ids.len()),1);
    assert_eq!(app.read(|cx|desk.read(cx).resource.selected_asset.as_ref().unwrap().id),id);
    eprintln!("PATH modifier-toggle one item");
    // Enter from multiple selection and move the viewer independently.
    ui.simulate_click(pos,Modifiers::default());app.run_until_parked();draw(&mut ui);
    ui.simulate_click(point(second.center().x,second.top()+px(35.)),Modifiers{shift:true,..Default::default()});
    app.run_until_parked();draw(&mut ui);
    let before=app.read(|cx|{let r=&desk.read(cx).resource;(r.selected_ids.clone(),r.selected_asset.as_ref().unwrap().id,r.selection_anchor)});
    assert_eq!(before.0.len(),2);
    ui.simulate_keystrokes("space");app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).resource.large_preview_open));
    ui.simulate_keystrokes("right");app.run_until_parked();draw(&mut ui);
    ui.simulate_keystrokes("escape");app.run_until_parked();draw(&mut ui);
    assert_eq!(app.read(|cx|{let r=&desk.read(cx).resource;(r.selected_ids.clone(),r.selected_asset.as_ref().unwrap().id,r.selection_anchor)}),before);
    ui.simulate_click(pos,Modifiers::default());app.run_until_parked();draw(&mut ui);

    ui.simulate_keystrokes("space");app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).resource.large_preview_open));
    ui.simulate_keystrokes("escape");app.run_until_parked();draw(&mut ui);
    assert!(!app.read(|cx|desk.read(cx).resource.large_preview_open));
    ui.simulate_keystrokes("right");app.run_until_parked();draw(&mut ui);
    assert_ne!(app.read(|cx|desk.read(cx).resource.selected_asset.as_ref().unwrap().id),id);
    ui.simulate_keystrokes("left");app.run_until_parked();draw(&mut ui);
    assert_eq!(app.read(|cx|desk.read(cx).resource.selected_asset.as_ref().unwrap().id),id);
    assert_eq!(ui.debug_bounds("tile-3").unwrap(),tile);
    eprintln!("PATH space-preview escape restores focus, arrows move selection");
    let toggle=ui.debug_bounds("toggle-inspector").unwrap();
    ui.simulate_click(toggle.center(),Modifiers::default());app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).inspector_open));
    assert!(app.read(|cx|desk.read(cx).inspector_model.read(cx).ready));
    assert!(ui.debug_bounds("tile-3").is_none());
    ui.update(|w,cx|assert!(desk.read(cx).resource.inspector_focus.is_focused(w)));
    let header=ui.debug_bounds("inspector-header").unwrap();
    ui.simulate_event(ScrollEvent{position:point(px(500.),px(540.)),delta:ScrollDelta::Pixels(point(px(0.),px(-1200.))),..Default::default()});
    app.run_until_parked();draw(&mut ui);
    assert_eq!(ui.debug_bounds("inspector-header").unwrap(),header);
    ui.simulate_keystrokes("escape");app.run_until_parked();draw(&mut ui);
    assert!(!app.read(|cx|desk.read(cx).inspector_open));
    assert_eq!(ui.debug_bounds("tile-3").unwrap(),tile);
    ui.update(|w,cx|assert!(desk.read(cx).resource.gallery_focus.is_focused(w)));
    ui.simulate_keystrokes("ctrl-i");app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).inspector_open));
    let preview=ui.debug_bounds("detail-preview").unwrap();
    ui.simulate_click(preview.center(),Modifiers::default());app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).resource.large_preview_open));
    ui.simulate_keystrokes("escape");app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|{let v=desk.read(cx);v.inspector_open&&!v.resource.large_preview_open}));
    ui.update(|w,cx|assert!(desk.read(cx).resource.inspector_focus.is_focused(w)));
    ui.simulate_keystrokes("ctrl-i");app.run_until_parked();draw(&mut ui);
    assert!(!app.read(|cx|desk.read(cx).inspector_open));
    ui.simulate_keystrokes("ctrl-i");app.run_until_parked();draw(&mut ui);
    let nav=ui.debug_bounds("nav-images").unwrap();
    ui.simulate_click(nav.center(),Modifiers::default());app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|{let v=desk.read(cx);!v.inspector_open&&v.resource.category=="image"}));
    ui.update(|w,cx|assert!(desk.read(cx).resource.gallery_focus.is_focused(w)));
    ui.simulate_click(pos,Modifiers::default());app.run_until_parked();draw(&mut ui);
    assert_eq!(ui.debug_bounds("tile-3").unwrap(),tile);
    eprintln!("PATH narrow inspector fixed header, escape, preview return and category focus");
    ui.simulate_event(MouseDownEvent{position:pos,button:MouseButton::Left,click_count:2,..Default::default()});
    ui.simulate_event(MouseUpEvent{position:pos,button:MouseButton::Left,click_count:2,..Default::default()});
    app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).resource.large_preview_open));
    assert_eq!(app.read(|cx|desk.read(cx).resource.selected_asset.as_ref().unwrap().id),id);
    assert!(app.read(|cx|desk.read(cx).viewer_model.read(cx).quality.is_none()));
    let load=ui.debug_bounds("local-quality").unwrap();
    ui.simulate_click(load.center(),Modifiers::default());app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).viewer_model.read(cx).quality.as_ref().is_some_and(|q|q.width==640&&q.height==480&&!q.remote)));
    let quality_id=app.read(|cx|desk.read(cx).viewer_model.read(cx).quality.as_ref().unwrap().image.id);
    // A rejected alternate source must not replace the current render image.
    app.update(|cx|desk.update(cx,|v,cx|v.load_quality(true,cx)));
    app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|{let r=desk.read(cx).viewer_model.read(cx);r.quality.as_ref().is_some_and(|q|q.image.id==quality_id)&&!r.status.is_empty()&&!r.busy}));
    let offline=ui.debug_bounds("offline-quality").unwrap();
    ui.simulate_click(offline.center(),Modifiers::default());app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|{let v=desk.read(cx);let r=v.viewer_model.read(cx);r.quality.is_none()&&v.resource.large_preview_path.is_some()&&r.status.is_empty()}));
    let load=ui.debug_bounds("local-quality").unwrap();
    ui.simulate_click(load.center(),Modifiers::default());app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).viewer_model.read(cx).quality.is_some()));
    let strip=ui.debug_bounds("preview-strip-2").unwrap();
    ui.simulate_click(strip.center(),Modifiers::default());app.run_until_parked();draw(&mut ui);
    assert_eq!(app.read(|cx|desk.read(cx).resource.selected_asset.as_ref().unwrap().id),2);
    assert!(app.read(|cx|desk.read(cx).viewer_model.read(cx).quality.is_none()));
    ui.simulate_keystrokes("left");app.run_until_parked();draw(&mut ui);
    ui.simulate_keystrokes("right");app.run_until_parked();draw(&mut ui);
    assert_ne!(app.read(|cx|desk.read(cx).resource.selected_asset.as_ref().unwrap().id),id);
    ui.simulate_keystrokes("left");app.run_until_parked();draw(&mut ui);
    assert_eq!(app.read(|cx|desk.read(cx).resource.selected_asset.as_ref().unwrap().id),id);
    let canvas=ui.debug_bounds("preview-canvas").unwrap();
    ui.simulate_event(ScrollEvent{position:canvas.center()+point(px(80.),px(0.)),delta:ScrollDelta::Pixels(point(px(0.),px(180.))),..Default::default()});
    app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).resource.zoom>1.));
    let start=canvas.center();
    ui.simulate_event(MouseDownEvent{position:start,button:MouseButton::Left,click_count:1,..Default::default()});
    ui.simulate_event(MouseMoveEvent{position:start+point(px(40.),px(25.)),pressed_button:Some(MouseButton::Left),..Default::default()});
    ui.simulate_event(MouseUpEvent{position:start+point(px(40.),px(25.)),button:MouseButton::Left,click_count:1,..Default::default()});
    app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).resource.drag_at.is_none()));

    ui.simulate_keystrokes("0");app.run_until_parked();draw(&mut ui);
    assert_eq!(app.read(|cx|desk.read(cx).resource.zoom),1.);
    ui.simulate_keystrokes("escape");app.run_until_parked();draw(&mut ui);
    assert!(!app.read(|cx|desk.read(cx).resource.large_preview_open));
    drop(ui);drop(desk);drop(app);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn preview_next_crosses_filtered_page_and_can_return(){
    let root=std::env::temp_dir().join(format!("yuque-preview-pages-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    for i in 0..151{
        let category=if i==0||i==150{"image"}else{"attachment"};
        let source=root.join(format!("fixture-{i}.png"));
        let (url,digest)=if category=="image"{
            image::RgbImage::from_pixel(640,320,image::Rgb([i as u8,100,30])).save(&source).unwrap();
            use sha2::{Digest,Sha256};
            (url::Url::from_file_path(&source).unwrap().to_string(),hex::encode(Sha256::digest(std::fs::read(&source).unwrap())))
        }else{(format!("file:///fixture-{i}.png"),format!("fixture-{i}"))};
        db.insert_or_get(&crate::storage::NewAsset{name:format!("fixture-{i}.png"),bytes:10,category:category.into(),provider:"demo".into(),owner_id:"local".into(),url,sha256:digest,filekey:String::new()}).unwrap();
    }
    let app=TestAppContext::single();app.update(gpui_kit::init);app.update(|cx|cx.set_reduce_motion(true));
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{
        window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(780.),px(640.))})),..Default::default()
    },cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.len()),150);
    let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|w,cx|{
        desk.update(cx,|v,cx|{let a=v.resource.assets[0].clone();v.select_asset(a,w,cx);v.open_large_preview(w,cx);});
        w.refresh();w.draw(cx).clear(cx);
    });
    app.run_until_parked();
    ui.simulate_keystrokes("right");
    for _ in 0..5{app.run_until_parked();ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});}
    assert_eq!(app.read(|cx|desk.read(cx).resource.selected_asset.as_ref().unwrap().id),1);
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.len()),151);
    assert!(app.read(|cx|desk.read(cx).resource.pending_preview.is_none()));
    assert!(app.read(|cx|{let r=&desk.read(cx).resource;r.cached_thumbnails.get(&1).is_some_and(|p|p.is_file())&&r.ratios.get(&1)==Some(&2.)}));
    assert!(app.read(|cx|desk.read(cx).preview_strip().iter().map(|a|a.id).collect::<Vec<_>>()==vec![151,1]));
    // Simulate upload completion invoking reload while on a later page.
    app.update(|cx|desk.update(cx,|v,cx|v.reload(cx)));app.run_until_parked();
    assert!(app.read(|cx|{let r=&desk.read(cx).resource;r.preview_reload_pending&&r.assets.len()==151&&r.selected_asset.as_ref().unwrap().id==1}));
    ui.simulate_keystrokes("left");app.run_until_parked();
    assert_eq!(app.read(|cx|desk.read(cx).resource.selected_asset.as_ref().unwrap().id),151);
    ui.simulate_keystrokes("escape");app.run_until_parked();
    assert!(app.read(|cx|{let r=&desk.read(cx).resource;!r.large_preview_open&&!r.preview_reload_pending&&r.assets.len()==150}));
    assert!(root.join("fixture-0.png").exists()&&root.join("fixture-150.png").exists());
    drop(ui);drop(desk);drop(app);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn resource_write_lock_does_not_block_shell_and_interlocks(){
    let root=std::env::temp_dir().join(format!("yuque-mutation-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let asset=db.insert_or_get(&crate::storage::NewAsset{name:"fixture.txt".into(),bytes:10,category:"attachment".into(),provider:"demo".into(),owner_id:"local".into(),url:"file:///fixture.txt".into(),sha256:"fixture".into(),filekey:String::new()}).unwrap();
    let read=db.clone();
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions::default(),cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let conn=rusqlite::Connection::open(root.join("assets.db")).unwrap();
    conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    let start=std::time::Instant::now();
    app.update(|cx|desk.update(cx,|v,cx|{
        v.toggle_favorite(&asset,cx);
        assert!(v.resource_write_busy);
        assert!(!v.desktop_can_quit(cx));
        v.select_files(cx);
        assert!(!v.picker_pending);
        v.export_album(cx);
        assert!(!v.picker_pending);
        v.navigate_page(Page::Data,cx);
        assert!(!v.data_scan_busy);
        v.navigate_page(Page::Jobs,cx);
        assert_eq!(v.page,Page::Jobs);
        // A second write must not replace the task.
        v.toggle_favorite(&asset,cx);
        assert!(v.resource_write_busy);
    }));
    assert!(start.elapsed()<std::time::Duration::from_secs(1),"UI dispatch blocked by SQLite lock");
    conn.execute_batch("COMMIT").unwrap();
    app.run_until_parked();
    assert!(app.read(|cx|{let v=desk.read(cx);!v.resource_write_busy&&v.resource.favorites.contains(&asset.id)}));
    assert!(read.favorite_ids(Some(("demo","local"))).unwrap().contains(&asset.id));
    let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    drop(conn);drop(ui);drop(desk);drop(app);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn inspector_exit_reserves_space_until_finished_and_reverses(){
    let root=std::env::temp_dir().join(format!("yuque-inspector-motion-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let app=TestAppContext::single();app.update(gpui_kit::init);app.update(|cx|cx.set_reduce_motion(false));
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(1160.),px(800.))})),..Default::default()},cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    let draw=|ui:&mut VisualTestContext|ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let advance=|n|app.executor().advance_clock(std::time::Duration::from_millis(n));
    draw(&mut ui);
    let toggle=|ui:&mut VisualTestContext|{let pos=ui.debug_bounds("toggle-inspector").unwrap().center();ui.simulate_click(pos,Modifiers::default());};
    assert!(ui.debug_bounds("inspector-slot").is_none());
    toggle(&mut ui);draw(&mut ui);advance(250);draw(&mut ui);
    let slot=ui.debug_bounds("inspector-slot").unwrap();
    toggle(&mut ui);draw(&mut ui);advance(60);draw(&mut ui);
    assert!(!app.read(|cx|desk.read(cx).inspector_open));
    assert_eq!(ui.debug_bounds("inspector-slot").unwrap(),slot);
    toggle(&mut ui);draw(&mut ui);advance(250);draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).inspector_open));
    assert_eq!(ui.debug_bounds("inspector-slot").unwrap(),slot);
    toggle(&mut ui);draw(&mut ui);advance(250);draw(&mut ui);
    assert!(ui.debug_bounds("inspector-slot").is_none());
    app.update(|cx|cx.set_reduce_motion(true));
    toggle(&mut ui);draw(&mut ui);
    assert!(ui.debug_bounds("inspector-slot").is_some());
    toggle(&mut ui);draw(&mut ui);
    assert!(ui.debug_bounds("inspector-slot").is_none());
    drop(ui);drop(desk);drop(app);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn queue_prepare_and_preferences_remain_responsive_under_write_lock(){
    let root=std::env::temp_dir().join(format!("yuque-prep-lock-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let source=root.join("fixture.txt");std::fs::write(&source,b"isolated").unwrap();
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let read=db.clone();
    let (_,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions::default(),cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let lock=rusqlite::Connection::open(root.join("assets.db")).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    let start=std::time::Instant::now();
    app.update(|cx|desk.update(cx,|v,cx|{
        v.start_upload(vec![source.clone()],cx);
        assert!(v.queue.read(cx).busy&&v.queue.read(cx).preparing);
        assert!(!v.desktop_can_quit(cx));
        v.start_upload(vec![source.clone()],cx);
        v.export_album(cx);assert!(!v.picker_pending);
        v.navigate_page(Page::Data,cx);assert!(!v.data_scan_busy);
        v.navigate_page(Page::Jobs,cx);assert_eq!(v.page,Page::Jobs);
        v.resource.tile_size=180.;v.persist_preferences(cx);
        v.resource.tile_size=204.;v.persist_preferences(cx);
        assert!(v.prefs_writing);
        assert!(!v.desktop_can_quit(cx));
    }));
    assert!(start.elapsed()<std::time::Duration::from_secs(1),"UI dispatch blocked on database write lock");
    assert!(read.jobs("demo","local",None).unwrap().is_empty());
    lock.execute_batch("COMMIT").unwrap();
    app.run_until_parked();
    app.executor().advance_clock(std::time::Duration::from_millis(250));app.run_until_parked();
    assert!(app.read(|cx|{let v=desk.read(cx);!v.queue.read(cx).busy&&!v.queue.read(cx).preparing&&!v.prefs_writing}));
    let jobs=read.jobs("demo","local",None).unwrap();
    assert_eq!(jobs.len(),1);assert_eq!(jobs[0].state,"done");
    assert_eq!(read.recoverable_count("demo","local").unwrap(),0);
    assert_eq!(preferences::Preferences::decode(read.preferences().unwrap()).tile,204.);
    app.update(|cx|desk.update(cx,|v,cx|assert!(v.desktop_can_quit(cx))));
    drop(lock);drop(desk);drop(app);drop(read);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn independent_runner_refreshes_gallery_once_after_batch(){
    let root=std::env::temp_dir().join(format!("yuque-runner-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let sources=(0..3).map(|i|{let p=root.join(format!("fixture-{i}.txt"));std::fs::write(&p,format!("content-{i}")).unwrap();p}).collect::<Vec<_>>();
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let read=db.clone();
    let (_,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions::default(),cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let generation=app.read(|cx|desk.read(cx).resource.history_generation);
    app.update(|cx|desk.update(cx,|v,cx|v.start_upload(sources,cx)));
    app.run_until_parked();
    app.read(|cx|{
        let v=desk.read(cx);let q=v.queue.read(cx);
        assert!(!q.busy&&!q.preparing);assert_eq!(q.succeeded,3);
        assert_eq!(v.resource.history_generation,generation+1);
        assert_eq!(v.resource.assets.len(),3);
        assert_eq!(v.job_history.read(cx).recovery_count,0);
    });
    assert_eq!(read.jobs("demo","local",None).unwrap().iter().filter(|j|j.state=="done").count(),3);
    drop(desk);drop(app);drop(read);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn sidebar_and_immersive_transition_reserve_geometry_and_reverse(){
    let root=std::env::temp_dir().join(format!("yuque-nav-motion-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let app=TestAppContext::single();app.update(gpui_kit::init);app.update(|cx|cx.set_reduce_motion(false));
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(1160.),px(800.))})),..Default::default()},cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    let draw=|ui:&mut VisualTestContext|ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    let advance=|n|app.executor().advance_clock(std::time::Duration::from_millis(n));
    let click=|ui:&mut VisualTestContext,id:&'static str|{let pos=ui.debug_bounds(id).unwrap().center();ui.simulate_click(pos,Modifiers::default());};
    draw(&mut ui);let wide=ui.debug_bounds("nav-slot").unwrap();
    click(&mut ui,"collapse-sidebar");draw(&mut ui);advance(60);draw(&mut ui);
    assert_eq!(ui.debug_bounds("nav-slot").unwrap(),wide);
    click(&mut ui,"expand-sidebar");draw(&mut ui);advance(250);draw(&mut ui);
    assert_eq!(ui.debug_bounds("nav-slot").unwrap(),wide);
    click(&mut ui,"collapse-sidebar");draw(&mut ui);advance(250);draw(&mut ui);
    assert_eq!(ui.debug_bounds("nav-slot").unwrap().size.width,px(56.));
    click(&mut ui,"immersive-toggle");draw(&mut ui);advance(60);draw(&mut ui);
    assert_eq!(ui.debug_bounds("nav-slot").unwrap().size.width,px(56.));
    click(&mut ui,"immersive-toggle");draw(&mut ui);advance(250);draw(&mut ui);
    assert!(!app.read(|cx|desk.read(cx).immersive));
    assert_eq!(ui.debug_bounds("nav-slot").unwrap().size.width,px(56.));
    click(&mut ui,"immersive-toggle");draw(&mut ui);advance(250);draw(&mut ui);
    assert!(ui.debug_bounds("nav-slot").is_none());
    app.update(|cx|cx.set_reduce_motion(true));
    click(&mut ui,"immersive-toggle");draw(&mut ui);
    assert_eq!(ui.debug_bounds("nav-slot").unwrap().size.width,px(56.));
    click(&mut ui,"expand-sidebar");draw(&mut ui);
    assert_eq!(ui.debug_bounds("nav-slot").unwrap(),wide);
    drop(ui);drop(desk);drop(app);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn browse_context_restores_query_and_clears_empty_results(){
    let root=std::env::temp_dir().join(format!("yuque-browse-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let path=root.join("source.png");
    image::RgbImage::from_pixel(64,48,image::Rgb([12,100,128])).save(&path).unwrap();
    let mut png=std::fs::read(&path).unwrap();
    let asset=db.insert_or_get(&crate::storage::NewAsset{name:"source.png".into(),bytes:png.len() as i64,category:"image".into(),provider:"demo".into(),owner_id:"local".into(),url:url::Url::from_file_path(&path).unwrap().to_string(),sha256:hex::encode(sha2::Sha256::digest(&png)),filekey:String::new()}).unwrap();
    use sha2::Digest;
    db.set_tags(asset.id,"design/ui").unwrap();db.toggle_favorite(&asset).unwrap();
    let path2=root.join("neighbor.png");png.push(0);std::fs::write(&path2,&png).unwrap();
    let other=db.insert_or_get(&crate::storage::NewAsset{name:"neighbor.png".into(),bytes:png.len() as i64,category:"image".into(),provider:"demo".into(),owner_id:"local".into(),url:url::Url::from_file_path(&path2).unwrap().to_string(),sha256:hex::encode(sha2::Sha256::digest(&png)),filekey:String::new()}).unwrap();
    preview::index_local(&db,&asset).unwrap();preview::index_local(&db,&other).unwrap();
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let app=TestAppContext::single();app.update(gpui_kit::init);app.update(|cx|cx.set_reduce_motion(true));
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions{window_bounds:Some(WindowBounds::Windowed(Bounds{origin:Point::default(),size:size(px(720.),px(600.))})),..Default::default()},cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    let draw=|ui:&mut VisualTestContext|ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    ui.update(|w,cx|desk.update(cx,|v,cx|{
        v.resource.category="image";v.resource.favorites_only=true;v.resource.all_accounts=true;
        v.resource.tag_filter=Some("design/ui".into());v.resource.group_dates=true;
        v.resource.search="source".into();v.resource.search_input.update(cx,|i,cx|i.set_value("source",w,cx));
        v.reset_history(cx);
    }));
    app.run_until_parked();app.executor().advance_clock(std::time::Duration::from_millis(200));app.run_until_parked();draw(&mut ui);
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.len()),1);
    assert_eq!(app.read(|cx|desk.read(cx).browse_count_label(2)),"1 项");
    app.update(|cx|desk.update(cx,|v,cx|v.find_similar(asset.clone(),cx)));
    app.run_until_parked();app.executor().advance_clock(std::time::Duration::from_millis(200));app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|{let r=&desk.read(cx).resource;r.similar_to.is_some()&&r.assets.iter().any(|a|a.id==other.id)&&r.group_dates&&r.favorites_only}));
    let back=ui.debug_bounds("browse-clear").unwrap();
    ui.simulate_click(back.center(),Modifiers::default());
    app.run_until_parked();app.executor().advance_clock(std::time::Duration::from_millis(200));app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|{let r=&desk.read(cx).resource;r.similar_to.is_none()&&r.assets.len()==1&&r.assets[0].id==asset.id&&r.search=="source"&&r.tag_filter.as_deref()==Some("design/ui")&&r.favorites_only&&r.all_accounts&&r.group_dates}));
    let search=ui.debug_bounds("gallery-search").unwrap();ui.simulate_click(search.center(),Modifiers::default());
    ui.simulate_keystrokes("end x x x");

    app.run_until_parked();app.executor().advance_clock(std::time::Duration::from_millis(200));app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|desk.read(cx).resource.assets.is_empty()));
    assert_eq!(app.read(|cx|desk.read(cx).browse_count_label(2)),"0 项");
    let clear=ui.debug_bounds("empty-clear").unwrap();ui.simulate_click(clear.center(),Modifiers::default());
    app.run_until_parked();draw(&mut ui);
    assert!(app.read(|cx|{let v=desk.read(cx);!v.has_browse_filters()&&v.resource.assets.len()==2&&v.resource.search_input.read(cx).value().is_empty()}));
    drop(ui);drop(desk);drop(app);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn acknowledging_unknown_requires_confirmation_and_does_not_upload(){
    let root=std::env::temp_dir().join(format!("yuque-ack-ui-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let source=root.join("fixture.txt");std::fs::write(&source,b"not uploaded").unwrap();
    let jobs=db.enqueue("demo","local",&[source.clone()]).unwrap();let id=jobs[0].0;
    db.job_state(id,"running","").unwrap();
    db.job_state(id,"unknown","fixture uncertain result").unwrap();
    let read=db.clone();let app=TestAppContext::single();app.update(gpui_kit::init);
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions::default(),cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|w,cx|desk.update(cx,|v,cx|{
        v.navigate_page(Page::Jobs,cx);
        v.acknowledge_job(id,w,cx);
        assert!(v.maintenance_busy);
        assert!(!v.desktop_can_quit(cx));
        v.retry_one(id,cx);assert!(!v.queue.read(cx).busy);
    }));
    app.run_until_parked();
    assert_eq!(read.jobs("demo","local",None).unwrap()[0].state,"unknown");
    app.simulate_prompt_answer("取消");app.run_until_parked();
    assert_eq!(read.jobs("demo","local",None).unwrap()[0].state,"unknown");
    ui.update(|w,cx|desk.update(cx,|v,cx|v.acknowledge_job(id,w,cx)));
    app.run_until_parked();app.simulate_prompt_answer("确认核实结果");app.run_until_parked();
    assert_eq!(read.jobs("demo","local",None).unwrap()[0].state,"failed");
    assert!(read.page_album(150,"","",Some(("demo","local")),None,false,None,false).unwrap().0.is_empty());
    assert!(!app.read(|cx|{let v=desk.read(cx);v.maintenance_busy||v.queue.read(cx).busy}));
    drop(ui);drop(desk);drop(app);drop(read);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn gallery_refresh_failure_clears_stale_actions_and_append_failure_keeps_selection(){
    let root=std::env::temp_dir().join(format!("yuque-refresh-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    for i in 0..2{
        db.insert_or_get(&crate::storage::NewAsset{name:format!("fixture-{i}.txt"),bytes:10,category:"attachment".into(),provider:"demo".into(),owner_id:"local".into(),url:format!("file:///fixture-{i}.txt"),sha256:format!("fixture-{i}"),filekey:String::new()}).unwrap();
    }
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions::default(),cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|w,cx|desk.update(cx,|v,cx|{
        let a=v.resource.assets[0].clone();
        v.click_tile(a,1,Modifiers::default(),w,cx);
        v.resource.batch_tag_open=true;
        v.resource.page_cursors.push(Some(v.resource.assets[1].id));
        let generation=v.resource.history_generation;
        v.apply_gallery_result(&gallery_query::GalleryResult{generation,append:true,result:Err("fixture append failure".into())},cx);
        assert_eq!(v.resource.assets.len(),2);
        assert_eq!(v.resource.selected_ids.len(),1);
        assert!(v.resource.selected_asset.is_some());
        assert_eq!(v.resource.page_cursors.len(),1);
        v.apply_gallery_result(&gallery_query::GalleryResult{generation,append:false,result:Err("fixture refresh failure".into())},cx);
        assert!(v.resource.assets.is_empty()&&v.resource.selected_ids.is_empty()&&v.resource.selected_asset.is_none());
        assert!(v.resource.selection_anchor.is_none()&&!v.resource.batch_tag_open&&v.resource.thumbnail_path.is_none());
        assert!(v.resource.history_error.is_some());
        v.reload(cx);
    }));
    app.run_until_parked();
    ui.update(|w,cx|desk.update(cx,|v,cx|{
        assert_eq!(v.resource.assets.len(),2);
        assert!(v.resource.history_error.is_none());
        let a=v.resource.assets[0].clone();
        let b=v.resource.assets[1].clone();
        v.click_tile(b.clone(),1,Modifiers::default(),w,cx);
        v.click_tile(a,1,Modifiers{control:true,..Default::default()},w,cx);
        assert_eq!(v.resource.selected_ids.len(),2);
        let generation=v.resource.history_generation;
        v.apply_gallery_result(&gallery_query::GalleryResult{generation,append:false,result:Ok(((vec![b],false,1),vec![],Default::default(),vec![]))},cx);
        assert!(v.resource.selected_ids.is_empty()&&v.resource.selected_asset.is_none());
        assert!(v.resource.selection_anchor.is_none());
    }));
    drop(ui);drop(desk);drop(app);let _=std::fs::remove_dir_all(root);
}

#[::core::prelude::v1::test]
fn thumbnail_lane_survives_pagination_and_pauses_on_navigation(){
    let root=std::env::temp_dir().join(format!("yuque-thumb-lane-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let db=Storage::new(root.join("assets.db")).unwrap();
    let path=root.join("source.png");
    image::RgbImage::from_pixel(48,24,image::Rgb([20,60,120])).save(&path).unwrap();
    use sha2::Digest;
    let bytes=std::fs::read(&path).unwrap();
    let valid=db.insert_or_get(&crate::storage::NewAsset{name:"source.png".into(),bytes:bytes.len() as i64,category:"image".into(),provider:"demo".into(),owner_id:"local".into(),url:url::Url::from_file_path(&path).unwrap().to_string(),sha256:hex::encode(sha2::Sha256::digest(&bytes)),filekey:String::new()}).unwrap();
    for i in 0..150{
        db.insert_or_get(&crate::storage::NewAsset{name:format!("missing-{i}.png"),bytes:10,category:"image".into(),provider:"demo".into(),owner_id:"local".into(),url:format!("file:///missing-{i}.png"),sha256:format!("missing-{i}"),filekey:String::new()}).unwrap();
    }
    let app=TestAppContext::single();app.update(gpui_kit::init);
    let config=Config{mode:Mode::Demo,creds:None,max_file_mb:1};
    let (window,desk)=app.update(|cx|gpui_kit::open_window(WindowOptions::default(),cx,move|w,cx|cx.new(|cx|UploadDesk::new(w,cx,config,db))).unwrap());
    app.run_until_parked();
    let first=app.read(|cx|desk.read(cx).resource.assets[0].id);
    assert!(app.read(|cx|desk.read(cx).thumbnails.read(cx).attempted(first)));
    app.update(|cx|desk.update(cx,|v,cx|{
        v.load_more(cx);
        assert!(v.thumbnails.read(cx).attempted(first));
        v.navigate_page(Page::Settings,cx);
        assert!(!v.thumbnails.read(cx).busy);
    }));
    app.run_until_parked();
    assert!(app.read(|cx|{let v=desk.read(cx);v.resource.assets.len()==151&&!v.thumbnails.read(cx).busy&&!v.thumbnails.read(cx).attempted(valid.id)}));
    app.update(|cx|desk.update(cx,|v,cx|{
        v.resource.visible_range=(150,151);
        v.navigate_page(Page::Gallery,cx);
    }));
    app.run_until_parked();
    assert!(app.read(|cx|{let v=desk.read(cx);v.thumbnails.read(cx).attempted(first)&&v.thumbnails.read(cx).attempted(valid.id)&&v.resource.cached_thumbnails.get(&valid.id).is_some_and(|p|p.is_file())&&v.resource.ratios.get(&valid.id)==Some(&2.)}));
    let mut ui=VisualTestContext::from_window(window,&app);
    ui.update(|w,cx|desk.update(cx,|v,cx|{
        v.click_tile(valid.clone(),1,Modifiers::default(),w,cx);
        assert!(!v.resource.preview_loading);
        assert_eq!(v.resource.thumbnail_path,v.resource.cached_thumbnails.get(&valid.id).cloned());
        v.inspector_open=true;
        cx.notify();
    }));
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    app.run_until_parked();
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    assert!(app.read(|cx|desk.read(cx).resource.image_cache.read(cx).len()>0));
    ui.update(|w,cx|desk.update(cx,|v,cx|{
        v.navigate_page(Page::Settings,cx);
        assert_eq!(v.resource.image_cache.read(cx).len(),0);
        w.refresh();
    }));
    app.run_until_parked();
    ui.update(|w,cx|desk.update(cx,|v,cx|{v.navigate_page(Page::Gallery,cx);w.refresh();}));
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    app.run_until_parked();
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    assert!(app.read(|cx|desk.read(cx).resource.image_cache.read(cx).len()>0));
    ui.update(|w,cx|desk.update(cx,|v,cx|{
        v.apply_account(None,cx);
        assert_eq!(v.resource.image_cache.read(cx).len(),0);
        assert!(v.resource.assets.is_empty()&&v.resource.assets.capacity()==0);
        assert!(v.resource.cached_thumbnails.is_empty()&&v.resource.cached_thumbnails.capacity()==0);
        assert!(v.resource.row_plan.is_empty()&&v.resource.masonry_plan.tiles.is_empty());
        assert!(!v.thumbnails.read(cx).attempted(first));
        w.refresh();
    }));
    app.run_until_parked();
    assert!(path.is_file());
    assert!(preview::cached_path(&root,&valid).is_file());
    drop(ui);drop(desk);drop(app);let _=std::fs::remove_dir_all(root);
}
