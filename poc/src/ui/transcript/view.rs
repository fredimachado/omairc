use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    div, list, px, App, Context, EventEmitter, FocusHandle, ListAlignment, ListOffset, ListState,
    MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, Render, Window, prelude::*,
};
#[cfg(target_os = "linux")]
use gpui::ClipboardItem;

use crate::model::cursor::{page_amount, scroll_target};
use crate::model::links::{conversation_channel_hits, invite_hit};
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::world::Message;
use crate::model::presence::NickFacts;
use crate::model::scroll_place::{resume, Resume, SavedPlace};
use crate::model::typing_tail::typing_continues;
use crate::model::unread::{self, UnseenLanding};
use crate::model::transcript::minute_of;
use crate::model::{mix_colors, row_from_message, Pos, Row, Style, Transcript};
use crate::ui::message_row::{
    render_transcript_row, RowCallbacks, RowContext, RowLayoutMap, RowPeers,
};
use crate::ui::scrollbar::{list_vertical_scrollbar, move_scrollbar_drag, ScrollbarDragCell};
use crate::ui::transcript::pointer::{self, RowBounds};
use crate::ui::transcript::unseen_jump::{render_unseen_jump, UnseenBounce};
use crate::ui::typing_row::render_typing_row;
use crate::ui::widgets::avatar::AvatarPhotos;
use crate::ui::Handler;
use crate::util::{style, to_gpui, OmaircStyleGlobal};

/// A click on a transcript author or avatar. The shell opens or creates the DM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NickClicked(pub String);

impl EventEmitter<NickClicked> for TranscriptView {}

/// The reader crossed the end of the transcript by a wheel scroll. Emitted only when
/// `at_end` differs from the bit last reported, so the shell publishes the read marker
/// on a change and not on every tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptScrolled {
    pub at_end: bool,
}

impl EventEmitter<TranscriptScrolled> for TranscriptView {}

/// What a release over a transcript link asks of the shell. The shell acts on it
/// with its window, so a channel can open its sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkClick {
    /// An http(s) URL: it opens in the browser.
    Url(String),
    /// The channel of an INVITE Status line: the shell joins it.
    Invite(String),
    /// A channel name in a chat line: the shell switches to it, or asks first.
    Channel(String),
}

/// The transcript list: its model, selection, drag-select and link hover.
/// Owned as an entity so the shell only holds a handle.
pub struct TranscriptView {
    model: Transcript,
    list_state: ListState,
    row_layouts: RowLayoutMap,
    row_bounds: RowBounds,
    /// What the rows read by nick, and whether peer photos show.
    peers: RowPeers,
    self_nick: String,
    /// The network's features: its CHANTYPES decide what a channel name in a line is.
    features: IrcServerFeatures,
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
    /// The DM peer whose typing dots end the list, when that peer is typing.
    typing_peer: Option<String>,
    /// The typing dots' pulse frame, set by the shell's typing pulse.
    typing_phase: u8,
    /// The row of the first line that arrived while the reader was scrolled up.
    /// The unseen jump lands there. It clears when the reader reaches the end, or
    /// jumps (Qt `firstUnseenIndex`).
    first_unseen: Option<usize>,
    /// The `↓` chip bounce after a detached arrival (`UnseenJumpButton.bounceBurst`).
    unseen_bounce: UnseenBounce,
    /// The at-end bit the shell last reported. A wheel scroll emits `TranscriptScrolled`
    /// only when the list's at-end bit differs from it.
    reported_at_end: bool,
}

