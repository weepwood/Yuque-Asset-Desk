use super::*;
use design::Spacing;

#[derive(Debug,Clone,PartialEq)]
pub(super) enum AlbumRow {
    Day(String),
    Tiles { items: Vec<(usize,f32)>, height:f32 },
    Item(usize),
}

/// Stable order, no cropping; last justified row never grows beyond target height.
pub(super) fn plan_rows(assets:&[Asset],ratios:&std::collections::HashMap<i64,f32>,width:f32,target:f32,grid:bool,justified:bool,dates:bool,spacing:Spacing)->Vec<AlbumRow>{
    let gap=spacing.gap;
    let width=width.max(160.);
    let target=target.clamp(96.,240.);
    let cols=((width+gap)/(target+gap)).floor().max(1.) as usize;
    let equal=(width-gap*(cols-1) as f32)/cols as f32;
    let mut rows=Vec::new();
    let mut cursor=0;
    while cursor<assets.len(){
        let day=assets[cursor].created_at.get(..10).unwrap_or("未知日期");
        let end=if dates {cursor+assets[cursor..].iter().take_while(|a|a.created_at.get(..10).unwrap_or("未知日期")==day).count()}else{assets.len()};
        if dates{rows.push(AlbumRow::Day(day.to_owned()));}
        while cursor<end{
            if !grid{rows.push(AlbumRow::Item(cursor));cursor+=1;continue;}
            if !justified{
                let next=(cursor+cols).min(end);
                rows.push(AlbumRow::Tiles{items:(cursor..next).map(|i|(i,equal)).collect(),height:equal*0.82});
                cursor=next;continue;
            }
            // Reserve enough horizontal space for the selection, favorite and
            // copy controls, even on narrow portrait images.
            let min_width=96.;
            let ratio=|i:usize|ratios.get(&assets[i].id).copied().unwrap_or(1.).clamp(0.4,3.);
            let mut sum=0.;
            let start=cursor;
            while cursor<end {
                let n=cursor-start+1;
                if n>1 && min_width*n as f32+gap*(n-1) as f32>width {break;}
                sum+=(ratio(cursor)*target).max(min_width);
                cursor+=1;
                if sum+gap*(n-1) as f32>=width{break;}
            }
            let count=cursor-start;
            let usable=width-gap*(count-1) as f32;
            let mut lo=0.;
            let mut hi=target;
            for _ in 0..20{
                let mid=(lo+hi)/2.;
                let needed=(start..cursor).map(|i|(ratio(i)*mid).max(min_width)).sum::<f32>();
                if needed<=usable{lo=mid;}else{hi=mid;}
            }
            let h=lo.max(1.);
            let items=(start..cursor).map(|i|(i,(h*ratio(i)).max(min_width))).collect();
            rows.push(AlbumRow::Tiles{items,height:h});
        }
    }
    rows
}

#[derive(Clone,Debug)]
pub(super) struct MasonryTile {pub index:usize,pub id:i64,pub x:f32,pub y:f32,pub width:f32,pub height:f32}
#[derive(Clone,Debug,Default)]
pub(super) struct MasonryPlan {pub tiles:Vec<MasonryTile>,pub days:Vec<(String,f32)>,pub height:f32}
pub(super) const TILE_FOOTER:f32=24.;

/// Shortest-column packing. Each date starts a new section, not a row.
pub(super) fn masonry(assets:&[Asset],ratios:&std::collections::HashMap<i64,f32>,width:f32,target:f32,dates:bool,spacing:Spacing)->MasonryPlan {
    let gap=spacing.gap;
    let width=width.max(160.);
    let cols=((width+gap)/(target.clamp(96.,240.)+gap)).floor().max(1.) as usize;
    let w=(width-(cols-1) as f32*gap)/cols as f32;
    let mut bottom=vec![0f32;cols];let mut result=MasonryPlan::default();let mut last=None;
    for (index,a) in assets.iter().enumerate(){
        let day=a.created_at.get(..10).unwrap_or("未知日期");
        if dates && last!=Some(day){
            let y=bottom.iter().copied().fold(0.,f32::max);
            result.days.push((day.to_owned(),y));bottom.fill(y+spacing.day);last=Some(day);
        }
        let col=(0..cols).min_by(|&a,&b|bottom[a].total_cmp(&bottom[b])).unwrap();
        let ratio=ratios.get(&a.id).copied().filter(|r|r.is_finite()&&*r>0.).unwrap_or(1.).clamp(0.3,4.);
        let h=(w/ratio).clamp(60.,w*3.);
        let y=bottom[col];
        result.tiles.push(MasonryTile{index,id:a.id,x:col as f32*(w+gap),y,width:w,height:h});
        bottom[col]=y+h+TILE_FOOTER+gap;
    }
    result.height=bottom.into_iter().fold(0.,f32::max);
    result
}
impl MasonryPlan{
    pub fn anchor(&self,top:f32)->Option<(i64,f32)>{
        self.tiles.iter().filter(|t|t.y+t.height+TILE_FOOTER>top).min_by(|a,b|a.y.total_cmp(&b.y)).map(|t|(t.id,top-t.y))
    }
    pub fn restore(&self,anchor:Option<(i64,f32)>)->Option<f32>{
        let (id,delta)=anchor?;
        self.tiles.iter().find(|t|t.id==id).map(|t|(t.y+delta).max(0.))
    }
}


