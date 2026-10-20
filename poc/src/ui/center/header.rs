use gpui::{div, prelude::*, MouseButton};

use crate::model::Style;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// What the header shows. A conversation passes its topic as the subtitle and
/// its people count; Status passes `Connected` and no count.
pub struct HeaderView<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    /// `Some` shows the `N PEOPLE` pill, which only channels have.
    pub people: Option<usize>,
}

/// The title, the subtitle line, and the `N PEOPLE` pill when there is a count.
pub fn render_header(
    view: HeaderView<'_>,
    style: &Style,
    on_toggle_members: Handler<()>,
) -> impl IntoElement {
    let HeaderView {
        title,
        subtitle,
        people,
    } = view;

    div()
        .px(scaled_px(style, 16.))
        .py(scaled_px(style, 10.))
        .border_b_1()
        .border_color(to_gpui(style.divider_color()))
        .flex()
        .flex_col()
        .gap(scaled_px(style, 4.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(scaled_px(style, 8.))
                .child(
                    div()
                        .flex_grow()
                        .text_size(scaled_px(style, 14.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(to_gpui(style.ink_color()))
                        .font_family(FONT_FAMILY)
                        .child(title.to_string()),
                )
                .when_some(people, |this, people| {
                    this.child(
                        div()
                            .px(scaled_px(style, 8.))
                            .py(scaled_px(style, 3.))
                            .rounded_full()
                            .border_1()
                            .border_color(to_gpui(style.divider_color()))
                            .cursor_pointer()
                            .text_size(scaled_px(style, 10.))
                            .text_color(to_gpui(style.muted_color()))
                            .font_family(FONT_FAMILY)
                            .child(format!("{} PEOPLE", people))
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                on_toggle_members((), window, cx);
                            }),
                    )
                }),
        )
        .child(
            div()
                .text_size(scaled_px(style, 11.))
                .text_color(to_gpui(style.muted_color()))
                .font_family(FONT_FAMILY)
                .child(subtitle.to_string()),
        )
}