impl TranscriptView {
    pub fn new(
        composer_focus: FocusHandle,
        scrollbar_drag: ScrollbarDragCell,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = Self {
            model: Transcript::new(72),
            // Measure every row so the scrollbar spans the whole transcript, not just
            // the rows rendered so far.
            list_state: ListState::new(0, ListAlignment::Bottom, px(120.)).measure_all(),
            row_layouts: Rc::new(RefCell::new(Default::default())),
            row_bounds: Rc::new(RefCell::new(Default::default())),
            peers: RowPeers::default(),
            self_nick: String::new(),
            features: IrcServerFeatures::new(),
            composer_focus,
            scrollbar_drag,
            dragging: false,
            drag_autoscroll: false,
            drag_edge: None,
            drag_pointer: None,
            last_click: (Instant::now(), 0, 0),
            hovered_link: None,
            find_row: None,
            typing_peer: None,
            typing_phase: 0,
            first_unseen: None,
            unseen_bounce: UnseenBounce::default(),
            reported_at_end: true,
        };
        // gpui calls the list's scroll handler only for wheel scrolling, and while the list
        // is still borrowed, so the at-end read waits for the effect cycle to finish. Every
        // other move of the list calls `note_scroll_end` itself, after the move.
        let transcript = cx.weak_entity();
        view.list_state.set_scroll_handler(move |_, _, cx| {
            let transcript = transcript.clone();
            cx.defer(move |cx| {
                transcript
                    .update(cx, |view, cx| view.note_scroll_end(cx))
                    .ok();
            });
        });
        view
    }

    /// Records the at-end bit the shell has just reported, so the next scroll emits only
    /// a change from it.
    pub fn note_reported_at_end(&mut self, at_end: bool) {
        self.reported_at_end = at_end;
    }

    /// Emits `TranscriptScrolled` when the at-end bit differs from the bit last reported.
    /// Every move of the list calls this after moving it: the wheel handler, and the key,
    /// scrollbar, find, link, unseen and notification moves. `restore_scroll_place` does
    /// not, because the list has just been reset and is not measured yet; the shell
    /// reports that viewport itself.
    pub fn note_scroll_end(&mut self, cx: &mut Context<Self>) {
        let at_end = self.at_end();
        if at_end != self.reported_at_end {
            self.reported_at_end = at_end;
            cx.emit(TranscriptScrolled { at_end });
        }
    }

    /// Shows a conversation's lines with a fresh selection and scroll. The lines carry
    /// their membership folding already: the reducer applies `/pref joins` as lines
    /// arrive. A DM whose peer is typing gets the typing dots after its last line.
    pub fn set_conversation(&mut self, lines: ConversationLines) {
        self.model.set_conversation(lines.messages, lines.mark);
        self.peers.facts = lines.facts;
        self.self_nick = lines.self_nick.to_owned();
        self.features = lines.features.clone();
        self.typing_peer = lines.typing_peer;
        self.model.clear_selection();
        self.find_row = None;
        // Switching buffers drops any catch-up (Qt `resetPending`).
        self.first_unseen = None;
        self.list_state.reset(self.list_len());
        self.row_layouts.borrow_mut().clear();
    }

    /// Reloads the open conversation without clearing selection, find, or unseen
    /// state. The list follows the end when the reader was already there.
    pub fn refresh_conversation(&mut self, lines: ConversationLines, cx: &mut Context<Self>) {
        let previous_rows = self.model.rows().len();
        let was_at_end = self.at_end();
        let saved = (!was_at_end).then(|| self.capture_scroll_place());
        self.peers.facts = lines.facts;
        self.self_nick = lines.self_nick.to_owned();
        self.features = lines.features.clone();
        self.typing_peer = lines.typing_peer;
        self.model
            .replace_conversation(lines.messages, lines.mark);
        let new_rows = self.model.rows().len();
        unread::note_row_growth(previous_rows, new_rows, was_at_end, &mut self.first_unseen);
        if unread::detached_arrival(previous_rows, new_rows, was_at_end) {
            self.unseen_bounce.restart();
        }
        self.list_state.reset(self.list_len());
        self.row_layouts.borrow_mut().clear();
        if was_at_end {
            self.model.set_pinned_to_bottom(true);
            self.list_state
                .scroll_to_reveal_item(self.list_len().saturating_sub(1));
            self.note_scroll_end(cx);
        } else if let Some(place) = saved {
            self.restore_scroll_place(Some(&place), false);
        }
        cx.notify();
    }

