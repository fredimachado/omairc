//! The send intents. Port of `SendToTarget`, `echoIfPresent`, the action path and
//! the join path in `tui/internal/controller/controller.go` (Go), and the send path of
//! `src/irc/irccontroller.cpp` (C++).
//!
//! One path for every line: the intent calls the session, the session builds the
//! frame (`irc::commandbuilder` rules, sized by the network's LINELEN) and writes it
//! through the transport. The transcript row comes back as the server's echo when
//! the server negotiated `echo-message`. Only without it does the controller add a
//! local echo, the same fallback Qt uses.

use crate::irc::capability::IrcCapability;
use crate::irc::event::{ActionEvent, Event, IrcConversationCause, MessageEvent, MsgId, NoticeEvent};
use crate::irc::jointarget::IrcJoinTarget;
use crate::irc::viewnotify::ViewNotify;
use crate::session::events::SessionState;
use crate::session::session::Session;

use super::controller::Controller;

/// What a send carries: a chat line, a CTCP ACTION (`/me`), or a NOTICE (`/notice`). A NOTICE
/// echoes locally even on an `echo-message` network, as Qt does (`EchoIfPresent`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outgoing {
    Say,
    Action,
    Notice,
}

impl Controller {
    // Phase 9: the product CLI `send` calls this; until then only the tests do.
    #[allow(dead_code)]
    /// Sends `text` to `target` on `network_id` (`SendToTarget`). Returns false, and
    /// records the reason as the network's last error, when the send is refused:
    /// missing input, an unknown network, a network that is not registered, or a
    /// frame the session would not write.
    pub fn send_to_target(&mut self, network_id: &str, target: &str, text: &str) -> bool {
        self.pump();
        let sent = self.send_now(network_id, target, text, Outgoing::Say);
        self.pump();
        sent
    }

    /// Sends a `/me` action to `target` on `network_id` (a CTCP ACTION PRIVMSG). The
    /// refusals are the same as `send_to_target`'s.
    #[allow(dead_code)]
    pub fn send_action(&mut self, network_id: &str, target: &str, text: &str) -> bool {
        self.pump();
        let sent = self.send_now(network_id, target, text, Outgoing::Action);
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
        if kind == Outgoing::Say {
            // Talking in a direct message is what keeps it for the next launch.
            self.remember_open_direct(network_id, target);
        }
        let key = self.reducer.conversation_key(network_id, target);
        // A quiet send never creates a conversation; it only reaches one that exists.
        self.reducer
            .ensure_conversation(&key, target, IrcConversationCause::QuietSend);
        self.echo_if_present(&session, target, text, kind);
        self.set_last_error(network_id, None);
        self.queue(vec![ViewNotify::Status]);
        true
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
}