// Fit the local preview within the viewport before applying user zoom.
pub(super) fn fit_preview(width:f32,height:f32,ratio:f32,zoom:f32)->(f32,f32){
    let ratio=if ratio.is_finite()&&ratio>0.{ratio}else{1.};
    let w=width.min(height*ratio).max(1.);
    (w*zoom,w/ratio*zoom)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[::core::prelude::v1::test]
    fn preview_fits_at_one_hundred_percent(){
        for ratio in [0.4,1.,2.4]{
            let (w,h)=fit_preview(900.,600.,ratio,1.);
            assert!(w<=900.01&&h<=600.01);
            assert!((w/h-ratio).abs()<0.001);
            assert_eq!(fit_preview(900.,600.,ratio,2.),(w*2.,h*2.));
        }
    }
    #[::core::prelude::v1::test]
    fn masonry_packs_shortest_column_without_overlap(){
        let items=(0..10000).map(|i|Asset{id:i,name:String::new(),bytes:0,category:"image".into(),provider:"demo".into(),url:String::new(),sha256:String::new(),owner_id:"local".into(),filekey:String::new(),created_at:"2026-11-24".into()}).collect::<Vec<_>>();
        let ratios=items.iter().map(|a|(a.id,0.3+(a.id%23) as f32*0.16)).collect();
        for comfortable in [false,true]{
        let spacing=Spacing::new(comfortable);
        let plan=masonry(&items,&ratios,800.,148.,true,spacing);
        assert_eq!(plan.tiles.len(),10000);
        let mut bottoms=std::collections::HashMap::<u32,f32>::new();
        for t in &plan.tiles{
            assert!(t.x+t.width<=800.1);
            assert!(t.y>=*bottoms.get(&t.x.to_bits()).unwrap_or(&0.));
            bottoms.insert(t.x.to_bits(),t.y+t.height+TILE_FOOTER);
        }
        let anchor=plan.anchor(2000.).unwrap();
        let next=masonry(&items,&ratios,620.,148.,true,Spacing::new(!comfortable));
        assert!(next.restore(Some(anchor)).is_some());
        }
    }
    #[::core::prelude::v1::test]
    fn rows_preserve_order_and_width(){
        let items=(0..10000).map(|i|Asset{id:i,name:String::new(),bytes:0,category:"image".into(),provider:"demo".into(),url:String::new(),sha256:String::new(),owner_id:"local".into(),filekey:String::new(),created_at:if i<5000{"2026-11-24"}else{"2026-11-23"}.into()}).collect::<Vec<_>>();
        let ratios=items.iter().map(|a|(a.id,0.4+(a.id%20) as f32*0.13)).collect();
        for comfortable in [false,true]{
        let spacing=Spacing::new(comfortable);
        for justified in [false,true]{
            let rows=plan_rows(&items,&ratios,800.,148.,true,justified,true,spacing);
            let mut seen=Vec::new();
            for row in rows {if let AlbumRow::Tiles{items,..}=row{
                assert!(items.iter().all(|(_,w)|*w>=96.));
                assert!(items.iter().map(|(_,w)|w).sum::<f32>()+spacing.gap*items.len().saturating_sub(1) as f32<=800.1);
                seen.extend(items.into_iter().map(|(i,_)|i));
            }}
            assert_eq!(seen,(0..10000).collect::<Vec<_>>());
        }
        }
    }
}