    /// Updates only the typing footer for the open direct message.
    pub fn refresh_typing_peer(&mut self, peer: Option<String>, cx: &mut Context<Self>) {
        if self.typing_peer == peer {
            return;
        }
        let was_at_end = self.at_end();
        let saved = (!was_at_end).then(|| self.capture_scroll_place());
        self.typing_peer = peer;
        self.list_state.reset(self.list_len());
        if was_at_end {
            self.model.set_pinned_to_bottom(true);
            self.list_state
                .scroll_to_reveal_item(self.list_len().saturating_sub(1));
            self.note_scroll_end(cx);
        } else if let Some(place) = saved {
            self.restore_scroll_place(Some(&place), false);
        }
        cx.notify();
    }

    /// The `avatars` preference. Off, the rows show initials. The rows are drawn
    /// again on the next frame, so a change shows at once.
    pub fn set_avatars(&mut self, on: bool) {
        self.peers.avatars = on;
    }

    /// The photos the frame draws. Returns whether the references changed, so the
    /// caller repaints only then.
    pub fn set_photos(&mut self, photos: AvatarPhotos) -> bool {
        if self.peers.photos.same_references(&photos) {
            return false;
        }
        self.peers.photos = photos;
        true
    }

    pub fn rows(&self) -> &[Row] {
        self.model.rows()
    }

    /// The DM peer whose typing dots end the list, when they show.
    pub fn typing_peer(&self) -> Option<&str> {
        self.typing_peer.as_deref()
    }

    /// The row of the first line that arrived while the reader was scrolled up.
    pub fn first_unseen_index(&self) -> Option<usize> {
        self.first_unseen
    }

    /// The list's logical top: which item and how far into that item.
    pub fn logical_scroll_top(&self) -> ListOffset {
        self.list_state.logical_scroll_top()
    }

    /// The list's items: every row, then the typing dots when they show.
    pub fn list_item_count(&self) -> usize {
        self.list_len()
    }

    /// Whether the content is taller than the viewport (the scrollbar can move).
    pub fn content_overflows_viewport(&self) -> bool {
        f32::from(self.list_state.max_offset_for_scrollbar().height) > 1.
    }

    /// Shows a network's Status rows with a fresh selection. Status opens at the end
    /// and has no typing dots.
    pub fn set_status(&mut self, rows: Vec<Row>) {
        self.model.set_rows(rows);
        self.peers.facts.clear();
        self.typing_peer = None;
        self.model.clear_selection();
        self.hovered_link = None;
        self.find_row = None;
        self.first_unseen = None;
        self.list_state.reset(self.list_len());
        self.row_layouts.borrow_mut().clear();
    }

    /// The list's items: every row, then the typing dots when they show.
    fn list_len(&self) -> usize {
        self.model.rows().len() + usize::from(self.typing_peer.is_some())
    }

    /// The typing dots' pulse frame. Repaints only while the dots show.
    pub fn set_typing_phase(&mut self, phase: u8, cx: &mut Context<Self>) {
        if self.typing_peer.is_some() && self.typing_phase != phase {
            self.typing_phase = phase;
            cx.notify();
        }
    }

    /// Where the reader is now, for the scroll-place memory. The live list position
    /// is written into `Transcript::scroll()`, and the place is read back from there.
    /// Pinned to the end, or scrolled onto the typing dots, is the end.
    pub fn capture_scroll_place(&mut self) -> SavedPlace {
        let rows = self.model.rows().len();
        let top = self.list_state.logical_scroll_top();
        if self.at_end() || top.item_ix >= rows {
            self.model.set_pinned_to_bottom(true);
        } else {
            let offset_px = f32::from(top.offset_in_item).round() as i32;
            self.model.set_scroll_anchor(top.item_ix, offset_px);
        }
        SavedPlace::from_anchor(self.model.scroll(), self.model.rows())
    }

    /// Puts the list at `place`, once `set_conversation` or `set_status` has reset
    /// it. A missing or end place leaves the reset list following the tail, unless
    /// `open_at_unread` is on and the conversation holds a "New messages" mark: then
    /// the mark is the top of the view (Qt `pinTranscriptToUnreadOr`).
    pub fn restore_scroll_place(&mut self, place: Option<&SavedPlace>, open_at_unread: bool) {
        let rows = self.model.rows();
        let mark = unread::opening_row(rows, open_at_unread);
        match resume(place, rows) {
            Resume::End => match mark {
                Some(row) => self.reveal_row(row),
                None => self.model.set_pinned_to_bottom(true),
            },
            Resume::Line { row, offset_px } => {
                self.model.set_scroll_anchor(row, offset_px);
                self.list_state.scroll_to(ListOffset {
                    item_ix: row,
                    offset_in_item: px(offset_px as f32),
                });
            }
        }
    }

