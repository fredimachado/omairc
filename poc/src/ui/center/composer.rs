use gpui::{div, prelude::*, px, AnyElement, CursorStyle, Entity, MouseButton};

use crate::model::{mix_colors, Color, Style};
use crate::ui::input::SingleLineInput;
use crate::ui::{Handler, BOTTOM_BAR_HEIGHT};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The shell's height: Qt's `composerShell` minimum.
const SHELL_HEIGHT: f32 = 46.;

/// The shell's side and bottom inset (Qt's `composerShell` margins). The slash popup
/// uses the same side inset, so its edges are the shell's edges.
pub const SHELL_INSET: f32 = 20.;

/// The composer strip under the transcript, a port of `composerShell` in
/// `ConversationColumn.qml`. The strip is `BOTTOM_BAR_HEIGHT` tall, the same as the
/// identity footer, and its top rule is drawn on the footer's divider row, so the two
/// rules line up across the window. The shell sits at the strip's top, `SHELL_INSET` in
/// from the sides and the bottom, so the shell's top edge shares that row too, as Qt's
/// does. The rule is an absolute child, not the strip's own border, so the shell keeps
/// its place at the top. Inside the strip sits the 46px shell that holds the single-line
/// draft and the SEND button. The shell's border is the accent while the draft has focus
/// (`focused`), else the divider.
///
/// `has_text` says whether the draft has anything to send. A click on SEND calls
/// `on_send`. Enter is handled by the shell root (`ComposerSubmit`), which checks
/// header focus. `popup` is the slash-complete popup, if it shows. It is a child of
/// the strip, not the shell, so it can hang above the shell's top edge with the shell's
/// side edges, and it stands above the rule.
pub fn render_composer(
    composer: Entity<SingleLineInput>,
    style: &Style,
    on_send: Handler<()>,
    popup: Option<AnyElement>,
    focused: bool,
    has_text: bool,
) -> impl IntoElement {
    let border = if focused {
        style.accent_color()
    } else {
        style.divider_color()
    };

    div()
        .relative()
        .px(scaled_px(style, SHELL_INSET))
        .pb(scaled_px(style, SHELL_INSET))
        .h(scaled_px(style, BOTTOM_BAR_HEIGHT))
        .flex_shrink_0()
        .flex()
        .items_start()
        // The rule on the footer's divider row, drawn as the footer draws its rule: a
        // 1px top border. A filled 1px bar snaps to two device rows at 150%, the border
        // to one. It comes before the shell, so the shell's own border paints over it
        // while the draft has focus.
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h(px(1.))
                .border_t_1()
                .border_color(to_gpui(style.divider_color())),
        )
        .child(
            div()
                .flex_grow()
                .min_w_0()
                .h(scaled_px(style, SHELL_HEIGHT))
                .flex()
                .items_center()
                .key_context("Composer")
                .rounded(scaled_px(style, 10.))
                .bg(to_gpui(style.panel_color()))
                .border_1()
                .border_color(to_gpui(border))
                .child(
                    div()
                        .flex_grow()
                        .min_w_0()
                        .pl(scaled_px(style, 8.))
                        .child(composer),
                )
                .child(send_button(style, has_text, on_send)),
        )
        .when_some(popup, |this, popup| this.child(popup))
}

/// SEND: 54 by 30 with a 7px radius, 8px in from the shell's right edge. With text
/// it is the accent with a white label. Hover deepens the accent toward the ink, and
/// a press calls `on_send`. Empty, it is the raised panel with a muted label, takes
/// no press, and shows the arrow cursor.
fn send_button(style: &Style, has_text: bool, on_send: Handler<()>) -> impl IntoElement {
    let accent = style.accent_color();
    let (fill, label) = if has_text {
        (accent, Color::rgb(1., 1., 1.))
    } else {
        (style.raised_color(), style.muted_color())
    };
    let lit_hover = mix_colors(accent, style.ink_color(), 0.14);

    div()
        .w(scaled_px(style, 54.))
        .h(scaled_px(style, 30.))
        .mr(scaled_px(style, 8.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(scaled_px(style, 7.))
        .bg(to_gpui(fill))
        .font_family(FONT_FAMILY)
        .font_weight(gpui::FontWeight::BOLD)
        .text_size(scaled_px(style, 9.))
        .text_color(to_gpui(label))
        .when(has_text, |this| {
            this.cursor_pointer()
                .hover(move |button| button.bg(to_gpui(lit_hover)))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    on_send((), window, cx);
                })
        })
        .when(!has_text, |this| this.cursor(CursorStyle::Arrow))
        .child("SEND")
}
