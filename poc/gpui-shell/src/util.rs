use gpui::{Hsla, px, Pixels, Rgba};
use shared::Style;

pub const FONT_FAMILY: &str = "iA Writer Mono S";

pub struct OmaircStyleGlobal {
    pub style: Style,
    pub theme_path: Option<std::path::PathBuf>,
}

impl gpui::Global for OmaircStyleGlobal {}

pub fn to_gpui(color: shared::Color) -> Hsla {
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

pub fn theme_path(cx: &gpui::App) -> Option<std::path::PathBuf> {
    cx.global::<OmaircStyleGlobal>().theme_path.clone()
}
