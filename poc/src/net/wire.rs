//! The messages between the UI thread and the net thread.
//!
//! `Command`s travel from the UI thread to the net thread, and `Delivery`s travel back.
//! Each one names its connection, and a connect, a write or an event also names the epoch
//! it belongs to. A connection bumps its epoch on every connect and shutdown, so a stale
//! event or write is dropped by the side that holds the newer epoch. Nothing here carries
//! a frame's contents in a log.

use crate::session::transport::TransportError;

/// One socket transport's identity. The hub hands them out, one per `register`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConnId(pub(super) u64);

/// A request from the UI thread to the net thread.
pub(super) enum Command {
    /// Start a connect. Any older task for `conn` is abandoned first.
    Connect {
        conn: ConnId,
        epoch: u64,
        host: String,
        port: u16,
        tls: bool,
    },
    /// Queue one framed line. The worker buffers it until the pipe is open.
    Write {
        conn: ConnId,
        epoch: u64,
        frame: Vec<u8>,
    },
    /// Abandon the connection, or the connect in flight. Commands run in order, so the task
    /// torn down is the connection's current one. The transport has already moved its epoch on,
    /// which is what keeps the abandoned task's late events out.
    Close { conn: ConnId },
    /// Tear down every task and end the net thread. Sent from `NetHub::drop`.
    Stop,
}

/// One event from the net thread, tagged with the connection and epoch that made it.
pub struct Delivery {
    pub(super) conn: ConnId,
    pub(super) epoch: u64,
    pub(super) event: IoEvent,
}

/// What a worker saw on its connection.
pub(super) enum IoEvent {
    /// The TCP connect finished. For TLS this comes before the handshake.
    Connected,
    /// The TLS handshake finished.
    Encrypted,
    /// Inbound bytes, unframed.
    Bytes(Vec<u8>),
    /// The peer ended the stream cleanly.
    Closed,
    /// The connection failed, with the reason the session will show.
    Failed(TransportError),
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Command>();
    assert_send::<Delivery>();
};
