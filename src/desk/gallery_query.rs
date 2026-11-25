//! Retained gallery query boundary. Superseded blocking work cooperatively stops.
use super::*;
use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
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

#[derive(Clone,Default)]
struct QueryCancellation(Arc<AtomicBool>);
impl QueryCancellation{
    fn cancel(&self){self.0.store(true,Ordering::Release);}
    fn cancelled(&self)->bool{self.0.load(Ordering::Acquire)}
    fn check(&self)->Result<()>{anyhow::ensure!(!self.cancelled(),"查询已取消");Ok(())}
}

#[derive(Default)]
pub(super) struct GalleryQuery{
    generation:u64,
    task:Option<Task<()>>,
    cancellation:QueryCancellation,
}
impl Drop for GalleryQuery{
    fn drop(&mut self){self.cancellation.cancel();}
}
impl GalleryQuery {
    pub fn request(&mut self,storage:Storage,request:GalleryRequest,cx:&mut Context<Self>)->u64{
        self.cancellation.cancel();
        self.cancellation=QueryCancellation::default();
        self.generation=self.generation.wrapping_add(1);
        let generation=self.generation;
        let append=request.append;
        let cancellation=self.cancellation.clone();
        self.task=Some(cx.spawn(async move|this,cx|{
            let result=execute(storage,request,cancellation,cx).await;
            let _=this.update(cx,|v,cx|{
                if generation==v.generation{cx.emit(GalleryResult{generation,append,result:result.map_err(|e|e.to_string())});}
            });
        }));
        generation
    }
}
async fn execute(storage:Storage,request:GalleryRequest,cancellation:QueryCancellation,cx:&mut AsyncApp)->Result<GalleryPage>{
    if !request.query.is_empty()&&!request.append&&request.similar.is_none(){
        cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
    }
    cancellation.check()?;
    cx.background_spawn(async move{execute_blocking(&storage,request,&cancellation)}).await
}

fn execute_blocking(storage:&Storage,request:GalleryRequest,cancellation:&QueryCancellation)->Result<GalleryPage>{
    let GalleryRequest{query,category,oldest,tag,favorites_only,similar,scope,before,..}=request;
    cancellation.check()?;
    let scope=scope.as_ref().map(|(provider,owner)|(provider.as_str(),owner.as_str()));
    let page=if let Some(asset)=similar.as_ref(){
        let path=preview::restore_local(storage,asset)?;
        cancellation.check()?;
        let (hash,_)=preview::visual_hash(&path)?;
        cancellation.check()?;
        storage.store_visual_hash(asset.id,hash)?;
        let mut cursor=None;
        loop{
            cancellation.check()?;
            let page=storage.unindexed_images(asset,cursor)?;
            if page.is_empty(){break;}
            cursor=page.last().map(|a|a.id);
            let last=page.len()<150;
            for candidate in &page{
                cancellation.check()?;
                let path=preview::cached_path(storage.data_dir(),candidate);
                if path.is_file(){
                    if let Ok((hash,_))=preview::visual_hash(&path){
                        cancellation.check()?;
                        storage.store_visual_hash(candidate.id,hash)?;
                    }
                }
            }
            if last{break;}
        }
        let items=storage.similar_assets_until(asset,hash,||cancellation.cancelled())?;
        let total=items.len() as i64;
        (items,false,total)
    }else{storage.page_album(PAGE_SIZE,&query,category,scope,before,oldest,tag.as_deref(),favorites_only)?};
    cancellation.check()?;
    let mut ratios=Vec::new();
    for a in page.0.iter().filter(|a|a.category=="image"){
        cancellation.check()?;
        let path=preview::cached_path(storage.data_dir(),a);
        if let Ok((w,h))=image::image_dimensions(&path){ratios.push((a.id,w as f32/h.max(1) as f32,path));}
    }
    let tags=storage.tag_catalog(scope)?;
    cancellation.check()?;
    let favorite_scope=similar.as_ref().map(|a|(a.provider.as_str(),a.owner_id.as_str())).or(scope);
    let favorites=storage.favorite_ids(favorite_scope)?;
    cancellation.check()?;
    Ok((page,tags,favorites,ratios))
}

#[cfg(test)]
mod tests{
    use super::*;
    #[::core::prelude::v1::test]
    fn cancelled_request_returns_before_query(){
        let root=std::env::temp_dir().join(format!("yuque-cancel-query-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let storage=Storage::new(root.join("assets.db")).unwrap();
        let token=QueryCancellation::default();
        let worker=token.clone();
        token.cancel();
        let request=GalleryRequest{query:String::new(),category:"",oldest:false,tag:None,favorites_only:false,similar:None,scope:None,before:None,append:false};
        assert_eq!(execute_blocking(&storage,request,&worker).unwrap_err().to_string(),"查询已取消");
        let _=std::fs::remove_dir_all(root);
    }
}
