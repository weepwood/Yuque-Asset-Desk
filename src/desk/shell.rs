//! Navigation and shared commands. A single primary destination, independent panels.
use super::*;
gpui_kit::actions!(yuque_gallery,[GalleryPreview,GalleryClear,GallerySelectAll,GalleryInspector]);
pub(super) fn bind_gallery(cx:&mut App){
    cx.bind_keys([
        KeyBinding::new("space",GalleryPreview,Some("YuqueGallery")),
        KeyBinding::new("escape",GalleryClear,Some("YuqueGallery")),
        KeyBinding::new("ctrl-a",GallerySelectAll,Some("YuqueGallery")),
        KeyBinding::new("cmd-a",GallerySelectAll,Some("YuqueGallery")),
        KeyBinding::new("ctrl-i",GalleryInspector,Some("YuqueGallery")),
        KeyBinding::new("cmd-i",GalleryInspector,Some("YuqueGallery")),
    ]);
}

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub(super) enum Page { #[default] Gallery, Jobs, Settings, Account, Data }
impl Page {
    pub fn settings(self)->bool{matches!(self,Self::Settings|Self::Account|Self::Data)}
}
#[derive(Clone,Copy)]
pub(super) enum Command { Navigate(Page), ToggleInspector, ToggleImmersive, SelectAll, ClearSelection, Preview }
impl UploadDesk{
    pub(super) fn navigate_page(&mut self,page:Page,cx:&mut Context<Self>){
        self.page=page;
        self.immersive=false;
        self.reset_armed=false;
        if page==Page::Jobs{self.refresh_jobs(cx);}
        if page==Page::Data{self.refresh_data(cx);}
        cx.notify();
    }
    pub(super) fn command(&mut self,command:Command,window:&mut Window,cx:&mut Context<Self>){
        match command{
            Command::Navigate(page)=>self.navigate_page(page,cx),
            Command::ToggleInspector=>{self.immersive=false;self.inspector_open=!self.inspector_open;},
            Command::ToggleImmersive=>{self.immersive=!self.immersive;if self.page==Page::Gallery{self.resource.gallery_focus.focus(window,cx);}},
            Command::SelectAll=>{
                self.resource.selected_ids=self.resource.assets.iter().take(selection::LIMIT).map(|a|a.id).collect();
                if self.resource.selected_asset.as_ref().is_none_or(|a|!self.resource.selected_ids.contains(&a.id)){
                    if let Some(a)=self.resource.assets.first().cloned(){self.select_asset(a,window,cx);}
                }
                self.resource.selection_anchor=self.resource.selected_asset.as_ref().map(|a|a.id);
                if self.resource.selected_ids.is_empty(){self.clear_resource_selection(cx);}
            },
            Command::ClearSelection=>self.clear_resource_selection(cx),
            Command::Preview=>self.open_large_preview(window,cx),
        }
        cx.notify();
    }
}
