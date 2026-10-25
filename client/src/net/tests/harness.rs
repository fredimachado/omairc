//! The test rig for the socket transport: a hub, the peer it dials, and the pump that
//! delivers the hub's results on the test thread.
//!
//! The conformance cases name fixed host names and ports, so `NetHarness` routes every
//! connect to its own peer. The redirect is the only departure from production: the
//! transport, the hub, the worker and the TLS path all run unchanged.

use std::cell::Cell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::{Duration, Instant};

use async_channel::Receiver;
use tokio::runtime::{Builder, Runtime};
use tokio::time::timeout;

use super::peer::{dead_address, pinned_client_config, Mode, Peer, QUIET};
use crate::net::{Delivery, NetHub, NetOptions, NetTransport};
use crate::session::conformance::{RecordingSink, TransportHarness};
use crate::session::transport::{ConnectionState, Transport, TransportError, TransportSink};

/// How long `settle` waits for a handshake or a connect that is still in flight.
const SETTLE_DEADLINE: Duration = Duration::from_secs(5);
/// The address every connect is redirected to: the peer's, and the one the test
/// certificate names.
const PEER_HOST: &str = "127.0.0.1";

/// Blocks the test thread on the hub's delivery channel with a timeout.
pub(super) struct Pump {
    runtime: Runtime,
    events: Receiver<Delivery>,
}

impl Pump {
    fn new(events: Receiver<Delivery>) -> Self {
        let runtime = Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("the pump runtime builds");
        Self { runtime, events }
    }

    /// The next delivery, or `None` once `wait` has passed with nothing queued.
    pub(super) fn next(&self, wait: Duration) -> Option<Delivery> {
        self.runtime.block_on(async {
            match timeout(wait, self.events.recv()).await {
                Ok(Ok(delivery)) => Some(delivery),
                _ => None,
            }
        })
    }
}

/// A hub, its peer, and the pump between them.
pub(super) struct Rig {
    pub(super) hub: Rc<NetHub>,
    pub(super) peer: Peer,
    pub(super) pump: Pump,
}

impl Rig {
    pub(super) fn new(options: NetOptions) -> Self {
        let hub = Rc::new(NetHub::with_options(options));
        let pump = Pump::new(hub.receiver());
        Self {
            hub,
            peer: Peer::spawn(),
            pump,
        }
    }

    /// Delivers the hub's results until nothing more arrives. While `pending` says a
    /// handshake or connect is still in flight, the wait is the full deadline instead
    /// of a quiet window, so a slow handshake is not mistaken for a finished one.
    /// Panics if a pending transport does not settle by the deadline.
    pub(super) fn settle(&self, pending: &dyn Fn() -> bool) {
        let deadline = Instant::now() + SETTLE_DEADLINE;
        loop {
            let handshaking = pending();
            let wait = if handshaking {
                deadline.saturating_duration_since(Instant::now())
            } else {
                QUIET
            };
            match self.pump.next(wait) {
                Some(delivery) => self.hub.deliver(delivery),
                None if handshaking => panic!("the transport did not settle in time"),
                None => return,
            }
        }
    }
}

/// Whether a transport is waiting on the peer: a connect in flight, or a TLS connect
/// whose handshake has not finished.
pub(super) fn awaiting_peer(transport: &NetTransport, tls: bool) -> bool {
    match transport.state() {
        ConnectionState::Connecting => true,
        ConnectionState::Connected => tls,
        _ => false,
    }
}

/// Records every callback of `transport`'s sink. The caller keeps both handles alive
/// for as long as the transport should report to them.
pub(super) fn record(transport: &dyn Transport) -> (Rc<RecordingSink>, Rc<dyn TransportSink>) {
    let sink = Rc::new(RecordingSink::default());
    let keep: Rc<dyn TransportSink> = sink.clone();
    transport.set_sink(Rc::downgrade(&keep));
    (sink, keep)
}

