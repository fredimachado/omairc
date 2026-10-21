//! The local lines the shell writes: a command outcome or CTCP reply on Status, a
//! whois-style reply in a conversation, and the two `/clear` forms. Nothing here
//! writes the wire. A line the server did not send is a local line, as Qt's
//! `IrcController` writes its own `IrcStatusEntry::outcome` and `IrcWhoisTranscript`
//! rows. Ports `Outcome`, `Clear` and the reply paths of `tui/internal/controller/`
//! (Go) and `src/irc/irccontroller.cpp` (C++).

use crate::irc::event::{Event, WhoisTranscriptEvent};
use crate::irc::statusentry;
use crate::irc::viewnotify::ViewNotify;

use super::controller::Controller;

impl Controller {
    /// Writes a command outcome to a network's Status log (`IrcStatusEntry::outcome`).
    /// A refused command, `/help` or `/pref` on Status, and `/version` on Status all
    /// land here.
    pub fn log_outcome(&mut self, network_id: &str, text: &str) {
        self.pump();
        let entry = statusentry::outcome(network_id, text, self.clock.now());
        self.append_status(entry);
        self.pump();
    }

    // Test seam: the shell writes CTCP replies through the dispatcher, so only the tests call this.
    #[allow(dead_code)]
    /// Writes a CTCP reply line to a network's Status log, labelled `CTCP`.
    pub fn log_ctcp_reply(&mut self, network_id: &str, text: &str) {
        self.pump();
        let entry = statusentry::ctcp_reply(network_id, text, self.clock.now());
        self.append_status(entry);
        self.pump();
    }

    // Test seam: the shell writes whois rows through the dispatcher, so only the tests call this.
    #[allow(dead_code)]
    /// Writes one whois-style line into the selected conversation. Returns false
    /// when Status is the selection, so the caller writes to Status instead.
    pub fn reply_in_selection(&mut self, text: &str) -> bool {
        self.pump();
        if self.shows_status() {
            return false;
        }
        let Some(key) = self.selected.clone() else {
            return false;
        };
        if self.reducer.find(&key).is_none() {
            return false;
        }
        self.apply(Event::WhoisTranscript(WhoisTranscriptEvent {
            destination: key,
            formatted_body: text.to_owned(),
        }));
        self.pump();
        true
    }

    /// Empties one network's Status log (`/clear` on Status).
    pub fn clear_status(&mut self, network_id: &str) {
        self.pump();
        self.console.clear(network_id);
        self.queue(vec![ViewNotify::Status]);
        self.pump();
    }

    /// Empties one conversation's stored lines (`/clear` in a conversation). Its
    /// msgid set stays, so a replayed line cannot put a cleared row back.
    pub fn clear_conversation(&mut self, network_id: &str, target: &str) {
        self.pump();
        if network_id.is_empty() || target.is_empty() {
            return;
        }
        let key = self.reducer.conversation_key(network_id, target);
        if self.reducer.find(&key).is_none() {
            return;
        }
        self.reducer.clear_messages(&key);
        self.queue(vec![ViewNotify::Messages]);
        self.pump();
    }
}
