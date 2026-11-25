//! Pure selection policy; layout is never mutated by selection.
use std::collections::HashSet;
pub(super) const LIMIT:usize=150;
pub(super) fn click(ids:&mut HashSet<i64>,order:&[i64],anchor:&mut Option<i64>,id:i64,extend:bool,toggle:bool){
    if !order.contains(&id){return;}
    if extend{
        let a=anchor.and_then(|id|order.iter().position(|x|*x==id));
        if let (Some(a),Some(b))=(a,order.iter().position(|x|*x==id)){
            if !toggle{ids.clear();}
            // Keep the clicked endpoint in a capped contiguous range, in either direction.
            // Additive selection retains the existing set and only fills free slots.
            if a<=b{
                for key in order[a..=b].iter().rev(){if ids.len()>=LIMIT{break;}ids.insert(*key);}
            }else{
                for key in &order[b..=a]{if ids.len()>=LIMIT{break;}ids.insert(*key);}
            }
            return;
        }
    }
    if toggle{if !ids.remove(&id)&&ids.len()<LIMIT{ids.insert(id);}}
    else{ids.clear();ids.insert(id);}
    *anchor=Some(id);
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn ranges_toggle_and_limit(){
        let order=(1..=200).collect::<Vec<_>>();let mut ids=HashSet::new();let mut a=None;
        click(&mut ids,&order,&mut a,3,false,false);assert_eq!(ids,HashSet::from([3]));
        click(&mut ids,&order,&mut a,6,true,false);assert_eq!(ids,HashSet::from([3,4,5,6]));
        click(&mut ids,&order,&mut a,5,false,true);assert!(!ids.contains(&5));
        click(&mut ids,&order,&mut a,200,true,false);assert_eq!(ids.len(),150);assert!(ids.contains(&200));assert!(ids.contains(&51));
        click(&mut ids,&order,&mut a,200,false,false);
        click(&mut ids,&order,&mut a,1,true,false);
        assert_eq!(ids.len(),150);assert!(ids.contains(&1));assert!(ids.contains(&150));assert!(!ids.contains(&151));
        click(&mut ids,&order,&mut a,200,true,true);
        assert_eq!(ids.len(),150);assert!(ids.contains(&1));
    }
}
