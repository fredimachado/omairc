use gpui::{div, prelude::*, px, AnyElement, CursorStyle, Entity, MouseButton};

use crate::model::{mix_colors, Color, Style};
use crate::ui::input::SingleLineInput;
use crate::ui::{Handler, BOTTOM_BAR_HEIGHT};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The shell's height: Qt's `composerShell` minimum.
const SHELL_HEIGHT: f32 = 46.;

/// The shell's side inset (Qt's `composerShell` margins). The slash popup uses the same
/// side inset, so its edges are the shell's edges.
pub const SHELL_INSET: f32 = 20.;

/// The composer strip under the transcript (`client/src/ui/center/`). The strip is
/// `BOTTOM_BAR_HEIGHT` tall, the same as the identity footer, and uses the same
/// `border_t_1` top rule so both dividers share one row across the window. The 46px
/// shell sits vertically centred in the full 66px strip: `(66 - 46) / 2` = 10px above
/// and below. The unscaled top border eats 1px of that top gap, so an unscaled 1px
/// bottom padding mirrors it; without that padding `items_center` would centre in the
/// 65px content box (9.5 / 10.5) and snap unevenly at 150%. It holds the single-line
/// draft and the SEND button, and its side edges are `SHELL_INSET` in. The shell's
/// border is the accent while the draft has focus (`focused`), else the divider.
/// `has_text` says whether the draft has anything to send. A click on SEND calls
/// `on_send`. Enter is handled by the shell root (`ComposerSubmit`), which checks
/// header focus. `popup` is the slash-complete popup, if it shows. It is a child of
/// the strip, not the shell, so it can hang above the strip's rule with the shell's
/// side edges.
pub fn render_composer(
    composer: Entity<SingleLineInput>,
    style: &Style,
    on_send: Handler<()>,
    on_file: Option<Handler<()>>,
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
        .h(scaled_px(style, BOTTOM_BAR_HEIGHT))
        .flex_shrink_0()
        .flex()
        .items_center()
        .border_t_1()
        .border_color(to_gpui(style.divider_color()))
        // Mirrors the unscaled top border so `items_center` centres the shell in the
        // full strip, not the 65px content box under the rule.
        .pb(px(1.))
        .debug_selector(|| "composer-strip".into())
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
                .debug_selector(|| "composer-shell".into())
                .child(
                    div()
                        .flex_grow()
                        .min_w_0()
                        .pl(scaled_px(style, 8.))
                        .child(composer),
                )
                .when_some(on_file, |this, on_file| {
                    this.child(file_button(style, on_file))
                })
                .child(send_button(style, has_text, on_send)),
        )
        .when_some(popup, |this, popup| this.child(popup))
}

/// FILE: 46 by 30 with a 7px radius, left of SEND. Hover mixes the raised panel
/// toward the ink; a press calls `on_file`.
fn file_button(style: &Style, on_file: Handler<()>) -> impl IntoElement {
    let raised = style.raised_color();
    let hover = mix_colors(raised, style.ink_color(), 0.08);
    div()
        .w(scaled_px(style, 46.))
        .h(scaled_px(style, 30.))
        .mr(scaled_px(style, 8.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(scaled_px(style, 7.))
        .bg(to_gpui(raised))
        .font_family(FONT_FAMILY)
        .font_weight(gpui::FontWeight::BOLD)
        .text_size(scaled_px(style, 9.))
        .text_color(to_gpui(style.muted_color()))
        .cursor_pointer()
        .hover(move |button| button.bg(to_gpui(hover)))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_file((), window, cx);
        })
        .child("FILE")
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
