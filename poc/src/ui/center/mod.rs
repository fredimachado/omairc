pub mod composer;
pub mod header;
pub mod header_actions;
pub mod slash_popup;

use gpui::{div, prelude::*, AnyElement, Entity};

use crate::model::Style;
use crate::ui::center::composer::render_composer;
use crate::ui::center::header::{render_header, HeaderView};
use crate::ui::input::SingleLineInput;
use crate::ui::transcript::TranscriptView;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui};

/// The gap between the transcript and the composer shell (Qt's `messageList`
/// `anchors.bottomMargin`).
const TRANSCRIPT_GAP: f32 = 12.;

/// The centre column: header, transcript, composer. The header is the
/// conversation's or the Status pane's, so the caller builds `header`. `popup`
/// is the slash-complete popup, when it shows. `focused` and `has_text` drive the
/// composer's border and SEND (see `render_composer`).
///
/// The column takes the width the row leaves after the sidebar and the member
/// panel. It may shrink below its content (`min_w_0`), so a long header subtitle or an
/// unwrapped line elides or clips inside it instead of pushing the people pill, SEND,
/// or the transcript's right edge past the window.
#[allow(clippy::too_many_arguments)]
pub fn render_center(
    header: HeaderView<'_>,
    transcript: Entity<TranscriptView>,
    composer: Entity<SingleLineInput>,
    style: &Style,
    on_toggle_members: Handler<()>,
    on_send: Handler<()>,
    popup: Option<AnyElement>,
    focused: bool,
    has_text: bool,
) -> impl IntoElement {
    div()
        .flex_grow()
        .min_w_0()
        .overflow_hidden()
        .h_full()
        .min_h_0()
        .flex()
        .flex_col()
        .bg(to_gpui(style.page_color()))
        .child(render_header(header, style, on_toggle_members))
        .child(
            div()
                .flex_grow()
                .min_h_0()
                .pb(scaled_px(style, TRANSCRIPT_GAP))
                .flex()
                .flex_col()
                .child(transcript),
        )
        .child(render_composer(
            composer, style, on_send, popup, focused, has_text,
        ))
}
