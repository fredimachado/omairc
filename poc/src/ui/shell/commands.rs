use gpui::{App, ClipboardItem, Context, Focusable, Window};

use crate::model::byte_budget::SendLimit;
use crate::model::chords::{chord_enabled, Chord, ChordContext};
use crate::model::composer::{nick_candidates, NickScope};
use crate::model::cursor::{move_index, page_amount};
use crate::model::format::TextHit;
use crate::model::navigation::{next_unread_or_status, step_conversation, step_network, UnreadTarget};
use crate::model::ConversationRef;
use crate::irc::reducer::MemberView;
use crate::ui::shell::modal::ActiveModal;
use crate::ui::shell::Shell;
use crate::ui::transcript::view::LinkClick;
use crate::util::{open_web_link, scaled_px, style};

/// One member row's height in logical pixels (`MembersColumn.qml`).
const MEMBER_ROW_HEIGHT: f32 = 43.;

/// Shell behaviour: selection, sending, focus. Builds no elements. Every state change
/// goes through a controller intent; the shell only asks and then follows (`sync_view`).
impl Shell {
    /// Makes `target` the open conversation and leaves Status, even when `target` is
    /// the one already selected. Selecting it reads it and clears its network's last
    /// command error, which the controller does.
    pub fn select(&mut self, target: ConversationRef, cx: &mut Context<Self>) {
        self.controller.select_conversation(&target.network_id, &target.key);
        self.sync_view(cx);
    }

    /// Re-applies the composer's send limit for the view on screen: the channel or nick
    /// the draft goes to, or Status (Qt's `composerByteBudgetFor`). Called whenever the
    /// view changes. A `/me` draft gets its own budget from the composer itself, as the
    /// text changes.
    pub fn sync_send_limit(&self, cx: &mut Context<Self>) {
        let network_id = self.shown.conversation.network_id.clone();
        let features = self.features_of(&network_id);
        let target = self.shown.status.is_none().then(|| self.controller.selected_target());
        let limit = SendLimit::new(target, &features);
        self.composer.update(cx, |c, cx| c.set_send_limit(Some(limit), cx));
    }

    /// Opens a network's Status pane in place of the conversation. Like Qt's
    /// `openNetworkStatus`, it clears header focus, shows the lines at the end, and
    /// focuses the composer. An unknown network does nothing.
    pub fn open_status(&mut self, network_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.controller.knows_network(network_id) {
            return;
        }
        self.controller.open_status(network_id);
        self.sidebar.focus = None;
        self.sync_view(cx);
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// Leaves Status for the selected conversation, which keeps its scroll place
    /// (Escape and Ctrl+`). Focus goes to the composer. Find mode ends too.
    pub fn close_status(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.status_showing() {
            return;
        }
        self.controller.leave_status();
        self.sync_view(cx);
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// Ctrl+`: opens Status for the focused network, or leaves it when Status is showing
    /// and a conversation is selected.
    pub fn toggle_status(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::StatusToggle, window, cx) {
            return;
        }
        if self.status_showing() && self.controller.selected().is_some() {
            self.close_status(window, cx);
        } else {
            let network_id = self.focused_network();
            self.open_status(&network_id, window, cx);
        }
    }

    /// A click on an INVITE line's channel in Status: joins it on that network and
    /// selects it (Qt `joinInviteChannel`).
    pub fn join_status_invite(&mut self, channel: &str, cx: &mut Context<Self>) {
        let Some(network_id) = self.status_network() else {
            return;
        };
        self.join_invite(&network_id, channel, cx);
    }

    /// A nick click (transcript, member list): open or select that DM on the selected
    /// conversation's network. Self and channel-looking nicks do nothing.
    pub fn open_direct_nick(&mut self, nick: &str, cx: &mut Context<Self>) {
        if self.controller.open_direct(nick) {
            self.sync_view(cx);
        }
    }

    /// A middle click on a sidebar row: closes that conversation through the controller's
    /// `close_conversation`. A direct message closes, and so does a channel the user has
    /// left. A joined channel is a no-op. Closing the selected row selects its neighbour.
    pub fn close_conversation_row(
        &mut self,
        target: &ConversationRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_selected = *target == self.selected_ref();
        if !self
            .controller
            .close_conversation(&target.network_id, &target.key)
        {
            return;
        }
        self.sync_view(cx);
        if was_selected {
            self.focus_composer(window, cx);
        }
        cx.notify();
    }

    /// The header's LEAVE: parts the selected channel. The controller sends the `/part`
    /// through the same path a typed line takes.
    pub fn leave_selected_channel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.controller.leave_selected_channel();
        self.after_header_action(window, cx);
    }