    /// Whether the list shows its end. The scrollbar's offset is measured in
    /// pixels, and a list that follows the tail reports past its maximum.
    pub fn at_end(&self) -> bool {
        let max = f32::from(self.list_state.max_offset_for_scrollbar().height);
        let current = -f32::from(self.list_state.scroll_px_offset_for_scrollbar().y);
        current >= max - 1.
    }

    /// Whether the list shows its top row.
    pub fn at_top(&self) -> bool {
        let current = -f32::from(self.list_state.scroll_px_offset_for_scrollbar().y);
        current <= 1.
    }

    /// Focus returns to the window: the list lands on its open mark, and keeps its
    /// place when there is none (Qt `pinTranscriptOnFocusReturn`).
    pub fn land_on_unread_mark(&mut self, cx: &mut Context<Self>) {
        let mark = unread::unread_mark_row(self.model.rows());
        if let Some(row) = mark {
            self.scroll_to_row(row, cx);
        }
    }

    /// Where the unseen jump lands now, for the `↓` chip and Alt+U.
    pub fn unseen_target(&self) -> Option<UnseenLanding> {
        unread::unseen_landing(!self.at_end(), self.first_unseen, self.model.rows())
    }

    /// Alt+U and the `↓` chip (Qt `jumpToUnseen`): to the first line that arrived
    /// while the reader was scrolled up, else to the end while a mark is open.
    /// Returns whether the view moved. It does nothing while the reader follows the
    /// end.
    /// When the unseen chip should repaint for its bounce animation.
    pub fn unseen_bounce_next_frame(&self) -> Option<std::time::Duration> {
        self.unseen_bounce.next_frame_in()
    }

    pub fn jump_to_unseen(&mut self, cx: &mut Context<Self>) -> bool {
        match self.unseen_target() {
            Some(UnseenLanding::Row(row)) => {
                self.first_unseen = None;
                self.scroll_to_row(row, cx);
                true
            }
            Some(UnseenLanding::End) => {
                self.first_unseen = None;
                self.scroll_to_bottom(cx);
                true
            }
            None => false,
        }
    }

    /// Scrolls to the chat row whose `msgid` matches, when the transcript holds one.
    pub fn scroll_to_msgid(&mut self, msgid: &str, cx: &mut Context<Self>) -> bool {
        let rows = self.model.rows();
        if let Some(row) = crate::model::transcript::row_for_msgid(rows, msgid) {
            self.scroll_to_row(row, cx);
            true
        } else {
            false
        }
    }

    /// Reveals `row_ix` at the top of the list and leaves follow-the-end, as Qt's
    /// `revealRow` does for a find or link match. The caller notifies.
    pub fn scroll_to_row(&mut self, row_ix: usize, cx: &mut Context<Self>) {
        self.reveal_row(row_ix);
        self.note_scroll_end(cx);
    }

