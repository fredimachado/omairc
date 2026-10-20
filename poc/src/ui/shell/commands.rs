use std::time::Instant;

use gpui::{App, ClipboardItem, Context, Focusable, Window};

use crate::model::chords::{chord_enabled, Chord, ChordContext};
use crate::model::cursor::{move_index, page_amount};
use crate::model::directs::{close_direct, open_direct};
use crate::model::navigation::{mark_all_read, next_unread, step_conversation, step_network};
use crate::model::sidebar::{
    collapse_all, move_focused, set_collapsed, set_collapsed_focused, sidebar_rows,
};
use crate::model::{
    continues_group, local_send_stamp, outgoing_message, row_from_mock_message, Conversation,
    ConversationRef, Member, Network,
};
use crate::ui::shell::modal::ActiveModal;
use crate::ui::shell::Shell;
use crate::util::{scaled_px, style};

/// One member row's height in logical pixels (`MembersColumn.qml`).
const MEMBER_ROW_HEIGHT: f32 = 43.;

/// Shell behaviour: selection, sending, focus. Builds no elements.
impl Shell {
    pub fn current_network(&self) -> &Network {
        self.world
            .network(&self.selected.network_id)
            .expect("selected network is in the world")
    }

    pub fn current_conversation(&self) -> &Conversation {
        self.world
            .find(&self.selected)
            .expect("selected conversation is in the world")
    }

    /// Makes `target` the open conversation. Opening reads it, so its unread
    /// count and mention flag clear.
    pub fn select(&mut self, target: ConversationRef, cx: &mut Context<Self>) {
        if let Some(conv) = self.world.find_mut(&target) {
            conv.mark_read();
        }
        if let Some(conv) = self.world.find(&target) {
            let is_channel = conv.is_channel();
            self.inbox
                .consume_conversation(&target.network_id, &target.key, is_channel);
        }
        self.selected = target;
        self.member_cursor = 0;
        let conv = self.current_conversation();
        let self_nick = self.current_network().nick.clone();
        self.transcript
            .update(cx, |t, _| t.set_conversation(conv, &self_nick));
    }

    /// A nick click (transcript, member list): open or select that DM on the
    /// selected conversation's network. Self and channel-looking nicks do nothing.
    pub fn open_direct_nick(&mut self, nick: &str, cx: &mut Context<Self>) {
        let network_id = self.selected.network_id.clone();
        if let Some(target) = open_direct(&mut self.world, &network_id, nick) {
            self.select(target, cx);
        }
    }

    /// Ctrl+W on a DM, or a middle click on its row. Closing the selected DM
    /// selects the neighbour (or the first row when none is left). Channels are a no-op.
    pub fn close_direct_message(
        &mut self,
        target: &ConversationRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_selected = *target == self.selected;
        let Some(next) = close_direct(&mut self.world, &self.sidebar, target) else {
            return;
        };
        if was_selected {
            let fallback = sidebar_rows(&self.world, &self.sidebar)
                .first()
                .map(|row| row.conversation.clone());
            if let Some(row) = next.or(fallback) {
                self.select(row, cx);
            }
            self.focus_composer(window, cx);
        }
        cx.notify();
    }

    pub fn select_by_ids(&mut self, network_id: &str, key: &str, cx: &mut Context<Self>) {
        let target = ConversationRef::new(network_id, key);
        if self.world.find(&target).is_some() {
            self.select(target, cx);
        }
    }

    /// The network header chevron. Collapsing a network never changes the
    /// selection, so a hidden selected row stays selected, as in Qt.
    pub fn toggle_collapsed(&mut self, network_id: &str, cx: &mut Context<Self>) {
        let collapsed = !self.sidebar.is_collapsed(network_id);
        if set_collapsed(&self.world, &mut self.sidebar, network_id, collapsed) {
            cx.notify();
        }
    }

    /// The chord context from the shell's state and the live keyboard focus.
    pub fn chord_context(&self, window: &Window, cx: &App) -> ChordContext {
        let conv = self.current_conversation();
        ChordContext {
            sheet: self.modal.as_ref().map(ActiveModal::sheet),
            channel: conv.is_channel(),
            direct: !conv.is_channel(),
            header_focused: self.sidebar.focus.is_some(),
            members_focused: self.member_focus.is_focused(window),
            composer_focused: self.composer.read(cx).focus_handle(cx).is_focused(window),
            transcript_selection: self.transcript.read(cx).has_selection(),
        }
    }

    /// Whether `chord` is live now. Every shell action checks this first.
    pub fn chord_live(&self, chord: Chord, window: &Window, cx: &App) -> bool {
        chord_enabled(chord, &self.chord_context(window, cx))
    }

