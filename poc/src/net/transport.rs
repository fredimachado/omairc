//! The socket transport, the UI-side half of Go's `NetTransport` (`transport_net.go`).
//!
//! It lives on the UI thread, held as an `Rc` and registered with its `NetHub`. Commands
//! go to the net thread over the hub's channel. Events come back through `apply`, which
//! moves the lifecycle and calls the sink. Every call borrows the state only long
//! enough to read or move it, so a sink may call back into the transport.
//!
//! Epochs make stale work harmless. Each connect and each shutdown moves the epoch on,
//! so an event from an abandoned connection fails the epoch check and is dropped.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use tokio::sync::mpsc::UnboundedSender;

use super::wire::{Command, ConnId, IoEvent};
use crate::session::transport::{ConnectionState, Transport, TransportError, TransportSink};

/// The transport's mutable state, behind one `RefCell`.
#[derive(Default)]
struct Inner {
    state: ConnectionState,
    epoch: u64,
    tls_on: bool,
    sink: Option<Weak<dyn TransportSink>>,
}

/// One socket connection as the session sees it. Built only by `NetHub::register`.
pub struct NetTransport {
    conn: ConnId,
    commands: UnboundedSender<Command>,
    inner: RefCell<Inner>,
}

/// The sink call an event produced, with its payload, after the state has moved.
enum Notice {
    Connected,
    Encrypted,
    Bytes(Vec<u8>),
    Disconnected,
    Error(TransportError),
}

impl NetTransport {
    pub(super) fn new(conn: ConnId, commands: UnboundedSender<Command>) -> Self {
        Self {
            conn,
            commands,
            inner: RefCell::default(),
        }
    }

    /// Applies one event from the net thread. An event from an older epoch is dropped,
    /// and so is one that does not fit the current state. The sink runs with no borrow
    /// held.
    pub(super) fn apply(&self, epoch: u64, event: IoEvent) {
        let (notice, sink) = {
            let mut inner = self.inner.borrow_mut();
            if inner.epoch != epoch {
                return;
            }
            let Some(notice) = transition(&mut inner, event) else {
                return;
            };
            (notice, inner.sink.as_ref().and_then(Weak::upgrade))
        };
        let Some(sink) = sink else {
            return;
        };
        match notice {
            Notice::Connected => sink.connected(),
            Notice::Encrypted => sink.encrypted(),
            Notice::Bytes(bytes) => sink.bytes_received(&bytes),
            Notice::Disconnected => sink.disconnected(),
            Notice::Error(error) => sink.error(&error),
        }
    }

    fn send(&self, command: Command) {
        // A hub that has stopped has nothing left to serve the command.
        let _ = self.commands.send(command);
    }

    fn live_sink(&self) -> Option<Rc<dyn TransportSink>> {
        self.inner.borrow().sink.as_ref().and_then(Weak::upgrade)
    }
}

/// Moves the lifecycle for one event and says what to report. `None` means the event
/// does not apply in this state.
fn transition(inner: &mut Inner, event: IoEvent) -> Option<Notice> {
    match event {
        IoEvent::Connected if inner.state == ConnectionState::Connecting => {
            inner.state = ConnectionState::Connected;
            Some(Notice::Connected)
        }
        IoEvent::Encrypted if inner.state == ConnectionState::Connected && inner.tls_on => {
            inner.state = ConnectionState::Encrypted;
            Some(Notice::Encrypted)
        }
        IoEvent::Bytes(bytes) if is_open(inner.state) => Some(Notice::Bytes(bytes)),
        IoEvent::Closed if is_open(inner.state) => {
            inner.state = ConnectionState::Disconnected;
            Some(Notice::Disconnected)
        }
        // The first failure is the one reported; a failed transport takes no more.
        IoEvent::Failed(error) if in_flight(inner.state) => {
            inner.state = ConnectionState::Failed;
            Some(Notice::Error(error))
        }
        _ => None,
    }
}

/// An open pipe, plaintext or encrypted.
fn is_open(state: ConnectionState) -> bool {
    matches!(
        state,
        ConnectionState::Connected | ConnectionState::Encrypted
    )
}

/// A connect that has started and not yet ended. Connect is ignored here, as in Go.
fn in_flight(state: ConnectionState) -> bool {
    matches!(
        state,
        ConnectionState::Connecting
            | ConnectionState::Connected
            | ConnectionState::Encrypted
            | ConnectionState::Closing
    )
}

impl Transport for NetTransport {
    /// Starts a connect on the net thread. It is ignored while one is in flight.
    fn connect(&self, host: &str, port: u16, tls_enabled: bool) {
        let epoch = {
            let mut inner = self.inner.borrow_mut();
            if in_flight(inner.state) {
                return;
            }
            inner.epoch += 1;
            inner.state = ConnectionState::Connecting;
            inner.tls_on = tls_enabled;
            inner.epoch
        };
        self.send(Command::Connect {
            conn: self.conn,
            epoch,
            host: host.to_owned(),
            port,
            tls: tls_enabled,
        });
    }

    /// Sends the frame to the net thread, which buffers it until the pipe is open. A
    /// frame with no open task behind it is dropped there.
    fn write(&self, frame: &[u8]) {
        if frame.is_empty() {
            return;
        }
        let epoch = self.inner.borrow().epoch;
        self.send(Command::Write {
            conn: self.conn,
            epoch,
            frame: frame.to_vec(),
        });
    }

    /// Abandons the connect or closes the open pipe, then reports `disconnected` once.
    /// It does nothing once the transport has finished.
    fn shutdown(&self) {
        {
            let mut inner = self.inner.borrow_mut();
            if matches!(
                inner.state,
                ConnectionState::Idle
                    | ConnectionState::Disconnected
                    | ConnectionState::Failed
                    | ConnectionState::Closing
            ) {
                return;
            }
            inner.epoch += 1;
            inner.state = ConnectionState::Disconnected;
        }
        self.send(Command::Close { conn: self.conn });
        if let Some(sink) = self.live_sink() {
            sink.disconnected();
        }
    }

    fn state(&self) -> ConnectionState {
        self.inner.borrow().state
    }

    fn set_sink(&self, sink: Weak<dyn TransportSink>) {
        self.inner.borrow_mut().sink = Some(sink);
    }
}
