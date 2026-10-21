//! The send intent. Port of `SendToTarget` and `echoIfPresent` in
//! `tui/internal/controller/controller.go` (Go), and the send path of
//! `src/irc/irccontroller.cpp` (C++).
//!
//! One path for every line: the intent calls the session, the session builds the
//! frame (`irc::commandbuilder` rules, sized by the network's LINELEN) and writes it
//! through the transport. The transcript row comes back as the server's echo when
//! the server negotiated `echo-message`. Only without it does the controller add a
//! local echo, the same fallback Qt uses.

use crate::irc::capability::IrcCapability;
use crate::irc::event::{Event, IrcConversationCause, MessageEvent, MsgId};
use crate::irc::viewnotify::ViewNotify;
use crate::session::events::SessionState;
use crate::session::session::Session;

use super::controller::Controller;

impl Controller {
    /// Sends `text` to `target` on `network_id` (`SendToTarget`). Returns false, and
    /// records the reason as the network's last error, when the send is refused:
    /// missing input, an unknown network, a network that is not registered, or a
    /// frame the session would not write.
    pub fn send_to_target(&mut self, network_id: &str, target: &str, text: &str) -> bool {
        self.pump();
        let sent = self.send_now(network_id, target, text);
        self.pump();
        sent
    }

    /// The send itself, without the pumps around it.
    fn send_now(&mut self, network_id: &str, target: &str, text: &str) -> bool {
        if network_id.is_empty() || target.is_empty() || text.is_empty() {
            return self.refuse_send(network_id, "Missing network, target, or text");
        }
        let Some(session) = self.session(network_id) else {
            return self.refuse_send(network_id, "That network is not configured");
        };
        if session.state() != SessionState::Registered {
            return self.refuse_send(network_id, "Not connected");
        }
        if !session.send_privmsg(target, text) {
            return self.refuse_send(network_id, "Failed to send message");
        }
        let key = self.reducer.conversation_key(network_id, target);
        // A quiet send never creates a conversation; it only reaches one that exists.
        self.reducer
            .ensure_conversation(&key, target, IrcConversationCause::QuietSend);
        self.echo_if_present(&session, target, text);
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
    fn echo_if_present(&mut self, session: &Session, target: &str, body: &str) {
        let network_id = session.network_id();
        let echo_negotiated = self
            .capabilities
            .get(&network_id)
            .is_some_and(|caps| caps.contains(IrcCapability::EchoMessage));
        if echo_negotiated {
            return;
        }
        let key = self.reducer.conversation_key(&network_id, target);
        if self.reducer.find(&key).is_none() {
            return;
        }
        let timestamp = self.clock.now();
        self.apply(Event::Message(MessageEvent {
            conversation: key,
            author: session.nick(),
            body: body.to_owned(),
            timestamp,
            server_time: None,
            target: target.to_owned(),
            msgid: MsgId::default(),
            only_visible_to_you: false,
        }));
    }
}
