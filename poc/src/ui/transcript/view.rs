use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    div, list, px, App, Context, EventEmitter, FocusHandle, ListAlignment, ListOffset, ListState,
    MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, Render, Window, prelude::*,
};

use crate::model::cursor::{page_amount, scroll_target};
use crate::model::links::invite_hit;
use crate::model::{mix_colors, row_from_message, Conversation, Pos, Row, Style, Transcript};
use crate::ui::message_row::{render_transcript_row, RowCallbacks, RowLayoutMap};
use crate::ui::scrollbar::{list_vertical_scrollbar, move_scrollbar_drag, ScrollbarDragCell};
use crate::ui::transcript::pointer::{self, RowBounds};
use crate::ui::Handler;
use crate::util::{open_web_link, style, to_gpui, OmaircStyleGlobal};

/// A click on a transcript author or avatar. The shell opens or creates the DM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NickClicked(pub String);

/// A click on the channel of an INVITE Status line. The shell joins it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteClicked(pub String);

impl EventEmitter<NickClicked> for TranscriptView {}
impl EventEmitter<InviteClicked> for TranscriptView {}

/// The transcript list: its model, selection, drag-select and link hover.
/// Owned as an entity so the shell only holds a handle.
pub struct TranscriptView {
    model: Transcript,
    list_state: ListState,
    row_layouts: RowLayoutMap,
    row_bounds: RowBounds,
    member_realnames: HashMap<String, String>,
    self_nick: String,
    composer_focus: FocusHandle,
    scrollbar_drag: ScrollbarDragCell,
    dragging: bool,
    drag_autoscroll: bool,
    drag_edge: Option<i8>,
    drag_pointer: Option<Point<Pixels>>,
    last_click: (Instant, u32, usize),
    hovered_link: Option<(usize, Range<usize>)>,
    /// The row find mode landed on, drawn with the selection colour.
    find_row: Option<usize>,
}

impl TranscriptView {
    pub fn new(composer_focus: FocusHandle, scrollbar_drag: ScrollbarDragCell) -> Self {
        Self {
            model: Transcript::new(72),
            // Measure every row so the scrollbar spans the whole transcript, not just
            // the rows rendered so far.
            list_state: ListState::new(0, ListAlignment::Bottom, px(120.)).measure_all(),
            row_layouts: Rc::new(RefCell::new(Default::default())),
            row_bounds: Rc::new(RefCell::new(Default::default())),
            member_realnames: HashMap::new(),
            self_nick: String::new(),
            composer_focus,
            scrollbar_drag,
            dragging: false,
            drag_autoscroll: false,
            drag_edge: None,
            drag_pointer: None,
            last_click: (Instant::now(), 0, 0),
            hovered_link: None,
            find_row: None,
        }
    }

    /// Shows a conversation's stored lines with a fresh selection and scroll.
    pub fn set_conversation(&mut self, conv: &Conversation, self_nick: &str) {
        self.model.set_conversation(&conv.messages);
        self.member_realnames = member_realnames(conv);
        self.self_nick = self_nick.to_string();
        self.model.clear_selection();
        self.find_row = None;
        self.list_state.reset(self.model.rows().len());
        self.row_layouts.borrow_mut().clear();
    }

    pub fn rows(&self) -> &[Row] {
        self.model.rows()
    }

    /// Shows a network's Status rows with a fresh selection. Status opens at the end.
    pub fn set_status(&mut self, rows: Vec<Row>) {
        self.model.set_rows(rows);
        self.member_realnames.clear();
        self.model.clear_selection();
        self.hovered_link = None;
        self.find_row = None;
        self.list_state.reset(self.model.rows().len());
        self.row_layouts.borrow_mut().clear();
    }

    /// Adds a line to the open conversation or Status pane: one the local user
    /// just sent, or a command reply. The list stays at the bottom.
    pub fn append_line(&mut self, row: Row) {
        let old_len = self.model.rows().len();
        self.model.append(row);
        self.model.set_pinned_to_bottom(true);
        self.list_state.splice(old_len..old_len, 1);
    }

