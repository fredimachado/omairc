//! Port of `tui/internal/session/transport_test.go`: the loopback transport on its
//! own, without a session.

use std::cell::RefCell;
use std::rc::Rc;

use crate::session::loopback::LoopbackTransport;
use crate::session::transport::{
    ConnectionState, Transport, TransportError, TransportErrorKind, TransportSink,
};

/// Records every sink callback, standing in for the Go `recordingSink`.
#[derive(Default)]
struct RecordingSink {
    connected: RefCell<usize>,
    encrypted: RefCell<usize>,
    disconnected: RefCell<usize>,
    received: RefCell<Vec<Vec<u8>>>,
    errors: RefCell<Vec<TransportError>>,
}

impl TransportSink for RecordingSink {
    fn connected(&self) {
        *self.connected.borrow_mut() += 1;
    }
    fn encrypted(&self) {
        *self.encrypted.borrow_mut() += 1;
    }
    fn disconnected(&self) {
        *self.disconnected.borrow_mut() += 1;
    }
    fn bytes_received(&self, bytes: &[u8]) {
        self.received.borrow_mut().push(bytes.to_vec());
    }
    fn error(&self, error: &TransportError) {
        self.errors.borrow_mut().push(error.clone());
    }
}

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
fn transport_injects_inbound_bytes() {
    let (transport, sink, _keep) = wired();
    transport.connect("irc.example", 6667, false);
    transport.complete_connect();
    transport.inject_bytes(b"PONG :x\r\n");
    assert_eq!(sink.received.borrow().len(), 1);
    assert_eq!(sink.received.borrow()[0], b"PONG :x\r\n");
}

#[test]
fn transport_remote_close_ends_connection() {
    let (transport, sink, _keep) = wired();
    transport.connect("irc.example", 6667, false);
    transport.complete_connect();
    transport.remote_close();
    assert_eq!(transport.state(), ConnectionState::Disconnected);
    assert_eq!(*sink.disconnected.borrow(), 1);
    assert!(sink.errors.borrow().is_empty());
}

#[test]
fn transport_shutdown_while_connecting() {
    let (transport, sink, _keep) = wired();
    transport.connect("irc.example", 6697, true);
    assert_eq!(transport.state(), ConnectionState::Connecting);
    transport.shutdown();
    assert_eq!(transport.state(), ConnectionState::Disconnected);
    assert_eq!(*sink.disconnected.borrow(), 1);
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
fn transport_second_shutdown_is_safe() {
    let (transport, sink, _keep) = wired();
    transport.connect("irc.example", 6667, false);
    transport.shutdown();
    transport.shutdown();
    transport.shutdown();
    assert_eq!(transport.state(), ConnectionState::Disconnected);
    assert_eq!(*sink.disconnected.borrow(), 1);
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
