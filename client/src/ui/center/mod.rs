pub mod composer;
pub mod header;
pub mod header_actions;
pub mod slash_popup;

use std::rc::Rc;

use gpui::{div, prelude::*, transparent_black, AnyElement, App, Entity, ExternalPaths, Window};

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

/// Hitbox group for the centre column. The absolute file-drag frame listens with
/// `group_drag_over` so the accent border appears only while an `ExternalPaths`
/// drag is over this column, not for a drag elsewhere in the window.
const CENTER_GROUP: &str = "center-column";

/// The centre column: header, transcript, composer. The header is the
/// conversation's or the Status pane's, so the caller builds `header`. `popup`
/// is the slash-complete popup, when it shows. `focused` and `has_text` drive the
/// composer's border and SEND (see `render_composer`).
/// The column takes the width the row leaves after the sidebar and the member
/// panel. It may shrink below its content (`min_w_0`), so a long header subtitle or an
/// unwrapped line elides or clips inside it instead of pushing the people pill, SEND
/// or the transcript's right edge past the window.
///
/// File-drag feedback is an absolute accent frame, not a layout border. A permanent
/// `border_2` (even painted `page_color`) insets the composer strip by 2px and lifts
/// its top rule above the identity footer's divider. The frame mounts only when a
/// file host is offered (`file_offered`); it keeps a transparent 2px border and takes
/// the accent via `group_drag_over` while an `ExternalPaths` drag is over this column.
/// The column's `.drag_over` is what makes gpui repaint as the drag enters and leaves.
/// Drops stay on the column's own `.on_drop`. The frame keeps a Normal, non-blocking
/// hitbox (a no-op `.hover`) so gpui's `group_drag_over` path can refine its border;
/// without that hitbox the accent never applies.
#[allow(clippy::too_many_arguments)]
pub fn render_center(
    header: HeaderView<'_>,
    transcript: Entity<TranscriptView>,
    composer: Entity<SingleLineInput>,
    style: &Style,
    on_toggle_members: Handler<()>,
    on_send: Handler<()>,
    on_file: Option<Handler<()>>,
    popup: Option<AnyElement>,
    focused: bool,
    has_text: bool,
    file_offered: bool,
    on_external_paths: Rc<dyn Fn(&ExternalPaths, &mut Window, &mut App)>,
) -> impl IntoElement {
    let accent = to_gpui(style.accent_color());
    div()
        .group(CENTER_GROUP)
        .relative()
        .flex_grow()
        .min_w_0()
        .overflow_hidden()
        .h_full()
        .min_h_0()
        .flex()
        .flex_col()
        .bg(to_gpui(style.page_color()))
        .drag_over::<ExternalPaths>(|style, _, _, _| style)
        .on_drop(move |paths, window, cx| on_external_paths(paths, window, cx))
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
            composer, style, on_send, on_file, popup, focused, has_text,
        ))
        .when(file_offered, |this| {
            this.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .border_2()
                    .border_color(transparent_black())
                    // Normal hitbox so `group_drag_over` can refine the border. gpui
                    // skips hover styles while a drag is active, and Normal does not
                    // block the column's on_drop or the transcript/composer below.
                    .hover(|style| style)
                    .group_drag_over::<ExternalPaths>(CENTER_GROUP, move |style| {
                        style.border_color(accent)
                    })
                    // Paint-time probe: drag_over only runs when this frame has a
                    // hitbox and is hovered, the same gate `group_drag_over` needs.
                    .drag_over::<ExternalPaths>(|style, _, _, _| {
                        #[cfg(test)]
                        drag_frame_probe::note_active();
                        style
                    })
                    .debug_selector(|| "center-file-drag-frame".into()),
            )
        })
}

/// Test-only paint probe for the file-drag frame's accent path. `drag_over` on the
/// frame calls `note_active` only when the frame has a hitbox and is hovered during
/// an `ExternalPaths` drag - the same condition `group_drag_over` needs to apply the
/// accent border.
#[cfg(test)]
pub(crate) mod drag_frame_probe {
    use std::cell::Cell;

    thread_local! {
        static ACTIVE: Cell<bool> = const { Cell::new(false) };
    }

    pub fn reset() {
        ACTIVE.with(|cell| cell.set(false));
    }

    pub fn note_active() {
        ACTIVE.with(|cell| cell.set(true));
    }

    pub fn active() -> bool {
        ACTIVE.with(|cell| cell.get())
    }
}
