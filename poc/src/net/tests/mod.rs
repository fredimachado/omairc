//! Tests for the socket transport. The shared conformance suite runs over a real TCP
//! socket and a real TLS handshake against the in-process peer. The cases below cover
//! what only the socket transport has: certificate trust, timeouts, refusal, and the
//! session driving a real wire (`session.rs`).

mod harness;
mod peer;
mod session;

use std::time::Duration;

use harness::{awaiting_peer, first_error, record, NetHarness, Rig};
use peer::{dead_address, Mode};

use crate::net::NetOptions;
use crate::session::transport::{ConnectionState, Transport, TransportErrorKind};

mod socket_suite {
    use super::NetHarness;
    use crate::session::conformance::conformance_suite;

    conformance_suite!(NetHarness::new);
}

/// A handshake that never completes ends in a timeout, with the loopback's text.
#[test]
fn handshake_that_never_answers_times_out() {
    let rig = Rig::new(NetOptions {
        handshake_timeout: Duration::from_millis(200),
        ..NetOptions::default()
    });
    let transport = rig.hub.register();
    let (sink, _keep) = record(transport.as_ref());

    transport.connect("127.0.0.1", rig.peer.port(), true);
    rig.peer.accept(Mode::Silent).expect("the peer accepts");
    rig.settle(&|| awaiting_peer(&transport, true));

    assert_eq!(transport.state(), ConnectionState::Failed);
    let error = first_error(&sink);
    assert_eq!(error.kind, TransportErrorKind::Timeout);
    assert_eq!(error.message, "Connection timed out");
    assert_eq!(*sink.encrypted.borrow(), 0);
}

/// A connect to a port nothing listens on is refused, which is a network failure.
#[test]
fn refused_port_is_a_network_error() {
    let rig = Rig::new(NetOptions::default());
    let transport = rig.hub.register();
    let (sink, _keep) = record(transport.as_ref());

    let refused = dead_address();
    transport.connect("127.0.0.1", refused.port(), false);
    rig.settle(&|| awaiting_peer(&transport, false));

    assert_eq!(transport.state(), ConnectionState::Failed);
    let error = first_error(&sink);
    assert_eq!(error.kind, TransportErrorKind::Network);
    assert!(
        error.message.starts_with("dial tcp 127.0.0.1:"),
        "{}",
        error.message
    );
}

/// A host name nothing resolves fails as a resolution error, before any socket opens.
#[test]
fn unresolvable_host_is_a_resolve_error() {
    let rig = Rig::new(NetOptions::default());
    let transport = rig.hub.register();
    let (sink, _keep) = record(transport.as_ref());

    transport.connect("no-such-host.invalid", 6667, false);
    rig.settle(&|| awaiting_peer(&transport, false));

    assert_eq!(transport.state(), ConnectionState::Failed);
    let error = first_error(&sink);
    assert_eq!(error.kind, TransportErrorKind::Resolve);
    assert!(
        error.message.starts_with("lookup no-such-host.invalid:"),
        "{}",
        error.message
    );
}

/// The system trust store does not hold the test certificate, so the handshake fails
/// with the certificate error, and the transport never reaches `Encrypted`.
#[test]
fn untrusted_certificate_is_a_certificate_error() {
    let rig = Rig::new(NetOptions::default());
    let transport = rig.hub.register();
    let (sink, _keep) = record(transport.as_ref());

    transport.connect("127.0.0.1", rig.peer.port(), true);
    // The client rejects the certificate and sends its alert, so the peer's own
    // handshake fails. That is the expected outcome here.
    let _ = rig.peer.accept(Mode::Tls);
    rig.settle(&|| awaiting_peer(&transport, true));

    assert_eq!(transport.state(), ConnectionState::Failed);
    let error = first_error(&sink);
    assert_eq!(error.kind, TransportErrorKind::Certificate);
    assert!(
        error.message.starts_with("TLS certificate error: "),
        "{}",
        error.message
    );
    assert_eq!(*sink.encrypted.borrow(), 0);
}

/// With the test certificate pinned as the one trusted server certificate, the same
/// handshake encrypts. The pin is the only difference from the case above.
#[test]
fn pinned_certificate_encrypts() {
    let rig = Rig::new(NetOptions {
        tls: peer::pinned_client_config(),
        ..NetOptions::default()
    });
    let transport = rig.hub.register();
    let (sink, _keep) = record(transport.as_ref());

    transport.connect("127.0.0.1", rig.peer.port(), true);
    rig.peer
        .accept(Mode::Tls)
        .expect("the peer completes the handshake");
    rig.settle(&|| awaiting_peer(&transport, true));

    assert_eq!(transport.state(), ConnectionState::Encrypted);
    assert_eq!(*sink.encrypted.borrow(), 1);
    assert!(sink.errors.borrow().is_empty());
}
