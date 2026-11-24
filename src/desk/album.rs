use super::*;

#[derive(Debug,Clone)]
pub(super) enum AlbumRow {
    Day(String),
    Tiles { items: Vec<(usize,f32)>, height:f32 },
    Item(usize),
}

/// Stable order, no cropping; last justified row never grows beyond target height.
pub(super) fn plan_rows(assets:&[Asset],ratios:&std::collections::HashMap<i64,f32>,width:f32,target:f32,grid:bool,justified:bool,dates:bool)->Vec<AlbumRow>{
    let width=width.max(160.);
    let target=target.clamp(96.,240.);
    let cols=((width+8.)/(target+8.)).floor().max(1.) as usize;
    let equal=(width-8.*(cols-1) as f32)/cols as f32;
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
                if n>1 && min_width*n as f32+8.*(n-1) as f32>width {break;}
                sum+=(ratio(cursor)*target).max(min_width);
                cursor+=1;
                if sum+8.*(n-1) as f32>=width{break;}
            }
            let count=cursor-start;
            let usable=width-8.*(count-1) as f32;
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

#[cfg(test)]
mod tests{
    use super::*;
    #[::core::prelude::v1::test]
    fn rows_preserve_order_and_width(){
        let items=(0..10000).map(|i|Asset{id:i,name:String::new(),bytes:0,category:"image".into(),provider:"demo".into(),url:String::new(),sha256:String::new(),owner_id:"local".into(),filekey:String::new(),created_at:if i<5000{"2026-11-24"}else{"2026-11-23"}.into()}).collect::<Vec<_>>();
        let ratios=items.iter().map(|a|(a.id,0.4+(a.id%20) as f32*0.13)).collect();
        for justified in [false,true]{
            let rows=plan_rows(&items,&ratios,800.,148.,true,justified,true);
            let mut seen=Vec::new();
            for row in rows {if let AlbumRow::Tiles{items,..}=row{
                assert!(items.iter().all(|(_,w)|*w>=96.));
                assert!(items.iter().map(|(_,w)|w).sum::<f32>()+8.*items.len().saturating_sub(1) as f32<=800.1);
                seen.extend(items.into_iter().map(|(i,_)|i));
            }}
            assert_eq!(seen,(0..10000).collect::<Vec<_>>());
        }
    }
}
