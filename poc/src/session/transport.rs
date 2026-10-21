//! Port of `tui/internal/session/transport.go` (Go) and `src/irc/irctransport.h` (C++).
//!
//! `Transport` is the duplex byte pipe the session drives. It takes commands
//! (`connect`, `write`, `shutdown`) and reports through a `TransportSink`. The
//! trait has no socket in it: this crate's `session/` stays portable, and the
//! TCP and TLS transport (`transport_net.go` in Go) waits for Phase 11. Methods
//! take `&self` so the transport can be shared between the session and its
//! owner, with interior mutability inside each implementation.

use std::rc::Weak;

/// The transport lifecycle (`IrcTransport::ConnectionState`). The transition
/// order matters: `Encrypted` means open and secure, `Connected` open but
/// plaintext.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionState {
    /// Never connected.
    #[default]
    Idle,
    /// The TCP connect, and for TLS the handshake, is in flight.
    Connecting,
    /// An open plaintext socket.
    Connected,
    /// An open TLS socket.
    Encrypted,
    /// A shutdown in progress.
    Closing,
    /// A cleanly ended connection.
    Disconnected,
    /// An error end. The sink's `error` carried the reason.
    Failed,
}

impl ConnectionState {
    /// The C++ enumerator name, for logs and tests.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Encrypted => "Encrypted",
            Self::Closing => "Closing",
            Self::Disconnected => "Disconnected",
            Self::Failed => "Failed",
        }
    }
}

/// Receives transport lifecycle and data events (`Sink` in Go,
/// `IrcTransport` signals in C++). Implementations must not block. A loopback
/// transport calls these synchronously, on the caller's stack, so a sink may
/// call back into the transport.
pub trait TransportSink {
    /// The socket is open. For TLS this fires before `encrypted`.
    fn connected(&self);
    /// The TLS handshake finished.
    fn encrypted(&self);
    /// The connection ended cleanly.
    fn disconnected(&self);
    /// Raw inbound bytes, in order, unframed.
    fn bytes_received(&self, bytes: &[u8]);
    /// A transport failure, already presentable.
    fn error(&self, message: &str);
}

/// The duplex byte pipe the session drives (`Transport` in Go).
pub trait Transport {
    /// Starts an asynchronous connection. `tls_enabled` selects a handshake
    /// before the pipe opens.
    fn connect(&self, host: &str, port: u16, tls_enabled: bool);
    /// Queues or sends one framed line, CRLF included. Bytes written before the
    /// pipe opens are buffered by a socket transport.
    fn write(&self, frame: &[u8]);
    /// Closes an open connection, or abandons a connect in flight.
    fn shutdown(&self);
    /// The current lifecycle state.
    fn state(&self) -> ConnectionState;
    /// Installs the receiver. The transport keeps only a weak reference, so the
    /// session owns the sink and no cycle forms.
    fn set_sink(&self, sink: Weak<dyn TransportSink>);
}