    /// Reveals `row_ix` at the top of the list and leaves follow-the-end, as Qt's
    /// `revealRow` does for a find or link match. The caller notifies.
    pub fn scroll_to_row(&mut self, row_ix: usize) {
        self.model.set_pinned_to_bottom(false);
        self.list_state.scroll_to(ListOffset {
            item_ix: row_ix,
            offset_in_item: px(0.),
        });
    }

    /// The row find mode is on, drawn with the selection colour. `None` clears it.
    /// The caller notifies.
    pub fn set_find_row(&mut self, row: Option<usize>) {
        self.find_row = row;
    }

    /// PgUp/PgDn (or the half-page chords): scrolls by `page_amount` of the
    /// viewport height. Scrolling up releases the bottom pin.
    pub fn scroll_page(&mut self, down: bool, half: bool) {
        let viewport = f32::from(self.list_state.viewport_bounds().size.height);
        let amount = page_amount(viewport, half);
        if amount <= 0. {
            return;
        }
        self.scroll_by_px(if down { amount } else { -amount });
    }

    /// Moves the list by `delta` pixels from its real top. `ListState::scroll_by`
    /// is not used: while the list follows the tail, its top is the full content
    /// height, so a relative move from there lands past the bottom and the next
    /// layout snaps back. Clamping to the real range and setting the absolute
    /// offset avoids that, and reaching the bottom re-pins the tail.
    fn scroll_by_px(&mut self, delta: f32) {
        let list = &self.list_state;
        let max = f32::from(list.max_offset_for_scrollbar().height);
        let current = -f32::from(list.scroll_px_offset_for_scrollbar().y);
        let target = scroll_target(current, max, delta);
        self.model.set_pinned_to_bottom(target >= max);
        list.set_offset_from_scrollbar(Point::new(px(0.), px(target)));
    }

