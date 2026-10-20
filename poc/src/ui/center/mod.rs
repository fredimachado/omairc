pub mod composer;
pub mod header;

use gpui::{div, prelude::*, Entity};

use crate::model::{Conversation, Style};
use crate::ui::center::composer::render_composer;
use crate::ui::center::header::render_header;
use crate::ui::input::SingleLineInput;
use crate::ui::transcript::TranscriptView;
use crate::ui::Handler;
use crate::util::to_gpui;

/// The conversation column: header, transcript, composer.
pub fn render_center(
    conv: &Conversation,
    title: &str,
    transcript: Entity<TranscriptView>,
    composer: Entity<SingleLineInput>,
    style: &Style,
    on_toggle_members: Handler<()>,
    on_send: Handler<()>,
) -> impl IntoElement {
    div()
        .flex_grow()
        .h_full()
        .min_h_0()
        .flex()
        .flex_col()
        .bg(to_gpui(style.page_color()))
        .child(render_header(conv, title, style, on_toggle_members))
        .child(
            div()
                .flex_grow()
                .min_h_0()
                .flex()
                .flex_col()
                .child(transcript),
        )
        .child(render_composer(composer, style, on_send))
}
