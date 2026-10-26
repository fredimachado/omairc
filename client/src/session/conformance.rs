//! The transport conformance suite. Every `Transport` runs the same cases through a
//! `TransportHarness`, the seam where a test drives the peer. The loopback harness
//! lives beside its tests in `session/tests/transport.rs`; the socket harness runs
//! these same cases over a real connection.
//! A case never sleeps. `settle` waits for the transport's pending events, and
//! `accept` is the peer's turn in a connect. A case asserts only what every
//! transport guarantees; `wire_is_real` gates the one case that needs real bytes.
//! `conformance_suite!` emits the cases as `#[test]`s over a harness factory.

use std::cell::RefCell;
use std::rc::Rc;

use crate::session::transport::{
    ConnectionState, Transport, TransportError, TransportErrorKind, TransportSink,
};

/// What a conformance case needs from a transport and the peer it talks to.
pub(crate) trait TransportHarness {
    /// The transport under test. Every call returns the same transport.
    fn transport(&self) -> Rc<dyn Transport>;
    /// The peer's turn in a pending connect. It accepts the connection, or lets a
    /// refusal or TLS failure armed beforehand land. With no connect pending it
    /// does nothing.
    fn accept(&self);
    /// The peer sends raw bytes to the transport.
    fn peer_send(&self, bytes: &[u8]);
    /// The bytes the peer has read since the last call, in order. Nothing is
    /// readable before the pipe opens.
    fn peer_received(&self) -> Vec<u8>;
    fn peer_close(&self);
    /// Arms the peer to refuse the next connect. Call it before `connect`; the
    /// refusal lands at `accept`.
    fn refuse_next_connect(&self);
    /// Arms the peer to fail the next TLS handshake. Call it before a TLS
    /// `connect`; the failure lands at `accept`.
    fn break_tls(&self);
    /// Waits until the transport has no event pending. The wait is bounded: a
    /// harness that cannot settle by its deadline panics rather than hangs. It
    /// never sleeps.
    fn settle(&self);
    /// Whether the peer sits on a real wire, where the first bytes of a TLS
    /// connection are a ClientHello a test can inspect.
    fn wire_is_real(&self) -> bool;
}

const HOST: &str = "irc.example";
const PLAIN_PORT: u16 = 6667;
const TLS_PORT: u16 = 6697;

/// Records every sink callback. `events` is the full log in call order; the
/// counters, `received` and `errors` are the same calls broken out for assertions.
#[derive(Default)]
pub(crate) struct RecordingSink {
    pub(crate) connected: RefCell<usize>,
    pub(crate) encrypted: RefCell<usize>,
    pub(crate) disconnected: RefCell<usize>,
    pub(crate) received: RefCell<Vec<Vec<u8>>>,
    pub(crate) errors: RefCell<Vec<TransportError>>,
    pub(crate) events: RefCell<Vec<&'static str>>,
}

impl TransportSink for RecordingSink {
    fn connected(&self) {
        *self.connected.borrow_mut() += 1;
        self.events.borrow_mut().push("connected");
    }
    fn encrypted(&self) {
        *self.encrypted.borrow_mut() += 1;
        self.events.borrow_mut().push("encrypted");
    }
    fn disconnected(&self) {
        *self.disconnected.borrow_mut() += 1;
        self.events.borrow_mut().push("disconnected");
    }
    fn bytes_received(&self, bytes: &[u8]) {
        self.received.borrow_mut().push(bytes.to_vec());
        self.events.borrow_mut().push("bytes");
    }
    fn error(&self, error: &TransportError) {
        self.errors.borrow_mut().push(error.clone());
        self.events.borrow_mut().push("error");
    }
}

/// Installs a recording sink on the harness's transport. The transport keeps only
/// a weak reference, so the caller keeps the returned sink alive for the case.
fn attach<H: TransportHarness>(harness: &H) -> Rc<RecordingSink> {
    let sink = Rc::new(RecordingSink::default());
    let dyn_sink: Rc<dyn TransportSink> = sink.clone();
    harness.transport().set_sink(Rc::downgrade(&dyn_sink));
    sink
}

/// `connect` enters `Connecting`, a second connect while that is in flight is
/// ignored, and the peer's accept reaches `Connected` with one `connected`.
pub(crate) fn connect_reaches_connected_once<H: TransportHarness>(harness: &H) {
    let sink = attach(harness);
    let transport = harness.transport();
    assert_eq!(transport.state(), ConnectionState::Idle);

    transport.connect(HOST, PLAIN_PORT, false);
    assert_eq!(transport.state(), ConnectionState::Connecting);
    // A second connect in flight changes nothing, not even the TLS choice.
    transport.connect("other.example", TLS_PORT, true);
    assert_eq!(transport.state(), ConnectionState::Connecting);

    harness.accept();
    harness.settle();
    assert_eq!(transport.state(), ConnectionState::Connected);
    assert_eq!(*sink.connected.borrow(), 1);
    assert_eq!(*sink.encrypted.borrow(), 0);
    assert!(sink.errors.borrow().is_empty());
}