    /// Ctrl+Shift+S: collapse or restore the server list by width. Drops
    /// header focus, as Qt does.
    pub fn toggle_rail(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::ServerListToggle, window, cx) {
            return;
        }
        self.sidebar.rail_visible = !self.sidebar.rail_visible;
        self.sidebar.focus = None;
        cx.notify();
    }

    /// Ctrl+W on the selected DM. Channels are a no-op.
    pub fn close_selected_direct(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::CloseDirect, window, cx) {
            return;
        }
        let target = self.selected.clone();
        self.close_direct_message(&target, window, cx);
    }

    /// Ctrl+C with no composer selection: the member list copies the current
    /// row's nick; otherwise the transcript selection is copied.
    pub fn copy_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Copy, window, cx) {
            return;
        }
        let text = if self.member_focus.is_focused(window) {
            self.current_member().map(|m| m.nick.clone()).unwrap_or_default()
        } else {
            self.transcript.read(cx).copy_text()
        };
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Enter in the composer. A focused network header makes it a no-op, as
    /// Qt does; the header's own Enter (Status in Qt) is out of scope.
    pub fn composer_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.chord_live(Chord::HeaderActivate, window, cx) {
            return;
        }
        self.send_message(cx);
        self.focus_composer(window, cx);
    }

    /// Ctrl+Shift+M: show or hide the member panel (channels only). Hiding it
    /// while the list has focus hands focus to the composer.
    pub fn toggle_members(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::MembersToggle, window, cx) {
            return;
        }
        if self.members_open && self.member_focus.is_focused(window) {
            self.focus_composer(window, cx);
        }
        self.members_open = !self.members_open;
        cx.notify();
    }

    /// Ctrl+Shift+P: reopen the member panel and focus its list.
    pub fn focus_members(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::MembersFocus, window, cx) {
            return;
        }
        self.members_open = true;
        window.focus(&self.member_focus);
        cx.notify();
    }

    /// Ctrl+L: clears header focus and focuses the composer.
    pub fn clear_header_and_focus_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::ComposerFocus, window, cx) {
            return;
        }
        self.sidebar.focus = None;
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// Ctrl+Q.
    pub fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.chord_live(Chord::Quit, window, cx) {
            cx.quit();
        }
    }

    /// Alt+Down / Alt+Up: the next visible conversation across networks.
    pub fn walk_conversation(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let chord = if delta > 0 {
            Chord::WalkConversationDown
        } else {
            Chord::WalkConversationUp
        };
        if !self.chord_live(chord, window, cx) {
            return;
        }
        let rows = sidebar_rows(&self.world, &self.sidebar);
        if let Some(target) = step_conversation(&rows, Some(&self.selected), delta) {
            self.select(target, cx);
            cx.notify();
        }
    }

    /// Alt+Left / Alt+Right: moves header focus to the neighbouring network
    /// (wrapping) and restores a collapsed server list. The selection stays.
    pub fn walk_header(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let chord = if delta > 0 {
            Chord::WalkNetworkRight
        } else {
            Chord::WalkNetworkLeft
        };
        if !self.chord_live(chord, window, cx) {
            return;
        }
        let current = self.selected.network_id.clone();
        if let Some(id) = step_network(&self.world, self.sidebar.focus.as_deref(), Some(&current), delta) {
            self.sidebar.focus = Some(id);
            self.sidebar.rail_visible = true;
            cx.notify();
        }
    }

    /// Alt+Shift+Left / Right: collapse or expand the focused header.
    pub fn set_focused_header_collapsed(
        &mut self,
        collapsed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chord = if collapsed {
            Chord::CollapseHeader
        } else {
            Chord::ExpandHeader
        };
        if !self.chord_live(chord, window, cx) {
            return;
        }
        if set_collapsed_focused(&self.world, &mut self.sidebar, collapsed) {
            cx.notify();
        }
    }

    /// Ctrl+Alt+Shift+Left / Right: collapse or expand every network. Focus stays.
    pub fn set_all_collapsed(&mut self, collapsed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let chord = if collapsed {
            Chord::CollapseAll
        } else {
            Chord::ExpandAll
        };
        if !self.chord_live(chord, window, cx) {
            return;
        }
        if collapse_all(&self.world, &mut self.sidebar, collapsed) {
            cx.notify();
        }
    }

    /// Alt+Shift+Up / Down: moves the focused network. Focus follows it.
    pub fn move_focused_header(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let chord = if delta > 0 {
            Chord::MoveNetworkDown
        } else {
            Chord::MoveNetworkUp
        };
        if !self.chord_live(chord, window, cx) {
            return;
        }
        if move_focused(&mut self.world, &self.sidebar, delta) {
            cx.notify();
        }
    }

    /// Alt+A: the next mention or unread conversation. Its network is expanded
    /// so the landing row is visible.
    pub fn select_next_unread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::NextUnread, window, cx) {
            return;
        }
        let rows = sidebar_rows(&self.world, &self.sidebar);
        let Some(target) = next_unread(&self.world, &rows, Some(&self.selected)) else {
            return;
        };
        set_collapsed(&self.world, &mut self.sidebar, &target.network_id, false);
        self.select(target, cx);
        cx.notify();
    }

    /// Alt+Shift+A: every conversation reads. The selection does not move.
    pub fn read_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::MarkAllRead, window, cx) {
            return;
        }
        mark_all_read(&mut self.world);
        cx.notify();
    }

    /// PgUp / PgDn and Shift variants: scrolls the transcript one page or half.
    pub fn transcript_scroll(
        &mut self,
        chord: Chord,
        down: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.chord_live(chord, window, cx) {
            return;
        }
        let half = matches!(chord, Chord::HalfPageUp | Chord::HalfPageDown);
        self.transcript.update(cx, |t, cx| {
            t.scroll_page(down, half);
            cx.notify();
        });
    }

    /// Ctrl+Home (top) and Ctrl+End (bottom) of the transcript.
    pub fn transcript_jump(&mut self, bottom: bool, window: &mut Window, cx: &mut Context<Self>) {
        let chord = if bottom {
            Chord::TranscriptBottom
        } else {
            Chord::TranscriptTop
        };
        if !self.chord_live(chord, window, cx) {
            return;
        }
        self.transcript.update(cx, |t, cx| {
            if bottom {
                t.scroll_to_bottom();
            } else {
                t.scroll_to_top();
            }
            cx.notify();
        });
    }

    /// The member under the list cursor, if the channel has members.
    pub fn current_member(&self) -> Option<&Member> {
        self.current_conversation().members.get(self.member_cursor)
    }

    /// Up / Down in the member list.
    pub fn member_move(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::MembersStep, window, cx) {
            return;
        }
        self.move_member_cursor(delta);
        cx.notify();
    }

    /// PgUp / PgDn and Shift variants in the member list. A page is
    /// `round(visible rows × 0.8)`, a half page `× 0.35`, at least one row.
    pub fn member_page(&mut self, down: bool, half: bool, window: &mut Window, cx: &mut Context<Self>) {
        let chord = if half {
            Chord::MembersHalfPage
        } else {
            Chord::MembersPage
        };
        if !self.chord_live(chord, window, cx) {
            return;
        }
        let row = f32::from(scaled_px(&style(cx), MEMBER_ROW_HEIGHT));
        let visible = (f32::from(self.members_scroll.bounds().size.height) / row).floor();
        let step = page_amount(visible, half).max(1.) as isize;
        self.move_member_cursor(if down { step } else { -step });
        cx.notify();
    }

    /// Home / End in the member list.
    pub fn member_jump(&mut self, bottom: bool, window: &mut Window, cx: &mut Context<Self>) {
        let chord = if bottom {
            Chord::MembersBottom
        } else {
            Chord::MembersTop
        };
        if !self.chord_live(chord, window, cx) {
            return;
        }
        let len = self.current_conversation().members.len();
        self.member_cursor = if bottom { len.saturating_sub(1) } else { 0 };
        self.members_scroll.scroll_to_item(self.member_cursor);
        cx.notify();
    }

    /// Enter in the member list: opens or selects the DM with the current
    /// nick and focuses the composer. Self is a no-op, as in the nick sheet.
    pub fn member_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::MembersActivate, window, cx) {
            return;
        }
        let Some(nick) = self.current_member().map(|m| m.nick.clone()) else {
            return;
        };
        if nick == self.current_network().nick {
            return;
        }
        self.open_direct_nick(&nick, cx);
        self.focus_composer(window, cx);
    }

    fn move_member_cursor(&mut self, delta: isize) {
        let len = self.current_conversation().members.len();
        self.member_cursor = move_index(len, self.member_cursor, delta);
        self.members_scroll.scroll_to_item(self.member_cursor);
    }

    pub fn focus_composer(&self, window: &mut Window, cx: &App) {
        window.focus(&self.composer.read(cx).focus_handle(cx));
    }

    pub fn send_message(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let append_start = self.bench.enabled.then(Instant::now);
        let author = self.current_network().nick.clone();
        let (day, time) = local_send_stamp();
        let message = outgoing_message(&text, &author, &time, &day);
        let (continues, id) = {
            let rows = self.transcript.read(cx).rows();
            (
                continues_group(rows.last(), &author, &time),
                rows.len() as u64 + 1,
            )
        };
        let row = row_from_mock_message(id, &message, continues);
        // Stored with the conversation, so switching away and back keeps it.
        let selected = self.selected.clone();
        self.world.record_outgoing(&selected, message);
        self.transcript.update(cx, |t, _| t.append_outgoing(row));
        self.composer.update(cx, |c, cx| {
            c.clear();
            cx.notify();
        });
        if let Some(start) = append_start {
            self.bench.append_latency = Some(start.elapsed());
        }
        cx.notify();
    }

    /// Ends a text drag on mouse-up; notifies only when a drag was running.
    pub fn end_text_drag(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        self.transcript.update(cx, |t, cx| {
            if t.finish_drag(position, cx) {
                cx.notify();
            }
        });
    }
}
