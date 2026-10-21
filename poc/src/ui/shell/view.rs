//! The shell's reads of the controller, and the view the transcript shows. Every value
//! here comes from `Controller` or from presentation state the shell owns (the composer,
//! the sidebar focus, the Connect model). Nothing here writes controller state.
//!
//! `sync_view` is the one place the transcript and the composer follow the controller's
//! selection. Every intent calls it, and so does every change the controller raises as a
//! surface, so a selection the controller makes on its own (a join, a close, a removal)
//! shows the same way a click does.

use gpui::Context;

use crate::controller::controller::Controller;
use crate::controller::snapshots::ConversationRow;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::model::composer::DraftKey;
use crate::model::sidebar::{sidebar_rows, NetworkHeader, SidebarRow};
use crate::model::status::{connection_line, status_rows};
use crate::model::scroll_place::SavedPlace;
use crate::model::ConversationRef;
use crate::ui::shell::Shell;
use crate::ui::transcript::view::ConversationLines;

/// The view the transcript was last built for: a network's Status, or a conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownView {
    /// The network whose Status is showing, or `None` while the conversation shows.
    pub status: Option<String>,
    /// The selected conversation. With nothing selected it names the focused network
    /// with an empty key, which no row matches.
    pub conversation: ConversationRef,
}

impl ShownView {
    /// The composer draft this view owns (Status for its network, else the conversation).
    pub fn draft_key(&self) -> DraftKey {
        DraftKey::for_view(self.status.as_deref(), &self.conversation)
    }
}

/// The view the controller shows right now: Status of the focused network while Status
/// shows, and the selected conversation (or an empty one) either way.
pub fn shown_view(controller: &Controller) -> ShownView {
    let conversation = match controller.selected() {
        Some(key) => ConversationRef::from_key(key),
        None => ConversationRef::new(controller.focused_network_id(), ""),
    };
    ShownView {
        status: controller
            .shows_status()
            .then(|| controller.focused_network_id().to_owned()),
        conversation,
    }
}

impl Shell {
    /// The view on screen right now, as the controller's selection and Status say.
    pub fn current_view(&self) -> ShownView {
        shown_view(&self.controller)
    }

    /// The selected conversation as a reference (`ConversationRef::from_key`).
    pub fn selected_ref(&self) -> ConversationRef {
        match self.controller.selected() {
            Some(key) => ConversationRef::from_key(key),
            None => ConversationRef::new(self.controller.focused_network_id(), ""),
        }
    }

    /// The network whose Status pane is showing, or `None` while a conversation shows.
    pub fn status_network(&self) -> Option<String> {
        self.controller
            .shows_status()
            .then(|| self.controller.focused_network_id().to_owned())
    }

    /// Whether the transcript shows a Status pane.
    pub fn status_showing(&self) -> bool {
        self.controller.shows_status()
    }

    /// The network the keyboard and the composer point at (`FocusedNetworkID`).
    pub fn focused_network(&self) -> String {
        self.controller.focused_network_id().to_owned()
    }

    /// The nick the focused network's session holds.
    pub fn current_nick(&self) -> String {
        self.controller
            .current_nick_for(self.controller.focused_network_id())
    }

    /// The features the network's ISUPPORT advertised (CHANTYPES, PREFIX, LINELEN).
    pub fn features_of(&self, network_id: &str) -> IrcServerFeatures {
        self.controller.features(network_id).clone()
    }

    /// The sidebar rows in sidebar order, collapsed networks included. The walks use them.
    pub fn sidebar_walk(&self) -> Vec<SidebarRow> {
        let rows = self.controller.conversations();
        sidebar_rows(&rows, |id| self.controller.is_network_collapsed(id))
    }

    /// The networks in sidebar order, each with its header and badges.
    pub fn network_headers(&self, rows: &[ConversationRow]) -> Vec<NetworkHeader> {
        self.controller
            .network_order()
            .into_iter()
            .map(|id| {
                let nick = self.controller.current_nick_for(&id);
                let avatar = self
                    .controller
                    .nick_facts(&id)
                    .get(&nick)
                    .and_then(|facts| facts.avatar.clone());
                let icon_color = self
                    .connect
                    .saved()
                    .iter()
                    .find(|profile| profile.network_id == id)
                    .and_then(|profile| profile.icon_color);
                NetworkHeader {
                    name: self.controller.network_name(&id),
                    nick,
                    avatar,
                    icon_color,
                    icon: self.controller.features(&id).icon_url().map(str::to_owned),
                    unread: self.controller.unread_count_for(&id) > 0,
                    mention: self.controller.mention_for(&id),
                    has_directs: rows.iter().any(|row| row.key.network_id == id && row.direct),
                    collapsed: self.controller.is_network_collapsed(&id),
                    status: connection_line(&self.controller, &id),
                    id,
                }
            })
            .collect()
    }

