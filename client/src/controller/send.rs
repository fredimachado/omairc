//! The send intents.
//! the join path in `tui/internal/controller/controller.go`.
//! One path for every line: the intent calls the session, the session builds the
//! frame (`irc::commandbuilder` rules, sized by the network's LINELEN) and writes it
//! through the transport. The transcript row comes back as the server's echo when
//! the server negotiated `echo-message`. Only without it does the controller add a
//! local echo, the same fallback Qt uses.

use crate::irc::capability::IrcCapability;
use crate::irc::command::{CommandOutcome, ComposerSurface, Words};
use crate::irc::event::{ActionEvent, Event, IrcConversationCause, MessageEvent, MsgId, NoticeEvent};
use crate::irc::jointarget::IrcJoinTarget;
use crate::irc::viewnotify::ViewNotify;
use crate::session::events::SessionState;
use crate::session::session::Session;

use super::commanddispatcher::refused_or_wrong_scope;
use super::controller::Controller;

/// What a send carries: a chat line, a CTCP ACTION (`/me`), or a NOTICE (`/notice`). A NOTICE
/// echoes locally even on an `echo-message` network, as Qt does (`EchoIfPresent`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outgoing {
    Say,
    Action,
    Notice,
}

/// The two quiet-send verbs. `/msg` is PRIVMSG to a nick only; `/notice` is NOTICE to any target
/// (`IrcCommandDispatcher::QuietSend`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QuietSend {
    Msg,
    Notice,
}

impl Controller {
    /// Sends `text` to `target` on `network_id` (`SendToTarget`). The product CLI's `send` calls
    /// this. It never changes the selection. Returns false, and records the reason as the network's
    /// last error, when the send is refused: missing input, an unknown network, a network that is not
    /// registered, or a frame the session would not write. A `/ns` or `/msg` line takes the same
    /// outbound path, so its Status entry is masked the same way.
    pub fn send_to_target(&mut self, network_id: &str, target: &str, text: &str) -> bool {
        self.pump();
        let sent = self.send_now(network_id, target, text, Outgoing::Say);
        self.pump();
        sent
    }

    /// Sends a `/me` action to `target` on `network_id` (a CTCP ACTION PRIVMSG). The
    /// refusals are the same as `send_to_target`'s.
    pub fn send_action(&mut self, network_id: &str, target: &str, text: &str) -> bool {
        self.pump();
        let sent = self.send_now(network_id, target, text, Outgoing::Action);
        self.pump();
        sent
    }

    /// Sends a NOTICE to `target` on `network_id`. The refusals are the same as
    /// `send_to_target`'s.
    pub fn send_notice(&mut self, network_id: &str, target: &str, text: &str) -> bool {
        self.pump();
        let sent = self.send_now(network_id, target, text, Outgoing::Notice);
        self.pump();
        sent
    }

    /// Joins `channel` on `network_id` and selects it (`JoinInvite`). The join clears
    /// that channel's invite row. False, with the reason as the last error, when the
    /// network is not registered or the join frame is refused.
    pub fn join_channel(&mut self, network_id: &str, channel: &str) -> bool {
        self.pump();
        let Some(session) = self.session(network_id) else {
            return self.refuse_send(network_id, "That network is not configured");
        };
        if session.state() != SessionState::Registered {
            return self.refuse_send(network_id, "Not connected");
        }
        let features = self.features(network_id).clone();
        let Some(target) = IrcJoinTarget::make(channel, None, &features) else {
            return self.refuse_send(network_id, "That is not a channel name");
        };
        if !session.join(&target) {
            return self.refuse_send(network_id, "Failed to join channel");
        }
        self.inbox
            .consume_invite(network_id, channel, features.case_mapping());
        self.select(network_id, channel);
        self.set_last_error(network_id, None);
        self.queue(vec![ViewNotify::Status]);
        self.pump();
        true
    }

