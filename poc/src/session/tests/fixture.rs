//! The session test fixture, the Rust shape of `sessionFixture` in
//! `tui/internal/session/session_test.go`: a session over a loopback transport and
//! a fake clock, with helpers that read the written frames and the drained
//! events.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use chrono::{TimeZone, Utc};

use crate::irc::capability::IrcCapabilitySet;
use crate::irc::message::IrcMessage;
use crate::session::clock::FakeClock;
use crate::session::config::SessionConfig;
use crate::session::events::{ErrorKind, HistoryBatch, SessionEvent, SessionState};
use crate::session::loopback::LoopbackTransport;
use crate::session::session::Session;
use crate::session::sts::MemoryStsStore;

pub const NETWORK_ID: &str = "network-a";

/// The Go `sessionTestConfig`: TLS on, two autojoin channels, a 250 ms base
/// backoff doubling to 1000 ms.
pub fn test_config() -> SessionConfig {
    SessionConfig {
        network_id: NETWORK_ID.to_owned(),
        host: "irc.example".to_owned(),
        port: 6697,
        tls_enabled: true,
        nick: "omairc".to_owned(),
        username: "omairc".to_owned(),
        realname: "Omairc User".to_owned(),
        autojoin_channels: vec!["#omarchy".to_owned(), "&local".to_owned()],
        reconnect_enabled: true,
        reconnect_base_delay_ms: 250,
        reconnect_maximum_delay_ms: 1000,
        capability_timeout_ms: 10000,
        ping_timeout_ms: 60000,
        labeled_response_timeout_ms: 45000,
        ..SessionConfig::default()
    }
}

/// The Go `sessionTestPlaintextConfig`: port 6667, no TLS, no autojoin.
pub fn plaintext_config() -> SessionConfig {
    let mut config = test_config();
    config.port = 6667;
    config.tls_enabled = false;
    config.autojoin_channels.clear();
    config
}

/// One session under test. `events` drains the session's queue into a log the
/// assertions read, so every event is seen exactly once across calls.
pub struct Fixture {
    pub transport: Rc<LoopbackTransport>,
    pub clock: Rc<FakeClock>,
    /// The STS cache the session reads and writes, kept here so a test can prime it.
    pub sts: Rc<MemoryStsStore>,
    pub session: Session,
    log: RefCell<Vec<SessionEvent>>,
}

impl Fixture {
    pub fn new(config: SessionConfig) -> Self {
        let transport = Rc::new(LoopbackTransport::new());
        let clock = Rc::new(FakeClock::new(Utc.timestamp_opt(0, 0).single().unwrap()));
        let sts = Rc::new(MemoryStsStore::default());
        let session = Session::new(config, transport.clone(), clock.clone(), sts.clone());
        Self {
            transport,
            clock,
            sts,
            session,
            log: RefCell::new(Vec::new()),
        }
    }

    /// Every event so far, in order.
    pub fn events(&self) -> Vec<SessionEvent> {
        self.log.borrow_mut().extend(self.session.drain_events());
        self.log.borrow().clone()
    }

    pub fn errors(&self) -> Vec<(ErrorKind, String)> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                SessionEvent::ErrorOccurred { kind, message, .. } => Some((kind, message)),
                _ => None,
            })
            .collect()
    }

    pub fn error_messages(&self) -> Vec<String> {
        self.errors()
            .into_iter()
            .map(|(_, message)| message)
            .collect()
    }

    pub fn states(&self) -> Vec<SessionState> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                SessionEvent::StateChanged { state, .. } => Some(state),
                _ => None,
            })
            .collect()
    }

    pub fn registered_count(&self) -> usize {
        self.events()
            .iter()
            .filter(|event| matches!(event, SessionEvent::Registered { .. }))
            .count()
    }

    /// `(delay_ms, attempt)` of every reconnect scheduled so far.
    pub fn reconnects(&self) -> Vec<(u64, u32)> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                SessionEvent::ReconnectScheduled {
                    delay_ms, attempt, ..
                } => Some((delay_ms, attempt)),
                _ => None,
            })
            .collect()
    }

    pub fn messages(&self) -> Vec<IrcMessage> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                SessionEvent::MessageReceived { message, .. } => Some(message),
                _ => None,
            })
            .collect()
    }

    pub fn batches(&self) -> Vec<HistoryBatch> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                SessionEvent::HistoryBatchReceived { batch, .. } => Some(batch),
                _ => None,
            })
            .collect()
    }

    pub fn capability_changes(&self) -> Vec<IrcCapabilitySet> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                SessionEvent::CapabilitiesChanged { capabilities, .. } => Some(capabilities),
                _ => None,
            })
            .collect()
    }

    pub fn finished_labels(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                SessionEvent::RequestLabelFinished { label, .. } => Some(label),
                _ => None,
            })
            .collect()
    }

    /// Feeds raw inbound bytes, as the socket would.
    pub fn inject(&self, text: &str) {
        self.transport.inject_bytes(text.as_bytes());
    }

    /// Starts the session and completes the TLS connect (`connectTLS`).
    pub fn connect_tls(&self) {
        self.session.start();
        self.transport.complete_connect();
    }

    /// TLS connect, then the CAP LS and 001 a plain server sends
    /// (`registerWithWelcome`).
    pub fn register_with_welcome(&self) {
        self.connect_tls();
        self.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n");
    }

    /// Every written frame, as text.
    pub fn frames(&self) -> Vec<String> {
        self.transport
            .written_frames()
            .into_iter()
            .map(|frame| String::from_utf8_lossy(&frame).into_owned())
            .collect()
    }

    /// Whether one exact frame was written.
    pub fn wrote(&self, frame: &str) -> bool {
        self.frames().iter().any(|candidate| candidate == frame)
    }

    pub fn last_frame(&self) -> String {
        self.frames().pop().unwrap_or_default()
    }

    /// The frames written from index `index` onward.
    pub fn written_since(&self, index: usize) -> Vec<String> {
        let frames = self.frames();
        if index > frames.len() {
            return Vec::new();
        }
        frames[index..].to_vec()
    }

    /// Advances virtual time by the most recent scheduled reconnect delay.
    pub fn fire_reconnect(&self) {
        let (delay, _) = *self.reconnects().last().expect("no reconnect scheduled");
        self.clock.advance(Duration::from_millis(delay));
    }

    /// Drops the TLS connection, fires the reconnect, and registers again
    /// (`reconnectRegistered`).
    pub fn reconnect_registered(&self) {
        self.transport.remote_close();
        self.fire_reconnect();
        self.transport.complete_connect();
        self.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n");
    }
}

/// Asserts the frames equal `want`, in order, with a readable diff on failure.
#[track_caller]
pub fn assert_frames(got: &[String], want: &[&str]) {
    let got_refs: Vec<&str> = got.iter().map(String::as_str).collect();
    assert_eq!(got_refs, want, "written frames differ");
}

/// The JOIN frames in `frames`.
pub fn joins(frames: &[String]) -> Vec<String> {
    frames
        .iter()
        .filter(|frame| frame.starts_with("JOIN "))
        .cloned()
        .collect()
}
