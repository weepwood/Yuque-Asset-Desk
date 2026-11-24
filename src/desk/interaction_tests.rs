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
    ui.simulate_click(point(px(90.),px(118.)),Modifiers::default());
    app.run_until_parked();
    assert_eq!(app.read(|cx|desk.read(cx).resource.category),"image");
    assert_eq!(app.read(|cx|desk.read(cx).resource.assets.len()),0);
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    ui.simulate_click(point(px(90.),px(230.)),Modifiers::default());
    app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).jobs_page));
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    ui.simulate_click(point(px(90.),px(257.)),Modifiers::default());
    app.run_until_parked();
    assert!(app.read(|cx|desk.read(cx).account_panel && desk.read(cx).inspector_open));
    let dark=app.read(|cx|desk.read(cx).dark);
    ui.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);});
    ui.simulate_click(point(px(90.),px(314.)),Modifiers::default());
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

    drop(ui);drop(desk);drop(app);
    let _=std::fs::remove_dir_all(root);
}