    /// Ctrl+Home: the first row.
    pub fn scroll_to_top(&mut self) {
        self.model.set_pinned_to_bottom(false);
        self.list_state.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.),
        });
    }

    /// Ctrl+End: the last row, and the list follows new lines again.
    pub fn scroll_to_bottom(&mut self) {
        let count = self.model.rows().len();
        if count == 0 {
            return;
        }
        self.model.set_pinned_to_bottom(true);
        self.list_state.scroll_to_reveal_item(count - 1);
    }

    pub fn has_selection(&self) -> bool {
        self.model.selection().is_some()
    }

    pub fn invalidate_all(&mut self) {
        self.model.invalidate_all();
    }

    pub fn copy_text(&self) -> String {
        self.model.copy_text()
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    fn mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Qt keeps focus in the composer when the transcript is clicked.
        window.focus(&self.composer_focus);
        let Some(row_ix) = self.row_at_position(ev.position) else {
            return;
        };
        let app_style = style(cx);
        let Some(pos) = self.pos_from_pointer(ev.position, row_ix, &app_style) else {
            return;
        };
        if ev.modifiers.shift {
            self.model.extend(pos);
        } else if !ev.modifiers.platform {
            self.model.begin(pos);
        }
        let now = Instant::now();
        if now.duration_since(self.last_click.0) < Duration::from_millis(400)
            && self.last_click.2 == row_ix
        {
            self.last_click.1 += 1;
        } else {
            self.last_click.1 = 1;
        }
        self.last_click.0 = now;
        self.last_click.2 = row_ix;
        if self.last_click.1 >= 3 {
            self.model.select_row(row_ix);
        } else if self.last_click.1 == 2 {
            self.model.select_word(pos);
        }
        self.dragging = true;
    }

    /// Extends the selection while the button is held. Returns false when no
    /// drag is in progress.
    pub fn drag_move(&mut self, position: Point<Pixels>, app_style: &Style) -> bool {
        if !self.dragging {
            return false;
        }
        self.update_drag_autoscroll(position);
        if !self.drag_autoscroll {
            if let Some(row_ix) = self.row_at_position(position) {
                if let Some(pos) = self.pos_from_pointer(position, row_ix, app_style) {
                    self.model.extend(pos);
                }
            }
        }
        true
    }

    /// Ends a drag on mouse-up. A release over a link acts on it: an http(s)
    /// URL opens in the browser, an INVITE channel asks the shell to join it.
    /// Returns false when no drag was in progress.
    pub fn finish_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        if !self.dragging {
            return false;
        }
        self.dragging = false;
        self.drag_autoscroll = false;
        self.drag_edge = None;
        self.drag_pointer = None;
        if let Some((row_ix, range)) = self.link_at(position) {
            self.activate_link(row_ix, range, cx);
        }
        true
    }

    fn activate_link(&self, row_ix: usize, range: Range<usize>, cx: &mut Context<Self>) {
        let row = &self.model.rows()[row_ix];
        // A URL takes precedence over the invite channel it might share a range with.
        if row.body.links.contains(&range) {
            if let Some(url) = row.body.plain.get(range) {
                open_web_link(cx, url);
            }
        } else if let Some((channel, channel_range)) = invite_hit(row) {
            if channel_range == range {
                cx.emit(InviteClicked(channel.to_string()));
            }
        }
    }

    pub fn tick_drag_autoscroll(&mut self, app_style: &Style) {
        if !self.dragging || !self.drag_autoscroll {
            return;
        }
        let Some(edge) = self.drag_edge else {
            return;
        };
        self.scroll_by_px(if edge < 0 { -8. } else { 8. });
        if let Some(position) = self.drag_pointer {
            self.extend_drag_autoscroll(position, app_style);
        }
    }

    fn row_at_position(&self, position: Point<Pixels>) -> Option<usize> {
        pointer::row_at(&self.row_bounds, position)
    }

    fn pos_from_pointer(
        &self,
        position: Point<Pixels>,
        row_ix: usize,
        app_style: &Style,
    ) -> Option<Pos> {
        pointer::pos_at(
            self.model.rows(),
            &self.row_layouts,
            &self.row_bounds,
            position,
            row_ix,
            app_style,
        )
    }

    fn link_at(&self, position: Point<Pixels>) -> Option<(usize, Range<usize>)> {
        pointer::link_at(self.model.rows(), &self.row_layouts, &self.row_bounds, position)
    }

    fn update_drag_autoscroll(&mut self, position: Point<Pixels>) {
        self.drag_pointer = Some(position);
        let viewport = self.list_state.viewport_bounds();
        if position.y < viewport.top() {
            self.drag_edge = Some(-1);
            self.drag_autoscroll = true;
        } else if position.y > viewport.bottom() {
            self.drag_edge = Some(1);
            self.drag_autoscroll = true;
        } else {
            self.drag_edge = None;
            self.drag_autoscroll = false;
        }
    }

    fn extend_drag_autoscroll(&mut self, position: Point<Pixels>, app_style: &Style) {
        let viewport = self.list_state.viewport_bounds();
        if position.y < viewport.top() {
            if let Some(row_ix) = pointer::first_visible(&self.row_bounds, viewport) {
                self.model.extend(Pos {
                    row: row_ix,
                    offset: 0,
                });
            }
            return;
        }
        if position.y > viewport.bottom() {
            if let Some(row_ix) = pointer::last_visible(&self.row_bounds, viewport) {
                let offset = self.model.rows()[row_ix].body.plain.len();
                self.model.extend(Pos { row: row_ix, offset });
            }
            return;
        }
        if let Some(row_ix) = self.row_at_position(position) {
            if let Some(pos) = self.pos_from_pointer(position, row_ix, app_style) {
                self.model.extend(pos);
            }
        }
    }

    /// Feeds the heights the last prepaint recorded into the transcript model.
    /// Reads `row_bounds` rather than `ListState::bounds_for_item`, which is
    /// `None` while the list follows the bottom.
    fn sync_measured_heights(&mut self) {
        for (ix, bounds) in self.row_bounds.borrow().iter() {
            self.model
                .set_measured(*ix, f32::from(bounds.size.height) as i32);
        }
    }
}

