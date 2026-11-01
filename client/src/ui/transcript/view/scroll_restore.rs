//! Pending scroll-place restore: wait for history, capture, and resume.

use gpui::{px, Context, ListOffset};

use super::{ConversationLines, TranscriptScrolled, TranscriptView};
use crate::model::scroll_place::{has_line, resume, LivePlace, Resume, SavedPlace};
use crate::model::unread;

impl TranscriptView {
    /// Follow-end for the controller. False while a restore is pending so the
    /// server read marker does not jump past unread backlog on the interim tail.
    /// `at_end` alone is not enough: after `list_state.reset` it reads true until the
    /// next layout measures the list, so a restore just applied would report the end.
    /// The bottom pin is off after a restore lands, so it is checked too.
    pub fn reports_follow_end(&self) -> bool {
        self.pending_restore.is_none() && self.at_end() && self.model.scroll().pinned_to_bottom
    }

    /// Whether the reader is at the end. Until a layout measures the rows after a reset,
    /// `at_end` reads true whatever the offset, so the bottom pin is the reader's bit.
    fn reader_at_end(&self) -> bool {
        if self.list_measured {
            self.at_end()
        } else {
            self.model.scroll().pinned_to_bottom
        }
    }

    /// Whether the reader follows the end, by its bottom pin. The pin is set by the reader's
    /// own moves and by landing at the end. The list clamps a restored line that sits within a
    /// screen of the loaded end, so `at_end` reads true there while the reader has not moved.
    fn reader_following(&self) -> bool {
        self.model.scroll().pinned_to_bottom
    }

    pub fn has_pending_restore(&self) -> bool {
        self.pending_restore.is_some()
    }

    /// Drops a pending restore so the next capture saves what is on screen.
    pub fn clear_pending_restore(&mut self) {
        self.pending_restore = None;
    }

    /// Gives up a pending restore whose history settled without its line, or whose request
    /// never settled. The list lands at the end, as a conversation with no saved place does.
    pub fn give_up_pending_restore(&mut self, cx: &mut Context<Self>) {
        if self.pending_restore.is_none() {
            return;
        }
        self.restore_scroll_place(None, false, false);
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
    /// the list so they match where the reader now is.
    pub fn note_transcript_scroll(&mut self, cx: &mut Context<Self>) {
        self.pending_restore = None;
        self.sync_scroll_from_list();
        self.note_scroll_end(cx);
    }

    /// Reloads the open conversation. A pending restore whose line arrived is applied.
    pub fn refresh_conversation(&mut self, lines: ConversationLines, cx: &mut Context<Self>) {
        let previous_rows = self.model.rows().len();
        let was_at_end = self.reader_following();
        let pending = self.pending_restore.clone();
        let live = (pending.is_none() && !was_at_end)
            .then(|| self.capture_live_place())
            .flatten();
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
        if let Some(place) = pending {
            if has_line(&place, self.model.rows()) {
                self.restore_scroll_place(Some(&place), false, false);
            } else {
                if was_at_end {
                    self.model.set_pinned_to_bottom(true);
                    self.list_state
                        .scroll_to_reveal_item(self.list_len().saturating_sub(1));
                }
                self.reported_at_end = false;
            }
        } else if was_at_end {
            self.model.set_pinned_to_bottom(true);
            self.list_state
                .scroll_to_reveal_item(self.list_len().saturating_sub(1));
            self.note_scroll_end(cx);
        } else if let Some(place) = live {
            self.restore_live_place(&place);
        }
        cx.notify();
    }

    /// Updates only the typing footer for the open direct message.
    pub fn refresh_typing_peer(&mut self, peer: Option<String>, cx: &mut Context<Self>) {
        if self.typing_peer == peer {
            return;
        }
        let was_at_end = self.reader_following();
        let pending = self.pending_restore.is_some();
        let live = (!pending && !was_at_end)
            .then(|| self.capture_live_place())
            .flatten();
        self.typing_peer = peer;
        self.reset_list();
        if pending {
            if was_at_end {
                self.model.set_pinned_to_bottom(true);
                self.list_state
                    .scroll_to_reveal_item(self.list_len().saturating_sub(1));
            }
            self.reported_at_end = false;
        } else if was_at_end {
            self.model.set_pinned_to_bottom(true);
            self.list_state
                .scroll_to_reveal_item(self.list_len().saturating_sub(1));
            self.note_scroll_end(cx);
        } else if let Some(place) = live {
            self.restore_live_place(&place);
        }
        cx.notify();
    }

    fn sync_scroll_from_list(&mut self) {
        let rows = self.model.rows().len();
        let top = self.list_state.logical_scroll_top();
        if self.reader_at_end() || top.item_ix >= rows {
            self.model.set_pinned_to_bottom(true);
        } else {
            let offset_px = f32::from(top.offset_in_item).round() as i32;
            self.model.set_scroll_anchor(top.item_ix, offset_px);
        }
    }

    /// Skips unread/day dividers for persistence. Returns a pending place while waiting.
    pub fn capture_scroll_place(&self) -> SavedPlace {
        if let Some(place) = self.pending_restore.clone() {
            return place;
        }
        SavedPlace::from_anchor(self.model.scroll(), self.model.rows())
    }

    fn capture_live_place(&self) -> Option<LivePlace> {
        LivePlace::from_anchor(self.model.scroll(), self.model.rows())
    }

    fn restore_live_place(&mut self, place: &LivePlace) {
        match place.resume(self.model.rows()) {
            Resume::Line { row, offset_px } => {
                self.model.set_scroll_anchor(row, offset_px);
                self.list_state.scroll_to(ListOffset {
                    item_ix: row,
                    offset_in_item: px(offset_px as f32),
                });
            }
            Resume::End => self.model.set_pinned_to_bottom(true),
        }
    }

    /// Lands on `place`. With `wait_for_history`, a line that is not shown yet keeps the
    /// place pending, and the list stays at the end until the line arrives or its
    /// history settles without it.
    pub fn restore_scroll_place(
        &mut self,
        place: Option<&SavedPlace>,
        open_at_unread: bool,
        wait_for_history: bool,
    ) {
        self.pending_restore = None;
        if let Some(line @ SavedPlace::Line { .. }) = place {
            if wait_for_history && !has_line(line, self.model.rows()) {
                self.pending_restore = Some(line.clone());
                self.model.set_pinned_to_bottom(true);
                self.reported_at_end = false;
                return;
            }
        }
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
}
