//! Pending scroll-place restore: wait for history, capture, and resume.

use chrono::{DateTime, TimeDelta, Utc};
use gpui::{px, Context, ListOffset, Pixels, Size};

use super::{ConversationLines, TranscriptScrolled, TranscriptView};
use crate::model::scroll_place::{has_line, resume, Resume, SavedPlace};
use crate::model::unread;

/// A saved line the transcript waits for. A give-up lands as a conversation with no saved
/// place does, so it keeps the unread preference the wait started with.
pub(super) struct PendingRestore {
    place: SavedPlace,
    open_at_unread: bool,
}

/// How long the width must stay still after a change before the burst is over. A drag changes the
/// width every frame, and its repeated changes wait for this before the rows are measured again.
pub(super) const WIDTH_QUIET: TimeDelta = TimeDelta::milliseconds(150);

/// Where the window's width is within a burst of changes. The first change of a burst measures
/// its rows in the frame that sees it. Whether the rows are measured now is `list_measured`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WidthState {
    /// No width change is waiting to settle.
    Settled,
    /// The width changed in a frame since the last tick.
    Moving,
    /// The last tick found a change and none has come since. It settles once `WIDTH_QUIET`
    /// has passed since `since`.
    Quiet { since: DateTime<Utc> },
}

impl TranscriptView {
    /// Follow-end for the controller. False while a restore is pending so the
    /// server read marker does not jump past unread backlog on the interim tail.
    pub fn reports_follow_end(&self) -> bool {
        self.pending_restore.is_none() && self.model.scroll().pinned_to_bottom
    }

    /// The list's frame is about to lay out at `size`. A width change makes gpui drop every row
    /// height in this frame, and rows it has not measured count as 0 px. The first change of a
    /// burst measures every row in this frame, so a lone change (the member panel, a channel
    /// switch) leaves the list measured at once. A change that repeats one within the quiet
    /// period (a drag) only drops the heights: a full layout for every frame of a drag is too
    /// costly, so that waits for the width to settle (`settle_width`). Until then the list shows
    /// the reader's place, and nothing reads its pixels.
    pub(super) fn list_sized(&mut self, size: Size<Pixels>) {
        let last = self.list_state.viewport_bounds().size;
        if last == size {
            return;
        }
        // Before the first layout there are no heights to drop.
        if last.width != size.width && last.width > px(0.) {
            match self.width {
                WidthState::Settled => self.reset_list(),
                WidthState::Moving | WidthState::Quiet { .. } => self.list_measured = false,
            }
            self.width = WidthState::Moving;
        }
        self.project_place();
    }

    /// The list's frame has laid out. A layout after a reset measures every row, which makes the
    /// pixels true (`list_measured`). A landing waits for such a layout, so it is the one that
    /// decides whether the landing clamped to the end. A clamped landing shows the end, and its
    /// reader follows it from here. The frame has already been built from the at-end bit, so
    /// one that changed asks for another.
    pub(super) fn list_laid_out(&mut self, cx: &mut Context<Self>) {
        let was_at_end = self.at_end();
        if std::mem::take(&mut self.measure_pending) {
            self.list_measured = true;
        }
        if self.list_measured && std::mem::take(&mut self.landing) && self.at_end() {
            self.model.set_pinned_to_bottom(true);
            self.note_scroll_end(cx);
        }
        if self.at_end() != was_at_end {
            let view = cx.weak_entity();
            cx.defer(move |cx| {
                view.update(cx, |_, cx| cx.notify()).ok();
            });
        }
    }

    /// The shell's tick. The first tick that finds the width changed starts the quiet period.
    /// Once the width has been still for `WIDTH_QUIET`, the rows are measured again, in the
    /// next frame, unless a layout since the last change already measured them. The place is
    /// projected either way.
    pub fn settle_width(&mut self, now: DateTime<Utc>, cx: &mut Context<Self>) {
        match self.width {
            WidthState::Settled => {}
            WidthState::Moving => self.width = WidthState::Quiet { since: now },
            WidthState::Quiet { since } if now - since >= WIDTH_QUIET => {
                self.width = WidthState::Settled;
                if !self.list_measured {
                    self.reset_list();
                }
                self.project_place();
                cx.notify();
            }
            WidthState::Quiet { .. } => {}
        }
    }

    /// Moves the list to the reader's place in the model: the end, or the anchor.
    pub(super) fn project_place(&mut self) {
        let scroll = self.model.scroll();
        let top = if scroll.pinned_to_bottom {
            ListOffset {
                item_ix: self.list_len(),
                offset_in_item: px(0.),
            }
        } else {
            ListOffset {
                item_ix: scroll.row,
                offset_in_item: px(scroll.offset_px as f32),
            }
        };
        self.list_state.scroll_to(top);
    }

    pub fn has_pending_restore(&self) -> bool {
        self.pending_restore.is_some()
    }

    /// Drops a pending restore so the next capture saves what is on screen.
    pub fn clear_pending_restore(&mut self) {
        self.pending_restore = None;
    }

