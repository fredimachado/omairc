//! `Transport` is the duplex byte pipe the session drives. It takes commands
//! (`connect`, `write`, `shutdown`) and reports through a `TransportSink`. The
//! trait has no socket in it: this crate's `session/` stays portable, and the
//! TCP and TLS transport (`transport_net.go` in Go) lives in `net/`. Methods
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
    // Nothing enters Closing yet: `shutdown` moves straight to Disconnected. The
    // net transport matches on it, so the variant stays until a shutdown reports it.
    #[allow(dead_code)]
    Closing,
    /// A cleanly ended connection.
    Disconnected,
    /// An error end. The sink's `error` carried the reason.
    Failed,
}

impl ConnectionState {
 /// The C++ enumerator name, for logs and tests.
    // No log line prints the transport state yet, so nothing calls this name.
    #[allow(dead_code)]
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

/// Why a transport failed. The session maps each kind to its `ErrorKind`:
/// `Tls` and `Certificate` are TLS failures, the rest are network failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportErrorKind {
    /// The host name did not resolve.
    Resolve,
    /// The connection could not be made, or dropped.
    Network,
    /// The connect did not finish in time.
    Timeout,
    /// The TLS handshake failed.
    Tls,
    /// The server certificate did not verify.
    Certificate,
}

/// A transport failure: its kind, and a message that is already presentable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportError {
    pub kind: TransportErrorKind,
    pub message: String,
}

/// Receives transport lifecycle and data events (`Sink` in Go
/// `IrcTransport` signals). Implementations must not block. A loopback
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
    /// A transport failure, with its kind.
    fn error(&self, error: &TransportError);
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
