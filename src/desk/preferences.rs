use serde_json::{Value,json};
#[derive(Clone,Debug,PartialEq)]
pub(crate) struct Preferences {
    pub comfortable:bool,pub dark:bool,pub layout:String,pub tile:f32,pub oldest:bool,pub favorites:bool,pub dates:bool,
    pub width:f32,pub height:f32,pub shortcut:String,
}
impl Default for Preferences {
    fn default()->Self{Self{comfortable:true,dark:true,layout:"grid".into(),tile:148.,oldest:false,favorites:false,dates:true,width:1160.,height:800.,shortcut:"U".into()}}
}
impl Preferences {
    pub fn decode(v:Value)->Self{
        let d=Self::default();
        let number=|k:&str,default:f32,min:f32,max:f32|v[k].as_f64().filter(|n|n.is_finite()).map(|n|(n as f32).clamp(min,max)).unwrap_or(default);
        Self{comfortable:v["comfortable"].as_bool().unwrap_or(d.comfortable),dark:v["dark"].as_bool().unwrap_or(d.dark),layout:match v["layout"].as_str(){Some("list")=>"list",Some("masonry")=>"masonry",Some("justified")=>"justified",_=>"grid"}.into(),
            tile:number("tile",d.tile,96.,240.),oldest:v["oldest"].as_bool().unwrap_or(d.oldest),favorites:v["favorites"].as_bool().unwrap_or(d.favorites),dates:v["dates"].as_bool().unwrap_or(d.dates),
            width:number("width",d.width,720.,3840.),height:number("height",d.height,600.,2160.),
            shortcut:match v["shortcut"].as_str(){Some("I")=>"I",Some("off")=>"off",_=>"U"}.into()}
    }
    pub fn encode(&self)->Value{json!({"comfortable":self.comfortable,"dark":self.dark,"layout":self.layout,"tile":self.tile,"oldest":self.oldest,"favorites":self.favorites,"dates":self.dates,"width":self.width,"height":self.height,"shortcut":self.shortcut})}
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn bounds_and_unknown_values(){let p=Preferences::decode(json!({"tile":999,"width":1,"layout":"invalid","shortcut":"evil","dark":false}));assert_eq!(p.tile,240.);assert_eq!(p.width,720.);assert_eq!(p.layout,"grid");assert_eq!(p.shortcut,"U");assert_eq!(Preferences::decode(p.encode()),p);assert!(p.comfortable);assert!(!Preferences::decode(json!({"comfortable":false})).comfortable);}
}
