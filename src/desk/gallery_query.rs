//! Retained gallery query boundary; stale results are discarded before delivery.
use super::*;
pub(super) struct GalleryRequest {
    pub query:String,pub category:&'static str,pub oldest:bool,pub tag:Option<String>,
    pub favorites_only:bool,pub similar:Option<Asset>,pub scope:Option<(String,String)>,
    pub before:Option<i64>,pub append:bool,
}
pub(super) type GalleryPage=((Vec<Asset>,bool,i64),Vec<String>,std::collections::HashSet<i64>,Vec<(i64,f32,PathBuf)>);
pub(super) struct GalleryResult {
    pub generation:u64,pub append:bool,pub result:std::result::Result<GalleryPage,String>,
}
impl EventEmitter<GalleryResult> for GalleryQuery {}
#[derive(Default)]
pub(super) struct GalleryQuery{
    generation:u64,
    task:Option<Task<()>>,
}
impl GalleryQuery {
    pub fn request(&mut self,storage:Storage,request:GalleryRequest,cx:&mut Context<Self>)->u64{
        self.generation=self.generation.wrapping_add(1);
        let generation=self.generation;
        let append=request.append;
        self.task=Some(cx.spawn(async move|this,cx|{
            let result=execute(storage,request,cx).await;
            let _=this.update(cx,|v,cx|{
                if generation==v.generation{cx.emit(GalleryResult{generation,append,result:result.map_err(|e|e.to_string())});}
            });
        }));
        generation
    }
}
async fn execute(storage:Storage,request:GalleryRequest,cx:&mut AsyncApp)->Result<GalleryPage>{
    let GalleryRequest{query,category,oldest,tag,favorites_only,similar,scope,before,..}=request;
    if !query.is_empty(){cx.background_executor().timer(std::time::Duration::from_millis(100)).await;}
        cx.background_spawn(async move {
                let scope=scope.as_ref().map(|(provider,owner)|(provider.as_str(),owner.as_str()));
                let page=if let Some(asset)=similar {
                    let path=preview::restore_local(&storage,&asset)?;
                    let (hash,_)=preview::visual_hash(&path)?;
                    storage.store_visual_hash(asset.id,hash)?;
                    let mut cursor=None;
                    loop {
                        let (page,more,_)=storage.page_album(150,"","image",Some((&asset.provider,&asset.owner_id)),cursor,false,None,false)?;
                        for candidate in &page {
                            if storage.visual_hash(candidate.id)?.is_none(){
                                let path=preview::cached_path(storage.data_dir(),candidate);
                                if path.is_file(){if let Ok((hash,_))=preview::visual_hash(&path){storage.store_visual_hash(candidate.id,hash)?;}}
                            }
                        }
                        if !more {break;}
                        cursor=page.last().map(|a|a.id);
                        if cursor.is_none(){break;}
                    }
                    let items=storage.similar_assets(&asset,hash)?;
                    let total=items.len() as i64;
                    (items,false,total)
                }else{storage.page_album(PAGE_SIZE,&query,category,scope,before,oldest,tag.as_deref(),favorites_only)?};
                let ratios=page.0.iter().filter(|a|a.category=="image").filter_map(|a|{
                    let path=preview::cached_path(storage.data_dir(),a);
                    image::image_dimensions(&path).ok().map(|(w,h)|(a.id,w as f32/h.max(1) as f32,path))
                }).collect::<Vec<_>>();
                let tags=storage.tag_catalog(scope)?;
                let favorites=storage.favorite_ids(scope)?;
                Ok::<_,anyhow::Error>((page,tags,favorites,ratios))
        }).await

}
