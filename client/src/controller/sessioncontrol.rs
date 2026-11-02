//! App quit shutdown, per-network disconnect, and the connection status and last-error
//! fields the shell and Connect read (`Controller.Shutdown` and friends in the TUI).

use crate::irc::viewnotify::ViewNotify;

use super::controller::Controller;
use super::snapshots::state_text;

impl Controller {
    /// Stops every session as the app quits, so each refreshes its STS expiry on the way out
    /// (`Controller.Shutdown` in the TUI). Unlike `disconnect`, it sends no QUIT.
    pub fn shutdown(&self) {
        self.manager.stop_all();
    }

    /// Sends QUIT and stops `network_id`'s session (`Disconnect`). Returns false when
    /// the network has no session or the session was already idle.
    pub fn disconnect(&mut self, network_id: &str) -> bool {
        self.disconnect_with_reason(network_id, "")
    }

    /// Sends QUIT with `reason` and stops `network_id`'s session. `/disconnect <reason>` and
    /// `/quit <reason>` run through here, as does `disconnect`.
    pub(super) fn disconnect_with_reason(&mut self, network_id: &str, reason: &str) -> bool {
        self.pump();
        let Some(session) = self.manager.find(network_id) else {
            return false;
        };
        if !session.quit(reason) {
            return false;
        }
        self.queue(vec![ViewNotify::Status]);
        self.pump();
        true
    }

    /// Whether `network_id` can be disconnected: its session is live or reconnecting, so
    /// Disconnect has something to stop. Connecting
    /// counts as live, as it does in Qt. False for a network with no session.
    pub fn can_disconnect(&self, network_id: &str) -> bool {
        self.manager.is_live(network_id)
    }

    /// The connection word for `network_id`, or for the focused network when the id
    /// is empty (`ConnectionStatusFor`). A network with no session is Offline.
    pub fn connection_status_for(&self, network_id: &str) -> &'static str {
        let id = if network_id.is_empty() {
            self.focused_network_id()
        } else {
            network_id
        };
        match self.manager.find(id) {
            Some(session) => state_text(session.state()),
            None => "Offline",
        }
    }

    /// The last error recorded for `network_id` (`LastErrorFor`), if any.
    pub fn last_error_for(&self, network_id: &str) -> Option<&str> {
        self.last_errors.get(network_id).map(String::as_str)
    }

    /// Records or clears a network's last error (`setLastError`). An empty or `None`
    /// message clears it.
    pub fn set_last_error(&mut self, network_id: &str, message: Option<String>) {
        match message {
            Some(message) if !message.is_empty() => {
                self.last_errors.insert(network_id.to_owned(), message);
            }
            _ => {
                self.last_errors.remove(network_id);
            }
        }
    }
}
