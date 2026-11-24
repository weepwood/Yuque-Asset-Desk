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
    assert!(app.read(|cx|desk.read(cx).jobs_page));
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("nav-settings").unwrap(); ui.simulate_click(bounds.center(),Modifiers::default());
    app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).account_panel && !desk.read(cx).inspector_open));
    let dark=app.read(|cx|desk.read(cx).dark);
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    let bounds=ui.debug_bounds("settings-theme").unwrap(); ui.simulate_click(bounds.center(),Modifiers::default());
    app.run_until_parked();
    assert_ne!(app.read(|cx|desk.read(cx).dark),dark);
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
    let bounds=ui.debug_bounds("album-density-increase").expect("density button layout");
    ui.simulate_click(bounds.center(),Modifiers::default());
    app.run_until_parked();
    assert_eq!(app.read(|cx|desk.read(cx).resource.tile_size),164.);
    ui.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);});
    assert_eq!(app.read(|cx|desk.read(cx).resource.density.read(cx).value().start()),164.);
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
    assert!(!app.read(|cx|desk.read(cx).queue.busy));
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
    assert!(app.read(|cx|desk.read(cx).resource.oldest));
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
