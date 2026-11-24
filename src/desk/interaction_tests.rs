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