    /// Gives up a pending restore whose history settled without its line, or whose request
    /// never settled. The list lands where a conversation with no saved place lands: on its
    /// unread mark when the wait's preference is on and a mark exists, else at the end.
    pub fn give_up_pending_restore(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_restore.take() else {
            return;
        };
        self.restore_scroll_place(None, pending.open_at_unread, false);
        cx.notify();
    }

    /// Emits `TranscriptScrolled` when follow-end changes. Does not retire a
    /// pending restore (sidebar/member Div scrollbar drags share the shell path).
    pub fn note_scroll_end(&mut self, cx: &mut Context<Self>) {
        let at_end = self.reports_follow_end();
        if at_end != self.reported_at_end {
            self.reported_at_end = at_end;
            cx.emit(TranscriptScrolled { at_end });
        }
    }

    /// A move of this list (wheel, keys, find, link, transcript scrollbar). The move
    /// retires a pending restore, and the anchor and bottom pin are read back from
    /// the list so they match where the reader now is. While the rows are not measured the
    /// list moved by heights that are not true, so the place is projected back instead.
    pub fn note_transcript_scroll(&mut self, cx: &mut Context<Self>) {
        self.pending_restore = None;
        if self.list_measured {
            self.sync_scroll_from_list();
        } else {
            self.project_place();
        }
        self.note_scroll_end(cx);
    }

    /// Reloads the open conversation. A pending restore whose line arrived is applied.
    pub fn refresh_conversation(&mut self, lines: ConversationLines, cx: &mut Context<Self>) {
        let previous_rows = self.model.rows().len();
        let was_at_end = self.model.scroll().pinned_to_bottom;
        let place = self.capture_scroll_place();
        self.peers.facts = lines.facts;
        self.self_nick = lines.self_nick.to_owned();
        self.features = lines.features.clone();
        self.typing_peer = lines.typing_peer;
        self.model
            .replace_conversation(lines.messages, lines.mark, lines.today);
        let new_rows = self.model.rows().len();
        unread::note_row_growth(previous_rows, new_rows, was_at_end, &mut self.first_unseen);
        if unread::detached_arrival(previous_rows, new_rows, was_at_end) {
            self.unseen_bounce.restart();
        }
        self.reset_list();
        self.row_layouts.borrow_mut().clear();
        self.reland(&place, cx);
        cx.notify();
    }

    /// Updates only the typing footer for the open direct message.
    pub fn refresh_typing_peer(&mut self, peer: Option<String>, cx: &mut Context<Self>) {
        if self.typing_peer == peer {
            return;
        }
        let place = self.capture_scroll_place();
        self.typing_peer = peer;
        self.reset_list();
        self.reland(&place, cx);
        cx.notify();
    }

    /// Puts the reader back on `place` after the list was reset. A wait stays at the end
    /// until its line is there.
    fn reland(&mut self, place: &SavedPlace, cx: &mut Context<Self>) {
        if self.pending_restore.is_none() {
            self.land(resume(Some(place), self.model.rows()));
            self.note_scroll_end(cx);
        } else if has_line(place, self.model.rows()) {
            self.restore_scroll_place(Some(place), false, false);
        } else {
            self.reported_at_end = false;
        }
    }

    fn sync_scroll_from_list(&mut self) {
        let top = self.list_state.logical_scroll_top();
        if self.at_end() {
            self.model.set_pinned_to_bottom(true);
        } else {
            let offset_px = f32::from(top.offset_in_item).round() as i32;
            self.model.set_scroll_anchor(top.item_ix, offset_px);
        }
    }

    /// The reader's place: the end, or a line and not a divider. Returns a pending place
    /// while waiting.
    pub fn capture_scroll_place(&self) -> SavedPlace {
        if let Some(pending) = &self.pending_restore {
            return pending.place.clone();
        }
        SavedPlace::from_anchor(self.model.scroll(), self.model.rows())
    }

    /// Sets the reader's place and moves the list to it.
    pub(super) fn land(&mut self, place: Resume) {
        match place {
            Resume::End => self.model.set_pinned_to_bottom(true),
            Resume::Line { row, offset_px } => self.model.set_scroll_anchor(row, offset_px),
        }
        self.project_place();
    }

    /// Lands on `place`. With `wait_for_history`, a line that is not shown yet keeps the
    /// place pending, and the list stays at the end until the line arrives or its
    /// history settles without it. `open_at_unread` is the unread preference: a place-less
    /// landing opens on the mark when it is on, and a pending wait keeps it for a give-up.
    pub fn restore_scroll_place(
        &mut self,
        place: Option<&SavedPlace>,
        open_at_unread: bool,
        wait_for_history: bool,
    ) {
        self.pending_restore = None;
        self.landing = false;
        if let Some(line @ SavedPlace::Line { .. }) = place {
            if wait_for_history && !has_line(line, self.model.rows()) {
                self.pending_restore = Some(PendingRestore {
                    place: line.clone(),
                    open_at_unread,
                });
                self.model.set_pinned_to_bottom(true);
                self.reported_at_end = false;
                return;
            }
        }
        let rows = self.model.rows();
        let mark = unread::opening_row(rows, open_at_unread);
        match (resume(place, rows), mark) {
            (Resume::End, Some(row)) => self.reveal_row(row),
            (resumed, _) => self.land(resumed),
        }
    }
}
