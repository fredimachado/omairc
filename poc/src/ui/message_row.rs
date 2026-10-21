use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, Div, Element, ElementId, GlobalElementId, Hsla, IntoElement, MouseButton,
    PaintQuad, Pixels, SharedString, Stateful, StyledText, TextLayout, Window, div, fill,
    prelude::*, px, size,
};
use crate::model::mention::{is_mention_row, mention_wash};
use crate::model::presence::{photo_for, NickFacts};
use crate::model::{mix_colors, Row, RowKind, Style};

use crate::ui::format_ui::{gpui_text_style, irc_runs_to_gpui, link_underline_run};
use crate::ui::status_row::render_status_row;
use crate::ui::widgets::avatar::{avatar_disc, AvatarPhotos};
use crate::ui::widgets::bot_mark::bot_mark;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// What a message row reports back to the shell. `self_nick` is inert: a click on
/// your own author or avatar does nothing.
pub struct RowCallbacks {
    pub on_nick: Handler<String>,
    pub self_nick: String,
}

/// What a row reads about the nicks it names: each nick's facts on the network,
/// whether peer photos show (the `avatars` preference), and the photos the frame draws.
#[derive(Clone, Default)]
pub struct RowPeers {
    pub facts: HashMap<String, NickFacts>,
    pub avatars: bool,
    pub photos: AvatarPhotos,
}

/// Makes an author name or avatar open that nick's DM. The click stops here so
/// it does not start a transcript selection.
trait NickClick: Sized {
    fn nick_click(self, nick: Option<String>, on_nick: &Handler<String>) -> Self;
}

impl NickClick for Div {
    fn nick_click(self, nick: Option<String>, on_nick: &Handler<String>) -> Self {
        match nick {
            Some(nick) => {
                let on_nick = on_nick.clone();
                self.cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        on_nick(nick.clone(), window, cx);
                    })
            }
            None => self,
        }
    }
}

#[derive(Clone, Default)]
pub struct RowLayout {
    pub body_bounds: Bounds<Pixels>,
    pub layout: TextLayout,
    pub avatar_top: Pixels,
    pub header_height: Pixels,
}

pub type RowLayoutMap = Rc<RefCell<std::collections::HashMap<usize, RowLayout>>>;

/// What every transcript row draws with: the style, the peers' facts, the map
/// rows record their layouts into, and the shell's callbacks. The view builds
/// one per row from the frame's values.
pub struct RowContext<'a> {
    pub style: &'a Style,
    pub peers: &'a RowPeers,
    pub layouts: RowLayoutMap,
    pub callbacks: &'a RowCallbacks,
}

pub fn render_transcript_row(
    row_ix: usize,
    row: &Row,
    ctx: RowContext<'_>,
    selection_range: Option<Range<usize>>,
    hovered_link: Option<Range<usize>>,
) -> impl IntoElement {
    let style = ctx.style;
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
        RowKind::Status => render_status_row(
            row_ix,
            row,
            style,
            selection_range,
            hovered_link,
            ctx.layouts,
        ),
        RowKind::Whois => render_whois_row(row_ix, row, style, selection_range, ctx.layouts),
        RowKind::UnreadMark => render_unread_mark_row(row_ix, style),
        RowKind::Message => div()
            .id(("transcript-row", row_ix))
            .w_full()
            .child(render_message_row(
                row_ix,
                row,
                ctx,
                selection_range,
                hovered_link,
            )),
    }
}

/// The "New messages" boundary (`UnreadMark.qml`): the caption between two rules.
/// Both are tinted toward the accent, so the boundary reads as a boundary rather
/// than as a join caption. It is not selectable and it does not take pointer hits.
fn render_unread_mark_row(row_ix: usize, style: &Style) -> Stateful<Div> {
    let dark = style.dark_mode();
    let mark = to_gpui(mix_colors(
        style.ink_color(),
        style.accent_color(),
        if dark { 0.55 } else { 0.42 },
    ));
    let rule = to_gpui(mix_colors(
        style.page_color(),
        style.accent_color(),
        if dark { 0.48 } else { 0.36 },
    ));
    div()
        .id(("transcript-row", row_ix))
        .w_full()
        .py(scaled_px(style, 8.))
        .px(scaled_px(style, 24.))
        .flex()
        .items_center()
        .child(div().flex_grow().h(scaled_px(style, 1.)).bg(rule))
        .child(
            div()
                .mx(scaled_px(style, 10.))
                .text_size(scaled_px(style, 10.))
                .text_color(mark)
                .font_family(FONT_FAMILY)
                .child("New messages"),
        )
        .child(div().flex_grow().h(scaled_px(style, 1.)).bg(rule))
}

