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

use crate::demo::seed::{OFTC_WORLD, OMARCHY_NETWORK_ID, OMARCHY_WORLD};
use crate::demo::server::{session_config, DemoServer, SeedControls};
use crate::demo::wire::{bench_channel_bytes, BENCH_CHANNEL};
use crate::desktop::notifier::{Notifier, NullNotifier};
use crate::session::clock::{Clock, RealClock};
use crate::session::loopback::LoopbackTransport;

use super::controller::{Controller, ControllerError};
use super::transports::LiveTransports;

impl Controller {
    /// The `--demo-server` controller, on the wall clock.
    pub fn new_demo() -> Result<Self, ControllerError> {
        Self::new_demo_with(Rc::new(RealClock::new()), Box::new(NullNotifier))
    }

    /// The demo controller on `clock`, with `notifier` for mentions. Tests pass a fake
    /// clock so the demo's TIME reply and the timers are deterministic.
    pub fn new_demo_with(
        clock: Rc<dyn Clock>,
        notifier: Box<dyn Notifier>,
    ) -> Result<Self, ControllerError> {
        let mut controller = Self::with_parts(clock.clone(), notifier, Box::new(LiveTransports));
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

    /// The `--lines` bench channel: `count` mixed lines after its JOIN, topic and
    /// NAMES, injected as frames on the omarchy network, and then selected. False when
    /// the controller is not running the demo.
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

impl SeedControls for Controller {
    fn set_network_order(&mut self, network_ids: &[String]) {
        Controller::set_network_order(self, network_ids);
    }

    fn select_conversation(&mut self, network_id: &str, target: &str) {
        Controller::select_conversation(self, network_id, target);
    }
}