/// The kind and message of the first error a sink recorded.
pub(super) fn first_error(sink: &RecordingSink) -> TransportError {
    let errors = sink.errors.borrow();
    assert_eq!(errors.len(), 1, "exactly one error: {:?}", errors);
    errors[0].clone()
}

/// The transport the conformance cases drive. It is the real transport, with the host
/// and port of each connect replaced by the harness's peer, or by a dead address when
/// a refusal is armed.
pub(super) struct Redirect {
    inner: Rc<NetTransport>,
    peer: SocketAddr,
    dead: SocketAddr,
    refuse_next: Cell<bool>,
    /// Whether the connect the transport took was sent to the dead address.
    refused: Cell<bool>,
    /// Whether the connect the transport took asked for TLS.
    tls: Cell<bool>,
}

impl Transport for Redirect {
    fn connect(&self, _host: &str, _port: u16, tls_enabled: bool) {
        let busy = matches!(
            self.inner.state(),
            ConnectionState::Connecting
                | ConnectionState::Connected
                | ConnectionState::Encrypted
                | ConnectionState::Closing
        );
        // An ignored connect leaves the armed refusal and the recorded choices alone.
        let refused = !busy && self.refuse_next.replace(false);
        if !busy {
            self.refused.set(refused);
            self.tls.set(tls_enabled);
        }
        let target = if refused { self.dead } else { self.peer };
        self.inner.connect(PEER_HOST, target.port(), tls_enabled);
    }

    fn write(&self, frame: &[u8]) {
        self.inner.write(frame);
    }

    fn shutdown(&self) {
        self.inner.shutdown();
    }

    fn state(&self) -> ConnectionState {
        self.inner.state()
    }

    fn set_sink(&self, sink: std::rc::Weak<dyn TransportSink>) {
        self.inner.set_sink(sink);
    }
}

/// The socket transport as a conformance harness.
pub(super) struct NetHarness {
    rig: Rig,
    wire: Rc<Redirect>,
    break_tls: Cell<bool>,
}

impl NetHarness {
    pub(super) fn new() -> Self {
        let options = NetOptions {
            tls: pinned_client_config(),
            ..NetOptions::default()
        };
        let rig = Rig::new(options);
        let inner = rig.hub.register();
        let wire = Rc::new(Redirect {
            inner,
            peer: SocketAddr::from(([127, 0, 0, 1], rig.peer.port())),
            dead: dead_address(),
            refuse_next: Cell::new(false),
            refused: Cell::new(false),
            tls: Cell::new(false),
        });
        Self {
            rig,
            wire,
            break_tls: Cell::new(false),
        }
    }
}

impl TransportHarness for NetHarness {
    fn transport(&self) -> Rc<dyn Transport> {
        self.wire.clone()
    }

    fn accept(&self) {
        if self.wire.refused.get() || self.wire.inner.state() != ConnectionState::Connecting {
            return;
        }
        let tls = self.wire.tls.get();
        let mode = match (tls, self.break_tls.get()) {
            (true, true) => {
                self.break_tls.set(false);
                Mode::BrokenTls
            }
            (true, false) => Mode::Tls,
            (false, _) => Mode::Plain,
        };
        self.rig
            .peer
            .accept(mode)
            .expect("the peer accepts the pending connect");
    }

    fn peer_send(&self, bytes: &[u8]) {
        self.rig.peer.send(bytes);
    }

    fn peer_received(&self) -> Vec<u8> {
        self.rig.peer.received()
    }

    fn peer_close(&self) {
        self.rig.peer.close();
    }

    fn refuse_next_connect(&self) {
        self.wire.refuse_next.set(true);
    }

    fn break_tls(&self) {
        self.break_tls.set(true);
    }

    fn settle(&self) {
        let wire = self.wire.clone();
        self.rig
            .settle(&|| awaiting_peer(&wire.inner, wire.tls.get()));
    }

    fn wire_is_real(&self) -> bool {
        true
    }
}