/// A whois-style line: left-aligned, muted, wrapping, and selectable. Qt's
/// `MessageRow.qml` sets its TextEdit in the body column: 70 from the left and
/// 34 from the right. It has no time, label or avatar, so nothing sits beside it.
/// Copy and search read `row.body.plain`, the same text the row draws.
fn render_whois_row(
    row_ix: usize,
    row: &Row,
    style: &Style,
    selection_range: Option<Range<usize>>,
    layouts: RowLayoutMap,
) -> Stateful<Div> {
    let mut base = gpui_text_style(style, 12., false);
    base.color = to_gpui(style.muted_color());
    let runs = irc_runs_to_gpui(
        &row.body.plain,
        &row.body.runs,
        style,
        &base,
        selection_range.clone(),
    );
    let text = StyledText::new(SharedString::from(row.body.plain.clone())).with_runs(runs);
    div()
        .id(("transcript-row", row_ix))
        .w_full()
        .pt(scaled_px(style, 4.))
        .pb(scaled_px(style, 4.))
        .pl(scaled_px(style, 70.))
        .pr(scaled_px(style, 34.))
        .text_size(scaled_px(style, 12.))
        .font_family(FONT_FAMILY)
        .child(BodyTextElement {
            row_ix,
            text,
            layouts,
            avatar_top: px(0.),
            header_height: px(0.),
            selection_range,
            selection_color: to_gpui(style.selection_color()),
        })
}

fn render_message_row(
    row_ix: usize,
    row: &Row,
    ctx: RowContext<'_>,
    selection_range: Option<Range<usize>>,
    hovered_link: Option<Range<usize>>,
) -> impl IntoElement {
    let RowContext {
        style,
        peers,
        layouts,
        callbacks,
    } = ctx;
    let author = row.author.as_deref().unwrap_or("");
    // Only the first row of a group has a clickable author. Self is inert.
    let nick_target = (!row.continues && !author.is_empty() && author != callbacks.self_nick)
        .then(|| author.to_string());
    let nick_color = to_gpui(style.nick_color(author));
    let facts = peers.facts.get(author);
    let realname = facts.map_or("", |facts| facts.realname.as_str());
    let realname_suffix = if realname.is_empty() {
        String::new()
    } else {
        format!(" · {realname}")
    };
    let bot = facts.is_some_and(|facts| facts.bot);
    let photo = photo_for(facts.and_then(|facts| facts.avatar.as_deref()), peers.avatars)
        .and_then(|reference| peers.photos.get(reference));

    let left_pad = scaled_px(style, 24.);
    let top_pad = scaled_px(style, 9.);
    let body_left = scaled_px(style, 70.);
    let body_right = scaled_px(style, 34.);
    let header_h = scaled_px(style, if row.continues { 0. } else { 16. });

    let mut base_body_style = gpui_text_style(style, 13., false);
    if row.action {
        base_body_style.font_style = gpui::FontStyle::Italic;
        base_body_style.color = to_gpui(style.muted_color());
    }
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
    // A line that names the local nick is washed, with a bar on its left edge.
    let mention = is_mention_row(row, &callbacks.self_nick);
    let (wash, wash_bar) = mention_wash(style);

    div()
        .w_full()
        .relative()
        .when(mention, |this| this.bg(to_gpui(wash)))
        .when(mention, |this| {
            this.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(scaled_px(style, 2.))
                    .bg(to_gpui(wash_bar)),
            )
        })
        // Qt draws the avatar only on the first line of a group.
        .when(!row.continues, |this| {
            this.child(
                avatar_disc(style, author, 34., 13., photo)
                    .absolute()
                    .left(left_pad)
                    .top(top_pad)
                    .nick_click(nick_target.clone(), &callbacks.on_nick),
            )
        })
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
                                                    .child(author.to_string())
                                                    .nick_click(nick_target.clone(), &callbacks.on_nick),
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
                                            })
                                            .when(bot, |this| {
                                                this.child(
                                                    div()
                                                        .ml(scaled_px(style, 4.))
                                                        .child(bot_mark(style)),
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
                        // StyledText takes its size from the ancestor text style and
                        // TextRun carries none, so the body sets Qt's 13px here.
                        .child(
                            div()
                                .text_size(scaled_px(style, 13.))
                                .font_family(FONT_FAMILY)
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

/// A row's body text. Painting it records its layout, which the transcript's
/// pointer hit testing reads. Message and Status rows both use it.
pub(crate) struct BodyTextElement {
    pub(crate) row_ix: usize,
    pub(crate) text: StyledText,
    pub(crate) layouts: RowLayoutMap,
    pub(crate) avatar_top: Pixels,
    pub(crate) header_height: Pixels,
    pub(crate) selection_range: Option<Range<usize>>,
    pub(crate) selection_color: Hsla,
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
            for quad in selection_quads(layout, bounds, range, self.selection_color) {
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