    /// The send itself, without the pumps around it.
    fn send_now(&mut self, network_id: &str, target: &str, text: &str, kind: Outgoing) -> bool {
        if network_id.is_empty() || target.is_empty() || text.is_empty() {
            return self.refuse_send(network_id, "Missing network, target, or text");
        }
        let Some(session) = self.session(network_id) else {
            return self.refuse_send(network_id, "That network is not configured");
        };
        if session.state() != SessionState::Registered {
            return self.refuse_send(network_id, "Not connected");
        }
        let written = match kind {
            Outgoing::Say => session.send_privmsg(target, text),
            Outgoing::Notice => session.send_notice(target, text),
            Outgoing::Action => session.send_action(target, text),
        };
        if !written {
            return self.refuse_send(network_id, "Failed to send message");
        }
        self.note_accepted_send(network_id, target);
        if kind == Outgoing::Say {
            // Talking in a direct message is what keeps it for the next launch.
            self.remember_open_direct(network_id, target);
            // A PRIVMSG counts as chat from this user: it records the nick and brings the user back
            // from away, as `/msg` and Qt's `sendToTarget` do.
            self.note_nick_delivery(network_id, target);
        }
        let key = self.reducer.conversation_key(network_id, target);
        // A quiet send never creates a conversation; it only reaches one that exists.
        self.reducer
            .ensure_conversation(&key, target, IrcConversationCause::QuietSend);
        self.echo_if_present(&session, target, text, kind);
        if kind == Outgoing::Say {
            self.note_chat_sent(network_id);
        }
        self.set_last_error(network_id, None);
        self.queue(vec![ViewNotify::Status]);
        true
    }

    /// Records a chat line the session accepted for `target`, as the `Sent` surface the shell
    /// drains. Every accepted send goes through here, so the shell sees each one the same way.
    pub(super) fn note_accepted_send(&mut self, network_id: &str, target: &str) {
        let key = self.reducer.conversation_key(network_id, target);
        self.queue(vec![ViewNotify::Sent(key)]);
    }

    /// Records a refused send as the network's last error and wakes Status.
    fn refuse_send(&mut self, network_id: &str, message: &str) -> bool {
        self.set_last_error(network_id, Some(message.to_owned()));
        self.queue(vec![ViewNotify::Status]);
        false
    }

    /// Adds the local echo of a sent line, when the network did not negotiate
    /// `echo-message` and the conversation exists (`echoIfPresent`).
    pub(super) fn echo_if_present(&mut self, session: &Session, target: &str, body: &str, kind: Outgoing) {
        let network_id = session.network_id();
        let echo_negotiated = self
            .capabilities
            .get(&network_id)
            .is_some_and(|caps| caps.contains(IrcCapability::EchoMessage));
        if echo_negotiated && kind != Outgoing::Notice {
            return;
        }
        let key = self.reducer.conversation_key(&network_id, target);
        if self.reducer.find(&key).is_none() {
            return;
        }
        let timestamp = self.clock.now();
        let event = match kind {
            Outgoing::Say => Event::Message(MessageEvent {
                conversation: key,
                author: session.nick(),
                body: body.to_owned(),
                timestamp,
                server_time: None,
                target: target.to_owned(),
                msgid: MsgId::default(),
                only_visible_to_you: false,
            }),
            Outgoing::Action => Event::Action(ActionEvent {
                conversation: key,
                author: session.nick(),
                body: body.to_owned(),
                timestamp,
                server_time: None,
                target: target.to_owned(),
                msgid: MsgId::default(),
                only_visible_to_you: false,
            }),
            Outgoing::Notice => Event::Notice(NoticeEvent {
                conversation: key,
                author: session.nick(),
                body: body.to_owned(),
                timestamp,
                server_time: None,
                target: target.to_owned(),
                msgid: MsgId::default(),
                only_visible_to_you: false,
            }),
        };
        self.apply(event);
    }
    /// Whether the selection is a channel that is not joined (`/close` and chat refuse it).
    fn selected_channel_left(&self) -> bool {
        match self.selected.as_ref() {
            Some(key) => match self.reducer.find(key) {
                Some(conversation) if conversation.is_channel() => {
                    !self.channel_is_joined(key)
                }
                _ => false,
            },
            None => false,
        }
    }