    /// The networks that hold at least one conversation (the Connect sheet's Remove
    /// check needs them).
    pub fn populated_networks(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for row in self.controller.conversations() {
            if !ids.contains(&row.key.network_id) {
                ids.push(row.key.network_id);
            }
        }
        ids
    }

    /// The channel row `name` names on `network_id`, if the network has one. Channel names
    /// match ignoring ASCII case, and a direct message never matches.
    pub fn find_channel(&self, network_id: &str, name: &str) -> Option<ConversationRef> {
        self.controller
            .conversations()
            .into_iter()
            .find(|row| {
                !row.direct
                    && row.key.network_id == network_id
                    && row.target.eq_ignore_ascii_case(name)
            })
            .map(|row| ConversationRef::from_key(&row.key))
    }

    /// Brings the transcript, the composer's draft and the send limit to the view the
    /// controller now shows. A view that is already on screen is left alone; a change
    /// remembers the place and the draft of the view it leaves.
    pub fn sync_view(&mut self, cx: &mut Context<Self>) {
        let view = self.current_view();
        if view == self.shown {
            return;
        }
        let previous = std::mem::replace(&mut self.shown, view.clone());
        if previous.status.is_none() {
            let place = self.transcript.update(cx, |t, _| t.capture_scroll_place());
            self.remember_place(previous.conversation, place);
        }
        let text = self.composer.read(cx).text().to_string();
        self.composer_state.stash(&text);
        self.end_find_mode(cx);
        self.member_cursor = 0;
        let draft = self.composer_state.switch_to(view.draft_key());
        self.composer.update(cx, |c, cx| {
            c.set_text(draft);
            cx.notify();
        });
        self.show_shown_view(None, cx);
        self.sync_send_limit(cx);
        cx.notify();
    }

    /// Stores where a conversation's transcript was left. The controller keeps it in the
    /// settings file, so it is there after a restart.
    fn remember_place(&mut self, conversation: ConversationRef, place: SavedPlace) {
        self.controller
            .remember_place(&conversation.network_id, &conversation.key, place.to_stored());
    }

    /// Where a conversation's transcript was last left, as the settings file has it.
    fn saved_place(&self, conversation: &ConversationRef) -> Option<SavedPlace> {
        self.controller
            .place_for(&conversation.network_id, &conversation.key)
            .and_then(|stored| SavedPlace::from_stored(&stored))
    }

    /// Rebuilds the transcript for the view on screen, keeping the reader's place. Used
    /// when the lines or the settings that shape them change.
    pub fn rebuild_view_keeping_place(&mut self, cx: &mut Context<Self>) {
        let place = self.transcript.update(cx, |t, _| t.capture_scroll_place());
        self.show_shown_view(Some(place), cx);
    }

    /// Fills the transcript with the shown view's rows. A Status view shows its log. A
    /// conversation shows its lines, its "New messages" mark and its people's presence,
    /// and the list lands at `place`, or at the mark or the end.
    pub fn show_shown_view(&mut self, place: Option<crate::model::scroll_place::SavedPlace>, cx: &mut Context<Self>) {
        match self.shown.status.clone() {
            Some(network_id) => {
                let lines = self.controller.status_lines(&network_id);
                let rows = status_rows(&lines);
                self.transcript.update(cx, |t, _| t.set_status(rows));
            }
            None => {
                let network_id = self.shown.conversation.network_id.clone();
                let features = self.features_of(&network_id);
                let self_nick = self.controller.current_nick_for(&network_id);
                // A DM whose peer is typing ends the list with the typing dots.
                let typing_peer = (!self.controller.is_channel()
                    && !self.controller.typing_nicks().is_empty())
                .then(|| self.controller.selected_target().to_owned());
                let lines = ConversationLines {
                    messages: self.controller.messages(),
                    mark: self.controller.unread_mark(),
                    facts: self.controller.nick_facts(&network_id),
                    self_nick: &self_nick,
                    features: &features,
                    typing_peer,
                };
                let open_at_unread = place.is_none() && self.controller.preferences().unread;
                let place = place.or_else(|| self.saved_place(&self.shown.conversation));
                self.transcript.update(cx, |t, _| {
                    t.set_conversation(lines);
                    t.restore_scroll_place(place.as_ref(), open_at_unread);
                });
            }
        }
    }
}
