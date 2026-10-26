//! The `--demo-server` controller. Port of the demo start in `tui/internal/demo/`
//! (`demo.go`, `attachController` in `demo_test.go`) and `IrcDemoServer` in
//! `src/irc/ircdemoserver.cpp` (C++).
//!
//! The demo is only the peer behind each seeded network's loopback. The controller
//! builds the two sessions, attaches the peers, and plays the seed in Qt's order.
//! Its `SeedControls` impl runs the two intents the seed interleaves with its bytes:
//! the sidebar order and each conversation selection. A selection first pumps what
//! the seed's bytes queued, so the transcript and the reads see them in order.

use std::rc::Rc;

use crate::demo::seed::{OFTC_WORLD, OMARCHY_WORLD, SeedNetwork};
use crate::demo::server::{session_config, DemoServer, SeedControls};
use crate::irc::statusentry::{self, LogSeverity};
use crate::session::clock::{Clock, RealClock};
use crate::session::loopback::LoopbackTransport;

use super::controller::{Controller, ControllerError};
use super::transports::NoTransports;

#[cfg(test)]
use crate::demo::seed::OMARCHY_NETWORK_ID;
#[cfg(test)]
use crate::demo::wire::{bench_channel_bytes, BENCH_CHANNEL};

impl Controller {
    /// The `--demo-server` controller, on the wall clock.
    pub fn new_demo() -> Result<Self, ControllerError> {
        Self::new_demo_with(Rc::new(RealClock::new()))
    }

    /// The demo controller on `clock`. Tests pass a fake clock so the demo's TIME
    /// reply and the timers are deterministic.
    pub fn new_demo_with(clock: Rc<dyn Clock>) -> Result<Self, ControllerError> {
        let mut controller = Self::with_parts(clock.clone(), Box::new(NoTransports));
        let demo = Rc::new(DemoServer::new(clock.clone()));
        for network in [&OMARCHY_WORLD, &OFTC_WORLD] {
            let transport = Rc::new(LoopbackTransport::new());
            let session = controller.manager.create(
                session_config(network),
                transport.clone(),
                clock.clone(),
            )?;
            demo.attach(&session, &transport)?;
        }
        controller.demo = Some(Rc::clone(&demo));
        demo.seed(&mut controller, true)?;
        controller.pump();
        Ok(controller)
    }
}

impl Controller {
    /// Starts a seeded demo network on a fresh loopback, with its registration burst
    /// and its auto-answers (`DemoServer::restart`).
    pub(super) fn restart_demo(&mut self, network: &'static SeedNetwork) -> bool {
        let Some(demo) = self.demo.clone() else {
            return self.refuse_start(network.network_id);
        };
        self.manager.discard(network.network_id);
        self.autoaway_forget_network(network.network_id);
        self.monitor_forget_presence(network.network_id);
        self.forget_channel_list(network.network_id);
        self.forget_replies(network.network_id);
        self.cancelled_joins.retain(|key| key.network_id != network.network_id);
        let transport = Rc::new(LoopbackTransport::new());
        let session = match self.manager.create(
            session_config(network),
            transport.clone(),
            self.clock.clone(),
        ) {
            Ok(session) => session,
            Err(error) => {
                self.refuse_with(network.network_id, &error.to_string());
                return false;
            }
        };
        self.attach_store_hooks(network.network_id);
        if let Err(error) = demo.restart(&session, &transport) {
            self.refuse_with(network.network_id, &error.to_string());
            return false;
        }
        true
    }

    /// A start the build cannot honour. The transport factory gives the reason, and
    /// the reason becomes the network's last error and a Status line.
    pub(super) fn refuse_start(&mut self, network_id: &str) -> bool {
        let message = match self.transports.open(network_id) {
            Err(message) => message,
            // A transport opened, but no profile exists to build a session from.
            Ok(_) => "That network is not configured".to_owned(),
        };
        self.refuse_with(network_id, &message);
        false
    }

    /// Records a refused start as the network's last error and writes it to Status.
    /// With no selection, Status is what the window shows, so it moves to this network
    /// (the first-run Apply case). A selected conversation stays on screen.
    pub(super) fn refuse_with(&mut self, network_id: &str, message: &str) {
        self.set_last_error(network_id, Some(message.to_owned()));
        let now = self.clock.now();
        let entry = statusentry::lifecycle(network_id, LogSeverity::Alert, "error", message, now);
        self.append_status(entry);
        if self.selected.is_none() {
            self.console_network = network_id.to_owned();
        }
    }
}

impl SeedControls for Controller {
    fn set_network_order(&mut self, network_ids: &[String]) {
        Controller::set_network_order(self, network_ids);
    }

    fn select_conversation(&mut self, network_id: &str, target: &str) {
        Controller::select_conversation(self, network_id, target);
    }
}

#[cfg(test)]
impl Controller {
    /// The seeded demo on a fixed clock for the presentation tests in `model/` and `ui/`.
    /// Frame-level tests use `tests/fixture.rs`.
    pub fn seeded_for_tests() -> Self {
        use chrono::{TimeZone, Utc};
        use crate::session::clock::FakeClock;

        let clock: Rc<dyn Clock> = Rc::new(FakeClock::new(
            Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
        ));
        Self::new_demo_with(clock).expect("the demo starts")
    }

    /// Test-only `#bench` seed: `count` mixed lines after its JOIN, topic and NAMES,
    /// injected as frames on the omarchy network, then selected. False when the
    /// controller is not running the demo.
    pub fn bench_channel(&mut self, count: usize) -> bool {
        self.pump();
        let Some(demo) = self.demo.clone() else {
            return false;
        };
        let nick = self.current_nick(OMARCHY_NETWORK_ID);
        if !demo.inject(OMARCHY_NETWORK_ID, &bench_channel_bytes(&nick, count)) {
            return false;
        }
        self.pump();
        self.select_conversation(OMARCHY_NETWORK_ID, BENCH_CHANNEL);
        true
    }
}
