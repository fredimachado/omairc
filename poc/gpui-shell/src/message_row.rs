use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, IntoElement, Pixels, SharedString,
    StyledText, TextLayout, Window, div, prelude::*,
};
use shared::{Row, RowKind, Style};

use crate::format_ui::{gpui_text_style, irc_runs_to_gpui, link_underline_run};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

#[derive(Clone, Default)]
pub struct RowLayout {
    pub body_bounds: Bounds<Pixels>,
    pub layout: TextLayout,
    pub avatar_top: Pixels,
    pub header_height: Pixels,
}

pub type RowLayoutMap = Rc<RefCell<std::collections::HashMap<usize, RowLayout>>>;

pub fn render_transcript_row(
    row_ix: usize,
    row: &Row,
    style: &Style,
    selection_range: Option<Range<usize>>,
    hovered_link: Option<Range<usize>>,
    layouts: RowLayoutMap,
) -> impl IntoElement {
    match row.kind {
        RowKind::DaySeparator => {
            let text = row
                .separator_text
                .as_deref()
                .unwrap_or(row.body.plain.as_str());
            div()
                .w_full()
                .py(scaled_px(style, 8.))
                .flex()
                .justify_center()
                .child(
                    div()
                        .text_size(scaled_px(style, 10.))
                        .text_color(to_gpui(style.muted_color()))
                        .font_family(FONT_FAMILY)
                        .child(text.to_string()),
                )
        }
        RowKind::Event => {
            let text = row
                .event_text
                .as_deref()
                .unwrap_or(row.body.plain.as_str());
            div()
                .w_full()
                .py(scaled_px(style, 6.))
                .flex()
                .justify_center()
                .child(
                    div()
                        .text_size(scaled_px(style, 10.))
                        .text_color(to_gpui(style.muted_color()))
                        .font_family(FONT_FAMILY)
                        .child(text.to_string()),
                )
        }
        RowKind::Message => div().child(render_message_row(
            row_ix,
            row,
            style,
            selection_range,
            hovered_link,
            layouts,
        )),
    }
}

fn render_message_row(
    row_ix: usize,
    row: &Row,
    style: &Style,
    selection_range: Option<Range<usize>>,
    hovered_link: Option<Range<usize>>,
    layouts: RowLayoutMap,
) -> impl IntoElement {
    let author = row.author.as_deref().unwrap_or("");
    let nick_color = to_gpui(style.nick_color(author));
    let fills = style.nick_avatar_fills();
    let avatar_fill = to_gpui(fills[style.nick_palette_index(author)]);
    let initials = style.initials(author);

    let avatar_size = scaled_px(style, 34.);
    let left_pad = scaled_px(style, 24.);
    let top_pad = scaled_px(style, 9.);
    let body_left = scaled_px(style, 70.);
    let body_right = scaled_px(style, 34.);
    let header_h = scaled_px(style, if row.continues { 0. } else { 16. });

    let base_body_style = gpui_text_style(style, 13., false);
    let mut runs = irc_runs_to_gpui(
        &row.body.plain,
        &row.body.runs,
        style,
        &base_body_style,
        selection_range,
    );
    if let Some(link) = hovered_link {
        runs = link_underline_run(runs, link, true);
    }

    let styled = StyledText::new(SharedString::from(row.body.plain.clone())).with_runs(runs);

    div()
        .w_full()
        .relative()
        .child(
            div()
                .absolute()
                .left(left_pad)
                .top(top_pad)
                .size(avatar_size)
                .rounded_full()
                .bg(avatar_fill)
                .flex()
                .items_center()
                .justify_center()
                .text_size(scaled_px(style, 12.))
                .text_color(nick_color)
                .font_family(FONT_FAMILY)
                .child(initials),
        )
        .child(
            div()
                .pl(body_left)
                .pr(body_right)
                .pt(if row.continues {
                    scaled_px(style, 4.)
                } else {
                    scaled_px(style, 6.)
                })
                .pb(scaled_px(style, 6.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(scaled_px(style, 2.))
                        .when(!row.continues, |this| {
                            this.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(scaled_px(style, 6.))
                                    .child(
                                        div()
                                            .text_size(scaled_px(style, 12.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(nick_color)
                                            .font_family(FONT_FAMILY)
                                            .child(author.to_string()),
                                    )
                                    .child(
                                        div()
                                            .flex_grow()
                                            .flex()
                                            .justify_end()
                                            .text_size(scaled_px(style, 9.))
                                            .text_color(to_gpui(style.muted_color()))
                                            .font_family(FONT_FAMILY)
                                            .child(
                                                row.time
                                                    .as_deref()
                                                    .unwrap_or("--:--")
                                                    .to_string(),
                                            ),
                                    ),
                            )
                        })
                        .child(BodyTextElement {
                            row_ix,
                            text: styled,
                            layouts,
                            avatar_top: top_pad,
                            header_height: header_h,
                        }),
                ),
        )
}

struct BodyTextElement {
    row_ix: usize,
    text: StyledText,
    layouts: RowLayoutMap,
    avatar_top: Pixels,
    header_height: Pixels,
}

impl IntoElement for BodyTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BodyTextElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        self.text.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.text
            .prepaint(id, inspector_id, bounds, request_layout, window, cx);
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.text.paint(id, inspector_id, bounds, request_layout, prepaint, window, cx);
        self.layouts.borrow_mut().insert(
            self.row_ix,
            RowLayout {
                body_bounds: bounds,
                layout: self.text.layout().clone(),
                avatar_top: self.avatar_top,
                header_height: self.header_height,
            },
        );
    }
}