/// With TLS requested, `connected` fires once and `encrypted` fires once after it.
pub(crate) fn tls_connects_then_encrypts<H: TransportHarness>(harness: &H) {
    let sink = attach(harness);
    let transport = harness.transport();
    transport.connect(HOST, TLS_PORT, true);
    harness.accept();
    harness.settle();
    assert_eq!(transport.state(), ConnectionState::Encrypted);
    assert_eq!(*sink.events.borrow(), ["connected", "encrypted"]);
    assert_eq!(*sink.connected.borrow(), 1);
    assert_eq!(*sink.encrypted.borrow(), 1);
}

/// Frames written before the pipe opens reach the peer, in order, once it opens.
pub(crate) fn writes_before_open_reach_peer_in_order<H: TransportHarness>(harness: &H) {
    let transport = harness.transport();
    transport.connect(HOST, PLAIN_PORT, false);
    transport.write(b"NICK omairc\r\n");
    transport.write(b"USER omairc 0 * :Omairc\r\n");
    assert!(
        harness.peer_received().is_empty(),
        "the peer reads nothing before the pipe opens"
    );
    harness.accept();
    harness.settle();
    assert_eq!(
        harness.peer_received(),
        b"NICK omairc\r\nUSER omairc 0 * :Omairc\r\n"
    );
}

/// With TLS requested on a real wire, the first bytes the peer reads are a TLS
/// ClientHello (record type 0x16, version 0x03), never a frame queued before the
/// handshake. A loopback has no wire to inspect, so the case returns early there.
pub(crate) fn tls_hello_precedes_any_frame<H: TransportHarness>(harness: &H) {
    if !harness.wire_is_real() {
        return;
    }
    let transport = harness.transport();
    transport.connect(HOST, TLS_PORT, true);
    transport.write(b"NICK omairc\r\n");
    harness.accept();
    harness.settle();
    let wire = harness.peer_received();
    assert!(
        wire.starts_with(&[0x16, 0x03]),
        "the first peer bytes must be a TLS ClientHello, got {:02x?}",
        wire
    );
}

/// Inbound bytes reach the sink in order and unframed: a line split across two
/// sends arrives as the same bytes it was sent as.
pub(crate) fn inbound_bytes_arrive_in_order<H: TransportHarness>(harness: &H) {
    let sink = attach(harness);
    let transport = harness.transport();
    transport.connect(HOST, PLAIN_PORT, false);
    harness.accept();
    harness.settle();
    harness.peer_send(b"PING :a\r\nPO");
    harness.peer_send(b"NG :b\r\n");
    harness.settle();
    assert_eq!(sink.received.borrow().concat(), b"PING :a\r\nPONG :b\r\n");
}

/// A remote close fires `disconnected` once and no `error`.
pub(crate) fn remote_close_is_a_clean_disconnect<H: TransportHarness>(harness: &H) {
    let sink = attach(harness);
    let transport = harness.transport();
    transport.connect(HOST, PLAIN_PORT, false);
    harness.accept();
    harness.settle();
    harness.peer_close();
    harness.settle();
    assert_eq!(transport.state(), ConnectionState::Disconnected);
    assert_eq!(*sink.disconnected.borrow(), 1);
    assert!(sink.errors.borrow().is_empty());
}

/// `shutdown` while connecting fires `disconnected` once, then the abandoned
/// connect delivers nothing.
pub(crate) fn shutdown_while_connecting_ends_quietly<H: TransportHarness>(harness: &H) {
    let sink = attach(harness);
    let transport = harness.transport();
    transport.connect(HOST, TLS_PORT, true);
    transport.shutdown();
    assert_shutdown_is_final(harness, &sink, transport.as_ref());
}

/// `shutdown` on an open connection fires `disconnected` once, then the peer's
/// later bytes are dropped.
pub(crate) fn shutdown_while_open_ends_quietly<H: TransportHarness>(harness: &H) {
    let sink = attach(harness);
    let transport = harness.transport();
    transport.connect(HOST, PLAIN_PORT, false);
    harness.accept();
    harness.settle();
    transport.shutdown();
    assert_shutdown_is_final(harness, &sink, transport.as_ref());
}

/// The tail both shutdown cases share. `disconnected` fired once during the call
/// which returned before any settle. Afterwards the peer's late turn and bytes
/// change nothing, and a second shutdown is a no-op.
fn assert_shutdown_is_final<H: TransportHarness>(
    harness: &H,
    sink: &RecordingSink,
    transport: &dyn Transport,
) {
    assert_eq!(
        *sink.disconnected.borrow(),
        1,
        "disconnected fires during shutdown"
    );
    let before = sink.events.borrow().clone();

    harness.accept();
    harness.peer_send(b"PING :late\r\n");
    harness.settle();
    transport.shutdown();
    harness.settle();

    assert_eq!(
        *sink.events.borrow(),
        before,
        "nothing from the connection arrives after shutdown"
    );
    assert_eq!(transport.state(), ConnectionState::Disconnected);
}