    /// The header's JOIN: joins the selected channel again, as a typed `/join` does.
    pub fn join_selected_channel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.controller.join_selected_channel();
        self.after_header_action(window, cx);
    }

    /// The header's CLOSE, and Ctrl+W once its chord is live. Closes the selection through
    /// `close_direct`, which refuses a joined channel. Focus returns to the composer.
    pub fn close_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.controller.close_direct() {
            return;
        }
        self.after_header_action(window, cx);
    }

    /// Brings the shell up to what a header action changed, then returns focus to the
    /// composer, as Qt's header actions do.
    fn after_header_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drain_notifications(cx);
        self.focus_composer(window, cx);
        cx.notify();
    }

    pub fn select_by_ids(&mut self, network_id: &str, key: &str, cx: &mut Context<Self>) {
        let target = ConversationRef::new(network_id, key);
        if self
            .controller
            .conversations()
            .iter()
            .any(|row| ConversationRef::from_key(&row.key) == target)
        {
            self.select(target, cx);
        }
    }

    /// The network header chevron. Collapsing a network never changes the selection, so
    /// a hidden selected row stays selected, as in Qt.
    pub fn toggle_collapsed(&mut self, network_id: &str, cx: &mut Context<Self>) {
        let collapsed = !self.controller.is_network_collapsed(network_id);
        self.controller.set_network_collapsed(network_id, collapsed);
        cx.notify();
    }

    /// The chord context from the shell's state and the live keyboard focus.
    pub fn chord_context(&self, window: &Window, cx: &App) -> ChordContext {
        ChordContext {
            sheet: self.modal.as_ref().map(ActiveModal::sheet),
            status_open: self.status_showing(),
            channel: self.controller.is_channel(),
            joined: self.controller.channel_joined(),
            can_close: self.controller.can_close_selection(),
            header_focused: self.sidebar.focus.is_some(),
            members_focused: self.member_focus.is_focused(window),
            transcript_selection: self.transcript.read(cx).has_selection(),
            slash_open: self.slash_popup_showing(),
            find_active: self.find.is_some(),
            connect_open: self.connect_open,
        }
    }

    /// Whether `chord` is live now. Every shell action checks this first.
    pub fn chord_live(&self, chord: Chord, window: &Window, cx: &App) -> bool {
        chord_enabled(chord, &self.chord_context(window, cx))
    }

    /// Ctrl+Shift+S: collapse or restore the server list by width. Drops header focus,
    /// as Qt does.
    pub fn toggle_rail(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::ServerListToggle, window, cx) {
            return;
        }
        self.sidebar.rail_visible = !self.sidebar.rail_visible;
        self.sidebar.focus = None;
        cx.notify();
    }

    /// Ctrl+W: closes the selection while the chord is live. A direct message closes, and so
    /// does a channel the user has left. A joined channel is refused by the controller.
    pub fn close_selected_direct(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::CloseDirect, window, cx) {
            return;
        }
        self.close_selection(window, cx);
    }

    /// Ctrl+C with no composer selection: the member list copies the current row's
    /// nick; otherwise the transcript selection is copied.
    pub fn copy_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Copy, window, cx) {
            return;
        }
        let text = if self.member_focus.is_focused(window) {
            self.current_member().map(|m| m.nick).unwrap_or_default()
        } else {
            self.transcript.read(cx).copy_text()
        };
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Enter in the composer. With a network header focused, Enter opens that network's
    /// Status (Qt `activateFocusedNetworkHeader`) and does not send.
    pub fn composer_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The composer is under the Connect sheet, so Enter there never sends.
        if self.connect_open {
            return;
        }
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
        self.send_message(window, cx);
        // A command that opened a sheet (`/list`) keeps the focus.
        if self.modal.is_none() {
            self.focus_composer(window, cx);
        }
    }

    /// Ctrl+Shift+M: show or hide the member panel (channels only). Hiding it while the
    /// list has focus hands focus to the composer.
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
        let rows = self.sidebar_walk();
        if let Some(target) = step_conversation(&rows, Some(&self.selected_ref()), delta) {
            self.select(target, cx);
            cx.notify();
        }
    }

    /// Alt+Left / Alt+Right: moves header focus to the neighbouring network (wrapping)
    /// and restores a collapsed server list. The selection stays.
    pub fn walk_header(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        // While Connect shows, the same keys walk its rail instead.
        let connect_chord = if delta > 0 {
            Chord::ConnectWalkNetworkRight
        } else {
            Chord::ConnectWalkNetworkLeft
        };
        if self.chord_live(connect_chord, window, cx) {
            self.connect_walk_network(delta, window, cx);
            return;
        }
        let chord = if delta > 0 {
            Chord::WalkNetworkRight
        } else {
            Chord::WalkNetworkLeft
        };
        if !self.chord_live(chord, window, cx) {
            return;
        }
        let current = self.focused_network();
        let networks = self.controller.network_order();
        if let Some(id) = step_network(&networks, self.sidebar.focus.as_deref(), Some(&current), delta) {
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
        if let Some(network_id) = self.sidebar.focus.clone() {
            self.controller.set_network_collapsed(&network_id, collapsed);
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
        self.controller.set_all_networks_collapsed(collapsed);
        cx.notify();
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
        if let Some(network_id) = self.sidebar.focus.clone() {
            if self.controller.move_network(&network_id, delta) {
                cx.notify();
            }
        }
    }

    /// Alt+A: the next mention or unread conversation. Its network is expanded so the
    /// landing row is visible. With nothing unread it opens Status: the network whose
    /// Status is showing, else the focused network (Qt's `focusedNetworkId`).
    pub fn select_next_unread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::NextUnread, window, cx) {
            return;
        }
        let rows = self.sidebar_walk();
        let status_network = self.focused_network();
        let networks = self.controller.network_order();
        let current = self.selected_ref();
        let landing = next_unread_or_status(&rows, Some(&current), Some(&status_network), &networks);
        match landing {
            Some(UnreadTarget::Conversation(target)) => {
                self.controller.set_network_collapsed(&target.network_id, false);
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
        self.controller.mark_all_read();
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
        if !down {
            let at_top = self.transcript.read(cx).at_top();
            if at_top && self.controller.chat_history_enabled() && !self.controller.shows_status() {
                if self.controller.request_older_transcript_history()
                    || self.controller.older_transcript_history_inflight()
                {
                    self.rebuild_view_keeping_place(cx);
                    return;
                }
            }
        }
        self.transcript.update(cx, |t, cx| {
            let was_at_end = t.at_end();
            t.scroll_page(down, half);
            if was_at_end && !t.at_end() {
                self.controller.clear_history_page_cap_tail();
            }
            if !was_at_end && t.at_end() {
                self.controller.clear_history_page_cap_tail();
            }
            cx.notify();
        });
        let at_end = self.transcript.read(cx).at_end();
        self.report_transcript_viewport(at_end, window, cx);
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
    pub fn current_member(&self) -> Option<MemberView> {
        self.controller.members().get(self.member_cursor).cloned()
    }

    /// Up / Down in the member list.
    pub fn member_move(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::MembersStep, window, cx) {
            return;
        }
        self.move_member_cursor(delta);
        cx.notify();
    }

    /// PgUp / PgDn and Shift variants in the member list. A page is `round(visible rows
    /// × 0.8)`, a half page `× 0.35`, at least one row.
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
        let len = self.controller.members().len();
        self.member_cursor = if bottom { len.saturating_sub(1) } else { 0 };
        self.members_scroll.scroll_to_item(self.member_cursor);
        cx.notify();
    }

    /// Enter in the member list: opens or selects the DM with the current nick and
    /// focuses the composer. Self is a no-op, as in the nick sheet.
    pub fn member_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::MembersActivate, window, cx) {
            return;
        }
        let Some(nick) = self.current_member().map(|m| m.nick) else {
            return;
        };
        if nick == self.current_nick() {
            return;
        }
        self.open_direct_nick(&nick, cx);
        self.focus_composer(window, cx);
    }

    fn move_member_cursor(&mut self, delta: isize) {
        let len = self.controller.members().len();
        self.member_cursor = move_index(len, self.member_cursor, delta);
        self.members_scroll.scroll_to_item(self.member_cursor);
    }

    pub fn focus_composer(&self, window: &mut Window, cx: &App) {
        window.focus(&self.composer.read(cx).focus_handle(cx));
    }

    /// Ends a text drag on mouse-up. A release over a link acts on it. Notifies only
    /// when a drag was running.
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

