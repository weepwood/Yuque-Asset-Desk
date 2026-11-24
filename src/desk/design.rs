//! Shared visual language for the workbench, independent of business state.
use gpui_kit::*;
use gpui_kit::component::theme::{Theme,ThemeMode};

pub(super) const CONTROL_RADIUS:f32=6.;
pub(super) const FEEDBACK_MS:u64=160;

#[derive(Clone,Copy)]
pub(super) struct Spacing{
    pub pad:f32,pub gap:f32,pub day:f32,pub section:f32,pub row:f32,pub nav:f32,pub sidebar:f32,pub inspector:f32,pub nav_pad:f32,pub footer:f32,pub title:f32,pub body:f32,pub caption:f32,pub control:f32,
}
impl Spacing{
    pub fn new(comfortable:bool)->Self{
        if comfortable{Self{pad:32.,gap:24.,day:44.,section:22.,row:20.,nav:10.,sidebar:200.,inspector:320.,nav_pad:12.,footer:32.,title:21.,body:14.,caption:12.,control:8.}}
        else{Self{pad:16.,gap:10.,day:28.,section:8.,row:12.,nav:4.,sidebar:184.,inspector:280.,nav_pad:8.,footer:24.,title:18.,body:13.,caption:11.,control:4.}}
    }
    pub fn sidebar_width(self,narrow:bool)->f32{if narrow{150.}else{self.sidebar}}
}
#[derive(Clone,Copy)]
pub(super) struct Palette {
    pub canvas:Rgba,pub sidebar:Rgba,pub surface:Rgba,pub subtle:Rgba,pub line:Rgba,
    pub text:Rgba,pub muted:Rgba,pub accent:Rgba,pub accent_soft:Rgba,pub error:Rgba,
}
impl Palette {
    pub fn new(dark:bool)->Self {
        let c=|a,b|rgb(if dark{a}else{b});
        Self{canvas:c(0x151619,0xfafaf9),sidebar:c(0x1c1d20,0xf0f1f2),surface:c(0x222428,0xffffff),
            subtle:c(0x2b2d32,0xe8eaed),line:c(0x2d3035,0xe3e4e6),text:c(0xe8e9ed,0x25272b),
            muted:c(0xa1a5af,0x60656f),accent:c(0x8bc9b5,0x28745e),accent_soft:c(0x263e37,0xdceee7),
            error:c(0xf2ad9e,0x9f392b)}
    }
}
pub(crate) fn apply(dark:bool,window:Option<&mut Window>,cx:&mut App){
    Theme::change(if dark{ThemeMode::Dark}else{ThemeMode::Light},window,cx);
    let p=Palette::new(dark);
    Theme::update(cx,|t|{
        t.colors.background=p.canvas.into();t.colors.foreground=p.text.into();
        t.colors.sidebar=p.sidebar.into();t.colors.sidebar_foreground=p.muted.into();
        t.colors.sidebar_border=p.line.into();t.colors.sidebar_accent=p.accent_soft.into();
        t.colors.sidebar_accent_foreground=p.text.into();t.colors.sidebar_primary=p.accent.into();
        t.colors.border=p.line.into();t.colors.muted=p.subtle.into();t.colors.muted_foreground=p.muted.into();
        t.colors.accent=p.accent_soft.into();t.colors.accent_foreground=p.text.into();
        t.colors.primary=p.accent.into();t.colors.primary_foreground=rgb(if dark{0x102a21}else{0xffffff}).into();
        t.colors.popover=p.surface.into();t.colors.popover_foreground=p.text.into();
        t.colors.button_primary=p.accent.into();
        t.colors.button_primary_foreground=t.colors.primary_foreground;
        t.colors.button_primary_hover=rgb(if dark{0xa0d4c3}else{0x216651}).into();
        t.colors.button_primary_active=rgb(if dark{0x76b49f}else{0x195440}).into();
        t.radius=px(CONTROL_RADIUS);
        t.motion.duration_fast=std::time::Duration::from_millis(120);
        t.motion.duration_normal=std::time::Duration::from_millis(180);
        t.motion.duration_slow=std::time::Duration::from_millis(240);
    });
}