    /// The move behind `scroll_to_row`, without the at-end note.
    fn reveal_row(&mut self, row_ix: usize) {
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
    pub fn scroll_page(&mut self, down: bool, half: bool, cx: &mut Context<Self>) {
        let viewport = f32::from(self.list_state.viewport_bounds().size.height);
        let amount = page_amount(viewport, half);
        if amount <= 0. {
            return;
        }
        self.scroll_by_px(if down { amount } else { -amount }, cx);
    }

    /// Moves the list by `delta` pixels from its real top. `ListState::scroll_by`
    /// is not used: while the list follows the tail, its top is the full content
    /// height, so a relative move from there lands past the bottom and the next
    /// layout snaps back. Clamping to the real range and setting the absolute
    /// offset avoids that, and reaching the bottom re-pins the tail.
    fn scroll_by_px(&mut self, delta: f32, cx: &mut Context<Self>) {
        let list = &self.list_state;
        let max = f32::from(list.max_offset_for_scrollbar().height);
        let current = -f32::from(list.scroll_px_offset_for_scrollbar().y);
        let target = scroll_target(current, max, delta);
        self.model.set_pinned_to_bottom(target >= max);
        list.set_offset_from_scrollbar(Point::new(px(0.), px(target)));
        self.note_scroll_end(cx);
    }

    /// Ctrl+Home: the first row.
    pub fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
        self.model.set_pinned_to_bottom(false);
        self.list_state.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.),
        });
        self.note_scroll_end(cx);
    }

    /// Ctrl+End: the last row, and the list follows new lines again.
    pub fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        let count = self.list_len();
        if count == 0 {
            return;
        }
        self.model.set_pinned_to_bottom(true);
        self.list_state.scroll_to_reveal_item(count - 1);
        self.note_scroll_end(cx);
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
        self.write_primary_selection(cx);
    }

    /// Extends the selection while the button is held. Returns false when no
    /// drag is in progress.
    pub fn drag_move(
        &mut self,
        position: Point<Pixels>,
        app_style: &Style,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.dragging {
            return false;
        }
        self.update_drag_autoscroll(position);
        if !self.drag_autoscroll {
            if let Some(row_ix) = self.row_at_position(position) {
                if let Some(pos) = self.pos_from_pointer(position, row_ix, app_style) {
                    self.model.extend(pos);
                    self.write_primary_selection(cx);
                }
            }
        }
        true
    }

    /// Ends a drag on mouse-up. Returns what a release over a link asks for, or
    /// None for a release anywhere else or with no drag in progress.
    pub fn finish_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<LinkClick> {
        if !self.dragging {
            return None;
        }
        self.dragging = false;
        self.drag_autoscroll = false;
        self.drag_edge = None;
        self.drag_pointer = None;
        self.write_primary_selection(cx);
        let (row_ix, range) = self.link_at(position)?;
        link_click(&self.model.rows()[row_ix], &range, &self.features)
    }

    pub fn tick_drag_autoscroll(&mut self, app_style: &Style, cx: &mut Context<Self>) {
        if !self.dragging || !self.drag_autoscroll {
            return;
        }
        let Some(edge) = self.drag_edge else {
            return;
        };
        self.scroll_by_px(if edge < 0 { -8. } else { 8. }, cx);
        if let Some(position) = self.drag_pointer {
            self.extend_drag_autoscroll(position, app_style);
            self.write_primary_selection(cx);
        }
    }

    /// Writes the transcript selection to the Linux primary buffer.
    fn write_primary_selection(&self, cx: &mut Context<Self>) {
        #[cfg(target_os = "linux")]
        {
            let text = self.model.copy_text();
            if !text.is_empty() {
                cx.write_to_primary(ClipboardItem::new_string(text));
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = cx;
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
        pointer::link_at(
            self.model.rows(),
            &self.row_layouts,
            &self.row_bounds,
            &self.features,
            position,
        )
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
        // A reader who follows the end has seen what arrived while scrolled up.
        // `at_end()` alone is not enough: after `list_state.reset` the scrollbar max
        // is 0 until the next measure, so a detached restore looks like the end for
        // one frame. The bottom pin is the real follow-the-end bit.
        if self.at_end() && self.model.scroll().pinned_to_bottom {
            self.first_unseen = None;
        }
        let unseen = self.unseen_target().is_some();
        if !unseen {
            self.unseen_bounce.stop();
        }
        let bounce_y = self
            .unseen_bounce
            .offset_y(f32::from(crate::util::scaled_px(&app_style, 1.)));
        let on_unseen = cx.listener(|view, _: &MouseDownEvent, _, cx| {
            if view.jump_to_unseen(cx) {
                cx.notify();
            }
        });
        // Rows re-record their bounds during this frame's prepaint; anything
        // scrolled out of the list must not keep a stale position.
        self.row_bounds.borrow_mut().clear();
        let entity = cx.entity();
        let drag = self.scrollbar_drag.clone();
        let row_bounds = self.row_bounds.clone();
        let layouts = self.row_layouts.clone();
        let peers = self.peers.clone();
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
                    view.note_scroll_end(cx);
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
                        // The typing dots end the list. They get no row bounds, so pointer
                        // hits and the drag autoscroll never see them.
                        if ix >= view.model.rows().len() {
                            return match view.typing_peer.clone() {
                                Some(peer) => {
                                    let continues = typing_continues(
                                        view.model.rows(),
                                        &peer,
                                        &minute_of(chrono::Utc::now()),
                                    );
                                    div()
                                        .w_full()
                                        .child(render_typing_row(
                                            &peer,
                                            continues,
                                            view.typing_phase,
                                            &peers,
                                            &app_style,
                                        ))
                                        .into_any_element()
                                }
                                None => div().into_any_element(),
                            };
                        }
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
                                RowContext {
                                    style: &app_style,
                                    peers: &peers,
                                    layouts: layouts.clone(),
                                    callbacks: &RowCallbacks {
                                        on_nick: on_nick.clone(),
                                        self_nick: self_nick.clone(),
                                    },
                                },
                                selection,
                                hover,
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
                TranscriptView::note_scroll_end,
            ))
            .when(unseen, |this| {
                this.child(render_unseen_jump(&app_style, bounce_y, on_unseen))
            })
    }
}

