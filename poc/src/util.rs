use gpui::{Hsla, px, Pixels, Rgba};
use crate::model::Style;

pub const FONT_FAMILY: &str = "iA Writer Mono S";

pub struct OmaircStyleGlobal {
    pub style: Style,
    pub theme_path: Option<std::path::PathBuf>,
}

impl gpui::Global for OmaircStyleGlobal {}

pub fn to_gpui(color: crate::model::Color) -> Hsla {
    Hsla::from(Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    })
}

pub fn scaled_px(style: &Style, value: f32) -> Pixels {
    px(style.scaled(value) as f32)
}

pub fn style(cx: &gpui::App) -> Style {
    cx.global::<OmaircStyleGlobal>().style.clone()
}

/// Opens an http(s) link in the browser. Anything else is refused, the same
/// policy the transcript and the Link sheet apply. Returns whether it opened.
pub fn open_web_link(cx: &gpui::App, url: &str) -> bool {
    if !crate::model::format::is_allowed_http_url(url) {
        return false;
    }
    cx.open_url(url);
    true
}

pub fn theme_path(cx: &gpui::App) -> Option<std::path::PathBuf> {
    cx.global::<OmaircStyleGlobal>().theme_path.clone()
}
