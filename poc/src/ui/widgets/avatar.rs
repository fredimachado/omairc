//! A nick's avatar: the initials on the nick's palette fill, with a photo clipped into
//! the circle over them once its image is ready, and the presence dot. Qt's
//! `NickGlyph.qml` shows the photo when there is one and the initials until it loads
//! or when there is none. The photo is an `ImageSource` the shell resolved
//! (`ui::shell::avatars`): a bundled asset, or a fetched avatar already decoded.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{div, img, prelude::*, Div, Image, ImageFormat, ImageSource, ObjectFit, Resource, StyledImage};

use crate::model::Style;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The photos one frame draws, keyed by avatar reference. A reference with no entry
/// draws its initials: the avatars preference is off, the fetch has not finished, or
/// it failed. The shell builds it each frame (`ui::shell::avatars`).
#[derive(Clone, Default)]
pub struct AvatarPhotos {
    sources: HashMap<String, ImageSource>,
}

impl AvatarPhotos {
    pub fn new(sources: HashMap<String, ImageSource>) -> Self {
        Self { sources }
    }

    /// The photo for an avatar reference, when it is ready.
    pub fn get(&self, reference: &str) -> Option<ImageSource> {
        self.sources.get(reference).cloned()
    }

    /// Whether `other` draws the same references. A view repaints only when this
    /// changes, since the image of a reference changes only when it becomes ready.
    pub fn same_references(&self, other: &Self) -> bool {
        self.sources.len() == other.sources.len()
            && self.sources.keys().all(|key| other.sources.contains_key(key))
    }
}

/// A bundled asset's photo source, by its path under `qrc:/` (`demo/mira-avatar.png`).
pub fn embedded_photo(path: &str) -> ImageSource {
    ImageSource::Resource(Resource::Embedded(path.to_string().into()))
}

/// A fetched avatar's photo source: the PNG the avatar fetch kept, handed to gpui to
/// decode. gpui decodes it on its background loader and paints nothing until then.
pub fn fetched_photo(png: &[u8]) -> ImageSource {
    ImageSource::Image(Arc::new(Image::from_bytes(ImageFormat::Png, png.to_vec())))
}

/// A photo over the full size of its positioned parent, cover-fitted. It paints
/// nothing until it has decoded, so the initials under it show meanwhile. The caller
/// sets the corner radius, which clips the photo.
pub fn photo_layer(source: ImageSource) -> gpui::Img {
    img(source)
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .object_fit(ObjectFit::Cover)
}

/// A round avatar of `size` px: the photo when there is one, else the initials at
/// `initials_px` on the nick's fill. The caller positions it.
pub fn avatar_disc(
    style: &Style,
    nick: &str,
    size: f32,
    initials_px: f32,
    photo: Option<ImageSource>,
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
        Some(source) => disc.child(photo_layer(source).rounded_full()),
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
    photo: Option<ImageSource>,
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