/// A refused connect fires one `error` of kind Network, and the state is Failed.
pub(crate) fn refused_connect_fails_once<H: TransportHarness>(harness: &H) {
    let sink = attach(harness);
    let transport = harness.transport();
    harness.refuse_next_connect();
    transport.connect(HOST, PLAIN_PORT, false);
    harness.accept();
    harness.settle();
    assert_eq!(transport.state(), ConnectionState::Failed);
    let errors = sink.errors.borrow();
    assert_eq!(errors.len(), 1, "{:?}", errors);
    assert_eq!(errors[0].kind, TransportErrorKind::Network);
    assert!(!errors[0].message.is_empty(), "the error carries a message");
    assert_eq!(*sink.connected.borrow(), 0);
}

/// A TLS failure fires one `error` of kind Tls or Certificate. The state is
/// Failed and the connection never reaches `Encrypted`.
pub(crate) fn tls_failure_fires_one_tls_error<H: TransportHarness>(harness: &H) {
    let sink = attach(harness);
    let transport = harness.transport();
    harness.break_tls();
    transport.connect(HOST, TLS_PORT, true);
    harness.accept();
    harness.settle();
    assert_eq!(transport.state(), ConnectionState::Failed);
    let errors = sink.errors.borrow();
    assert_eq!(errors.len(), 1, "{:?}", errors);
    assert!(
        matches!(
            errors[0].kind,
            TransportErrorKind::Tls | TransportErrorKind::Certificate
        ),
        "{:?}",
        errors[0]
    );
    assert_eq!(*sink.encrypted.borrow(), 0);
}

/// After a failure the same transport connects again, and bytes from the old
/// connection never reach the new one.
pub(crate) fn reconnect_after_failure_drops_old_bytes<H: TransportHarness>(harness: &H) {
    let sink = attach(harness);
    let transport = harness.transport();

    harness.refuse_next_connect();
    transport.connect(HOST, PLAIN_PORT, false);
    harness.accept();
    harness.settle();
    assert_eq!(transport.state(), ConnectionState::Failed);

    transport.connect(HOST, PLAIN_PORT, false);
    harness.accept();
    harness.settle();
    assert_eq!(transport.state(), ConnectionState::Connected);
    assert_eq!(*sink.connected.borrow(), 1);

    harness.peer_send(b"OLD\r\n");
    transport.shutdown();
    let old_chunks = sink.received.borrow().len();
    harness.peer_send(b"LATE\r\n");
    harness.settle();
    assert_eq!(
        sink.received.borrow().len(),
        old_chunks,
        "bytes from the old connection are dropped"
    );

    transport.connect(HOST, PLAIN_PORT, false);
    harness.accept();
    harness.settle();
    assert_eq!(transport.state(), ConnectionState::Connected);
    harness.peer_send(b"NEW\r\n");
    harness.settle();
    assert_eq!(sink.received.borrow()[old_chunks..].concat(), b"NEW\r\n");
}

/// Emits every conformance case as a `#[test]`. `$make` is a path to a function
/// that returns a fresh harness, so each case starts from a clean transport.
macro_rules! conformance_suite {
    ($make:path) => {
        #[test]
        fn connect_reaches_connected_once() {
            $crate::session::conformance::connect_reaches_connected_once(&$make());
        }

        #[test]
        fn tls_connects_then_encrypts() {
            $crate::session::conformance::tls_connects_then_encrypts(&$make());
        }

        #[test]
        fn writes_before_open_reach_peer_in_order() {
            $crate::session::conformance::writes_before_open_reach_peer_in_order(&$make());
        }

        #[test]
        fn tls_hello_precedes_any_frame() {
            $crate::session::conformance::tls_hello_precedes_any_frame(&$make());
        }

        #[test]
        fn inbound_bytes_arrive_in_order() {
            $crate::session::conformance::inbound_bytes_arrive_in_order(&$make());
        }

        #[test]
        fn remote_close_is_a_clean_disconnect() {
            $crate::session::conformance::remote_close_is_a_clean_disconnect(&$make());
        }

        #[test]
        fn shutdown_while_connecting_ends_quietly() {
            $crate::session::conformance::shutdown_while_connecting_ends_quietly(&$make());
        }

        #[test]
        fn shutdown_while_open_ends_quietly() {
            $crate::session::conformance::shutdown_while_open_ends_quietly(&$make());
        }

        #[test]
        fn refused_connect_fails_once() {
            $crate::session::conformance::refused_connect_fails_once(&$make());
        }

        #[test]
        fn tls_failure_fires_one_tls_error() {
            $crate::session::conformance::tls_failure_fires_one_tls_error(&$make());
        }

        #[test]
        fn reconnect_after_failure_drops_old_bytes() {
            $crate::session::conformance::reconnect_after_failure_drops_old_bytes(&$make());
        }
    };
}
pub(crate) use conformance_suite;
