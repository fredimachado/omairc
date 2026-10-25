//! A real `Session` over the socket transport: the state machine, the clock and the
//! hub, with bytes on a real socket. Both cases start from `SessionConfig` and read the
//! outcome from the session's own events, so they check the whole path, not one layer.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::rc::Rc;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::harness::Rig;
use super::peer::Mode;
use crate::net::NetOptions;
use crate::session::clock::RealClock;
use crate::session::config::SessionConfig;
use crate::session::events::{ErrorKind, SessionEvent, SessionState};
use crate::session::session::Session;
use crate::session::sts::MemoryStsStore;
use crate::session::transport::{ConnectionState, Transport};

/// How long a session case may take before it fails rather than hangs.
const DRIVE_DEADLINE: Duration = Duration::from_secs(10);

/// Delivers the hub's results and fires due timers until `done` holds. Panics at the
/// deadline. Each pass drains what is already queued, then waits briefly for more.
fn drive(rig: &Rig, clock: &RealClock, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + DRIVE_DEADLINE;
    loop {
        let delivered = rig.hub.drain_ready();
        clock.fire_due();
        if done() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the session did not finish in time"
        );
        if delivered == 0 {
            if let Some(delivery) = rig.pump.next(Duration::from_millis(20)) {
                rig.hub.deliver(delivery);
            }
        }
    }
}

/// A scripted ircd on its own thread. It answers `CAP LS` with no capabilities, welcomes
/// the registration with `001`, `005` and `376`, and sends a PING. It reports the
/// session's PONG and keeps the connection open, since a server hang-up is a disconnect.
fn scripted_ircd(listener: TcpListener, pong: Sender<()>) {
    thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let Ok(mut writer) = stream.try_clone() else {
            return;
        };
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else {
                return;
            };
            if line.starts_with("CAP LS") {
                let _ = writer.write_all(b"CAP * LS :\r\n");
            } else if line.starts_with("USER ") {
                let _ = writer.write_all(
                    b":irc.example 001 fred :Welcome\r\n\
                      :irc.example 005 fred CHANTYPES=# :are supported\r\n\
                      :irc.example 376 fred :End of MOTD\r\n\
                      PING :abc\r\n",
                );
            } else if line == "PONG :abc" {
                let _ = pong.send(());
            }
        }
    });
}

/// The `ErrorOccurred` kinds in `events`, in order.
fn error_kinds(events: &[SessionEvent]) -> Vec<ErrorKind> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ErrorOccurred { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect()
}

/// Registration and PING/PONG over a plaintext socket, against the scripted ircd.
#[test]
fn session_registers_and_answers_ping_over_tcp() {
    let rig = Rig::new(NetOptions::default());
    let listener = TcpListener::bind("127.0.0.1:0").expect("the ircd binds a port");
    let port = listener
        .local_addr()
        .expect("the ircd has an address")
        .port();
    let (pong_tx, pong_rx) = mpsc::channel();
    scripted_ircd(listener, pong_tx);

    let mut config = SessionConfig::with_defaults("net", "Net", "127.0.0.1", "fred");
    config.port = port;
    config.tls_enabled = false;
    config.reconnect_enabled = false;
    let clock = Rc::new(RealClock::new());
    let transport = rig.hub.register();
    let sts = Rc::new(MemoryStsStore::default());
    let session = Session::new(config, transport.clone(), clock.clone(), sts);
    session.start();

    let mut ponged = false;
    drive(&rig, &clock, || {
        ponged |= pong_rx.try_recv().is_ok();
        ponged && session.state() == SessionState::Registered
    });

    assert_eq!(session.nick(), "fred");
    assert_eq!(transport.state(), ConnectionState::Connected);
    let events = session.drain_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SessionEvent::Registered { .. })),
        "the session announced registration"
    );
    assert!(error_kinds(&events).is_empty(), "no errors: {events:?}");
}

/// The system trust store does not hold the test certificate. The session sees the
/// certificate failure as a TLS error, and with reconnect off it stops in `Failed`.
#[test]
fn session_reports_an_untrusted_certificate_as_a_tls_error() {
    let rig = Rig::new(NetOptions::default());
    let mut config = SessionConfig::with_defaults("net", "Net", "127.0.0.1", "fred");
    config.port = rig.peer.port();
    config.tls_enabled = true;
    config.reconnect_enabled = false;
    let clock = Rc::new(RealClock::new());
    let transport = rig.hub.register();
    let sts = Rc::new(MemoryStsStore::default());
    let session = Session::new(config, transport.clone(), clock.clone(), sts);
    session.start();

    // The client rejects the certificate, so the peer's handshake fails. Expected.
    let _ = rig.peer.accept(Mode::Tls);
    drive(&rig, &clock, || session.state() == SessionState::Failed);

    let events = session.drain_events();
    assert_eq!(error_kinds(&events), [ErrorKind::Tls], "{events:?}");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, SessionEvent::Registered { .. })),
        "a session that failed TLS never registers"
    );
}