impl Render for TranscriptView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_style = style(cx);
        // Heights measured by the last frame's prepaint feed the transcript model.
        self.sync_measured_heights();
        // Rows re-record their bounds during this frame's prepaint; anything
        // scrolled out of the list must not keep a stale position.
        self.row_bounds.borrow_mut().clear();
        let entity = cx.entity();
        let drag = self.scrollbar_drag.clone();
        let row_bounds = self.row_bounds.clone();
        let layouts = self.row_layouts.clone();
        let member_realnames = self.member_realnames.clone();
        let list_state = self.list_state.clone();
        let focus = self.composer_focus.clone();
        let on_nick: Handler<String> = {
            let entity = entity.clone();
            Rc::new(move |nick: String, window: &mut Window, app: &mut App| {
                window.focus(&focus);
                entity.update(app, |_, cx| cx.emit(NickClicked(nick)));
            })
        };
        let self_nick = self.self_nick.clone();

        div()
            .id("transcript")
            .relative()
            .flex_grow()
            .min_h_0()
            // Not focusable: a tracked focus handle would take focus on click
            // and override the composer focus that Qt keeps.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, ev: &MouseDownEvent, window, cx| {
                    view.mouse_down(ev, window, cx);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|view, ev: &MouseMoveEvent, _, cx| {
                if view.scrollbar_drag.borrow().is_some() {
                    let app_style = style(cx);
                    move_scrollbar_drag(&view.scrollbar_drag, ev.position, &app_style);
                    cx.notify();
                } else if !view.dragging {
                    view.hovered_link = view.link_at(ev.position);
                    cx.notify();
                }
            }))
            .child({
                let entity = entity.clone();
                list(list_state, move |ix, window, app| {
                    let app_style = app.global::<OmaircStyleGlobal>().style.clone();
                    // The transcript selection is painted only while the window is active.
                    let window_active = window.is_window_active();
                    entity.update(app, |view, _cx| {
                        let row = view
                            .model
                            .rows()
                            .get(ix)
                            .cloned()
                            .unwrap_or_else(|| row_from_message(0, "", "", "", false));
                        let selection = window_active
                            .then(|| view.model.range_for_row(ix))
                            .flatten();
                        let hover = view
                            .hovered_link
                            .as_ref()
                            .filter(|(r, _)| *r == ix)
                            .map(|(_, range)| range.clone());
                        // Qt's find match is the page colour mixed 42% toward the selection.
                        let finding = view.find_row == Some(ix);
                        let find_fill = to_gpui(mix_colors(
                            app_style.page_color(),
                            app_style.selection_color(),
                            0.42,
                        ));
                        div()
                            .on_children_prepainted({
                                let row_bounds = row_bounds.clone();
                                move |bounds, _, _| {
                                    if let Some(bounds) = bounds.first() {
                                        row_bounds.borrow_mut().insert(ix, *bounds);
                                    }
                                }
                            })
                            .id(("transcript-row-hit", ix))
                            .w_full()
                            .when(finding, |this| this.bg(find_fill))
                            .child(render_transcript_row(
                                ix,
                                &row,
                                &app_style,
                                &member_realnames,
                                selection,
                                hover,
                                layouts.clone(),
                                &RowCallbacks {
                                    on_nick: on_nick.clone(),
                                    self_nick: self_nick.clone(),
                                },
                            ))
                            .into_any_element()
                    })
                })
                .size_full()
                .with_sizing_behavior(gpui::ListSizingBehavior::Auto)
            })
            .child(list_vertical_scrollbar(
                self.list_state.clone(),
                &app_style,
                entity,
                drag,
            ))
    }
}

/// Real names keyed by nick, plus the DM peer's own name.
pub fn member_realnames(conv: &Conversation) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for m in &conv.members {
        map.insert(m.nick.clone(), m.realname.clone());
    }
    if let Some(real) = &conv.peer_realname {
        if let Some(name) = conv.name.strip_prefix('#').or(Some(conv.name.as_str())) {
            map.entry(name.to_string()).or_insert_with(|| real.clone());
        }
        map.entry(conv.name.clone()).or_insert_with(|| real.clone());
    }
    map
}