/// The composer's draft history and Tab completion. These edit the same line as the
/// shell's slash popup, so they live with the commands.
impl Shell {
    /// Up (`delta` -1) or Down (`delta` +1) in the composer. Returns whether the key was
    /// used. With no history to walk it is not, and the key falls through to the text
    /// field.
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

    /// Tab (`backward` false) or Shift+Tab (`backward` true) in the composer. The key is
    /// always consumed here, even with nothing to complete, so focus never leaves the
    /// composer.
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

    /// The nicks Tab can complete on the open view: the members or the peer, then the
    /// channel names on the selected network.
    pub fn completion_candidates(&self) -> Vec<String> {
        if self.status_showing() {
            return nick_candidates(NickScope::Status);
        }
        let rows = self.sidebar_walk();
        let network = self.selected_ref().network_id;
        let channels: Vec<&str> = rows
            .iter()
            .filter(|row| row.channel && row.conversation.network_id == network)
            .map(|row| row.name.as_str())
            .collect();
        if self.controller.is_channel() {
            let members: Vec<String> = self.controller.members().into_iter().map(|m| m.nick).collect();
            let members: Vec<&str> = members.iter().map(String::as_str).collect();
            nick_candidates(NickScope::Channel {
                members: &members,
                channels: &channels,
            })
        } else {
            let peer = self.controller.selected_target().to_owned();
            nick_candidates(NickScope::Direct {
                peer: &peer,
                channels: &channels,
            })
        }
    }
}
