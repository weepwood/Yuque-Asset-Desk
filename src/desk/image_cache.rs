//! GPUI's native cache entry loader with an explicit LRU entry bound.
use gpui_kit::*;
use std::collections::HashMap;
pub(super) const MAX_IMAGES:usize=192;
pub(super) struct AlbumImageCache{items:HashMap<Resource,(u64,ImageCacheItem)>,clock:u64}
impl AlbumImageCache{
    #[cfg(feature="qa")]
    pub fn len(&self)->usize{self.items.len()}
    pub fn new(cx:&mut App)->Entity<Self>{
        let e=cx.new(|_|Self{items:HashMap::new(),clock:0});
        cx.observe_release(&e,|cache,cx|{
            for (_,(_,item)) in cache.items.drain(){if let Some(Ok(image))=item.get(){cx.drop_image(image,None);}}
        }).detach();e
    }
}
impl ImageCache for AlbumImageCache{
    fn load(&mut self,resource:&Resource,window:&mut Window,cx:&mut App)->Option<Result<std::sync::Arc<RenderImage>,ImageCacheError>>{
        // Only local path resources are accepted.
        if !matches!(resource,Resource::Path(_)){return None;}
        self.clock+=1;
        if !self.items.contains_key(resource)&&self.items.len()>=MAX_IMAGES{
            if let Some(key)=self.items.iter().min_by_key(|(_,v)|v.0).map(|(k,_)|k.clone()){
                if let Some((_,item))=self.items.remove(&key){if let Some(Ok(image))=item.get(){cx.drop_image(image,Some(window));}}
            }
        }
        let entry=self.items.entry(resource.clone()).or_insert_with(||(self.clock,ImageCacheItem::new(resource,cx)));
        entry.0=self.clock;entry.1.use_image(window)
    }
}