/// What a release over `range` in `row` asks for. A URL wins over an invite or a
/// channel that shares its range.
fn link_click(row: &Row, range: &Range<usize>, features: &IrcServerFeatures) -> Option<LinkClick> {
    if row.body.links.contains(range) {
        return row
            .body
            .plain
            .get(range.clone())
            .map(|url| LinkClick::Url(url.to_string()));
    }
    if let Some((channel, channel_range)) = invite_hit(row) {
        if &channel_range == range {
            return Some(LinkClick::Invite(channel.to_string()));
        }
    }
    conversation_channel_hits(row, features)
        .into_iter()
        .find(|span| &span.range == range)
        .map(|span| LinkClick::Channel(span.name))
}

/// What a conversation's transcript is built from: its lines, the "New messages" mark,
/// the nicks' facts, the local nick, the network's features, and the DM peer whose typing
/// dots end the list.
pub struct ConversationLines<'a> {
    pub messages: &'a [Message],
    pub mark: Option<i64>,
    pub facts: HashMap<String, NickFacts>,
    pub self_nick: &'a str,
    pub features: &'a IrcServerFeatures,
    pub typing_peer: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    use gpui::{AppContext, TestAppContext};

    use crate::irc::event::MsgId;
    use crate::irc::world::{MessageKind, Origin};

    fn chat(author: &str, body: &str, sequence: i64) -> Message {
        Message {
            kind: MessageKind::Message,
            author: author.to_owned(),
            body: body.to_owned(),
            time: Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap(),
            server_time: None,
            msgid: MsgId::default(),
            origin: Origin::Live,
            mentioned: false,
            collapsible: false,
            sequence,
            only_visible_to_you: false,
        }
    }

    #[gpui::test]
    fn refresh_typing_peer_keeps_scroll_when_not_at_end(cx: &mut TestAppContext) {
        let cxw = cx.add_empty_window();
        let drag = ScrollbarDragCell::default();
        let transcript = cxw.update(|_, cx| {
            let focus = cx.focus_handle();
            cx.new(|cx| TranscriptView::new(focus, drag, cx))
        });
        let messages: Vec<Message> = (0..20)
            .map(|i| chat("peer", &format!("line {i}"), i as i64 + 1))
            .collect();
        let features = IrcServerFeatures::new();
        let lines = ConversationLines {
            messages: &messages,
            mark: None,
            facts: HashMap::new(),
            self_nick: "me",
            features: &features,
            typing_peer: None,
        };
        transcript.update(cx, |view, cx| {
            view.set_conversation(lines);
            view.scroll_to_top(cx);
            assert!(view.at_top(), "reader starts at the top");
            view.refresh_typing_peer(Some("peer".into()), cx);
            assert!(
                view.at_top(),
                "typing-only refresh must not jump to the end"
            );
        });
    }
}
