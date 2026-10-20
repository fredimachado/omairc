//! A nick's avatar: the initials on the nick's palette fill, with a bundled photo
//! clipped into the circle over them once it has decoded, and the presence dot.
//! Qt's `NickGlyph.qml` shows the photo when there is one and the initials until
//! it loads or when there is none.

use gpui::{div, img, prelude::*, Div, ImageSource, ObjectFit, Resource, StyledImage};

use crate::model::Style;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// A bundled photo over the full size of its positioned parent, cover-fitted. It
/// paints nothing until it has decoded, so the initials under it show meanwhile.
/// The caller sets the corner radius, which clips the photo.
pub fn photo_layer(path: &str) -> gpui::Img {
    img(ImageSource::Resource(Resource::Embedded(
        path.to_string().into(),
    )))
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .object_fit(ObjectFit::Cover)
}

/// A round avatar of `size` px: the photo when `photo` is a bundled path, else the
/// initials at `initials_px` on the nick's fill. The caller positions it.
pub fn avatar_disc(
    style: &Style,
    nick: &str,
    size: f32,
    initials_px: f32,
    photo: Option<&str>,
) -> Div {
    let fill = to_gpui(style.nick_avatar_fills()[style.nick_palette_index(nick)]);
    let disc = div()
        .relative()
        .size(scaled_px(style, size))
        .flex_shrink_0()
        .rounded_full()
        .bg(fill)
        .flex()
        .items_center()
        .justify_center()
        .text_size(scaled_px(style, initials_px))
        // Qt's NickGlyph initial is bold (iA Writer Mono S Bold, weight 700).
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(to_gpui(style.nick_color(nick)))
        .font_family(FONT_FAMILY)
        .child(style.initials(nick));
    match photo {
        Some(path) => disc.child(photo_layer(path).rounded_full()),
        None => disc,
    }
}

/// An avatar with the presence dot in its bottom-right corner. `presence` is the
/// member's `presence` string (`online` or `away`).
pub fn avatar_with_presence(
    style: &Style,
    nick: &str,
    size: f32,
    initials_px: f32,
    photo: Option<&str>,
    presence: &str,
) -> impl IntoElement {
    let avatar_sz = scaled_px(style, size);
    let dot_sz = scaled_px(style, 7.);
    let inset = avatar_sz - dot_sz;
    div()
        .relative()
        .size(avatar_sz)
        .flex_shrink_0()
        .child(avatar_disc(style, nick, size, initials_px, photo))
        .child(
            div()
                .absolute()
                .left(inset)
                .top(inset)
                .size(dot_sz)
                .rounded_full()
                .border(scaled_px(style, 2.))
                .border_color(to_gpui(style.panel_color()))
                .bg(to_gpui(style.presence_color(presence))),
        )
}
