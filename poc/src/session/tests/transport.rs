//! Port of `tui/internal/session/transport_test.go`: the loopback transport on its
//! own, without a session. The shared conformance cases run through
//! `LoopbackHarness` at the bottom; the tests above keep what only the loopback
//! exposes (the frame hook, recorded frames, its host and port, its error text).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::session::conformance::{RecordingSink, TransportHarness};
use crate::session::loopback::LoopbackTransport;
use crate::session::transport::{
    ConnectionState, Transport, TransportError, TransportErrorKind, TransportSink,
};

/// A loopback with `sink` installed. The sink is returned too, so the caller
/// keeps it alive for the weak reference the transport holds.
fn wired() -> (
    Rc<LoopbackTransport>,
    Rc<RecordingSink>,
    Rc<dyn TransportSink>,
) {
    let transport = Rc::new(LoopbackTransport::new());
    let sink = Rc::new(RecordingSink::default());
    let dyn_sink: Rc<dyn TransportSink> = sink.clone();
    transport.set_sink(Rc::downgrade(&dyn_sink));
    (transport, sink, dyn_sink)
}

#[test]
fn transport_succeeds_connect() {
    let (transport, sink, _keep) = wired();
    assert_eq!(transport.state(), ConnectionState::Idle);
    transport.connect("irc.example", 6667, false);
    assert_eq!(transport.state(), ConnectionState::Connecting);
    assert_eq!(transport.connected_host(), "irc.example");
    assert_eq!(transport.connected_port(), 6667);
    assert!(!transport.tls_requested());
    transport.complete_connect();
    assert_eq!(transport.state(), ConnectionState::Connected);
    assert_eq!(*sink.connected.borrow(), 1);
    assert_eq!(*sink.encrypted.borrow(), 0);
}

#[test]
fn transport_records_writes() {
    let transport = LoopbackTransport::new();
    transport.connect("irc.example", 6667, false);
    transport.complete_connect();
    transport.write(b"PING :x\r\n");
    transport.write(b"NICK omairc\r\n");
    let frames = transport.written_frames();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0], b"PING :x\r\n");
    assert_eq!(frames[1], b"NICK omairc\r\n");
}

#[test]
fn transport_on_frame_written_hook() {
    let (transport, sink, _keep) = wired();
    transport.connect("irc.example", 6697, true);
    transport.complete_connect();

    let seen: Rc<RefCell<Vec<Vec<u8>>>> = Rc::new(RefCell::new(Vec::new()));
    let hook_seen = seen.clone();
    let hook_transport = transport.clone();
    transport.set_frame_written_hook(Rc::new(move |frame: &[u8]| {
        hook_seen.borrow_mut().push(frame.to_vec());
        if frame == b"PING :x\r\n" {
            hook_transport.inject_bytes(b":server PONG irc.example :x\r\n");
        }
    }));
    transport.write(b"PING :x\r\n");
    transport.write(b"LIST\r\n");
    let seen = seen.borrow();
    assert_eq!(seen.len(), 2, "the hook fires once per write");
    assert_eq!(seen[0], b"PING :x\r\n");
    assert_eq!(seen[1], b"LIST\r\n");
    assert_eq!(
        sink.received.borrow().len(),
        1,
        "the hook re-entered inject_bytes"
    );
    assert_eq!(
        sink.received.borrow()[0],
        b":server PONG irc.example :x\r\n"
    );

    // With the hook cleared, a write is still recorded.
    transport.clear_frame_written_hook();
    transport.write(b"NICK omairc\r\n");
    let frames = transport.written_frames();
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[2], b"NICK omairc\r\n");
}

#[test]
fn transport_tls_certificate_failure_surfaces_error() {
    let (transport, sink, _keep) = wired();
    transport.connect("irc.example", 6697, true);
    assert!(transport.tls_requested());
    let details = "TLS certificate error: The certificate has expired (CN=irc.example)";
    transport.fail_tls(details);
    assert_eq!(transport.state(), ConnectionState::Failed);
    assert_eq!(
        sink.errors.borrow().as_slice(),
        [TransportError {
            kind: TransportErrorKind::Tls,
            message: details.to_owned(),
        }]
    );
    assert_eq!(transport.last_error(), details);
    assert_eq!(*sink.encrypted.borrow(), 0);
}

#[test]
fn transport_connect_failure_surfaces_network_error() {
    let (transport, sink, _keep) = wired();
    transport.connect("irc.example", 6667, false);
    transport.fail_connect("no route to host");
    assert_eq!(transport.state(), ConnectionState::Failed);
    assert_eq!(
        sink.errors.borrow().as_slice(),
        [TransportError {
            kind: TransportErrorKind::Network,
            message: "no route to host".to_owned(),
        }]
    );
}

#[test]
fn transport_timeout_fails_without_sleep() {
    let (transport, sink, _keep) = wired();
    transport.connect("irc.example", 6667, false);
    transport.timeout_connect();
    assert_eq!(transport.state(), ConnectionState::Failed);
    let errors = sink.errors.borrow();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, TransportErrorKind::Timeout);
    assert!(errors[0].message.contains("timed out"), "{:?}", errors[0]);
}

/// The loopback as a conformance harness. Its peer is the loopback's own record:
/// `accept` takes the pending connect, `peer_send` injects bytes, and
/// `peer_received` reads the frames written while the pipe is open. Delivery is
/// synchronous, so nothing is ever pending for `settle` to wait on.
struct LoopbackHarness {
    transport: Rc<LoopbackTransport>,
    refuse_next: Cell<bool>,
    break_tls_next: Cell<bool>,
    /// How many written frames the peer has already read.
    frames_read: Cell<usize>,
}

impl LoopbackHarness {
    fn new() -> Self {
        Self {
            transport: Rc::new(LoopbackTransport::new()),
            refuse_next: Cell::new(false),
            break_tls_next: Cell::new(false),
            frames_read: Cell::new(0),
        }
    }
}

impl TransportHarness for LoopbackHarness {
    fn transport(&self) -> Rc<dyn Transport> {
        self.transport.clone()
    }

    fn accept(&self) {
        if self.transport.state() != ConnectionState::Connecting {
            return;
        }
        if self.refuse_next.replace(false) {
            self.transport.fail_connect("Connection refused");
        } else if self.break_tls_next.get() && self.transport.tls_requested() {
            self.break_tls_next.set(false);
            self.transport.fail_tls("TLS handshake failed");
        } else {
            self.transport.complete_connect();
        }
    }

    fn peer_send(&self, bytes: &[u8]) {
        self.transport.inject_bytes(bytes);
    }

    fn peer_received(&self) -> Vec<u8> {
        if !matches!(
            self.transport.state(),
            ConnectionState::Connected | ConnectionState::Encrypted
        ) {
            return Vec::new();
        }
        let frames = self.transport.written_frames();
        let unread = self.frames_read.replace(frames.len());
        frames[unread..].concat()
    }

    fn peer_close(&self) {
        self.transport.remote_close();
    }

    fn refuse_next_connect(&self) {
        self.refuse_next.set(true);
    }

    fn break_tls(&self) {
        self.break_tls_next.set(true);
    }

    fn settle(&self) {
        // Every loopback event runs on the caller's stack, so none is ever pending.
    }

    fn wire_is_real(&self) -> bool {
        false
    }
}

mod loopback_suite {
    use super::LoopbackHarness;
    use crate::session::conformance::conformance_suite;

    conformance_suite!(LoopbackHarness::new);
}