    /// `/msg <nick> <text>` and `/notice <target> <text>` (`dispatchQuietSend`). The line goes out
    /// without opening or selecting a conversation: `/msg` never invents a direct message
    /// (`QuietSend`). The echo shows only where a conversation already exists, and a PRIVMSG
    /// counts as chat activity, so the user is brought back from away.
    pub(super) fn dispatch_quiet_send(&mut self, kind: QuietSend, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        let words = Words::split(argument);
        let target = words.first.clone();
        let body = words.rest.clone();
        if target.is_empty() || body.is_empty() {
            return CommandOutcome::Refused;
        }
        let network = self.surface_network(surface);
        if network.is_empty() {
            return refused_or_wrong_scope(surface);
        }
        if kind == QuietSend::Msg && self.reducer.server_features(&network).is_channel(&target) {
            return CommandOutcome::Refused;
        }
        let Some(active) = self.registered_session(&network) else {
            return CommandOutcome::NotConnected;
        };

        let sent = match kind {
            QuietSend::Msg => active.send_privmsg(&target, &body),
            QuietSend::Notice => active.send_notice(&target, &body),
        };
        if !sent {
            return CommandOutcome::Refused;
        }

        self.note_accepted_send(&network, &target);
        self.note_nick_delivery(&network, &target);
        if kind == QuietSend::Msg {
            // A private message the user sent is a query worth reopening (`rememberOpenDirect`).
            self.remember_open_direct(&network, &target);
        }
        let key = self.reducer.conversation_key(&network, &target);
        self.reducer
            .ensure_conversation(&key, &target, IrcConversationCause::QuietSend);
        let outgoing = match kind {
            QuietSend::Msg => Outgoing::Say,
            QuietSend::Notice => Outgoing::Notice,
        };
        self.echo_if_present(&active, &target, &body, outgoing);
        if kind == QuietSend::Msg {
            self.note_chat_sent(&network);
        }
        CommandOutcome::Sent
    }

    /// `/me <text>` (`dispatch`'s action branch). A CTCP ACTION to the selected target. A channel
    /// you have left refuses it. A sent action is chat activity, and it clears the typing hint.
    pub(super) fn dispatch_action(&mut self, text: &str) -> CommandOutcome {
        let Some(active) = self.selected_session() else {
            return CommandOutcome::WrongScope;
        };
        if self.selected.is_none() {
            return CommandOutcome::WrongScope;
        }
        if self.selected_channel_left() {
            return CommandOutcome::Refused;
        }
        let network = active.network_id();
        let target = self.selected_target().to_owned();
        if !active.send_action(&target, text) {
            return CommandOutcome::Refused;
        }
        self.note_accepted_send(&network, &target);
        self.note_nick_delivery(&network, &target);
        self.echo_if_present(&active, &target, text, Outgoing::Action);
        self.typing_target = None;
        self.note_chat_sent(&network);
        CommandOutcome::Sent
    }

    /// Plain text to the selected conversation (`sendSelectedMessageOutcome`). The message
    /// echoes as the server's own line when `echo-message` is on, and locally otherwise. The
    /// target becomes a direct message the user has now spoken in.
    pub(super) fn send_selected_message(&mut self, text: &str) -> CommandOutcome {
        let Some(active) = self.selected_session() else {
            return CommandOutcome::WrongScope;
        };
        if self.selected.is_none() {
            return CommandOutcome::WrongScope;
        }
        if self.selected_channel_left() {
            return CommandOutcome::Refused;
        }
        let network = active.network_id();
        let target = self.selected_target().to_owned();
        if !active.send_privmsg(&target, text) {
            return CommandOutcome::Refused;
        }
        self.note_accepted_send(&network, &target);
        self.note_nick_delivery(&network, &target);
        self.echo_if_present(&active, &target, text, Outgoing::Say);
        self.note_chat_sent(&network);
        CommandOutcome::Sent
    }
}
