//! The demo test harness, the Rust shape of `attachController` in
//! `tui/internal/demo/demo_test.go`. Each seeded network gets a real `Session`
//! over a `LoopbackTransport` and a `FakeClock`. The demo attaches to both and
//! plays the seed. The tests read what the sessions emitted, through
//! `drain_events`, and what the transports wrote.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::irc::message::IrcMessage;
use crate::irc::wiretext::wire_text;
use crate::session::clock::FakeClock;
use crate::session::events::SessionEvent;
use crate::session::loopback::LoopbackTransport;
use crate::session::session::Session;
use crate::session::sts::MemoryStsStore;
use crate::session::transport::Transport;

use crate::demo::seed::{SeedNetwork, OFTC_WORLD, OMARCHY_WORLD};
use crate::demo::server::{session_config, DemoError, DemoServer, SeedControls};

/// The controller intents the seed produced, in the order it ran them.
#[derive(Default)]
pub struct Recorder {
    pub orders: Vec<Vec<String>>,
    pub selects: Vec<(String, String)>,
}

impl SeedControls for Recorder {
    fn set_network_order(&mut self, network_ids: &[String]) {
        self.orders.push(network_ids.to_vec());
    }

    fn select_conversation(&mut self, network_id: &str, target: &str) {
        self.selects
            .push((network_id.to_owned(), target.to_owned()));
    }
}

/// One network's session and its loopback, with a log of every event its session
/// has emitted so far.
pub struct Lane {
    pub session: Session,
    pub transport: Rc<LoopbackTransport>,
    log: RefCell<Vec<SessionEvent>>,
}

impl Lane {
    /// A session for `network` over a fresh loopback. Nothing is started.
    pub fn new(network: &SeedNetwork, clock: &Rc<FakeClock>) -> Self {
        let transport = Rc::new(LoopbackTransport::new());
        let sts = Rc::new(MemoryStsStore::default());
        let session = Session::new(
            session_config(network),
            transport.clone(),
            clock.clone(),
            sts,
        );
        Self {
            session,
            transport,
            log: RefCell::new(Vec::new()),
        }
    }

    /// Drains the session's queue into the log and returns what arrived since the
    /// last call.
    pub fn pump(&self) -> Vec<SessionEvent> {
        let fresh = self.session.drain_events();
        self.log.borrow_mut().extend(fresh.iter().cloned());
        fresh
    }

    /// Every event since the session was built.
    pub fn events(&self) -> Vec<SessionEvent> {
        self.pump();
        self.log.borrow().clone()
    }

    /// Every inbound message since the session was built, in order.
    pub fn inbound(&self) -> Vec<IrcMessage> {
        inbound_of(&self.events())
    }

    /// The inbound messages that arrived since the last call.
    pub fn fresh_inbound(&self) -> Vec<IrcMessage> {
        inbound_of(&self.pump())
    }

    /// Every frame the session wrote, as text.
    pub fn frames(&self) -> Vec<String> {
        self.transport
            .written_frames()
            .into_iter()
            .map(|frame| String::from_utf8_lossy(&frame).into_owned())
            .collect()
    }

    /// Writes one client line through the loopback, as the session would. The
    /// auto-answer hook sees it exactly as it sees a session write.
    pub fn send(&self, line: &str) {
        self.transport.write(line.as_bytes());
    }
}

/// The inbound messages in `events`.
pub fn inbound_of(events: &[SessionEvent]) -> Vec<IrcMessage> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::Inbound { message, .. } => Some(message.clone()),
            _ => None,
        })
        .collect()
}

/// The error messages in `events`.
pub fn errors_of(events: &[SessionEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ErrorOccurred { message, .. } => Some(message.clone()),
            _ => None,
        })
        .collect()
}

/// Both seeded networks, attached and seeded.
// test fixture: not every test module uses this helper
#[allow(dead_code)]
pub struct Harness {
    // test fixture: not every test module uses this helper
    #[allow(dead_code)]
    pub clock: Rc<FakeClock>,
    pub demo: DemoServer,
    pub omarchy: Lane,
    pub oftc: Lane,
    pub controls: Recorder,
}

impl Harness {
    /// Builds the two sessions, attaches the demo, and plays the seed. With
    /// `auto_echo` the auto-answer hook is on after the seed, as the app runs it.
    pub fn seeded(auto_echo: bool) -> Self {
        let clock = Rc::new(FakeClock::new(
            Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
        ));
        let demo = DemoServer::new(clock.clone());
        let omarchy = Lane::new(&OMARCHY_WORLD, &clock);
        let oftc = Lane::new(&OFTC_WORLD, &clock);
        demo.attach(&omarchy.session, &omarchy.transport)
            .expect("attach omarchy");
        demo.attach(&oftc.session, &oftc.transport)
            .expect("attach oftc");
        let mut controls = Recorder::default();
        demo.seed(&mut controls, auto_echo)
            .expect("seed both networks");
        // The seed's own traffic moves into each log now, so `fresh_inbound`
        // reports only what a test does after the seed.
        omarchy.pump();
        oftc.pump();
        Self {
            clock,
            demo,
            omarchy,
            oftc,
            controls,
        }
    }

    /// The lane for `network_id`.
    pub fn lane(&self, network_id: &str) -> &Lane {
        match network_id {
            "omarchy" => &self.omarchy,
            "oftc" => &self.oftc,
            other => panic!("no lane for {other}"),
        }
    }
}

/// The nick of a message's source, or empty.
pub fn nick_of(message: &IrcMessage) -> String {
    message
        .prefix
        .as_ref()
        .map(|prefix| wire_text(&prefix.nick))
        .unwrap_or_default()
}

/// The parameters of a message as display text.
pub fn params_of(message: &IrcMessage) -> Vec<String> {
    message
        .params
        .iter()
        .map(|param| wire_text(param))
        .collect()
}

/// The first tag value named `name`, decoded.
pub fn tag_of(message: &IrcMessage, name: &str) -> Option<String> {
    message
        .tag(name)
        .and_then(|tag| tag.value.as_deref())
        .map(wire_text)
}

/// The messages whose command is `command`.
pub fn with_command<'a>(messages: &'a [IrcMessage], command: &str) -> Vec<&'a IrcMessage> {
    messages
        .iter()
        .filter(|message| message.command == command)
        .collect()
}

/// The error returned by `attach` or `seed`, for assertions.
pub fn expect_err<T: std::fmt::Debug>(result: Result<T, DemoError>) -> DemoError {
    result.expect_err("expected the demo to refuse")
}
