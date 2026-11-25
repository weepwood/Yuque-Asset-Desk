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

#[derive(Clone,Debug,PartialEq)]
pub(super) struct MasonryTile {pub index:usize,pub id:i64,pub x:f32,pub y:f32,pub width:f32,pub height:f32}
#[derive(Clone,Debug,Default)]
pub(super) struct MasonryPlan {pub tiles:Vec<MasonryTile>,pub days:Vec<(String,f32)>,pub height:f32,pub footer:f32,pub max_tile_height:f32,bottom:Vec<f32>}

/// Shortest-column packing. Each date starts a new section, not a row.
pub(super) fn masonry(assets:&[Asset],ratios:&std::collections::HashMap<i64,f32>,width:f32,target:f32,dates:bool,spacing:Spacing)->MasonryPlan {
    let mut plan=MasonryPlan::default();
    plan.extend(assets,ratios,width,target,dates,spacing);
    plan
}
impl MasonryPlan{
    /// Extend only after the caller verifies identical geometry and unchanged prefix ratios.
    pub fn extend(&mut self,assets:&[Asset],ratios:&std::collections::HashMap<i64,f32>,width:f32,target:f32,dates:bool,spacing:Spacing){
        let gap=spacing.gap;
        let width=width.max(160.);
        let cols=((width+gap)/(target.clamp(96.,240.)+gap)).floor().max(1.) as usize;
        let w=(width-(cols-1) as f32*gap)/cols as f32;
        if self.bottom.is_empty(){self.bottom=vec![0.;cols];self.footer=spacing.footer;}
        debug_assert_eq!(self.bottom.len(),cols);
        let start=self.tiles.len();
        for (index,a) in assets.iter().enumerate().skip(start){
            let day=a.created_at.get(..10).unwrap_or("未知日期");
            if dates && self.days.last().is_none_or(|(last,_)|last!=day){
                let y=self.bottom.iter().copied().fold(0.,f32::max);
                self.days.push((day.to_owned(),y));self.bottom.fill(y+spacing.day);
            }
            let col=(0..cols).min_by(|&a,&b|self.bottom[a].total_cmp(&self.bottom[b])).unwrap();
            let ratio=ratios.get(&a.id).copied().filter(|r|r.is_finite()&&*r>0.).unwrap_or(1.).clamp(0.3,4.);
            let h=(w/ratio).clamp(60.,w*3.);
            let y=self.bottom[col];
            self.tiles.push(MasonryTile{index,id:a.id,x:col as f32*(w+gap),y,width:w,height:h});
            self.bottom[col]=y+h+spacing.footer+gap;
            self.max_tile_height=self.max_tile_height.max(h);
        }
        self.height=self.bottom.iter().copied().fold(0.,f32::max);
    }
    pub fn visible_tiles(&self,lo:f32,hi:f32)->impl Iterator<Item=&MasonryTile>{
        let from=self.tiles.partition_point(|t|t.y<lo-self.max_tile_height-self.footer);
        self.tiles[from..].iter().take_while(move|t|t.y<=hi).filter(move|t|t.y+t.height+self.footer>=lo)
    }
    pub fn anchor(&self,top:f32)->Option<(i64,f32)>{
        self.visible_tiles(top,f32::INFINITY).find(|t|t.y+t.height+self.footer>top).map(|t|(t.id,top-t.y))
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

/// Limit panning to the portion of the image outside the canvas.
pub(super) fn clamp_pan(canvas:(f32,f32),image:(f32,f32),pan:(f32,f32))->(f32,f32){
    let x=((image.0-canvas.0)*0.5).max(0.);
    let y=((image.1-canvas.1)*0.5).max(0.);
    (pan.0.clamp(-x,x),pan.1.clamp(-y,y))
}
/// Anchor the same image coordinate at the pointer when changing scale.
pub(super) fn anchor_pan(pan:(f32,f32),anchor:(f32,f32),old:f32,new:f32)->(f32,f32){
    let scale=new/old.max(1.);
    (anchor.0+(pan.0-anchor.0)*scale,anchor.1+(pan.1-anchor.1)*scale)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[::core::prelude::v1::test]
    fn preview_anchor_and_pan_bounds(){
        let p=anchor_pan((10.,-20.),(80.,40.),2.,3.);
        assert_eq!(p,(-25.,-50.));
        assert_eq!(clamp_pan((800.,600.),(1200.,400.),(500.,-99.)),(200.,0.));
        assert_eq!(clamp_pan((800.,600.),(800.,600.),(20.,30.)),(0.,0.));
    }
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
            bottoms.insert(t.x.to_bits(),t.y+t.height+spacing.footer);
        }
        let anchor=plan.anchor(2000.).unwrap();
        let next=masonry(&items,&ratios,620.,148.,true,Spacing::new(!comfortable));
        assert!(next.restore(Some(anchor)).is_some());
        }
    }
    #[::core::prelude::v1::test]
    fn masonry_append_matches_full_layout_and_culls_tall_tiles(){
        let items=(0..10000).map(|i|Asset{id:i,name:String::new(),bytes:0,category:"image".into(),provider:"demo".into(),url:String::new(),sha256:String::new(),owner_id:"local".into(),filekey:String::new(),created_at:format!("2026-11-{:02}",25-i/500)}).collect::<Vec<_>>();
        let ratios=items.iter().map(|a|(a.id,0.3+(a.id%23) as f32*0.16)).collect();
        for comfortable in [false,true]{
            let spacing=Spacing::new(comfortable);
            for dates in [false,true]{
                let full=masonry(&items,&ratios,480.,240.,dates,spacing);
                let mut incremental=MasonryPlan::default();
                for end in (150..10000).step_by(150).chain(std::iter::once(10000)){
                    let old=incremental.tiles.len();
                    incremental.extend(&items[..end],&ratios,480.,240.,dates,spacing);
                    assert_eq!(incremental.tiles[..old],full.tiles[..old]);
                }
                assert_eq!(incremental.tiles,full.tiles);
                assert_eq!(incremental.days,full.days);
                assert_eq!(incremental.height,full.height);
                assert_eq!(incremental.max_tile_height,full.max_tile_height);
                assert!(full.max_tile_height>1200.);
                for lo in [0.,1250.,2000.,full.height/2.,full.height-500.]{
                    let hi=lo+600.;
                    let visible=full.visible_tiles(lo,hi).map(|t|t.id).collect::<Vec<_>>();
                    let brute=full.tiles.iter().filter(|t|t.y<=hi&&t.y+t.height+full.footer>=lo).map(|t|t.id).collect::<Vec<_>>();
                    assert_eq!(visible,brute);
                }
            }
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

/// Navigate by spatial geometry, not source index, in masonry.
pub(super) fn masonry_neighbor(plan:&MasonryPlan,index:usize,key:&str)->Option<usize>{
    let from=plan.tiles.iter().find(|t|t.index==index)?;
    let (x,y)=(from.x+from.width/2.,from.y+from.height/2.);
    plan.tiles.iter().filter(|t|t.index!=index).filter_map(|t|{
        let dx=t.x+t.width/2.-x;let dy=t.y+t.height/2.-y;
        let (primary,cross)=match key{"left"=>(-dx,dy),"right"=>(dx,dy),"up"=>(-dy,dx),"down"=>(dy,dx),_=>return None};
        if primary<=0.5{return None;}
        Some((t.index,primary+cross.abs()*3.))
    }).min_by(|a,b|a.1.total_cmp(&b.1)).map(|a|a.0)
}
/// Left/right within row; up/down select nearest horizontal center in next row.
pub(super) fn row_neighbor(rows:&[AlbumRow],index:usize,key:&str,gap:f32)->Option<usize>{
    fn centers(row:&AlbumRow,gap:f32)->Vec<(usize,f32)>{
        match row{
            AlbumRow::Tiles{items,..}=>{let mut x=0.;items.iter().map(|(i,w)|{let c=x+w/2.;x+=w+gap;(*i,c)}).collect()},
            AlbumRow::Item(i)=>vec![(*i,0.)],AlbumRow::Day(_)=>vec![],
        }
    }
    let r=rows.iter().position(|row|match row{
        AlbumRow::Tiles{items,..}=>items.first().zip(items.last()).is_some_and(|(a,b)|a.0<=index&&index<=b.0),
        AlbumRow::Item(i)=>*i==index,AlbumRow::Day(_)=>false,
    })?;
    let items=centers(&rows[r],gap);
    let at=items.iter().position(|(i,_)|*i==index)?;
    match key{
        "left"=>at.checked_sub(1).map(|i|items[i].0),
        "right"=>items.get(at+1).map(|t|t.0),
        "up"|"down"=>{
            let row=if key=="up"{rows[..r].iter().rev().find(|row|!matches!(row,AlbumRow::Day(_)))}else{rows[r+1..].iter().find(|row|!matches!(row,AlbumRow::Day(_)))}?;
            centers(row,gap).into_iter().min_by(|a,b|(a.1-items[at].1).abs().total_cmp(&(b.1-items[at].1).abs())).map(|t|t.0)
        }
        _=>None
    }
}
#[cfg(test)]
#[::core::prelude::v1::test]
fn spatial_navigation_uses_rows_and_columns(){
    let rows=vec![AlbumRow::Tiles{items:vec![(0,100.),(1,100.),(2,100.)],height:100.},AlbumRow::Day("next".into()),AlbumRow::Tiles{items:vec![(3,150.),(4,150.)],height:100.}];
    assert_eq!(row_neighbor(&rows,1,"down",10.),Some(4));
    assert_eq!(row_neighbor(&rows,2,"down",10.),Some(4));
    assert_eq!(row_neighbor(&rows,1,"left",10.),Some(0));
    assert_eq!(row_neighbor(&rows,2,"right",10.),None);
}
