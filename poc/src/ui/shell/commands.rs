use gpui::{App, ClipboardItem, Context, Focusable, Window};

use crate::model::byte_budget::{SendLimit, CLASSIC_FRAME_BYTES};
use crate::model::chords::{chord_enabled, Chord, ChordContext};
use crate::model::composer::{nick_candidates, DraftKey, NickScope};
use crate::model::cursor::{move_index, page_amount};
use crate::model::directs::{close_direct, open_direct};
use crate::model::format::TextHit;
use crate::model::navigation::{
    mark_all_read, next_unread_or_status, step_conversation, step_network, UnreadTarget,
};
use crate::model::scroll_place::SavedPlace;
use crate::model::sidebar::{
    collapse_all, move_focused, set_collapsed, set_collapsed_focused, sidebar_rows,
};
use crate::model::status::status_rows;
use crate::model::{Conversation, ConversationRef, Member, Network};
use crate::ui::shell::modal::ActiveModal;
use crate::ui::shell::Shell;
use crate::ui::transcript::view::LinkClick;
use crate::util::{open_web_link, scaled_px, style};

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

    /// Makes `target` the open conversation and leaves Status, even when `target`
    /// is the one already selected. Opening reads it, so its unread count and
    /// mention flag clear.
    pub fn select(&mut self, target: ConversationRef, cx: &mut Context<Self>) {
        if let Some(conv) = self.world.find_mut(&target) {
            conv.mark_read();
        }
        if let Some(conv) = self.world.find(&target) {
            let is_channel = conv.is_channel();
            self.inbox
                .consume_conversation(&target.network_id, &target.key, is_channel);
        }
        // Re-selecting the target on screen is not a switch, so the draft stays put
        // and the transcript keeps its place.
        let switching = !self.composer_state.is_showing(&DraftKey::for_view(None, &target));
        if switching {
            self.stash_composer_draft(cx);
            // Switching abandons find mode. The draft it saved was stashed just above.
            self.end_find_mode(cx);
            self.remember_scroll_place(cx);
        }
        // Selecting a conversation clears its network's last command error, so the
        // header shows the topic again. Qt clears it on a successful command or a
        // closed DM, and this PoC adds a selection.
        self.world.set_last_error(&target.network_id, None);
        self.selected = target;
        self.status = None;
        self.member_cursor = 0;
        if switching {
            self.show_selected_conversation(cx);
            self.restore_composer_draft(cx);
        }
    }

    /// Puts the selected conversation's stored lines in the transcript, at the place
    /// the conversation was last left.
    pub fn show_selected_conversation(&mut self, cx: &mut Context<Self>) {
        let place = self.scroll_places.get(&self.selected).cloned();
        self.show_selected_at(place, cx);
    }

    /// Rebuilds the selected conversation's lines, keeping the reader's place. Used
    /// when a setting changes how the lines show (`/pref joins`). Status has none.
    pub fn rebuild_selected_keeping_place(&mut self, cx: &mut Context<Self>) {
        if self.status.is_some() {
            return;
        }
        let place = self.transcript.update(cx, |t, _| t.capture_scroll_place());
        self.show_selected_at(Some(place), cx);
    }

    /// Shows the selected conversation's lines and puts the list at `place`.
    fn show_selected_at(&mut self, place: Option<SavedPlace>, cx: &mut Context<Self>) {
        let conv = self.current_conversation();
        let self_nick = self.current_network().nick.clone();
        let chantypes = self.current_network().chantypes.clone();
        let joins = self.preferences.joins;
        self.transcript.update(cx, |t, _| {
            t.set_conversation(conv, &self_nick, &chantypes, joins);
            t.restore_scroll_place(place.as_ref());
        });
        self.sync_send_limit(cx);
    }

    /// Saves the place the transcript is left at, for the conversation on screen.
    /// Status is not saved: it always opens at the end.
    fn remember_scroll_place(&mut self, cx: &mut Context<Self>) {
        if self.status.is_some() {
            return;
        }
        let place = self.transcript.update(cx, |t, _| t.capture_scroll_place());
        self.scroll_places.remember(self.selected.clone(), place);
    }

    /// Gives the composer its send limit for what is showing: the channel or nick
    /// the draft goes to, or Status (Qt's `composerByteBudgetFor`). Called whenever
    /// the conversation or Status changes. A `/me` draft gets its own budget from
    /// the composer itself, as the text changes.
    pub fn sync_send_limit(&self, cx: &mut Context<Self>) {
        let target = self
            .status
            .is_none()
            .then(|| self.current_conversation().name.as_str());
        let limit = SendLimit::new(target, CLASSIC_FRAME_BYTES);
        self.composer.update(cx, |c, cx| c.set_send_limit(Some(limit), cx));
    }

    /// Opens a network's Status pane in place of the conversation. Like Qt's
    /// `openNetworkStatus`, it clears header focus, shows the lines at the end,
    /// and focuses the composer. An unknown network does nothing.
    pub fn open_status(&mut self, network_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.world.network(network_id).is_none() {
            return;
        }
        let status_key = DraftKey::for_view(Some(network_id), &self.selected);
        let switching = !self.composer_state.is_showing(&status_key);
        if switching {
            self.stash_composer_draft(cx);
            self.end_find_mode(cx);
            // Leaving a conversation keeps its place. Status itself opens at the end.
            self.remember_scroll_place(cx);
        }
        self.sidebar.focus = None;
        self.status = Some(network_id.to_string());
        let rows = status_rows(&self.world, network_id);
        self.transcript.update(cx, |t, _| t.set_status(rows));
        self.sync_send_limit(cx);
        if switching {
            self.restore_composer_draft(cx);
        }
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// Leaves Status for the selected conversation, which keeps its scroll place
    /// (Escape and Ctrl+`). Focus goes to the composer. Find mode ends too.
    pub fn close_status(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.status.is_none() {
            return;
        }
        self.stash_composer_draft(cx);
        self.end_find_mode(cx);
        self.status = None;
        self.show_selected_conversation(cx);
        self.restore_composer_draft(cx);
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// The draft key for what is on screen: Status for its network, or the
    /// selected conversation.
    fn draft_key(&self) -> DraftKey {
        DraftKey::for_view(self.status.as_deref(), &self.selected)
    }

    /// Saves the composer's unsent text under the target on screen. Call it
    /// before the target changes and before `sync_send_limit` cuts the field to
    /// the next target's limit, so the text is kept in full.
    fn stash_composer_draft(&mut self, cx: &App) {
        let text = self.composer.read(cx).text().to_string();
        self.composer_state.stash(&text);
    }

    /// Shows the draft of the target now on screen. Call it after
    /// `sync_send_limit`, so the draft is cut to the limit that applies to it.
    fn restore_composer_draft(&mut self, cx: &mut Context<Self>) {
        let key = self.draft_key();
        let text = self.composer_state.switch_to(key);
        self.composer.update(cx, |c, cx| {
            c.set_text(text);
            cx.notify();
        });
    }

    /// Up (`delta` -1) or Down (`delta` +1) in the composer. Returns whether the
    /// key was used. With no history to walk it is not, and the key falls
    /// through to the text field.
    pub fn recall_history(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // While the slash popup shows, Up and Down move its highlight instead.
        if self.move_slash_selection(delta, window, cx) {
            return true;
        }
        if !self.chord_live(Chord::ComposerHistory, window, cx) {
            return false;
        }
        let text = self.composer.read(cx).text().to_string();
        let Some(next) = self.composer_state.recall(&text, delta) else {
            return false;
        };
        self.composer.update(cx, |c, cx| {
            c.set_text(next);
            cx.notify();
        });
        true
    }

    /// Tab (`backward` false) or Shift+Tab (`backward` true) in the composer.
    /// The key is always consumed here, even with nothing to complete, so focus
    /// never leaves the composer.
    pub fn complete_nick(&mut self, backward: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Tab inserts the highlighted popup row. Shift+Tab does not, and falls through.
        if !backward && self.complete_slash_verb(window, cx) {
            return;
        }
        if !self.chord_live(Chord::NickComplete, window, cx) {
            return;
        }
        let names = self.completion_candidates();
        let (text, cursor, serial) = {
            let field = self.composer.read(cx);
            (field.text().to_string(), field.cursor(), field.input_serial())
        };
        if let Some(edit) = self
            .composer_state
            .complete_nick(&text, cursor, serial, &names, backward)
        {
            self.composer.update(cx, |c, cx| {
                c.set_text_with_cursor(edit.text, edit.cursor);
                cx.notify();
            });
        }
    }

    /// The nicks Tab can complete on the open view: the members or the peer,
    /// then the channel names on the selected network.
    fn completion_candidates(&self) -> Vec<String> {
        if self.status.is_some() {
            return nick_candidates(NickScope::Status);
        }
        let rows = sidebar_rows(&self.world, &self.sidebar);
        let channels: Vec<&str> = rows
            .iter()
            .filter(|row| row.channel && row.conversation.network_id == self.selected.network_id)
            .map(|row| row.name.as_str())
            .collect();
        let conv = self.current_conversation();
        if conv.is_channel() {
            let members: Vec<&str> = conv.members.iter().map(|m| m.nick.as_str()).collect();
            nick_candidates(NickScope::Channel {
                members: &members,
                channels: &channels,
            })
        } else {
            nick_candidates(NickScope::Direct {
                peer: &conv.name,
                channels: &channels,
            })
        }
    }

    /// Ctrl+`: opens Status for the selected network, or leaves it when Status
    /// is showing and a conversation is selected.
    pub fn toggle_status(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::StatusToggle, window, cx) {
            return;
        }
        if self.status.is_some() && self.world.find(&self.selected).is_some() {
            self.close_status(window, cx);
        } else {
            let network_id = self.selected.network_id.clone();
            self.open_status(&network_id, window, cx);
        }
    }

    /// A click on an INVITE line's channel in Status: joins it on that network
    /// and selects it (Qt `joinInviteChannel`).
    pub fn join_status_invite(&mut self, channel: &str, cx: &mut Context<Self>) {
        let Some(network_id) = self.status.clone() else {
            return;
        };
        self.join_invite(&network_id, channel, cx);
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
            status_open: self.status.is_some(),
            channel: conv.is_channel(),
            direct: !conv.is_channel(),
            header_focused: self.sidebar.focus.is_some(),
            members_focused: self.member_focus.is_focused(window),
            transcript_selection: self.transcript.read(cx).has_selection(),
            slash_open: self.slash_popup_showing(),
            find_active: self.find.is_some(),
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

    /// Enter in the composer. With a network header focused, Enter opens that
    /// network's Status (Qt `activateFocusedNetworkHeader`) and does not send.
    pub fn composer_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // In find mode Enter moves to the next match. It never sends.
        if self.find.is_some() {
            self.begin_or_advance_find(window, cx);
            return;
        }
        if self.chord_live(Chord::HeaderActivate, window, cx) {
            if let Some(network_id) = self.sidebar.focus.clone() {
                self.open_status(&network_id, window, cx);
            }
            return;
        }
        // Enter inserts a partial verb from the popup. An exact name, or no popup, sends.
        if self.enter_slash_verb(window, cx) {
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
    /// so the landing row is visible. With nothing unread it opens Status: the
    /// network whose Status is showing, else the selected network (Qt's
    /// `focusedNetworkId`, which is the open console or the selection).
    pub fn select_next_unread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::NextUnread, window, cx) {
            return;
        }
        let rows = sidebar_rows(&self.world, &self.sidebar);
        let status_network = self
            .status
            .clone()
            .unwrap_or_else(|| self.selected.network_id.clone());
        let landing = next_unread_or_status(
            &self.world,
            &rows,
            Some(&self.selected),
            Some(&status_network),
        );
        match landing {
            Some(UnreadTarget::Conversation(target)) => {
                set_collapsed(&self.world, &mut self.sidebar, &target.network_id, false);
                self.select(target, cx);
                cx.notify();
            }
            Some(UnreadTarget::Status(network_id)) => self.open_status(&network_id, window, cx),
            None => {}
        }
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

    /// Ends a text drag on mouse-up. A release over a link acts on it. Notifies
    /// only when a drag was running.
    pub fn end_text_drag(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.transcript.read(cx).is_dragging() {
            return;
        }
        let click = self.transcript.update(cx, |t, _| t.finish_drag(position));
        cx.notify();
        match click {
            Some(LinkClick::Url(url)) => {
                open_web_link(cx, &url);
            }
            Some(LinkClick::Invite(channel)) => self.join_status_invite(&channel, cx),
            Some(LinkClick::Channel(name)) => self.open_channel_name(&name, window, cx),
            None => {}
        }
    }

    /// A click on a URL or channel name in the header's topic.
    pub fn activate_text_hit(
        &mut self,
        hit: TextHit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match hit {
            TextHit::Url(url) => {
                open_web_link(cx, &url);
            }
            TextHit::Channel(name) => self.open_channel_name(&name, window, cx),
        }
    }
}
