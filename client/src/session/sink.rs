//! The transport's callbacks into the session: `connected`, `encrypted`, `disconnected` and
//! `errorOccurred` (`IrcTransport`'s signals in `tui/internal/session/session.go`, and the Go `Connected`,
//! `Encrypted`, `Disconnected` and `Error` entry points of `session.go`). The session's
//! `Core` answers them here, so the lifecycle entry points sit in one file.

use std::rc::Weak;

use super::connection::DisconnectOutcome;
use super::events::{ErrorKind, SessionState};
use super::session::{Core, Session, Shared};
use super::transport::{TransportError, TransportErrorKind, TransportSink};

/// Receives the transport's callbacks and forwards them to the session. Holds a weak reference,
/// so a dropped session stops receiving.
pub(super) struct SinkAdapter {
    pub(super) session: Weak<Shared>,
}

impl SinkAdapter {
    fn with_session(&self, action: impl FnOnce(&Session)) {
        if let Some(shared) = self.session.upgrade() {
            action(&Session { shared });
        }
    }
}

impl TransportSink for SinkAdapter {
    fn connected(&self) {
        self.with_session(|session| session.locked(Core::on_connected));
    }

    fn encrypted(&self) {
        self.with_session(|session| session.locked(Core::on_encrypted));
    }

    fn disconnected(&self) {
        self.with_session(|session| session.locked(Core::on_disconnected));
    }

    fn bytes_received(&self, bytes: &[u8]) {
        self.with_session(|session| session.locked(|core| core.handle_bytes(bytes)));
    }

    fn error(&self, error: &TransportError) {
        self.with_session(|session| session.locked(|core| core.error_occurred(error)));
    }
}

impl Core {
    /// The socket is open. A plaintext session starts capability negotiation, or resumes it
    /// after a plaintext STS upgrade (`Connected`).
    pub(crate) fn on_connected(&mut self) {
        if !self.tls_enabled && self.awaiting_socket() {
            self.begin_capability_negotiation();
        }
    }

    /// The TLS handshake finished. A TLS session starts capability negotiation (`Encrypted`).
    pub(crate) fn on_encrypted(&mut self) {
        if self.tls_enabled && self.awaiting_socket() {
            self.begin_capability_negotiation();
        }
    }

    /// Whether the session waits for its socket to open: the first connect, or the connect
    /// that follows an STS upgrade.
    fn awaiting_socket(&self) -> bool {
        matches!(
            self.state,
            SessionState::Connecting | SessionState::StsUpgrading
        )
    }

    /// The connection ended (`IrcTransport::disconnected`). An STS upgrade reconnects at once.
    /// Every other end refreshes the STS expiry, then settles, reconnects or fails.
    pub(crate) fn on_disconnected(&mut self) {
        let outcome = self.connection.on_disconnect(self.state);
        if outcome == DisconnectOutcome::FinishStsUpgrade {
            self.finish_sts_upgrade();
            return;
        }
        self.reschedule_sts_expiry();
        match outcome {
            DisconnectOutcome::ScheduleReconnect => self.schedule_reconnect(),
            DisconnectOutcome::SettleIdle => self.set_state(SessionState::Idle),
            DisconnectOutcome::FailUnexpected => {
                self.fail(ErrorKind::Network, "Connection closed by the server", true);
            }
            DisconnectOutcome::Ignore | DisconnectOutcome::FinishStsUpgrade => {}
        }
    }

    /// A transport failure (`IrcTransport::errorOccurred`). The transport names the kind, so the
    /// session classifies it without reading the message.
    pub(crate) fn error_occurred(&mut self, error: &TransportError) {
        if self.connection.is_stopping() {
            return;
        }
        let kind = match error.kind {
            TransportErrorKind::Tls | TransportErrorKind::Certificate => ErrorKind::Tls,
            TransportErrorKind::Resolve
            | TransportErrorKind::Network
            | TransportErrorKind::Timeout => ErrorKind::Network,
        };
        self.fail(kind, &error.message, true);
    }
}
