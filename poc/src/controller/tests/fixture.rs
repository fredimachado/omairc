//! The shared fixture: the demo controller on a fake clock, with a notifier that
//! records each mention. The Go equivalent is `attachController` in
//! `tui/internal/demo/demo_test.go`.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::controller::controller::Controller;
use crate::desktop::notifier::Notifier;
use crate::irc::event::MentionArrival;
use crate::session::clock::{Clock, FakeClock};

pub const OMARCHY: &str = "omarchy";
pub const OFTC: &str = "oftc";

/// Records every mention the controller hands to the notifier.
#[derive(Clone, Default)]
pub struct Mentions(pub Rc<RefCell<Vec<MentionArrival>>>);

impl Notifier for Mentions {
    fn mention(&self, arrival: &MentionArrival) {
        self.0.borrow_mut().push(arrival.clone());
    }
}

/// The seeded demo controller, its clock and its mention log.
pub struct Fixture {
    pub clock: Rc<FakeClock>,
    pub controller: Controller,
    pub mentions: Mentions,
}

impl Fixture {
    /// The demo as `--demo-server` starts it, on a fixed clock.
    pub fn seeded() -> Self {
        let clock = Rc::new(FakeClock::new(
            Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
        ));
        let mentions = Mentions::default();
        let dyn_clock: Rc<dyn Clock> = clock.clone();
        let controller = Controller::new_demo_with(dyn_clock, Box::new(mentions.clone()))
            .expect("the demo starts");
        // The seed's own transcript raises mention arrivals. Tests start from a quiet
        // log, so each one counts only what it sends or injects.
        mentions.0.borrow_mut().clear();
        Self {
            clock,
            controller,
            mentions,
        }
    }

    /// Every frame the client wrote to `network_id`, as text, oldest first.
    pub fn frames(&self, network_id: &str) -> Vec<String> {
        self.loopback_frames(network_id)
            .into_iter()
            .map(|frame| String::from_utf8_lossy(&frame).into_owned())
            .collect()
    }

    /// Forgets the frames written so far, so a test reads only what follows.
    pub fn clear_frames(&self, network_id: &str) {
        if let Some(transport) = self
            .controller
            .demo()
            .and_then(|demo| demo.transport(network_id))
        {
            transport.clear_written_frames();
        }
    }

    /// Delivers one server line to `network_id`, then pumps what it queued.
    pub fn deliver(&mut self, network_id: &str, line: &str) {
        let mut bytes = line.as_bytes().to_vec();
        bytes.extend_from_slice(b"\r\n");
        self.controller
            .demo()
            .expect("demo runs")
            .inject(network_id, &bytes);
        self.controller.tick();
    }

    fn loopback_frames(&self, network_id: &str) -> Vec<Vec<u8>> {
        self.controller
            .demo()
            .and_then(|demo| demo.transport(network_id))
            .map(|transport| transport.written_frames())
            .unwrap_or_default()
    }
}

/// The display text of every Status line for a network.
pub fn status_text(controller: &Controller, network_id: &str) -> Vec<String> {
    controller
        .status_lines(network_id)
        .into_iter()
        .map(|entry| entry.text.clone())
        .collect()
}

/// The body of every transcript row in the selected conversation.
pub fn bodies(controller: &Controller) -> Vec<String> {
    controller
        .messages()
        .iter()
        .map(|message| message.body.clone())
        .collect()
}
