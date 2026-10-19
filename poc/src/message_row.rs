use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, Hsla, IntoElement, PaintQuad, Pixels,
    SharedString, StyledText, TextLayout, Window, div, fill, prelude::*, px, size,
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
    member_realnames: &HashMap<String, String>,
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
                .id(("transcript-row", row_ix))
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
                .id(("transcript-row", row_ix))
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
        RowKind::Message => div()
            .id(("transcript-row", row_ix))
            .w_full()
            .child(render_message_row(
                row_ix,
                row,
                style,
                member_realnames,
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
    member_realnames: &HashMap<String, String>,
    selection_range: Option<Range<usize>>,
    hovered_link: Option<Range<usize>>,
    layouts: RowLayoutMap,
) -> impl IntoElement {
    let author = row.author.as_deref().unwrap_or("");
    let nick_color = to_gpui(style.nick_color(author));
    let fills = style.nick_avatar_fills();
    let avatar_fill = to_gpui(fills[style.nick_palette_index(author)]);
    let initials = style.initials(author);
    let realname = member_realnames.get(author).map(String::as_str).unwrap_or("");
    let realname_suffix = if realname.is_empty() {
        String::new()
    } else {
        format!(" · {realname}")
    };

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
        selection_range.clone(),
    );
    if let Some(link) = hovered_link {
        runs = link_underline_run(runs, link, true);
    }

    let styled = StyledText::new(SharedString::from(row.body.plain.clone())).with_runs(runs);
    let selection_color = to_gpui(style.selection_color());

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
                                            .flex()
                                            .items_baseline()
                                            .gap_0()
                                            .child(
                                                div()
                                                    .text_size(scaled_px(style, 12.))
                                                    .font_weight(gpui::FontWeight::BOLD)
                                                    .text_color(nick_color)
                                                    .font_family(FONT_FAMILY)
                                                    .child(author.to_string()),
                                            )
                                            .when(!realname_suffix.is_empty(), |this| {
                                                this.child(
                                                    div()
                                                        .text_size(scaled_px(style, 12.))
                                                        .text_color(to_gpui(
                                                            style.muted_color(),
                                                        ))
                                                        .font_family(FONT_FAMILY)
                                                        .child(realname_suffix),
                                                )
                                            }),
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
                            selection_range,
                            selection_color,
                        }),
                ),
        )
}

fn selection_quads(
    layout: &TextLayout,
    bounds: Bounds<Pixels>,
    range: Range<usize>,
    color: Hsla,
) -> Vec<PaintQuad> {
    let line_height = layout.line_height();
    let len = layout.len();
    let start = range.start.min(len);
    let end = range.end.min(len);
    if start >= end {
        return Vec::new();
    }

    let mut line_starts = vec![0usize];
    for i in 1..len {
        let Some(pos) = layout.position_for_index(i) else {
            break;
        };
        let Some(prev) = layout.position_for_index(i - 1) else {
            continue;
        };
        if pos.y > prev.y {
            line_starts.push(i);
        }
    }
    line_starts.push(len);

    let mut quads = Vec::new();
    for chunk in line_starts.windows(2) {
        let line_start = chunk[0];
        let line_end = chunk[1];
        let sel_start = start.max(line_start);
        let sel_end = end.min(line_end);
        if sel_start >= sel_end {
            continue;
        }
        let Some(start_pos) = layout.position_for_index(sel_start) else {
            continue;
        };
        let width = if sel_end >= line_end {
            bounds.origin.x + bounds.size.width - start_pos.x
        } else {
            layout
                .position_for_index(sel_end)
                .map(|end_pos| (end_pos.x - start_pos.x).max(px(1.)))
                .unwrap_or(px(1.))
        };
        quads.push(fill(
            Bounds::new(start_pos, size(width.max(px(1.)), line_height)),
            color,
        ));
    }
    quads
}

struct BodyTextElement {
    row_ix: usize,
    text: StyledText,
    layouts: RowLayoutMap,
    avatar_top: Pixels,
    header_height: Pixels,
    selection_range: Option<Range<usize>>,
    selection_color: Hsla,
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
        let layout = self.text.layout();
        if let Some(range) = self.selection_range.clone() {
            for quad in selection_quads(&layout, bounds, range, self.selection_color) {
                window.paint_quad(quad);
            }
        }
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
