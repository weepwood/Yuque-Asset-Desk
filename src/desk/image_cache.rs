//! GPUI's native cache entry loader with an explicit LRU entry bound.
use gpui_kit::*;
use std::collections::HashMap;
pub(super) const MAX_IMAGES:usize=192;
/// GPUI temporarily removes the current window from App.windows during window
/// updates. Defer release until the outer update returns it to the window map.
pub(super) fn release_images(images:Vec<std::sync::Arc<RenderImage>>,cx:&mut App){
    if !images.is_empty(){cx.defer(move|cx|{for image in images{cx.drop_image(image,None);}});}
}
pub(super) struct AlbumImageCache{items:HashMap<Resource,(u64,ImageCacheItem)>,clock:u64}
impl AlbumImageCache{
    #[cfg(feature="qa")]
    pub fn len(&self)->usize{self.items.len()}
    pub fn invalidate_path(&mut self,path:&std::path::Path,cx:&mut App){
        if let Some((_,item))=self.items.remove(&Resource::Path(path.to_owned().into())){release_images(item.get().and_then(Result::ok).into_iter().collect(),cx);}
    }
    pub fn clear(&mut self,cx:&mut App){
        let images=self.items.drain().filter_map(|(_,(_,item))|item.get().and_then(Result::ok)).collect();
        release_images(images,cx);
        self.clock=0;
    }
    pub fn new(cx:&mut App)->Entity<Self>{
        let e=cx.new(|_|Self{items:HashMap::new(),clock:0});
        cx.observe_release(&e,|cache,cx|{
            cache.clear(cx);
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
